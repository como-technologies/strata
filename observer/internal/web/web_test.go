package web

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path"
	"path/filepath"
	"regexp"
	"slices"
	"strconv"
	"strings"
	"testing"
	"testing/fstest"

	"github.com/como-technologies/strata/observer/internal/alert"
	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/testkit"
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

func (noSource) Summary() model.Summary   { return model.Summary{} }
func (noSource) Observed() model.Observed { return model.Observed{} }
func (noSource) PackDiff(context.Context, string, string, model.Side) (docs.Pack, bool, error) {
	return docs.Pack{}, false, nil
}
func (noSource) Ask(context.Context, string, string) (model.Answer, bool, error) {
	return model.Answer{}, false, nil
}
func (noSource) Answers() []model.Answer               { return nil }
func (noSource) Alerts() alert.View                    { return alert.Unset() }
func (noSource) StartCycle(string) (model.Start, bool) { return model.Start{}, false }

func serve(t *testing.T, target string) *httptest.ResponseRecorder {
	t.Helper()
	h := (&api.Server{Source: noSource{}, Page: api.Page(files(t))}).Handler()
	req := httptest.NewRequest(http.MethodGet, target, nil)
	req.Host = "127.0.0.1:8080"
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, req)
	return rec
}

func TestEachAssetIsServedWithItsType(t *testing.T) {
	var found []string
	err := fs.WalkDir(files(t), ".", func(name string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return err
		}
		found = append(found, name)
		want, ok := testkit.PageTypes[path.Ext(name)]
		if !ok {
			t.Errorf("%s has a type that the test does not know", name)
			return nil
		}
		rec := serve(t, "/"+name)
		if rec.Code != http.StatusOK || rec.Header().Get("Content-Type") != want {
			t.Errorf("%s: status %d, type %q, want %q", name, rec.Code, rec.Header().Get("Content-Type"), want)
		}
		policy := rec.Header().Get(testkit.HeaderPolicy)
		if rec.Header().Get(testkit.HeaderNoSniff) != "nosniff" || !strings.Contains(policy, "font-src 'self'") || !strings.Contains(policy, "img-src 'self'") {
			t.Errorf(testkit.HeadersMissing, name)
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"index.html", "app.js", themeScript, "app.css", "favicon-light.svg", "favicon-dark.svg", "LICENSES.md", "fonts/public-sans-latin-wght-normal.woff2", "fonts/roboto-mono-latin-wght-normal.woff2"} {
		if !slices.Contains(found, want) {
			t.Errorf("assets/ has no %s", want)
		}
	}
}

// startCall is the call at the top level of a script that starts the page.
var startCall = regexp.MustCompile(`(?m)^start\(\);$`)

// index.html loads each script of assets/ once: theme.js in the head, and the scripts of the screens at the end of the
// body. These scripts share one scope. So the first one holds the template h, and the last one starts the page.
func TestIndexLoadsEachScriptOnceAndTheScriptThatStartsThePageLast(t *testing.T) {
	index := string(readPage(t, "index.html"))
	scripts, err := fs.Glob(files(t), "*.js")
	if err != nil {
		t.Fatal(err)
	}
	var loaded []string
	for _, m := range scriptSrc.FindAllStringSubmatch(index, -1) {
		loaded = append(loaded, m[1])
	}
	if sorted := slices.Sorted(slices.Values(loaded)); !slices.Equal(sorted, scripts) {
		t.Errorf("index.html loads %v, and assets/ holds the scripts %v", loaded, scripts)
	}
	screens := screenScripts(t)
	var tags []string
	for _, name := range screens {
		tags = append(tags, `<script src="`+name+`"></script>`)
	}
	if !strings.Contains(index, strings.Join(tags, "\n")+"\n</body>") {
		t.Errorf("the scripts of the screens %v are not at the end of the body", screens)
	}
	if !strings.Contains(string(readPage(t, screens[0])), "const h = ") {
		t.Errorf("%s loads first, and it does not hold the template h", screens[0])
	}
	for i, name := range screens {
		want := 0
		if i == len(screens)-1 {
			want = 1
		}
		if got := len(startCall.FindAllString(string(readPage(t, name)), -1)); got != want {
			t.Errorf("%s calls start() %d times at its top level, want %d: only the last script starts the page", name, got, want)
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
	// The page links two SVG icons, one for each color scheme. A browser that does not use them asks
	// for /favicon.ico, and a 404 would be an error in its console.
	if rec := serve(t, "/favicon.ico"); rec.Code != http.StatusNoContent || rec.Body.Len() != 0 {
		t.Errorf("/favicon.ico gave %d with %d bytes", rec.Code, rec.Body.Len())
	}
	for _, icon := range []string{"/favicon-light.svg", "/favicon-dark.svg"} {
		if rec := serve(t, icon); rec.Code != http.StatusOK || rec.Body.Len() == 0 {
			t.Errorf("%s gave %d with %d bytes", icon, rec.Code, rec.Body.Len())
		}
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
	jsComment       = regexp.MustCompile(`(?s:(?:^|\s)/\*.*?\*/)|(?m:^\s*//.*$)`)
	cssURL          = regexp.MustCompile(`(?i)url\(\s*["']?([^)"']*)`)
	rawCall         = regexp.MustCompile(`\braw\(([^)]*)\)`)
	rawArgument     = regexp.MustCompile(`^"[A-Za-z0-9&;# ]*"$`)
	cssRule         = regexp.MustCompile(`([^{}]+)\{([^{}]*)\}`)
	cssComment      = regexp.MustCompile(`(?s)/\*.*?\*/`)
	setCall         = regexp.MustCompile(`setAttribute\(\s*([^,)]*)`)
)

// Selectors that must have no maximum width: the content fills the window at each desktop size.
var wide = []string{".page", ".main", ".app", "body"}

// svgNamespace is the one address that an SVG may hold. It is a name, and a browser does not load it.
const svgNamespace = `xmlns="http://www.w3.org/2000/svg"`

// problems lists what is wrong in the files of a page, in the words of the issue: an external
// address, an inline style, an inline script, a value that is not escaped, and a maximum width.
func problems(files fs.FS) []string {
	var out []string
	// screens holds the text of each script of the screens. The rules of the one write of markup count over all of them.
	var screens []string
	_ = fs.WalkDir(files, ".", func(name string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() || name == "LICENSES.md" || path.Ext(name) == ".woff2" {
			return err
		}
		raw, _ := fs.ReadFile(files, name)
		text := string(raw)
		add := func(what string) { out = append(out, name+": "+what) }
		if path.Ext(name) == ".svg" {
			// The namespace of an SVG is a name, and no browser loads it.
			text = strings.ReplaceAll(text, svgNamespace, "")
		}
		if m := externalAddress.FindString(text); m != "" {
			add(problemAddress + " " + m)
		}
		switch path.Ext(name) {
		case ".html":
			problemsInPage(text, add)
		case ".svg":
			problemsInImage(text, add)
		case ".js":
			problemsInScript(name, text, add)
			if name != themeScript {
				screens = append(screens, text)
			}
		case ".css":
			problemsInStyle(text, add)
		}
		return nil
	})
	if len(screens) > 0 {
		problemsInScreens(strings.Join(screens, "\n"), func(what string) { out = append(out, "the scripts of the screens: "+what) })
	}
	return out
}

// The words of the problems that the check of the assets finds, which its test names.
const (
	problemHandler   = "an inline event handler"
	problemInnerHTML = "innerHTML must appear once"
	problemRaw       = "holds more than a fixed word"
	problemAttribute = "sets more than a fixed attribute"
	problemStyle     = "an inline style"
	problemScript    = "an inline script"
	problemAddress   = "an external address"
	problemMaxWidth  = "holds a max-width"
	// inMarkup ends a problem of a string of markup in a script.
	inMarkup = " in a string of markup"
	// screensOnly says where markup is written, oneWrite is that write, and notAFont is the problem of an address of a
	// font.
	screensOnly = "only the scripts of the screens write markup"
	oneWrite    = "el.innerHTML = part(markup);"
	notAFont    = "is not a file of fonts/"
	// inAString and styledCell name and make a problem of a string of the script, and secondWrite names a second write
	// of markup, in the names of the cases.
	inAString   = " in a string of the script"
	styledCell  = "\nconst s = `<td style=\"width:90px\">`;"
	secondWrite = "a second write of markup"
)

func problemsInPage(text string, add func(string)) {
	if inlineStyle.MatchString(text) || styleTag.MatchString(text) {
		add(problemStyle)
	}
	for _, m := range scriptTag.FindAllStringSubmatch(text, -1) {
		if strings.TrimSpace(m[1]) != "" {
			add(problemScript)
		}
	}
	if inlineHandler.MatchString(text) {
		add(problemHandler)
	}
}

// problemsInImage lists what is wrong in an SVG that the page links: an SVG can hold a style, a
// script, and an event attribute, as a page can.
func problemsInImage(text string, add func(string)) {
	problemsInPage(text, add)
	if strings.Contains(strings.ToLower(text), "<script") {
		add("a script")
	}
}

// themeScript is the script of the theme, in the head. It writes no markup, and it has the rules below
// instead. Each other script is a script of the screens. It writes markup, so it has the rules for markup.
const themeScript = "theme.js"

// themeAttributes are the two attributes that the script of the theme can set, each with a fixed name.
var themeAttributes = []string{`"data-theme"`, `"aria-pressed"`}

func problemsInScript(name, text string, add func(string)) {
	text = jsComment.ReplaceAllString(text, "")
	if inlineStyle.MatchString(text) {
		add(problemStyle + inMarkup)
	}
	if inlineHandler.MatchString(text) {
		add(problemHandler + inMarkup)
	}
	banned := []string{"insertAdjacentHTML", "outerHTML", "document.write", "eval(", "new Function", "javascript:", "srcdoc"}
	if name != themeScript {
		banned = append(banned, "setAttribute(")
	}
	for _, word := range banned {
		if strings.Contains(text, word) {
			add("it uses " + word)
		}
	}
	if name == themeScript {
		if strings.Contains(text, "innerHTML") {
			add("it uses innerHTML: " + screensOnly)
		}
		for _, m := range setCall.FindAllStringSubmatch(text, -1) {
			if !slices.Contains(themeAttributes, strings.TrimSpace(m[1])) {
				add("setAttribute(" + m[1] + ") " + problemAttribute + " of the theme")
			}
		}
		return
	}
	// raw() marks markup that the code wrote itself, so its argument is a fixed word or entity.
	for _, m := range rawCall.FindAllStringSubmatch(text, -1) {
		if !rawArgument.MatchString(strings.TrimSpace(m[1])) {
			add("raw(" + m[1] + ") " + problemRaw)
		}
	}
}

// problemsInScreens checks the scripts of the screens as one text, because the page runs them in one scope. The one write
// of markup is put(), and it writes what part() escapes.
func problemsInScreens(text string, add func(string)) {
	text = jsComment.ReplaceAllString(text, "")
	if n := strings.Count(text, "innerHTML"); n != 1 || !strings.Contains(text, oneWrite) {
		add(problemInnerHTML + ", in put(), with the value that part() escapes")
	}
	for _, pair := range [][2]string{{`"&": "&amp;"`, "&"}, {`"<": "&lt;"`, "<"}, {`">": "&gt;"`, ">"}, {`'"': "&quot;"`, `"`}, {`"'": "&#39;"`, "'"}} {
		if !strings.Contains(text, pair[0]) {
			add("e() does not escape " + pair[1])
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
			add("url(" + m[1] + ") " + notAFont)
		}
	}
	for _, rule := range cssRule.FindAllStringSubmatch(text, -1) {
		for _, selector := range strings.Split(rule[1], ",") {
			// A rule inside @media starts after the brace of the block, so its selector is whole.
			if slices.Contains(wide, strings.TrimSpace(selector)) && strings.Contains(rule[2], "max-width") {
				add("the rule of " + strings.TrimSpace(selector) + " " + problemMaxWidth)
			}
		}
	}
}

// noProblems fails when a check found problems in what.
func noProblems(t *testing.T, what string, got []string) {
	t.Helper()
	if len(got) != 0 {
		t.Errorf("%s has problems:\n%s", what, strings.Join(got, "\n"))
	}
}

// findsProblem fails when no problem that a check found in what holds want.
func findsProblem(t *testing.T, what string, got []string, want string) {
	t.Helper()
	if !strings.Contains(strings.Join(got, "\n"), want) {
		t.Errorf("%s: problems %q, want one with %q", what, got, want)
	}
}

func TestTheAssetsHoldNoExternalAddressNoInlineStyleNoInlineScriptAndNoUnescapedValue(t *testing.T) {
	noProblems(t, "assets/", problems(files(t)))
}

// The check above must be able to fail, for each kind of problem.
func TestTheCheckFindsEachKindOfProblem(t *testing.T) {
	good := `const e = s => String(s).replace(/[&<>"']/g, c => ESCAPES[c]);
const ESCAPES = {"&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;"};
// raw() is for a fixed word, and innerHTML is for put() alone.
function put(el, markup) {
  ` + oneWrite + `
}
const A = raw(" &rarr; ");`
	cases := []struct {
		name, file, content, want string
	}{
		{"an address in the page", "index.html", `<link rel="stylesheet" href="https://fonts.googleapis.com/css2">`, problemAddress},
		{"an address with no scheme", "index.html", `<script src="//cdn.example/x.js"></script>`, problemAddress},
		{"an address in the script", "app.js", good + "\nfetch('http://example.com')", problemAddress},
		{"an address in the style", "app.css", `@font-face{src:url(https://x.example/a.woff2)}`, problemAddress},
		{"a font outside fonts", "app.css", `@font-face{src:url(other/a.woff2)}`, notAFont},
		{"an import", "app.css", `@import "other.css";`, "an @import"},
		{problemStyle, "index.html", `<div style="color:red"></div>`, problemStyle},
		{"a style tag", "index.html", `<style>.a{}</style>`, problemStyle},
		{problemScript, "index.html", `<script>alert(1)</script>`, problemScript},
		{"an inline handler", "index.html", `<button onclick="go()">x</button>`, problemHandler},
		{problemStyle + inAString, "app.js", good + styledCell, problemStyle},
		{"an inline handler" + inAString, "app.js", good + "\nconst s = `<a onclick=\"go()\">`;", problemHandler},
		{problemStyle + " after a glob in a string", "app.js", good + "\nconst g = \"releases/*/dev.json\";" + styledCell + "\nconst z = \"*/\";", problemStyle},
		{secondWrite, "app.js", good + "\nx.innerHTML = y;", problemInnerHTML},
		{"no write through part", "app.js", strings.Replace(good, "part(markup)", "markup", 1), problemInnerHTML},
		{"eval", "app.js", good + "\neval(x)", "it uses eval("},
		{"a helper that does not escape a quote", "app.js", strings.Replace(good, `'"': "&quot;", `, "", 1), "e() does not escape"},
		{"raw with a value", "app.js", good + "\nconst r = raw(name);", problemRaw},
		{"raw with a template", "app.js", good + "\nconst r = raw(`<b>${name}</b>`);", problemRaw},
		{"a maximum width on the page", "app.css", `.page{padding:18px;max-width:1200px}`, problemMaxWidth},
		{"a maximum width on a list of selectors", "app.css", `.x,.page{max-width:90vw}`, problemMaxWidth},
		{"a maximum width in a media rule", "app.css", `@media (max-width:760px){.page{max-width:600px}}`, problemMaxWidth},
		{"a maximum width on the app", "app.css", `.app{max-width:1400px;margin:auto}`, problemMaxWidth},
		{"a script in an icon", "favicon-light.svg", `<svg ` + svgNamespace + `><script href="x.js"/></svg>`, "a script"},
		{problemStyle + " in an icon", "favicon-light.svg", `<svg ` + svgNamespace + `><path style="fill:red" d="M0 0"/></svg>`, problemStyle},
		{"a style tag in an icon", "favicon-light.svg", `<svg ` + svgNamespace + `><style>path{fill:red}</style></svg>`, problemStyle},
		{"an address in an icon", "favicon-light.svg", `<svg ` + svgNamespace + `><image href="https://x.example/a.png"/></svg>`, problemAddress},
		{"an event attribute in an icon", "favicon-light.svg", `<svg ` + svgNamespace + ` onload="go()"></svg>`, problemHandler},
		{"innerHTML in the script of the theme", themeScript, `document.body.innerHTML = x;`, screensOnly},
		{"an attribute that is not of the theme", themeScript, `el.setAttribute("onclick", v);`, problemAttribute},
		{"an attribute with a name from a variable", themeScript, `el.setAttribute(name, v);`, problemAttribute},
		{"eval in the script of the theme", themeScript, `eval(x)`, "it uses eval("},
		{"setAttribute in app.js", "app.js", good + "\nel.setAttribute(\"data-theme\", v);", "it uses setAttribute("},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			findsProblem(t, c.name, problems(fstest.MapFS{c.file: {Data: []byte(c.content)}}), c.want)
		})
	}
	// A page with none of these is accepted, and a max-width on a rule that is not a page rule is too.
	clean := fstest.MapFS{
		"index.html": {Data: []byte(`<!DOCTYPE html><link rel="stylesheet" href="app.css"><script src="app.js"></script>`)},
		"app.js":     {Data: []byte(good)},
		themeScript:  {Data: []byte(`document.documentElement.setAttribute("data-theme", v); b.setAttribute("aria-pressed", String(on));`)},
		"icon.svg":   {Data: []byte(`<svg ` + svgNamespace + ` viewBox="0 0 100 100"><path fill="#5b2fc4" d="M3 3L9 9Z"/></svg>`)},
		"app.css":    {Data: []byte(`@font-face{src:url(fonts/a.woff2)} .page{padding:1px} .val{max-width:60ch} @media (max-width:760px){.page{padding:0}}`)},
	}
	noProblems(t, "a clean page", problems(clean))
	// The scripts of the screens count as one text. The escape and the write of markup can be in two scripts, and a write in a
	// third script is a second write.
	escape, write, _ := strings.Cut(good, "// raw()")
	split := fstest.MapFS{"app.js": {Data: []byte(escape)}, "routes.js": {Data: []byte("// raw()" + write)}}
	noProblems(t, "a page with the escape and the write of markup in two scripts", problems(split))
	split["frame.js"] = &fstest.MapFile{Data: []byte("x.innerHTML = y;")}
	findsProblem(t, secondWrite+" in another script", problems(split), problemInnerHTML)
}

/* The tests below run the scripts of the page with node, when node is there. A session has no browser,
so the lead checks the render. These tests stand for the view functions: they fail on a value that
reaches the page as markup, and on a screen that throws. */

// The tags and the attributes that a screen can hold.
var (
	anyTag     = regexp.MustCompile(`<[^>]*>`)
	validTag   = regexp.MustCompile(`^</?(div|span|a|nav|section|table|colgroup|col|thead|tbody|tr|th|td|h1|h2|h3|p|b|i|em|code|button|input|select|option|label|details|summary|ul|li|noscript|svg|path)(\s+[a-zA-Z][a-zA-Z0-9-]*(="[^"<>]*")?)*\s*/?>$`)
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

// whenNodeIsMissing ends a test that needs node. With the variable CI set, the test fails: a runner with
// no node would skip each render test, and the checks would pass with nothing checked. Else the test
// skips, and the lead checks the render.
func whenNodeIsMissing(ci string, fail, skip func(args ...any)) {
	if ci != "" {
		fail("node is not on the PATH, and CI is set: a runner must have node, or the render tests check nothing")
		return
	}
	skip("node is not on the PATH: the lead checks the render")
}

// runNode runs testdata/render.js for one set of testdata/api, with the arguments that follow the
// set, and gives what it prints.
func runNode(t *testing.T, state devdata.State, extra ...string) []byte {
	t.Helper()
	return runNodeIn(t, "testdata/api", state, extra...)
}

// nodeJSON decodes the output of render.js into a value of T, and fails the test when the output is no JSON.
func nodeJSON[T any](t *testing.T, raw []byte) T {
	t.Helper()
	var v T
	if err := json.Unmarshal(raw, &v); err != nil {
		t.Fatalf("the output of render.js is no JSON: %v", err)
	}
	return v
}

// showIn shows some screens of a set in a directory of sets, with no crawl.
func showIn(t *testing.T, dir string, state devdata.State, hashes ...string) []string {
	t.Helper()
	out := nodeJSON[[]string](t, runNodeIn(t, dir, state, append([]string{"show"}, hashes...)...))
	if len(out) != len(hashes) {
		t.Fatalf("the output of render.js holds %d screens for %d hashes", len(out), len(hashes))
	}
	return out
}

// retried runs the mode retry on the screen hash of the set ok. It gives the first render, whose request of a diff
// fails, and the next one.
func retried(t *testing.T, hash string) []string {
	t.Helper()
	pages := nodeJSON[[]string](t, runNode(t, devdata.StateOK, "retry", hash))
	if len(pages) != 2 {
		t.Fatalf("the mode retry gave %d renders, want 2", len(pages))
	}
	return pages
}

// offline runs the mode offline on the overview of the set ok. It gives the page that cannot reach the observer, and the
// page after a click on "Try now" when the observer is back.
func offline(t *testing.T) (before, after string) {
	t.Helper()
	out := nodeJSON[struct{ Before, After string }](t, runNode(t, devdata.StateOK, "offline", "#overview"))
	return out.Before, out.After
}

// request is a request that the page sent.
type request struct {
	Method string `json:"method"`
	Path   string `json:"path"`
}

// requestsOf gives the requests that a crawl of each screen of the set sends.
func requestsOf(t *testing.T, state devdata.State) []request {
	t.Helper()
	return nodeJSON[[]request](t, runNode(t, state, "requests"))
}

// runNodeIn does the same for a set in another directory of sets. It skips the test when node is not
// there, and it fails the test when node is not there and CI is set.
func runNodeIn(t *testing.T, apiDir string, state devdata.State, extra ...string) []byte {
	t.Helper()
	return runNodeAssets(t, "assets", apiDir, state, extra...)
}

// runNodeAssets does the same with the scripts of another directory. A test uses it to run a changed copy of the scripts.
// render.js gets the files of the scripts in the order of the index.html of that directory.
func runNodeAssets(t *testing.T, assets, apiDir string, state devdata.State, extra ...string) []byte {
	t.Helper()
	node, err := exec.LookPath("node")
	if err != nil {
		whenNodeIsMissing(os.Getenv("CI"), t.Fatal, t.Skip)
		return nil
	}
	names, err := scriptsOf(os.DirFS(assets))
	if err != nil {
		t.Fatal(err)
	}
	var scripts []string
	for _, name := range names {
		scripts = append(scripts, filepath.Join(assets, name))
	}
	args := append([]string{"testdata/render.js", strings.Join(scripts, ","), apiDir, string(state)}, extra...)
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
	screens := nodeJSON[[]screen](t, runNode(t, state, extra...))
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

// evil is a tag that runs a script, and injection is the same tag as the page must show it: as text.
const (
	evil      = "<img src=x onerror=alert(1)>"
	injection = "&lt;img src=x onerror=alert(1)&gt;"
)

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

// The words of the overview that more than one test reads. publishedChanged is the label of a finding: the published version
// changed after its record.
const (
	needsAttention   = "Needs attention"
	imageChanges     = "Image changes"
	podImages        = "8 images in pods"
	newerVersions    = "Newer versions"
	publishedChanged = "Published version changed after its record"
	// The words of the read of the pods.
	tagMoved     = "A tag runs another digest than the first one seen"
	tagSplit     = "One tag runs with two digests"
	movedOrSplit = "Moved or split"
	notReadYet   = "Not read yet"
	byNameAndTag = "Strata compares the name and the tag."
	// notSet is the word of the nav item Alerts of an observer that sends no alert.
	notSet = "Not set"
	// The pill and the words of the records: with no record read yet, and with first-seen records.
	unknownRecords = "records: <b>unknown</b>"
	firstSeenPill  = "records: <b>first-seen</b>"
	firstSeenWords = "No pipeline approved these records yet."
	// comparedFirst names the compare of each digest with the first digest of its tag. comparedLink is its link on the
	// overview.
	comparedFirst = "Compared with the first digest"
	comparedSeen  = comparedFirst + " seen in pods"
	comparedLink  = `<a href="#images">` + comparedSeen + `</a>`
	// matchedByName says that a row of an image matched its pack by the name and the tag. whenThisHappens heads the cause
	// of a finding of an image.
	matchedByName   = "matched by name and tag"
	whenThisHappens = "<b>When this happens</b>"
	// dexIDTokens is the path of a value of dex that its change sets.
	dexIDTokens = testkit.DexExpiry + ".idTokens"
	// mutedCell starts a muted cell of the table of the images, catchTab ends the tab of what the pods catch, imagesOn is
	// the nav item Images when it is on, and machineImage ends the sentence of a finding of an image id.
	mutedCell    = `<td class="m nw"><span class="mut">`
	catchTab     = "What pods do catch</button>"
	imagesOn     = `<a href="#images" class="on">`
	machineImage = "changed the machine image on this cluster."
	// The words of the badges of an image and of a pack, and of a row of a profile with a newer version.
	imageMoved     = "Image moved"
	twoDigests     = "Two digests"
	wordNotChecked = "Not checked"
	newerProfile   = "Newer profile version"
	// traefikPin is the pack of the row of wilma-gateway that has a newer version.
	traefikPin = `traefik<span class="ver">@38.0.1</span>`
	// wrapAnywhere is the declaration of a rule that lets a long word wrap.
	wrapAnywhere = "overflow-wrap:anywhere;"
	// podsFailed starts the words of a read of the pods that failed, and podsRefusedWords give the cause of a refusal.
	podsFailed       = "The last read of the pods failed"
	roleRefused      = "HTTP 403, the role of the observer was refused"
	podsRefusedWords = podsFailed + ". <b>" + roleRefused + ".</b>"
)

// imageScreens name the overview, the list of the images, and the page of an image, for the failures of the tests below.
var imageScreens = []string{"the overview", "the list", "the page of the image"}

// Each set renders each screen with no error, and says what its state says.
func TestEachSetShowsItsWords(t *testing.T) {
	// The sets that change no count of ok show these words of its overview.
	const okInFull = "26 of 29 packs in full"
	okOverview := []string{needsAttention, "2 causes · " + okInFull, "6 profiles · 2 this week", newerVersions}
	cases := []struct {
		state devdata.State
		words []string
	}{
		{devdata.StateOK, append([]string{coverJump, comparedSeen, imageChanges, podImages}, okOverview...)},
		{devdata.StateQuiet, []string{calmChanged, calmPacks, "29 of 29 packs in full", quietTitle, calmNothing + ".", byNameAndTag}},
		{devdata.StateFirstLoad, []string{"Reading the layers", firstCheck, unknownRecords}},
		{devdata.StateNoRecords, []string{noRecordsTitle, noRecordsFact, firstSeenCmd, unknownRecords}},
		{devdata.StateStale, []string{"Checks stopped", "expected every 15m"}},
		{devdata.StateLost, []string{lostContact, keyRefused, noClear}},
		{devdata.StateFirstSeen, []string{firstSeenPill, firstSeenWords}},
		{devdata.StateLostUnchecked, []string{lostTitle, lostFact, keySecret}},
		{devdata.StateStaleUnchecked, []string{stoppedTitle, noGoodCheck, kubeLogs}},
		{devdata.StateReadError, []string{needsAttention, "This cluster runs other content than its record", "3 causes · 1 of 6 packs in full"}},
		{devdata.StateImages, []string{needsAttention, imageMoved, tagMoved, movedOrSplit, imageChanges}},
		{devdata.StateImagesSplit, []string{needsAttention, twoDigests, tagSplit, movedOrSplit, imageChanges}},
		{devdata.StateImagesOff, []string{needsAttention, byNameAndTag}},
		{devdata.StateImagesUnread, []string{imageChanges, notReadYet, "3 causes · " + okInFull, byNameAndTag}},
		{devdata.StateAsks, okOverview},
		{devdata.StateTextTooLarge, okOverview},
		{devdata.StateAlerts, []string{needsAttention, "Alerts"}},
		{devdata.StateAlertsUnset, []string{needsAttention, notSet}},
		{devdata.StateMovedDrift, []string{needsAttention, publishedChanged, "3 of 3 packs in full", "1 profile · 1 this week"}},
		{devdata.StateUpdateChecks, okOverview},
	}
	if len(cases) != len(devdata.States) {
		t.Fatalf("the table has %d sets, and devdata lists %d", len(cases), len(devdata.States))
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

// Before the first good check, the page claims nothing. A restart in an outage reaches this state: the
// state file holds the failed runs, and the memory holds no answer. The page then has page lost or
// stale, with checkedAt null, and each count is 0 because nothing was read.
func TestAnOutageBeforeTheFirstGoodCheckClaimsNothing(t *testing.T) {
	banned := []string{calmChanged, "All 0 packs", "0 of 0 packs", "0 packs on this cluster", calmNothing, "needs a record", "Nothing. All"}
	for _, state := range []devdata.State{devdata.StateLostUnchecked, devdata.StateStaleUnchecked} {
		t.Run(string(state), func(t *testing.T) {
			screens := render(t, state, false)
			if len(screens) < 5 {
				t.Fatalf("only %d screens", len(screens))
			}
			for _, s := range screens {
				for _, b := range banned {
					if strings.Contains(s.HTML, b) {
						t.Errorf("%s %s says %q", s.Hash, s.Variant, b)
					}
				}
			}
			for _, s := range screens {
				if s.Hash == "#overview" && s.Variant == "" {
					mustHold(t, "the overview", s.HTML, noGoodCheck)
				}
			}
		})
	}
}

// A published side that strata could not read has no answer. The page says that it was not read, and it sends no
// request for it. It never says that the published version is identical.
func TestAPackWithAnErrorRowDoesNotSayIdenticalForThePublishedSide(t *testing.T) {
	var published, cluster bool
	for _, s := range render(t, devdata.StateReadError, false) {
		if s.Hash != dexHash {
			continue
		}
		switch s.Variant {
		case "side published":
			published = true
			if strings.Contains(s.HTML, "Identical") || !strings.Contains(s.HTML, "This side was not read") || !strings.Contains(s.HTML, "Strata could not read the published version") {
				t.Errorf("the published side of a row with an error: %.600s", s.HTML)
			}
		case "":
			// The finding is on the cluster side, and the cluster side was read.
			cluster = true
			if !strings.Contains(s.HTML, "Changed") || !strings.Contains(s.HTML, dexIDTokens) {
				t.Errorf("the cluster side: %.600s", s.HTML)
			}
		}
	}
	if !published || !cluster {
		t.Errorf("screens: published %v, cluster %v", published, cluster)
	}
}

// A move that strata could not compare keeps its row, and the row has no counts and no impact.
func TestAMoveThatWasNotReadShowsNoCount(t *testing.T) {
	var table, pack bool
	for _, s := range render(t, devdata.StateReadError, false) {
		switch {
		case s.Hash == "#newer" && s.Variant == "":
			table = true
			for _, row := range strings.Split(s.HTML, "</tr>") {
				switch {
				case strings.Contains(row, ">vault<"):
					if !strings.Contains(row, "Not read") || strings.Contains(row, "<b>0</b>") {
						t.Errorf("the row of the move that was not read: %s", row)
					}
				case strings.Contains(row, ">ubuntu-aws<"):
					if strings.Contains(row, "Not read") || !strings.Contains(row, "Repave") {
						t.Errorf("the row of the move that was read: %s", row)
					}
				}
			}
		case s.Hash == "#k-bambam-secrets@2.0.1/addon/vault" && s.Variant == "":
			pack = true
			if !strings.Contains(s.HTML, "Strata could not compare these versions now") || strings.Contains(s.HTML, repaveHint) {
				t.Errorf("the pack of the move that was not read: %.800s", s.HTML)
			}
		}
	}
	if !table || !pack {
		t.Errorf("screens: table %v, pack %v", table, pack)
	}
}

// A request of the detail route that fails is not kept. A 503 is a normal answer while a cycle holds the
// slot, so the next render asks again.
func TestAFailedDetailRequestAsksAgainOnTheNextRender(t *testing.T) {
	pages := retried(t, dexHash)
	if !strings.Contains(pages[0], busyTitle) || !strings.Contains(pages[0], "Try now") {
		t.Errorf("the first render does not say that the request failed: %.600s", pages[0])
	}
	if strings.Contains(pages[1], busyTitle) || !strings.Contains(pages[1], dexIDTokens) {
		t.Errorf("the second render did not ask again: %.600s", pages[1])
	}
}

// With first-seen records, each screen shows the pill and the words. With approved records, none does.
func TestFirstSeenShowsOnEachScreen(t *testing.T) {
	first := render(t, devdata.StateFirstSeen, false)
	if len(first) < 10 {
		t.Fatalf("only %d screens", len(first))
	}
	for _, s := range first {
		if !strings.Contains(s.HTML, firstSeenPill) || !strings.Contains(s.HTML, firstSeenWords) {
			t.Errorf("%s %s has no pill or no words of first-seen", s.Hash, s.Variant)
		}
	}
	for _, s := range render(t, devdata.StateOK, false) {
		if strings.Contains(s.HTML, firstSeenPill) || strings.Contains(s.HTML, firstSeenWords) {
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
	reach := func(hash string) {
		t.Helper()
		if !reached[hash] {
			t.Errorf("the crawl did not reach %s", hash)
		}
	}
	for _, img := range set.Images.Images {
		reach(imageHash(img.Ref))
	}
	for _, p := range set.Profiles.Profiles {
		reach("#p-" + p.Name + "@" + p.Version)
		for _, k := range p.Packs {
			reach("#k-" + p.Name + "@" + p.Version + "/" + k.Layer + "/" + k.Name)
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
					var links []string
					for _, h := range want {
						links = append(links, `href="`+h+`"`)
					}
					for _, name := range c.names {
						links = append(links, `href="#p-`+name+`@`)
					}
					mustHold(t, "the filter", s.HTML, links...)
				}
			}
			if !shown {
				t.Error("no screen shows the filter Newer")
			}
		})
	}
}

// chipCount reads the number of a chip of the filter: <button data-filter="drift">Drift 2</button>.
var chipCount = regexp.MustCompile(`data-filter="[a-z-]+"[^>]*>[A-Za-z ]+ ([^<]*)</button>`)

// The chips of the filter show a number after a good check. A page with nothing read shows a splash in place of the table of
// profiles, so it shows no chip, and a count of 0 would claim a check that did not happen.
func TestTheChipsOfTheFilterShowANumberAfterAGoodCheckAndNoChipBefore(t *testing.T) {
	cases := []struct {
		state devdata.State
		chips bool
	}{
		{devdata.StateFirstLoad, false},
		{devdata.StateNoRecords, false},
		{devdata.StateLostUnchecked, false},
		{devdata.StateStaleUnchecked, false},
		{devdata.StateOK, true},
		{devdata.StateQuiet, true},
	}
	for _, c := range cases {
		t.Run(string(c.state), func(t *testing.T) {
			var chips int
			for _, s := range render(t, c.state, false) {
				for _, m := range chipCount.FindAllStringSubmatch(s.HTML, -1) {
					chips++
					if _, err := strconv.Atoi(m[1]); err != nil {
						t.Errorf("%s %s: a chip says %q, want a number", s.Hash, s.Variant, m[1])
					}
				}
				if !c.chips && strings.Contains(s.HTML, "data-filter=") {
					t.Errorf("%s %s shows a filter before a check ended well", s.Hash, s.Variant)
				}
			}
			if c.chips && chips == 0 {
				t.Error("no screen shows the chips of the filter")
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
	got := nodeJSON[[]string](t, runNode(t, devdata.StateOK, extra...))
	if len(got) != len(cases) {
		t.Fatalf("ago gave %d answers for %d ages", len(got), len(cases))
	}
	for i, c := range cases {
		if got[i] != c.want {
			t.Errorf("ago of %d seconds is %q, want %q", c.seconds, got[i], c.want)
		}
	}
}

// ruleOf gives the declarations of the rules with exactly this selector. It removes each space, so
// `light dark` reads `lightdark`, and it ends each rule with one semicolon.
func ruleOf(css, selector string) string {
	var out strings.Builder
	for _, rule := range cssRule.FindAllStringSubmatch(cssComment.ReplaceAllString(css, ""), -1) {
		if strings.TrimSpace(rule[1]) == selector {
			out.WriteString(strings.TrimSuffix(strings.Join(strings.Fields(rule[2]), ""), ";") + ";")
		}
	}
	return out.String()
}

// ruleProblem gives the problem of the rule of the selector in css when its declarations are not want, or "".
func ruleProblem(css, selector, want string) string {
	if got := ruleOf(css, selector); got != want {
		return fmt.Sprintf("the rule of %s is %q, want %q", selector, got, want)
	}
	return ""
}

// ruleIs fails when the rule of the selector in css does not hold exactly the declarations want.
func ruleIs(t *testing.T, css, selector, want string) {
	t.Helper()
	if p := ruleProblem(css, selector, want); p != "" {
		t.Error(p)
	}
}

// rulesHold fails when the rule of a selector in css lacks one of the declarations that the map gives it.
func rulesHold(t *testing.T, css string, rules map[string][]string) {
	t.Helper()
	for selector, want := range rules {
		got := ruleOf(css, selector)
		for _, w := range want {
			if !strings.Contains(got, w) {
				t.Errorf("the rule of %s is %q, and it lacks %s", selector, got, w)
			}
		}
	}
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
	noProblems(t, "app.css", tabRowProblems(string(css)))
	// The check can fail.
	const noX, noY = "no overflow-x:auto", "no overflow-y:hidden"
	for css, want := range map[string]string{
		`.tabs{display:flex;overflow-x:auto}`:                         noY,
		`.tabs{display:flex;overflow-y:hidden}`:                       noX,
		`.tab{overflow-y:hidden}`:                                     noX,
		`@media (max-width:760px){.tabs{overflow-x:auto}} .tabs{a:b}`: noY,
	} {
		findsProblem(t, css, tabRowProblems(css), want)
	}
	noProblems(t, "a good rule", tabRowProblems(`.tabs{display:flex; overflow-x: auto; overflow-y: hidden}`))
}

/* The tests below hold the look of the page: the logo, the choice of a theme, and the colors. */

// The logo lives in four places. assets/logo/ at the root of the repository is the source. The page
// has a favicon for each color scheme and an inline mark in index.html, and the book has a favicon.
const (
	logoDir     = "../../../assets/logo/"
	bookFavicon = "../../../docs/theme/favicon.svg"
)

// markLight and markDark are the marks of the logo for a light and for a dark background.
const markLight, markDark = "strata-mark-light.svg", "strata-mark-dark.svg"

var logoFiles = []string{
	markLight,
	markDark,
	"strata-lockup-horizontal-light.svg",
	"strata-lockup-horizontal-dark.svg",
	"strata-lockup-stacked-light.svg",
	"strata-lockup-stacked-dark.svg",
}

var (
	markPath = regexp.MustCompile(`<path [^>]*/>`)
	markDiv  = regexp.MustCompile(`(?s)<div class="logo">(.*?)</div>`)
)

func readFile(t *testing.T, name string) []byte {
	t.Helper()
	raw, err := os.ReadFile(name)
	if err != nil {
		t.Fatal(err)
	}
	return raw
}

func readPage(t *testing.T, name string) []byte {
	t.Helper()
	raw, err := fs.ReadFile(files(t), name)
	if err != nil {
		t.Fatal(err)
	}
	return raw
}

// scriptSrc finds the file of each script that index.html loads.
var scriptSrc = regexp.MustCompile(`<script src="([^"]*)"></script>`)

// scriptsOf gives the scripts of the screens of the page in fsys: the scripts that the body of index.html loads, in their
// order. theme.js is the script in the head.
func scriptsOf(fsys fs.FS) ([]string, error) {
	index, err := fs.ReadFile(fsys, "index.html")
	if err != nil {
		return nil, err
	}
	_, body, _ := strings.Cut(string(index), "</head>")
	var names []string
	for _, m := range scriptSrc.FindAllStringSubmatch(body, -1) {
		names = append(names, m[1])
	}
	if len(names) == 0 {
		return nil, errors.New("the body of index.html loads no script")
	}
	return names, nil
}

// screenScripts gives the scripts of the screens of the page that the program carries.
func screenScripts(t *testing.T) []string {
	t.Helper()
	names, err := scriptsOf(files(t))
	if err != nil {
		t.Fatal(err)
	}
	return names
}

// readScripts gives the text of the scripts of the screens, in the order of index.html. A test that reads the script of the
// page as text reads all of them. So a count over the page stays a count over the page.
func readScripts(t *testing.T) string {
	t.Helper()
	var texts []string
	for _, name := range screenScripts(t) {
		texts = append(texts, string(readPage(t, name)))
	}
	return strings.Join(texts, "\n")
}

// copyScripts writes index.html and each script of the screens to a new directory, and gives the directory. change gives the
// text to write for the text of a script. runNodeAssets runs the changed copy.
func copyScripts(t *testing.T, change func(string) string) string {
	t.Helper()
	dir := t.TempDir()
	write := func(name, text string) {
		if err := os.WriteFile(filepath.Join(dir, name), []byte(text), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	write("index.html", string(readPage(t, "index.html")))
	for _, name := range screenScripts(t) {
		write(name, change(string(readPage(t, name))))
	}
	return dir
}

// differsAt is the problem of a copy of the logo that is not its source.
const differsAt = "differs from its source at byte"

// copyProblem says how a copy differs from its source, or gives "" when it is the same, byte for byte.
func copyProblem(name string, source, copy []byte) string {
	if bytes.Equal(source, copy) {
		return ""
	}
	at := 0
	for at < len(source) && at < len(copy) && source[at] == copy[at] {
		at++
	}
	return fmt.Sprintf("%s "+differsAt+" %d (%d bytes, the source has %d)", name, at, len(copy), len(source))
}

func TestEachCopyOfTheLogoIsItsSourceByteForByte(t *testing.T) {
	for _, name := range logoFiles {
		if raw := readFile(t, logoDir+name); !bytes.Contains(raw, []byte("<title>strata</title>")) {
			t.Errorf("%s is not a logo of strata", name)
		}
	}
	light, dark := readFile(t, logoDir+markLight), readFile(t, logoDir+markDark)
	for _, c := range []struct {
		name         string
		source, copy []byte
	}{
		{"favicon-light.svg", light, readPage(t, "favicon-light.svg")},
		{"favicon-dark.svg", dark, readPage(t, "favicon-dark.svg")},
		{"the favicon of the book", light, readFile(t, bookFavicon)},
	} {
		if problem := copyProblem(c.name, c.source, c.copy); problem != "" {
			t.Error(problem)
		}
	}
	// The nav is dark in both schemes, so the inline mark is the dark mark.
	div := markDiv.FindSubmatch(readPage(t, "index.html"))
	if div == nil {
		t.Fatal("index.html has no mark in the logo of the nav")
	}
	want := markPath.FindAll(dark, -1)
	if got := markPath.FindAll(div[1], -1); len(want) != 4 || !slices.EqualFunc(got, want, bytes.Equal) {
		t.Errorf("the inline mark of index.html has %d paths, and they are not the 4 paths of the dark mark", len(got))
	}
}

// The check of a copy must be able to fail.
func TestTheCheckOfACopyFindsAByteThatDiffers(t *testing.T) {
	source := []byte(`<svg><path fill="#5b2fc4"/></svg>`)
	if got := copyProblem("copy", source, slices.Clone(source)); got != "" {
		t.Errorf("a copy that is the same has the problem %q", got)
	}
	for name, copy := range map[string][]byte{
		"a byte of a color":   bytes.Replace(source, []byte("5b2fc4"), []byte("5b2fc5"), 1),
		"a missing last byte": source[:len(source)-1],
		"a byte at the end":   append(slices.Clone(source), '\n'),
	} {
		if got := copyProblem("copy", source, copy); !strings.Contains(got, differsAt) {
			t.Errorf("%s: the problem is %q", name, got)
		}
	}
	if got := copyProblem("copy", source, bytes.Replace(source, []byte("5b2fc4"), []byte("5b2fc5"), 1)); !strings.Contains(got, "at byte 23 ") {
		t.Errorf("the problem is %q, want the byte 23", got)
	}
}

// driftRed is the red of a drift. A drift is white on this red in both schemes, so its rule holds the
// color itself: a token would take a second value in the dark scheme.
const driftRed = "#c9252f"

// redBadge starts the rule of the badge of a drift.
const redBadge = ".badge.r{background:"

// driftProblems lists what is wrong in the two rules that paint a drift: the pill and the count in the nav.
func driftProblems(css string) []string {
	var out []string
	for _, selector := range []string{".badge.r", ".cnt.r"} {
		if p := ruleProblem(css, selector, "background:"+driftRed+";color:#fff;"); p != "" {
			out = append(out, p)
		}
	}
	return out
}

func TestADriftIsWhiteOnRedInBothSchemes(t *testing.T) {
	noProblems(t, "app.css", driftProblems(string(readPage(t, "app.css"))))
	// The two rules as the check wants them.
	const goodBadge, goodCount = redBadge + driftRed + ";color:#fff}", ".cnt.r{background:" + driftRed + ";color:#fff}"
	for name, css := range map[string]string{
		"a token":            redBadge + `var(--red);color:#fff}` + goodCount,
		"another red":        redBadge + `#d92d3a;color:#fff}` + goodCount,
		"a dark copy":        goodBadge + goodCount + `@media (x){` + redBadge + `#ff6b77}}`,
		"another text color": redBadge + driftRed + `;color:#eee}` + goodCount,
		"no count rule":      goodBadge,
	} {
		if got := driftProblems(css); len(got) == 0 {
			t.Errorf("%s: the check found nothing", name)
		}
	}
	noProblems(t, "a good rule", driftProblems(`.badge.r{ background: `+driftRed+`; color: #fff } .cnt.r{background:`+driftRed+`;color:#fff;}`))
}

// The chip "new" is pink, which no status uses. It has the class fresh on each screen that shows it.
func TestTheChipNewHasTheClassFreshAndItsPink(t *testing.T) {
	ruleIs(t, string(readPage(t, "app.css")), ".badge.fresh", "background:light-dark(#fde8f0,#3f1830);color:light-dark(#b01b57,#ff9dbd);")
	chips := regexp.MustCompile(`<span class="([^"]*)">new</span>`).FindAllSubmatch([]byte(readScripts(t)), -1)
	if len(chips) == 0 {
		t.Error("the scripts have no chip new")
	}
	for _, c := range chips {
		if string(c[1]) != "badge fresh" {
			t.Errorf("a chip new has the class %q, want %q", c[1], "badge fresh")
		}
	}
}

var lightDark = regexp.MustCompile(`light-dark\(([^()]*)\)`)

// lightTheme and darkTheme are the rules of the choice of a theme. schemeQuery is the media query of the scheme of the system.
// twoColors is the problem of a light-dark() that holds one color.
const (
	lightTheme  = `:root[data-theme="light"]`
	darkTheme   = `:root[data-theme="dark"]`
	schemeQuery = "prefers-color-scheme"
	twoColors   = "does not hold two colors that differ"
	// schemeOfRoot is the scheme that the root sets, and lightChoice the rule of the choice of light. noSchemeOnRoot and
	// notAChoice are the problems of a root with no scheme and of a rule of a theme that sets more than its scheme.
	schemeOfRoot   = "color-scheme:light dark;"
	lightChoice    = "{color-scheme:light}"
	noSchemeOnRoot = "does not set color-scheme to light dark"
	notAChoice     = "is not a choice of color-scheme"
)

// themeProblems lists what is wrong in the way that a stylesheet holds the two color schemes. A color has
// one line, `light-dark(light, dark)`, so that no color has a second copy for the dark scheme. A person
// who chooses a theme sets data-theme on the root, and that sets color-scheme and no color.
func themeProblems(css string) []string {
	css = cssComment.ReplaceAllString(css, "")
	var out []string
	if strings.Contains(css, schemeQuery) {
		out = append(out, "it has a "+schemeQuery+" rule: a color would have a second copy")
	}
	if !strings.Contains(ruleOf(css, ":root"), "color-scheme:lightdark;") {
		out = append(out, "the rule of :root "+noSchemeOnRoot)
	}
	choices := map[string]string{
		lightTheme: "color-scheme:light;",
		darkTheme:  "color-scheme:dark;",
	}
	tokens := map[string]int{}
	for _, rule := range cssRule.FindAllStringSubmatch(css, -1) {
		selector := strings.TrimSpace(rule[1])
		if _, ok := choices[selector]; !ok && strings.Contains(selector, "data-theme") {
			out = append(out, "the rule of "+selector+" "+notAChoice)
		}
		for _, declaration := range strings.Split(rule[2], ";") {
			if name, _, ok := strings.Cut(strings.TrimSpace(declaration), ":"); ok && strings.HasPrefix(name, "--") {
				tokens[name]++
			}
		}
	}
	for selector, want := range choices {
		if p := ruleProblem(css, selector, want); p != "" {
			out = append(out, p)
		}
	}
	for name, n := range tokens {
		if n > 1 {
			out = append(out, fmt.Sprintf("the token %s has %d lines: a color has one", name, n))
		}
	}
	for _, m := range lightDark.FindAllStringSubmatch(css, -1) {
		parts := strings.Split(m[1], ",")
		if len(parts) != 2 || strings.TrimSpace(parts[0]) == strings.TrimSpace(parts[1]) {
			out = append(out, "light-dark("+m[1]+") "+twoColors)
		}
	}
	return out
}

func TestNoColorHasTwoCopies(t *testing.T) {
	css := string(readPage(t, "app.css"))
	noProblems(t, "app.css", themeProblems(css))
	// A token that differs between the schemes is a light-dark() and not a plain color.
	root := ruleOf(css, ":root")
	for _, token := range []string{"--bg", "--panel", "--ink", "--nav", "--accent", "--red", "--amb", "--violet", "--grn", "--add", "--addw", "--del", "--delw", "--gut"} {
		if !regexp.MustCompile(token + `:light-dark\(`).MatchString(root) {
			t.Errorf("the token %s is not a light-dark()", token)
		}
	}
	// twoTones is the one color of the good stylesheet, with a light and a dark value.
	const twoTones = "light-dark(#fff,#000)"
	const good = `:root{` + schemeOfRoot + `--bg:` + twoTones + `;--navink:#ccc}
` + lightTheme + lightChoice + `
` + darkTheme + `{color-scheme:dark}`
	noProblems(t, "a good stylesheet", themeProblems(good))
	for name, c := range map[string]struct{ css, want string }{
		"a media query":         {good + `@media (` + schemeQuery + `:dark){:root{--bg:#000}}`, schemeQuery},
		"a dark token":          {good + darkTheme + `{--bg:#000}`, "the rule of " + darkTheme},
		"a dark rule":           {good + darkTheme + ` .x{color:#fff}`, notAChoice},
		"a second line":         {good + `:root{--bg:#eee}`, "has 2 lines"},
		"one color twice":       {strings.Replace(good, twoTones, "light-dark(#fff,#fff)", 1), twoColors},
		"one color only":        {strings.Replace(good, twoTones, "light-dark(#fff)", 1), twoColors},
		"no scheme on the root": {strings.Replace(good, schemeOfRoot, "", 1), noSchemeOnRoot},
		"no choice of light":    {strings.Replace(good, lightTheme+lightChoice, "", 1), "the rule of " + lightTheme},
		"a choice with a color": {strings.Replace(good, `{color-scheme:dark}`, `{color-scheme:dark;--bg:#000}`, 1), "the rule of " + darkTheme},
	} {
		findsProblem(t, name, themeProblems(c.css), c.want)
	}
}

// The old blue of the dashboard is gone from each file of assets/, in a color and in a token.
func TestTheOldBlueIsGone(t *testing.T) {
	_ = fs.WalkDir(files(t), ".", func(name string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() || path.Ext(name) == ".woff2" {
			return err
		}
		text := strings.ToLower(string(readPage(t, name)))
		for _, old := range []string{"2f6fde", "6ea2f7", "4d8df0", "--blue"} {
			if strings.Contains(text, old) {
				t.Errorf("%s still has %s", name, old)
			}
		}
		return nil
	})
}

var themeButton = regexp.MustCompile(`<button type="button" data-theme-choice="([a-z]+)" aria-pressed="(true|false)">([A-Za-z]+)</button>`)

// The foot of the nav has three buttons, Auto, Light, and Dark. Each says its state with aria-pressed,
// and Auto is pressed until theme.js reads the choice. theme.js is a file of its own in the head, so
// that it runs before the first paint, and the head links the two icons.
func TestTheFootOfTheNavHasTheControlOfTheTheme(t *testing.T) {
	index := string(readPage(t, "index.html"))
	buttons := themeButton.FindAllStringSubmatch(index, -1)
	want := [][]string{{"auto", "true", "Auto"}, {"light", "false", "Light"}, {"dark", "false", "Dark"}}
	if len(buttons) != len(want) {
		t.Fatalf("index.html has %d buttons of the theme, want %d", len(buttons), len(want))
	}
	for i, b := range buttons {
		if !slices.Equal(b[1:], want[i]) {
			t.Errorf("button %d is %q, want %q", i, b[1:], want[i])
		}
	}
	mustHoldInOrder(t, "the nav of index.html", index, `id="navl"`, `<div class="navfoot">`, `data-theme-choice="dark"`, "</aside>")
	head, body, _ := strings.Cut(index, "</head>")
	if !strings.Contains(head, `<script src="`+themeScript+`"></script>`) || strings.Contains(body, themeScript) {
		t.Error(themeScript + " must be a plain script in the head, and only there")
	}
	for _, icon := range []string{
		`<link rel="icon" type="image/svg+xml" href="favicon-light.svg" media="(` + schemeQuery + `: light)">`,
		`<link rel="icon" type="image/svg+xml" href="favicon-dark.svg" media="(` + schemeQuery + `: dark)">`,
	} {
		if !strings.Contains(head, icon) {
			t.Errorf("the head has no %s", icon)
		}
	}
	script := string(readPage(t, themeScript))
	for _, word := range []string{`"strata-observer-theme"`, "localStorage.removeItem", "aria-pressed", "data-theme-choice", "catch"} {
		if !strings.Contains(script, word) {
			t.Errorf(themeScript+" has no %s", word)
		}
	}
	if strings.Contains(readScripts(t), "localStorage") {
		t.Error("a script of the screens reads the storage: the choice of a theme is in " + themeScript)
	}
}

/* The tests below hold the screens of the pods: the list of images, the page of an image, the finding of
an image, the tile, the nav, and the words for a read that is off or failed. They run with node as the
tests above do. */

// With the variable CI set, a missing node fails the test. Else it skips. This test needs no node.
func TestWithCISetAMissingNodeFailsTheTest(t *testing.T) {
	for _, c := range []struct {
		name, ci      string
		failed, skips bool
	}{{"no CI", "", false, true}, {"CI is true", "true", true, false}, {"CI is 1", "1", true, false}} {
		t.Run(c.name, func(t *testing.T) {
			var failed, skipped bool
			whenNodeIsMissing(c.ci, func(...any) { failed = true }, func(...any) { skipped = true })
			if failed != c.failed || skipped != c.skips {
				t.Errorf("failed %v and skipped %v, want %v and %v", failed, skipped, c.failed, c.skips)
			}
		})
	}
}

// encodeURIComponent writes a text as the function of the same name does in the page. The page keeps
// the @ of a name.
func encodeURIComponent(s string) string {
	var b strings.Builder
	for _, c := range []byte(s) {
		if c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || strings.IndexByte("-_.!~*'()@", c) >= 0 {
			b.WriteByte(c)
		} else {
			fmt.Fprintf(&b, "%%%02X", c)
		}
	}
	return b.String()
}

// imageHash is the hash of the page of an image.
func imageHash(ref string) string { return "#i-" + encodeURIComponent(ref) }

func loadSet(t *testing.T, state devdata.State) *devdata.Set {
	t.Helper()
	set, err := devdata.Load(os.DirFS("testdata/api"), state, devdata.Reference)
	if err != nil {
		t.Fatal(err)
	}
	return set
}

// pageOf finds the screen of a crawl by its hash and its variant, and gives its markup.
func pageOf(t *testing.T, screens []screen, hash, variant string) string {
	t.Helper()
	for _, s := range screens {
		if s.Hash == hash && s.Variant == variant {
			return s.HTML
		}
	}
	t.Fatalf("the crawl has no screen %s %q", hash, variant)
	return ""
}

// rowWith gives the first row of a table in the markup that holds the text.
func rowWith(t *testing.T, html, text string) string {
	t.Helper()
	for _, row := range strings.Split(html, "</tr>") {
		if strings.Contains(row, text) {
			return row
		}
	}
	t.Fatalf("no row holds %q", text)
	return ""
}

// linkRowStart starts a row of a table that leads to a screen. linkRow gives the start of the row that leads to the screen hash.
const linkRowStart = `<tr class="link"`

func linkRow(hash string) string { return linkRowStart + ` ` + goTo(hash) + `>` }

// goTo gives the data attribute of an element that leads to the screen hash.
func goTo(hash string) string { return `data-go="` + hash + `"` }

// rowTo finds the row of a table that leads to the screen hash.
func rowTo(t *testing.T, html, hash string) string {
	t.Helper()
	return rowWith(t, html, goTo(hash))
}

// tileLabel gives the label of a tile of the overview. tileLink gives the start of the link of a tile to the screen hash, up
// to its label.
func tileLabel(label string) string { return `<div class="l">` + label + `</div>` }

func tileLink(hash, label string) string { return `href="` + hash + `"><div class="l">` + label }

// tileValue gives the value of a tile of the overview.
func tileValue(value string) string { return `<div class="v">` + value + `</div>` }

// badgeOf is the start of a badge of the class, and badge is the badge of the class with its word.
func badgeOf(class string) string { return `<span class="badge ` + class + `">` }

func badge(class, word string) string { return badgeOf(class) + word + "</span>" }

// navLink is the start of the link of the nav to the screen hash.
func navLink(hash string) string { return `<a href="` + hash + `" class=` }

func mustHold(t *testing.T, where, html string, words ...string) {
	t.Helper()
	for _, w := range words {
		if !strings.Contains(html, w) {
			t.Errorf("%s has no %q", where, w)
		}
	}
}

func mustNotHold(t *testing.T, where, html string, words ...string) {
	t.Helper()
	for _, w := range words {
		if strings.Contains(html, w) {
			t.Errorf("%s holds %q", where, w)
		}
	}
}

// The badge of an image, and the badge of one digest, as the page draws them.
var (
	imageBadges = map[model.ImageStatus]string{
		model.ImageStatusSteady:     badge("g", "First digest"),
		model.ImageStatusMoved:      badge("a", imageMoved),
		model.ImageStatusSplit:      badge("r", twoDigests),
		model.ImageStatusNotChecked: badge("y", wordNotChecked),
	}
	digestBadges = map[model.DigestStatus]string{
		model.FirstDigest:   badge("g", "First digest"),
		model.AnotherDigest: badge("r", "Another digest"),
		model.NoRepoDigest:  badge("y", wordNotChecked) + ` <span class="mut">No repo digest</span>`,
		model.NotStarted:    badge("y", wordNotChecked) + ` <span class="mut">Not started</span>`,
	}
)

// profileBadges is the badge of the status of a profile. The row Record of an image shows it.
var profileBadges = map[model.Status]string{
	model.StatusDrift:      badge("r", "Drift"),
	model.StatusNewer:      badge("b", "Newer"),
	model.StatusNotChecked: badge("y", wordNotChecked),
	model.StatusOK:         badge("g", "OK"),
	model.StatusNoRecord:   badge("y", "No record"),
}

// cutDigest writes a digest as the page does: sha256: and 12 digits.
func cutDigest(d string) string { return "sha256:" + strings.TrimPrefix(d, "sha256:")[:12] }

// packName is the name of a pack of the key layer/name.
func packName(key string) string { _, name, _ := strings.Cut(key, "/"); return name }

// The sets that hold images that a pod runs.
var imageSets = []devdata.State{devdata.StateOK, devdata.StateImages, devdata.StateImagesSplit}

// The list shows the count line, and one row for each image: its status, its reference, its pack, the
// number of its pods, and its digest. An image with no digest of a repo reads Not checked and never OK.
func TestTheListOfImagesShowsEachImageWithItsStatusPackPodsAndDigest(t *testing.T) {
	for _, state := range imageSets {
		t.Run(string(state), func(t *testing.T) {
			set := loadSet(t, state)
			o := set.Images
			list := pageOf(t, render(t, state, false), "#images", "")
			mustHold(t, "the list", list, fmt.Sprintf(`<h2>Images <span class="sub">%d images. %d match a pack. %d do not.</span></h2>`, o.Counts.Images, o.Counts.Matched, o.Counts.Images-o.Counts.Matched),
				`data-col="status">Status<`, `data-col="image">Image<`, `<th class="narrow-hide" aria-sort="none"><button type="button" class="sort" data-sort="it" data-col="pack">Pack<`,
				`<th class="n narrow-hide" aria-sort="none"><button type="button" class="sort" data-sort="it" data-col="pods">Pods<`, `<th>Digest</th></tr></thead>`)
			if got := strings.Count(list, linkRowStart); got != len(o.Images) {
				t.Errorf("the list has %d rows for %d images", got, len(o.Images))
			}
			for _, img := range o.Images {
				row := rowTo(t, list, imageHash(img.Ref))
				mustHold(t, img.Ref, row, imageBadges[img.Status], `<td class="m wrap"><a href="`+imageHash(img.Ref)+`">`+img.Ref+`</a>`)
				var pods int
				var real []string
				for _, d := range img.Digests {
					pods += d.Pods
					if d.Digest != "" {
						real = append(real, d.Digest)
					}
				}
				mustHold(t, img.Ref, row, fmt.Sprintf(`<td class="n narrow-hide">%d</td>`, pods))
				switch {
				case len(real) == 0:
					mustHold(t, img.Ref, row, mutedCell+wordNotChecked+`</span></td>`)
					mustNotHold(t, img.Ref, row, ">OK<", "sha256:")
				case len(real) == 1:
					mustHold(t, img.Ref, row, `>`+cutDigest(real[0])+`</span></td>`)
				default:
					mustHold(t, img.Ref, row, `>`+cutDigest(real[0])+`</span> <span class="mut">+`+strconv.Itoa(len(real)-1)+`</span></td>`)
				}
				if img.Match == model.MatchNone {
					mustHold(t, img.Ref, row, `<span class="mut">no pack</span>`)
					mustNotHold(t, img.Ref, row, "<i>pack=</i>", matchedByName)
				} else {
					mustHold(t, img.Ref, row, `<i>pack=</i>"`+packName(img.Packs[0].Key)+`"`)
				}
				if says := strings.Contains(row, matchedByName); (img.Match == model.MatchNameAndTag) != says {
					t.Errorf("%s has the match %s, and the row says %v about name and tag", img.Ref, img.Match, says)
				}
			}
		})
	}
}

// At 760 pixels and below, the columns Pack and Pods go, and the pack moves under the image, so that
// the row fits: the block with the class narrow-show holds what the cell with the class narrow-hide holds.
func TestTheListOfImagesMovesThePackUnderTheImageOnAPhone(t *testing.T) {
	cell := regexp.MustCompile(narrowCell + `(.*?)</td>`)
	list := pageOf(t, render(t, devdata.StateImages, false), "#images", "")
	var rows int
	for _, row := range strings.Split(list, "</tr>") {
		m := cell.FindStringSubmatch(row)
		if m == nil {
			continue
		}
		rows++
		if !strings.Contains(row, `<div class="narrow-show">`+m[1]+`</div>`) {
			t.Errorf("the row has no block narrow-show with %q: %s", m[1], row)
		}
	}
	if rows == 0 {
		t.Error("no row of the list has a pack cell")
	}
}

// The page of an image shows its header, the two tabs, and the table of the record and of the pods.
// A moved or split image has a note and the sentence of the fix. A digest that is not a repo digest
// reads Not checked, with the reason.
func TestThePageOfAnImageShowsTheRecordAndEachDigestOfThePods(t *testing.T) {
	for _, state := range imageSets {
		t.Run(string(state), func(t *testing.T) {
			set := loadSet(t, state)
			screens := render(t, state, false)
			for _, img := range set.Images.Images {
				page := pageOf(t, screens, imageHash(img.Ref), "")
				named, match := "a tag", "Exact"
				if img.ByDigest {
					named = "a digest"
				}
				switch img.Match {
				case model.MatchNameAndTag:
					match = "Matched by name and tag"
				case model.MatchNone:
					match = "No pack"
				}
				mustHold(t, img.Ref, page, `<h1 class="m h1m">`+img.Ref+`</h1>`+imageBadges[img.Status], "<span>Named by</span>"+named+"</div>", "<span>Match</span>"+match+"</div>",
					`<nav class="crumbs" aria-label="Where you are"><a href="#images">Images</a> <span>/</span> <b>`+img.Ref+`</b></nav>`,
					`data-k="this">This image</button>`, `data-k="catch">`+catchTab,
					`<th>Where</th><th class="narrow-hide">Reference</th><th>Digest</th><th>Status</th>`)
				mustNotHold(t, img.Ref, page, "Your real case", "With a source read")
				for _, p := range img.Packs {
					mustHold(t, img.Ref, page, `<i>pack=</i>"`+packName(p.Key)+`"`, `<a class="m" href="#p-`+encodeURIComponent(p.Profile)+`">`+p.Profile+`</a>`)
				}
				if got := strings.Count(page, "<td>Record"); got != len(img.Packs) {
					t.Errorf("%s has %d rows of the record for %d packs", img.Ref, got, len(img.Packs))
				}
				for _, p := range img.Packs {
					record := rowWith(t, page, "<tr><td>Record")
					if img.ByDigest {
						mustHold(t, img.Ref, record, `<td class="m nw">digest</td>`)
					} else {
						mustHold(t, img.Ref, record, mutedCell+`tag only</span></td>`)
					}
					for _, x := range set.Profiles.Profiles {
						if model.Subject(x.Name, x.Version) == p.Profile {
							mustHold(t, img.Ref, record, profileBadges[x.Status])
						}
					}
				}
				var pods []string
				for _, row := range strings.Split(page, "</tr>") {
					if strings.Contains(row, "<tr><td>Pods (") {
						pods = append(pods, row)
					}
				}
				if len(pods) != len(img.Digests) {
					t.Fatalf("%s has %d rows of pods for %d digests", img.Ref, len(pods), len(img.Digests))
				}
				for i, d := range img.Digests {
					mustHold(t, img.Ref, pods[i], fmt.Sprintf("<td>Pods (%d)</td>", d.Pods), digestBadges[d.Status], `<td class="m wrap narrow-hide">`+img.Ref+`</td>`)
					if d.Digest == "" {
						mustHold(t, img.Ref, pods[i], mutedCell+`none</span></td>`)
					} else {
						mustHold(t, img.Ref, pods[i], `<td class="m nw"><span title="`+d.Digest+`">`+cutDigest(d.Digest)+`</span></td>`)
					}
				}
				if img.Status == model.ImageStatusMoved || img.Status == model.ImageStatusSplit {
					cls := map[model.ImageStatus]string{model.ImageStatusMoved: "a", model.ImageStatusSplit: "r"}[img.Status]
					mustHold(t, img.Ref, page, `<div class="note `+cls+` small">`, "<h2>The fix</h2>", "Name the image by its digest where the chart or the manifest reads it", "a registry with immutable tags")
				} else {
					mustNotHold(t, img.Ref, page, `class="note`, "The fix")
				}
			}
		})
	}
}

// Under the tab "What pods do catch", the page says what the read catches and what it does not. The tab
// "This image" shows the table, and this one does not.
func TestTheTabWhatPodsDoCatchIsTheSameForEachImage(t *testing.T) {
	for _, state := range imageSets {
		set := loadSet(t, state)
		screens := render(t, state, false)
		for _, img := range set.Images.Images {
			id := "tab image-" + img.Ref
			catch, this := pageOf(t, screens, imageHash(img.Ref), id+" catch"), pageOf(t, screens, imageHash(img.Ref), id+" this")
			mustHold(t, img.Ref, catch, tabOn("image-"+img.Ref, "catch")+catchTab,
				whenThisHappens, "A tag pulled directly from a mutable registry, when a pod lands on a node with no cached copy.",
				`<span class="m">imagePullPolicy: Always</span> or <span class="m">:latest</span>, at any restart.`, "A mutable mirror, or a pipeline that copies over the same tag.",
				"Needs read access to pods, and no registry and no credential. No Palette route has this digest.",
				"<b>What it does not catch</b>", "A move with no new pull.", "An image that no pod runs.", "A difference between clusters.",
				"It stays silent when the pods pull an unchanged digest from a mirror")
			mustNotHold(t, img.Ref, catch, "<th>Where</th>", "<th>Digest</th>")
			mustHold(t, img.Ref, this, tabOn("image-"+img.Ref, "this")+"This image</button>", "<th>Where</th>")
			mustNotHold(t, img.Ref, this, whenThisHappens)
		}
	}
}

// The finding of an image has the badge of its image and not the badge of a drift, and it opens the page
// of the image. The Drift tile does not count it.
func TestTheFindingOfAnImageHasItsOwnBadgeAndOpensTheImage(t *testing.T) {
	for state, c := range map[devdata.State]struct{ badge, title string }{
		devdata.StateImages:      {badge("a", imageMoved), tagMoved},
		devdata.StateImagesSplit: {badge("r", twoDigests), tagSplit},
	} {
		t.Run(string(state), func(t *testing.T) {
			overview := pageOf(t, render(t, state, false), "#overview", "")
			href := imageHash(testkit.VendorRef)
			row := rowTo(t, overview, href)
			mustHold(t, "the finding", row, `<td class="w90">`+c.badge+`</td>`, `<a href="`+href+`">`+c.title+`</a>`,
				`<span class="lab"><i>profile=</i>"`+wilmaSubject+`"</span>`, `<span class="lab wrap"><i>image=</i>"`+testkit.VendorRef+`"</span>`)
			mustNotHold(t, "the finding", row, ">Drift<")
			mustHold(t, "the tile", overview, `<a class="tile r" `+tileLink("#findings", "Drift")+"</div>"+tileValue("1"))
			mustHold(t, "the nav", overview, `<span class="cnt r">2</span>`)
		})
	}
}

// A profile that drifts, and a profile that does not, both say the finding of an image in the list.
func TestTheListOfProfilesSaysTheFindingOfAnImage(t *testing.T) {
	profiles := pageOf(t, render(t, devdata.StateImages, false), "#profiles", "")
	row := rowTo(t, profiles, wilmaHash)
	mustHold(t, "the row", row, traefikPin, "<div>vendor-product ran another digest in pods than the first one seen.</div>")
}

// Issue #707. The row of a profile names the newer published version of the profile itself. Its words
// differ from the words of a pack with a newer version. The tile of the newer versions counts it,
// and the filter "Newer" holds it.
func TestTheRowOfAProfileNamesItsNewerProfileVersionInWordsOfItsOwn(t *testing.T) {
	screens := render(t, devdata.StateOK, false)
	profiles := pageOf(t, screens, "#profiles", "")
	wilma := rowTo(t, profiles, wilmaHash)
	mustHold(t, "the row of wilma-gateway", wilma,
		newerProfile+` wilma-gateway<span class="ver">@3.2.0</span>. `+badge("fresh", "new"),
		traefikPin)
	// a profile with a pack that has a newer version, and no newer version of its own
	barney := rowTo(t, profiles, barneyHash)
	mustNotHold(t, "the row of barney-base", barney, newerProfile)
	overview := pageOf(t, screens, "#overview", "")
	mustHold(t, "the tile", overview, ` · 1 newer profile version</div>`)
}

var (
	tileOfImages = regexp.MustCompile(`<a class="tile (\w+)" ` + tileLink("#images", imageChanges) + `</div><div class="v">([^<]*)</div><div class="s">([^<]*)</div></a>`)
	navOfImages  = regexp.MustCompile(`<a href="#images" class="[^"]*"><span>Images</span><span class="cnt">([^<]*)</span></a>`)
)

// The tile "Image changes" sits between Drift and Newer versions. The nav item Images sits between Newer
// versions and Not checked. An observer that reads the pods shows the tile, and before its first read each says "-" and
// claims nothing. An observer that reads no pods shows no tile, and the nav item says "Off".
func TestTheTileAndTheNavItemOfTheImagesFollowTheRead(t *testing.T) {
	cases := []struct {
		state                    devdata.State
		cls, value, sub, counted string
		noTile                   bool
	}{
		{devdata.StateOK, "g", "0", podImages, "8", false},
		{devdata.StateImages, "a", "1", movedOrSplit, "8", false},
		{devdata.StateImagesSplit, "a", "1", movedOrSplit, "8", false},
		{devdata.StateQuiet, "", "", "", "Off", true},
		{devdata.StateNoRecords, "", "", "", "Off", true},
		{devdata.StateReadError, "", "", "", "Off", true},
		{devdata.StateImagesOff, "", "", "", "Off", true},
		{devdata.StateImagesUnread, "y", "-", notReadYet, "-", false},
	}
	for _, c := range cases {
		t.Run(string(c.state), func(t *testing.T) {
			overview := pageOf(t, render(t, c.state, false), "#overview", "")
			// A page with no records shows a splash in place of the tiles.
			m := tileOfImages.FindStringSubmatch(overview)
			if c.noTile {
				if m != nil {
					t.Errorf("the page shows the tile %q with no read of the pods", m)
				}
			} else if m == nil || m[1] != c.cls || m[2] != c.value || m[3] != c.sub {
				t.Errorf("the tile is %q, want %s %q %q", m, c.cls, c.value, c.sub)
			}
			n := navOfImages.FindStringSubmatch(overview)
			if n == nil || n[1] != c.counted {
				t.Errorf("the nav item is %q, want the count %q", n, c.counted)
			}
			// The tiles and the nav keep one order: findings, images, newer versions, not checked, profiles.
			tiles := []string{tileLink("#findings", "Drift"), tileLink("#newer", newerVersions), tileLink("#notchecked", wordNotChecked), tileLink("#profiles", "Profiles")}
			if !c.noTile {
				tiles = slices.Insert(tiles, 1, tileLink("#images", imageChanges))
			}
			if tileSection.MatchString(overview) {
				mustHoldInOrder(t, "the tiles", overview, tiles...)
			}
			nav := []string{navLink("#findings"), navLink("#images"), navLink("#newer"), navLink("#notchecked"), navLink("#profiles")}
			mustHoldInOrder(t, "the nav", overview, nav...)
		})
	}
}

// indexes gives the place of each of the words in s, or -1 for a word that s does not hold.
func indexes(s string, words []string) []int {
	at := make([]int, len(words))
	for i, w := range words {
		at[i] = strings.Index(s, w)
	}
	return at
}

// mustHoldInOrder fails when the html does not hold each of the words, or holds them in another order.
func mustHoldInOrder(t *testing.T, where, html string, words ...string) {
	t.Helper()
	if at := indexes(html, words); !slices.IsSorted(at) || slices.Contains(at, -1) {
		t.Errorf("%s does not hold %q in this order: %v", where, words, at)
	}
}

var tileSection = regexp.MustCompile(`(?s)<section class="tiles".*?</section>`)

// With no read of the pods, the tile "Image changes" is not there, and the other four tiles fill the row. The nav item stays and
// says "Off", as the nav item Alerts says "Not set". It opens the offer of the read.
func TestWithNoReadOfThePodsTheTileIsGoneAndTheNavItemSaysOff(t *testing.T) {
	for _, state := range []devdata.State{devdata.StateQuiet, devdata.StateImagesOff, devdata.StateReadError} {
		t.Run(string(state), func(t *testing.T) {
			screens := render(t, state, false)
			overview := pageOf(t, screens, "#overview", "")
			tiles := tileSection.FindString(overview)
			if got := strings.Count(tiles, `<a class="tile `); got != 4 {
				t.Errorf("the overview has %d tiles, want 4", got)
			}
			mustHold(t, "the tiles", tiles, tileLabel("Drift"), tileLabel(newerVersions), tileLabel(wordNotChecked), tileLabel("Profiles"))
			mustNotHold(t, "the overview", overview, imageChanges, tileLink("#images", ""))
			if n := navOfImages.FindStringSubmatch(overview); n == nil || n[1] != "Off" {
				t.Errorf("the nav item is %q, want the count Off", n)
			}
			mustHold(t, "the images", pageOf(t, screens, "#images", ""), podsOffTitle, imagesOn+`<span>Images</span><span class="cnt">Off</span></a>`)
		})
	}
}

// With the read of the pods on, the tile is there, also when the last read failed: after a good read, and before the first one.
func TestWithTheReadOfThePodsOnTheTileShowsAlsoAfterAFailedRead(t *testing.T) {
	failed := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{
		"images.json": podsRefused(t),
	})
	for name, overview := range map[string]string{
		"a good read":                         pagesOf(t, writeSet(t, devdata.StateOK, nil), "#overview")[0],
		"a read that failed after a good one": pagesOf(t, failed, "#overview")[0],
		"a read that never ended well":        pageOf(t, render(t, devdata.StateImagesUnread, false), "#overview", ""),
	} {
		t.Run(name, func(t *testing.T) {
			if got := strings.Count(tileSection.FindString(overview), `<a class="tile `); got != 5 {
				t.Errorf("the overview has %d tiles, want 5", got)
			}
			if tileOfImages.FindStringSubmatch(overview) == nil {
				t.Error("the overview has no tile " + imageChanges)
			}
			if n := navOfImages.FindStringSubmatch(overview); n == nil || n[1] == "Off" {
				t.Errorf("the nav item is %q, and it must not say Off", n)
			}
		})
	}
}

// With the read of the pods off, the screen is an offer: it says what the read gives, what it costs, and the next
// step, with the command. It shows no table.
func TestWithThePodsOffTheScreenOfTheImagesSaysHowToTurnTheReadOn(t *testing.T) {
	for _, state := range []devdata.State{devdata.StateImagesOff, devdata.StateQuiet} {
		t.Run(string(state), func(t *testing.T) {
			list := pageOf(t, render(t, state, false), "#images", "")
			mustHold(t, "the screen", list, `<span class="sub">Pods not read</span>`, podsOffTitle, podsOffFact,
				`The read needs a role that can list pods, and that role can also read their <span class="m">env</span>.`,
				podsOffNext, splashClass("md", "off"))
			mustNotHold(t, "the screen", list, "<table", comparedFirst)
		})
	}
}

// With no read of the pods that ended well, the screen says that the observer could not read the pods,
// with the words of the error.
func TestWithAReadThatNeverEndedWellTheScreenOfTheImagesGivesTheError(t *testing.T) {
	list := pageOf(t, render(t, devdata.StateImagesUnread, false), "#images", "")
	mustHold(t, "the screen", list, `<span class="sub">`+notReadYet+`</span>`, podsTitle,
		podsRefusedWords,
		askListPods, canListPods,
		splashClass("md", "warn"))
	mustNotHold(t, "the screen", list, "<table", podsOffFact)
}

// The cause pods-not-read has a row with no profile and no pack. The row says "The pods", and a click goes
// to the screen of the images. The page does not break on the empty list of profiles.
func TestTheCauseOfPodsNotReadHasARowWithNoProfileAndOpensTheImages(t *testing.T) {
	screens := render(t, devdata.StateImagesUnread, false)
	notChecked := pageOf(t, screens, "#notchecked", "")
	row := rowTo(t, notChecked, "#images")
	mustHold(t, "the row", row, `<td class="m"><span class="mut">The pods</span></td>`, `<td class="m"><span class="mut">-</span></td>`,
		"<td>The observer could not read the pods, so the list of images is old or empty.</td>")
	mustHold(t, "the other rows", notChecked, goTo("#p-gazoo-backup@1.2.0"), goTo("#p-hoppy-autoscale@1.3.0"))
	mustHold(t, "the heading", notChecked, wordNotChecked+` on this run <span class="sub">3.`)
}

// The row "Image behind a tag" says what the check does. With a read of the pods it links the images.
// The card keeps its four rows.
func TestTheCoverageRowOfTheImagesFollowsTheRead(t *testing.T) {
	for state, read := range map[devdata.State]bool{devdata.StateOK: true, devdata.StateImages: true, devdata.StateImagesOff: false, devdata.StateImagesUnread: false, devdata.StateQuiet: false} {
		t.Run(string(state), func(t *testing.T) {
			for _, hash := range []string{"#overview", "#notchecked"} {
				page := pageOf(t, render(t, state, false), hash, "")
				mustHold(t, hash, page, "Image behind a tag", "Bytes of a chart", "Masked values", "Edits with kubectl")
				if read {
					mustHold(t, hash, page, comparedLink)
					mustNotHold(t, hash, page, byNameAndTag)
				} else {
					mustHold(t, hash, page, byNameAndTag)
					mustNotHold(t, hash, page, comparedFirst)
				}
			}
		})
	}
}

// writeSet copies a set into a new directory of sets, as the set "scratch", with the files that edit
// names changed. It gives the directory.
func writeSet(t *testing.T, from devdata.State, edit map[string]func([]byte) []byte) string {
	t.Helper()
	dir := t.TempDir()
	if err := os.MkdirAll(path.Join(dir, "scratch"), 0o755); err != nil {
		t.Fatal(err)
	}
	// The files alerts.json and replies.json are optional. A set without alerts.json is the page of an observer
	// with no alerts, and a set without replies.json gives no answer to a click on a move that asks.json lacks.
	for _, name := range append(slices.Clone(devdata.Files), "alerts.json", "replies.json") {
		raw, err := devdata.Read(os.DirFS("testdata/api"), from, name)
		if (name == "alerts.json" || name == "replies.json") && errors.Is(err, fs.ErrNotExist) {
			continue
		}
		if err != nil {
			t.Fatal(err)
		}
		if f := edit[name]; f != nil {
			raw = f(raw)
		}
		if err := os.WriteFile(path.Join(dir, "scratch", name), raw, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	return dir
}

// editJSON gives an edit of a file of a set for writeSet. It decodes the file into a T, calls change on it, and
// encodes it again.
func editJSON[T any](t *testing.T, change func(*T)) func([]byte) []byte {
	return func(raw []byte) []byte {
		var v T
		if err := devdata.Decode(raw, &v); err != nil {
			t.Fatal(err)
		}
		change(&v)
		return testkit.MustJSON(t, v)
	}
}

// replaceText gives an edit of a file of a set for writeSet. It puts to in place of each from in the text.
func replaceText(from, to string) func([]byte) []byte {
	return func(raw []byte) []byte { return []byte(strings.ReplaceAll(string(raw), from, to)) }
}

// podsRefused edits the document of the pods of a set: the last read failed, because the API server refused it with a 403.
func podsRefused(t *testing.T) func([]byte) []byte {
	return editJSON(t, func(o *model.Observed) { o.Error = &model.ErrorRef{Kind: docs.KindKubeAPI, Status: 403} })
}

// pagesOf shows some screens of the set that writeSet wrote in dir, with no crawl.
func pagesOf(t *testing.T, dir string, hashes ...string) []string {
	t.Helper()
	return showIn(t, dir, "scratch", hashes...)
}

// A read that failed after a good one keeps the list of the good read. Each screen says so in an amber
// bar, with the words of the error and the time of the read that the list shows.
func TestAReadThatFailedAfterAGoodOneShowsAnAmberBarOnEachScreen(t *testing.T) {
	dir := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{
		"images.json": podsRefused(t),
	})
	const bar = amberBar + podsFailed + `: ` + roleRefused + `. <span>The list shows the read of 11:54Z.</span></div>`
	pages := pagesOf(t, dir, "#overview", "#images", imageHash(testkit.VendorRef))
	for i, page := range pages {
		mustHold(t, imageScreens[i], page, bar)
	}
	mustHold(t, imageScreens[1], pages[1], "8 images", linkRowStart)
	for _, hash := range []string{"#overview", "#images"} {
		mustNotHold(t, hash, pagesOf(t, writeSet(t, devdata.StateOK, nil), hash)[0], podsFailed)
	}
}

// The words of a failed read name the API server and the role of the observer, and never Palette.
func TestTheWordsOfAFailedReadOfThePodsNameTheApiServer(t *testing.T) {
	for _, c := range []struct {
		err  model.ErrorRef
		want string
	}{
		{model.ErrorRef{Kind: docs.KindKubeAPI, Status: 401}, "HTTP 401, the role of the observer was refused"},
		{model.ErrorRef{Kind: docs.KindKubeAPI, Status: 403}, roleRefused},
		{model.ErrorRef{Kind: docs.KindKubeAPI, Status: 500}, "HTTP 500, the API server gave an error"},
		{model.ErrorRef{Kind: docs.KindKubeTransport}, "the link to the API server failed"},
		{model.ErrorRef{Kind: docs.KindKubeConfig}, "the observer could not read its token or its CA file"},
		{model.ErrorRef{Kind: docs.KindNotJSON}, "the API server sent " + testkit.NotJSONAnswer},
		{model.ErrorRef{Kind: docs.KindTimeout}, "the API server did not answer in time"},
	} {
		t.Run(c.err.Kind+strconv.Itoa(c.err.Status), func(t *testing.T) {
			dir := writeSet(t, devdata.StateImagesUnread, map[string]func([]byte) []byte{
				"images.json": editJSON(t, func(o *model.Observed) { o.Error = &c.err }),
			})
			list := pagesOf(t, dir, "#images")[0]
			mustHold(t, imageScreens[1], list, podsFailed+". <b>"+c.want+".</b>")
			mustNotHold(t, imageScreens[1], list, "Palette")
		})
	}
}

// An image reference with the text of a tag shows as text on the list, on the page of the image, and in
// the finding. No screen holds the tag.
func TestAnImageReferenceWithAnImgTagShowsAsText(t *testing.T) {
	swap := replaceText(testkit.VendorRef, evil)
	dir := writeSet(t, devdata.StateImages, map[string]func([]byte) []byte{"images.json": swap, "findings.json": swap, "profiles.json": swap})
	pages := pagesOf(t, dir, "#overview", "#images", imageHash(evil), imageHash(evil))
	for i, page := range pages {
		where := imageScreens[min(i, 2)]
		checkMarkup(t, screen{Hash: where, HTML: page})
		mustHold(t, where, page, injection)
		mustNotHold(t, where, page, "<img")
	}
	mustHold(t, imageScreens[2], pages[2], `<h1 class="m h1m">`+injection+`</h1>`)
}

// An image that the data does not hold shows the page of a missing item, under the nav item Images.
func TestAnImageThatNoPodRunsShowsAMissingPage(t *testing.T) {
	page := pagesOf(t, writeSet(t, devdata.StateOK, nil), imageHash(testkit.NoImage))[0]
	mustHold(t, "the page", page, "No pod runs this image", `<span class="m">`+testkit.NoImage+`</span> runs in no pod now.`, `<a class="btn" href="#images">All images</a>`, imagesOn)
}

// The line of a profile for a finding reads right for one pack and for many. The line of an image id
// does not say "name" for one pack.
func TestTheLineOfAFindingReadsRightForOneOrManyPacks(t *testing.T) {
	for _, c := range []struct {
		kind  model.Kind
		packs []string
		want  string
	}{
		{model.ImageIDChanged, []string{"addon/dex"}, "dex " + machineImage},
		{model.ImageIDChanged, []string{"addon/dex", "addon/reloader"}, "dex, reloader " + machineImage},
		{model.ImageMoved, []string{"addon/dex"}, "dex ran another digest in pods than the first one seen."},
		{model.ImageMoved, []string{"addon/dex", "addon/reloader"}, "dex, reloader ran another digest in pods than the first one seen."},
		{model.ImageSplit, []string{"addon/dex"}, "dex ran with two digests in pods at one time."},
		{model.ImageSplit, []string{"addon/dex", "addon/reloader"}, "dex, reloader ran with two digests in pods at one time."},
	} {
		t.Run(string(c.kind)+strconv.Itoa(len(c.packs)), func(t *testing.T) {
			add := editJSON(t, func(v *api.FindingsView) {
				v.Findings = append(v.Findings, model.Finding{ID: model.FindingID(c.kind, dexSubject), Kind: c.kind, Profile: dexSubject, Packs: c.packs,
					Images: []string{"a/b:1"}, FirstSeen: devdata.Reference, LastSeen: devdata.Reference, Who: []docs.Who{}})
			})
			dir := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"findings.json": add})
			row := rowTo(t, pagesOf(t, dir, "#profiles")[0], fredHash)
			mustHold(t, "the row", row, c.want)
		})
	}
}

// The old words of the line of an image id are gone from the script: they read wrong for one pack.
func TestTheLineOfAnImageIdNoLongerSaysNameAnotherMachineImage(t *testing.T) {
	script := readScripts(t)
	mustNotHold(t, "the scripts", script, "name another machine image")
	mustHold(t, "the scripts", script, machineImage)
}

// The rules that let a long reference wrap are in the style: the reference of an image has no space, and
// it must not push a table out of its card.
func TestALongReferenceCanWrap(t *testing.T) {
	css := string(readPage(t, "app.css"))
	ruleIs(t, css, ".wrap", wrapAnywhere)
	ruleIs(t, css, ".lab.wrap", "white-space:normal;")
}
