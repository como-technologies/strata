package devdata

import (
	"encoding/json"
	"net/http"
	"path"
	"slices"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

func loadSet(t *testing.T, state State) *SetData {
	t.Helper()
	s, err := LoadSet(sets(), state, time.Time{})
	if err != nil {
		t.Fatal(err)
	}
	return s
}

// Each file of a set of clusters decodes into the type of its route, and the routes of the set serve them. Each row of
// the clusters has the set of its cluster under its uid, with the same uid and name. The counts are the counts of the
// rows.
func TestEachSetOfClustersMatchesTheTypesOfItsRoutesAndEachRowHasItsCluster(t *testing.T) {
	for _, state := range SetStates {
		s := loadSet(t, state)
		h := (&api.Server{Set: s}).Handler()
		for route, file := range map[string]string{api.PatternClusters: fileClusters, api.PatternShared: fileShared} {
			raw := testkit.ReadFile(t, path.Join("../testdata/api", string(state), file))
			rec := testkit.Get(h, route)
			if rec.Code != http.StatusOK {
				t.Errorf("%s: %s gave %d", state, route, rec.Code)
			}
			testkit.SamePlain(t, string(state)+": "+route, json.RawMessage(rec.Body.Bytes()), json.RawMessage(raw))
		}
		rows := s.Summary.Clusters
		var counts model.SetCounts
		for _, r := range rows {
			m, ok := s.Members[r.UID]
			if !ok || m.Cluster.Cluster.UID != r.UID || m.Cluster.Cluster.Name != r.Name {
				t.Errorf("%s: the row %+v has no set of its cluster", state, r)
				continue
			}
			if rec := testkit.Get(h, "/v1/clusters/"+r.UID+"/cluster"); rec.Code != http.StatusOK {
				t.Errorf("%s: the route of the cluster %s gave %d", state, r.UID, rec.Code)
			}
			counts.Clusters++
			counts.Findings += r.Drift
			switch r.Status {
			case model.RowDrift, model.RowDiffers:
				counts.Drift++
			case model.RowNotChecked:
				counts.NotChecked++
			}
			if r.Newer > 0 {
				counts.Newer++
			}
		}
		if counts != s.Summary.Counts || len(s.Members) != len(rows) {
			t.Errorf("%s: counts %+v, the rows give %+v, with %d sets of clusters", state, s.Summary.Counts, counts, len(s.Members))
		}
		for _, sh := range s.SharedList.Shared {
			for _, c := range sh.Copies.Differ {
				if !slices.ContainsFunc(rows, func(r model.ClusterRow) bool { return r.UID == c.UID }) {
					t.Errorf("%s: %s differs on %s, which is no cluster of the set", state, sh.UID, c.UID)
				}
			}
		}
	}
}

// The set fleet holds a cluster of each state: a drift, a cluster that is OK, and a cluster of a refused key. One
// selector selects no cluster.
func TestTheSetClustersHoldsEachStateOfACluster(t *testing.T) {
	s := loadSet(t, StateFleet)
	var statuses []model.ClusterStatus
	for _, r := range s.Summary.Clusters {
		statuses = append(statuses, r.Status)
	}
	if want := []model.ClusterStatus{model.RowDrift, model.RowNotChecked, model.RowOK}; !slices.Equal(statuses, want) {
		t.Errorf("the rows are %v, want %v, the worst first", statuses, want)
	}
	if !slices.ContainsFunc(s.Summary.Selectors, func(sel model.Selector) bool { return sel.Clusters == 0 && !sel.Unread }) {
		t.Errorf("no selector selects no cluster: %+v", s.Summary.Selectors)
	}
}
