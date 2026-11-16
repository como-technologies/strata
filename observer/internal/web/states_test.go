package web

import (
	"fmt"
	htmlpkg "html"
	"net/url"
	"regexp"
	"slices"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
	"github.com/como-technologies/strata/observer/internal/testkit"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

/* The tests below hold the splash on each screen. They run the scripts with node, as the tests of web_test.go do. Each place that
had one gray line has a state with a tone, a title, a fact, and the next step when one exists. */

// crawls holds the crawl of each set, so that the tests of the states run node once for each set.
var crawls = map[devdata.State][]screen{}

func crawl(t *testing.T, state devdata.State) []screen {
	t.Helper()
	if s, ok := crawls[state]; ok {
		return s
	}
	s := render(t, state, false)
	crawls[state] = s
	return s
}

// shown shows some screens of a set of testdata/api, with no crawl.
func shown(t *testing.T, state devdata.State, hashes ...string) []string {
	t.Helper()
	return showIn(t, "testdata/api", state, hashes...)
}

// pageWith runs the code in the page, and then it shows the screen.
func pageWith(t *testing.T, apiDir string, state devdata.State, hash, code string) string {
	t.Helper()
	return nodeJSON[string](t, runNodeIn(t, apiDir, state, "with", hash, code))
}

// packPage is the hash of the page of a pack.
func packPage(subject, layer, name string) string {
	return "#k-" + encodeURIComponent(subject) + "/" + encodeURIComponent(layer) + "/" + encodeURIComponent(name)
}

// place is one place of the page that had a gray line, with the words that its state must show. The size is lg for the
// screen, md for a card, sm for a card inside a card, and inl for a line.
type place struct {
	name        string
	screen      func(t *testing.T) string
	tone, size  string
	title       string
	fact, steps []string
}

// splashClass gives the class of a splash of the size and the tone, and splashOpen starts it, before the size.
func splashClass(size, tone string) string { return splashOpen + size + ` tone-` + tone + `"` }

const splashOpen = `class="splash `

// markClass gives the class of the mark of the tone, and markOpen starts it, before the tone.
func markClass(tone string) string { return markOpen + tone + `"` }

const markOpen = `class="slabs tone-`

// bookNext gives the next step of a place that ends with the section of The observer in the book. step holds the first
// words of the step.
func bookNext(step, section string) string {
	return nextOpen + step + " The book of strata tells how, in The observer: " + section + ".</div>"
}

// retryButton gives the button of a place that asks again for what, such as the page or a diff.
func retryButton(what string) string {
	return buttonOpen + `class="btn" data-retry="` + what + `">Try now</button>`
}

// span gives a span of the text with no class, such as the label of a row or of an item of the nav.
func span(text string) string { return "<span>" + text + "</span>" }

// labelStart starts a row of a header or of a panel with its label.
func labelStart(label string) string { return "<div>" + span(label) }

// labelRow gives a row of a header or of a panel with its label and its value.
func labelRow(label, value string) string { return labelStart(label) + value + "</div>" }

// upstreamRow gives the row of the upstream digest of a pack.
func upstreamRow(digest string) string {
	return labelRow("Upstream", `<span class="m bright">`+digest+`</span>`)
}

// checkPlace fails when the screen lacks the mark of the tone, the title, the fact, or the next step of the place.
func checkPlace(t *testing.T, p place, html string) {
	t.Helper()
	mark := "<svg " + markClass(p.tone) + ` viewBox="0 0 100 100" width="\d+" height="\d+" aria-hidden="true">` + strings.Repeat(`<path class="s s\d" d="M[^"]+"/>`, 4) + `</svg>`
	var head string
	if p.size == "inl" {
		head = regexp.QuoteMeta(`<div class="inl tone-`+p.tone+`">`) + mark
	} else {
		head = regexp.QuoteMeta("<div "+splashClass(p.size, p.tone)+">") + mark + regexp.QuoteMeta(`<h3>`+p.title+`</h3><p>`)
	}
	if !regexp.MustCompile(head).MatchString(html) {
		t.Errorf("the screen has no %s splash of the tone %s with the title %q: %.600s", p.size, p.tone, p.title, html)
	}
	mustHold(t, p.name, html, p.fact...)
	mustHold(t, p.name, html, p.steps...)
}

// The words of the places that more than one test reads: the titles, the facts, the next steps, and the commands.
const (
	kubeNS         = "kubectl -n strata-observer"
	kubeLogs       = kubeNS + " logs"
	logCommand     = "<code>" + kubeLogs + " deploy/strata-observer --tail=20</code>"
	keySecret      = "strata-observer-key"
	noRecordsTitle = "Nothing to compare against yet"
	noRecordsFact  = "A record is the copy of a profile that the observer checks against. Start with what the cluster runs now, and let your pipeline approve records later."
	firstSeenCmd   = testkit.RecordsTool + " --first-seen " + testkit.ClusterUID
	applyEnd       = " | kubectl apply --server-side -f -</code>"
	// nextOpen opens the next step of a place, and nextWord is its first word. nextStep starts the next step, and readTheLog
	// ends a next step that sends a person to the log.
	nextOpen       = `<div class="next">`
	nextWord       = "<b>Next</b> "
	nextStep       = nextOpen + nextWord
	readTheLog     = ", then read the log."
	lostTitle      = "We lost the signal from Palette"
	noGoodCheck    = "No check has ended well"
	keyRefused     = "HTTP 401, the API key was refused"
	lostFact       = "The last 3 checks failed: <b>" + keyRefused + ".</b> " + noGoodCheck
	noClear        = "no finding clears until a check proves it"
	firstCheck     = "The first check is running"
	noApproval     = "No pipeline approved that content."
	alertsOffFact  = "This install has no alert rules."
	alertsNext     = nextWord + "Turn on the alerts when you install the observer."
	podsOffNext    = nextWord + "Turn on the read of the pods when you install the observer: with the chart, set readPods to true and upgrade."
	podsUnread     = "the read of the pods failed"
	stoppedTitle   = "Checks stopped before one ended well"
	offlineTitle   = "This page lost its line to the observer"
	quietTitle     = calmQuiet + " on quarry-1"
	busyTitle      = "One moment, a check is running"
	tooLargeTitle  = "Too much rock to carry"
	tooLargeNote   = "This text is too large to keep here."
	tooLargeFact   = tooLargeNote + " The observer keeps at most 256 KiB for each side. A terminal shows all of it."
	podsOffTitle   = "Pods are not on the menu here"
	podsOffFact    = "This install does not read the pods."
	podsTitle      = "The pods stay silent"
	askListPods    = "Ask the API server if the role of the observer can list pods."
	canListPods    = "<code>kubectl auth can-i list pods --all-namespaces --as=system:serviceaccount:strata-observer:strata-observer</code>"
	alertsOffTitle = "Nobody gets paged yet"
	hiddenTitle    = "Part of this layer is hidden"
	sameFact       = "The published version holds what the record holds."
	dexCompare     = compareRecord + testkit.DexProfile + " " + runner.FlagFull
	dexFullCompare = "<code>" + dexCompare + "</code>"
	checkTheKey    = "Check the key in the Secret"
	reachPalette   = "Check that the observer can reach Palette"
	checkTheLink   = reachPalette + readTheLog
)

// The markup of the page with a span that the tests of more than one file read. publishedNow and clusterCopy name the
// published side and the side of the cluster in the card of a pack.
var (
	publishedNow   = span("Published now")
	clusterCopy    = span("This cluster&#39;s copy")
	newerSpan      = span("Newer version")
	samePublished  = sameBadge + publishedNow
	sameCopy       = sameBadge + clusterCopy
	noNewerVersion = `<b class="ok">Current</b>` + span("No newer version")
	newerFact      = span("newer versions")
)

// The words and the markup of the page that the tests of more than one file read.
const (
	sameBadge   = `<b class="ok">Same</b>`
	coverJump   = "What a check does not cover"
	lostContact = "Lost contact with Palette"
	repaveHint  = "is a hint from strata"
	// alertBar starts the bar of an alert, before the letter of its tone. amberBar starts an amber bar.
	alertBar = `<div class="alertbar `
	amberBar = alertBar + `a">`
	// buildWords starts the words of a pack whose build changed and whose content stays the same.
	buildWords  = "Only the build of the pack"
	buttonOpen  = `<button type="button" `
	linkButton  = buttonOpen + `class="lnk" `
	changedHere = " changed here."
	refSpan     = `<span class="m ref">`
	flagWrap    = `<span class="flagw">`
	// checkCard is the head of the card "Check this yourself", qaRow starts a row of it, and codeOf finds a command.
	checkCard = "<h2>Check this yourself</h2>"
	qaRow     = `<li><div class="qa">`
	codeOf    = "<code>([^<]*)</code>"
	// recordsSecret starts the name of each Secret of the records.
	recordsSecret = "strata-observer-records-"
	// matchesRecord names a pack that matches its record, in the names of the cases.
	matchesRecord = "a pack that matches its record"
	// newestOption is the choice of the newest version in the list of a compare, and newestLabel its words.
	newestOption   = `<option value="` + string(model.SideNewest) + `"`
	newestSelected = newestOption + " selected>"
	newestLabel    = "Record → Newest version"
	// comparePacks starts the command that compares two packs. cardHead names the head of a card in a check.
	comparePacks = "strata compare pack:"
	cardHead     = "the head of the card"
	// gaveArgs is the failure of a command of the page that gave other arguments to strata.
	gaveArgs = "the command %q gave the arguments %q, want %q"
)

// places are the places of the issue. They are the first check, no records, lost with nothing read, a page that cannot reach the
// observer, and a route that does not exist. They are also no findings, no newer versions, nothing unchecked, the pods off, the read
// of the pods failed, and alerts not set. They are also a pack that matches its record, a pack not checked, a diff that a cycle
// holds, and a text too large. The last three are an empty tab, a filter with no match, and no events.
//
// The page also has a place for a first read that the observer has not ended. It also has a place for the stop of the checks
// before a good one. A third place is for the read of the pods that is not done yet.
var places = []place{
	{placeFirstCheck, func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateFirstLoad), "#overview", "") }, "build", "lg", "Reading the layers",
		[]string{firstCheck + ", and this page fills in when it ends. The observer reads each record, then asks Palette for the same version."}, nil},
	{placeNoRecords, func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateNoRecords), "#overview", "") }, "off", "lg", noRecordsTitle,
		[]string{noRecordsFact},
		[]string{`<p class="sub"><b>Start now.</b> An install with the chart runs a job that records what each profile of this cluster holds now. Its log says what it did.</p>`,
			"<code>" + kubeLogs + " -l app.kubernetes.io/name=strata-observer-seed --tail=20</code>",
			`<p class="sub">With no chart, this command records the same. ` + noApproval + `</p>`,
			"<code>" + firstSeenCmd + applyEnd,
			`<p class="sub"><b>From your pipeline.</b> Publish the records that a review approved.</p>`,
			"<code>" + testkit.RecordsTool + " --revision &quot;$GIT_SHA&quot; releases/*.json" + applyEnd}},
	{placeLostUnread, func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateLostUnchecked), "#overview", "") }, "lost", "lg", lostTitle,
		[]string{lostFact + ", and " + noClear + "."},
		[]string{nextStep + checkTheKey + ` <span class="m">` + keySecret + `</span>` + readTheLog + `</div>`, logCommand}},
	{"checks stopped with nothing read", func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateStaleUnchecked), "#overview", "") }, "warn", "lg", stoppedTitle,
		[]string{"The content check runs every 15m. " + noGoodCheck + ", so this page has nothing to show yet."}, []string{nextStep + `Read the log.</div>`, logCommand}},
	{"the page " + cannotReach, func(t *testing.T) string {
		before, _ := offline(t)
		return before
	}, "lost", "lg", offlineTitle,
		[]string{"The port-forward may have ended, or the pod may have restarted. This page asks again each 30 seconds."},
		[]string{"<code>" + kubeNS + " port-forward deploy/strata-observer 18080:8080</code>", retryButton("page")}},
	{"a route that does not exist", func(t *testing.T) string { return shown(t, devdata.StateOK, "#p-old-profile@1.0.0")[0] }, "off", "lg", "This profile is not here",
		[]string{`<span class="m">old-profile@1.0.0</span> is in no record now. A new revision of the records can remove it.`}, []string{`<a class="btn" href="#profiles">All profiles</a>`}},
	{placeNoFindings, func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateQuiet), "#findings", "") }, "calm", "md", quietTitle,
		[]string{calmNothing + ". <b>29 of 29 packs</b> were checked in full, ", " ago."},
		[]string{span("profiles match their records"), span("not checked"), newerFact}},
	{"no newer versions", func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateQuiet), "#newer", "") }, "calm", "md", "Nothing newer to dig up",
		[]string{calmCurrent + ". The registries list nothing newer than what you pin. Read ", " ago, every 6h."}, nil},
	{"nothing unchecked", func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateQuiet), "#notchecked", "") }, "calm", "md", "Every layer was read",
		[]string{"Each pack was checked in full. Strata read each record, and each answer came back. A check still does not cover the items below."},
		[]string{linkButton + `data-jump="coverage">` + coverJump + `</button>`}},
	{placePodsOff, func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateImagesOff), "#images", "") }, "off", "md", podsOffTitle,
		[]string{podsOffFact + " With the read on, the page shows the digest that each tag runs, and finds a tag that moved."},
		[]string{bookNext(podsOffNext, "The pods of the cluster")}},
	{podsUnread, func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateImagesUnread), "#images", "") }, "warn", "md", podsTitle,
		[]string{podsRefusedWords + " The findings of the content check are not affected."},
		[]string{askListPods, canListPods}},
	{placeAlertsOff, func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateAlertsUnset), "#alerts", "") }, "off", "md", alertsOffTitle,
		[]string{alertsOffFact + " A message goes to a webhook when a finding opens and when it clears."},
		[]string{bookNext(alertsNext, "Alerts")}},
	{matchesRecord, func(t *testing.T) string {
		return pageOf(t, crawl(t, devdata.StateQuiet), dexHash, "")
	}, "same", "md", "Undisturbed",
		[]string{sameFact},
		[]string{samePublished, sameCopy, noNewerVersion}},
	{matchesRecord + ", on a page with findings", func(t *testing.T) string {
		return pageOf(t, crawl(t, devdata.StateOK), reloaderHash, "")
	}, "same", "md", "Undisturbed", []string{sameFact},
		[]string{samePublished, noNewerVersion}},
	{"a pack that was not checked", func(t *testing.T) string {
		return pageOf(t, crawl(t, devdata.StateOK), "#k-"+testkit.GazooProfile+"/"+testkit.VeleroKey, "")
	}, "warn", "md", hiddenTitle,
		[]string{"This pack was not fully checked. velero has no upstream digest. The content check runs every 15m."}, []string{nextStep + `<a href="#notchecked">Why, on the ` + wordNotChecked + ` page</a></div>`}},
	{placeBusy, func(t *testing.T) string { return retried(t, dexHash)[0] }, "build", "sm", busyTitle, []string{"The observer answers one read at a time. This page asks again each 30 seconds."},
		[]string{retryButton("diff")}},
	{placeTooLarge, func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateTextTooLarge), dexHash, "") }, "off", "sm", tooLargeTitle,
		[]string{tooLargeFact},
		[]string{dexFullCompare}},
	{"an empty tab", func(t *testing.T) string {
		return pageWith(t, "testdata/api", devdata.StateOK, dexHash, `S.tabs["`+dexPublished+`"] = "images"`)
	}, "same", "inl", "",
		[]string{span("No image" + changedHere), tabLink(dexPublished, "values", "Values")}, nil},
	{"a filter with no match", func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateOK), "#profiles", "search none") }, "off", "inl", "",
		[]string{span(`No profile matches <span class="m">no profile has this name</span>.`), linkButton + `data-clear="filter">Clear the filter</button>`}, nil},
	{"no events", func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateQuiet), "#overview", "") }, "calm", "inl", "",
		[]string{span(calmQuiet + ": no events in the last 7 days.")}, nil},
}

