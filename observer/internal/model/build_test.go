package model

import (
	"encoding/json"
	"reflect"
	"slices"
	"strings"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

func marshal(t *testing.T, v any) string {
	t.Helper()
	b, err := json.Marshal(v)
	if err != nil {
		t.Fatal(err)
	}
	return string(b)
}

func pin(name, layer, version, newest string, subjects ...string) docs.Pin {
	p := docs.Pin{Name: name, Layer: layer, Registry: "Public Repo", RegistryUID: "reg1", Version: version}
	for _, s := range subjects {
		p.PinnedBy = append(p.PinnedBy, docs.PinnedBy{Subject: s})
	}
	if newest != "" {
		p.Behind = true
		p.Newer = []docs.Newer{{Version: newest, Listed: t0.Add(-24 * time.Hour).Format(time.RFC3339)}}
	}
	return p
}

func TestTheSummaryHoldsNoOldValueAndNoNewValue(t *testing.T) {
	r := onCluster(drifted(rec("aws-stack", "1.1.0", docs.VerifyUnchanged, pk("csi", "csi-aws-ebs", "1.65.0")), &docs.Verify{Who: who()}, "csi/csi-aws-ebs"),
		docs.Diff{RootB: "r3", Packs: []docs.Pack{changed("csi/csi-aws-ebs", "charts.a.b")}})
	out := marshal(t, build(ptr(content(t, r)), nil))
	for _, banned := range []string{"OLD-SENTINEL", "NEW-SENTINEL", `"old"`, `"new":"`, "charts.a.", "changedPaths"} {
		if strings.Contains(out, banned) {
			t.Errorf("the summary holds %s: %s", banned, out)
		}
	}
	if !strings.Contains(out, `"fold":{"settings":2,"followsTheVersion":0,"repeatsTheLists":0}`) {
		t.Errorf("the count of the changed values is not there: %s", out)
	}
}

func TestAnEmptyListIsNeverNull(t *testing.T) {
	out := marshal(t, Build(Input{Now: t0}))
	for _, name := range []string{"findings", "profiles", "newer", "notChecked", "events"} {
		if !strings.Contains(out, `"`+name+`":[]`) {
			t.Errorf("%s is not an empty list: %s", name, out)
		}
	}
	good := marshal(t, build(ptr(content(t, rec("aws-stack", "1.1.0", docs.VerifyUnchanged, pk("csi", "csi-aws-ebs", "1.65.0")))), nil))
	for _, banned := range []string{":null,\"flags\"", `"flags":null`, `"packs":null`, `"profiles":null`, `"findings":null`} {
		if strings.Contains(good, banned) {
			t.Errorf("a list is null (%s): %s", banned, good)
		}
	}
}

func TestTheClusterFlagsAppearOnlyWhenAFindingExists(t *testing.T) {
	clean := build(ptr(content(t, rec("aws-stack", "1.1.0", docs.VerifyUnchanged))), nil)
	if clean.Cluster.UpdatesPending != nil || clean.Cluster.RepavePending != nil || strings.Contains(marshal(t, clean.Cluster), "Pending") {
		t.Errorf("cluster %+v", clean.Cluster)
	}
	drift := build(ptr(content(t, drifted(rec("aws-stack", "1.1.0", docs.VerifyUnchanged), &docs.Verify{}))), nil)
	if drift.Cluster.UpdatesPending == nil || !*drift.Cluster.UpdatesPending || drift.Cluster.RepavePending == nil || *drift.Cluster.RepavePending {
		t.Errorf("cluster %+v", drift.Cluster)
	}
}

func TestTheClusterCopyHasOneStatusForEachAnswer(t *testing.T) {
	cases := []struct {
		name string
		side ClusterSide
		want CopyStatus
		root string
	}{
		{"same", ClusterSide{Runs: true, Diff: &docs.Diff{Same: true, SameContent: true, RootB: "rb"}}, CopySame, "rb"},
		{"same content", ClusterSide{Runs: true, Diff: &docs.Diff{SameContent: true, RootB: "rb"}}, CopySameContent, "rb"},
		{"differs", ClusterSide{Runs: true, Diff: &docs.Diff{RootB: "rb"}}, CopyDiffers, "rb"},
		{"not on this cluster", ClusterSide{}, CopyNotOnThisCluster, ""},
		{"not read", ClusterSide{Runs: true}, CopyNotRead, ""},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			r := rec("aws-stack", "1.1.0", docs.VerifyUnchanged)
			r.Cluster = c.side
			s := build(ptr(content(t, r)), nil)
			if got := s.Profiles[0].ClusterCopy; got == nil || got.Status != c.want || got.RootB != c.root {
				t.Errorf("clusterCopy %+v", got)
			}
		})
	}
}

