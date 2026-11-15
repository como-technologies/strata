package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"slices"
	"strconv"
	"strings"
	"syscall"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/api"
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
		t.Fatalf(testkit.ExitStderr, code, errs.String())
	}
	if !strings.HasPrefix(out.String(), "strata-observer ") {
		t.Fatalf("stdout %q", out.String())
	}
}

func TestAnUnknownCommandExitsTwo(t *testing.T) {
	exitsTwoSaying(t, []string{"nope"}, "usage:")
}

// exitsTwoSaying runs the observer with args, and fails when it does not exit 2 with each of the words on stderr and
// nothing on stdout. It gives the text on stderr.
func exitsTwoSaying(t *testing.T, args []string, words ...string) string {
	t.Helper()
	var out, errs bytes.Buffer
	code := run(args, &out, &errs)
	testkit.ExitsTwo(t, code, out.String(), errs.String(), words...)
	return errs.String()
}

// runOnce runs the observer with args, and reads the summary that it prints on stdout. It gives the exit code, the
// summary, and the text of stdout and of stderr.
func runOnce(t *testing.T, args []string) (code int, s model.Summary, stdout, stderr string) {
	t.Helper()
	var out, errs bytes.Buffer
	code = run(args, &out, &errs)
	testkit.Decode(t, fmt.Sprintf("the stdout of a run that exits %d, with the stderr %q,", code, errs.String()), out.Bytes(), &s)
	return code, s, out.String(), errs.String()
}

const (
	// clusterUIDRequired is the error of a run with no --cluster-uid.
	clusterUIDRequired = "--cluster-uid is required"
	// alertSend is the file that sends the message of an alert to a webhook.
	alertSend = "internal/alert/send.go"
	// exitFindings is the failure of a check of the exit code and the findings of a run. kubeRequests counts the requests
	// to the API server of the cluster.
	exitFindings = "exit %d, findings %+v"
	kubeRequests = "%d requests went to the API server"
)

// fakeStrata writes a script that stands for the strata binary. compare is the line that
// `compare` prints. verifyExit is the exit code of `verify`.
func fakeStrata(t *testing.T, compare string, verifyExit int, behindFails bool) string {
	t.Helper()
	return testkit.Strata(t, testkit.VerifyFirst+`  if [ `+strconv.Itoa(verifyExit)+` = 2 ]; then `+testkit.ErrorLine(docs.KindTransport, `"message":"SENTINEL"`)+`; exit 2; fi
`+testkit.VerifyAndSnapshot+`compare) echo '`+compare+`'; case '`+compare+`' in *'"same":true'*) exit 0 ;; *) exit 1 ;; esac ;;
behind)
  if [ `+strconv.FormatBool(behindFails)+` = true ]; then `+testkit.ErrorLine(docs.KindAPI, `"status":404`)+`; exit 2; fi
  echo '{"schema":"`+docs.BehindSchema+`","tool":{"version":"0.29.0"},"pins":[]}' ;;
esac`)
}

// onceArgs makes the directories of a run of `once` with one record.
func onceArgs(t *testing.T, bin string) []string {
	t.Helper()
	d := testkit.NewRigDirs(t)
	testkit.WriteGzip(t, d.Records, testkit.RecordFile, testkit.AWSStack(testkit.NoPacks))
	return []string{"once", "--strata", bin, "--cluster-uid", testkit.PaymentsUID, "--records", d.Records, "--work", d.Work, "--state", d.State}
}

func TestOnceExitsZeroWithNoFindingAndPrintsTheSummary(t *testing.T) {
	code, s, _, errs := runOnce(t, onceArgs(t, fakeStrata(t, testkit.CompareSame, 0, false)))
	if code != 0 {
		t.Fatalf(testkit.ExitStderr, code, errs)
	}
	if s.Page != model.PageOK || s.Cluster.Name != testkit.PaymentsName || len(s.Findings) != 0 || s.Records.Count != 1 {
		t.Errorf("summary %+v", s)
	}
}

