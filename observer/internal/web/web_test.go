package web

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io/fs"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path"
	"regexp"
	"slices"
	"strconv"
	"strings"
	"testing"
	"testing/fstest"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

func files(t *testing.T) fs.FS {
	t.Helper()
	f, err := Files()
	if err != nil {
		t.Fatal(err)
	}
	return f
}

// noSource is a Source with no data: these tests serve the page only.
type noSource struct{}

func (noSource) Summary() model.Summary { return model.Summary{} }
func (noSource) PackDiff(context.Context, string, string, model.Side) (docs.Pack, bool, error) {
	return docs.Pack{}, false, nil
}

func serve(t *testing.T, target string) *httptest.ResponseRecorder {
	t.Helper()
	h := (&api.Server{Source: noSource{}, Page: api.Page(files(t))}).Handler()
	req := httptest.NewRequest(http.MethodGet, target, nil)
	req.Host = "127.0.0.1:8080"
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, req)
	return rec
}

// types is the Content-Type that each kind of file must have.
var types = map[string]string{
	".html":  "text/html; charset=utf-8",
	".js":    "text/javascript; charset=utf-8",
	".css":   "text/css; charset=utf-8",
	".woff2": "font/woff2",
	".md":    "text/markdown; charset=utf-8",
}

func TestEachAssetIsServedWithItsType(t *testing.T) {
	var found []string
	err := fs.WalkDir(files(t), ".", func(name string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return err
		}
		found = append(found, name)
		want, ok := types[path.Ext(name)]
		if !ok {
			t.Errorf("%s has a type that the test does not know", name)
			return nil
		}
		rec := serve(t, "/"+name)
		if rec.Code != http.StatusOK || rec.Header().Get("Content-Type") != want {
			t.Errorf("%s: status %d, type %q, want %q", name, rec.Code, rec.Header().Get("Content-Type"), want)
		}
		if rec.Header().Get("X-Content-Type-Options") != "nosniff" || !strings.Contains(rec.Header().Get("Content-Security-Policy"), "font-src 'self'") {
			t.Errorf("%s: the headers of the server are missing", name)
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"index.html", "app.js", "app.css", "LICENSES.md", "fonts/public-sans-latin-wght-normal.woff2", "fonts/roboto-mono-latin-wght-normal.woff2"} {
		if !slices.Contains(found, want) {
			t.Errorf("assets/ has no %s", want)
		}
	}
}

func TestTheRouteOfThePageGivesTheIndexAndNoPathElseGivesAFile(t *testing.T) {
	index, err := fs.ReadFile(files(t), "index.html")
	if err != nil {
		t.Fatal(err)
	}
	if rec := serve(t, "/"); rec.Code != http.StatusOK || rec.Body.String() != string(index) {
		t.Errorf("/ gave %d", rec.Code)
	}
	// The page has no icon. A browser asks for one, and a 404 would be an error in its console.
	if rec := serve(t, "/favicon.ico"); rec.Code != http.StatusNoContent || rec.Body.Len() != 0 {
		t.Errorf("/favicon.ico gave %d with %d bytes", rec.Code, rec.Body.Len())
	}
	for _, target := range []string{"/nowhere", "/fonts", "/fonts/", "/app.js/", "/app.txt", "/assets/app.js", "/../app.js", "/./app.js", "/v1/nowhere", "/index.html/x"} {
		if rec := serve(t, target); rec.Code == http.StatusOK {
			t.Errorf("%s gave 200", target)
		}
	}
}

// Each font file has its SHA-256 in LICENSES.md, with its version and the license.
func TestLicensesNamesEachFontWithItsChecksum(t *testing.T) {
	licenses, err := fs.ReadFile(files(t), "LICENSES.md")
	if err != nil {
		t.Fatal(err)
	}
	fonts, err := fs.Glob(files(t), "fonts/*.woff2")
	if err != nil || len(fonts) != 2 {
		t.Fatalf("fonts %v, %v", fonts, err)
	}
	for _, f := range fonts {
		raw, err := fs.ReadFile(files(t), f)
		if err != nil {
			t.Fatal(err)
		}
		sum := sha256.Sum256(raw)
		if !strings.Contains(string(licenses), "`"+f+"`") || !strings.Contains(string(licenses), hex.EncodeToString(sum[:])) {
			t.Errorf("LICENSES.md does not name %s with its checksum", f)
		}
	}
	for _, want := range []string{"OFL-1.1", "SIL OPEN FONT LICENSE Version 1.1", "5.3.0"} {
		if !strings.Contains(string(licenses), want) {
			t.Errorf("LICENSES.md has no %q", want)
		}
	}
}

var (
	externalAddress = regexp.MustCompile(`(?i)https?://|["'(=]\s*//[a-z0-9]`)
	inlineStyle     = regexp.MustCompile(`(?i)\sstyle\s*=`)
	styleTag        = regexp.MustCompile(`(?i)<style\b`)
	scriptTag       = regexp.MustCompile(`(?is)<script\b[^>]*>(.*?)</script>`)
	inlineHandler   = regexp.MustCompile(`(?i)\son[a-z]+\s*=\s*["']`)
	jsComment       = regexp.MustCompile(`(?s:/\*.*?\*/)|(?m:^\s*//.*$)`)
	cssURL          = regexp.MustCompile(`(?i)url\(\s*["']?([^)"']*)`)
	rawCall         = regexp.MustCompile(`\braw\(([^)]*)\)`)
	rawArgument     = regexp.MustCompile(`^"[A-Za-z0-9&;# ]*"$`)
	cssRule         = regexp.MustCompile(`([^{}]+)\{([^{}]*)\}`)
	cssComment      = regexp.MustCompile(`(?s)/\*.*?\*/`)
)

// Selectors that must have no maximum width: the content fills the window at each desktop size.
var wide = []string{".page", ".main", ".app", "body"}

// problems lists what is wrong in the files of a page, in the words of the issue: an external
// address, an inline style, an inline script, a value that is not escaped, and a maximum width.
func problems(files fs.FS) []string {
	var out []string
	_ = fs.WalkDir(files, ".", func(name string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() || name == "LICENSES.md" || path.Ext(name) == ".woff2" {
			return err
		}
		raw, _ := fs.ReadFile(files, name)
		text := string(raw)
		add := func(what string) { out = append(out, name+": "+what) }
		if m := externalAddress.FindString(text); m != "" {
			add("an external address " + m)
		}
		switch path.Ext(name) {
		case ".html":
			problemsInPage(text, add)
		case ".js":
			problemsInScript(text, add)
		case ".css":
			problemsInStyle(text, add)
		}
		return nil
	})
	return out
}

func problemsInPage(text string, add func(string)) {
	if inlineStyle.MatchString(text) || styleTag.MatchString(text) {
		add("an inline style")
	}
	for _, m := range scriptTag.FindAllStringSubmatch(text, -1) {
		if strings.TrimSpace(m[1]) != "" {
			add("an inline script")
		}
	}
	if inlineHandler.MatchString(text) {
		add("an inline event handler")
	}
}

func problemsInScript(text string, add func(string)) {
	text = jsComment.ReplaceAllString(text, "")
	if inlineStyle.MatchString(text) {
		add("an inline style in a string of markup")
	}
	if inlineHandler.MatchString(text) {
		add("an inline event handler in a string of markup")
	}
	for _, banned := range []string{"insertAdjacentHTML", "outerHTML", "document.write", "eval(", "new Function", "setAttribute(", "javascript:", "srcdoc"} {
		if strings.Contains(text, banned) {
			add("it uses " + banned)
		}
	}
	// The one write of markup is put(), and it writes what part() escapes.
	if n := strings.Count(text, "innerHTML"); n != 1 || !strings.Contains(text, "el.innerHTML = part(markup);") {
		add("innerHTML must appear once, in put(), with the value that part() escapes")
	}
	for _, pair := range [][2]string{{`"&": "&amp;"`, "&"}, {`"<": "&lt;"`, "<"}, {`">": "&gt;"`, ">"}, {`'"': "&quot;"`, `"`}, {`"'": "&#39;"`, "'"}} {
		if !strings.Contains(text, pair[0]) {
			add("e() does not escape " + pair[1])
		}
	}
	// raw() marks markup that the code wrote itself, so its argument is a fixed word or entity.
	for _, m := range rawCall.FindAllStringSubmatch(text, -1) {
		if !rawArgument.MatchString(strings.TrimSpace(m[1])) {
			add("raw(" + m[1] + ") holds more than a fixed word")
		}
	}
}

func problemsInStyle(text string, add func(string)) {
	text = cssComment.ReplaceAllString(text, "")
	if strings.Contains(text, "@import") {
		add("an @import")
	}
	for _, m := range cssURL.FindAllStringSubmatch(text, -1) {
		if !strings.HasPrefix(m[1], "fonts/") {
			add("url(" + m[1] + ") is not a file of fonts/")
		}
	}
	for _, rule := range cssRule.FindAllStringSubmatch(text, -1) {
		for _, selector := range strings.Split(rule[1], ",") {
			// A rule inside @media starts after the brace of the block, so its selector is whole.
			if slices.Contains(wide, strings.TrimSpace(selector)) && strings.Contains(rule[2], "max-width") {
				add("the rule of " + strings.TrimSpace(selector) + " holds a max-width")
			}
		}
	}
}

func TestTheAssetsHoldNoExternalAddressNoInlineStyleNoInlineScriptAndNoUnescapedValue(t *testing.T) {
	if got := problems(files(t)); len(got) != 0 {
		t.Errorf("%s", strings.Join(got, "\n"))
	}
}

// The check above must be able to fail, for each kind of problem.
func TestTheCheckFindsEachKindOfProblem(t *testing.T) {
	good := `const e = s => String(s).replace(/[&<>"']/g, c => ESCAPES[c]);
const ESCAPES = {"&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;"};
// raw() is for a fixed word, and innerHTML is for put() alone.
function put(el, markup) {
  el.innerHTML = part(markup);
}
const A = raw(" &rarr; ");`
	cases := []struct {
		name, file, content, want string
	}{
		{"an address in the page", "index.html", `<link rel="stylesheet" href="https://fonts.googleapis.com/css2">`, "an external address"},
		{"an address with no scheme", "index.html", `<script src="//cdn.example/x.js"></script>`, "an external address"},
		{"an address in the script", "app.js", good + "\nfetch('http://example.com')", "an external address"},
		{"an address in the style", "app.css", `@font-face{src:url(https://x.example/a.woff2)}`, "an external address"},
		{"a font outside fonts", "app.css", `@font-face{src:url(other/a.woff2)}`, "is not a file of fonts/"},
		{"an import", "app.css", `@import "other.css";`, "an @import"},
		{"an inline style", "index.html", `<div style="color:red"></div>`, "an inline style"},
		{"a style tag", "index.html", `<style>.a{}</style>`, "an inline style"},
		{"an inline script", "index.html", `<script>alert(1)</script>`, "an inline script"},
		{"an inline handler", "index.html", `<button onclick="go()">x</button>`, "an inline event handler"},
		{"an inline style in a string of the script", "app.js", good + "\nconst s = `<td style=\"width:90px\">`;", "an inline style"},
		{"an inline handler in a string of the script", "app.js", good + "\nconst s = `<a onclick=\"go()\">`;", "an inline event handler"},
		{"a second write of markup", "app.js", good + "\nx.innerHTML = y;", "innerHTML must appear once"},
		{"no write through part", "app.js", strings.Replace(good, "part(markup)", "markup", 1), "innerHTML must appear once"},
		{"eval", "app.js", good + "\neval(x)", "it uses eval("},
		{"a helper that does not escape a quote", "app.js", strings.Replace(good, `'"': "&quot;", `, "", 1), "e() does not escape"},
		{"raw with a value", "app.js", good + "\nconst r = raw(name);", "holds more than a fixed word"},
		{"raw with a template", "app.js", good + "\nconst r = raw(`<b>${name}</b>`);", "holds more than a fixed word"},
		{"a maximum width on the page", "app.css", `.page{padding:18px;max-width:1200px}`, "holds a max-width"},
		{"a maximum width on a list of selectors", "app.css", `.x,.page{max-width:90vw}`, "holds a max-width"},
		{"a maximum width in a media rule", "app.css", `@media (max-width:760px){.page{max-width:600px}}`, "holds a max-width"},
		{"a maximum width on the app", "app.css", `.app{max-width:1400px;margin:auto}`, "holds a max-width"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			got := problems(fstest.MapFS{c.file: {Data: []byte(c.content)}})
			if !strings.Contains(strings.Join(got, "\n"), c.want) {
				t.Errorf("problems %q, want one with %q", got, c.want)
			}
		})
	}
	// A page with none of these is accepted, and a max-width on a rule that is not a page rule is too.
	clean := fstest.MapFS{
		"index.html": {Data: []byte(`<!DOCTYPE html><link rel="stylesheet" href="app.css"><script src="app.js"></script>`)},
		"app.js":     {Data: []byte(good)},
		"app.css":    {Data: []byte(`@font-face{src:url(fonts/a.woff2)} .page{padding:1px} .val{max-width:60ch} @media (max-width:760px){.page{padding:0}}`)},
	}
	if got := problems(clean); len(got) != 0 {
		t.Errorf("a clean page has problems: %q", got)
	}
}

