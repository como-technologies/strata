package api

import (
	"bytes"
	"cmp"
	"context"
	"fmt"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/alert"
	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/check"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/kube"
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
	// sentinelText stands for the text of a diff, in each unified field of testdata/.
	sentinelText = "SENTINEL-TEXT"
)

// compareFixture is the fixture of the compare of a record with the cluster.
const compareFixture = "compare-cluster.json"

// csiChart starts the path of a value of the chart of the pack csi-aws-ebs. replicaPath is the path of the setting that
// the fixtures change, after the chart.
const (
	csiChart    = "charts.aws-ebs-csi-driver."
	replicaPath = "controller.replicaCount"
)

// noSuchSide names a side of a diff that does not exist, in the names of the cases.
const noSuchSide = "a side that does not exist"

// The paths of the routes, with the profile aws-stack@1.1.0 of testdata/.
const (
	// pathProfileByName names the profile with no version.
	pathProfileByName = PatternProfiles + "/aws-stack"
)

// kubernetesDiff gives the route of the diff of the pack kubernetes of aws-stack@1.1.0 of testdata/ on the side. An
// empty side gives the route with no side.
func kubernetesDiff(side string) string {
	return testkit.DiffPath(testkit.AWSStackProfile, testkit.KubernetesKey, side)
}

// The routes of the diffs of the packs of aws-stack@1.1.0 of testdata/.
var (
	pathPublished = testkit.DiffPath(testkit.AWSStackProfile, testkit.CSIKey, string(model.SidePublished))
	pathCluster   = kubernetesDiff(string(model.SideCluster))
	pathNewer     = kubernetesDiff(string(model.SideNewer))
)

// pathAsk is the route of the question to Palette for the move of the pack kubernetes of aws-stack@1.1.0.
var pathAsk = testkit.AskPath(testkit.AWSStackProfile, testkit.KubernetesKey)

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
		raw := testkit.ReadFile(t, filepath.Join("testdata", e.Name()))
		testkit.WriteIn(t, dir, e.Name(), times.Replace(string(raw)))
	}
	return dir
}

// fakeStrata answers each command of a cycle from the fixtures, as the binary does. It counts the
// compares of two packs, and it can fail them. It keeps the arguments of each call. A `behind` with
// --pin is the question of a click: it answers from askFixture, and it fails with askErr.
type fakeStrata struct {
	dir string

	mu         sync.Mutex
	moves      int
	failMoves  bool
	failBehind bool
	calls      [][]string
	askFixture string
	askErr     error
}

// The fixtures that answer a question about one move.
const (
	askNoError = "ask-no-error.json"
	askRefuses = "ask-refuses.json"
	ask403     = "ask-403.json"
	// askChecks holds the checks of the update that strata gives beside the answer of Palette. The registry gained
	// 1.37.0 after the versions cycle, and the move to it skips a minor version.
	askChecks = "ask-checks.json"
)

// answer prints a fixture and exits with the code exit.
func (f *fakeStrata) answer(exit int, name string) (runner.Result, error) {
	raw, err := os.ReadFile(filepath.Join(f.dir, name))
	return runner.Result{Exit: exit, Stdout: raw}, err
}

func (f *fakeStrata) Run(_ context.Context, dir string, args ...string) (runner.Result, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.calls = append(f.calls, args)
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
			return f.answer(1, compareFixture)
		}
		f.moves++
		if f.failMoves {
			return runner.Result{}, &runner.RunError{Kind: docs.KindAPI, Status: 500}
		}
		return f.answer(1, "move.json")
	case runner.CmdBehind:
		if slices.Contains(args, runner.FlagPin) {
			if f.askErr != nil {
				return runner.Result{}, f.askErr
			}
			return f.answer(1, cmp.Or(f.askFixture, askNoError))
		}
		if f.failBehind {
			return runner.Result{}, &runner.RunError{Kind: docs.KindAPI, Status: 500}
		}
		return f.answer(1, testkit.BehindDoc)
	}
	return runner.Result{}, testkit.Unknown(args)
}

