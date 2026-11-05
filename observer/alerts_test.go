package main

import (
	"bytes"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/alert"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// webhook is a receiver that keeps the body of each request.
type webhook struct {
	*httptest.Server
	mu     sync.Mutex
	bodies []string
}

func newWebhook(t *testing.T) *webhook {
	t.Helper()
	w := &webhook{}
	w.Server = httptest.NewServer(http.HandlerFunc(func(rw http.ResponseWriter, req *http.Request) {
		raw, _ := io.ReadAll(req.Body)
		w.mu.Lock()
		w.bodies = append(w.bodies, string(raw))
		w.mu.Unlock()
	}))
	t.Cleanup(w.Close)
	return w
}

func (w *webhook) count() int {
	w.mu.Lock()
	defer w.mu.Unlock()
	return len(w.bodies)
}

// rulesFile writes a file of rules with one channel, oncall, whose address is that of the receiver, and
// the rules for drift and for the loss of contact.
func rulesFile(t *testing.T, w *webhook) string {
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
	w := newWebhook(t)
	var out, errs bytes.Buffer
	args := append(onceArgs(t, fakeStrata(t, drifting, 0, false)), "--alerts", rulesFile(t, w), "--alerts-allow-http")
	if code := run(args, &out, &errs); code != 1 {
		t.Fatalf("exit %d, stderr %q", code, errs.String())
	}
	if w.count() != 0 {
		t.Errorf("once sent %d messages", w.count())
	}
}

func TestOnceAndServeStopAtTheStartWithAFileOfRulesThatCannotBeUsed(t *testing.T) {
	bad := filepath.Join(t.TempDir(), "alerts.json")
	if err := os.WriteFile(bad, []byte("{\n\"rules\": [\n{\"level\": \"profile\", \"match\": \"barney-base\", \"news\": \"newer\"}\n]\n}"), 0o600); err != nil {
		t.Fatal(err)
	}
	for _, command := range []string{"once", "serve"} {
		var out, errs bytes.Buffer
		args := append(onceArgs(t, "strata"), "--alerts", bad)
		args[0] = command
		if code := run(args, &out, &errs); code != 2 || out.Len() != 0 {
			t.Fatalf("%s: exit %d, stdout %q", command, code, out.String())
		}
		for _, want := range []string{"--alerts", "alerts.json", "line 3", "rule 1", "end date"} {
			if !strings.Contains(errs.String(), want) {
				t.Errorf("%s: stderr lacks %q: %s", command, want, errs.String())
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
	args, _ := serveArgs(t, "strata")
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
	w := newWebhook(t)
	args, addr := serveArgs(t, fakeStrata(t, drifting, 0, false))
	args = append(args, "--alerts", rulesFile(t, w), "--alerts-allow-http")
	stop, stderr := startServe(t, args, addr)
	var view alert.View
	testkit.WaitFor(t, "the rule of drift fires", func() bool {
		resp, err := http.Get("http://" + addr + "/v1/alerts")
		if err != nil {
			return false
		}
		defer resp.Body.Close()
		raw, _ := io.ReadAll(resp.Body)
		return json.Unmarshal(raw, &view) == nil && len(view.Rules) == 2 && view.Rules[0].State == alert.StateFiring
	})
	time.Sleep(50 * time.Millisecond)
	stop()
	if w.count() != 1 || !strings.Contains(w.bodies[0], "Drift") || !strings.Contains(w.bodies[0], "payments") {
		t.Errorf("the receiver got %d messages: %q", w.count(), w.bodies)
	}
	if strings.Contains(stderr.String(), "TOKEN-SECRET") || !strings.Contains(stderr.String(), "channel=oncall") {
		t.Errorf("stderr %s", stderr.String())
	}
}

func TestServeWithoutAlertsSendsNothingAndTheRouteSaysSo(t *testing.T) {
	args, addr := serveArgs(t, fakeStrata(t, drifting, 0, false))
	stop, stderr := startServe(t, args, addr)
	defer stop()
	testkit.WaitFor(t, "the page state ok", func() bool {
		resp, err := http.Get("http://" + addr + "/v1/findings")
		if err != nil {
			return false
		}
		defer resp.Body.Close()
		var got struct {
			Findings []json.RawMessage `json:"findings"`
		}
		return json.NewDecoder(resp.Body).Decode(&got) == nil && len(got.Findings) == 1
	})
	resp, err := http.Get("http://" + addr + "/v1/alerts")
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	var view alert.View
	if err := json.NewDecoder(resp.Body).Decode(&view); err != nil || view.Set || len(view.Rules) != 0 {
		t.Errorf("view %+v, error %v", view, err)
	}
	if strings.Contains(stderr.String(), "msg=alert") {
		t.Errorf("stderr %s", stderr.String())
	}
}
