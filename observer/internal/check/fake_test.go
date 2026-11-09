package check

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/runner"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// The tests give the observer a fake strata. The fake answers from a world: the documents that the
// real binary prints, and the errors that it gives. It records each call.

var t0 = time.Date(2026, 11, 3, 14, 0, 0, 0, time.UTC)

const sentinel = "SENTINEL-CLEAR-VALUE"

// world holds the answers of the fake strata. A record is known by its index: record-<i>.json.
type world struct {
	// rows gives one `verify` row for each record, by index. The fake sets the path of the row.
	rows []docs.VerifyRow
	// verifySchema is the schema id of the `verify` document. It is the right id when empty.
	verifySchema string
	verifyTool   string
	verifyExit   int
	verifyErr    error

	// The fake gives the right schema id to a document that has none. A test that sets another id keeps it.
	cluster    docs.ClusterRead
	clusterErr error
	sideErr    error
	// profileErr fails the snapshot of a profile, name@version, which the seed and the observer take.
	profileErr error
	// compares gives the diff of the compare of a record with its side, by index of the record.
	compares map[int]docs.Diff

	behind     docs.Behind
	behindExit int
	behindErr  error
	// moves gives the diff of a compare of two packs, by "ref ref".
	moves    map[string]docs.Diff
	moveErrs map[string]error

	// delay makes each call take time, so that a test can see two calls at once.
	delay time.Duration

	mu                sync.Mutex
	calls             [][]string
	active, maxActive int
	modes             []os.FileMode
	// dirs holds the directory of each call, so that a test can see the working directory.
	dirs []string
}

func (w *world) Run(_ context.Context, dir string, args ...string) (runner.Result, error) {
	w.mu.Lock()
	w.calls = append(w.calls, args)
	w.dirs = append(w.dirs, dir)
	if info, err := os.Stat(dir); err == nil {
		w.modes = append(w.modes, info.Mode())
	}
	w.active++
	w.maxActive = max(w.maxActive, w.active)
	w.mu.Unlock()
	defer func() {
		w.mu.Lock()
		w.active--
		w.mu.Unlock()
	}()
	time.Sleep(w.delay)
	switch args[0] {
	case runner.CmdVerify:
		return w.runVerify(args)
	case runner.CmdSnapshot:
		return w.runSnapshot(dir, args)
	case runner.CmdCompare:
		return w.runCompare(args)
	case runner.CmdBehind:
		return w.runBehind()
	}
	return runner.Result{}, fmt.Errorf("the fake does not know %q", args[0])
}

// doc makes the result of a command that prints v and exits with the code exit.
func doc(v any, exit int) (runner.Result, error) {
	b, err := json.Marshal(v)
	return runner.Result{Stdout: b, Exit: exit}, err
}

func (w *world) runVerify(args []string) (runner.Result, error) {
	if w.verifyErr != nil {
		return runner.Result{}, w.verifyErr
	}
	s := docs.VerifySummary{Schema: docs.VerifySummarySchema, Tool: docs.Tool{Version: w.verifyTool}}
	if w.verifySchema != "" {
		s.Schema = w.verifySchema
	}
	for i, a := range args[1:] {
		if strings.HasPrefix(a, "-") || i >= len(w.rows) {
			break
		}
		row := w.rows[i]
		row.Path = a
		s.Records = append(s.Records, row)
	}
	return doc(s, w.verifyExit)
}

func fileIndex(name string) (int, error) {
	digits := strings.TrimFunc(filepath.Base(name), func(r rune) bool { return r < '0' || r > '9' })
	return strconv.Atoi(digits)
}

func (w *world) runSnapshot(dir string, args []string) (runner.Result, error) {
	out, err := testkit.OutFile(args)
	if err != nil {
		return runner.Result{}, err
	}
	if strings.Contains(args[1], "/") {
		if w.sideErr != nil {
			return runner.Result{}, w.sideErr
		}
		return runner.Result{}, os.WriteFile(filepath.Join(dir, out), []byte("{}"), 0o600)
	}
	if name, version, ok := strings.Cut(args[1], "@"); ok && !strings.HasPrefix(args[1], "cluster:") {
		if w.profileErr != nil {
			return runner.Result{}, w.profileErr
		}
		raw, err := json.Marshal(map[string]any{"schema": docs.SnapshotSchema, "tool": map[string]any{"version": "0.28.0"}, "root": "r-" + name,
			"source": map[string]any{"uid": "u-" + name, "name": name, "version": version, "type": "cluster"}, "packs": []any{}})
		if err != nil {
			return runner.Result{}, err
		}
		return runner.Result{}, os.WriteFile(filepath.Join(dir, out), raw, 0o600)
	}
	if w.clusterErr != nil {
		return runner.Result{}, w.clusterErr
	}
	// The cluster side holds a clear value, as the real file does.
	if err := os.WriteFile(filepath.Join(dir, "clear-value.txt"), []byte(sentinel), 0o600); err != nil {
		return runner.Result{}, err
	}
	c := w.cluster
	if c.Schema == "" {
		c.Schema = docs.SnapshotSchema
	}
	raw, err := json.Marshal(c)
	if err != nil {
		return runner.Result{}, err
	}
	return runner.Result{}, os.WriteFile(filepath.Join(dir, out), raw, 0o600)
}

