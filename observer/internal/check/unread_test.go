package check

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
)

// notRead is the row of a record that Palette refused to answer for, such as with a 429.
func notRead() docs.VerifyRow {
	return docs.VerifyRow{Status: docs.VerifyError, Error: &docs.Error{Kind: docs.KindAPI, Status: 429}}
}

// at moves the clock of the observer and runs a content cycle.
func at(t *testing.T, o *Observer, when time.Time) model.Summary {
	t.Helper()
	o.now = func() time.Time { return when }
	if err := o.RunContent(context.Background()); err != nil {
		t.Fatalf("the cycle failed: %v", err)
	}
	return o.Summary()
}

// causeOf gives the profiles that the rows of a cause name.
func causeOf(s model.Summary, cause model.Cause) []string {
	var out []string
	for _, row := range s.NotChecked {
		if row.Cause == cause {
			out = append(out, row.Profiles...)
		}
	}
	return out
}

func firstSeenInState(t *testing.T, r *rig, id string) (time.Time, bool) {
	t.Helper()
	state, err := loadState(r.state)
	if err != nil {
		t.Fatal(err)
	}
	f, ok := state.Findings[id]
	return f.FirstSeen, ok
}

func TestAVerifyRowThatFailsKeepsTheDriftAndItsFirstSight(t *testing.T) {
	r, w := twoRecords(t)
	w.rows[0] = drift(t)
	o := r.observer(w)
	id := model.FindingID(model.PublishedChanged, "aws-stack@1.1.0")
	if s := at(t, o, t0); len(s.Findings) != 1 || !s.Findings[0].FirstSeen.Equal(t0) {
		t.Fatalf("findings %+v", s.Findings)
	}
	// One 429 for one record: the other row is good, so the cycle is good.
	w.rows[0] = notRead()
	s := at(t, o, t0.Add(time.Hour))
	if len(s.Findings) != 1 || s.Findings[0].ID != id || !s.Findings[0].FirstSeen.Equal(t0) || s.Counts.Drift != 1 || s.Profiles[0].Status != model.StatusDrift {
		t.Fatalf("a failed read cleared the drift: findings %+v, counts %+v", s.Findings, s.Counts)
	}
	if got := causeOf(s, model.ReadError); len(got) != 1 || got[0] != "aws-stack@1.1.0" {
		t.Errorf("the cause read-error names %v", got)
	}
	if got, ok := firstSeenInState(t, r, id); !ok || !got.Equal(t0) {
		t.Errorf("the state holds the first sight %v, %v", got, ok)
	}
	// A later read that shows the drift keeps the first sight, and the cause goes.
	w.rows[0] = drift(t)
	s = at(t, o, t0.Add(2*time.Hour))
	if len(s.Findings) != 1 || !s.Findings[0].FirstSeen.Equal(t0) || len(causeOf(s, model.ReadError)) != 0 {
		t.Fatalf("findings %+v, notChecked %+v", s.Findings, s.NotChecked)
	}
	// A later read of no change clears it.
	w.rows[0] = unchanged()
	s = at(t, o, t0.Add(3*time.Hour))
	if len(s.Findings) != 0 || s.Counts.Drift != 0 {
		t.Errorf("a read of no change did not clear the drift: %+v", s.Findings)
	}
	if _, ok := firstSeenInState(t, r, id); ok {
		t.Error("the state keeps the first sight of a drift that a read cleared")
	}
}

func TestASideThatFailsKeepsTheClusterDifferenceAndItsFirstSight(t *testing.T) {
	r, w := twoRecords(t)
	w.compares = map[int]docs.Diff{0: {RootB: "e06b", Summary: docs.DiffSummary{MaxImpact: "cluster-wide"}, Packs: []docs.Pack{changedPack("csi/csi-aws-ebs")}}}
	fails := &failOnce{world: w, name: ""}
	o := r.observer(fails)
	id := model.FindingID(model.ClusterDiffers, "aws-stack@1.1.0")
	if s := at(t, o, t0); len(s.Findings) != 1 || s.Findings[0].ID != id {
		t.Fatalf("findings %+v", s.Findings)
	}
	// The side of one profile fails, and the side of the other one is read: a good cycle.
	fails.name = "aws-stack"
	s := at(t, o, t0.Add(time.Hour))
	if len(s.Findings) != 1 || s.Findings[0].ID != id || !s.Findings[0].FirstSeen.Equal(t0) || s.Profiles[0].Status != model.StatusDrift {
		t.Fatalf("a failed side cleared the drift: findings %+v", s.Findings)
	}
	if got := causeOf(s, model.ReadError); len(got) != 1 || got[0] != "aws-stack@1.1.0" {
		t.Errorf("the cause read-error names %v", got)
	}
	if got, ok := firstSeenInState(t, r, id); !ok || !got.Equal(t0) {
		t.Errorf("the state holds the first sight %v, %v", got, ok)
	}
	fails.name = ""
	s = at(t, o, t0.Add(2*time.Hour))
	if len(s.Findings) != 1 || !s.Findings[0].FirstSeen.Equal(t0) || len(causeOf(s, model.ReadError)) != 0 {
		t.Fatalf("findings %+v, notChecked %+v", s.Findings, s.NotChecked)
	}
	w.compares = nil
	s = at(t, o, t0.Add(3*time.Hour))
	if len(s.Findings) != 0 {
		t.Errorf("a read of no change did not clear the drift: %+v", s.Findings)
	}
	if _, ok := firstSeenInState(t, r, id); ok {
		t.Error("the state keeps the first sight of a drift that a read cleared")
	}
}

