package check

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/runner"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// The tests of a set give it a fake strata that answers for many clusters in many projects. The fake reads the key of
// each run, as the runner gives it to strata. It refuses each run with the key of a project that a test names.

// The projects, the keys, and the clusters of the tests of a set. Home is the project of the key api-key.
const (
	home      = testkit.SetHome
	other     = "p-other"
	homeKey   = testkit.SetHomeKey
	otherKey  = "key-of-other"
	prod      = testkit.SetProd
	nothing   = "tag:env=void"
	cPayments = "c-1001"
	cLedger   = "c-1002"
	cSearch   = "c-1003"
	webName   = "storefront"
	webV1     = "4.2.0"
	webHome   = "u-web-home"
	webOther  = "u-web-other"
	dbName    = "db"
	dbHome    = "u-db-home"
)

// fleetCluster is one cluster of the fake: its project, its name, and the uid of each profile that it runs.
type fleetCluster struct {
	project, name string
	profiles      []string
}

// fleetCall is one run of the fake strata, with the key of the run.
type fleetCall struct {
	args  []string
	key   runner.Key
	keyed bool
}

// fleet is the fake strata of a set.
type fleet struct {
	mu sync.Mutex
	// clusters holds each cluster by uid, and selected the uids that each selector selects in each project.
	clusters map[string]fleetCluster
	selected map[string][]string
	// records gives the name and the version of each record by uid, for the read of a cluster.
	records map[string][2]string
	// refused holds the projects whose key Palette refuses: each run with that key gets HTTP 403.
	refused map[string]bool
	// drifted holds the uid of each record whose published version changed. differs holds each copy of a cluster
	// that differs from its record, as "<cluster>/<record uid>".
	drifted map[string]bool
	differs map[string]bool
	calls   []fleetCall
}

func newFleet() *fleet {
	return &fleet{clusters: map[string]fleetCluster{}, selected: map[string][]string{}, records: map[string][2]string{},
		refused: map[string]bool{}, drifted: map[string]bool{}, differs: map[string]bool{}}
}

func (f *fleet) Run(ctx context.Context, dir string, args ...string) (runner.Result, error) {
	key, keyed := runner.KeyOf(ctx)
	f.mu.Lock()
	f.calls = append(f.calls, fleetCall{args: slices.Clone(args), key: key, keyed: keyed})
	refused := keyed && f.refused[key.Project]
	f.mu.Unlock()
	if refused {
		return runner.Result{}, &runner.RunError{Kind: docs.KindAPI, Status: 403}
	}
	switch args[0] {
	case runner.CmdList:
		return f.list(args)
	case runner.CmdVerify:
		return f.verify(dir, args)
	case runner.CmdSnapshot:
		return f.snapshot(dir, args)
	case runner.CmdCompare:
		return f.compare(dir, args)
	case runner.CmdBehind:
		return f.behind(dir, args)
	}
	return runner.Result{}, testkit.Unknown(args)
}

// after gives the argument after the flag name, or empty text.
func after(args []string, name string) string {
	i := slices.Index(args, name)
	if i < 0 || i+1 >= len(args) {
		return ""
	}
	return args[i+1]
}

// selectorOf reads the selector of a run of `list` from its flags.
func selectorOf(args []string) string {
	if name := after(args, runner.FlagFilter); name != "" {
		return filterPrefix + name
	}
	var labels []string
	for i, a := range args {
		if a == runner.FlagTag && i+1 < len(args) {
			labels = append(labels, args[i+1])
		}
	}
	return tagPrefix + strings.Join(labels, labelJoin)
}

// list answers the search of one selector in one project, as Palette answers the search of one project.
func (f *fleet) list(args []string) (runner.Result, error) {
	project := after(args, runner.FlagProject)
	list := docs.ClusterList{Schema: docs.ListSchema, Clusters: []docs.ListedCluster{}}
	f.mu.Lock()
	defer f.mu.Unlock()
	for _, uid := range f.selected[selectorOf(args)] {
		if c := f.clusters[uid]; c.project == project {
			list.Clusters = append(list.Clusters, docs.ListedCluster{UID: uid, Name: c.name, ProjectUID: c.project, ProjectName: "name-of-" + c.project})
		}
	}
	return doc(list, 0)
}

// readRecord reads a record file of the directory of a run.
func readRecord(dir, name string) (docs.Record, error) {
	var r docs.Record
	raw, err := os.ReadFile(filepath.Join(dir, name))
	if err != nil {
		return r, err
	}
	return r, json.Unmarshal(raw, &r)
}

// verify answers one row for each record file of the run: drifted for a record of drifted, else unchanged.
func (f *fleet) verify(dir string, args []string) (runner.Result, error) {
	s := docs.VerifySummary{Schema: docs.VerifySummarySchema, Tool: docs.Tool{Version: "0.36.0"}}
	for _, a := range args[1:] {
		if strings.HasPrefix(a, "-") {
			break
		}
		r, err := readRecord(dir, a)
		if err != nil {
			return runner.Result{}, err
		}
		row := docs.VerifyRow{Path: a, Status: docs.VerifyUnchanged, Name: r.Source.Name, Version: r.Source.Version, RootRecorded: r.Root, RootLive: r.Root}
		f.mu.Lock()
		if f.drifted[r.Source.UID] {
			row.Status, row.RootLive = docs.VerifyDrifted, "r-edited"
		}
		f.mu.Unlock()
		s.Records = append(s.Records, row)
	}
	return doc(s, 0)
}