func TestSameContentIsNoFindingAndTheProfileStaysOK(t *testing.T) {
	r := onCluster(rec("aws-stack", "1.1.0", docs.VerifyUnchanged), docs.Diff{SameContent: true})
	s := build(ptr(content(t, r)), nil)
	if len(s.Findings) != 0 || s.Profiles[0].Status != StatusOK || s.Profiles[0].ClusterCopy.Status != CopySameContent || s.Cluster.UpdatesPending != nil {
		t.Errorf("summary %+v", s)
	}
}

func TestAProfileTakesTheFirstStatusThatHolds(t *testing.T) {
	pins := func(ps ...docs.Pin) *Versions {
		return &Versions{At: t0, Strata: "0.29.0", Pins: ps, Moves: map[string]Move{}}
	}
	drift := drifted(rec("aws-stack", "1.1.0", docs.VerifyUnchanged, pk("csi", "csi-aws-ebs", "1.65.0")), &docs.Verify{}, "csi/csi-aws-ebs")
	unverified := rec("aws-stack", "1.1.0", docs.VerifyUnverified, pk("csi", "csi-aws-ebs", "1.65.0"))
	ok := rec("aws-stack", "1.1.0", docs.VerifyUnchanged, pk("csi", "csi-aws-ebs", "1.65.0"))
	newer := pin("csi-aws-ebs", "csi", "1.65.0", "1.66.0", "aws-stack@1.1.0")
	cases := []struct {
		name    string
		record  Record
		pins    *Versions
		profile Status
		pack    Status
	}{
		{"drift beats newer", drift, pins(newer), StatusDrift, StatusDrift},
		{"newer beats not checked", unverified, pins(newer), StatusNewer, StatusNewer},
		{"not checked", unverified, nil, StatusNotChecked, StatusNotChecked},
		{"ok", ok, nil, StatusOK, StatusOK},
		{"a pin that is current is no newer", ok, pins(pin("csi-aws-ebs", "csi", "1.65.0", "", "aws-stack@1.1.0")), StatusOK, StatusOK},
		{"a pin of another profile", ok, pins(pin("csi-aws-ebs", "csi", "1.65.0", "1.66.0", "other@1.0.0")), StatusOK, StatusOK},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if c.record.Row.Status == docs.VerifyUnverified {
				c.record.Row.Diff = &docs.Diff{UpstreamUnknown: []string{"csi/csi-aws-ebs"}}
			}
			s := build(ptr(content(t, c.record)), c.pins)
			if s.Profiles[0].Status != c.profile || s.Profiles[0].Packs[0].Status != c.pack {
				t.Errorf("profile %q, pack %q", s.Profiles[0].Status, s.Profiles[0].Packs[0].Status)
			}
		})
	}
}

func TestAProfileThatTheClusterRunsWithNoRecordIsNoRecord(t *testing.T) {
	c := content(t, rec("aws-stack", "1.1.0", docs.VerifyUnchanged, pk("csi", "csi-aws-ebs", "1.65.0")))
	c.Cluster = testkit.Cluster("payments", "aws-stack@1.1.0", "platform-addons@1.0.0")
	s := build(&c, nil)
	if len(s.Profiles) != 2 || s.Profiles[1].Name != "platform-addons" || s.Profiles[1].Status != StatusNoRecord || s.Profiles[1].Published != nil || s.Profiles[1].ClusterCopy != nil {
		t.Fatalf("profiles %+v", s.Profiles)
	}
	want := NotChecked{Cause: NoRecord, Profiles: []string{"platform-addons@1.0.0"}, Packs: []string{}}
	if len(s.NotChecked) != 1 || !equalRow(s.NotChecked[0], want) {
		t.Errorf("notChecked %+v", s.NotChecked)
	}
	if s.Counts.Packs != 1 || s.Counts.PacksInFull != 1 || s.Counts.Profiles != 2 {
		t.Errorf("counts %+v", s.Counts)
	}
}

func equalRow(a, b NotChecked) bool {
	return a.Cause == b.Cause && slices.Equal(a.Profiles, b.Profiles) && slices.Equal(a.Packs, b.Packs) && a.Registry == b.Registry
}

