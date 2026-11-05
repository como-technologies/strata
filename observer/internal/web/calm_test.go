package web

import (
	"encoding/json"
	"os"
	"regexp"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

// editCluster changes the answer of /v1/cluster in a set.
func editCluster(t *testing.T, change func(*api.ClusterView)) func([]byte) []byte {
	return func(raw []byte) []byte {
		var v api.ClusterView
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

// calmImages is the document of the pods of the set ok: the pods are read and no image changed.
func calmImages(t *testing.T) func([]byte) []byte {
	raw, err := devdata.Read(os.DirFS("testdata/api"), devdata.StateOK, "images.json")
	if err != nil {
		t.Fatal(err)
	}
	return func([]byte) []byte { return raw }
}

var greenImageTile = regexp.MustCompile(`<a class="tile g" href="#images"><div class="l">Image changes</div>`)

// calmPlaces are the screens that hold the calm words of the page: the tiles, the empty list of findings,
// the list of newer versions, and the list of events.
var calmPlaces = []string{"#overview", "#findings", "#newer"}

// The set quiet is the one set with a good check. Its words are calm: the tiles say "Nothing changed" and
// "All 29 packs current", the empty list says "All quiet", and the tile of the images is green when the
// pods are read. Each one of these words must go through the gate calmOk(), so none of them stays on a page
// whose checks cannot speak for it.
func TestNoCalmWordGoesAroundTheGate(t *testing.T) {
	const (
		packs   = "All 29 packs current"
		changed = "Nothing changed"
		quiet   = "All quiet"
		nothing = "Nothing needs attention"
		current = "Every pack is current"
	)
	lastRead := func(v *api.ClusterView) {
		v.Content.FailedRuns, v.Content.LastError = 3, &model.ErrorRef{Kind: "api", Status: 401}
	}
	cases := []struct {
		name     string
		edit     map[string]func([]byte) []byte
		code     string
		banned   []string
		greenTil bool
	}{
		{"the versions are not read", map[string]func([]byte) []byte{"cluster.json": editCluster(t, func(v *api.ClusterView) { v.Versions.ReadAt = nil })},
			"", []string{packs}, true},
		{"the page is lost", map[string]func([]byte) []byte{"cluster.json": editCluster(t, func(v *api.ClusterView) { v.Page = model.PageLost; lastRead(v) })},
			"", []string{packs, changed, quiet, nothing, current}, false},
		{"the page is stale", map[string]func([]byte) []byte{"cluster.json": editCluster(t, func(v *api.ClusterView) { v.Page = model.PageStale; lastRead(v) })},
			"", []string{packs, changed, quiet, nothing, current}, false},
		{"the read of the pods failed", map[string]func([]byte) []byte{"images.json": func(raw []byte) []byte {
			return editImages(t, func(o *model.Observed) { o.Error = &model.ErrorRef{Kind: "kube-api", Status: 403} })(calmImages(t)(raw))
		}},
			"", []string{packs, changed, quiet, nothing, current}, false},
		{"the page is offline", nil, "S.offline = true", []string{packs, changed, quiet, nothing, current}, false},
	}
	// The control: with no fault, the set shows each calm word, and the tile of the images is green.
	base := writeSet(t, devdata.StateQuiet, map[string]func([]byte) []byte{"images.json": calmImages(t)})
	control := strings.Join(calmPages(t, base, ""), "\n")
	mustHold(t, "the control", control, packs, changed, quiet, nothing, current)
	if !greenImageTile.MatchString(control) {
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
			if got := greenImageTile.MatchString(pages); got != c.greenTil {
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
		out = append(out, pageWith(t, dir, "scratch", hash, code))
	}
	return out
}
