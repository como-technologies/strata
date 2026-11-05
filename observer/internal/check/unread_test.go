package check

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/kube"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
	"github.com/como-technologies/strata/observer/internal/testkit"
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
	w := &world{cluster: testkit.Cluster("quarry-1", names...)}
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

// breakRecord overwrites the file of a record with a file that the observer cannot read.
func breakRecord(t *testing.T, r *rig, how string) {
	t.Helper()
	const file = "aws-stack@1.1.0.json.gz"
	switch how {
	case "a bad gzip":
		if err := os.WriteFile(filepath.Join(r.records, file), []byte("not gzip"), 0o600); err != nil {
			t.Fatal(err)
		}
	default:
		testkit.WriteGzip(t, r.records, file, `{"schema":"strata-snapshot/2","source":{"name":"aws-stack","version":"1.1.0"}}`)
	}
}

// restoreRecord writes the record of aws-stack again, as the pipeline does after it fixed the file.
func restoreRecord(r *rig) {
	r.record("aws-stack", "1.1.0", "csi/csi-aws-ebs", "os/ubuntu")
}

func TestAnUnreadableRecordKeepsItsFindingAndItsFirstSightAndSendsNoClear(t *testing.T) {
	for _, how := range []string{"a bad gzip", "another schema"} {
		t.Run(how, func(t *testing.T) {
			r, w := twoRecords(t)
			w.rows[0] = drift(t)
			sender := &capture{}
			o := alerting(t, r, w, sender)
			id := model.FindingID(model.PublishedChanged, "aws-stack@1.1.0")
			at(t, o, t0)
			if len(sender.texts) != 1 {
				t.Fatalf("the open message %q", sender.texts)
			}
			// The file of the drifted record is not a record any more. The row of the other record is the only one.
			breakRecord(t, r, how)
			w.rows = []docs.VerifyRow{unchanged()}
			s := at(t, o, t0.Add(time.Hour))
			if len(s.Findings) != 1 || s.Findings[0].ID != id || !s.Findings[0].FirstSeen.Equal(t0) || s.Profiles[0].Status != model.StatusDrift {
				t.Fatalf("an unreadable record cleared the finding: findings %+v", s.Findings)
			}
			if got := causeOf(s, model.ReadError); !slices.Contains(got, "aws-stack@1.1.0") || !slices.Contains(got, "aws-stack@1.1.0.json.gz") {
				t.Errorf("the cause read-error names %v", got)
			}
			if got, ok := firstSeenInState(t, r, id); !ok || !got.Equal(t0) {
				t.Errorf("the state holds the first sight %v, %v", got, ok)
			}
			if len(sender.texts) != 1 {
				t.Fatalf("an unreadable record sent %q", sender.texts[1:])
			}
			// A later good read decides.
			restoreRecord(r)
			w.rows = []docs.VerifyRow{unchanged(), unchanged()}
			s = at(t, o, t0.Add(2*time.Hour))
			if len(s.Findings) != 0 || len(sender.texts) != 2 || !strings.Contains(sender.texts[1], "Drift cleared") {
				t.Errorf("findings %+v, messages %q", s.Findings, sender.texts)
			}
			if _, ok := firstSeenInState(t, r, id); ok {
				t.Error("the state keeps the first sight of a drift that a read cleared")
			}
		})
	}
}

func TestAnUnreadableRecordWithNoLastRecordKeepsTheFirstSightsAndSendsNoClear(t *testing.T) {
	r, w := twoRecords(t)
	w.rows = []docs.VerifyRow{drift(t), drift(t)}
	sender := &capture{}
	at(t, alerting(t, r, w, sender), t0)
	broken := model.FindingID(model.PublishedChanged, "aws-stack@1.1.0")
	readable := model.FindingID(model.PublishedChanged, "platform-addons@1.0.0")
	if len(sender.texts) != 2 {
		t.Fatalf("the open messages %q", sender.texts)
	}
	// A new process has no last record in memory. One file is bad, and the record that is read shows no change.
	breakRecord(t, r, "a bad gzip")
	w.rows = []docs.VerifyRow{unchanged()}
	o := alerting(t, r, w, sender)
	s := at(t, o, t0.Add(time.Hour))
	if len(s.Findings) != 0 {
		t.Fatalf("the process invented a finding: %+v", s.Findings)
	}
	if got, ok := firstSeenInState(t, r, broken); !ok || !got.Equal(t0) {
		t.Errorf("the state holds the first sight %v, %v", got, ok)
	}
	// The record that was read decides for its own profile, so its clear goes. The unreadable file decides for none.
	if _, ok := firstSeenInState(t, r, readable); ok {
		t.Error("the state keeps the first sight of a profile that a record clears")
	}
	if len(sender.texts) != 3 || !strings.Contains(sender.texts[2], "Drift cleared\nplatform-addons@1.0.0") {
		t.Fatalf("messages %q", sender.texts)
	}
	// The record is read again and the drift is still there: the first sight is the first one, and nothing goes again.
	restoreRecord(r)
	w.rows = []docs.VerifyRow{drift(t), unchanged()}
	s = at(t, o, t0.Add(2*time.Hour))
	if len(s.Findings) != 1 || !s.Findings[0].FirstSeen.Equal(t0) || len(sender.texts) != 3 {
		t.Errorf("findings %+v, messages %q", s.Findings, sender.texts)
	}
	// A read of no change decides.
	w.rows = []docs.VerifyRow{unchanged(), unchanged()}
	at(t, o, t0.Add(3*time.Hour))
	if len(sender.texts) != 4 || !strings.Contains(sender.texts[3], "Drift cleared\naws-stack@1.1.0") {
		t.Errorf("messages %q", sender.texts)
	}
}

func TestAFailedReadOfMetaJSONKeepsEachFirstDigestAndARealNewRevisionResetsThem(t *testing.T) {
	r, _, o, pods := podRig(t, runs("web-1", digest1))
	cycle(t, o)
	pods.set([]kube.Container{runs("web-1", digest2)}, nil)
	cycle(t, o)
	if got := imageKindsOf(o.Summary()); !slices.Equal(got, []model.Kind{model.ImageMoved}) {
		t.Fatalf("findings %v", got)
	}
	// The file is gone for one cycle, as when the kubelet updates the Secret.
	if err := os.Remove(filepath.Join(r.records, "meta.json")); err != nil {
		t.Fatal(err)
	}
	cycle(t, o)
	if got := imageKindsOf(o.Summary()); !slices.Equal(got, []model.Kind{model.ImageMoved}) {
		t.Errorf("a failed read of meta.json leaves the findings %v", got)
	}
	st := readState(t, r)
	if st.Images.Revision != "4e1c7a2" || st.Images.First[model.ImageKey(imageRef)] != digest1 {
		t.Errorf("a failed read of meta.json reset the state: %+v", st.Images)
	}
	// The file comes back with the same revision, and the finding holds.
	r.meta(`{"baseline":"approved","revision":"4e1c7a2"}`)
	cycle(t, o)
	if got := imageKindsOf(o.Summary()); !slices.Equal(got, []model.Kind{model.ImageMoved}) {
		t.Errorf("findings %v", got)
	}
	// A real new revision resets the first digests.
	r.meta(`{"baseline":"approved","revision":"5f2d8b1"}`)
	cycle(t, o)
	st = readState(t, r)
	if got := imageKindsOf(o.Summary()); len(got) != 0 || st.Images.Revision != "5f2d8b1" || st.Images.First[model.ImageKey(imageRef)] != digest2 {
		t.Errorf("findings %v, state %+v", got, st.Images)
	}
}