func TestNotCheckedGroupsEachCause(t *testing.T) {
	dex, velero := pk("addon", "dex", "2.42.0"), pk("addon", "velero", "1.0.0")
	unverified := rec("fred-identity", "2.3.0", docs.VerifyUnverified, dex, velero)
	unverified.Row.Diff = &docs.Diff{UpstreamUnknown: []string{"addon/velero"}, Verify: &docs.Verify{UpstreamUnknown: []string{"addon/velero"}},
		Packs: []docs.Pack{{Key: "addon/dex", UpstreamUnknown: true}}}
	second := rec("gazoo-backup", "1.2.0", docs.VerifyUnverified, velero)
	second.Row.Diff = &docs.Diff{UpstreamUnknown: []string{"addon/velero"}}
	broken := rec("broken", "1.0.0", docs.VerifyError, pk("os", "ubuntu", "22.04"), pk("k8s", "kubernetes", "1.35.3"))
	broken.Row.Error = &docs.Error{Kind: docs.KindAPI, Status: 500}
	cont := content(t, unverified, second, broken)
	cont.Unreadable = []string{"zz.json.gz"}
	versions := &Versions{At: t0, Strata: "0.29.0", Moves: map[string]Move{}, Pins: []docs.Pin{
		func() docs.Pin {
			p := pin("kubernetes", "k8s", "1.35.3", "", "broken@1.0.0")
			p.RegistrySync = &docs.RegistrySync{LastSyncedTime: "2026-10-30T09:00:00Z"}
			return p
		}(),
		func() docs.Pin {
			p := pin("cilium", "cni", "1.19.6", "", "fred-identity@2.3.0", "gazoo-backup@1.2.0")
			p.Error = &docs.Error{Kind: docs.KindAPI, Status: 404}
			return p
		}(),
	}}
	s := build(&cont, versions)
	var got []string
	for _, row := range s.NotChecked {
		got = append(got, string(row.Cause)+" "+row.Registry+" "+strings.Join(row.Profiles, ",")+" "+strings.Join(row.Packs, ","))
	}
	want := []string{
		"no-upstream-digest  fred-identity@2.3.0 addon/dex",
		"no-upstream-digest  fred-identity@2.3.0,gazoo-backup@1.2.0 addon/velero",
		"older-registry-copy Public Repo broken@1.0.0 k8s/kubernetes",
		"read-error  broken@1.0.0 k8s/kubernetes,os/ubuntu",
		"read-error  zz.json.gz ",
		"pin-error  fred-identity@2.3.0,gazoo-backup@1.2.0 cni/cilium",
	}
	// The rows keep the order of the table of the causes, and then the order of the group.
	if !slices.Equal(got, want) {
		t.Errorf("rows:\n%s\nwant:\n%s", strings.Join(got, "\n"), strings.Join(want, "\n"))
	}
	// 5 packs in the three records. The packs that a cause names are dex and velero of fred-identity,
	// velero of gazoo-backup, and the two packs of broken: five of six packs.
	if s.Counts.Packs != 5 || s.Counts.PacksInFull != 0 || s.Counts.NotChecked != 6 {
		t.Errorf("counts %+v", s.Counts)
	}
	if s.Profiles[2].Status != StatusNotChecked {
		t.Errorf("status of the broken record %q", s.Profiles[2].Status)
	}
}

func TestPacksInFullIsThePacksMinusTheOnesThatACauseNames(t *testing.T) {
	dex, cert := pk("addon", "dex", "2.42.0"), pk("addon", "cert-manager", "1.20.3")
	r := rec("fred-identity", "2.3.0", docs.VerifyUnverified, dex, cert)
	r.Row.Diff = &docs.Diff{UpstreamUnknown: []string{"addon/dex"}}
	other := notOnCluster(rec("other", "1.0.0", docs.VerifyUnchanged, pk("os", "ubuntu", "22.04")))
	s := build(ptr(content(t, r, other)), nil)
	if s.Counts.Packs != 2 || s.Counts.PacksInFull != 1 {
		t.Errorf("counts %+v", s.Counts)
	}
}

func TestACountIsTheNumberOfRowsOfItsList(t *testing.T) {
	r := drifted(rec("aws-stack", "1.1.0", docs.VerifyUnchanged, pk("csi", "csi-aws-ebs", "1.65.0"), pk("k8s", "kubernetes", "1.35.3")), &docs.Verify{}, "csi/csi-aws-ebs")
	other := rec("other", "1.0.0", docs.VerifyUnchanged, pk("k8s", "kubernetes", "1.35.3"))
	versions := &Versions{At: t0, Strata: "0.29.0", Moves: map[string]Move{}, Pins: []docs.Pin{
		pin("kubernetes", "k8s", "1.35.3", "1.36.2", "aws-stack@1.1.0", "other@1.0.0"),
		pin("csi-aws-ebs", "csi", "1.65.0", "1.66.0", "aws-stack@1.1.0"),
	}}
	s := build(ptr(content(t, r, other)), versions)
	want := Counts{Findings: 1, Drift: 1, Newer: 2, NewerProfiles: 2, NotChecked: 0, Packs: 3, PacksInFull: 3, Profiles: 2}
	if s.Counts != want {
		t.Errorf("counts %+v, want %+v", s.Counts, want)
	}
}