// The names of the places that other tests check with placeNamed.
const (
	placeFirstCheck = "the first check"
	placeNoRecords  = "no records"
	placeLostUnread = "lost with nothing read"
	placeNoFindings = "no findings"
	placeAlertsOff  = "alerts not set"
	placePodsOff    = "the pods off"
	placeBusy       = "a diff that a cycle holds"
	placeTooLarge   = "a text too large"
)

// placeNamed gives the place of the table places with the name.
func placeNamed(t *testing.T, name string) place {
	t.Helper()
	for _, p := range places {
		if p.name == name {
			return p
		}
	}
	t.Fatalf("no place is named %q", name)
	return place{}
}

// Each place of the table places has its tone, its title, its fact, and its next step.
func TestEachPlaceHasItsToneItsTitleItsFactAndItsNextStep(t *testing.T) {
	for _, p := range places {
		t.Run(p.name, func(t *testing.T) { checkPlace(t, p, p.screen(t)) })
	}
}

// Each of the five tones is drawn by a place above: calm breathes, build stacks, off marches, warn pulses, and lost slips.
func TestEachToneIsDrawnByAPlaceWithItsMark(t *testing.T) {
	for tone, name := range map[string]string{"calm": placeNoFindings, "build": placeFirstCheck, "off": placeNoRecords, "warn": podsUnread, "lost": placeLostUnread} {
		p := placeNamed(t, name)
		if p.tone != tone {
			t.Errorf("the place %q has the tone %s, want %s", name, p.tone, tone)
		}
		mustHold(t, tone, p.screen(t), markClass(tone), splashOpen)
	}
}

