package web

import (
	"fmt"
	htmlpkg "html"
	"regexp"
	"strconv"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
	"github.com/como-technologies/strata/observer/internal/testkit"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

/* The tests below hold the viewer. They test the text of the diffs in two layouts, the note for a text above
the cap, and the question to Palette. They read the markup that the scripts write, and they check it against a
second read of the same data in Go. */

var tagPattern = regexp.MustCompile(`<[^>]*>`)

// textOf gives the words of a screen: no tag, and each entity as the character that it stands for.
func textOf(markup string) string {
	return htmlpkg.UnescapeString(tagPattern.ReplaceAllString(markup, ""))
}

// cardOf gives the card of a screen that holds the text.
func cardOf(t *testing.T, markup, text string) string {
	t.Helper()
	at := strings.Index(markup, text)
	if at < 0 {
		t.Fatalf("no card holds %q", text)
	}
	start, end := strings.LastIndex(markup[:at], `<section class="card">`), strings.Index(markup[at:], "</section>")
	if start < 0 || end < 0 {
		t.Fatalf("%q is not in a card", text)
	}
	return markup[start : at+end]
}

// The head of the table of the settings, and the classes of the head and of the body of a diff, on the Values tab.
const (
	settingsHead = "<th>Setting</th>"
	diffHead     = `class="diffhead"`
	diffBody     = `class="diff"`
	// diffHeadTag starts the head of a diff, and countsFormat is the format of its counts of added and removed lines.
	diffHeadTag  = "<div " + diffHead + ">"
	countsFormat = `<span class="now">+%d</span> <span class="was">-%d</span>`
)

// diffLine is one line of a hunk, as the second reading of a unified text gives it. The sign "@" stands
// for the head of a hunk that follows another hunk.
type diffLine struct {
	sign, na, nb, text string
	noEOL              bool
}

// readUnified reads a unified text by the rules of the issue: the two header lines come before the first
// hunk, a line "--- x" in a hunk is a removed line, and the line numbers come from the heads of the hunks.
func readUnified(text string) []diffLine {
	var out []diffLine
	var a, b, hunks int
	for _, l := range strings.Split(strings.TrimSuffix(text, "\n"), "\n") {
		if m := testkit.HunkHead.FindStringSubmatch(l); m != nil {
			if hunks > 0 {
				out = append(out, diffLine{sign: "@", text: l})
			}
			hunks++
			a, _ = strconv.Atoi(m[1])
			b, _ = strconv.Atoi(m[3])
			continue
		}
		if hunks == 0 || l == "" {
			continue
		}
		switch l[0] {
		case '\\':
			out[len(out)-1].noEOL = true
		case '-':
			out = append(out, diffLine{sign: "-", na: strconv.Itoa(a), text: l[1:]})
			a++
		case '+':
			out = append(out, diffLine{sign: "+", nb: strconv.Itoa(b), text: l[1:]})
			b++
		default:
			out = append(out, diffLine{sign: " ", na: strconv.Itoa(a), nb: strconv.Itoa(b), text: l[1:]})
			a++
			b++
		}
	}
	return out
}

// The words of a diff that more than one test reads. manifestCell starts the cell of a manifest in the table of the
// changes. notAHeader is a line of a hunk that looks like a header. idTokens is a line of the values of dex that its change
// adds.
const (
	manifestCell = `<td>Manifest</td><td class="m">`
	notAHeader   = "++ not a header either"
	idTokens     = "+        idTokens: 168h\n"
)

// splitPressed is the part of the button of the layout split that says that it is on.
var splitPressed = pressedAttrs("view", layoutSplit, true)

// rowString writes a row of a diff table as the tests compare it: the class of the row, then the class and
// the text of each cell.
func rowString(class string, cells ...[2]string) string {
	parts := make([]string, 0, len(cells))
	for _, c := range cells {
		parts = append(parts, c[0]+":"+c[1])
	}
	return class + "[" + strings.Join(parts, "|") + "]"
}

// unifiedRowsOf gives the rows that the layout Unified must show for a text.
func unifiedRowsOf(text string) []string {
	var out []string
	for _, l := range readUnified(text) {
		if l.sign == "@" {
			out = append(out, rowString("h", [2]string{"no", ""}, [2]string{"no", ""}, [2]string{"c", l.text}))
			continue
		}
		class := map[string]string{"+": "a", "-": "d", " ": ""}[l.sign]
		out = append(out, rowString(class, [2]string{"no", l.na}, [2]string{"no", l.nb}, [2]string{"c", l.sign + " " + l.text}))
		if l.noEOL {
			out = append(out, rowString("n", [2]string{"no", ""}, [2]string{"no", ""}, [2]string{"c", testkit.NoEOL}))
		}
	}
	return out
}

// noteRow is the row of a split layout that says a line has no end of line, on the side of the line.
func noteRow(left, right bool) string {
	word := func(on bool) string {
		if on {
			return testkit.NoEOL
		}
		return ""
	}
	return rowString("n", [2]string{"no", ""}, [2]string{"", word(left)}, [2]string{"no", ""}, [2]string{"", word(right)})
}

// splitRowsOf gives the rows that the layout Split must show for a text. A run of removed and added lines
// pairs the removed lines on the left with the added lines on the right.
func splitRowsOf(text string) []string {
	var out []string
	var dels, adds []diffLine
	flush := func() {
		for i := 0; i < len(dels) || i < len(adds); i++ {
			left, right := [3]string{"", "", ""}, [3]string{"", "", ""}
			if i < len(dels) {
				left = [3]string{dels[i].na, "cd", dels[i].text}
			}
			if i < len(adds) {
				right = [3]string{adds[i].nb, "ca", adds[i].text}
			}
			out = append(out, rowString("", [2]string{"no", left[0]}, [2]string{left[1], left[2]}, [2]string{"no", right[0]}, [2]string{right[1], right[2]}))
		}
		var left, right bool
		for _, d := range dels {
			left = left || d.noEOL
		}
		for _, a := range adds {
			right = right || a.noEOL
		}
		if left || right {
			out = append(out, noteRow(left, right))
		}
		dels, adds = nil, nil
	}
	for _, l := range readUnified(text) {
		switch l.sign {
		case "-":
			dels = append(dels, l)
		case "+":
			adds = append(adds, l)
		case "@":
			flush()
			out = append(out, rowString("h", [2]string{"", l.text}))
		default:
			flush()
			out = append(out, rowString("", [2]string{"no", l.na}, [2]string{"", l.text}, [2]string{"no", l.nb}, [2]string{"", l.text}))
			if l.noEOL {
				out = append(out, noteRow(true, true))
			}
		}
	}
	flush()
	return out
}

var (
	diffTable = regexp.MustCompile(`(?s)<div ` + diffBody + `><table class="(?:` + layoutUnified + `|` + layoutSplit + `)"><colgroup>.*?</colgroup><tbody>(.*?)</tbody></table></div>`)
	diffRow   = regexp.MustCompile(`<tr(?: class="([^"]*)")?>(.*?)</tr>`)
	diffCell  = regexp.MustCompile(`<td(?: class="([^"]*)")?(?: colspan="4")?>([^<]*)</td>`)
)

// diffRowsOf gives the rows of each diff in the markup, in order, in the form of rowString.
func diffRowsOf(markup string) []string {
	var out []string
	for _, table := range diffTable.FindAllStringSubmatch(markup, -1) {
		for _, row := range diffRow.FindAllStringSubmatch(table[1], -1) {
			var cells [][2]string
			for _, c := range diffCell.FindAllStringSubmatch(row[2], -1) {
				cells = append(cells, [2]string{c[1], htmlpkg.UnescapeString(c[2])})
			}
			out = append(out, rowString(row[1], cells...))
		}
	}
	return out
}

// sameRows fails for the first row that differs.
func sameRows(t *testing.T, where string, got, want []string) {
	t.Helper()
	if len(got) != len(want) {
		t.Errorf("%s: %d rows, want %d", where, len(got), len(want))
	}
	for i := 0; i < len(got) && i < len(want); i++ {
		if got[i] != want[i] {
			t.Errorf("%s: row %d is %q, want %q", where, i, got[i], want[i])
			return
		}
	}
}

// layoutRows gives the rows that a layout must show for a text, and the variant of the crawl that sets it.
func layoutRows(layout, text string) []string {
	if layout == layoutSplit {
		return splitRowsOf(text)
	}
	return unifiedRowsOf(text)
}

// headCounts gives the number of added and removed lines in a text.
func headCounts(text string) (added, removed int) {
	for _, l := range readUnified(text) {
		switch l.sign {
		case "+":
			added++
		case "-":
			removed++
		}
	}
	return added, removed
}

// diffOf finds the entry of the detail route that a set holds for a side.
func diffOf(t *testing.T, set *devdata.Set, profile, key string, side model.Side) api.PackDiff {
	t.Helper()
	return *entryOf(t, set.Diffs, profile, key, side)
}

const (
	dexHash       = "#k-" + testkit.DexProfile + "/" + testkit.DexKey
	k8sHash       = "#k-" + testkit.BaseProfile + "/" + testkit.KubernetesKey
	traefikHash   = "#k-" + testkit.GatewayProfile + "/" + testkit.TraefikKey
	vendorHash    = "#k-" + testkit.GatewayProfile + "/" + testkit.VendorPack
	layoutUnified = "unified"
	layoutSplit   = "split"
)

// The ids of the tabs of the published side of dex, and of the moves of dex, kubernetes, and traefik.
var (
	dexPublished = tabId(api.PackDiff{Profile: testkit.DexProfile, Key: testkit.DexKey, Side: model.SidePublished})
	dexMove      = tabId(api.PackDiff{Profile: testkit.DexProfile, Key: testkit.DexKey, Side: model.SideNewer})
	k8sMove      = tabId(api.PackDiff{Profile: testkit.BaseProfile, Key: testkit.KubernetesKey, Side: model.SideNewer})
	traefikMove  = tabId(api.PackDiff{Profile: testkit.GatewayProfile, Key: testkit.TraefikKey, Side: model.SideNewer})
)

// fredHash is the hash of the page of the profile of dex. dexManifest is the head of the manifest of dex that the change of the
// published version changes.
const (
	fredHash    = "#p-" + testkit.DexProfile
	dexManifest = "manifest fred-realm-config"
)

// The hashes of the packs csi-aws-ebs of barney-base and reloader of fred-identity.
const (
	csiHash      = "#k-" + testkit.BaseProfile + "/" + testkit.CSIKey
	reloaderHash = "#k-" + testkit.DexProfile + "/addon/reloader"
	// splitClick and unifiedClick are the clicks on the two layouts of a diff.
	splitClick   = `{"view":"split"}`
	unifiedClick = `{"view":"unified"}`
)

// The bodies of a click on "Ask Palette" for a move of the dev data.
var (
	k8sClick    = askClick(testkit.BaseProfile, testkit.KubernetesKey)
	dexClick    = askClick(testkit.DexProfile, testkit.DexKey)
	ubuntuClick = askClick(testkit.BaseProfile, testkit.UbuntuAWSKey)
	csiClick    = askClick(testkit.BaseProfile, testkit.CSIKey)
)

// askClick gives the body of a click on "Ask Palette" for the move of the pack key of the profile.
func askClick(profile, key string) string { return `{"ask":"` + profile + `","pack":"` + key + `"}` }

// tabClick gives the click on the tab k of the tabs id.
func tabClick(id, k string) string { return `{"tab":"` + id + `","k":"` + k + `"}` }

// variantOf gives the variant of the crawl that sets a side of the dropdown, a layout, and a tab or none. The side is ""
// for the side that the pack opens on.
func variantOf(side, layout, tab, k string) string {
	prefix := ""
	if side != "" {
		prefix = "side " + side + " "
	}
	switch {
	case tab == "" && layout == layoutSplit:
		return prefix + layoutSplit
	case tab == "":
		return strings.TrimSuffix(prefix, " ")
	case layout == layoutSplit:
		return prefix + layoutSplit + " tab " + tab + " " + k
	}
	return prefix + "tab " + tab + " " + k
}

// The Values tab shows the raw text of the values, after the table of the settings and the masked note, and
// before the folded parts. Both layouts show each line with its numbers and its sign, and the head counts
// the added and the removed lines.
func TestTheValuesTabShowsTheRawTextInBothLayouts(t *testing.T) {
	set := loadSet(t, devdata.StateOK)
	screens := render(t, devdata.StateOK, false)
	// The dropdown of a pack opens on the first side with a change. Dex has a change on the published side and a newer
	// version, so the newer side shows when the person chooses it. The other two packs open on the newer side.
	for _, c := range []struct {
		hash, tab, profile, key string
		side                    model.Side
		open                    string
	}{
		{dexHash, dexPublished, testkit.DexProfile, testkit.DexKey, model.SidePublished, ""},
		{dexHash, dexMove, testkit.DexProfile, testkit.DexKey, model.SideNewer, string(model.SideNewer)},
		{k8sHash, k8sMove, testkit.BaseProfile, testkit.KubernetesKey, model.SideNewer, ""},
		{traefikHash, traefikMove, testkit.GatewayProfile, testkit.TraefikKey, model.SideNewer, ""},
	} {
		text := diffOf(t, set, c.profile, c.key, c.side).Values.Unified
		if text == "" {
			t.Fatalf("the set holds no text for %s %s", c.profile, c.side)
		}
		for _, layout := range []string{layoutUnified, layoutSplit} {
			where := c.tab + " " + layout
			card := cardOf(t, pageOf(t, screens, c.hash, variantOf(c.open, layout, "", "")), tabOf(c.tab))
			sameRows(t, where, diffRowsOf(card), layoutRows(layout, text))
			added, removed := headCounts(text)
			mustHold(t, where, card, diffHeadTag+span(fmt.Sprintf(`%s values `+countsFormat, c.key, added, removed)),
				tabAttrs(c.tab, "values")+">Values")
			mustHoldInOrder(t, where+": the table of the settings, then the text", card, settingsHead, diffHead)
		}
	}
	// The order on the Values tab: the table of the settings, the masked note, the text, and then the folded parts.
	dex := cardOf(t, pageOf(t, screens, dexHash, ""), tabOf(dexPublished))
	mustHoldInOrder(t, "the Values tab of dex", dex, settingsHead, "Masked:", diffHead)
	k8s := cardOf(t, pageOf(t, screens, k8sHash, ""), tabOf(k8sMove))
	mustHoldInOrder(t, "the Values tab of kubernetes", k8s, diffHead, "<details")
	// The text of kubernetes holds three hunks, so the text shows two gaps.
	if got := len(regexp.MustCompile(`<tr class="h">`).FindAllString(k8s, -1)); got != 2 {
		t.Errorf("the text of kubernetes shows %d gap rows, want 2", got)
	}
}

// The Manifests tab shows the text of each manifest with the operation in its head. A parameter has a row
// in the small table, and a manifest with text has none.
func TestTheManifestsTabShowsTheTextOfEachManifestInBothLayouts(t *testing.T) {
	const manifestLines = "%s manifest has %d added and %d removed lines"
	set := loadSet(t, devdata.StateOK)
	screens := render(t, devdata.StateOK, false)
	for _, c := range []struct {
		tab, profile string
		side         model.Side
		table        []string
		open         string
	}{
		{dexPublished, testkit.DexProfile, model.SidePublished, nil, ""},
		{dexMove, testkit.DexProfile, model.SideNewer, []string{`<td class="m">added</td><td>Parameter</td><td class="m">dex_hostname</td>`}, string(model.SideNewer)},
	} {
		d := diffOf(t, set, c.profile, testkit.DexKey, c.side)
		var want, wantSplit []string
		var heads []string
		for _, m := range d.Manifests {
			if m.Unified == "" {
				t.Fatalf("the manifest %s has no text", m.Name)
			}
			want = append(want, unifiedRowsOf(m.Unified)...)
			wantSplit = append(wantSplit, splitRowsOf(m.Unified)...)
			added, removed := headCounts(m.Unified)
			heads = append(heads, diffHeadTag+span(fmt.Sprintf(`manifest %s <span class="mut">%s</span> `+countsFormat, m.Name, m.Op, added, removed)))
		}
		for _, layout := range []string{layoutUnified, layoutSplit} {
			where := c.tab + " " + layout
			page := pageOf(t, screens, dexHash, variantOf(c.open, layout, c.tab, "manifests"))
			card := cardOf(t, page, tabAttrs(c.tab, "manifests"))
			rows := want
			if layout == layoutSplit {
				rows = wantSplit
			}
			sameRows(t, where, diffRowsOf(card), rows)
			mustHold(t, where, card, heads...)
			mustHold(t, where, card, c.table...)
			if len(c.table) == 0 {
				mustNotHold(t, where, card, "<th>Change</th>")
			}
			// A manifest with text has no row in the small table.
			for _, m := range d.Manifests {
				mustNotHold(t, where, card, manifestCell+m.Name+`</td>`)
			}
		}
	}
	// The ops of the manifests show in the heads: changed, added, and removed.
	d := diffOf(t, set, testkit.DexProfile, testkit.DexKey, model.SideNewer)
	if len(d.Manifests) != 2 || d.Manifests[0].Op != "added" || d.Manifests[1].Op != "removed" {
		t.Fatalf("the manifests of the newer side: %+v", d.Manifests)
	}
	// An added manifest is all added lines, and a removed manifest is all removed lines.
	added, removed := headCounts(d.Manifests[0].Unified)
	if added == 0 || removed != 0 {
		t.Errorf(manifestLines, "an added", added, removed)
	}
	added, removed = headCounts(d.Manifests[1].Unified)
	if added != 0 || removed == 0 {
		t.Errorf(manifestLines, "a removed", added, removed)
	}
}

// The line "\ No newline at end of file" is a muted note row with no number, and it counts as no line. A
// head of a hunk shows the gap, a line "--- x" in a hunk is a removed line, and a head with a missing
// count or with an empty side gives the right numbers.
func TestTheReadingOfTheTextFollowsTheRulesOfTheIssue(t *testing.T) {
	const text = testkit.DexValuesHead +
		"@@ -0,0 +1,2 @@\n+one\n+two\n" + testkit.NoEOL + "\n" +
		"@@ -7 +9,2 @@\n--- not a header\n+" + notAHeader + "\n+added\n" +
		"@@ -20,3 +22,3 @@\n keep\n-old\n" + testkit.NoEOL + "\n+new\n" + testkit.NoEOL + "\n keep\n"
	dir := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"diffs.json": editDiffs(t, func(ds []api.PackDiff) {
		entryOf(t, ds, testkit.DexProfile, testkit.DexKey, model.SidePublished).Values.Unified = text
	})})
	for _, layout := range []string{layoutUnified, layoutSplit} {
		clicks := []string{}
		if layout == layoutSplit {
			clicks = append(clicks, splitClick)
		}
		card := cardOf(t, clickIn(t, dir, scratch, dexHash, "0", clicks...).After, tabOf(dexPublished))
		sameRows(t, layout, diffRowsOf(card), layoutRows(layout, text))
		mustHold(t, layout, card, fmt.Sprintf(countsFormat, 5, 2))
	}
	// The reference reading follows the rules too, so the two readings do not agree by accident.
	lines := readUnified(text)
	if len(lines) != 11 || lines[2].sign != "@" || lines[3].sign != "-" || lines[3].text != "-- not a header" || lines[3].na != "7" ||
		lines[4].sign != "+" || lines[4].text != notAHeader || lines[4].nb != "9" || lines[5].nb != "10" || lines[0].nb != "1" || lines[1].nb != "2" {
		t.Errorf("the lines: %+v", lines)
	}
}

