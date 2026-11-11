package web

import (
	"os"
	"regexp"
	"slices"
	"strings"
	"testing"
)

/* The tests below hold the splash: the mark, the five tones with their animations, the rule for a person who asks for less
motion, and the policy of the page. The tests that draw the screens are in states_test.go. */

// block gives the text between the braces of the first block that starts with head, or "" when the style has none.
func block(css, head string) string {
	css = cssComment.ReplaceAllString(css, "")
	at := strings.Index(css, head)
	if at < 0 {
		return ""
	}
	open := strings.Index(css[at:], "{")
	if open < 0 {
		return ""
	}
	depth := 0
	for i := at + open; i < len(css); i++ {
		switch css[i] {
		case '{':
			depth++
		case '}':
			depth--
			if depth == 0 {
				return css[at+open+1 : i]
			}
		}
	}
	return ""
}

// without removes the first block that starts with head, with its head.
func without(css, head string) string {
	css = cssComment.ReplaceAllString(css, "")
	at := strings.Index(css, head)
	if at < 0 {
		return css
	}
	depth := 0
	for i := at + strings.Index(css[at:], "{"); i < len(css); i++ {
		switch css[i] {
		case '{':
			depth++
		case '}':
			depth--
			if depth == 0 {
				return css[:at] + css[i+1:]
			}
		}
	}
	return css
}

// reducedMotion is the head of the block that stops each animation.
const reducedMotion = "@media (prefers-reduced-motion:reduce)"

// motionProblems lists what is wrong in the way that a style stops its animations for a person who asks for less motion. Each
// rule outside that block that runs an animation belongs to a mark (a selector that starts with .slabs), and the block turns off
// the animation of each slab with !important, so that no tone keeps one. A mark holds its state: the slabs that build stay in place,
// and the top slab of a lost mark stays off its place.
func motionProblems(css string) []string {
	var out []string
	still := block(css, reducedMotion)
	if still == "" {
		return []string{"the style has no rule for prefers-reduced-motion"}
	}
	rest := without(css, reducedMotion)
	for strings.Contains(rest, "@keyframes ") {
		rest = without(rest, "@keyframes ")
	}
	for _, rule := range cssRule.FindAllStringSubmatch(rest, -1) {
		if !strings.Contains(rule[2], "animation") {
			continue
		}
		for _, selector := range strings.Split(rule[1], ",") {
			if selector = strings.TrimSpace(selector); !strings.HasPrefix(selector, ".slabs") {
				out = append(out, "the rule of "+selector+" runs an animation that the rule for less motion does not stop")
			}
		}
	}
	if got := ruleOf(still, ".slabs .s"); got != "animation:none!important;" {
		out = append(out, "the rule for less motion gives the slabs "+got+", want animation:none!important;")
	}
	if !strings.Contains(ruleOf(still, ".slabs.tone-build .s"), "opacity:") {
		out = append(out, "a mark that builds does not hold its state for less motion")
	}
	if !strings.Contains(ruleOf(still, ".slabs.tone-lost .s0"), "transform:") {
		out = append(out, "a lost mark does not hold its state for less motion")
	}
	return out
}

func TestTheStyleStopsEachAnimationForAPersonWhoAsksForLessMotion(t *testing.T) {
	if got := motionProblems(string(readPage(t, "app.css"))); len(got) != 0 {
		t.Errorf("%s", strings.Join(got, "\n"))
	}
	// The slabs are the one thing that moves.
	css := string(readPage(t, "app.css"))
	if n := strings.Count(cssComment.ReplaceAllString(css, ""), "@keyframes"); n != 5 {
		t.Errorf("the style has %d sets of keyframes, want the 5 of the tones", n)
	}
}

// The check must be able to fail, for each kind of problem.
func TestTheCheckOfMotionFindsEachProblem(t *testing.T) {
	const good = `.slabs.tone-calm .s{animation:breathe 4.6s infinite}
@keyframes breathe{0%{transform:none}}
@keyframes rise{0%{opacity:0}}
@keyframes march{to{stroke-dashoffset:-70}}
@keyframes pulse{0%{opacity:.5}}
@keyframes slip{0%{transform:none}}
@media (prefers-reduced-motion:reduce){.slabs .s{animation:none!important}.slabs.tone-build .s{opacity:.7}.slabs.tone-lost .s0{transform:translate(9px,-8px)}}`
	if got := motionProblems(good); len(got) != 0 {
		t.Errorf("a good style has problems: %q", got)
	}
	cases := []struct{ name, css, want string }{
		{"no rule for less motion", strings.Replace(good, reducedMotion, "@media (min-width:1px)", 1), "no rule for prefers-reduced-motion"},
		{"an animation on a card", good + ".card{animation:breathe 1s infinite}", "the rule of .card runs an animation"},
		{"a spinner in a list of selectors", good + ".slabs.tone-calm .s,.spin{animation:rise 1s infinite}", "the rule of .spin runs an animation"},
		{"no important", strings.Replace(good, "animation:none!important", "animation:none", 1), "want animation:none!important;"},
		{"the build slabs vanish", strings.Replace(good, ".slabs.tone-build .s{opacity:.7}", "", 1), "a mark that builds does not hold its state"},
		{"the lost slab comes back", strings.Replace(good, ".slabs.tone-lost .s0{transform:translate(9px,-8px)}", "", 1), "a lost mark does not hold its state"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if got := strings.Join(motionProblems(c.css), "\n"); !strings.Contains(got, c.want) {
				t.Errorf("problems %q, want one with %q", got, c.want)
			}
		})
	}
}

