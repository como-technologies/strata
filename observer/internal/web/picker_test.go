package web

import (
	"regexp"
	"testing"
)

/* The tests below hold the list of the dropdown "Compare". The native list takes the blue of the operating system in both themes, so
the style draws the list itself where the browser can, with the tokens of the theme and with no script. */

const baseSelect = "@supports (appearance:base-select)"

// A browser that knows base-select draws the list with the rules of one @supports block, and the rules use the tokens of the
// theme: a token has a light value and a dark value, so the list and its hover follow the theme. Any other browser skips the block
// and keeps its native list, which still works.
func TestTheListOfTheCompareDropdownTakesTheTokensOfTheTheme(t *testing.T) {
	css := string(readPage(t, "app.css"))
	inside := block(css, baseSelect)
	if inside == "" {
		t.Fatalf("app.css has no block %q", baseSelect)
	}
	// pickerRule is the rule of the list that a select opens.
	const pickerRule = "select.search::picker(select)"
	// ruleOf removes the spaces of a value, so the border reads border:1pxsolidvar(--line).
	rulesHold(t, inside, map[string][]string{
		"select.search," + pickerRule: {"appearance:base-select;"},
		pickerRule:                    {"background:var(--panel);", "color:var(--ink);", "border:1pxsolidvar(--line);"},
		"select.search option:hover,select.search option:focus-visible": {"background:var(--violet-t);", "color:var(--violet);"},
	})
	// A color of the list is a token, and never a literal that holds one theme only.
	if m := regexp.MustCompile(`#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(`).FindString(inside); m != "" {
		t.Errorf("the block of the dropdown holds the color %q: use a token", m)
	}
	// Nothing outside the block names the list, so a browser with no support sees no rule of it.
	mustNotHold(t, "the style outside the block", without(css, baseSelect), "::picker")
}

// The dropdown needs no script: the change of a side still goes through the one handler of the page.
func TestTheDropdownCompareHasNoScriptForItsList(t *testing.T) {
	script := readScripts(t)
	mustNotHold(t, "the scripts", script, []string{"showPicker", "picker", "base-select"}...)
}
