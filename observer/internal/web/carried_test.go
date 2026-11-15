package web

import (
	"testing"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/testkit"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

const (
	barneyHash = "#p-" + testkit.BaseProfile
	ubuntuHash = "#k-" + testkit.BaseProfile + "/" + testkit.UbuntuAWSKey
	ubuntuSide = "side-" + testkit.BaseProfile + "-" + testkit.UbuntuAWSKey
	// unchangedWords say that the published version of a profile is the same as its record.
	unchangedWords = "Unchanged since the record"
)

// carriedSet is the set read-error, where the last read of barney-base@1.5.0 failed in part and the cause
// read-error names the profile and its packs. The summary keeps the last good answer of the part, and it
// has no flag for it. The profile has a newer version of the pack os/ubuntu-aws, so the pack reads newer
// and not not-checked. This copy also says that the cluster copy is the same as the record.
func carriedSet(t *testing.T) string {
	t.Helper()
	return writeSet(t, devdata.StateReadError, map[string]func([]byte) []byte{"profiles.json": editJSON(t, func(v *api.ProfilesView) {
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
	for _, side := range []string{string(model.SidePublished), string(model.SideCluster)} {
		t.Run(side, func(t *testing.T) {
			page := pageWith(t, dir, scratch, ubuntuHash, `S.side["`+ubuntuSide+`"] = "`+side+`"`)
			mustHold(t, side, page, `<option value="`+side+`" selected>`, hiddenTitle, "Strata could not read the record.")
			mustNotHold(t, side, page, "Identical", "Undisturbed", sameBadge, "tone-calm")
		})
	}
}

// The profile page and the card "Check this yourself" do not call the sides OK for a profile whose last read failed in part.
func TestACarriedAnswerIsNoOKOnTheProfilePageAndInCheckThisYourself(t *testing.T) {
	dir := carriedSet(t)
	for _, page := range pagesOf(t, dir, barneyHash, ubuntuHash) {
		mustNotHold(t, "the page", page, unchangedWords, "Same content")
		for _, question := range []string{qPaletteHas, qClusterRuns, "What changed?"} {
			answer := rowAsking(t, page, question).answer
			mustNotHold(t, question, answer, badgeOf("g"))
			mustHold(t, question, answer, badgeOf("y"))
		}
	}
}

// The rule does not reach a profile that no cause names: its sides keep their words.
func TestAProfileWithNoCauseKeepsTheWordsOfItsSides(t *testing.T) {
	dir := carriedSet(t)
	page := pagesOf(t, dir, "#p-bambam-secrets@2.0.1")[0]
	mustHold(t, "the profile", page, unchangedWords)
	mustHold(t, "the row", rowAsking(t, page, qPaletteHas).answer, badge("g", "OK"))
}
