package web

import (
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/testkit"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

/* The tests below show the chain of a pack that is behind (#709). The chain is the pack, the newer profile version that carries a
newer version of it, and the record of the cluster. In the set ok, traefik of wilma-gateway@3.1.2 is behind 38.1.0, and
wilma-gateway@3.2.0 carries that version. They need node, like the other tests of the page. */

var (
	carriedByWilma = `carried by <span class="m">` + wilmaNewest + `</span>`
	traefikSlab    = refSpan + traefikPin + `</span>`
)

// Two places name the newer profile version that carries a newer version of a pack that is behind. One is the slab of the pack
// on the page of its profile. The other is the row of the pack in the table of the newer versions. A pack of a profile with no
// newer version names no carrier.
func TestThePackThatIsBehindNamesTheNewerProfileVersionThatCarriesIt(t *testing.T) {
	pages := shown(t, devdata.StateOK, wilmaHash, "#newer", fredHash)
	mustHold(t, "the slab of traefik", slabOf(t, pages[0], traefikSlab),
		`newer: <span class="m">`+ref("traefik", "38.1.0")+`</span> · `+carriedByWilma+` · `)
	mustHold(t, "the row of traefik in the newer versions", rowWith(t, pages[1], `>traefik</a>`),
		`<div class="mut small">`+carriedByWilma+`</div>`)
	mustNotHold(t, "the row of dex, whose profile has no newer version", rowWith(t, pages[1], `>dex</a>`), "carried by")
	mustNotHold(t, "the page of "+testkit.NoNewerProfile, pages[2], "carried by", "carrier not read")
}

// A carrier with no version says that no newer profile version carries the pack. A carrier at a version of the pack below the
// newest names that version first. A carrier that strata could not read says so.
func TestTheWordsOfTheCarrierFollowTheAnswerOfBehind(t *testing.T) {
	for _, c := range []struct {
		name          string
		version, pack string
		unread        bool
		want          string
	}{
		{"no carrier", "", "", false, "carried by no newer profile version"},
		{"a version below the newest", "3.2.0", "38.0.5", false, `<span class="m">38.0.5</span> ` + carriedByWilma},
		{"a read that failed", "", "", true, "carrier not read"},
	} {
		t.Run(c.name, func(t *testing.T) {
			dir := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"profiles.json": editJSON(t, func(v *api.ProfilesView) {
				for i := range v.Profiles {
					for j := range v.Profiles[i].Packs {
						if k := v.Profiles[i].Packs[j].Carried; k != nil {
							k.Version, k.Pack, k.Unread = c.version, c.pack, c.unread
						}
					}
				}
			})})
			slab := slabOf(t, pagesOf(t, dir, wilmaHash)[0], traefikSlab)
			mustHold(t, "the slab of traefik", slab, c.want)
			if c.version == "" {
				mustNotHold(t, "the slab of traefik", slab, carriedByWilma)
			}
		})
	}
}

// The Compare badge of a side of a newer version keeps the neutral badge "Newer" that #643 rules for a newer version. These sides
// are the move of a pack to its newest version, and the move of the profile to its newest version. A side of a drift keeps the
// red badge "Changed".
func TestTheCompareBadgeOfANewerVersionIsTheNeutralBadgeNewer(t *testing.T) {
	screens := crawl(t, devdata.StateOK)
	vendor := packPage(testkit.GatewayProfile, "addon", "vendor-product")
	for _, c := range []struct {
		where, hash, variant, option, badge string
	}{
		{"the newer version of traefik", traefikHash, "", `<option value="newer" selected>`, badge("b", "Newer")},
		{"the newest profile version of vendor-product", vendor, "", newestSelected, badge("b", "Newer")},
		{"the published side of dex", dexHash, "", `<option value="published" selected>`, badge("r", "Changed")},
	} {
		head := compareHead(t, pageOf(t, screens, c.hash, c.variant))
		mustHold(t, c.where, head, c.option, c.badge)
		if c.badge != badge("r", "Changed") {
			mustNotHold(t, c.where, head, badgeOf("r"))
		}
	}
}

// slabOf gives the slab of the stack of packs that holds the text.
func slabOf(t *testing.T, markup, text string) string {
	t.Helper()
	at := strings.Index(markup, text)
	if at < 0 {
		t.Fatalf("no slab holds %q", text)
	}
	start, end := strings.LastIndex(markup[:at], `<li class="pslab`), strings.Index(markup[at:], "</li>")
	if start < 0 || end < 0 {
		t.Fatalf("%q is not in a slab", text)
	}
	return markup[start : at+end]
}

// compareLabel is the label of the dropdown Compare of a pack page.
const compareLabel = `<label for="side" class="small mut">Compare</label>`

// compareHead gives the head of the card of comparison of a pack page: the dropdown, the roots, and the badge of the side.
func compareHead(t *testing.T, markup string) string {
	t.Helper()
	at := strings.Index(markup, compareLabel)
	if at < 0 {
		t.Fatal("the page has no dropdown Compare")
	}
	end := strings.Index(markup[at:], "</div>")
	if end < 0 {
		t.Fatal(cardHead + " Compare has no end")
	}
	return markup[at : at+end]
}