func TestTheNewerRowsJoinThePinAndTheMove(t *testing.T) {
	floating := pin("ubuntu-aws", "os", "20.04", "24.04", "aws-stack@1.0.0")
	floating.Floating, floating.PinnedState = true, "deprecated"
	floating.Newer[0].Drops = []string{"arm64"}
	floating.Newer[0].Listed = t0.Add(-8 * 24 * time.Hour).Format(time.RFC3339)
	fresh := pin("kubernetes", "k8s", "1.35.3", "1.36.2", "other@1.0.0", "aws-stack@1.0.0")
	fresh.Registry = ""
	current := pin("cert-manager", "addon", "1.21.1", "", "aws-stack@1.0.0")
	move := Move{Impact: "repave", Flags: []string{docs.FlagIdentity, docs.FlagAuthored}, Fold: Fold{Settings: 2, Follows: 1, Repeats: 8}, Images: 8, Charts: 1}
	v := &Versions{At: t0, Strata: "0.29.0", Pins: []docs.Pin{floating, fresh, current}, Moves: map[string]Move{MoveKey(fresh): move}}
	rows := build(nil, v).Newer
	if len(rows) != 2 {
		t.Fatalf("rows %+v", rows)
	}
	u, k := rows[0], rows[1]
	if u.Name != "ubuntu-aws" || u.Version != "20.04" || u.Newest != "24.04" || !u.Floating || u.PinnedState != "deprecated" || u.New ||
		!slices.Equal(u.Drops, []string{"arm64"}) || !slices.Equal(u.Profiles, []string{"aws-stack@1.0.0"}) || u.Registry != "Public Repo" {
		t.Errorf("floating row %+v", u)
	}
	if u.Flags == nil || len(u.Flags) != 0 || u.Impact != "" {
		t.Errorf("a move that is not there gives an empty move: %+v", u.Move)
	}
	if k.Newest != "1.36.2" || !k.New || k.Registry != "Public Repo" || k.RegistryUID != "reg1" || !slices.Equal(k.Profiles, []string{"aws-stack@1.0.0", "other@1.0.0"}) ||
		k.Drops == nil || k.Move.Impact != "repave" || k.Images != 8 || k.Charts != 1 || k.Fold != move.Fold || !slices.Equal(k.Flags, move.Flags) {
		t.Errorf("row %+v", k)
	}
}

func TestAVersionIsNewForSevenDays(t *testing.T) {
	now := t0
	for listed, want := range map[string]bool{
		now.Add(-6 * 24 * time.Hour).Format(time.RFC3339): true,
		now.Add(-7 * 24 * time.Hour).Format(time.RFC3339): true,
		now.Add(-8 * 24 * time.Hour).Format(time.RFC3339): false,
		"2026-11-01T10:00:00.123Z":                        true,
		"":                                                false,
		"yesterday":                                       false,
	} {
		if got := listedWithin(listed, now); got != want {
			t.Errorf("%q: %v", listed, got)
		}
	}
}

func TestThePackRowJoinsTheRecordAndThePins(t *testing.T) {
	dex := pk("addon", "dex", "2.42.0")
	dex.RegistryProviderType = ""
	dex.Type = docs.PackHelm
	r := drifted(rec("fred-identity", "2.3.0", docs.VerifyUnchanged, dex), &docs.Verify{}, "addon/dex")
	r.Doc.Source.ProjectUID = "p1"
	r.Row.Diff.Packs[0].Values.Diff = append(r.Row.Diff.Packs[0].Values.Diff, docs.Leaf{Path: "pack.content.images[0]"}, docs.Leaf{Path: "charts.dex.image.tag"})
	v := &Versions{At: t0, Strata: "0.29.0", Moves: map[string]Move{}, Pins: []docs.Pin{pin("dex", "addon", "2.42.0", "2.44.0", "fred-identity@2.3.0")}}
	s := build(ptr(content(t, r)), v)
	p := s.Profiles[0]
	if p.Scope != ScopeProject || p.Status != StatusDrift {
		t.Errorf("profile %+v", p)
	}
	got := p.Packs[0]
	if got.Registry != "Public Repo" || got.Kind != docs.PackHelm || got.Upstream != "sha256:dex" || got.UpstreamSource != "packRef.digest" || got.Images != 2 ||
		got.Newer != "2.44.0" || got.Status != StatusDrift || !slices.Equal(got.Flags, []string{docs.FlagAuthored}) {
		t.Errorf("pack %+v", got)
	}
	if got.Fold == nil || *got.Fold != (Fold{Settings: 2, Follows: 1, Repeats: 1}) {
		t.Errorf("fold %+v", got.Fold)
	}
	if s.Limits != (Limits{ImagesNamedByATag: 1, HelmPacks: 1}) {
		t.Errorf("limits %+v", s.Limits)
	}
}

