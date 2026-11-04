package devdata

import (
	"context"
	"encoding/json"
	"io/fs"
	"net/http"
	"net/http/httptest"
	"os"
	"reflect"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/model"
)

// sets is the directory of the sets, as the dev server reads it.
func sets() fs.FS { return os.DirFS("../testdata/api") }

func load(t *testing.T, state State) *Set {
	t.Helper()
	s, err := Load(sets(), state, time.Time{})
	if err != nil {
		t.Fatal(err)
	}
	return s
}

// normal reads JSON into plain values, so that two documents compare with no regard to the layout.
func normal(t *testing.T, raw []byte) any {
	t.Helper()
	var v any
	if err := json.Unmarshal(raw, &v); err != nil {
		t.Fatalf("%v: %s", err, raw)
	}
	return v
}

func encoded(t *testing.T, v any) any {
	t.Helper()
	raw, err := json.Marshal(v)
	if err != nil {
		t.Fatal(err)
	}
	return normal(t, raw)
}

// Each file of each set decodes into the type of its route with no field to spare, and it encodes to
// the same document. So a field that the API adds or drops makes this test fail.
func TestEachFileOfEachSetMatchesTheTypeOfItsRoute(t *testing.T) {
	for _, state := range States {
		s := load(t, state)
		for _, f := range []struct {
			name string
			v    any
		}{{fileCluster, s.Cluster}, {fileFindings, s.Findings}, {fileProfiles, s.Profiles}, {fileNewer, s.Newer},
			{fileNotChecked, s.NotChecked}, {fileEvents, s.Events}, {fileDiffs, s.Diffs}} {
			t.Run(string(state)+"/"+f.name, func(t *testing.T) {
				raw, err := Read(sets(), state, f.name)
				if err != nil {
					t.Fatal(err)
				}
				if got, want := encoded(t, f.v), normal(t, raw); !reflect.DeepEqual(got, want) {
					t.Errorf("the type gives\n%v\nthe file holds\n%v", got, want)
				}
			})
		}
	}
}

func TestAFieldThatTheTypeDoesNotHaveIsRefused(t *testing.T) {
	var v api.EventsView
	if err := Decode([]byte(`{"events":[],"extra":1}`), &v); err == nil {
		t.Error("an extra field passed")
	}
	if err := Decode([]byte(`{"events":[]} {}`), &v); err == nil {
		t.Error("data after the document passed")
	}
}

// The server serves the files: each summary route gives the file of its route, and the detail route
// gives each entry of diffs.json. So the data that the page reads is the data of the files.
func TestTheRoutesServeTheFilesOfEachSet(t *testing.T) {
	for _, state := range States {
		s := load(t, state)
		h := (&api.Server{Source: s}).Handler()
		get := func(path string) (int, any) {
			req := httptest.NewRequest(http.MethodGet, path, nil)
			req.Host = "127.0.0.1:8080"
			rec := httptest.NewRecorder()
			h.ServeHTTP(rec, req)
			var v any
			_ = json.Unmarshal(rec.Body.Bytes(), &v)
			return rec.Code, v
		}
		for route, file := range map[string]string{"/v1/cluster": fileCluster, "/v1/findings": fileFindings, "/v1/profiles": fileProfiles,
			"/v1/newer": fileNewer, "/v1/notchecked": fileNotChecked, "/v1/events": fileEvents} {
			raw, err := Read(sets(), state, file)
			if err != nil {
				t.Fatal(err)
			}
			if code, got := get(route); code != http.StatusOK || !reflect.DeepEqual(got, normal(t, raw)) {
				t.Errorf("%s: %s gave %d and\n%v\nthe file holds\n%v", state, route, code, got, normal(t, raw))
			}
		}
		for _, d := range s.Diffs {
			path := "/v1/packs/" + d.Profile + "/" + d.Key + "/diff?side=" + string(d.Side)
			if code, got := get(path); code != http.StatusOK || !reflect.DeepEqual(got, encoded(t, d)) {
				t.Errorf("%s: %s gave %d and\n%v\nthe file holds\n%v", state, path, code, got, encoded(t, d))
			}
		}
		if _, found, _ := s.PackDiff(context.Background(), "nobody@1.0.0", "addon/none", model.SideNewer); found {
			t.Errorf("%s: a pack that no entry names has a diff", state)
		}
	}
}

