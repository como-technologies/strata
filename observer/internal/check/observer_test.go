package check

import (
	"context"
	"encoding/json"
	"log/slog"
	"os"
	"path/filepath"
	"slices"
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
	w.compares = map[int]docs.Diff{0: {RootB: w.rows[0].RootLive, Packs: []docs.Pack{changedPack("csi/csi-aws-ebs")}}}
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
	if err := r.observer(seen).RunContent(context.Background()); err != nil {
		t.Fatal(err)
	}
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
	keep := filepath.Join(r.work, "other")
	if err := os.WriteFile(keep, []byte("x"), 0o600); err != nil {
		t.Fatal(err)
	}
	r.observer(&world{})
	if got := entries(t, r.work); len(got) != 1 || got[0] != "other" {
		t.Errorf("work holds %v", got)
	}
}

func TestTheStateHoldsTheFirstSightAndNoValue(t *testing.T) {
	r, w := edited(t)
	o := r.observer(w)
	if _, err := o.Once(context.Background()); err != nil {
		t.Fatal(err)
	}
	raw, err := os.ReadFile(filepath.Join(r.state, "state.json"))
	if err != nil {
		t.Fatal(err)
	}
	for _, banned := range []string{sentinel, "charts.a.b", "Betty", "John", "csi-aws-ebs"} {
		if strings.Contains(string(raw), banned) {
			t.Errorf("the state holds %q: %s", banned, raw)
		}
	}
	id := model.FindingID(model.PublishedChanged, "aws-stack@1.1.0")
	state, err := loadState(r.state)
	if err != nil || state.Findings[id].FirstSeen != t0 || state.StartedAt != t0 {
		t.Errorf("state %+v, error %v", state, err)
	}
	if entries := entries(t, r.state); len(entries) != 1 || entries[0] != "state.json" {
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
	if err := o.RunContent(context.Background()); err != nil {
		t.Fatal(err)
	}
	w.rows[0], w.compares = unchanged(), nil
	if err := o.RunContent(context.Background()); err != nil {
		t.Fatal(err)
	}
	if state, _ := loadState(r.state); len(state.Findings) != 0 {
		t.Errorf("findings %+v", state.Findings)
	}
}

func TestTwoRunsWithOneStateDirectoryReachLost(t *testing.T) {
	r, w := oneRecord(t, docs.VerifyRow{Status: docs.VerifyError, Error: &docs.Error{Kind: docs.KindTransport}})
	w.verifyExit = 2
	w.behindErr = &runner.RunError{Kind: docs.KindTransport}
	first, err := r.observer(w).Once(context.Background())
	if err == nil || first.Page != model.PageFirstLoad || first.Content.FailedRuns != 1 {
		t.Fatalf("first: page %q, content %+v, error %v", first.Page, first.Content, err)
	}
	second, err := r.restart(w).Once(context.Background())
	if err == nil || second.Page != model.PageLost || second.Content.LastError.Kind != docs.KindTransport || second.Versions.LastError.Kind != docs.KindTransport {
		t.Fatalf("second: page %q, content %+v, error %v", second.Page, second.Content, err)
	}
}

func TestAStateThatCannotBeReadIsALogLineAndAnEmptyState(t *testing.T) {
	r := newRig(t)
	if err := os.MkdirAll(r.state, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(r.state, "state.json"), []byte("{"+sentinel), 0o600); err != nil {
		t.Fatal(err)
	}
	o := r.observer(&world{})
	if !strings.Contains(r.log.String(), "the state cannot be read") || strings.Contains(r.log.String(), sentinel) {
		t.Errorf("log %q", r.log.String())
	}
	if s := o.Summary(); s.Content.FailedRuns != 0 || s.Records.StartedAt.IsZero() {
		t.Errorf("summary %+v", s)
	}
}

func TestNothingThatTheObserverWritesHoldsAValueOfTheCluster(t *testing.T) {
	r, w := edited(t)
	s, err := r.observer(w).Once(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	raw, err := json.Marshal(s)
	if err != nil {
		t.Fatal(err)
	}
	summary := string(raw)
	for name, text := range map[string]string{"summary": summary, "log": r.log.String()} {
		if strings.Contains(text, sentinel) || strings.Contains(text, "OLD") {
			t.Errorf("the %s holds a value: %s", name, text)
		}
	}
	// A line of the log holds a command, an exit code, a duration, and an error kind.
	for _, line := range strings.Split(strings.TrimSpace(r.log.String()), "\n") {
		if strings.Contains(line, "record-0") || strings.Contains(line, "cluster:") {
			t.Errorf("a log line holds an argument: %s", line)
		}
	}
	if !strings.Contains(r.log.String(), "command=verify exit=0") {
		t.Errorf("log %s", r.log.String())
	}
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

func TestTheFirstCycleOfEachTimerRunsAtTheStartAndOneProcessRunsAtATime(t *testing.T) {
	r, w := withBehind(t)
	w.delay = 5 * time.Millisecond
	r.cfg.ContentEvery, r.cfg.VersionsEvery = time.Hour, time.Hour
	o := New(r.cfg, w, slog.New(slog.DiscardHandler))
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() { o.Run(ctx); close(done) }()
	testkit.WaitFor(t, "the first cycle of each timer", func() bool {
		return w.count(runner.CmdVerify) >= 1 && w.count(runner.CmdBehind) >= 1 && w.countMoves() >= 4
	})
	cancel()
	<-done
	if w.count(runner.CmdVerify) != 1 || w.count(runner.CmdBehind) != 1 {
		t.Errorf("verify %d, behind %d", w.count(runner.CmdVerify), w.count(runner.CmdBehind))
	}
	if w.maxActive != 1 {
		t.Errorf("%d processes ran at once", w.maxActive)
	}
}

func TestTheTimersRunAgainAfterTheirWait(t *testing.T) {
	r, w := withBehind(t)
	r.cfg.ContentEvery, r.cfg.VersionsEvery = 20*time.Millisecond, 30*time.Millisecond
	o := New(r.cfg, w, slog.New(slog.DiscardHandler))
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() { o.Run(ctx); close(done) }()
	testkit.WaitFor(t, "three cycles of each timer", func() bool { return w.count(runner.CmdVerify) >= 3 && w.count(runner.CmdBehind) >= 3 })
	cancel()
	<-done
	if w.maxActive != 1 {
		t.Errorf("%d processes ran at once", w.maxActive)
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
	if _, err := r.observer(w).Once(context.Background()); err != nil {
		t.Fatal(err)
	}
	w.mu.Lock()
	defer w.mu.Unlock()
	snapshots := 0
	for i, call := range w.calls {
		if call[0] != runner.CmdSnapshot {
			continue
		}
		snapshots++
		out := call[slices.Index(call, "-o")+1]
		if !filepath.IsLocal(out) || filepath.Dir(w.dirs[i]) != r.work {
			t.Errorf("a snapshot writes %q from %s", out, w.dirs[i])
		}
	}
	if snapshots != 2 {
		t.Errorf("%d snapshots", snapshots)
	}
}
