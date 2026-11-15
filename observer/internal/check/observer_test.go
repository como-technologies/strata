package check

import (
	"context"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// edited is a world with a record that drifted, and a compare with the cluster that holds a clear value.
func edited(t *testing.T) (*rig, *world) {
	t.Helper()
	r, w := oneRecord(t, drift(t))
	w.compares = map[int]docs.Diff{0: {RootB: w.rows[0].RootLive, Packs: []docs.Pack{changedPack(testkit.CSIKey)}}}
	return r, w
}

func TestWorkIsEmptyAfterACycleAndAfterAFailedCycle(t *testing.T) {
	cases := []struct {
		name  string
		setup func(w *world)
		fails bool
	}{
		{"a good cycle", func(w *world) {}, false},
		{"a verify that fails", func(w *world) { w.verifyErr = &runner.RunError{Kind: docs.KindAPI, Status: 500} }, true},
		{"a snapshot of the cluster that fails", func(w *world) { w.clusterErr = &runner.RunError{Kind: docs.KindUnknown} }, true},
		{"every side that fails", func(w *world) { w.sideErr = &runner.RunError{Kind: docs.KindCrash} }, true},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			r, w := edited(t)
			c.setup(w)
			o := r.observer(w)
			err := o.RunContent(context.Background())
			if (err != nil) != c.fails {
				t.Fatalf("error %v", err)
			}
			if names := entries(t, r.work); len(names) != 0 {
				t.Errorf("work holds %v", names)
			}
			w.mu.Lock()
			defer w.mu.Unlock()
			if len(w.dirs) == 0 {
				t.Fatal("no process ran")
			}
			for i, d := range w.dirs {
				if filepath.Dir(d) != r.work || !strings.HasPrefix(filepath.Base(d), cyclePrefix) || w.modes[i].Perm() != 0o700 {
					t.Errorf("the directory of a cycle is %s, mode %v", d, w.modes[i])
				}
			}
		})
	}
}

func TestWorkIsEmptyAfterAVersionsCycleToo(t *testing.T) {
	for _, fail := range []bool{false, true} {
		r, w := withBehind(t)
		if fail {
			w.behindErr = &runner.RunError{Kind: docs.KindTransport}
		}
		_ = r.observer(w).RunVersions(context.Background())
		if names := entries(t, r.work); len(names) != 0 {
			t.Errorf("fail %v: work holds %v", fail, names)
		}
	}
}

func TestACycleWritesTheRecordsAfterTheUnzipAndNothingOutlivesIt(t *testing.T) {
	r, w := edited(t)
	seen := &fileSeer{world: w}
	cycle(t, r.observer(seen))
	if !seen.sawRecord || !seen.sawClearValue {
		t.Errorf("record %v, clear value %v", seen.sawRecord, seen.sawClearValue)
	}
	if names := entries(t, r.work); len(names) != 0 {
		t.Errorf("work holds %v", names)
	}
}

// fileSeer looks into the directory of the cycle while a process runs.
type fileSeer struct {
	*world
	sawRecord, sawClearValue bool
}

func (f *fileSeer) Run(ctx context.Context, dir string, args ...string) (runner.Result, error) {
	if raw, err := os.ReadFile(filepath.Join(dir, "record-0.json")); err == nil && strings.HasPrefix(string(raw), "{") {
		f.sawRecord = true
	}
	if _, err := os.Stat(filepath.Join(dir, "clear-value.txt")); err == nil {
		f.sawClearValue = true
	}
	return f.world.Run(ctx, dir, args...)
}

func TestADirectoryThatAStoppedProcessLeftIsRemovedAtTheStart(t *testing.T) {
	r := newRig(t)
	stale := filepath.Join(r.work, cyclePrefix+"old")
	if err := os.MkdirAll(filepath.Join(stale, "raw"), 0o700); err != nil {
		t.Fatal(err)
	}
	testkit.WriteIn(t, r.work, "other", "x")
	r.observer(&world{})
	if got := entries(t, r.work); len(got) != 1 || got[0] != "other" {
		t.Errorf("work holds %v", got)
	}
}

func TestTheStateHoldsTheFirstSightAndNoValue(t *testing.T) {
	r, w := edited(t)
	o := r.observer(w)
	once(t, o)
	stateHoldsNone(t, r, sentinel, "charts.a.b", "Betty", "John", "csi-aws-ebs")
	keepsFirstSight(t, r, model.FindingID(model.PublishedChanged, testkit.AWSStackProfile))
	if state := readState(t, r); state.StartedAt != t0 {
		t.Errorf("state %+v", state)
	}
	if entries := entries(t, r.state); len(entries) != 1 || entries[0] != testkit.StateFile {
		t.Errorf("the state directory holds %v", entries)
	}
}

