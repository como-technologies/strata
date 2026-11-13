package web

import (
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

// Issue #834. The set ok holds the record of wilma-gateway@3.0.4, a version that the cluster no longer runs: the cluster
// moved to 3.1.2. The filters All and Newer do not count it, and the table of the profiles does not list it. A fold under
// the table lists it, with the status "Not on this cluster" and no newer version. The fold shows with the filter All only.
// The tile Profiles counts the profiles of the cluster, and the page of the version names no move.
func TestAVersionThatTheClusterNoLongerRunsIsInAFoldWithAStatusOfItsOwn(t *testing.T) {
	const subject = "wilma-gateway@3.0.4"
	screens := render(t, devdata.StateOK, false)
	profiles := pageOf(t, screens, "#profiles", "")
	mustHold(t, "the filters", profiles, `<button data-filter="all" class="on">All 12</button>`, `<button data-filter="newer" class="">Newer 6</button>`)
	table, fold, found := strings.Cut(profiles, `<details data-open="not-on-cluster"`)
	if !found {
		t.Fatal("the screen has no fold of the versions that the cluster does not run")
	}
	mustNotHold(t, "the table", table, `data-go="#p-`+subject+`"`)
	mustHold(t, "the fold", fold, "<summary>1 record of a profile version that this cluster does not run</summary>")
	row := rowWith(t, fold, `data-go="#p-`+subject+`"`)
	mustHold(t, "the row", row, `<span class="badge y">Not on this cluster</span>`, `<span class="mut">This cluster does not run this version.</span>`)
	mustNotHold(t, "the row", row, "Newer", "newer")
	for _, variant := range []string{"filter newer", "filter ok", "filter drift", "filter not-checked"} {
		mustNotHold(t, "the profiles with the "+variant, pageOf(t, screens, "#profiles", variant), subject, `data-open="not-on-cluster"`)
	}
	overview := pageOf(t, screens, "#overview", "")
	mustHold(t, "the overview", overview, `<div class="l">Profiles</div><div class="v">12</div>`)
	page := pageOf(t, screens, "#p-"+subject, "")
	mustHold(t, "the page of the version", page, `<span class="badge y">Not on this cluster</span>`, "This cluster does not run this version.")
	mustNotHold(t, "the page of the version", page, "Move to", "Newer profile version")
}

// The section "A version that the cluster no longer runs" of the page of the service quotes the words of the page for such a
// version, and the screens of the set ok show each of them.
func TestTheBookQuotesTheWordsOfAVersionThatTheClusterNoLongerRuns(t *testing.T) {
	section := strings.Join(strings.Fields(sectionOf(t, servicePage, "### A version that the cluster no longer runs")), " ")
	screens := render(t, devdata.StateOK, false)
	shown := pageOf(t, screens, "#profiles", "") + pageOf(t, screens, "#p-wilma-gateway@3.0.4", "")
	for _, words := range []string{"1 record of a profile version that this cluster does not run", "Not on this cluster", "This cluster does not run this version."} {
		mustHold(t, "the book", section, `"`+words+`"`)
		mustHold(t, "the page", shown, words)
	}
}
