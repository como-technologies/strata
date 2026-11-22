package api

import (
	"context"
	"encoding/json"
	"net/http"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// The sentinel test fills the server from testdata/, with the sentinel in each Leaf.Old and each
// Leaf.New, and calls every route. Each summary route must hold no sentinel. The detail route must hold it.
//
// summaryRoutes and detailRoutes are the tables of the test. A new summary route needs a line in
// summaryRoutes, and TestEachRouteHasALineInTheSentinelTable fails until it has one.
var summaryRoutes = map[string]string{
	patternCluster:    patternCluster,
	patternFindings:   patternFindings,
	patternProfiles:   patternProfiles,
	patternProfile:    patternProfiles + "/" + testkit.AWSStackProfile,
	patternNewer:      patternNewer,
	patternNotChecked: patternNotChecked,
	patternEvents:     patternEvents,
	patternImages:     patternImages,
	patternImage:      patternImages + "/a%2Fb:1",
	patternAlerts:     patternAlerts,
}

var detailRoutes = map[string][]string{
	patternPackDiff: {pathPublished, pathCluster, pathNewer},
}

// askRoutes are the routes of the click "Ask Palette". ask_test.go holds the tests of their answers.
var askRoutes = map[string]string{
	patternAsks: patternAsks,
	patternAsk:  pathAsk,
}

// cycleRoutes are the routes of the click "Check now". cycle_test.go holds the tests of their answers.
var cycleRoutes = map[string]string{
	patternCycle: pathCycle,
}

func TestEachRouteHasALineInTheSentinelTable(t *testing.T) {
	for _, r := range (&Server{}).Routes() {
		var listed bool
		switch r.Tier {
		case TierSummary:
			_, listed = summaryRoutes[r.Pattern]
		case TierDetail:
			_, listed = detailRoutes[r.Pattern]
			if _, asked := askRoutes[r.Pattern]; asked {
				listed = true
			}
		case TierHealth:
			listed = r.Pattern == patternHealth
		case TierCycle:
			_, listed = cycleRoutes[r.Pattern]
		}
		if !listed {
			t.Errorf("the route %s of the tier %s has no line in the table of the sentinel test", r.Pattern, r.Tier)
		}
	}
	for pattern := range summaryRoutes {
		if !hasRoute(pattern, TierSummary) {
			t.Errorf("the table lists %s, and the server has no such summary route", pattern)
		}
	}
}

// Each const of testkit that copies a const of api holds the value of that const.
func TestEachCopyInTestkitHoldsTheValueOfItsConst(t *testing.T) {
	for _, c := range []struct{ got, want string }{
		{testkit.AsksRoute, patternAsks},
		{testkit.FetchSite, headerFetchSite},
		{testkit.SameOrigin, sameOrigin},
	} {
		if c.got != c.want {
			t.Errorf("testkit holds %q, and api holds %q", c.got, c.want)
		}
	}
}

func hasRoute(pattern string, tier Tier) bool {
	for _, r := range (&Server{}).Routes() {
		if r.Pattern == pattern && r.Tier == tier {
			return true
		}
	}
	return false
}

// noSummaryRouteHolds fails when a summary route does not answer 200, or its body holds one of the words.
func noSummaryRouteHolds(t *testing.T, h http.Handler, words ...string) {
	t.Helper()
	for pattern, path := range summaryRoutes {
		rec := get(h, path)
		body := rec.Body.String()
		if rec.Code != http.StatusOK {
			t.Errorf("%s: status %d, body %s", pattern, rec.Code, body)
		}
		testkit.HoldsNone(t, pattern, body, words...)
	}
}

func TestEachSummaryRouteHoldsNoSentinelAndTheDetailRouteHoldsIt(t *testing.T) {
	_, o, _ := asked(t, "")
	h := handlerOf(o)
	noSummaryRouteHolds(t, h, "SENTINEL")
	for pattern, paths := range detailRoutes {
		for _, path := range paths {
			t.Run(pattern+" "+path, func(t *testing.T) {
				rec := get(h, path)
				body := rec.Body.String()
				if rec.Code != http.StatusOK || !strings.Contains(body, sentinelValue) {
					t.Fatalf("status %d, body %s", rec.Code, body)
				}
				testkit.HoldsNone(t, "the body", body, sentinelVariable)
			})
		}
	}
}

func TestNoRouteCarriesAVariableOrTheEnvironment(t *testing.T) {
	const key = "SENTINEL-KEY"
	t.Setenv(runner.EnvKey, key)
	_, o, _ := asked(t, "")
	h := handlerOf(o)
	noSummaryRouteHolds(t, h, key, sentinelVariable)
	unknown := []string{"/debug", "/debug/vars", "/env", "/v1/env", "/v1/records", "/v1/raw"}
	paths := append([]string{patternHealth, "/"}, unknown...)
	for _, ps := range detailRoutes {
		paths = append(paths, ps...)
	}
	for _, path := range paths {
		testkit.HoldsNone(t, path, get(h, path).Body.String(), key, sentinelVariable)
	}
	for _, path := range unknown {
		if rec := get(h, path); rec.Code != http.StatusNotFound {
			t.Errorf("%s gave %d", path, rec.Code)
		}
	}
}

func TestEachRouteGivesItsStatusAndContentType(t *testing.T) {
	_, o, _ := asked(t, "")
	h := handlerOf(o)
	const jsonType, textType = testkit.JSONType, "text/plain; charset=utf-8"
	for _, c := range []struct {
		name, path string
		status     int
		contentTyp string
	}{
		{"health", patternHealth, 200, textType},
		{"cluster", summaryRoutes[patternCluster], 200, jsonType},
		{"findings", summaryRoutes[patternFindings], 200, jsonType},
		{"profiles", summaryRoutes[patternProfiles], 200, jsonType},
		{"a profile by name and version", summaryRoutes[patternProfile], 200, jsonType},
		{"a profile by name", pathProfileByName, 200, jsonType},
		{"newer", summaryRoutes[patternNewer], 200, jsonType},
		{"not checked", summaryRoutes[patternNotChecked], 200, jsonType},
		{"events", summaryRoutes[patternEvents], 200, jsonType},
		{"the diff of the published side", pathPublished, 200, jsonType},
		{"the diff of the cluster side", pathCluster, 200, jsonType},
		{"the diff of the newer side", pathNewer, 200, jsonType},
		{"an unknown profile", patternProfiles + "/nope", 404, jsonType},
		{"the diff of an unknown profile", testkit.DiffPath("nope", testkit.KubernetesKey, string(model.SideCluster)), 404, jsonType},
		{"the diff of an unknown pack", testkit.DiffPath(testkit.AWSStackProfile, testkit.NopeKey, string(model.SideCluster)), 404, jsonType},
		{"the diff of a pack with no change on the side", kubernetesDiff(string(model.SidePublished)), 404, jsonType},
		{"the diff of a pack that no pin names", testkit.DiffPath(testkit.AWSStackProfile, testkit.CSIKey, string(model.SideNewer)), 404, jsonType},
		{"the diff of " + testkit.NoNewerProfile, kubernetesDiff(string(model.SideNewest)), 404, jsonType},
		{"a diff with no side", kubernetesDiff(""), 400, jsonType},
		{"a diff with " + noSuchSide, kubernetesDiff("nope"), 400, jsonType},
		{"an unknown route", "/v1/nope", 404, jsonType},
		{"the root with no page", "/", 404, jsonType},
	} {
		t.Run(c.name, func(t *testing.T) {
			rec := get(h, c.path)
			if rec.Code != c.status || rec.Header().Get("Content-Type") != c.contentTyp {
				t.Errorf("status %d, type %q, body %q", rec.Code, rec.Header().Get("Content-Type"), rec.Body.String())
			}
		})
	}
}

func TestAnUnknownProfileOrPackGets404AndNamesNoValue(t *testing.T) {
	_, o, _ := asked(t, "")
	h := handlerOf(o)
	for _, path := range []string{
		patternProfiles + "/SENTINEL-PROFILE",
		testkit.DiffPath("SENTINEL-PROFILE", testkit.KubernetesKey, string(model.SideCluster)),
		testkit.DiffPath(testkit.AWSStackProfile, "SENTINEL-LAYER/SENTINEL-PACK", string(model.SideCluster)),
		kubernetesDiff("SENTINEL-SIDE"),
		"/SENTINEL-PATH",
	} {
		rec := get(h, path)
		if rec.Code != http.StatusNotFound && rec.Code != http.StatusBadRequest {
			t.Errorf("%s gave %d", path, rec.Code)
		}
		if body := rec.Body.String(); strings.Contains(body, "SENTINEL") || strings.TrimSpace(body) != `{"error":"`+strings.ToLower(http.StatusText(rec.Code))+`"}` {
			t.Errorf("%s: the body is %q", path, body)
		}
	}
}

func TestTheRootServesThePageAndEachOtherPathIsNotAnAnswerOfTheAPI(t *testing.T) {
	page := http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "text/html")
		_, _ = w.Write([]byte("<title>page</title>"))
	})
	h := (&Server{Source: stub{}, Page: page}).Handler()
	if rec := get(h, "/"); rec.Code != 200 || !strings.Contains(rec.Body.String(), "page") || rec.Header().Get(testkit.HeaderPolicy) == "" {
		t.Errorf("status %d, body %q, headers %v", rec.Code, rec.Body.String(), rec.Header())
	}
	if rec := get(h, "/v1/nope"); rec.Code != 404 || rec.Header().Get("Content-Type") != testkit.JSONType {
		t.Errorf("status %d, type %q", rec.Code, rec.Header().Get("Content-Type"))
	}
}

