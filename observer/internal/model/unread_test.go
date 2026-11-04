package model

import (
	"slices"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
)

// notRead makes a `verify` row that strata could not read.
func notRead(r Record) Record {
	r.Row = &docs.VerifyRow{Status: docs.VerifyError, Error: &docs.Error{Kind: docs.KindAPI, Status: 429}}
	return r
}

func TestASideThatWasNotReadIsNotCheckedForTheProfileAndItsPacks(t *testing.T) {
	dex, cert := pk("addon", "dex", "2.42.0"), pk("addon", "cert-manager", "1.20.3")
	r := rec("fred-identity", "2.3.0", docs.VerifyUnchanged, dex, cert)
	r.Cluster = ClusterSide{Runs: true}
	other := rec("other", "1.0.0", docs.VerifyUnchanged, pk("os", "ubuntu", "22.04"))
	s := build(ptr(content(t, r, other)), nil)
	p := s.Profiles[0]
	if p.Status != StatusNotChecked || p.ClusterCopy.Status != CopyNotRead {
		t.Errorf("profile %q, clusterCopy %+v", p.Status, p.ClusterCopy)
	}
	for _, k := range p.Packs {
		if k.Status != StatusNotChecked {
			t.Errorf("pack %s is %q", k.Key, k.Status)
		}
	}
	want := NotChecked{Cause: ReadError, Profiles: []string{"fred-identity@2.3.0"}, Packs: []string{"addon/cert-manager", "addon/dex"}}
	if len(s.NotChecked) != 1 || !equalRow(s.NotChecked[0], want) {
		t.Errorf("notChecked %+v", s.NotChecked)
	}
	if s.Counts.Packs != 3 || s.Counts.PacksInFull != 1 {
		t.Errorf("counts %+v", s.Counts)
	}
	if s.Profiles[1].Status != StatusOK {
		t.Errorf("a record that was read is %q", s.Profiles[1].Status)
	}
}

func TestAPackWithChangesUnderAFindingWithNoPackKeyDoesNotReadOK(t *testing.T) {
	cases := []struct {
		name string
		v    docs.Verify
		kind Kind
	}{
		{"overwritten", docs.Verify{OverwrittenBy: "aws-stack@1.1.1"}, Overwritten},
		{"renamed", docs.Verify{RenamedTo: "aws-stack-2@1.1.0"}, Renamed},
		{"gone", docs.Verify{Gone: true}, Gone},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			r := drifted(rec("aws-stack", "1.1.0", docs.VerifyUnchanged, pk("csi", "csi-aws-ebs", "1.65.0"), pk("os", "ubuntu", "22.04")), &c.v, "csi/csi-aws-ebs")
			s := build(ptr(content(t, r)), &Versions{At: t0, Strata: "0.29.0", Moves: map[string]Move{}})
			if len(s.Findings) != 1 || s.Findings[0].Kind != c.kind || len(s.Findings[0].Packs) != 0 {
				t.Fatalf("findings %+v", s.Findings)
			}
			if got := s.Profiles[0].Packs[0].Status; got != StatusDrift {
				t.Errorf("the pack with changes is %q", got)
			}
			if got := s.Profiles[0].Packs[1].Status; got != StatusOK {
				t.Errorf("the pack with no change is %q", got)
			}
			if s.Profiles[0].Status != StatusDrift || s.Counts.Findings != 1 || s.Counts.Drift != 1 {
				t.Errorf("profile %q, counts %+v", s.Profiles[0].Status, s.Counts)
			}
		})
	}
}

