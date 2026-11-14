package model

import (
	"slices"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/kube"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// d3 is a third digest of an image, after testkit.Digest1 and testkit.Digest2.
const d3 = "sha256:3333333333333333333333333333333333333333333333333333333333333333"

// nginxFull and nginxHub are the same image of nginx, with its full name and with the short name of Docker Hub. vendorAt
// starts the image id of the image testkit.VendorRef.
const (
	nginxFull = testkit.NginxImage + ":1.27"
	nginxHub  = "docker.io/nginx:1.27"
	vendorAt  = testkit.VendorImage + "@"
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

// podsOf makes a content with one record whose pack names testkit.VendorRef, and the given containers.
func podsOf(t *testing.T, containers ...kube.Container) Content {
	t.Helper()
	return podsWith(t, rec("wilma-gateway", "3.1.2", docs.VerifyUnchanged, naming(pk("addon", "vendor-product", "2026.2.0"), testkit.VendorRef)), containers...)
}

// podsWith makes a content with the record r, and the given containers, with withPods.
func podsWith(t *testing.T, r Record, containers ...kube.Container) Content {
	t.Helper()
	return withPods(content(t, r), containers...)
}

// withPods gives the content c with the given containers, which a read of the pods gave at t0.
func withPods(c Content, containers ...kube.Container) Content {
	c.Pods = PodRead{Containers: containers, At: t0}
	return c
}

// firstOf gives the first digest of an image by its id, as the state keeps it.
func firstOf(ref, digest string) map[string]string {
	return map[string]string{ImageKey(ref): digest}
}

// podsInput is the input of goodInput of c, with one file, that reads the pods with the first digests first.
func podsInput(c *Content, first map[string]string) Input {
	in := goodInput(c, nil)
	in.Files, in.ReadPods, in.FirstDigests = 1, true, first
	return in
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
		{"registry.k8s.io/pause@" + testkit.Digest1, testkit.Digest1},
		{"docker-pullable://" + vendorAt + testkit.Digest2, testkit.Digest2},
		{"sha256:" + strings.Repeat("a", 64), ""},
		{vendorAt + "md5:abc", ""},
		{"", ""},
	} {
		if got := digestOf(c.id); got != c.want {
			t.Errorf("digestOf(%q) = %q, want %q", c.id, got, c.want)
		}
	}
}

func TestOnePodOnOneDigestIsSteadyAndMakesNoFinding(t *testing.T) {
	c := podsOf(t, ran("a", testkit.VendorRef, testkit.Digest1), ran("b", testkit.VendorRef, testkit.Digest1))
	rows := imageRows(c, firstOf(testkit.VendorRef, testkit.Digest1))
	if len(rows) != 1 || rows[0].Status != ImageStatusSteady || rows[0].Digests[0].Status != FirstDigest || rows[0].Digests[0].Pods != 2 {
		t.Fatalf("rows %+v", rows)
	}
	if got := FindingsOf(c, firstOf(testkit.VendorRef, testkit.Digest1)); len(got) != 0 {
		t.Errorf("findings %+v", got)
	}
}

func TestTheDigestOfTheFirstSightIsTheFirstDigestAlsoBeforeTheStateHoldsIt(t *testing.T) {
	rows := imageRows(podsOf(t, ran("a", testkit.VendorRef, testkit.Digest1)), nil)
	if rows[0].Status != ImageStatusSteady || rows[0].Digests[0].Status != FirstDigest {
		t.Errorf("row %+v", rows[0])
	}
}

func TestADigestThatIsNotTheFirstOneIsImageMovedUntilItComesBack(t *testing.T) {
	first := firstOf(testkit.VendorRef, testkit.Digest1)
	moved := podsOf(t, ran("a", testkit.VendorRef, testkit.Digest2), ran("b", testkit.VendorRef, testkit.Digest2))
	got := FindingsOf(moved, first)
	if len(got) != 1 || got[0].Kind != ImageMoved || got[0].Profile != "wilma-gateway@3.1.2" ||
		!slices.Equal(got[0].Packs, []string{testkit.VendorPack}) || !slices.Equal(got[0].Images, []string{testkit.VendorRef}) || got[0].ReBaselineNeeded {
		t.Fatalf("findings %+v", got)
	}
	row := imageRows(moved, first)[0]
	if row.Status != ImageStatusMoved || row.Digests[0].Status != AnotherDigest {
		t.Errorf("row %+v", row)
	}
	back := podsOf(t, ran("a", testkit.VendorRef, testkit.Digest1))
	if got := FindingsOf(back, first); len(got) != 0 {
		t.Errorf("the digest came back and the finding stays: %+v", got)
	}
}

func TestTwoDigestsAtOneTimeAreImageSplitAndNotAlsoImageMoved(t *testing.T) {
	first := firstOf(testkit.VendorRef, testkit.Digest1)
	split := podsOf(t, ran("a", testkit.VendorRef, testkit.Digest1), ran("b", testkit.VendorRef, testkit.Digest1), ran("c", testkit.VendorRef, testkit.Digest2))
	got := FindingsOf(split, first)
	if len(got) != 1 || got[0].Kind != ImageSplit {
		t.Fatalf("findings %+v", got)
	}
	row := imageRows(split, first)[0]
	if row.Status != ImageStatusSplit || len(row.Digests) != 2 || row.Digests[0].Status != FirstDigest || row.Digests[0].Pods != 2 ||
		row.Digests[1].Status != AnotherDigest || row.Digests[1].Pods != 1 {
		t.Errorf("row %+v", row)
	}
	neither := podsOf(t, ran("a", testkit.VendorRef, testkit.Digest2), ran("b", testkit.VendorRef, d3))
	if got := kindsOf(FindingsOf(neither, first)); !slices.Equal(got, []Kind{ImageSplit}) {
		t.Errorf("two digests, and none is the first one: %v", got)
	}
}

func TestAContainerThatNeverStartedOrRunsALocalImageReadsNotCheckedAndMakesNoFinding(t *testing.T) {
	never := kube.Container{Pod: "ns/a", Image: testkit.VendorRef}
	local := kube.Container{Pod: "ns/b", Image: testkit.VendorRef, ImageID: "sha256:" + strings.Repeat("e", 64)}
	c := podsOf(t, never, local)
	row := imageRows(c, firstOf(testkit.VendorRef, testkit.Digest1))[0]
	if row.Status != ImageStatusNotChecked || len(row.Digests) != 2 || row.Digests[0].Status != NoRepoDigest || row.Digests[1].Status != NotStarted {
		t.Errorf("row %+v", row)
	}
	for _, d := range row.Digests {
		if d.Digest != "" {
			t.Errorf("a digest %q with status %s", d.Digest, d.Status)
		}
	}
	if got := FindingsOf(c, firstOf(testkit.VendorRef, testkit.Digest1)); len(got) != 0 {
		t.Errorf("findings %+v", got)
	}
	// A pod that never started beside a pod that runs the first digest does not make the image split.
	both := podsOf(t, never, ran("c", testkit.VendorRef, testkit.Digest1))
	if row := imageRows(both, firstOf(testkit.VendorRef, testkit.Digest1))[0]; row.Status != ImageStatusSteady {
		t.Errorf("status %s", row.Status)
	}
}

func TestAnImageThatMatchesNoPackIsListedWithNoPackAndMakesNoFinding(t *testing.T) {
	agent := "gcr.io/spectro-images-public/cluster-api/cluster-api-controller:v1.11.1"
	c := podsOf(t, ran("a", agent, testkit.Digest2))
	row := imageRows(c, firstOf(agent, testkit.Digest1))[0]
	if row.Match != MatchNone || len(row.Packs) != 0 || row.Status != ImageStatusMoved {
		t.Errorf("row %+v", row)
	}
	if got := FindingsOf(c, firstOf(agent, testkit.Digest1)); len(got) != 0 {
		t.Errorf("findings %+v", got)
	}
	if o := ObservedOf(podsInput(&c, firstOf(agent, testkit.Digest1))); o.Counts.Images != 1 || o.Counts.Matched != 0 || o.Counts.Changed != 0 {
		t.Errorf("counts %+v", o.Counts)
	}
}

func TestAnImageNamedByADigestCannotMoveAndMakesNoFinding(t *testing.T) {
	ref := vendorAt + testkit.Digest1
	c := podsWith(t, rec("wilma-gateway", "3.1.2", docs.VerifyUnchanged, naming(pk("addon", "vendor-product", "2026.2.0"), ref)),
		kube.Container{Pod: "ns/a", Image: ref, ImageID: vendorAt + testkit.Digest2})
	row := imageRows(c, firstOf(ref, testkit.Digest1))[0]
	if !row.ByDigest || row.Match != MatchExact {
		t.Errorf("row %+v", row)
	}
	if got := FindingsOf(c, firstOf(ref, testkit.Digest1)); len(got) != 0 {
		t.Errorf("findings %+v", got)
	}
}

func TestAnImageMatchesByTheExactTextFirstAndThenByNameAndTag(t *testing.T) {
	exact := rec("aws-stack", "1.0.0", docs.VerifyUnchanged, naming(pk("addon", "nginx", "1.0"), nginxFull))
	byName := rec("other", "1.0.0", docs.VerifyUnchanged, naming(pk("addon", "web", "1.0"), "nginx:1.27"))
	c := withPods(content(t, exact, byName),
		ran("a", nginxFull, testkit.Digest1), ran("b", nginxHub, testkit.Digest1), ran("c", "nginx:1.27", testkit.Digest1), ran("d", "nginx:1.28", testkit.Digest1))
	got := map[string]Image{}
	for _, img := range imageRows(c, nil) {
		got[img.Ref] = img
	}
	if g := got[nginxFull]; g.Match != MatchExact || !slices.Equal(g.Packs, []ImagePack{{"aws-stack@1.0.0", "addon/nginx"}}) {
		t.Errorf("exact: %+v", g)
	}
	if g := got[nginxHub]; g.Match != MatchNameAndTag || len(g.Packs) != 2 {
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
	c := podsWith(t, notOnCluster(rec("elsewhere", "1.0.0", docs.VerifyUnchanged, naming(pk("addon", "vendor-product", "1"), testkit.VendorRef))),
		ran("a", testkit.VendorRef, testkit.Digest2))
	if row := imageRows(c, firstOf(testkit.VendorRef, testkit.Digest1))[0]; row.Match != MatchNone {
		t.Errorf("row %+v", row)
	}
}

func TestAPodWithTwoContainersOnOneImageCountsOnce(t *testing.T) {
	c := podsOf(t, ran("a", testkit.VendorRef, testkit.Digest1), ran("a", testkit.VendorRef, testkit.Digest1), ran("b", testkit.VendorRef, testkit.Digest1))
	if row := imageRows(c, nil)[0]; row.Digests[0].Pods != 2 {
		t.Errorf("row %+v", row)
	}
}

func TestOneFindingNamesEachPackAndEachImageOfAProfile(t *testing.T) {
	second := "vendor.example/sidecar:2026.2"
	r := rec("wilma-gateway", "3.1.2", docs.VerifyUnchanged,
		naming(pk("addon", "vendor-product", "2026.2.0"), testkit.VendorRef), naming(pk("addon", "vendor-sidecar", "2026.2.0"), second))
	c := podsWith(t, r, ran("a", testkit.VendorRef, testkit.Digest2), ran("b", second, testkit.Digest2))
	first := map[string]string{ImageKey(testkit.VendorRef): testkit.Digest1, ImageKey(second): testkit.Digest1}
	got := FindingsOf(c, first)
	if len(got) != 1 || got[0].Kind != ImageMoved || !slices.Equal(got[0].Packs, []string{testkit.VendorPack, "addon/vendor-sidecar"}) ||
		!slices.Equal(got[0].Images, []string{testkit.VendorRef, second}) {
		t.Errorf("findings %+v", got)
	}
}

func TestTheFirstDigestIsTheOneOfMostPodsAndTheSmallerTextWinsATie(t *testing.T) {
	one := FirstDigests([]kube.Container{ran("a", testkit.VendorRef, testkit.Digest2), ran("b", testkit.VendorRef, testkit.Digest1), ran("c", testkit.VendorRef, testkit.Digest1)}, nil)
	if one[ImageKey(testkit.VendorRef)] != testkit.Digest1 {
		t.Errorf("most pods: %v", one)
	}
	tie := FirstDigests([]kube.Container{ran("a", testkit.VendorRef, testkit.Digest2), ran("b", testkit.VendorRef, testkit.Digest1)}, nil)
	if tie[ImageKey(testkit.VendorRef)] != testkit.Digest1 {
		t.Errorf("tie: %v", tie)
	}
	known := FirstDigests([]kube.Container{ran("a", testkit.VendorRef, testkit.Digest2)}, firstOf(testkit.VendorRef, testkit.Digest1))
	if len(known) != 0 {
		t.Errorf("an image that the state holds gets no new first digest: %v", known)
	}
	none := FirstDigests([]kube.Container{{Pod: "ns/a", Image: testkit.VendorRef}}, nil)
	if len(none) != 0 {
		t.Errorf("a container that never started is no first digest: %v", none)
	}
}

func TestTheStateKeepsAnIdOfTheImageAndNeverItsReference(t *testing.T) {
	id := ImageKey(testkit.VendorRef)
	if len(id) != 16 || strings.Contains(id, "vendor") || id != ImageKey(testkit.VendorRef) || id == ImageKey(testkit.VendorRef+"x") {
		t.Errorf("id %q", id)
	}
	if id == FindingID(ImageMoved, testkit.VendorRef) {
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
	if o := ObservedOf(podsInput(&failed, nil)); o.Pods != PodsUnread || o.Error == nil || o.Error.Status != 403 {
		t.Errorf("failed: %+v", o)
	}
	c := podsOf(t, ran("a", testkit.VendorRef, testkit.Digest1))
	o := ObservedOf(podsInput(&c, nil))
	if o.Pods != PodsRead || o.ReadAt == nil || !o.ReadAt.Equal(t0) || o.Counts.Images != 1 || o.Counts.Matched != 1 || o.Counts.Changed != 0 {
		t.Errorf("read: %+v", o)
	}
}

func TestAFailedReadKeepsTheLastGoodContainersAndHoldsTheFindings(t *testing.T) {
	first := firstOf(testkit.VendorRef, testkit.Digest1)
	last := podsOf(t, ran("a", testkit.VendorRef, testkit.Digest2))
	now := podsOf(t)
	now.Pods = PodRead{Err: &ErrorRef{Kind: docs.KindKubeTransport}}
	now.Carry(&last)
	if now.Pods.Err == nil || !now.Pods.read() || !now.Pods.At.Equal(t0) || len(now.Pods.Containers) != 1 {
		t.Fatalf("pods %+v", now.Pods)
	}
	if got := kindsOf(FindingsOf(now, first)); !slices.Equal(got, []Kind{ImageMoved}) {
		t.Errorf("findings %v", got)
	}
	o := ObservedOf(podsInput(&now, first))
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
		t.Errorf(testkit.ProfileCounts, s.Profiles[0].Status, s.Counts)
	}
	ok := podsOf(t, ran("a", testkit.VendorRef, testkit.Digest1))
	if got := build(&ok, nil).NotChecked; len(got) != 0 {
		t.Errorf("a read that worked: %+v", got)
	}
}

func TestAnImageFindingIsNotADriftOfTheContent(t *testing.T) {
	c := podsOf(t, ran("a", testkit.VendorRef, testkit.Digest2))
	in := podsInput(&c, firstOf(testkit.VendorRef, testkit.Digest1))
	s := Build(in)
	if len(s.Findings) != 1 || s.Findings[0].Kind != ImageMoved || s.Counts.Findings != 1 {
		t.Fatalf(testkit.FindingsCounts, s.Findings, s.Counts)
	}
	if s.Counts.Drift != 0 || s.Profiles[0].Status != StatusOK || s.Profiles[0].Packs[0].Status != StatusOK {
		t.Errorf("profile %q, pack %q, counts %+v", s.Profiles[0].Status, s.Profiles[0].Packs[0].Status, s.Counts)
	}
	if len(s.Events) != 1 || s.Events[0].Reason != "ImageMoved" || s.Events[0].Type != EventDrift {
		t.Errorf("events %+v", s.Events)
	}
}

func TestNoAnswerOfThePodsCarriesAPodNameOrANamespace(t *testing.T) {
	marker := kube.Container{Pod: "POD-NAMESPACE-MARKER/POD-NAME-MARKER", Image: testkit.VendorRef, ImageID: testkit.VendorRef + "@" + testkit.Digest2}
	c := podsOf(t, marker)
	in := podsInput(&c, firstOf(testkit.VendorRef, testkit.Digest1))
	for name, v := range map[string]any{"summary": Build(in), "observed": ObservedOf(in)} {
		raw := testkit.MustJSON(t, v)
		testkit.HoldsNone(t, "the "+name, string(raw), "MARKER")
	}
	o := ObservedOf(in)
	view, ok := o.ImageOf(testkit.VendorRef)
	if !ok || view.Image.Ref != testkit.VendorRef || view.Schema != ObservedSchema {
		t.Errorf("view %+v", view)
	}
	if _, ok := o.ImageOf("nope"); ok {
		t.Error("an unknown reference has a view")
	}
}

func TestTheRowsAreSortedByStatusAndThenByReference(t *testing.T) {
	steady, moved, split := "a/steady:1", "z/moved:1", "m/split:1"
	c := podsOf(t, ran("a", steady, testkit.Digest1), ran("b", moved, testkit.Digest2), ran("c", split, testkit.Digest1), ran("d", split, testkit.Digest2), kube.Container{Pod: "ns/e", Image: "b/never:1"})
	first := map[string]string{ImageKey(steady): testkit.Digest1, ImageKey(moved): testkit.Digest1, ImageKey(split): testkit.Digest1}
	var refs []string
	for _, img := range imageRows(c, first) {
		refs = append(refs, img.Ref)
	}
	if want := []string{split, moved, steady, "b/never:1"}; !slices.Equal(refs, want) {
		t.Errorf("refs %v, want %v", refs, want)
	}
}
