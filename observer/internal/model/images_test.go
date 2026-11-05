package model

import (
	"encoding/json"
	"slices"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/kube"
)

const (
	d1 = "sha256:1111111111111111111111111111111111111111111111111111111111111111"
	d2 = "sha256:2222222222222222222222222222222222222222222222222222222222222222"
	d3 = "sha256:3333333333333333333333333333333333333333333333333333333333333333"

	vendorRef = "vendor.example/product:2026.2"
)

// ran makes a container of the given pod that runs the image with the image id repo@digest.
func ran(pod, image, digest string) kube.Container {
	return kube.Container{Pod: "ns/" + pod, Image: image, ImageID: strings.SplitN(image, ":", 2)[0] + "@" + digest}
}

// naming makes the pack of a record name the given images.
func naming(p docs.RecordPack, images ...string) docs.RecordPack {
	p.Images = images
	return p
}

// podsOf makes a content with one record whose pack names vendorRef, and the given containers.
func podsOf(t *testing.T, containers ...kube.Container) Content {
	t.Helper()
	r := rec("wilma-gateway", "3.1.2", docs.VerifyUnchanged, naming(pk("addon", "vendor-product", "2026.2.0"), vendorRef))
	c := content(t, r)
	c.Pods = PodRead{Containers: containers, At: t0}
	return c
}

// firstOf gives the first digest of an image by its id, as the state keeps it.
func firstOf(ref, digest string) map[string]string {
	return map[string]string{ImageKey(ref): digest}
}

func kindsOf(findings []Finding) []Kind {
	out := []Kind{}
	for _, f := range findings {
		out = append(out, f.Kind)
	}
	return out
}

func TestTheDigestOfAnImageIDIsItsRepositoryDigest(t *testing.T) {
	for _, c := range []struct{ id, want string }{
		{"registry.k8s.io/pause@" + d1, d1},
		{"docker-pullable://vendor.example/product@" + d2, d2},
		{"sha256:" + strings.Repeat("a", 64), ""},
		{"vendor.example/product@md5:abc", ""},
		{"", ""},
	} {
		if got := digestOf(c.id); got != c.want {
			t.Errorf("digestOf(%q) = %q, want %q", c.id, got, c.want)
		}
	}
}

func TestOnePodOnOneDigestIsSteadyAndMakesNoFinding(t *testing.T) {
	c := podsOf(t, ran("a", vendorRef, d1), ran("b", vendorRef, d1))
	rows := imageRows(c, firstOf(vendorRef, d1))
	if len(rows) != 1 || rows[0].Status != ImageStatusSteady || rows[0].Digests[0].Status != FirstDigest || rows[0].Digests[0].Pods != 2 {
		t.Fatalf("rows %+v", rows)
	}
	if got := FindingsOf(c, firstOf(vendorRef, d1)); len(got) != 0 {
		t.Errorf("findings %+v", got)
	}
}

func TestTheDigestOfTheFirstSightIsTheFirstDigestAlsoBeforeTheStateHoldsIt(t *testing.T) {
	rows := imageRows(podsOf(t, ran("a", vendorRef, d1)), nil)
	if rows[0].Status != ImageStatusSteady || rows[0].Digests[0].Status != FirstDigest {
		t.Errorf("row %+v", rows[0])
	}
}

func TestADigestThatIsNotTheFirstOneIsImageMovedUntilItComesBack(t *testing.T) {
	first := firstOf(vendorRef, d1)
	moved := podsOf(t, ran("a", vendorRef, d2), ran("b", vendorRef, d2))
	got := FindingsOf(moved, first)
	if len(got) != 1 || got[0].Kind != ImageMoved || got[0].Profile != "wilma-gateway@3.1.2" ||
		!slices.Equal(got[0].Packs, []string{"addon/vendor-product"}) || !slices.Equal(got[0].Images, []string{vendorRef}) || got[0].ReBaselineNeeded {
		t.Fatalf("findings %+v", got)
	}
	row := imageRows(moved, first)[0]
	if row.Status != ImageStatusMoved || row.Digests[0].Status != AnotherDigest {
		t.Errorf("row %+v", row)
	}
	back := podsOf(t, ran("a", vendorRef, d1))
	if got := FindingsOf(back, first); len(got) != 0 {
		t.Errorf("the digest came back and the finding stays: %+v", got)
	}
}

