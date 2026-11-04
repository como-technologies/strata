package main

import (
	"bytes"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// palette answers each request with the host and the key it got, so that a test sees what the
// program forwards.
func palette(t *testing.T) *httptest.Server {
	t.Helper()
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("X-Seen-Host", r.Host)
		w.Header().Set("X-Seen-Key", r.Header.Get("ApiKey"))
		w.WriteHeader(http.StatusTeapot)
	}))
	t.Cleanup(srv.Close)
	return srv
}

func TestRunCountsEachRequestAndLogsNeitherQueryNorKey(t *testing.T) {
	srv := palette(t)
	logPath := filepath.Join(t.TempDir(), "requests.log")
	var out, errs bytes.Buffer
	// The command sends three requests to PALETTE_API, two of them with a query.
	script := `for p in /v1/a /v1/b?name=secret-name /v1/a?x=1; do curl -s -o /dev/null -H 'ApiKey: secret-key' "$PALETTE_API$p"; done; curl -s -D - -o /dev/null -H 'ApiKey: secret-key' "$PALETTE_API/v1/c"`
	code := run([]string{"-target", srv.URL, "-log", logPath, "--", "bash", "-c", script}, strings.NewReader(""), &out, &errs)
	if code != 0 {
		t.Fatalf("exit %d: %s", code, errs.String())
	}
	got, err := os.ReadFile(logPath)
	if err != nil {
		t.Fatal(err)
	}
	if want := "GET /v1/a\nGET /v1/b\nGET /v1/a\nGET /v1/c\n"; string(got) != want {
		t.Errorf("log %q, want %q", got, want)
	}
	for _, secret := range []string{"secret-key", "secret-name"} {
		if strings.Contains(string(got), secret) {
			t.Errorf("the log holds %q", secret)
		}
	}
	// The proxy sends the key on, and it gives Palette the Host of Palette.
	if !strings.Contains(out.String(), "X-Seen-Key: secret-key") || !strings.Contains(out.String(), "X-Seen-Host: "+strings.TrimPrefix(srv.URL, "http://")) {
		t.Errorf("the request did not reach Palette as sent: %s", out.String())
	}
}

func TestRunExitsWithTheCodeOfTheCommandAndRefusesBadArguments(t *testing.T) {
	srv := palette(t)
	dir := t.TempDir()
	cases := []struct {
		name string
		args []string
		want int
	}{
		{"the code of the command", []string{"-target", srv.URL, "-log", filepath.Join(dir, "a"), "--", "bash", "-c", "exit 7"}, 7},
		{"no command", []string{"-target", srv.URL, "-log", filepath.Join(dir, "b")}, 2},
		{"no log", []string{"-target", srv.URL, "--", "true"}, 2},
		{"a target that is no address", []string{"-target", "ftp://x", "-log", filepath.Join(dir, "c"), "--", "true"}, 2},
		{"a command that does not exist", []string{"-target", srv.URL, "-log", filepath.Join(dir, "d"), "--", "no-such-command-here"}, 2},
	}
	for _, c := range cases {
		var out, errs bytes.Buffer
		if got := run(c.args, strings.NewReader(""), &out, &errs); got != c.want {
			t.Errorf("%s: exit %d, want %d (%s)", c.name, got, c.want, errs.String())
		}
	}
}