func TestASecondRunAfterARestartReadsFirstSeenFromTheState(t *testing.T) {
	r, w := edited(t)
	first, _ := r.observer(w).Once(context.Background())
	again := r.restart(w)
	later := t0.Add(2 * time.Hour)
	again.now = func() time.Time { return later }
	second, _ := again.Once(context.Background())
	if len(first.Findings) != 1 || len(second.Findings) != 1 {
		t.Fatalf("findings %v %v", first.Findings, second.Findings)
	}
	f := second.Findings[0]
	if !f.FirstSeen.Equal(t0) || !f.LastSeen.Equal(later) || f.ID != first.Findings[0].ID {
		t.Errorf("first seen %v, last seen %v", f.FirstSeen, f.LastSeen)
	}
	if !second.Records.StartedAt.Equal(t0) {
		t.Errorf("started at %v", second.Records.StartedAt)
	}
}

func TestAFindingThatGoesLeavesTheState(t *testing.T) {
	r, w := edited(t)
	o := r.observer(w)
	cycle(t, o)
	w.rows[0], w.compares = unchanged(), nil
	cycle(t, o)
	if state := readState(t, r); len(state.Findings) != 0 {
		t.Errorf("findings %+v", state.Findings)
	}
}

func TestTwoRunsWithOneStateDirectoryReachLost(t *testing.T) {
	r, w := oneRecord(t, docs.VerifyRow{Status: docs.VerifyError, Error: &docs.Error{Kind: docs.KindTransport}})
	w.verifyExit = 2
	w.behindErr = &runner.RunError{Kind: docs.KindTransport}
	first, err := r.observer(w).Once(context.Background())
	if err == nil || first.Page != model.PageFirstLoad || first.Content.FailedRuns != 1 {
		t.Fatalf("first: "+pageAndContent+", error %v", first.Page, first.Content, err)
	}
	second, err := r.restart(w).Once(context.Background())
	if err == nil || second.Page != model.PageLost || second.Content.LastError.Kind != docs.KindTransport || second.Versions.LastError.Kind != docs.KindTransport {
		t.Fatalf("second: "+pageAndContent+", error %v", second.Page, second.Content, err)
	}
}

func TestAStateThatCannotBeReadIsALogLineAndAnEmptyState(t *testing.T) {
	r := newRig(t)
	if err := os.MkdirAll(r.state, 0o700); err != nil {
		t.Fatal(err)
	}
	testkit.WriteIn(t, r.state, testkit.StateFile, "{"+sentinel)
	o := r.observer(&world{})
	testkit.HoldsAll(t, "the log", r.log.String(), "the state cannot be read")
	testkit.HoldsNone(t, "the log", r.log.String(), sentinel)
	if s := o.Summary(); s.Content.FailedRuns != 0 || s.Records.StartedAt.IsZero() {
		t.Errorf("summary %+v", s)
	}
}

func TestNothingThatTheObserverWritesHoldsAValueOfTheCluster(t *testing.T) {
	r, w := edited(t)
	s := once(t, r.observer(w))
	raw := testkit.MustJSON(t, s)
	summary := string(raw)
	for name, text := range map[string]string{"summary": summary, "log": r.log.String()} {
		testkit.HoldsNone(t, "the "+name, text, sentinel, "OLD")
	}
	// A line of the log holds a command, an exit code, a duration, and an error kind.
	for _, line := range strings.Split(strings.TrimSpace(r.log.String()), "\n") {
		testkit.HoldsNone(t, "the line", line, "record-0", "cluster:")
	}
	testkit.HoldsAll(t, "the log", r.log.String(), "command=verify exit=0")
}

func TestTheWaitAddsZeroToTenPercent(t *testing.T) {
	for _, c := range []struct {
		jitter float64
		want   time.Duration
	}{{0, 15 * time.Minute}, {0.5, 15*time.Minute + 45*time.Second}, {0.999, 15*time.Minute + 89*time.Second + 910*time.Millisecond}} {
		o := &Observer{jitter: func() float64 { return c.jitter }}
		if got := o.wait(15 * time.Minute); got != c.want {
			t.Errorf("jitter %v: %v, want %v", c.jitter, got, c.want)
		}
	}
	o := New(newRig(t).cfg, &world{}, slog.New(slog.DiscardHandler))
	for range 100 {
		if got := o.wait(time.Hour); got < time.Hour || got >= time.Hour+6*time.Minute {
			t.Fatalf("wait %v", got)
		}
	}
}

// Each timer runs its first cycle at the start, in its own loop: the two cycles overlap, and the pool
// keeps the processes at its size.
func TestTheFirstCycleOfEachTimerRunsAtTheStartAndThePoolBoundsTheProcesses(t *testing.T) {
	r, w := withBehind(t)
	// The first process waits for a second one, so the two cycles overlap with no race against the delay.
	// The delay lets more processes run together than the pool lets, when the pool is wrong.
	w.overlap = &testkit.Overlap{Delay: 5 * time.Millisecond, Hold: 2}
	r.cfg.ContentEvery, r.cfg.VersionsEvery = time.Hour, time.Hour
	o := New(r.cfg, w, slog.New(slog.DiscardHandler))
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() { o.Run(ctx); close(done) }()
	testkit.WaitFor(t, testkit.FirstCycles, func() bool {
		return w.count(runner.CmdVerify) >= 1 && w.count(runner.CmdBehind) >= 1 && w.countMoves() >= 4
	})
	cancel()
	<-done
	if w.count(runner.CmdVerify) != 1 || w.count(runner.CmdBehind) != 1 {
		t.Errorf(verifyBehind, w.count(runner.CmdVerify), w.count(runner.CmdBehind))
	}
	if most := w.overlap.Most(); most < 2 || most > defaultProcesses {
		t.Errorf("%d processes ran at once, want from 2 to %d", most, defaultProcesses)
	}
}

