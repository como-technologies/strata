package testkit

import (
	"context"
	"sync"
	"testing"
)

// A hold of 2 keeps the first call until a second one runs. A call whose context ended waits no more.
func TestOverlapHoldsTheFirstCallUntilASecondOneRunsAndCountsTheMost(t *testing.T) {
	both := &Overlap{Hold: 2}
	var wg sync.WaitGroup
	for range 2 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			end := both.Start(context.Background())
			end()
		}()
	}
	wg.Wait()
	if both.Most() != 2 {
		t.Errorf("%d calls ran at one time, want 2", both.Most())
	}
	alone := &Overlap{Hold: 2}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	end := alone.Start(ctx)
	end()
	if alone.Most() != 1 {
		t.Errorf("%d calls ran at one time, want 1", alone.Most())
	}
}
