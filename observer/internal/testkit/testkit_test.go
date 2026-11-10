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

func TestOutFileGivesTheArgumentAfterTheFlagAndAnErrorWhenThereIsNone(t *testing.T) {
	for _, c := range []struct {
		args []string
		want string
		ok   bool
	}{
		{[]string{"snapshot", "a", "-o", "out.json", "-q"}, "out.json", true},
		{[]string{"snapshot", "a", "-q"}, "", false},
		{[]string{"snapshot", "a", "-o"}, "", false},
	} {
		got, err := OutFile(c.args)
		if got != c.want || (err == nil) != c.ok {
			t.Errorf("%v: got %q, %v", c.args, got, err)
		}
	}
}

func TestClusterRunsTheProfilesThatTheCallGives(t *testing.T) {
	c := Cluster("payments", "aws-stack@1.1.0", "platform-addons@1.0.0")
	got := c.Source.Cluster
	if got.UID != "c1" || got.Name != "payments" || !got.UpdatesPending || len(got.Profiles) != 2 {
		t.Fatalf("cluster %+v", got)
	}
	if p := got.Profiles[1]; p.Name != "platform-addons" || p.Version != "1.0.0" || p.Type != "cluster" {
		t.Errorf("profile %+v", p)
	}
}
