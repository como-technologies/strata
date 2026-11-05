package check

import (
	"context"
	"encoding/json"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/alert"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/kube"
	"github.com/como-technologies/strata/observer/internal/runner"
)

// capture is a sender that keeps the text of each message that went. While down is set, it refuses.
type capture struct {
	texts []string
	down  bool
}

func (c *capture) Send(_ context.Context, channel, text string) error {
	if c.down {
		return &alert.SendError{Channel: channel, Kind: alert.KindStatus, Status: 503}
	}
	c.texts = append(c.texts, text)
	return nil
}

const everyone = `{"channels": {"oncall": {"urlFile": "f"}}, "rules": [
	{"level": "cluster", "news": "drift", "send": "now", "to": "oncall"},
	{"level": "cluster", "news": "lost", "send": "now", "to": "oncall"}]}`

// alerting gives the observer of a rig that sends the alerts of the rules to the sender.
func alerting(t *testing.T, r *rig, w Strata, sender alert.Sender) *Observer {
	t.Helper()
	rules, err := alert.Parse([]byte(everyone))
	if err != nil {
		t.Fatal(err)
	}
	return r.observer(w).WithAlerts(alert.New(rules, sender, 7*24*time.Hour, slog.New(slog.NewTextHandler(r.log, nil))))
}

func TestOneMessageWhenAFindingOpensNoneOnTheNextCycleAndOneWhenItClears(t *testing.T) {
	r, w := twoRecords(t)
	w.rows[0] = drift(t)
	sender := &capture{}
	o := alerting(t, r, w, sender)
	at(t, o, t0)
	if len(sender.texts) != 1 || !strings.HasPrefix(sender.texts[0], "strata · payments · Drift\naws-stack@1.1.0: ") {
		t.Fatalf("the open message %q", sender.texts)
	}
	at(t, o, t0.Add(15*time.Minute))
	if len(sender.texts) != 1 {
		t.Fatalf("the next cycle sent %d messages", len(sender.texts))
	}
	w.rows[0] = unchanged()
	at(t, o, t0.Add(30*time.Minute))
	if len(sender.texts) != 2 || !strings.HasPrefix(sender.texts[1], "strata · payments · Drift cleared\naws-stack@1.1.0\n") {
		t.Fatalf("the clear message %q", sender.texts)
	}
}

func TestAFailedReadSendsNoClearAndAGoodReadSendsIt(t *testing.T) {
	r, w := twoRecords(t)
	w.rows[0] = drift(t)
	sender := &capture{}
	o := alerting(t, r, w, sender)
	at(t, o, t0)
	// One 429 for one record: the other row is good, so the cycle is good and the part of the record is not read.
	w.rows[0] = notRead()
	at(t, o, t0.Add(time.Hour))
	if len(sender.texts) != 1 {
		t.Fatalf("a read that failed sent %q", sender.texts[1:])
	}
	w.rows[0] = unchanged()
	at(t, o, t0.Add(2*time.Hour))
	if len(sender.texts) != 2 || !strings.Contains(sender.texts[1], "Drift cleared") {
		t.Errorf("messages %q", sender.texts)
	}
}

func TestACycleThatFailedSendsNoClear(t *testing.T) {
	r, w := edited(t)
	sender := &capture{}
	o := alerting(t, r, w, sender)
	at(t, o, t0)
	w.verifyErr = &runner.RunError{Kind: docs.KindAPI, Status: 500}
	o.now = func() time.Time { return t0.Add(time.Hour) }
	if err := o.RunContent(context.Background()); err == nil {
		t.Fatal("the cycle did not fail")
	}
	if len(sender.texts) != 1 {
		t.Errorf("messages %q", sender.texts)
	}
}

func TestTheLossOfContactSendsOneMessageAndOneClear(t *testing.T) {
	r, w := edited(t)
	sender := &capture{}
	o := alerting(t, r, w, sender)
	at(t, o, t0)
	w.verifyErr = &runner.RunError{Kind: docs.KindAPI, Status: 401}
	for i := 1; i <= 4; i++ {
		o.now = func() time.Time { return t0.Add(time.Duration(i) * 15 * time.Minute) }
		_ = o.RunContent(context.Background())
	}
	lost := sender.texts[1:]
	if len(lost) != 1 || !strings.HasPrefix(lost[0], "strata · payments · Lost contact\nThe observer cannot reach Palette. The last 2 content checks failed: api, HTTP 401.") {
		t.Fatalf("the messages of the loss %q", lost)
	}
	w.verifyErr = nil
	at(t, o, t0.Add(2*time.Hour))
	if len(sender.texts) != 3 || !strings.Contains(sender.texts[2], "Lost contact cleared") {
		t.Errorf("messages %q", sender.texts)
	}
}

func TestAMessageHoldsNoValueAndTheStateAndTheLogHoldNone(t *testing.T) {
	r, w := edited(t)
	sender := &capture{}
	o := alerting(t, r, w, sender)
	at(t, o, t0)
	w.rows[0] = unchanged()
	w.compares = nil
	at(t, o, t0.Add(time.Hour))
	if len(sender.texts) != 2 {
		t.Fatalf("messages %q", sender.texts)
	}
	state, err := os.ReadFile(filepath.Join(r.state, stateFile))
	if err != nil {
		t.Fatal(err)
	}
	view, err := json.Marshal(o.Alerts())
	if err != nil {
		t.Fatal(err)
	}
	places := []string{strings.Join(sender.texts, "\n"), string(state), r.log.String(), string(view)}
	for _, banned := range []string{sentinel, "charts.a.b", "Betty", "John", "OLD", "SENTINEL"} {
		for i, place := range places {
			if strings.Contains(place, banned) {
				t.Errorf("place %d holds %q: %s", i, banned, place)
			}
		}
	}
}

