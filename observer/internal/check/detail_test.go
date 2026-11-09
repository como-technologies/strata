package check

import (
	"context"
	"errors"
	"slices"
	"sync"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
)

func TestPackDiffGivesThePackOfEachSideWithItsValues(t *testing.T) {
	r, w := edited(t)
	o := r.observer(w)
	ctx := context.Background()
	if _, ok, err := o.PackDiff(ctx, "aws-stack@1.1.0", "csi/csi-aws-ebs", model.SidePublished); ok || err != nil {
		t.Fatalf("before a cycle: %v %v", ok, err)
	}
	if err := o.RunContent(ctx); err != nil {
		t.Fatal(err)
	}
	published, ok, err := o.PackDiff(ctx, "aws-stack@1.1.0", "csi/csi-aws-ebs", model.SidePublished)
	if err != nil || !ok || published.Key != "csi/csi-aws-ebs" || len(published.Values.Diff) == 0 {
		t.Fatalf("published %+v %v %v", published, ok, err)
	}
	cluster, ok, err := o.PackDiff(ctx, "aws-stack@1.1.0", "csi/csi-aws-ebs", model.SideCluster)
	if err != nil || !ok || cluster.Values.Diff[0].Old != sentinel {
		t.Fatalf("cluster %+v %v %v", cluster, ok, err)
	}
	for _, c := range []struct {
		name, subject, key string
		side               model.Side
	}{
		{"a profile that is not known", "nope@1.0.0", "csi/csi-aws-ebs", model.SidePublished},
		{"a version that is not known", "aws-stack@9.9.9", "csi/csi-aws-ebs", model.SidePublished},
		{"a pack that is not in the diff", "aws-stack@1.1.0", "os/ubuntu", model.SidePublished},
		{"a pack that is not known", "aws-stack@1.1.0", "csi/nope", model.SideCluster},
		{"the newer side with no versions cycle", "aws-stack@1.1.0", "csi/csi-aws-ebs", model.SideNewer},
	} {
		if _, ok, err := o.PackDiff(ctx, c.subject, c.key, c.side); ok || err != nil {
			t.Errorf("%s: %v %v", c.name, ok, err)
		}
	}
}

func TestPackDiffOfARecordWithNoSideReadsNothing(t *testing.T) {
	r, w := edited(t)
	w.compares = nil // the compare gives a document with no pack
	o := r.observer(w)
	if err := o.RunContent(context.Background()); err != nil {
		t.Fatal(err)
	}
	if _, ok, err := o.PackDiff(context.Background(), "aws-stack@1.1.0", "csi/csi-aws-ebs", model.SideCluster); ok || err != nil {
		t.Errorf("%v %v", ok, err)
	}
}

const moveSubject = "aws-stack@1.0.0"

func TestPackDiffOfTheNewerSideComesFromMemoryAfterAFreshCompare(t *testing.T) {
	r, w := withBehind(t)
	o := r.observer(w)
	ctx := context.Background()
	if err := o.RunVersions(ctx); err != nil {
		t.Fatal(err)
	}
	before := w.countMoves()
	pack, ok, err := o.PackDiff(ctx, moveSubject, "k8s/kubernetes", model.SideNewer)
	if err != nil || !ok || pack.Key != "k8s/kubernetes" || len(pack.Values.Diff) != 11 {
		t.Fatalf("%+v %v %v", pack.Key, ok, err)
	}
	if w.countMoves() != before {
		t.Errorf("the request ran a compare, and the cycle had the values")
	}
	for _, c := range []struct{ subject, key string }{{"other@1.0.0", "k8s/kubernetes"}, {moveSubject, "k8s/nope"}, {moveSubject, "addon/cert-manager"}} {
		if _, ok, err := o.PackDiff(ctx, c.subject, c.key, model.SideNewer); ok || err != nil {
			t.Errorf("%v: %v %v", c, ok, err)
		}
	}
}

func TestAfterARestartTheNewerSideAsksForOneCompareAtATimeAndKeepsTheAnswer(t *testing.T) {
	r, w := withBehind(t)
	ctx := context.Background()
	if err := r.observer(w).RunVersions(ctx); err != nil {
		t.Fatal(err)
	}
	o := r.restart(w)
	if err := o.RunVersions(ctx); err != nil {
		t.Fatal(err)
	}
	cycle := w.countMoves()
	for i := range 3 {
		pack, ok, err := o.PackDiff(ctx, moveSubject, "k8s/kubernetes", model.SideNewer)
		if err != nil || !ok || len(pack.Values.Diff) != 11 {
			t.Fatalf("request %d: %v %v", i, ok, err)
		}
		if got := w.countMoves() - cycle; got != 1 {
			t.Fatalf("request %d: %d compares since the cycle, want 1", i, got)
		}
	}
	// The compare was a read: a compare of two packs, and no other command.
	w.mu.Lock()
	defer w.mu.Unlock()
	last := w.calls[len(w.calls)-1]
	if last[0] != runner.CmdCompare || last[1] != "pack:"+registryUID+"/kubernetes@1.35.3" || last[2] != "pack:"+registryUID+"/kubernetes@1.36.2" {
		t.Errorf("the last command is %v", last)
	}
	if slices.Contains(last, "POST") {
		t.Errorf("a POST: %v", last)
	}
}

