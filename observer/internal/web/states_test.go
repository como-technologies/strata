package web

import (
	"encoding/json"
	"fmt"
	htmlpkg "html"
	"net/url"
	"os"
	"path/filepath"
	"regexp"
	"slices"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

/* The tests below hold the splash on each screen. They run app.js with node, as the tests of web_test.go do. Each place that
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
	var out []string
	if err := json.Unmarshal(runNode(t, state, append([]string{"show"}, hashes...)...), &out); err != nil || len(out) != len(hashes) {
		t.Fatalf("the output of render.js: %v, %d screens for %d hashes", err, len(out), len(hashes))
	}
	return out
}

// pageWith runs the code in the page, and then it shows the screen.
func pageWith(t *testing.T, apiDir string, state devdata.State, hash, code string) string {
	t.Helper()
	var out string
	if err := json.Unmarshal(runNodeIn(t, apiDir, state, "with", hash, code), &out); err != nil {
		t.Fatalf("the output of render.js: %v", err)
	}
	return out
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

// checkPlace fails when the screen lacks the mark of the tone, the title, the fact, or the next step of the place.
func checkPlace(t *testing.T, p place, html string) {
	t.Helper()
	mark := `<svg class="slabs tone-` + p.tone + `" viewBox="0 0 100 100" width="\d+" height="\d+" aria-hidden="true">` + strings.Repeat(`<path class="s s\d" d="M[^"]+"/>`, 4) + `</svg>`
	var head string
	if p.size == "inl" {
		head = regexp.QuoteMeta(`<div class="inl tone-`+p.tone+`">`) + mark
	} else {
		head = regexp.QuoteMeta(`<div class="splash `+p.size+` tone-`+p.tone+`">`) + mark + regexp.QuoteMeta(`<h3>`+p.title+`</h3><p>`)
	}
	if !regexp.MustCompile(head).MatchString(html) {
		t.Errorf("the screen has no %s splash of the tone %s with the title %q: %.600s", p.size, p.tone, p.title, html)
	}
	mustHold(t, p.name, html, p.fact...)
	mustHold(t, p.name, html, p.steps...)
}

const (
	logCommand = "<code>kubectl -n strata-observer logs deploy/strata-observer --tail=20</code>"
)

// The places of the issue: the first check, no records, lost with nothing read, a page that cannot reach the observer, a route
// that does not exist, no findings, no newer versions, nothing unchecked, the pods off, the read of the pods failed, alerts not
// set, a pack that matches its record, a pack not checked, a diff that a cycle holds, a text too large, an empty tab, a filter with
// no match, and no events. The page also has a place for a first read that the observer has not ended, the stop of the checks
// before a good one, and the read of the pods that is not done yet.
func TestEachPlaceHasItsToneItsTitleItsFactAndItsNextStep(t *testing.T) {
	const dexTab = "side-fred-identity@2.3.0-addon/dex-published"
	places := []place{
		{"the first check", func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateFirstLoad), "#overview", "") }, "build", "lg", "Reading the layers",
			[]string{"The first check is running, and this page fills in when it ends. The observer reads each record, then asks Palette for the same version."}, nil},
		{"no records", func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateNoRecords), "#overview", "") }, "off", "lg", "Nothing to compare against yet",
			[]string{"A record is the copy your team approved. Your pipeline writes the records, and the observer checks against them."},
			[]string{"<code>scripts/ci/observer-records.sh --revision &quot;$GIT_SHA&quot; releases/*.json | kubectl apply --server-side -f -</code>",
				`<p class="sub">On a test cluster, this command records what each profile holds now. No person approved that content.</p>`,
				"<code>scripts/ci/observer-records.sh --first-seen 7b1f3c9e-5a42-4d8e-9a67-0c2e4f1a8b35 | kubectl apply --server-side -f -</code>"}},
		{"lost with nothing read", func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateLostUnchecked), "#overview", "") }, "lost", "lg", "We lost the signal from Palette",
			[]string{"The last 3 checks failed: <b>HTTP 401, the API key was refused.</b> No check has ended well, and no finding clears until a check proves it."},
			[]string{`<div class="next"><b>Next</b> Check the key in the Secret <span class="m">strata-observer-key</span>, then read the log.</div>`, logCommand}},
		{"checks stopped with nothing read", func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateStaleUnchecked), "#overview", "") }, "warn", "lg", "Checks stopped before one ended well",
			[]string{"The content check runs every 15m. No check has ended well, so this page has nothing to show yet."}, []string{`<div class="next"><b>Next</b> Read the log.</div>`, logCommand}},
		{"the page cannot reach the observer", func(t *testing.T) string {
			var out struct{ Before, After string }
			if err := json.Unmarshal(runNode(t, devdata.StateOK, "offline", "#overview"), &out); err != nil {
				t.Fatal(err)
			}
			return out.Before
		}, "lost", "lg", "This page lost its line to the observer",
			[]string{"The port-forward may have ended, or the pod may have restarted. This page asks again each 30 seconds."},
			[]string{"<code>kubectl -n strata-observer port-forward deploy/strata-observer 18080:8080</code>", `<button type="button" class="btn" data-retry="page">Try now</button>`}},
		{"a route that does not exist", func(t *testing.T) string { return shown(t, devdata.StateOK, "#p-old-profile@1.0.0")[0] }, "off", "lg", "This profile is not here",
			[]string{`<span class="m">old-profile@1.0.0</span> is in no record now. A new revision of the records can remove it.`}, []string{`<a class="btn" href="#profiles">All profiles</a>`}},
		{"no findings", func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateQuiet), "#findings", "") }, "calm", "md", "All quiet on quarry-1",
			[]string{"Nothing needs attention. <b>29 of 29 packs</b> were checked in full, ", " ago."},
			[]string{"<span>profiles match their records</span>", "<span>not checked</span>", "<span>newer versions</span>"}},
		{"no newer versions", func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateQuiet), "#newer", "") }, "calm", "md", "Nothing newer to dig up",
			[]string{"Every pack is current. The registries list nothing newer than what you pin. Read ", " ago, every 6h."}, nil},
		{"nothing unchecked", func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateQuiet), "#notchecked", "") }, "calm", "md", "Every layer was read",
			[]string{"Each pack was checked in full. Strata read each record, and each answer came back. A check still does not cover the items below."},
			[]string{`<button type="button" class="lnk" data-jump="coverage">What a check does not cover</button>`}},
		{"the pods off", func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateImagesOff), "#images", "") }, "off", "md", "Pods are not on the menu here",
			[]string{"This install does not read the pods. With the read on, the page shows the digest that each tag runs, and finds a tag that moved."},
			[]string{`<div class="next"><b>Next</b> Turn on the read of the pods when you install the observer. The book of strata tells how, in The observer: The pods of the cluster.</div>`}},
		{"the read of the pods failed", func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateImagesUnread), "#images", "") }, "warn", "md", "The pods stay silent",
			[]string{"The last read of the pods failed. <b>HTTP 403, the role of the observer was refused.</b> The findings of the content check are not affected."},
			[]string{"Ask the API server if the role of the observer can list pods.", "<code>kubectl auth can-i list pods --all-namespaces --as=system:serviceaccount:strata-observer:strata-observer</code>"}},
		{"alerts not set", func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateAlertsUnset), "#alerts", "") }, "off", "md", "Nobody gets paged yet",
			[]string{"This install has no alert rules. A message goes to a webhook when a finding opens and when it clears."},
			[]string{`<div class="next"><b>Next</b> Turn on the alerts when you install the observer. The book of strata tells how, in The observer: Alerts.</div>`}},
		{"a pack that matches its record", func(t *testing.T) string {
			return pageOf(t, crawl(t, devdata.StateQuiet), "#k-fred-identity@2.3.0/addon/dex", "")
		}, "same", "md", "Undisturbed",
			[]string{"The published version holds what the record holds."},
			[]string{`<b class="ok">Same</b><span>Published now</span>`, `<b class="ok">Same</b><span>This cluster&#39;s copy</span>`, `<b class="ok">Current</b><span>No newer version</span>`}},
		{"a pack that matches its record, on a page with findings", func(t *testing.T) string {
			return pageOf(t, crawl(t, devdata.StateOK), "#k-fred-identity@2.3.0/addon/reloader", "")
		}, "same", "md", "Undisturbed", []string{"The published version holds what the record holds."},
			[]string{`<b class="ok">Same</b><span>Published now</span>`, `<b class="ok">Current</b><span>No newer version</span>`}},
		{"a pack that was not checked", func(t *testing.T) string {
			return pageOf(t, crawl(t, devdata.StateOK), "#k-gazoo-backup@1.2.0/addon/velero", "")
		}, "warn", "md", "Part of this layer is hidden",
			[]string{"This pack was not fully checked. velero has no upstream digest. The content check runs every 15m."}, []string{`<div class="next"><b>Next</b> <a href="#notchecked">Why, on the Not checked page</a></div>`}},
		{"a diff that a cycle holds", func(t *testing.T) string {
			var pages []string
			if err := json.Unmarshal(runNode(t, devdata.StateOK, "retry", dexHash), &pages); err != nil || len(pages) != 2 {
				t.Fatalf("the output of render.js: %v", err)
			}
			return pages[0]
		}, "build", "sm", "One moment, a check is running", []string{"The observer answers one read at a time. This page asks again each 30 seconds."},
			[]string{`<button type="button" class="btn" data-retry="diff">Try now</button>`}},
		{"a text too large", func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateTextTooLarge), dexHash, "") }, "off", "sm", "Too much rock to carry",
			[]string{"This text is too large to keep here. The observer keeps at most 256 KiB for each side. A terminal shows all of it."},
			[]string{"<code>strata compare record.json fred-identity@2.3.0 --full</code>"}},
		{"an empty tab", func(t *testing.T) string {
			return pageWith(t, "testdata/api", devdata.StateOK, dexHash, `S.tabs["`+dexTab+`"] = "images"`)
		}, "same", "inl", "",
			[]string{"<span>No image changed here.</span>", `<button type="button" class="lnk" data-tab="` + dexTab + `" data-k="values">The change is on Values →</button>`}, nil},
		{"a filter with no match", func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateOK), "#profiles", "search none") }, "off", "inl", "",
			[]string{`<span>No profile matches <span class="m">no profile has this name</span>.</span>`, `<button type="button" class="lnk" data-clear="filter">Clear the filter</button>`}, nil},
		{"no events", func(t *testing.T) string { return pageOf(t, crawl(t, devdata.StateQuiet), "#overview", "") }, "calm", "inl", "",
			[]string{"<span>All quiet: no events in the last 7 days.</span>"}, nil},
	}
	for _, p := range places {
		t.Run(p.name, func(t *testing.T) { checkPlace(t, p, p.screen(t)) })
	}
}

// Each of the five tones is drawn by a place above: calm breathes, build stacks, off marches, warn pulses, and lost slips.
func TestEachToneIsDrawnByAPlaceWithItsMark(t *testing.T) {
	seen := map[string]string{
		"calm": pageOf(t, crawl(t, devdata.StateQuiet), "#findings", ""), "build": pageOf(t, crawl(t, devdata.StateFirstLoad), "#overview", ""),
		"off": pageOf(t, crawl(t, devdata.StateNoRecords), "#overview", ""), "warn": pageOf(t, crawl(t, devdata.StateImagesUnread), "#images", ""),
		"lost": pageOf(t, crawl(t, devdata.StateLostUnchecked), "#overview", ""),
	}
	for tone, html := range seen {
		mustHold(t, tone, html, `class="slabs tone-`+tone+`"`, `class="splash`)
		if got := strings.Count(html, `class="slabs tone-`+tone+`"`); got == 0 {
			t.Errorf("no mark of the tone %s", tone)
		}
	}
}

// A click on "Try now" of a page that cannot reach the observer asks again, and the page shows its data when the observer is back.
func TestTryNowAsksTheObserverAgainAndTheDataComesBack(t *testing.T) {
	var out struct{ Before, After string }
	if err := json.Unmarshal(runNode(t, devdata.StateOK, "offline", "#overview"), &out); err != nil {
		t.Fatal(err)
	}
	mustHold(t, "before", out.Before, "This page lost its line to the observer")
	mustNotHold(t, "before", out.Before, "Needs attention", "<table")
	mustHold(t, "after", out.After, "Needs attention")
	mustNotHold(t, "after", out.After, "This page lost its line to the observer")
}

// A click on "Clear the filter" shows each profile again. A click on Copy sends the text of the command to the clipboard.
func TestClearTheFilterAndCopyDoWhatTheyName(t *testing.T) {
	r := click(t, devdata.StateQuiet, "#profiles", "0", `{"filter":"drift"}`)
	mustHold(t, "the empty filter", r.After, "No profile has this status.", `data-clear="filter"`)
	r = click(t, devdata.StateQuiet, "#profiles", "0", `{"filter":"drift"}`, `{"clear":"filter"}`)
	mustNotHold(t, "after Clear", r.After, "No profile has this status.")
	mustHold(t, "after Clear", r.After, `<tr class="link" data-go="#p-fred-identity@2.3.0">`)
	r = click(t, devdata.StateOK, "#p-fred-identity@2.3.0", "0", `{"copy":"strata verify record.json"}`)
	if len(r.Copies) == 0 || r.Copies[0] != "strata verify record.json" {
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
		out = append(out, "a cause in Not checked")
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

// The test above fails without the rule. A copy of the script whose gate of the calm tone always says yes shows a calm tone on
// the set ok with no events, which holds a finding and two causes: the card of the events says "All quiet". A place that one
// check found the same takes the tone same, which is no claim about the page, so the set ok needs the empty list of events.
func TestTheRuleOfACalmToneIsWhatKeepsItOffAPageWithAFinding(t *testing.T) {
	gate := regexp.MustCompile(`(?m)^const calmOk = .*$`)
	script := string(readPage(t, "app.js"))
	if !gate.MatchString(script) {
		t.Fatal("app.js has no line that starts with const calmOk")
	}
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "app.js"), []byte(gate.ReplaceAllString(script, "const calmOk = () => true;")), 0o644); err != nil {
		t.Fatal(err)
	}
	quiet := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"events.json": func([]byte) []byte { return []byte(`{"events":[]}`) }})
	var screens []screen
	if err := json.Unmarshal(runNodeAssets(t, dir, quiet, "scratch"), &screens); err != nil {
		t.Fatal(err)
	}
	if got := calmScreens(screens); len(got) == 0 {
		t.Error("without the rule, the page with a finding still shows no calm tone: the test cannot fail")
	}
	var real []screen
	if err := json.Unmarshal(runNodeAssets(t, "assets", quiet, "scratch"), &real); err != nil {
		t.Fatal(err)
	}
	if got := calmScreens(real); len(got) != 0 {
		t.Errorf("the real script shows a calm tone on %q", got)
	}
}

// A page that is lost or stale with findings keeps the findings under the bar. It shows no splash of the size of the screen.
func TestLostAndStaleWithFindingsStayABarOverTheFindings(t *testing.T) {
	for _, c := range []struct {
		state devdata.State
		bar   string
	}{
		{devdata.StateLost, `<div class="alertbar r">Lost contact with Palette.`},
		{devdata.StateStale, `<div class="alertbar a">Checks stopped.`},
	} {
		t.Run(string(c.state), func(t *testing.T) {
			screens := crawl(t, c.state)
			for _, hash := range []string{"#overview", "#findings"} {
				page := pageOf(t, screens, hash, "")
				bar, row := strings.Index(page, c.bar), strings.Index(page, "Published version changed after its record")
				if bar < 0 || row < 0 || bar > row {
					t.Errorf("%s: the bar is at %d and the finding is at %d", hash, bar, row)
				}
				mustHold(t, hash, page, `<tr class="link" data-go="#k-fred-identity@2.3.0/addon/dex">`)
				mustNotHold(t, hash, page, `class="splash lg`)
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
	labels = map[string]string{"published": "Record → Published now", "cluster": "Record → This cluster's copy"}
	options = []string{"published"}
	if c := x.ClusterCopy; c != nil && (c.Status == model.CopySame || c.Status == model.CopySameContent || c.Status == model.CopyDiffers) {
		options = append(options, "cluster")
	}
	var row *model.Newer
	for i, r := range set.Newer.Newer {
		if r.Name == k.Name && r.Version == k.Version && r.Registry == k.Registry && slices.Contains(r.Profiles, subject) {
			row = &set.Newer.Newer[i]
		}
	}
	if row != nil {
		options = append(options, "newer")
		labels["newer"] = fmt.Sprintf("%s@%s → %s@%s", row.Name, row.Version, row.Name, row.Newest)
	}
	changed = map[string]bool{"newer": row != nil}
	var ofRecord int
	for _, f := range x.Findings {
		if !slices.Contains(f.Packs, k.Key) || f.Kind.IsImage() {
			continue
		}
		ofRecord++
		if f.Kind == model.ClusterDiffers || f.Kind == model.ImageIDChanged {
			changed["cluster"] = true
		} else {
			changed["published"] = true
		}
		if f.AlsoOnCluster {
			changed["cluster"] = true
		}
	}
	if k.Status == model.StatusDrift && ofRecord == 0 {
		changed["published"] = true
	}
	open = "published"
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
					mustHold(t, where, page, `<label for="side" class="small mut">Compare</label><select id="side" class="search auto" data-id="side-`+subject+`-`+k.Key+`" aria-label="Compare">`)
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
			var requests []struct {
				Method string `json:"method"`
				Path   string `json:"path"`
			}
			if err := json.Unmarshal(runNode(t, state, "requests"), &requests); err != nil {
				t.Fatal(err)
			}
			var diffs int
			for _, r := range requests {
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
	cluster := pageOf(t, screens, dexHash, "side cluster")
	mustHold(t, "the copy of the cluster", cluster, "Undisturbed", "The copy of this cluster holds what the record holds.",
		`<b class="ok">Same</b><span>This cluster&#39;s copy</span>`, `<b class="">Changed</b><span>Published now</span>`, `<b class="">2.44.0</b><span>Newer version</span>`)
	mustNotHold(t, "the copy of the cluster", cluster, `role="tablist"`)
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
	return "side-" + d.Profile + "-" + d.Key + "-" + string(d.Side)
}

func tabOn(id, k string) string {
	return `<button class="tab on" role="tab" aria-selected="true" data-tab="` + id + `" data-k="` + k + `">`
}

// The pack page opens on the first tab with a change.
func TestThePackPageOpensOnTheFirstTabWithAChange(t *testing.T) {
	set, screens := loadSet(t, devdata.StateOK), crawl(t, devdata.StateOK)
	var buildOnly int
	for _, d := range set.Diffs {
		layer, name, _ := strings.Cut(d.Key, "/")
		page := pageOf(t, screens, packPage(d.Profile, layer, name), "side "+string(d.Side))
		where := d.Profile + " " + d.Key + " " + string(d.Side)
		if onlyTheBuild(d) {
			// A change of the build alone shows the build and one line, and no tab: each tab would be empty.
			buildOnly++
			mustHold(t, where, page, "<th>The build of the pack</th>", "<td>Version</td>", "<td>Upstream digest</td>",
				"Only the build of the pack changes. Strata says: "+d.Impact.Text+".")
			mustNotHold(t, where, page, `data-tab="`+tabId(d)+`" data-k="values"`)
			continue
		}
		mustHold(t, where, page, tabOn(tabId(d), firstTabOf(d)))
	}
	if buildOnly == 0 {
		t.Error("no diff of the set changes the build alone")
	}
	// The card of the move says the same, and it gives no count of 0 of 0.
	ubuntu := pageOf(t, screens, packPage("barney-base@1.5.0", "os", "ubuntu-aws"), "")
	mustHold(t, "the move of ubuntu-aws", ubuntu, "<span>Content</span>The same: only the build of the pack changes. Strata says: node repave (OS image change).")
	mustNotHold(t, "the move of ubuntu-aws", ubuntu, "of 0 values")
	// A pack whose change is in a manifest alone opens on Manifests.
	dir := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"diffs.json": editDiffs(t, func(ds []api.PackDiff) {
		d := entryOf(t, ds, dexSubject, "addon/dex", model.SidePublished)
		d.Values.Settings, d.Values.Unified = []docs.Leaf{}, ""
	})})
	page := pagesOf(t, dir, dexHash)[0]
	mustHold(t, "a manifest alone", page, tabOn(dexPublished, "manifests"))
	mustNotHold(t, "a manifest alone", page, tabOn(dexPublished, "values"), "<th>Setting</th>")
}

// An empty tab says so, and it links to the tab that has the change. A click on the link opens that tab.
func TestAnEmptyTabSaysSoAndLinksToTheTabWithTheChange(t *testing.T) {
	dir := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"diffs.json": editDiffs(t, func(ds []api.PackDiff) {
		d := entryOf(t, ds, dexSubject, "addon/dex", model.SidePublished)
		d.Values.Settings, d.Values.Unified = []docs.Leaf{}, ""
	})})
	empty := pageWith(t, dir, "scratch", dexHash, `S.tabs["`+dexPublished+`"] = "values"`)
	link := `<button type="button" class="lnk" data-tab="` + dexPublished + `" data-k="manifests">The change is on Manifests →</button>`
	mustHold(t, "the empty Values tab", empty, "<span>No value changed here.</span>", link)
	for _, c := range []struct{ tab, noun string }{{"images", "image"}, {"charts", "chart"}} {
		page := pageWith(t, dir, "scratch", dexHash, `S.tabs["`+dexPublished+`"] = "`+c.tab+`"`)
		mustHold(t, "the empty tab "+c.tab, page, "<span>No "+c.noun+" changed here.</span>", `data-k="manifests">The change is on Manifests →</button>`)
	}
	r := clickIn(t, dir, "scratch", dexHash, "0", `{"tab":"`+dexPublished+`","k":"values"}`, `{"tab":"`+dexPublished+`","k":"manifests"}`)
	mustHold(t, "after the link", r.After, tabOn(dexPublished, "manifests"), "manifest fred-realm-config")
	// A card with no change anywhere says only that nothing changed, and it links nowhere.
	none := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"diffs.json": editDiffs(t, func(ds []api.PackDiff) {
		d := entryOf(t, ds, dexSubject, "addon/dex", model.SidePublished)
		d.Values.Settings, d.Values.Unified, d.Manifests = []docs.Leaf{}, "", []docs.Manifest{}
	})})
	page := pageWith(t, none, "scratch", dexHash, `S.tabs["`+dexPublished+`"] = "values"`)
	mustHold(t, "a card with no change", page, "<span>No value changed.</span>")
	mustNotHold(t, "a card with no change", page, "The change is on")
}

// ---- The five change flags ----

var flagPattern = regexp.MustCompile(`<span class="flagw">(?:<a class="badge (b|y off)" href="([^"]*)" data-tab="([^"]*)" data-k="([^"]*)">([A-Z]+)</a>|<span class="badge (b|y off)" tabindex="0">([A-Z]+)</span>)<span class="tip" role="tooltip"><b>([A-Z]+)</b>([^<]*)</span></span>`)

var flagNames = []string{"IDENTITY", "UPSTREAM", "AUTHORED", "IMAGES", "CHARTS"}

// flagDefinitions reads the table of the flags in docs/src/reference/impact.md: when a flag lights, and what it means.
func flagDefinitions(t *testing.T) map[string][2]string {
	t.Helper()
	row := regexp.MustCompile("(?m)^\\| `([A-Z]+)` \\| ([^|]*) \\| ([^|]*) \\|$")
	out := map[string][2]string{}
	for _, m := range row.FindAllStringSubmatch(string(readFile(t, "../../../docs/src/reference/impact.md")), -1) {
		out[m[1]] = [2]string{strings.ReplaceAll(m[2], "`", ""), strings.ReplaceAll(m[3], "`", "")}
	}
	if len(out) != len(flagNames) {
		t.Fatalf("impact.md defines %d flags, want %d", len(out), len(flagNames))
	}
	return out
}

// flagTabOf reads a diff a second time, in Go, and gives the tab that a lit flag opens, or "" for a flag with no tab.
func flagTabOf(flag string, d api.PackDiff) string {
	switch flag {
	case "AUTHORED":
		if valuesChanged(d) {
			return "values"
		}
		if len(d.Manifests)+len(d.ParamChanges) > 0 {
			return "manifests"
		}
	case "IMAGES":
		if len(d.ImageChanges) > 0 {
			return "images"
		}
	case "CHARTS":
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
		page := pageOf(t, screens, packPage(d.Profile, layer, name), "side "+string(d.Side))
		got := flagPattern.FindAllStringSubmatch(page, -1)
		if n := strings.Count(page, `<span class="flagw">`); n != len(flagNames) || len(got) != len(flagNames) {
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
	if got := ruleOf(css, ".flagw:hover .tip,.flagw:has(:focus-visible) .tip"); got != "display:block;" {
		t.Errorf("the words of a flag show on hover and on a focus from the keyboard: the rule is %q", got)
	}
	// One tip at a time: a flag that keeps the focus hides its tip while the pointer is on another flag.
	if got := ruleOf(css, ".flags:hover .flagw:not(:hover) .tip"); got != "display:none;" {
		t.Errorf("two tips can show at once: the rule is %q", got)
	}
	if !strings.Contains(ruleOf(css, ".tip"), "display:none;") {
		t.Error("the words of a flag show before a hover or a focus")
	}
}

// The words of the flags are in one place in the script, and they are the words of the book.
func TestTheWordsOfTheFlagsHaveOneCopyAndTheyComeFromTheBook(t *testing.T) {
	defs, script := flagDefinitions(t), string(readPage(t, "app.js"))
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
	dir := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"diffs.json": editDiffs(t, func(ds []api.PackDiff) {
		d := entryOf(t, ds, dexSubject, "addon/dex", model.SidePublished)
		d.Values.Settings, d.Values.Unified = []docs.Leaf{}, ""
	})})
	page := pagesOf(t, dir, dexHash)[0]
	mustHold(t, "the flag", page, `<a class="badge b" href="`+dexHash+`" data-tab="`+dexPublished+`" data-k="manifests">AUTHORED</a>`)
	r := clickIn(t, dir, "scratch", dexHash, "0", `{"tab":"`+dexPublished+`","k":"manifests"}`)
	mustHold(t, "after a click on the flag", r.After, tabOn(dexPublished, "manifests"), "manifest fred-realm-config")
	// The newer side of dex lights all five flags: three open a tab, and two stay plain.
	move := pageOf(t, crawl(t, devdata.StateOK), dexHash, "side newer")
	for _, f := range []string{"IDENTITY", "UPSTREAM"} {
		mustHold(t, f, move, `<span class="badge b" tabindex="0">`+f+`</span>`)
	}
	for f, k := range map[string]string{"AUTHORED": "values", "IMAGES": "images", "CHARTS": "charts"} {
		mustHold(t, f, move, `<a class="badge b" href="`+dexHash+`" data-tab="`+dexMove+`" data-k="`+k+`">`+f+`</a>`)
	}
	// A flag that did not light is dim and plain.
	mustHold(t, "the dim flags", pageOf(t, crawl(t, devdata.StateOK), dexHash, ""), `<span class="badge y off" tabindex="0">IDENTITY</span>`, `<span class="badge y off" tabindex="0">CHARTS</span>`)
}

// ---- The card "Check this yourself" and the header of a pack ----

var rowPattern = regexp.MustCompile(`<li><div class="qa"><span class="q">([^<]*)</span><span class="a">(.*?)</span></div><div class="cmd"><code>([^<]*)</code><button type="button" class="copy" data-copy="([^"]*)">Copy</button></div></li>`)

// termRow is a row of the card: a question, the command that answers it, and what the page says now.
type termRow struct{ question, command, answer string }

func termRows(t *testing.T, page string) []termRow {
	t.Helper()
	card := cardOf(t, page, "<h2>Check this yourself</h2>")
	var out []termRow
	for _, m := range rowPattern.FindAllStringSubmatch(card, -1) {
		if m[3] != m[4] {
			t.Errorf("the button Copy holds %q, and the command is %q", m[4], m[3])
		}
		out = append(out, termRow{m[1], htmlpkg.UnescapeString(m[3]), m[2]})
	}
	if got := strings.Count(card, `<li><div class="qa">`); got != len(out) {
		t.Fatalf("the card has %d rows, and the pattern reads %d", got, len(out))
	}
	return out
}

const recordRead = "for i in 0 1 2 3; do kubectl -n strata-observer get secret strata-observer-records-$i -o json | jq -r --arg k '%s' '.data[$k] // empty'; done | base64 -d | gunzip > record.json"

// The card "Check this yourself" replaces "At a terminal". Each row is a question, a command with Copy, and what the page says now.
// The names are real, and the rows change with the page: a pack with a newer version adds the move and the question to Palette.
func TestTheCardCheckThisYourselfHasTheRowsOfTheMockWithRealNames(t *testing.T) {
	set := loadSet(t, devdata.StateOK)
	uid := set.Newer.Newer[0].RegistryUID
	profile := termRows(t, shown(t, devdata.StateOK, "#p-fred-identity@2.3.0")[0])
	// The card is closed at first, and its summary counts the commands.
	mustHold(t, "the card", shown(t, devdata.StateOK, "#p-fred-identity@2.3.0")[0], `<details class="check" data-open="check"><summary><h2>Check this yourself</h2><span class="sub">5 commands give the same answers in a terminal</span></summary>`)
	if len(profile) != 5 {
		t.Fatalf("the profile page has %d rows, want 5: %+v", len(profile), profile)
	}
	for i, want := range []termRow{
		{"Get the record that this check used", fmt.Sprintf(recordRead, "fred-identity_2.3.0.json.gz"), `<span class="m">rev 4e1c7a2</span>`},
		{"Does Palette still have this version?", "strata verify record.json", `<span class="badge r">Drift</span>`},
		{"Who changed it?", "strata verify record.json --who", ""},
		{"What changed?", "strata compare record.json fred-identity@2.3.0 --full", "1 pack"},
		{"Does this cluster still run the record?", "strata compare record.json cluster:quarry-1/fred-identity", `<span class="badge g">OK</span>`},
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
	mustHold(t, "the move", pack[5].command, "strata compare pack:"+uid+"/dex@2.42.0 pack:"+uid+"/dex@2.44.0 --full")
	mustHold(t, "the question", pack[6].command, "strata behind record.json --pin addon/dex")
	// A pack with no newer version shows neither, and a profile page shows neither.
	for _, rows := range [][]termRow{termRows(t, shown(t, devdata.StateOK, "#k-fred-identity@2.3.0/addon/reloader")[0]), profile} {
		for _, r := range rows {
			if strings.Contains(r.command, "compare pack:") || strings.Contains(r.command, "behind") {
				t.Errorf("a row with no move holds the command %q", r.command)
			}
		}
	}
	// A profile with no drift has no row about who. An old placeholder is gone.
	other := termRows(t, shown(t, devdata.StateOK, "#p-barney-base@1.5.0")[0])
	for _, r := range other {
		mustNotHold(t, "the rows of barney-base", r.command, "--who", "RECORD")
	}
	mustNotHold(t, "the page", shown(t, devdata.StateOK, "#p-fred-identity@2.3.0")[0], "At a terminal", "strata verify RECORD")
}

// The names in the commands are the names of the files of the repository: the four Secrets, the namespace, the key of a record, and
// the options of the commands. The contract test below runs each command against the stand-in.
func TestTheNamesInTheCommandsOfTheCardAreRealNames(t *testing.T) {
	deployment, namespace := string(readFile(t, "../../../deploy/observer/deployment.yaml")), string(readFile(t, "../../../deploy/observer/namespace.yaml"))
	for i := 0; i < 4; i++ {
		if !strings.Contains(deployment, fmt.Sprintf("strata-observer-records-%d", i)) {
			t.Errorf("deploy/observer holds no Secret strata-observer-records-%d", i)
		}
	}
	if !strings.Contains(namespace, "name: strata-observer") {
		t.Error("the namespace of deploy/observer is not strata-observer")
	}
	script := string(readFile(t, "../../../scripts/ci/observer-records.sh"))
	for _, want := range []string{`"\(.source.name)_\(.source.version).json.gz"`, `tr -c 'A-Za-z0-9._\n-' _`, "strata-observer-records-$i"} {
		if !strings.Contains(script, want) {
			t.Errorf("scripts/ci/observer-records.sh no longer holds %q, and the first row of the card reads the key that it writes", want)
		}
	}
	app := string(readPage(t, "app.js"))
	if !strings.Contains(app, "replace(/[^A-Za-z0-9._-]/g, \"_\")") {
		t.Error("the page writes the key of a record by another rule than the script of the records")
	}
	for file, flags := range map[string][]string{"help-behind.txt": {"--pin <LAYER/NAME>"}, "help-verify.txt": {"--who", "--full"}, "help-compare.txt": {"--full"}} {
		help := string(readFile(t, "../../../docs/src/output/"+file))
		for _, f := range flags {
			if !strings.Contains(help, f) {
				t.Errorf("%s has no %s", file, f)
			}
		}
	}
}

// The digest in Upstream reads like the other values, and a blank value reads "none".
func TestTheHeaderOfAPackShowsTheDigestLikeTheOtherValuesAndNoneForABlank(t *testing.T) {
	page := shown(t, devdata.StateOK, "#k-fred-identity@2.3.0/addon/reloader")[0]
	mustHold(t, "the header", page, `<div><span>Upstream</span><span class="m bright">sha256:94ccf3b3b185</span></div>`, `<div><span>Registry</span>Public Repo</div>`)
	css := string(readPage(t, "app.css"))
	if got := ruleOf(css, ".meta div span.bright"); !strings.Contains(got, "color:var(--ink);") || !strings.Contains(got, "display:inline;") {
		t.Errorf("the rule of the digest is %q", got)
	}
	dir := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"profiles.json": func(raw []byte) []byte {
		var v api.ProfilesView
		if err := devdata.Decode(raw, &v); err != nil {
			t.Fatal(err)
		}
		for i := range v.Profiles {
			for j := range v.Profiles[i].Packs {
				if v.Profiles[i].Packs[j].Name == "reloader" {
					v.Profiles[i].Packs[j].Registry, v.Profiles[i].Packs[j].Kind, v.Profiles[i].Packs[j].Upstream = "", "", ""
				}
			}
		}
		out, err := json.Marshal(v)
		if err != nil {
			t.Fatal(err)
		}
		return out
	}})
	blank := pagesOf(t, dir, "#k-fred-identity@2.3.0/addon/reloader")[0]
	mustHold(t, "a blank value", blank, `<div><span>Registry</span>none</div><div><span>Kind</span>none</div><div><span>Upstream</span><span class="m bright">none</span></div>`)
}

// No inline style reaches a screen of any set. The policy of the page stops it, and the color of a slab is a rule of app.css.
func TestNoInlineStyleReachesAScreenOfAnySet(t *testing.T) {
	for _, state := range devdata.States {
		t.Run(string(state), func(t *testing.T) {
			for _, s := range crawl(t, state) {
				if inlineStyle.MatchString(s.HTML) || strings.Contains(s.HTML, "<style") {
					t.Errorf("%s %s holds an inline style", s.Hash, s.Variant)
				}
			}
		})
	}
}

// ---- The stack of the packs ----

var slabPattern = regexp.MustCompile(`<li class="pslab t-([rbgy])( here)?"><a href="([^"]*)"(?: aria-current="page")?><span class="layer">([^<]*)</span><span class="who"><span class="m ref">([^<]*)<span class="ver">@([^<]*)</span></span>`)

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
	for _, m := range regexp.MustCompile(`<tr class="link" data-go="[^"]*"><td class="m"><a href="[^"]*">([^<]+)</a>`).FindAllStringSubmatch(rest, -1) {
		out = append(out, m[1])
	}
	return out
}

// A click on the head of a column sorts the table up, a second click sorts it down, and a third gives back
// the order of the page. A version sorts by SemVer 2.0.0, so 1.20.1 comes before 1.36.2, and a version that
// is no SemVer sorts in natural order.
func TestAHeadSortsItsTableUpThenDownThenBack(t *testing.T) {
	const subject = "barney-base@1.5.0"
	natural := newerOrder(pageOf(t, render(t, devdata.StateOK, false), "#newer", ""), subject)
	up := pageWith(t, "testdata/api", devdata.StateOK, "#newer", `S.sort.nt = {col: "move", dir: 1}`)
	mustHold(t, "up", up, `<th class="" aria-sort="ascending"><button type="button" class="sort on asc" data-sort="nt" data-col="move">Move<svg class="sorti"`)
	if got, want := newerOrder(up, subject), []string{"cni-cilium-oss", "cert-manager", "kubernetes", "csi-aws-ebs", "ubuntu-aws"}; !slices.Equal(got, want) {
		t.Errorf("up: %v, want %v", got, want)
	}
	down := pageWith(t, "testdata/api", devdata.StateOK, "#newer", `S.sort.nt = {col: "move", dir: -1}`)
	mustHold(t, "down", down, `aria-sort="descending"`, `class="sort on desc" data-sort="nt" data-col="move"`)
	if got, want := newerOrder(down, subject), []string{"ubuntu-aws", "csi-aws-ebs", "kubernetes", "cert-manager", "cni-cilium-oss"}; !slices.Equal(got, want) {
		t.Errorf("down: %v, want %v", got, want)
	}
	// The harness clicks twice on each element. So one double click sorts up and then down, and three
	// double clicks go round the cycle twice, back to the order of the page.
	sortMove := `{"sort":"nt","col":"move"}`
	mustHold(t, "a double click", click(t, devdata.StateOK, "#newer", "0", sortMove).After, `aria-sort="descending"`)
	back := click(t, devdata.StateOK, "#newer", "0", sortMove, sortMove, sortMove).After
	mustNotHold(t, "three double clicks", back, `aria-sort="ascending"`, `aria-sort="descending"`)
	if got := newerOrder(back, subject); !slices.Equal(got, natural) {
		t.Errorf("back: %v, want the order of the page %v", got, natural)
	}
	// A pre-release sorts below its release, and numbers compare as numbers.
	order := pageWith(t, "testdata/api", devdata.StateOK, "#profiles", `S.q = ["1.0.0", "1.0.0-rc.10", "0.9.0", "1.0.0-alpha", "1.0.0-rc.2", "1.10.0", "1.2.0"].sort(semverCmp).join(" ")`)
	mustHold(t, "the order of the versions", order, `value="0.9.0 1.0.0-alpha 1.0.0-rc.2 1.0.0-rc.10 1.0.0 1.2.0 1.10.0"`)
}

// A filter of the Newer table keeps a whole group when its profile matches, and else the rows whose pack
// matches. A filter that matches nothing says so.
func TestTheFilterOfTheNewerTableKeepsAGroupOrItsMatchingRows(t *testing.T) {
	byPack := pageWith(t, "testdata/api", devdata.StateOK, "#newer", `S.qs.newer = "cilium"`)
	if got := newerOrder(byPack, "barney-base@1.5.0"); !slices.Equal(got, []string{"cni-cilium-oss"}) {
		t.Errorf("by pack: %v", got)
	}
	mustNotHold(t, "by pack", byPack, `href="#p-fred-identity@2.3.0"`)
	byProfile := pageWith(t, "testdata/api", devdata.StateOK, "#newer", `S.qs.newer = "barney"`)
	if got := newerOrder(byProfile, "barney-base@1.5.0"); len(got) != 5 {
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
// the build says "same" in the table.
func TestTheNewerPageHasTilesATableAndATimeline(t *testing.T) {
	set := loadSet(t, devdata.StateOK)
	table := pageOf(t, render(t, devdata.StateOK, false), "#newer", "")
	mustHold(t, "the tiles", table, `<div class="l">Packs behind</div><div class="v">10</div>`, `<div class="l">Moves that repave</div><div class="v">2</div>`,
		`<div class="l">New this week</div>`, `<div class="l">Versions behind</div><div class="v">16</div>`)
	mustHold(t, "the head", table, `<button data-nview="table" aria-pressed="true" class="on">Table</button>`, `id="q-newer"`)
	mustHold(t, "the row of ubuntu-aws", rowWith(t, table, `data-go="#k-barney-base@1.5.0/os/ubuntu-aws"`), `title="Only the build of the pack changes: the values stay the same">same</span>`)
	timeline := pageWith(t, "testdata/api", devdata.StateOK, "#newer", `S.newerView = "timeline"`)
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