// A click on "Try now" of a page that cannot reach the observer asks again, and the page shows its data when the observer is back.
func TestTryNowAsksTheObserverAgainAndTheDataComesBack(t *testing.T) {
	before, after := offline(t)
	mustHold(t, "before", before, offlineTitle)
	mustNotHold(t, "before", before, needsAttention, "<table")
	mustHold(t, "after", after, needsAttention)
	mustNotHold(t, "after", after, offlineTitle)
}

// A click on "Clear the filter" shows each profile again. A click on Copy sends the text of the command to the clipboard.
func TestClearTheFilterAndCopyDoWhatTheyName(t *testing.T) {
	const noMatch = "No profile has this status."
	r := click(t, devdata.StateQuiet, "#profiles", "0", `{"filter":"drift"}`)
	mustHold(t, "the empty filter", r.After, noMatch, `data-clear="filter"`)
	r = click(t, devdata.StateQuiet, "#profiles", "0", `{"filter":"drift"}`, `{"clear":"filter"}`)
	mustNotHold(t, "after Clear", r.After, noMatch)
	mustHold(t, "after Clear", r.After, linkRow(fredHash))
	r = click(t, devdata.StateOK, fredHash, "0", `{"copy":"`+verifyRecord+`"}`)
	if len(r.Copies) == 0 || r.Copies[0] != verifyRecord {
		t.Errorf("a click on Copy sent %q to the clipboard", r.Copies)
	}
}

// ---- The rule of a state ----

// calmScreens lists the screens of a crawl that show a calm tone.
func calmScreens(screens []screen) []string {
	var out []string
	for _, s := range screens {
		if strings.Contains(s.HTML, "tone-calm") {
			out = append(out, strings.TrimSpace(s.Hash+" "+s.Variant))
		}
	}
	return out
}

// blockers says why a set may not show a calm tone: a finding, a failed read, a cause in Not checked, or a page that is not ok.
func blockers(set *devdata.Set) []string {
	var out []string
	if set.Cluster.Counts.Findings > 0 || len(set.Findings.Findings) > 0 {
		out = append(out, "a finding")
	}
	if set.Cluster.Counts.NotChecked > 0 || len(set.NotChecked.NotChecked) > 0 {
		out = append(out, "a cause in "+wordNotChecked)
	}
	if set.Cluster.Content.FailedRuns > 0 || set.Cluster.Versions.FailedRuns > 0 || set.Images.Error != nil {
		out = append(out, "a failed read")
	}
	if set.Cluster.Page != model.PageOK {
		out = append(out, "a page that is "+string(set.Cluster.Page))
	}
	return out
}

// A page with a finding, a failed read, or a cause in Not checked shows no calm tone on any screen. Only the set quiet has a good
// check, and it shows the calm tone.
func TestAPageWithAFindingAFailedReadOrACauseShowsNoCalmTone(t *testing.T) {
	var clean []devdata.State
	for _, state := range devdata.States {
		if len(blockers(loadSet(t, state))) == 0 {
			clean = append(clean, state)
		}
	}
	if !slices.Equal(clean, []devdata.State{devdata.StateQuiet}) {
		t.Fatalf("the sets with a good check are %v, want the set quiet alone", clean)
	}
	for _, state := range devdata.States {
		t.Run(string(state), func(t *testing.T) {
			set, calm := loadSet(t, state), calmScreens(crawl(t, state))
			if why := blockers(set); len(why) > 0 {
				if len(calm) > 0 {
					t.Errorf("a page with %s shows a calm tone on %q", strings.Join(why, " and "), calm)
				}
				return
			}
			if len(calm) == 0 {
				t.Errorf("the set %s has a good check and shows no calm tone", state)
			}
		})
	}
}

// The test above fails without the rule. A copy of the scripts whose gate of the calm tone always says yes shows a calm tone on
// the set ok with no events. That set holds a finding and two causes. The card of the events on that set says "All quiet". A
// place that one check found the same takes the tone same, which is no claim about the page. So the set ok needs the empty list
// of events.
func TestTheRuleOfACalmToneIsWhatKeepsItOffAPageWithAFinding(t *testing.T) {
	gate := regexp.MustCompile(`(?m)^const calmOk = .*$`)
	if got := len(gate.FindAllString(readScripts(t), -1)); got != 1 {
		t.Fatalf("the scripts have %d lines that start with const calmOk, want 1", got)
	}
	dir := copyScripts(t, func(script string) string { return gate.ReplaceAllString(script, "const calmOk = () => true;") })
	quiet := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"events.json": wholeFile(`{"events":[]}`)})
	screens := renderIn(t, dir, quiet, scratch)
	if got := calmScreens(screens); len(got) == 0 {
		t.Error("without the rule, the page with a finding still shows no calm tone: the test cannot fail")
	}
	real := renderIn(t, "assets", quiet, scratch)
	if got := calmScreens(real); len(got) != 0 {
		t.Errorf("the real scripts show a calm tone on %q", got)
	}
}

// A page that is lost or stale with findings keeps the findings under the bar. It shows no splash of the size of the screen.
func TestLostAndStaleWithFindingsStayABarOverTheFindings(t *testing.T) {
	for _, c := range []struct {
		state devdata.State
		bar   string
	}{
		{devdata.StateLost, alertBar + `r">` + lostContact + `.`},
		{devdata.StateStale, amberBar + `Checks stopped.`},
	} {
		t.Run(string(c.state), func(t *testing.T) {
			screens := crawl(t, c.state)
			for _, hash := range []string{"#overview", "#findings"} {
				page := pageOf(t, screens, hash, "")
				mustHoldInOrder(t, hash, page, c.bar, publishedChanged)
				mustHold(t, hash, page, linkRow(dexHash))
				mustNotHold(t, hash, page, splashOpen+"lg")
			}
		})
	}
}

// ---- One Compare dropdown on each pack page ----

var optionPattern = regexp.MustCompile(`<option value="([a-z]+)" (selected)?>([^<]*)</option>`)

// expectedSides reads the data a second time, in Go, and gives the sides that a pack page offers, in order, and the sides
// that hold a change. The page opens on the first side with a change, and on the published side when none has one.
func expectedSides(set *devdata.Set, x api.ProfileView, k model.Pack) (options []string, labels map[string]string, changed map[string]bool, open string) {
	subject := model.Subject(x.Name, x.Version)
	labels = map[string]string{string(model.SidePublished): "Record → Published now", string(model.SideCluster): "Record → This cluster's copy"}
	options = []string{string(model.SidePublished)}
	if c := x.ClusterCopy; c != nil && (c.Status == model.CopySame || c.Status == model.CopySameContent || c.Status == model.CopyDiffers) {
		options = append(options, string(model.SideCluster))
	}
	var row *model.Newer
	for i, r := range set.Newer.Newer {
		if r.Name == k.Name && r.Version == k.Version && r.Registry == k.Registry && slices.Contains(r.Profiles, subject) {
			row = &set.Newer.Newer[i]
		}
	}
	if row != nil {
		options = append(options, string(model.SideNewer))
		labels[string(model.SideNewer)] = fmt.Sprintf("%s@%s → %s@%s", row.Name, row.Version, row.Name, row.Newest)
	}
	changed = map[string]bool{string(model.SideNewer): row != nil}
	var ofRecord int
	for _, f := range x.Findings {
		if !slices.Contains(f.Packs, k.Key) || f.Kind.IsImage() {
			continue
		}
		ofRecord++
		if f.Kind == model.ClusterDiffers || f.Kind == model.ImageIDChanged {
			changed[string(model.SideCluster)] = true
		} else {
			changed[string(model.SidePublished)] = true
		}
		if f.AlsoOnCluster {
			changed[string(model.SideCluster)] = true
		}
	}
	if k.Status == model.StatusDrift && ofRecord == 0 {
		changed[string(model.SidePublished)] = true
	}
	// issue #708: the record against the newest version of its profile, when strata compared the two
	if n := x.NewerVersion; n != nil && n.Move != nil {
		options = append(options, string(model.SideNewest))
		labels[string(model.SideNewest)] = newestLabel
		changed[string(model.SideNewest)] = slices.ContainsFunc(n.Move.Packs, func(q model.MovePack) bool { return q.Key == k.Key })
	}
	open = string(model.SidePublished)
	for _, side := range options {
		if changed[side] {
			open = side
			break
		}
	}
	return options, labels, changed, open
}

