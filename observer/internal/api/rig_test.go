package api

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/check"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// sentinelValue stands for a changed value. It is in each Leaf.Old, each Leaf.New, and each change of
// an image in testdata/. sentinelVariable stands for the clear value of a variable of the cluster,
// which no tier ever carries.
const (
	sentinelValue    = "SENTINEL-VALUE"
	sentinelVariable = "SENTINEL-VARIABLE"
)

// The paths of the routes, with the profile aws-stack@1.1.0 of testdata/.
const (
	pathPublished = "/v1/packs/aws-stack@1.1.0/csi/csi-aws-ebs/diff?side=published"
	pathCluster   = "/v1/packs/aws-stack@1.1.0/k8s/kubernetes/diff?side=cluster"
	pathNewer     = "/v1/packs/aws-stack@1.1.0/k8s/kubernetes/diff?side=newer"
)

// fixtures copies testdata/ to a directory of the test. The times of the documents are relative to
// the clock, so the events of the last 7 days are in the window.
func fixtures(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	now := time.Now().UTC()
	times := strings.NewReplacer(
		"@WHO@", now.Add(-3*time.Hour).Format(time.RFC3339),
		"@LISTED@", now.Add(-48*time.Hour).Format(time.RFC3339),
		"@SYNCED@", now.Add(-96*time.Hour).Format(time.RFC3339),
	)
	list, err := os.ReadDir("testdata")
	if err != nil {
		t.Fatal(err)
	}
	for _, e := range list {
		raw, err := os.ReadFile(filepath.Join("testdata", e.Name()))
		if err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(dir, e.Name()), []byte(times.Replace(string(raw))), 0o600); err != nil {
			t.Fatal(err)
		}
	}
	return dir
}

// fakeStrata answers each command of a cycle from the fixtures, as the binary does. It counts the
// compares of two packs, and it can fail them.
type fakeStrata struct {
	dir string

	mu         sync.Mutex
	moves      int
	failMoves  bool
	failBehind bool
}

// answer prints a fixture and exits with the code exit.
func (f *fakeStrata) answer(exit int, name string) (runner.Result, error) {
	raw, err := os.ReadFile(filepath.Join(f.dir, name))
	return runner.Result{Exit: exit, Stdout: raw}, err
}

func (f *fakeStrata) Run(_ context.Context, dir string, args ...string) (runner.Result, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	switch args[0] {
	case runner.CmdVerify:
		return f.answer(1, "verify.json")
	case runner.CmdSnapshot:
		out, err := testkit.OutFile(args)
		if err != nil {
			return runner.Result{}, err
		}
		res, err := f.answer(0, "cluster.json")
		if err != nil {
			return runner.Result{}, err
		}
		return runner.Result{}, os.WriteFile(filepath.Join(dir, out), res.Stdout, 0o600)
	case runner.CmdCompare:
		if !strings.HasPrefix(args[1], "pack:") {
			return f.answer(1, "compare-cluster.json")
		}
		f.moves++
		if f.failMoves {
			return runner.Result{}, &runner.RunError{Kind: docs.KindAPI, Status: 500}
		}
		return f.answer(1, "move.json")
	case runner.CmdBehind:
		if f.failBehind {
			return runner.Result{}, &runner.RunError{Kind: docs.KindAPI, Status: 500}
		}
		return f.answer(1, "behind.json")
	}
	return runner.Result{}, fmt.Errorf("the fake does not know %q", args[0])
}

func (f *fakeStrata) moveCompares() int {
	f.mu.Lock()
	defer f.mu.Unlock()
	return f.moves
}

// rig holds the directories of one test. A test makes more than one observer on them, to show a restart.
type rig struct {
	t                    *testing.T
	records, work, state string
	log                  *bytes.Buffer
	fixtures             string
}

func newRig(t *testing.T) *rig {
	t.Helper()
	r := &rig{t: t, records: t.TempDir(), work: t.TempDir(), state: filepath.Join(t.TempDir(), "state"), log: &bytes.Buffer{}, fixtures: fixtures(t)}
	record, err := json.Marshal(map[string]any{
		"schema": docs.SnapshotSchema, "tool": map[string]any{"version": "0.28.0"}, "root": "r1",
		"source": map[string]any{"uid": "u1", "name": "aws-stack", "version": "1.1.0", "type": "cluster"},
		"packs": []map[string]any{
			{"key": "k8s/kubernetes", "layer": "k8s", "name": "kubernetes", "version": "1.35.3", "registryUid": "reg1", "images": []string{"a/b:1"}},
			{"key": "csi/csi-aws-ebs", "layer": "csi", "name": "csi-aws-ebs", "version": "1.63.1", "registryUid": "reg1", "images": []string{"a/c@sha256:abc"}},
		},
		"variables": []map[string]any{{"name": "team", "value": sentinelVariable}},
	})
	if err != nil {
		t.Fatal(err)
	}
	testkit.WriteGzip(t, r.records, "aws-stack@1.1.0.json.gz", string(record))
	if err := os.WriteFile(filepath.Join(r.records, "meta.json"), []byte(`{"baseline":"approved","revision":"4e1c7a2"}`), 0o600); err != nil {
		t.Fatal(err)
	}
	return r
}

func (r *rig) config() check.Config {
	return check.Config{ClusterUID: "c1", Records: r.records, Work: r.work, State: r.state, ContentEvery: 15 * time.Minute, VersionsEvery: 6 * time.Hour}
}

// observer makes an observer on the directories. A second call is a restart of the process.
func (r *rig) observer(s check.Strata) *check.Observer {
	r.t.Helper()
	return check.New(r.config(), s, slog.New(slog.NewTextHandler(r.log, nil)))
}

// cycled makes an observer that ran one content cycle and one versions cycle.
func (r *rig) cycled(s check.Strata) *check.Observer {
	r.t.Helper()
	o := r.observer(s)
	if _, err := o.Once(context.Background()); err != nil {
		r.t.Fatalf("the cycles failed: %v\n%s", err, r.log.String())
	}
	return o
}

// get makes a request to the handler. The Host is a loopback address, as a port-forward gives.
func get(h http.Handler, path string) *httptest.ResponseRecorder {
	return do(h, http.MethodGet, path, "127.0.0.1:8080")
}

func do(h http.Handler, method, path, host string) *httptest.ResponseRecorder {
	req := httptest.NewRequest(method, path, nil)
	req.Host = host
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, req)
	return rec
}

func handlerOf(src Source) http.Handler {
	return (&Server{Source: src}).Handler()
}

// decode reads the body of an answer into v.
func decode(t *testing.T, rec *httptest.ResponseRecorder, v any) {
	t.Helper()
	if err := json.Unmarshal(rec.Body.Bytes(), v); err != nil {
		t.Fatalf("status %d, body %q: %v", rec.Code, rec.Body.String(), err)
	}
}

// stub is a Source with a fixed summary and no detail.
type stub struct{ sum model.Summary }

func (s stub) Summary() model.Summary { return s.sum }
func (s stub) PackDiff(context.Context, string, string, model.Side) (docs.Pack, bool, error) {
	return docs.Pack{}, false, nil
}