func TestOnceExitsOneWhenAFindingExists(t *testing.T) {
	code, s, _, _ := runOnce(t, onceArgs(t, fakeStrata(t, drifting, 0, false)))
	if code != 1 || len(s.Findings) != 1 || s.Findings[0].Kind != model.ClusterDiffers {
		t.Fatalf(exitFindings, code, s.Findings)
	}
}

func TestOnceExitsTwoWhenACycleFailsAndStillPrintsTheSummary(t *testing.T) {
	code, s, out, errs := runOnce(t, onceArgs(t, fakeStrata(t, testkit.CompareSame, 2, false)))
	if code != 2 || s.Content.FailedRuns != 1 || s.Content.LastError == nil || s.Content.LastError.Kind != docs.KindTransport {
		t.Fatalf("exit %d, content %+v", code, s.Content)
	}
	testkit.HoldsNone(t, "the output of a failed cycle", out+errs, "SENTINEL")
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
		{"no cluster uid", without("--cluster-uid"), clusterUIDRequired},
		{"no records", without("--records"), "--records is required"},
		{"no work", without("--work"), "--work is required"},
		{"no state", without("--state"), "--state is required"},
		{"a period of zero", append(slices.Clone(good), "--content-every", "0s"), "more than zero"},
		{"a flag that does not exist", append(slices.Clone(good), "--nope"), "flag provided but not defined"},
	} {
		t.Run(c.name, func(t *testing.T) { exitsTwoSaying(t, c.args, c.want) })
	}
}

// The code of the observer sends no HTTP POST to Palette. It asks Palette only through strata, and a POST to
// Palette comes from the click "Ask Palette" alone: see runner.FlagPin. The server takes that one POST, so the
// file of its routes names the method.
//
// The observer has four HTTP clients. The health check calls a loopback address. The read of the pods sends
// one GET to the API server of Kubernetes. The seed gets and updates the four Secrets of the records. The
// alerts send one POST of a message to the webhook of a team.
//
// Only internal/api, internal/kube, and internal/alert import a package of the network. So do the stand-in
// for the API server and the webhook of the tests in internal/testkit, which only test files import. Only
// internal/alert/send.go names the method for the webhook. testkit.Serve gives a request to a handler with
// httptest.NewRequest, which sends nothing on the network, so the rule bans http.NewRequest.
func TestOnlyTheHealthCheckTheReadOfThePodsTheSeedAndTheAlertsSendARequestAndOnlyThePageAndTheAlertsPost(t *testing.T) {
	rules := []struct {
		banned []string
		// allowed lists the prefixes of the paths that may hold the text. An empty list allows none.
		allowed []string
	}{
		{[]string{`"POST"`, `http.Post(`, `.PostForm(`}, nil},
		{[]string{`http.MethodPost`}, []string{"internal/api/api.go", alertSend}},
		{[]string{`"net/http"`, `"net"`}, []string{"internal/api/", "internal/kube/", alertSend, "internal/testkit/"}},
		{[]string{`http.Client`, `http.Get(`, `http.Head(`, `http.NewRequest`, `http.DefaultClient`, `net.Dial`}, []string{"internal/api/health.go", "internal/kube/kube.go", "internal/kube/secrets.go", alertSend}},
	}
	for path, raw := range goFiles(t) {
		if strings.HasSuffix(path, "_test.go") {
			continue
		}
		for _, r := range rules {
			if slices.ContainsFunc(r.allowed, func(prefix string) bool { return strings.HasPrefix(filepath.ToSlash(path), prefix) }) {
				continue
			}
			testkit.HoldsNone(t, path, string(raw), r.banned...)
		}
	}
}

