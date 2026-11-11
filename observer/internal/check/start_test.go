package check

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// running starts the loops of o. It gives the function that moves the clock of o, and the function that
// stops the loops and waits for them. The loops stop at the end of the test too.
func running(t *testing.T, o *Observer) (advance func(time.Duration), stop func()) {
	t.Helper()
	var mu sync.Mutex
	now := t0
	o.now = func() time.Time {
		mu.Lock()
		defer mu.Unlock()
		return now
	}
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() { o.Run(ctx); close(done) }()
	var once sync.Once
	stop = func() { once.Do(func() { cancel(); <-done }) }
	t.Cleanup(stop)
	return func(d time.Duration) {
		mu.Lock()
		defer mu.Unlock()
		now = now.Add(d)
	}, stop
}

// The click "Check now" waits a minute after the end of a cycle, then starts one at once. A click while
// a cycle of that kind runs joins it and leaves no token for a second cycle. A name that is no cycle
// gets no answer.
func TestStartCycleWaitsAMinuteAfterAnEndThenStartsACycleAndJoinsARunningOne(t *testing.T) {
	r, w := withBehind(t)
	r.cfg.ContentEvery, r.cfg.VersionsEvery = time.Hour, time.Hour
	g := gated(w)
	o := r.observer(g)
	advance, _ := running(t, o)
	<-g.waiting
	g.gate <- struct{}{} // the `behind` of the first versions cycle
	testkit.WaitFor(t, "the first cycle of each kind", func() bool {
		s := o.Summary()
		return s.Content.CheckedAt != nil && s.Versions.ReadAt != nil
	})

	for _, step := range []struct {
		wait  time.Duration
		state string
		retry int
	}{{0, model.StartWait, 60}, {30 * time.Second, model.StartWait, 30}, {31 * time.Second, model.StartStarted, 0}} {
		advance(step.wait)
		start, ok := o.StartCycle(cycleContent)
		if !ok || start != (model.Start{Cycle: cycleContent, State: step.state, RetryAfter: step.retry}) {
			t.Fatalf("after %v more: %+v %v", step.wait, start, ok)
		}
	}
	testkit.WaitFor(t, "the content cycle of the click", func() bool { return w.count(runner.CmdVerify) == 2 })

	if start, _ := o.StartCycle(cycleVersions); start.State != model.StartStarted {
		t.Fatalf("versions: %+v", start)
	}
	// The gate holds the versions cycle of the click at its `behind`, so the cycle runs until the end of
	// the test. A wait for its progress can miss a cycle that ends between two reads.
	<-g.waiting
	if start, _ := o.StartCycle(cycleVersions); start.State != model.StartRunning {
		t.Fatalf("a click while the cycle runs: %+v", start)
	}
	// The loop takes a token only before a cycle and after it, so a token of the click stays here.
	if n := len(o.kicks[cycleVersions]); n != 0 {
		t.Errorf("the click that joined left %d tokens", n)
	}
	if _, ok := o.StartCycle("pods"); ok {
		t.Error("a name that is no cycle got an answer")
	}
}

// A token that comes while a cycle runs starts no second cycle after it: the loop removes it first.
func TestALoopRemovesATokenThatCameBeforeItsCycle(t *testing.T) {
	o := newRig(t).observer(&world{})
	kick := make(chan struct{}, 1)
	kick <- struct{}{}
	ctx, cancel := context.WithCancel(context.Background())
	var cycles int
	cycle := func(context.Context) error {
		cycles++
		cancel()
		return nil
	}
	o.loop(ctx, time.Hour, kick, cycle)
	if cycles != 1 || len(kick) != 0 {
		t.Errorf("%d cycles, %d tokens", cycles, len(kick))
	}
}

