package check

import (
	"context"
	"errors"
	"log/slog"
	"maps"
	"math/rand/v2"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"time"

	"github.com/como-technologies/strata/observer/internal/alert"
	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
)

// cyclePrefix starts the name of the directory of a cycle under the work directory.
const cyclePrefix = "cycle-"

// jitterShare is the largest share of a period that the observer adds to a wait: 10 percent.
const jitterShare = 0.1

// startPause is the shortest time between the end of a cycle and a cycle that a person starts. A person
// who clicks again and again then starts at most one cycle of each kind in a minute and its run.
const startPause = time.Minute

// recordsPoll is the period of the look at the records. A change of the records starts a content cycle.
const recordsPoll = 15 * time.Second

// Config holds the flags that `serve` and `once` share.
type Config struct {
	ClusterUID    string
	Records       string
	Work          string
	State         string
	ContentEvery  time.Duration
	VersionsEvery time.Duration
	// Processes is the most strata processes that run at one time. 0 is defaultProcesses.
	Processes int
}

// defaultProcesses keeps the memory of the pod in its limit: each strata process holds the documents
// that it reads, and a compare of a large pack holds two of them.
const defaultProcesses = 4

// Observer runs the cycles and keeps the last good answer of each. A content cycle and a versions
// cycle run at the same time, each in its own slot, so a long versions cycle never delays a check of
// the content. The pool of processes bounds the strata processes of both, and of the detail tier. A
// call of Summary can happen while a cycle runs.
type Observer struct {
	cfg    Config
	strata Strata
	// pods is nil when the observer does not read pods: then it makes no call to Kubernetes.
	pods Pods
	// alerts is nil when the observer sends no alert.
	alerts *alert.Engine
	now    func() time.Time
	// jitter gives a number from 0 up to, and not including, 1.
	jitter func() float64
	log    *slog.Logger

	// contentSlot and versionsSlot each let one cycle of their kind run. A click "Ask Palette" holds the
	// versions slot, so that no versions cycle moves the pin while Palette answers.
	contentSlot, versionsSlot chan struct{}
	// detailSlot lets one compare of the detail tier run. A request that waited for it finds the answer of
	// the request before it in memory, so many requests for one move run one compare.
	detailSlot chan struct{}
	// procs has one token for each strata process that can run at one time.
	procs chan struct{}
	// running holds the progress of the cycle of each kind that runs now. The lock guards it.
	running map[string]*model.Running
	// kicks has one channel for each kind of cycle. A token in it starts that cycle at once, and the
	// loop of the cycle takes it. A second token while one waits adds nothing.
	kicks map[string]chan struct{}
	// ended holds the end of the last cycle of each kind, good or failed. The lock guards it.
	ended map[string]time.Time
	// recordsPoll is the period of the look at the records.
	recordsPoll time.Duration
	// taken holds the first-seen records that the observer took itself, by profile version: see
	// taken.go. It stays in memory. The lock guards it.
	taken map[string]baseline.Entry

	mu       sync.Mutex
	state    State
	content  *model.Content
	versions *model.Versions
	// contentAt and versionsAt are the ends of the last good cycles.
	contentAt, versionsAt *time.Time
	files                 int
	filesKnown            bool
	// details holds the pack of each move that the observer compared, by the key of the cache. It holds
	// changed values, so it stays in memory: the state never holds it.
	details map[string]docs.Pack
	// asks holds the answer of Palette to each question about a move, by profile and move. It stays in
	// memory: a restart forgets it, and a pin that moves drops it.
	asks map[askKey]model.Answer
}

// New makes an observer. It reads the state, and starts with an empty state when the file cannot be
// read. It removes the directories that a stopped process left in the work directory.
func New(cfg Config, strata Strata, log *slog.Logger) *Observer {
	return newObserver(cfg, strata, log, time.Now)
}