func TestAFailedReadAfterARestartKeepsTheFirstSightInTheState(t *testing.T) {
	r, w := twoRecords(t)
	w.rows[0] = drift(t)
	id := model.FindingID(model.PublishedChanged, "aws-stack@1.1.0")
	at(t, r.observer(w), t0)
	// The new process has no last answer in memory.
	o := r.restart(w)
	w.rows[0] = notRead()
	s := at(t, o, t0.Add(time.Hour))
	if len(s.Findings) != 0 {
		t.Fatalf("the process invented a finding: %+v", s.Findings)
	}
	if got := causeOf(s, model.ReadError); len(got) != 1 || got[0] != "aws-stack@1.1.0" || s.Profiles[0].Status != model.StatusNotChecked {
		t.Errorf("notChecked %+v, profile %q", s.NotChecked, s.Profiles[0].Status)
	}
	if got, ok := firstSeenInState(t, r, id); !ok || !got.Equal(t0) {
		t.Fatalf("the state holds the first sight %v, %v", got, ok)
	}
	w.rows[0] = drift(t)
	s = at(t, o, t0.Add(2*time.Hour))
	if len(s.Findings) != 1 || !s.Findings[0].FirstSeen.Equal(t0) {
		t.Errorf("a read that shows the drift lost the first sight: %+v", s.Findings)
	}
}

func TestAFailedReadKeepsTheFirstSightOfThePartThatFailedOnly(t *testing.T) {
	r, w := twoRecords(t)
	w.rows[0] = drift(t)
	w.compares = map[int]docs.Diff{0: {RootB: "e06b", Packs: []docs.Pack{changedPack("csi/csi-aws-ebs")}}}
	published := model.FindingID(model.PublishedChanged, "aws-stack@1.1.0")
	cluster := model.FindingID(model.ClusterDiffers, "aws-stack@1.1.0")
	at(t, r.observer(w), t0)
	// After a restart the row fails, and the side works and shows no change.
	o := r.restart(w)
	w.rows[0], w.compares = notRead(), nil
	at(t, o, t0.Add(time.Hour))
	if _, ok := firstSeenInState(t, r, published); !ok {
		t.Error("the first sight of the row that failed is gone")
	}
	if _, ok := firstSeenInState(t, r, cluster); ok {
		t.Error("the first sight of the side that a read cleared stays")
	}
}

func TestASideThatFailsIsNotCheckedForTheProfileAndItsPacks(t *testing.T) {
	r, w := twoRecords(t)
	o := r.observer(&failOnce{world: w, name: "aws-stack"})
	s := at(t, o, t0)
	p := s.Profiles[0]
	if p.Status != model.StatusNotChecked || p.ClusterCopy.Status != model.CopyNotRead {
		t.Errorf("profile %q, clusterCopy %+v", p.Status, p.ClusterCopy)
	}
	for _, k := range p.Packs {
		if k.Status != model.StatusNotChecked {
			t.Errorf("pack %s is %q", k.Key, k.Status)
		}
	}
	if s.Profiles[1].Status != model.StatusOK || s.Counts.Packs != 3 || s.Counts.PacksInFull != 1 {
		t.Errorf("profile %q, counts %+v", s.Profiles[1].Status, s.Counts)
	}
}