// goFiles gives the text of each Go file of the observer, by its path.
func goFiles(t *testing.T) map[string][]byte {
	t.Helper()
	files := map[string][]byte{}
	err := filepath.WalkDir(".", func(path string, d os.DirEntry, err error) error {
		if err != nil || d.IsDir() || !strings.HasSuffix(path, ".go") {
			return err
		}
		files[path], err = os.ReadFile(path)
		return err
	})
	if err != nil {
		t.Fatal(err)
	}
	return files
}

// parsedFiles parses each Go file of the observer, and gives each file by its path.
func parsedFiles(t *testing.T) (*token.FileSet, map[string]*ast.File) {
	t.Helper()
	fset, parsed := token.NewFileSet(), map[string]*ast.File{}
	for path, raw := range goFiles(t) {
		f, err := parser.ParseFile(fset, path, raw, parser.SkipObjectResolution)
		if err != nil {
			t.Fatal(err)
		}
		parsed[path] = f
	}
	return fset, parsed
}

// A test that closes a free port and gives it to a server can lose the port. Another process can bind
// it first, and answer in place of the server. So no test of the observer calls net.Listen. A test
// starts its server on testkit.PortZero and reads the port from the log, or it uses httptest.
func TestNoTestGivesAServerAPortThatItClosed(t *testing.T) {
	fset, files := parsedFiles(t)
	for path, f := range files {
		if !strings.HasSuffix(path, "_test.go") {
			continue
		}
		ast.Inspect(f, func(n ast.Node) bool {
			if s, ok := n.(*ast.SelectorExpr); ok && s.Sel.Name == "Listen" {
				if x, ok := s.X.(*ast.Ident); ok && x.Name == "net" {
					t.Errorf("%s calls net.Listen", fset.Position(s.Pos()))
				}
			}
			return true
		})
	}
}

// The kernel can give the port of a closed server to another process at once, and a server there can
// answer. So no test uses a name after a statement that closes it, such as srv.Close(). A test of "no
// server" asks testkit.PortZero. This test finds only a call of Close. It does not find a server that a
// test stops with a cancel of its context, such as the stop of testkit.StartServer.
func TestNoTestAsksAServerThatItClosed(t *testing.T) {
	fset, files := parsedFiles(t)
	for path, f := range files {
		if !strings.HasSuffix(path, "_test.go") {
			continue
		}
		ast.Inspect(f, func(n ast.Node) bool {
			var body *ast.BlockStmt
			switch n := n.(type) {
			case *ast.FuncDecl:
				body = n.Body
			case *ast.FuncLit:
				body = n.Body
			}
			if body == nil {
				return true
			}
			// A close in a nested function runs later, so only the statements of this body count.
			closed := map[string]token.Pos{}
			ast.Inspect(body, func(n ast.Node) bool {
				if _, ok := n.(*ast.FuncLit); ok {
					return false
				}
				if s, ok := n.(*ast.ExprStmt); ok {
					if c, ok := s.X.(*ast.CallExpr); ok {
						if sel, ok := c.Fun.(*ast.SelectorExpr); ok && sel.Sel.Name == "Close" {
							if x, ok := sel.X.(*ast.Ident); ok {
								if _, seen := closed[x.Name]; !seen {
									closed[x.Name] = s.End()
								}
							}
						}
					}
				}
				return true
			})
			ast.Inspect(body, func(n ast.Node) bool {
				if id, ok := n.(*ast.Ident); ok {
					if end, ok := closed[id.Name]; ok && id.Pos() > end {
						t.Errorf("%s uses %s after it closes it", fset.Position(id.Pos()), id.Name)
					}
				}
				return true
			})
			return true
		})
	}
}