// entryOf finds an entry in a list of diffs that a test edits.
func entryOf(t *testing.T, ds []api.PackDiff, profile, key string, side model.Side) *api.PackDiff {
	t.Helper()
	for i := range ds {
		if ds[i].Profile == profile && ds[i].Key == key && ds[i].Side == side {
			return &ds[i]
		}
	}
	t.Fatalf(testkit.NoDiff, profile, key, side)
	return nil
}

// editDiffs changes the diffs of a set in place.
func editDiffs(t *testing.T, change func([]api.PackDiff)) func([]byte) []byte {
	return editJSON(t, func(ds *[]api.PackDiff) { change(*ds) })
}

// clicked is what render.js prints for the mode click.
type clicked struct {
	Before, During, After string
	Hash                  string
	Jumps                 []string
	Copies                []string
	Posts                 []struct {
		Method  string   `json:"method"`
		Path    string   `json:"path"`
		Headers []string `json:"headers"`
	}
}

// click shows a screen of a set, clicks the elements that the data attributes name, and gives what the
// page showed before, during, and after. status is the HTTP status of a POST that fails, "network", or "0".
func click(t *testing.T, state devdata.State, hash, status string, clicks ...string) clicked {
	t.Helper()
	return clickIn(t, "testdata/api", state, hash, status, clicks...)
}

