package main

import (
	"bytes"
	"path/filepath"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/alert"
	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// rulesFile writes a file of rules with one channel, oncall, whose address is that of the receiver, and
// the rules for drift and for the loss of contact.
func rulesFile(t *testing.T, w *testkit.Receiver) string {
	t.Helper()
	dir := t.TempDir()
	address := filepath.Join(dir, "oncall")
	testkit.WriteFile(t, address, w.URL+"/TOKEN-SECRET\n")
	path := filepath.Join(dir, "alerts.json")
	rules := testkit.OncallRules(address, testkit.DriftNow, testkit.LostNow)
	testkit.WriteFile(t, path, rules)
	return path
}

// drifting is the answer of `compare` for a record whose pack kubernetes changed.
const drifting = `{"schema":"` + docs.DiffSchema + `","same":false,"sameContent":false,"rootB":"x","packs":[{"key":"k8s/kubernetes","status":"changed"}]}`

func TestOnceSendsNothingAlsoWithTheFlagAlerts(t *testing.T) {
	w := testkit.NewReceiver(t)
	var out, errs bytes.Buffer
	args := append(onceArgs(t, fakeStrata(t, drifting, 0, false)), "--alerts", rulesFile(t, w), "--alerts-allow-http")
	if code := run(args, &out, &errs); code != 1 {
		t.Fatalf(testkit.ExitStderr, code, errs.String())
	}
	if w.Count() != 0 {
		t.Errorf("once sent %d messages", w.Count())
	}
}

func TestOnceAndServeStopAtTheStartWithAFileOfRulesThatCannotBeUsed(t *testing.T) {
	for _, c := range []testkit.BadRule{testkit.NoEndDate, testkit.NewerNow} {
		bad := filepath.Join(t.TempDir(), "alerts.json")
		file := testkit.OncallRules("f", c.Rule)
		testkit.WriteFile(t, bad, file)
		for _, command := range []string{"once", "serve"} {
			t.Run(c.Name+", "+command, func(t *testing.T) {
				args := append(onceArgs(t, "strata"), "--alerts", bad)
				args[0] = command
				exitsTwoSaying(t, args, "--alerts", "alerts.json", "line 4", "rule 1", c.Want)
			})
		}
	}
}

func TestAFileOfRulesThatIsAbsentStopsTheStart(t *testing.T) {
	exitsTwoSaying(t, append(onceArgs(t, "strata"), "--alerts", filepath.Join(t.TempDir(), "absent.json")), "cannot be read")
}

func TestServeStopsAtTheStartWhenTheAddressOfAChannelCannotBeUsedAndNamesNoAddress(t *testing.T) {
	dir := t.TempDir()
	testkit.WriteFile(t, filepath.Join(dir, "oncall"), "http://hooks.example/TOKEN-SECRET")
	rules := filepath.Join(dir, "alerts.json")
	testkit.WriteFile(t, rules, testkit.OncallRules(filepath.Join(dir, "oncall")))
	if stderr := exitsTwoSaying(t, append(serveArgs(t, "strata"), "--alerts", rules), `channel "oncall"`, "https"); strings.Contains(stderr, "TOKEN-SECRET") {
		t.Errorf("the error names the address: %s", stderr)
	}
}

func TestADigestPeriodThatIsNotPositiveStopsTheStart(t *testing.T) {
	exitsTwoSaying(t, append(onceArgs(t, "strata"), "--digest-every", "0s"), "more than zero")
}

func TestServeWithAlertsSendsOneMessageForAFindingAndShowsItsRuleAsFiring(t *testing.T) {
	w := testkit.NewReceiver(t)
	args := serveArgs(t, fakeStrata(t, drifting, 0, false))
	args = append(args, "--alerts", rulesFile(t, w), "--alerts-allow-http")
	addr, stop, stderr := startServe(t, args)
	var view alert.View
	testkit.WaitFor(t, "the rule of drift fires", func() bool {
		return getJSON(addr, "/v1/alerts", &view) && len(view.Rules) == 2 && view.Rules[0].State == alert.StateFiring
	})
	// The observer sets the state of the rule before it sends the message. So the state does not show
	// the send, and the count does.
	testkit.WaitFor(t, "the message of the drift", func() bool { return w.Count() > 0 })
	stop()
	if got := w.Bodies(); len(got) != 1 || !strings.Contains(got[0], "Drift") || !strings.Contains(got[0], testkit.PaymentsName) {
		t.Errorf("the receiver got %d messages: %q", len(got), got)
	}
	if strings.Contains(stderr.String(), "TOKEN-SECRET") || !strings.Contains(stderr.String(), "channel=oncall") {
		t.Errorf("stderr %s", stderr.String())
	}
}

func TestServeWithoutAlertsSendsNothingAndTheRouteSaysSo(t *testing.T) {
	args := serveArgs(t, fakeStrata(t, drifting, 0, false))
	addr, stop, stderr := startServe(t, args)
	defer stop()
	testkit.WaitFor(t, "the finding of the drift", func() bool {
		var got api.FindingsView
		return getJSON(addr, "/v1/findings", &got) && len(got.Findings) == 1
	})
	var view alert.View
	if !getJSON(addr, "/v1/alerts", &view) || view.Set || len(view.Rules) != 0 {
		t.Errorf("view %+v", view)
	}
	if strings.Contains(stderr.String(), "msg=alert") {
		t.Errorf("stderr %s", stderr.String())
	}
}