func TestTheRegistryIsTheUidWhenNoPinNamesIt(t *testing.T) {
	s := build(ptr(content(t, rec("aws-stack", "1.1.0", docs.VerifyUnchanged, pk("csi", "csi-aws-ebs", "1.65.0")))), nil)
	if got := s.Profiles[0].Packs[0].Registry; got != "reg1" {
		t.Errorf("registry %q", got)
	}
}

func TestTheLimitsCountMaskedPathsOfEachDiff(t *testing.T) {
	r := onCluster(drifted(rec("aws-stack", "1.1.0", docs.VerifyUnchanged), &docs.Verify{}, "csi/csi-aws-ebs"),
		docs.Diff{Packs: []docs.Pack{{Key: "csi/csi-aws-ebs", Masked: []string{"a.b", "a.c"}}}})
	r.Row.Diff.Packs[0].Masked = []string{"x.y"}
	if got := build(ptr(content(t, r)), nil).Limits.MaskedPaths; got != 3 {
		t.Errorf("masked paths %d", got)
	}
}

func TestAFindingStartsAtTheStateOrAtTheCycle(t *testing.T) {
	r := drifted(rec("aws-stack", "1.1.0", docs.VerifyUnchanged), &docs.Verify{Who: who()})
	c := content(t, r)
	id := FindingID(PublishedChanged, "aws-stack@1.1.0")
	earlier := t0.Add(-48 * time.Hour)
	in := Input{Now: t0, Content: &c, ContentClock: goodClock(time.Minute, t0), FirstSeen: map[string]time.Time{id: earlier}, FilesKnown: true, Files: 1}
	f := Build(in).Findings[0]
	if !f.FirstSeen.Equal(earlier) || !f.LastSeen.Equal(t0) {
		t.Errorf("times %v %v", f.FirstSeen, f.LastSeen)
	}
	in.FirstSeen = nil
	if f := Build(in).Findings[0]; !f.FirstSeen.Equal(t0) {
		t.Errorf("first seen %v", f.FirstSeen)
	}
}

func TestEachFindingMakesOneEventTheNewestFirst(t *testing.T) {
	old := drifted(rec("a-stack", "1.0.0", docs.VerifyUnchanged), &docs.Verify{})
	young := drifted(rec("b-stack", "1.0.0", docs.VerifyUnchanged), &docs.Verify{Gone: true})
	c := content(t, old, young)
	in := Input{Now: t0, Content: &c, ContentClock: goodClock(time.Minute, t0), Files: 2, FilesKnown: true,
		FirstSeen: map[string]time.Time{FindingID(PublishedChanged, "a-stack@1.0.0"): t0.Add(-time.Hour)}}
	events := Build(in).Events
	if len(events) != 2 || events[0].Object != "b-stack@1.0.0" || events[0].Reason != "Gone" || events[1].Reason != "DriftFound" || events[1].Type != "drift" {
		t.Errorf("events %+v", events)
	}
	for kind := range eventKinds {
		if !slices.Contains(allKinds, kind) {
			t.Errorf("event of an unknown kind %s", kind)
		}
	}
	if len(eventKinds) != len(allKinds) {
		t.Errorf("%d event kinds", len(eventKinds))
	}
}

func TestTheRecordsPartSaysWhereTheBaselineCameFrom(t *testing.T) {
	c := content(t, rec("aws-stack", "1.1.0", docs.VerifyUnchanged))
	c.Meta = baseline.Meta{Baseline: baseline.FirstSeen, FirstSeenFrom: "c9"}
	s := build(&c, nil)
	if s.Records.Count != 1 || s.Records.Baseline != baseline.FirstSeen || s.Records.FirstSeenFrom != "c9" || !s.Records.StartedAt.Equal(t0) {
		t.Errorf("records %+v", s.Records)
	}
	if got := build(nil, nil).Records.Baseline; got != baseline.Unknown {
		t.Errorf("baseline %q", got)
	}
}

