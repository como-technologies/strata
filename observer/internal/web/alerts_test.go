package web

import (
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/alert"
	"github.com/como-technologies/strata/observer/internal/testkit"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

// rulesHead starts the head of the screen of the alert rules, before the count.
const rulesHead = `<h2>Alert rules <span class="sub">`

// navOfAlerts finds the nav item Alerts and its count.
var navOfAlerts = navItem("#alerts", "Alerts", "( r)?")

// The badge of each state of a rule, as the page draws it.
var alertBadges = map[alert.State]string{
	alert.StateFiring:   badge("r", "Firing"),
	alert.StatePending:  badge("a", "Pending"),
	alert.StateInactive: badge("g", "Inactive"),
	alert.StateMuted:    badge("y", "Muted"),
}

// The nav item Alerts is the last item. It says "Not set" when the observer sends no alert, and else it
// counts the rules that fire.
func TestTheNavItemAlertsSaysNotSetOrCountsTheRulesThatFire(t *testing.T) {
	cases := []struct {
		state devdata.State
		red   string
		count string
	}{
		{devdata.StateOK, "", notSet},
		{devdata.StateAlertsUnset, "", notSet},
		{devdata.StateAlerts, " r", "1"},
	}
	for _, c := range cases {
		t.Run(string(c.state), func(t *testing.T) {
			overview := pageOf(t, render(t, c.state, false), "#overview", "")
			m := navOfAlerts.FindStringSubmatch(overview)
			if m == nil || m[1] != c.red || m[2] != c.count {
				t.Fatalf("the nav item is %q, want %q %q", m, c.red, c.count)
			}
			mustHoldInOrder(t, "the nav", overview, navLink("#notchecked"), navLink("#alerts"))
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
		alertBar+`b">One message when a finding opens, and one when it clears. `+span("A lower level overrides a higher one. A mute always has an end date. Messages hold no values.")+`</div>`,
		rulesHead+`7</span></h2>`,
		`<th>State</th><th>Level</th><th>Applies to</th><th>News</th><th>Send</th><th>Where</th>`,
		`<h2>Sample message</h2>`)
	mustNotHold(t, "the screen", screen, "A sketch for later", notSet)
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
	mustHold(t, "the sample message", screen, `<div class="pre mt0">`+testkit.QuarryDrift)
}

// The screen of an observer that started without --alerts is an offer: it says that no rule is set, what a message
// is, and the next step with its command. It shows no table and no sample.
func TestWithNoAlertsFlagTheScreenSaysTheSendIsOptIn(t *testing.T) {
	for _, state := range []devdata.State{devdata.StateOK, devdata.StateAlertsUnset} {
		t.Run(string(state), func(t *testing.T) {
			screen := pageOf(t, render(t, state, false), "#alerts", "")
			checkPlace(t, placeNamed(t, "alerts not set"), screen)
			mustHold(t, "the screen", screen, rulesHead+notSet+`</span></h2>`, "The rules are a file in Git, so a mute always has a name and an end date.")
			mustNotHold(t, "the screen", screen, "<table>", "Sample message", "Firing")
		})
	}
}

// A mute that ended reads "Mute ended" and Inactive, and a message that waits for a retry shows an amber
// bar. The log names the channel and the status, and the bar says so.
func TestAMuteThatEndedAndAMessageThatWaitsShowTheirWords(t *testing.T) {
	dir := writeSet(t, devdata.StateAlerts, map[string]func([]byte) []byte{
		"alerts.json": editJSON(t, func(v *alert.View) {
			v.Waiting = 2
			v.Rules[5].State = alert.StateInactive
			v.Rules[5].MuteUntil = "2026-10-01"
		}),
	})
	screen := pagesOf(t, dir, "#alerts")[0]
	mustHold(t, "the screen", screen, amberBar+`2 messages wait for a retry. `+span("The log names the channel and the status. It never holds the address.")+`</div>`,
		`<td>Mute ended 2026-10-01</td>`)
	mustNotHold(t, "the screen", screen, "Muted until 2026-10-01")
	one := writeSet(t, devdata.StateAlerts, map[string]func([]byte) []byte{
		"alerts.json": editJSON(t, func(v *alert.View) {
			v.Waiting = 1
		}),
	})
	mustHold(t, "the screen", pagesOf(t, one, "#alerts")[0], "1 message waits for a retry.")
}

// A digest period other than a week reads as the period.
func TestADigestPeriodOtherThanAWeekReadsAsThePeriod(t *testing.T) {
	dir := writeSet(t, devdata.StateAlerts, map[string]func([]byte) []byte{
		"alerts.json": editJSON(t, func(v *alert.View) {
			v.DigestEvery = "24h"
		}),
	})
	screen := pagesOf(t, dir, "#alerts")[0]
	mustHold(t, "the screen", screen, "<td>Digest every 24h</td>")
	mustNotHold(t, "the screen", screen, "Weekly digest")
}

// The names of a rule and of a channel come from a file of a team, so the screen shows them as text.
func TestTheNamesOfTheRulesShowAsTextOnTheScreenOfTheAlerts(t *testing.T) {
	screen := pageOf(t, render(t, devdata.StateAlerts, true), "#alerts", "")
	mustNotHold(t, "the screen", screen, "<img")
	mustHold(t, "the screen", screen, injection)
}
