package web

import (
	"regexp"
	"strings"
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
	// ruleOf removes the spaces of a value, so the border reads border:1pxsolidvar(--line).
	for selector, want := range map[string][]string{
		"select.search,select.search::picker(select)":                   {"appearance:base-select;"},
		"select.search::picker(select)":                                 {"background:var(--panel);", "color:var(--ink);", "border:1pxsolidvar(--line);"},
		"select.search option:hover,select.search option:focus-visible": {"background:var(--violet-t);", "color:var(--violet);"},
	} {
		got := ruleOf(inside, selector)
		for _, w := range want {
			if !strings.Contains(got, w) {
				t.Errorf("the rule of %s is %q, and it lacks %s", selector, got, w)
			}
		}
	}
	// A color of the list is a token, and never a literal that holds one theme only.
	if m := regexp.MustCompile(`#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(`).FindString(inside); m != "" {
		t.Errorf("the block of the dropdown holds the color %q: use a token", m)
	}
	// Nothing outside the block names the list, so a browser with no support sees no rule of it.
	if strings.Contains(without(css, baseSelect), "::picker") {
		t.Error("a rule outside the block names ::picker")
	}
}

// The dropdown needs no script: the change of a side still goes through the one handler of the page.
func TestTheDropdownCompareHasNoScriptForItsList(t *testing.T) {
	script := readScripts(t)
	for _, word := range []string{"showPicker", "picker", "base-select"} {
		if strings.Contains(script, word) {
			t.Errorf("the scripts hold %q: the style draws the list", word)
		}
	}
}
