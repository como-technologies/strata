package check

import (
	"context"
	"slices"
	"strings"
	"sync"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/kube"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// fakePods answers the read of the pods with a list or an error, and counts the calls.
type fakePods struct {
	mu         sync.Mutex
	containers []kube.Container
	err        error
	calls      int
}

func (f *fakePods) Pods(context.Context) ([]kube.Container, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.calls++
	return f.containers, f.err
}

func (f *fakePods) set(containers []kube.Container, err error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.containers, f.err = containers, err
}

// runs makes a container of a pod that runs the image of the records with the digest.
func runs(pod, digest string) kube.Container {
	return kube.Container{Pod: "ns/" + pod, Image: testkit.ImageRef, ImageID: testkit.ImageName + "@" + digest}
}

// podRig is a world with one record whose two packs name a/b:1, an observer that reads the pods, and
// the fake of the pods.
func podRig(t *testing.T, containers ...kube.Container) (*rig, *world, *Observer, *fakePods) {
	t.Helper()
	r, w := oneRecord(t, unchanged())
	pods := &fakePods{containers: containers}
	return r, w, r.observer(w).WithPods(pods), pods
}

// cycle runs a content cycle of o, and fails the test when it fails.
func cycle(t *testing.T, o *Observer) {
	t.Helper()
	if err := o.RunContent(context.Background()); err != nil {
		t.Fatalf("the content cycle failed: %v", err)
	}
}

// versions runs a versions cycle of o, and fails the test when it fails.
func versions(t *testing.T, o *Observer) {
	t.Helper()
	if err := o.RunVersions(context.Background()); err != nil {
		t.Fatalf("the versions cycle failed: %v", err)
	}
}

// once runs a content cycle and a versions cycle of o, and gives the summary. It fails the test when a cycle fails.
func once(t *testing.T, o *Observer) model.Summary {
	t.Helper()
	s, err := o.Once(context.Background())
	if err != nil {
		t.Fatalf(testkit.CyclesFailed, err)
	}
	return s
}

func imageKindsOf(s model.Summary) []model.Kind {
	var kinds []model.Kind
	for _, f := range s.Findings {
		kinds = append(kinds, f.Kind)
	}
	return kinds
}

func readState(t *testing.T, r *rig) State {
	t.Helper()
	s, err := loadState(r.state)
	if err != nil {
		t.Fatal(err)
	}
	return s
}

// stateText gives the text of the state file.
func stateText(t *testing.T, r *rig) string {
	t.Helper()
	return testkit.StateText(t, r.state)
}

// stateHoldsNone fails when the state file holds one of the words.
func stateHoldsNone(t *testing.T, r *rig, words ...string) {
	t.Helper()
	testkit.HoldsNone(t, "the state", stateText(t, r), words...)
}

func TestWithoutPodsTheObserverReadsNoPodAndTheDocumentIsOff(t *testing.T) {
	r, w := oneRecord(t, unchanged())
	o := r.observer(w)
	cycle(t, o)
	if got := o.Observed(); got.Pods != model.PodsOff || len(got.Images) != 0 {
		t.Errorf("document %+v", got)
	}
	if st := readState(t, r); len(st.Images.First) != 0 {
		t.Errorf("state %+v", st.Images)
	}
	if causes := o.Summary().NotChecked; len(causes) != 0 {
		t.Errorf("notChecked %+v", causes)
	}
}

func TestAGoodReadFillsTheDocumentAndTheStateKeepsTheFirstDigestByIdOnly(t *testing.T) {
	r, _, o, pods := podRig(t, runs("web-1", testkit.Digest1), runs("web-2", testkit.Digest1))
	cycle(t, o)
	if pods.calls != 1 {
		t.Errorf("%d reads in one cycle", pods.calls)
	}
	doc := o.Observed()
	if doc.Pods != model.PodsRead || doc.ReadAt == nil || doc.Counts.Images != 1 || doc.Counts.Matched != 1 || doc.Counts.Changed != 0 ||
		len(doc.Images) != 1 || doc.Images[0].Status != model.ImageStatusSteady || doc.Images[0].Digests[0].Pods != 2 {
		t.Fatalf("document %+v", doc)
	}
	st := readState(t, r)
	if st.Images.Revision != testkit.Revision || st.Images.First[model.ImageKey(testkit.ImageRef)] != testkit.Digest1 || len(st.Images.First) != 1 {
		t.Errorf("state %+v", st.Images)
	}
	stateHoldsNone(t, r, testkit.ImageRef, "web-1", "ns/")
}

func TestADigestOtherThanTheFirstIsImageMovedUntilTheDigestComesBack(t *testing.T) {
	_, _, o, pods := podRig(t, runs("web-1", testkit.Digest1))
	cycle(t, o)
	if got := imageKindsOf(o.Summary()); len(got) != 0 {
		t.Fatalf("the first sight makes findings %v", got)
	}
	pods.set([]kube.Container{runs("web-1", testkit.Digest2)}, nil)
	cycle(t, o)
	s := o.Summary()
	if got := imageKindsOf(s); !slices.Equal(got, []model.Kind{model.ImageMoved}) || s.Counts.Findings != 1 || s.Counts.Drift != 0 {
		t.Fatalf("findings %v, counts %+v", got, s.Counts)
	}
	if f := s.Findings[0]; f.Profile != testkit.AWSStackProfile || !slices.Equal(f.Packs, []string{"csi/csi-aws-ebs", "os/ubuntu"}) || !slices.Equal(f.Images, []string{testkit.ImageRef}) {
		t.Errorf("finding %+v", f)
	}
	if doc := o.Observed(); doc.Counts.Changed != 1 || doc.Images[0].Status != model.ImageStatusMoved || doc.Images[0].Digests[0].Status != model.AnotherDigest {
		t.Errorf("document %+v", doc)
	}
	pods.set([]kube.Container{runs("web-1", testkit.Digest1)}, nil)
	cycle(t, o)
	if got := imageKindsOf(o.Summary()); len(got) != 0 {
		t.Errorf("the digest came back, and the findings are %v", got)
	}
}

func TestTwoDigestsAtOneTimeAreImageSplit(t *testing.T) {
	_, _, o, pods := podRig(t, runs("web-1", testkit.Digest1))
	cycle(t, o)
	pods.set([]kube.Container{runs("web-1", testkit.Digest1), runs("web-2", testkit.Digest2)}, nil)
	cycle(t, o)
	if got := imageKindsOf(o.Summary()); !slices.Equal(got, []model.Kind{model.ImageSplit}) {
		t.Errorf("findings %v", got)
	}
	if doc := o.Observed(); doc.Images[0].Status != model.ImageStatusSplit || len(doc.Images[0].Digests) != 2 {
		t.Errorf("document %+v", doc)
	}
}

func TestANewRevisionOfTheRecordsResetsTheFirstDigest(t *testing.T) {
	r, w, o, pods := podRig(t, runs("web-1", testkit.Digest1))
	cycle(t, o)
	pods.set([]kube.Container{runs("web-1", testkit.Digest2)}, nil)
	cycle(t, o)
	if got := imageKindsOf(o.Summary()); !slices.Equal(got, []model.Kind{model.ImageMoved}) {
		t.Fatalf("findings %v", got)
	}
	r.meta(testkit.ApprovedMeta(testkit.NewRevision))
	cycle(t, o)
	if got := imageKindsOf(o.Summary()); len(got) != 0 {
		t.Errorf("a new revision leaves the findings %v", got)
	}
	st := readState(t, r)
	if st.Images.Revision != testkit.NewRevision || st.Images.First[model.ImageKey(testkit.ImageRef)] != testkit.Digest2 {
		t.Errorf("state %+v", st.Images)
	}
	_ = w
}

func TestARestartKeepsTheFirstDigestsOfTheState(t *testing.T) {
	r, w, o, pods := podRig(t, runs("web-1", testkit.Digest1))
	cycle(t, o)
	pods.set([]kube.Container{runs("web-1", testkit.Digest2)}, nil)
	again := r.restart(w).WithPods(pods)
	cycle(t, again)
	if got := imageKindsOf(again.Summary()); !slices.Equal(got, []model.Kind{model.ImageMoved}) {
		t.Errorf("findings %v", got)
	}
}

func TestAnImageWithNoRepoDigestAndAContainerThatNeverStartedReadNotCheckedAndMakeNoFinding(t *testing.T) {
	local := kube.Container{Pod: "ns/a", Image: testkit.ImageRef, ImageID: "sha256:" + strings.Repeat("e", 64)}
	never := kube.Container{Pod: "ns/b", Image: testkit.ImageRef}
	_, _, o, _ := podRig(t, local, never)
	cycle(t, o)
	doc := o.Observed()
	if doc.Images[0].Status != model.ImageStatusNotChecked || o.Summary().Counts.Findings != 0 {
		t.Errorf("document %+v", doc)
	}
}

func TestAFailedReadGivesTheCausePodsNotReadAndTheCycleEndsWell(t *testing.T) {
	r, _, o, pods := podRig(t)
	pods.set(nil, &kube.Error{Kind: docs.KindKubeAPI, Status: 403})
	cycle(t, o)
	s := o.Summary()
	if s.Content.FailedRuns != 0 || s.Content.LastError != nil || s.Content.CheckedAt == nil {
		t.Errorf("the content cycle failed: %+v", s.Content)
	}
	if len(s.NotChecked) != 1 || s.NotChecked[0].Cause != model.PodsNotRead || len(s.NotChecked[0].Profiles) != 0 {
		t.Errorf("notChecked %+v", s.NotChecked)
	}
	if s.Profiles[0].Status != model.StatusOK {
		t.Errorf("the profile reads %q", s.Profiles[0].Status)
	}
	doc := o.Observed()
	if doc.Pods != model.PodsUnread || doc.Error == nil || doc.Error.Kind != docs.KindKubeAPI || doc.Error.Status != 403 || len(doc.Images) != 0 {
		t.Errorf("document %+v", doc)
	}
	logged := r.log.String()
	if !strings.Contains(logged, docs.KindKubeAPI) || strings.Contains(logged, "forbidden") {
		t.Errorf("log %s", logged)
	}
}

func TestAFailedReadKeepsTheLastGoodImagesAndTheFindings(t *testing.T) {
	r, w, o, pods := podRig(t, runs("web-1", testkit.Digest1))
	cycle(t, o)
	pods.set([]kube.Container{runs("web-1", testkit.Digest2)}, nil)
	cycle(t, o)
	id := o.Summary().Findings[0].ID
	pods.set(nil, &kube.Error{Kind: docs.KindKubeTransport})
	cycle(t, o)
	s := o.Summary()
	if len(s.Findings) != 1 || s.Findings[0].ID != id || !s.Findings[0].FirstSeen.Equal(t0) {
		t.Errorf("findings %+v", s.Findings)
	}
	if doc := o.Observed(); doc.Pods != model.PodsRead || doc.Error == nil || len(doc.Images) != 1 {
		t.Errorf("document %+v", doc)
	}
	// A restart that cannot read the pods holds the first sight of the finding in the state.
	again := r.restart(w).WithPods(pods)
	cycle(t, again)
	keepsFirstSight(t, r, id)
	if doc := again.Observed(); doc.Pods != model.PodsUnread || len(doc.Images) != 0 {
		t.Errorf("document %+v", doc)
	}
}

func TestTheRealClientFeedsTheCycleAndNoValueOfAPodReachesTheOutput(t *testing.T) {
	r, w := oneRecord(t, unchanged())
	kubeAPI := testkit.MarkerPods(t)
	client := kube.New(kube.Config{Host: kubeAPI.Host, TokenFile: kubeAPI.TokenFile, CAFile: kubeAPI.CAFile})
	o := r.observer(w).WithPods(client)
	cycle(t, o)
	doc := o.Observed()
	if doc.Pods != model.PodsRead || len(doc.Images) != 3 {
		t.Fatalf("document %+v", doc)
	}
	var outputs []string
	for _, v := range []any{o.Summary(), doc} {
		raw := testkit.MustJSON(t, v)
		outputs = append(outputs, string(raw))
	}
	outputs = append(outputs, stateText(t, r), r.log.String())
	for _, out := range outputs {
		testkit.HoldsNone(t, "an output", out, "MARKER")
	}
}
