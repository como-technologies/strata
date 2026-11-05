package api

import (
	"context"
	"encoding/json"
	"net/http"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
)

// The sentinel test fills the server from testdata/, with the sentinel in each Leaf.Old and each
// Leaf.New, and calls every route. Each summary route must hold no sentinel. The detail route must hold it.
//
// summaryRoutes and detailRoutes are the tables of the test. A new summary route needs a line in
// summaryRoutes, and TestEachRouteHasALineInTheSentinelTable fails until it has one.
var summaryRoutes = map[string]string{
	patternCluster:    "/v1/cluster",
	patternFindings:   "/v1/findings",
	patternProfiles:   "/v1/profiles",
	patternProfile:    "/v1/profiles/aws-stack@1.1.0",
	patternNewer:      "/v1/newer",
	patternNotChecked: "/v1/notchecked",
	patternEvents:     "/v1/events",
	patternImages:     "/v1/images",
	patternImage:      "/v1/images/a%2Fb:1",
}

var detailRoutes = map[string][]string{
	patternPackDiff: {pathPublished, pathCluster, pathNewer},
}

// askRoutes are the routes of the click "Ask Palette". ask_test.go holds the tests of their answers.
var askRoutes = map[string]string{
	patternAsks: "/v1/asks",
	patternAsk:  pathAsk,
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

func hasRoute(pattern string, tier Tier) bool {
	for _, r := range (&Server{}).Routes() {
		if r.Pattern == pattern && r.Tier == tier {
			return true
		}
	}
	return false
}

func TestEachSummaryRouteHoldsNoSentinelAndTheDetailRouteHoldsIt(t *testing.T) {
	r := newRig(t)
	h := handlerOf(r.cycled(&fakeStrata{dir: r.fixtures}))
	for pattern, path := range summaryRoutes {
		t.Run(pattern, func(t *testing.T) {
			rec := get(h, path)
			if rec.Code != http.StatusOK {
				t.Fatalf("status %d, body %s", rec.Code, rec.Body.String())
			}
			if body := rec.Body.String(); strings.Contains(body, "SENTINEL") {
				t.Errorf("the summary route holds a sentinel: %s", body)
			}
		})
	}
	for pattern, paths := range detailRoutes {
		for _, path := range paths {
			t.Run(pattern+" "+path, func(t *testing.T) {
				rec := get(h, path)
				body := rec.Body.String()
				if rec.Code != http.StatusOK || !strings.Contains(body, sentinelValue) {
					t.Fatalf("status %d, body %s", rec.Code, body)
				}
				if strings.Contains(body, sentinelVariable) {
					t.Errorf("the detail route holds the value of a variable: %s", body)
				}
			})
		}
	}
}

func TestNoRouteCarriesAVariableOrTheEnvironment(t *testing.T) {
	t.Setenv("PALETTE_API_KEY", "SENTINEL-KEY")
	r := newRig(t)
	h := handlerOf(r.cycled(&fakeStrata{dir: r.fixtures}))
	paths := []string{"/healthz", "/", "/debug", "/debug/vars", "/env", "/v1/env", "/v1/records", "/v1/raw"}
	for _, p := range summaryRoutes {
		paths = append(paths, p)
	}
	for _, ps := range detailRoutes {
		paths = append(paths, ps...)
	}
	for _, path := range paths {
		if body := get(h, path).Body.String(); strings.Contains(body, "SENTINEL-KEY") || strings.Contains(body, sentinelVariable) {
			t.Errorf("%s: %s", path, body)
		}
	}
	for _, path := range []string{"/debug", "/debug/vars", "/env", "/v1/env", "/v1/records", "/v1/raw"} {
		if rec := get(h, path); rec.Code != http.StatusNotFound {
			t.Errorf("%s gave %d", path, rec.Code)
		}
	}
}

func TestEachRouteGivesItsStatusAndContentType(t *testing.T) {
	r := newRig(t)
	h := handlerOf(r.cycled(&fakeStrata{dir: r.fixtures}))
	const jsonType, textType = "application/json", "text/plain; charset=utf-8"
	for _, c := range []struct {
		name, path string
		status     int
		contentTyp string
	}{
		{"health", "/healthz", 200, textType},
		{"cluster", "/v1/cluster", 200, jsonType},
		{"findings", "/v1/findings", 200, jsonType},
		{"profiles", "/v1/profiles", 200, jsonType},
		{"a profile by name and version", "/v1/profiles/aws-stack@1.1.0", 200, jsonType},
		{"a profile by name", "/v1/profiles/aws-stack", 200, jsonType},
		{"newer", "/v1/newer", 200, jsonType},
		{"not checked", "/v1/notchecked", 200, jsonType},
		{"events", "/v1/events", 200, jsonType},
		{"the diff of the published side", pathPublished, 200, jsonType},
		{"the diff of the cluster side", pathCluster, 200, jsonType},
		{"the diff of the newer side", pathNewer, 200, jsonType},
		{"an unknown profile", "/v1/profiles/nope", 404, jsonType},
		{"the diff of an unknown profile", "/v1/packs/nope/k8s/kubernetes/diff?side=cluster", 404, jsonType},
		{"the diff of an unknown pack", "/v1/packs/aws-stack@1.1.0/k8s/nope/diff?side=cluster", 404, jsonType},
		{"the diff of a pack with no change on the side", "/v1/packs/aws-stack@1.1.0/k8s/kubernetes/diff?side=published", 404, jsonType},
		{"the diff of a pack that no pin names", "/v1/packs/aws-stack@1.1.0/csi/csi-aws-ebs/diff?side=newer", 404, jsonType},
		{"a diff with no side", "/v1/packs/aws-stack@1.1.0/k8s/kubernetes/diff", 400, jsonType},
		{"a diff with a side that does not exist", "/v1/packs/aws-stack@1.1.0/k8s/kubernetes/diff?side=nope", 400, jsonType},
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
	r := newRig(t)
	h := handlerOf(r.cycled(&fakeStrata{dir: r.fixtures}))
	for _, path := range []string{
		"/v1/profiles/SENTINEL-PROFILE",
		"/v1/packs/SENTINEL-PROFILE/k8s/kubernetes/diff?side=cluster",
		"/v1/packs/aws-stack@1.1.0/SENTINEL-LAYER/SENTINEL-PACK/diff?side=cluster",
		"/v1/packs/aws-stack@1.1.0/k8s/kubernetes/diff?side=SENTINEL-SIDE",
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
	if rec := get(h, "/"); rec.Code != 200 || !strings.Contains(rec.Body.String(), "page") || rec.Header().Get("Content-Security-Policy") == "" {
		t.Errorf("status %d, body %q, headers %v", rec.Code, rec.Body.String(), rec.Header())
	}
	if rec := get(h, "/v1/nope"); rec.Code != 404 || rec.Header().Get("Content-Type") != "application/json" {
		t.Errorf("status %d, type %q", rec.Code, rec.Header().Get("Content-Type"))
	}
}

func TestTheClusterRouteGivesEachValueOfThePage(t *testing.T) {
	now := time.Date(2026, 11, 3, 14, 0, 0, 0, time.UTC)
	good := now.Add(-time.Minute)
	old := now.Add(-time.Hour)
	every := 15 * time.Minute
	cases := []struct {
		page  model.Page
		files int
		clock model.Clock
	}{
		{model.PageNoRecords, 0, model.Clock{Every: every}},
		{model.PageLost, 1, model.Clock{Every: every, FailedRuns: 2, LastError: &model.ErrorRef{Kind: "transport"}}},
		{model.PageLost, 1, model.Clock{Every: every, FailedRuns: 2, LastError: &model.ErrorRef{Kind: "api", Status: 401}}},
		{model.PageStale, 1, model.Clock{Every: every, FailedRuns: 2, LastError: &model.ErrorRef{Kind: "unknown"}}},
		{model.PageStale, 1, model.Clock{Every: every, Last: &old}},
		{model.PageFirstLoad, 1, model.Clock{Every: every}},
		{model.PageOK, 1, model.Clock{Every: every, Last: &good}},
	}
	for _, c := range cases {
		t.Run(string(c.page), func(t *testing.T) {
			sum := model.Build(model.Input{Now: now, ClusterUID: "c1", StartedAt: now.Add(-time.Hour), Files: c.files, FilesKnown: true, ContentClock: c.clock})
			var got ClusterView
			rec := get(handlerOf(stub{sum}), "/v1/cluster")
			decode(t, rec, &got)
			if got.Page != c.page {
				t.Errorf("page %q, want %q", got.Page, c.page)
			}
		})
	}
}

func TestClusterGivesTheFieldsUpToTheCounts(t *testing.T) {
	r := newRig(t)
	h := handlerOf(r.cycled(&fakeStrata{dir: r.fixtures}))
	var got ClusterView
	decode(t, get(h, "/v1/cluster"), &got)
	if got.Cluster.Name != "payments" || got.Page != model.PageOK || got.StrataVersion != "0.29.0" || got.Records.Count != 1 ||
		got.Records.Baseline != "approved" || got.Records.Revision != "4e1c7a2" || got.Content.Every != "15m" || got.Versions.Every != "6h" ||
		got.Counts.Findings != 2 || got.Counts.Newer != 1 {
		t.Errorf("%+v", got)
	}
	var top map[string]json.RawMessage
	decode(t, get(h, "/v1/cluster"), &top)
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
	r := newRig(t)
	h := handlerOf(r.cycled(&fakeStrata{dir: r.fixtures}))
	var all ProfilesView
	decode(t, get(h, "/v1/profiles"), &all)
	var one ProfileView
	decode(t, get(h, "/v1/profiles/aws-stack"), &one)
	if len(all.Profiles) != 1 || one.Name != "aws-stack" || one.Status != model.StatusDrift || len(one.Findings) != 2 || len(one.Who) != 1 || one.Who[0].Actor != "Betty Rubble" {
		t.Fatalf("one %+v", one)
	}
	if one.Published == nil || one.Published.RootLive != "r2" || one.ClusterCopy == nil || one.ClusterCopy.RootB != "r3" || len(one.Packs) != 2 {
		t.Errorf("the two sides: %+v", one)
	}
	var findings FindingsView
	decode(t, get(h, "/v1/findings"), &findings)
	if len(findings.Findings) != 2 {
		t.Errorf("findings %+v", findings)
	}
}

func TestNewerNotCheckedAndEventsGiveTheirPartsOfTheSummary(t *testing.T) {
	r := newRig(t)
	h := handlerOf(r.cycled(&fakeStrata{dir: r.fixtures}))
	var newer NewerView
	decode(t, get(h, "/v1/newer"), &newer)
	if len(newer.Newer) != 1 || newer.Newer[0].Name != "kubernetes" || newer.Newer[0].Newest != "1.36.2" || newer.Newer[0].Impact != "may-repave" || newer.Newer[0].Fold.Settings != 2 || newer.Newer[0].Fold.Repeats != 2 || !newer.Newer[0].New {
		t.Errorf("newer %+v", newer)
	}
	var notChecked NotCheckedView
	decode(t, get(h, "/v1/notchecked"), &notChecked)
	if len(notChecked.NotChecked) != 1 || notChecked.NotChecked[0].Cause != model.OlderRegistryCopy || notChecked.Limits.HelmPacks != 0 || notChecked.Limits.ImagesNamedByATag != 1 || notChecked.Limits.MaskedPaths != 1 {
		t.Errorf("not checked %+v", notChecked)
	}
	var events EventsView
	decode(t, get(h, "/v1/events"), &events)
	reasons := map[string]int{}
	for _, e := range events.Events {
		reasons[e.Reason]++
	}
	if reasons["DriftFound"] != 1 || reasons["ClusterDiffers"] != 1 || reasons["ProfileUpdated"] != 1 || reasons["NewVersion"] != 1 || reasons["OldCopy"] != 1 || len(events.Events) != 5 {
		t.Errorf("events %+v", events)
	}
}

func TestTheDetailRouteSplitsTheValuesInThreeLists(t *testing.T) {
	r := newRig(t)
	h := handlerOf(r.cycled(&fakeStrata{dir: r.fixtures}))
	var got PackDiff
	decode(t, get(h, pathPublished), &got)
	if got.Profile != "aws-stack@1.1.0" || got.Key != "csi/csi-aws-ebs" || got.Side != model.SidePublished || got.Status != "changed" || got.Impact.Level != "cluster-wide" {
		t.Errorf("%+v", got)
	}
	v := got.Values
	if len(v.Settings) != 1 || v.Settings[0].Path != "charts.aws-ebs-csi-driver.controller.replicaCount" || v.Settings[0].Old != sentinelValue || v.Settings[0].New != sentinelValue ||
		len(v.FollowsTheVersion) != 1 || v.FollowsTheVersion[0].Path != "charts.aws-ebs-csi-driver.image.tag" ||
		len(v.RepeatsTheLists) != 1 || v.RepeatsTheLists[0].Path != "pack.content.images[0].image" || !v.Parseable {
		t.Errorf("values %+v", v)
	}
	if len(got.Flags) != 2 || got.Flags[0] != "AUTHORED" || got.Flags[1] != "IMAGES" ||
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
	r := newRig(t)
	good := &fakeStrata{dir: r.fixtures}
	r.cycled(good)
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
	raw, err := os.ReadFile(filepath.Join(r.fixtures, "verify.json"))
	if err != nil {
		t.Fatal(err)
	}
	planted := strings.Replace(string(raw), "controller.replicaCount", "clients[name="+pathMarker+"].id", 1)
	if planted == string(raw) {
		t.Fatal("the fixture holds no path to change")
	}
	if err := os.WriteFile(filepath.Join(r.fixtures, "verify.json"), []byte(planted), 0o600); err != nil {
		t.Fatal(err)
	}
	o := r.observer(&fakeStrata{dir: r.fixtures})
	once, err := o.Once(context.Background())
	if err != nil {
		t.Fatalf("the cycles failed: %v", err)
	}
	printed, err := json.Marshal(once)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(printed), pathMarker) {
		t.Errorf("once prints the path: %s", printed)
	}
	h := handlerOf(o)
	for _, path := range summaryRoutes {
		if body := get(h, path).Body.String(); strings.Contains(body, pathMarker) {
			t.Errorf("%s holds the path: %s", path, body)
		}
	}
	if body := get(h, pathPublished).Body.String(); !strings.Contains(body, pathMarker) {
		t.Errorf("the detail route does not hold the path, so the marker tests nothing: %s", body)
	}
}

func TestTheDetailRouteAnswersWith503WhenTheWaitEnds(t *testing.T) {
	r := newRig(t)
	o := r.cycled(&fakeStrata{dir: r.fixtures})
	for _, err := range []error{context.DeadlineExceeded, context.Canceled} {
		h := handlerOf(slow{stub{o.Summary()}, err})
		if rec := get(h, pathNewer); rec.Code != http.StatusServiceUnavailable {
			t.Errorf("%v gave %d", err, rec.Code)
		}
	}
}

func TestAnUnreadAuditLogKeepsTheFindingAndTheRoutesCarryAFlagAndNotTheText(t *testing.T) {
	r := newRig(t)
	raw, err := os.ReadFile(filepath.Join(r.fixtures, "verify.json"))
	if err != nil {
		t.Fatal(err)
	}
	// strata keeps the verdict of the drifted record, and gives the reason in place of the writes
	unread := regexp.MustCompile(`"who":\[[^\]]*\]`).ReplaceAllString(string(raw), `"whoError":"HTTP 403: AUDIT-REASON-SENTINEL"`)
	if unread == string(raw) {
		t.Fatal("the fixture holds no who")
	}
	if err := os.WriteFile(filepath.Join(r.fixtures, "verify.json"), []byte(unread), 0o600); err != nil {
		t.Fatal(err)
	}
	h := handlerOf(r.cycled(&fakeStrata{dir: r.fixtures}))
	var findings FindingsView
	decode(t, get(h, "/v1/findings"), &findings)
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
	decode(t, get(h, "/v1/notchecked"), &notChecked)
	for _, n := range notChecked.NotChecked {
		if n.Cause == model.ReadError {
			t.Errorf("the drift is no read-error: %+v", n)
		}
	}
	var events EventsView
	decode(t, get(h, "/v1/events"), &events)
	sentences := 0
	for _, e := range events.Events {
		if e.Reason == "AuditUnread" && e.Message == "The audit log could not be read, so this does not say who made the change." {
			sentences++
		}
	}
	if sentences != 1 {
		t.Errorf("events %+v", events)
	}
	for _, route := range []string{"cluster", "findings", "profiles", "newer", "notchecked", "events"} {
		if body := get(h, "/v1/"+route).Body.String(); strings.Contains(body, "AUDIT-REASON-SENTINEL") {
			t.Errorf("/v1/%s holds the text of the error: %s", route, body)
		}
	}
	// the log of the pod names the status and never the body of the answer
	if line := r.log.String(); strings.Contains(line, "AUDIT-REASON-SENTINEL") || !strings.Contains(line, `reason="HTTP 403"`) {
		t.Errorf("log %q", line)
	}
}

// Ask is the click of the stub's Source: it waits until the time limit ends.
func (s slow) Ask(context.Context, string, string) (model.Answer, bool, error) {
	return model.Answer{}, false, s.err
}
