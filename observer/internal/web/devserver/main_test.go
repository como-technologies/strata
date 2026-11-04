package main

import (
	"bytes"
	"context"
	"io"
	"net"
	"net/http"
	"strings"
	"testing"
	"time"
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
	for i := 0; i < 100; i++ {
		resp, err = http.Get("http://" + addr + "/v1/cluster")
		if err == nil {
			break
		}
		time.Sleep(20 * time.Millisecond)
	}
	if err != nil {
		t.Fatal(err)
	}
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