/* The tests below run app.js with node, when node is there. A session has no browser, so the lead
checks the render. These tests stand for the view functions: they fail on a value that reaches the
page as markup, and on a screen that throws. */

// The tags and the attributes that a screen can hold.
var (
	anyTag     = regexp.MustCompile(`<[^>]*>`)
	validTag   = regexp.MustCompile(`^</?(div|span|a|nav|section|table|thead|tbody|tr|th|td|h1|h2|b|i|em|button|input|select|option|label|details|summary|ul|li|noscript)(\s+[a-z][a-z0-9-]*(="[^"<>]*")?)*\s*/?>$`)
	handler    = regexp.MustCompile(`(?i)\son[a-z]+=`)
	attrValue  = regexp.MustCompile(`="[^"]*"`)
	hrefValues = regexp.MustCompile(`href="([^"]*)"`)
)

type screen struct {
	Hash    string `json:"hash"`
	Variant string `json:"variant"`
	HTML    string `json:"html"`
	Error   string `json:"error"`
}

// runNode runs testdata/render.js for one set, with the arguments that follow the set, and gives
// what it prints. It skips the test when node is not there.
func runNode(t *testing.T, state devdata.State, extra ...string) []byte {
	t.Helper()
	node, err := exec.LookPath("node")
	if err != nil {
		t.Skip("node is not on the PATH: the lead checks the render")
	}
	args := append([]string{"testdata/render.js", "assets", "testdata/api", string(state)}, extra...)
	out, err := exec.Command(node, args...).Output()
	if err != nil {
		var exit *exec.ExitError
		if errors.As(err, &exit) {
			t.Fatalf("node: %v\n%s", err, exit.Stderr)
		}
		t.Fatalf("node: %v", err)
	}
	return out
}

