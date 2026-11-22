package check

import (
	"cmp"
	"context"
	"log/slog"
	"maps"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"time"

	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
)

// membersOf gives the directory of the state of the members, under the directory of the state of the set. Each member
// has its state in the directory of its uid there.
func membersOf(state string) string {
	return filepath.Join(state, "clusters")
}

// SetConfig holds the flags of a set of clusters, beside the flags of Config. Config.ClusterUID is the cluster where
// the observer runs. It is always in the set, and it can be empty: the observer can run on a cluster that Palette does
// not manage.
type SetConfig struct {
	Config
	Selectors []Selector
	// Keys is the directory of the mounted Secret of the keys. Home is the project of its entry api-key, the project
	// of the ConfigMap.
	Keys string
	Home string
}

// Set observes a set of clusters. It finds the clusters of its selectors in the project of each key, checks each
// record one time for its project, and reads each cluster in turn. Each cluster is a member: an Observer of its own,
// with its own findings and its own state. The set holds the cycles, the pool of processes, the clocks of the set,
// the cache of the moves, and the alerts. The embedded Observer holds them: a member never runs a cycle of its own.
type Set struct {
	*Observer
	scfg SetConfig
	// versionsTurn lets one versions cycle of the set run. The cycle takes the versions slot for one project at a
	// time, so a click "Ask Palette" waits for one project, and not for the whole cycle.
	versionsTurn chan struct{}

	// keysMu guards keys, the keys of the last read. A member reads them with this lock and never with the lock of
	// the set, so a member that holds its own lock can read them.
	keysMu sync.Mutex
	keys   []runner.Key

	// The lock of the Observer guards the fields below.
	members   map[string]*setMember
	projects  map[string]*setProject
	selectors []model.Selector
	meta      baseline.Meta
	version   string
}

// setMember is one cluster of the set: its observer, its name, and its project. lastError is the refusal of the key
// of its project at the last search.
type setMember struct {
	uid       string
	obs       *Observer
	name      string
	project   model.ProjectRef
	lastError *model.ErrorRef
}

// setProject is one project with a key. clusters holds the clusters of the last good search, by uid, with their
// names: a search that fails keeps them. versions holds the last good versions of its records.
type setProject struct {
	name      string
	lastError *model.ErrorRef
	clusters  map[string]string
	versions  *model.Versions
	clock     ClockState
}

// NewSet makes the observer of a set of clusters. It reads the state of the set, and the state of each member that
// a run before this one left, so that a restart lists the clusters before the first search ends. pods reads the
// pods of the cluster where the observer runs, and it is nil when the observer reads no pod.
func NewSet(cfg SetConfig, strata Strata, pods Pods, log *slog.Logger) *Set {
	return newSet(cfg, strata, pods, log, time.Now)
}

func newSet(cfg SetConfig, strata Strata, pods Pods, log *slog.Logger, now func() time.Time) *Set {
	s := &Set{Observer: newObserver(cfg.Config, strata, log, now), scfg: cfg, versionsTurn: make(chan struct{}, 1),
		members: map[string]*setMember{}, projects: map[string]*setProject{}}
	s.pods = pods
	s.runContent, s.runVersions = s.RunContent, s.RunVersions
	entries, err := os.ReadDir(membersOf(cfg.State))
	if err != nil {
		return s
	}
	for _, e := range entries {
		if !e.IsDir() {
			continue
		}
		m := &setMember{uid: e.Name(), obs: s.newMember(e.Name())}
		if st := m.obs.state.Member; st != nil {
			m.name, m.project = st.Name, st.Project
		}
		s.members[m.uid] = m
	}
	return s
}

// newMember makes the observer of one cluster of the set. It shares the pool of processes and the versions slot of
// the set. Only the cluster where the observer runs reads the pods.
func (s *Set) newMember(uid string) *Observer {
	cfg := s.cfg
	cfg.ClusterUID = uid
	cfg.State = filepath.Join(membersOf(s.cfg.State), uid)
	m := buildObserver(cfg, s.strata, s.log.With("clusterUid", uid), s.now)
	m.procs, m.versionsSlot = s.procs, s.versionsSlot
	m.member = &membership{home: s.scfg.Home, keys: s.currentKeys}
	if uid == s.cfg.ClusterUID {
		m.pods = s.pods
	}
	return m
}

// currentKeys gives the keys of the last read.
func (s *Set) currentKeys() []runner.Key {
	s.keysMu.Lock()
	defer s.keysMu.Unlock()
	return slices.Clone(s.keys)
}