// Each tone has its own look and its own animation, as the comment of the mock names them: calm breathes, build stacks, off
// marches, warn pulses, and lost slips the top slab off.
func TestEachToneHasItsLookAndItsAnimationInTheStyle(t *testing.T) {
	css := cssComment.ReplaceAllString(string(readPage(t, "app.css")), "")
	for tone, animation := range map[string]string{"calm": "breathe", "build": "rise", "off": "march", "warn": "pulse", "lost": "slip"} {
		var runs bool
		for _, rule := range cssRule.FindAllStringSubmatch(css, -1) {
			if strings.Contains(rule[1], ".slabs.tone-"+tone) && strings.Contains(strings.Join(strings.Fields(rule[2]), ""), "animation:"+animation) {
				runs = true
			}
		}
		if !runs {
			t.Errorf("no rule runs the animation %s for the tone %s", animation, tone)
		}
		if !strings.Contains(css, "@keyframes "+animation+"{") {
			t.Errorf("the style has no keyframes %s", animation)
		}
	}
	// The look: an outline for off, an amber mark for warn, and a red top slab for lost. Calm and build keep the colors of the logo.
	for selector, want := range map[string]string{
		".slabs.tone-off .s":       "fill:none;",
		".slabs.tone-warn .s":      "stroke:var(--amb);",
		".slabs.tone-lost .s0":     "fill:var(--red);",
		".splash.tone-lost":        "var(--red)",
		".splash.tone-warn":        "var(--amb)",
		".splash.md":               "padding:",
		".splash.sm":               "padding:",
		".inl":                     "display:flex;",
		".cmd":                     "display:flex;",
		".facts":                   "display:flex;",
		".slabs.tone-lost .s":      "fill:color-mix(",
		".slabs.tone-warn .s0":     "fill:none;",
		".slabs.tone-build .s0":    "animation-delay:",
		".slabs.tone-calm .s1":     "animation-delay:",
		".splash.tone-off":         "background:none;",
		".splash.tone-lost>.slabs": "drop-shadow(",
	} {
		if got := ruleOf(css, selector); !strings.Contains(got, want) {
			t.Errorf("the rule of %s is %q, and it lacks %s", selector, got, want)
		}
	}
}

// The mark of the splash is the geometry of the logo in assets/logo/: the four paths in the script are the four paths of the
// dark mark, in the same order, and the style gives each slab the color of its path.
func TestTheMarkOfTheSplashIsTheGeometryOfTheLogo(t *testing.T) {
	logo := readFile(t, logoDir+"strata-mark-dark.svg")
	want := regexp.MustCompile(`<path fill="(#[0-9a-fA-F]+)" d="([^"]+)"`).FindAllStringSubmatch(string(logo), -1)
	script := readScripts(t)
	_, rest, ok := strings.Cut(script, "const SLABS = [")
	list, _, ok2 := strings.Cut(rest, "];")
	if !ok || !ok2 {
		t.Fatal("the scripts have no list SLABS")
	}
	got := regexp.MustCompile(`"(M[^"]+)"`).FindAllStringSubmatch(list, -1)
	if len(want) != 4 || len(got) != 4 {
		t.Fatalf("the logo has %d paths and the script has %d", len(want), len(got))
	}
	css := string(readPage(t, "app.css"))
	for i := range want {
		if got[i][1] != want[i][2] {
			t.Errorf("slab %d of the script is not the path of the logo", i)
		}
		if rule, color := ruleOf(css, ".slabs .s"+string(rune('0'+i))), strings.ToLower(want[i][1]); rule != "fill:"+color+";" {
			t.Errorf("the rule of slab %d is %q, want fill:%s;", i, rule, color)
		}
	}
}

// The policy of the page does not change: it stops an inline style, so the color of a slab is a rule of app.css. The files of
// assets/ keep the rules of the page, and the splash adds no font, no library, and no image.
func TestThePolicyOfThePageStaysAndTheSplashAddsNoFileAndNoInlineStyle(t *testing.T) {
	policy := serve(t, "/").Header().Get("Content-Security-Policy")
	for _, want := range []string{"default-src 'none'", "script-src 'self'", "style-src 'self'", "font-src 'self'", "img-src 'self'"} {
		if !strings.Contains(policy, want) {
			t.Errorf("the policy %q lacks %s", policy, want)
		}
	}
	if strings.Contains(policy, "unsafe-inline") || strings.Contains(policy, "data:") {
		t.Errorf("the policy %q allows an inline style or a data address", policy)
	}
	if got := problems(files(t)); len(got) != 0 {
		t.Errorf("%s", strings.Join(got, "\n"))
	}
	var names []string
	entries, err := os.ReadDir("assets")
	if err != nil {
		t.Fatal(err)
	}
	for _, e := range entries {
		names = append(names, e.Name())
	}
	want := []string{"LICENSES.md", "app.css", "app.js", "favicon-dark.svg", "favicon-light.svg", "fonts", "frame.js", "index.html", "pack.js", "routes.js", "screens.js", "theme.js"}
	if !slices.Equal(names, want) {
		t.Errorf("assets/ holds %v, want %v", names, want)
	}
	script := readScripts(t)
	for _, banned := range []string{"<img", "new Image", "@import", "<link", "<style"} {
		if strings.Contains(script, banned) {
			t.Errorf("the scripts hold %s", banned)
		}
	}
	// The mark is drawn in markup through h``, like every other part of the page.
	if !strings.Contains(script, `<svg class="slabs tone-${tone}"`) || strings.Count(script, "innerHTML") != 1 {
		t.Error("the mark is not drawn through the template of the scripts")
	}
}
