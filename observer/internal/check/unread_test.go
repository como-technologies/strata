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

	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/kube"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
	"github.com/como-technologies/strata/observer/internal/testkit"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

// at moves the clock of the observer and runs a content cycle.
func at(t *testing.T, o *Observer, when time.Time) model.Summary {
	t.Helper()
	o.now = func() time.Time { return when }
	cycle(t, o)
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
	f, ok := readState(t, r).Findings[id]
	return f.FirstSeen, ok
}

// keepsFirstSight fails when the state does not hold t0 as the first sight of the finding id.
func keepsFirstSight(t *testing.T, r *rig, id string) {
	t.Helper()
	if got, ok := firstSeenInState(t, r, id); !ok || !got.Equal(t0) {
		t.Errorf("the state holds the first sight %v, %v", got, ok)
	}
}

// dropsFirstSight fails when the state keeps a first sight of the finding id that a read cleared.
func dropsFirstSight(t *testing.T, r *rig, id string) {
	t.Helper()
	if _, ok := firstSeenInState(t, r, id); ok {
		t.Errorf("the state keeps the first sight of %s, which a read cleared", id)
	}
}

// readErrorNames fails when the cause read-error does not name exactly the names.
func readErrorNames(t *testing.T, s model.Summary, names ...string) {
	t.Helper()
	if got := causeOf(s, model.ReadError); !slices.Equal(got, names) {
		t.Errorf("the cause read-error names %v, want %v", got, names)
	}
}

// driftAgain fails when a read that shows the drift again does not keep its first sight, or keeps the cause read-error.
func driftAgain(t *testing.T, s model.Summary) {
	t.Helper()
	if len(s.Findings) != 1 || !s.Findings[0].FirstSeen.Equal(t0) || len(causeOf(s, model.ReadError)) != 0 {
		t.Fatalf(testkit.FindingsNotChecked, s.Findings, s.NotChecked)
	}
}

// noChangeClears fails when a read of no change does not clear the drift.
func noChangeClears(t *testing.T, s model.Summary) {
	t.Helper()
	if len(s.Findings) != 0 || s.Counts.Drift != 0 {
		t.Errorf("a read of no change did not clear the drift: %+v", s.Findings)
	}
}

// inventsNoFinding fails when a new process that read no drift shows a finding.
func inventsNoFinding(t *testing.T, s model.Summary) {
	t.Helper()
	if len(s.Findings) != 0 {
		t.Fatalf("the process invented a finding: %+v", s.Findings)
	}
}

// countsAllButKubernetes fails when the newer row of kubernetes, whose move failed, has an impact and images, or when
// another newer row has none.
func countsAllButKubernetes(t *testing.T, s model.Summary) {
	t.Helper()
	for _, n := range s.Newer {
		if hasCounts := n.Impact != "" && n.Images > 0; (n.Name == "kubernetes") == hasCounts {
			t.Errorf("%s: impact %q, images %d", n.Name, n.Impact, n.Images)
		}
	}
}

// findingsAndMessages is the failure of a check of the findings and of the messages that the observer sent.
const findingsAndMessages = "findings %+v, messages %q"

func TestAVerifyRowThatFailsKeepsTheDriftAndItsFirstSight(t *testing.T) {
	r, w := twoRecords(t)
	w.rows[0] = drift(t)
	o := r.observer(w)
	id := model.FindingID(model.PublishedChanged, testkit.AWSStackProfile)
	if s := at(t, o, t0); len(s.Findings) != 1 || !s.Findings[0].FirstSeen.Equal(t0) {
		t.Fatalf("findings %+v", s.Findings)
	}
	// One 429 for one record: the other row is good, so the cycle is good.
	w.rows[0] = testkit.NotReadRow()
	s := at(t, o, t0.Add(time.Hour))
	if len(s.Findings) != 1 || s.Findings[0].ID != id || !s.Findings[0].FirstSeen.Equal(t0) || s.Counts.Drift != 1 || s.Profiles[0].Status != model.StatusDrift {
		t.Fatalf("a failed read cleared the drift: "+testkit.FindingsCounts, s.Findings, s.Counts)
	}
	readErrorNames(t, s, testkit.AWSStackProfile)
	keepsFirstSight(t, r, id)
	// A later read that shows the drift keeps the first sight, and the cause goes.
	w.rows[0] = drift(t)
	s = at(t, o, t0.Add(2*time.Hour))
	driftAgain(t, s)
	// A later read of no change clears it.
	w.rows[0] = unchanged()
	s = at(t, o, t0.Add(3*time.Hour))
	noChangeClears(t, s)
	dropsFirstSight(t, r, id)
}

