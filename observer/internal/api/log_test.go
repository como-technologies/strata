package api

import (
	"context"
	"net/http"
	"strings"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/check"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/runner"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// script writes a shell script that stands for the strata binary. It answers from the fixtures. A
// command that fails prints the sentinel to its stderr, in a line of the error and in a plain line.
// A message of Palette can do the same. A marker file makes a command fail: fail-verify,
// fail-behind, or fail-moves. When log is not empty, the script first keeps the command line of each
// call in that file. `behind --pin` prints the fixture pin.
func script(t *testing.T, fixtures, log, pin string) string {
	t.Helper()
	keep := ""
	if log != "" {
		keep = `echo "$cmd $*" >> '` + log + `'` + "\n"
	}
	return testkit.Strata(t, keep+`fail() {
  `+testkit.ErrorLine(docs.KindAPI, `"status":500,"message":"`+sentinelValue+`"`)+`
  echo '`+sentinelValue+`' >&2
  exit 2
}
`+testkit.VerifyFirst+`  if [ -e '`+fixtures+`/fail-verify' ]; then fail; fi
  cat '`+fixtures+`/verify.json'; exit 1 ;;
snapshot) cp '`+fixtures+`/cluster.json' "$out"; exit 0 ;;
`+testkit.PacksFirst+`    if [ -e '`+fixtures+`/fail-moves' ]; then fail; fi
    cat '`+fixtures+`/move.json'; exit 1 ;;
  *) cat '`+fixtures+`/`+compareFixture+`'; exit 1 ;;
  esac ;;
behind)
  if [ -e '`+fixtures+`/fail-behind' ]; then fail; fi
  case "$*" in
  *--pin*) cat '`+fixtures+`/`+pin+`'; exit 1 ;;
  esac
  cat '`+fixtures+`/`+testkit.BehindDoc+`'; exit 1 ;;
esac
exit 3`)
}

func marker(t *testing.T, fixtures, name string) {
	t.Helper()
	testkit.WriteIn(t, fixtures, name, "")
}

// scripted makes the strata of a test from script, with the real runner. log and pin go to script.
func scripted(t *testing.T, r *rig, log, pin string) check.Strata {
	t.Helper()
	return check.InDir{Runner: runner.Runner{Binary: script(t, r.fixtures, log, pin), Timeout: time.Minute}}
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
		o := r.observer(scripted(t, r, "", testkit.BehindDoc))
		if _, err := o.Once(context.Background()); err == nil {
			t.Fatal("the versions cycle did not fail")
		}
		requestEverything(t, handlerOf(o))
		assertLog(t, r)
	})
	t.Run("a compare for a request that fails after a restart", func(t *testing.T) {
		r := newRig(t)
		r.cycled(scripted(t, r, "", testkit.BehindDoc))
		marker(t, r.fixtures, "fail-moves")
		o := r.cycled(scripted(t, r, "", testkit.BehindDoc))
		h := handlerOf(o)
		rec := get(h, pathNewer)
		if rec.Code != http.StatusBadGateway {
			t.Fatalf("the failed compare gave %d: %s", rec.Code, rec.Body.String())
		}
		testkit.HoldsNone(t, "the body", rec.Body.String(), "SENTINEL")
		requestEverything(t, h)
		assertLog(t, r)
	})
	t.Run("a content cycle that fails", func(t *testing.T) {
		r := newRig(t)
		r.cycled(scripted(t, r, "", testkit.BehindDoc))
		marker(t, r.fixtures, "fail-verify")
		o := r.observer(scripted(t, r, "", testkit.BehindDoc))
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
	testkit.HoldsAll(t, "the log", log, "command=", "verdict=")
	testkit.HoldsNone(t, "the log", log, "SENTINEL", sentinelValue, sentinelVariable, "Betty")
}