// A local name hides a package in its scope when the file imports a package of that name. An example is
// a variable api in a file that imports internal/api. No file of the observer declares such a name.
func TestNoLocalNameHidesAPackageThatItsFileImports(t *testing.T) {
	fset, files := parsedFiles(t)
	for _, f := range files {
		imported := map[string]bool{}
		for _, imp := range f.Imports {
			name := strings.Trim(imp.Path.Value, `"`)
			name = name[strings.LastIndex(name, "/")+1:]
			if imp.Name != nil {
				name = imp.Name.Name
			}
			imported[name] = true
		}
		check := func(ids ...*ast.Ident) {
			for _, id := range ids {
				if imported[id.Name] {
					t.Errorf("%s: the name %s hides the package", fset.Position(id.Pos()), id.Name)
				}
			}
		}
		fields := func(l *ast.FieldList) {
			for _, field := range l.List {
				check(field.Names...)
			}
		}
		ast.Inspect(f, func(n ast.Node) bool {
			switch n := n.(type) {
			case *ast.AssignStmt:
				if n.Tok == token.DEFINE {
					for _, e := range n.Lhs {
						if id, ok := e.(*ast.Ident); ok {
							check(id)
						}
					}
				}
			case *ast.RangeStmt:
				if n.Tok == token.DEFINE {
					for _, e := range []ast.Expr{n.Key, n.Value} {
						if id, ok := e.(*ast.Ident); ok {
							check(id)
						}
					}
				}
			case *ast.ValueSpec:
				check(n.Names...)
			case *ast.FuncDecl:
				if n.Recv != nil {
					fields(n.Recv)
				}
			case *ast.FuncType:
				fields(n.Params)
				if n.Results != nil {
					fields(n.Results)
				}
			}
			return true
		})
	}
}

func TestOnceExitsOneWhenAFindingExistsAlsoIfTheOtherCycleFailed(t *testing.T) {
	code, s, _, _ := runOnce(t, onceArgs(t, fakeStrata(t, drifting, 0, true)))
	if code != 1 || len(s.Findings) != 1 || s.Versions.FailedRuns != 1 || s.Versions.LastError == nil || s.Versions.LastError.Status != 404 {
		t.Fatalf(exitFindings+", versions %+v", code, s.Findings, s.Versions)
	}
}

// serveArgs changes the arguments of `once` to the arguments of `serve` on testkit.PortZero. So the
// kernel gives the server a free port.
func serveArgs(t *testing.T, bin string) []string {
	t.Helper()
	return append(commandArgs(t, "serve", bin), "--listen", testkit.PortZero)
}

// commandArgs gives the arguments of onceArgs for another command, such as serve.
func commandArgs(t *testing.T, command, bin string) []string {
	t.Helper()
	return append([]string{command}, onceArgs(t, bin)[1:]...)
}

// startServe runs `serve` until the test stops it. It gives the address of the server, the function that
// stops it and gives its exit code, and its log.
func startServe(t *testing.T, args []string) (addr string, stop func() int, stderr *testkit.Log) {
	t.Helper()
	return testkit.StartServer(t, func(ctx context.Context, log io.Writer) int {
		return runContext(ctx, args, io.Discard, log)
	})
}

func TestServeRefusesAnAddressThatIsNotLoopbackWithExitTwo(t *testing.T) {
	for _, addr := range []string{"0.0.0.0:8080", ":8080", "[::]:8080", "10.0.0.5:8080", "localhost:8080", "127.0.0.1"} {
		t.Run(addr, func(t *testing.T) {
			args := commandArgs(t, "serve", "strata")
			exitsTwoSaying(t, append(args, "--listen="+addr), "--listen", "loopback")
		})
	}
}

func TestServeRefusesAMissingFlagLikeOnce(t *testing.T) {
	exitsTwoSaying(t, []string{"serve", "--listen", testkit.PortZero}, clusterUIDRequired)
}

