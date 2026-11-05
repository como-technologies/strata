package web

import (
	"fmt"
	htmlpkg "html"
	"io/fs"
	"regexp"
	"strconv"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

/* The tests below hold the table of changed values for a value of many lines. The table shows the lines that
differ, and the text of the values below it keeps the whole diff. */

// contentPath is the path of the file of the kubeadm configuration, in the set ok and in the tests below.
const contentPath = "kubeadmconfig.files[2].content"

// textID is the id of the text of the values on the Values tab of the move of kubernetes.
const textID = k8sMove + "-text"

// lineValue makes a value of n lines. Line i is "  key-ii: v", and v is tag for the lines from first to last,
// else "same".
func lineValue(n, first, last int, tag string) string {
	var b strings.Builder
	for i := 1; i <= n; i++ {
		v := "same"
		if i >= first && i <= last {
			v = tag
		}
		fmt.Fprintf(&b, "  key-%02d: %s\n", i, v)
	}
	return b.String()
}

// longSet makes a directory of sets whose pack kubernetes holds the one changed value leaf. It also changes the
// entry of the newer side with each edit.
func longSet(t *testing.T, leaf docs.Leaf, edit ...func(*api.PackDiff)) string {
	t.Helper()
	return writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"diffs.json": editDiffs(t, func(ds []api.PackDiff) {
		d := entryOf(t, ds, k8sSubject, "k8s/kubernetes", model.SideNewer)
		d.Values.Settings = []docs.Leaf{leaf}
		for _, f := range edit {
			f(d)
		}
	})})
}

func changedLeaf(old, now any) docs.Leaf {
	return docs.Leaf{Op: "changed", Path: contentPath, Old: old, New: now}
}

var (
	valueLineMarkup = regexp.MustCompile(`<div class="vl ([a-z]+)"><span class="no">([^<]*)</span><span class="mk">([^<]*)</span><span class="t">([^<]*)</span></div>`)
	valueCell       = regexp.MustCompile(`(?s)<td class="(?:was|now) val vm">(.*?)</td>`)
)

const gap = "…"

// shownLines reads the lines that a cell shows. A line is "class|number|mark|text", and a gap is "…". The class
// is c for a line that did not change, d for a removed line, and a for an added line.
func shownLines(cell string) []string {
	var out []string
	for _, m := range valueLineMarkup.FindAllStringSubmatch(cell, -1) {
		if m[1] == "e" {
			out = append(out, gap)
			continue
		}
		out = append(out, strings.Join([]string{m[1], m[2], m[3], htmlpkg.UnescapeString(m[4])}, "|"))
	}
	return out
}

// sidesOf gives the lines of the Before cell and of the After cell in the row of a path.
func sidesOf(t *testing.T, page, path string) (before, after []string) {
	t.Helper()
	row := rowWith(t, page, `<td class="m">`+path+`</td>`)
	cells := valueCell.FindAllStringSubmatch(row, -1)
	if len(cells) != 2 {
		t.Fatalf("the row of %s has %d cells with lines, want 2: %.300s", path, len(cells), row)
	}
	return shownLines(cells[0][1]), shownLines(cells[1][1])
}

var (
	lineHeight = regexp.MustCompile(`font:[0-9.]+px/([0-9.]+)px`)
	padding    = regexp.MustCompile(`padding:([0-9.]+)px`)
	border     = regexp.MustCompile(`border-bottom:([0-9.]+)px`)
)

// maxChars is the longest line that fits in a cell: the cell has a maximum width of 60 characters, less the
// padding, the number, and the mark. A longer line wraps and makes the row taller.
const maxChars = 48

// rowHeight gives the height in pixels that app.css gives to a row of lines: one line height for each line of
// the longer side, with the padding and the border of a cell. A browser adds no more while no line wraps, so
// the test refuses a line above maxChars. The width of the window changes nothing, because the cell has a
// maximum width and the table gives the rest of the width to the column of the path.
func rowHeight(t *testing.T, before, after []string) float64 {
	t.Helper()
	raw, err := fs.ReadFile(files(t), "app.css")
	if err != nil {
		t.Fatal(err)
	}
	css := string(raw)
	line, cell := lineHeight.FindStringSubmatch(ruleOf(css, ".vls")), ruleOf(css, "td")
	pad, edge := padding.FindStringSubmatch(cell), border.FindStringSubmatch(cell)
	if line == nil || pad == nil || edge == nil {
		t.Fatalf("app.css has no line height for .vls, or no padding and border for td: %q %q", ruleOf(css, ".vls"), cell)
	}
	px := func(m []string) float64 {
		v, err := strconv.ParseFloat(m[1], 64)
		if err != nil {
			t.Fatal(err)
		}
		return v
	}
	for _, l := range append(append([]string{}, before...), after...) {
		if l == gap {
			continue
		}
		if text := strings.SplitN(l, "|", 4)[3]; len([]rune(text)) > maxChars {
			t.Errorf("the line %q has more than %d characters, so it can wrap", l, maxChars)
		}
	}
	return float64(max(len(before), len(after)))*px(line) + 2*px(pad) + px(edge)
}

