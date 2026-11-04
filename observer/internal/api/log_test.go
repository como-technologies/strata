package api

import (
	"context"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/check"
	"github.com/como-technologies/strata/observer/internal/runner"
)

// script writes a shell script that stands for the strata binary. It answers from the fixtures. A
// command that fails prints the sentinel to its stderr, in a line of the error and in a plain line.
// A message of Palette can do the same. A marker file makes a command fail: fail-verify,
// fail-behind, or fail-moves.
func script(t *testing.T, fixtures string) string {
	t.Helper()
	body := `#!/bin/sh
cmd=$1
shift
out=
prev=
for a in "$@"; do
  if [ "$prev" = -o ]; then out=$a; fi
  prev=$a
done
fail() {
  echo '{"error":{"kind":"api","status":500,"message":"` + sentinelValue + `"}}' >&2
  echo '` + sentinelValue + `' >&2
  exit 2
}
case $cmd in
verify)
  if [ -e '` + fixtures + `/fail-verify' ]; then fail; fi
  cat '` + fixtures + `/verify.json'; exit 1 ;;
snapshot) cp '` + fixtures + `/cluster.json' "$out"; exit 0 ;;
compare)
  case $1 in
  pack:*)
    if [ -e '` + fixtures + `/fail-moves' ]; then fail; fi
    cat '` + fixtures + `/move.json'; exit 1 ;;
  *) cat '` + fixtures + `/compare-cluster.json'; exit 1 ;;
  esac ;;
behind)
  if [ -e '` + fixtures + `/fail-behind' ]; then fail; fi
  cat '` + fixtures + `/behind.json'; exit 1 ;;
esac
exit 3
`
	path := filepath.Join(t.TempDir(), "strata")
	if err := os.WriteFile(path, []byte(body), 0o755); err != nil {
		t.Fatal(err)
	}
	return path
}

func marker(t *testing.T, fixtures, name string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(fixtures, name), nil, 0o600); err != nil {
		t.Fatal(err)
	}
}

// scripted makes the strata of a test from the script, with the real runner.
func scripted(t *testing.T, r *rig) check.Strata {
	t.Helper()
	return check.InDir{Runner: runner.Runner{Binary: script(t, r.fixtures), Timeout: time.Minute}}
}

// requestEverything calls each route, so that the log holds the lines of a full run.
func requestEverything(t *testing.T, h http.Handler) string {
	t.Helper()
	var bodies strings.Builder
	for _, p := range summaryRoutes {
		bodies.WriteString(get(h, p).Body.String())
	}
	for _, ps := range detailRoutes {
		for _, p := range ps {
			bodies.WriteString(get(h, p).Body.String())
		}
	}
	return bodies.String()
}

// The log test: a full cycle with a runner that fails, with the sentinel in its stderr and in each
// Leaf, must leave no sentinel in the log.
func TestTheLogHoldsNoSentinelThroughAFullCycleThatFails(t *testing.T) {
	t.Run("a versions cycle that fails", func(t *testing.T) {
		r := newRig(t)
		marker(t, r.fixtures, "fail-behind")
		o := r.observer(scripted(t, r))
		if _, err := o.Once(context.Background()); err == nil {
			t.Fatal("the versions cycle did not fail")
		}
		requestEverything(t, handlerOf(o))
		assertLog(t, r)
	})
	t.Run("a compare for a request that fails after a restart", func(t *testing.T) {
		r := newRig(t)
		r.cycled(scripted(t, r))
		marker(t, r.fixtures, "fail-moves")
		o := r.cycled(scripted(t, r))
		h := handlerOf(o)
		rec := get(h, pathNewer)
		if rec.Code != http.StatusBadGateway {
			t.Fatalf("the failed compare gave %d: %s", rec.Code, rec.Body.String())
		}
		if strings.Contains(rec.Body.String(), "SENTINEL") {
			t.Errorf("the body of the failure holds the sentinel: %s", rec.Body.String())
		}
		requestEverything(t, h)
		assertLog(t, r)
	})
	t.Run("a content cycle that fails", func(t *testing.T) {
		r := newRig(t)
		r.cycled(scripted(t, r))
		marker(t, r.fixtures, "fail-verify")
		o := r.observer(scripted(t, r))
		if err := o.RunContent(context.Background()); err == nil {
			t.Fatal("the content cycle did not fail")
		}
		requestEverything(t, handlerOf(o))
		assertLog(t, r)
	})
}

func assertLog(t *testing.T, r *rig) {
	t.Helper()
	log := r.log.String()
	if !strings.Contains(log, "command=") || !strings.Contains(log, "verdict=") {
		t.Fatalf("the log is empty or not what the test expects:\n%s", log)
	}
	for _, s := range []string{"SENTINEL", sentinelValue, sentinelVariable, "Betty"} {
		if strings.Contains(log, s) {
			t.Errorf("the log holds %q:\n%s", s, log)
		}
	}
}