func (s *Set) setKeys(keys []runner.Key) {
	s.keysMu.Lock()
	defer s.keysMu.Unlock()
	s.keys = keys
}

// memberList gives the members in the order of their names. The caller does not hold the lock.
func (s *Set) memberList() []setMember {
	s.mu.Lock()
	defer s.mu.Unlock()
	out := make([]setMember, 0, len(s.members))
	for _, m := range s.members {
		out = append(out, *m)
	}
	slices.SortFunc(out, func(a, b setMember) int {
		return cmp.Or(strings.Compare(a.name, b.name), strings.Compare(a.uid, b.uid))
	})
	return out
}

// project gives the state of a project, and makes it at the first call. The caller holds the lock.
func (s *Set) project(uid string) *setProject {
	p, ok := s.projects[uid]
	if !ok {
		p = &setProject{clusters: map[string]string{}}
		s.projects[uid] = p
	}
	return p
}

// projectOf gives the project whose key checks a record: its project, or the project of api-key for a record that
// a request at tenant scope took.
func (s *Set) projectOf(r docs.Record) string {
	if r.Source.ProjectUID == "" {
		return s.scfg.Home
	}
	return r.Source.ProjectUID
}

// entriesOf gives the records of one project, one for each uid.
func (s *Set) entriesOf(records baseline.Set, project string) []baseline.Entry {
	var out []baseline.Entry
	for _, e := range records.Entries {
		if s.projectOf(e.Record) == project && !slices.ContainsFunc(out, func(x baseline.Entry) bool { return x.Record.Source.UID == e.Record.Source.UID }) {
			out = append(out, e)
		}
	}
	return out
}

// Clusters gives the summary of the set: a row for each cluster, the projects, and the selectors. The state of
// each row comes from the summary of its member at this time.
func (s *Set) Clusters() model.SetSummary {
	return model.SetOf(s.setInput())
}

// Shared gives each profile version that a cluster of the set runs, with its copies.
func (s *Set) Shared() []model.Shared {
	return model.SharedOf(s.setInput().Members)
}

// setInput collects what SetOf reads. It reads the summary of each member without the lock of the set.
func (s *Set) setInput() model.SetInput {
	var in model.SetInput
	for _, m := range s.memberList() {
		in.Members = append(in.Members, model.Member{Summary: m.obs.Summary(), Project: m.project, Here: m.uid == s.cfg.ClusterUID, LastError: m.lastError})
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	in.Now, in.StartedAt, in.Files, in.FilesKnown, in.Meta, in.Strata = s.now(), s.state.StartedAt, s.files, s.filesKnown, s.meta, s.version
	in.ContentClock = model.Clock{Every: s.cfg.ContentEvery, Last: s.contentAt, FailedRuns: s.state.Content.FailedRuns, LastError: s.state.Content.LastError, Running: s.runningOf(cycleContent)}
	in.VersionsClock = model.Clock{Every: s.cfg.VersionsEvery, Last: s.versionsAt, FailedRuns: s.state.Versions.FailedRuns, LastError: s.state.Versions.LastError, Running: s.runningOf(cycleVersions)}
	in.Selectors = slices.Clone(s.selectors)
	for _, k := range slices.Sorted(maps.Keys(s.projects)) {
		p := s.projects[k]
		in.Projects = append(in.Projects, model.Project{ProjectRef: model.ProjectRef{UID: k, Name: p.name}, LastError: p.lastError})
	}
	return in
}

// Member gives the view of one cluster of the set, for the routes of that cluster. The second result is false for
// a cluster that is not in the set.
func (s *Set) Member(uid string) (*MemberView, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	m, ok := s.members[uid]
	if !ok {
		return nil, false
	}
	return &MemberView{set: s, Observer: m.obs}, true
}

// MemberView is one cluster of a set, as the routes of that cluster serve it. Its own observer gives the summary,
// the pods, the detail tier, and the answers of Palette. The set gives the alerts and starts the cycles.
type MemberView struct {
	*Observer
	set *Set
}

// StartCycle starts a cycle of the set: a cluster has no cycle of its own.
func (v *MemberView) StartCycle(name string) (model.Start, bool) {
	return v.set.StartCycle(name)
}

// Once runs one content cycle and one versions cycle of the set, and gives the summary of the set. The error joins
// the failures of the two cycles.
func (s *Set) Once(ctx context.Context) (model.SetSummary, error) {
	_, err := s.Observer.Once(ctx)
	return s.Clusters(), err
}
