package model

import (
	"encoding/json"
	"os"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
)

// The moves are the documents of the real binary, kept in the test data of docs.
var realMoves = []string{"move-kubernetes.json", "move-cert-manager.json", "move-cni-cilium-oss.json"}

func realValues(t *testing.T, file string) docs.Values {
	t.Helper()
	raw, err := os.ReadFile("../docs/testdata/" + file)
	if err != nil {
		t.Fatal(err)
	}
	var d docs.Diff
	if err := json.Unmarshal(raw, &d); err != nil {
		t.Fatal(err)
	}
	return d.Packs[0].Values
}

func TestFoldMatchesTheRealMoves(t *testing.T) {
	for _, c := range []struct {
		file string
		want Fold
	}{
		{"move-kubernetes.json", Fold{Settings: 1, Follows: 2, Repeats: 8}},
		{"move-cert-manager.json", Fold{Settings: 9, Follows: 10, Repeats: 6}},
		{"move-cni-cilium-oss.json", Fold{Settings: 68, Follows: 28, Repeats: 16}},
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
		"pack.content.images[0].image":     GroupRepeats,
		"pack.content.charts[1].version":   GroupRepeats,
		"charts.dex.image.tag":             GroupFollows,
		"charts.dex.image.repository":      GroupFollows,
		"charts.dex.config.expiry.idToken": GroupSettings,
		"pack.k8sHardening":                GroupSettings,
	} {
		if got := GroupOf(path); got != want {
			t.Errorf("%s: %v, want %v", path, got, want)
		}
	}
}
