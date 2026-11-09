package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"slices"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// run runs a command with a context that never ends.
func run(args []string, stdout, stderr io.Writer) int {
	return runContext(context.Background(), args, stdout, stderr)
}

func TestVersionPrintsTheName(t *testing.T) {
	var out, errs bytes.Buffer
	if code := run([]string{"version"}, &out, &errs); code != 0 {
		t.Fatalf("exit %d, stderr %q", code, errs.String())
	}
	if !strings.HasPrefix(out.String(), "strata-observer ") {
		t.Fatalf("stdout %q", out.String())
	}
}

func TestAnUnknownCommandExitsTwo(t *testing.T) {
	var out, errs bytes.Buffer
	if code := run([]string{"nope"}, &out, &errs); code != 2 {
		t.Fatalf("exit %d", code)
	}
	if !strings.Contains(errs.String(), "usage:") {
		t.Fatalf("stderr %q", errs.String())
	}
}

// fakeStrata writes a script that stands for the strata binary. compare is the line that
// `compare` prints. verifyExit is the exit code of `verify`.
func fakeStrata(t *testing.T, compare string, verifyExit int, behindFails bool) string {
	t.Helper()
	compare = strings.Replace(compare, "{", `{"schema":"`+docs.DiffSchema+`",`, 1)
	return testkit.Strata(t, `case $cmd in
verify)
  if [ `+strconv.Itoa(verifyExit)+` = 2 ]; then echo '{"error":{"kind":"transport","message":"SENTINEL"}}' >&2; exit 2; fi
  echo '`+testkit.VerifyUnchanged+`' ;;
snapshot) echo '`+testkit.ClusterPayments+`' > "$out" ;;
compare) echo '`+compare+`'; case '`+compare+`' in *'"same":true'*) exit 0 ;; *) exit 1 ;; esac ;;
behind)
  if [ `+strconv.FormatBool(behindFails)+` = true ]; then echo '{"error":{"kind":"api","status":404}}' >&2; exit 2; fi
  echo '{"schema":"`+docs.BehindSchema+`","tool":{"version":"0.29.0"},"pins":[]}' ;;
esac`)
}

// onceArgs makes the directories of a run of `once` with one record.
func onceArgs(t *testing.T, bin string) []string {
	t.Helper()
	records := t.TempDir()
	testkit.WriteGzip(t, records, "aws-stack.json.gz", `{"schema":"`+docs.SnapshotSchema+`","tool":{"version":"0.28.0"},"source":{"uid":"u1","name":"aws-stack","version":"1.1.0","type":"cluster"},"packs":[]}`)
	return []string{"once", "--strata", bin, "--cluster-uid", "c1", "--records", records, "--work", t.TempDir(), "--state", filepath.Join(t.TempDir(), "state")}
}

func TestOnceExitsZeroWithNoFindingAndPrintsTheSummary(t *testing.T) {
	var out, errs bytes.Buffer
	code := run(onceArgs(t, fakeStrata(t, `{"same":true,"sameContent":true}`, 0, false)), &out, &errs)
	if code != 0 {
		t.Fatalf("exit %d, stderr %q", code, errs.String())
	}
	var s model.Summary
	if err := json.Unmarshal(out.Bytes(), &s); err != nil {
		t.Fatalf("stdout %q: %v", out.String(), err)
	}
	if s.Page != model.PageOK || s.Cluster.Name != "payments" || len(s.Findings) != 0 || s.Records.Count != 1 {
		t.Errorf("summary %+v", s)
	}
}

func TestOnceExitsOneWhenAFindingExists(t *testing.T) {
	var out, errs bytes.Buffer
	code := run(onceArgs(t, fakeStrata(t, `{"same":false,"sameContent":false,"rootB":"x","packs":[{"key":"k8s/kubernetes","status":"changed"}]}`, 0, false)), &out, &errs)
	var s model.Summary
	if err := json.Unmarshal(out.Bytes(), &s); err != nil {
		t.Fatalf("stdout %q: %v", out.String(), err)
	}
	if code != 1 || len(s.Findings) != 1 || s.Findings[0].Kind != model.ClusterDiffers {
		t.Fatalf("exit %d, findings %+v", code, s.Findings)
	}
}