func countClass(lines []string, class string) int {
	n := 0
	for _, l := range lines {
		if strings.HasPrefix(l, class+"|") {
			n++
		}
	}
	return n
}

// A value of 24 lines with 6 changed lines shows those lines and the lines around them: at most 3 lines that
// did not change, and a gap where lines are left out. The row is less than 200 pixels tall.
func TestAValueOfManyLinesShowsOnlyTheLinesThatDifferWithAtMostThreeLinesOfContext(t *testing.T) {
	dir := longSet(t, changedLeaf(lineValue(24, 10, 15, "old"), lineValue(24, 10, 15, "new")))
	before, after := sidesOf(t, pagesOf(t, dir, k8sHash)[0], contentPath)
	wantBefore := []string{gap, "c|8||  key-08: same", "c|9||  key-09: same"}
	wantAfter := append([]string{}, wantBefore...)
	for i := 10; i <= 15; i++ {
		wantBefore = append(wantBefore, fmt.Sprintf("d|%d|-|  key-%02d: old", i, i))
		wantAfter = append(wantAfter, fmt.Sprintf("a|%d|+|  key-%02d: new", i, i))
	}
	wantBefore = append(wantBefore, "c|16||  key-16: same", gap)
	wantAfter = append(wantAfter, "c|16||  key-16: same", gap)
	sameRows(t, "Before", before, wantBefore)
	sameRows(t, "After", after, wantAfter)
	for side, lines := range map[string][]string{"Before": before, "After": after} {
		if n := countClass(lines, "c"); n > 3 {
			t.Errorf("%s shows %d lines of context, want at most 3", side, n)
		}
	}
	if h := rowHeight(t, before, after); h >= 200 {
		t.Errorf("the row is %.0f pixels tall, want less than 200", h)
	}
}

// The real file of the pack kubernetes, from a run of strata compare on the move to 1.36.2, has 21 lines and
// three changed versions. A line that holds the same text on both sides, between two changed lines, is not marked.
func TestTheRealKubeadmFileOfKubernetesShowsItsThreeChangedLines(t *testing.T) {
	set := loadSet(t, devdata.StateOK)
	leaf := entryOf(t, set.Diffs, k8sSubject, "k8s/kubernetes", model.SideNewer).Values.Settings[0]
	if old, _ := leaf.Old.(string); leaf.Path != contentPath || strings.Count(old, "\n") != 21 {
		t.Fatalf("the set ok must hold the real file of 21 lines as the first setting of kubernetes, not %q", leaf.Path)
	}
	before, after := sidesOf(t, pagesOf(t, writeSet(t, devdata.StateOK, nil), k8sHash)[0], contentPath)
	side := func(class, mark, version string) []string {
		return []string{
			gap,
			"c|8||    defaults:",
			"c|9||      enforce: \"baseline\"",
			class + "|10|" + mark + "|      enforce-version: \"" + version + "\"",
			"c|11||      audit: \"baseline\"",
			class + "|12|" + mark + "|      audit-version: \"" + version + "\"",
			"c|13||      warn: \"restricted\"",
			class + "|14|" + mark + "|      warn-version: \"" + version + "\"",
			"c|15||    exemptions:",
			gap,
		}
	}
	sameRows(t, "Before", before, side("d", "-", "v1.35"))
	sameRows(t, "After", after, side("a", "+", "v1.36"))
	if h := rowHeight(t, before, after); h >= 200 {
		t.Errorf("the row is %.0f pixels tall, want less than 200", h)
	}
}

// A value of one line reads as it did: the whole value, with quotes for a string. A value with one end of line at
// its end, on both sides, is a value of one line.
func TestAValueOfOneLineReadsAsItDid(t *testing.T) {
	for _, c := range []struct {
		name     string
		old, now any
		want     string
	}{
		{"strings", "v1.35", "v1.36", `<td class="was val">&quot;v1.35&quot;</td><td class="now val">&quot;v1.36&quot;</td>`},
		{"numbers", 1, 2, `<td class="was val">1</td><td class="now val">2</td>`},
		{"one line with its end of line", "v1\n", "v2\n", "<td class=\"was val\">&quot;v1\n&quot;</td><td class=\"now val\">&quot;v2\n&quot;</td>"},
		{"an added value", nil, "code", `<td class="was val">&ndash;</td><td class="now val">&quot;code&quot;</td>`},
	} {
		leaf := changedLeaf(c.old, c.now)
		page := pagesOf(t, longSet(t, leaf), k8sHash)[0]
		row := rowWith(t, page, `<td class="m">`+contentPath+`</td>`)
		mustHold(t, c.name, row, c.want)
		mustNotHold(t, c.name, row, `class="vls"`, "val vm")
	}
}

