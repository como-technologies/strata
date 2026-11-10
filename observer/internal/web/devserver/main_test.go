package main

import (
	"bytes"
	"context"
	"io"
	"net"
	"net/http"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/testkit"
)

func TestAnUnknownStateExitsTwoAndNamesTheSets(t *testing.T) {
	var errs bytes.Buffer
	if code := run(context.Background(), []string{"-state", "nope"}, &errs); code != 2 || !strings.Contains(errs.String(), "first-seen") {
		t.Errorf("exit %d, stderr %q", code, errs.String())
	}
}

func TestAnAddressThatIsNotLoopbackExitsTwo(t *testing.T) {
	var errs bytes.Buffer
	if code := run(context.Background(), []string{"-data", "../testdata/api", "-listen", "0.0.0.0:0"}, &errs); code != 2 {
		t.Errorf("exit %d, stderr %q", code, errs.String())
	}
}

func TestADirectoryWithNoSetExitsTwo(t *testing.T) {
	var errs bytes.Buffer
	if code := run(context.Background(), []string{"-data", t.TempDir()}, &errs); code != 2 {
		t.Errorf("exit %d, stderr %q", code, errs.String())
	}
}

// The server gives the page and the routes, with the headers of the real server.
func TestTheServerGivesThePageAndTheRoutesOfTheSet(t *testing.T) {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	addr := ln.Addr().String()
	if err := ln.Close(); err != nil {
		t.Fatal(err)
	}
	ctx, stop := context.WithCancel(context.Background())
	done := make(chan int, 1)
	go func() {
		done <- run(ctx, []string{"-data", "../testdata/api", "-state", "lost", "-listen", addr}, io.Discard)
	}()
	var resp *http.Response
	testkit.WaitFor(t, "the start of the server", func() bool {
		resp, err = http.Get("http://" + addr + "/v1/cluster")
		return err == nil
	})
	body, _ := io.ReadAll(resp.Body)
	resp.Body.Close()
	if resp.StatusCode != 200 || !strings.Contains(string(body), `"page":"lost"`) || resp.Header.Get("Content-Security-Policy") == "" {
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
	stop()
	if code := <-done; code != 0 {
		t.Errorf("exit %d after a stop that ctx asked for", code)
	}
}

// serveSet starts the dev server on a free port with one set, and it gives the address. The server stops
// when the test ends.
func serveSet(t *testing.T, state string) string {
	t.Helper()
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	addr := ln.Addr().String()
	if err := ln.Close(); err != nil {
		t.Fatal(err)
	}
	ctx, stop := context.WithCancel(context.Background())
	done := make(chan int, 1)
	go func() {
		done <- run(ctx, []string{"-data", "../testdata/api", "-state", state, "-listen", addr}, io.Discard)
	}()
	t.Cleanup(func() {
		stop()
		<-done
	})
	testkit.WaitFor(t, "the start of the server", func() bool {
		resp, err := http.Get("http://" + addr + "/v1/cluster")
		if err != nil {
			return false
		}
		resp.Body.Close()
		return true
	})
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
		req.Header.Set("Sec-Fetch-Site", "same-origin")
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
func TestAClickOnAMoveGivesTheReplyOfTheSetAndTheListShowsIt(t *testing.T) {
	base := "http://" + serveSet(t, "ok")
	if code, body := send(t, http.MethodGet, base+"/v1/asks", false); code != 200 || body != "{\"asks\":[]}\n" {
		t.Fatalf("the list before the click: %d %s", code, body)
	}
	const dex = "/v1/packs/fred-identity@2.3.0/addon/dex/ask"
	if code, _ := send(t, http.MethodPost, base+dex, false); code != http.StatusForbidden {
		t.Errorf("a POST that the page did not send gave %d", code)
	}
	code, body := send(t, http.MethodPost, base+dex, true)
	if code != 200 || !strings.Contains(body, `"verdict":"no-error"`) || !strings.Contains(body, `"newest":"2.44.0"`) {
		t.Fatalf("the click gave %d %s", code, body)
	}
	if again, bodyAgain := send(t, http.MethodPost, base+dex, true); again != 200 || bodyAgain != body {
		t.Errorf("the second click gave %d %s", again, bodyAgain)
	}
	if code, list := send(t, http.MethodGet, base+"/v1/asks", false); code != 200 || strings.Count(list, `"verdict"`) != 1 || !strings.Contains(list, `"key":"addon/dex"`) {
		t.Errorf("the list after the click: %d %s", code, list)
	}
	// A move with no reply, and a profile that is not there, are not found.
	for _, path := range []string{"/v1/packs/dino-storage@1.1.0/addon/longhorn/ask", "/v1/packs/nobody@1.0.0/addon/dex/ask"} {
		if code, _ := send(t, http.MethodPost, base+path, true); code != http.StatusNotFound {
			t.Errorf("%s gave %d", path, code)
		}
	}
}

// The set asks shows one answer of each verdict from the start, and a click on a move that it holds gives
// the answer again.
func TestTheSetAsksListsOneAnswerOfEachVerdictAtTheStart(t *testing.T) {
	base := "http://" + serveSet(t, "asks")
	code, list := send(t, http.MethodGet, base+"/v1/asks", false)
	if code != 200 {
		t.Fatalf("the list gave %d", code)
	}
	for _, want := range []string{`"verdict":"no-error"`, `"verdict":"refuses"`, `"verdict":"no-answer"`, `"code":"DependencyPackUpgradeNotSupported"`, `"status":403`} {
		if !strings.Contains(list, want) {
			t.Errorf("the list has no %s: %s", want, list)
		}
	}
	code, body := send(t, http.MethodPost, base+"/v1/packs/wilma-gateway@3.1.2/addon/traefik/ask", true)
	if code != 200 || !strings.Contains(body, `"verdict":"refuses"`) {
		t.Errorf("a click on the refused move gave %d %s", code, body)
	}
}
