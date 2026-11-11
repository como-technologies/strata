package testkit

import (
	"io"
	"net/http"
	"net/http/httptest"
	"slices"
	"sync"
	"testing"
)

// Received is one request that a Receiver got.
type Received struct {
	Method, ContentType, Body string
}

// Receiver is a webhook for a test. It keeps each request that it gets, and it answers each one with its
// status, which starts as 200.
type Receiver struct {
	*httptest.Server

	mu     sync.Mutex
	status int
	got    []Received
}

// NewReceiver starts a receiver on HTTP. It closes at the end of the test.
func NewReceiver(t *testing.T) *Receiver {
	t.Helper()
	return newReceiver(t, httptest.NewServer)
}

// NewTLSReceiver starts a receiver on HTTPS, with a certificate of its own. The client of its Server trusts
// the certificate. It closes at the end of the test.
func NewTLSReceiver(t *testing.T) *Receiver {
	t.Helper()
	return newReceiver(t, httptest.NewTLSServer)
}

func newReceiver(t *testing.T, start func(http.Handler) *httptest.Server) *Receiver {
	t.Helper()
	r := &Receiver{status: http.StatusOK}
	r.Server = start(http.HandlerFunc(func(w http.ResponseWriter, req *http.Request) {
		raw, _ := io.ReadAll(req.Body)
		r.mu.Lock()
		r.got = append(r.got, Received{req.Method, req.Header.Get("Content-Type"), string(raw)})
		status := r.status
		r.mu.Unlock()
		w.WriteHeader(status)
	}))
	t.Cleanup(r.Close)
	return r
}

// SetStatus sets the status of each next answer.
func (r *Receiver) SetStatus(status int) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.status = status
}

// Got gives each request so far, in the order of arrival.
func (r *Receiver) Got() []Received {
	r.mu.Lock()
	defer r.mu.Unlock()
	return slices.Clone(r.got)
}

// Bodies gives the body of each request so far, in the order of arrival.
func (r *Receiver) Bodies() []string {
	r.mu.Lock()
	defer r.mu.Unlock()
	bodies := make([]string, len(r.got))
	for i, g := range r.got {
		bodies[i] = g.Body
	}
	return bodies
}

// Count gives the number of requests so far.
func (r *Receiver) Count() int {
	r.mu.Lock()
	defer r.mu.Unlock()
	return len(r.got)
}