// Each set gives the value of `page` that its name says, and the counts agree with the lists.
func TestEachSetGivesItsPageAndConsistentCounts(t *testing.T) {
	pages := map[State]model.Page{StateOK: model.PageOK, StateQuiet: model.PageOK, StateFirstLoad: model.PageFirstLoad,
		StateNoRecords: model.PageNoRecords, StateStale: model.PageStale, StateLost: model.PageLost, StateFirstSeen: model.PageOK,
		StateStaleUnchecked: model.PageStale, StateLostUnchecked: model.PageLost, StateReadError: model.PageOK}
	if len(pages) != len(States) {
		t.Fatalf("States lists %d sets, and the table of pages %d", len(States), len(pages))
	}
	for _, state := range States {
		s := load(t, state)
		if s.Cluster.Page != pages[state] {
			t.Errorf("%s: page %q, want %q", state, s.Cluster.Page, pages[state])
		}
		n := s.Cluster.Counts
		var drift, packs int
		for _, p := range s.Profiles.Profiles {
			if p.Status == model.StatusDrift {
				drift++
			}
			if p.ClusterCopy == nil || p.ClusterCopy.Status != model.CopyNotOnThisCluster {
				packs += len(p.Packs)
			}
		}
		profiles := map[string]bool{}
		for _, r := range s.Newer.Newer {
			for _, p := range r.Profiles {
				profiles[p] = true
			}
		}
		for what, got := range map[string][2]int{
			"findings": {n.Findings, len(s.Findings.Findings)}, "profiles": {n.Profiles, len(s.Profiles.Profiles)}, "drift": {n.Drift, drift},
			"newer": {n.Newer, len(s.Newer.Newer)}, "newerProfiles": {n.NewerProfiles, len(profiles)},
			"notChecked": {n.NotChecked, len(s.NotChecked.NotChecked)}, "packs": {n.Packs, packs},
		} {
			if got[0] != got[1] {
				t.Errorf("%s: counts.%s is %d, and the lists give %d", state, what, got[0], got[1])
			}
		}
	}
}

func TestTheStatesAreTheStatesOfTheRecipe(t *testing.T) {
	entries, err := fs.ReadDir(sets(), ".")
	if err != nil {
		t.Fatal(err)
	}
	dirs := map[string]bool{}
	for _, e := range entries {
		dirs[e.Name()] = true
	}
	for _, state := range States {
		if !dirs[string(state)] {
			t.Errorf("no directory for the state %s", state)
		}
		delete(dirs, string(state))
	}
	for name := range dirs {
		t.Errorf("the directory %s is no state", name)
	}
}

// The set ok holds the made-up data of the mock.
func TestTheOKSetHoldsTheDataOfTheMock(t *testing.T) {
	s := load(t, StateOK)
	n := s.Cluster.Counts
	if n.Profiles != 12 || n.Packs != 29 || n.Findings != 1 || n.Newer != 10 || n.NewerProfiles != 6 || n.NotChecked != 2 || n.PacksInFull != 26 {
		t.Errorf("counts %+v", n)
	}
	if s.Cluster.Records.Baseline != baseline.Approved || s.Cluster.Records.Revision == "" {
		t.Errorf("records %+v", s.Cluster.Records)
	}
}

func TestFirstSeenIsTheOKSetWithAFirstSeenBaseline(t *testing.T) {
	ok, first := load(t, StateOK), load(t, StateFirstSeen)
	if first.Cluster.Records.Baseline != baseline.FirstSeen || first.Cluster.Records.FirstSeenFrom == "" || first.Cluster.Records.Revision != "" {
		t.Errorf("records %+v", first.Cluster.Records)
	}
	if !reflect.DeepEqual(ok.Profiles, first.Profiles) {
		t.Error("first-seen does not take the profiles of ok")
	}
}

func TestTheSetsWithNoRecordsAndNoFirstCheckHoldNoFinding(t *testing.T) {
	for _, state := range []State{StateQuiet, StateFirstLoad, StateNoRecords} {
		s := load(t, state)
		if len(s.Findings.Findings) != 0 || len(s.Newer.Newer) != 0 || len(s.Events.Events) != 0 {
			t.Errorf("%s holds a finding, a newer version, or an event", state)
		}
	}
	if load(t, StateNoRecords).Cluster.Records.Baseline != baseline.Unknown {
		t.Error("no-records has a baseline")
	}
}

func TestLoadMovesEachTimeByTheGapToTheReference(t *testing.T) {
	now := Reference.Add(90 * time.Minute)
	moved, err := Load(sets(), StateOK, now)
	if err != nil {
		t.Fatal(err)
	}
	plain := load(t, StateOK)
	if got, want := *moved.Cluster.Content.CheckedAt, plain.Cluster.Content.CheckedAt.Add(90*time.Minute); !got.Equal(want) {
		t.Errorf("checkedAt %v, want %v", got, want)
	}
	if got, want := moved.Findings.Findings[0].FirstSeen, plain.Findings.Findings[0].FirstSeen.Add(90*time.Minute); !got.Equal(want) {
		t.Errorf("firstSeen %v, want %v", got, want)
	}
	if moved.Newer.Newer[0].Listed == plain.Newer.Newer[0].Listed {
		t.Error("listed did not move")
	}
}

func TestAMissingFileIsAnError(t *testing.T) {
	if _, err := Load(os.DirFS(t.TempDir()), StateOK, time.Time{}); err == nil {
		t.Error("an empty directory loaded")
	}
}
