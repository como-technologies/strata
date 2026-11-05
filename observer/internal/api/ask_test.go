package api

import (
	"context"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/check"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
)

// click sends the POST that the page sends: a fetch from the page itself.
func click(h http.Handler, path string) *httptest.ResponseRecorder {
	return clickWith(h, path, "127.0.0.1:8080", sameOrigin)
}

// clickWith sends a POST with the Host and the Sec-Fetch-Site that the test names. An empty site sends
// no header.
func clickWith(h http.Handler, path, host, site string) *httptest.ResponseRecorder {
	req := httptest.NewRequest(http.MethodPost, path, nil)
	req.Host = host
	if site != "" {
		req.Header.Set(headerFetchSite, site)
	}
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, req)
	return rec
}

// asked makes an observer that ran its cycles, and the fake that answers it.
func asked(t *testing.T, fixture string) (*rig, *check.Observer, *fakeStrata) {
	t.Helper()
	r := newRig(t)
	fake := &fakeStrata{dir: r.fixtures, askFixture: fixture}
	return r, r.cycled(fake), fake
}

func TestTheClickAsksPaletteOnceForOneMoveAndASecondClickSendsNothing(t *testing.T) {
	_, o, fake := asked(t, askNoError)
	h := handlerOf(o)
	if body := get(h, "/v1/asks").Body.String(); !strings.Contains(body, `"asks":[]`) {
		t.Fatalf("a move that nobody asked about must read not asked: %s", body)
	}
	var first model.Answer
	rec := click(h, pathAsk)
	decode(t, rec, &first)
	if rec.Code != http.StatusOK || first.Verdict != model.VerdictNoError || first.Profile != "aws-stack@1.1.0" || first.Key != "k8s/kubernetes" || first.Newest != "1.36.2" || first.Refusals == nil {
		t.Fatalf("status %d, answer %+v", rec.Code, first)
	}
	// One question: the behind run of one record, for one pack, which asks about no other pin.
	q := fake.questions()
	if len(q) != 1 || !slices.Contains(q[0], "record-0.json") || len(q[0]) != 7 || q[0][2] != runner.FlagPin || q[0][3] != "k8s/kubernetes" || slices.Contains(q[0], runner.FlagNoImpact) {
		t.Fatalf("the questions %v", q)
	}
	var second model.Answer
	decode(t, click(h, pathAsk), &second)
	if len(fake.questions()) != 1 {
		t.Errorf("a second click sent %d questions in all", len(fake.questions()))
	}
	if second.Verdict != first.Verdict || second.Newest != first.Newest {
		t.Errorf("the second answer %+v is not the first %+v", second, first)
	}
	var list AsksView
	decode(t, get(h, "/v1/asks"), &list)
	if len(list.Asks) != 1 || list.Asks[0].Verdict != model.VerdictNoError {
		t.Errorf("the list of answers %+v", list)
	}
}

func TestARefusalHoldsTheCodeTheMessageAndTheClustersOnTheDetailTierAndOnNoOtherRoute(t *testing.T) {
	r, o, _ := asked(t, askRefuses)
	h := handlerOf(o)
	var got model.Answer
	decode(t, click(h, pathAsk), &got)
	if got.Verdict != model.VerdictRefuses || len(got.Refusals) != 1 {
		t.Fatalf("answer %+v", got)
	}
	f := got.Refusals[0]
	if f.Code != "DependencyPackUpgradeNotSupported" || !strings.Contains(f.Message, sentinelValue) || !slices.Equal(f.Clusters, []string{"payments"}) {
		t.Errorf("refusal %+v", f)
	}
	for pattern, path := range summaryRoutes {
		if body := get(h, path).Body.String(); strings.Contains(body, "DependencyPackUpgradeNotSupported") || strings.Contains(body, "SENTINEL") {
			t.Errorf("%s holds the answer: %s", pattern, body)
		}
	}
	// The answer stays in memory: the state holds none of it.
	raw, err := os.ReadFile(filepath.Join(r.state, "state.json"))
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(raw), "DependencyPackUpgradeNotSupported") || strings.Contains(string(raw), "refuses") {
		t.Errorf("the state holds the answer: %s", raw)
	}
	if strings.Contains(r.log.String(), "DependencyPackUpgradeNotSupported") || strings.Contains(r.log.String(), "SENTINEL") {
		t.Errorf("the log holds the answer:\n%s", r.log.String())
	}
}