func TestACompareThatFailsForTheNewerSideIsNotKept(t *testing.T) {
	r, w := withBehind(t)
	ctx := context.Background()
	if err := r.observer(w).RunVersions(ctx); err != nil {
		t.Fatal(err)
	}
	o := r.restart(w)
	if err := o.RunVersions(ctx); err != nil {
		t.Fatal(err)
	}
	key := move("kubernetes", "1.35.3", "1.36.2")
	w.moveErrs[key] = &runner.RunError{Kind: docs.KindAPI, Status: 500}
	_, ok, err := o.PackDiff(ctx, moveSubject, "k8s/kubernetes", model.SideNewer)
	var re *runner.RunError
	if ok || !errors.As(err, &re) || re.Kind != docs.KindAPI {
		t.Fatalf("%v %v", ok, err)
	}
	delete(w.moveErrs, key)
	if pack, ok, err := o.PackDiff(ctx, moveSubject, "k8s/kubernetes", model.SideNewer); err != nil || !ok || len(pack.Values.Diff) != 11 {
		t.Errorf("the next request asks again: %v %v", ok, err)
	}
	if entries := entries(t, r.work); len(entries) != 0 {
		t.Errorf("work holds %v", entries)
	}
}

func TestAMoveThatTheNextCycleDropsLeavesTheMemory(t *testing.T) {
	r, w := withBehind(t)
	ctx := context.Background()
	o := r.observer(w)
	if err := o.RunVersions(ctx); err != nil {
		t.Fatal(err)
	}
	// The next cycle sees no pin that is behind, so the memory of the moves goes with the state.
	w.behind.Pins = nil
	if err := o.RunVersions(ctx); err != nil {
		t.Fatal(err)
	}
	if _, ok, _ := o.PackDiff(ctx, moveSubject, "k8s/kubernetes", model.SideNewer); ok {
		t.Error("a pin that is gone still has a diff")
	}
	o.mu.Lock()
	defer o.mu.Unlock()
	if len(o.details) != 0 || len(o.state.Moves) != 0 {
		t.Errorf("details %d, moves %d", len(o.details), len(o.state.Moves))
	}
}

// slowStrata makes each command take time, and counts the processes that run together.
type slowStrata struct {
	Strata
	mu             sync.Mutex
	active, mostAt int
}

func (s *slowStrata) Run(ctx context.Context, dir string, args ...string) (runner.Result, error) {
	s.mu.Lock()
	s.active++
	s.mostAt = max(s.mostAt, s.active)
	s.mu.Unlock()
	time.Sleep(10 * time.Millisecond)
	defer func() {
		s.mu.Lock()
		s.active--
		s.mu.Unlock()
	}()
	return s.Strata.Run(ctx, dir, args...)
}

// The two cycles and a request of the detail tier run at the same time, and the pool bounds their
// processes: with a pool of 2, two processes run together and never three.
func TestTheCyclesAndARequestShareThePoolOfProcesses(t *testing.T) {
	r, w := withBehind(t)
	ctx := context.Background()
	if err := r.observer(w).RunVersions(ctx); err != nil {
		t.Fatal(err)
	}
	r.cfg.Processes = 2
	slow := &slowStrata{Strata: w}
	o := r.restart(slow)
	var wg sync.WaitGroup
	wg.Add(3)
	go func() { defer wg.Done(); _ = o.RunVersions(ctx) }()
	go func() { defer wg.Done(); _ = o.RunContent(ctx) }()
	go func() {
		defer wg.Done()
		_, _, _ = o.PackDiff(ctx, moveSubject, "k8s/kubernetes", model.SideNewer)
	}()
	wg.Wait()
	if slow.mostAt != 2 {
		t.Errorf("%d processes ran together, want the 2 of the pool", slow.mostAt)
	}
}

func TestARequestThatWaitsEndsWithItsContext(t *testing.T) {
	r, w := withBehind(t)
	ctx := context.Background()
	if err := r.observer(w).RunVersions(ctx); err != nil {
		t.Fatal(err)
	}
	o := r.restart(w)
	if err := o.RunVersions(ctx); err != nil {
		t.Fatal(err)
	}
	for range cap(o.procs) { // the cycles hold each token of the pool
		if err := o.acquire(ctx, o.procs); err != nil {
			t.Fatal(err)
		}
	}
	defer func() {
		for range cap(o.procs) {
			o.release(o.procs)
		}
	}()
	short, cancel := context.WithTimeout(ctx, 20*time.Millisecond)
	defer cancel()
	if _, ok, err := o.PackDiff(short, moveSubject, "k8s/kubernetes", model.SideNewer); ok || !errors.Is(err, context.DeadlineExceeded) {
		t.Errorf("%v %v", ok, err)
	}
}