func newObserver(cfg Config, strata Strata, log *slog.Logger, now func() time.Time) *Observer {
	procs := cfg.Processes
	if procs <= 0 {
		procs = defaultProcesses
	}
	o := &Observer{cfg: cfg, strata: strata, now: now, jitter: rand.Float64, log: log,
		contentSlot: make(chan struct{}, 1), versionsSlot: make(chan struct{}, 1), detailSlot: make(chan struct{}, 1), procs: make(chan struct{}, procs),
		running: map[string]*model.Running{}, details: map[string]docs.Pack{}, asks: map[askKey]model.Answer{},
		kicks: map[string]chan struct{}{cycleContent: make(chan struct{}, 1), cycleVersions: make(chan struct{}, 1)},
		ended: map[string]time.Time{}, recordsPoll: recordsPoll, taken: map[string]baseline.Entry{}}
	state, err := loadState(cfg.State)
	if err != nil {
		log.Error("the state cannot be read", "kind", docs.KindIO)
	}
	o.state = state
	if o.state.StartedAt.IsZero() {
		o.state.StartedAt = o.now().UTC().Truncate(time.Second)
	}
	o.sweep()
	return o
}

// sweep removes the directory of a cycle that a stopped process left in the work directory.
func (o *Observer) sweep() {
	entries, err := os.ReadDir(o.cfg.Work)
	if err != nil {
		return
	}
	for _, e := range entries {
		if strings.HasPrefix(e.Name(), cyclePrefix) {
			if err := os.RemoveAll(filepath.Join(o.cfg.Work, e.Name())); err != nil {
				o.log.Error("a directory of a cycle stays", "kind", docs.KindIO)
			}
		}
	}
}

// run starts one strata command when the pool has a token, and writes one log line: the command, the
// exit code, the duration, and the kind of the error. The line holds no argument and no output. The
// process counts as one step of the cycle of ctx.
func (o *Observer) run(ctx context.Context, dir string, exits []int, args ...string) (runner.Result, error) {
	if err := o.acquire(ctx, o.procs); err != nil {
		return runner.Result{}, err
	}
	defer o.release(o.procs)
	defer o.step(ctx)
	start := time.Now()
	res, err := o.strata.Run(ctx, dir, args...)
	if err == nil && !slices.Contains(exits, res.Exit) {
		err = failure(docs.KindCrash)
	}
	line := []any{"command", args[0], "exit", res.Exit, "duration", time.Since(start).Round(time.Millisecond)}
	var re *runner.RunError
	if errors.As(err, &re) {
		line = append(line, "kind", re.Kind, "status", re.Status)
	}
	o.log.Info("strata", line...)
	return res, err
}

// RunContent runs one content cycle, and keeps its answer or its failure. Then it hands the cycle to the
// alerts, when the observer sends them.
func (o *Observer) RunContent(ctx context.Context) error {
	var content model.Content
	var good bool
	var held map[string]bool
	err := o.runCycle(ctx, cycleContent, o.contentSlot, &o.state.Content,
		func(ctx context.Context) (files int, err error) {
			content, files, err = o.contentCycle(ctx)
			return files, err
		},
		func() {
			// A read that failed tells nothing about the drift, so the last good answer of that part stays.
			content.Carry(o.content)
			o.content, o.contentAt = &content, &content.At
			o.keepFirstDigests(content)
			held = o.heldIDs(content)
			o.keepFirstSeen(content, held)
			good = true
		})
	o.alertStep(ctx, good, held)
	return err
}

// heldIDs gives the ids of the findings that this content cannot clear: the ones of a part that the
// cycle could not read, and the ones that a file with no last record can hold. The caller reads it
// before keepFirstSeen changes the first sights.
func (o *Observer) heldIDs(content model.Content) map[string]bool {
	held := map[string]bool{}
	for _, id := range model.HeldIDs(content) {
		held[id] = true
	}
	for _, id := range model.BlindIDs(content, slices.Collect(maps.Keys(o.state.Findings))) {
		held[id] = true
	}
	return held
}

// RunVersions runs one versions cycle, and keeps its answer or its failure.
func (o *Observer) RunVersions(ctx context.Context) error {
	var versions model.Versions
	return o.runCycle(ctx, cycleVersions, o.versionsSlot, &o.state.Versions,
		func(ctx context.Context) (files int, err error) {
			versions, files, err = o.versionsCycle(ctx)
			return files, err
		},
		func() {
			// A cycle with no record reads no version, so it has no end to show.
			o.versions, o.versionsAt = &versions, nil
			if !versions.At.IsZero() {
				o.versionsAt = &versions.At
			}
		})
}

