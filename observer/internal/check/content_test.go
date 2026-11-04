package check

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
)

// drift reads the real `verify` document that the stand-in gave for an edit in place.
func drift(t *testing.T) docs.VerifyRow {
	t.Helper()
	raw, err := os.ReadFile("../docs/testdata/verify-summary-drift.json")
	if err != nil {
		t.Fatal(err)
	}
	var s docs.VerifySummary
	if err := json.Unmarshal(raw, &s); err != nil {
		t.Fatal(err)
	}
	return s.Records[0]
}

// oneRecord makes a world with one record, aws-stack@1.1.0, that the cluster runs.
func oneRecord(t *testing.T, row docs.VerifyRow) (*rig, *world) {
	t.Helper()
	r := newRig(t)
	r.record("aws-stack", "1.1.0", "csi/csi-aws-ebs", "os/ubuntu")
	r.meta(`{"baseline":"approved","revision":"4e1c7a2"}`)
	return r, &world{rows: []docs.VerifyRow{row}, verifyTool: "0.29.0", cluster: cluster(t, "payments", "aws-stack@1.1.0")}
}

func changedPack(key string) docs.Pack {
	return docs.Pack{Key: key, Status: docs.PackChanged, Authored: true, Values: docs.Values{Diff: []docs.Leaf{{Op: "changed", Path: "charts.a.b", Old: sentinel, New: "y"}}}}
}

func TestContentCases(t *testing.T) {
	cases := []struct {
		name  string
		setup func(t *testing.T, w *world)
		want  []string // kind and packs of each finding
		copy  model.CopyStatus
		also  bool
		who   int
	}{
		{name: "no change", setup: func(t *testing.T, w *world) {}, copy: model.CopySame},
		{name: "published changed with who", setup: func(t *testing.T, w *world) { w.rows[0] = drift(t) }, copy: model.CopySame,
			want: []string{"published-changed csi/csi-aws-ebs"}, who: 1},
		{name: "published changed and the same on the cluster", setup: func(t *testing.T, w *world) {
			w.rows[0] = drift(t)
			w.compares = map[int]docs.Diff{0: {RootB: w.rows[0].RootLive, Packs: []docs.Pack{changedPack("csi/csi-aws-ebs")}}}
		}, copy: model.CopyDiffers, want: []string{"published-changed csi/csi-aws-ebs"}, also: true, who: 1},
		{name: "cluster differs only", setup: func(t *testing.T, w *world) {
			w.compares = map[int]docs.Diff{0: {RootB: "e06b", Summary: docs.DiffSummary{MaxImpact: "cluster-wide"}, Packs: []docs.Pack{changedPack("csi/csi-aws-ebs")}}}
		}, copy: model.CopyDiffers, want: []string{"cluster-differs csi/csi-aws-ebs"}},
		{name: "same content", setup: func(t *testing.T, w *world) {
			w.compares = map[int]docs.Diff{0: {SameContent: true, Packs: []docs.Pack{{Key: "os/ubuntu", Status: docs.PackChanged}}}}
		}, copy: model.CopySameContent},
		{name: "image id changed with same true", setup: func(t *testing.T, w *world) {
			w.compares = map[int]docs.Diff{0: {Same: true, SameContent: true, ImageIDsChanged: []docs.KeyOnly{{Key: "os/ubuntu"}}}}
		}, copy: model.CopySame, want: []string{"image-id-changed os/ubuntu"}},
		{name: "overwritten", setup: func(t *testing.T, w *world) {
			w.rows[0].Status = docs.VerifyDrifted
			w.rows[0].Diff = &docs.Diff{Verify: &docs.Verify{OverwrittenBy: "aws-stack@1.1.1"}}
		}, copy: model.CopySame, want: []string{"overwritten "}},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			r, w := oneRecord(t, unchanged())
			c.setup(t, w)
			o := r.observer(w)
			s, err := o.Once(context.Background())
			if err != nil {
				t.Fatalf("error %v", err)
			}
			var got []string
			for _, f := range s.Findings {
				got = append(got, string(f.Kind)+" "+strings.Join(f.Packs, ","))
			}
			if !slices.Equal(got, c.want) {
				t.Fatalf("findings %v, want %v", got, c.want)
			}
			if s.Profiles[0].ClusterCopy.Status != c.copy {
				t.Errorf("clusterCopy %+v", s.Profiles[0].ClusterCopy)
			}
			if len(s.Findings) > 0 && (s.Findings[0].AlsoOnCluster != c.also || len(s.Findings[0].Who) != c.who) {
				t.Errorf("finding %+v", s.Findings[0])
			}
			if s.Page != model.PageOK || s.Cluster.Name != "payments" || s.StrataVersion != "0.29.0" {
				t.Errorf("page %q, cluster %+v, strata %q", s.Page, s.Cluster, s.StrataVersion)
			}
		})
	}
}

