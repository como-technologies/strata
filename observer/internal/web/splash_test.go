package web

import (
	"os"
	"regexp"
	"slices"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/testkit"
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

// motionQuery is the media feature of less motion, and reducedMotion is the head of the block that stops each animation.
const (
	motionQuery   = "prefers-reduced-motion"
	reducedMotion = "@media (" + motionQuery + ":reduce)"
)

// lostTop is the top slab of a lost mark, which is red and moves from its place.
const lostTop = ".slabs.tone-lost .s0"

// The parts of the rules for less motion, and the problems that motionProblems gives. noAnimation stops the animation of
// a slab. buildSlabs are the slabs of a mark that builds, and buildHeld holds them in their state. lostMoved holds the
// top slab of a lost mark off its place. slabsSVG starts the markup of a mark.
const (
	noAnimation     = "animation:none!important"
	wantNoAnimation = `want "` + noAnimation + `;"`
	buildSlabs      = ".slabs.tone-build .s"
	buildHeld       = buildSlabs + "{opacity:.7}"
	lostMoved       = "{transform:translate(9px,-8px)}"
	noMotionRule    = "no rule for " + motionQuery
	slabsSVG        = `<svg ` + markOpen
)

// motionProblems lists what is wrong in the way that a style stops its animations for a person who asks for less motion. Each
// rule outside that block that runs an animation belongs to a mark (a selector that starts with .slabs), and the block turns off
// the animation of each slab with !important, so that no tone keeps one. A mark holds its state: the slabs that build stay in place,
// and the top slab of a lost mark stays off its place.
func motionProblems(css string) []string {
	var out []string
	still := block(css, reducedMotion)
	if still == "" {
		return []string{"the style has " + noMotionRule}
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
	for _, p := range []string{ruleProblem(still, ".slabs .s", noAnimation+";"), lackProblem(still, buildSlabs, "opacity:"), lackProblem(still, lostTop, "transform:")} {
		if p != "" {
			out = append(out, p)
		}
	}
	return out
}

func TestTheStyleStopsEachAnimationForAPersonWhoAsksForLessMotion(t *testing.T) {
	css := string(readPage(t, "app.css"))
	noProblems(t, "app.css", motionProblems(css))
	// The slabs are the one thing that moves.
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
` + reducedMotion + `{.slabs .s{` + noAnimation + `}` + buildHeld + lostTop + lostMoved + `}`
	noProblems(t, "a good style", motionProblems(good))
	cases := []struct{ name, css, want string }{
		{"no rule for less motion", strings.Replace(good, reducedMotion, "@media (min-width:1px)", 1), noMotionRule},
		{"an animation on a card", good + ".card{animation:breathe 1s infinite}", "the rule of .card runs an animation"},
		{"a spinner in a list of selectors", good + ".slabs.tone-calm .s,.spin{animation:rise 1s infinite}", "the rule of .spin runs an animation"},
		{"no important", strings.Replace(good, noAnimation, "animation:none", 1), wantNoAnimation},
		{"the build slabs vanish", strings.Replace(good, buildHeld, "", 1), "the rule of " + buildSlabs + ` is "", and it lacks opacity:`},
		{"the lost slab comes back", strings.Replace(good, lostTop+lostMoved, "", 1), "the rule of " + lostTop + ` is "", and it lacks transform:`},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) { findsProblem(t, c.name, motionProblems(c.css), c.want) })
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
	rulesHold(t, css, map[string][]string{
		".slabs.tone-off .s":       {"fill:none;"},
		".slabs.tone-warn .s":      {"stroke:var(--amb);"},
		lostTop:                    {"fill:var(--red);"},
		".splash.tone-lost":        {"var(--red)"},
		".splash.tone-warn":        {"var(--amb)"},
		".splash.md":               {"padding:"},
		".splash.sm":               {"padding:"},
		".inl":                     {"display:flex;"},
		".cmd":                     {"display:flex;"},
		".facts":                   {"display:flex;"},
		".slabs.tone-lost .s":      {"fill:color-mix("},
		".slabs.tone-warn .s0":     {"fill:none;"},
		buildSlabs + "0":           {"animation-delay:"},
		".slabs.tone-calm .s1":     {"animation-delay:"},
		".splash.tone-off":         {"background:none;"},
		".splash.tone-lost>.slabs": {"drop-shadow("},
	})
}

// The mark of the splash is the geometry of the logo in assets/logo/: the four paths in the script are the four paths of the
// dark mark, in the same order, and the style gives each slab the color of its path.
func TestTheMarkOfTheSplashIsTheGeometryOfTheLogo(t *testing.T) {
	logo := readFile(t, logoDir+markDark)
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
		ruleIs(t, css, ".slabs .s"+string(rune('0'+i)), "fill:"+strings.ToLower(want[i][1])+";")
	}
}

// The policy of the page does not change: it stops an inline style, so the color of a slab is a rule of app.css. The files of
// assets/ keep the rules of the page, and the splash adds no font, no library, and no image.
func TestThePolicyOfThePageStaysAndTheSplashAddsNoFileAndNoInlineStyle(t *testing.T) {
	policy := serve(t, "/").Header().Get(testkit.HeaderPolicy)
	mustHold(t, "the policy", policy, []string{"default-src 'none'", "script-src 'self'", "style-src 'self'", "font-src 'self'", "img-src 'self'"}...)
	if strings.Contains(policy, "unsafe-inline") || strings.Contains(policy, "data:") {
		t.Errorf("the policy %q allows "+problemStyle+" or a data address", policy)
	}
	noProblems(t, "assets/", problems(files(t)))
	var names []string
	entries, err := os.ReadDir("assets")
	if err != nil {
		t.Fatal(err)
	}
	for _, e := range entries {
		names = append(names, e.Name())
	}
	want := []string{"LICENSES.md", "app.css", "app.js", testkit.FaviconDark, testkit.FaviconLight, "fonts", "frame.js", "index.html", "pack.js", "routes.js", "screens.js", themeScript}
	if !slices.Equal(names, want) {
		t.Errorf("assets/ holds %v, want %v", names, want)
	}
	script := readScripts(t)
	mustNotHold(t, "the scripts", script, []string{"<img", "new Image", "@import", "<link", "<style"}...)
	// The mark is drawn in markup through h``, like every other part of the page.
	mustHold(t, "the scripts", script, slabsSVG+`${tone}"`)
	var got []string
	problemsInScreens(script, func(p string) { got = append(got, p) })
	noProblems(t, "the scripts", got)
}