// dropdownSets are the sets whose pack pages the tests of the dropdown read.
var dropdownSets = []devdata.State{devdata.StateOK, devdata.StateQuiet, devdata.StateReadError, devdata.StateImages, devdata.StateAsks}

// Each pack page has one Compare dropdown. It offers the published side always, the copy of the cluster when strata read it, and
// the move to a newer version when one exists. It opens on the first side with a change, else on the published side.
func TestEachPackPageHasOneCompareDropdownWithItsOptionsAndItsDefault(t *testing.T) {
	for _, state := range dropdownSets {
		t.Run(string(state), func(t *testing.T) {
			set, screens := loadSet(t, state), crawl(t, state)
			var packs int
			for _, x := range set.Profiles.Profiles {
				for _, k := range x.Packs {
					packs++
					subject := model.Subject(x.Name, x.Version)
					page := pageOf(t, screens, packPage(subject, k.Layer, k.Name), "")
					where := subject + " " + k.Key
					if got := strings.Count(page, "<select"); got != 1 {
						t.Errorf("%s: %d dropdowns", where, got)
					}
					mustHold(t, where, page, compareLabel+`<select id="side" class="search auto" data-id="`+sideSelect(subject, k.Key)+`" aria-label="Compare">`)
					options, labels, _, open := expectedSides(set, x, k)
					var gotOptions []string
					for _, m := range optionPattern.FindAllStringSubmatch(page, -1) {
						gotOptions = append(gotOptions, m[1])
						if m[3] = htmlpkg.UnescapeString(m[3]); m[3] != labels[m[1]] {
							t.Errorf("%s: the option %s says %q, want %q", where, m[1], m[3], labels[m[1]])
						}
						if (m[2] == "selected") != (m[1] == open) {
							t.Errorf("%s: the option %s selected %v, and the page must open on %s", where, m[1], m[2] == "selected", open)
						}
					}
					if !slices.Equal(gotOptions, options) {
						t.Errorf("%s: the options are %v, want %v", where, gotOptions, options)
					}
				}
			}
			if packs == 0 {
				t.Error("the set holds no pack")
			}
		})
	}
}

var diffRequest = regexp.MustCompile(`^/v1/packs/([^/]+)/([^/]+)/([^/]+)/diff\?side=([a-z]+)$`)

// A side with no change sends no request. A crawl shows every side of every pack, and each request for a diff names a side that
// holds a change. The set quiet holds no change, so it sends none.
func TestASideWithNoChangeSendsNoRequest(t *testing.T) {
	for _, state := range dropdownSets {
		t.Run(string(state), func(t *testing.T) {
			set := loadSet(t, state)
			var diffs int
			for _, r := range requestsOf(t, state) {
				m := diffRequest.FindStringSubmatch(r.Path)
				if m == nil {
					continue
				}
				diffs++
				subject, layer, name := mustUnescape(t, m[1]), mustUnescape(t, m[2]), mustUnescape(t, m[3])
				var found bool
				for _, x := range set.Profiles.Profiles {
					for _, k := range x.Packs {
						if model.Subject(x.Name, x.Version) == subject && k.Layer == layer && k.Name == name {
							found = true
							if _, _, changed, _ := expectedSides(set, x, k); !changed[m[4]] {
								t.Errorf("the page asked for the side %s of %s %s/%s, and that side holds no change", m[4], subject, layer, name)
							}
						}
					}
				}
				if !found {
					t.Errorf("the page asked for a pack that the set does not hold: %s", r.Path)
				}
			}
			if state == devdata.StateQuiet && diffs != 0 {
				t.Errorf("the set quiet holds no change, and the page sent %d requests for a diff", diffs)
			}
			if state == devdata.StateOK && diffs == 0 {
				t.Error("the crawl of the set ok sent no request for a diff")
			}
		})
	}
}

func mustUnescape(t *testing.T, s string) string {
	t.Helper()
	out, err := url.PathUnescape(s)
	if err != nil {
		t.Fatal(err)
	}
	return out
}

// A side with no change shows the splash Undisturbed with its three facts. The side with a change shows its tabs.
func TestASideWithNoChangeShowsUndisturbedAndTheSideWithAChangeShowsItsTabs(t *testing.T) {
	screens := crawl(t, devdata.StateOK)
	// Dex has a change on the published side and a newer version, and the copy of the cluster is the record.
	cluster := pageOf(t, screens, dexHash, variantOf(string(model.SideCluster), layoutUnified, "", ""))
	where := "the copy of the cluster"
	mustHold(t, where, cluster, "Undisturbed", "The copy of this cluster holds what the record holds.",
		sameCopy, `<b class="">Changed</b>`+publishedNow, `<b class="">2.44.0</b>`+newerSpan)
	mustNotHold(t, where, cluster, `role="tablist"`)
	mustHold(t, "the published side", pageOf(t, screens, dexHash, ""), `role="tablist"`)
	mustNotHold(t, "the published side", pageOf(t, screens, dexHash, ""), "Undisturbed")
}

// ---- The tab with the change opens first ----

// firstTabOf reads a diff a second time, in Go, and gives the first tab that holds a change.
// valuesChanged says that the Values tab of a diff holds a change: a value, text that strata cannot read as settings, or the text.
func valuesChanged(d api.PackDiff) bool {
	return len(d.Values.Settings)+len(d.Values.FollowsTheVersion)+len(d.Values.RepeatsTheLists) > 0 || !d.Values.Parseable || d.Values.Unified != ""
}

// onlyTheBuild says that a diff changes the build of the pack, and no value, manifest, image, or chart.
func onlyTheBuild(d api.PackDiff) bool {
	return len(d.Build) > 0 && !valuesChanged(d) && d.Values.Unified == "" && len(d.Manifests)+len(d.ParamChanges)+len(d.ImageChanges)+len(d.ChartChanges) == 0
}

func firstTabOf(d api.PackDiff) string {
	switch {
	case valuesChanged(d):
		return "values"
	case len(d.Manifests)+len(d.ParamChanges) > 0:
		return "manifests"
	case len(d.ImageChanges) > 0:
		return "images"
	case len(d.ChartChanges) > 0:
		return "charts"
	}
	return "values"
}

// tabId is the id of the tabs of a side of a pack.
func tabId(d api.PackDiff) string {
	if d.Side == model.SideNewer {
		return "move-" + d.Profile + "-" + d.Key
	}
	return sideSelect(d.Profile, d.Key) + "-" + string(d.Side)
}

// sideSelect is the id of the list of the sides of the pack key of the profile subject. It is also the key of the choice of
// that list in S.side.
func sideSelect(subject, key string) string { return "side-" + subject + "-" + key }

// tabOf gives the data attribute of the tabs id, and tabAttrs adds the tab k: values, manifests, images, or charts.
func tabOf(id string) string { return `data-tab="` + id + `"` }

func tabAttrs(id, k string) string { return tabOf(id) + ` data-k="` + k + `"` }

func tabOn(id, k string) string {
	return `<button class="tab on" role="tab" aria-selected="true" ` + tabAttrs(id, k) + `>`
}

// tabLink gives the link of an empty tab to the tab k of the tabs id, which has the change. words name the tab k.
// changeIsOn starts the words of the link.
func tabLink(id, k, words string) string {
	return linkButton + tabAttrs(id, k) + `>` + changeIsOn + words + ` →</button>`
}

const changeIsOn = "The change is on "

