package api

import (
	"context"
	"net/http"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/runner"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// The state keeps the counts of a move, and not its values. After a restart the memory holds no
// values for a move. So the detail route asks the worker for one compare of two packs, one at a
// time, and keeps the answer in memory.
func TestTheNewerSideAfterARestartAsksForOneCompareAndKeepsTheAnswer(t *testing.T) {
	r := newRig(t)
	strata := &fakeStrata{dir: r.fixtures}
	first := r.cycled(strata)
	if strata.moveCompares() != 1 {
		t.Fatalf("the first cycle ran %d compares of two packs", strata.moveCompares())
	}
	// Before the restart the values are in memory: the request runs no compare.
	if rec := get(handlerOf(first), pathNewer); rec.Code != http.StatusOK || !strings.Contains(rec.Body.String(), sentinelValue) || strata.moveCompares() != 1 {
		t.Fatalf("status %d, compares %d", rec.Code, strata.moveCompares())
	}

	// The restart: the state holds the move, the cycle asks for no compare, and the memory is empty.
	h := handlerOf(r.cycled(strata))
	if strata.moveCompares() != 1 {
		t.Fatalf("the cycle after the restart ran a compare: %d", strata.moveCompares())
	}
	for i := 1; i <= 3; i++ {
		rec := get(h, pathNewer)
		if rec.Code != http.StatusOK || !strings.Contains(rec.Body.String(), sentinelValue) {
			t.Fatalf("request %d: status %d, body %s", i, rec.Code, rec.Body.String())
		}
		if strata.moveCompares() != 2 {
			t.Fatalf("request %d: %d compares, want 2: the first request asks, the others read the memory", i, strata.moveCompares())
		}
	}
}

func TestRequestsForTheSameMoveRunOneCompareAndNeverTwoProcessesTogether(t *testing.T) {
	r := newRig(t)
	strata := &fakeStrata{dir: r.fixtures}
	r.cycled(strata)
	// Each process takes 20 ms, so that two processes that are allowed to overlap do overlap.
	slow := &countingStrata{fakeStrata: strata, overlap: testkit.Overlap{Delay: 20 * time.Millisecond}}
	h := handlerOf(r.cycled(slow))
	var wg sync.WaitGroup
	for i := 0; i < 8; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if rec := get(h, pathNewer); rec.Code != http.StatusOK {
				t.Errorf("status %d", rec.Code)
			}
		}()
	}
	wg.Wait()
	if strata.moveCompares() != 2 {
		t.Errorf("%d compares, want 2", strata.moveCompares())
	}
	if most := slow.overlap.Most(); most != 1 {
		t.Errorf("%d processes ran together", most)
	}
}

// countingStrata counts the processes that run at one time. The fake holds its lock for each whole
// call, so the count is in front of it.
type countingStrata struct {
	*fakeStrata
	overlap testkit.Overlap
}

func (c *countingStrata) Run(ctx context.Context, dir string, args ...string) (runner.Result, error) {
	end := c.overlap.Start(ctx)
	defer end()
	return c.fakeStrata.Run(ctx, dir, args...)
}
