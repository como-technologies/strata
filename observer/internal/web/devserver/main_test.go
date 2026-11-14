package main

import (
	"bytes"
	"context"
	"io"
	"net/http"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/testkit"
)

// exitsTwo runs the server with args, and fails when it does not exit 2 with each of the words on stderr.
func exitsTwo(t *testing.T, args []string, words ...string) {
	t.Helper()
	var errs bytes.Buffer
	code := run(context.Background(), args, &errs)
	testkit.ExitsTwo(t, code, "", errs.String(), words...)
}

func TestAnUnknownStateExitsTwoAndNamesTheSets(t *testing.T) {
	exitsTwo(t, []string{"-state", "nope"}, "first-seen")
}

func TestAnAddressThatIsNotLoopbackExitsTwo(t *testing.T) {
	exitsTwo(t, []string{"-data", "../testdata/api", "-listen", "0.0.0.0:0"})
}

func TestADirectoryWithNoSetExitsTwo(t *testing.T) {
	exitsTwo(t, []string{"-data", t.TempDir()})
}

// The server gives the page and the routes, with the headers of the real server.
func TestTheServerGivesThePageAndTheRoutesOfTheSet(t *testing.T) {
	addr, stop := startSet(t, "lost")
	resp, err := http.Get("http://" + addr + "/v1/cluster")
	if err != nil {
		t.Fatal(err)
	}
	body, _ := io.ReadAll(resp.Body)
	resp.Body.Close()
	if resp.StatusCode != 200 || !strings.Contains(string(body), `"page":"lost"`) || resp.Header.Get(testkit.HeaderPolicy) == "" {
		t.Errorf("status %d, headers %v, body %s", resp.StatusCode, resp.Header, body)
	}
	page, err := http.Get("http://" + addr + "/")
	if err != nil {
		t.Fatal(err)
	}
	html, _ := io.ReadAll(page.Body)
	page.Body.Close()
	if page.StatusCode != 200 || !strings.HasPrefix(page.Header.Get("Content-Type"), "text/html") || !strings.Contains(string(html), "app.js") {
		t.Errorf("the page: status %d, type %q", page.StatusCode, page.Header.Get("Content-Type"))
	}
	if code := stop(); code != 0 {
		t.Errorf("exit %d after a stop that ctx asked for", code)
	}
}

// startSet starts the dev server on a free port with one set. It gives the address, and the function that
// stops the server and gives its exit code.
func startSet(t *testing.T, state string) (addr string, stop func() int) {
	t.Helper()
	addr, stop, _ = testkit.StartServer(t, func(ctx context.Context, log io.Writer) int {
		return run(ctx, []string{"-data", "../testdata/api", "-state", state, "-listen", testkit.PortZero}, log)
	})
	return addr, stop
}

// serveSet starts the dev server on a free port with one set, and it gives the address. The server stops
// when the test ends.
func serveSet(t *testing.T, state string) string {
	t.Helper()
	addr, stop := startSet(t, state)
	t.Cleanup(func() { stop() })
	return addr
}

// send sends a request to the server and gives the status and the body. A request from the page of the
// server has the header Sec-Fetch-Site: a browser sets it, and a test sets it here.
func send(t *testing.T, method, url string, fromPage bool) (int, string) {
	t.Helper()
	req, err := http.NewRequest(method, url, nil)
	if err != nil {
		t.Fatal(err)
	}
	if fromPage {
		req.Header.Set(testkit.FetchSite, testkit.SameOrigin)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(resp.Body)
	return resp.StatusCode, string(body)
}

// A person can click: a POST for a move gives the reply of the set, and the list of answers shows it after.
// testkit.AskFlow makes the checks.
func TestAClickOnAMoveGivesTheReplyOfTheSetAndTheListShowsIt(t *testing.T) {
	base := "http://" + serveSet(t, "ok")
	testkit.AskFlow(t, func(path string) (int, string) { return send(t, http.MethodGet, base+path, false) },
		func(path string, page bool) (int, string) { return send(t, http.MethodPost, base+path, page) })
}

// The set asks shows one answer of each verdict from the start, and a click on a move that it holds gives
// the answer again.
func TestTheSetAsksListsOneAnswerOfEachVerdictAtTheStart(t *testing.T) {
	base := "http://" + serveSet(t, "asks")
	code, list := send(t, http.MethodGet, base+testkit.AsksRoute, false)
	if code != 200 {
		t.Fatalf("the list gave %d", code)
	}
	for _, want := range []string{testkit.NoErrorVerdict, testkit.RefusesVerdict, testkit.NoAnswerVerdict, `"code":"` + testkit.CodeDependency + `"`, `"status":403`} {
		if !strings.Contains(list, want) {
			t.Errorf("the list has no %s: %s", want, list)
		}
	}
	code, body := send(t, http.MethodPost, base+"/v1/packs/wilma-gateway@3.1.2/addon/traefik/ask", true)
	if code != 200 || !strings.Contains(body, testkit.RefusesVerdict) {
		t.Errorf("a click on the refused move gave %d %s", code, body)
	}
}
