// Package testkit holds the helpers that the tests of more than one package of the observer use:
// a script that stands for the strata binary, a gzip file, a cluster read, the documents that the
// commands promise, a wait, and a count of the calls that run at one time. Only test files import it.
package testkit

import (
	"bytes"
	"compress/gzip"
	"context"
	"errors"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
)

// The smallest document that a command of a cycle promises, as one line of JSON.
const (
	// VerifyUnchanged is the answer of `verify` for one record, record-0.json, that did not change.
	VerifyUnchanged = `{"schema":"` + docs.VerifySummarySchema + `","tool":{"version":"0.29.0"},"records":[{"path":"record-0.json","status":"unchanged","name":"aws-stack","version":"1.1.0"}]}`
	// ClusterPayments is the snapshot of the cluster c1, named payments, that runs aws-stack@1.1.0.
	ClusterPayments = `{"schema":"` + docs.SnapshotSchema + `","source":{"cluster":{"uid":"c1","name":"payments","profiles":[{"name":"aws-stack","version":"1.1.0","type":"cluster"}]}}}`
	// CompareSame is the answer of `compare` for two sides that are the same.
	CompareSame = `{"schema":"` + docs.DiffSchema + `","same":true,"sameContent":true}`
)

// preamble starts each script of Strata. It sets cmd to the command, ref to the first argument after
// it, and out to the value of -o.
const preamble = `
cmd=$1
shift
ref=$1
out=
prev=
for a in "$@"; do
  if [ "$prev" = -o ]; then out=$a; fi
  prev=$a
done
`

// Script writes an executable script named strata to a new directory of the test, and gives its path.
// body is the shell code after the line that names the shell.
func Script(t *testing.T, body string) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "strata")
	if err := os.WriteFile(path, []byte("#!/bin/sh\n"+body+"\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	return path
}

// Strata writes a script that stands for the strata binary, and gives its path. body is the shell
// code after the lines that set $cmd to the command, $ref to the first argument after it, and $out to
// the value of -o. The arguments after the command stay in $1 and the next.
func Strata(t *testing.T, body string) string {
	t.Helper()
	return Script(t, preamble+body)
}

// WriteGzip writes body, compressed, to the file name of dir.
func WriteGzip(t *testing.T, dir, name, body string) {
	t.Helper()
	var buf bytes.Buffer
	w := gzip.NewWriter(&buf)
	if _, err := w.Write([]byte(body)); err != nil {
		t.Fatal(err)
	}
	if err := w.Close(); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, name), buf.Bytes(), 0o600); err != nil {
		t.Fatal(err)
	}
}

// Cluster makes the read of the cluster c1 that runs the given profiles, each as name@version. The
// cluster has an update pending.
func Cluster(name string, profiles ...string) docs.ClusterRead {
	var c docs.ClusterRead
	c.Source.Cluster.UID, c.Source.Cluster.Name, c.Source.Cluster.UpdatesPending = "c1", name, true
	for _, p := range profiles {
		n, v, _ := strings.Cut(p, "@")
		c.Source.Cluster.Profiles = append(c.Source.Cluster.Profiles, docs.ClusterProfile{Name: n, Version: v, Type: "cluster"})
	}
	return c
}

// OutFile gives the file that a command writes: the argument after -o.
func OutFile(args []string) (string, error) {
	i := slices.Index(args, "-o")
	if i < 0 || i+1 >= len(args) {
		return "", errors.New("the command has no -o and a file")
	}
	return args[i+1], nil
}

// waitLimit is the longest time that a test waits for an event.
const waitLimit = 10 * time.Second

// WaitFor calls ready every 20 ms until it is true, and fails the test after 10 seconds. what names
// the event for the message of the failure.
func WaitFor(t *testing.T, what string, ready func() bool) {
	t.Helper()
	deadline := time.Now().Add(waitLimit)
	for !ready() {
		if time.Now().After(deadline) {
			t.Fatalf("%s did not happen", what)
		}
		time.Sleep(20 * time.Millisecond)
	}
}

// Overlap counts the calls of a stand-in for strata that run at one time. The stand-in calls Start when
// a call starts, and the function that Start gives when the call ends.
type Overlap struct {
	// Delay makes each call take this time, so that calls that can run together do.
	Delay time.Duration
	// Hold makes each call wait until this many calls ran at one time, for 10 seconds at most. A test
	// that must see calls run together sets it, so that its count does not depend on a delay.
	Hold int

	mu           sync.Mutex
	active, most int
}

// Start counts a call that starts, and waits as Hold and then Delay say. The wait for Hold also ends
// when ctx ends. Start gives the function that counts the end of the call.
func (o *Overlap) Start(ctx context.Context) (end func()) {
	o.mu.Lock()
	o.active++
	o.most = max(o.most, o.active)
	o.mu.Unlock()
	deadline := time.Now().Add(waitLimit)
	for o.Most() < o.Hold && ctx.Err() == nil && time.Now().Before(deadline) {
		time.Sleep(time.Millisecond)
	}
	time.Sleep(o.Delay)
	return func() {
		o.mu.Lock()
		defer o.mu.Unlock()
		o.active--
	}
}

// Most gives the most calls that ran at one time.
func (o *Overlap) Most() int {
	o.mu.Lock()
	defer o.mu.Unlock()
	return o.most
}