func TestARecordForAVersionThatTheClusterDoesNotRunHasNoSide(t *testing.T) {
	r, w := oneRecord(t, unchanged())
	r.record("aws-stack", "1.0.0", "csi/csi-aws-ebs")
	w.rows = []docs.VerifyRow{unchanged(), unchanged()}
	s, _ := r.observer(w).Once(context.Background())
	// The files come in the order of their names: 1.0.0 before 1.1.0.
	if len(s.Profiles) != 2 || s.Profiles[0].Version != "1.0.0" || s.Profiles[0].ClusterCopy.Status != model.CopyNotOnThisCluster || s.Profiles[1].ClusterCopy.Status != model.CopySame {
		t.Fatalf("profiles %+v", s.Profiles)
	}
	if w.count(runner.CmdCompare) != 1 || s.Counts.Packs != 2 {
		t.Errorf("compares %d, packs %d", w.count(runner.CmdCompare), s.Counts.Packs)
	}
}

func TestAProfileWithNoRecordIsACauseAndAddsNoPack(t *testing.T) {
	r, w := oneRecord(t, unchanged())
	w.cluster = cluster(t, "payments", "aws-stack@1.1.0", "platform-addons@1.0.0")
	s, _ := r.observer(w).Once(context.Background())
	if len(s.NotChecked) != 1 || s.NotChecked[0].Cause != model.NoRecord || s.Profiles[1].Status != model.StatusNoRecord || s.Counts.Packs != 2 {
		t.Errorf("notChecked %+v, profiles %+v, counts %+v", s.NotChecked, s.Profiles, s.Counts)
	}
}

func TestAnOlderToolVersionNeedsARebaseline(t *testing.T) {
	r, w := oneRecord(t, drift(t))
	s, _ := r.observer(w).Once(context.Background())
	if len(s.Findings) != 1 || !s.Findings[0].ReBaselineNeeded {
		t.Fatalf("findings %+v", s.Findings)
	}
	w.verifyTool = "0.28.3"
	s, _ = r.observer(w).Once(context.Background())
	if len(s.Findings) != 1 || s.Findings[0].ReBaselineNeeded {
		t.Fatalf("findings %+v", s.Findings)
	}
}

func TestAnUnreadableRecordIsACauseAndStopsNoOther(t *testing.T) {
	r, w := oneRecord(t, unchanged())
	if err := os.WriteFile(filepath.Join(r.records, "zz.json.gz"), []byte("not gzip"), 0o600); err != nil {
		t.Fatal(err)
	}
	s, _ := r.observer(w).Once(context.Background())
	if len(s.NotChecked) != 1 || s.NotChecked[0].Cause != model.ReadError || s.NotChecked[0].Profiles[0] != "zz.json.gz" || s.Records.Count != 1 || s.Page != model.PageOK {
		t.Errorf("summary %+v", s)
	}
}

func TestARowWithAnErrorBesideGoodRowsIsACauseAndNotAFailedCycle(t *testing.T) {
	r, w := oneRecord(t, unchanged())
	r.record("platform-addons", "1.0.0", "addon/cert-manager")
	bad := docs.VerifyRow{Status: docs.VerifyError, Error: &docs.Error{Kind: docs.KindAPI, Status: 404}}
	w.rows = []docs.VerifyRow{unchanged(), bad}
	w.cluster = cluster(t, "payments", "aws-stack@1.1.0", "platform-addons@1.0.0")
	s, _ := r.observer(w).Once(context.Background())
	if s.Content.FailedRuns != 0 || len(s.NotChecked) != 1 || s.NotChecked[0].Cause != model.ReadError || s.NotChecked[0].Profiles[0] != "platform-addons@1.0.0" {
		t.Errorf("content %+v, notChecked %+v", s.Content, s.NotChecked)
	}
}