func TestTheClocksShowThePeriodAndTheLastError(t *testing.T) {
	in := Input{Now: t0, ContentClock: Clock{Every: 15 * time.Minute, FailedRuns: 1, LastError: &ErrorRef{Kind: docs.KindAPI, Status: 401}},
		VersionsClock: Clock{Every: 6 * time.Hour, Last: ptr(t0)}}
	out := marshal(t, Build(in))
	for _, want := range []string{
		`"content":{"checkedAt":null,"every":"15m","failedRuns":1,"lastError":{"kind":"api","status":401}}`,
		`"versions":{"readAt":"2026-11-03T14:00:00Z","every":"6h","failedRuns":0,"lastError":null}`,
	} {
		if !strings.Contains(out, want) {
			t.Errorf("missing %s in %s", want, out)
		}
	}
}

func TestAMoveReadsTheCountsOfTheCompare(t *testing.T) {
	d := docs.Diff{Summary: docs.DiffSummary{MaxImpact: "repave", ImagesChanged: 8, ChartsChanged: 1}, Packs: []docs.Pack{{Identity: true, Authored: true, Images: true,
		Values: docs.Values{Diff: []docs.Leaf{{Path: "pack.content.images[0]"}, {Path: "charts.x.image.tag"}, {Path: "charts.x.replicas"}}}}}}
	want := Move{Impact: "repave", Flags: []string{docs.FlagIdentity, docs.FlagAuthored, docs.FlagImages}, Fold: Fold{Settings: 1, Follows: 1, Repeats: 1}, Images: 8, Charts: 1}
	got := MoveOf(d)
	if got.Impact != want.Impact || !slices.Equal(got.Flags, want.Flags) || got.Fold != want.Fold || got.Images != 8 || got.Charts != 1 {
		t.Errorf("move %+v", got)
	}
	if empty := MoveOf(docs.Diff{}); empty.Flags == nil {
		t.Error("an empty move has a nil list of flags")
	}
}

func TestTheKeyOfAMoveNamesTheRegistryThePackAndTheTwoVersions(t *testing.T) {
	p := pin("kubernetes", "k8s", "1.35.3", "1.36.2", "x@1")
	if got := MoveKey(p); got != "reg1/kubernetes@1.35.3->1.36.2" {
		t.Errorf("key %q", got)
	}
	if got := MoveKey(pin("kubernetes", "k8s", "1.35.3", "")); got != "" {
		t.Errorf("key %q", got)
	}
}

func TestTheRegistryOfARowIsTheUidWhenNoPinNamesIt(t *testing.T) {
	lone := pin("kubernetes", "k8s", "1.35.3", "1.36.2", "aws-stack@1.0.0")
	lone.Registry = ""
	rows := build(nil, &Versions{At: t0, Pins: []docs.Pin{lone}}).Newer
	if len(rows) != 1 || rows[0].Registry != "reg1" {
		t.Errorf("rows %+v", rows)
	}
}

func TestAnUnverifiedRecordThatNamesNoPackStillShowsItsProfile(t *testing.T) {
	s := build(ptr(content(t, rec("aws-stack", "1.1.0", docs.VerifyUnverified, pk("csi", "csi-aws-ebs", "1.65.0")))), nil)
	if len(s.NotChecked) != 1 || s.NotChecked[0].Cause != NoUpstreamDigest || !slices.Equal(s.NotChecked[0].Profiles, []string{"aws-stack@1.1.0"}) || s.Profiles[0].Status != StatusNotChecked {
		t.Errorf("notChecked %+v", s.NotChecked)
	}
}

// profilePin is the answer of `behind` for the profile version name@version, which holds the
// newest published version newest. An empty newest is a version that is current.
func profilePin(name, version, newest, created string) docs.ProfilePin {
	p := docs.ProfilePin{Name: name, Version: version, PinnedBy: []docs.PinnedBy{{Subject: Subject(name, version)}}}
	if newest != "" {
		p.Behind = true
		p.Newer = []docs.NewerProfile{{Version: newest, Created: created}, {Version: version + "-older"}}
	}
	return p
}