// render shows each screen of one set.
func render(t *testing.T, state devdata.State, poison bool) []screen {
	t.Helper()
	var extra []string
	if poison {
		extra = append(extra, "poison")
	}
	out := runNode(t, state, extra...)
	var screens []screen
	if err := json.Unmarshal(out, &screens); err != nil {
		t.Fatalf("the output of render.js is no JSON: %v", err)
	}
	for _, s := range screens {
		if s.Error != "" {
			t.Errorf("%s %s threw: %s", s.Hash, s.Variant, s.Error)
		}
	}
	return screens
}

// checkMarkup fails for a tag that the code does not write, an event attribute, and a link that
// leaves the page.
func checkMarkup(t *testing.T, s screen) {
	t.Helper()
	for _, tag := range anyTag.FindAllString(s.HTML, -1) {
		if !validTag.MatchString(tag) || handler.MatchString(attrValue.ReplaceAllString(tag, `=""`)) {
			t.Errorf("%s %s: the markup holds the tag %.80q", s.Hash, s.Variant, tag)
		}
	}
	for _, m := range hrefValues.FindAllStringSubmatch(s.HTML, -1) {
		if !strings.HasPrefix(m[1], "#") {
			t.Errorf("%s %s: a link leaves the page: %.80q", s.Hash, s.Variant, m[1])
		}
	}
}