// The names of the two cycles, in the log, in the progress, and in the context of a cycle.
const (
	cycleContent  = "content"
	cycleVersions = "versions"
)

type cycleKey struct{}

// runCycle runs one cycle in its slot. read runs the cycle and gives the number of record files, which
// is -1 when the directory was not read. keep stores the answer of a cycle that worked, with the lock
// held. A failure goes to clock, and a good cycle clears it. The log has one line when the cycle starts
// and one when it ends, with the number of strata processes that it ran.
func (o *Observer) runCycle(ctx context.Context, name string, slot chan struct{}, clock *ClockState, read func(context.Context) (int, error), keep func()) error {
	if err := o.acquire(ctx, slot); err != nil {
		return err
	}
	defer o.release(slot)
	start := time.Now()
	o.mu.Lock()
	o.running[name] = &model.Running{StartedAt: o.now().UTC()}
	o.mu.Unlock()
	o.log.Info("cycle", "cycle", name, "phase", "start")
	files, err := read(context.WithValue(ctx, cycleKey{}, name))
	o.mu.Lock()
	defer o.mu.Unlock()
	processes := o.running[name].Done
	delete(o.running, name)
	o.ended[name] = o.now()
	if ctx.Err() != nil {
		return ctx.Err()
	}
	o.readFiles(files)
	if err != nil {
		o.fail(name, clock, start, processes, err)
		return err
	}
	keep()
	*clock = ClockState{}
	o.log.Info("cycle", "cycle", name, "phase", "end", "verdict", "ok", "processes", processes, "duration", time.Since(start).Round(time.Millisecond))
	o.save()
	return nil
}

// expect adds n processes to the progress of the cycle of ctx. A call with no cycle does nothing.
func (o *Observer) expect(ctx context.Context, n int) {
	o.progress(ctx, func(r *model.Running) { r.Total += n })
}

// step counts one process of the cycle of ctx as done.
func (o *Observer) step(ctx context.Context) {
	o.progress(ctx, func(r *model.Running) {
		r.Done++
		r.Total = max(r.Total, r.Done)
	})
}

func (o *Observer) progress(ctx context.Context, change func(*model.Running)) {
	name, _ := ctx.Value(cycleKey{}).(string)
	o.mu.Lock()
	defer o.mu.Unlock()
	if r := o.running[name]; r != nil {
		change(r)
	}
}

// runningOf gives a copy of the progress of a cycle, or nil. The caller holds the lock.
func (o *Observer) runningOf(name string) *model.Running {
	r := o.running[name]
	if r == nil {
		return nil
	}
	c := *r
	return &c
}

// readFiles keeps the number of record files of the last read. -1 says that the read failed.
func (o *Observer) readFiles(files int) {
	if files >= 0 {
		o.files, o.filesKnown = files, true
	}
}

// fail keeps the last answer, adds 1 to the failed runs, and keeps the kind and the status.
func (o *Observer) fail(name string, clock *ClockState, start time.Time, processes int, err error) {
	clock.FailedRuns++
	ref := errorRefOf(err)
	clock.LastError = &ref
	o.log.Info("cycle", "cycle", name, "phase", "end", "verdict", "failed", "kind", ref.Kind, "status", ref.Status, "failedRuns", clock.FailedRuns,
		"processes", processes, "duration", time.Since(start).Round(time.Millisecond))
	o.save()
}

// errorRefOf gives the kind and the status of a failed run of strata, and the kind unknown for another error.
func errorRefOf(err error) model.ErrorRef {
	var re *runner.RunError
	if errors.As(err, &re) {
		return model.ErrorRef{Kind: re.Kind, Status: re.Status}
	}
	return model.ErrorRef{Kind: docs.KindUnknown}
}

