package api

import (
	"slices"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/alert"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// oneCluster is the file of the routes, the ids of the findings, and the ids of the subjects of the alerts of the
// observer of one cluster, as the observer of 0.35.0 gave them. With no --clusters, the code of a set of clusters
// changes none of them.
const oneCluster = "testdata/one-cluster.txt"

// oneClusterLines gives the lines of the file oneCluster: each route with its method and its tier, and then the id of
// each finding and of each subject of the alerts after one content cycle and one versions cycle of the rig.
func oneClusterLines(t *testing.T) []string {
	t.Helper()
	r := newRig(t)
	o := r.cycled(&fakeStrata{dir: r.fixtures})
	var lines []string
	for _, route := range (&Server{Source: o}).Routes() {
		lines = append(lines, "route "+route.Method+" "+route.Pattern+" "+string(route.Tier))
	}
	sum := o.Summary()
	for _, f := range sum.Findings {
		lines = append(lines, "finding "+string(f.Kind)+" "+f.Profile+" "+f.ID)
	}
	for _, s := range alert.SubjectsOf(sum) {
		lines = append(lines, "alert "+s.ID)
	}
	return lines
}

func TestWithNoClustersTheRoutesTheFindingIdsAndTheAlertIdsAreTheOnesOfOneCluster(t *testing.T) {
	got := oneClusterLines(t)
	want := strings.Split(strings.TrimSpace(string(testkit.ReadFile(t, oneCluster))), "\n")
	if !slices.Equal(got, want) {
		t.Errorf("with no --clusters, the observer gives\n%s\nwant\n%s", strings.Join(got, "\n"), strings.Join(want, "\n"))
	}
}