const injection = "&lt;img src=x onerror=alert(1)&gt;"

// A name, a path, and a value with <img src=x> show as text on each screen of each set.
func TestAValueWithMarkupShowsAsTextOnEachScreen(t *testing.T) {
	for _, state := range devdata.States {
		t.Run(string(state), func(t *testing.T) {
			screens := render(t, state, true)
			var all strings.Builder
			for _, s := range screens {
				checkMarkup(t, s)
				all.WriteString(s.HTML)
			}
			if strings.Contains(all.String(), "<img") {
				t.Error("a value reached the page as an img tag")
			}
			if state != devdata.StateFirstLoad && !strings.Contains(all.String(), injection) {
				t.Error("the injection does not show as text: the test did not reach a value")
			}
		})
	}
}

// The page shows a pack name with <img src=x> as text, on the screen of the pack.
func TestAPackNameWithAnImgTagShowsAsText(t *testing.T) {
	screens := render(t, devdata.StateOK, true)
	var pack *screen
	for i, s := range screens {
		if strings.HasPrefix(s.Hash, "#k-") && s.Variant == "" && strings.Contains(s.Hash, "dex") {
			pack = &screens[i]
		}
	}
	if pack == nil {
		t.Fatal("no screen of the pack dex")
	}
	if !strings.Contains(pack.HTML, "dex"+injection) {
		t.Errorf("the name of the pack does not show as text: %.400s", pack.HTML)
	}
}

