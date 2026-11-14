package api

import (
	"cmp"
	"context"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/check"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// click sends the POST that the page sends: a fetch from the page itself.
func click(h http.Handler, path string) *httptest.ResponseRecorder {
	return testkit.Serve(h, http.MethodPost, path, testkit.LoopbackHost, sameOrigin)
}

// answersOf gives the list of answers that the server holds.
func answersOf(t *testing.T, h http.Handler) AsksView {
	t.Helper()
	var list AsksView
	decode(t, get(h, askRoutes[patternAsks]), &list)
	return list
}

// The failures of the checks of an answer, of an answer after the next click, and of the list of answers.
const (
	statusAndAnswer = "status %d, answer %+v"
	answerAfter     = "answer %+v after %d questions"
	listOfAnswers   = "the list of answers %+v"
)

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
	if body := get(h, askRoutes[patternAsks]).Body.String(); !strings.Contains(body, testkit.NoAnswers) {
		t.Fatalf("a move that nobody asked about must read not asked: %s", body)
	}
	var first model.Answer
	rec := click(h, pathAsk)
	decode(t, rec, &first)
	if rec.Code != http.StatusOK || first.Verdict != model.VerdictNoError || first.Profile != testkit.AWSStackProfile || first.Key != testkit.KubernetesKey || first.Newest != "1.36.2" || first.Refusals == nil {
		t.Fatalf(statusAndAnswer, rec.Code, first)
	}
	// One question: the behind run of one record, for one pack, which asks about no other pin.
	q := fake.questions()
	if len(q) != 1 || !slices.Contains(q[0], "record-0.json") || len(q[0]) != 7 || q[0][2] != runner.FlagPin || q[0][3] != testkit.KubernetesKey || slices.Contains(q[0], runner.FlagNoImpact) {
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
	list := answersOf(t, h)
	if len(list.Asks) != 1 || list.Asks[0].Verdict != model.VerdictNoError {
		t.Errorf(listOfAnswers, list)
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
	if f.Code != testkit.CodeDependency || !strings.Contains(f.Message, sentinelValue) || !slices.Equal(f.Clusters, []string{testkit.PaymentsName}) {
		t.Fatalf("refusal %+v", f)
	}
	noSummaryRouteStateOrLogHolds(t, r, h, f.Code, "SENTINEL")
	// The answer stays in memory: the state holds none of it. The log names the verdict of each answer.
	testkit.HoldsNone(t, "the state", r.stateText(), string(model.VerdictRefuses))
}

// noSummaryRouteStateOrLogHolds fails when a summary route, the state, or the log holds one of the words.
func noSummaryRouteStateOrLogHolds(t *testing.T, r *rig, h http.Handler, words ...string) {
	t.Helper()
	noSummaryRouteHolds(t, h, words...)
	testkit.HoldsNone(t, "the state", r.stateText(), words...)
	testkit.HoldsNone(t, "the log", r.log.String(), words...)
}

// Issue #867. The reply of the click and the list of answers hold the checks of the update that strata gives
// beside the answer of Palette. They hold names of a pack and of a variable, so no summary route, no state, and
// no log holds them.
func TestTheChecksOfStrataReachTheDetailTierBesideTheAnswerAndNoOtherRoute(t *testing.T) {
	r, o, _ := asked(t, askChecks)
	h := handlerOf(o)
	var got model.Answer
	decode(t, click(h, pathAsk), &got)
	if got.UpdateChecks == nil {
		t.Fatalf("the answer holds no check: %+v", got)
	}
	skip, variables := got.UpdateChecks.SkippedMinor, got.UpdateChecks.RequiredVariables
	if got.Verdict != model.VerdictNoError || got.Newest != "1.37.0" || skip == nil || variables == nil {
		t.Fatalf("answer %+v", got)
	}
	if skip.Code != testkit.CodeMinorSkip || skip.From != "1.35.3" || skip.To != "1.37.0" || skip.NextMinor != "1.36" || skip.NextStep != "1.36.2" {
		t.Fatalf("the skip %+v", skip)
	}
	if variables.Code != testkit.CodeVariable || !slices.Equal(variables.Names, []string{"platform_owner"}) {
		t.Fatalf("the variables %+v", variables)
	}
	list := answersOf(t, h)
	if len(list.Asks) != 1 || !reflect.DeepEqual(list.Asks[0].UpdateChecks, got.UpdateChecks) {
		t.Errorf(listOfAnswers, list)
	}
	noSummaryRouteStateOrLogHolds(t, r, h, skip.Code, variables.Code, variables.Names[0], "nextStep")
}

func TestA403FromPaletteReadsNoAnswerWithItsStatusAndMakesNoFinding(t *testing.T) {
	r, o, fake := asked(t, ask403)
	h := handlerOf(o)
	before := get(h, summaryRoutes[patternCluster]).Body.String() + get(h, summaryRoutes[patternFindings]).Body.String() + get(h, summaryRoutes[patternEvents]).Body.String()
	rec := click(h, pathAsk)
	var got model.Answer
	decode(t, rec, &got)
	if rec.Code != http.StatusOK || got.Verdict != model.VerdictNoAnswer || got.Status != 403 || len(got.Refusals) != 0 {
		t.Fatalf(statusAndAnswer, rec.Code, got)
	}
	if strings.Contains(rec.Body.String(), "SENTINEL") || strings.Contains(r.log.String(), "SENTINEL") {
		t.Errorf("the body of the answer of Palette left the observer: %s\n%s", rec.Body.String(), r.log.String())
	}
	if after := get(h, summaryRoutes[patternCluster]).Body.String() + get(h, summaryRoutes[patternFindings]).Body.String() + get(h, summaryRoutes[patternEvents]).Body.String(); after != before {
		t.Errorf("a failed question changed the summary:\n%s\n%s", before, after)
	}
	// A request that failed is no answer, so the next click asks again. The role can change meanwhile.
	fake.setAsk(askNoError, nil)
	decode(t, click(h, pathAsk), &got)
	if got.Verdict != model.VerdictNoError || len(fake.questions()) != 2 {
		t.Errorf(answerAfter, got, len(fake.questions()))
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
		t.Errorf(statusAndAnswer, rec.Code, got)
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
	for _, site := range []string{"", "cross-site", "same-site", "none", "Same-Origin ", testkit.SameOrigin + "; x"} {
		if rec := testkit.Serve(h, http.MethodPost, pathAsk, testkit.LoopbackHost, site); rec.Code != http.StatusForbidden {
			t.Errorf(testkit.FetchSite+" %q gave %d", site, rec.Code)
		} else if strings.Contains(rec.Body.String(), site) && site != "" {
			t.Errorf("the refusal echoes the header: %s", rec.Body.String())
		}
	}
	if len(fake.questions()) != 0 {
		t.Errorf("a refused request sent %d questions", len(fake.questions()))
	}
	// The Host comes first: a page that reaches the port after a DNS rebinding gets 421 whatever it sends.
	if rec := testkit.Serve(h, http.MethodPost, pathAsk, "evil.example", sameOrigin); rec.Code != http.StatusMisdirectedRequest {
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
		path := cmp.Or(askRoutes[route.Pattern], cycleRoutes[route.Pattern])
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
		rec := testkit.Serve(h, other, path, testkit.LoopbackHost, sameOrigin)
		refusedWithAllow(t, rec, other+" "+path, route.Method)
	}
	// The POSTs of the server are the two clicks: a POST with the header to a route that takes GET is refused.
	refusedWithAllow(t, click(h, summaryRoutes[patternCluster]), "a POST to a summary route", http.MethodGet)
	if len(fake.questions()) != 0 {
		t.Errorf("%d questions", len(fake.questions()))
	}
}

func TestTheClickAnswers404ForAProfileAPackOrAMoveThatIsNotKnownAndSendsNothing(t *testing.T) {
	_, o, fake := asked(t, askNoError)
	h := handlerOf(o)
	for _, path := range []string{
		"/v1/packs/nope@1/" + testkit.KubernetesKey + "/ask",
		awsPacks + "/k8s/nope/ask",
		// cert-manager is current: no move exists for it
		awsPacks + "/addon/cert-manager/ask",
		// the layer is part of the key
		awsPacks + "/addon/kubernetes/ask",
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
	if body := get(restarted, askRoutes[patternAsks]).Body.String(); !strings.Contains(body, testkit.NoAnswers) {
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
	for _, name := range []string{testkit.BehindDoc, askNoError} {
		raw, err := os.ReadFile(filepath.Join(r.fixtures, name))
		if err != nil {
			t.Fatal(err)
		}
		moved := strings.ReplaceAll(string(raw), `"version":"1.36.2"`, `"version":"1.37.0"`)
		moved = strings.ReplaceAll(moved, `"to":"1.36.2"`, `"to":"1.37.0"`)
		if moved == string(raw) {
			t.Fatalf("%s holds no newer version to change", name)
		}
		testkit.WriteIn(t, r.fixtures, name, moved)
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
		t.Errorf(answerAfter, got, len(fake.questions()))
	}
}

func TestTheTextOfADiffIsOnTheDetailTierAndOnNoSummaryRouteNorInTheStateNorInTheLog(t *testing.T) {
	r, o, _ := asked(t, askRefuses)
	h := handlerOf(o)
	for _, path := range detailRoutes[patternPackDiff] {
		if body := get(h, path).Body.String(); !strings.Contains(body, sentinelText) {
			t.Errorf("%s holds no text: %s", path, body)
		}
	}
	click(h, pathAsk)
	if body := get(h, askRoutes[patternAsks]).Body.String(); strings.Contains(body, sentinelText) {
		t.Errorf("the list of answers holds the text: %s", body)
	}
	noSummaryRouteStateOrLogHolds(t, r, h, sentinelText)
}

func TestNoCycleAsksPaletteAndTheClickDoesThroughTheRunnerWithOnePin(t *testing.T) {
	// The script keeps the command line of each call in calls.log. The real runner runs it, so a call that the runner
	// refuses does not reach the log.
	r := newRig(t)
	log := filepath.Join(r.fixtures, "calls.log")
	o := r.cycled(scripted(t, r, log, askRefuses))
	lines := func() []string {
		raw, err := os.ReadFile(log)
		if err != nil {
			t.Fatal(err)
		}
		return strings.Split(strings.TrimSpace(string(raw)), "\n")
	}
	cycles := lines()
	if len(cycles) < 5 {
		t.Fatalf("the cycles ran %d commands: %v", len(cycles), cycles)
	}
	var behind int
	for _, line := range cycles {
		switch {
		case strings.HasPrefix(line, "behind "):
			behind++
			testkit.HoldsAll(t, "the line", line, " --no-impact ")
			testkit.HoldsNone(t, "the line", line, "--pin")
		case strings.HasPrefix(line, "compare "), strings.HasPrefix(line, "verify "):
			if !strings.Contains(line, " --full ") || strings.Contains(line, "--validate") || strings.Contains(line, "--repave") {
				t.Errorf("a cycle ran %q", line)
			}
		}
	}
	if behind != 1 {
		t.Errorf("the versions cycle ran %d behind", behind)
	}
	if rec := click(handlerOf(o), pathAsk); rec.Code != http.StatusOK || !strings.Contains(rec.Body.String(), testkit.RefusesVerdict) {
		t.Fatalf("status %d: %s", rec.Code, rec.Body.String())
	}
	after := lines()[len(cycles):]
	if len(after) != 1 || after[0] != "behind record-0.json --pin "+testkit.KubernetesKey+" --json --compact -q" {
		t.Errorf("the click ran %v", after)
	}
}
