package model

import (
	"testing"

	"github.com/como-technologies/strata/observer/internal/testkit"
)

func TestOnlyTheKindsOfThePodsAreImageKinds(t *testing.T) {
	for _, k := range allKinds {
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
	p := pin("dex", "addon", "2.42.0", "2.44.0")
	row := Newer{Name: p.Name, RegistryUID: p.RegistryUID, Version: p.Version, Newest: "2.44.0"}
	if row.Key() != MoveKey(p) || row.Key() != "reg1"+testkit.DexMove {
		t.Errorf("key %q, pin %q", row.Key(), MoveKey(p))
	}
}