func clickIn(t *testing.T, dir string, state devdata.State, hash, status string, clicks ...string) clicked {
	t.Helper()
	return nodeJSON[clicked](t, runNodeIn(t, dir, state, append([]string{"click", hash, status}, clicks...)...))
}

// onClass gives the class of a button that is on, or no class.
func onClass(on bool) string {
	if on {
		return "on"
	}
	return ""
}

// pressedButton gives a button that sets how the page shows a view, with its label. The attribute data-<attr> names the
// view, and the button is pressed when on is true.
func pressedButton(attr, view, label string, on bool) string {
	return `<button ` + pressedAttrs(attr, view, on) + ` class="` + onClass(on) + `">` + label + `</button>`
}

// pressedAttrs gives the attributes of pressedButton that name the view and say if the button is pressed. pressedAttrsOf
// gives them with the text of aria-pressed, such as a pattern.
func pressedAttrs(attr, view string, on bool) string {
	return pressedAttrsOf(attr, view, strconv.FormatBool(on))
}

func pressedAttrsOf(attr, view, pressed string) string {
	return viewAttr(attr, view) + ` aria-pressed="` + pressed + `"`
}

// viewAttr gives the attribute data-<attr> of a button that names the view.
func viewAttr(attr, view string) string { return `data-` + attr + `="` + view + `"` }

