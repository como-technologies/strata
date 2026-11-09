package web

import (
	"testing"

	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

// overviewWith shows the overview of the set ok after the code ran in the page.
func overviewWith(t *testing.T, code string) string {
	t.Helper()
	return pageWith(t, "testdata/api", devdata.StateOK, "#overview", code)
}

// failPost makes the next POST of the page get the answer that answer gives, and each GET the answer of
// the set.
func failPost(answer string) string {
	return `const pass = fetch; fetch = async (u, o) => o && o.method === "POST" ? (` + answer + `) : pass(u, o);`
}

// The pill of each cycle is a menu. A closed menu draws no panel, so the words of the panel are on no
// screen until a person opens it.
func TestThePillOfEachCycleIsAMenuThatDrawsNoPanelWhileClosed(t *testing.T) {
	page := shown(t, devdata.StateOK, "#overview")[0]
	for _, name := range []string{"content", "versions"} {
		mustHold(t, name, page, `<details class="cyclemenu" data-open="cycle-`+name+`"><summary class="chip"`)
	}
	mustHold(t, "the caret", page, `<span class="caret" aria-hidden="true"></span></summary></details>`)
	mustNotHold(t, "a closed menu", page, `class="cpanel"`)
}

// An open menu shows the last good end, the period, the next cycle, what the cycle does, and the button
// that starts it now.
func TestAnOpenMenuShowsTheTimerOfItsCycleAndTheButtonThatStartsIt(t *testing.T) {
	page := overviewWith(t, `S.open["cycle-content"] = true`)
	mustHold(t, "the open menu", page, `<details class="cyclemenu" data-open="cycle-content" open>`)
	for _, want := range []string{`<div><span>Last good end</span>`, `<div><span>Every</span>15m</div>`, `<div><span>Next</span>about `,
		"Compares the records with the cluster and with Palette.", `<button class="btn" data-now="content">Check the content now</button>`} {
		mustHold(t, "the panel of content", page, want)
	}
	mustNotHold(t, "one panel", page, `data-now="versions"`)
	versions := overviewWith(t, `S.open["cycle-versions"] = true`)
	mustHold(t, "the panel of versions", versions, `<button class="btn" data-now="versions">Read the versions now</button>`)
	mustHold(t, "the panel of versions", versions, "Reads the registries for newer versions of each pack.")
}

// While a cycle runs, the pill shows its progress and the button is off, with the reason.
func TestTheButtonIsOffWhileItsCycleRuns(t *testing.T) {
	page := overviewWith(t, `D.cluster.content.running = {startedAt: new Date().toISOString(), done: 3, total: 12}; S.open["cycle-content"] = true`)
	mustHold(t, "the pill", page, `<summary class="chip run" title="Started`)
	mustHold(t, "the pill", page, `Content <b>checking 3 of 12</b>`)
	mustHold(t, "the panel", page, `<div><span>Now</span>checking 3 of 12</div>`)
	mustHold(t, "the panel", page, `<button class="btn" data-now="content" disabled>Check the content now</button><span class="sub">A cycle runs now. The pill shows its progress.</span>`)
	mustNotHold(t, "a running cycle", page, `<span>Next</span>`)
}

// A click asks once, also when the person clicks twice, and the pill shows the cycle at once.
func TestAClickOnTheButtonSendsOnePostAndThePillShowsTheCycleAtOnce(t *testing.T) {
	got := click(t, devdata.StateOK, "#overview", "0", `{"now":"versions"}`)
	if len(got.Posts) != 1 || got.Posts[0].Path != "/v1/cycles/versions" {
		t.Fatalf("posts %+v", got.Posts)
	}
	mustHold(t, "before", got.Before, `data-open="cycle-versions"><summary class="chip" title=`)
	mustHold(t, "while the request is on its way", got.During, `Versions <b>checking</b>`)
	mustHold(t, "after the answer", got.After, `Versions <b>checking</b>`)
}

// A cycle that ended less than a minute ago keeps the button off for the seconds that the observer
// names. A request that fails says so, and the button stays on.
func TestTheButtonWaitsTheSecondsOfA429AndSaysWhenARequestFailed(t *testing.T) {
	wait := overviewWith(t, failPost(`{ok: false, status: 429, json: async () => ({cycle: "content", state: "wait", retryAfter: 42})}`)+
		`S.open["cycle-content"] = true; checkNow("content")`)
	mustHold(t, "a wait", wait, `<button class="btn" data-now="content" disabled>Check the content now</button><span class="sub">A cycle ended less than a minute ago. Try again in 42 s.</span>`)
	mustHold(t, "a wait", wait, `Content <b>`)
	mustNotHold(t, "a wait", wait, `Content <b>checking`)

	failed := overviewWith(t, failPost(`{ok: false, status: 500, json: async () => ({error: "internal server error"})}`)+
		`S.open["cycle-content"] = true; checkNow("content")`)
	mustHold(t, "a failed request", failed, `<button class="btn" data-now="content">Check the content now</button><span class="sub">The observer did not take the request. Try again.</span>`)
}