func TestTheClusterRouteGivesEachValueOfThePage(t *testing.T) {
	now := time.Date(2026, 11, 3, 14, 0, 0, 0, time.UTC)
	good := now.Add(-time.Minute)
	old := now.Add(-time.Hour)
	every := testkit.ContentEvery
	cases := []struct {
		page  model.Page
		files int
		clock model.Clock
	}{
		{model.PageNoRecords, 0, model.Clock{Every: every}},
		{model.PageLost, 1, model.Clock{Every: every, FailedRuns: 2, LastError: &model.ErrorRef{Kind: docs.KindTransport}}},
		{model.PageLost, 1, model.Clock{Every: every, FailedRuns: 2, LastError: &model.ErrorRef{Kind: docs.KindAPI, Status: 401}}},
		{model.PageStale, 1, model.Clock{Every: every, FailedRuns: 2, LastError: &model.ErrorRef{Kind: docs.KindUnknown}}},
		{model.PageStale, 1, model.Clock{Every: every, Last: &old}},
		{model.PageFirstLoad, 1, model.Clock{Every: every}},
		{model.PageOK, 1, model.Clock{Every: every, Last: &good}},
	}
	for _, c := range cases {
		t.Run(string(c.page), func(t *testing.T) {
			sum := model.Build(model.Input{Now: now, ClusterUID: testkit.PaymentsUID, StartedAt: now.Add(-time.Hour), Files: c.files, FilesKnown: true, ContentClock: c.clock})
			var got ClusterView
			rec := get(handlerOf(stub{sum}), summaryRoutes[patternCluster])
			decode(t, rec, &got)
			if got.Page != c.page {
				t.Errorf("page %q, want %q", got.Page, c.page)
			}
		})
	}
}