func TestNoRecordRunsNoVerifyAndNoBehind(t *testing.T) {
	r := newRig(t)
	w := &world{cluster: cluster(t, "payments", "aws-stack@1.1.0")}
	s, err := r.observer(w).Once(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if s.Page != model.PageNoRecords || w.count(runner.CmdVerify) != 0 || w.count(runner.CmdBehind) != 0 || s.Records.Baseline != "unknown" || s.Records.Count != 0 {
		t.Errorf("page %q, verify %d, behind %d, records %+v", s.Page, w.count(runner.CmdVerify), w.count(runner.CmdBehind), s.Records)
	}
}

func TestAMissingRecordsDirectoryFailsTheCycleWithTheKindIO(t *testing.T) {
	r := newRig(t)
	r.cfg.Records = filepath.Join(r.records, "nope")
	o := r.observer(&world{})
	err := o.RunContent(context.Background())
	var re *runner.RunError
	if !errorsAs(err, &re) || re.Kind != docs.KindIO {
		t.Fatalf("error %v", err)
	}
	if s := o.Summary(); s.Content.LastError == nil || s.Content.LastError.Kind != docs.KindIO || s.Page != model.PageFirstLoad {
		t.Errorf("summary %+v", s.Content)
	}
}

func TestACycleFailsWhenEachRowOfTheVerifyHasAnError(t *testing.T) {
	r, w := oneRecord(t, docs.VerifyRow{Status: docs.VerifyError, Error: &docs.Error{Kind: docs.KindTransport}})
	w.verifyExit = 2
	o := r.observer(w)
	if err := o.RunContent(context.Background()); err == nil {
		t.Fatal("no error")
	}
	s := o.Summary()
	if s.Content.FailedRuns != 1 || s.Content.LastError.Kind != docs.KindTransport || s.Content.CheckedAt != nil {
		t.Errorf("content %+v", s.Content)
	}
	if w.count(runner.CmdSnapshot) != 0 {
		t.Error("the cycle read the cluster after it failed")
	}
}

func TestAVerifyThatExitsTwoWithNoDocumentGivesTheKindOfItsError(t *testing.T) {
	r, w := oneRecord(t, unchanged())
	w.verifyErr = &runner.RunError{Kind: docs.KindAPI, Status: 401}
	o := r.observer(w)
	if err := o.RunContent(context.Background()); err == nil {
		t.Fatal("no error")
	}
	if e := o.Summary().Content.LastError; e == nil || e.Kind != docs.KindAPI || e.Status != 401 {
		t.Errorf("lastError %+v", e)
	}
}

func TestACycleThatFailsTwiceWithStatus401IsLost(t *testing.T) {
	r, w := oneRecord(t, unchanged())
	o := r.observer(w)
	if err := o.RunContent(context.Background()); err != nil {
		t.Fatal(err)
	}
	good := o.Summary()
	w.verifyErr = &runner.RunError{Kind: docs.KindAPI, Status: 401}
	_ = o.RunContent(context.Background())
	if s := o.Summary(); s.Page != model.PageOK || s.Content.FailedRuns != 1 || s.Profiles[0].Name != "aws-stack" {
		t.Errorf("after one failure: page %q, content %+v", s.Page, s.Content)
	}
	_ = o.RunContent(context.Background())
	s := o.Summary()
	if s.Page != model.PageLost || s.Content.FailedRuns != 2 || len(s.Profiles) != 1 || s.Content.CheckedAt == nil || !s.Content.CheckedAt.Equal(*good.Content.CheckedAt) {
		t.Errorf("page %q, content %+v", s.Page, s.Content)
	}
	// A good cycle sets the failed runs to 0.
	w.verifyErr = nil
	if err := o.RunContent(context.Background()); err != nil {
		t.Fatal(err)
	}
	if s := o.Summary(); s.Page != model.PageOK || s.Content.FailedRuns != 0 || s.Content.LastError != nil {
		t.Errorf("page %q, content %+v", s.Page, s.Content)
	}
}

func TestAWrongKeyWithNoFinishedCycleIsLost(t *testing.T) {
	r, w := oneRecord(t, docs.VerifyRow{Status: docs.VerifyError, Error: &docs.Error{Kind: docs.KindAPI, Status: 401}})
	w.verifyExit = 2
	o := r.observer(w)
	_ = o.RunContent(context.Background())
	if s := o.Summary(); s.Page != model.PageFirstLoad {
		t.Errorf("after one run: page %q", s.Page)
	}
	_ = o.RunContent(context.Background())
	s := o.Summary()
	if s.Page != model.PageLost || s.Content.LastError.Kind != docs.KindAPI || s.Content.LastError.Status != 401 {
		t.Errorf("page %q, lastError %+v", s.Page, s.Content.LastError)
	}
}

func TestASnapshotThatFailsWithAPlainLineTakesTheKindOfTheRowsOfTheVerify(t *testing.T) {
	plain := &runner.RunError{Kind: docs.KindUnknown}
	cases := []struct {
		name string
		rows []docs.VerifyRow
		want string
	}{
		{"the rows name transport", []docs.VerifyRow{unchanged(), {Status: docs.VerifyError, Error: &docs.Error{Kind: docs.KindTransport}}}, docs.KindTransport},
		{"the rows are good", []docs.VerifyRow{unchanged(), unchanged()}, docs.KindUnknown},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			r, w := oneRecord(t, unchanged())
			r.record("platform-addons", "1.0.0", "addon/cert-manager")
			w.rows, w.clusterErr = c.rows, plain
			o := r.observer(w)
			_ = o.RunContent(context.Background())
			if e := o.Summary().Content.LastError; e == nil || e.Kind != c.want {
				t.Errorf("lastError %+v", e)
			}
		})
	}
}

