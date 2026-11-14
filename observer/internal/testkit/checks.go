package testkit

import (
	"os"
	"os/exec"
	"strings"
	"testing"
)

// HoldsAll fails when text lacks one of the words. where names the text in the failure.
func HoldsAll(t *testing.T, where, text string, words ...string) {
	t.Helper()
	for _, w := range words {
		if !strings.Contains(text, w) {
			t.Errorf("%s has no %q", where, w)
		}
	}
}

// HoldsNone fails when text holds one of the words. where names the text in the failure.
func HoldsNone(t *testing.T, where, text string, words ...string) {
	t.Helper()
	for _, w := range words {
		if strings.Contains(text, w) {
			t.Errorf("%s holds %q", where, w)
		}
	}
}

// NoProblems fails with the list when a check found problems in what. An empty problem is no problem, so a check that
// gives one problem or "" can give it in a list of one.
func NoProblems(t *testing.T, what string, got []string) {
	t.Helper()
	if found := problemsOf(got); len(found) != 0 {
		t.Errorf("%s:\n%s", what, strings.Join(found, "\n"))
	}
}

// FindsProblem fails when no problem that a check found in what holds want. An empty want asks for any problem.
func FindsProblem(t *testing.T, what string, got []string, want string) {
	t.Helper()
	for _, p := range problemsOf(got) {
		if strings.Contains(p, want) {
			return
		}
	}
	t.Errorf("%s: problems %q, want one with %q", what, got, want)
}

func problemsOf(got []string) []string {
	var out []string
	for _, p := range got {
		if p != "" {
			out = append(out, p)
		}
	}
	return out
}

// WhenMissing ends a test that needs tool, which is not on the PATH. With ci set, as the variable CI of a runner sets
// it, the test fails: a runner with no tool would skip the test, and the check would pass with nothing checked. Else the
// test skips.
func WhenMissing(ci, tool string, fail, skip func(args ...any)) {
	if ci != "" {
		fail(tool + " is not on the PATH, and CI is set: a runner must have it, or the test checks nothing")
		return
	}
	skip(tool + " is not on the PATH")
}

// NeedTools ends the test with WhenMissing for the first of the tools that is not on the PATH.
func NeedTools(t *testing.T, tools ...string) {
	t.Helper()
	for _, tool := range tools {
		if _, err := exec.LookPath(tool); err != nil {
			WhenMissing(os.Getenv("CI"), tool, t.Fatal, t.Skip)
			return
		}
	}
}