// Both layouts have the same control in the head of each diff, on the Values tab and on the Manifests
// tab. One state serves the whole page: a click on Split changes each diff, and the choice stays when the
// person changes the tab.
func TestTheControlOfTheLayoutSitsInEachDiffHeadAndSetsOneLayoutForThePage(t *testing.T) {
	const seg = `<span class="seg" role="group" aria-label="Diff layout">`
	screens := render(t, devdata.StateOK, false)
	for _, c := range []struct{ variant, where string }{{"", "values"}, {variantOf("", layoutUnified, dexPublished, "manifests"), "manifests"}, {variantOf(string(model.SideNewer), layoutUnified, dexMove, "manifests"), "manifests of the move"}} {
		page := pageOf(t, screens, dexHash, c.variant)
		heads := strings.Count(page, diffHeadTag)
		if heads == 0 || strings.Count(page, seg) != heads {
			t.Errorf("%s: %d heads and %d controls", c.where, heads, strings.Count(page, seg))
		}
		mustHold(t, c.where, page, pressedButton("view", layoutUnified, "Unified", true)+pressedButton("view", layoutSplit, "Split", false))
		mustNotHold(t, c.where, page, pressedButton("view", layoutSplit, "Split", true))
	}
	split := pageOf(t, screens, dexHash, layoutSplit)
	mustHold(t, "split", split, pressedButton("view", layoutUnified, "Unified", false)+pressedButton("view", layoutSplit, "Split", true))
	mustNotHold(t, "split", split, `<td class="c">`)

	// A click on Split changes each diff of the page and nothing else. A click on a tab keeps the layout.
	r := click(t, devdata.StateOK, dexHash, "0", splitClick)
	mustHold(t, "before the click", r.Before, `<td class="c">`)
	mustNotHold(t, "before the click", r.Before, `class="cd"`)
	mustHold(t, "after the click", r.After, `class="cd"`, `class="ca"`)
	mustNotHold(t, "after the click", r.After, `<td class="c">`)
	if got, want := strings.Count(r.After, splitPressed), strings.Count(r.After, diffHeadTag); got != want || got < 1 {
		t.Errorf("%d controls show Split for %d diffs", got, want)
	}
	if len(r.Posts) != 0 {
		t.Errorf("a click on the layout sent %v", r.Posts)
	}
	r = click(t, devdata.StateOK, dexHash, "0", splitClick, tabClick(dexPublished, "manifests"))
	where := "manifests after Split"
	mustHold(t, where, r.After, `class="cd"`, dexManifest)
	mustNotHold(t, where, r.After, `<td class="c">`)
	r = click(t, devdata.StateOK, dexHash, "0", splitClick, unifiedClick)
	mustHold(t, "back to Unified", r.After, `<td class="c">`)
	mustNotHold(t, "back to Unified", r.After, `class="cd"`)
}

// A line of a diff with the text of a tag shows as text, in both layouts, in a value and in a manifest.
func TestALineOfADiffWithMarkupShowsAsText(t *testing.T) {
	dir := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"diffs.json": editDiffs(t, func(ds []api.PackDiff) {
		d := entryOf(t, ds, testkit.DexProfile, testkit.DexKey, model.SidePublished)
		d.Values.Unified = strings.Replace(d.Values.Unified, idTokens, idTokens+"+"+evil+"\n-"+evil+"\n context "+evil+"\n", 1)
		d.Manifests[0].Unified = strings.Replace(d.Manifests[0].Unified, "@@ -5,5 +5,5 @@\n", "@@ -5,5 +5,5 @@ "+evil+"\n+"+evil+"\n", 1) + "@@ -90 +90 @@ " + evil + "\n-" + evil + "\n+" + evil + "\n"
	})})
	for _, clicks := range [][]string{nil, {splitClick}, {tabClick(dexPublished, "manifests")}, {splitClick, tabClick(dexPublished, "manifests")}} {
		r := clickIn(t, dir, scratch, dexHash, "0", clicks...)
		where := fmt.Sprint("clicks ", clicks)
		checkMarkup(t, screen{Hash: where, HTML: r.After})
		mustHold(t, where, r.After, injection)
		mustNotHold(t, where, r.After, "<img")
		if got := strings.Count(strings.Join(diffRowsOf(r.After), "\n"), evil); got < 3 {
			t.Errorf("%s: %d rows hold the text of the tag", where, got)
		}
	}
}

// registryUidOf gives the registry of the row of the Newer table that moves a pack.
func registryUidOf(t *testing.T, set *devdata.Set, name string) string {
	t.Helper()
	for _, r := range set.Newer.Newer {
		if r.Name == name {
			return r.RegistryUID
		}
	}
	t.Fatalf("the Newer table has no row for %s", name)
	return ""
}