// Issue #707. A profile whose newest published version is above the record shows that version, apart
// from the newer versions of its packs. The overview counts it, and it makes an event at its create.
func TestAProfileWithANewerPublishedVersionShowsItAndCountsIt(t *testing.T) {
	c := content(t,
		rec("barney-addons", "1.3.1", docs.VerifyUnchanged, pk("addon", "cert-manager", "1.21.1")),
		rec("barney-base", "1.5.0", docs.VerifyUnchanged, pk("csi", "csi-aws-ebs", "1.65.0")))
	failed := profilePin("barney-base", "1.5.0", "1.6.0", at(-time.Hour))
	failed.Error = &docs.Error{Kind: docs.KindAPI, Status: 500}
	v := &Versions{At: t0, Strata: "0.29.0", Moves: map[string]Move{},
		Pins:     []docs.Pin{pin("csi-aws-ebs", "csi", "1.65.0", "1.66.0", "barney-base@1.5.0")},
		Profiles: []docs.ProfilePin{profilePin("barney-addons", "1.3.1", "1.4.0", at(-2*time.Hour)), failed}}
	s := build(&c, v)
	addons, base := s.Profiles[0], s.Profiles[1]
	want := &NewerVersion{Version: "1.4.0", Created: at(-2 * time.Hour), New: true}
	if addons.NewerVersion == nil || *addons.NewerVersion != *want || addons.Status != StatusNewer {
		t.Errorf("barney-addons: %+v, status %q", addons.NewerVersion, addons.Status)
	}
	// a pack of barney-base has a newer version. strata could not read the versions of the profile, so
	// the row names no newer profile version
	if base.NewerVersion != nil || base.Status != StatusNewer {
		t.Errorf("barney-base: %+v, status %q", base.NewerVersion, base.Status)
	}
	if s.Counts.NewerProfileVersions != 1 || s.Counts.NewerProfiles != 1 || s.Counts.Newer != 1 {
		t.Errorf("counts %+v", s.Counts)
	}
	got := reasons(s.Events)
	addonsEvent, baseEvent := "newer "+reasonNewProfile+" barney-addons@1.3.1", "newer "+reasonNewProfile+" barney-base@1.5.0"
	if !slices.Contains(got, addonsEvent) || slices.Contains(got, baseEvent) {
		t.Errorf("events %q", got)
	}
	for _, e := range s.Events {
		if e.Reason == reasonNewProfile && (e.Message != "barney-addons@1.4.0 is a newer version of the profile." || !e.Time.Equal(t0.Add(-2*time.Hour))) {
			t.Errorf("the event %+v", e)
		}
	}
	// a version that is current, and an answer of an older strata with no profiles, show none
	v.Profiles = []docs.ProfilePin{profilePin("barney-addons", "1.3.1", "", "")}
	for _, versions := range []*Versions{v, {At: t0, Strata: "0.29.0", Moves: map[string]Move{}}} {
		s := build(&c, versions)
		if s.Profiles[0].NewerVersion != nil || s.Profiles[0].Status != StatusOK || s.Counts.NewerProfileVersions != 0 {
			t.Errorf("a current version: %+v, status %q, counts %+v", s.Profiles[0].NewerVersion, s.Profiles[0].Status, s.Counts)
		}
	}
}

// Issue #708. The row of a profile with a newer published version holds the move from the record to
// that version: the impact level, the flags of its packs, each pack whose version changes, each pack
// that the move adds or removes, and each pack whose values change. It holds no value.
func TestTheMoveToTheNewestProfileVersionNamesEachPackThatChangesAndNoValue(t *testing.T) {
	c := content(t, rec("barney-addons", "1.3.1", docs.VerifyUnchanged, pk("addon", "fred-compositions", "0.1.0"), pk("addon", "wilma-values", "1.0.0"), pk("addon", "dino-logs", "2.0.0")))
	side := func(version string) *docs.PackSide { return &docs.PackSide{Version: version} }
	fred := docs.Pack{Key: "addon/fred-compositions", Status: docs.PackChanged, Identity: true, Upstream: true, A: side("0.1.0"), B: side("0.2.0")}
	wilma := changed("addon/wilma-values", "replicas")
	wilma.Values.Changed, wilma.A, wilma.B = true, side("1.0.0"), side("1.0.0")
	diff := docs.Diff{Summary: docs.DiffSummary{MaxImpact: "in-place"}, Packs: []docs.Pack{
		{Key: "addon/dino-logs", Status: docs.PackRemoved, A: side("2.0.0")}, fred,
		{Key: "addon/pebbles-dns", Status: docs.PackAdded, B: side("1.0.0")},
		{Key: "addon/unchanged", Status: docs.PackUnchanged}, wilma,
	}}
	v := &Versions{At: t0, Strata: "0.29.0", Moves: map[string]Move{},
		Profiles:     []docs.ProfilePin{profilePin("barney-addons", "1.3.1", "1.4.0", at(-2*time.Hour))},
		ProfileMoves: map[string]docs.Diff{"barney-addons@1.3.1": diff}}
	s := build(&c, v)
	n := s.Profiles[0].NewerVersion
	if n == nil || n.Move == nil || n.Unread {
		t.Fatalf("the newer version %+v", n)
	}
	want := ProfileMove{Impact: "in-place", Flags: []string{docs.FlagIdentity, docs.FlagUpstream, docs.FlagAuthored}, Packs: []MovePack{
		{Key: "addon/dino-logs", Status: docs.PackRemoved, From: "2.0.0", Flags: []string{}},
		{Key: "addon/fred-compositions", Status: docs.PackChanged, From: "0.1.0", To: "0.2.0", Flags: []string{docs.FlagIdentity, docs.FlagUpstream}},
		{Key: "addon/pebbles-dns", Status: docs.PackAdded, To: "1.0.0", Flags: []string{}},
		{Key: "addon/wilma-values", Status: docs.PackChanged, From: "1.0.0", To: "1.0.0", Flags: []string{docs.FlagAuthored}, ValuesChanged: true},
	}}
	if !reflect.DeepEqual(*n.Move, want) {
		t.Errorf("the move\n got %+v\nwant %+v", *n.Move, want)
	}
	raw := marshal(t, s)
	for _, banned := range []string{"OLD-SENTINEL", "NEW-SENTINEL", "replicas"} {
		if strings.Contains(raw, banned) {
			t.Errorf("the summary holds %q", banned)
		}
	}

	// a compare that failed names the newer version with no move, and says so
	v.ProfileMoves, v.ProfileUnread = nil, map[string]bool{"barney-addons@1.3.1": true}
	if n := build(&c, v).Profiles[0].NewerVersion; n == nil || n.Move != nil || !n.Unread {
		t.Errorf("a failed compare: %+v", n)
	}
	// a version that is current shows no move, also when a compare of an earlier cycle is there
	v.Profiles, v.ProfileMoves, v.ProfileUnread = []docs.ProfilePin{profilePin("barney-addons", "1.3.1", "", "")}, map[string]docs.Diff{"barney-addons@1.3.1": diff}, nil
	if n := build(&c, v).Profiles[0].NewerVersion; n != nil {
		t.Errorf("a current version: %+v", n)
	}
}