func TestAMoveThatWasNotReadIsACauseAndKeepsItsRow(t *testing.T) {
	read := pin("kubernetes", "k8s", "1.35.3", "1.36.2", "aws-stack@1.1.0")
	unread := pin("cert-manager", "addon", "1.20.3", "1.21.1", "aws-stack@1.1.0")
	move := Move{Impact: "repave", Flags: []string{docs.FlagIdentity}, Fold: docs.Fold{Settings: 2}, Images: 8}
	v := &Versions{At: t0, Strata: "0.29.0", Pins: []docs.Pin{read, unread},
		Moves: map[string]Move{MoveKey(read): move}, Unread: map[string]bool{MoveKey(unread): true}}
	r := rec("aws-stack", "1.1.0", docs.VerifyUnchanged, pk("k8s", "kubernetes", "1.35.3"), pk("addon", "cert-manager", "1.20.3"))
	s := build(ptr(content(t, r)), v)
	if len(s.Newer) != 2 {
		t.Fatalf("rows %+v", s.Newer)
	}
	if got := s.Newer[0]; got.Name != "kubernetes" || got.Impact != "repave" || got.Images != 8 {
		t.Errorf("the move that was read has %+v", got.Move)
	}
	if got := s.Newer[1]; got.Name != "cert-manager" || got.Newest != "1.21.1" || got.Impact != "" || got.Images != 0 || got.Fold != (docs.Fold{}) {
		t.Errorf("the move that was not read has counts: %+v", got)
	}
	want := NotChecked{Cause: PinError, Profiles: []string{"aws-stack@1.1.0"}, Packs: []string{"addon/cert-manager"}}
	if len(s.NotChecked) != 1 || !equalRow(s.NotChecked[0], want) {
		t.Errorf("notChecked %+v", s.NotChecked)
	}
}

func TestAMoveThatWasNotReadKeepsTheCauseOfAnOlderRegistryCopy(t *testing.T) {
	unread := pin("cert-manager", "addon", "1.20.3", "1.21.1", "aws-stack@1.1.0")
	unread.RegistrySync = &docs.RegistrySync{LastSyncedTime: "2026-10-30T09:00:00Z"}
	v := &Versions{At: t0, Strata: "0.29.0", Pins: []docs.Pin{unread}, Moves: map[string]Move{}, Unread: map[string]bool{MoveKey(unread): true}}
	var causes []Cause
	for _, row := range build(nil, v).NotChecked {
		causes = append(causes, row.Cause)
	}
	if !slices.Equal(causes, []Cause{OlderRegistryCopy, PinError}) {
		t.Errorf("causes %v", causes)
	}
}

func TestCarryPutsTheLastGoodAnswerInPlaceOfAReadThatFailed(t *testing.T) {
	dex := pk("addon", "dex", "2.42.0")
	drift := drifted(rec("fred-identity", "2.3.0", docs.VerifyUnchanged, dex), &docs.Verify{}, "addon/dex")
	differs := onCluster(drift, docs.Diff{RootB: "r3", Packs: []docs.Pack{changed("addon/dex", "charts.a.b")}})
	last := content(t, differs)
	unreadSide := func(r Record) Record {
		r.Cluster = ClusterSide{Runs: true}
		return r
	}
	good := rec("fred-identity", "2.3.0", docs.VerifyUnchanged, dex)
	goodSide := onCluster(good, docs.Diff{Same: true, SameContent: true})
	cases := []struct {
		name         string
		now          Record
		rowCarried   bool
		sideCarried  bool
		wantFindings []Kind
	}{
		{"a row that failed", notRead(goodSide), true, false, []Kind{PublishedChanged}},
		{"a side that failed", unreadSide(good), false, true, []Kind{ClusterDiffers}},
		{"a row and a side that failed", unreadSide(notRead(good)), true, true, []Kind{ClusterDiffers, PublishedChanged}},
		{"a read that worked", goodSide, false, false, nil},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			now := content(t, c.now)
			now.Carry(&last)
			r := now.Records[0]
			if r.RowCarried != c.rowCarried || r.SideCarried != c.sideCarried {
				t.Errorf("carried: row %v, side %v", r.RowCarried, r.SideCarried)
			}
			var kinds []Kind
			for _, f := range FindingsOf(now) {
				kinds = append(kinds, f.Kind)
			}
			if !slices.Equal(kinds, c.wantFindings) {
				t.Errorf("findings %v, want %v", kinds, c.wantFindings)
			}
		})
	}
}

func TestCarryTakesNothingFromAnAnswerThatIsNotGood(t *testing.T) {
	dex := pk("addon", "dex", "2.42.0")
	good := drifted(rec("fred-identity", "2.3.0", docs.VerifyUnchanged, dex), &docs.Verify{}, "addon/dex")
	changedRecord := good
	changedRecord.Doc.Root = "r-new"
	cases := []struct {
		name string
		last *Content
	}{
		{"no last cycle", nil},
		{"a last row that failed too", ptr(content(t, notRead(good)))},
		{"another profile", ptr(content(t, drifted(rec("other", "1.0.0", docs.VerifyUnchanged, dex), &docs.Verify{}, "addon/dex")))},
		{"a record that changed", ptr(content(t, changedRecord))},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			now := content(t, notRead(rec("fred-identity", "2.3.0", docs.VerifyUnchanged, dex)))
			now.Carry(c.last)
			if r := now.Records[0]; r.RowCarried || r.Row.Status != docs.VerifyError {
				t.Errorf("record %+v", r)
			}
			if len(FindingsOf(now)) != 0 {
				t.Error("a finding came from nowhere")
			}
		})
	}
}