// Each set renders each screen with no error, and says what its state says.
func TestEachSetShowsItsWords(t *testing.T) {
	cases := []struct {
		state devdata.State
		words []string
	}{
		{devdata.StateOK, []string{"Needs attention", "What a check does not cover", "Strata compares the name and the tag.", "2 causes · 26 of 29 packs in full", "6 profiles · 2 this week", "Newer versions"}},
		{devdata.StateQuiet, []string{"Nothing changed", "All 29 packs current", "29 of 29 packs in full", "Nothing needs attention."}},
		{devdata.StateFirstLoad, []string{"The first check has not ended", "Baseline unknown"}},
		{devdata.StateNoRecords, []string{"No records yet", "A record is the copy your team approved. Your pipeline writes records from Git.", "Baseline unknown", "Needs a record"}},
		{devdata.StateStale, []string{"Checks stopped", "expected every 15m"}},
		{devdata.StateLost, []string{"Lost contact with Palette", "HTTP 401, the API key was refused", "no finding clears until a check proves it"}},
		{devdata.StateFirstSeen, []string{"First-seen records", "No person approved these records"}},
	}
	for _, c := range cases {
		t.Run(string(c.state), func(t *testing.T) {
			for _, s := range render(t, c.state, false) {
				checkMarkup(t, s)
				if s.Hash == "#overview" && s.Variant == "" {
					for _, w := range c.words {
						if !strings.Contains(s.HTML, w) {
							t.Errorf("the overview of %s has no %q", c.state, w)
						}
					}
				}
			}
		})
	}
}

// With first-seen records, each screen shows the pill and the words. With approved records, none does.
func TestFirstSeenShowsOnEachScreen(t *testing.T) {
	const pill, words = "First-seen records", "No person approved these records"
	first := render(t, devdata.StateFirstSeen, false)
	if len(first) < 10 {
		t.Fatalf("only %d screens", len(first))
	}
	for _, s := range first {
		if !strings.Contains(s.HTML, pill) || !strings.Contains(s.HTML, words) {
			t.Errorf("%s %s has no pill or no words of first-seen", s.Hash, s.Variant)
		}
	}
	for _, s := range render(t, devdata.StateOK, false) {
		if strings.Contains(s.HTML, pill) || strings.Contains(s.HTML, words) {
			t.Errorf("%s %s of an approved baseline shows first-seen", s.Hash, s.Variant)
		}
	}
}