func TestASideThatFailsKeepsTheClusterDifferenceAndItsFirstSight(t *testing.T) {
	r, w := twoRecords(t)
	w.compares = map[int]docs.Diff{0: {RootB: "e06b", Summary: docs.DiffSummary{MaxImpact: "cluster-wide"}, Packs: []docs.Pack{changedPack(testkit.CSIKey)}}}
	fails := &failOnce{world: w, name: ""}
	o := r.observer(fails)
	id := model.FindingID(model.ClusterDiffers, testkit.AWSStackProfile)
	if s := at(t, o, t0); len(s.Findings) != 1 || s.Findings[0].ID != id {
		t.Fatalf("findings %+v", s.Findings)
	}
	// The side of one profile fails, and the side of the other one is read: a good cycle.
	fails.name = "aws-stack"
	s := at(t, o, t0.Add(time.Hour))
	if len(s.Findings) != 1 || s.Findings[0].ID != id || !s.Findings[0].FirstSeen.Equal(t0) || s.Profiles[0].Status != model.StatusDrift {
		t.Fatalf("a failed side cleared the drift: findings %+v", s.Findings)
	}
	readErrorNames(t, s, testkit.AWSStackProfile)
	keepsFirstSight(t, r, id)
	fails.name = ""
	s = at(t, o, t0.Add(2*time.Hour))
	driftAgain(t, s)
	w.compares = nil
	s = at(t, o, t0.Add(3*time.Hour))
	noChangeClears(t, s)
	dropsFirstSight(t, r, id)
}

func TestAFailedReadAfterARestartKeepsTheFirstSightInTheState(t *testing.T) {
	r, w := twoRecords(t)
	w.rows[0] = drift(t)
	id := model.FindingID(model.PublishedChanged, testkit.AWSStackProfile)
	at(t, r.observer(w), t0)
	// The new process has no last answer in memory.
	o := r.restart(w)
	w.rows[0] = testkit.NotReadRow()
	s := at(t, o, t0.Add(time.Hour))
	inventsNoFinding(t, s)
	readErrorNames(t, s, testkit.AWSStackProfile)
	if s.Profiles[0].Status != model.StatusNotChecked {
		t.Errorf("profile %q", s.Profiles[0].Status)
	}
	keepsFirstSight(t, r, id)
	w.rows[0] = drift(t)
	driftAgain(t, at(t, o, t0.Add(2*time.Hour)))
}

// restartWithAKeyThatFails runs a good cycle of the world edited at t0, and gives its summary. Then it restarts the
// observer with a key that Palette refuses with HTTP 401, and runs two content cycles. So the new process has no
// good content cycle, and its memory holds no answer.
func restartWithAKeyThatFails(t *testing.T) (*rig, *world, *Observer, model.Summary) {
	t.Helper()
	r, w := edited(t)
	before := at(t, r.observer(w), t0)
	if len(before.Findings) != 1 || before.Findings[0].Unread {
		t.Fatalf("before the restart: findings %+v", before.Findings)
	}
	w.verifyErr = &runner.RunError{Kind: docs.KindAPI, Status: 401}
	o := r.restart(w)
	for i := 1; i <= 2; i++ {
		o.now = func() time.Time { return t0.Add(time.Duration(i) * testkit.ContentEvery) }
		if err := o.RunContent(context.Background()); err == nil {
			t.Fatal("a content cycle with a key that fails ended well")
		}
	}
	return r, w, o, before
}

func TestARestartWithAKeyThatFailsKeepsTheFindingAndSaysThatItWasNotReadNow(t *testing.T) {
	r, w, o, before := restartWithAKeyThatFails(t)
	s := o.Summary()
	if s.Page != model.PageLost || s.Content.CheckedAt != nil || len(s.Profiles) != 0 {
		t.Fatalf(pageAndContent, s.Page, s.Content)
	}
	was := before.Findings[0]
	if len(s.Findings) != 1 || s.Counts.Findings != 1 {
		t.Fatalf("after the restart: "+testkit.FindingsCounts, s.Findings, s.Counts)
	}
	f := s.Findings[0]
	if f.ID != was.ID || f.Kind != was.Kind || f.Profile != was.Profile || !slices.Equal(f.Packs, was.Packs) || !f.FirstSeen.Equal(t0) || !f.LastSeen.Equal(t0) || !f.Unread {
		t.Errorf("after the restart the summary gives %+v, and before it %+v", f, was)
	}
	keepsFirstSight(t, r, was.ID)
	// A good read reads the finding again. It keeps its first sight, and it is not unread.
	w.verifyErr = nil
	if s := at(t, o, t0.Add(time.Hour)); len(s.Findings) != 1 || s.Findings[0].Unread || !s.Findings[0].FirstSeen.Equal(t0) {
		t.Errorf("after a good read: findings %+v", s.Findings)
	}
}

