package api

import (
	"context"
	"errors"
	"log/slog"
	"net"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/testkit"
)

// The four headers, as the issue states them.
var wantHeaders = map[string]string{
	testkit.HeaderPolicy:  "default-src 'none'; script-src 'self'; style-src 'self'; font-src 'self'; img-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
	testkit.HeaderNoSniff: "nosniff",
	"Referrer-Policy":     "no-referrer",
	"Cache-Control":       "no-store",
}

// refusedWithAllow fails when the answer to the request is not a 405 whose header Allow names the method allow.
func refusedWithAllow(t *testing.T, rec *httptest.ResponseRecorder, request, allow string) {
	t.Helper()
	if rec.Code != http.StatusMethodNotAllowed || rec.Header().Get("Allow") != allow {
		t.Errorf("%s gave %d, Allow %q", request, rec.Code, rec.Header().Get("Allow"))
	}
}

// healthFails fails when the health check of the address passes.
func healthFails(t *testing.T, addr string) {
	t.Helper()
	if err := Health(context.Background(), addr); err == nil {
		t.Errorf("the health check of %s passed", addr)
	}
}

func TestEachAnswerHasTheFourHeaders(t *testing.T) {
	_, o, _ := asked(t, "")
	h := (&Server{Source: o, Page: http.NotFoundHandler()}).Handler()
	for _, c := range []struct {
		name, method, path, host string
		status                   int
	}{
		{"health", "GET", patternHealth, testkit.LoopbackHost, 200},
		{"a summary route", "GET", summaryRoutes[PatternCluster], testkit.LoopbackHost, 200},
		{"the detail route", "GET", pathPublished, "localhost:1", 200},
		{"a profile that does not exist", "GET", PatternProfiles + "/nope", testkit.LoopbackHost, 404},
		{noSuchSide, "GET", kubernetesDiff("x"), testkit.LoopbackHost, 400},
		{"the page", "GET", "/", testkit.LoopbackHost, 404},
		{"a post", "POST", summaryRoutes[PatternCluster], testkit.LoopbackHost, 405},
		{"a host that is not loopback", "GET", summaryRoutes[PatternCluster], "evil.example", 421},
		{"a path that the mux redirects", "GET", "/v1//cluster", testkit.LoopbackHost, http.StatusTemporaryRedirect},
	} {
		t.Run(c.name, func(t *testing.T) {
			rec := testkit.Serve(h, c.method, c.path, c.host, "")
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
	_, o, _ := asked(t, "")
	h := handlerOf(o)
	for _, c := range []struct {
		host string
		want int
	}{
		{"127.0.0.1", 200},
		{testkit.LoopbackHost, 200},
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
		rec := testkit.Serve(h, http.MethodGet, summaryRoutes[PatternCluster], c.host, "")
		if rec.Code != c.want {
			t.Errorf("Host %q gave %d, want %d", c.host, rec.Code, c.want)
		}
		if c.want == 421 && strings.Contains(rec.Body.String(), "evil") {
			t.Errorf("Host %q: the body echoes the host: %s", c.host, rec.Body.String())
		}
	}
}

func TestOnlyGetIsServed(t *testing.T) {
	_, o, _ := asked(t, "")
	h := handlerOf(o)
	for _, method := range []string{"POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "TRACE", "CONNECT"} {
		for _, path := range []string{summaryRoutes[PatternCluster], patternHealth, pathPublished, "/nowhere"} {
			rec := testkit.Serve(h, method, path, testkit.LoopbackHost, "")
			refusedWithAllow(t, rec, method+" "+path, http.MethodGet)
		}
	}
}

func TestListenTakesOnlyALoopbackAddress(t *testing.T) {
	for _, c := range []struct {
		addr string
		ok   bool
	}{
		{DefaultListen, true},
		{testkit.PortZero, true},
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
	listener(t).Close()
}

func TestTheServerSetsEachTimeout(t *testing.T) {
	srv := (&Server{Source: stub{}}).httpServer(context.Background())
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
	ln, err := Listen(testkit.PortZero)
	if err != nil {
		t.Fatal(err)
	}
	return ln
}

func TestHealthSucceedsWithTheServerUpAndFailsWithNone(t *testing.T) {
	ln := listener(t)
	ctx, stop := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { done <- (&Server{Source: stub{}}).Serve(ctx, ln, slog.New(slog.DiscardHandler)) }()
	if err := Health(context.Background(), ln.Addr().String()); err != nil {
		t.Errorf("the server is up: %v", err)
	}
	stop()
	if err := <-done; err != nil {
		t.Errorf("a stop that ctx asked for gave %v", err)
	}
	// The check asks testkit.PortZero, because another process can take the port of the stopped server.
	healthFails(t, testkit.PortZero)
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
			healthFails(t, strings.TrimPrefix(srv.URL, "http://"))
		})
	}
}

func TestHealthStopsAfterThreeSeconds(t *testing.T) {
	release := make(chan struct{})
	srv := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) { <-release }))
	defer srv.Close()
	defer close(release)
	start := time.Now()
	healthFails(t, strings.TrimPrefix(srv.URL, "http://"))
	if took := time.Since(start); took < healthTimeout || took > healthTimeout+2*time.Second {
		t.Errorf("the check took %v", took)
	}
}

func TestHealthAsksOnlyALoopbackAddress(t *testing.T) {
	if err := Health(context.Background(), "10.0.0.5:8080"); !errors.Is(err, ErrNotLoopback) {
		t.Errorf("error %v", err)
	}
}