func TestServeAnswersOnTheLoopbackAddressAndStopsWithItsContext(t *testing.T) {
	args := serveArgs(t, fakeStrata(t, testkit.CompareSame, 0, false))
	addr, stop, stderr := startServe(t, args)
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
	if resp := get("GET", "/v1/cluster", ""); resp.StatusCode != 200 || resp.Header.Get("Content-Type") != testkit.JSONType || resp.Header.Get("Cache-Control") != "no-store" {
		t.Errorf("GET /v1/cluster: %d %v", resp.StatusCode, resp.Header)
	}
	// The route / serves the page, with the headers of the server.
	if resp := get("GET", "/", ""); resp.StatusCode != 200 || !strings.HasPrefix(resp.Header.Get("Content-Type"), "text/html") || resp.Header.Get(testkit.HeaderPolicy) == "" {
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
		t.Errorf(testkit.ExitStderr, code, stderr.String())
	}
	testkit.HoldsNone(t, "stderr", stderr.String(), "SENTINEL")
}

func TestServeRunsTheFirstCycleOfEachTimerAtTheStart(t *testing.T) {
	args := serveArgs(t, fakeStrata(t, testkit.CompareSame, 0, false))
	addr, stop, _ := startServe(t, args)
	defer stop()
	// The state of the page reads the content cycle only, so the wait also reads the versions cycle.
	testkit.WaitFor(t, testkit.FirstCycles, func() bool {
		var got api.ClusterView
		return getJSON(addr, "/v1/cluster", &got) && got.Page == model.PageOK && got.Versions.ReadAt != nil
	})
}

func TestHealthExitsZeroWithTheServerUpAndOneWithNone(t *testing.T) {
	args := serveArgs(t, fakeStrata(t, testkit.CompareSame, 0, false))
	addr, stop, _ := startServe(t, args)
	var errs bytes.Buffer
	if code := run([]string{"health", "--listen", addr}, io.Discard, &errs); code != 0 {
		t.Errorf("with the server up: "+testkit.ExitStderr, code, errs.String())
	}
	stop()
	// The check asks testkit.PortZero, because another process can take the port of the stopped server.
	errs.Reset()
	if code := run([]string{"health", "--listen", testkit.PortZero}, io.Discard, &errs); code != 1 || errs.Len() == 0 {
		t.Errorf("with no server: "+testkit.ExitStderr, code, errs.String())
	}
}

func TestHealthRefusesAWrongFlagAndAnAddressThatIsNotLoopback(t *testing.T) {
	for _, args := range [][]string{{"health", "--nope"}, {"health", "--listen", "10.0.0.5:8080"}, {"health", "--listen", "evil.example:80"}} {
		t.Run(strings.Join(args, " "), func(t *testing.T) { exitsTwoSaying(t, args) })
	}
}

// holdingStrata writes a script that stands for the strata binary, with one pin that is behind. While
// the file hold exists, the compare of two packs writes its process id to pids and waits. It replaces
// itself with the wait, so the process id is the one that the observer started. While hold exists,
// `verify` also waits 0.2 s, so the content cycle usually ends after the versions cycle.
func holdingStrata(t *testing.T, hold, pids string) string {
	t.Helper()
	return testkit.Strata(t, testkit.VerifyFirst+`  if [ -e '`+hold+`' ]; then sleep 0.2; fi
`+testkit.VerifyAndSnapshot+testkit.PacksFirst+`    if [ -e '`+hold+`' ]; then echo $$ > '`+pids+`'; exec sleep 60; fi
    echo '{"schema":"`+docs.DiffSchema+`","same":false,"sameContent":false,"summary":{"packsChanged":1,"maxImpact":"in-place"},"packs":[{"key":"`+testkit.KubernetesKey+`","name":"kubernetes","layer":"k8s","status":"changed","authored":true,"impact":{"level":"in-place","text":"x"},"values":{"parseable":true,"changedCount":1,"diff":[{"op":"changed","path":"a.b","old":"1","new":"2"}]}}]}'
    exit 1 ;;
  *) echo '`+testkit.CompareSame+`' ;;
  esac ;;
behind)
  echo '`+testkit.KubernetesBehind+`'
  exit 1 ;;
esac`)
}

