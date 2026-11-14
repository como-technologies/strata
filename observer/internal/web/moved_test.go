package web

import (
	"fmt"
	"slices"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

// moved is the record of a version that the cluster no longer runs: the cluster moved to wilma-gateway@3.1.2. The set ok holds it
// with no finding, and the set moved-drift holds it with a finding.
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

// Issue #834. A finding wins: in the set moved-drift, a person edited the published version that the cluster no longer runs. Its row
// stays in the table of the profiles with the badge "Drift", the filters count it, and no fold shows. The page of the version names no
// move, and no pack of it says "current". The page reads the copy of this cluster for that rule, as the summary does, and not the
// status: so the rule holds for a drift too.
func TestAVersionThatTheClusterNoLongerRunsWithAFindingIsADriftWithNoNewerVersion(t *testing.T) {
	screens := crawl(t, devdata.StateMovedDrift)
	profiles := pageOf(t, screens, "#profiles", "")
	mustHold(t, "the filters", profiles, `<button data-filter="all" class="on">All 2</button>`, `<button data-filter="drift" class="">Drift 1</button>`,
		`<button data-filter="newer" class="">Newer 1</button>`)
	mustNotHold(t, "the profiles", profiles, `data-open="not-on-cluster"`)
	mustHold(t, "the row", rowWith(t, profiles, `data-go="#p-`+moved+`"`), `<span class="badge r">Drift</span>`, "traefik changed in the published version.")
	overview := pageOf(t, screens, "#overview", "")
	mustHold(t, "the overview", overview, `<div class="l">Profiles</div><div class="v">2</div>`)
	page := pageOf(t, screens, "#p-"+moved, "")
	mustHold(t, "the page of the version", page, `<span class="badge r">Drift</span>`, "This cluster does not run this version.")
	mustNotHold(t, "the page of the version", page, "Move to", "Newer profile version", "current")
	if n := strings.Count(page, `<span class="sub">1 image listed</span>`); n != 3 {
		t.Errorf("%d of the 3 packs say only what images they list", n)
	}
}

// The page of each pack of the version shows no fact of a newer version. Strata does not look for a newer version of a version that the
// cluster does not run, so no side of the pack says "Current" or "No newer version". The nav of each screen holds "Newer versions",
// so the test looks for the label of the fact. A pack with no change shows the facts of the published version and of the copy of this
// cluster. In the set moved-drift, the published version of traefik changed, so its page shows the change.
func TestThePageOfAPackOfAVersionThatTheClusterNoLongerRunsHasNoFactOfANewerVersion(t *testing.T) {
	for _, c := range []struct {
		state   devdata.State
		changed string
	}{{devdata.StateOK, ""}, {devdata.StateMovedDrift, "traefik"}} {
		screens := crawl(t, c.state)
		for _, name := range []string{"traefik", "aws-application-loadbalancer", "vendor-product"} {
			hash, where := packPage(moved, "addon", name), fmt.Sprintf("%s: the page of %s", c.state, name)
			if name == c.changed {
				mustHold(t, where, pageOf(t, screens, hash, ""), `<span class="badge r">Changed</span>`, "charts.traefik.deployment.replicas")
			} else {
				mustHold(t, where, pageOf(t, screens, hash, ""), `<b class="ok">Same</b><span>Published now</span>`, `<b class="">Not here</b><span>This cluster&#39;s copy</span>`)
			}
			for _, s := range screens {
				if s.Hash == hash {
					mustNotHold(t, fmt.Sprintf("%s %q", where, s.Variant), s.HTML, "Current", "No newer version", "<span>Newer version</span>")
				}
			}
		}
	}
}

// The tile "Versions newer than the pins" counts the other packs that are current. A pack of a version that the cluster does not
// run is not one of them, also when a finding names the version. The test reads that rule from the copy of this cluster, as the
// summary does.
func TestTheTilesOfTheNewerVersionsCountNoPackOfAVersionThatTheClusterDoesNotRun(t *testing.T) {
	for state, want := range map[devdata.State]int{devdata.StateOK: 16, devdata.StateMovedDrift: 2} {
		current := 0
		for _, p := range loadSet(t, state).Profiles.Profiles {
			if p.ClusterCopy != nil && p.ClusterCopy.Status == model.CopyNotOnThisCluster {
				continue
			}
			current += len(slices.DeleteFunc(slices.Clone(p.Packs), func(k model.Pack) bool { return k.Newer != "" || k.Status == model.StatusNotChecked }))
		}
		if current != want {
			t.Errorf("%s: %d packs are current, want %d", state, current, want)
		}
		newer := pageOf(t, crawl(t, state), "#newer", "")
		mustHold(t, fmt.Sprintf("%s: the tiles of the newer versions", state), newer, fmt.Sprintf("%d other packs are current", current))
	}
}

// The section "A version that the cluster no longer runs" of the page of the service quotes the words of the page for such a
// version, and the screens of the sets ok and moved-drift show each of them.
func TestTheBookQuotesTheWordsOfAVersionThatTheClusterNoLongerRuns(t *testing.T) {
	section := strings.Join(strings.Fields(sectionOf(t, servicePage, "### A version that the cluster no longer runs")), " ")
	var shown string
	for _, state := range []devdata.State{devdata.StateOK, devdata.StateMovedDrift} {
		screens := crawl(t, state)
		shown += pageOf(t, screens, "#profiles", "") + pageOf(t, screens, "#p-"+moved, "")
	}
	for _, words := range []string{"1 record of a profile version that this cluster does not run", "Not on this cluster", "This cluster does not run this version.", "Drift"} {
		mustHold(t, "the book", section, `"`+words+`"`)
		mustHold(t, "the page", shown, words)
	}
}