// keepFirstSeen keeps the first sight of each finding of a good cycle, and drops the others. The ids in
// held keep their first sights too, because only a read that works clears a finding. This holds also
// when the process has no last answer in memory.
func (o *Observer) keepFirstSeen(content model.Content, held map[string]bool) {
	kept := map[string]FindingState{}
	for _, f := range model.FindingsOf(content, o.state.Images.First) {
		seen, ok := o.state.Findings[f.ID]
		if !ok {
			seen = FindingState{FirstSeen: content.At.UTC().Truncate(time.Second)}
		}
		kept[f.ID] = seen
	}
	for id := range held {
		if seen, ok := o.state.Findings[id]; ok {
			if _, found := kept[id]; !found {
				kept[id] = seen
			}
		}
	}
	o.state.Findings = kept
}

// cachedMove, storeMove, and pruneMoves are the cache of the moves. The key holds the version of the
// binary, so a new binary asks again. The state keeps the counts of a move. The memory keeps its
// pack, with the changed values, in details.
func (o *Observer) cachedMove(key string) (model.Move, bool) {
	o.mu.Lock()
	defer o.mu.Unlock()
	m, ok := o.state.Moves[key]
	return m, ok
}

func (o *Observer) storeMove(key string, m model.Move) {
	o.mu.Lock()
	defer o.mu.Unlock()
	o.state.Moves[key] = m
}

func (o *Observer) cachedDetail(key string) (docs.Pack, bool) {
	o.mu.Lock()
	defer o.mu.Unlock()
	p, ok := o.details[key]
	return p, ok
}

func (o *Observer) storeDetail(key string, p docs.Pack) {
	o.mu.Lock()
	defer o.mu.Unlock()
	o.details[key] = p
}

func (o *Observer) pruneMoves(used map[string]bool) {
	o.mu.Lock()
	defer o.mu.Unlock()
	for k := range o.state.Moves {
		if !used[k] {
			delete(o.state.Moves, k)
		}
	}
	for k := range o.details {
		if !used[k] {
			delete(o.details, k)
		}
	}
	for k := range o.asks {
		if !used[k.move] {
			delete(o.asks, k)
		}
	}
}

// save writes the state. A write that fails is a log line, and the cycle goes on.
func (o *Observer) save() {
	if err := saveState(o.cfg.State, o.state); err != nil {
		o.log.Error("the state cannot be written", "kind", docs.KindIO)
	}
}

// input collects what Build reads. The caller holds the lock.
func (o *Observer) input() model.Input {
	firstSeen := make(map[string]time.Time, len(o.state.Findings))
	for id, f := range o.state.Findings {
		firstSeen[id] = f.FirstSeen
	}
	return model.Input{
		Now: o.now(), ClusterUID: o.cfg.ClusterUID, StartedAt: o.state.StartedAt,
		Files: o.files, FilesKnown: o.filesKnown,
		Content:       o.content,
		ContentClock:  model.Clock{Every: o.cfg.ContentEvery, Last: o.contentAt, FailedRuns: o.state.Content.FailedRuns, LastError: o.state.Content.LastError, Running: o.runningOf(cycleContent)},
		Versions:      o.versions,
		VersionsClock: model.Clock{Every: o.cfg.VersionsEvery, Last: o.versionsAt, FailedRuns: o.state.Versions.FailedRuns, LastError: o.state.Versions.LastError, Running: o.runningOf(cycleVersions)},
		FirstSeen:     firstSeen,
		ReadPods:      o.pods != nil,
		FirstDigests:  o.state.Images.First,
	}
}

// Summary gives the summary at this time. The page state is computed here, at each call.
func (o *Observer) Summary() model.Summary {
	o.mu.Lock()
	defer o.mu.Unlock()
	return model.Build(o.input())
}

// Observed gives the document of the pods at this time. It lists the images of the last good read. It
// says so when the read is off or has not ended well.
func (o *Observer) Observed() model.Observed {
	o.mu.Lock()
	defer o.mu.Unlock()
	return model.ObservedOf(o.input())
}

// Once runs one content cycle and one versions cycle, and gives the summary. The error joins the
// failures of the two cycles: each holds a kind and a status, and no message.
func (o *Observer) Once(ctx context.Context) (model.Summary, error) {
	err := errors.Join(o.RunContent(ctx), o.RunVersions(ctx))
	return o.Summary(), err
}