// A pack whose text is above the cap shows a splash and the command for the side, in place of the diff. The
// command is the one that prints the text: the published side compares the record, the cluster side
// compares the record with the copy of the cluster, and the newer side compares two versions of the pack.
func TestATextAboveTheCapShowsTheSplashAndTheCommandOfItsSide(t *testing.T) {
	set := loadSet(t, devdata.StateTextTooLarge)
	screens := render(t, devdata.StateTextTooLarge, false)
	tooLarge := placeNamed(t, placeTooLarge)
	uid := registryUidOf(t, set, "kubernetes")
	published := cardOf(t, pageOf(t, screens, dexHash, ""), tabOf(dexPublished))
	where := "the values of dex"
	checkPlace(t, tooLarge, published)
	mustNotHold(t, where, published, diffHead, diffBody, `data-view=`)
	// The manifest keeps its row in the small table, because it has no text, and the splash comes with the command.
	manifests := cardOf(t, pageOf(t, screens, dexHash, variantOf("", layoutUnified, dexPublished, "manifests")), tabAttrs(dexPublished, "manifests"))
	where = "the manifests of dex"
	checkPlace(t, tooLarge, manifests)
	mustHold(t, where, manifests, `<tr><td class="m">changed</td>`+manifestCell+`fred-realm-config</td></tr>`)
	mustNotHold(t, where, manifests, diffHead, diffBody)
	// The newer side of the same pack keeps its text.
	move := cardOf(t, pageOf(t, screens, dexHash, variantOf(string(model.SideNewer), layoutUnified, "", "")), tabOf(dexMove))
	where = "the move of dex"
	mustHold(t, where, move, diffHead)
	mustNotHold(t, where, move, tooLargeTitle)
	// The newer side of kubernetes has the command with the two versions of the pack.
	k8s := cardOf(t, pageOf(t, screens, k8sHash, ""), tabOf(k8sMove))
	where = testkit.KubernetesValues
	tooLarge.steps = []string{"<code>" + comparePacks + uid + "/" + testkit.KubernetesFrom + " pack:" + uid + "/" + testkit.KubernetesTo + " " + runner.FlagFull + "</code>"}
	checkPlace(t, tooLarge, k8s)
	mustNotHold(t, where, k8s, diffHead)
	// The pack has no manifest, so the Manifests tab says so, links to the tab with the change, and shows no splash of a large text.
	none := cardOf(t, pageOf(t, screens, k8sHash, variantOf("", layoutUnified, k8sMove, "manifests")), tabAttrs(k8sMove, "manifests"))
	where = "the manifests of kubernetes"
	mustHold(t, where, none, "No manifest"+changedHere, tabLink(k8sMove, "values", "Values"))
	mustNotHold(t, where, none, tooLargeTitle)
	// The set ok has the text for the same packs.
	for _, s := range render(t, devdata.StateOK, false) {
		mustNotHold(t, s.Hash+" "+s.Variant, s.HTML, tooLargeTitle)
	}
}

// dexClusterTooLarge edits the diffs of a set: the text of the cluster side of dex is above the cap, so the diff holds no
// text.
func dexClusterTooLarge(t *testing.T) func([]byte) []byte {
	return editDiffs(t, func(ds []api.PackDiff) {
		d := entryOf(t, ds, testkit.DexProfile, testkit.DexKey, model.SideCluster)
		d.Values.Unified, d.TextTooLarge = "", true
	})
}

// The cluster side compares the record with the copy of the cluster, and the profile page already names
// this command.
func TestTheCommandOfTheClusterSideNamesTheCluster(t *testing.T) {
	set := loadSet(t, devdata.StateReadError)
	dir := writeSet(t, devdata.StateReadError, map[string]func([]byte) []byte{"diffs.json": dexClusterTooLarge(t)})
	command := compareCluster + set.Cluster.Cluster.Name + "/fred-identity"
	r := clickIn(t, dir, scratch, dexHash, "0")
	card := cardOf(t, r.After, tabOf(tabId(api.PackDiff{Profile: testkit.DexProfile, Key: testkit.DexKey, Side: model.SideCluster})))
	mustHold(t, "the cluster side", card, tooLargeNote, "<code>"+command+" "+runner.FlagFull+"</code>")
	mustNotHold(t, "the cluster side", card, diffHead)
	// The profile page shows the same command, with the pack name left out and no flag.
	profile := pagesOf(t, dir, fredHash)[0]
	mustHold(t, "the profile", profile, "<code>"+command+"</code>")
}

// A command with a name that holds markup shows as text. The name has spaces and a "<", so the command puts it in single quotes,
// and the page escapes each quote.
func TestTheCommandOfATextAboveTheCapShowsNamesAsText(t *testing.T) {
	swap := replaceText("quarry-1", evil)
	dir := writeSet(t, devdata.StateReadError, map[string]func([]byte) []byte{"cluster.json": swap, "diffs.json": dexClusterTooLarge(t)})
	page := clickIn(t, dir, scratch, dexHash, "0").After
	mustHold(t, "the command", page, `&#39;cluster:`+injection+`/fred-identity&#39; `+runner.FlagFull)
	mustNotHold(t, "the command", page, "<img")
}

// The Newer table has the column Palette with the button "Ask Palette" in each row, and the card "Move to"
// of a pack has the button "Ask Palette about this move". A set that holds no answer shows no answer.
func TestTheNewerTableAndTheCardOfAMoveOfferTheQuestion(t *testing.T) {
	set := loadSet(t, devdata.StateOK)
	screens := render(t, devdata.StateOK, false)
	table := pageOf(t, screens, "#newer", "")
	mustHold(t, "the table", table, sortButton("nt", "listed", "", "Listed")+sortIcon, `</svg></button></th><th class="narrow-hide" title="Asks Palette whether it would take this move on the clusters that run this profile. It changes nothing.">Palette</th></tr></thead>`)
	if got := strings.Count(table, askButtonEnd); got != len(set.Newer.Newer) {
		t.Errorf("the table has %d buttons for %d rows", got, len(set.Newer.Newer))
	}
	mustNotHold(t, "the table", table, wordNoError, wordRefuses, wordNoAnswer, paletteGave)
	// The group rows span the new column too, and each row keeps its cell in the class narrow-hide.
	mustHold(t, "the table", table, `<tr class="g"><td colspan="8">`)
	mustNotHold(t, "the table", table, `colspan="7"`)
	for _, row := range strings.Split(table, "</tr>") {
		if strings.Contains(row, goToStart+"#k-") {
			mustHold(t, "a row with a link", row, narrowCell+rowButton)
		}
	}
	// The button is in the row, which opens the pack: its data attributes name the move.
	row := rowTo(t, table, dexHash)
	mustHold(t, "the row of dex", row, rowButton+dexMoveAttrs+askButtonEnd)
	for _, hash := range []string{dexHash, k8sHash, traefikHash} {
		page := pageOf(t, screens, hash, "")
		mustHold(t, hash, page, cardButton, askThisMoveEnd)
		mustNotHold(t, hash, page, paletteGave, paletteRefuses, asking)
	}
	// A pack with no move has no button.
	mustNotHold(t, "a pack with no move", pageOf(t, screens, reloaderHash, ""), askButton)
}

// The words of the column "Palette" and of the card "Move to" for a move and its answer.
const (
	askButton      = "Ask Palette"
	askThisMove    = askButton + " about this move"
	askAgain       = "Ask again"
	paletteGave    = "Palette gave"
	gaveNoError    = paletteGave + " no error"
	gaveNoAnswer   = paletteGave + " no answer"
	gave403        = gaveNoAnswer + ": HTTP 403."
	paletteRefuses = "Palette refuses"
	nothingChanged = "Nothing on this cluster changed."
	wordNoError    = "No error"
	wordRefuses    = "Refuses"
	wordNoAnswer   = "No answer"
	askButtonEnd   = ">" + askButton + "</button>"
	asking         = "Asking..."
	couldNotAsk    = "The observer could not ask now"
	// askFailed is the note of a question that the page could not send, or that the observer could not answer.
	askFailed   = couldNotAsk + ". Try again later."
	cannotReach = "cannot reach the observer"
	// refusesWithCode is the head of the note of the refusal of the dev data.
	refusesWithCode = paletteRefuses + ": " + testkit.CodeDependency
)

