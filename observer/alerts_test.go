package main

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/alert"
	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// rulesFile writes a file of rules with one channel, oncall, whose address is that of the receiver, and
// the rules for drift and for the loss of contact.
func rulesFile(t *testing.T, w *testkit.Receiver) string {
	t.Helper()
	dir := t.TempDir()
	address := filepath.Join(dir, "oncall")
	if err := os.WriteFile(address, []byte(w.URL+"/TOKEN-SECRET\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(dir, "alerts.json")
	rules := `{"channels": {"oncall": {"urlFile": "` + address + `"}}, "rules": [
  {"level": "cluster", "news": "drift", "send": "now", "to": "oncall"},
  {"level": "cluster", "news": "lost", "send": "now", "to": "oncall"}]}`
	if err := os.WriteFile(path, []byte(rules), 0o600); err != nil {
		t.Fatal(err)
	}
	return path
}

const drifting = `{"same":false,"sameContent":false,"rootB":"x","packs":[{"key":"k8s/kubernetes","status":"changed"}]}`

func TestOnceSendsNothingAlsoWithTheFlagAlerts(t *testing.T) {
	w := testkit.NewReceiver(t)
	var out, errs bytes.Buffer
	args := append(onceArgs(t, fakeStrata(t, drifting, 0, false)), "--alerts", rulesFile(t, w), "--alerts-allow-http")
	if code := run(args, &out, &errs); code != 1 {
		t.Fatalf("exit %d, stderr %q", code, errs.String())
	}
	if w.Count() != 0 {
		t.Errorf("once sent %d messages", w.Count())
	}
}

func TestOnceAndServeStopAtTheStartWithAFileOfRulesThatCannotBeUsed(t *testing.T) {
	cases := []struct{ name, rule, want string }{
		{"a mute with no end date", `{"level": "profile", "match": "barney-base", "news": "newer"}`, "end date"},
		{"a newer version that pages a person", `{"level": "cluster", "news": "newer", "send": "now", "to": "oncall"}`, "never pages"},
	}
	for _, c := range cases {
		bad := filepath.Join(t.TempDir(), "alerts.json")
		file := "{\n\"channels\": {\"oncall\": {\"urlFile\": \"f\"}},\n\"rules\": [\n" + c.rule + "\n]\n}"
		if err := os.WriteFile(bad, []byte(file), 0o600); err != nil {
			t.Fatal(err)
		}
		for _, command := range []string{"once", "serve"} {
			var out, errs bytes.Buffer
			args := append(onceArgs(t, "strata"), "--alerts", bad)
			args[0] = command
			if code := run(args, &out, &errs); code != 2 || out.Len() != 0 {
				t.Fatalf("%s, %s: exit %d, stdout %q", c.name, command, code, out.String())
			}
			for _, want := range []string{"--alerts", "alerts.json", "line 4", "rule 1", c.want} {
				if !strings.Contains(errs.String(), want) {
					t.Errorf("%s, %s: stderr lacks %q: %s", c.name, command, want, errs.String())
				}
			}
		}
	}
}

func TestAFileOfRulesThatIsAbsentStopsTheStart(t *testing.T) {
	var out, errs bytes.Buffer
	args := append(onceArgs(t, "strata"), "--alerts", filepath.Join(t.TempDir(), "absent.json"))
	if code := run(args, &out, &errs); code != 2 || !strings.Contains(errs.String(), "cannot be read") {
		t.Errorf("exit %d, stderr %q", code, errs.String())
	}
}

func TestServeStopsAtTheStartWhenTheAddressOfAChannelCannotBeUsedAndNamesNoAddress(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "oncall"), []byte("http://hooks.example/TOKEN-SECRET"), 0o600); err != nil {
		t.Fatal(err)
	}
	rules := filepath.Join(dir, "alerts.json")
	if err := os.WriteFile(rules, []byte(`{"channels": {"oncall": {"urlFile": "`+filepath.Join(dir, "oncall")+`"}}, "rules": []}`), 0o600); err != nil {
		t.Fatal(err)
	}
	var out, errs bytes.Buffer
	args := serveArgs(t, "strata")
	if code := run(append(args, "--alerts", rules), &out, &errs); code != 2 || !strings.Contains(errs.String(), `channel "oncall"`) || !strings.Contains(errs.String(), "https") || strings.Contains(errs.String(), "TOKEN-SECRET") {
		t.Errorf("exit %d, stderr %q", code, errs.String())
	}
}

func TestADigestPeriodThatIsNotPositiveStopsTheStart(t *testing.T) {
	var out, errs bytes.Buffer
	if code := run(append(onceArgs(t, "strata"), "--digest-every", "0s"), &out, &errs); code != 2 || !strings.Contains(errs.String(), "more than zero") {
		t.Errorf("exit %d, stderr %q", code, errs.String())
	}
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
	if got := w.Bodies(); len(got) != 1 || !strings.Contains(got[0], "Drift") || !strings.Contains(got[0], "payments") {
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