func TestTwoDigestsAtOneTimeAreImageSplitAndNotAlsoImageMoved(t *testing.T) {
	first := firstOf(vendorRef, d1)
	split := podsOf(t, ran("a", vendorRef, d1), ran("b", vendorRef, d1), ran("c", vendorRef, d2))
	got := FindingsOf(split, first)
	if len(got) != 1 || got[0].Kind != ImageSplit {
		t.Fatalf("findings %+v", got)
	}
	row := imageRows(split, first)[0]
	if row.Status != ImageStatusSplit || len(row.Digests) != 2 || row.Digests[0].Status != FirstDigest || row.Digests[0].Pods != 2 ||
		row.Digests[1].Status != AnotherDigest || row.Digests[1].Pods != 1 {
		t.Errorf("row %+v", row)
	}
	neither := podsOf(t, ran("a", vendorRef, d2), ran("b", vendorRef, d3))
	if got := kindsOf(FindingsOf(neither, first)); !slices.Equal(got, []Kind{ImageSplit}) {
		t.Errorf("two digests, and none is the first one: %v", got)
	}
}

func TestAContainerThatNeverStartedOrRunsALocalImageReadsNotCheckedAndMakesNoFinding(t *testing.T) {
	never := kube.Container{Pod: "ns/a", Image: vendorRef}
	local := kube.Container{Pod: "ns/b", Image: vendorRef, ImageID: "sha256:" + strings.Repeat("e", 64)}
	c := podsOf(t, never, local)
	row := imageRows(c, firstOf(vendorRef, d1))[0]
	if row.Status != ImageStatusNotChecked || len(row.Digests) != 2 || row.Digests[0].Status != NoRepoDigest || row.Digests[1].Status != NotStarted {
		t.Errorf("row %+v", row)
	}
	for _, d := range row.Digests {
		if d.Digest != "" {
			t.Errorf("a digest %q with status %s", d.Digest, d.Status)
		}
	}
	if got := FindingsOf(c, firstOf(vendorRef, d1)); len(got) != 0 {
		t.Errorf("findings %+v", got)
	}
	// A pod that never started beside a pod that runs the first digest does not make the image split.
	both := podsOf(t, never, ran("c", vendorRef, d1))
	if row := imageRows(both, firstOf(vendorRef, d1))[0]; row.Status != ImageStatusSteady {
		t.Errorf("status %s", row.Status)
	}
}

func TestAnImageThatMatchesNoPackIsListedWithNoPackAndMakesNoFinding(t *testing.T) {
	agent := "gcr.io/spectro-images-public/cluster-api/cluster-api-controller:v1.11.1"
	c := podsOf(t, ran("a", agent, d2))
	row := imageRows(c, firstOf(agent, d1))[0]
	if row.Match != MatchNone || len(row.Packs) != 0 || row.Status != ImageStatusMoved {
		t.Errorf("row %+v", row)
	}
	if got := FindingsOf(c, firstOf(agent, d1)); len(got) != 0 {
		t.Errorf("findings %+v", got)
	}
	if o := ObservedOf(Input{ReadPods: true, Content: &c, FirstDigests: firstOf(agent, d1)}); o.Counts.Images != 1 || o.Counts.Matched != 0 || o.Counts.Changed != 0 {
		t.Errorf("counts %+v", o.Counts)
	}
}

func TestAnImageNamedByADigestCannotMoveAndMakesNoFinding(t *testing.T) {
	ref := "vendor.example/product@" + d1
	r := rec("wilma-gateway", "3.1.2", docs.VerifyUnchanged, naming(pk("addon", "vendor-product", "2026.2.0"), ref))
	c := content(t, r)
	c.Pods = PodRead{Containers: []kube.Container{{Pod: "ns/a", Image: ref, ImageID: "vendor.example/product@" + d2}}, At: t0}
	row := imageRows(c, firstOf(ref, d1))[0]
	if !row.ByDigest || row.Match != MatchExact {
		t.Errorf("row %+v", row)
	}
	if got := FindingsOf(c, firstOf(ref, d1)); len(got) != 0 {
		t.Errorf("findings %+v", got)
	}
}

