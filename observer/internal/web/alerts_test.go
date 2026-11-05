package web

import (
	"encoding/json"
	"regexp"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/alert"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

// navOfAlerts finds the nav item Alerts and its count.
var navOfAlerts = regexp.MustCompile(`<a href="#alerts" class="[^"]*"><span>Alerts</span><span class="cnt( r)?">([^<]*)</span></a>`)

// The badge of each state of a rule, as the page draws it.
var alertBadges = map[alert.State]string{
	alert.StateFiring:   `<span class="badge r">Firing</span>`,
	alert.StatePending:  `<span class="badge a">Pending</span>`,
	alert.StateInactive: `<span class="badge g">Inactive</span>`,
	alert.StateMuted:    `<span class="badge y">Muted</span>`,
}

// The nav item Alerts is the last item. It says "Not set" when the observer sends no alert, and else it
// counts the rules that fire.
func TestTheNavItemAlertsSaysNotSetOrCountsTheRulesThatFire(t *testing.T) {
	cases := []struct {
		state devdata.State
		red   string
		count string
	}{
		{devdata.StateOK, "", "Not set"},
		{devdata.StateAlertsUnset, "", "Not set"},
		{devdata.StateAlerts, " r", "1"},
	}
	for _, c := range cases {
		t.Run(string(c.state), func(t *testing.T) {
			overview := pageOf(t, render(t, c.state, false), "#overview", "")
			m := navOfAlerts.FindStringSubmatch(overview)
			if m == nil || m[1] != c.red || m[2] != c.count {
				t.Fatalf("the nav item is %q, want %q %q", m, c.red, c.count)
			}
			if notChecked, alerts := strings.Index(overview, `<a href="#notchecked" class=`), strings.Index(overview, `<a href="#alerts" class=`); !(notChecked >= 0 && notChecked < alerts) {
				t.Errorf("the nav item Alerts does not follow Not checked: %d %d", notChecked, alerts)
			}
		})
	}
}

// The screen shows the bar with the rules of the page, the table of the rules, and the sample message.
// Each rule has its state, its level, what it applies to, its news, when it sends, and where. The
// words of the mock stay, and the words "A sketch for later" go.
func TestTheScreenOfTheAlertsShowsEachRuleWithItsStateAndTheSampleMessage(t *testing.T) {
	set := loadSet(t, devdata.StateAlerts)
	screen := pageOf(t, render(t, devdata.StateAlerts, false), "#alerts", "")
	mustHold(t, "the screen", screen,
		`<div class="alertbar b">One message when a finding opens, and one when it clears. <span>A lower level overrides a higher one. A mute always has an end date. Messages hold no values.</span></div>`,
		`<h2>Alert rules <span class="sub">7</span></h2>`,
		`<th>State</th><th>Level</th><th>Applies to</th><th>News</th><th>Send</th><th>Where</th>`,
		`<h2>Sample message</h2>`)
	mustNotHold(t, "the screen", screen, "A sketch for later", "Not set")
	rows := map[string]string{ // the cells of a row, by the text that marks it
		"identity-oncall":   `<td>Profile</td><td class="m">fred-identity</td><td>Drift</td><td>At once</td><td class="m">identity-oncall</td>`,
		"platform-planning": `<td>Cluster</td><td class="m">quarry-1</td><td>Newer version</td><td>Weekly digest</td><td class="m">platform-planning</td>`,
		"2027-01-31":        `<td>Profile</td><td class="m">barney-base</td><td>Newer version</td><td>Muted until 2027-01-31</td><td class="m">&ndash;</td>`,
		"wilma-team":        `<td>Pack</td><td class="m">traefik</td><td>Image split or moved</td><td>At once</td><td class="m">wilma-team</td>`,
	}
	for marker, cells := range rows {
		row := rowWith(t, screen, marker)
		mustHold(t, marker, row, cells)
	}
	for _, r := range set.AlertRules.Rules {
		mustHold(t, string(r.News)+" "+r.Match, screen, alertBadges[r.State])
	}
	for _, line := range strings.Split(set.AlertRules.Sample, "\n") {
		mustHold(t, "the sample message", screen, line)
	}
	mustHold(t, "the sample message", screen, `<div class="pre mt0">strata · quarry-1 · Drift`)
}

// The screen of an observer that started without --alerts is an offer: it says that no rule is set, what a message
// is, and the next step with its command. It shows no table and no sample.
func TestWithNoAlertsFlagTheScreenSaysTheSendIsOptIn(t *testing.T) {
	for _, state := range []devdata.State{devdata.StateOK, devdata.StateAlertsUnset} {
		t.Run(string(state), func(t *testing.T) {
			screen := pageOf(t, render(t, state, false), "#alerts", "")
			mustHold(t, "the screen", screen, `<h2>Alert rules <span class="sub">Not set</span></h2>`, "Nobody gets paged yet", "This install has no alert rules.",
				"The rules are a file in Git, so a mute always has a name and an end date.",
				`<b>Next</b> Write the rules in <span class="m">alerts.json</span>, name the component <span class="m">deploy/observer/alerts</span> in the kustomization, then apply it.`,
				"<code>kubectl apply -k .</code>", `class="splash md tone-off"`)
			mustNotHold(t, "the screen", screen, "<table>", "Sample message", "Firing")
		})
	}
}

// A mute that ended reads "Mute ended" and Inactive, and a message that waits for a retry shows an amber
// bar. The log names the channel and the status, and the bar says so.
func TestAMuteThatEndedAndAMessageThatWaitsShowTheirWords(t *testing.T) {
	dir := writeSet(t, devdata.StateAlerts, map[string]func([]byte) []byte{
		"alerts.json": func(raw []byte) []byte {
			var v alert.View
			if err := devdata.Decode(raw, &v); err != nil {
				t.Fatal(err)
			}
			v.Waiting = 2
			v.Rules[5].State = alert.StateInactive
			v.Rules[5].MuteUntil = "2026-10-01"
			return mustJSON(t, v)
		},
	})
	screen := pagesOf(t, dir, "#alerts")[0]
	mustHold(t, "the screen", screen, `<div class="alertbar a">2 messages wait for a retry. <span>The log names the channel and the status. It never holds the address.</span></div>`,
		`<td>Mute ended 2026-10-01</td>`)
	mustNotHold(t, "the screen", screen, "Muted until 2026-10-01")
	one := writeSet(t, devdata.StateAlerts, map[string]func([]byte) []byte{
		"alerts.json": func(raw []byte) []byte {
			var v alert.View
			if err := devdata.Decode(raw, &v); err != nil {
				t.Fatal(err)
			}
			v.Waiting = 1
			return mustJSON(t, v)
		},
	})
	mustHold(t, "the screen", pagesOf(t, one, "#alerts")[0], "1 message waits for a retry.")
}

// A digest period other than a week reads as the period.
func TestADigestPeriodOtherThanAWeekReadsAsThePeriod(t *testing.T) {
	dir := writeSet(t, devdata.StateAlerts, map[string]func([]byte) []byte{
		"alerts.json": func(raw []byte) []byte {
			var v alert.View
			if err := devdata.Decode(raw, &v); err != nil {
				t.Fatal(err)
			}
			v.DigestEvery = "24h"
			return mustJSON(t, v)
		},
	})
	screen := pagesOf(t, dir, "#alerts")[0]
	mustHold(t, "the screen", screen, "<td>Digest every 24h</td>")
	mustNotHold(t, "the screen", screen, "Weekly digest")
}

// The names of a rule and of a channel come from a file of a team, so the screen shows them as text.
func TestTheNamesOfTheRulesShowAsTextOnTheScreenOfTheAlerts(t *testing.T) {
	screen := pageOf(t, render(t, devdata.StateAlerts, true), "#alerts", "")
	mustNotHold(t, "the screen", screen, "<img")
	mustHold(t, "the screen", screen, "&lt;img src=x onerror=alert(1)&gt;")
}

func mustJSON(t *testing.T, v any) []byte {
	t.Helper()
	raw, err := json.Marshal(v)
	if err != nil {
		t.Fatal(err)
	}
	return raw
}