// The buttons of the question. cardButton and rowButton start a button of the card "Move to" and of a row of the table.
// dexMoveAttrs are the data attributes of the move of dex. askThisMoveEnd, askAgainEnd, and askingEnd close a button, and
// neutralNote starts the note of an answer that failed.
const (
	cardButton     = `<button class="btn" data-ask="`
	rowButton      = `<button class="lnk" data-ask="`
	dexMoveAttrs   = testkit.DexProfile + `" data-pack="` + testkit.DexKey + `"`
	askThisMoveEnd = ">" + askThisMove + "</button>"
	askAgainEnd    = ">" + askAgain + "</button>"
	askingEnd      = " disabled>" + asking + "</button>"
	neutralNote    = `<div class="note small"><b>`
)

// The word of each answer in the column "Palette", and the cell of each answer of the set asks. narrowCell starts a cell
// that a phone hides, such as the cell of the column "Palette".
const (
	noErrorSpan  = `<span class="fw">` + wordNoError + `</span>`
	refusesSpan  = `<span class="fw">` + wordRefuses + `</span>`
	noAnswerSpan = `<span class="mut">` + wordNoAnswer + `</span>`
	narrowCell   = `<td class="narrow-hide">`
	noErrorCell  = narrowCell + noErrorSpan + `</td>`
	refusesCell  = narrowCell + refusesSpan + `</td>`
	noAnswerCell = narrowCell + noAnswerSpan + " " + rowButton + dexMoveAttrs + askAgainEnd + "</td>"
)

// noErrorCovers is the note of the card "Move to" for the answer "no error": what Palette checked, and what it did not check.
const noErrorCovers = gaveNoError + ". Palette checked the profile with the new version and its default values against each cluster that runs the profile. " + notYourValues

// notYourValues ends the note of the answer "no error": Palette did not check the values of a person.
const notYourValues = "It did not check your own values of the pack."

// The set asks holds one answer of each verdict. The Newer table and the card say each answer in its
// words, and the word "accepts" is not there. A request that failed is no finding.
func TestTheAnswersOfPaletteReadExactlyAsTheIssueSays(t *testing.T) {
	set := loadSet(t, devdata.StateAsks)
	screens := render(t, devdata.StateAsks, false)
	table := pageOf(t, screens, "#newer", "")
	refusal := set.Asks.Asks[1].Refusals[0]
	if refusal.Code != testkit.CodeDependency || set.Asks.Asks[2].Status != 403 {
		t.Fatalf("the data of the set changed: %+v", set.Asks.Asks)
	}
	// The table: the word of each answer in the column Palette, and the button where it is not an answer.
	for _, c := range []struct{ hash, cell string }{
		{k8sHash, noErrorCell},
		{traefikHash, refusesCell},
		{dexHash, noAnswerCell},
	} {
		mustHold(t, c.hash, rowTo(t, table, c.hash), c.cell)
	}
	if got, want := strings.Count(table, askButtonEnd), len(set.Newer.Newer)-3; got != want {
		t.Errorf("%d rows offer the question, want %d", got, want)
	}
	// The cards: the words of the issue, exactly.
	noError := textOf(cardOf(t, pageOf(t, screens, k8sHash, ""), gaveNoError))
	mustHold(t, "no error", noError, noErrorCovers)
	mustNotHold(t, "no error", noError, "It read default values", askThisMove)
	refuses := textOf(cardOf(t, pageOf(t, screens, traefikHash, ""), paletteRefuses))
	mustHold(t, "refuses", refuses, paletteRefuses+": "+refusal.Code, refusal.Message, "on "+refusal.Clusters[0], nothingChanged)
	mustNotHold(t, "refuses", refuses, askAgain, askThisMove)
	mustHoldInOrder(t, "refuses", refuses, refusal.Message, "on "+refusal.Clusters[0])
	dex := pageOf(t, screens, dexHash, "")
	noAnswer := textOf(cardOf(t, dex, gaveNoAnswer))
	mustHold(t, "no answer", noAnswer, gave403, askAgain)
	mustHold(t, "no answer", dex, cardButton+dexMoveAttrs+askAgainEnd)
	// A request that failed is neutral: no red, no badge, no note of a finding.
	block := dex[strings.Index(dex, gaveNoAnswer)-40:]
	block = block[:strings.Index(block, "</div></div>")]
	mustHold(t, "no answer", block, neutralNote+gaveNoAnswer)
	mustNotHold(t, "no answer", block, "badge", `note r`, `note a`, "Drift")
}

// noMoveScreens are the screens that show no move. They show no answer of Palette and no check of strata.
var noMoveScreens = []string{"#overview", "#findings", "#profiles", "#notchecked", "#images"}

// sameScreensWithNoMove fails when a screen that shows no move has another nav or other tiles in got than in want,
// or holds one of the words in got. The nav and the tiles hold every count.
func sameScreensWithNoMove(t *testing.T, want, got []screen, words ...string) {
	t.Helper()
	for _, hash := range noMoveScreens {
		a, b := pageOf(t, want, hash, ""), pageOf(t, got, hash, "")
		sameNav(t, hash, a, b)
		if tileSection.FindString(a) != tileSection.FindString(b) {
			t.Errorf("%s: the tiles differ", hash)
		}
		mustNotHold(t, hash, b, words...)
	}
}

// sameNav fails when two pages have another nav. The nav is the first line of a page.
func sameNav(t *testing.T, where, a, b string) {
	t.Helper()
	navA, _, _ := strings.Cut(a, "\n")
	navB, _, _ := strings.Cut(b, "\n")
	if navA != navB {
		t.Errorf("%s: the nav differs\n%s\n%s", where, navA, navB)
	}
}

// A no-answer raises no severity. The summary tier does not see an answer: the screens that do not show a
// move are the same as in ok, and the rows of the move hold no badge for it.
func TestAnAnswerRaisesNoSeverityAndANoAnswerIsNoFinding(t *testing.T) {
	ok, asks := render(t, devdata.StateOK, false), render(t, devdata.StateAsks, false)
	sameScreensWithNoMove(t, ok, asks, wordNoAnswer, wordRefuses, paletteRefuses, paletteGave)
	table := pageOf(t, asks, "#newer", "")
	dexRow := rowTo(t, table, dexHash)
	mustNotHold(t, "the row of a no-answer", dexRow, badgeOf("r"), ">Drift<", wordNotChecked)
	// The same counts on the cards of the pack: one drift pack, as in ok.
	sameNav(t, "the pack", pageOf(t, ok, dexHash, ""), pageOf(t, asks, dexHash, ""))
}

