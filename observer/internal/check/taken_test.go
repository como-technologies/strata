package check

import (
	"context"
	"slices"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// firstSeenWithANewProfile makes a world with first-seen records of aws-stack@1.1.0, and a cluster
// that also runs ngp-f5@1.0.0, which no record holds: the pipeline of the team added it after the seed.
func firstSeenWithANewProfile(t *testing.T) (*rig, *world) {
	t.Helper()
	r, w := oneRecord(t, unchanged())
	r.meta(`{"baseline":"first-seen","firstSeenFrom":"c1"}`)
	w.rows = []docs.VerifyRow{unchanged(), unchanged()}
	w.cluster = testkit.Cluster("payments", "aws-stack@1.1.0", "ngp-f5@1.0.0")
	return r, w
}

func profileStatus(s model.Summary, subject string) model.Status {
	for _, p := range s.Profiles {
		if model.Subject(p.Name, p.Version) == subject {
			return p.Status
		}
	}
	return ""
}

func snapshotsOf(w *world, ref string) int {
	w.mu.Lock()
	defer w.mu.Unlock()
	n := 0
	for _, c := range w.calls {
		if c[0] == runner.CmdSnapshot && c[1] == ref {
			n++
		}
	}
	return n
}

// While the records are first-seen, a content cycle records a profile version that the cluster runs and
// that no record holds, and starts the next cycle at once. That cycle checks it as a record, so no
// cause no-record stays. A later cycle takes no second snapshot of it.
func TestFirstSeenRecordsGetARecordOfAProfileThatTheClusterRunsAfterTheSeed(t *testing.T) {
	r, w := firstSeenWithANewProfile(t)
	o := r.observer(w)
	if err := o.RunContent(context.Background()); err != nil {
		t.Fatal(err)
	}
	if s := o.Summary(); profileStatus(s, "ngp-f5@1.0.0") != model.StatusNoRecord {
		t.Fatalf("the first cycle: %q", profileStatus(s, "ngp-f5@1.0.0"))
	}
	if snapshotsOf(w, "ngp-f5@1.0.0") != 1 || len(o.kicks[cycleContent]) != 1 {
		t.Fatalf("snapshots %d, tokens %d", snapshotsOf(w, "ngp-f5@1.0.0"), len(o.kicks[cycleContent]))
	}
	if !strings.Contains(r.log.String(), "verdict=taken records=1") {
		t.Errorf("the log:\n%s", r.log.String())
	}
	if err := o.RunContent(context.Background()); err != nil {
		t.Fatal(err)
	}
	s := o.Summary()
	if st := profileStatus(s, "ngp-f5@1.0.0"); st == model.StatusNoRecord || st == "" {
		t.Errorf("the second cycle: %q", st)
	}
	if slices.ContainsFunc(s.NotChecked, func(n model.NotChecked) bool { return n.Cause == model.NoRecord }) {
		t.Errorf("not checked: %+v", s.NotChecked)
	}
	if snapshotsOf(w, "ngp-f5@1.0.0") != 1 {
		t.Errorf("the second cycle took a second snapshot")
	}
	if s.Records.Baseline != "first-seen" {
		t.Errorf("baseline %q", s.Records.Baseline)
	}
}

// Records that a pipeline approved are the pipeline's: the observer records nothing, and the profile
// stays no-record until the pipeline publishes its record.
func TestApprovedRecordsGetNoRecordFromTheObserver(t *testing.T) {
	r, w := firstSeenWithANewProfile(t)
	r.meta(`{"baseline":"approved","revision":"4e1c7a2"}`)
	o := r.observer(w)
	for range 2 {
		if err := o.RunContent(context.Background()); err != nil {
			t.Fatal(err)
		}
	}
	if snapshotsOf(w, "ngp-f5@1.0.0") != 0 || len(o.kicks[cycleContent]) != 0 {
		t.Errorf("snapshots %d, tokens %d", snapshotsOf(w, "ngp-f5@1.0.0"), len(o.kicks[cycleContent]))
	}
	if st := profileStatus(o.Summary(), "ngp-f5@1.0.0"); st != model.StatusNoRecord {
		t.Errorf("status %q", st)
	}
}

// A snapshot that fails keeps the content cycle good. The profile stays no-record, and the next cycle
// tries again. A record that the records of the Secrets now hold drops what the observer took.
func TestAFailedSnapshotKeepsTheCycleGoodAndARecordOfTheSecretsWins(t *testing.T) {
	r, w := firstSeenWithANewProfile(t)
	w.profileErr = &runner.RunError{Kind: docs.KindAPI, Status: 429}
	o := r.observer(w)
	if err := o.RunContent(context.Background()); err != nil {
		t.Fatalf("a failed snapshot failed the cycle: %v", err)
	}
	if len(o.kicks[cycleContent]) != 0 || !strings.Contains(r.log.String(), "verdict=failed kind=api status=429") {
		t.Fatalf("tokens %d, the log:\n%s", len(o.kicks[cycleContent]), r.log.String())
	}
	w.profileErr = nil
	if err := o.RunContent(context.Background()); err != nil {
		t.Fatal(err)
	}
	if snapshotsOf(w, "ngp-f5@1.0.0") != 2 || len(o.taken) != 1 {
		t.Fatalf("snapshots %d, taken %d", snapshotsOf(w, "ngp-f5@1.0.0"), len(o.taken))
	}
	// the pipeline, or a new seed, writes the record into the Secrets
	r.record("ngp-f5", "1.0.0", "addon/f5")
	if err := o.RunContent(context.Background()); err != nil {
		t.Fatal(err)
	}
	if len(o.taken) != 0 {
		t.Errorf("the observer keeps %d records that the Secrets hold", len(o.taken))
	}
}

// The versions cycle reads the newer versions of the records that the observer took too.
func TestTheVersionsCycleReadsTheRecordsThatTheObserverTook(t *testing.T) {
	r, w := firstSeenWithANewProfile(t)
	o := r.observer(w)
	if err := o.RunContent(context.Background()); err != nil {
		t.Fatal(err)
	}
	if err := o.RunVersions(context.Background()); err != nil {
		t.Fatal(err)
	}
	if n := w.count(runner.CmdBehind); n != 2 {
		t.Errorf("behind ran %d times, want one for each of the two records", n)
	}
}