// getJSON reads the answer of the route path of the server into v. It is false when the server does
// not answer 200, or when the answer does not decode.
func getJSON(addr, path string, v any) bool {
	resp, err := http.Get("http://" + addr + path)
	if err != nil {
		return false
	}
	defer resp.Body.Close()
	// A read to the end of the body lets the client use the connection again. So a wait does not open
	// a new connection at each try.
	raw, err := io.ReadAll(resp.Body)
	return err == nil && resp.StatusCode == http.StatusOK && json.Unmarshal(raw, v) == nil
}

// newerRows counts the rows of /v1/newer.
func newerRows(addr string) int {
	var got api.NewerView
	if !getJSON(addr, "/v1/newer", &got) {
		return 0
	}
	return len(got.Newer)
}

// After a restart the memory holds no values for a move, so a request of the newer side starts one
// compare. A stop during that request must cancel it: the server exits 0, and no process stays.
func TestAStopWithADetailRequestRunningExitsZeroAndLeavesNoProcess(t *testing.T) {
	const subject = testkit.AWSStackProfile
	dir := t.TempDir()
	hold, pids := filepath.Join(dir, "hold"), filepath.Join(dir, "pids")
	args := serveArgs(t, holdingStrata(t, hold, pids))
	firstAddr, first, _ := startServe(t, args)
	// The observer keeps the answer of a cycle and writes the state while it holds one lock. So a row
	// shows that the state holds the counts of the move.
	testkit.WaitFor(t, "the first versions cycle", func() bool { return newerRows(firstAddr) == 1 })
	if code := first(); code != 0 {
		t.Fatalf("the first server exited %d", code)
	}

	// The restart: the state holds the counts of the move, and the compare waits. The server gets a
	// new port.
	testkit.WriteFile(t, hold, "")
	addr, stop, stderr := startServe(t, args)
	// The detail route finds the pin in the versions cycle and the profile in the content cycle. The
	// two cycles end in either order, so the wait reads both.
	testkit.WaitFor(t, "the two cycles after the restart", func() bool {
		return newerRows(addr) == 1 && getJSON(addr, "/v1/profiles/"+subject, &api.ProfileView{})
	})
	answered := make(chan struct{})
	go func() {
		defer close(answered)
		client := &http.Client{Timeout: 30 * time.Second}
		resp, err := client.Get("http://" + addr + "/v1/packs/" + subject + testkit.KubernetesDiff + "?side=newer")
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
		t.Errorf(testkit.ExitStderr, code, stderr.String())
	}
	if err := syscall.Kill(pid, 0); !errors.Is(err, syscall.ESRCH) {
		t.Errorf("the process %d of the compare is still there: %v", pid, err)
	}
	select {
	case <-answered:
	case <-time.After(testkit.WaitLimit):
		t.Error("the detail request did not end")
	}
}

