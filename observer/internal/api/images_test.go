package api

import (
	"context"
	"net/http"
	"net/url"
	"testing"

	"github.com/como-technologies/strata/observer/internal/check"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/kube"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

func TestTheImagesRouteServesTheDocumentOfThePods(t *testing.T) {
	_, o, _ := asked(t, "")
	h := handlerOf(o)
	rec := get(h, summaryRoutes[PatternImages])
	var got model.Observed
	decode(t, rec, &got)
	if rec.Code != http.StatusOK || got.Schema != model.ObservedSchema || got.Pods != model.PodsRead || got.ReadAt == nil || len(got.Images) != 1 ||
		got.Images[0].Ref != testkit.ImageRef || got.Images[0].Match != model.MatchExact || got.Images[0].Digests[0].Digest != testkit.Digest1 || got.Counts.Images != 1 {
		t.Errorf("status %d, document %+v", rec.Code, got)
	}
}

func TestTheImageRouteTakesTheReferenceAsOneEscapedSegment(t *testing.T) {
	_, o, _ := asked(t, "")
	h := handlerOf(o)
	for _, c := range []struct {
		path string
		code int
	}{
		{PatternImages + "/" + url.PathEscape(testkit.ImageRef), 200},
		{summaryRoutes[patternImage], 200},
		{PatternImages + "/" + testkit.ImageRef, 404},
		{PatternImages + "/" + url.PathEscape("a/c:9"), 404},
		{PatternImages + "/", 404},
	} {
		if rec := get(h, c.path); rec.Code != c.code {
			t.Errorf("%s gave %d, want %d: %s", c.path, rec.Code, c.code, rec.Body.String())
		}
	}
	var view model.ImageView
	decode(t, get(h, summaryRoutes[patternImage]), &view)
	if view.Schema != model.ObservedSchema || view.Image.Ref != testkit.ImageRef || view.Pods != model.PodsRead {
		t.Errorf("view %+v", view)
	}
	testkit.HoldsNone(t, "the answer of an unknown reference", get(h, PatternImages+"/"+url.PathEscape("SECRET-REF:1")).Body.String(), "SECRET-REF")
}

func TestWithTheReadOffTheRouteSaysSoAndListsNoImage(t *testing.T) {
	r := newRig(t)
	r.pods = nil
	h := handlerOf(r.cycled(&fakeStrata{dir: r.fixtures}))
	var got model.Observed
	decode(t, get(h, summaryRoutes[PatternImages]), &got)
	if got.Pods != model.PodsOff || got.ReadAt != nil || got.Images == nil || len(got.Images) != 0 {
		t.Errorf("document %+v", got)
	}
	if rec := get(h, summaryRoutes[patternImage]); rec.Code != http.StatusNotFound {
		t.Errorf("an image with the read off gave %d", rec.Code)
	}
}

func TestAReadThatFailsShowsTheKindAndTheStatusAndNothingElse(t *testing.T) {
	r := newRig(t)
	r.pods = check.PodsFunc(func(context.Context) ([]kube.Container, error) {
		return nil, &kube.Error{Kind: docs.KindKubeAPI, Status: 403}
	})
	h := handlerOf(r.cycled(&fakeStrata{dir: r.fixtures}))
	rec := get(h, summaryRoutes[PatternImages])
	var got model.Observed
	decode(t, rec, &got)
	if got.Pods != model.PodsUnread || got.Error == nil || got.Error.Kind != docs.KindKubeAPI || got.Error.Status != 403 || len(got.Images) != 0 {
		t.Errorf("document %+v", got)
	}
	var nc NotCheckedView
	decode(t, get(h, summaryRoutes[PatternNotChecked]), &nc)
	found := false
	for _, row := range nc.NotChecked {
		found = found || row.Cause == model.PodsNotRead
	}
	if !found {
		t.Errorf("notChecked %+v", nc.NotChecked)
	}
}

func TestNoRouteCarriesAValueOfAPodThatTheAPIServerSends(t *testing.T) {
	r := newRig(t)
	kubeAPI := testkit.MarkerPods(t)
	r.pods = kube.New(kube.Config{Host: kubeAPI.Host, TokenFile: kubeAPI.TokenFile, CAFile: kubeAPI.CAFile})
	h := handlerOf(r.cycled(&fakeStrata{dir: r.fixtures}))
	if len(kubeAPI.Requests()) == 0 {
		t.Fatal("the observer did not read the pods")
	}
	noSummaryRouteStateOrLogHolds(t, r, h, "MARKER")
}