// A click asks once. The button is off and reads "Asking..." while the question is on its way, a second click
// sends nothing, and the answer shows beside the move, in the table and on the card.
func TestAClickSendsOneQuestionAndTheAnswerShowsBesideTheMove(t *testing.T) {
	r := click(t, devdata.StateOK, dexHash, "0", dexClick)
	mustHold(t, "before", r.Before, cardButton+dexMoveAttrs+askThisMoveEnd)
	mustHold(t, "during", r.During, cardButton+dexMoveAttrs+askingEnd)
	mustNotHold(t, "during", r.During, askThisMove, paletteGave)
	mustHold(t, "after", textOf(r.After), noErrorCovers)
	mustNotHold(t, "after", r.After, asking, askThisMove, couldNotAsk)
	if len(r.Posts) != 1 || r.Posts[0].Method != "POST" || r.Posts[0].Path != testkit.DexAsk {
		t.Fatalf("the posts: %+v", r.Posts)
	}
	// The browser sets Sec-Fetch-Site by itself. The page sets no header.
	if len(r.Posts[0].Headers) != 0 {
		t.Errorf("the page set the headers %v", r.Posts[0].Headers)
	}

	// The table: the cell of the row changes in place, and the click does not open the row.
	r = click(t, devdata.StateOK, "#newer", "0", strings.TrimSuffix(dexClick, "}")+`,"go":"#profiles"}`)
	if r.Hash != "#newer" {
		t.Errorf("the click on the button opened %s", r.Hash)
	}
	mustHold(t, "the table before", rowTo(t, r.Before, dexHash), askButtonEnd)
	mustHold(t, "the table during", rowTo(t, r.During, dexHash), rowButton+dexMoveAttrs+askingEnd)
	row := rowTo(t, r.After, dexHash)
	mustHold(t, "the table after", row, noErrorCell)
	mustNotHold(t, "the table after", row, askButton, askAgain, asking)
	if len(r.Posts) != 1 {
		t.Errorf("the table sent %d posts", len(r.Posts))
	}
	// The other rows keep their button.
	if got := strings.Count(r.After, askButtonEnd); got != 9 {
		t.Errorf("%d rows still offer the question, want 9", got)
	}

	// A refusal shows the code, the message, and the clusters, and the words of Palette stay as text.
	r = click(t, devdata.StateOK, ubuntuHash, "0", ubuntuClick)
	words := textOf(r.After)
	mustHold(t, "the refusal", words, refusesWithCode, "The upgrade of the pack ubuntu-aws from 20.04 to 24.04 is not supported", "on quarry-1", nothingChanged)
	mustNotHold(t, "the refusal", words, askAgain)

	// A request that failed is an answer of the kind "no answer". The next click asks again.
	r = click(t, devdata.StateOK, csiHash, "0", csiClick)
	mustHold(t, "no answer", textOf(r.After), gave403)
	mustHold(t, "no answer", r.After, askAgainEnd)
	again := click(t, devdata.StateOK, csiHash, "0", csiClick, csiClick)
	if len(again.Posts) != 2 {
		t.Errorf("each click on a no-answer must ask again: %d posts for 2 clicks", len(again.Posts))
	}
}

// A question that the page could not send, or that the observer could not answer, leaves a short note
// and no answer. The button comes back, and the next click asks again.
func TestAQuestionThatFailsLeavesANoteAndNoAnswer(t *testing.T) {
	for _, status := range []string{"502", "503", "404", "network"} {
		t.Run(status, func(t *testing.T) {
			r := click(t, devdata.StateOK, dexHash, status, dexClick)
			mustHold(t, "during", r.During, asking)
			mustHold(t, "after", r.After, askFailed, cardButton+dexMoveAttrs+askThisMoveEnd)
			mustNotHold(t, "after", r.After, paletteGave, paletteRefuses, wordNoAnswer, asking)
			if len(r.Posts) != 1 {
				t.Errorf("%d posts", len(r.Posts))
			}
		})
	}
	// The table shows the note in the cell of the row, and the other rows say nothing.
	r := click(t, devdata.StateOK, "#newer", "503", dexClick)
	mustHold(t, "the table", rowTo(t, r.After, dexHash), askFailed, askButtonEnd)
	if got := strings.Count(r.After, askFailed); got != 1 {
		t.Errorf("the table says the note %d times", got)
	}
	// A move with no reply in the dev data is not found, and the page says the same.
	r = click(t, devdata.StateOK, "#k-"+testkit.LonghornProfile+"/"+testkit.LonghornKey, "0", askClick(testkit.LonghornProfile, testkit.LonghornKey))
	mustHold(t, "no reply", r.After, askFailed)
	// A click on a move that the data does not hold sends nothing.
	r = click(t, devdata.StateOK, "#newer", "0", askClick(testkit.NobodyProfile, testkit.NoneKey))
	if len(r.Posts) != 0 {
		t.Errorf("a click on an unknown move sent %v", r.Posts)
	}
}

// The held answer of a request that failed gives way to the new answer, in the table and on the card.
func TestAnotherClickOnANoAnswerReplacesIt(t *testing.T) {
	r := click(t, devdata.StateAsks, dexHash, "0", dexClick)
	mustHold(t, "before", textOf(r.Before), gave403)
	mustHold(t, "after", textOf(r.After), noErrorCovers)
	mustNotHold(t, "after", textOf(r.After), gaveNoAnswer, "HTTP 403")
	mustNotHold(t, "after", r.After, askAgain)
}

// The observer holds one answer for a pack of a profile, and it drops the answer when the move changes. The
// registry can gain a version after the last versions cycle. The answer is then for a newer version than the
// row, and the card and the table show it.
func TestAnAnswerForANewerVersionThanTheRowShowsBesideTheMove(t *testing.T) {
	dir := writeSet(t, devdata.StateAsks, map[string]func([]byte) []byte{"asks.json": replaceText(`"newest":"1.36.2"`, `"newest":"1.37.0"`)})
	pages := pagesOf(t, dir, k8sHash, "#newer")
	mustHold(t, "the card", textOf(pages[0]), noErrorCovers)
	mustNotHold(t, "the card", pages[0], askThisMoveEnd)
	mustHold(t, "the table", rowTo(t, pages[1], k8sHash), noErrorCell)
}

// A click always ends with a word. The observer names the newest version of the row also for a pin that failed
// or is not behind any more. An older observer sent no version, and the page still says no answer.
func TestAClickThatGetsAnAnswerWithoutAVersionEndsWithTheWordNoAnswer(t *testing.T) {
	for _, newest := range []string{"2.44.0", ""} {
		t.Run("newest "+newest, func(t *testing.T) {
			reply := `{"asks":[{"profile":"` + testkit.DexProfile + `","key":"` + testkit.DexKey + `","newest":"` + newest + `",` + testkit.NoAnswerVerdict + `,"refusals":[],"status":502}]}`
			dir := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"replies.json": wholeFile(reply)})
			r := clickIn(t, dir, scratch, dexHash, "0", dexClick)
			mustHold(t, "after", textOf(r.After), gaveNoAnswer+": HTTP 502.")
			mustHold(t, "after", r.After, cardButton+dexMoveAttrs+askAgainEnd)
			mustNotHold(t, "after", r.After, askThisMove, couldNotAsk)
			table := clickIn(t, dir, scratch, "#newer", "0", dexClick)
			mustHold(t, "the table", rowTo(t, table.After, dexHash), noAnswerSpan, askAgain)
		})
	}
}