func TestAnImageMatchesByTheExactTextFirstAndThenByNameAndTag(t *testing.T) {
	exact := rec("aws-stack", "1.0.0", docs.VerifyUnchanged, naming(pk("addon", "nginx", "1.0"), "docker.io/library/nginx:1.27"))
	byName := rec("other", "1.0.0", docs.VerifyUnchanged, naming(pk("addon", "web", "1.0"), "nginx:1.27"))
	c := content(t, exact, byName)
	c.Pods = PodRead{At: t0, Containers: []kube.Container{
		ran("a", "docker.io/library/nginx:1.27", d1), ran("b", "docker.io/nginx:1.27", d1), ran("c", "nginx:1.27", d1), ran("d", "nginx:1.28", d1)}}
	got := map[string]Image{}
	for _, img := range imageRows(c, nil) {
		got[img.Ref] = img
	}
	if g := got["docker.io/library/nginx:1.27"]; g.Match != MatchExact || !slices.Equal(g.Packs, []ImagePack{{"aws-stack@1.0.0", "addon/nginx"}}) {
		t.Errorf("exact: %+v", g)
	}
	if g := got["docker.io/nginx:1.27"]; g.Match != MatchNameAndTag || len(g.Packs) != 2 {
		t.Errorf("name and tag: %+v", g)
	}
	if g := got["nginx:1.27"]; g.Match != MatchExact || !slices.Equal(g.Packs, []ImagePack{{"other@1.0.0", "addon/web"}}) {
		t.Errorf("exact, short form: %+v", g)
	}
	if g := got["nginx:1.28"]; g.Match != MatchNone || len(g.Packs) != 0 {
		t.Errorf("another tag: %+v", g)
	}
}

func TestOnlyARecordThatTheClusterRunsMatches(t *testing.T) {
	r := notOnCluster(rec("elsewhere", "1.0.0", docs.VerifyUnchanged, naming(pk("addon", "vendor-product", "1"), vendorRef)))
	c := content(t, r)
	c.Pods = PodRead{Containers: []kube.Container{ran("a", vendorRef, d2)}, At: t0}
	if row := imageRows(c, firstOf(vendorRef, d1))[0]; row.Match != MatchNone {
		t.Errorf("row %+v", row)
	}
}

func TestAPodWithTwoContainersOnOneImageCountsOnce(t *testing.T) {
	c := podsOf(t, ran("a", vendorRef, d1), ran("a", vendorRef, d1), ran("b", vendorRef, d1))
	if row := imageRows(c, nil)[0]; row.Digests[0].Pods != 2 {
		t.Errorf("row %+v", row)
	}
}

func TestOneFindingNamesEachPackAndEachImageOfAProfile(t *testing.T) {
	second := "vendor.example/sidecar:2026.2"
	r := rec("wilma-gateway", "3.1.2", docs.VerifyUnchanged,
		naming(pk("addon", "vendor-product", "2026.2.0"), vendorRef), naming(pk("addon", "vendor-sidecar", "2026.2.0"), second))
	c := content(t, r)
	c.Pods = PodRead{At: t0, Containers: []kube.Container{ran("a", vendorRef, d2), ran("b", second, d2)}}
	first := map[string]string{ImageKey(vendorRef): d1, ImageKey(second): d1}
	got := FindingsOf(c, first)
	if len(got) != 1 || got[0].Kind != ImageMoved || !slices.Equal(got[0].Packs, []string{"addon/vendor-product", "addon/vendor-sidecar"}) ||
		!slices.Equal(got[0].Images, []string{vendorRef, second}) {
		t.Errorf("findings %+v", got)
	}
}