func TestCarryKeepsTheAnswerThroughTwoFailedCycles(t *testing.T) {
	dex := pk("addon", "dex", "2.42.0")
	last := content(t, drifted(rec("fred-identity", "2.3.0", docs.VerifyUnchanged, dex), &docs.Verify{}, "addon/dex"))
	second := content(t, notRead(rec("fred-identity", "2.3.0", docs.VerifyUnchanged, dex)))
	second.Carry(&last)
	third := content(t, notRead(rec("fred-identity", "2.3.0", docs.VerifyUnchanged, dex)))
	third.Carry(&second)
	if len(FindingsOf(third)) != 1 || !third.Records[0].RowCarried {
		t.Errorf("record %+v", third.Records[0])
	}
}

func TestAReadThatFailedIsACauseAlsoWhenItsAnswerIsCarried(t *testing.T) {
	dex := pk("addon", "dex", "2.42.0")
	last := content(t, drifted(rec("fred-identity", "2.3.0", docs.VerifyUnchanged, dex), &docs.Verify{}, "addon/dex"))
	now := content(t, notRead(rec("fred-identity", "2.3.0", docs.VerifyUnchanged, dex)))
	now.Carry(&last)
	s := build(&now, nil)
	want := NotChecked{Cause: ReadError, Profiles: []string{"fred-identity@2.3.0"}, Packs: []string{"addon/dex"}}
	if len(s.Findings) != 1 || len(s.NotChecked) != 1 || !equalRow(s.NotChecked[0], want) {
		t.Fatalf("findings %+v, notChecked %+v", s.Findings, s.NotChecked)
	}
	if s.Profiles[0].Status != StatusDrift || s.Profiles[0].Packs[0].Status != StatusDrift || s.Counts.PacksInFull != 0 {
		t.Errorf("profile %+v, counts %+v", s.Profiles[0], s.Counts)
	}
}

func TestHeldIDsNameTheFindingsOfAPartThatWasNotRead(t *testing.T) {
	dex := pk("addon", "dex", "2.42.0")
	id := func(k ...Kind) []string {
		var out []string
		for _, kind := range k {
			out = append(out, FindingID(kind, "fred-identity@2.3.0"))
		}
		slices.Sort(out)
		return out
	}
	sideless := rec("fred-identity", "2.3.0", docs.VerifyUnchanged, dex)
	sideless.Cluster = ClusterSide{Runs: true}
	cases := []struct {
		name string
		r    Record
		want []string
	}{
		{"a read that worked", rec("fred-identity", "2.3.0", docs.VerifyUnchanged, dex), nil},
		{"a row that failed", notRead(onCluster(rec("fred-identity", "2.3.0", docs.VerifyUnchanged, dex), docs.Diff{Same: true})),
			id(PublishedChanged, Overwritten, Renamed, Gone)},
		{"a side that failed", sideless, id(ClusterDiffers, ImageIDChanged)},
		{"a cluster that does not run the profile", func() Record {
			r := notRead(rec("fred-identity", "2.3.0", docs.VerifyUnchanged, dex))
			r.Cluster = ClusterSide{}
			return r
		}(), id(PublishedChanged, Overwritten, Renamed, Gone)},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			got := HeldIDs(content(t, c.r))
			slices.Sort(got)
			if !slices.Equal(got, c.want) {
				t.Errorf("ids %v, want %v", got, c.want)
			}
		})
	}
}

// A finding comes from the row of `verify` or from the compare with the cluster side, and from one of
// them only. A kind in no list, or in both, would hold or drop a first sight by mistake.
func TestEachKindOfFindingComesFromOnePart(t *testing.T) {
	for _, kind := range []Kind{PublishedChanged, Overwritten, Renamed, Gone, ClusterDiffers, ImageIDChanged} {
		fromRow, fromSide := slices.Contains(publishedKinds, kind), slices.Contains(clusterKinds, kind)
		if fromRow == fromSide {
			t.Errorf("%s: published list %v, cluster list %v", kind, fromRow, fromSide)
		}
	}
}
