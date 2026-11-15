package api

import (
	"context"
	"log/slog"
	"net/http"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/alert"
	"github.com/como-technologies/strata/observer/internal/check"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// alerting gives an observer of the rig that sends the alerts of the rules to the receiver, through the
// real sender. The address is in a file, as a Secret gives it.
func alerting(t *testing.T, r *rig, w *testkit.Receiver, s check.Strata) *check.Observer {
	t.Helper()
	file := testkit.OncallFile(t, w.MarkedAddress())
	rules, err := alert.Parse([]byte(testkit.OncallRules(file, testkit.DriftNow, `{"level": "cluster", "news": "newer", "send": "digest", "to": "oncall"}`,
		`{"level": "profile", "match": "other-profile", "news": "drift", "muteUntil": "2099-01-01"}`)))
	if err != nil {
		t.Fatal(err)
	}
	engine := alert.New(rules, alert.NewWebhook(rules.Channels, true), testkit.DigestEvery, slog.New(slog.NewTextHandler(r.log, nil)))
	return r.observer(s).WithAlerts(engine)
}

func TestTheRouteAlertsShowsTheRulesTheirStatesAndTheChannelNames(t *testing.T) {
	r := newRig(t)
	w := testkit.NewReceiver(t)
	o := alerting(t, r, w, &fakeStrata{dir: r.fixtures})
	r.once(o)
	var v alert.View
	rec := get(handlerOf(o), summaryRoutes[patternAlerts])
	decode(t, rec, &v)
	if rec.Code != http.StatusOK || !v.Set || len(v.Channels) != 1 || v.Channels[0] != "oncall" || len(v.Rules) != 3 {
		t.Fatalf("status %d, view %+v", rec.Code, v)
	}
	if v.Rules[0].State != alert.StateFiring || v.Rules[2].State != alert.StateMuted {
		t.Errorf("states %+v", v.Rules)
	}
	testkit.HoldsNone(t, "the route", rec.Body.String(), testkit.AddressMarker, "127.0.0.1")
}

func TestAnObserverWithoutAlertsSaysSoOnTheRoute(t *testing.T) {
	var v alert.View
	r := newRig(t)
	rec := get(handlerOf(r.observer(&fakeStrata{dir: r.fixtures})), summaryRoutes[patternAlerts])
	decode(t, rec, &v)
	if rec.Code != http.StatusOK || v.Set || len(v.Rules) != 0 || v.Channels == nil || v.Rules == nil {
		t.Errorf("status %d, view %+v, body %s", rec.Code, v, rec.Body.String())
	}
}

// A full cycle with the sentinel in each changed value, in a variable, and in the text of a diff, must
// leave none in the message that the receiver got. The writer of the audit log stays out too.
func TestAMessageThatReachesTheReceiverHoldsNoValueAndTheAddressReachesNoRouteNoLogAndNoState(t *testing.T) {
	r := newRig(t)
	w := testkit.NewReceiver(t)
	w.SetStatus(http.StatusInternalServerError)
	o := alerting(t, r, w, &fakeStrata{dir: r.fixtures})
	r.once(o)
	w.SetStatus(http.StatusOK)
	if err := o.RunContent(context.Background()); err != nil {
		t.Fatal(err)
	}
	got := strings.Join(w.Bodies(), "\n")
	if !strings.Contains(got, "Drift") || !strings.Contains(got, testkit.AWSStackProfile) {
		t.Fatalf("the receiver got %q", got)
	}
	h, host := handlerOf(o), strings.TrimPrefix(w.URL, "http://")
	noSummaryRouteStateOrLogHolds(t, r, h, testkit.AddressMarker, host)
	testkit.HoldsNone(t, "the list of answers", get(h, askRoutes[patternAsks]).Body.String(), testkit.AddressMarker, host)
	banned := []string{"SENTINEL", sentinelValue, sentinelVariable, sentinelText, "Betty"}
	testkit.HoldsNone(t, "the messages", got, banned...)
	testkit.StateHoldsNone(t, r.state, banned...)
	testkit.HoldsAll(t, "the log", r.log.String(), "channel=oncall", "verdict=failed", "status=500", "verdict=sent")
}
