// Package api serves the answers of the observer on a loopback address, in two tiers.
//
// The summary tier carries verdicts, roots, counts, paths, and flags, and no value. Its types are
// parts of model.Summary, and none of them has a field of type docs.Leaf, so the compiler keeps a
// value out. The detail tier carries the old and the new value of a pack, the text of its values and
// its manifests, and the answers of Palette to the questions of a person. The gate of the detail tier
// is Kubernetes: the address is loopback, so a person reaches it with `kubectl port-forward`, and the
// RBAC verb `create pods/portforward` decides who can. The server holds no key and makes no call to
// Kubernetes. It takes two POSTs: the click "Ask Palette" on one move, and the click "Check now" on one
// cycle, which runs the reads of that cycle. No cycle sends a POST to Palette.
// The route /v1/alerts shows the rules of the alerts with their states, and it names the channels and holds
// no address.
//
// No answer carries a variable of a cluster, a whole record, a raw directory, or the environment of
// a process: the types of internal/docs have no field for them.
package api

import (
	"context"
	"encoding/json"
	"errors"
	"net"
	"net/http"
	"strconv"
	"strings"
	"time"

	"github.com/como-technologies/strata/observer/internal/alert"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
)

// detailTimeout limits one request of the detail tier. A request can wait for a cycle and then run
// one compare. So the limit is long. It is shorter than the write timeout of the server.
const detailTimeout = 50 * time.Second

// Source is what the server serves. The Observer of internal/check is the Source of the program.
type Source interface {
	// Summary gives the summary at this time.
	Summary() model.Summary
	// Observed gives the document of the pods: the images, with the digests that the pods run.
	Observed() model.Observed
	// PackDiff gives the entry of a pack in the diff of one side of a profile, with its changed
	// values and their text. The profile is name@version. The second result is false when nothing
	// is known.
	PackDiff(ctx context.Context, profile, key string, side model.Side) (docs.Pack, bool, error)
	// Ask asks Palette about the move of one pack of a profile. A second call for an answered move
	// gives the first answer and sends nothing. The second result is false when the move is not known.
	Ask(ctx context.Context, profile, key string) (model.Answer, bool, error)
	// Answers gives the answer to each question that was asked since the start.
	Answers() []model.Answer
	// Alerts gives the rules of the alerts with their states. It says so when the observer sends no alert.
	Alerts() alert.View
	// StartCycle starts the cycle that name names at once, or says why it does not. The second result
	// is false for a name that is no cycle.
	StartCycle(name string) (model.Start, bool)
}

// Set is what the server serves for a set of clusters, with --clusters. Each cluster of the set is a Source of its
// own, under /v1/clusters/{uid}. The alerts and the cycles belong to the whole set.
type Set interface {
	// Clusters gives the summary of the set: a row for each cluster, the projects, and the selectors.
	Clusters() model.SetSummary
	// Shared gives each profile version that a cluster of the set runs, with its copies.
	Shared() []model.Shared
	// Member gives the Source of one cluster. The second result is false for a cluster that is not in the set.
	Member(uid string) (Source, bool)
	// Alerts gives the rules of the alerts of the set with their states.
	Alerts() alert.View
	// StartCycle starts a cycle of the set: see Source.StartCycle.
	StartCycle(name string) (model.Start, bool)
}

// Tier says what the answer of a route carries.
type Tier string

// The tiers of the routes.
const (
	TierHealth  Tier = "health"
	TierSummary Tier = "summary"
	TierDetail  Tier = "detail"
	// TierCycle is the click "Check now". Its answer says if a cycle starts, and holds no value.
	TierCycle Tier = "cycle"
)

// Route names a route of the API, the method that it takes, and the tier of its answer. Each route
// takes GET, except the clicks "Ask Palette" and "Check now".
type Route struct {
	Pattern string
	Method  string
	Tier    Tier
}

// The patterns of the routes.
const (
	patternHealth     = "/healthz"
	patternCluster    = "/v1/cluster"
	patternFindings   = "/v1/findings"
	patternProfiles   = "/v1/profiles"
	patternProfile    = "/v1/profiles/{name}"
	patternNewer      = "/v1/newer"
	patternNotChecked = "/v1/notchecked"
	patternEvents     = "/v1/events"
	patternImages     = "/v1/images"
	patternImage      = "/v1/images/{ref}"
	patternAlerts     = "/v1/alerts"
	patternPackDiff   = "/v1/packs/{profile}/{layer}/{name}/diff"
	patternAsks       = "/v1/asks"
	patternAsk        = "/v1/packs/{profile}/{layer}/{name}/ask"
	patternCycle      = "/v1/cycles/{cycle}"
	// The routes of a set of clusters. Each route of one cluster, but the health, is also under patternMember.
	patternClusters = "/v1/clusters"
	patternShared   = "/v1/shared"
	patternMember   = "/v1/clusters/{uid}"
)