func TestTheFirstDigestIsTheOneOfMostPodsAndTheSmallerTextWinsATie(t *testing.T) {
	one := FirstDigests([]kube.Container{ran("a", vendorRef, d2), ran("b", vendorRef, d1), ran("c", vendorRef, d1)}, nil)
	if one[ImageKey(vendorRef)] != d1 {
		t.Errorf("most pods: %v", one)
	}
	tie := FirstDigests([]kube.Container{ran("a", vendorRef, d2), ran("b", vendorRef, d1)}, nil)
	if tie[ImageKey(vendorRef)] != d1 {
		t.Errorf("tie: %v", tie)
	}
	known := FirstDigests([]kube.Container{ran("a", vendorRef, d2)}, firstOf(vendorRef, d1))
	if len(known) != 0 {
		t.Errorf("an image that the state holds gets no new first digest: %v", known)
	}
	none := FirstDigests([]kube.Container{{Pod: "ns/a", Image: vendorRef}}, nil)
	if len(none) != 0 {
		t.Errorf("a container that never started is no first digest: %v", none)
	}
}

func TestTheStateKeepsAnIdOfTheImageAndNeverItsReference(t *testing.T) {
	id := ImageKey(vendorRef)
	if len(id) != 16 || strings.Contains(id, "vendor") || id != ImageKey(vendorRef) || id == ImageKey(vendorRef+"x") {
		t.Errorf("id %q", id)
	}
	if id == FindingID(ImageMoved, vendorRef) {
		t.Error("an image id and a finding id of one subject are the same")
	}
}

func TestTheDocumentSaysOffNotReadOrRead(t *testing.T) {
	off := ObservedOf(Input{})
	if off.Schema != ObservedSchema || off.Pods != PodsOff || off.Images == nil || len(off.Images) != 0 || off.ReadAt != nil {
		t.Errorf("off: %+v", off)
	}
	if o := ObservedOf(Input{ReadPods: true}); o.Pods != PodsUnread || o.ReadAt != nil || len(o.Images) != 0 {
		t.Errorf("not read yet: %+v", o)
	}
	failed := podsOf(t)
	failed.Pods = PodRead{Err: &ErrorRef{Kind: docs.KindKubeAPI, Status: 403}}
	if o := ObservedOf(Input{ReadPods: true, Content: &failed}); o.Pods != PodsUnread || o.Error == nil || o.Error.Status != 403 {
		t.Errorf("failed: %+v", o)
	}
	c := podsOf(t, ran("a", vendorRef, d1))
	o := ObservedOf(Input{ReadPods: true, Content: &c})
	if o.Pods != PodsRead || o.ReadAt == nil || !o.ReadAt.Equal(t0) || o.Counts.Images != 1 || o.Counts.Matched != 1 || o.Counts.Changed != 0 {
		t.Errorf("read: %+v", o)
	}
}

func TestAFailedReadKeepsTheLastGoodContainersAndHoldsTheFindings(t *testing.T) {
	first := firstOf(vendorRef, d1)
	last := podsOf(t, ran("a", vendorRef, d2))
	now := podsOf(t)
	now.Pods = PodRead{Err: &ErrorRef{Kind: docs.KindKubeTransport}}
	now.Carry(&last)
	if now.Pods.Err == nil || !now.Pods.read() || !now.Pods.At.Equal(t0) || len(now.Pods.Containers) != 1 {
		t.Fatalf("pods %+v", now.Pods)
	}
	if got := kindsOf(FindingsOf(now, first)); !slices.Equal(got, []Kind{ImageMoved}) {
		t.Errorf("findings %v", got)
	}
	o := ObservedOf(Input{ReadPods: true, Content: &now, FirstDigests: first})
	if o.Pods != PodsRead || o.Error == nil || len(o.Images) != 1 {
		t.Errorf("document %+v", o)
	}
	// With no good read before, the content holds the findings of the state and shows no image.
	cold := podsOf(t)
	cold.Pods = PodRead{Err: &ErrorRef{Kind: docs.KindKubeAPI, Status: 500}}
	cold.Carry(&Content{})
	for _, kind := range imageKinds {
		if !slices.Contains(HeldIDs(cold), FindingID(kind, "wilma-gateway@3.1.2")) {
			t.Errorf("the finding %s is not held", kind)
		}
	}
	if slices.Contains(HeldIDs(podsOf(t)), FindingID(ImageMoved, "wilma-gateway@3.1.2")) {
		t.Error("a read that worked holds the finding")
	}
}