// The crawl reaches each profile and each pack of the ok set, so the tests above cover each screen.
func TestTheCrawlReachesEachProfileAndEachPack(t *testing.T) {
	set, err := devdata.Load(os.DirFS("testdata/api"), devdata.StateOK, devdata.Reference)
	if err != nil {
		t.Fatal(err)
	}
	reached := map[string]bool{}
	for _, s := range render(t, devdata.StateOK, false) {
		reached[s.Hash] = true
	}
	for _, p := range set.Profiles.Profiles {
		if h := "#p-" + p.Name + "@" + p.Version; !reached[h] {
			t.Errorf("the crawl did not reach %s", h)
		}
		for _, k := range p.Packs {
			if h := "#k-" + p.Name + "@" + p.Version + "/" + k.Layer + "/" + k.Name; !reached[h] {
				t.Errorf("the crawl did not reach %s", h)
			}
		}
	}
}

// The filter "Newer" holds each profile with a pack that has a newer version, also a profile that
// drifts. The tile of newer versions counts the same profiles.
func TestTheFilterNewerHoldsEachProfileWithANewerPack(t *testing.T) {
	cases := []struct {
		state devdata.State
		count int
		names []string
	}{
		{devdata.StateOK, 6, []string{"fred-identity"}},
		{devdata.StateQuiet, 0, nil},
	}
	for _, c := range cases {
		t.Run(string(c.state), func(t *testing.T) {
			set, err := devdata.Load(os.DirFS("testdata/api"), c.state, devdata.Reference)
			if err != nil {
				t.Fatal(err)
			}
			var want []string
			for _, p := range set.Profiles.Profiles {
				if slices.ContainsFunc(p.Packs, func(k model.Pack) bool { return k.Newer != "" }) {
					want = append(want, "#p-"+p.Name+"@"+p.Version)
				}
			}
			if len(want) != c.count || set.Cluster.Counts.NewerProfiles != c.count {
				t.Fatalf("the set has %d profiles with a newer pack and %d in the tile, want %d", len(want), set.Cluster.Counts.NewerProfiles, c.count)
			}
			button := regexp.MustCompile(`data-filter="newer"[^>]*>Newer ` + strconv.Itoa(c.count) + `<`)
			var shown bool
			for _, s := range render(t, c.state, false) {
				if s.Hash != "#profiles" {
					continue
				}
				switch s.Variant {
				case "":
					if !button.MatchString(s.HTML) {
						t.Errorf("the button of the filter does not say Newer %d", c.count)
					}
				case "filter newer":
					shown = true
					if rows := strings.Count(s.HTML, `data-go="#p-`); rows != len(want) {
						t.Errorf("the filter shows %d profiles, want %d", rows, len(want))
					}
					for _, h := range want {
						if !strings.Contains(s.HTML, `href="`+h+`"`) {
							t.Errorf("the filter does not show %s", h)
						}
					}
					for _, name := range c.names {
						if !strings.Contains(s.HTML, `href="#p-`+name+`@`) {
							t.Errorf("the filter does not show %s", name)
						}
					}
				}
			}
			if !shown {
				t.Error("no screen shows the filter Newer")
			}
		})
	}
}

var (
	ageCell  = regexp.MustCompile(`<td class="([^"]*\bw60\b[^"]*)">([^<]*)</td>`)
	anyBlock = regexp.MustCompile(`<div class="([^"]*)">([^<]*)</div>`)
)