// questions gives the arguments of each question about a move that the fake got.
func (f *fakeStrata) questions() [][]string {
	f.mu.Lock()
	defer f.mu.Unlock()
	var out [][]string
	for _, c := range f.calls {
		if c[0] == runner.CmdBehind && slices.Contains(c, runner.FlagPin) {
			out = append(out, c)
		}
	}
	return out
}

// setAsk changes the answer to the next questions.
func (f *fakeStrata) setAsk(fixture string, err error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.askFixture, f.askErr = fixture, err
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
	// pods is what the observer reads. A test sets it to nil to turn the read off.
	pods check.Pods
}

func newRig(t *testing.T) *rig {
	t.Helper()
	d := testkit.NewRigDirs(t)
	r := &rig{t: t, records: d.Records, work: d.Work, state: d.State, log: &bytes.Buffer{}, fixtures: fixtures(t), pods: oneImage}
	packs := []map[string]any{testkit.RigPack(testkit.KubernetesKey, "1.35.3", testkit.ImageRef), testkit.RigPack(testkit.CSIKey, "1.63.1", testkit.DigestImage)}
	testkit.WriteRecord(t, r.records, "aws-stack", "1.1.0", packs, sentinelVariable)
	testkit.WriteIn(t, r.records, baseline.MetaFile, testkit.ApprovedMeta(testkit.Revision))
	return r
}

func (r *rig) config() check.Config {
	return check.Config{ClusterUID: testkit.PaymentsUID, Records: r.records, Work: r.work, State: r.state, ContentEvery: testkit.ContentEvery, VersionsEvery: testkit.VersionsEvery}
}

// observer makes an observer on the directories. A second call is a restart of the process.
func (r *rig) observer(s check.Strata) *check.Observer {
	r.t.Helper()
	o := check.New(r.config(), s, slog.New(slog.NewTextHandler(r.log, nil)))
	if r.pods != nil {
		o.WithPods(r.pods)
	}
	return o
}

// oneImage is a read of the pods with one pod that runs a/b:1, the image that the record names, with testkit.Digest1.
var oneImage = check.PodsFunc(func(context.Context) ([]kube.Container, error) {
	return []kube.Container{{Pod: "ns/web-1", Image: testkit.ImageRef, ImageID: testkit.ImageRan}}, nil
})

// cycled makes an observer that ran one content cycle and one versions cycle.
func (r *rig) cycled(s check.Strata) *check.Observer {
	r.t.Helper()
	o := r.observer(s)
	r.once(o)
	return o
}

// once runs one content cycle and one versions cycle of o, and gives the summary. It fails the test when a cycle
// fails.
func (r *rig) once(o *check.Observer) model.Summary {
	r.t.Helper()
	s, err := o.Once(context.Background())
	if err != nil {
		r.t.Fatalf(testkit.CyclesFailed+"\n%s", err, r.log.String())
	}
	return s
}

// get makes a request to the handler with testkit.Get. The Host is a loopback address, as a port-forward gives.
var get = testkit.Get

func handlerOf(src Source) http.Handler {
	return (&Server{Source: src}).Handler()
}

// decode reads the body of an answer into v.
func decode(t *testing.T, rec *httptest.ResponseRecorder, v any) {
	t.Helper()
	testkit.Decode(t, fmt.Sprintf("the body of the answer of status %d", rec.Code), rec.Body.Bytes(), v)
}

// stub is a Source with a fixed summary and no detail.
type stub struct{ sum model.Summary }

func (s stub) Summary() model.Summary   { return s.sum }
func (s stub) Observed() model.Observed { return model.Observed{} }
func (s stub) PackDiff(context.Context, string, string, model.Side) (docs.Pack, bool, error) {
	return docs.Pack{}, false, nil
}

func (s stub) Ask(context.Context, string, string) (model.Answer, bool, error) {
	return model.Answer{}, false, nil
}
func (s stub) Answers() []model.Answer               { return nil }
func (s stub) Alerts() alert.View                    { return alert.Unset() }
func (s stub) StartCycle(string) (model.Start, bool) { return model.Start{}, false }