// wait gives the time to the next cycle: the period, and a random 0 to 10 percent more. The random
// part keeps many observers from asking Palette in the same minute.
func (o *Observer) wait(every time.Duration) time.Duration {
	return every + time.Duration(float64(every)*jitterShare*o.jitter())
}

// Run runs each timer in its own loop until ctx ends: the first cycle at the start, and each next one
// after its wait. A content cycle never waits for a versions cycle. A third loop looks at the records.
func (o *Observer) Run(ctx context.Context) {
	// The first mark of the records comes before the first content cycle reads them. So the look misses no
	// change after that read, also when its goroutine starts late.
	mark := baseline.Mark(o.cfg.Records)
	var wg sync.WaitGroup
	wg.Add(3)
	go func() { defer wg.Done(); o.loop(ctx, o.cfg.ContentEvery, o.kicks[cycleContent], o.RunContent) }()
	go func() { defer wg.Done(); o.loop(ctx, o.cfg.VersionsEvery, o.kicks[cycleVersions], o.RunVersions) }()
	go func() { defer wg.Done(); o.watchRecords(ctx, mark) }()
	wg.Wait()
}

// loop runs one kind of cycle, then waits its period and a random share of it, or a token of kick. The
// failure of a cycle is in the state and in the log. A token that came while the cycle ran starts no
// second cycle: the loop removes it before each cycle. Each cycle starts a full wait again.
func (o *Observer) loop(ctx context.Context, every time.Duration, kick chan struct{}, cycle func(context.Context) error) {
	for ctx.Err() == nil {
		select {
		case <-kick:
		default:
		}
		_ = cycle(ctx)
		timer := time.NewTimer(o.wait(every))
		select {
		case <-ctx.Done():
			timer.Stop()
			return
		case <-timer.C:
		case <-kick:
			timer.Stop()
		}
	}
}

// StartCycle asks the loop of the cycle name to run it at once: the click "Check now" of the page. A
// cycle of that kind that runs now gets no second one: the click joins it. A cycle that ended less than
// startPause ago gets no new one, and the answer says when to try again. The second result is false
// for a name that is no cycle.
func (o *Observer) StartCycle(name string) (model.Start, bool) {
	kick, ok := o.kicks[name]
	if !ok {
		return model.Start{}, false
	}
	o.mu.Lock()
	defer o.mu.Unlock()
	start := model.Start{Cycle: name, State: model.StartStarted}
	if o.running[name] != nil {
		start.State = model.StartRunning
		return start, true
	}
	if end, ok := o.ended[name]; ok {
		if left := startPause - o.now().Sub(end); left > 0 {
			start.State, start.RetryAfter = model.StartWait, int((left+time.Second-1)/time.Second)
			return start, true
		}
	}
	o.kick(kick)
	return start, true
}

// kick puts a token in the channel of a loop. A token that waits there already is enough.
func (o *Observer) kick(kick chan struct{}) {
	select {
	case kick <- struct{}{}:
	default:
	}
}

// watchRecords starts a content cycle when the records change, so that the page shows the records that
// a pipeline published within a minute or two, and not at the end of the period. The kubelet updates
// the mounted Secrets in place, so the look compares the names, the sizes, and the times of the files.
// It sends no request. last is the mark of the records before the first content cycle.
func (o *Observer) watchRecords(ctx context.Context, last string) {
	ticker := time.NewTicker(o.recordsPoll)
	defer ticker.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
		}
		mark := baseline.Mark(o.cfg.Records)
		if mark == last {
			continue
		}
		last = mark
		o.log.Info("records changed", "cycle", cycleContent)
		o.kick(o.kicks[cycleContent])
	}
}

// acquire waits for a token of slot. It gives up when ctx ends.
func (o *Observer) acquire(ctx context.Context, slot chan struct{}) error {
	select {
	case slot <- struct{}{}:
		return nil
	case <-ctx.Done():
		return ctx.Err()
	}
}

func (o *Observer) release(slot chan struct{}) {
	<-slot
}
