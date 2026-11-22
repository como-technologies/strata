package web

import (
	"maps"
	"os"
	"path/filepath"
	"regexp"
	"slices"
	"strings"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/testkit"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

/* The tests below hold the page of a set of clusters, with the set fleet of testdata/api. With no --clusters, the page
is the page of one cluster, and the tests of the other files hold it. */

// The clusters of the set fleet: quarry-1 runs the observer and has a drift, and ledger is OK. The key of the project
// of search is refused.
const (
	quarryHash = "#c-" + testkit.ClusterUID
	ledgerHash = "#c-c-ledger"
	// wilmaShared is the profile version whose copy on ledger differs from its record, by its uid.
	wilmaShared = "u-wilma-gateway-312"
)

// linkValues finds the target of each link and each row of a screen.
var linkValues = regexp.MustCompile(`(?:href|data-go)="(#[^"]*)"`)

// screenOf finds the screen of a hash, with no variant.
func screenOf(t *testing.T, screens []screen, hash string) string {
	t.Helper()
	i := slices.IndexFunc(screens, func(s screen) bool { return s.Hash == hash && s.Variant == "" })
	if i < 0 {
		t.Fatalf("the crawl did not reach %s", hash)
	}
	return screens[i].HTML
}

func TestEachScreenOfASetRendersAndEachLinkOfAClusterStaysInItsCluster(t *testing.T) {
	screens := crawl(t, devdata.StateFleet)
	hashes := map[string]bool{}
	for _, s := range screens {
		checkMarkup(t, s)
		hashes[s.Hash] = true
		if !strings.HasPrefix(s.Hash, "#c-") {
			continue
		}
		prefix := s.Hash[:strings.Index(s.Hash+"/", "/")+1]
		for _, m := range linkValues.FindAllStringSubmatch(s.HTML, -1) {
			if m[1] != "#clusters" && !strings.HasPrefix(m[1], "#c-") {
				t.Errorf("%s %s: the link %s leaves the clusters of the set", s.Hash, s.Variant, m[1])
			}
			if strings.HasPrefix(m[1], prefix) || m[1] == "#clusters" || strings.HasSuffix(m[1], "/overview") {
				continue
			}
			t.Errorf("%s %s: the link %s goes to another cluster", s.Hash, s.Variant, m[1])
		}
	}
	for _, want := range []string{"#clusters", "#shared", "#alerts", quarryHash + "/overview", ledgerHash + "/overview", ledgerHash + "/p-" + wilmaShared} {
		if !hashes[want] {
			t.Errorf("the crawl did not reach %s", want)
		}
	}
}

func TestTheScreenOfTheClustersShowsTheWorstFirstARefusedKeyAndASelectorThatSelectsNoCluster(t *testing.T) {
	html := screenOf(t, crawl(t, devdata.StateFleet), "#clusters")
	mustHold(t, "the clusters", html, "Clusters", "Drift", newerVersions, wordNotChecked, "Drift 1", "runs the observer",
		"HTTP 403, the API key was refused", "No cluster matches", "tag:env=nowhere", "Check the label on the clusters, or the saved filter in Palette.")
	mustHoldInOrder(t, "the rows", html, ">quarry-1<", ">search<", ">ledger<")
	// a cluster that is not checked, and a selector with no cluster, keep the calm away
	mustNotHold(t, "the clusters", html, calmChanged, calmQuiet)
}

func TestAScreenOfAClusterOfASetNamesTheSetAndItsImagesSayThatTheObserverReadsItsOwnClusterOnly(t *testing.T) {
	pages := shown(t, devdata.StateFleet, ledgerHash+"/overview", ledgerHash+"/images", quarryHash+"/images")
	mustHold(t, "the overview of ledger", pages[0], `<a href="#clusters">Clusters</a>`, `<a href="`+ledgerHash+`/overview">ledger</a>`, "Own cluster only")
	mustHold(t, "the images of ledger", pages[1], "Pods of another cluster", "The observer reads the pods of its own cluster only.")
	mustNotHold(t, "the images of quarry-1", pages[2], "Own cluster only", "Pods of another cluster")
}

func TestTheScreenOfTheSharedVersionsCountsTheClustersAndLinksEachCopyThatDiffers(t *testing.T) {
	html := screenOf(t, crawl(t, devdata.StateFleet), "#shared")
	mustHold(t, "the shared versions", html, "2 clusters in 1 project", "1 of 2 same", `href="`+ledgerHash+`/p-`+wilmaShared+`"`, "No record", "1 not read")
}

// checkSet names a directory with the answers of /v1/clusters, /v1/shared, and /v1/alerts of a real server, and an empty
// folder members. scripts/observer-contract.sh serves the set of the chapter observer-clusters, and sets it. Without it, the
// test skips.
const checkSet = "STRATA_CHECK_SET"

// The set of the chapter observer-clusters holds platform-addons@1.0.0 in dev and in prod, by two uids. The test draws the
// screen of the shared versions from the answers of a real server.
func TestEachRowOfProjectScopeOfTheSharedVersionsOfTheStandInNamesItsProject(t *testing.T) {
	dir := os.Getenv(checkSet)
	if dir == "" {
		t.Skip(checkSet + " is not set: " + testkit.RunContract)
	}
	sets, state := filepath.Dir(dir), devdata.State(filepath.Base(dir))
	set, err := devdata.LoadSet(os.DirFS(sets), state, time.Time{})
	if err != nil {
		t.Fatal(err)
	}
	names := map[string]string{}
	for _, p := range set.Summary.Projects {
		names[p.UID] = p.Name
	}
	html := showIn(t, sets, state, "#shared")[0]
	uids := map[string]int{}
	for _, s := range set.SharedList.Shared {
		if s.Scope != model.ScopeProject {
			continue
		}
		var projects []string
		for _, uid := range s.Projects {
			projects = append(projects, names[uid])
		}
		mustHold(t, "the shared versions", html, ref(s.Name, s.Version)+"</td>"+narrowCell+s.Scope+" · "+strings.Join(projects, ", ")+"</td>")
		uids[model.Subject(s.Name, s.Version)]++
	}
	if !slices.ContainsFunc(slices.Collect(maps.Values(uids)), func(n int) bool { return n > 1 }) {
		t.Errorf("no profile version of project scope has two uids: %v", uids)
	}
}

func TestTheChoiceOfAProjectFiltersTheClustersAndTheSharedVersions(t *testing.T) {
	clusters := pageWith(t, "testdata/api", devdata.StateFleet, "#clusters", `S.project = "p-prod"`)
	mustHold(t, "the clusters of prod", clusters, ">search<", `<option value="p-prod" selected>prod</option>`)
	mustNotHold(t, "the clusters of prod", clusters, ">ledger<", ">quarry-1<")
	shared := pageWith(t, "testdata/api", devdata.StateFleet, "#shared", `S.project = "p-prod"`)
	mustHold(t, "the shared versions of prod", shared, "edge-agent")
	mustNotHold(t, "the shared versions of prod", shared, "wilma-gateway")
}

// A set serves no route of one cluster at the top. So the first read of the page gets 404, and the page reads the set. Each
// later read of a cluster names its uid.
func TestThePageOfASetReadsTheSetAfterThe404OfTheRouteOfOneCluster(t *testing.T) {
	var paths []string
	for _, r := range requestsOf(t, devdata.StateFleet) {
		paths = append(paths, r.Path)
	}
	first := slices.Index(paths, api.PatternClusters)
	if first < 0 || slices.Index(paths, "/v1/cluster") > first || slices.Contains(paths[first:], "/v1/cluster") {
		t.Errorf("the requests %q", paths)
	}
	if !slices.Contains(paths, "/v1/clusters/c-ledger/profiles") {
		t.Errorf("the page did not read a cluster under its uid: %q", paths)
	}
}

func TestAValueWithMarkupShowsAsTextOnEachScreenOfASet(t *testing.T) {
	screens := render(t, devdata.StateFleet, true)
	var all strings.Builder
	for _, s := range screens {
		checkMarkup(t, s)
		all.WriteString(s.HTML)
	}
	mustNotHold(t, "the screens", all.String(), "<img")
	mustHold(t, "the screens", all.String(), injection)
}

// With no --clusters, the hashes of the page are the hashes of one cluster. No screen names a cluster, and the page asks no
// route of a set.
func TestWithNoClustersTheHashesOfThePageAreTheHashesOfOneCluster(t *testing.T) {
	hashes := map[string]bool{}
	for _, s := range crawl(t, devdata.StateOK) {
		hashes[s.Hash] = true
		if strings.HasPrefix(s.Hash, "#c-") || strings.Contains(s.HTML, `href="#c-`) || strings.Contains(s.HTML, `href="#clusters"`) {
			t.Errorf("%s %s names a cluster of a set", s.Hash, s.Variant)
		}
	}
	for _, want := range []string{"#overview", "#profiles", "#p-" + testkit.DexProfile, packPage(testkit.DexProfile, "addon", "dex"), "#images"} {
		if !hashes[want] {
			t.Errorf("the crawl of one cluster did not reach %s", want)
		}
	}
	for _, r := range requestsOf(t, devdata.StateOK) {
		if strings.HasPrefix(r.Path, api.PatternClusters) || strings.HasPrefix(r.Path, api.PatternShared) {
			t.Errorf("the page of one cluster asked %s", r.Path)
		}
	}
}
