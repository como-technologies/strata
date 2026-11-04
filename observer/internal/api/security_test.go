package api

import (
	"context"
	"errors"
	"net"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

// The four headers, as the issue states them.
var wantHeaders = map[string]string{
	"Content-Security-Policy": "default-src 'none'; script-src 'self'; style-src 'self'; font-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
	"X-Content-Type-Options":  "nosniff",
	"Referrer-Policy":         "no-referrer",
	"Cache-Control":           "no-store",
}

func TestEachAnswerHasTheFourHeaders(t *testing.T) {
	r := newRig(t)
	h := (&Server{Source: r.cycled(&fakeStrata{dir: r.fixtures}), Page: http.NotFoundHandler()}).Handler()
	for _, c := range []struct {
		name, method, path, host string
		status                   int
	}{
		{"health", "GET", "/healthz", "127.0.0.1:8080", 200},
		{"a summary route", "GET", "/v1/cluster", "127.0.0.1:8080", 200},
		{"the detail route", "GET", pathPublished, "localhost:1", 200},
		{"a profile that does not exist", "GET", "/v1/profiles/nope", "127.0.0.1:8080", 404},
		{"a side that does not exist", "GET", "/v1/packs/aws-stack/k8s/kubernetes/diff?side=x", "127.0.0.1:8080", 400},
		{"the page", "GET", "/", "127.0.0.1:8080", 404},
		{"a post", "POST", "/v1/cluster", "127.0.0.1:8080", 405},
		{"a host that is not loopback", "GET", "/v1/cluster", "evil.example", 421},
		{"a path that the mux redirects", "GET", "/v1//cluster", "127.0.0.1:8080", http.StatusTemporaryRedirect},
	} {
		t.Run(c.name, func(t *testing.T) {
			rec := do(h, c.method, c.path, c.host)
			if rec.Code != c.status {
				t.Fatalf("status %d, want %d", rec.Code, c.status)
			}
			for name, want := range wantHeaders {
				if got := rec.Header().Get(name); got != want {
					t.Errorf("%s is %q, want %q", name, got, want)
				}
			}
		})
	}
}

func TestARequestWithAHostThatIsNotLoopbackGets421(t *testing.T) {
	r := newRig(t)
	h := handlerOf(r.cycled(&fakeStrata{dir: r.fixtures}))
	for _, c := range []struct {
		host string
		want int
	}{
		{"127.0.0.1", 200},
		{"127.0.0.1:8080", 200},
		{"127.0.0.1:54321", 200},
		{"localhost", 200},
		{"localhost:1", 200},
		{"LocalHost:9999", 200},
		{"[::1]:8080", 200},
		{"[::1]", 200},
		{"evil.example", 421},
		{"evil.example:8080", 421},
		{"127.0.0.1.evil.example:8080", 421},
		{"localhost.evil.example", 421},
		{"evil.example@127.0.0.1", 421},
		{"0.0.0.0:8080", 421},
		{"10.0.0.5:8080", 421},
		{"192.168.1.1", 421},
		{"", 421},
	} {
		rec := do(h, "GET", "/v1/cluster", c.host)
		if rec.Code != c.want {
			t.Errorf("Host %q gave %d, want %d", c.host, rec.Code, c.want)
		}
		if c.want == 421 && strings.Contains(rec.Body.String(), "evil") {
			t.Errorf("Host %q: the body echoes the host: %s", c.host, rec.Body.String())
		}
	}
}

func TestOnlyGetIsServed(t *testing.T) {
	r := newRig(t)
	h := handlerOf(r.cycled(&fakeStrata{dir: r.fixtures}))
	for _, method := range []string{"POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "TRACE", "CONNECT"} {
		for _, path := range []string{"/v1/cluster", "/healthz", pathPublished, "/nowhere"} {
			rec := do(h, method, path, "127.0.0.1:8080")
			if rec.Code != http.StatusMethodNotAllowed || rec.Header().Get("Allow") != "GET" {
				t.Errorf("%s %s gave %d, Allow %q", method, path, rec.Code, rec.Header().Get("Allow"))
			}
		}
	}
}

func TestListenTakesOnlyALoopbackAddress(t *testing.T) {
	for _, c := range []struct {
		addr string
		ok   bool
	}{
		{"127.0.0.1:8080", true},
		{"127.0.0.1:0", true},
		{"127.1.2.3:8080", true},
		{"[::1]:8080", true},
		{"0.0.0.0:8080", false},
		{":8080", false},
		{"[::]:8080", false},
		{"localhost:8080", false},
		{"10.0.0.5:8080", false},
		{"192.168.1.1:80", false},
		{"127.0.0.1", false},
		{"127.0.0.1:99999", false},
		{"127.0.0.1:port", false},
		{"", false},
	} {
		err := checkLoopback(c.addr)
		if (err == nil) != c.ok || (err != nil && !errors.Is(err, ErrNotLoopback)) {
			t.Errorf("checkLoopback(%q) = %v, want ok %v", c.addr, err, c.ok)
		}
	}
	// Listen binds nothing for an address that is not loopback.
	if ln, err := Listen("0.0.0.0:0"); err == nil || ln != nil || !errors.Is(err, ErrNotLoopback) {
		t.Errorf("Listen(0.0.0.0:0) = %v, %v", ln, err)
	}
	ln, err := Listen("127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	ln.Close()
}

func TestTheServerSetsEachTimeout(t *testing.T) {
	srv := (&Server{Source: stub{}}).httpServer()
	if srv.ReadHeaderTimeout <= 0 || srv.ReadTimeout <= 0 || srv.WriteTimeout <= 0 || srv.IdleTimeout <= 0 {
		t.Errorf("timeouts %v %v %v %v", srv.ReadHeaderTimeout, srv.ReadTimeout, srv.WriteTimeout, srv.IdleTimeout)
	}
	if srv.WriteTimeout <= detailTimeout {
		t.Errorf("the write timeout %v must be longer than the detail timeout %v", srv.WriteTimeout, detailTimeout)
	}
}

// listener opens a loopback port for a test.
func listener(t *testing.T) net.Listener {
	t.Helper()
	ln, err := Listen("127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	return ln
}

func TestHealthSucceedsWithTheServerUpAndFailsWithNone(t *testing.T) {
	ln := listener(t)
	ctx, stop := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { done <- (&Server{Source: stub{}}).Serve(ctx, ln, nil2()) }()
	if err := Health(context.Background(), ln.Addr().String()); err != nil {
		t.Errorf("the server is up: %v", err)
	}
	stop()
	if err := <-done; err != nil {
		t.Errorf("a stop that ctx asked for gave %v", err)
	}
	if err := Health(context.Background(), ln.Addr().String()); err == nil {
		t.Error("no server runs, and the health check passed")
	}
}

func TestHealthFailsOnAnotherStatusAndDoesNotFollowARedirect(t *testing.T) {
	for _, c := range []struct {
		name    string
		handler http.HandlerFunc
	}{
		{"a server error", func(w http.ResponseWriter, _ *http.Request) { w.WriteHeader(500) }},
		{"a redirect", func(w http.ResponseWriter, r *http.Request) { http.Redirect(w, r, "/elsewhere", http.StatusFound) }},
		{"a not found", http.NotFound},
	} {
		t.Run(c.name, func(t *testing.T) {
			srv := httptest.NewServer(c.handler)
			defer srv.Close()
			if err := Health(context.Background(), strings.TrimPrefix(srv.URL, "http://")); err == nil {
				t.Error("the health check passed")
			}
		})
	}
}

func TestHealthStopsAfterThreeSeconds(t *testing.T) {
	release := make(chan struct{})
	srv := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) { <-release }))
	defer srv.Close()
	defer close(release)
	start := time.Now()
	if err := Health(context.Background(), strings.TrimPrefix(srv.URL, "http://")); err == nil {
		t.Error("the health check passed")
	}
	if took := time.Since(start); took < healthTimeout || took > healthTimeout+2*time.Second {
		t.Errorf("the check took %v", took)
	}
}

func TestHealthAsksOnlyALoopbackAddress(t *testing.T) {
	if err := Health(context.Background(), "10.0.0.5:8080"); !errors.Is(err, ErrNotLoopback) {
		t.Errorf("error %v", err)
	}
}
