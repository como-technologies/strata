package testkit

import (
	"context"
	"sync"
	"time"
)

// Overlap counts the calls of a stand-in for strata that run at one time. The stand-in calls Start when
// a call starts, and the function that Start gives when the call ends.
type Overlap struct {
	// Delay makes each call take this time, so that calls that can run together do.
	Delay time.Duration
	// Hold makes each call wait until this many calls ran at one time, for 10 seconds at most. A test
	// that must see calls run together sets it, so that its count does not depend on a delay.
	Hold int

	mu           sync.Mutex
	active, most int
}

// Start counts a call that starts, and waits as Hold and then Delay say. The wait for Hold also ends
// when ctx ends. Start gives the function that counts the end of the call.
func (o *Overlap) Start(ctx context.Context) (end func()) {
	o.mu.Lock()
	o.active++
	o.most = max(o.most, o.active)
	o.mu.Unlock()
	deadline := time.Now().Add(waitLimit)
	for o.Most() < o.Hold && ctx.Err() == nil && time.Now().Before(deadline) {
		time.Sleep(time.Millisecond)
	}
	time.Sleep(o.Delay)
	return func() {
		o.mu.Lock()
		defer o.mu.Unlock()
		o.active--
	}
}

// Most gives the most calls that ran at one time.
func (o *Overlap) Most() int {
	o.mu.Lock()
	defer o.mu.Unlock()
	return o.most
}
