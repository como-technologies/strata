package web

import (
	"testing"

	"github.com/como-technologies/strata/observer/internal/baseline"
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

// The markup and the scripts of the menus of the cycles that more than one test reads. cycleMenu starts the menu of a
// cycle, and chipSummary follows its name. openContent opens the menu of the content cycle, and checkContent also clicks
// its button. nowButton starts the button of a cycle in its menu, and contentNow ends the button of the content cycle.
// contentButton is that button, contentOff is the button when it is off, and checkingVersions is the pill of the
// versions while they are read.
const (
	cycleMenu        = `<details class="cyclemenu" data-open="cycle-`
	chipSummary      = `"><summary class="chip"`
	openContent      = `S.open["cycle-content"] = true`
	checkContent     = openContent + `; checkNow("content")`
	nowButton        = `<button class="btn" data-now="`
	contentNow       = "Check the content now</button>"
	contentButton    = nowButton + `content">` + contentNow
	contentOff       = nowButton + `content" disabled>` + contentNow
	checkingVersions = "Versions <b>checking</b>"
	panelOfVersions  = "the panel of versions"
	recordsIt        = "the observer records it"
)

// The pill of each cycle is a menu. A closed menu draws no panel, so the words of the panel are on no
// screen until a person opens it.
func TestThePillOfEachCycleIsAMenuThatDrawsNoPanelWhileClosed(t *testing.T) {
	page := shown(t, devdata.StateOK, "#overview")[0]
	for _, name := range []string{"content", "versions"} {
		mustHold(t, name, page, cycleMenu+name+chipSummary)
	}
	mustHold(t, "the caret", page, `<span class="caret" aria-hidden="true"></span></summary></details>`)
	mustNotHold(t, "a closed menu", page, `class="cpanel"`)
}

// An open menu shows the last good end, the period, the next cycle, what the cycle does, and the button
// that starts it now.
func TestAnOpenMenuShowsTheTimerOfItsCycleAndTheButtonThatStartsIt(t *testing.T) {
	page := overviewWith(t, openContent)
	mustHold(t, "the open menu", page, cycleMenu+`content" open>`)
	for _, want := range []string{labelStart("Last good end"), labelRow("Every", "15m"), labelStart("Next") + "about ",
		"Compares the records with the cluster and with Palette.", contentButton} {
		mustHold(t, "the panel of content", page, want)
	}
	mustNotHold(t, "one panel", page, `data-now="versions"`)
	versions := overviewWith(t, `S.open["cycle-versions"] = true`)
	mustHold(t, panelOfVersions, versions, nowButton+`versions">Read the versions now</button>`)
	mustHold(t, panelOfVersions, versions, "Reads the registries for newer versions of each pack.")
}

// While a cycle runs, the pill shows its progress and the button is off, with the reason.
func TestTheButtonIsOffWhileItsCycleRuns(t *testing.T) {
	page := overviewWith(t, runningJS("content", "", 3, 12)+" "+openContent)
	mustHold(t, "the pill", page, `<summary class="chip run" title="Started`)
	mustHold(t, "the pill", page, `Content <b>checking 3 of 12</b>`)
	mustHold(t, "the panel", page, labelRow("Now", "checking 3 of 12"))
	mustHold(t, "the panel", page, contentOff+`<span class="sub">A cycle runs now. The pill shows its progress.</span>`)
	mustNotHold(t, "a running cycle", page, `<span>Next</span>`)
}

// A click asks once, also when the person clicks twice, and the pill shows the cycle at once.
func TestAClickOnTheButtonSendsOnePostAndThePillShowsTheCycleAtOnce(t *testing.T) {
	got := click(t, devdata.StateOK, "#overview", "0", `{"now":"versions"}`)
	if len(got.Posts) != 1 || got.Posts[0].Path != "/v1/cycles/versions" {
		t.Fatalf("posts %+v", got.Posts)
	}
	mustHold(t, "before", got.Before, `data-open="cycle-versions`+chipSummary+` title=`)
	mustHold(t, "while the request is on its way", got.During, checkingVersions)
	mustHold(t, "after the answer", got.After, checkingVersions)
}

// A cycle that ended less than a minute ago keeps the button off for the seconds that the observer
// names. A request that fails says so, and the button stays on.
func TestTheButtonWaitsTheSecondsOfA429AndSaysWhenARequestFailed(t *testing.T) {
	wait := overviewWith(t, failPost(`{ok: false, status: 429, json: async () => ({cycle: "content", state: "wait", retryAfter: 42})}`)+
		checkContent)
	mustHold(t, "a wait", wait, contentOff+`<span class="sub">A cycle ended less than a minute ago. Try again in 42 s.</span>`)
	mustHold(t, "a wait", wait, `Content <b>`)
	mustNotHold(t, "a wait", wait, `Content <b>checking`)

	failed := overviewWith(t, failPost(`{ok: false, status: 500, json: async () => ({error: "internal server error"})}`)+
		checkContent)
	mustHold(t, "a failed request", failed, contentButton+`<span class="sub">The observer did not take the request. Try again.</span>`)
}

// A profile version with no record says what adds its record. With first-seen records the observer
// records it at its next content check. With approved records only the pipeline can, and a check
// cannot: the row says so.
func TestARowOfNoRecordSaysWhatAddsTheRecordForEachBaseline(t *testing.T) {
	row := `D.notChecked = [{cause: "no-record", profiles: ["fred-edge@1.0.0"], packs: [], registry: ""}];`
	firstSeen := pageWith(t, "testdata/api", devdata.StateOK, "#notchecked", row+`D.cluster.records = {baseline: "`+baseline.FirstSeen+`", firstSeenFrom: "c1"}`)
	mustHold(t, "first-seen", firstSeen, "No record holds this profile version yet. The records are first-seen, so "+recordsIt+" at its next content check.")
	approved := pageWith(t, "testdata/api", devdata.StateOK, "#notchecked", row)
	mustHold(t, "approved", approved, "No record holds this profile version, so strata cannot say that anything changed. Your pipeline must publish its record: a check cannot add one.")
	mustNotHold(t, "approved", approved, recordsIt)
}
