package main

import (
	"bytes"
	"compress/gzip"
	"context"
	"encoding/json"
	"io"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"slices"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
)

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
	body := `#!/bin/sh
cmd=$1
shift
out=
while [ $# -gt 0 ]; do
  if [ "$1" = -o ]; then out=$2; fi
  shift
done
case $cmd in
verify)
  if [ ` + strconv.Itoa(verifyExit) + ` = 2 ]; then echo '{"error":{"kind":"transport","message":"SENTINEL"}}' >&2; exit 2; fi
  echo '{"schema":"strata-verify-summary/1","tool":{"version":"0.29.0"},"records":[{"path":"record-0.json","status":"unchanged","name":"aws-stack","version":"1.1.0"}]}' ;;
snapshot) echo '{"source":{"cluster":{"uid":"c1","name":"payments","profiles":[{"name":"aws-stack","version":"1.1.0","type":"cluster"}]}}}' > "$out" ;;
compare) echo '` + compare + `'; case '` + compare + `' in *'"same":true'*) exit 0 ;; *) exit 1 ;; esac ;;
behind)
  if [ ` + strconv.FormatBool(behindFails) + ` = true ]; then echo '{"error":{"kind":"api","status":404}}' >&2; exit 2; fi
  echo '{"schema":"strata-behind/1","tool":{"version":"0.29.0"},"pins":[]}' ;;
esac
`
	bin := filepath.Join(t.TempDir(), "strata")
	if err := os.WriteFile(bin, []byte(body), 0o755); err != nil {
		t.Fatal(err)
	}
	return bin
}

// onceArgs makes the directories of a run of `once` with one record.
func onceArgs(t *testing.T, bin string) []string {
	t.Helper()
	records := t.TempDir()
	record := `{"schema":"strata-snapshot/1","tool":{"version":"0.28.0"},"source":{"uid":"u1","name":"aws-stack","version":"1.1.0","type":"cluster"},"packs":[]}`
	var buf bytes.Buffer
	zw := gzip.NewWriter(&buf)
	if _, err := zw.Write([]byte(record)); err != nil {
		t.Fatal(err)
	}
	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(records, "aws-stack.json.gz"), buf.Bytes(), 0o600); err != nil {
		t.Fatal(err)
	}
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

// The observer sends no POST. It asks Palette only through strata. The one HTTP client is the health
// check, and it calls a loopback address. Only internal/api imports a package of the network.
func TestOnlyTheHealthCheckSendsARequestAndNoCodeSendsAPost(t *testing.T) {
	rules := []struct {
		banned []string
		// allowed is the prefix of the paths that may hold the text. An empty prefix allows none.
		allowed string
	}{
		{[]string{`http.MethodPost`, `"POST"`, `http.Post(`, `.PostForm(`}, ""},
		{[]string{`"net/http"`, `"net"`}, "internal/api/"},
		{[]string{`http.Client`, `http.Get(`, `http.Head(`, `NewRequest`, `http.DefaultClient`, `net.Dial`}, "internal/api/health.go"},
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
			if r.allowed != "" && strings.HasPrefix(filepath.ToSlash(path), r.allowed) {
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
	deadline := time.Now().Add(10 * time.Second)
	for {
		resp, err := http.Get("http://" + addr + "/v1/cluster")
		if err != nil {
			t.Fatal(err)
		}
		var got struct {
			Page string `json:"page"`
		}
		err = json.NewDecoder(resp.Body).Decode(&got)
		resp.Body.Close()
		if err != nil {
			t.Fatal(err)
		}
		if got.Page == string(model.PageOK) {
			return
		}
		if time.Now().After(deadline) {
			t.Fatalf("the page is %q", got.Page)
		}
		time.Sleep(20 * time.Millisecond)
	}
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
