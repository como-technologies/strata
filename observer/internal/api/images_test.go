package api

import (
	"context"
	"net/http"
	"net/url"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/check"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/kube"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

func TestTheImagesRouteServesTheDocumentOfThePods(t *testing.T) {
	r := newRig(t)
	h := handlerOf(r.cycled(&fakeStrata{dir: r.fixtures}))
	rec := get(h, "/v1/images")
	var got model.Observed
	decode(t, rec, &got)
	if rec.Code != http.StatusOK || got.Schema != model.ObservedSchema || got.Pods != model.PodsRead || got.ReadAt == nil || len(got.Images) != 1 ||
		got.Images[0].Ref != "a/b:1" || got.Images[0].Match != model.MatchExact || got.Images[0].Digests[0].Digest != imageDigest || got.Counts.Images != 1 {
		t.Errorf("status %d, document %+v", rec.Code, got)
	}
}

func TestTheImageRouteTakesTheReferenceAsOneEscapedSegment(t *testing.T) {
	r := newRig(t)
	h := handlerOf(r.cycled(&fakeStrata{dir: r.fixtures}))
	for _, c := range []struct {
		path string
		code int
	}{
		{"/v1/images/" + url.PathEscape("a/b:1"), 200},
		{"/v1/images/a%2Fb:1", 200},
		{"/v1/images/a/b:1", 404},
		{"/v1/images/" + url.PathEscape("a/c:9"), 404},
		{"/v1/images/", 404},
	} {
		if rec := get(h, c.path); rec.Code != c.code {
			t.Errorf("%s gave %d, want %d: %s", c.path, rec.Code, c.code, rec.Body.String())
		}
	}
	var view model.ImageView
	decode(t, get(h, "/v1/images/a%2Fb:1"), &view)
	if view.Schema != model.ObservedSchema || view.Image.Ref != "a/b:1" || view.Pods != model.PodsRead {
		t.Errorf("view %+v", view)
	}
	if body := get(h, "/v1/images/"+url.PathEscape("SECRET-REF:1")).Body.String(); strings.Contains(body, "SECRET-REF") {
		t.Errorf("the answer of an unknown reference echoes the input: %s", body)
	}
}

func TestWithTheReadOffTheRouteSaysSoAndListsNoImage(t *testing.T) {
	r := newRig(t)
	r.pods = nil
	h := handlerOf(r.cycled(&fakeStrata{dir: r.fixtures}))
	var got model.Observed
	decode(t, get(h, "/v1/images"), &got)
	if got.Pods != model.PodsOff || got.ReadAt != nil || got.Images == nil || len(got.Images) != 0 {
		t.Errorf("document %+v", got)
	}
	if rec := get(h, "/v1/images/a%2Fb:1"); rec.Code != http.StatusNotFound {
		t.Errorf("an image with the read off gave %d", rec.Code)
	}
}

func TestAReadThatFailsShowsTheKindAndTheStatusAndNothingElse(t *testing.T) {
	r := newRig(t)
	r.pods = check.PodsFunc(func(context.Context) ([]kube.Container, error) {
		return nil, &kube.Error{Kind: docs.KindKubeAPI, Status: 403}
	})
	h := handlerOf(r.cycled(&fakeStrata{dir: r.fixtures}))
	rec := get(h, "/v1/images")
	var got model.Observed
	decode(t, rec, &got)
	if got.Pods != model.PodsUnread || got.Error == nil || got.Error.Kind != docs.KindKubeAPI || got.Error.Status != 403 || len(got.Images) != 0 {
		t.Errorf("document %+v", got)
	}
	var nc NotCheckedView
	decode(t, get(h, "/v1/notchecked"), &nc)
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
	kubeAPI := testkit.NewKubeAPI(t, func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write([]byte(testkit.PodList("", testkit.PodJSON("NS-MARKER", "POD-MARKER", "a/b:1", "a/b@"+imageDigest))))
	})
	r.pods = kube.New(kube.Config{Host: kubeAPI.Host, TokenFile: kubeAPI.TokenFile, CAFile: kubeAPI.CAFile})
	h := handlerOf(r.cycled(&fakeStrata{dir: r.fixtures}))
	if len(kubeAPI.Requests()) == 0 {
		t.Fatal("the observer did not read the pods")
	}
	for pattern, path := range summaryRoutes {
		rec := get(h, path)
		if rec.Code != http.StatusOK || strings.Contains(rec.Body.String(), "MARKER") {
			t.Errorf("%s: status %d, body %s", pattern, rec.Code, rec.Body.String())
		}
	}
	if strings.Contains(r.log.String(), "MARKER") {
		t.Errorf("the log holds a value of a pod: %s", r.log.String())
	}
}