// snapshot writes the read of a cluster, or the side of one profile, which names the cluster for the compare.
func (f *fleet) snapshot(dir string, args []string) (runner.Result, error) {
	out, err := testkit.OutFile(args)
	if err != nil {
		return runner.Result{}, err
	}
	ref := strings.TrimPrefix(args[1], "cluster:")
	uid, _, side := strings.Cut(ref, "/")
	if side {
		return runner.Result{}, os.WriteFile(filepath.Join(dir, out), []byte(`{"cluster":"`+uid+`"}`), 0o600)
	}
	f.mu.Lock()
	c, ok := f.clusters[uid]
	f.mu.Unlock()
	if !ok {
		return runner.Result{}, fmt.Errorf("the fake has no cluster %q", uid)
	}
	var read docs.ClusterRead
	read.Schema = docs.SnapshotSchema
	read.Source.Cluster.UID, read.Source.Cluster.Name = uid, c.name
	for _, p := range c.profiles {
		nv := f.records[p]
		read.Source.Cluster.Profiles = append(read.Source.Cluster.Profiles, docs.ClusterProfile{UID: p, Name: nv[0], Version: nv[1]})
	}
	raw, err := json.Marshal(read)
	if err != nil {
		return runner.Result{}, err
	}
	return runner.Result{}, os.WriteFile(filepath.Join(dir, out), raw, 0o600)
}

// compare compares a record with the side of a cluster: same, unless differs names the copy.
func (f *fleet) compare(dir string, args []string) (runner.Result, error) {
	r, err := readRecord(dir, args[1])
	if err != nil {
		return runner.Result{}, err
	}
	var side struct {
		Cluster string `json:"cluster"`
	}
	raw, err := os.ReadFile(filepath.Join(dir, args[2]))
	if err != nil {
		return runner.Result{}, err
	}
	if err := json.Unmarshal(raw, &side); err != nil {
		return runner.Result{}, err
	}
	f.mu.Lock()
	differs := f.differs[side.Cluster+"/"+r.Source.UID]
	f.mu.Unlock()
	if differs {
		return doc(docs.Diff{Schema: docs.DiffSchema, RootB: "r-cluster", Packs: []docs.Pack{{Key: testkit.CertManagerKey, Status: docs.PackChanged, Authored: true}}}, 1)
	}
	return doc(docs.Diff{Schema: docs.DiffSchema, Same: true, SameContent: true, RootB: r.Root}, 0)
}

// behind answers that nothing is newer, and names the profile version of the record.
func (f *fleet) behind(dir string, args []string) (runner.Result, error) {
	r, err := readRecord(dir, args[1])
	if err != nil {
		return runner.Result{}, err
	}
	subject := r.Source.Name + "@" + r.Source.Version
	return doc(docs.Behind{Schema: docs.BehindSchema, Tool: docs.Tool{Version: "0.36.0"},
		Profiles: []docs.ProfilePin{{Name: r.Source.Name, Version: r.Source.Version, PinnedBy: []docs.PinnedBy{{Subject: subject}}}}}, 0)
}

// runsOf gives each run of a command, with its key.
func (f *fleet) runsOf(command string) []fleetCall {
	f.mu.Lock()
	defer f.mu.Unlock()
	var out []fleetCall
	for _, c := range f.calls {
		if c.args[0] == command {
			out = append(out, c)
		}
	}
	return out
}

// setRig holds the directories of a test of a set: the records, the keys, the work, and the state.
type setRig struct {
	*rig
	keys string
	cfg  SetConfig
	f    *fleet
}

// newSetRig makes the rig of a set whose selectors are sels. The keys are homeKey for the project home and
// otherKey for the project other.
func newSetRig(t *testing.T, sels ...string) *setRig {
	t.Helper()
	r := &setRig{rig: newRig(t), keys: t.TempDir(), f: newFleet()}
	testkit.WriteIn(t, r.keys, HomeKeyFile, homeKey)
	testkit.WriteIn(t, r.keys, other, otherKey)
	var selectors []Selector
	for _, s := range sels {
		sel, err := ParseSelector(s)
		if err != nil {
			t.Fatal(err)
		}
		selectors = append(selectors, sel)
	}
	cfg := r.rig.cfg
	cfg.ClusterUID = ""
	r.cfg = SetConfig{Config: cfg, Selectors: selectors, Keys: r.keys, Home: home}
	return r
}

// setRecord writes the record of a profile version of a project, and tells the fake its name and its version. An
// empty project is a record that a request at tenant scope took.
func (r *setRig) setRecord(uid, name, version, project string) {
	r.t.Helper()
	body := testkit.Record(docs.SnapshotSchema, name, version, `,"uid":"`+uid+`","projectUid":"`+project+`","type":"cluster"`,
		`,"tool":{"version":"0.36.0"},"root":"r-`+uid+`","packs":[]`)
	testkit.WriteGzip(r.t, r.records, name+"_"+version+"_"+uid+".json.gz", body)
	r.f.records[uid] = [2]string{name, version}
}

// cluster adds a cluster of a project to the fake, with the uid of each profile that it runs. Each selector of sels
// selects it.
func (r *setRig) cluster(uid, project, name string, profiles []string, sels ...string) {
	r.f.clusters[uid] = fleetCluster{project: project, name: name, profiles: profiles}
	for _, s := range sels {
		r.f.selected[s] = append(r.f.selected[s], uid)
	}
}

// set makes the observer of the set, with the clock at t0.
func (r *setRig) set() *Set {
	r.t.Helper()
	return newSet(r.cfg, r.f, nil, slog.New(slog.NewTextHandler(r.log, nil)), func() time.Time { return t0 })
}
