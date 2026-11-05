package runner

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"regexp"
	"slices"
	"strings"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// fake writes a script that stands for the strata binary.
func fake(t *testing.T, body string) Runner {
	t.Helper()
	p := testkit.Script(t, body)
	return Runner{Binary: p, Dir: filepath.Dir(p), Timeout: 2 * time.Second}
}

func TestExitOneIsAnAnswer(t *testing.T) {
	r, err := fake(t, `echo '{"same":false}'; exit 1`).Run(context.Background(), CmdCompare, "a.json", "b.json", "--json", "-q")
	if err != nil || r.Exit != 1 || !strings.Contains(string(r.Stdout), "same") {
		t.Fatalf("result %+v, error %v", r, err)
	}
}

func TestExitTwoWithADocumentIsAnAnswer(t *testing.T) {
	r, err := fake(t, `echo '{"pins":[]}'; exit 2`).Run(context.Background(), CmdBehind, "a.json", FlagNoImpact, "--json", "-q")
	if err != nil || r.Exit != 2 {
		t.Fatalf("result %+v, error %v", r, err)
	}
}

func TestExitTwoWithNoDocumentIsTheKindOfTheError(t *testing.T) {
	_, err := fake(t, `echo '{"error":{"kind":"api","message":"HTTP 401","status":401}}' >&2; exit 2`).Run(context.Background(), CmdVerify, "a.json", "--json", "-q")
	var re *RunError
	if !errors.As(err, &re) || re.Kind != docs.KindAPI || re.Status != 401 {
		t.Fatalf("error %v", err)
	}
}

func TestStderrStaysOutOfTheError(t *testing.T) {
	for _, body := range []string{
		`echo 'SENTINEL value' >&2; exit 2`,
		`echo '{"error":{"kind":"api","message":"SENTINEL body of Palette","status":500}}' >&2; exit 2`,
	} {
		_, err := fake(t, body).Run(context.Background(), CmdVerify, "a.json", "--json", "-q")
		if err == nil || strings.Contains(err.Error(), "SENTINEL") {
			t.Fatalf("error %v", err)
		}
	}
}

func TestATimeoutKillsTheRun(t *testing.T) {
	x := fake(t, `exec sleep 30`)
	x.Timeout = 100 * time.Millisecond
	_, err := x.Run(context.Background(), CmdVerify, "a.json", "--json", "-q")
	var re *RunError
	if !errors.As(err, &re) || re.Kind != docs.KindTimeout {
		t.Fatalf("error %v", err)
	}
}

func TestACommandOrAFlagOutsideTheListIsRefused(t *testing.T) {
	x := fake(t, `echo ran > ran.txt`)
	for _, args := range [][]string{
		{"report", "--link"},
		{"preflight"},
		{CmdCompare, "a", "b", "--validate"},
		{CmdCompare, "a", "b", "--repave"},
		{CmdBehind, "a.json", FlagFull},
		{CmdSnapshot, "x", FlagFull},
		{CmdCompare, "a", "b", FlagPin, "k8s/kubernetes"},
		{CmdVerify, "a.json", FlagPin, "k8s/kubernetes"},
		{CmdBehind, "a.json", "--validate"},
		{CmdSnapshot, "x", "--validate"},
	} {
		if _, err := x.Run(context.Background(), args...); err == nil {
			t.Errorf("%v ran", args)
		}
	}
	if _, err := os.Stat(filepath.Join(x.Dir, "ran.txt")); err == nil {
		t.Error("a refused command started a process")
	}
}

// The flags --full and --pin go to the commands that take them, and a run of `behind` that asks Palette
// names one pack. Each other run of `behind` must say --no-impact.
func TestTheFlagsOfTheClickAreAllowedForTheirCommandsOnly(t *testing.T) {
	x := fake(t, `echo '{}'; exit 0`)
	for _, c := range []struct {
		args []string
		ok   bool
	}{
		{[]string{CmdCompare, "a", "b", FlagFull, "--json", "-q"}, true},
		{[]string{CmdVerify, "a.json", "--summary", FlagFull, "--json", "-q"}, true},
		{[]string{CmdBehind, "a.json", FlagNoImpact, "--json", "-q"}, true},
		{[]string{CmdBehind, "a.json", FlagPin, "k8s/kubernetes", "--json", "-q"}, true},
		{[]string{CmdBehind, "a.json", "--json", "-q"}, false},
		{[]string{CmdBehind, "a.json", "b.json", "--json", "-q"}, false},
		{[]string{CmdBehind, FlagNoImpact}, true},
		{[]string{CmdBehind, "a.json", FlagFull, FlagNoImpact}, false},
		{[]string{CmdSnapshot, "cluster:c1", FlagPin, "k8s/kubernetes"}, false},
	} {
		_, err := x.Run(context.Background(), c.args...)
		if (err == nil) != c.ok {
			t.Errorf("%v: error %v, want ok %v", c.args, err, c.ok)
		}
	}
}