// At 760 pixels and below, the age of a finding moves under the labels, so that the row fits with no
// scroll: the cell of the age has the class narrow-hide, and the row holds the age again in a block
// with the class narrow-show.
func TestTheAgeOfAFindingMovesUnderTheLabelsOnAPhone(t *testing.T) {
	var rows int
	for _, s := range render(t, devdata.StateOK, false) {
		for _, row := range strings.Split(s.HTML, "</tr>") {
			cell := ageCell.FindStringSubmatch(row)
			if cell == nil {
				continue
			}
			rows++
			if !slices.Contains(strings.Fields(cell[1]), "narrow-hide") || cell[2] == "" {
				t.Errorf("%s: the cell of the age is %q with the text %q", s.Hash, cell[1], cell[2])
			}
			var under bool
			for _, b := range anyBlock.FindAllStringSubmatch(row, -1) {
				if slices.Contains(strings.Fields(b[1]), "narrow-show") && b[2] == cell[2] {
					under = true
				}
			}
			if !under {
				t.Errorf("%s: the row has no block narrow-show with the age %q", s.Hash, cell[2])
			}
		}
	}
	if rows == 0 {
		t.Error("no screen shows the age of a finding")
	}
}

// ago leaves out the minutes when they are 0.
func TestAgoLeavesOutMinutesThatAreZero(t *testing.T) {
	cases := []struct {
		seconds int
		want    string
	}{
		{30, "under 1m"},
		{60, "1m"},
		{59 * 60, "59m"},
		{2 * 3600, "2h"},
		{2*3600 + 59, "2h"},
		{5*3600 + 51*60, "5h 51m"},
		{3 * 86400, "3d"},
	}
	var extra = []string{"ago"}
	for _, c := range cases {
		extra = append(extra, strconv.Itoa(c.seconds))
	}
	var got []string
	if err := json.Unmarshal(runNode(t, devdata.StateOK, extra...), &got); err != nil {
		t.Fatalf("the output of render.js is no JSON: %v", err)
	}
	if len(got) != len(cases) {
		t.Fatalf("ago gave %d answers for %d ages", len(got), len(cases))
	}
	for i, c := range cases {
		if got[i] != c.want {
			t.Errorf("ago of %d seconds is %q, want %q", c.seconds, got[i], c.want)
		}
	}
}

// ruleOf gives the declarations of the rules with exactly this selector, with no space.
func ruleOf(css, selector string) string {
	var out strings.Builder
	for _, rule := range cssRule.FindAllStringSubmatch(cssComment.ReplaceAllString(css, ""), -1) {
		if strings.TrimSpace(rule[1]) == selector {
			out.WriteString(strings.Join(strings.Fields(rule[2]), "") + ";")
		}
	}
	return out.String()
}

// tabRowProblems lists what is missing in the rule of the row of tabs. A tab has a margin of -1px, so
// the row overflows on the vertical axis, and a scrollbar shows. The row scrolls sideways on a phone.
func tabRowProblems(css string) []string {
	var out []string
	rule := ruleOf(css, ".tabs")
	for _, want := range []string{"overflow-x:auto;", "overflow-y:hidden;"} {
		if !strings.Contains(rule, want) {
			out = append(out, "the rule of .tabs has no "+strings.TrimSuffix(want, ";"))
		}
	}
	return out
}

func TestTheRowOfTabsHasNoVerticalScrollbar(t *testing.T) {
	css, err := fs.ReadFile(files(t), "app.css")
	if err != nil {
		t.Fatal(err)
	}
	if got := tabRowProblems(string(css)); len(got) != 0 {
		t.Errorf("%s", strings.Join(got, "\n"))
	}
	// The check can fail.
	for css, want := range map[string]string{
		`.tabs{display:flex;overflow-x:auto}`:                         "no overflow-y:hidden",
		`.tabs{display:flex;overflow-y:hidden}`:                       "no overflow-x:auto",
		`.tab{overflow-y:hidden}`:                                     "no overflow-x:auto",
		`@media (max-width:760px){.tabs{overflow-x:auto}} .tabs{a:b}`: "no overflow-y:hidden",
	} {
		if got := strings.Join(tabRowProblems(css), "\n"); !strings.Contains(got, want) {
			t.Errorf("problems %q for %s, want one with %q", got, css, want)
		}
	}
	if got := tabRowProblems(`.tabs{display:flex; overflow-x: auto; overflow-y: hidden}`); len(got) != 0 {
		t.Errorf("a good rule has problems: %q", got)
	}
}
