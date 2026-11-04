package main

import (
	"bytes"
	"strings"
	"testing"
)

func TestVersionPrintsTheName(t *testing.T) {
	var out, errs bytes.Buffer
	if code := run([]string{"version"}, &out, &errs); code != 0 {
		t.Fatalf("exit %d, stderr %q", code, errs.String())
	}
	if !strings.HasPrefix(out.String(), "strata-observer ") {
		t.Fatalf("stdout %q", out.String())
	}
}

func TestAnUnknownCommandExitsTwo(t *testing.T) {
	var out, errs bytes.Buffer
	if code := run([]string{"nope"}, &out, &errs); code != 2 {
		t.Fatalf("exit %d", code)
	}
	if !strings.Contains(errs.String(), "usage:") {
		t.Fatalf("stderr %q", errs.String())
	}
}