func TestA403FromPaletteReadsNoAnswerWithItsStatusAndMakesNoFinding(t *testing.T) {
	r, o, fake := asked(t, ask403)
	h := handlerOf(o)
	before := get(h, "/v1/cluster").Body.String() + get(h, "/v1/findings").Body.String() + get(h, "/v1/events").Body.String()
	rec := click(h, pathAsk)
	var got model.Answer
	decode(t, rec, &got)
	if rec.Code != http.StatusOK || got.Verdict != model.VerdictNoAnswer || got.Status != 403 || len(got.Refusals) != 0 {
		t.Fatalf("status %d, answer %+v", rec.Code, got)
	}
	if strings.Contains(rec.Body.String(), "SENTINEL") || strings.Contains(r.log.String(), "SENTINEL") {
		t.Errorf("the body of the answer of Palette left the observer: %s\n%s", rec.Body.String(), r.log.String())
	}
	if after := get(h, "/v1/cluster").Body.String() + get(h, "/v1/findings").Body.String() + get(h, "/v1/events").Body.String(); after != before {
		t.Errorf("a failed question changed the summary:\n%s\n%s", before, after)
	}
	// A request that failed is no answer, so the next click asks again. The role can change meanwhile.
	fake.setAsk(askNoError, nil)
	decode(t, click(h, pathAsk), &got)
	if got.Verdict != model.VerdictNoError || len(fake.questions()) != 2 {
		t.Errorf("answer %+v after %d questions", got, len(fake.questions()))
	}
}

func TestARunThatFailsGivesNoAnswerWithTheStatusOfTheRun(t *testing.T) {
	_, o, fake := asked(t, askNoError)
	fake.setAsk("", &runner.RunError{Kind: docs.KindAPI, Status: 500})
	h := handlerOf(o)
	var got model.Answer
	rec := click(h, pathAsk)
	decode(t, rec, &got)
	if rec.Code != http.StatusOK || got.Verdict != model.VerdictNoAnswer || got.Status != 500 {
		t.Errorf("status %d, answer %+v", rec.Code, got)
	}
	fake.setAsk("", &runner.RunError{Kind: docs.KindTransport})
	decode(t, click(h, pathAsk), &got)
	if got.Verdict != model.VerdictNoAnswer || got.Status != 0 {
		t.Errorf("answer %+v", got)
	}
}

func TestAPostNeedsTheHeaderOfARequestFromThePageItself(t *testing.T) {
	_, o, fake := asked(t, askNoError)
	h := handlerOf(o)
	for _, site := range []string{"", "cross-site", "same-site", "none", "Same-Origin ", "same-origin; x"} {
		if rec := clickWith(h, pathAsk, "127.0.0.1:8080", site); rec.Code != http.StatusForbidden {
			t.Errorf("Sec-Fetch-Site %q gave %d", site, rec.Code)
		} else if strings.Contains(rec.Body.String(), site) && site != "" {
			t.Errorf("the refusal echoes the header: %s", rec.Body.String())
		}
	}
	if len(fake.questions()) != 0 {
		t.Errorf("a refused request sent %d questions", len(fake.questions()))
	}
	// The Host comes first: a page that reaches the port after a DNS rebinding gets 421 whatever it sends.
	if rec := clickWith(h, pathAsk, "evil.example", sameOrigin); rec.Code != http.StatusMisdirectedRequest {
		t.Errorf("a Host that is not loopback gave %d", rec.Code)
	}
	if rec := click(h, pathAsk); rec.Code != http.StatusOK || len(fake.questions()) != 1 {
		t.Errorf("the request of the page gave %d after %d questions", rec.Code, len(fake.questions()))
	}
}

func TestEachMethodButTheOneOfARouteGets405WithItsAllowHeader(t *testing.T) {
	_, o, fake := asked(t, askNoError)
	h := handlerOf(o)
	for _, route := range (&Server{}).Routes() {
		path := map[string]string{patternAsk: pathAsk, patternAsks: "/v1/asks"}[route.Pattern]
		if route.Method != http.MethodGet && path == "" {
			t.Fatalf("the route %s takes %s and the test does not know its path", route.Pattern, route.Method)
		}
		if path == "" {
			continue
		}
		other := http.MethodPost
		if route.Method == http.MethodPost {
			other = http.MethodGet
		}
		req := httptest.NewRequest(other, path, nil)
		req.Host = "127.0.0.1:8080"
		req.Header.Set(headerFetchSite, sameOrigin)
		rec := httptest.NewRecorder()
		h.ServeHTTP(rec, req)
		if rec.Code != http.StatusMethodNotAllowed || rec.Header().Get("Allow") != route.Method {
			t.Errorf("%s %s gave %d, Allow %q", other, path, rec.Code, rec.Header().Get("Allow"))
		}
	}
	// The one POST of the server is the click: a POST with the header to a route that takes GET is refused.
	if rec := clickWith(h, "/v1/cluster", "127.0.0.1:8080", sameOrigin); rec.Code != http.StatusMethodNotAllowed {
		t.Errorf("a POST to a summary route gave %d", rec.Code)
	}
	if len(fake.questions()) != 0 {
		t.Errorf("%d questions", len(fake.questions()))
	}
}

