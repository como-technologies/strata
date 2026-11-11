package web

import (
	"encoding/json"
	"fmt"
	htmlpkg "html"
	"regexp"
	"strconv"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

/* The tests below hold the viewer: the text of the diffs in two layouts, the note for a text above the cap,
and the question to Palette. They read the markup that the scripts write, and they check it against a second
reading of the same data in Go. */

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

// diffLine is one line of a hunk, as the second reading of a unified text gives it. The sign "@" stands
// for the head of a hunk that follows another hunk.
type diffLine struct {
	sign, na, nb, text string
	noEOL              bool
}

var hunkHead = regexp.MustCompile(`^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@`)

// readUnified reads a unified text by the rules of the issue: the two header lines come before the first
// hunk, a line "--- x" in a hunk is a removed line, and the line numbers come from the heads of the hunks.
func readUnified(text string) []diffLine {
	var out []diffLine
	var a, b, hunks int
	for _, l := range strings.Split(strings.TrimSuffix(text, "\n"), "\n") {
		if m := hunkHead.FindStringSubmatch(l); m != nil {
			if hunks > 0 {
				out = append(out, diffLine{sign: "@", text: l})
			}
			hunks++
			a, _ = strconv.Atoi(m[1])
			b, _ = strconv.Atoi(m[2])
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

const noEOLNote = `\ No newline at end of file`

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
			out = append(out, rowString("n", [2]string{"no", ""}, [2]string{"no", ""}, [2]string{"c", noEOLNote}))
		}
	}
	return out
}

// noteRow is the row of a split layout that says a line has no end of line, on the side of the line.
func noteRow(left, right bool) string {
	word := func(on bool) string {
		if on {
			return noEOLNote
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
	diffTable = regexp.MustCompile(`(?s)<div class="diff"><table class="(?:unified|split)"><colgroup>.*?</colgroup><tbody>(.*?)</tbody></table></div>`)
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
	if layout == "split" {
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
	dexHash       = "#k-fred-identity@2.3.0/addon/dex"
	dexPublished  = "side-fred-identity@2.3.0-addon/dex-published"
	dexMove       = "move-fred-identity@2.3.0-addon/dex"
	k8sHash       = "#k-barney-base@1.5.0/k8s/kubernetes"
	k8sMove       = "move-barney-base@1.5.0-k8s/kubernetes"
	traefikHash   = "#k-wilma-gateway@3.1.2/addon/traefik"
	traefikMove   = "move-wilma-gateway@3.1.2-addon/traefik"
	dexSubject    = "fred-identity@2.3.0"
	k8sSubject    = "barney-base@1.5.0"
	traefikSubj   = "wilma-gateway@3.1.2"
	layoutUnified = "unified"
	layoutSplit   = "split"
)

// variantOf gives the variant of the crawl that sets a side of the dropdown, a layout, and a tab or none. The side is ""
// for the side that the pack opens on.
func variantOf(side, layout, tab, k string) string {
	prefix := ""
	if side != "" {
		prefix = "side " + side + " "
	}
	switch {
	case tab == "" && layout == layoutSplit:
		return prefix + "split"
	case tab == "":
		return strings.TrimSuffix(prefix, " ")
	case layout == layoutSplit:
		return prefix + "split tab " + tab + " " + k
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
		{dexHash, dexPublished, dexSubject, "addon/dex", model.SidePublished, ""},
		{dexHash, dexMove, dexSubject, "addon/dex", model.SideNewer, "newer"},
		{k8sHash, k8sMove, k8sSubject, "k8s/kubernetes", model.SideNewer, ""},
		{traefikHash, traefikMove, traefikSubj, "addon/traefik", model.SideNewer, ""},
	} {
		text := diffOf(t, set, c.profile, c.key, c.side).Values.Unified
		if text == "" {
			t.Fatalf("the set holds no text for %s %s", c.profile, c.side)
		}
		for _, layout := range []string{layoutUnified, layoutSplit} {
			where := c.tab + " " + layout
			card := cardOf(t, pageOf(t, screens, c.hash, variantOf(c.open, layout, "", "")), `data-tab="`+c.tab+`"`)
			sameRows(t, where, diffRowsOf(card), layoutRows(layout, text))
			added, removed := headCounts(text)
			mustHold(t, where, card, fmt.Sprintf(`<div class="diffhead"><span>%s values <span class="now">+%d</span> <span class="was">-%d</span></span>`, c.key, added, removed),
				`data-k="values">Values`)
			if strings.Index(card, "<th>Setting</th>") > strings.Index(card, `class="diffhead"`) {
				t.Errorf("%s: the table of the settings comes after the text", where)
			}
		}
	}
	// The order on the Values tab: the table of the settings, the masked note, the text, and then the folded parts.
	dex := cardOf(t, pageOf(t, screens, dexHash, ""), `data-tab="`+dexPublished+`"`)
	if !(strings.Index(dex, "<th>Setting</th>") < strings.Index(dex, "Masked:") && strings.Index(dex, "Masked:") < strings.Index(dex, `class="diffhead"`)) {
		t.Error("the Values tab of dex does not show the table, then the masked note, then the text")
	}
	k8s := cardOf(t, pageOf(t, screens, k8sHash, ""), `data-tab="`+k8sMove+`"`)
	if !(strings.Index(k8s, `class="diffhead"`) < strings.Index(k8s, "<details")) {
		t.Error("the Values tab of kubernetes does not show the text before the folded parts")
	}
	// The text of kubernetes holds three hunks, so the text shows two gaps.
	if got := len(regexp.MustCompile(`<tr class="h">`).FindAllString(k8s, -1)); got != 2 {
		t.Errorf("the text of kubernetes shows %d gap rows, want 2", got)
	}
}

// The Manifests tab shows the text of each manifest with the operation in its head. A parameter has a row
// in the small table, and a manifest with text has none.
func TestTheManifestsTabShowsTheTextOfEachManifestInBothLayouts(t *testing.T) {
	set := loadSet(t, devdata.StateOK)
	screens := render(t, devdata.StateOK, false)
	for _, c := range []struct {
		tab, profile string
		side         model.Side
		table        []string
		open         string
	}{
		{dexPublished, dexSubject, model.SidePublished, nil, ""},
		{dexMove, dexSubject, model.SideNewer, []string{`<td class="m">added</td><td>Parameter</td><td class="m">dex_hostname</td>`}, "newer"},
	} {
		d := diffOf(t, set, c.profile, "addon/dex", c.side)
		var want, wantSplit []string
		var heads []string
		for _, m := range d.Manifests {
			if m.Unified == "" {
				t.Fatalf("the manifest %s has no text", m.Name)
			}
			want = append(want, unifiedRowsOf(m.Unified)...)
			wantSplit = append(wantSplit, splitRowsOf(m.Unified)...)
			added, removed := headCounts(m.Unified)
			heads = append(heads, fmt.Sprintf(`<div class="diffhead"><span>manifest %s <span class="mut">%s</span> <span class="now">+%d</span> <span class="was">-%d</span></span>`, m.Name, m.Op, added, removed))
		}
		for _, layout := range []string{layoutUnified, layoutSplit} {
			where := c.tab + " " + layout
			page := pageOf(t, screens, dexHash, variantOf(c.open, layout, c.tab, "manifests"))
			card := cardOf(t, page, `data-tab="`+c.tab+`" data-k="manifests"`)
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
				mustNotHold(t, where, card, `<td>Manifest</td><td class="m">`+m.Name+`</td>`)
			}
		}
	}
	// The ops of the manifests show in the heads: changed, added, and removed.
	d := diffOf(t, set, dexSubject, "addon/dex", model.SideNewer)
	if len(d.Manifests) != 2 || d.Manifests[0].Op != "added" || d.Manifests[1].Op != "removed" {
		t.Fatalf("the manifests of the newer side: %+v", d.Manifests)
	}
	// An added manifest is all added lines, and a removed manifest is all removed lines.
	added, removed := headCounts(d.Manifests[0].Unified)
	if added == 0 || removed != 0 {
		t.Errorf("an added manifest has %d added and %d removed lines", added, removed)
	}
	added, removed = headCounts(d.Manifests[1].Unified)
	if added != 0 || removed == 0 {
		t.Errorf("a removed manifest has %d added and %d removed lines", added, removed)
	}
}

// The line "\ No newline at end of file" is a muted note row with no number, and it counts as no line. A
// head of a hunk shows the gap, a line "--- x" in a hunk is a removed line, and a head with a missing
// count or with an empty side gives the right numbers.
func TestTheReadingOfTheTextFollowsTheRulesOfTheIssue(t *testing.T) {
	const text = "--- a: addon/dex values\n+++ b: addon/dex values\n" +
		"@@ -0,0 +1,2 @@\n+one\n+two\n\\ No newline at end of file\n" +
		"@@ -7 +9,2 @@\n--- not a header\n+++ not a header either\n+added\n" +
		"@@ -20,3 +22,3 @@\n keep\n-old\n\\ No newline at end of file\n+new\n\\ No newline at end of file\n keep\n"
	dir := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"diffs.json": editDiffs(t, func(ds []api.PackDiff) {
		entryOf(t, ds, dexSubject, "addon/dex", model.SidePublished).Values.Unified = text
	})})
	for _, layout := range []string{layoutUnified, layoutSplit} {
		var out struct{ After string }
		clicks := []string{}
		if layout == layoutSplit {
			clicks = append(clicks, `{"view":"split"}`)
		}
		raw := runNodeIn(t, dir, "scratch", append([]string{"click", dexHash, "0"}, clicks...)...)
		if err := json.Unmarshal(raw, &out); err != nil {
			t.Fatal(err)
		}
		card := cardOf(t, out.After, `data-tab="`+dexPublished+`"`)
		sameRows(t, layout, diffRowsOf(card), layoutRows(layout, text))
		mustHold(t, layout, card, `<span class="now">+5</span> <span class="was">-2</span>`)
	}
	// The reference reading follows the rules too, so the two readings do not agree by accident.
	lines := readUnified(text)
	if len(lines) != 11 || lines[2].sign != "@" || lines[3].sign != "-" || lines[3].text != "-- not a header" || lines[3].na != "7" ||
		lines[4].sign != "+" || lines[4].text != "++ not a header either" || lines[4].nb != "9" || lines[5].nb != "10" || lines[0].nb != "1" || lines[1].nb != "2" {
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
	t.Fatalf("no diff for %s %s %s", profile, key, side)
	return nil
}

// editDiffs changes the diffs of a set.
func editDiffs(t *testing.T, change func([]api.PackDiff)) func([]byte) []byte {
	return func(raw []byte) []byte {
		var ds []api.PackDiff
		if err := devdata.Decode(raw, &ds); err != nil {
			t.Fatal(err)
		}
		change(ds)
		out, err := json.Marshal(ds)
		if err != nil {
			t.Fatal(err)
		}
		return out
	}
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
	var out clicked
	if err := json.Unmarshal(runNodeIn(t, dir, state, append([]string{"click", hash, status}, clicks...)...), &out); err != nil {
		t.Fatal(err)
	}
	return out
}

// Both layouts have the same control in the head of each diff, on the Values tab and on the Manifests
// tab. One state serves the whole page: a click on Split changes each diff, and the choice stays when the
// person changes the tab.
func TestTheControlOfTheLayoutSitsInEachDiffHeadAndSetsOneLayoutForThePage(t *testing.T) {
	const seg = `<span class="seg" role="group" aria-label="Diff layout">`
	screens := render(t, devdata.StateOK, false)
	for _, c := range []struct{ variant, where string }{{"", "values"}, {"tab " + dexPublished + " manifests", "manifests"}, {"side newer tab " + dexMove + " manifests", "manifests of the move"}} {
		page := pageOf(t, screens, dexHash, c.variant)
		heads := strings.Count(page, `<div class="diffhead">`)
		if heads == 0 || strings.Count(page, seg) != heads {
			t.Errorf("%s: %d heads and %d controls", c.where, heads, strings.Count(page, seg))
		}
		mustHold(t, c.where, page, `<button data-view="unified" aria-pressed="true" class="on">Unified</button><button data-view="split" aria-pressed="false" class="">Split</button>`)
		mustNotHold(t, c.where, page, `aria-pressed="true" class="on">Split`)
	}
	split := pageOf(t, screens, dexHash, "split")
	mustHold(t, "split", split, `<button data-view="unified" aria-pressed="false" class="">Unified</button><button data-view="split" aria-pressed="true" class="on">Split</button>`)
	mustNotHold(t, "split", split, `<td class="c">`)

	// A click on Split changes each diff of the page and nothing else. A click on a tab keeps the layout.
	r := click(t, devdata.StateOK, dexHash, "0", `{"view":"split"}`)
	mustHold(t, "before the click", r.Before, `<td class="c">`)
	mustNotHold(t, "before the click", r.Before, `class="cd"`)
	mustHold(t, "after the click", r.After, `class="cd"`, `class="ca"`)
	mustNotHold(t, "after the click", r.After, `<td class="c">`)
	if got, want := strings.Count(r.After, `data-view="split" aria-pressed="true"`), strings.Count(r.After, `<div class="diffhead">`); got != want || got < 1 {
		t.Errorf("%d controls show Split for %d diffs", got, want)
	}
	if len(r.Posts) != 0 {
		t.Errorf("a click on the layout sent %v", r.Posts)
	}
	r = click(t, devdata.StateOK, dexHash, "0", `{"view":"split"}`, `{"tab":"`+dexPublished+`","k":"manifests"}`)
	mustHold(t, "manifests after Split", r.After, `class="cd"`, "manifest fred-realm-config")
	mustNotHold(t, "manifests after Split", r.After, `<td class="c">`)
	r = click(t, devdata.StateOK, dexHash, "0", `{"view":"split"}`, `{"view":"unified"}`)
	mustHold(t, "back to Unified", r.After, `<td class="c">`)
	mustNotHold(t, "back to Unified", r.After, `class="cd"`)
}

// A line of a diff with the text of a tag shows as text, in both layouts, in a value and in a manifest.
func TestALineOfADiffWithMarkupShowsAsText(t *testing.T) {
	const evil = "<img src=x onerror=alert(1)>"
	dir := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"diffs.json": editDiffs(t, func(ds []api.PackDiff) {
		d := entryOf(t, ds, dexSubject, "addon/dex", model.SidePublished)
		d.Values.Unified = strings.Replace(d.Values.Unified, "+        idTokens: 168h\n", "+        idTokens: 168h\n+"+evil+"\n-"+evil+"\n context "+evil+"\n", 1)
		d.Manifests[0].Unified = strings.Replace(d.Manifests[0].Unified, "@@ -5,5 +5,5 @@\n", "@@ -5,5 +5,5 @@ "+evil+"\n+"+evil+"\n", 1) + "@@ -90 +90 @@ " + evil + "\n-" + evil + "\n+" + evil + "\n"
	})})
	for _, clicks := range [][]string{nil, {`{"view":"split"}`}, {`{"tab":"` + dexPublished + `","k":"manifests"}`}, {`{"view":"split"}`, `{"tab":"` + dexPublished + `","k":"manifests"}`}} {
		r := clickIn(t, dir, "scratch", dexHash, "0", clicks...)
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
	const title, body = "Too much rock to carry", "This text is too large to keep here. The observer keeps at most 256 KiB for each side. A terminal shows all of it."
	const tone = `class="splash sm tone-off"`
	uid := registryUidOf(t, set, "kubernetes")
	published := cardOf(t, pageOf(t, screens, dexHash, ""), `data-tab="`+dexPublished+`"`)
	mustHold(t, "the values of dex", published, title, body, tone, "<code>strata compare record.json fred-identity@2.3.0 --full</code>")
	mustNotHold(t, "the values of dex", published, `class="diffhead"`, `class="diff"`, `data-view=`)
	// The manifest keeps its row in the small table, because it has no text, and the splash comes with the command.
	manifests := cardOf(t, pageOf(t, screens, dexHash, "tab "+dexPublished+" manifests"), `data-tab="`+dexPublished+`" data-k="manifests"`)
	mustHold(t, "the manifests of dex", manifests, title, body, "<code>strata compare record.json fred-identity@2.3.0 --full</code>",
		`<tr><td class="m">changed</td><td>Manifest</td><td class="m">fred-realm-config</td></tr>`)
	mustNotHold(t, "the manifests of dex", manifests, `class="diffhead"`, `class="diff"`)
	// The newer side of the same pack keeps its text.
	move := cardOf(t, pageOf(t, screens, dexHash, "side newer"), `data-tab="`+dexMove+`"`)
	mustHold(t, "the move of dex", move, `class="diffhead"`)
	mustNotHold(t, "the move of dex", move, title)
	// The newer side of kubernetes has the command with the two versions of the pack.
	k8s := cardOf(t, pageOf(t, screens, k8sHash, ""), `data-tab="`+k8sMove+`"`)
	mustHold(t, "the values of kubernetes", k8s, title, body, "<code>strata compare pack:"+uid+"/kubernetes@1.35.3 pack:"+uid+"/kubernetes@1.36.2 --full</code>")
	mustNotHold(t, "the values of kubernetes", k8s, `class="diffhead"`)
	// The pack has no manifest, so the Manifests tab says so, links to the tab with the change, and shows no splash of a large text.
	none := cardOf(t, pageOf(t, screens, k8sHash, "tab "+k8sMove+" manifests"), `data-tab="`+k8sMove+`" data-k="manifests"`)
	mustHold(t, "the manifests of kubernetes", none, "No manifest changed here.", `data-tab="`+k8sMove+`" data-k="values">The change is on Values →</button>`)
	mustNotHold(t, "the manifests of kubernetes", none, title)
	// The set ok has the text for the same packs.
	for _, s := range render(t, devdata.StateOK, false) {
		mustNotHold(t, s.Hash+" "+s.Variant, s.HTML, title)
	}
}

// The cluster side compares the record with the copy of the cluster, and the profile page already names
// this command.
func TestTheCommandOfTheClusterSideNamesTheCluster(t *testing.T) {
	set := loadSet(t, devdata.StateReadError)
	dir := writeSet(t, devdata.StateReadError, map[string]func([]byte) []byte{"diffs.json": editDiffs(t, func(ds []api.PackDiff) {
		d := entryOf(t, ds, dexSubject, "addon/dex", model.SideCluster)
		d.Values.Unified, d.TextTooLarge = "", true
	})})
	command := "strata compare record.json cluster:" + set.Cluster.Cluster.Name + "/fred-identity"
	r := clickIn(t, dir, "scratch", dexHash, "0")
	card := cardOf(t, r.After, `data-tab="side-fred-identity@2.3.0-addon/dex-cluster"`)
	mustHold(t, "the cluster side", card, "This text is too large to keep here.", "<code>"+command+" --full</code>")
	mustNotHold(t, "the cluster side", card, `class="diffhead"`)
	// The profile page shows the same command, with the pack name left out and no flag.
	profile := pagesOf(t, dir, "#p-"+dexSubject)[0]
	mustHold(t, "the profile", profile, "<code>"+command+"</code>")
}

// A command with a name that holds markup shows as text. The name has spaces and a "<", so the command puts it in single quotes,
// and the page escapes each quote.
func TestTheCommandOfATextAboveTheCapShowsNamesAsText(t *testing.T) {
	const evil = "<img src=x onerror=alert(1)>"
	swap := func(raw []byte) []byte { return []byte(strings.ReplaceAll(string(raw), "quarry-1", evil)) }
	dir := writeSet(t, devdata.StateReadError, map[string]func([]byte) []byte{"cluster.json": swap, "diffs.json": editDiffs(t, func(ds []api.PackDiff) {
		d := entryOf(t, ds, dexSubject, "addon/dex", model.SideCluster)
		d.Values.Unified, d.TextTooLarge = "", true
	})})
	page := clickIn(t, dir, "scratch", dexHash, "0").After
	mustHold(t, "the command", page, `&#39;cluster:`+injection+`/fred-identity&#39; --full`)
	mustNotHold(t, "the command", page, "<img")
}

// The Newer table has the column Palette with the button "Ask Palette" in each row, and the card "Move to"
// of a pack has the button "Ask Palette about this move". A set that holds no answer shows no answer.
func TestTheNewerTableAndTheCardOfAMoveOfferTheQuestion(t *testing.T) {
	set := loadSet(t, devdata.StateOK)
	screens := render(t, devdata.StateOK, false)
	table := pageOf(t, screens, "#newer", "")
	mustHold(t, "the table", table, `data-sort="nt" data-col="listed">Listed<svg class="sorti"`, `</svg></button></th><th class="narrow-hide" title="Asks Palette whether it would take this move on the clusters that run this profile. It changes nothing.">Palette</th></tr></thead>`)
	if got := strings.Count(table, ">Ask Palette</button>"); got != len(set.Newer.Newer) {
		t.Errorf("the table has %d buttons for %d rows", got, len(set.Newer.Newer))
	}
	mustNotHold(t, "the table", table, "No error", "Refuses", "No answer", "Palette gave")
	// The group rows span the new column too, and each row keeps its cell in the class narrow-hide.
	mustHold(t, "the table", table, `<tr class="g"><td colspan="8">`)
	mustNotHold(t, "the table", table, `colspan="7"`)
	for _, row := range strings.Split(table, "</tr>") {
		if strings.Contains(row, `data-go="#k-`) && !strings.Contains(row, `<td class="narrow-hide"><button class="lnk" data-ask="`) {
			t.Errorf("a row has no button in a cell of the class narrow-hide: %s", row)
		}
	}
	// The button is in the row, which opens the pack: its data attributes name the move.
	row := rowWith(t, table, `data-go="`+dexHash+`"`)
	mustHold(t, "the row of dex", row, `<button class="lnk" data-ask="fred-identity@2.3.0" data-pack="addon/dex">Ask Palette</button>`)
	for _, hash := range []string{dexHash, k8sHash, traefikHash} {
		page := pageOf(t, screens, hash, "")
		mustHold(t, hash, page, `<button class="btn" data-ask="`, ">Ask Palette about this move</button>")
		mustNotHold(t, hash, page, "Palette gave", "Palette refuses", "Asking...")
	}
	// A pack with no move has no button.
	mustNotHold(t, "a pack with no move", pageOf(t, screens, "#k-fred-identity@2.3.0/addon/reloader", ""), "Ask Palette")
}

// The set asks holds one answer of each verdict. The Newer table and the card say each answer in its
// words, and the word "accepts" is not there. A request that failed is no finding.
func TestTheAnswersOfPaletteReadExactlyAsTheIssueSays(t *testing.T) {
	set := loadSet(t, devdata.StateAsks)
	screens := render(t, devdata.StateAsks, false)
	table := pageOf(t, screens, "#newer", "")
	refusal := set.Asks.Asks[1].Refusals[0]
	if refusal.Code != "DependencyPackUpgradeNotSupported" || set.Asks.Asks[2].Status != 403 {
		t.Fatalf("the data of the set changed: %+v", set.Asks.Asks)
	}
	// The table: the word of each answer in the column Palette, and the button where it is not an answer.
	for _, c := range []struct{ hash, cell string }{
		{k8sHash, `<td class="narrow-hide"><span class="fw">No error</span></td>`},
		{traefikHash, `<td class="narrow-hide"><span class="fw">Refuses</span></td>`},
		{dexHash, `<td class="narrow-hide"><span class="mut">No answer</span> <button class="lnk" data-ask="fred-identity@2.3.0" data-pack="addon/dex">Ask again</button></td>`},
	} {
		mustHold(t, c.hash, rowWith(t, table, `data-go="`+c.hash+`"`), c.cell)
	}
	if got, want := strings.Count(table, ">Ask Palette</button>"), len(set.Newer.Newer)-3; got != want {
		t.Errorf("%d rows offer the question, want %d", got, want)
	}
	// The cards: the words of the issue, exactly.
	noError := textOf(cardOf(t, pageOf(t, screens, k8sHash, ""), "Palette gave no error"))
	mustHold(t, "no error", noError, `Palette gave no error. Palette checked the profile with the new version and its default values against each cluster that runs the profile. It did not check your own values of the pack.`)
	mustNotHold(t, "no error", noError, "It read default values", "Ask Palette about this move")
	refuses := textOf(cardOf(t, pageOf(t, screens, traefikHash, ""), "Palette refuses"))
	mustHold(t, "refuses", refuses, "Palette refuses: "+refusal.Code, refusal.Message, "on "+refusal.Clusters[0], "Nothing on this cluster changed.")
	mustNotHold(t, "refuses", refuses, "Ask again", "Ask Palette about this move")
	if strings.Index(refuses, refusal.Message) > strings.Index(refuses, "on "+refusal.Clusters[0]) {
		t.Error("the clusters come before the message")
	}
	dex := pageOf(t, screens, dexHash, "")
	noAnswer := textOf(cardOf(t, dex, "Palette gave no answer"))
	mustHold(t, "no answer", noAnswer, "Palette gave no answer: HTTP 403.", "Ask again")
	mustHold(t, "no answer", dex, `<button class="btn" data-ask="fred-identity@2.3.0" data-pack="addon/dex">Ask again</button>`)
	// A request that failed is neutral: no red, no badge, no note of a finding.
	block := dex[strings.Index(dex, "Palette gave no answer")-40:]
	block = block[:strings.Index(block, "</div></div>")]
	mustHold(t, "no answer", block, `<div class="note small"><b>Palette gave no answer`)
	mustNotHold(t, "no answer", block, "badge", `note r`, `note a`, "Drift")
}

// A no-answer raises no severity. The summary tier does not see an answer: the screens that do not show a
// move are the same as in ok, and the rows of the move hold no badge for it.
func TestAnAnswerRaisesNoSeverityAndANoAnswerIsNoFinding(t *testing.T) {
	ok, asks := render(t, devdata.StateOK, false), render(t, devdata.StateAsks, false)
	for _, hash := range []string{"#overview", "#findings", "#profiles", "#notchecked", "#images"} {
		a, b := pageOf(t, ok, hash, ""), pageOf(t, asks, hash, "")
		// The nav, the tiles, and every count are the same.
		navA, _, _ := strings.Cut(a, "\n")
		navB, _, _ := strings.Cut(b, "\n")
		if navA != navB {
			t.Errorf("%s: the nav differs\n%s\n%s", hash, navA, navB)
		}
		tiles := regexp.MustCompile(`(?s)<section class="tiles".*?</section>`)
		if tiles.FindString(a) != tiles.FindString(b) {
			t.Errorf("%s: the tiles differ", hash)
		}
	}
	table := pageOf(t, asks, "#newer", "")
	dexRow := rowWith(t, table, `data-go="`+dexHash+`"`)
	mustNotHold(t, "the row of a no-answer", dexRow, "badge r", ">Drift<", "Not checked")
	for _, hash := range []string{"#overview", "#findings", "#profiles", "#notchecked"} {
		page := pageOf(t, asks, hash, "")
		mustNotHold(t, hash, page, "No answer", "Refuses", "Palette refuses", "Palette gave")
	}
	// The same counts on the cards of the pack: one drift pack, as in ok.
	navOK, _, _ := strings.Cut(pageOf(t, ok, dexHash, ""), "\n")
	navAsks, _, _ := strings.Cut(pageOf(t, asks, dexHash, ""), "\n")
	if navOK != navAsks {
		t.Error("the nav of the pack differs")
	}
}

// A click asks once. The button is off and reads "Asking..." while the question is on its way, a second click
// sends nothing, and the answer shows beside the move, in the table and on the card.
func TestAClickSendsOneQuestionAndTheAnswerShowsBesideTheMove(t *testing.T) {
	const dex = `{"ask":"fred-identity@2.3.0","pack":"addon/dex"}`
	r := click(t, devdata.StateOK, dexHash, "0", dex)
	mustHold(t, "before", r.Before, `<button class="btn" data-ask="fred-identity@2.3.0" data-pack="addon/dex">Ask Palette about this move</button>`)
	mustHold(t, "during", r.During, `<button class="btn" data-ask="fred-identity@2.3.0" data-pack="addon/dex" disabled>Asking...</button>`)
	mustNotHold(t, "during", r.During, "Ask Palette about this move", "Palette gave")
	mustHold(t, "after", textOf(r.After), `Palette gave no error. Palette checked the profile with the new version and its default values against each cluster that runs the profile. It did not check your own values of the pack.`)
	mustNotHold(t, "after", r.After, "Asking...", "Ask Palette about this move", "The observer could not ask now")
	if len(r.Posts) != 1 || r.Posts[0].Method != "POST" || r.Posts[0].Path != "/v1/packs/fred-identity@2.3.0/addon/dex/ask" {
		t.Fatalf("the posts: %+v", r.Posts)
	}
	// The browser sets Sec-Fetch-Site by itself. The page sets no header.
	if len(r.Posts[0].Headers) != 0 {
		t.Errorf("the page set the headers %v", r.Posts[0].Headers)
	}

	// The table: the cell of the row changes in place, and the click does not open the row.
	r = click(t, devdata.StateOK, "#newer", "0", `{"ask":"fred-identity@2.3.0","pack":"addon/dex","go":"#profiles"}`)
	if r.Hash != "#newer" {
		t.Errorf("the click on the button opened %s", r.Hash)
	}
	mustHold(t, "the table before", rowWith(t, r.Before, `data-go="`+dexHash+`"`), ">Ask Palette</button>")
	mustHold(t, "the table during", rowWith(t, r.During, `data-go="`+dexHash+`"`), `data-pack="addon/dex" disabled>Asking...</button>`)
	row := rowWith(t, r.After, `data-go="`+dexHash+`"`)
	mustHold(t, "the table after", row, `<td class="narrow-hide"><span class="fw">No error</span></td>`)
	mustNotHold(t, "the table after", row, "Ask Palette", "Ask again", "Asking...")
	if len(r.Posts) != 1 {
		t.Errorf("the table sent %d posts", len(r.Posts))
	}
	// The other rows keep their button.
	if got := strings.Count(r.After, ">Ask Palette</button>"); got != 9 {
		t.Errorf("%d rows still offer the question, want 9", got)
	}

	// A refusal shows the code, the message, and the clusters, and the words of Palette stay as text.
	r = click(t, devdata.StateOK, "#k-barney-base@1.5.0/os/ubuntu-aws", "0", `{"ask":"barney-base@1.5.0","pack":"os/ubuntu-aws"}`)
	words := textOf(r.After)
	mustHold(t, "the refusal", words, "Palette refuses: DependencyPackUpgradeNotSupported", "The upgrade of the pack ubuntu-aws from 20.04 to 24.04 is not supported", "on quarry-1", "Nothing on this cluster changed.")
	mustNotHold(t, "the refusal", words, "Ask again")

	// A request that failed is an answer of the kind "no answer". The next click asks again.
	r = click(t, devdata.StateOK, "#k-barney-base@1.5.0/csi/csi-aws-ebs", "0", `{"ask":"barney-base@1.5.0","pack":"csi/csi-aws-ebs"}`)
	mustHold(t, "no answer", textOf(r.After), "Palette gave no answer: HTTP 403.")
	mustHold(t, "no answer", r.After, ">Ask again</button>")
	again := click(t, devdata.StateOK, "#k-barney-base@1.5.0/csi/csi-aws-ebs", "0", `{"ask":"barney-base@1.5.0","pack":"csi/csi-aws-ebs"}`, `{"ask":"barney-base@1.5.0","pack":"csi/csi-aws-ebs"}`)
	if len(again.Posts) != 2 {
		t.Errorf("each click on a no-answer must ask again: %d posts for 2 clicks", len(again.Posts))
	}
}

// A question that the page could not send, or that the observer could not answer, leaves a short note
// and no answer. The button comes back, and the next click asks again.
func TestAQuestionThatFailsLeavesANoteAndNoAnswer(t *testing.T) {
	const note = "The observer could not ask now. Try again later."
	for _, status := range []string{"502", "503", "404", "network"} {
		t.Run(status, func(t *testing.T) {
			r := click(t, devdata.StateOK, dexHash, status, `{"ask":"fred-identity@2.3.0","pack":"addon/dex"}`)
			mustHold(t, "during", r.During, "Asking...")
			mustHold(t, "after", r.After, note, `<button class="btn" data-ask="fred-identity@2.3.0" data-pack="addon/dex">Ask Palette about this move</button>`)
			mustNotHold(t, "after", r.After, "Palette gave", "Palette refuses", "No answer", "Asking...")
			if len(r.Posts) != 1 {
				t.Errorf("%d posts", len(r.Posts))
			}
		})
	}
	// The table shows the note in the cell of the row, and the other rows say nothing.
	r := click(t, devdata.StateOK, "#newer", "503", `{"ask":"fred-identity@2.3.0","pack":"addon/dex"}`)
	mustHold(t, "the table", rowWith(t, r.After, `data-go="`+dexHash+`"`), note, ">Ask Palette</button>")
	if got := strings.Count(r.After, note); got != 1 {
		t.Errorf("the table says the note %d times", got)
	}
	// A move with no reply in the dev data is not found, and the page says the same.
	r = click(t, devdata.StateOK, "#k-dino-storage@1.1.0/addon/longhorn", "0", `{"ask":"dino-storage@1.1.0","pack":"addon/longhorn"}`)
	mustHold(t, "no reply", r.After, note)
	// A click on a move that the data does not hold sends nothing.
	r = click(t, devdata.StateOK, "#newer", "0", `{"ask":"nobody@1.0.0","pack":"addon/none"}`)
	if len(r.Posts) != 0 {
		t.Errorf("a click on an unknown move sent %v", r.Posts)
	}
}

// The held answer of a request that failed gives way to the new answer, in the table and on the card.
func TestAnotherClickOnANoAnswerReplacesIt(t *testing.T) {
	r := click(t, devdata.StateAsks, dexHash, "0", `{"ask":"fred-identity@2.3.0","pack":"addon/dex"}`)
	mustHold(t, "before", textOf(r.Before), "Palette gave no answer: HTTP 403.")
	mustHold(t, "after", textOf(r.After), `Palette gave no error. Palette checked the profile with the new version and its default values against each cluster that runs the profile. It did not check your own values of the pack.`)
	mustNotHold(t, "after", textOf(r.After), "Palette gave no answer", "HTTP 403")
	mustNotHold(t, "after", r.After, "Ask again")
}

// The observer holds one answer for a pack of a profile, and it drops the answer when the move changes. The
// registry can gain a version after the last versions cycle. The answer is then for a newer version than the
// row, and the card and the table show it.
func TestAnAnswerForANewerVersionThanTheRowShowsBesideTheMove(t *testing.T) {
	dir := writeSet(t, devdata.StateAsks, map[string]func([]byte) []byte{"asks.json": func(raw []byte) []byte {
		return []byte(strings.ReplaceAll(string(raw), `"newest":"1.36.2"`, `"newest":"1.37.0"`))
	}})
	pages := pagesOf(t, dir, k8sHash, "#newer")
	mustHold(t, "the card", textOf(pages[0]), `Palette gave no error. Palette checked the profile with the new version and its default values against each cluster that runs the profile. It did not check your own values of the pack.`)
	mustNotHold(t, "the card", pages[0], ">Ask Palette about this move</button>")
	mustHold(t, "the table", rowWith(t, pages[1], `data-go="`+k8sHash+`"`), `<td class="narrow-hide"><span class="fw">No error</span></td>`)
}

// A click always ends with a word. The observer names the newest version of the row also for a pin that failed
// or is not behind any more. An older observer sent no version, and the page still says no answer.
func TestAClickThatGetsAnAnswerWithoutAVersionEndsWithTheWordNoAnswer(t *testing.T) {
	for _, newest := range []string{"2.44.0", ""} {
		t.Run("newest "+newest, func(t *testing.T) {
			reply := `{"asks":[{"profile":"fred-identity@2.3.0","key":"addon/dex","newest":"` + newest + `","verdict":"no-answer","refusals":[],"status":502}]}`
			dir := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"replies.json": func([]byte) []byte { return []byte(reply) }})
			r := clickIn(t, dir, "scratch", dexHash, "0", `{"ask":"fred-identity@2.3.0","pack":"addon/dex"}`)
			mustHold(t, "after", textOf(r.After), "Palette gave no answer: HTTP 502.")
			mustHold(t, "after", r.After, `<button class="btn" data-ask="fred-identity@2.3.0" data-pack="addon/dex">Ask again</button>`)
			mustNotHold(t, "after", r.After, "Ask Palette about this move", "The observer could not ask now")
			table := clickIn(t, dir, "scratch", "#newer", "0", `{"ask":"fred-identity@2.3.0","pack":"addon/dex"}`)
			mustHold(t, "the table", rowWith(t, table.After, `data-go="`+dexHash+`"`), `<span class="mut">No answer</span>`, "Ask again")
		})
	}
}