// The header that a browser sets on a request, and the value of a request from the page itself. A
// POST needs it: a page of another origin in the browser of a person sends another value, or none.
const (
	headerFetchSite = "Sec-Fetch-Site"
	sameOrigin      = "same-origin"
)

// Server serves a Source, or a Set. Page is the handler of the route `/` and of each path that no route owns.
// With a nil Page, these paths get 404. With a Set, the server serves the routes of the set, and each route of one
// cluster under /v1/clusters/{uid}. With no Set, it serves the routes of the Source, as with no --clusters.
type Server struct {
	Source Source
	Set    Set
	Page   http.Handler
}

type entry struct {
	Route
	handler http.HandlerFunc
}

// sourced is the handler of a route of one cluster. src is the Source of that cluster.
type sourced func(w http.ResponseWriter, r *http.Request, src Source)

// clusterEntry is a route of one cluster, before the server knows which Source serves it.
type clusterEntry struct {
	Route
	handler sourced
}

// clusterTable lists the routes of one cluster, with the patterns of an observer of one cluster.
func clusterTable() []clusterEntry {
	return []clusterEntry{
		{Route{patternCluster, http.MethodGet, TierSummary}, cluster},
		{Route{patternFindings, http.MethodGet, TierSummary}, findings},
		{Route{patternProfiles, http.MethodGet, TierSummary}, profiles},
		{Route{patternProfile, http.MethodGet, TierSummary}, profile},
		{Route{patternNewer, http.MethodGet, TierSummary}, newer},
		{Route{patternNotChecked, http.MethodGet, TierSummary}, notChecked},
		{Route{patternEvents, http.MethodGet, TierSummary}, events},
		{Route{patternImages, http.MethodGet, TierSummary}, images},
		{Route{patternImage, http.MethodGet, TierSummary}, image},
		{Route{patternAlerts, http.MethodGet, TierSummary}, alerts},
		{Route{patternPackDiff, http.MethodGet, TierDetail}, packDiff},
		{Route{patternAsks, http.MethodGet, TierDetail}, asks},
		{Route{patternAsk, http.MethodPost, TierDetail}, ask},
		{Route{patternCycle, http.MethodPost, TierCycle}, func(w http.ResponseWriter, r *http.Request, src Source) { startCycle(w, r, src) }},
	}
}

// table lists every route. The server registers no other route. With no Set, the routes are the routes of one
// cluster, as with no --clusters. With a Set, the set has its routes, and each route of one cluster is under
// patternMember. A test fails when a summary route has no line in the table of the sentinel test.
func (s *Server) table() []entry {
	out := []entry{{Route{patternHealth, http.MethodGet, TierHealth}, s.health}}
	if s.Set == nil {
		for _, e := range clusterTable() {
			out = append(out, entry{e.Route, func(w http.ResponseWriter, r *http.Request) { e.handler(w, r, s.Source) }})
		}
		return out
	}
	out = append(out,
		entry{Route{patternClusters, http.MethodGet, TierSummary}, s.clusters},
		entry{Route{patternShared, http.MethodGet, TierSummary}, s.shared},
		entry{Route{patternAlerts, http.MethodGet, TierSummary}, func(w http.ResponseWriter, _ *http.Request) { writeJSON(w, http.StatusOK, s.Set.Alerts()) }},
		entry{Route{patternCycle, http.MethodPost, TierCycle}, func(w http.ResponseWriter, r *http.Request) { startCycle(w, r, s.Set) }})
	for _, e := range clusterTable() {
		route := e.Route
		route.Pattern = patternMember + strings.TrimPrefix(e.Pattern, routePrefix)
		out = append(out, entry{route, func(w http.ResponseWriter, r *http.Request) {
			src, ok := s.Set.Member(r.PathValue("uid"))
			if !ok {
				notFound(w, r)
				return
			}
			e.handler(w, r, src)
		}})
	}
	return out
}

// starter starts a cycle: the Source of one cluster, or a Set.
type starter interface {
	StartCycle(name string) (model.Start, bool)
}

// routePrefix starts each route of the API. A route of one cluster of a set takes patternMember in its place.
const routePrefix = "/v1"

// clusters serves the summary of the set: a row for each cluster, the projects, and the selectors.
func (s *Server) clusters(w http.ResponseWriter, _ *http.Request) {
	writeJSON(w, http.StatusOK, s.Set.Clusters())
}

// shared serves each profile version that a cluster of the set runs.
func (s *Server) shared(w http.ResponseWriter, _ *http.Request) {
	writeJSON(w, http.StatusOK, SharedView{Shared: nonNil(s.Set.Shared())})
}

