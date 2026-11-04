package devdata

import (
	"context"
	"encoding/json"
	"io/fs"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"reflect"
	"slices"
	"strings"
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

// Each file of each set decodes into the type of its route, and the decode refuses a field that the
// type lacks. It encodes to the same document. So a field that the API adds or drops makes this test
// fail.
func TestEachFileOfEachSetMatchesTheTypeOfItsRoute(t *testing.T) {
	for _, state := range States {
		s := load(t, state)
		for _, f := range []struct {
			name string
			v    any
		}{{fileCluster, s.Cluster}, {fileFindings, s.Findings}, {fileProfiles, s.Profiles}, {fileNewer, s.Newer},
			{fileNotChecked, s.NotChecked}, {fileEvents, s.Events}, {fileDiffs, s.Diffs}, {fileImages, s.Images}} {
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
			"/v1/newer": fileNewer, "/v1/notchecked": fileNotChecked, "/v1/events": fileEvents, "/v1/images": fileImages} {
			raw, err := Read(sets(), state, file)
			if err != nil {
				t.Fatal(err)
			}
			if code, got := get(route); code != http.StatusOK || !reflect.DeepEqual(got, normal(t, raw)) {
				t.Errorf("%s: %s gave %d and\n%v\nthe file holds\n%v", state, route, code, got, normal(t, raw))
			}
		}
		for _, img := range s.Images.Images {
			view, _ := s.Images.ImageOf(img.Ref)
			path := "/v1/images/" + url.PathEscape(img.Ref)
			if code, got := get(path); code != http.StatusOK || !reflect.DeepEqual(got, encoded(t, view)) {
				t.Errorf("%s: %s gave %d and\n%v\nthe file holds\n%v", state, path, code, got, encoded(t, view))
			}
		}
		if code, _ := get("/v1/images/" + url.PathEscape("nobody.example/none:0")); code != http.StatusNotFound {
			t.Errorf("%s: an image that no pod runs gave %d", state, code)
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
		StateStaleUnchecked: model.PageStale, StateLostUnchecked: model.PageLost, StateReadError: model.PageOK,
		StateImages: model.PageOK, StateImagesSplit: model.PageOK, StateImagesOff: model.PageOK, StateImagesUnread: model.PageOK}
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

// imageStatusOrder is the order of the rows of the list: the worst image first.
var imageStatusOrder = []model.ImageStatus{model.ImageStatusSplit, model.ImageStatusMoved, model.ImageStatusSteady, model.ImageStatusNotChecked}

// statusOfDigests is the rule of the status of an image: two real digests are a split, one that is not
// the first digest is a move, one that is the first digest is steady, and none is not checked.
func statusOfDigests(digests []model.Digest) model.ImageStatus {
	var real int
	var another bool
	for _, d := range digests {
		if d.Digest != "" {
			real++
			another = another || d.Status == model.AnotherDigest
		}
	}
	switch {
	case real >= 2:
		return model.ImageStatusSplit
	case real == 1 && another:
		return model.ImageStatusMoved
	case real == 1:
		return model.ImageStatusSteady
	}
	return model.ImageStatusNotChecked
}

// The document of the pods agrees with itself, with the rules of the model, and with the other files of
// its set: the counts match the rows, the rows are in order, a row has the status of its digests, and a
// pack that a row names is in the profile that it names.
func TestTheDocumentOfThePodsAgreesWithItselfAndWithItsSet(t *testing.T) {
	for _, state := range States {
		t.Run(string(state), func(t *testing.T) {
			s := load(t, state)
			o := s.Images
			if o.Schema != model.ObservedSchema {
				t.Errorf("schema %q", o.Schema)
			}
			if (o.Pods == model.PodsRead) != (o.ReadAt != nil) {
				t.Errorf("pods %q with readAt %v", o.Pods, o.ReadAt)
			}
			if (o.Pods == model.PodsUnread) != (o.Error != nil) {
				t.Errorf("pods %q with error %v", o.Pods, o.Error)
			}
			if o.Pods != model.PodsRead && (len(o.Images) != 0 || o.Counts != (model.ImageCounts{})) {
				t.Errorf("pods %q with images %d and counts %+v", o.Pods, len(o.Images), o.Counts)
			}
			var matched, changed int
			for i, img := range o.Images {
				if img.Match != model.MatchNone {
					matched++
				}
				if _, ok := img.Finding(); ok {
					changed++
				}
				if i > 0 {
					before := o.Images[i-1]
					a, b := slices.Index(imageStatusOrder, before.Status), slices.Index(imageStatusOrder, img.Status)
					if a > b || a == b && before.Ref >= img.Ref {
						t.Errorf("%s comes after %s: the rows go by status, then by reference", img.Ref, before.Ref)
					}
				}
				if got := statusOfDigests(img.Digests); got != img.Status {
					t.Errorf("%s has the status %s, and its digests give %s", img.Ref, img.Status, got)
				}
				if (img.Match == model.MatchNone) != (len(img.Packs) == 0) || img.Packs == nil {
					t.Errorf("%s has the match %s and %d packs", img.Ref, img.Match, len(img.Packs))
				}
				if img.ByDigest != strings.Contains(img.Ref, "@sha256:") {
					t.Errorf("%s has byDigest %v", img.Ref, img.ByDigest)
				}
				for _, d := range img.Digests {
					hasDigest := d.Status == model.FirstDigest || d.Status == model.AnotherDigest
					if hasDigest != (d.Digest != "") || d.Pods < 1 || d.Digest != "" && !strings.HasPrefix(d.Digest, "sha256:") {
						t.Errorf("%s has the digest %+v", img.Ref, d)
					}
				}
				for _, p := range img.Packs {
					if !hasPack(s, p.Profile, p.Key) {
						t.Errorf("%s names the pack %s of %s, which the profiles do not hold", img.Ref, p.Key, p.Profile)
					}
				}
			}
			if got := (model.ImageCounts{Images: len(o.Images), Matched: matched, Changed: changed}); o.Pods == model.PodsRead && got != o.Counts {
				t.Errorf("counts %+v, and the rows give %+v", o.Counts, got)
			}
		})
	}
}

// hasPack says whether the profiles hold a pack with this key in this profile.
func hasPack(s *Set, profile, key string) bool {
	for _, p := range s.Profiles.Profiles {
		if model.Subject(p.Name, p.Version) != profile {
			continue
		}
		for _, k := range p.Packs {
			if k.Key == key {
				return true
			}
		}
	}
	return false
}

// A finding of an image names the images of the document, and an image that makes a finding has one. The
// finding has an event, and the profile lists it. The other kinds of finding name no image.
func TestEachFindingOfAnImageNamesAnImageOfTheDocumentAndHasItsEvent(t *testing.T) {
	kinds := map[model.Kind]struct {
		status model.ImageStatus
		reason string
	}{model.ImageMoved: {model.ImageStatusMoved, "ImageMoved"}, model.ImageSplit: {model.ImageStatusSplit, "ImageSplit"}}
	for _, state := range States {
		t.Run(string(state), func(t *testing.T) {
			s := load(t, state)
			byRef := map[string]model.Image{}
			for _, img := range s.Images.Images {
				byRef[img.Ref] = img
			}
			named := map[string]bool{}
			var found int
			for _, f := range s.Findings.Findings {
				kind, isImage := kinds[f.Kind]
				if !isImage {
					if len(f.Images) != 0 {
						t.Errorf("the finding %s of the kind %s names images", f.ID, f.Kind)
					}
					continue
				}
				found++
				if len(f.Images) == 0 || f.Impact != "" || f.ReBaselineNeeded || f.ID != model.FindingID(f.Kind, f.Profile) {
					t.Errorf("the finding %+v", f)
				}
				for _, ref := range f.Images {
					img, ok := byRef[ref]
					if !ok || img.Status != kind.status || !slices.ContainsFunc(img.Packs, func(p model.ImagePack) bool { return p.Profile == f.Profile && slices.Contains(f.Packs, p.Key) }) {
						t.Errorf("the finding %s names %s, which the document does not hold as %s of this profile", f.ID, ref, kind.status)
					}
					named[ref] = true
				}
				if !slices.ContainsFunc(s.Events.Events, func(e model.Event) bool {
					return e.Reason == kind.reason && e.Object == f.Profile && e.Type == model.EventDrift
				}) {
					t.Errorf("the finding %s has no event %s", f.ID, kind.reason)
				}
			}
			for _, img := range s.Images.Images {
				if _, ok := img.Finding(); ok && !named[img.Ref] {
					t.Errorf("%s makes a finding, and no finding names it", img.Ref)
				}
			}
			var events int
			for _, e := range s.Events.Events {
				if e.Reason == "ImageMoved" || e.Reason == "ImageSplit" {
					events++
				}
			}
			if events != found {
				t.Errorf("%d events of an image, and %d findings", events, found)
			}
		})
	}
}

// A profile lists the findings of findings.json that name it. So the set cannot hold a finding that its
// profile does not list.
func TestEachProfileListsTheFindingsThatNameIt(t *testing.T) {
	for _, state := range States {
		t.Run(string(state), func(t *testing.T) {
			s := load(t, state)
			for _, p := range s.Profiles.Profiles {
				subject := model.Subject(p.Name, p.Version)
				var want, got []string
				for _, f := range s.Findings.Findings {
					if f.Profile == subject {
						want = append(want, f.ID)
					}
				}
				for _, f := range p.Findings {
					got = append(got, f.ID)
				}
				if !slices.Equal(got, want) {
					t.Errorf("%s lists the findings %v, and findings.json gives %v", subject, got, want)
				}
			}
		})
	}
}

// The sets of the pods hold the states that their names say, and they change nothing else of ok.
func TestTheSetsOfThePodsHoldTheirStates(t *testing.T) {
	ok := load(t, StateOK)
	if o := ok.Images; o.Pods != model.PodsRead || o.Counts.Images != 8 || o.Counts.Changed != 0 || ok.Cluster.Counts.Findings != 1 {
		t.Errorf("ok: pods %q, counts %+v", o.Pods, o.Counts)
	}
	var steady, notChecked, byTagMatch, byDigest, noPack, notStarted, noRepo int
	for _, img := range ok.Images.Images {
		switch img.Status {
		case model.ImageStatusSteady:
			steady++
		case model.ImageStatusNotChecked:
			notChecked++
		}
		if img.Match == model.MatchNameAndTag {
			byTagMatch++
		}
		if img.Match == model.MatchNone {
			noPack++
		}
		if img.ByDigest {
			byDigest++
		}
		for _, d := range img.Digests {
			switch d.Status {
			case model.NotStarted:
				notStarted++
			case model.NoRepoDigest:
				noRepo++
			}
		}
	}
	if steady != 7 || notChecked != 1 || byTagMatch != 1 || noPack < 1 || byDigest != 1 || notStarted != 1 || noRepo != 1 {
		t.Errorf("ok: %d steady, %d not checked, %d by name and tag, %d with no pack, %d by digest, %d never started, %d with no repo digest",
			steady, notChecked, byTagMatch, noPack, byDigest, notStarted, noRepo)
	}
	for _, state := range []State{StateQuiet, StateFirstLoad, StateNoRecords, StateReadError, StateImagesOff} {
		if o := load(t, state).Images; o.Pods != model.PodsOff || o.Error != nil {
			t.Errorf("%s: pods %q, error %v", state, o.Pods, o.Error)
		}
	}
	for state, want := range map[State]model.ImageStatus{StateImages: model.ImageStatusMoved, StateImagesSplit: model.ImageStatusSplit} {
		s := load(t, state)
		img := s.Images.Images[0]
		if img.Ref != "vendor.example/product:2026.2" || img.Status != want || len(s.Findings.Findings) != 2 || s.Cluster.Counts.Findings != 2 ||
			s.Cluster.Counts.Drift != 1 || s.Images.Counts.Changed != 1 || img.Packs[0].Profile != "wilma-gateway@3.1.2" || img.Packs[0].Key != "addon/vendor-product" {
			t.Errorf("%s: first image %+v, %d findings, counts %+v", state, img, len(s.Findings.Findings), s.Cluster.Counts)
		}
	}
	// Images and images-split change the findings, the events, and the profile of the image. The off and unread sets do not.
	for _, state := range []State{StateImagesOff, StateImagesUnread} {
		s := load(t, state)
		if !reflect.DeepEqual(s.Profiles, ok.Profiles) || !reflect.DeepEqual(s.Findings, ok.Findings) || !reflect.DeepEqual(s.Events, ok.Events) {
			t.Errorf("%s changes more than the pods", state)
		}
	}
	unread := load(t, StateImagesUnread)
	row := model.NotChecked{Cause: model.PodsNotRead, Profiles: []string{}, Packs: []string{}}
	if unread.Images.Pods != model.PodsUnread || unread.Images.Error == nil || unread.Images.Error.Kind != "kube-api" || unread.Images.Error.Status != 403 ||
		!slices.ContainsFunc(unread.NotChecked.NotChecked, func(r model.NotChecked) bool { return reflect.DeepEqual(r, row) }) ||
		unread.Cluster.Counts.NotChecked != ok.Cluster.Counts.NotChecked+1 || unread.Cluster.Counts.PacksInFull != ok.Cluster.Counts.PacksInFull {
		t.Errorf("images-unread: %+v, counts %+v", unread.Images, unread.Cluster.Counts)
	}
}