func TestTheCausePodsNotReadNamesNoProfileAndChangesNoStatus(t *testing.T) {
	c := podsOf(t)
	c.Pods = PodRead{Err: &ErrorRef{Kind: docs.KindKubeAPI, Status: 403}}
	s := build(&c, nil)
	if len(s.NotChecked) != 1 || s.NotChecked[0].Cause != PodsNotRead || len(s.NotChecked[0].Profiles) != 0 || len(s.NotChecked[0].Packs) != 0 {
		t.Fatalf("notChecked %+v", s.NotChecked)
	}
	if s.Profiles[0].Status != StatusOK || s.Counts.NotChecked != 1 || s.Counts.PacksInFull != s.Counts.Packs {
		t.Errorf("profile %q, counts %+v", s.Profiles[0].Status, s.Counts)
	}
	ok := podsOf(t, ran("a", vendorRef, d1))
	if got := build(&ok, nil).NotChecked; len(got) != 0 {
		t.Errorf("a read that worked: %+v", got)
	}
}

func TestAnImageFindingIsNotADriftOfTheContent(t *testing.T) {
	c := podsOf(t, ran("a", vendorRef, d2))
	in := Input{Now: t0, ClusterUID: "c1", StartedAt: t0, Files: 1, FilesKnown: true, Content: &c, ContentClock: goodClock(15*60e9, t0),
		ReadPods: true, FirstDigests: firstOf(vendorRef, d1)}
	s := Build(in)
	if len(s.Findings) != 1 || s.Findings[0].Kind != ImageMoved || s.Counts.Findings != 1 {
		t.Fatalf("findings %+v, counts %+v", s.Findings, s.Counts)
	}
	if s.Counts.Drift != 0 || s.Profiles[0].Status != StatusOK || s.Profiles[0].Packs[0].Status != StatusOK {
		t.Errorf("profile %q, pack %q, counts %+v", s.Profiles[0].Status, s.Profiles[0].Packs[0].Status, s.Counts)
	}
	if len(s.Events) != 1 || s.Events[0].Reason != "ImageMoved" || s.Events[0].Type != EventDrift {
		t.Errorf("events %+v", s.Events)
	}
}

func TestNoAnswerOfThePodsCarriesAPodNameOrANamespace(t *testing.T) {
	marker := kube.Container{Pod: "POD-NAMESPACE-MARKER/POD-NAME-MARKER", Image: vendorRef, ImageID: vendorRef + "@" + d2}
	c := podsOf(t, marker)
	in := Input{Now: t0, ClusterUID: "c1", StartedAt: t0, Files: 1, FilesKnown: true, Content: &c, ContentClock: goodClock(15*60e9, t0),
		ReadPods: true, FirstDigests: firstOf(vendorRef, d1)}
	for name, v := range map[string]any{"summary": Build(in), "observed": ObservedOf(in)} {
		raw, err := json.Marshal(v)
		if err != nil {
			t.Fatal(err)
		}
		if strings.Contains(string(raw), "MARKER") {
			t.Errorf("%s holds a marker: %s", name, raw)
		}
	}
	o := ObservedOf(in)
	view, ok := o.ImageOf(vendorRef)
	if !ok || view.Image.Ref != vendorRef || view.Schema != ObservedSchema {
		t.Errorf("view %+v", view)
	}
	if _, ok := o.ImageOf("nope"); ok {
		t.Error("an unknown reference has a view")
	}
}

func TestTheRowsAreSortedByStatusAndThenByReference(t *testing.T) {
	steady, moved, split := "a/steady:1", "z/moved:1", "m/split:1"
	c := podsOf(t, ran("a", steady, d1), ran("b", moved, d2), ran("c", split, d1), ran("d", split, d2), kube.Container{Pod: "ns/e", Image: "b/never:1"})
	first := map[string]string{ImageKey(steady): d1, ImageKey(moved): d1, ImageKey(split): d1}
	var refs []string
	for _, img := range imageRows(c, first) {
		refs = append(refs, img.Ref)
	}
	if want := []string{split, moved, steady, "b/never:1"}; !slices.Equal(refs, want) {
		t.Errorf("refs %v, want %v", refs, want)
	}
}