// The set lost-held of the test data for the page holds the output that a restart with a key that fails gives. The page
// shows the set, so the set must hold the fields of that output. The finding of each names its own profile, packs, and
// times, so the test drops them.
func TestARestartWithAKeyThatFailsGivesTheFieldsOfTheSetLostHeld(t *testing.T) {
	_, _, o, _ := restartWithAKeyThatFails(t)
	s := o.Summary()
	set, err := devdata.Load(os.DirFS(filepath.FromSlash("../web/testdata/api")), devdata.StateLostHeld, devdata.Reference)
	if err != nil {
		t.Fatal(err)
	}
	if c := set.Cluster; s.Page != c.Page || !reflect.DeepEqual(s.Content, c.Content) || s.Counts != c.Counts {
		t.Errorf("the observer gives %q, %+v, %+v, and the set holds %q, %+v, %+v", s.Page, s.Content, s.Counts, c.Page, c.Content, c.Counts)
	}
	if len(s.Findings) != 1 || len(set.Findings.Findings) != 1 {
		t.Fatalf("the observer gives %+v, and the set holds %+v", s.Findings, set.Findings.Findings)
	}
	got, want := s.Findings[0], set.Findings.Findings[0]
	got.ID, got.Profile, got.Packs, got.FirstSeen, got.LastSeen = want.ID, want.Profile, want.Packs, want.FirstSeen, want.LastSeen
	if !reflect.DeepEqual(got, want) {
		t.Errorf("the observer gives the finding\n%+v\nthe set holds\n%+v", got, want)
	}
}

func TestAFailedReadKeepsTheFirstSightOfThePartThatFailedOnly(t *testing.T) {
	r, w := twoRecords(t)
	w.rows[0] = drift(t)
	w.compares = map[int]docs.Diff{0: {RootB: "e06b", Packs: []docs.Pack{changedPack(testkit.CSIKey)}}}
	published := model.FindingID(model.PublishedChanged, testkit.AWSStackProfile)
	cluster := model.FindingID(model.ClusterDiffers, testkit.AWSStackProfile)
	at(t, r.observer(w), t0)
	// After a restart the row fails, and the side works and shows no change.
	o := r.restart(w)
	w.rows[0], w.compares = testkit.NotReadRow(), nil
	at(t, o, t0.Add(time.Hour))
	keepsFirstSight(t, r, published)
	dropsFirstSight(t, r, cluster)
}

func TestASideThatFailsIsNotCheckedForTheProfileAndItsPacks(t *testing.T) {
	r, w := twoRecords(t)
	o := r.observer(&failOnce{world: w, name: "aws-stack"})
	s := at(t, o, t0)
	p := s.Profiles[0]
	if p.Status != model.StatusNotChecked || p.ClusterCopy.Status != model.CopyNotRead {
		t.Errorf(testkit.ProfileCopy, p.Status, p.ClusterCopy)
	}
	for _, k := range p.Packs {
		if k.Status != model.StatusNotChecked {
			t.Errorf("pack %s is %q", k.Key, k.Status)
		}
	}
	if s.Profiles[1].Status != model.StatusOK || s.Counts.Packs != 3 || s.Counts.PacksInFull != 1 {
		t.Errorf(testkit.ProfileCounts, s.Profiles[1].Status, s.Counts)
	}
}