func TestAFailedSendKeepsTheMessageInTheStateAndSendsItOnceAfterItWorks(t *testing.T) {
	r, w := edited(t)
	sender := &capture{down: true}
	o := alerting(t, r, w, sender)
	at(t, o, t0)
	at(t, o, t0.Add(15*time.Minute))
	if len(sender.texts) != 0 || o.Alerts().Waiting != 1 {
		t.Fatalf("sent %q, waiting %d", sender.texts, o.Alerts().Waiting)
	}
	// A restart keeps the message: the state holds it.
	o = alerting(t, r, w, sender)
	sender.down = false
	at(t, o, t0.Add(30*time.Minute))
	at(t, o, t0.Add(45*time.Minute))
	if len(sender.texts) != 1 || o.Alerts().Waiting != 0 || o.Alerts().Rules[0].State != alert.StateFiring {
		t.Errorf("sent %q, view %+v", sender.texts, o.Alerts())
	}
}

func TestARestartSendsNothingAgainAndANewPodWithAnEmptyStateSendsTheOpenFindingOnce(t *testing.T) {
	r, w := edited(t)
	sender := &capture{}
	at(t, alerting(t, r, w, sender), t0)
	at(t, alerting(t, r, w, sender), t0.Add(time.Hour))
	if len(sender.texts) != 1 {
		t.Fatalf("a restart with the state sent %d messages in all", len(sender.texts))
	}
	if err := os.Remove(filepath.Join(r.state, stateFile)); err != nil {
		t.Fatal(err)
	}
	pod := alerting(t, r, w, sender)
	at(t, pod, t0.Add(2*time.Hour))
	at(t, pod, t0.Add(3*time.Hour))
	if len(sender.texts) != 2 {
		t.Errorf("a new pod sent %d messages in all", len(sender.texts))
	}
}

func TestAnObserverWithoutAlertsSendsNothingAndItsViewIsUnset(t *testing.T) {
	r, w := edited(t)
	o := r.observer(w)
	at(t, o, t0)
	if v := o.Alerts(); v.Set || len(v.Rules) != 0 {
		t.Errorf("view %+v", v)
	}
	state, err := os.ReadFile(filepath.Join(r.state, stateFile))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(state), `"alerts":{"items":{},"outbox":[]}`) {
		t.Errorf("state %s", state)
	}
}

func TestTheStateThatAnOlderObserverWroteLoadsWithAnEmptyStoreOfAlerts(t *testing.T) {
	r := newRig(t)
	if err := os.MkdirAll(r.state, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(r.state, stateFile), []byte(`{"startedAt":"2026-11-03T14:00:00Z"}`), 0o600); err != nil {
		t.Fatal(err)
	}
	s, err := loadState(r.state)
	if err != nil || s.Alerts.Items == nil || s.Alerts.Outbox == nil {
		t.Errorf("state %+v, error %v", s.Alerts, err)
	}
}

const imageRules = `{"channels": {"oncall": {"urlFile": "f"}}, "rules": [{"level": "cluster", "news": "image", "send": "now", "to": "oncall"}]}`

// withRules gives the observer the alerts of the rules, as `serve` does.
func withRules(t *testing.T, r *rig, o *Observer, sender alert.Sender, rules string) *Observer {
	t.Helper()
	parsed, err := alert.Parse([]byte(rules))
	if err != nil {
		t.Fatal(err)
	}
	return o.WithAlerts(alert.New(parsed, sender, 7*24*time.Hour, slog.New(slog.NewTextHandler(r.log, nil))))
}

func TestAnImageFindingClearsOnlyAfterACycleThatReadThePods(t *testing.T) {
	r, w, o, pods := podRig(t, runs("web-1", digest1))
	sender := &capture{}
	o = withRules(t, r, o, sender, imageRules)
	cycle(t, o)
	pods.set([]kube.Container{runs("web-1", digest2)}, nil)
	cycle(t, o)
	if len(sender.texts) != 1 || !strings.HasPrefix(sender.texts[0], "strata · payments · Image moved\naws-stack@1.1.0: ") {
		t.Fatalf("messages %q", sender.texts)
	}
	// a restart without the read of the pods finds no image finding, and that proves nothing
	cycle(t, withRules(t, r, r.observer(w), sender, imageRules))
	if len(sender.texts) != 1 {
		t.Fatalf("a cycle that read no pod sent a clear: %q", sender.texts)
	}
	// a restart that reads the pods and sees the first digest clears the finding
	pods.set([]kube.Container{runs("web-1", digest1)}, nil)
	cycle(t, withRules(t, r, r.observer(w).WithPods(pods), sender, imageRules))
	if len(sender.texts) != 2 || !strings.Contains(sender.texts[1], "Image moved cleared") {
		t.Errorf("messages %q", sender.texts)
	}
}