func TestClusterGivesTheFieldsUpToTheCounts(t *testing.T) {
	_, o, _ := asked(t, "")
	h := handlerOf(o)
	var got ClusterView
	decode(t, get(h, summaryRoutes[patternCluster]), &got)
	if got.Cluster.Name != testkit.PaymentsName || got.Page != model.PageOK || got.StrataVersion != "0.29.0" || got.Records.Count != 1 ||
		got.Records.Baseline != baseline.Approved || got.Records.Revision != testkit.Revision || got.Content.Every != "15m" || got.Versions.Every != "6h" ||
		got.Counts.Findings != 2 || got.Counts.Newer != 1 {
		t.Errorf("%+v", got)
	}
	var top map[string]json.RawMessage
	decode(t, get(h, summaryRoutes[patternCluster]), &top)
	for _, key := range []string{"findings", "profiles", "newer", "notChecked", "limits", "events"} {
		if _, ok := top[key]; ok {
			t.Errorf("/v1/cluster holds %s", key)
		}
	}
	if len(top) != 7 {
		t.Errorf("/v1/cluster holds %d fields", len(top))
	}
}

func TestTheProfileRowsHoldTheirFindingsAndWho(t *testing.T) {
	_, o, _ := asked(t, "")
	h := handlerOf(o)
	var all ProfilesView
	decode(t, get(h, summaryRoutes[patternProfiles]), &all)
	var one ProfileView
	decode(t, get(h, pathProfileByName), &one)
	if len(all.Profiles) != 1 || one.Name != "aws-stack" || one.Status != model.StatusDrift || len(one.Findings) != 2 || len(one.Who) != 1 || one.Who[0].Actor != "Betty Rubble" {
		t.Fatalf("one %+v", one)
	}
	if one.Published == nil || one.Published.RootLive != "r2" || one.ClusterCopy == nil || one.ClusterCopy.RootB != "r3" || len(one.Packs) != 2 {
		t.Errorf("the two sides: %+v", one)
	}
	var findings FindingsView
	decode(t, get(h, summaryRoutes[patternFindings]), &findings)
	if len(findings.Findings) != 2 {
		t.Errorf("findings %+v", findings)
	}
}

