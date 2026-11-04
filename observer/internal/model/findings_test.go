package model

import (
	"slices"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
)

func who() []docs.Who {
	return []docs.Who{{Time: "2026-11-03T13:00:00Z", Actor: "Betty Rubble", ActorType: "user", Action: "update"}}
}

func TestFindingsOfEachKind(t *testing.T) {
	aws := func() Record {
		return rec("aws-stack", "1.1.0", docs.VerifyUnchanged, pk("csi", "csi-aws-ebs", "1.65.0"))
	}
	cases := []struct {
		name    string
		record  Record
		kind    Kind
		packs   []string
		also    bool
		whoLen  int
		impact  string
		nothing bool
	}{
		{name: "no change", record: onCluster(aws(), docs.Diff{Same: true, SameContent: true}), nothing: true},
		{name: "published changed with who", record: drifted(aws(), &docs.Verify{Who: who()}, "csi/csi-aws-ebs"),
			kind: PublishedChanged, packs: []string{"csi/csi-aws-ebs"}, whoLen: 1, impact: "in-place"},
		{name: "published changed and the cluster has the same edit", record: onCluster(drifted(aws(), &docs.Verify{Who: who()}, "csi/csi-aws-ebs"),
			docs.Diff{RootB: "r2", Packs: []docs.Pack{changed("csi/csi-aws-ebs", "charts.a.b")}}),
			kind: PublishedChanged, packs: []string{"csi/csi-aws-ebs"}, also: true, whoLen: 1, impact: "in-place"},
		{name: "cluster differs only", record: onCluster(aws(), docs.Diff{RootB: "r3", Summary: docs.DiffSummary{MaxImpact: "cluster-wide"},
			Packs: []docs.Pack{{Key: "os/ubuntu", Status: docs.PackUnchanged}, changed("csi/csi-aws-ebs", "charts.a.b")}}),
			kind: ClusterDiffers, packs: []string{"csi/csi-aws-ebs"}, impact: "cluster-wide"},
		{name: "same content", record: onCluster(aws(), docs.Diff{SameContent: true, Packs: []docs.Pack{{Key: "os/ubuntu", Status: docs.PackChanged}}}), nothing: true},
		{name: "image id changed with same true", record: onCluster(aws(), docs.Diff{Same: true, SameContent: true, ImageIDsChanged: []docs.KeyOnly{{Key: "os/ubuntu"}}}),
			kind: ImageIDChanged, packs: []string{"os/ubuntu"}},
		{name: "overwritten", record: drifted(aws(), &docs.Verify{OverwrittenBy: "aws-stack@1.1.1", Who: who()}), kind: Overwritten, packs: []string{}, whoLen: 1, impact: "in-place"},
		{name: "renamed", record: drifted(aws(), &docs.Verify{RenamedTo: "aws-main"}), kind: Renamed, packs: []string{}, impact: "in-place"},
		{name: "gone", record: drifted(aws(), &docs.Verify{Gone: true}), kind: Gone, packs: []string{}, impact: "in-place"},
		{name: "a record that the cluster does not run still has its verify", record: notOnCluster(drifted(aws(), &docs.Verify{}, "csi/csi-aws-ebs")),
			kind: PublishedChanged, packs: []string{"csi/csi-aws-ebs"}, impact: "in-place"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			got := FindingsOf(content(t, c.record))
			if c.nothing {
				if len(got) != 0 {
					t.Fatalf("findings %+v", got)
				}
				return
			}
			if len(got) != 1 {
				t.Fatalf("findings %+v", got)
			}
			f := got[0]
			if f.Kind != c.kind || !slices.Equal(f.Packs, c.packs) || f.AlsoOnCluster != c.also || len(f.Who) != c.whoLen || f.Impact != c.impact {
				t.Errorf("finding %+v", f)
			}
			if f.Profile != "aws-stack@1.1.0" || f.ID != FindingID(c.kind, "aws-stack@1.1.0") {
				t.Errorf("profile %q, id %q", f.Profile, f.ID)
			}
		})
	}
}