func TestOneMoveThatFailsLeavesTheOtherPinsComparedAndTheListNew(t *testing.T) {
	r, w := withBehind(t)
	failing := move("kubernetes", "1.35.3", "1.36.2")
	w.moveErrs[failing] = &runner.RunError{Kind: docs.KindCrash}
	o := r.observer(w)
	if err := o.RunVersions(context.Background()); err != nil {
		t.Fatalf("a move that fails failed the cycle: %v", err)
	}
	s := o.Summary()
	if s.Versions.ReadAt == nil || s.Versions.FailedRuns != 0 || s.Versions.LastError != nil || len(s.Newer) != 4 || w.countMoves() != 4 {
		t.Fatalf("versions %+v, newer %d, compares %d", s.Versions, len(s.Newer), w.countMoves())
	}
	for _, n := range s.Newer {
		hasCounts := n.Impact != "" && n.Images > 0
		if (n.Name == "kubernetes") == hasCounts {
			t.Errorf("%s: impact %q, images %d", n.Name, n.Impact, n.Images)
		}
	}
	if len(s.NotChecked) != 1 || s.NotChecked[0].Cause != model.PinError || s.NotChecked[0].Packs[0] != "k8s/kubernetes" || s.NotChecked[0].Profiles[0] != "aws-stack@1.0.0" {
		t.Errorf("notChecked %+v", s.NotChecked)
	}
	// The cache holds the three moves that worked. The next cycle asks for the one that failed.
	state, err := loadState(r.state)
	if err != nil || len(state.Moves) != 3 {
		t.Fatalf("moves in the state %d, error %v", len(state.Moves), err)
	}
	delete(w.moveErrs, failing)
	before := w.countMoves()
	if err := o.RunVersions(context.Background()); err != nil {
		t.Fatal(err)
	}
	if got := w.countMoves() - before; got != 1 {
		t.Errorf("the second cycle asked for %d moves, want 1", got)
	}
	if s := o.Summary(); len(s.NotChecked) != 0 || s.Versions.FailedRuns != 0 {
		t.Errorf("notChecked %+v", s.NotChecked)
	}
}

func TestACycleWhereEveryMoveFailsIsStillAGoodCycle(t *testing.T) {
	r, w := withBehind(t)
	for key := range w.moves {
		w.moveErrs[key] = &runner.RunError{Kind: docs.KindAPI, Status: 503}
	}
	o := r.observer(w)
	if err := o.RunVersions(context.Background()); err != nil {
		t.Fatal(err)
	}
	s := o.Summary()
	if s.Versions.ReadAt == nil || s.Versions.FailedRuns != 0 || len(s.Newer) != 4 || len(s.NotChecked) != 4 {
		t.Errorf("versions %+v, newer %d, notChecked %d", s.Versions, len(s.Newer), len(s.NotChecked))
	}
}

func TestNoRecordLeavesTheVersionsClockEmpty(t *testing.T) {
	r := newRig(t)
	w := &world{}
	o := r.observer(w)
	if err := o.RunVersions(context.Background()); err != nil {
		t.Fatal(err)
	}
	s := o.Summary()
	if w.count(runner.CmdBehind) != 0 || s.Versions.ReadAt != nil || s.Versions.FailedRuns != 0 || s.Versions.LastError != nil {
		t.Errorf("behind %d, versions %+v", w.count(runner.CmdBehind), s.Versions)
	}
}

// The set no-records of the test data for the page holds the output that the observer gives with no
// record. The page shows the set, so the set must be the output.
func TestTheRealOutputWithNoRecordIsTheSetNoRecords(t *testing.T) {
	raw, err := os.ReadFile(filepath.Join("..", "web", "testdata", "api", "no-records", "cluster.json"))
	if err != nil {
		t.Fatal(err)
	}
	var setProfiles struct {
		Profiles []model.Profile `json:"profiles"`
	}
	profiles, err := os.ReadFile(filepath.Join("..", "web", "testdata", "api", "no-records", "profiles.json"))
	if err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(profiles, &setProfiles); err != nil {
		t.Fatal(err)
	}
	var names []string
	for _, p := range setProfiles.Profiles {
		names = append(names, model.Subject(p.Name, p.Version))
	}
	r := newRig(t)
	w := &world{cluster: cluster(t, "quarry-1", names...)}
	s, err := r.observer(w).Once(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	got, want := asMap(t, s), asMap(t, json.RawMessage(raw))
	// A run names its own times and cluster.
	for _, m := range []map[string]any{got, want} {
		delete(m["cluster"].(map[string]any), "uid")
		delete(m["records"].(map[string]any), "startedAt")
		delete(m["content"].(map[string]any), "checkedAt")
	}
	for key, wantValue := range want {
		if !reflect.DeepEqual(got[key], wantValue) {
			t.Errorf("%s: the observer gives\n%v\nthe set holds\n%v", key, got[key], wantValue)
		}
	}
	if !strings.Contains(string(raw), `"readAt":null`) {
		t.Error("the set does not say readAt null")
	}
}

func asMap(t *testing.T, v any) map[string]any {
	t.Helper()
	raw, ok := v.(json.RawMessage)
	if !ok {
		var err error
		if raw, err = json.Marshal(v); err != nil {
			t.Fatal(err)
		}
	}
	var out map[string]any
	if err := json.Unmarshal(raw, &out); err != nil {
		t.Fatal(err)
	}
	return out
}
