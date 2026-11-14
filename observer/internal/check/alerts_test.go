package check

import (
	"context"
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
	"github.com/como-technologies/strata/observer/internal/testkit"
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

// everyone sends a drift and a loss of contact at once to oncall.
var everyone = testkit.OncallRules("f", testkit.DriftNow, testkit.LostNow)

// payments starts the head of a message of the cluster payments of the rig. paymentsCleared is in the message when the
// drift of aws-stack@1.1.0 clears.
const (
	payments        = "strata · " + testkit.PaymentsName + " · "
	paymentsCleared = "Drift cleared\n" + testkit.AWSStackProfile
)

// alerting gives the observer of a rig that sends the alerts of the rules everyone to the sender.
func alerting(t *testing.T, r *rig, w Strata, sender alert.Sender) *Observer {
	t.Helper()
	return withRules(t, r, r.observer(w), sender, everyone)
}

func TestOneMessageWhenAFindingOpensNoneOnTheNextCycleAndOneWhenItClears(t *testing.T) {
	r, w := twoRecords(t)
	w.rows[0] = drift(t)
	sender := &capture{}
	o := alerting(t, r, w, sender)
	at(t, o, t0)
	if len(sender.texts) != 1 || !strings.HasPrefix(sender.texts[0], payments+"Drift\n"+testkit.AWSStackProfile+": ") {
		t.Fatalf(testkit.OpenMessage, sender.texts)
	}
	at(t, o, t0.Add(testkit.ContentEvery))
	if len(sender.texts) != 1 {
		t.Fatalf(testkit.NextCycle, len(sender.texts))
	}
	w.rows[0] = unchanged()
	at(t, o, t0.Add(30*time.Minute))
	if len(sender.texts) != 2 || !strings.HasPrefix(sender.texts[1], payments+paymentsCleared+"\n") {
		t.Fatalf(testkit.ClearMessage, sender.texts)
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
		o.now = func() time.Time { return t0.Add(time.Duration(i) * testkit.ContentEvery) }
		_ = o.RunContent(context.Background())
	}
	lost := sender.texts[1:]
	if len(lost) != 1 || !strings.HasPrefix(lost[0], payments+"Lost contact\n"+testkit.NoReachAfter(2)) {
		t.Fatalf("the messages of the loss %q", lost)
	}
	w.verifyErr = nil
	at(t, o, t0.Add(2*time.Hour))
	if len(sender.texts) != 3 || !strings.Contains(sender.texts[2], testkit.LostCleared) {
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
	view := testkit.MustJSON(t, o.Alerts())
	banned := []string{sentinel, "charts.a.b", "Betty", "John", "OLD", "SENTINEL"}
	testkit.HoldsNone(t, "the messages", strings.Join(sender.texts, "\n"), banned...)
	stateHoldsNone(t, r, banned...)
	testkit.HoldsNone(t, "the log", r.log.String(), banned...)
	testkit.HoldsNone(t, "the view of the alerts", string(view), banned...)
}

func TestAFailedSendKeepsTheMessageInTheStateAndSendsItOnceAfterItWorks(t *testing.T) {
	r, w := edited(t)
	sender := &capture{down: true}
	o := alerting(t, r, w, sender)
	at(t, o, t0)
	at(t, o, t0.Add(testkit.ContentEvery))
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
	if err := os.Remove(filepath.Join(r.state, testkit.StateFile)); err != nil {
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
	state := stateText(t, r)
	if !strings.Contains(state, `"alerts":{"items":{},"outbox":[]}`) {
		t.Errorf("state %s", state)
	}
}

func TestTheStateThatAnOlderObserverWroteLoadsWithAnEmptyStoreOfAlerts(t *testing.T) {
	r := newRig(t)
	if err := os.MkdirAll(r.state, 0o700); err != nil {
		t.Fatal(err)
	}
	testkit.WriteIn(t, r.state, testkit.StateFile, `{"startedAt":"2026-11-03T14:00:00Z"}`)
	s, err := loadState(r.state)
	if err != nil || s.Alerts.Items == nil || s.Alerts.Outbox == nil {
		t.Errorf("state %+v, error %v", s.Alerts, err)
	}
}

// imageRules sends a change of an image of the cluster at once to oncall.
var imageRules = testkit.OncallRules("f", testkit.ImageNow)

// withRules gives the observer the alerts of the rules, as `serve` does.
func withRules(t *testing.T, r *rig, o *Observer, sender alert.Sender, rules string) *Observer {
	t.Helper()
	parsed, err := alert.Parse([]byte(rules))
	if err != nil {
		t.Fatal(err)
	}
	return o.WithAlerts(alert.New(parsed, sender, testkit.DigestEvery, slog.New(slog.NewTextHandler(r.log, nil))))
}

func TestAnImageFindingClearsOnlyAfterACycleThatReadThePods(t *testing.T) {
	r, w, o, pods := podRig(t, runs("web-1", testkit.Digest1))
	sender := &capture{}
	o = withRules(t, r, o, sender, imageRules)
	cycle(t, o)
	pods.set([]kube.Container{runs("web-1", testkit.Digest2)}, nil)
	cycle(t, o)
	if len(sender.texts) != 1 || !strings.HasPrefix(sender.texts[0], payments+testkit.ImageMoved+"\n"+testkit.AWSStackProfile+": ") {
		t.Fatalf("messages %q", sender.texts)
	}
	// a restart without the read of the pods finds no image finding, and that proves nothing
	cycle(t, withRules(t, r, r.observer(w), sender, imageRules))
	if len(sender.texts) != 1 {
		t.Fatalf("a cycle that read no pod sent a clear: %q", sender.texts)
	}
	// a restart that reads the pods and sees the first digest clears the finding
	pods.set([]kube.Container{runs("web-1", testkit.Digest1)}, nil)
	cycle(t, withRules(t, r, r.observer(w).WithPods(pods), sender, imageRules))
	if len(sender.texts) != 2 || !strings.Contains(sender.texts[1], testkit.ImageMoved+" cleared") {
		t.Errorf("messages %q", sender.texts)
	}
}