func TestOnceExitsTwoWhenACycleFailsAndStillPrintsTheSummary(t *testing.T) {
	var out, errs bytes.Buffer
	code := run(onceArgs(t, fakeStrata(t, `{"same":true}`, 2, false)), &out, &errs)
	var s model.Summary
	if err := json.Unmarshal(out.Bytes(), &s); err != nil {
		t.Fatalf("stdout %q: %v", out.String(), err)
	}
	if code != 2 || s.Content.FailedRuns != 1 || s.Content.LastError == nil || s.Content.LastError.Kind != docs.KindTransport {
		t.Fatalf("exit %d, content %+v", code, s.Content)
	}
	if strings.Contains(out.String()+errs.String(), "SENTINEL") {
		t.Errorf("the message of strata left the process: %s %s", out.String(), errs.String())
	}
}

func TestOnceRefusesAMissingFlagAndAPeriodThatIsNotPositive(t *testing.T) {
	good := onceArgs(t, "strata")
	without := func(flag string) []string {
		var out []string
		for i := 0; i < len(good); i++ {
			if good[i] == flag {
				i++
				continue
			}
			out = append(out, good[i])
		}
		return out
	}
	for _, c := range []struct {
		name string
		args []string
		want string
	}{
		{"no cluster uid", without("--cluster-uid"), "--cluster-uid is required"},
		{"no records", without("--records"), "--records is required"},
		{"no work", without("--work"), "--work is required"},
		{"no state", without("--state"), "--state is required"},
		{"a period of zero", append(slices.Clone(good), "--content-every", "0s"), "more than zero"},
		{"a flag that does not exist", append(slices.Clone(good), "--nope"), "flag provided but not defined"},
	} {
		t.Run(c.name, func(t *testing.T) {
			var out, errs bytes.Buffer
			if code := run(c.args, &out, &errs); code != 2 || !strings.Contains(errs.String(), c.want) || out.Len() != 0 {
				t.Errorf("exit %d, stdout %q, stderr %q", code, out.String(), errs.String())
			}
		})
	}
}