func TestAWrongClusterUIDIsStaleWithTheLastError(t *testing.T) {
	r, w := oneRecord(t, unchanged())
	w.clusterErr = &runner.RunError{Kind: docs.KindUnknown}
	o := r.observer(w)
	_ = o.RunContent(context.Background())
	_ = o.RunContent(context.Background())
	s := o.Summary()
	if s.Page != model.PageStale || s.Content.LastError == nil || s.Content.LastError.Kind != docs.KindUnknown || s.Content.FailedRuns != 2 {
		t.Errorf("page %q, content %+v", s.Page, s.Content)
	}
}

func TestAnExitThatIsNoAnswerIsACrash(t *testing.T) {
	r, w := oneRecord(t, unchanged())
	w.verifyExit = 3
	o := r.observer(w)
	_ = o.RunContent(context.Background())
	if e := o.Summary().Content.LastError; e == nil || e.Kind != docs.KindCrash {
		t.Errorf("lastError %+v", e)
	}
}

func TestACycleFailsWhenEverySideFails(t *testing.T) {
	r, w := oneRecord(t, unchanged())
	w.sideErr = &runner.RunError{Kind: docs.KindCrash}
	o := r.observer(w)
	if err := o.RunContent(context.Background()); err == nil {
		t.Fatal("no error")
	}
	// One side that fails beside good sides is a profile that the page says is not read.
	r.record("platform-addons", "1.0.0", "addon/cert-manager")
	w.rows = []docs.VerifyRow{unchanged(), unchanged()}
	w.cluster = cluster(t, "payments", "aws-stack@1.1.0", "platform-addons@1.0.0")
	w.sideErr = nil
	fails := &failOnce{world: w, name: "aws-stack"}
	o = r.observer(fails)
	if err := o.RunContent(context.Background()); err != nil {
		t.Fatal(err)
	}
	s := o.Summary()
	if s.Profiles[0].ClusterCopy.Status != model.CopyNotRead || s.Profiles[1].ClusterCopy.Status != model.CopySame {
		t.Errorf("profiles %+v", s.Profiles)
	}
}

// failOnce fails the side snapshot of one profile.
type failOnce struct {
	*world
	name string
}

func (f *failOnce) Run(ctx context.Context, dir string, args ...string) (runner.Result, error) {
	if args[0] == runner.CmdSnapshot && strings.HasSuffix(args[1], "/"+f.name) {
		return runner.Result{}, &runner.RunError{Kind: docs.KindCrash}
	}
	return f.world.Run(ctx, dir, args...)
}