func TestNewerNotCheckedAndEventsGiveTheirPartsOfTheSummary(t *testing.T) {
	_, o, _ := asked(t, "")
	h := handlerOf(o)
	var newer NewerView
	decode(t, get(h, summaryRoutes[patternNewer]), &newer)
	if len(newer.Newer) != 1 || newer.Newer[0].Name != "kubernetes" || newer.Newer[0].Newest != "1.36.2" || newer.Newer[0].Impact != "may-repave" || newer.Newer[0].Fold.Settings != 2 || newer.Newer[0].Fold.Repeats != 2 || !newer.Newer[0].New {
		t.Errorf("newer %+v", newer)
	}
	var notChecked NotCheckedView
	decode(t, get(h, summaryRoutes[patternNotChecked]), &notChecked)
	if len(notChecked.NotChecked) != 1 || notChecked.NotChecked[0].Cause != model.OlderRegistryCopy || notChecked.Limits.HelmPacks != 0 || notChecked.Limits.ImagesNamedByATag != 1 || notChecked.Limits.MaskedPaths != 1 {
		t.Errorf("not checked %+v", notChecked)
	}
	var events EventsView
	decode(t, get(h, summaryRoutes[patternEvents]), &events)
	reasons := map[string]int{}
	for _, e := range events.Events {
		reasons[e.Reason]++
	}
	if reasons["DriftFound"] != 1 || reasons["ClusterDiffers"] != 1 || reasons["ProfileUpdated"] != 1 || reasons["NewVersion"] != 1 || reasons["OldCopy"] != 1 || len(events.Events) != 5 {
		t.Errorf("events %+v", events)
	}
}

func TestTheDetailRouteSplitsTheValuesInThreeLists(t *testing.T) {
	_, o, _ := asked(t, "")
	h := handlerOf(o)
	var got PackDiff
	decode(t, get(h, pathPublished), &got)
	if got.Profile != testkit.AWSStackProfile || got.Key != testkit.CSIKey || got.Side != model.SidePublished || got.Status != docs.PackChanged || got.Impact.Level != "cluster-wide" {
		t.Errorf("%+v", got)
	}
	v := got.Values
	if len(v.Settings) != 1 || v.Settings[0].Path != csiChart+replicaPath || v.Settings[0].Old != sentinelValue || v.Settings[0].New != sentinelValue ||
		len(v.FollowsTheVersion) != 1 || v.FollowsTheVersion[0].Path != csiChart+"image.tag" ||
		len(v.RepeatsTheLists) != 1 || v.RepeatsTheLists[0].Path != testkit.ImagePath || !v.Parseable {
		t.Errorf("values %+v", v)
	}
	if len(got.Flags) != 2 || got.Flags[0] != docs.FlagAuthored || got.Flags[1] != docs.FlagImages ||
		len(got.ImageChanges) != 1 || got.ImageChanges[0].Name != "ebs-csi-driver" || got.ImageChanges[0].Old != sentinelValue ||
		len(got.ChartChanges) != 0 || got.ChartChanges == nil || len(got.Masked) != 1 ||
		len(got.Manifests) != 1 || got.Manifests[0].Op != "added" || got.Manifests[0].Name != "storageclass" ||
		len(got.ParamChanges) != 1 || got.ParamChanges[0].Name != "region" {
		t.Errorf("lists %+v", got)
	}
	// The cluster side and the newer side read other diffs.
	decode(t, get(h, pathCluster), &got)
	if got.Side != model.SideCluster || len(got.Values.Settings) != 1 || got.Values.Settings[0].Path != "kubeadmconfig.apiServer.extraArgs.audit-log-maxage" {
		t.Errorf("cluster %+v", got)
	}
	decode(t, get(h, pathNewer), &got)
	if got.Side != model.SideNewer || got.Impact.Level != "may-repave" || len(got.Values.Settings) != 2 || len(got.Values.RepeatsTheLists) != 2 || len(got.Values.FollowsTheVersion) != 0 {
		t.Errorf("newer %+v", got)
	}
}