func TestOneMoveThatFailsLeavesTheOtherPinsComparedAndTheListNew(t *testing.T) {
	r, w := withBehind(t)
	failing := move("kubernetes", "1.35.3", "1.36.2")
	w.moveErrs[failing] = &runner.RunError{Kind: docs.KindCrash}
	o := r.observer(w)
	versions(t, o)
	s := o.Summary()
	if s.Versions.ReadAt == nil || s.Versions.FailedRuns != 0 || s.Versions.LastError != nil || len(s.Newer) != 4 || w.countMoves() != 4 {
		t.Fatalf(versionsNewer+", compares %d", s.Versions, len(s.Newer), w.countMoves())
	}
	countsAllButKubernetes(t, s)
	if len(s.NotChecked) != 1 || s.NotChecked[0].Cause != model.PinError || s.NotChecked[0].Packs[0] != testkit.KubernetesKey || s.NotChecked[0].Profiles[0] != testkit.AWSStackOld {
		t.Errorf("notChecked %+v", s.NotChecked)
	}
	// The cache holds the three moves that worked. The next cycle asks for the one that failed.
	if state := readState(t, r); len(state.Moves) != 3 {
		t.Fatalf("moves in the state %d", len(state.Moves))
	}
	delete(w.moveErrs, failing)
	before := w.countMoves()
	versions(t, o)
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
	versions(t, o)
	s := o.Summary()
	if s.Versions.ReadAt == nil || s.Versions.FailedRuns != 0 || len(s.Newer) != 4 || len(s.NotChecked) != 4 {
		t.Errorf(versionsNewer+", notChecked %d", s.Versions, len(s.Newer), len(s.NotChecked))
	}
}

func TestNoRecordLeavesTheVersionsClockEmpty(t *testing.T) {
	r := newRig(t)
	w := &world{}
	o := r.observer(w)
	versions(t, o)
	s := o.Summary()
	if w.count(runner.CmdBehind) != 0 || s.Versions.ReadAt != nil || s.Versions.FailedRuns != 0 || s.Versions.LastError != nil {
		t.Errorf("behind %d, versions %+v", w.count(runner.CmdBehind), s.Versions)
	}
}

// The set no-records of the test data for the page holds the output that the observer gives with no
// record. The page shows the set, so the set must be the output.
func TestTheRealOutputWithNoRecordIsTheSetNoRecords(t *testing.T) {
	raw := testkit.ReadFile(t, filepath.Join("..", "web", "testdata", "api", string(devdata.StateNoRecords), "cluster.json"))
	var setProfiles struct {
		Profiles []model.Profile `json:"profiles"`
	}
	testkit.DecodeFile(t, filepath.Join("..", "web", "testdata", "api", string(devdata.StateNoRecords), "profiles.json"), &setProfiles)
	var names []string
	for _, p := range setProfiles.Profiles {
		names = append(names, model.Subject(p.Name, p.Version))
	}
	r := newRig(t)
	w := &world{cluster: testkit.Cluster("quarry-1", names...)}
	s := once(t, r.observer(w))
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
	testkit.HoldsAll(t, "the set", string(raw), `"readAt":null`)
}

func asMap(t *testing.T, v any) map[string]any {
	t.Helper()
	return testkit.Plain(t, v).(map[string]any)
}

// breakRecord overwrites the file of a record with a file that the observer cannot read.
func breakRecord(t *testing.T, r *rig, how string) {
	t.Helper()
	switch how {
	case "a bad gzip":
		testkit.WriteIn(t, r.records, testkit.RecordFile, "not gzip")
	default:
		testkit.WriteGzip(t, r.records, testkit.RecordFile, testkit.Record("strata-snapshot/2", "aws-stack", "1.1.0", "", ""))
	}
}

func TestAnUnreadableRecordKeepsItsFindingAndItsFirstSightAndSendsNoClear(t *testing.T) {
	for _, how := range []string{"a bad gzip", "another schema"} {
		t.Run(how, func(t *testing.T) {
			r, w := twoRecords(t)
			w.rows[0] = drift(t)
			sender := &capture{}
			o := alerting(t, r, w, sender)
			id := model.FindingID(model.PublishedChanged, testkit.AWSStackProfile)
			at(t, o, t0)
			if len(sender.texts) != 1 {
				t.Fatalf(testkit.OpenMessage, sender.texts)
			}
			// The file of the drifted record is not a record any more. The row of the other record is the only one.
			breakRecord(t, r, how)
			w.rows = []docs.VerifyRow{unchanged()}
			s := at(t, o, t0.Add(time.Hour))
			if len(s.Findings) != 1 || s.Findings[0].ID != id || !s.Findings[0].FirstSeen.Equal(t0) || s.Profiles[0].Status != model.StatusDrift {
				t.Fatalf("an unreadable record cleared the finding: findings %+v", s.Findings)
			}
			readErrorNames(t, s, testkit.AWSStackProfile, testkit.RecordFile)
			keepsFirstSight(t, r, id)
			if len(sender.texts) != 1 {
				t.Fatalf("an unreadable record sent %q", sender.texts[1:])
			}
			// A later good read decides.
			awsRecord(r)
			w.rows = []docs.VerifyRow{unchanged(), unchanged()}
			noChangeClears(t, at(t, o, t0.Add(2*time.Hour)))
			if len(sender.texts) != 2 || !strings.Contains(sender.texts[1], "Drift cleared") {
				t.Errorf("messages %q", sender.texts)
			}
			dropsFirstSight(t, r, id)
		})
	}
}

