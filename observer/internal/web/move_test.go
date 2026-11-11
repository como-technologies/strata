package web

import (
	"os/exec"
	"path/filepath"
	"slices"
	"testing"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

/* The tests below show the move of a profile to its newest published version (#708). The set ok holds wilma-gateway@3.1.2, whose
newest version 3.2.0 moves traefik from 38.0.1 to 38.1.0, changes the values of vendor-product, removes
aws-application-loadbalancer, and adds aws-load-balancer-controller. They need node, like the other tests of the page. */

const (
	wilmaSubject = "wilma-gateway@3.1.2"
	wilmaHash    = "#p-wilma-gateway@3.1.2"
	moveQuestion = "What does the move to the newest version change?"
)

// The page of a profile with a newer version has the card "Move to". It names the impact level and the flags of the move. It lists
// each pack whose version changes, as old → new, each pack that the move adds or removes, and each pack whose values change. A pack
// of the record links to its page. The card "Check this yourself" gives the command of the same compare.
func TestThePageOfAProfileListsWhatTheMoveToItsNewestVersionChanges(t *testing.T) {
	page := shown(t, devdata.StateOK, wilmaHash)[0]
	card := cardOf(t, page, "<h2>Move to ")
	mustHold(t, "the head of the card", card, `<h2>Move to wilma-gateway<span class="ver">@3.2.0</span> <span class="sub">Created `,
		`</h2><span class="badge y">In place</span>`)
	for _, flag := range []string{"IDENTITY", "UPSTREAM", "AUTHORED", "IMAGES", "CHARTS"} {
		mustHold(t, "the flags", card, `<span class="badge b" tabindex="0">`+flag+`</span>`)
	}
	mustHold(t, "the version change", rowWith(t, card, "addon/traefik"),
		`<a href="#k-wilma-gateway@3.1.2/addon/traefik">addon/traefik</a>`, `<td>Changed</td>`, `38.0.1 &rarr; 38.1.0`, `<td>Change</td>`)
	mustHold(t, "the values change", rowWith(t, card, "addon/vendor-product"),
		`<td>Changed</td>`, `2026.2.0 <span class="mut">same version</span>`, `<td>Change</td>`)
	mustHold(t, "the removed pack", rowWith(t, card, "addon/aws-application-loadbalancer"),
		`data-go="#k-wilma-gateway@3.1.2/addon/aws-application-loadbalancer"`, `<td>Removed</td>`, `>2.13.4</td>`)
	// a pack that the move adds is in no record, so it has no page
	added := rowWith(t, card, "addon/aws-load-balancer-controller")
	mustHold(t, "the added pack", added, `<td>Added</td>`, `>2.14.0</td>`)
	mustNotHold(t, "the added pack", added, "data-go=", "<a ")

	var found bool
	for _, r := range termRows(t, page) {
		if r.question == moveQuestion {
			found = true
			if r.command != "strata compare record.json wilma-gateway@3.2.0 --full" || r.answer != `<span class="badge y">In place</span>` {
				t.Errorf("the row of the move: %+v", r)
			}
		}
	}
	if !found {
		t.Error("the card Check this yourself has no row for the move")
	}
	// a profile with no newer version shows no move
	other := shown(t, devdata.StateOK, "#p-fred-identity@2.3.0")[0]
	mustNotHold(t, "a profile with no newer version", other, "<h2>Move to ", moveQuestion)
}

// The Compare menu of a pack page has the choice "Record → Newest version" when strata compared the record with the newest version
// of the profile. A pack that the move changes opens on it, with the full diff of the pack, as "Record → Published now" shows it.
// A pack that the move does not change shows "Undisturbed". A compare that failed gives no choice, and the card "Move to" says so.
func TestTheCompareMenuShowsTheFullDiffOfTheMoveToTheNewestProfileVersion(t *testing.T) {
	vendor := packPage(wilmaSubject, "addon", "vendor-product")
	page := pageOf(t, crawl(t, devdata.StateOK), vendor, "")
	mustHold(t, "the pack page of vendor-product", page, `<option value="newest" selected>Record → Newest version</option>`,
		`<span class="badge r">Changed</span>`, "product.replicas")

	// the move changes no pack but traefik: vendor-product is the same on that side
	dir := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"profiles.json": editProfiles(t, func(v *api.ProfilesView) {
		for i := range v.Profiles {
			if n := v.Profiles[i].NewerVersion; n != nil && n.Move != nil {
				n.Move.Packs = slices.DeleteFunc(n.Move.Packs, func(q model.MovePack) bool { return q.Key != "addon/traefik" })
			}
		}
	})})
	same := pageWith(t, dir, "scratch", vendor, `S.side["side-`+wilmaSubject+`-addon/vendor-product"] = "newest"`)
	mustHold(t, "a pack that the move does not change", same, `<span class="badge g">Identical</span>`,
		"The newest version of the profile holds what the record holds for this pack.")

	// a compare that failed
	dir = writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"profiles.json": editProfiles(t, func(v *api.ProfilesView) {
		for i := range v.Profiles {
			if n := v.Profiles[i].NewerVersion; n != nil {
				n.Move, n.Unread = nil, true
			}
		}
	})})
	pages := pagesOf(t, dir, wilmaHash, vendor)
	mustHold(t, "the card of a move that strata could not compare", cardOf(t, pages[0], "<h2>Move to "),
		`<span class="badge y">Not read</span>`, "Strata could not compare the record with this version now.")
	mustNotHold(t, "the pack page of a move that strata could not compare", pages[1], `<option value="newest"`)
}

// The command of the move of a profile pastes as the arguments it means, also for a name that a shell reads as code.
func TestTheCommandOfTheMoveOfAProfilePastesAsTheArgumentsItMeans(t *testing.T) {
	if _, err := exec.LookPath("bash"); err != nil {
		t.Skip("bash is not on the PATH")
	}
	for _, c := range shellNames {
		t.Run(c.kind, func(t *testing.T) {
			bin, dir := checkWork(t, "")
			writeTool(t, filepath.Join(bin, "strata"), standIn)
			profile := c.value("p")
			set := renamedSet(t, devdata.StateOK, [][2]string{{"wilma-gateway", profile}})
			var rows []termRow
			for _, r := range termRows(t, pagesOf(t, set, "#p-"+encodeURIComponent(profile+"@3.1.2"))[0]) {
				if r.question == moveQuestion {
					rows = append(rows, r)
				}
			}
			if len(rows) != 1 {
				t.Fatalf("%d rows of the move", len(rows))
			}
			want := []string{"compare", "record.json", profile + "@3.2.0", "--full"}
			if got := argumentsOf(t, bin, dir, rows[0].command); got != nil && !slices.Equal(got, want) {
				t.Errorf("the command %q gave the arguments %q, want %q", rows[0].command, got, want)
			}
			if made := markersIn(t, dir); len(made) != 0 {
				t.Errorf("a name ran as code and wrote %q", made)
			}
		})
	}
}
