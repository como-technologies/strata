package check

import (
	"bytes"
	"compress/gzip"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
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

	// A document that has no schema id gets the right one. A test that sets another id keeps it.
	cluster    docs.ClusterRead
	clusterErr error
	sideErr    error
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

func doc(v any) runner.Result {
	b, err := json.Marshal(v)
	if err != nil {
		panic(err)
	}
	return runner.Result{Stdout: b}
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
	res := doc(s)
	res.Exit = w.verifyExit
	return res, nil
}

func fileIndex(name string) int {
	digits := strings.TrimFunc(filepath.Base(name), func(r rune) bool { return r < '0' || r > '9' })
	n, err := strconv.Atoi(digits)
	if err != nil {
		panic(err)
	}
	return n
}

func (w *world) runSnapshot(dir string, args []string) (runner.Result, error) {
	out := args[slicesIndex(args, "-o")+1]
	if strings.Contains(args[1], "/") {
		if w.sideErr != nil {
			return runner.Result{}, w.sideErr
		}
		return runner.Result{}, os.WriteFile(filepath.Join(dir, out), []byte("{}"), 0o600)
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

func slicesIndex(args []string, want string) int {
	for i, a := range args {
		if a == want {
			return i
		}
	}
	panic("no " + want)
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
		d.Schema = schemaOr(d.Schema, docs.DiffSchema)
		res := doc(d)
		if !d.Same {
			res.Exit = 1
		}
		return res, nil
	}
	d, ok := w.compares[fileIndex(args[1])]
	if !ok {
		d = docs.Diff{Same: true, SameContent: true}
	}
	d.Schema = schemaOr(d.Schema, docs.DiffSchema)
	res := doc(d)
	if !d.Same {
		res.Exit = 1
	}
	return res, nil
}

func (w *world) runBehind() (runner.Result, error) {
	if w.behindErr != nil {
		return runner.Result{}, w.behindErr
	}
	b := w.behind
	b.Schema = schemaOr(b.Schema, docs.BehindSchema)
	res := doc(b)
	res.Exit = w.behindExit
	return res, nil
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
	var buf bytes.Buffer
	zw := gzip.NewWriter(&buf)
	if _, err := zw.Write(body); err != nil {
		r.t.Fatal(err)
	}
	if err := zw.Close(); err != nil {
		r.t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(r.records, name+"@"+version+".json.gz"), buf.Bytes(), 0o600); err != nil {
		r.t.Fatal(err)
	}
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

// cluster makes a cluster read with the given profiles as name@version.
func cluster(t *testing.T, name string, profiles ...string) docs.ClusterRead {
	t.Helper()
	list := []map[string]string{}
	for _, p := range profiles {
		n, v, _ := strings.Cut(p, "@")
		list = append(list, map[string]string{"name": n, "version": v, "type": "cluster"})
	}
	raw, err := json.Marshal(map[string]any{"source": map[string]any{"cluster": map[string]any{"uid": "c1", "name": name, "profiles": list}}})
	if err != nil {
		t.Fatal(err)
	}
	var c docs.ClusterRead
	if err := json.Unmarshal(raw, &c); err != nil {
		t.Fatal(err)
	}
	return c
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

func errorsAs(err error, target any) bool {
	return errors.As(err, target)
}

// nil2 gives a logger that writes nowhere.
func nil2() *slog.Logger {
	return slog.New(slog.NewTextHandler(io.Discard, nil))
}

func marshalForTest(t *testing.T, v any) string {
	t.Helper()
	b, err := json.Marshal(v)
	if err != nil {
		t.Fatal(err)
	}
	return string(b)
}