// The route /v1/asks can fail. The page stays online, and it keeps the last list of answers. With no list
// yet, the page offers the question.
func TestAFailedListOfAnswersKeepsThePageOnlineAndTheLastList(t *testing.T) {
	read := func(state devdata.State, extra ...string) string {
		var page string
		if err := json.Unmarshal(runNode(t, state, append([]string{"asksdown", "#newer"}, extra...)...), &page); err != nil {
			t.Fatal(err)
		}
		return page
	}
	kept := read(devdata.StateAsks)
	mustHold(t, "the kept list", kept, `<span class="fw">No error</span>`, `<span class="fw">Refuses</span>`, `<span class="mut">No answer</span>`)
	mustNotHold(t, "the kept list", kept, "cannot reach the observer")
	first := read(devdata.StateAsks, "first")
	mustHold(t, "no list", first, ">Ask Palette</button>", "Newer versions")
	mustNotHold(t, "no list", first, "cannot reach the observer", `<span class="fw">`, "No answer")
}

// Nothing asks Palette by itself. A crawl of every screen of every set sends GET requests only, and the
// question is sent in one place of the script: the function that a click calls.
func TestNothingAsksWithoutAClick(t *testing.T) {
	for _, state := range []devdata.State{devdata.StateOK, devdata.StateAsks, devdata.StateTextTooLarge, devdata.StateReadError} {
		t.Run(string(state), func(t *testing.T) {
			var requests []struct {
				Method string `json:"method"`
				Path   string `json:"path"`
			}
			if err := json.Unmarshal(runNode(t, state, "requests"), &requests); err != nil {
				t.Fatal(err)
			}
			var asks, diffs int
			for _, r := range requests {
				if r.Method != "GET" {
					t.Errorf("the page sent %s %s without a click", r.Method, r.Path)
				}
				asks += strings.Count(r.Path, "/v1/asks")
				diffs += strings.Count(r.Path, "/diff?side=")
			}
			if asks == 0 || diffs == 0 {
				t.Errorf("the crawl read the list of answers %d times and %d diffs", asks, diffs)
			}
		})
	}
	script := readScripts(t)
	if got := strings.Count(script, `"POST"`); got != 2 {
		t.Errorf("the scripts hold %d POST requests, want 2: the clicks Ask Palette and Check now", got)
	}
	if got := strings.Count(script, "checkNow("); got != 2 {
		t.Errorf("checkNow is called in %d places and defined once, want 2 uses", got)
	}
	if !strings.Contains(script, "if (nw) { checkNow(nw.dataset.now); return; }") {
		t.Error("the click handler does not call checkNow")
	}
	if got := strings.Count(script, "askPalette("); got != 2 {
		t.Errorf("askPalette is called in %d places and defined once, want 2 uses", got)
	}
	if !strings.Contains(script, "if (q) { askPalette(q.dataset.ask, q.dataset.pack); return; }") {
		t.Error("the click handler does not call askPalette")
	}
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
		{devdata.StateOK, dexHash, `{"ask":"fred-identity@2.3.0","pack":"addon/dex"}`},
		{devdata.StateOK, "#k-barney-base@1.5.0/os/ubuntu-aws", `{"ask":"barney-base@1.5.0","pack":"os/ubuntu-aws"}`},
		{devdata.StateOK, "#k-barney-base@1.5.0/csi/csi-aws-ebs", `{"ask":"barney-base@1.5.0","pack":"csi/csi-aws-ebs"}`},
		{devdata.StateOK, "#newer", `{"ask":"fred-identity@2.3.0","pack":"addon/dex"}`},
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
		if strings.Contains(s.HTML, "Palette refuses:") {
			reached++
			mustHold(t, s.Hash+" "+s.Variant, s.HTML, `<span class="m">DependencyPackUpgradeNotSupported`+injection+`&quot;&#39;&amp;</span>`, "on quarry-1"+injection)
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
			if strings.Contains(row, "<img src=x onerror=alert(1)>") {
				lines++
			}
		}
		if s.Variant == "split" || strings.HasPrefix(s.Variant, "split tab") {
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
	for selector, want := range map[string][]string{
		".diff table":     {"table-layout:fixed;", "width:100%;"},
		".diff col.no":    {"width:48px;"},
		".diff td":        {"white-space:pre-wrap;", "overflow-wrap:anywhere;", "border:0;"},
		".diff td.no":     {"user-select:none;", "background:var(--gut);"},
		".diff tr.a td.c": {"background:var(--add);", "color:var(--addw);"},
		".diff tr.d td.c": {"background:var(--del);", "color:var(--delw);"},
		".diff td.ca":     {"background:var(--add);"},
		".diff td.cd":     {"background:var(--del);"},
		".diffhead":       {"display:flex;", "flex-wrap:wrap;"},
		".btn:disabled":   {"cursor:default;"},
	} {
		got := ruleOf(css, selector)
		for _, w := range want {
			if !strings.Contains(got, w) {
				t.Errorf("the rule of %s is %q, and it lacks %s", selector, got, w)
			}
		}
	}
	// The control of the layout keeps its place on a phone: the rule of .seg in the small width scrolls.
	if !strings.Contains(ruleOf(css, ".seg"), "overflow-x:auto;") {
		t.Error("the rule of .seg in the small width does not scroll")
	}
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