// More than 40 lines that differ show as a count that links to the text of the values. Exactly 40 show their lines.
func TestAValueWithMoreThanFortyChangedLinesSaysHowManyAndLinksToTheText(t *testing.T) {
	for _, c := range []struct {
		differ int
		shows  bool
	}{{40, true}, {41, false}, {60, false}} {
		dir := longSet(t, changedLeaf(lineValue(70, 3, 2+c.differ, "old"), lineValue(70, 3, 2+c.differ, "new")))
		page := pagesOf(t, dir, k8sHash)[0]
		row := rowWith(t, page, `<td class="m">`+contentPath+`</td>`)
		count := fmt.Sprintf("%d lines differ", c.differ)
		if c.shows {
			mustNotHold(t, count, row, count, "data-jump")
			before, _ := sidesOf(t, page, contentPath)
			if got := countClass(before, "d"); got != c.differ {
				t.Errorf("%d changed lines show %d removed lines", c.differ, got)
			}
			continue
		}
		mustHold(t, count, row, `<td class="val" colspan="2"><button class="lnk" data-jump="`+textID+`">`+count+`</button></td>`)
		mustNotHold(t, count, row, `class="vls"`)
		// The text below the table has that id, and a click on the link jumps to it without a change of the screen.
		mustHold(t, count, page, `<div class="dblock" id="`+textID+`">`)
		r := clickIn(t, dir, "scratch", k8sHash, "0", `{"jump":"`+textID+`"}`)
		if len(r.Jumps) == 0 || r.Jumps[0] != textID || r.Hash != k8sHash {
			t.Errorf("a click on the link jumped to %v and left the hash %q, want the id %q and the hash %q", r.Jumps, r.Hash, textID, k8sHash)
		}
	}
}

// A page that holds no text of the values gives the count and no link.
func TestAValueWithMoreThanFortyChangedLinesHasNoLinkWhenThePageHoldsNoText(t *testing.T) {
	dir := longSet(t, changedLeaf(lineValue(70, 3, 50, "old"), lineValue(70, 3, 50, "new")), func(d *api.PackDiff) { d.Values.Unified = "" })
	row := rowWith(t, pagesOf(t, dir, k8sHash)[0], `<td class="m">`+contentPath+`</td>`)
	mustHold(t, "the row", row, `<td class="val" colspan="2">48 lines differ</td>`)
	mustNotHold(t, "the row", row, "data-jump", "<button")
}

// A mask in a line is the text of the line: the page shows the mask, in a changed line and in a line around it, and it
// shows no other text for the secret. A line that the page leaves out shows nothing.
func TestAMaskInALineOfAValueStaysAMask(t *testing.T) {
	const clear = "hunter2-CLEAR-VALUE"
	text := func(version string) string {
		return strings.Join([]string{"note: " + clear, "user: admin", "password: ********", "version: " + version, "token: ********", "port: 8080", ""}, "\n")
	}
	dir := longSet(t, changedLeaf(text("1"), text("2")))
	page := pagesOf(t, dir, k8sHash)[0]
	before, after := sidesOf(t, page, contentPath)
	wantMasked := "c|3||password: ********"
	if !(strings.Contains(strings.Join(before, "\n"), wantMasked) && strings.Contains(strings.Join(after, "\n"), wantMasked)) {
		t.Errorf("the lines do not show the mask of the password: %q and %q", before, after)
	}
	sameRows(t, "Before", before, []string{gap, "c|2||user: admin", wantMasked, "d|4|-|version: 1", "c|5||token: ********", gap})
	sameRows(t, "After", after, []string{gap, "c|2||user: admin", wantMasked, "a|4|+|version: 2", "c|5||token: ********", gap})
	mustNotHold(t, "the page", page, clear)
}

// The text of the values below the table, in Unified and in Split, is the same with or without a value of many lines.
func TestTheUnifiedAndSplitViewsDoNotChange(t *testing.T) {
	with := longSet(t, changedLeaf(lineValue(24, 10, 15, "old"), lineValue(24, 10, 15, "new")))
	without := longSet(t, changedLeaf("v1.35", "v1.36"))
	textOf := func(page string) string {
		card := cardOf(t, page, `data-tab="`+k8sMove+`"`)
		at := strings.Index(card, `<div class="dblock"`)
		if at < 0 {
			t.Fatal("the card shows no text of the values")
		}
		return card[at:]
	}
	for _, layout := range []string{layoutUnified, layoutSplit} {
		var pages [2]string
		for i, dir := range []string{with, without} {
			if layout == layoutSplit {
				pages[i] = clickIn(t, dir, "scratch", k8sHash, "0", `{"view":"split"}`).After
			} else {
				pages[i] = pagesOf(t, dir, k8sHash)[0]
			}
		}
		if a, b := textOf(pages[0]), textOf(pages[1]); a != b {
			t.Errorf("the %s text changes with a value of many lines", layout)
		}
	}
}

// A line of a value with markup shows as text.
func TestALineOfAValueOfManyLinesWithMarkupShowsAsText(t *testing.T) {
	const evil = "<img src=x onerror=alert(1)>"
	dir := longSet(t, changedLeaf("a: 1\n"+evil+"\n", "a: 2\n"+evil+"\n"))
	page := pagesOf(t, dir, k8sHash)[0]
	mustHold(t, "the row", rowWith(t, page, `<td class="m">`+contentPath+`</td>`), `<span class="t">`+injection+`</span>`)
	mustNotHold(t, "the page", page, "<img")
}