func TestAnUnreadableRecordWithNoLastRecordKeepsTheFirstSightsAndSendsNoClear(t *testing.T) {
	r, w := twoRecords(t)
	w.rows = []docs.VerifyRow{drift(t), drift(t)}
	sender := &capture{}
	at(t, alerting(t, r, w, sender), t0)
	broken := model.FindingID(model.PublishedChanged, testkit.AWSStackProfile)
	readable := model.FindingID(model.PublishedChanged, testkit.AddonsProfile)
	if len(sender.texts) != 2 {
		t.Fatalf("the open messages %q", sender.texts)
	}
	// A new process has no last record in memory. One file is bad, and the record that is read shows no change.
	breakRecord(t, r, "a bad gzip")
	w.rows = []docs.VerifyRow{unchanged()}
	o := alerting(t, r, w, sender)
	s := at(t, o, t0.Add(time.Hour))
	inventsNoFinding(t, s)
	keepsFirstSight(t, r, broken)
	// The record that was read decides for its own profile, so its clear goes. The unreadable file decides for none.
	dropsFirstSight(t, r, readable)
	if len(sender.texts) != 3 || !strings.Contains(sender.texts[2], "Drift cleared\n"+testkit.AddonsProfile) {
		t.Fatalf("messages %q", sender.texts)
	}
	// The record is read again and the drift is still there: the first sight is the first one, and nothing goes again.
	awsRecord(r)
	w.rows = []docs.VerifyRow{drift(t), unchanged()}
	driftAgain(t, at(t, o, t0.Add(2*time.Hour)))
	if len(sender.texts) != 3 {
		t.Errorf("messages %q", sender.texts)
	}
	// A read of no change decides.
	w.rows = []docs.VerifyRow{unchanged(), unchanged()}
	at(t, o, t0.Add(3*time.Hour))
	if len(sender.texts) != 4 || !strings.Contains(sender.texts[3], paymentsCleared) {
		t.Errorf("messages %q", sender.texts)
	}
}

func TestAFailedReadOfMetaJSONKeepsEachFirstDigestAndARealNewRevisionResetsThem(t *testing.T) {
	r, _, o, pods := podRig(t, runs("web-1", testkit.Digest1))
	cycle(t, o)
	pods.set([]kube.Container{runs("web-1", testkit.Digest2)}, nil)
	cycle(t, o)
	if got := imageKindsOf(o.Summary()); !slices.Equal(got, []model.Kind{model.ImageMoved}) {
		t.Fatalf("findings %v", got)
	}
	// The file is gone for one cycle, as when the kubelet updates the Secret.
	if err := os.Remove(filepath.Join(r.records, baseline.MetaFile)); err != nil {
		t.Fatal(err)
	}
	cycle(t, o)
	if got := imageKindsOf(o.Summary()); !slices.Equal(got, []model.Kind{model.ImageMoved}) {
		t.Errorf(findingsLeft, "a failed read of "+baseline.MetaFile, got)
	}
	st := readState(t, r)
	if st.Images.Revision != testkit.Revision || st.Images.First[model.ImageKey(testkit.ImageRef)] != testkit.Digest1 {
		t.Errorf("a failed read of "+baseline.MetaFile+" reset the state: %+v", st.Images)
	}
	// The file comes back with the same revision, and the finding holds.
	r.meta(testkit.ApprovedMeta(testkit.Revision))
	cycle(t, o)
	if got := imageKindsOf(o.Summary()); !slices.Equal(got, []model.Kind{model.ImageMoved}) {
		t.Errorf("findings %v", got)
	}
	// A real new revision resets the first digests.
	r.meta(testkit.ApprovedMeta(testkit.NewRevision))
	cycle(t, o)
	st = readState(t, r)
	if got := imageKindsOf(o.Summary()); len(got) != 0 || st.Images.Revision != testkit.NewRevision || st.Images.First[model.ImageKey(testkit.ImageRef)] != testkit.Digest2 {
		t.Errorf("findings %v, state %+v", got, st.Images)
	}
}