// A process runs in the directory of its cycle, and Go reads a relative path of a command from the
// directory of the process. So a relative --strata must become absolute at the start, or each run
// fails with no process.
func TestARelativeStrataPathAndANameOnThePathRunInEachCycle(t *testing.T) {
	bin := fakeStrata(t, testkit.CompareSame, 0, false)
	dir := filepath.Dir(bin)
	for _, c := range []struct{ name, path string }{
		{"the current directory", "./strata"},
		{"a sibling directory", "../" + filepath.Base(dir) + "/strata"},
		{"a name on the path", "strata"},
	} {
		t.Run(c.name, func(t *testing.T) {
			t.Chdir(dir)
			t.Setenv("PATH", dir+string(os.PathListSeparator)+os.Getenv("PATH"))
			code, s, _, errs := runOnce(t, onceArgs(t, c.path))
			if code != 0 {
				t.Fatalf(testkit.ExitStderr, code, errs)
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
	kubeAPI := testkit.MarkerPods(t)
	for name, value := range kubeAPI.Env() {
		t.Setenv(name, value)
	}
	return kubeAPI
}

func TestWithoutReadPodsTheObserverMakesNoKubernetesCall(t *testing.T) {
	kubeAPI := podsAPI(t)
	args := append(onceArgs(t, fakeStrata(t, testkit.CompareSame, 0, false)), "--kube-dir", kubeAPI.Dir())
	if code, _, _, errs := runOnce(t, args); code != 0 {
		t.Fatalf(testkit.ExitStderr, code, errs)
	}
	if n := len(kubeAPI.Requests()); n != 0 {
		t.Errorf(kubeRequests, n)
	}
	serve := serveArgs(t, fakeStrata(t, testkit.CompareSame, 0, false))
	addr, stop, _ := startServe(t, append(serve, "--kube-dir", kubeAPI.Dir()))
	// With --read-pods, the content cycle reads the pods. So the count of the requests waits for its end.
	testkit.WaitFor(t, "the first content cycle", func() bool {
		var got api.ClusterView
		return getJSON(addr, "/v1/cluster", &got) && got.Content.CheckedAt != nil
	})
	var doc model.Observed
	if !getJSON(addr, "/v1/images", &doc) || doc.Pods != model.PodsOff || len(doc.Images) != 0 {
		t.Errorf("document %+v", doc)
	}
	stop()
	if n := len(kubeAPI.Requests()); n != 0 {
		t.Errorf(kubeRequests+" with the server running", n)
	}
}

func TestWithReadPodsTheContentCycleReadsThePodsOnceAndNoValueOfAPodLeaves(t *testing.T) {
	kubeAPI := podsAPI(t)
	args := append(onceArgs(t, fakeStrata(t, testkit.CompareSame, 0, false)), "--read-pods", "--kube-dir", kubeAPI.Dir())
	code, _, out, errs := runOnce(t, args)
	if code != 0 {
		t.Fatalf(testkit.ExitStderr, code, errs)
	}
	seen := kubeAPI.Requests()
	if len(seen) != 1 || seen[0].Method != http.MethodGet || seen[0].Path != "/api/v1/pods" || seen[0].Auth != "Bearer token-1" {
		t.Errorf("requests %+v", seen)
	}
	testkit.HoldsNone(t, "the output of a run with --read-pods", out+errs, "MARKER")
}

func TestServeWithReadPodsServesTheDigestsOfTheImages(t *testing.T) {
	kubeAPI := podsAPI(t)
	args := serveArgs(t, fakeStrata(t, testkit.CompareSame, 0, false))
	addr, stop, _ := startServe(t, append(args, "--read-pods", "--kube-dir", kubeAPI.Dir()))
	var doc model.Observed
	testkit.WaitFor(t, "the first read of the pods", func() bool {
		doc = model.Observed{}
		return getJSON(addr, "/v1/images", &doc) && doc.Pods == model.PodsRead
	})
	if doc.Counts.Images != 3 || doc.Images[0].Ref == "" {
		t.Errorf("document %+v", doc)
	}
	stop()
}

func TestReadPodsNeedsTheVariablesOfTheKubelet(t *testing.T) {
	t.Setenv(testkit.KubeHost, "")
	t.Setenv(testkit.KubePort, "")
	for _, command := range []string{"once", "serve"} {
		t.Run(command, func(t *testing.T) {
			args := commandArgs(t, command, "strata")
			exitsTwoSaying(t, append(args, "--read-pods"), testkit.KubeHost)
		})
	}
}

// The seed needs the uid of the cluster, and the variables that the kubelet gives a pod. Outside a pod
// it stops with the kind of the error, and it runs no strata.
func TestSeedNeedsTheClusterAndThePod(t *testing.T) {
	exitsTwoSaying(t, []string{"seed"}, "--cluster-uid is empty")
	t.Setenv(testkit.KubeHost, "")
	exitsTwoSaying(t, []string{"seed", "--cluster-uid", testkit.PaymentsUID, "--namespace", "strata-observer", "--strata", "/no/strata"}, docs.KindKubeConfig)
}
