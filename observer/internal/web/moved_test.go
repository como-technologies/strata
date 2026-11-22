package web

import (
	"fmt"
	"regexp"
	"slices"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/testkit"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

// The words of a version that the cluster no longer runs, and the fold of the table of the profiles that lists it. The version is
// testkit.GatewayMoved: the cluster moved to wilma-gateway@3.1.2. The set ok holds it with no finding, and the set moved-drift holds
// it with a finding. The set moved-unchecked holds it with a cause of Not checked. The overview of the two sets of a move counts
// the 3 packs of the record that the cluster runs, and its newer version. movedCause is the line of the tile Not checked of the set
// moved-unchecked.
const (
	movedHash    = "#p-" + testkit.GatewayMoved
	notOnCluster = "Not on this cluster"
	notRunFact   = "This cluster does not run this version."
	movedFold    = `data-open="not-on-cluster"`
	foldWords    = "1 record of a profile version that this cluster does not run"
	movedInFull  = "3 of 3 packs in full"
	movedNewer   = "1 profile · 1 this week"
	movedCause   = "1 cause · " + movedInFull
)

// filterButton gives the button of the filter of the table of the profiles, with its words. The filter is on when on is
// true.
func filterButton(filter, words string, on bool) string {
	return filterButtonClass(filter, words, onClass(on))
}

// filterButtonClass gives the button of the filter of the table of the profiles, with its words and its class.
// filterOpen starts the button, before the filter.
func filterButtonClass(filter, words, class string) string {
	return filterOpen + filter + `" class="` + class + `">` + words + `</button>`
}

const filterOpen = `<button data-filter="`

// filterVariant names the variant of the crawl of the table of the profiles with the filter of the status.
func filterVariant(status model.Status) string { return "filter " + string(status) }

// versionPageOfMoved checks the page of the version moved. It must hold the badge of the status and the fact of a version
// that the cluster no longer runs. It names no move and no newer version. Each of its 3 packs says only what images it lists.
func versionPageOfMoved(t *testing.T, screens []screen, badgeOfStatus string) {
	t.Helper()
	const where = "the page of the version"
	page := pageOf(t, screens, movedHash, "")
	mustHold(t, where, page, badgeOfStatus, notRunFact)
	mustNotHold(t, where, page, "Move to", newerProfile, "current")
	if n := strings.Count(page, `<span class="sub">1 image listed</span>`); n != 3 {
		t.Errorf("%d of the 3 packs say only what images they list", n)
	}
}

// Issue #834. The filters All and Newer do not count the record, and the table of the profiles does not list it. A fold under the
// table lists it, with the status "Not on this cluster" and no newer version. The fold shows with the filter All only. The tile
// Profiles counts the profiles of the cluster.
//
// Strata does not look for a newer version of a version that the cluster does not run. So the page of the version names no
// move, and no pack of it says "current".
func TestAVersionThatTheClusterNoLongerRunsIsInAFoldWithAStatusOfItsOwn(t *testing.T) {
	screens := crawl(t, devdata.StateOK)
	profiles := pageOf(t, screens, "#profiles", "")
	mustHold(t, "the filters", profiles, filterButton("all", "All 12", true), filterButton(string(model.StatusNewer), "Newer 6", false))
	table, fold, found := strings.Cut(profiles, `<details `+movedFold)
	if !found {
		t.Fatal("the screen has no fold of the versions that the cluster does not run")
	}
	mustNotHold(t, "the table", table, goTo(movedHash))
	mustHold(t, "the fold", fold, "<summary>"+foldWords+"</summary>")
	row := rowTo(t, fold, movedHash)
	mustHold(t, "the row", row, badge("y", notOnCluster), `<span class="mut">`+notRunFact+`</span>`)
	mustNotHold(t, "the row", row, "Newer", string(model.StatusNewer))
	for _, status := range []model.Status{model.StatusNewer, model.StatusOK, model.StatusDrift, model.StatusNotChecked} {
		variant := filterVariant(status)
		mustNotHold(t, "the profiles with the "+variant, pageOf(t, screens, "#profiles", variant), testkit.GatewayMoved, movedFold)
	}
	overview := pageOf(t, screens, "#overview", "")
	mustHold(t, "the overview", overview, tileLabel("Profiles")+tileValue("12"))
	versionPageOfMoved(t, screens, badge("y", notOnCluster))
}

// Issue #834. A finding wins: in the set moved-drift, a person edited the published version that the cluster no longer runs. Its row
// stays in the table of the profiles with the badge "Drift", the filters count it, and no fold shows. The page of the version names no
// move, and no pack of it says "current". The page reads the copy of this cluster for that rule, as the summary does, and not the
// status. So the rule holds for a drift too.
func TestAVersionThatTheClusterNoLongerRunsWithAFindingIsADriftWithNoNewerVersion(t *testing.T) {
	screens := crawl(t, devdata.StateMovedDrift)
	profiles := pageOf(t, screens, "#profiles", "")
	mustHold(t, "the filters", profiles, filterButton("all", "All 2", true), filterButton(string(model.StatusDrift), "Drift 1", false), filterButton(string(model.StatusNewer), "Newer 1", false))
	mustNotHold(t, "the profiles", profiles, movedFold)
	mustHold(t, "the row", rowTo(t, profiles, movedHash), badge("r", "Drift"), "traefik changed in the published version.")
	overview := pageOf(t, screens, "#overview", "")
	mustHold(t, "the overview", overview, tileLabel("Profiles")+tileValue("2"))
	versionPageOfMoved(t, screens, badge("r", "Drift"))
}

// Issue #895. In the set moved-unchecked, a cause of Not checked names the record of a version that the cluster no longer runs. The
// tile, the screen, and the filter Not checked count the cause. The filter shows the row of the record in the table, with its badge
// "Not on this cluster" and no fold. The filter All lists the record in the fold only, and the filters Drift, Newer, and OK do not
// show it.
func TestACauseOfAVersionThatTheClusterNoLongerRunsCountsOnTheTileTheScreenAndTheFilter(t *testing.T) {
	screens := crawl(t, devdata.StateMovedUnchecked)
	notChecked := string(model.StatusNotChecked)
	mustHold(t, "the overview", pageOf(t, screens, "#overview", ""), tileLabel(wordNotChecked)+tileValue("1"), movedCause)
	mustHold(t, "the screen", rowTo(t, pageOf(t, screens, "#notchecked", ""), movedHash), "traefik@37.4.0", testkit.GatewayMoved)
	profiles := pageOf(t, screens, "#profiles", "")
	mustHold(t, "the filters", profiles, filterButton("all", "All 1", true), filterButton(notChecked, wordNotChecked+" 1", false))
	table, fold, found := strings.Cut(profiles, `<details `+movedFold)
	if !found {
		t.Fatal("the screen has no fold of the versions that the cluster does not run")
	}
	mustNotHold(t, "the table", table, goTo(movedHash))
	mustHold(t, "the fold", fold, "<summary>"+foldWords+"</summary>")
	filtered := pageOf(t, screens, "#profiles", filterVariant(model.StatusNotChecked))
	mustHold(t, "the row", rowTo(t, filtered, movedHash), badge("y", notOnCluster), `<span class="mut">`+notRunFact+`</span>`)
	mustNotHold(t, "the filter "+wordNotChecked, filtered, movedFold, goTo(wilmaHash))
	for _, status := range []model.Status{model.StatusDrift, model.StatusNewer, model.StatusOK} {
		variant := filterVariant(status)
		mustNotHold(t, "the profiles with the "+variant, pageOf(t, screens, "#profiles", variant), goTo(movedHash), movedFold)
	}
}

// profileRow finds the hash of each row of the table of the profiles.
var profileRow = regexp.MustCompile(linkRow(`#p-([^"]*)`))

// Issue #895. In each set with a table of the profiles, the filter Not checked shows each profile that a row of Not checked names. It
// shows no other profile. The count of the filter is the number of those profiles. So the filter counts the causes that the tile and
// the screen count. In the set moved-unchecked, a cause names a record of a version that the cluster no longer runs. In the set
// read-error, a cause pin-error names a profile with the status newer. A page with no record file shows no table.
func TestTheFilterNotCheckedShowsEachProfileThatACauseNames(t *testing.T) {
	notChecked := string(model.StatusNotChecked)
	for _, state := range devdata.States {
		set := loadSet(t, state)
		if len(set.Profiles.Profiles) == 0 || set.Cluster.Page == model.PageNoRecords {
			continue
		}
		var named []string
		for _, r := range set.NotChecked.NotChecked {
			for _, subject := range r.Profiles {
				named = append(named, encodeURIComponent(subject))
			}
		}
		slices.Sort(named)
		named = slices.Compact(named)
		page := pageOf(t, crawl(t, state), "#profiles", filterVariant(model.StatusNotChecked))
		mustHold(t, string(state), page, filterButton(notChecked, fmt.Sprintf("%s %d", wordNotChecked, len(named)), true))
		var shown []string
		for _, m := range profileRow.FindAllStringSubmatch(page, -1) {
			shown = append(shown, m[1])
		}
		slices.Sort(shown)
		if !slices.Equal(shown, named) {
			t.Errorf("%s: the filter %s shows %v, and the causes name %v", state, wordNotChecked, shown, named)
		}
	}
}

// The page of each pack of the version shows no fact of a newer version. Strata does not look for a newer version of a version that the
// cluster does not run. So no side of the pack says "Current" or "No newer version". The nav of each screen holds "Newer versions",
// so the test looks for the label of the fact. A pack with no change shows the facts of the published version and of the copy of this
// cluster. In the set moved-drift, the published version of traefik changed, so its page shows the change.
func TestThePageOfAPackOfAVersionThatTheClusterNoLongerRunsHasNoFactOfANewerVersion(t *testing.T) {
	for _, c := range []struct {
		state   devdata.State
		changed string
	}{{devdata.StateOK, ""}, {devdata.StateMovedDrift, "traefik"}} {
		screens := crawl(t, c.state)
		for _, name := range []string{"traefik", albPack, "vendor-product"} {
			hash, where := packPage(testkit.GatewayMoved, "addon", name), fmt.Sprintf("%s: the page of %s", c.state, name)
			if name == c.changed {
				mustHold(t, where, pageOf(t, screens, hash, ""), badge("r", "Changed"), "charts.traefik.deployment.replicas")
			} else {
				mustHold(t, where, pageOf(t, screens, hash, ""), samePublished, `<b class="">Not here</b>`+clusterCopy)
			}
			for _, s := range screens {
				if s.Hash == hash {
					mustNotHold(t, fmt.Sprintf("%s %q", where, s.Variant), s.HTML, "Current", "No newer version", newerSpan)
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

// The page of the checks has the section "A version that the cluster no longer runs". It quotes the words of the page for such
// a version. The screens of the sets ok and moved-drift show each of them.
func TestTheBookQuotesTheWordsOfAVersionThatTheClusterNoLongerRuns(t *testing.T) {
	section := unwrapped(sectionOf(t, checksPage, "### A version that the cluster no longer runs"))
	var shown string
	for _, state := range []devdata.State{devdata.StateOK, devdata.StateMovedDrift} {
		screens := crawl(t, state)
		shown += pageOf(t, screens, "#profiles", "") + pageOf(t, screens, movedHash, "")
	}
	for _, words := range []string{foldWords, notOnCluster, notRunFact, "Drift"} {
		mustHold(t, "the book", section, `"`+words+`"`)
		mustHold(t, "the page", shown, words)
	}
}

// Issue #895. The page of the service has the section "A version that the cluster no longer runs". It quotes the words of the page
// for a cause of Not checked that names such a version. They are the badge "Not on this cluster", and the filters "Not checked" and
// "All". With the filter Not checked, the set moved-unchecked shows the two filters and the badge. The set ok holds the same version with
// no cause, and its filter Not checked shows no such badge.
func TestTheBookOfTheServiceQuotesTheWordsOfACauseOfAVersionThatTheClusterNoLongerRuns(t *testing.T) {
	section := unwrapped(sectionOf(t, servicePage, "### A version that the cluster no longer runs"))
	for _, words := range []string{notOnCluster, wordNotChecked, "All"} {
		mustHold(t, "the book", section, `"`+words+`"`)
	}
	filtered := func(state devdata.State) string {
		return pageOf(t, crawl(t, state), "#profiles", filterVariant(model.StatusNotChecked))
	}
	mustHold(t, "the filter of the set moved-unchecked", filtered(devdata.StateMovedUnchecked), filterButton("all", "All 1", false),
		filterButton(string(model.StatusNotChecked), wordNotChecked+" 1", true), badge("y", notOnCluster))
	mustNotHold(t, "the filter of the set ok", filtered(devdata.StateOK), notOnCluster)
}