// The pack row and the newer row name the newer profile version that carries a newer version of the pack (#709), as `behind`
// names it for each profile version that holds the pin. A profile version with no carrier says so, and a failed read says that
// the carrier was not read. A profile that the pin does not name has no carrier on its row.
func TestTheRowsOfAPackThatIsBehindNameItsCarrier(t *testing.T) {
	fred := pin("fred-compositions", "addon", "0.1.0", "0.2.0", "barney-addons@1.3.1", "barney-addons@1.3.0", "other@1.0.0")
	fred.Carriers = []docs.PinCarrier{
		{Profile: "barney-addons", Version: "1.3.1", Carrier: &docs.Carrier{Version: "1.4.0", PackVersion: "0.1.5"}},
		{Profile: "barney-addons", Version: "1.3.0"},
		{Profile: "other", Version: "1.0.0", Error: &docs.Error{Kind: "api", Status: 500}},
	}
	records := []Record{
		rec("barney-addons", "1.3.1", docs.VerifyUnchanged, pk("addon", "fred-compositions", "0.1.0")),
		rec("barney-addons", "1.3.0", docs.VerifyUnchanged, pk("addon", "fred-compositions", "0.1.0")),
		rec("other", "1.0.0", docs.VerifyUnchanged, pk("addon", "fred-compositions", "0.1.0")),
		rec("wilma", "1.0.0", docs.VerifyUnchanged, pk("addon", "fred-compositions", "0.1.0")),
	}
	v := &Versions{At: t0, Strata: "0.33.0", Moves: map[string]Move{}, Pins: []docs.Pin{fred}}
	s := build(ptr(content(t, records...)), v)
	want := map[string]*Carried{
		"barney-addons@1.3.1": {Profile: "barney-addons@1.3.1", Version: "1.4.0", Pack: "0.1.5"},
		"barney-addons@1.3.0": {Profile: "barney-addons@1.3.0"},
		"other@1.0.0":         {Profile: "other@1.0.0", Unread: true},
		"wilma@1.0.0":         nil,
	}
	for _, p := range s.Profiles {
		got, w := p.Packs[0].Carried, want[Subject(p.Name, p.Version)]
		if (got == nil) != (w == nil) || got != nil && *got != *w {
			t.Errorf("%s@%s: the pack row carries %+v, want %+v", p.Name, p.Version, got, w)
		}
	}
	rows := s.Newer
	if len(rows) != 1 {
		t.Fatalf("rows %+v", rows)
	}
	carriers := []Carried{*want["barney-addons@1.3.1"], *want["barney-addons@1.3.0"], *want["other@1.0.0"]}
	if !slices.Equal(rows[0].Carriers, carriers) {
		t.Errorf("the newer row carries %+v, want %+v", rows[0].Carriers, carriers)
	}
}