func TestTheTimersRunAgainAfterTheirWait(t *testing.T) {
	r, w := withBehind(t)
	w.overlap = &testkit.Overlap{}
	r.cfg.ContentEvery, r.cfg.VersionsEvery = 20*time.Millisecond, 30*time.Millisecond
	o := New(r.cfg, w, slog.New(slog.DiscardHandler))
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() { o.Run(ctx); close(done) }()
	testkit.WaitFor(t, "three cycles of each timer", func() bool { return w.count(runner.CmdVerify) >= 3 && w.count(runner.CmdBehind) >= 3 })
	cancel()
	<-done
	if most := w.overlap.Most(); most > defaultProcesses {
		t.Errorf("%d processes ran at once, and the pool has %d", most, defaultProcesses)
	}
}

func TestACycleThatIsCanceledIsNoFailedRun(t *testing.T) {
	r, w := oneRecord(t, unchanged())
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	w.verifyErr = &runner.RunError{Kind: docs.KindTimeout}
	o := r.observer(w)
	if err := o.RunContent(ctx); err == nil {
		t.Fatal("no error")
	}
	if o.Summary().Content.FailedRuns != 0 {
		t.Error("a stop counted as a failed run")
	}
}

func TestASnapshotWritesOnlyIntoTheDirectoryOfTheCycle(t *testing.T) {
	r, w := edited(t)
	once(t, r.observer(w))
	w.mu.Lock()
	defer w.mu.Unlock()
	snapshots := 0
	for i, call := range w.calls {
		if call[0] != runner.CmdSnapshot {
			continue
		}
		snapshots++
		out, err := testkit.OutFile(call)
		if err != nil {
			t.Fatal(err)
		}
		if !filepath.IsLocal(out) || filepath.Dir(w.dirs[i]) != r.work {
			t.Errorf("a snapshot writes %q from %s", out, w.dirs[i])
		}
	}
	if snapshots != 2 {
		t.Errorf("%d snapshots", snapshots)
	}
}

// gatedStrata holds each `behind` at the gate, and lets each other command run. A send on gate lets one
// `behind` go, and a close of gate lets each one go. So a test can hold a versions cycle while it runs.
// waiting gets a token when a `behind` comes to the gate. A token that waits there already is enough.
type gatedStrata struct {
	Strata
	gate    chan struct{}
	waiting chan struct{}
}

// gated makes a gatedStrata in front of s, with its gate shut.
func gated(s Strata) *gatedStrata {
	return &gatedStrata{Strata: s, gate: make(chan struct{}), waiting: make(chan struct{}, 1)}
}

func (g *gatedStrata) Run(ctx context.Context, dir string, args ...string) (runner.Result, error) {
	if args[0] == runner.CmdBehind {
		select {
		case g.waiting <- struct{}{}:
		default:
		}
		select {
		case <-g.gate:
		case <-ctx.Done():
			return runner.Result{}, ctx.Err()
		}
	}
	return g.Strata.Run(ctx, dir, args...)
}

// A content cycle ends while a versions cycle waits for its `behind`: each cycle has its own slot. The
// summary shows the progress of the versions cycle while it runs, and drops it at the end. The log has
// a line for the start and the end of each cycle, with the number of processes.
func TestAContentCycleNeverWaitsForAVersionsCycleAndTheProgressShows(t *testing.T) {
	const stillRunning = "the %s cycle ended, and the summary still shows its progress"
	r, w := withBehind(t)
	g := gated(w)
	o := r.observer(g)
	ctx := context.Background()
	versions := make(chan error, 1)
	go func() { versions <- o.RunVersions(ctx) }()
	<-g.waiting // the versions cycle holds its slot, and its `behind` runs
	cycle(t, o)
	running := o.Summary().Versions.Running
	if running == nil || running.Done != 0 || running.Total != 1 {
		t.Fatalf("the progress of the versions cycle during its behind: %+v", running)
	}
	if o.Summary().Content.Running != nil {
		t.Errorf(stillRunning, "content")
	}
	close(g.gate)
	if err := <-versions; err != nil {
		t.Fatalf("the versions cycle: %v", err)
	}
	if o.Summary().Versions.Running != nil {
		t.Errorf(stillRunning, "versions")
	}
	log := r.log.String()
	testkit.HoldsAll(t, "the log", log, []string{"cycle=versions phase=start", "cycle=content phase=start", "cycle=content phase=end verdict=ok processes=", "cycle=versions phase=end verdict=ok processes="}...)
}