// Routes lists the routes of the API, not counting the route of the page.
func (s *Server) Routes() []Route {
	var out []Route
	for _, e := range s.table() {
		out = append(out, e.Route)
	}
	return out
}

// Handler gives the handler of the server. It sets the headers on each answer, also on a refusal.
// It refuses a request with a Host that is not loopback (421), a method that the route does not take
// (405), and a POST that the page itself did not send (403).
func (s *Server) Handler() http.Handler {
	mux := http.NewServeMux()
	methods := map[string]string{}
	for _, e := range s.table() {
		mux.HandleFunc(e.Pattern, e.handler)
		methods[e.Pattern] = e.Method
	}
	mux.HandleFunc("/v1/", notFound)
	if s.Page != nil {
		mux.Handle("/", s.Page)
	} else {
		mux.HandleFunc("/", notFound)
	}
	return guard(mux, methods)
}

// The policy of the page. The page loads its script, style, fonts, and icons from the server itself,
// and from nowhere else.
const contentSecurityPolicy = "default-src 'none'; script-src 'self'; style-src 'self'; font-src 'self'; img-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"

// guard sets the four headers on each answer and refuses the requests that the server does not serve.
// methods gives the method of each pattern of the routes. A path with no route takes GET.
func guard(next *http.ServeMux, methods map[string]string) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		h := w.Header()
		h.Set("Content-Security-Policy", contentSecurityPolicy)
		h.Set("X-Content-Type-Options", "nosniff")
		h.Set("Referrer-Policy", "no-referrer")
		h.Set("Cache-Control", "no-store")
		_, pattern := next.Handler(r)
		method, ok := methods[pattern]
		if !ok {
			method = http.MethodGet
		}
		switch {
		case !loopbackHost(r.Host):
			// A page in the browser of a person can reach 127.0.0.1 after a DNS rebinding. Its Host is
			// then the name of the page, and not a loopback name.
			fail(w, http.StatusMisdirectedRequest)
		case r.Method != method:
			h.Set("Allow", method)
			fail(w, http.StatusMethodNotAllowed)
		case method == http.MethodPost && r.Header.Get(headerFetchSite) != sameOrigin:
			// A page of another origin can send a POST to 127.0.0.1 from the browser of a person. The
			// browser says where a request comes from, and a request that is not from the page is refused.
			fail(w, http.StatusForbidden)
		default:
			next.ServeHTTP(w, r)
		}
	})
}

// loopbackHost says that the Host of a request is localhost or a loopback address, with any port. A
// person can choose the local port of a port-forward, so the port can be any port.
func loopbackHost(host string) bool {
	name := host
	if h, _, err := net.SplitHostPort(host); err == nil {
		name = h
	} else if strings.HasPrefix(host, "[") && strings.HasSuffix(host, "]") {
		name = host[1 : len(host)-1]
	}
	if strings.EqualFold(name, "localhost") {
		return true
	}
	ip := net.ParseIP(name)
	return ip != nil && ip.IsLoopback()
}

// errorBody is the body of a refusal. It has a fixed text, so that it names no value and echoes no
// part of the request.
type errorBody struct {
	Error string `json:"error"`
}

func fail(w http.ResponseWriter, status int) {
	writeJSON(w, status, errorBody{Error: strings.ToLower(http.StatusText(status))})
}

func notFound(w http.ResponseWriter, _ *http.Request) {
	fail(w, http.StatusNotFound)
}

// writeJSON writes the whole answer or a 500. It encodes first, so that a failure never sends half a body.
func writeJSON(w http.ResponseWriter, status int, v any) {
	body, err := json.Marshal(v)
	if err != nil {
		status, body = http.StatusInternalServerError, []byte(`{"error":"internal server error"}`)
	}
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_, _ = w.Write(append(body, '\n'))
}

func (s *Server) health(w http.ResponseWriter, _ *http.Request) {
	w.Header().Set("Content-Type", "text/plain; charset=utf-8")
	_, _ = w.Write([]byte("ok\n"))
}

func cluster(w http.ResponseWriter, _ *http.Request, src Source) {
	writeJSON(w, http.StatusOK, ClusterOf(src.Summary()))
}

func findings(w http.ResponseWriter, _ *http.Request, src Source) {
	writeJSON(w, http.StatusOK, FindingsView{Findings: src.Summary().Findings})
}

func profiles(w http.ResponseWriter, _ *http.Request, src Source) {
	writeJSON(w, http.StatusOK, ProfilesOf(src.Summary()))
}

func profile(w http.ResponseWriter, r *http.Request, src Source) {
	sum := src.Summary()
	p, ok := findProfile(sum.Profiles, r.PathValue("name"))
	if !ok {
		notFound(w, r)
		return
	}
	writeJSON(w, http.StatusOK, profileView(p, sum.Findings))
}

