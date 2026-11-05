package check

import (
	"context"
	"errors"
	"log/slog"
	"math/rand/v2"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
)

// cyclePrefix starts the name of the directory of a cycle under the work directory.
const cyclePrefix = "cycle-"

// jitterShare is the largest share of a period that the observer adds to a wait: 10 percent.
const jitterShare = 0.1

// Config holds the flags that `serve` and `once` share.
type Config struct {
	ClusterUID    string
	Records       string
	Work          string
	State         string
	ContentEvery  time.Duration
	VersionsEvery time.Duration
}

// Observer runs the cycles and keeps the last good answer of each. One worker runs the cycles, so
// one strata process runs at a time. A call of Summary can happen while a cycle runs.
type Observer struct {
	cfg    Config
	strata Strata
	// pods is nil when the observer does not read pods: then it makes no call to Kubernetes.
	pods Pods
	now  func() time.Time
	// jitter gives a number from 0 up to, and not including, 1.
	jitter func() float64
	log    *slog.Logger

	// worker has one slot. A strata process runs only while its holder has the slot, so that a cycle and
	// a request of the detail tier never run two processes at once.
	worker chan struct{}

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
}

// New makes an observer. It reads the state, and starts with an empty state when the file cannot be
// read. It removes the directories that a stopped process left in the work directory.
func New(cfg Config, strata Strata, log *slog.Logger) *Observer {
	return newObserver(cfg, strata, log, time.Now)
}

func newObserver(cfg Config, strata Strata, log *slog.Logger, now func() time.Time) *Observer {
	o := &Observer{cfg: cfg, strata: strata, now: now, jitter: rand.Float64, log: log,
		worker: make(chan struct{}, 1), details: map[string]docs.Pack{}}
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

// run starts one strata command and writes one log line: the command, the exit code, the duration,
// and the kind of the error. The line holds no argument and no output.
func (o *Observer) run(ctx context.Context, dir string, exits []int, args ...string) (runner.Result, error) {
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

// RunContent runs one content cycle, and keeps its answer or its failure.
func (o *Observer) RunContent(ctx context.Context) error {
	var content model.Content
	return o.runCycle(ctx, "content", &o.state.Content,
		func(ctx context.Context) (files int, err error) {
			content, files, err = o.contentCycle(ctx)
			return files, err
		},
		func() {
			// A read that failed tells nothing about the drift, so the last good answer of that part stays.
			content.Carry(o.content)
			o.content, o.contentAt = &content, &content.At
			o.keepFirstDigests(content)
			o.keepFirstSeen(content)
		})
}

// RunVersions runs one versions cycle, and keeps its answer or its failure.
func (o *Observer) RunVersions(ctx context.Context) error {
	var versions model.Versions
	return o.runCycle(ctx, "versions", &o.state.Versions,
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

// runCycle runs one cycle in the slot of the worker. read runs the cycle and gives the number of
// record files, which is -1 when the directory was not read. keep stores the answer of a cycle that
// worked, with the lock held. A failure goes to clock, and a good cycle clears it.
func (o *Observer) runCycle(ctx context.Context, name string, clock *ClockState, read func(context.Context) (int, error), keep func()) error {
	if err := o.acquire(ctx); err != nil {
		return err
	}
	defer o.release()
	start := time.Now()
	files, err := read(ctx)
	if ctx.Err() != nil {
		return ctx.Err()
	}
	o.mu.Lock()
	defer o.mu.Unlock()
	o.readFiles(files)
	if err != nil {
		o.fail(name, clock, start, err)
		return err
	}
	keep()
	*clock = ClockState{}
	o.log.Info("cycle", "cycle", name, "verdict", "ok", "duration", time.Since(start).Round(time.Millisecond))
	o.save()
	return nil
}

// readFiles keeps the number of record files of the last read. -1 says that the read failed.
func (o *Observer) readFiles(files int) {
	if files >= 0 {
		o.files, o.filesKnown = files, true
	}
}

// fail keeps the last answer, adds 1 to the failed runs, and keeps the kind and the status.
func (o *Observer) fail(name string, clock *ClockState, start time.Time, err error) {
	clock.FailedRuns++
	ref := model.ErrorRef{Kind: docs.KindUnknown}
	var re *runner.RunError
	if errors.As(err, &re) {
		ref = model.ErrorRef{Kind: re.Kind, Status: re.Status}
	}
	clock.LastError = &ref
	o.log.Info("cycle", "cycle", name, "verdict", "failed", "kind", ref.Kind, "status", ref.Status, "failedRuns", clock.FailedRuns,
		"duration", time.Since(start).Round(time.Millisecond))
	o.save()
}

// keepFirstSeen keeps the first sight of each finding of a good cycle, and drops the others. A part
// of a record that the cycle could not read keeps its first sights too, because only a read that
// works clears a finding. This holds also when the process has no last answer in memory.
func (o *Observer) keepFirstSeen(content model.Content) {
	kept := map[string]FindingState{}
	for _, f := range model.FindingsOf(content, o.state.Images.First) {
		seen, ok := o.state.Findings[f.ID]
		if !ok {
			seen = FindingState{FirstSeen: content.At.UTC().Truncate(time.Second)}
		}
		kept[f.ID] = seen
	}
	for _, id := range model.HeldIDs(content) {
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
		ContentClock:  model.Clock{Every: o.cfg.ContentEvery, Last: o.contentAt, FailedRuns: o.state.Content.FailedRuns, LastError: o.state.Content.LastError},
		Versions:      o.versions,
		VersionsClock: model.Clock{Every: o.cfg.VersionsEvery, Last: o.versionsAt, FailedRuns: o.state.Versions.FailedRuns, LastError: o.state.Versions.LastError},
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

// Run runs the first cycle of each timer at the start, and then each cycle after its wait, until
// ctx ends. It is the one worker: a cycle ends before the next one starts.
func (o *Observer) Run(ctx context.Context) {
	contentDue, versionsDue := o.now(), o.now()
	for ctx.Err() == nil {
		if wait := earlier(contentDue, versionsDue).Sub(o.now()); wait > 0 {
			timer := time.NewTimer(wait)
			select {
			case <-ctx.Done():
				timer.Stop()
				return
			case <-timer.C:
			}
		}
		if !o.now().Before(contentDue) && ctx.Err() == nil {
			_ = o.RunContent(ctx) // the failure is in the state and in the log
			contentDue = o.now().Add(o.wait(o.cfg.ContentEvery))
		}
		if !o.now().Before(versionsDue) && ctx.Err() == nil {
			_ = o.RunVersions(ctx) // the failure is in the state and in the log
			versionsDue = o.now().Add(o.wait(o.cfg.VersionsEvery))
		}
	}
}

func earlier(a, b time.Time) time.Time {
	if b.Before(a) {
		return b
	}
	return a
}

// acquire waits for the slot of the worker. It gives up when ctx ends.
func (o *Observer) acquire(ctx context.Context) error {
	select {
	case o.worker <- struct{}{}:
		return nil
	case <-ctx.Done():
		return ctx.Err()
	}
}

func (o *Observer) release() {
	<-o.worker
}
