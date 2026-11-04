package runner

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
)

// fake writes a script that stands for the strata binary.
func fake(t *testing.T, body string) Runner {
	t.Helper()
	dir := t.TempDir()
	p := filepath.Join(dir, "strata")
	if err := os.WriteFile(p, []byte("#!/bin/sh\n"+body+"\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	return Runner{Binary: p, Dir: dir, Timeout: 2 * time.Second}
}

func TestExitOneIsAnAnswer(t *testing.T) {
	r, err := fake(t, `echo '{"same":false}'; exit 1`).Run(context.Background(), "compare", "a.json", "b.json", "--json", "-q")
	if err != nil || r.Exit != 1 || !strings.Contains(string(r.Stdout), "same") {
		t.Fatalf("result %+v, error %v", r, err)
	}
}

func TestExitTwoWithADocumentIsAnAnswer(t *testing.T) {
	r, err := fake(t, `echo '{"pins":[]}'; exit 2`).Run(context.Background(), "behind", "a.json", "--json", "-q")
	if err != nil || r.Exit != 2 {
		t.Fatalf("result %+v, error %v", r, err)
	}
}

func TestExitTwoWithNoDocumentIsTheKindOfTheError(t *testing.T) {
	_, err := fake(t, `echo '{"error":{"kind":"api","message":"HTTP 401","status":401}}' >&2; exit 2`).Run(context.Background(), "verify", "a.json", "--json", "-q")
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
		_, err := fake(t, body).Run(context.Background(), "verify", "a.json", "--json", "-q")
		if err == nil || strings.Contains(err.Error(), "SENTINEL") {
			t.Fatalf("error %v", err)
		}
	}
}

func TestATimeoutKillsTheRun(t *testing.T) {
	x := fake(t, `exec sleep 30`)
	x.Timeout = 100 * time.Millisecond
	_, err := x.Run(context.Background(), "verify", "a.json", "--json", "-q")
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
		{"compare", "a", "b", "--validate"},
		{"compare", "a", "b", "--repave"},
		{"compare", "a", "b", "--full"},
		{"snapshot", "x", "--validate"},
	} {
		if _, err := x.Run(context.Background(), args...); err == nil {
			t.Errorf("%v ran", args)
		}
	}
	if _, err := os.Stat(filepath.Join(x.Dir, "ran.txt")); err == nil {
		t.Error("a refused command started a process")
	}
}

func TestTheProcessGetsNoOtherVariable(t *testing.T) {
	t.Setenv("PALETTE_API_KEY", "key")
	t.Setenv("SOME_OTHER_SECRET", "SENTINEL")
	r, err := fake(t, `env`).Run(context.Background(), "verify", "a.json")
	if err != nil {
		t.Fatal(err)
	}
	out := string(r.Stdout)
	if strings.Contains(out, "SENTINEL") || !strings.Contains(out, "STRATA_REPORT=0") || !strings.Contains(out, "PALETTE_API_KEY=key") {
		t.Fatalf("env: %s", out)
	}
}