func newer(w http.ResponseWriter, _ *http.Request, src Source) {
	writeJSON(w, http.StatusOK, NewerView{Newer: src.Summary().Newer})
}

func notChecked(w http.ResponseWriter, _ *http.Request, src Source) {
	sum := src.Summary()
	writeJSON(w, http.StatusOK, NotCheckedView{NotChecked: sum.NotChecked, Limits: sum.Limits})
}

func events(w http.ResponseWriter, _ *http.Request, src Source) {
	writeJSON(w, http.StatusOK, EventsView{Events: src.Summary().Events})
}

// images serves strata-observed/1. With the read of the pods off, it says so and lists no image.
func images(w http.ResponseWriter, _ *http.Request, src Source) {
	writeJSON(w, http.StatusOK, src.Observed())
}

// alerts serves the rules of the alerts with their states. With no --alerts, it says so and lists no rule.
// It names the channels and holds no address.
func alerts(w http.ResponseWriter, _ *http.Request, src Source) {
	writeJSON(w, http.StatusOK, src.Alerts())
}

// image serves one row of the document. The reference of the image is one escaped segment of the path.
// An image that no pod runs gets 404, and the answer does not name the input.
func image(w http.ResponseWriter, r *http.Request, src Source) {
	view, ok := src.Observed().ImageOf(r.PathValue("ref"))
	if !ok {
		notFound(w, r)
		return
	}
	writeJSON(w, http.StatusOK, view)
}

// packDiff serves the comparison of a pack in the detail tier. A profile, a pack, or a side that is
// unknown gets 404, and a side that is no side gets 400. Neither answer names the input.
func packDiff(w http.ResponseWriter, r *http.Request, src Source) {
	side, ok := model.ParseSide(r.URL.Query().Get("side"))
	if !ok {
		fail(w, http.StatusBadRequest)
		return
	}
	subject, key, ok := moveOf(r, src)
	if !ok {
		notFound(w, r)
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), detailTimeout)
	defer cancel()
	pack, found, err := src.PackDiff(ctx, subject, key, side)
	if detailFailed(w, err) {
		return
	}
	if !found {
		notFound(w, r)
		return
	}
	writeJSON(w, http.StatusOK, PackDiffOf(subject, key, side, pack))
}

// ask serves the click "Ask Palette" on one move: the one POST of the server. It reads no body. A
// profile, a pack, or a move that is not known gets 404. The answer is the verdict of Palette, also
// when the request to Palette failed: then the verdict says so, with the status.
func ask(w http.ResponseWriter, r *http.Request, src Source) {
	subject, key, ok := moveOf(r, src)
	if !ok {
		notFound(w, r)
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), detailTimeout)
	defer cancel()
	answer, found, err := src.Ask(ctx, subject, key)
	if detailFailed(w, err) {
		return
	}
	if !found {
		notFound(w, r)
		return
	}
	writeJSON(w, http.StatusOK, answer)
}

// startCycle serves the click "Check now" on one cycle: content or versions. It reads no body. A cycle
// that starts, or one that runs already, gets 202. A cycle that ended less than a minute ago gets 429,
// with the seconds to wait in the header Retry-After. Another name gets 404.
func startCycle(w http.ResponseWriter, r *http.Request, src starter) {
	start, ok := src.StartCycle(r.PathValue("cycle"))
	if !ok {
		notFound(w, r)
		return
	}
	if start.State == model.StartWait {
		w.Header().Set("Retry-After", strconv.Itoa(start.RetryAfter))
		writeJSON(w, http.StatusTooManyRequests, start)
		return
	}
	writeJSON(w, http.StatusAccepted, start)
}

// asks serves the answers that the observer holds, so that the page shows them after a reload.
func asks(w http.ResponseWriter, _ *http.Request, src Source) {
	writeJSON(w, http.StatusOK, AsksView{Asks: nonNil(src.Answers())})
}

// moveOf reads the profile and the pack of a request of the detail tier. The profile is name@version,
// and the key is <layer>/<name>. It is false when the profile is not known.
func moveOf(r *http.Request, src Source) (subject, key string, ok bool) {
	profile, ok := findProfile(src.Summary().Profiles, r.PathValue("profile"))
	if !ok {
		return "", "", false
	}
	return model.Subject(profile.Name, profile.Version), r.PathValue("layer") + "/" + r.PathValue("name"), true
}

// detailFailed answers a request of the detail tier that failed, and says so. A stop of the request, or
// the time limit, is 503: a cycle holds the worker. Another error is 502.
func detailFailed(w http.ResponseWriter, err error) bool {
	switch {
	case err == nil:
		return false
	case errors.Is(err, context.DeadlineExceeded), errors.Is(err, context.Canceled):
		fail(w, http.StatusServiceUnavailable)
	default:
		fail(w, http.StatusBadGateway)
	}
	return true
}
