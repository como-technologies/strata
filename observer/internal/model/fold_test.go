package model

import (
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// dexTag is the path of the tag of the image of dex in its values.
const dexTag = "charts.dex.image.tag"

// The moves are the documents of the real binary, kept in the test data of docs.
var realMoves = []string{testkit.MoveKubernetes, testkit.MoveCertManager, testkit.MoveCilium}

func realValues(t *testing.T, file string) docs.Values {
	t.Helper()
	var d docs.Diff
	testkit.ReadDoc(t, file, &d)
	return d.Packs[0].Values
}

func TestFoldMatchesTheRealMoves(t *testing.T) {
	for _, c := range []struct {
		file string
		want Fold
	}{
		{testkit.MoveKubernetes, Fold{Settings: 1, Follows: 2, Repeats: 8}},
		{testkit.MoveCertManager, Fold{Settings: 9, Follows: 10, Repeats: 6}},
		{testkit.MoveCilium, Fold{Settings: 68, Follows: 28, Repeats: 16}},
	} {
		v := realValues(t, c.file)
		got := FoldOf(v.Paths())
		if got != c.want {
			t.Errorf("%s: got %+v, want %+v", c.file, got, c.want)
		}
		if got.Settings+got.Follows+got.Repeats != len(v.Diff) {
			t.Errorf("%s: the fold %+v does not count each of the %d values", c.file, got, len(v.Diff))
		}
	}
}

func TestSplitPutsEachValueInTheListOfItsGroupAndTheListsMatchTheFold(t *testing.T) {
	for _, file := range realMoves {
		v := realValues(t, file)
		settings, follows, repeats := Split(v)
		want := FoldOf(v.Paths())
		if len(settings) != want.Settings || len(follows) != want.Follows || len(repeats) != want.Repeats {
			t.Errorf("%s: lists %d %d %d, fold %+v", file, len(settings), len(follows), len(repeats), want)
		}
		for _, l := range settings {
			if GroupOf(l.Path) != GroupSettings {
				t.Errorf("%s: %s is not a setting", file, l.Path)
			}
		}
		for _, l := range follows {
			if GroupOf(l.Path) != GroupFollows {
				t.Errorf("%s: %s does not follow the version", file, l.Path)
			}
		}
		for _, l := range repeats {
			if GroupOf(l.Path) != GroupRepeats {
				t.Errorf("%s: %s does not repeat the lists", file, l.Path)
			}
		}
	}
}

func TestSplitGivesEmptyListsAndNeverNull(t *testing.T) {
	settings, follows, repeats := Split(docs.Values{})
	if settings == nil || follows == nil || repeats == nil {
		t.Errorf("%v %v %v", settings, follows, repeats)
	}
}

func TestGroupOfReadsThePathOfAValue(t *testing.T) {
	for path, want := range map[string]Group{
		testkit.ImagePath:                  GroupRepeats,
		"pack.content.charts[1].version":   GroupRepeats,
		dexTag:                             GroupFollows,
		"charts.dex.image.repository":      GroupFollows,
		"charts.dex.config.expiry.idToken": GroupSettings,
		"pack.k8sHardening":                GroupSettings,
	} {
		if got := GroupOf(path); got != want {
			t.Errorf("%s: %v, want %v", path, got, want)
		}
	}
}