// flagLink gives the badge of the flag f that opens the tab k of the tabs id on the page hash. flagLinkOf gives that
// badge with the class. plainFlag gives a badge of the class that opens no tab.
func flagLink(hash, id, k, f string) string { return flagLinkOf("b", hash, id, k, f) }

func flagLinkOf(class, hash, id, k, f string) string {
	return `<a class="badge ` + class + `" href="` + hash + `" ` + tabAttrs(id, k) + `>` + f + `</a>`
}

func plainFlag(class, f string) string {
	return badgeOpen + class + `" tabindex="0">` + f + `</span>`
}

// manifestAlone writes the set ok, where the published side of dex changes in a manifest alone, and gives its folder.
func manifestAlone(t *testing.T) string {
	t.Helper()
	return writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"diffs.json": editDiffs(t, func(ds []api.PackDiff) {
		d := entryOf(t, ds, testkit.DexProfile, testkit.DexKey, model.SidePublished)
		d.Values.Settings, d.Values.Unified = []docs.Leaf{}, ""
	})})
}

// The pack page opens on the first tab with a change.
func TestThePackPageOpensOnTheFirstTabWithAChange(t *testing.T) {
	set, screens := loadSet(t, devdata.StateOK), crawl(t, devdata.StateOK)
	var buildOnly int
	for _, d := range set.Diffs {
		layer, name, _ := strings.Cut(d.Key, "/")
		page := pageOf(t, screens, packPage(d.Profile, layer, name), variantOf(string(d.Side), layoutUnified, "", ""))
		where := d.Profile + " " + d.Key + " " + string(d.Side)
		if onlyTheBuild(d) {
			// A change of the build alone shows the build and one line, and no tab: each tab would be empty.
			buildOnly++
			mustHold(t, where, page, "<th>The build of the pack</th>", "<td>Version</td>", "<td>Upstream digest</td>",
				buildWords+" changes. Strata says: "+d.Impact.Text+".")
			mustNotHold(t, where, page, tabAttrs(tabId(d), "values"))
			continue
		}
		mustHold(t, where, page, tabOn(tabId(d), firstTabOf(d)))
	}
	if buildOnly == 0 {
		t.Error("no diff of the set changes the build alone")
	}
	// The card of the move says the same, and it gives no count of 0 of 0.
	ubuntu := pageOf(t, screens, ubuntuHash, "")
	where := "the move of ubuntu-aws"
	mustHold(t, where, ubuntu, labelRow("Content", "<b>The same</b>"), labelRow("Changes", buildWords), "Repave "+repaveHint+"&#39;s table: node repave (OS image change).")
	mustNotHold(t, where, ubuntu, "of 0 values")
	// A pack whose change is in a manifest alone opens on Manifests.
	dir := manifestAlone(t)
	page := pagesOf(t, dir, dexHash)[0]
	mustHold(t, "a manifest alone", page, tabOn(dexPublished, "manifests"))
	mustNotHold(t, "a manifest alone", page, tabOn(dexPublished, "values"), settingsHead)
}

// An empty tab says so, and it links to the tab that has the change. A click on the link opens that tab.
func TestAnEmptyTabSaysSoAndLinksToTheTabWithTheChange(t *testing.T) {
	dir := manifestAlone(t)
	empty := pageWith(t, dir, scratch, dexHash, `S.tabs["`+dexPublished+`"] = "values"`)
	link := tabLink(dexPublished, "manifests", "Manifests")
	mustHold(t, "the empty Values tab", empty, span("No value"+changedHere), link)
	for _, c := range []struct{ tab, noun string }{{"images", "image"}, {"charts", "chart"}} {
		page := pageWith(t, dir, scratch, dexHash, `S.tabs["`+dexPublished+`"] = "`+c.tab+`"`)
		mustHold(t, "the empty tab "+c.tab, page, span("No "+c.noun+changedHere), link)
	}
	r := clickIn(t, dir, scratch, dexHash, "0", tabClick(dexPublished, "values"), tabClick(dexPublished, "manifests"))
	mustHold(t, "after the link", r.After, tabOn(dexPublished, "manifests"), dexManifest)
	// A card with no change anywhere says only that nothing changed, and it links nowhere.
	none := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"diffs.json": editDiffs(t, func(ds []api.PackDiff) {
		d := entryOf(t, ds, testkit.DexProfile, testkit.DexKey, model.SidePublished)
		d.Values.Settings, d.Values.Unified, d.Manifests = []docs.Leaf{}, "", []docs.Manifest{}
	})})
	page := pageWith(t, none, scratch, dexHash, `S.tabs["`+dexPublished+`"] = "values"`)
	where := "a card with no change"
	mustHold(t, where, page, span("No value changed."))
	mustNotHold(t, where, page, changeIsOn)
}

// ---- The five change flags ----

var flagPattern = regexp.MustCompile(flagWrap + `(?:` + flagLinkOf(`(b|y off)`, `([^"]*)`, `([^"]*)`, `([^"]*)`, `([A-Z]+)`) + `|` +
	plainFlag(`(b|y off)`, `([A-Z]+)`) + `)<span class="tip" role="tooltip"><b>([A-Z]+)</b>([^<]*)</span></span>`)

var flagNames = []string{docs.FlagIdentity, docs.FlagUpstream, docs.FlagAuthored, docs.FlagImages, docs.FlagCharts}

// flagDefinitions reads the table of the flags in docs/src/reference/impact.md: when a flag lights, and what it means.
func flagDefinitions(t *testing.T) map[string][2]string {
	t.Helper()
	out := map[string][2]string{}
	for _, cells := range tableRows(t, string(testkit.ReadFile(t, "../../../docs/src/reference/impact.md"))) {
		if len(cells) != 3 {
			t.Fatalf("a row of the table of the flags has %d cells, want 3: %q", len(cells), cells)
		}
		for i := range cells {
			cells[i] = strings.ReplaceAll(cells[i], "`", "")
		}
		out[cells[0]] = [2]string{cells[1], cells[2]}
	}
	if len(out) != len(flagNames) {
		t.Fatalf("impact.md defines %d flags, want %d", len(out), len(flagNames))
	}
	return out
}

// flagTabOf reads a diff a second time, in Go, and gives the tab that a lit flag opens, or "" for a flag with no tab.
func flagTabOf(flag string, d api.PackDiff) string {
	switch flag {
	case docs.FlagAuthored:
		if valuesChanged(d) {
			return "values"
		}
		if len(d.Manifests)+len(d.ParamChanges) > 0 {
			return "manifests"
		}
	case docs.FlagImages:
		if len(d.ImageChanges) > 0 {
			return "images"
		}
	case docs.FlagCharts:
		if len(d.ChartChanges) > 0 {
			return "charts"
		}
	}
	return ""
}

// Each flag has its words, from the table of docs/src/reference/impact.md, for a mouse (hover) and for a keyboard (focus). A
// lit flag with a tab is a link to that tab. IDENTITY and UPSTREAM have no tab, and a dim flag is no link. A lit flag keeps one
// color and a dim flag keeps one color.
func TestEachFlagHasItsWordsAndALitFlagWithATabOpensIt(t *testing.T) {
	defs, set, screens := flagDefinitions(t), loadSet(t, devdata.StateOK), crawl(t, devdata.StateOK)
	for _, d := range set.Diffs {
		layer, name, _ := strings.Cut(d.Key, "/")
		where := d.Profile + " " + d.Key + " " + string(d.Side)
		page := pageOf(t, screens, packPage(d.Profile, layer, name), variantOf(string(d.Side), layoutUnified, "", ""))
		got := flagPattern.FindAllStringSubmatch(page, -1)
		if n := strings.Count(page, flagWrap); n != len(flagNames) || len(got) != len(flagNames) {
			t.Fatalf("%s: %d flags in the markup and %d that the pattern reads, want %d", where, n, len(got), len(flagNames))
		}
		for i, f := range flagNames {
			m, lit := got[i], slices.Contains(d.Flags, f)
			link := m[5] != ""
			tip, cls := htmlpkg.UnescapeString(m[9]), m[6]
			if link {
				cls = m[1]
			}
			if m[8] != f || (m[5] != f && m[7] != f) {
				t.Errorf("%s: flag %d is %q, want %s", where, i, m[8], f)
			}
			// A flag that lit says when it lights and what it means, and a flag that did not says when it would light.
			if lit {
				means, ok := strings.CutPrefix(tip, "Lit: "+defs[f][0]+". ")
				if !ok || means == "" || !strings.HasPrefix(defs[f][1], means) {
					t.Errorf("%s: the words of %s are %q, and they are not the words of the book", where, f, tip)
				}
			} else if want := "Dim. It lights when " + defs[f][0] + "."; tip != want {
				t.Errorf("%s: the words of %s are %q, want %q", where, f, tip, want)
			}
			if wantClass := map[bool]string{true: "b", false: "y off"}[lit]; cls != wantClass {
				t.Errorf("%s: %s has the color %q, want %q: lit or dim is the signal", where, f, cls, wantClass)
			}
			tab := ""
			if lit {
				tab = flagTabOf(f, d)
			}
			if link != (tab != "") {
				t.Errorf("%s: %s is a link: %v, want %v", where, f, link, tab != "")
				continue
			}
			if link && (m[2] != packPage(d.Profile, layer, name) || m[3] != tabId(d) || m[4] != tab) {
				t.Errorf("%s: %s links to %q %q %q, want %q %q %q", where, f, m[2], m[3], m[4], packPage(d.Profile, layer, name), tabId(d), tab)
			}
		}
	}
	css := string(readPage(t, "app.css"))
	// The words of a flag show on hover and on a focus from the keyboard, and not before.
	ruleIs(t, css, ".flagw:hover .tip,.flagw:has(:focus-visible) .tip", "display:block;")
	rulesHold(t, css, map[string][]string{".tip": {"display:none;"}})
	// One tip at a time: a flag that keeps the focus hides its tip while the pointer is on another flag.
	ruleIs(t, css, ".flags:hover .flagw:not(:hover) .tip", "display:none;")
}

