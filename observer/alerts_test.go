package main

import (
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
	return rulesFileAt(t, w.MarkedAddress(), testkit.DriftNow, testkit.LostNow)
}

// rulesFileAt writes the file alerts.json of rules with one channel, oncall, whose file of the address holds address,
// and the rules. It gives the path of alerts.json.
func rulesFileAt(t *testing.T, address string, rules ...string) string {
	t.Helper()
	file := testkit.OncallFile(t, address)
	return testkit.WriteIn(t, filepath.Dir(file), "alerts.json", testkit.OncallRules(file, rules...))
}

// drifting is the answer of `compare` for a record whose pack kubernetes changed.
const drifting = `{"schema":"` + docs.DiffSchema + `","same":false,"sameContent":false,"rootB":"x","packs":[{"key":"` + testkit.KubernetesKey + `","status":"changed"}]}`

func TestOnceSendsNothingAlsoWithTheFlagAlerts(t *testing.T) {
	w := testkit.NewReceiver(t)
	args := append(onceArgs(t, fakeStrata(t, drifting, 0, false)), "--alerts", rulesFile(t, w), "--alerts-allow-http")
	if code, _, _, errs := runOnce(t, args); code != 1 {
		t.Fatalf(testkit.ExitStderr, code, errs)
	}
	if w.Count() != 0 {
		t.Errorf("once sent %d messages", w.Count())
	}
}

func TestOnceAndServeStopAtTheStartWithAFileOfRulesThatCannotBeUsed(t *testing.T) {
	for _, c := range []testkit.BadRule{testkit.NoEndDate, testkit.NewerNow} {
		bad := rulesFileAt(t, "https://"+testkit.HooksExample, c.Rule)
		for _, command := range []string{"once", "serve"} {
			t.Run(c.Name+", "+command, func(t *testing.T) {
				args := append(commandArgs(t, command, "strata"), "--alerts", bad)
				exitsTwoSaying(t, args, "--alerts", "alerts.json", "line 4", "rule 1", c.Want)
			})
		}
	}
}

func TestAFileOfRulesThatIsAbsentStopsTheStart(t *testing.T) {
	exitsTwoSaying(t, append(onceArgs(t, "strata"), "--alerts", filepath.Join(t.TempDir(), "absent.json")), "cannot be read")
}

func TestServeStopsAtTheStartWhenTheAddressOfAChannelCannotBeUsedAndNamesNoAddress(t *testing.T) {
	rules := rulesFileAt(t, "http://"+testkit.HooksExample+testkit.AddressMarker)
	stderr := exitsTwoSaying(t, append(serveArgs(t, "strata"), "--alerts", rules), `channel "oncall"`, "https")
	testkit.HoldsNone(t, "the error", stderr, testkit.AddressMarker)
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
		return getJSON(addr, api.PatternAlerts, &view) && len(view.Rules) == 2 && view.Rules[0].State == alert.StateFiring
	})
	// The observer sets the state of the rule before it sends the message. So the state does not show
	// the send, and the count does.
	testkit.WaitFor(t, "the message of the drift", func() bool { return w.Count() > 0 })
	stop()
	if got := w.Bodies(); len(got) != 1 || !strings.Contains(got[0], "Drift") || !strings.Contains(got[0], testkit.PaymentsName) {
		t.Errorf("the receiver got %d messages: %q", len(got), got)
	}
	testkit.HoldsNone(t, "stderr", stderr.String(), testkit.AddressMarker)
	testkit.HoldsAll(t, "stderr", stderr.String(), "channel=oncall")
}

func TestServeWithoutAlertsSendsNothingAndTheRouteSaysSo(t *testing.T) {
	args := serveArgs(t, fakeStrata(t, drifting, 0, false))
	addr, stop, stderr := startServe(t, args)
	defer stop()
	testkit.WaitFor(t, "the finding of the drift", func() bool {
		var got api.FindingsView
		return getJSON(addr, api.PatternFindings, &got) && len(got.Findings) == 1
	})
	var view alert.View
	if !getJSON(addr, api.PatternAlerts, &view) || view.Set || len(view.Rules) != 0 {
		t.Errorf("view %+v", view)
	}
	testkit.HoldsNone(t, "stderr", stderr.String(), "msg=alert")
}
