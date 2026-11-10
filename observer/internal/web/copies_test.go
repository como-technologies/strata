package web

import (
	"regexp"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

/* The tests below hold the words and the copies of the scripts. Some read the scripts as text, so they need no node. The others draw a
screen with node, as the tests of web_test.go do. */

// builders are the functions of the scripts that put a value from the data into a command. Each one quotes its values with shq().
var builders = []string{"publishedCommand", "clusterCommand", "packRef", "pinCommand", "firstSeenCommand"}

// linesOf gives the lines of the script without a comment line.
func linesOf(script string) []string {
	var out []string
	for _, l := range strings.Split(script, "\n") {
		if !strings.HasPrefix(strings.TrimSpace(l), "//") {
			out = append(out, l)
		}
	}
	return out
}

// A value reaches a command only through shq(). The builders quote each value, no other line puts a value in a command, and shq()
// writes each apostrophe of a value as '\” inside single quotes.
func TestEachBuilderOfACommandQuotesItsValues(t *testing.T) {
	script := readScripts(t)
	mustHold(t, "the scripts", script, "const shq = ", "`'${String(s).replace(/'/g, `'\\\\''`)}'`")
	if n := strings.Count(script, "const shq = "); n != 1 {
		t.Errorf("the scripts define shq %d times, want 1", n)
	}
	builder := map[string]string{}
	for _, l := range linesOf(script) {
		for _, name := range builders {
			if strings.HasPrefix(l, "const "+name+" = ") {
				builder[name] = l
			}
		}
	}
	for _, name := range builders {
		if line, ok := builder[name]; !ok {
			t.Errorf("the scripts have no builder %s", name)
		} else if !strings.Contains(line, "shq(") {
			t.Errorf("the builder %s puts a value in a command with no shq(): %s", name, line)
		}
	}
	// A line that is no builder writes a command with the file of the record and a quoted word only.
	command := regexp.MustCompile("`((?:strata (?:compare|behind|verify)|scripts/ci/observer-records\\.sh)[^`]*)`")
	for _, l := range linesOf(script) {
		if startsBuilder(strings.TrimSpace(l)) {
			continue
		}
		for _, m := range command.FindAllStringSubmatch(l, -1) {
			for _, part := range strings.Split(m[1], "${")[1:] {
				if !strings.HasPrefix(part, "RECORD_FILE}") && !strings.HasPrefix(part, "packRef(") && !strings.HasPrefix(part, "shq(") {
					t.Errorf("a line writes a command with a value that no builder quotes: %s", l)
				}
			}
		}
	}
}

// startsBuilder says that the line starts the const of a builder.
func startsBuilder(line string) bool {
	for _, name := range builders {
		if strings.HasPrefix(line, "const "+name+" = ") {
			return true
		}
	}
	return false
}

// Each copy of the script has one home: the test for a refused status, the period of the refresh, the name of the cluster, the
// badge of an event, and the groups of the statuses of a copy.
func TestEachCopyOfTheScriptHasOneHome(t *testing.T) {
	script := readScripts(t)
	for _, c := range []struct {
		what, text string
		want       int
	}{
		{"the test for a refused status", "status === 401", 1},
		{"the literal of the period", "30 seconds", 0},
		{"the words of the period", "REFRESH_MS / 1000", 1},
		{"the name of the cluster", "cluster.name || D.cluster.cluster.uid", 1},
		{"a badge of an event by hand", "EVENT_BADGE[", 0},
		{"a badge drawn from a lookup by hand", "badge(c, t)", 1},
	} {
		if got := strings.Count(script, c.text); got != c.want {
			t.Errorf("%s: the scripts hold %q %d times, want %d", c.what, c.text, got, c.want)
		}
	}
	if m := regexp.MustCompile(`===\s*COPY\.\w+\s*\|\|`).FindString(script); m != "" {
		t.Errorf("the scripts test a group of the statuses of a copy by hand: %q", m)
	}
}

// The check of a copy of this cluster writes the badge of "same-content" in the words of the book, "Same content", in the card of the
// versions and in the card "Check this yourself". It is not a drift, and it does not read OK.
func TestSameContentReadsTheWordsOfTheBookEverywhere(t *testing.T) {
	book := strings.Join(strings.Fields(string(readFile(t, "../../../docs/src/development/observer-api.md"))), " ")
	mustHold(t, "observer-api.md", book, "`same-content` reads \"Same content\" on the row of the cluster copy.")
	dir := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"profiles.json": func(raw []byte) []byte {
		var v api.ProfilesView
		if err := devdata.Decode(raw, &v); err != nil {
			t.Fatal(err)
		}
		for i := range v.Profiles {
			if v.Profiles[i].ClusterCopy != nil {
				v.Profiles[i].ClusterCopy.Status = model.CopySameContent
			}
		}
		return mustJSON(t, v)
	}})
	const same = `<span class="badge g">Same content</span>`
	page := pagesOf(t, dir, "#p-"+dexSubject)[0]
	mustHold(t, "the card of the versions", page, `<div class="h">This cluster's copy in Palette `+same+`</div>`)
	var answer string
	for _, r := range termRows(t, page) {
		if r.question == "Does this cluster still run the record?" {
			answer = r.answer
		}
	}
	if answer != same {
		t.Errorf("the card Check this yourself says %q for the copy, want %q", answer, same)
	}
}

// The note of a finding about a published version that changed agrees with the use case "A published version is edited in place": the
// book gives the reason, so the note says not to repair the version in place, and to publish a new version through review.
func TestTheNoteOfAFindingAgreesWithTheUseCaseOfTheEditInPlace(t *testing.T) {
	useCase := strings.Join(strings.Fields(string(readFile(t, "../../../docs/src/use-cases/edited-in-place.md"))), " ")
	script := readScripts(t)
	for _, sentence := range []string{
		"Do not repair the edited version in place: that is a second edit.",
		"Publish a new version with the change through review, and move the clusters to it.",
	} {
		mustHold(t, "the use case", useCase, sentence)
		mustHold(t, "the scripts", script, sentence)
	}
	mustNotHold(t, "the scripts", script, "Restore the version first")
	mustNotHold(t, "the use case", useCase, "Restore the version first")
	page := shown(t, devdata.StateOK, "#p-"+dexSubject)[0]
	mustHold(t, "the profile", page, `<div class="note r small"><b>What to do.</b> Do not repair the edited version in place: that is a second edit. `+
		`Publish a new version with the change through review, and move the clusters to it. A new cluster gets this edit at once. `+
		`The finding clears when the version goes back to its record.`)
	mustNotHold(t, "the profile", page, "Restore the version")
}