func TestTheDetailRouteAnswersWith502WhenTheCompareFails(t *testing.T) {
	r, _, _ := asked(t, "")
	// A restart: the state holds the counts of the move, the memory holds no values, and the compare fails.
	failing := &fakeStrata{dir: r.fixtures, failMoves: true}
	h := handlerOf(r.cycled(failing))
	rec := get(h, pathNewer)
	if rec.Code != http.StatusBadGateway || strings.Contains(rec.Body.String(), "SENTINEL") || failing.moveCompares() != 1 {
		t.Errorf("status %d, body %q, compares %d", rec.Code, rec.Body.String(), failing.moveCompares())
	}
	// A failed compare is not kept: the next request asks again.
	get(h, pathNewer)
	if failing.moveCompares() != 2 {
		t.Errorf("compares %d", failing.moveCompares())
	}
}

// slow is a Source whose detail call ends with the error that it is given.
type slow struct {
	stub
	err error
}

func (s slow) PackDiff(context.Context, string, string, model.Side) (docs.Pack, bool, error) {
	return docs.Pack{}, false, s.err
}

// strata writes a list item as [name=<value>], so the name of an item can be in a changed path. A path
// is a value of the cluster, and only the detail tier sends it.
func TestAMarkerInAChangedPathReachesNoSummaryRouteAndNotOnce(t *testing.T) {
	const pathMarker = "PATH-MARKER"
	r := newRig(t)
	raw := testkit.ReadFile(t, filepath.Join(r.fixtures, "verify.json"))
	planted := strings.Replace(string(raw), replicaPath, "clients[name="+pathMarker+"].id", 1)
	if planted == string(raw) {
		t.Fatal("the fixture holds no path to change")
	}
	testkit.WriteIn(t, r.fixtures, "verify.json", planted)
	o := r.observer(&fakeStrata{dir: r.fixtures})
	testkit.HoldsNone(t, "the summary of once", string(testkit.MustJSON(t, r.once(o))), pathMarker)
	h := handlerOf(o)
	noSummaryRouteHolds(t, h, pathMarker)
	// When the detail route does not hold the path, the marker tests nothing.
	testkit.HoldsAll(t, "the detail route", get(h, pathPublished).Body.String(), pathMarker)
}

func TestTheDetailRouteAnswersWith503WhenTheWaitEnds(t *testing.T) {
	_, o, _ := asked(t, "")
	for _, err := range []error{context.DeadlineExceeded, context.Canceled} {
		h := handlerOf(slow{stub{o.Summary()}, err})
		if rec := get(h, pathNewer); rec.Code != http.StatusServiceUnavailable {
			t.Errorf("%v gave %d", err, rec.Code)
		}
	}
}

func TestAnUnreadAuditLogKeepsTheFindingAndTheRoutesCarryAFlagAndNotTheText(t *testing.T) {
	r := newRig(t)
	raw := testkit.ReadFile(t, filepath.Join(r.fixtures, "verify.json"))
	// strata keeps the verdict of the drifted record, and gives the reason in place of the writes
	unread := regexp.MustCompile(`"who":\[[^\]]*\]`).ReplaceAllString(string(raw), `"whoError":"HTTP 403: `+testkit.AuditSentinel+`"`)
	if unread == string(raw) {
		t.Fatal("the fixture holds no who")
	}
	testkit.WriteIn(t, r.fixtures, "verify.json", unread)
	h := handlerOf(r.cycled(&fakeStrata{dir: r.fixtures}))
	var findings FindingsView
	decode(t, get(h, summaryRoutes[patternFindings]), &findings)
	flagged := 0
	for _, f := range findings.Findings {
		if f.Kind == model.PublishedChanged && f.WhoUnread && len(f.Who) == 0 {
			flagged++
		}
	}
	if len(findings.Findings) != 2 || flagged != 1 {
		t.Errorf("findings %+v", findings)
	}
	var notChecked NotCheckedView
	decode(t, get(h, summaryRoutes[patternNotChecked]), &notChecked)
	for _, n := range notChecked.NotChecked {
		if n.Cause == model.ReadError {
			t.Errorf("the drift is no read-error: %+v", n)
		}
	}
	var events EventsView
	decode(t, get(h, summaryRoutes[patternEvents]), &events)
	sentences := 0
	for _, e := range events.Events {
		if e.Reason == "AuditUnread" && e.Message == testkit.AuditUnread {
			sentences++
		}
	}
	if sentences != 1 {
		t.Errorf("events %+v", events)
	}
	// the log of the pod names the status and never the body of the answer
	noSummaryRouteStateOrLogHolds(t, r, h, testkit.AuditSentinel)
	testkit.HoldsAll(t, "the log", r.log.String(), `reason="HTTP 403"`)
}

// Ask is the click of the stub's Source: it waits until the time limit ends.
func (s slow) Ask(context.Context, string, string) (model.Answer, bool, error) {
	return model.Answer{}, false, s.err
}