// The route /v1/asks can fail. The page stays online, and it keeps the last list of answers. With no list
// yet, the page offers the question.
func TestAFailedListOfAnswersKeepsThePageOnlineAndTheLastList(t *testing.T) {
	read := func(state devdata.State, extra ...string) string {
		return nodeJSON[string](t, runNode(t, state, append([]string{"asksdown", "#newer"}, extra...)...))
	}
	kept := read(devdata.StateAsks)
	mustHold(t, "the kept list", kept, noErrorSpan, refusesSpan, noAnswerSpan)
	mustNotHold(t, "the kept list", kept, cannotReach)
	first := read(devdata.StateAsks, "first")
	mustHold(t, "no list", first, askButtonEnd, newerVersions)
	mustNotHold(t, "no list", first, cannotReach, `<span class="fw">`, wordNoAnswer)
}

// Nothing asks Palette by itself. A crawl of every screen of every set sends GET requests only, and the
// question is sent in one place of the script: the function that a click calls.
func TestNothingAsksWithoutAClick(t *testing.T) {
	const usedTwice = "%s is called in %d places and defined once, want 2 uses"
	for _, state := range []devdata.State{devdata.StateOK, devdata.StateAsks, devdata.StateTextTooLarge, devdata.StateReadError} {
		t.Run(string(state), func(t *testing.T) {
			var asks, diffs int
			for _, r := range requestsOf(t, state) {
				if r.Method != "GET" {
					t.Errorf("the page sent %s %s without a click", r.Method, r.Path)
				}
				asks += strings.Count(r.Path, testkit.AsksRoute)
				diffs += strings.Count(r.Path, "/diff?side=")
			}
			if asks == 0 || diffs == 0 {
				t.Errorf("the crawl read the list of answers %d times and %d diffs", asks, diffs)
			}
		})
	}
	script := readScripts(t)
	if got := strings.Count(script, `"POST"`); got != 2 {
		t.Errorf("the scripts hold %d POST requests, want 2: the clicks "+askButton+" and Check now", got)
	}
	if got := strings.Count(script, "checkNow("); got != 2 {
		t.Errorf(usedTwice, "checkNow", got)
	}
	if got := strings.Count(script, "askPalette("); got != 2 {
		t.Errorf(usedTwice, "askPalette", got)
	}
	mustHold(t, "the click handler of the scripts", script, "if (nw) { checkNow(nw.dataset.now); return; }",
		"if (q) { askPalette(q.dataset.ask, q.dataset.pack); return; }")
	if got := strings.Count(script, "setInterval("); got != 1 || !strings.Contains(script, "setInterval(refresh, REFRESH_MS)") {
		t.Errorf("the scripts hold %d timers, and the only timer must read the routes", got)
	}
}

// The word "accepts" is on no screen of any set, in any state of the page, and not in the files that make
// the page. Palette gives no error for default values, and that is no promise about the values of a person.
func TestTheWordAcceptsIsOnNoScreen(t *testing.T) {
	accepts := regexp.MustCompile(`(?i)accepts`)
	for _, state := range devdata.States {
		t.Run(string(state), func(t *testing.T) {
			for _, poison := range []bool{false, true} {
				for _, s := range render(t, state, poison) {
					if accepts.MatchString(s.HTML) {
						t.Errorf("%s %s says the word", s.Hash, s.Variant)
					}
				}
			}
		})
	}
	for _, name := range append(screenScripts(t), "index.html", "app.css") {
		if accepts.Match(readPage(t, name)) {
			t.Errorf("%s holds the word", name)
		}
	}
	// The screens after a click hold no such word either, for each verdict.
	for _, c := range []struct {
		state       devdata.State
		hash, click string
	}{
		{devdata.StateOK, dexHash, dexClick},
		{devdata.StateOK, ubuntuHash, ubuntuClick},
		{devdata.StateOK, csiHash, csiClick},
		{devdata.StateOK, "#newer", dexClick},
	} {
		r := click(t, c.state, c.hash, "0", c.click)
		if accepts.MatchString(r.Before + r.During + r.After) {
			t.Errorf("%s after a click says the word", c.hash)
		}
	}
}

// A refusal, a code, and a cluster with the text of a tag show as text, on the card and in the table.
func TestAnAnswerWithMarkupShowsAsText(t *testing.T) {
	screens := render(t, devdata.StateAsks, true)
	var reached int
	for _, s := range screens {
		checkMarkup(t, s)
		if strings.Contains(s.HTML, paletteRefuses+":") {
			reached++
			mustHold(t, s.Hash+" "+s.Variant, s.HTML, `<span class="m">`+testkit.CodeDependency+injection+`&quot;&#39;&amp;</span>`, "on quarry-1"+injection)
			mustNotHold(t, s.Hash+" "+s.Variant, s.HTML, "<img")
		}
	}
	if reached == 0 {
		t.Error("no screen of the poisoned set shows a refusal")
	}
	// A poisoned diff shows its lines as text, in both layouts.
	var lines int
	for _, s := range render(t, devdata.StateOK, true) {
		for _, row := range diffRowsOf(s.HTML) {
			if strings.Contains(row, evil) {
				lines++
			}
		}
		if s.Variant == layoutSplit || strings.HasPrefix(s.Variant, layoutSplit+" tab") {
			mustNotHold(t, s.Hash+" "+s.Variant, s.HTML, "<img")
		}
	}
	if lines == 0 {
		t.Error("no row of a poisoned diff holds the text of the tag")
	}
}

// The rules of the style for the viewer keep the phone and the two themes: a long line of a diff wraps
// in its own column, the text keeps its spaces, and each color of the diff is a token with a light and a
// dark value.
func TestTheStyleOfTheViewerKeepsThePhoneAndTheThemes(t *testing.T) {
	css := string(readPage(t, "app.css"))
	// The backgrounds of an added and of a removed line.
	const added, removed = "background:var(--add);", "background:var(--del);"
	rulesHold(t, css, map[string][]string{
		".diff table":     {"table-layout:fixed;", "width:100%;"},
		".diff col.no":    {"width:48px;"},
		".diff td":        {"white-space:pre-wrap;", wrapAnywhere, "border:0;"},
		".diff td.no":     {"user-select:none;", "background:var(--gut);"},
		".diff tr.a td.c": {added, "color:var(--addw);"},
		".diff tr.d td.c": {removed, "color:var(--delw);"},
		".diff td.ca":     {added},
		".diff td.cd":     {removed},
		".diffhead":       {"display:flex;", "flex-wrap:wrap;"},
		".btn:disabled":   {"cursor:default;"},
		// The control of the layout keeps its place on a phone: the rule of .seg in the small width scrolls.
		".seg": {"overflow-x:auto;"},
	})
	// Each token that a rule uses is a token that the style defines.
	defined := map[string]bool{}
	for _, m := range regexp.MustCompile(`(--[a-z0-9-]+)\s*:`).FindAllStringSubmatch(cssComment.ReplaceAllString(css, ""), -1) {
		defined[m[1]] = true
	}
	for _, m := range regexp.MustCompile(`var\((--[a-z0-9-]+)\)`).FindAllStringSubmatch(css, -1) {
		if !defined[m[1]] {
			t.Errorf("the style uses the token %s, and it defines none", m[1])
		}
	}
}
