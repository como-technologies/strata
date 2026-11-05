package web

import (
	"regexp"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

/* The tests below hold the words that the book quotes from the page. The book names a headline, a button, or a question of the
card, and app.js writes it. A word of the book that app.js lacks is a word that a person looks for and does not find. */

const (
	servicePage = "../../../docs/src/automation/observer.md"
	serverPage  = "../../../docs/src/development/observer-api.md"
	useCasePage = "../../../docs/src/use-cases/observer.md"
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

// headlineIn gives the way a headline appears in app.js: in double quotes, or in a template when it holds the name of the cluster.
func headlineIn(headline string) string {
	if strings.Contains(headline, "`<cluster>`") {
		return "`" + strings.ReplaceAll(headline, "`<cluster>`", "${clusterName()}") + "`"
	}
	return `"` + headline + `"`
}

// words gives the part of a cell of the book that app.js must hold: no code span, nothing from a placeholder on, and no end mark.
func words(cell string) string {
	cell = strings.ReplaceAll(cell, "`", "")
	if i := strings.Index(cell, "<"); i >= 0 {
		cell = cell[:i]
	}
	return strings.Trim(cell, " .:")
}

// The section "The states of the page" of the page of the service gives the headline of each state, and it quotes the buttons and
// the pages that the next step names. Each of them is a word of app.js. The note of a lost page and of a failed read of the pods
// names a key or a role only for a refusal, as app.js does.
func TestTheBookQuotesTheHeadlinesAndTheWordsOfTheStatesOfThePage(t *testing.T) {
	script := string(readPage(t, "app.js"))
	rows := tableRows(t, sectionOf(t, servicePage, "## The states of the page"))
	if len(rows) < 13 {
		t.Fatalf("the table of the states has %d rows", len(rows))
	}
	quoted := regexp.MustCompile(`"([^"]+)"`)
	for _, row := range rows {
		if len(row) != 3 {
			t.Fatalf("a row of the table of the states has %d cells: %q", len(row), row)
		}
		mustHold(t, "app.js", script, headlineIn(row[0]))
		for _, cell := range row[1:] {
			for _, m := range quoted.FindAllStringSubmatch(cell, -1) {
				mustHold(t, "app.js", script, m[1])
			}
		}
	}
	for _, row := range rows {
		switch row[0] {
		case "We lost the signal from Palette":
			mustHold(t, "the next step of the lost page", row[2], "For HTTP 401 and 403, check the key in the Secret `strata-observer-key`", "For another error, check that the observer reaches Palette")
			mustHold(t, "app.js", script, "Check the key in the Secret", "Check that the observer can reach Palette, then read the log.")
		case "The pods stay silent":
			mustHold(t, "the next step of the pods", row[2], "For HTTP 401 and 403, ask the API server if the role of the observer can list pods", "For another error, read the log")
			mustHold(t, "app.js", script, "Ask the API server if the role of the observer can list pods.", `next("Read the log."`)
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
		{"a refused key", model.ErrorRef{Kind: "api", Status: 403}, "Check the key in the Secret", "Check that the observer can reach Palette"},
		{"a failed link", model.ErrorRef{Kind: "transport"}, "Check that the observer can reach Palette, then read the log.", "Check the key in the Secret"},
		{"another error of Palette", model.ErrorRef{Kind: "api", Status: 500}, "Check that the observer can reach Palette, then read the log.", "Check the key in the Secret"},
	} {
		t.Run(c.name, func(t *testing.T) {
			lastError := c.err
			dir := writeSet(t, devdata.StateLostUnchecked, map[string]func([]byte) []byte{
				"cluster.json": editCluster(t, func(v *api.ClusterView) { v.Content.LastError = &lastError }),
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
	script := string(readPage(t, "app.js"))
	mustNotHold(t, "app.js", script, "a choice and not a fault")
	// The screen of the newer versions keeps the sentence that a newer version is a choice.
	mustHold(t, "app.js", script, "A choice, not a fault, so they stay out of Needs attention.")
	for _, c := range []struct {
		newer int
		want  string
	}{{0, "<span>newer versions</span>"}, {1, "<span>newer version</span>"}, {3, "<span>newer versions</span>"}} {
		dir := writeSet(t, devdata.StateQuiet, map[string]func([]byte) []byte{
			"cluster.json": editCluster(t, func(v *api.ClusterView) { v.Counts.Newer = c.newer }),
		})
		mustHold(t, "the facts of All quiet", pageWith(t, dir, "scratch", "#findings", ""), c.want)
	}
}

// The page of the service and the page of the server give the same questions as the card "Check this yourself", in the words of app.js.
func TestTheBookQuotesTheQuestionsOfTheCardCheckThisYourself(t *testing.T) {
	script := string(readPage(t, "app.js"))
	table := tableRows(t, sectionOf(t, serverPage, "### Check this yourself"))
	if len(table) != 7 {
		t.Fatalf("the table of the card has %d rows", len(table))
	}
	useCase := strings.Join(strings.Fields(string(readFile(t, useCasePage))), " ")
	for _, row := range table {
		question := strings.TrimSuffix(row[0], ".")
		mustHold(t, "app.js", script, `["`+question+`"`)
		mustHold(t, "the use case", useCase, question)
	}
}

// The column "Palette" of the table of the newer versions, and the card "Move to", give an answer of Palette in words. The page of the
// service quotes both. The words of the button and of each verdict are words of app.js.
func TestTheBookQuotesTheWordsOfTheColumnPaletteAndOfTheCardMoveTo(t *testing.T) {
	script := string(readPage(t, "app.js"))
	rows := tableRows(t, sectionOf(t, servicePage, "## Ask Palette about a move"))
	if len(rows) != 4 {
		t.Fatalf("the table of the answers has %d rows", len(rows))
	}
	for _, row := range rows {
		mustHold(t, "app.js", script, words(row[0]), words(row[1]))
	}
	mustHold(t, "app.js", script, `<th class="narrow-hide">Palette</th>`, "Ask again", `The page "behind" in the book says what this answer covers.`)
}

// The table of the splashes in the page of the server holds the headline of each splash of app.js, and no other. A place with a
// splash that the table omits is a place that a person cannot find in the book.
func TestTheTableOfTheSplashesHoldsEachHeadlineOfTheScript(t *testing.T) {
	script := string(readPage(t, "app.js"))
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
			mustHold(t, "app.js", script, headlineIn(headline))
		}
	}
	titles := regexp.MustCompile("splash\\((?:TONE\\.[A-Z]+|settledTone\\(\\)|quietTone\\(\\)), (\"[^\"]+\"|`[^`]+`)").FindAllStringSubmatch(script, -1)
	titles = append(titles, regexp.MustCompile(`\{title: ("[^"]+"), name[:,]`).FindAllStringSubmatch(script, -1)...)
	if len(titles) < 25 {
		t.Fatalf("the test found %d splashes in app.js", len(titles))
	}
	for _, m := range titles {
		headline := strings.ReplaceAll(m[1][1:len(m[1])-1], "${clusterName()}", "`<cluster>`")
		if !inTable[headline] {
			t.Errorf("app.js has a splash %q, and the table of the splashes in the page of the server omits it", headline)
		}
	}
}
