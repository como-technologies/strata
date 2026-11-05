package web

import (
	"encoding/json"
	"testing"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

const (
	barneyHash = "#p-barney-base@1.5.0"
	ubuntuHash = "#k-barney-base@1.5.0/os/ubuntu-aws"
	ubuntuSide = "side-barney-base@1.5.0-os/ubuntu-aws"
)

// editProfiles changes the profiles of a set.
func editProfiles(t *testing.T, change func(*api.ProfilesView)) func([]byte) []byte {
	return func(raw []byte) []byte {
		var v api.ProfilesView
		if err := devdata.Decode(raw, &v); err != nil {
			t.Fatal(err)
		}
		change(&v)
		out, err := json.Marshal(v)
		if err != nil {
			t.Fatal(err)
		}
		return out
	}
}

// carriedSet is the set read-error, where the last read of barney-base@1.5.0 failed in part and the cause
// read-error names the profile and its packs. The summary keeps the last good answer of the part, and it
// has no flag for it. The profile has a newer version of the pack os/ubuntu-aws, so the pack reads newer
// and not not-checked. This copy also says that the cluster copy is the same as the record.
func carriedSet(t *testing.T) string {
	t.Helper()
	return writeSet(t, devdata.StateReadError, map[string]func([]byte) []byte{"profiles.json": editProfiles(t, func(v *api.ProfilesView) {
		for i, p := range v.Profiles {
			if p.Name == "barney-base" {
				v.Profiles[i].ClusterCopy = &model.ClusterCopy{Status: model.CopySame, RootB: p.Published.RootRecorded}
			}
		}
	})})
}

// A carried answer is the last good answer of a part that the last read could not give. On a side with no change,
// it is no "Identical", no "Undisturbed", and no "Same", also when the pack has a newer version.
func TestACarriedAnswerOnAPackWithANewerVersionIsNotIdenticalUndisturbedOrSame(t *testing.T) {
	dir := carriedSet(t)
	for _, side := range []string{"published", "cluster"} {
		t.Run(side, func(t *testing.T) {
			page := pageWith(t, dir, "scratch", ubuntuHash, `S.side["`+ubuntuSide+`"] = "`+side+`"`)
			mustHold(t, side, page, `<option value="`+side+`" selected>`, "Part of this layer is hidden", "Strata could not read the record.")
			mustNotHold(t, side, page, "Identical", "Undisturbed", `<b class="ok">Same</b>`, "tone-calm")
		})
	}
}

// The profile page and the card "Check this yourself" do not call the sides OK for a profile whose last read failed in part.
func TestACarriedAnswerIsNoOKOnTheProfilePageAndInCheckThisYourself(t *testing.T) {
	dir := carriedSet(t)
	for _, page := range pagesOf(t, dir, barneyHash, ubuntuHash) {
		mustNotHold(t, "the page", page, "Unchanged since the record", "Same content")
		for _, question := range []string{"Does Palette still have this version?", "Does this cluster still run the record?", "What changed?"} {
			row := rowWith(t, page, question)
			mustNotHold(t, question, row, `<span class="badge g">`)
			mustHold(t, question, row, `<span class="badge y">`)
		}
	}
}

// The rule does not reach a profile that no cause names: its sides keep their words.
func TestAProfileWithNoCauseKeepsTheWordsOfItsSides(t *testing.T) {
	dir := carriedSet(t)
	page := pagesOf(t, dir, "#p-bambam-secrets@2.0.1")[0]
	mustHold(t, "the profile", page, "Unchanged since the record")
	mustHold(t, "the row", rowWith(t, page, "Does Palette still have this version?"), `<span class="badge g">OK</span>`)
}
