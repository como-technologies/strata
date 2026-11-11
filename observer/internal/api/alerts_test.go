package api

import (
	"context"
	"log/slog"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/alert"
	"github.com/como-technologies/strata/observer/internal/check"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// addressMarker is in the address of the webhook of these tests. It stands for the token that a real
// address holds, and it must reach no route, no log, and no file of the state.
const addressMarker = "TOKEN-SECRET"

// alerting gives an observer of the rig that sends the alerts of the rules to the receiver, through the
// real sender. The address is in a file, as a Secret gives it.
func alerting(t *testing.T, r *rig, w *testkit.Receiver, s check.Strata) *check.Observer {
	t.Helper()
	file := filepath.Join(t.TempDir(), "oncall")
	if err := os.WriteFile(file, []byte(w.URL+"/"+addressMarker+"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	rules, err := alert.Parse([]byte(`{"channels": {"oncall": {"urlFile": "` + file + `"}}, "rules": [
		{"level": "cluster", "news": "drift", "send": "now", "to": "oncall"},
		{"level": "cluster", "news": "newer", "send": "digest", "to": "oncall"},
		{"level": "profile", "match": "other-profile", "news": "drift", "muteUntil": "2099-01-01"}]}`))
	if err != nil {
		t.Fatal(err)
	}
	engine := alert.New(rules, alert.NewWebhook(rules.Channels, true), 7*24*time.Hour, slog.New(slog.NewTextHandler(r.log, nil)))
	return r.observer(s).WithAlerts(engine)
}

func TestTheRouteAlertsShowsTheRulesTheirStatesAndTheChannelNames(t *testing.T) {
	r := newRig(t)
	w := testkit.NewReceiver(t)
	o := alerting(t, r, w, &fakeStrata{dir: r.fixtures})
	if _, err := o.Once(context.Background()); err != nil {
		t.Fatal(err)
	}
	var v alert.View
	rec := get(handlerOf(o), "/v1/alerts")
	decode(t, rec, &v)
	if rec.Code != http.StatusOK || !v.Set || len(v.Channels) != 1 || v.Channels[0] != "oncall" || len(v.Rules) != 3 {
		t.Fatalf("status %d, view %+v", rec.Code, v)
	}
	if v.Rules[0].State != alert.StateFiring || v.Rules[2].State != alert.StateMuted {
		t.Errorf("states %+v", v.Rules)
	}
	if strings.Contains(rec.Body.String(), addressMarker) || strings.Contains(rec.Body.String(), "127.0.0.1") {
		t.Errorf("the route shows the address: %s", rec.Body.String())
	}
}

func TestAnObserverWithoutAlertsSaysSoOnTheRoute(t *testing.T) {
	var v alert.View
	rec := get(handlerOf(newRig(t).observer(&fakeStrata{dir: newRig(t).fixtures})), "/v1/alerts")
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
	if _, err := o.Once(context.Background()); err != nil {
		t.Fatal(err)
	}
	w.SetStatus(http.StatusOK)
	if err := o.RunContent(context.Background()); err != nil {
		t.Fatal(err)
	}
	got := strings.Join(w.Bodies(), "\n")
	if !strings.Contains(got, "Drift") || !strings.Contains(got, "aws-stack@1.1.0") {
		t.Fatalf("the receiver got %q", got)
	}
	state, err := os.ReadFile(filepath.Join(r.state, "state.json"))
	if err != nil {
		t.Fatal(err)
	}
	routes := strings.Builder{}
	for _, path := range append(routePaths(), "/v1/asks") {
		routes.WriteString(get(handlerOf(o), path).Body.String())
	}
	for _, place := range []struct{ name, text string }{{"the log", r.log.String()}, {"the state", string(state)}, {"the routes", routes.String()}} {
		if strings.Contains(place.text, addressMarker) || strings.Contains(place.text, strings.TrimPrefix(w.URL, "http://")) {
			t.Errorf("%s shows the address", place.name)
		}
	}
	for _, banned := range []string{"SENTINEL", sentinelValue, sentinelVariable, sentinelText, "Betty"} {
		if strings.Contains(got, banned) || strings.Contains(string(state), banned) {
			t.Errorf("a message or the state holds %q", banned)
		}
	}
	for _, want := range []string{"channel=oncall", "verdict=failed", "status=500", "verdict=sent"} {
		if !strings.Contains(r.log.String(), want) {
			t.Errorf("the log lacks %q:\n%s", want, r.log.String())
		}
	}
}

// routePaths lists the path of each summary route of the sentinel table.
func routePaths() []string {
	var paths []string
	for _, p := range summaryRoutes {
		paths = append(paths, p)
	}
	return paths
}