// The words of the flags are in one place in the script, and they are the words of the book.
func TestTheWordsOfTheFlagsHaveOneCopyAndTheyComeFromTheBook(t *testing.T) {
	defs, script := flagDefinitions(t), readScripts(t)
	for _, f := range flagNames {
		when := defs[f][0]
		if got := strings.Count(script, `"`+when+`"`); got != 1 {
			t.Errorf("the script holds the words of when %s lights %d times, want 1", f, got)
		}
		means := regexp.MustCompile(f + `: \{when: "[^"]*", means: "([^"]*)"\}`).FindStringSubmatch(script)
		if means == nil || !strings.HasPrefix(defs[f][1], means[1]) {
			t.Errorf("the words of what %s means are not the first words of the book: %q", f, means)
		}
	}
}

// AUTHORED opens Manifests when only a manifest changed. IMAGES opens Images, CHARTS opens Charts, and IDENTITY and UPSTREAM stay plain.
func TestAuthoredOpensManifestsWhenOnlyAManifestChanged(t *testing.T) {
	dir := manifestAlone(t)
	page := pagesOf(t, dir, dexHash)[0]
	mustHold(t, "the flag", page, flagLink(dexHash, dexPublished, "manifests", docs.FlagAuthored))
	r := clickIn(t, dir, scratch, dexHash, "0", tabClick(dexPublished, "manifests"))
	mustHold(t, "after a click on the flag", r.After, tabOn(dexPublished, "manifests"), dexManifest)
	// The newer side of dex lights all five flags: three open a tab, and two stay plain.
	move := pageOf(t, crawl(t, devdata.StateOK), dexHash, variantOf(string(model.SideNewer), layoutUnified, "", ""))
	for _, f := range []string{docs.FlagIdentity, docs.FlagUpstream} {
		mustHold(t, f, move, plainFlag("b", f))
	}
	for f, k := range map[string]string{docs.FlagAuthored: "values", docs.FlagImages: "images", docs.FlagCharts: "charts"} {
		mustHold(t, f, move, flagLink(dexHash, dexMove, k, f))
	}
	// A flag that did not light is dim and plain.
	mustHold(t, "the dim flags", pageOf(t, crawl(t, devdata.StateOK), dexHash, ""), plainFlag("y off", docs.FlagIdentity), plainFlag("y off", docs.FlagCharts))
}

// ---- The card "Check this yourself" and the header of a pack ----

var rowPattern = regexp.MustCompile(qaRow + `<span class="q">([^<]*)</span><span class="a">(.*?)</span></div><div class="cmd">` + codeOf + buttonOpen + `class="copy" data-copy="([^"]*)">Copy</button></div></li>`)

// termRow is a row of the card: a question, the command that answers it, and what the page says now.
type termRow struct{ question, command, answer string }

func termRows(t *testing.T, page string) []termRow {
	t.Helper()
	card := cardOf(t, page, checkCard)
	var out []termRow
	for _, m := range rowPattern.FindAllStringSubmatch(card, -1) {
		if m[3] != m[4] {
			t.Errorf("the button Copy holds %q, and the command is %q", m[4], m[3])
		}
		out = append(out, termRow{m[1], htmlpkg.UnescapeString(m[3]), m[2]})
	}
	if got := strings.Count(card, qaRow); got != len(out) {
		t.Fatalf("the card has %d rows, and the pattern reads %d", got, len(out))
	}
	return out
}

// rowAsking gives the row of the card "Check this yourself" of the page that asks the question. It fails the test
// unless one row asks it.
func rowAsking(t *testing.T, page, question string) termRow {
	t.Helper()
	var rows []termRow
	for _, r := range termRows(t, page) {
		if r.question == question {
			rows = append(rows, r)
		}
	}
	if len(rows) != 1 {
		t.Fatalf("the card has %d rows that ask %q, want 1", len(rows), question)
	}
	return rows[0]
}

const recordRead = "for i in 0 1 2 3; do " + kubeNS + " get secret " + recordsSecret + "$i -o json | jq -r --arg k '%s' '.data[$k] // empty'; done | base64 -d | gunzip > record.json"

// The questions of the card "Check this yourself", and the commands of strata that it gives, that more than one test reads.
const (
	qPaletteHas    = "Does Palette still have this version?"
	qClusterRuns   = "Does this cluster still run the record?"
	verifyRecord   = "strata verify record.json"
	verifyWho      = verifyRecord + " --who"
	compareRecord  = "strata compare record.json "
	compareCluster = compareRecord + "cluster:"
)

// The card "Check this yourself" replaces "At a terminal". Each row is a question, a command with Copy, and what the page says now.
// The names are real, and the rows change with the page: a pack with a newer version adds the move and the question to Palette.
func TestTheCardCheckThisYourselfHasTheRowsOfTheMockWithRealNames(t *testing.T) {
	set := loadSet(t, devdata.StateOK)
	uid := set.Newer.Newer[0].RegistryUID
	profile := termRows(t, shown(t, devdata.StateOK, fredHash)[0])
	// The card is closed at first, and its summary counts the commands.
	mustHold(t, "the card", shown(t, devdata.StateOK, fredHash)[0], `<details class="check" data-open="check"><summary>`+checkCard+`<span class="sub">5 commands give the same answers in a terminal</span></summary>`)
	if len(profile) != 5 {
		t.Fatalf("the profile page has %d rows, want 5: %+v", len(profile), profile)
	}
	for i, want := range []termRow{
		{"Get the record that this check used", fmt.Sprintf(recordRead, "fred-identity_2.3.0.json.gz"), `<span class="m">rev ` + testkit.Revision + `</span>`},
		{qPaletteHas, verifyRecord, badge("r", "Drift")},
		{"Who changed it?", verifyWho, ""},
		{"What changed?", dexCompare, "1 pack"},
		{qClusterRuns, compareCluster + "quarry-1/fred-identity", badge("g", "OK")},
	} {
		got := profile[i]
		if got.question != want.question || got.command != want.command || (want.answer != "" && got.answer != want.answer) {
			t.Errorf("row %d is %+v, want %+v", i, got, want)
		}
	}
	if !regexp.MustCompile(`^Betty Rubble, .+ ago$`).MatchString(profile[2].answer) {
		t.Errorf("the answer of the question about who is %q", profile[2].answer)
	}
	// The pack with a newer version adds the move and the question about the versions that Palette lists.
	pack := termRows(t, shown(t, devdata.StateOK, dexHash)[0])
	if len(pack) != 7 {
		t.Fatalf("the pack page of dex has %d rows, want 7: %+v", len(pack), pack)
	}
	mustHold(t, "the move", pack[5].command, comparePacks+uid+"/"+testkit.DexFrom+" pack:"+uid+"/"+testkit.DexTo+" "+runner.FlagFull)
	mustHold(t, "the question", pack[6].command, "strata behind record.json "+runner.FlagPin+" "+testkit.DexKey)
	// A pack with no newer version shows neither, and a profile page shows neither.
	for _, rows := range [][]termRow{termRows(t, shown(t, devdata.StateOK, reloaderHash)[0]), profile} {
		for _, r := range rows {
			mustNotHold(t, "the command", r.command, runner.CmdCompare+" pack:", runner.CmdBehind)
		}
	}
	// A profile with no drift has no row about who. An old placeholder is gone.
	other := termRows(t, shown(t, devdata.StateOK, barneyHash)[0])
	for _, r := range other {
		mustNotHold(t, "the rows of barney-base", r.command, "--who", "RECORD")
	}
	mustNotHold(t, "the page", shown(t, devdata.StateOK, fredHash)[0], "At a terminal", "strata verify RECORD")
}

