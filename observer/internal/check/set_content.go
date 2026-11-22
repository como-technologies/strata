package check

import (
	"cmp"
	"context"
	"maps"
	"slices"

	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
)

// exitsOfAList are the exits of `list` that give a document. A list that cannot read the clusters exits 2 with an
// error line.
var exitsOfAList = []int{0}

// setCycle is what one content cycle of the set tells the alerts: the ids that it cannot clear, and each member
// whose check did not end well.
type setCycle struct {
	held   map[string]bool
	failed map[string]bool
}

// found is the answer of the search of one project: its clusters, or the error of a search that failed.
type found struct {
	clusters []docs.ListedCluster
	err      error
}

// RunContent runs one content cycle of the set, and hands it to the alerts. A cycle fails only when no check of a
// cluster ended well. A refused key leaves its clusters not checked, and the checks of the other projects go on.
func (s *Set) RunContent(ctx context.Context) error {
	c := setCycle{held: map[string]bool{}, failed: map[string]bool{}}
	err := s.runCycle(ctx, cycleContent, s.contentSlot, &s.state.Content,
		func(ctx context.Context) (int, error) { return s.contentCycle(ctx, c) },
		func() {
			at := s.now()
			s.contentAt = &at
		})
	s.giveVersions()
	s.alertsOfSet(ctx, err == nil, c)
	return err
}

// contentCycle runs one content cycle of the set. It reads the keys and the records, finds the clusters of each
// project, checks the records of each project one time, and then reads each cluster in turn. files is -1 when the
// directory of the records was not read.
func (s *Set) contentCycle(ctx context.Context, c setCycle) (int, error) {
	keys, err := readKeys(s.scfg.Keys, s.scfg.Home)
	if err != nil {
		s.log.Error("the keys cannot be read", "kind", docs.KindIO)
		return -1, failure(docs.KindIO)
	}
	s.setKeys(keys)
	records, err := baseline.Load(s.cfg.Records)
	if err != nil {
		return -1, failure(docs.KindIO)
	}
	finds := s.find(ctx, keys)
	rows, strata, checked := s.check(ctx, keys, records)
	members := s.reconcile(keys, finds, checked, records.Meta, strata)
	s.expect(ctx, len(members))
	var good int
	var first error
	for _, k := range keys {
		if err := finds[k.Project].err; err != nil && first == nil {
			first = err
		}
	}
	for _, m := range members {
		if err := ctx.Err(); err != nil {
			return records.Files(), err
		}
		ok, err := s.readMember(ctx, m, keys, finds, setRead{records: records, rows: rows, strata: strata}, c)
		s.step(ctx)
		if ok {
			good++
		} else if first == nil {
			first = err
		}
	}
	if good == 0 && first != nil {
		return records.Files(), first
	}
	return records.Files(), nil
}

// readMember reads one cluster with the key of its project, and keeps its answer in the member. A member of a
// project whose search failed is not read: it counts the failure of the search, and keeps its last good answer.
func (s *Set) readMember(ctx context.Context, m setMember, keys []runner.Key, finds map[string]found, in setRead, c setCycle) (bool, error) {
	key, keyed := keyOfProject(keys, m.project.UID)
	err := finds[m.project.UID].err
	if err == nil && !keyed {
		err = failure(docs.KindAPI)
	}
	if err != nil {
		m.obs.failMember(err)
		c.failed[m.uid] = true
		return false, err
	}
	held, good, err := m.obs.keepContent(runner.WithKey(ctx, key), func(ctx context.Context) (model.Content, int, error) {
		return m.obs.memberCycle(ctx, in)
	})
	for id := range held {
		c.held[id] = true
	}
	if !good {
		c.failed[m.uid] = true
	}
	return good, err
}

// find searches the clusters of each selector in the project of each key. The search of one project gives the
// clusters of that project, and the observer keeps only those. A search that fails is the error of its project. The
// set of the clusters is the union of the selectors, with each cluster once, by uid.
func (s *Set) find(ctx context.Context, keys []runner.Key) map[string]found {
	out := map[string]found{}
	counts := make([]model.Selector, len(s.scfg.Selectors))
	for i, sel := range s.scfg.Selectors {
		counts[i] = model.Selector{Selector: sel.Text}
	}
	err := s.withCycleDir(func(dir string) error {
		for _, k := range keys {
			var f found
			seen := map[string]bool{}
			for i, sel := range s.scfg.Selectors {
				listed, err := s.search(runner.WithKey(ctx, k), dir, sel, k.Project)
				if err != nil {
					f.err = err
					counts[i].Unread = true
					continue
				}
				for _, c := range listed {
					counts[i].Clusters++
					if !seen[c.UID] {
						seen[c.UID] = true
						f.clusters = append(f.clusters, c)
					}
				}
			}
			out[k.Project] = f
		}
		return nil
	})
	// with no directory, no search ran, so each project keeps its clusters
	if err != nil {
		for _, k := range keys {
			out[k.Project] = found{err: err}
		}
	}
	s.mu.Lock()
	s.selectors = counts
	s.mu.Unlock()
	return out
}

