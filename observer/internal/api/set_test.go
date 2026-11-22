package api

import (
	"net/http"
	"slices"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/alert"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// setStub is a Set whose clusters are the sources of members, by uid.
type setStub struct {
	members map[string]Source
	sum     model.SetSummary
	shared  []model.Shared
}

func (s setStub) Clusters() model.SetSummary { return s.sum }
func (s setStub) Shared() []model.Shared     { return s.shared }
func (s setStub) Member(uid string) (Source, bool) {
	src, ok := s.members[uid]
	return src, ok
}
func (s setStub) Alerts() alert.View { return alert.Unset() }
func (s setStub) StartCycle(string) (model.Start, bool) {
	return model.Start{Cycle: "content", State: model.StartStarted}, true
}

// setRoutes are the routes of the set itself. Each other route of a set is a route of one cluster under its uid.
var setRoutes = []string{patternHealth, PatternClusters, PatternShared, patternAlerts, patternCycle}

// memberPath gives the path of a route of one cluster of a set.
func memberPath(uid, path string) string {
	return PatternClusters + "/" + uid + strings.TrimPrefix(path, routePrefix)
}

func TestEachRouteOfASetIsARouteOfTheSetOrARouteOfOneClusterUnderItsUid(t *testing.T) {
	var one []string
	for _, e := range clusterTable() {
		one = append(one, e.Pattern)
	}
	var members int
	for _, r := range (&Server{Set: setStub{}}).Routes() {
		rest, member := strings.CutPrefix(r.Pattern, patternMember)
		switch {
		case member && slices.Contains(one, routePrefix+rest):
			members++
		case !member && slices.Contains(setRoutes, r.Pattern):
		default:
			t.Errorf("the route %s of a set is no route of the set and no route of one cluster", r.Pattern)
		}
	}
	if members != len(one) {
		t.Errorf("a set has %d routes of one cluster, want %d", members, len(one))
	}
}

func TestEachSummaryRouteOfAClusterOfASetHoldsNoSentinelAndAnUnknownClusterGets404(t *testing.T) {
	_, o, _ := asked(t, "")
	sum := model.SetSummary{Page: model.PageOK, Clusters: []model.ClusterRow{{UID: testkit.PaymentsUID, Name: testkit.PaymentsName}}}
	h := (&Server{Set: setStub{members: map[string]Source{testkit.PaymentsUID: o}, sum: sum}}).Handler()
	for pattern, path := range summaryRoutes {
		rec := get(h, memberPath(testkit.PaymentsUID, path))
		if rec.Code != http.StatusOK {
			t.Errorf("%s: status %d", pattern, rec.Code)
		}
		testkit.HoldsNone(t, pattern, rec.Body.String(), "SENTINEL")
		if rec := get(h, memberPath("nobody", path)); rec.Code != http.StatusNotFound {
			t.Errorf("%s of a cluster that is not in the set: status %d", pattern, rec.Code)
		}
	}
	var got model.SetSummary
	decode(t, get(h, PatternClusters), &got)
	if len(got.Clusters) != 1 || got.Clusters[0].UID != testkit.PaymentsUID {
		t.Errorf("the clusters %+v", got)
	}
	var shared SharedView
	decode(t, get(h, PatternShared), &shared)
	if shared.Shared == nil {
		t.Error("the list of the shared versions is null, and not empty")
	}
	// a set serves no route of one cluster at the top, because the page names each cluster
	if rec := get(h, patternCluster); rec.Code != http.StatusNotFound {
		t.Errorf("%s of a set: status %d", patternCluster, rec.Code)
	}
	for _, path := range detailRoutes[patternPackDiff] {
		if rec := get(h, memberPath(testkit.PaymentsUID, path)); rec.Code != http.StatusOK || !strings.Contains(rec.Body.String(), sentinelValue) {
			t.Errorf("%s: status %d", path, rec.Code)
		}
	}
}

func TestAPostToAClusterOfASetNeedsTheHeaderOfARequestFromThePage(t *testing.T) {
	h := (&Server{Set: setStub{members: map[string]Source{testkit.PaymentsUID: stub{}}}}).Handler()
	path := memberPath(testkit.PaymentsUID, pathCycle)
	if rec := testkit.Serve(h, http.MethodPost, path, testkit.LoopbackHost, ""); rec.Code != http.StatusForbidden {
		t.Errorf("a POST with no header: status %d", rec.Code)
	}
	if rec := testkit.Serve(h, http.MethodPost, pathCycle, testkit.LoopbackHost, testkit.SameOrigin); rec.Code != http.StatusAccepted {
		t.Errorf("the click Check now of the set: status %d", rec.Code)
	}
}
