package web

import (
	"regexp"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/testkit"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

// The words of the card "Published version" of a profile page for the statuses unverified and error. publishedSection is the
// section of servicePage about the card.
const (
	unverifiedWords  = "Strata read this version, but not the upstream digest of each pack."
	errorWords       = "Strata could not read this version."
	publishedSection = `### The card "Published version"`
)

// vcardOpen starts the card of a side of a profile page, and publishedHead starts the card "Published version".
const (
	vcardOpen     = `<div class="vcard">`
	publishedHead = vcardOpen + `<div class="h">Published version `
)

// publishedCard gives the card "Published version" of a profile page, up to the card of the other side.
func publishedCard(t *testing.T, page string) string {
	t.Helper()
	_, card, found := strings.Cut(page, publishedHead)
	if !found {
		t.Fatal("the page has no card Published version")
	}
	card, _, _ = strings.Cut(card, vcardOpen)
	return card
}

// publishedOf gives the card "Published version" of the page of the profile subject in a crawl of the set.
func publishedOf(t *testing.T, state devdata.State, subject string) string {
	t.Helper()
	return publishedCard(t, pageOf(t, crawl(t, state), "#p-"+subject, ""))
}

// Issue #958. gazoo-backup@1.2.0 of the set ok is unverified, and fred-identity@2.3.0 of the set read-error has an error. The card
// "Published version" of each one has the badge "Not checked" and the words of its status. It does not have the words of the other
// status.
func TestTheCardPublishedVersionGivesUnverifiedAndErrorWordsOfTheirOwn(t *testing.T) {
	for _, c := range []struct {
		state      devdata.State
		subject    string
		words, not string
	}{
		{devdata.StateOK, testkit.GazooProfile, unverifiedWords, errorWords},
		{devdata.StateReadError, testkit.DexProfile, errorWords, unverifiedWords},
	} {
		card := publishedOf(t, c.state, c.subject)
		mustHold(t, string(c.state), card, badge("y", wordNotChecked), c.words)
		mustNotHold(t, string(c.state), card, c.not)
	}
}

// Issue #958. The page of the service has a section about the card "Published version". Its table has one row for each case below,
// in this order. The first cell names the status of the record in the set. The card of the profile shows each word that the other
// cells quote. barney-base@1.5.0 of the set read-error is unchanged, and the cause read-error names it. The book also quotes the
// words for a profile with no record, and the scripts hold them.
func TestTheBookGivesTheBadgeAndTheWordsOfTheCardPublishedVersionForEachStatus(t *testing.T) {
	cases := []struct {
		state   devdata.State
		subject string
		status  string
	}{
		{devdata.StateOK, testkit.BaseProfile, docs.VerifyUnchanged},
		{devdata.StateReadError, testkit.BaseProfile, docs.VerifyUnchanged},
		{devdata.StateOK, testkit.DexProfile, docs.VerifyDrifted},
		{devdata.StateOK, testkit.GazooProfile, docs.VerifyUnverified},
		{devdata.StateReadError, testkit.DexProfile, docs.VerifyError},
	}
	section := sectionOf(t, servicePage, publishedSection)
	rows := tableRows(t, section)
	if len(rows) != len(cases) {
		t.Fatalf("the table of the card has %d rows, want %d", len(rows), len(cases))
	}
	quoted := regexp.MustCompile(`"([^"]+)"`)
	for i, c := range cases {
		where := string(c.state) + " " + c.subject
		if row := rows[i]; !strings.HasPrefix(row[0], "`"+c.status+"`") {
			t.Errorf("row %d of the table names %q, want the status %s", i+1, row[0], c.status)
		}
		var status string
		for _, p := range loadSet(t, c.state).Profiles.Profiles {
			if model.Subject(p.Name, p.Version) == c.subject && p.Published != nil {
				status = p.Published.Status
			}
		}
		if status != c.status {
			t.Errorf("%s: the published version is %q, want %q", where, status, c.status)
		}
		card := publishedOf(t, c.state, c.subject)
		for _, cell := range rows[i][1:] {
			for _, m := range quoted.FindAllStringSubmatch(cell, -1) {
				mustHold(t, where, card, m[1])
			}
		}
	}
	const noRecord, nothing = "No record", "Nothing to compare with."
	mustHold(t, "the book", unwrapped(section), `the badge "`+noRecord+`" and the words "`+nothing+`"`)
	mustHold(t, "the scripts", readScripts(t), `Published version ${badge("y", "`+noRecord+`")}</div><div class="small mut mt6">`+nothing)
}