// The names in the commands are the names of the files of the repository: the four Secrets, the namespace, the key of a record, and
// the options of the commands. The contract test below runs each command against the stand-in.
func TestTheNamesInTheCommandsOfTheCardAreRealNames(t *testing.T) {
	deployment, namespace := string(testkit.ReadFile(t, "../../../deploy/observer/deployment.yaml")), string(testkit.ReadFile(t, "../../../deploy/observer/namespace.yaml"))
	for i := 0; i < 4; i++ {
		mustHold(t, "the deployment", deployment, fmt.Sprintf(recordsSecret+"%d", i))
	}
	mustHold(t, "the namespace", namespace, "name: strata-observer")
	script := string(testkit.ReadFile(t, testkit.RecordsScript))
	mustHold(t, testkit.RecordsTool, script, []string{`"\(.source.name)_\(.source.version).json.gz"`, `tr -c 'A-Za-z0-9._\n-' _`, recordsSecret + "$i"}...)
	app := readScripts(t)
	mustHold(t, "the scripts", app, "replace(/[^A-Za-z0-9._-]/g, \"_\")")
	for file, flags := range map[string][]string{"help-behind.txt": {runner.FlagPin + " <LAYER/NAME>"}, "help-verify.txt": {"--who", runner.FlagFull}, "help-compare.txt": {runner.FlagFull}} {
		help := string(testkit.ReadFile(t, "../../../docs/src/output/"+file))
		mustHold(t, file, help, flags...)
	}
}

// The digest in Upstream reads like the other values, and a blank value reads "none".
func TestTheHeaderOfAPackShowsTheDigestLikeTheOtherValuesAndNoneForABlank(t *testing.T) {
	page := shown(t, devdata.StateOK, reloaderHash)[0]
	mustHold(t, "the header", page, upstreamRow("sha256:94ccf3b3b185"), labelRow("Registry", "Public Repo"))
	css := string(readPage(t, "app.css"))
	rulesHold(t, css, map[string][]string{".meta div span.bright": {"color:var(--ink);", "display:inline;"}})
	dir := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"profiles.json": editJSON(t, func(v *api.ProfilesView) {
		for i := range v.Profiles {
			for j := range v.Profiles[i].Packs {
				if v.Profiles[i].Packs[j].Name == "reloader" {
					v.Profiles[i].Packs[j].Registry, v.Profiles[i].Packs[j].Kind, v.Profiles[i].Packs[j].Upstream = "", "", ""
				}
			}
		}
	})})
	blank := pagesOf(t, dir, reloaderHash)[0]
	mustHold(t, "a blank value", blank, labelRow("Registry", "none")+labelRow("Kind", "none")+upstreamRow("none"))
}

// No inline style reaches a screen of any set. The policy of the page stops it, and the color of a slab is a rule of app.css.
func TestNoInlineStyleReachesAScreenOfAnySet(t *testing.T) {
	for _, state := range devdata.States {
		t.Run(string(state), func(t *testing.T) {
			for _, s := range crawl(t, state) {
				var got []string
				problemsInPage(s.HTML, func(p string) { got = append(got, p) })
				noProblems(t, s.Hash+" "+s.Variant, got)
			}
		})
	}
}

// ---- The stack of the packs ----

var slabPattern = regexp.MustCompile(`<li class="pslab t-([rbgy])( here)?"><a href="([^"]*)"(?: aria-current="page")?><span class="layer">([^<]*)</span><span class="who">` + refSpan + ref(`([^<]*)`, `([^<]*)`) + `</span>`)

// The page of a profile lists its packs as a stack, from the top layer to the bottom one: each slab
// is a link to its pack, with the ref of the pack and the color of its status. The page of a pack
// shows the same stack, small, and lights its own slab alone.
func TestThePacksOfAProfileAreAStackAndThePageOfAPackLightsItsSlab(t *testing.T) {
	set, screens := loadSet(t, devdata.StateOK), crawl(t, devdata.StateOK)
	for _, x := range set.Profiles.Profiles {
		if len(x.Packs) == 0 {
			continue
		}
		subject := x.Name + "@" + x.Version
		slabs := slabPattern.FindAllStringSubmatch(pageOf(t, screens, "#p-"+subject, ""), -1)
		if len(slabs) != len(x.Packs) {
			t.Fatalf("%s: %d slabs for %d packs", subject, len(slabs), len(x.Packs))
		}
		for i, m := range slabs {
			p := x.Packs[len(x.Packs)-1-i]
			if m[4] != p.Layer || m[5] != p.Name || m[6] != p.Version || m[3] != packPage(subject, p.Layer, p.Name) || m[2] != "" {
				t.Errorf("%s: slab %d is %q %q %q, want the pack %s from the top", subject, i, m[4], m[5], m[3], p.Key)
			}
		}
		p := x.Packs[0]
		lit := 0
		for _, m := range slabPattern.FindAllStringSubmatch(pageOf(t, screens, packPage(subject, p.Layer, p.Name), ""), -1) {
			if m[2] != "" {
				lit++
				if m[5] != p.Name || m[6] != p.Version {
					t.Errorf("%s: the page of %s lights %s", subject, p.Key, m[5])
				}
			}
		}
		if lit != 1 {
			t.Errorf("%s: the page of %s lights %d slabs", subject, p.Key, lit)
		}
	}
}

// ---- Sort and filter ----

// The order of the packs of a group of the Newer table, by the name of each pack.
func newerOrder(page, subject string) []string {
	at := strings.Index(page, `href="#p-`+subject+`"`)
	if at < 0 {
		return nil
	}
	rest := page[at:]
	if end := strings.Index(rest[1:], `<tr class="g">`); end >= 0 {
		rest = rest[:end+1]
	}
	var out []string
	for _, m := range regexp.MustCompile(linkRow(`[^"]*`)+`<td class="m"><a href="[^"]*">([^<]+)</a>`).FindAllStringSubmatch(rest, -1) {
		out = append(out, m[1])
	}
	return out
}

// A click on the head of a column sorts the table up, a second click sorts it down, and a third gives back
// the order of the page. A version sorts by SemVer 2.0.0, so 1.20.1 comes before 1.36.2, and a version that
// is no SemVer sorts in natural order.
func TestAHeadSortsItsTableUpThenDownThenBack(t *testing.T) {
	sortedUp, sortedDown := sortAria("ascending"), sortAria("descending")
	natural := newerOrder(pageOf(t, render(t, devdata.StateOK, false), "#newer", ""), testkit.BaseProfile)
	up := pageWith(t, "testdata/api", devdata.StateOK, "#newer", `S.sort.nt = {col: "move", dir: 1}`)
	mustHold(t, "up", up, sortTh("", "ascending")+sortButton("nt", "move", " on asc", "Move")+sortIcon)
	if got, want := newerOrder(up, testkit.BaseProfile), []string{"cni-cilium-oss", "cert-manager", "kubernetes", "csi-aws-ebs", "ubuntu-aws"}; !slices.Equal(got, want) {
		t.Errorf("up: %v, want %v", got, want)
	}
	down := pageWith(t, "testdata/api", devdata.StateOK, "#newer", `S.sort.nt = {col: "move", dir: -1}`)
	mustHold(t, "down", down, sortedDown, sortButton("nt", "move", " on desc", "Move"))
	if got, want := newerOrder(down, testkit.BaseProfile), []string{"ubuntu-aws", "csi-aws-ebs", "kubernetes", "cert-manager", "cni-cilium-oss"}; !slices.Equal(got, want) {
		t.Errorf("down: %v, want %v", got, want)
	}
	// The harness clicks twice on each element. So one double click sorts up and then down, and three
	// double clicks go round the cycle twice, back to the order of the page.
	sortMove := `{"sort":"nt","col":"move"}`
	mustHold(t, "a double click", click(t, devdata.StateOK, "#newer", "0", sortMove).After, sortedDown)
	back := click(t, devdata.StateOK, "#newer", "0", sortMove, sortMove, sortMove).After
	mustNotHold(t, "three double clicks", back, sortedUp, sortedDown)
	if got := newerOrder(back, testkit.BaseProfile); !slices.Equal(got, natural) {
		t.Errorf("back: %v, want the order of the page %v", got, natural)
	}
}