func TestTheClickAnswers404ForAProfileAPackOrAMoveThatIsNotKnownAndSendsNothing(t *testing.T) {
	_, o, fake := asked(t, askNoError)
	h := handlerOf(o)
	for _, path := range []string{
		"/v1/packs/nope@1/k8s/kubernetes/ask",
		"/v1/packs/aws-stack@1.1.0/k8s/nope/ask",
		// cert-manager is current: no move exists for it
		"/v1/packs/aws-stack@1.1.0/addon/cert-manager/ask",
		// the layer is part of the key
		"/v1/packs/aws-stack@1.1.0/addon/kubernetes/ask",
	} {
		rec := click(h, path)
		if rec.Code != http.StatusNotFound || strings.Contains(rec.Body.String(), "nope") {
			t.Errorf("%s gave %d: %s", path, rec.Code, rec.Body.String())
		}
	}
	if len(fake.questions()) != 0 {
		t.Errorf("%d questions", len(fake.questions()))
	}
}

func TestTheClickAnswers503WhenTheWaitEndsAndAnswers502WhenTheObserverFails(t *testing.T) {
	r, o, _ := asked(t, askNoError)
	for _, err := range []error{context.DeadlineExceeded, context.Canceled} {
		h := handlerOf(slow{stub{o.Summary()}, err})
		if rec := click(h, pathAsk); rec.Code != http.StatusServiceUnavailable {
			t.Errorf("%v gave %d", err, rec.Code)
		}
	}
	// A record directory that cannot be listed is a failure of the observer, and not an answer of Palette.
	if err := os.RemoveAll(r.records); err != nil {
		t.Fatal(err)
	}
	if rec := click(handlerOf(o), pathAsk); rec.Code != http.StatusBadGateway || strings.Contains(rec.Body.String(), r.records) {
		t.Errorf("status %d: %s", rec.Code, rec.Body.String())
	}
}

func TestAnAnswerIsForgottenAtARestart(t *testing.T) {
	r, o, fake := asked(t, askRefuses)
	if rec := click(handlerOf(o), pathAsk); rec.Code != http.StatusOK {
		t.Fatal(rec.Code)
	}
	if len(o.Answers()) != 1 {
		t.Fatalf("the observer holds %d answers", len(o.Answers()))
	}
	// A restart: the state is on disk and the answer is not.
	restarted := handlerOf(r.cycled(fake))
	if body := get(restarted, "/v1/asks").Body.String(); !strings.Contains(body, `"asks":[]`) {
		t.Errorf("a restart kept the answer: %s", body)
	}
}

func TestAnAnswerIsDroppedWhenThePinMovesAndTheNextClickAsksAgain(t *testing.T) {
	r, o, fake := asked(t, askNoError)
	h := handlerOf(o)
	click(h, pathAsk)
	click(h, pathAsk)
	if len(fake.questions()) != 1 {
		t.Fatalf("%d questions", len(fake.questions()))
	}
	// The registry has a newer version now, so the move is another move.
	for _, name := range []string{"behind.json", askNoError} {
		raw, err := os.ReadFile(filepath.Join(r.fixtures, name))
		if err != nil {
			t.Fatal(err)
		}
		moved := strings.ReplaceAll(string(raw), `"version":"1.36.2"`, `"version":"1.37.0"`)
		moved = strings.ReplaceAll(moved, `"to":"1.36.2"`, `"to":"1.37.0"`)
		if moved == string(raw) {
			t.Fatalf("%s holds no newer version to change", name)
		}
		if err := os.WriteFile(filepath.Join(r.fixtures, name), []byte(moved), 0o600); err != nil {
			t.Fatal(err)
		}
	}
	if err := o.RunVersions(t.Context()); err != nil {
		t.Fatal(err)
	}
	if answers := o.Answers(); len(answers) != 0 {
		t.Errorf("the observer holds %+v for a move that is gone", answers)
	}
	var got model.Answer
	decode(t, click(h, pathAsk), &got)
	if got.Newest != "1.37.0" || len(fake.questions()) != 2 {
		t.Errorf("answer %+v after %d questions", got, len(fake.questions()))
	}
}

func TestTheTextOfADiffIsOnTheDetailTierAndOnNoSummaryRouteNorInTheStateNorInTheLog(t *testing.T) {
	r, o, _ := asked(t, askRefuses)
	h := handlerOf(o)
	for _, path := range []string{pathPublished, pathCluster, pathNewer} {
		if body := get(h, path).Body.String(); !strings.Contains(body, sentinelText) {
			t.Errorf("%s holds no text: %s", path, body)
		}
	}
	click(h, pathAsk)
	for pattern, path := range summaryRoutes {
		if body := get(h, path).Body.String(); strings.Contains(body, sentinelText) {
			t.Errorf("%s holds the text: %s", pattern, body)
		}
	}
	if body := get(h, "/v1/asks").Body.String(); strings.Contains(body, sentinelText) {
		t.Errorf("the list of answers holds the text: %s", body)
	}
	raw, err := os.ReadFile(filepath.Join(r.state, "state.json"))
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(raw), sentinelText) || strings.Contains(r.log.String(), sentinelText) {
		t.Errorf("the state or the log holds the text:\n%s\n%s", raw, r.log.String())
	}
}