// search runs `list` for one selector in one project, and gives the clusters of that project.
func (s *Set) search(ctx context.Context, dir string, sel Selector, project string) ([]docs.ListedCluster, error) {
	s.expect(ctx, 1)
	res, err := s.run(ctx, dir, exitsOfAList, sel.args(project)...)
	if err != nil {
		return nil, err
	}
	doc, err := decode[docs.ClusterList](res.Stdout, docs.ListSchema)
	if err != nil {
		return nil, err
	}
	// a row that names no project is of the project of the search
	return slices.DeleteFunc(doc.Clusters, func(c docs.ListedCluster) bool { return c.ProjectUID != "" && c.ProjectUID != project }), nil
}

// check runs `verify` one time for each project, on the records of that project, with its key. A profile version
// that ten clusters run is one check. It gives the row of each record by uid, the version of strata, and the error of
// each project whose check failed.
func (s *Set) check(ctx context.Context, keys []runner.Key, records baseline.Set) (map[string]docs.VerifyRow, string, map[string]error) {
	rows, errs := map[string]docs.VerifyRow{}, map[string]error{}
	var strata string
	for _, k := range keys {
		entries := s.entriesOf(records, k.Project)
		if len(entries) == 0 {
			continue
		}
		err := s.withCycleDir(func(dir string) error {
			names, err := writeRecords(dir, entries)
			if err != nil {
				return err
			}
			s.expect(ctx, 1)
			doc, err := s.verify(runner.WithKey(ctx, k), dir, names)
			if err != nil {
				return err
			}
			strata = cmp.Or(strata, doc.Tool.Version)
			byPath := rowsByPath(doc)
			for i, e := range entries {
				if row, ok := byPath[names[i]]; ok {
					rows[e.Record.Source.UID] = row
				}
			}
			return nil
		})
		if err != nil {
			errs[k.Project] = err
		}
	}
	return rows, strata, errs
}

// reconcile makes the members of this cycle from the answers of the searches. A project whose search failed keeps
// the clusters of its last good search, and they are not checked. A cluster that leaves the set leaves the page, with
// its findings: see leave. The cluster where the observer runs is always in the set, in the project of api-key when
// no search names it.
func (s *Set) reconcile(keys []runner.Key, finds map[string]found, checked map[string]error, meta baseline.Meta, strata string) []setMember {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.meta, s.version = meta, strata
	want := map[string]setMember{}
	for _, k := range keys {
		p, f := s.project(k.Project), finds[k.Project]
		p.lastError = nil
		if err := cmp.Or(f.err, checked[k.Project]); err != nil {
			ref := errorRefOf(err)
			p.lastError = &ref
		}
		if f.err == nil {
			p.clusters = map[string]string{}
			for _, c := range f.clusters {
				p.clusters[c.UID] = c.Name
				p.name = cmp.Or(p.name, c.ProjectName)
			}
		}
		for uid, name := range p.clusters {
			if _, taken := want[uid]; !taken {
				want[uid] = setMember{uid: uid, name: name, project: model.ProjectRef{UID: k.Project, Name: p.name}}
			}
		}
	}
	if home := s.cfg.ClusterUID; home != "" {
		if _, ok := want[home]; !ok {
			want[home] = setMember{uid: home, project: model.ProjectRef{UID: s.scfg.Home, Name: s.project(s.scfg.Home).name}}
		}
	}
	for uid := range s.projects {
		if _, ok := keyOfProject(keys, uid); !ok {
			delete(s.projects, uid)
		}
	}
	for uid, m := range s.members {
		if _, ok := want[uid]; !ok {
			s.leave(m)
		}
	}
	for uid, w := range want {
		m, ok := s.members[uid]
		if !ok {
			m = &setMember{uid: uid, obs: s.newMember(uid)}
			s.members[uid] = m
		}
		m.name, m.project = cmp.Or(w.name, m.name), w.project
		m.lastError = nil
		if err := finds[w.project.UID].err; err != nil {
			ref := errorRefOf(err)
			m.lastError = &ref
		}
		m.obs.nameMember(m.name, m.project)
	}
	out := make([]setMember, 0, len(s.members))
	for _, uid := range slices.Sorted(maps.Keys(s.members)) {
		out = append(out, *s.members[uid])
	}
	return out
}

// leftSet is the message of the log line of a cluster that left the set.
const leftSet = "a cluster left the set"

// leave drops a cluster that left the set: its member, its state, and its items in the store of the alerts. The
// alerts send no message for it: no check showed that a finding of the cluster is fixed. The caller holds the lock.
func (s *Set) leave(m *setMember) {
	dropped := s.state.Alerts.Leave(m.uid)
	m.obs.removeState()
	delete(s.members, m.uid)
	s.log.Info(leftSet, "clusterUid", m.uid, "items", dropped)
}