// The code of the observer sends no HTTP POST to Palette. It asks Palette only through strata, and a POST to
// Palette comes from the click "Ask Palette" alone: see runner.FlagPin. The server takes that one POST, so the
// file of its routes names the method. The observer has four HTTP clients: the health check, which calls a
// loopback address, the read of the pods, which sends one GET to the API server of Kubernetes, the seed, which
// gets and updates the four Secrets of the records, and the alerts, which send one POST of a message to the
// webhook of a team. Only internal/api, internal/kube, and
// internal/alert import a package of the network, and so does the stand-in for the API server in
// internal/testkit, which only test files import. Only internal/alert/send.go names the method for the webhook.
func TestOnlyTheHealthCheckTheReadOfThePodsTheSeedAndTheAlertsSendARequestAndOnlyThePageAndTheAlertsPost(t *testing.T) {
	rules := []struct {
		banned []string
		// allowed lists the prefixes of the paths that may hold the text. An empty list allows none.
		allowed []string
	}{
		{[]string{`"POST"`, `http.Post(`, `.PostForm(`}, nil},
		{[]string{`http.MethodPost`}, []string{"internal/api/api.go", "internal/alert/send.go"}},
		{[]string{`"net/http"`, `"net"`}, []string{"internal/api/", "internal/kube/", "internal/alert/send.go", "internal/testkit/"}},
		{[]string{`http.Client`, `http.Get(`, `http.Head(`, `NewRequest`, `http.DefaultClient`, `net.Dial`}, []string{"internal/api/health.go", "internal/kube/kube.go", "internal/kube/secrets.go", "internal/alert/send.go"}},
	}
	err := filepath.WalkDir(".", func(path string, d os.DirEntry, err error) error {
		if err != nil || d.IsDir() || !strings.HasSuffix(path, ".go") || strings.HasSuffix(path, "_test.go") {
			return err
		}
		raw, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		for _, r := range rules {
			if slices.ContainsFunc(r.allowed, func(prefix string) bool { return strings.HasPrefix(filepath.ToSlash(path), prefix) }) {
				continue
			}
			for _, b := range r.banned {
				if strings.Contains(string(raw), b) {
					t.Errorf("%s holds %s", path, b)
				}
			}
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
}
func TestOnceExitsOneWhenAFindingExistsAlsoIfTheOtherCycleFailed(t *testing.T) {
	var out, errs bytes.Buffer
	code := run(onceArgs(t, fakeStrata(t, `{"same":false,"sameContent":false,"rootB":"x","packs":[{"key":"k8s/kubernetes","status":"changed"}]}`, 0, true)), &out, &errs)
	var s model.Summary
	if err := json.Unmarshal(out.Bytes(), &s); err != nil {
		t.Fatalf("stdout %q: %v", out.String(), err)
	}
	if code != 1 || len(s.Findings) != 1 || s.Versions.FailedRuns != 1 || s.Versions.LastError == nil || s.Versions.LastError.Status != 404 {
		t.Fatalf("exit %d, findings %+v, versions %+v", code, s.Findings, s.Versions)
	}
}

// serveArgs turns the arguments of `once` into the arguments of `serve` on a free loopback port.
func serveArgs(t *testing.T, bin string) (args []string, addr string) {
	t.Helper()
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	addr = ln.Addr().String()
	if err := ln.Close(); err != nil {
		t.Fatal(err)
	}
	args = append([]string{"serve"}, onceArgs(t, bin)[1:]...)
	return append(args, "--listen", addr), addr
}

// syncBuffer is a buffer that the goroutine of `serve` and the test share.
type syncBuffer struct {
	mu sync.Mutex
	b  bytes.Buffer
}

func (s *syncBuffer) Write(p []byte) (int, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.b.Write(p)
}

func (s *syncBuffer) String() string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.b.String()
}

// startServe runs `serve` until the test stops it. It gives the function that stops it and gives its exit code.
func startServe(t *testing.T, args []string, addr string) (stop func() int, stderr *syncBuffer) {
	t.Helper()
	ctx, cancel := context.WithCancel(context.Background())
	stderr = &syncBuffer{}
	code := make(chan int, 1)
	go func() { code <- runContext(ctx, args, io.Discard, stderr) }()
	t.Cleanup(cancel)
	deadline := time.Now().Add(10 * time.Second)
	for {
		if run([]string{"health", "--listen", addr}, io.Discard, io.Discard) == 0 {
			break
		}
		select {
		case c := <-code:
			t.Fatalf("serve exited %d: %s", c, stderr.String())
		default:
		}
		if time.Now().After(deadline) {
			t.Fatalf("the server did not start: %s", stderr.String())
		}
		time.Sleep(20 * time.Millisecond)
	}
	return func() int {
		cancel()
		select {
		case c := <-code:
			return c
		case <-time.After(10 * time.Second):
			t.Fatal("serve did not stop")
			return -1
		}
	}, stderr
}

func TestServeRefusesAnAddressThatIsNotLoopbackWithExitTwo(t *testing.T) {
	for _, addr := range []string{"0.0.0.0:8080", ":8080", "[::]:8080", "10.0.0.5:8080", "localhost:8080", "127.0.0.1"} {
		t.Run(addr, func(t *testing.T) {
			var out, errs bytes.Buffer
			args := append([]string{"serve"}, onceArgs(t, "strata")[1:]...)
			code := run(append(args, "--listen="+addr), &out, &errs)
			if code != 2 || !strings.Contains(errs.String(), "--listen") || !strings.Contains(errs.String(), "loopback") || out.Len() != 0 {
				t.Errorf("exit %d, stdout %q, stderr %q", code, out.String(), errs.String())
			}
		})
	}
}

func TestServeRefusesAMissingFlagLikeOnce(t *testing.T) {
	var out, errs bytes.Buffer
	if code := run([]string{"serve", "--listen", "127.0.0.1:0"}, &out, &errs); code != 2 || !strings.Contains(errs.String(), "--cluster-uid is required") {
		t.Errorf("exit %d, stderr %q", code, errs.String())
	}
}

func TestServeAnswersOnTheLoopbackAddressAndStopsWithItsContext(t *testing.T) {
	args, addr := serveArgs(t, fakeStrata(t, `{"same":true,"sameContent":true}`, 0, false))
	stop, stderr := startServe(t, args, addr)
	client := &http.Client{Timeout: 5 * time.Second}
	get := func(method, path, host string) *http.Response {
		t.Helper()
		req, err := http.NewRequest(method, "http://"+addr+path, nil)
		if err != nil {
			t.Fatal(err)
		}
		if host != "" {
			req.Host = host
		}
		resp, err := client.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		t.Cleanup(func() { resp.Body.Close() })
		return resp
	}
	if resp := get("GET", "/v1/cluster", ""); resp.StatusCode != 200 || resp.Header.Get("Content-Type") != "application/json" || resp.Header.Get("Cache-Control") != "no-store" {
		t.Errorf("GET /v1/cluster: %d %v", resp.StatusCode, resp.Header)
	}
	// The route / serves the page, with the headers of the server.
	if resp := get("GET", "/", ""); resp.StatusCode != 200 || !strings.HasPrefix(resp.Header.Get("Content-Type"), "text/html") || resp.Header.Get("Content-Security-Policy") == "" {
		t.Errorf("GET /: %d %v", resp.StatusCode, resp.Header)
	}
	if resp := get("GET", "/app.js", ""); resp.StatusCode != 200 || !strings.HasPrefix(resp.Header.Get("Content-Type"), "text/javascript") {
		t.Errorf("GET /app.js: %d %v", resp.StatusCode, resp.Header)
	}
	if resp := get("POST", "/v1/cluster", ""); resp.StatusCode != http.StatusMethodNotAllowed {
		t.Errorf("POST gave %d", resp.StatusCode)
	}
	if resp := get("GET", "/v1/cluster", "evil.example"); resp.StatusCode != http.StatusMisdirectedRequest {
		t.Errorf("Host: evil.example gave %d", resp.StatusCode)
	}
	if code := stop(); code != 0 {
		t.Errorf("exit %d, stderr %s", code, stderr.String())
	}
	if !strings.Contains(stderr.String(), "listening") || strings.Contains(stderr.String(), "SENTINEL") {
		t.Errorf("stderr %s", stderr.String())
	}
}

func TestServeRunsTheFirstCycleOfEachTimerAtTheStart(t *testing.T) {
	args, addr := serveArgs(t, fakeStrata(t, `{"same":true,"sameContent":true}`, 0, false))
	stop, _ := startServe(t, args, addr)
	defer stop()
	testkit.WaitFor(t, "the page state ok", func() bool {
		resp, err := http.Get("http://" + addr + "/v1/cluster")
		if err != nil {
			return false
		}
		defer resp.Body.Close()
		var got struct {
			Page string `json:"page"`
		}
		return json.NewDecoder(resp.Body).Decode(&got) == nil && got.Page == string(model.PageOK)
	})
}

func TestHealthExitsZeroWithTheServerUpAndOneWithNone(t *testing.T) {
	args, addr := serveArgs(t, fakeStrata(t, `{"same":true,"sameContent":true}`, 0, false))
	stop, _ := startServe(t, args, addr)
	var errs bytes.Buffer
	if code := run([]string{"health", "--listen", addr}, io.Discard, &errs); code != 0 {
		t.Errorf("with the server up: exit %d, stderr %q", code, errs.String())
	}
	stop()
	errs.Reset()
	if code := run([]string{"health", "--listen", addr}, io.Discard, &errs); code != 1 || errs.Len() == 0 {
		t.Errorf("with no server: exit %d, stderr %q", code, errs.String())
	}
}

func TestHealthRefusesAWrongFlagAndAnAddressThatIsNotLoopback(t *testing.T) {
	for _, args := range [][]string{{"health", "--nope"}, {"health", "--listen", "10.0.0.5:8080"}, {"health", "--listen", "evil.example:80"}} {
		var errs bytes.Buffer
		if code := run(args, io.Discard, &errs); code != 2 {
			t.Errorf("%v: exit %d, stderr %q", args, code, errs.String())
		}
	}
}

// holdingStrata writes a script that stands for the strata binary, with one pin that is behind. While
// the file hold exists, the compare of two packs writes its process id to pids and waits. It replaces
// itself with the wait, so the process id is the one that the observer started.
func holdingStrata(t *testing.T, hold, pids string) string {
	t.Helper()
	return testkit.Strata(t, `case $cmd in
verify) echo '`+testkit.VerifyUnchanged+`' ;;
snapshot) echo '`+testkit.ClusterPayments+`' > "$out" ;;
compare)
  case $ref in
  pack:*)
    if [ -e '`+hold+`' ]; then echo $$ > '`+pids+`'; exec sleep 60; fi
    echo '{"schema":"`+docs.DiffSchema+`","same":false,"sameContent":false,"summary":{"packsChanged":1,"maxImpact":"in-place"},"packs":[{"key":"k8s/kubernetes","name":"kubernetes","layer":"k8s","status":"changed","authored":true,"impact":{"level":"in-place","text":"x"},"values":{"parseable":true,"changedCount":1,"diff":[{"op":"changed","path":"a.b","old":"1","new":"2"}]}}]}'
    exit 1 ;;
  *) echo '`+testkit.CompareSame+`' ;;
  esac ;;
behind)
  echo '{"schema":"`+docs.BehindSchema+`","tool":{"version":"0.29.0"},"pins":[{"name":"kubernetes","layer":"k8s","registry":"Public Repo","registryUid":"reg1","version":"1.35.3","tag":"1.35.3","pinnedBy":[{"subject":"aws-stack@1.1.0"}],"newer":[{"version":"1.36.2","listed":"2026-11-01T00:00:00Z","drops":[]}],"behind":true}]}'
  exit 1 ;;
esac`)
}

// newerRows counts the rows of /v1/newer.
func newerRows(addr string) int {
	resp, err := http.Get("http://" + addr + "/v1/newer")
	if err != nil {
		return 0
	}
	defer resp.Body.Close()
	var got struct {
		Newer []json.RawMessage `json:"newer"`
	}
	if json.NewDecoder(resp.Body).Decode(&got) != nil {
		return 0
	}
	return len(got.Newer)
}

// After a restart the memory holds no values for a move, so a request of the newer side starts one
// compare. A stop during that request must cancel it: the server exits 0, and no process stays.
func TestAStopWithADetailRequestRunningExitsZeroAndLeavesNoProcess(t *testing.T) {
	dir := t.TempDir()
	hold, pids := filepath.Join(dir, "hold"), filepath.Join(dir, "pids")
	args, addr := serveArgs(t, holdingStrata(t, hold, pids))
	first, _ := startServe(t, args, addr)
	testkit.WaitFor(t, "the first versions cycle", func() bool { return newerRows(addr) == 1 })
	if code := first(); code != 0 {
		t.Fatalf("the first server exited %d", code)
	}

	// The restart: the state holds the counts of the move, and the compare waits.
	if err := os.WriteFile(hold, nil, 0o600); err != nil {
		t.Fatal(err)
	}
	stop, stderr := startServe(t, args, addr)
	testkit.WaitFor(t, "the versions cycle after the restart", func() bool { return newerRows(addr) == 1 })
	answered := make(chan struct{})
	go func() {
		defer close(answered)
		client := &http.Client{Timeout: 30 * time.Second}
		resp, err := client.Get("http://" + addr + "/v1/packs/aws-stack@1.1.0/k8s/kubernetes/diff?side=newer")
		if err == nil {
			resp.Body.Close()
		}
	}()
	var pid int
	testkit.WaitFor(t, "the compare of the detail request", func() bool {
		raw, err := os.ReadFile(pids)
		if err != nil {
			return false
		}
		pid, err = strconv.Atoi(strings.TrimSpace(string(raw)))
		return err == nil
	})

	if code := stop(); code != 0 {
		t.Errorf("exit %d, stderr %s", code, stderr.String())
	}
	if err := syscall.Kill(pid, 0); !errors.Is(err, syscall.ESRCH) {
		t.Errorf("the process %d of the compare is still there: %v", pid, err)
	}
	select {
	case <-answered:
	case <-time.After(10 * time.Second):
		t.Error("the detail request did not end")
	}
}

// A process runs in the directory of its cycle, and Go reads a relative path of a command from the
// directory of the process. So a relative --strata must become absolute at the start, or each run
// fails with no process.
func TestARelativeStrataPathAndANameOnThePathRunInEachCycle(t *testing.T) {
	bin := fakeStrata(t, `{"same":true,"sameContent":true}`, 0, false)
	dir := filepath.Dir(bin)
	for _, c := range []struct{ name, path string }{
		{"the current directory", "./strata"},
		{"a sibling directory", "../" + filepath.Base(dir) + "/strata"},
		{"a name on the path", "strata"},
	} {
		t.Run(c.name, func(t *testing.T) {
			t.Chdir(dir)
			t.Setenv("PATH", dir+string(os.PathListSeparator)+os.Getenv("PATH"))
			var out, errs bytes.Buffer
			if code := run(onceArgs(t, c.path), &out, &errs); code != 0 {
				t.Fatalf("exit %d, stderr %q", code, errs.String())
			}
			var s model.Summary
			if err := json.Unmarshal(out.Bytes(), &s); err != nil {
				t.Fatalf("stdout %q: %v", out.String(), err)
			}
			if s.Page != model.PageOK || s.Content.FailedRuns != 0 {
				t.Errorf("summary %+v", s)
			}
		})
	}
}

// podsAPI starts a stand-in for the API server of Kubernetes with one pod, and points the variables
// of the kubelet at it. The pod holds a marker in its environment.
func podsAPI(t *testing.T) *testkit.KubeAPI {
	t.Helper()
	api := testkit.NewKubeAPI(t, func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write([]byte(testkit.PodList("", testkit.PodJSON("NS-MARKER", "POD-MARKER", "a/b:1", "a/b@sha256:"+strings.Repeat("1", 64)))))
	})
	for name, value := range api.Env() {
		t.Setenv(name, value)
	}
	return api
}

func TestWithoutReadPodsTheObserverMakesNoKubernetesCall(t *testing.T) {
	api := podsAPI(t)
	var out, errs bytes.Buffer
	args := append(onceArgs(t, fakeStrata(t, `{"same":true,"sameContent":true}`, 0, false)), "--kube-dir", api.Dir())
	if code := run(args, &out, &errs); code != 0 {
		t.Fatalf("exit %d, stderr %s", code, errs.String())
	}
	if n := len(api.Requests()); n != 0 {
		t.Errorf("%d requests went to the API server", n)
	}
	serve, addr := serveArgs(t, fakeStrata(t, `{"same":true,"sameContent":true}`, 0, false))
	stop, _ := startServe(t, append(serve, "--kube-dir", api.Dir()), addr)
	resp, err := http.Get("http://" + addr + "/v1/images")
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	var doc model.Observed
	if err := json.NewDecoder(resp.Body).Decode(&doc); err != nil || doc.Pods != model.PodsOff || len(doc.Images) != 0 {
		t.Errorf("document %+v, error %v", doc, err)
	}
	stop()
	if n := len(api.Requests()); n != 0 {
		t.Errorf("%d requests went to the API server with the server running", n)
	}
}

func TestWithReadPodsTheContentCycleReadsThePodsOnceAndNoValueOfAPodLeaves(t *testing.T) {
	api := podsAPI(t)
	var out, errs bytes.Buffer
	args := append(onceArgs(t, fakeStrata(t, `{"same":true,"sameContent":true}`, 0, false)), "--read-pods", "--kube-dir", api.Dir())
	if code := run(args, &out, &errs); code != 0 {
		t.Fatalf("exit %d, stderr %s", code, errs.String())
	}
	seen := api.Requests()
	if len(seen) != 1 || seen[0].Method != http.MethodGet || seen[0].Path != "/api/v1/pods" || seen[0].Auth != "Bearer token-1" {
		t.Errorf("requests %+v", seen)
	}
	if strings.Contains(out.String()+errs.String(), "MARKER") {
		t.Errorf("a value of a pod left the process: %s %s", out.String(), errs.String())
	}
}

func TestServeWithReadPodsServesTheDigestsOfTheImages(t *testing.T) {
	api := podsAPI(t)
	args, addr := serveArgs(t, fakeStrata(t, `{"same":true,"sameContent":true}`, 0, false))
	stop, _ := startServe(t, append(args, "--read-pods", "--kube-dir", api.Dir()), addr)
	var doc model.Observed
	testkit.WaitFor(t, "the first read of the pods", func() bool {
		resp, err := http.Get("http://" + addr + "/v1/images")
		if err != nil {
			return false
		}
		defer resp.Body.Close()
		doc = model.Observed{}
		return json.NewDecoder(resp.Body).Decode(&doc) == nil && doc.Pods == model.PodsRead
	})
	if doc.Counts.Images != 3 || doc.Images[0].Ref == "" {
		t.Errorf("document %+v", doc)
	}
	stop()
}

func TestReadPodsNeedsTheVariablesOfTheKubelet(t *testing.T) {
	t.Setenv("KUBERNETES_SERVICE_HOST", "")
	t.Setenv("KUBERNETES_SERVICE_PORT", "")
	for _, command := range []string{"once", "serve"} {
		t.Run(command, func(t *testing.T) {
			var out, errs bytes.Buffer
			args := append([]string{command}, onceArgs(t, "strata")[1:]...)
			if code := run(append(args, "--read-pods"), &out, &errs); code != 2 || !strings.Contains(errs.String(), "KUBERNETES_SERVICE_HOST") || out.Len() != 0 {
				t.Errorf("exit %d, stdout %q, stderr %q", code, out.String(), errs.String())
			}
		})
	}
}

// The seed needs the uid of the cluster, and the variables that the kubelet gives a pod. Outside a pod
// it stops with the kind of the error, and it runs no strata.
func TestSeedNeedsTheClusterAndThePod(t *testing.T) {
	var out, errs strings.Builder
	if code := runContext(context.Background(), []string{"seed"}, &out, &errs); code != 2 || !strings.Contains(errs.String(), "--cluster-uid is empty") {
		t.Errorf("exit %d, stderr %q", code, errs.String())
	}
	errs.Reset()
	t.Setenv("KUBERNETES_SERVICE_HOST", "")
	if code := runContext(context.Background(), []string{"seed", "--cluster-uid", "c1", "--namespace", "strata-observer", "--strata", "/no/strata"}, &out, &errs); code != 2 || !strings.Contains(errs.String(), "kube-config") {
		t.Errorf("exit %d, stderr %q", code, errs.String())
	}
}