// searchPattern finds the value of the box "Filter by name" of the screen of the profiles. A test of a rule of the
// page puts the result of the rule in that box.
var searchPattern = regexp.MustCompile(`id="q" type="search"[^>]* value="([^"]*)"`)

// searchOf gives the value of the box "Filter by name" of a screen of the profiles.
func searchOf(t *testing.T, html string) string {
	t.Helper()
	m := searchPattern.FindStringSubmatch(html)
	if m == nil {
		t.Fatalf("the screen has no box Filter by name: %.300s", html)
	}
	return m[1]
}

// Issue #707. A column of versions sorts in the order of strata. testdata/versions.json holds the cases,
// with those of #616 and each form of the order. A test of strata sorts the same file with the order of
// strata. The page sorts the cases from the last one to the first, and it must give the order of the file.
// A sort compares only some pairs, so the page also compares each pair of cases in the two directions.
func TestTheVersionSortOfThePageGivesTheOrderOfStrata(t *testing.T) {
	var cases struct {
		Ascending []string `json:"ascending"`
	}
	testkit.DecodeFile(t, "testdata/versions.json", &cases)
	reversed := slices.Clone(cases.Ascending)
	slices.Reverse(reversed)
	given, ascending := testkit.MustJSON(t, reversed), testkit.MustJSON(t, cases.Ascending)
	order := searchOf(t, pageWith(t, "testdata/api", devdata.StateOK, "#profiles", fmt.Sprintf(`S.q = %s.sort(versionCmp).join("|")`, given)))
	if want := strings.Join(cases.Ascending, "|"); order != want {
		t.Errorf("the order of the versions:\n got %s\nwant %s", order, want)
	}
	// a case gives 0 with itself, and a value below 0 with each later case and above 0 from it
	pairs := `S.q = (c => c.flatMap((a, i) => c.slice(i).filter((b, j) =>
		Math.sign(versionCmp(a, b)) !== -Math.sign(j) || Math.sign(versionCmp(b, a)) !== Math.sign(j)).map(b => a + " and " + b)).join("|"))(%s)`
	if wrong := searchOf(t, pageWith(t, "testdata/api", devdata.StateOK, "#profiles", fmt.Sprintf(pairs, ascending))); wrong != "" {
		t.Errorf("the page orders these pairs of versions wrong: %s", wrong)
	}
}

// A filter of the Newer table keeps a whole group when its profile matches, and else the rows whose pack
// matches. A filter that matches nothing says so.
func TestTheFilterOfTheNewerTableKeepsAGroupOrItsMatchingRows(t *testing.T) {
	byPack := pageWith(t, "testdata/api", devdata.StateOK, "#newer", `S.qs.newer = "cilium"`)
	if got := newerOrder(byPack, testkit.BaseProfile); !slices.Equal(got, []string{"cni-cilium-oss"}) {
		t.Errorf("by pack: %v", got)
	}
	mustNotHold(t, "by pack", byPack, `href="`+fredHash+`"`)
	byProfile := pageWith(t, "testdata/api", devdata.StateOK, "#newer", `S.qs.newer = "barney"`)
	if got := newerOrder(byProfile, testkit.BaseProfile); len(got) != 5 {
		t.Errorf("by profile: %v", got)
	}
	none := pageWith(t, "testdata/api", devdata.StateOK, "#newer", `S.qs.newer = "wilma-nothing"`)
	mustHold(t, "no match", none, `No pack or profile matches <span class="m">wilma-nothing</span>.`)
}

// ---- The page of the newer versions ----

var lanePattern = regexp.MustCompile(`<a class="lane" href="[^"]*"><span class="lname m">([^<]+)</span><span class="steps">(.*?)</span><span class="limp">`)
var stepPattern = regexp.MustCompile(`<span class="step ([a-z]*)"><i></i><b>([^<]+)</b>`)

// The page opens with four tiles and the table. The switch shows the timeline: one lane for each pack of
// each profile, from the pinned version through each listed version to the newest. A move that changes only
// the build says "same" in the table. No word of the two views says "behind" (#643).
func TestTheNewerPageHasTilesATableAndATimeline(t *testing.T) {
	set := loadSet(t, devdata.StateOK)
	table := pageOf(t, render(t, devdata.StateOK, false), "#newer", "")
	mustHold(t, "the tiles", table, tileLabel("Packs with a newer version")+tileValue("10"), tileLabel("Moves that repave")+tileValue("2"),
		tileLabel("New this week"), tileLabel("Versions newer than the pins")+tileValue("16"))
	mustNotHold(t, "the words of the table", strings.ToLower(textOf(table)), "behind")
	mustHold(t, "the head", table, pressedButton("nview", "table", "Table", true), `id="q-newer"`)
	mustHold(t, "the row of ubuntu-aws", rowTo(t, table, ubuntuHash), `title="`+buildWords+` changes: the values stay the same">same</span>`)
	timeline := pageWith(t, "testdata/api", devdata.StateOK, "#newer", `S.newerView = "timeline"`)
	mustNotHold(t, "the words of the timeline", strings.ToLower(textOf(timeline)), "behind")
	lanes := lanePattern.FindAllStringSubmatch(timeline, -1)
	want := 0
	for _, r := range set.Newer.Newer {
		want += len(r.Profiles)
	}
	if len(lanes) != want {
		t.Fatalf("%d lanes, want %d", len(lanes), want)
	}
	for _, l := range lanes {
		if l[1] != "cni-cilium-oss" {
			continue
		}
		var got []string
		for _, s := range stepPattern.FindAllStringSubmatch(l[2], -1) {
			got = append(got, s[1]+":"+s[2])
		}
		if want := []string{"pin:1.19.6", ":1.20.0", "newest:1.20.1"}; !slices.Equal(got, want) {
			t.Errorf("the lane of cni-cilium-oss: %v, want %v", got, want)
		}
	}
}

// runningJS gives the code of the page that sets the progress of the cycle, such as content. The cycle started at the
// time of the JavaScript expression start, or now when start is empty.
func runningJS(cycle, start string, done, total int) string {
	return fmt.Sprintf("D.cluster.%s.running = {startedAt: new Date(%s).toISOString(), done: %d, total: %d};", cycle, start, done, total)
}

// versionsUnread is the code of the page for a versions cycle that has not read the registries yet.
const versionsUnread = "D.cluster.versions.readAt = null; "

// While a cycle runs, its pill shows a ring of its progress and how much is done, with the age of the
// last good cycle of its kind. A cycle that knows no work yet has an open ring and says "checking".
func TestThePillOfACycleThatRunsShowsItsProgress(t *testing.T) {
	page := overviewWith(t, runningJS("versions", "Date.now() - 120000", 3, 8)+" "+runningJS("content", "", 0, 1))
	mustHold(t, "the pill of the versions", page, `<svg class="ring" viewBox="0 0 16 16"`, `Versions <b>checking 3 of 8</b> · last `)
	mustHold(t, "the pill of the content", page, `<svg class="ring open" viewBox="0 0 16 16"`, `Content <b>checking</b> · last `)
	mustNotHold(t, "the pills", page, "every 15m", "every 6h")
}

// Before the first read of the versions ends, the tile and the nav item of the newer versions show a
// dash and say why: a count of 0 would read as "every pack is current".
func TestNewerVersionsShowADashUntilTheFirstReadOfTheVersions(t *testing.T) {
	unread := overviewWith(t, versionsUnread+"D.cluster.versions.running = null;")
	mustHold(t, "unread", unread, tileLabel(newerVersions)+tileValue("-")+tileSub(notReadYet),
		navCount(newerVersions, "", "-"))
	reading := overviewWith(t, versionsUnread+runningJS("versions", "", 0, 1))
	mustHold(t, "reading", reading, tileValue("-")+tileSub("Reading the registries"))
	read := shown(t, devdata.StateOK, "#overview")[0]
	mustNotHold(t, "read", read, tileLabel(newerVersions)+tileValue("-"))
}