// schemaOr gives the schema id that a test set, or the right one when it set none.
func schemaOr(set, right string) string {
	if set != "" {
		return set
	}
	return right
}

func (w *world) runCompare(args []string) (runner.Result, error) {
	if strings.HasPrefix(args[1], "pack:") {
		key := args[1] + " " + args[2]
		if err := w.moveErrs[key]; err != nil {
			return runner.Result{}, err
		}
		d, ok := w.moves[key]
		if !ok {
			return runner.Result{}, fmt.Errorf("the fake has no move %q", key)
		}
		return diffDoc(d)
	}
	i, err := fileIndex(args[1])
	if err != nil {
		return runner.Result{}, err
	}
	d, ok := w.compares[i]
	if !ok {
		d = docs.Diff{Same: true, SameContent: true}
	}
	return diffDoc(d)
}

// diffDoc prints a diff, and exits 1 when the two sides differ, as `compare` does.
func diffDoc(d docs.Diff) (runner.Result, error) {
	d.Schema = schemaOr(d.Schema, docs.DiffSchema)
	exit := 0
	if !d.Same {
		exit = 1
	}
	return doc(d, exit)
}

func (w *world) runBehind() (runner.Result, error) {
	if w.behindErr != nil {
		return runner.Result{}, w.behindErr
	}
	b := w.behind
	b.Schema = schemaOr(b.Schema, docs.BehindSchema)
	return doc(b, w.behindExit)
}

// count gives the number of calls of a command.
func (w *world) count(command string) int {
	w.mu.Lock()
	defer w.mu.Unlock()
	n := 0
	for _, c := range w.calls {
		if c[0] == command {
			n++
		}
	}
	return n
}

// countMoves gives the number of compares of two packs.
func (w *world) countMoves() int {
	w.mu.Lock()
	defer w.mu.Unlock()
	n := 0
	for _, c := range w.calls {
		if c[0] == runner.CmdCompare && strings.HasPrefix(c[1], "pack:") {
			n++
		}
	}
	return n
}

// rig holds the directories of one test.
type rig struct {
	t                    *testing.T
	records, work, state string
	cfg                  Config
	log                  *bytes.Buffer
}

func newRig(t *testing.T) *rig {
	t.Helper()
	r := &rig{t: t, records: t.TempDir(), work: t.TempDir(), state: filepath.Join(t.TempDir(), "state"), log: &bytes.Buffer{}}
	r.cfg = Config{ClusterUID: "c1", Records: r.records, Work: r.work, State: r.state, ContentEvery: 15 * time.Minute, VersionsEvery: 6 * time.Hour}
	return r
}

// record writes a record for a profile, with one pack for each key (layer/name).
func (r *rig) record(name, version string, keys ...string) {
	r.t.Helper()
	packs := []map[string]any{}
	for _, k := range keys {
		layer, pack, _ := strings.Cut(k, "/")
		packs = append(packs, map[string]any{"key": k, "layer": layer, "name": pack, "version": "1.0.0", "registryUid": "reg1", "images": []string{"a/b:1"}})
	}
	body, err := json.Marshal(map[string]any{
		"schema": docs.SnapshotSchema, "tool": map[string]any{"version": "0.28.0"}, "root": "r1",
		"source":    map[string]any{"uid": "u-" + name, "name": name, "version": version, "type": "cluster"},
		"packs":     packs,
		"variables": []map[string]any{{"name": "team", "value": sentinel}},
	})
	if err != nil {
		r.t.Fatal(err)
	}
	testkit.WriteGzip(r.t, r.records, name+"@"+version+".json.gz", string(body))
}

func (r *rig) meta(body string) {
	r.t.Helper()
	if err := os.WriteFile(filepath.Join(r.records, "meta.json"), []byte(body), 0o600); err != nil {
		r.t.Fatal(err)
	}
}

// observer makes an observer with the clock at now. A test changes o.now to move the clock.
func (r *rig) observer(w Strata) *Observer {
	r.t.Helper()
	return newObserver(r.cfg, w, slog.New(slog.NewTextHandler(r.log, nil)), func() time.Time { return t0 })
}

// restart makes a new observer on the same directories, as a restart of the process does.
func (r *rig) restart(w Strata) *Observer {
	return r.observer(w)
}

func unchanged() docs.VerifyRow {
	return docs.VerifyRow{Status: docs.VerifyUnchanged, RootRecorded: "r1", RootLive: "r1"}
}

func entries(t *testing.T, dir string) []string {
	t.Helper()
	list, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	var names []string
	for _, e := range list {
		names = append(names, e.Name())
	}
	return names
}