// notSet lists the variables that the book says the runner does not set. Each call has -q, and none
// writes a terminal report, so STRATA_QUIET and NO_COLOR would change nothing.
var notSet = []string{"STRATA_QUIET", "NO_COLOR"}

// variablesOf runs `env` as the runner does and gives the names of the variables that the process got.
func variablesOf(t *testing.T) []string {
	t.Helper()
	r, err := fake(t, `env`).Run(context.Background(), CmdVerify, "a.json", "--json", "-q")
	if err != nil {
		t.Fatal(err)
	}
	var names []string
	for _, line := range strings.Split(string(r.Stdout), "\n") {
		if name, _, ok := strings.Cut(line, "="); ok && !strings.HasPrefix(name, "_") {
			names = append(names, name)
		}
	}
	slices.Sort(names)
	return names
}

func TestTheProcessGetsTheVariablesOfTheListAndStrataReportAndNoOtherVariable(t *testing.T) {
	// A variable of the list that the test host lacks gets a value that is a real directory, because
	// TMPDIR and HOME must stay usable.
	for _, name := range allowedEnv {
		if _, ok := os.LookupEnv(name); !ok {
			t.Setenv(name, os.TempDir())
		}
	}
	t.Setenv("PALETTE_API_KEY", "key")
	for _, name := range append([]string{"SOME_OTHER_SECRET"}, notSet...) {
		t.Setenv(name, "SENTINEL")
	}
	want := append([]string{"STRATA_REPORT"}, allowedEnv...)
	slices.Sort(want)
	// The shell of the fake adds PWD, SHLVL, and OLDPWD to a process that it starts. They are not
	// variables of the runner.
	got := slices.DeleteFunc(variablesOf(t), func(name string) bool { return name == "PWD" || name == "SHLVL" || name == "OLDPWD" })
	if !slices.Equal(got, want) {
		t.Errorf("the process got %v, want %v", got, want)
	}
	r, err := fake(t, `echo "report=$STRATA_REPORT key=$PALETTE_API_KEY"`).Run(context.Background(), CmdVerify, "a.json", "--json", "-q")
	if err != nil || string(r.Stdout) != "report=0 key=key\n" {
		t.Errorf("output %q, error %v", r.Stdout, err)
	}
}

// The rule of code.md names the variables of the runner in code spans, and it names the ones that
// the runner does not set. The test holds both lists against the code, so the book cannot drift.
func TestCodeMdNamesEachVariableThatTheRunnerSets(t *testing.T) {
	raw, err := os.ReadFile("../../../docs/src/development/code.md")
	if errors.Is(err, os.ErrNotExist) {
		t.Skip("the book is not in this tree")
	}
	if err != nil {
		t.Fatal(err)
	}
	_, rest, ok := strings.Cut(string(raw), "- `internal/runner` is the one place")
	if !ok {
		t.Fatal("code.md has no rule for internal/runner")
	}
	rule, _, _ := strings.Cut(rest, "\n- ")
	var named []string
	for _, m := range regexp.MustCompile("`([A-Z][A-Z0-9_]+)(?:=[^`]*)?`").FindAllStringSubmatch(rule, -1) {
		named = append(named, m[1])
	}
	want := append(append([]string{"STRATA_REPORT"}, allowedEnv...), notSet...)
	slices.Sort(named)
	slices.Sort(want)
	if !slices.Equal(named, want) {
		t.Errorf("code.md names %v, want %v", named, want)
	}
	if !strings.Contains(rule, "`STRATA_REPORT=0`") {
		t.Error("code.md does not give the value of STRATA_REPORT")
	}
}