// A change of the records starts a content cycle at once, and no versions cycle. The log says why.
func TestAChangeOfTheRecordsStartsAContentCycle(t *testing.T) {
	r, w := withBehind(t)
	r.cfg.ContentEvery, r.cfg.VersionsEvery = time.Hour, time.Hour
	o := r.observer(w)
	o.recordsPoll = 5 * time.Millisecond
	_, stop := running(t, o)
	testkit.WaitFor(t, "the first cycles", func() bool {
		return w.count(runner.CmdVerify) == 1 && w.count(runner.CmdBehind) == 1
	})
	// the kubelet changes a mounted Secret in one step, so the test writes a new file and renames it
	newMeta := filepath.Join(t.TempDir(), "meta.json")
	if err := os.WriteFile(newMeta, []byte(`{"baseline":"approved","revision":"5f2d8b3c"}`), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.Rename(newMeta, filepath.Join(r.records, "meta.json")); err != nil {
		t.Fatal(err)
	}
	// a change during a cycle starts one more cycle after it, so the count can pass 2
	testkit.WaitFor(t, "the content cycle of the change", func() bool { return w.count(runner.CmdVerify) >= 2 })
	stop()
	if n := w.count(runner.CmdBehind); n != 1 {
		t.Errorf("the change ran %d behinds", n)
	}
	if !strings.Contains(r.log.String(), "records changed") {
		t.Errorf("the log:\n%s", r.log.String())
	}
}

// The look at the records compares with the mark that Run took before the first content cycle, and not
// with a mark of its own start. So a change that comes before its goroutine runs still starts a content
// cycle. A slow runner of CI showed the fault: the look took its first mark after the change of the test.
func TestTheLookAtTheRecordsComparesWithTheMarkOfTheStartOfRun(t *testing.T) {
	r, _ := withBehind(t)
	o := r.observer(&world{})
	o.recordsPoll = time.Millisecond
	mark := baseline.Mark(r.records)
	r.meta(`{"baseline":"approved","revision":"5f2d8b3c"}`)
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() { o.watchRecords(ctx, mark); close(done) }()
	testkit.WaitFor(t, "a token for a content cycle", func() bool { return len(o.kicks[cycleContent]) == 1 })
	cancel()
	<-done
}

// The versions cycle runs `behind` on each record by itself, so the progress counts the records.
func TestTheVersionsCycleRunsBehindOnEachRecord(t *testing.T) {
	r, w := withBehind(t)
	r.record("aws-stack", "1.1.0", "k8s/kubernetes")
	o := r.observer(w)
	if err := o.RunVersions(context.Background()); err != nil {
		t.Fatal(err)
	}
	w.mu.Lock()
	var files [][]string
	for _, c := range w.calls {
		if c[0] == runner.CmdBehind {
			files = append(files, slices.DeleteFunc(slices.Clone(c[1:]), func(a string) bool { return strings.HasPrefix(a, "-") }))
		}
	}
	w.mu.Unlock()
	if len(files) != 2 || len(files[0]) != 1 || len(files[1]) != 1 || files[0][0] == files[1][0] {
		t.Fatalf("behind ran on %v", files)
	}
	// the stand-in gives the same pins for each record, and the merge keeps one row for each
	if s := o.Summary(); len(s.Newer) != 4 || s.Counts.Newer != 4 {
		t.Errorf("rows %d, counts %+v", len(s.Newer), s.Counts)
	}
}

// The merge keeps one pin for each pack name, registry, pinned version, and tag, in that order, with
// each record that pins it once. A read with no error takes the place of a read that failed.
func TestMergeBehindKeepsOnePinForEachKeyWithEachRecordThatPinsIt(t *testing.T) {
	pin := func(name, version, tag, by string, failed bool) docs.Pin {
		p := docs.Pin{Name: name, RegistryUID: "reg1", Version: version, Tag: tag, PinnedBy: []docs.PinnedBy{{Subject: by}}}
		if failed {
			p.Error = &docs.Error{Kind: docs.KindAPI, Status: 429}
		}
		return p
	}
	a := docs.Behind{Schema: docs.BehindSchema, Tool: docs.Tool{Version: "0.32.0"}, Pins: []docs.Pin{
		pin("ubuntu-aws", "22.04", "", "a@1", true),
		pin("kubernetes", "1.36.2", "", "a@1", false),
	}}
	b := docs.Behind{Schema: docs.BehindSchema, Tool: docs.Tool{Version: "0.32.0"}, Pins: []docs.Pin{
		pin("ubuntu-aws", "22.04", "", "b@1", false),
		pin("kubernetes", "1.36.2", "", "a@1", false),
		pin("kubernetes", "1.36.2", "1.36.x", "b@1", false),
	}}
	got := mergeBehind([]docs.Behind{a, b})
	if got.Schema != docs.BehindSchema || got.Tool.Version != "0.32.0" || len(got.Pins) != 3 {
		t.Fatalf("%+v", got)
	}
	var keys, by []string
	for _, p := range got.Pins {
		keys = append(keys, p.Name+"@"+p.Version+"#"+p.Tag)
		var subjects []string
		for _, s := range p.PinnedBy {
			subjects = append(subjects, s.Subject)
		}
		by = append(by, strings.Join(subjects, ","))
	}
	if !slices.Equal(keys, []string{"kubernetes@1.36.2#", "kubernetes@1.36.2#1.36.x", "ubuntu-aws@22.04#"}) {
		t.Errorf("keys %v", keys)
	}
	if !slices.Equal(by, []string{"a@1", "b@1", "a@1,b@1"}) {
		t.Errorf("pinned by %v", by)
	}
	if got.Pins[2].Error != nil {
		t.Errorf("the read that failed stays: %+v", got.Pins[2].Error)
	}
}

// The merge keeps one profile version for each name@version, in the order of the answers, with each
// record that holds it once. A read with no error takes the place of a read that failed (#707).
func TestMergeBehindKeepsOneProfileVersionForEachSubjectWithEachRecordThatHoldsIt(t *testing.T) {
	held := func(name, version, by string, failed bool) docs.ProfilePin {
		p := docs.ProfilePin{Name: name, Version: version, PinnedBy: []docs.PinnedBy{{Subject: by}}}
		if failed {
			p.Error = &docs.Error{Kind: docs.KindAPI, Status: 429}
		} else {
			p.Behind, p.Newer = true, []docs.NewerProfile{{Version: "9.0.0"}}
		}
		return p
	}
	a := docs.Behind{Schema: docs.BehindSchema, Profiles: []docs.ProfilePin{
		held("barney-addons", "1.3.1", "barney-addons@1.3.1", true),
	}}
	b := docs.Behind{Schema: docs.BehindSchema, Profiles: []docs.ProfilePin{
		held("barney-base", "1.5.0", "barney-base@1.5.0", false),
		held("barney-addons", "1.3.1", "barney-addons@1.3.1", false),
	}}
	got := mergeBehind([]docs.Behind{a, b}).Profiles
	if len(got) != 2 || got[0].Name != "barney-addons" || got[1].Name != "barney-base" {
		t.Fatalf("%+v", got)
	}
	if got[0].Error != nil || !got[0].Behind || len(got[0].PinnedBy) != 1 {
		t.Errorf("the read with no error takes the place of the one that failed: %+v", got[0])
	}
}

// Each answer of `behind` on one record names the carriers of the profile versions of that record (#709). The merge keeps the
// carriers of each answer, one for each profile version, also when a later answer reads the pin again. A carrier with no error
// takes the place of one that failed, and the pin keeps it when its own read takes the place of a read that failed.
func TestMergeBehindKeepsTheCarriersOfEachAnswer(t *testing.T) {
	failed := &docs.Error{Kind: docs.KindAPI, Status: 500}
	carrier := func(profile, version, carries string, err *docs.Error) docs.PinCarrier {
		c := docs.PinCarrier{Profile: profile, Version: version, Error: err}
		if carries != "" {
			c.Carrier = &docs.Carrier{Version: carries, PackVersion: "0.2.0"}
		}
		return c
	}
	pin := func(by string, err *docs.Error, carriers ...docs.PinCarrier) docs.Pin {
		return docs.Pin{Name: "fred-compositions", RegistryUID: "reg1", Version: "0.1.0", PinnedBy: []docs.PinnedBy{{Subject: by}},
			Behind: err == nil, Error: err, Carriers: carriers}
	}
	answers := []docs.Behind{
		{Schema: docs.BehindSchema, Pins: []docs.Pin{pin("barney-addons@1.3.1", failed, carrier("barney-addons", "1.3.1", "", failed))}},
		{Schema: docs.BehindSchema, Pins: []docs.Pin{pin("barney-addons@1.3.0", nil, carrier("barney-addons", "1.3.0", "", nil))}},
		{Schema: docs.BehindSchema, Pins: []docs.Pin{pin("barney-addons@1.3.1", nil, carrier("barney-addons", "1.3.1", "1.4.0", nil))}},
	}
	got := mergeBehind(answers).Pins
	if len(got) != 1 || got[0].Error != nil {
		t.Fatalf("%+v", got)
	}
	var named []string
	for _, c := range got[0].Carriers {
		by := "none"
		if c.Carrier != nil {
			by = c.Carrier.Version
		}
		named = append(named, fmt.Sprintf("%s@%s:%s:%v", c.Profile, c.Version, by, c.Error != nil))
	}
	if want := []string{"barney-addons@1.3.1:1.4.0:false", "barney-addons@1.3.0:none:false"}; !slices.Equal(named, want) {
		t.Errorf("carriers %v, want %v", named, want)
	}
}
