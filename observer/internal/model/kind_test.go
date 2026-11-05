package model

import (
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
)

func TestOnlyTheKindsOfThePodsAreImageKinds(t *testing.T) {
	for _, k := range []Kind{PublishedChanged, Overwritten, Renamed, Gone, ClusterDiffers, ImageIDChanged, ImageMoved, ImageSplit} {
		if got, want := k.IsImage(), k == ImageMoved || k == ImageSplit; got != want {
			t.Errorf("%s: IsImage is %v, want %v", k, got, want)
		}
	}
}

func TestEachKindHasASentenceThatIsTheMessageOfItsEvent(t *testing.T) {
	for _, k := range append(append(append([]Kind{}, publishedKinds...), clusterKinds...), imageKinds...) {
		if k.Sentence() == "" || k.Sentence() != eventKinds[k].message {
			t.Errorf("%s has the sentence %q", k, k.Sentence())
		}
	}
	if Kind("unknown").Sentence() != "" {
		t.Error("a kind that is not in the list has a sentence")
	}
}

func TestTheKeyOfARowIsTheKeyOfItsPin(t *testing.T) {
	pin := docs.Pin{Name: "dex", RegistryUID: "reg1", Version: "2.42.0", Newer: []docs.Newer{{Version: "2.44.0"}}}
	row := Newer{Name: pin.Name, RegistryUID: pin.RegistryUID, Version: pin.Version, Newest: "2.44.0"}
	if row.Key() != MoveKey(pin) || row.Key() != "reg1/dex@2.42.0->2.44.0" {
		t.Errorf("key %q, pin %q", row.Key(), MoveKey(pin))
	}
}
