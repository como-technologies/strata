package alert

import (
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
)

func TestTheSubjectsOfASummaryAreItsFindingsItsNewerVersionsForEachProfileAndTheLoss(t *testing.T) {
	sum := model.Summary{
		Page:    model.PageLost,
		Content: model.ContentClock{Health: model.Health{FailedRuns: 3, LastError: &model.ErrorRef{Kind: docs.KindAPI, Status: 401}}},
		Findings: []model.Finding{
			{ID: "f1", Kind: model.PublishedChanged, Profile: "fred-identity@2.3.0", Packs: []string{"addon/dex"}, FirstSeen: now},
			{ID: "f2", Kind: model.ImageSplit, Profile: "wilma-gateway@3.1.2", Packs: []string{"addon/traefik"}, Images: []string{"a/b:1"}},
		},
		Newer: []model.Newer{{Name: "dex", RegistryUID: "r1", Version: "2.42.0", Newest: "2.44.0", Profiles: []string{"fred-identity@2.3.0", "barney-base@1.5.0"}}},
	}
	got := SubjectsOf(sum)
	if len(got) != 5 {
		t.Fatalf("subjects %+v", got)
	}
	want := []struct {
		id   string
		news News
	}{{"f1", NewsDrift}, {"f2", NewsImage}, {"newer:r1/dex@2.42.0->2.44.0|fred-identity@2.3.0", NewsNewer}, {"newer:r1/dex@2.42.0->2.44.0|barney-base@1.5.0", NewsNewer}, {lostID, NewsLost}}
	for i, w := range want {
		if got[i].ID != w.id || got[i].News != w.news {
			t.Errorf("subject %d is %s %s, want %s %s", i, got[i].ID, got[i].News, w.id, w.news)
		}
	}
	if got[4].Detail != "The observer cannot reach Palette. The last 3 content checks failed: api, HTTP 401." {
		t.Errorf("the detail of the loss: %q", got[4].Detail)
	}
	if none := SubjectsOf(model.Summary{Page: model.PageOK}); len(none) != 0 {
		t.Errorf("a summary with nothing open gave %+v", none)
	}
}

// A message names a profile, its packs, a fixed sentence, and a time. It holds nothing else of a finding,
// also what the summary tier carries: the writer, the images, and the impact.
func TestAMessageHoldsNothingOfAFindingButItsProfilePacksSentenceAndFirstSight(t *testing.T) {
	f := model.Finding{ID: "f1", Kind: model.PublishedChanged, Profile: "fred-identity@2.3.0", Packs: []string{"addon/dex"}, Impact: "IMPACT-MARKER",
		Images: []string{"IMAGE-MARKER:1"}, Who: []docs.Who{{Actor: "ACTOR-MARKER", Action: "ACTION-MARKER"}}, FirstSeen: now}
	s := findingSubject(f)
	for _, text := range []string{openText("quarry-1", s), clearText("quarry-1", s, now), digestText("quarry-1", []Subject{s})} {
		if strings.Contains(text, "MARKER") {
			t.Errorf("the message holds a marker: %s", text)
		}
		if !strings.Contains(text, "fred-identity@2.3.0") {
			t.Errorf("the message lacks the profile: %s", text)
		}
	}
	want := "strata · quarry-1 · Drift\nfred-identity@2.3.0: The published version differs from its record.\nPacks: addon/dex\nFirst seen: 2026-11-04T12:00:00Z"
	if got := openText("quarry-1", s); got != want {
		t.Errorf("the open message\n%s\nwant\n%s", got, want)
	}
}

func TestEachKindOfNewsHasItsTitle(t *testing.T) {
	for _, c := range []struct {
		s    Subject
		want string
	}{
		{Subject{News: NewsDrift, Kind: model.Gone}, titleDrift},
		{Subject{News: NewsImage, Kind: model.ImageMoved}, titleMoved},
		{Subject{News: NewsImage, Kind: model.ImageSplit}, titleSplit},
		{Subject{News: NewsNewer}, titleNewer},
		{Subject{News: NewsLost}, titleLost},
	} {
		if got := c.s.title(); got != c.want {
			t.Errorf("%+v has the title %q, want %q", c.s, got, c.want)
		}
	}
}

func TestADigestListsAtMostThirtySubjectsAndSaysHowManyMore(t *testing.T) {
	var subjects []Subject
	for i := 0; i < digestLimit+5; i++ {
		subjects = append(subjects, Subject{News: NewsNewer, Profile: "p@1", Detail: "x"})
	}
	lines := strings.Split(digestText("quarry-1", subjects), "\n")
	if len(lines) != 2+digestLimit+1 || lines[len(lines)-1] != "and 5 more." {
		t.Errorf("the digest has %d lines, ending %q", len(lines), lines[len(lines)-1])
	}
}

func TestTheSampleIsTheMessageOfAFindingForAMadeUpProfile(t *testing.T) {
	if got := Sample("quarry-1"); !strings.HasPrefix(got, "strata · quarry-1 · Drift\nfred-identity@2.3.0: ") || !strings.HasSuffix(got, "First seen: 2026-11-03T06:09:00Z") {
		t.Errorf("sample %q", got)
	}
}

// The package reads the summary tier only. It names internal/model for the types of the summary and no
// other package of the observer: not internal/docs, which holds the changed values.
func TestThePackageImportsNoPackageThatHoldsAValue(t *testing.T) {
	files, err := filepath.Glob("*.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, name := range files {
		if strings.HasSuffix(name, "_test.go") {
			continue
		}
		parsed, err := parser.ParseFile(token.NewFileSet(), name, nil, parser.ImportsOnly)
		if err != nil {
			t.Fatal(err)
		}
		for _, imp := range parsed.Imports {
			path := strings.Trim(imp.Path.Value, `"`)
			if strings.Contains(path, "/observer/internal/") && !strings.HasSuffix(path, "/internal/model") {
				t.Errorf("%s imports %s", name, path)
			}
		}
	}
	if _, err := os.Stat("subject.go"); err != nil {
		t.Fatal(err)
	}
}
