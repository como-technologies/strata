package testkit

import (
	"bytes"
	"context"
	"io"
	"slices"
	"strings"
	"sync"
	"testing"
	"time"
)

// PortZero is the loopback address with port 0. A server that listens on it gets a free port from the
// kernel, so no other process holds that port. No server can listen on port 0 itself, so a request to
// PortZero finds no server.
//
// A test of "no server" asks PortZero, and not a port that a server closed. The kernel can give a
// closed port to another process at once, and a server there can answer.
const PortZero = "127.0.0.1:0"

// Log is a buffer for the log of a server. The goroutine of the server writes it while the test reads
// it, so each method takes a lock.
type Log struct {
	mu sync.Mutex
	b  bytes.Buffer
}

func (l *Log) Write(p []byte) (int, error) {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.b.Write(p)
}

func (l *Log) String() string {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.b.String()
}

// listenAddr gives the address of the line "listening" of a log, and false when the log has no such
// line.
func listenAddr(log string) (string, bool) {
	for _, line := range strings.Split(log, "\n") {
		fields := strings.Fields(line)
		if !slices.Contains(fields, "msg=listening") {
			continue
		}
		for _, f := range fields {
			if addr, ok := strings.CutPrefix(f, "addr="); ok {
				return addr, true
			}
		}
	}
	return "", false
}

// StartServer runs serve until the test stops it. serve gets a context and a log, and gives its exit
// code. It must listen on PortZero, and log the address that it got on the line "listening".
//
// StartServer waits for that line, and gives the address, the function that stops the server and gives
// its exit code, and the log. The server binds its port before it logs the line, so a request after the
// line reaches it and no other process. The test fails when serve exits before the line, and then shows
// the log.
func StartServer(t *testing.T, serve func(ctx context.Context, log io.Writer) int) (addr string, stop func() int, log *Log) {
	t.Helper()
	ctx, cancel := context.WithCancel(context.Background())
	log = &Log{}
	code := make(chan int, 1)
	go func() { code <- serve(ctx, log) }()
	t.Cleanup(func() {
		cancel()
		if t.Failed() {
			t.Logf("the log of the server:\n%s", log.String())
		}
	})
	WaitFor(t, "the line listening of the server", func() bool {
		select {
		case c := <-code:
			t.Fatalf("the server exited %d", c)
		default:
		}
		var ok bool
		addr, ok = listenAddr(log.String())
		return ok
	})
	return addr, func() int {
		cancel()
		select {
		case c := <-code:
			return c
		case <-time.After(WaitLimit):
			t.Fatal("the server did not stop")
			return -1
		}
	}, log
}
