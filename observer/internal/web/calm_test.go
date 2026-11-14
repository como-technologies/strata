package web

import (
	"os"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

// calmImages is the document of the pods of the set ok: the pods are read and no image changed.
func calmImages(t *testing.T) func([]byte) []byte {
	raw, err := devdata.Read(os.DirFS("testdata/api"), devdata.StateOK, "images.json")
	if err != nil {
		t.Fatal(err)
	}
	return func([]byte) []byte { return raw }
}

// The calm words of the set quiet: the tiles, the empty list of findings, and the list of newer versions.
const (
	calmPacks   = "All 29 packs current"
	calmChanged = "Nothing changed"
	calmQuiet   = "All quiet"
	calmNothing = "Nothing needs attention"
	calmCurrent = "Every pack is current"
)

// greenImageTile is true when the markup shows a tile of the images in green.
func greenImageTile(markup string) bool {
	for _, m := range tileOfImages.FindAllStringSubmatch(markup, -1) {
		if m[1] == "g" {
			return true
		}
	}
	return false
}

// calmPlaces are the screens that hold the calm words of the page: the tiles, the empty list of findings,
// the list of newer versions, and the list of events.
var calmPlaces = []string{"#overview", "#findings", "#newer"}

// The set quiet is the one set with a good check. Its words are calm: the tiles say "Nothing changed" and
// "All 29 packs current", the empty list says "All quiet", and the tile of the images is green when the
// pods are read. Each one of these words must go through the gate calmOk(), so none of them stays on a page
// whose checks cannot speak for it.
func TestNoCalmWordGoesAroundTheGate(t *testing.T) {
	calm := []string{calmPacks, calmChanged, calmQuiet, calmNothing, calmCurrent}
	lastRead := func(v *api.ClusterView) {
		v.Content.FailedRuns, v.Content.LastError = 3, &model.ErrorRef{Kind: docs.KindAPI, Status: 401}
	}
	cases := []struct {
		name     string
		edit     map[string]func([]byte) []byte
		code     string
		banned   []string
		greenTil bool
	}{
		{"the versions are not read", map[string]func([]byte) []byte{"cluster.json": editJSON(t, func(v *api.ClusterView) { v.Versions.ReadAt = nil })},
			"", []string{calmPacks}, true},
		{"the page is lost", map[string]func([]byte) []byte{"cluster.json": editJSON(t, func(v *api.ClusterView) { v.Page = model.PageLost; lastRead(v) })},
			"", calm, false},
		{"the page is stale", map[string]func([]byte) []byte{"cluster.json": editJSON(t, func(v *api.ClusterView) { v.Page = model.PageStale; lastRead(v) })},
			"", calm, false},
		{podsUnread, map[string]func([]byte) []byte{"images.json": chain(calmImages(t), podsRefused(t))}, "", calm, false},
		{"the page is offline", nil, "S.offline = true", calm, false},
	}
	// The control: with no fault, the set shows each calm word, and the tile of the images is green.
	base := writeSet(t, devdata.StateQuiet, map[string]func([]byte) []byte{"images.json": calmImages(t)})
	control := strings.Join(calmPages(t, base, ""), "\n")
	mustHold(t, "the control", control, calm...)
	if !greenImageTile(control) {
		t.Fatal("the control has no green tile of the images")
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			edit := map[string]func([]byte) []byte{"images.json": calmImages(t)}
			for name, f := range c.edit {
				edit[name] = f
			}
			pages := strings.Join(calmPages(t, writeSet(t, devdata.StateQuiet, edit), c.code), "\n")
			mustNotHold(t, c.name, pages, c.banned...)
			if got := greenImageTile(pages); got != c.greenTil {
				t.Errorf("the tile of the images is green: %v, want %v", got, c.greenTil)
			}
		})
	}
}

// calmPages shows the screens that hold calm words, after the code ran in the page.
func calmPages(t *testing.T, dir, code string) []string {
	t.Helper()
	var out []string
	for _, hash := range calmPlaces {
		out = append(out, pageWith(t, dir, scratch, hash, code))
	}
	return out
}