func TestAnEditWithAnotherRootOnTheClusterIsTwoFindings(t *testing.T) {
	r := onCluster(drifted(rec("aws-stack", "1.1.0", docs.VerifyUnchanged), &docs.Verify{}, "csi/csi-aws-ebs"),
		docs.Diff{RootB: "r3", Packs: []docs.Pack{changed("csi/csi-aws-ebs", "charts.a.b")}})
	got := FindingsOf(content(t, r))
	if len(got) != 2 || got[0].Kind != ClusterDiffers || got[1].Kind != PublishedChanged || got[1].AlsoOnCluster {
		t.Fatalf("findings %+v", got)
	}
}

func TestACompareThatIsDifferentAndHasAnImageIDGivesBothFindings(t *testing.T) {
	r := onCluster(rec("aws-stack", "1.1.0", docs.VerifyUnchanged), docs.Diff{ImageIDsChanged: []docs.KeyOnly{{Key: "os/ubuntu"}}})
	got := FindingsOf(content(t, r))
	if len(got) != 2 || got[0].Kind != ClusterDiffers || got[1].Kind != ImageIDChanged {
		t.Fatalf("findings %+v", got)
	}
}

func TestEachFindingHasAUniqueIDOfSixteenHexDigits(t *testing.T) {
	seen := map[string]bool{}
	for _, k := range []Kind{PublishedChanged, Overwritten, Renamed, Gone, ClusterDiffers, ImageIDChanged} {
		for _, subject := range []string{"aws-stack@1.1.0", "aws-stack@1.1.1"} {
			id := FindingID(k, subject)
			if len(id) != 16 || strings.Trim(id, "0123456789abcdef") != "" || seen[id] || id != FindingID(k, subject) {
				t.Errorf("id %q of %s %s", id, k, subject)
			}
			seen[id] = true
		}
	}
}

func TestReBaselineNeededComparesTheMinorVersion(t *testing.T) {
	for _, c := range []struct {
		recorded, binary string
		want             bool
	}{
		{"0.28.0", "0.29.0", true},
		{"0.28.4", "0.28.9", false},
		{"0.29.0", "0.28.0", false},
		{"0.28.0", "0.28.0", false},
		{"0.9.0", "0.10.0", true},
		{"0.28.0", "1.0.0", true},
		{"1.0.0", "0.28.0", false},
		{"0.28.0-rc.1", "0.29.0", true},
		{"", "0.29.0", false},
		{"0.28.0", "dev", false},
		{"x.y.z", "0.29.0", false},
	} {
		if got := ReBaselineNeeded(c.recorded, c.binary); got != c.want {
			t.Errorf("ReBaselineNeeded(%q, %q) = %v", c.recorded, c.binary, got)
		}
	}
}

func TestAFindingOfARecordFromAnOlderMinorVersionNeedsARebaseline(t *testing.T) {
	r := drifted(rec("aws-stack", "1.1.0", docs.VerifyUnchanged), &docs.Verify{}, "csi/csi-aws-ebs")
	c := content(t, r)
	c.Strata = "0.29.0"
	if got := FindingsOf(c); len(got) != 1 || !got[0].ReBaselineNeeded {
		t.Fatalf("findings %+v", got)
	}
	r.Doc.Tool.Version = "0.29.1"
	if got := FindingsOf(content(t, r)); len(got) != 1 || got[0].ReBaselineNeeded {
		t.Fatalf("findings %+v", got)
	}
}

func TestTheFindingsComeInTheOrderOfTheProfileAndTheKind(t *testing.T) {
	b := drifted(rec("b-stack", "1.0.0", docs.VerifyUnchanged), &docs.Verify{})
	a := onCluster(drifted(rec("a-stack", "1.0.0", docs.VerifyUnchanged), &docs.Verify{}), docs.Diff{RootB: "other"})
	got := FindingsOf(content(t, b, a))
	var order []string
	for _, f := range got {
		order = append(order, f.Profile+" "+string(f.Kind))
	}
	want := []string{"a-stack@1.0.0 cluster-differs", "a-stack@1.0.0 published-changed", "b-stack@1.0.0 published-changed"}
	if !slices.Equal(order, want) {
		t.Fatalf("order %v", order)
	}
}
