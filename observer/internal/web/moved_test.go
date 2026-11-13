package web

import (
	"fmt"
	"os"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

// moved is the record of the set ok of a version that the cluster no longer runs: the cluster moved to wilma-gateway@3.1.2.
const moved = "wilma-gateway@3.0.4"

// Issue #834. The filters All and Newer do not count the record, and the table of the profiles does not list it. A fold under the
// table lists it, with the status "Not on this cluster" and no newer version. The fold shows with the filter All only. The tile
// Profiles counts the profiles of the cluster. The page of the version names no move, and no pack of it says "current": strata
// does not look for a newer version of a version that the cluster does not run.
func TestAVersionThatTheClusterNoLongerRunsIsInAFoldWithAStatusOfItsOwn(t *testing.T) {
	screens := crawl(t, devdata.StateOK)
	profiles := pageOf(t, screens, "#profiles", "")
	mustHold(t, "the filters", profiles, `<button data-filter="all" class="on">All 12</button>`, `<button data-filter="newer" class="">Newer 6</button>`)
	table, fold, found := strings.Cut(profiles, `<details data-open="not-on-cluster"`)
	if !found {
		t.Fatal("the screen has no fold of the versions that the cluster does not run")
	}
	mustNotHold(t, "the table", table, `data-go="#p-`+moved+`"`)
	mustHold(t, "the fold", fold, "<summary>1 record of a profile version that this cluster does not run</summary>")
	row := rowWith(t, fold, `data-go="#p-`+moved+`"`)
	mustHold(t, "the row", row, `<span class="badge y">Not on this cluster</span>`, `<span class="mut">This cluster does not run this version.</span>`)
	mustNotHold(t, "the row", row, "Newer", "newer")
	for _, variant := range []string{"filter newer", "filter ok", "filter drift", "filter not-checked"} {
		mustNotHold(t, "the profiles with the "+variant, pageOf(t, screens, "#profiles", variant), moved, `data-open="not-on-cluster"`)
	}
	overview := pageOf(t, screens, "#overview", "")
	mustHold(t, "the overview", overview, `<div class="l">Profiles</div><div class="v">12</div>`)
	page := pageOf(t, screens, "#p-"+moved, "")
	mustHold(t, "the page of the version", page, `<span class="badge y">Not on this cluster</span>`, "This cluster does not run this version.")
	mustNotHold(t, "the page of the version", page, "Move to", "Newer profile version", "current")
	if n := strings.Count(page, `<span class="sub">1 image listed</span>`); n != 3 {
		t.Errorf("%d of the 3 packs say only what images they list", n)
	}
}

// The page of each pack of the version shows the facts of the published version and of the copy of this cluster, and no fact of a
// newer version. Strata does not look for a newer version of a version that the cluster does not run, so no side of the pack says
// "Current" or "No newer version".
func TestThePageOfAPackOfAVersionThatTheClusterNoLongerRunsHasNoFactOfANewerVersion(t *testing.T) {
	screens := crawl(t, devdata.StateOK)
	for _, name := range []string{"traefik", "aws-application-loadbalancer", "vendor-product"} {
		hash := packPage(moved, "addon", name)
		mustHold(t, "the page of "+name, pageOf(t, screens, hash, ""), `<b class="ok">Same</b><span>Published now</span>`, `<b class="">Not here</b><span>This cluster&#39;s copy</span>`)
		for _, s := range screens {
			if s.Hash == hash {
				mustNotHold(t, fmt.Sprintf("the page of %s %q", name, s.Variant), s.HTML, "Current", "No newer version", "Newer version")
			}
		}
	}
}

// The tile "Versions newer than the pins" counts the other packs that are current. A pack of a version that the cluster does not
// run is not one of them.
func TestTheTilesOfTheNewerVersionsCountNoPackOfAVersionThatTheClusterDoesNotRun(t *testing.T) {
	set, err := devdata.Load(os.DirFS("testdata/api"), devdata.StateOK, devdata.Reference)
	if err != nil {
		t.Fatal(err)
	}
	current := 0
	for _, p := range set.Profiles.Profiles {
		for _, k := range p.Packs {
			if p.Status != model.StatusNotOnThisCluster && k.Newer == "" && k.Status != model.StatusNotChecked {
				current++
			}
		}
	}
	newer := pageOf(t, crawl(t, devdata.StateOK), "#newer", "")
	mustHold(t, "the tiles of the newer versions", newer, fmt.Sprintf("%d other packs are current", current))
}

// The section "A version that the cluster no longer runs" of the page of the service quotes the words of the page for such a
// version, and the screens of the set ok show each of them.
func TestTheBookQuotesTheWordsOfAVersionThatTheClusterNoLongerRuns(t *testing.T) {
	section := strings.Join(strings.Fields(sectionOf(t, servicePage, "### A version that the cluster no longer runs")), " ")
	screens := crawl(t, devdata.StateOK)
	shown := pageOf(t, screens, "#profiles", "") + pageOf(t, screens, "#p-"+moved, "")
	for _, words := range []string{"1 record of a profile version that this cluster does not run", "Not on this cluster", "This cluster does not run this version."} {
		mustHold(t, "the book", section, `"`+words+`"`)
		mustHold(t, "the page", shown, words)
	}
}
