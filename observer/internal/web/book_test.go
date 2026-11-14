package web

import (
	"regexp"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

/* The tests below hold the words that the book quotes from the page. The book names a headline, a button, or a question of the
card, and the scripts write it. A word of the book that the scripts lack is a word that a person looks for and does not find. */

const (
	servicePage = "../../../docs/src/automation/observer.md"
	checksPage  = "../../../docs/src/development/observer.md"
	serverPage  = "../../../docs/src/development/observer-api.md"
	useCasePage = "../../../docs/src/use-cases/observer.md"
)

// askSection is the section of servicePage about the button "Ask Palette", and askMove is its heading with no level.
const (
	askMove    = askButton + " about a move"
	askSection = "## " + askMove
)

// sectionOf gives the text of a section of a page, from its heading to the next heading of the same level.
func sectionOf(t *testing.T, page, heading string) string {
	t.Helper()
	text := "\n" + string(readFile(t, page))
	level := strings.SplitN(heading, " ", 2)[0]
	from := strings.Index(text, "\n"+heading+"\n")
	if from < 0 {
		t.Fatalf("%s has no section %q", page, heading)
	}
	rest := text[from+1:]
	if to := strings.Index(rest[1:], "\n"+level+" "); to >= 0 {
		rest = rest[:to+1]
	}
	return rest
}

// tableRows gives the cells of each row of the first table in the text, with no header row and no rule row.
func tableRows(t *testing.T, text string) [][]string {
	t.Helper()
	var rows [][]string
	seen := 0
	for _, line := range strings.Split(text, "\n") {
		if !strings.HasPrefix(line, "| ") {
			if len(rows) > 0 {
				break
			}
			seen = 0
			continue
		}
		seen++
		if seen <= 2 {
			continue
		}
		cells := strings.Split(strings.Trim(line, "| "), " | ")
		rows = append(rows, cells)
	}
	if len(rows) == 0 {
		t.Fatalf("no table in %q", text)
	}
	return rows
}

// headlineIn gives the way a headline appears in the scripts: in double quotes, or in a template when it holds the name of the cluster.
func headlineIn(headline string) string {
	if strings.Contains(headline, "`<cluster>`") {
		return "`" + strings.ReplaceAll(headline, "`<cluster>`", "${clusterName()}") + "`"
	}
	return `"` + headline + `"`
}

// words gives the part of a cell of the book that the scripts must hold. The part holds no code span, nothing from a
// placeholder on, and no end mark.
func words(cell string) string {
	cell = strings.ReplaceAll(cell, "`", "")
	if i := strings.Index(cell, "<"); i >= 0 {
		cell = cell[:i]
	}
	return strings.Trim(cell, " .:")
}

// The section "What each state means" of the page of the server gives the headline of each state. It quotes the buttons and the
// pages that the next step names. Each of them is a word of the scripts. The note of a lost page and of a failed read of the pods
// names a key or a role only for a refusal. The scripts do the same.
func TestTheBookQuotesTheHeadlinesAndTheWordsOfTheStatesOfThePage(t *testing.T) {
	script := readScripts(t)
	rows := tableRows(t, sectionOf(t, serverPage, "### What each state means"))
	if len(rows) < 13 {
		t.Fatalf("the table of the states has %d rows", len(rows))
	}
	quoted := regexp.MustCompile(`"([^"]+)"`)
	for _, row := range rows {
		if len(row) != 3 {
			t.Fatalf("a row of the table of the states has %d cells: %q", len(row), row)
		}
		mustHold(t, "the scripts", script, headlineIn(row[0]))
		for _, cell := range row[1:] {
			for _, m := range quoted.FindAllStringSubmatch(cell, -1) {
				mustHold(t, "the scripts", script, m[1])
			}
		}
	}
	// The book quotes a next step of the page with a small first letter and no period.
	quote := func(words string) string { return strings.ToLower(words[:1]) + strings.TrimSuffix(words[1:], ".") }
	const refusal, other = "For HTTP 401 and 403, ", "For another error, "
	for _, row := range rows {
		switch row[0] {
		case lostTitle:
			mustHold(t, "the next step of the lost page", row[2], refusal+quote(checkTheKey)+" `"+keySecret+"`", other+"check that the observer reaches Palette")
			mustHold(t, "the scripts", script, checkTheKey, checkTheLink)
		case podsTitle:
			mustHold(t, "the next step of the pods", row[2], refusal+quote(askListPods), other+"read the log")
			mustHold(t, "the scripts", script, askListPods, `next("Read the log."`)
		}
	}
}

// A lost page asks for the key only when Palette refused it: HTTP 401 or 403. Another error asks for the link.
func TestALostPageNamesTheKeyOnlyForARefusal(t *testing.T) {
	for _, c := range []struct {
		name string
		err  model.ErrorRef
		want string
		not  string
	}{
		{"a refused key", model.ErrorRef{Kind: docs.KindAPI, Status: 403}, checkTheKey, reachPalette},
		{"a failed link", model.ErrorRef{Kind: docs.KindTransport}, checkTheLink, checkTheKey},
		{"another error of Palette", model.ErrorRef{Kind: docs.KindAPI, Status: 500}, checkTheLink, checkTheKey},
	} {
		t.Run(c.name, func(t *testing.T) {
			lastError := c.err
			dir := writeSet(t, devdata.StateLostUnchecked, map[string]func([]byte) []byte{
				"cluster.json": editJSON(t, func(v *api.ClusterView) { v.Content.LastError = &lastError }),
			})
			page := pageWith(t, dir, "scratch", "#overview", "")
			mustHold(t, c.name, page, c.want)
			mustNotHold(t, c.name, page, c.not)
		})
	}
}

// The third fact of "All quiet" has the words of the other two: a count and a short label, for one line each. It reads "newer
// version" for one, as the first fact reads "profile matches its record".
func TestTheThirdFactOfAllQuietIsASingleShortLabel(t *testing.T) {
	script := readScripts(t)
	mustNotHold(t, "the scripts", script, "a choice and not a fault")
	// The screen of the newer versions keeps the sentence that a newer version is a choice.
	mustHold(t, "the scripts", script, "A choice, not a fault, so they stay out of "+needsAttention+".")
	for _, c := range []struct {
		newer int
		want  string
	}{{0, newerFact}, {1, "<span>newer version</span>"}, {3, newerFact}} {
		dir := writeSet(t, devdata.StateQuiet, map[string]func([]byte) []byte{
			"cluster.json": editJSON(t, func(v *api.ClusterView) { v.Counts.Newer = c.newer }),
		})
		mustHold(t, "the facts of "+calmQuiet, pageWith(t, dir, "scratch", "#findings", ""), c.want)
	}
}

// The page of the service and the page of the server give the same questions as the card "Check this yourself". They use the
// words of the scripts.
func TestTheBookQuotesTheQuestionsOfTheCardCheckThisYourself(t *testing.T) {
	script := readScripts(t)
	table := tableRows(t, sectionOf(t, serverPage, "### Check this yourself"))
	if len(table) != 8 {
		t.Fatalf("the table of the card has %d rows", len(table))
	}
	useCase := strings.Join(strings.Fields(string(readFile(t, useCasePage))), " ")
	for _, row := range table {
		question := strings.TrimSuffix(row[0], ".")
		mustHold(t, "the scripts", script, `["`+question+`"`)
		mustHold(t, "the use case", useCase, question)
	}
}

// The column "Palette" of the table of the newer versions, and the card "Move to", give an answer of Palette in words. The page of the
// service quotes both. The words of the button and of each verdict are words of the scripts.
func TestTheBookQuotesTheWordsOfTheColumnPaletteAndOfTheCardMoveTo(t *testing.T) {
	script := readScripts(t)
	rows := tableRows(t, sectionOf(t, servicePage, askSection))
	if len(rows) != 4 {
		t.Fatalf("the table of the answers has %d rows", len(rows))
	}
	for _, row := range rows {
		mustHold(t, "the scripts", script, words(row[0]), words(row[1]))
	}
	mustHold(t, "the scripts", script, `<th class="narrow-hide" title="${ASK_WHAT}">Palette</th>`, askAgain, "${ASK_COVERS}", "It changes nothing.", notYourValues)
}

// The table of the splashes in the page of the server holds the headline of each splash of the scripts, and no other. A place with a
// splash that the table omits is a place that a person cannot find in the book.
func TestTheTableOfTheSplashesHoldsEachHeadlineOfTheScript(t *testing.T) {
	script := readScripts(t)
	section := sectionOf(t, serverPage, "### The splash")
	rows := tableRows(t, section[strings.Index(section, "These places have a splash:"):])
	inTable := map[string]bool{}
	for _, row := range rows {
		if len(row) != 4 {
			t.Fatalf("a row of the table of the splashes has %d cells: %q", len(row), row)
		}
		if strings.HasPrefix(row[3], "The sentence of") {
			continue
		}
		// A cell holds one headline, or a list of them. A headline can hold a comma, as "One moment, a check is running" does.
		headlines := []string{row[3]}
		if !strings.Contains(script, headlineIn(row[3])) {
			headlines = strings.Split(strings.ReplaceAll(row[3], ", or ", ", "), ", ")
		}
		for _, headline := range headlines {
			inTable[headline] = true
			mustHold(t, "the scripts", script, headlineIn(headline))
		}
	}
	titles := regexp.MustCompile("splash\\((?:TONE\\.[A-Z]+|settledTone\\(\\)|quietTone\\(\\)), (\"[^\"]+\"|`[^`]+`)").FindAllStringSubmatch(script, -1)
	titles = append(titles, regexp.MustCompile(`\{title: ("[^"]+"), name[:,]`).FindAllStringSubmatch(script, -1)...)
	if len(titles) < 25 {
		t.Fatalf("the test found %d splashes in the scripts", len(titles))
	}
	for _, m := range titles {
		headline := strings.ReplaceAll(m[1][1:len(m[1])-1], "${clusterName()}", "`<cluster>`")
		if !inTable[headline] {
			t.Errorf("the scripts have a splash %q, and the table of the splashes in the page of the server omits it", headline)
		}
	}
}
