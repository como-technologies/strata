package model

import (
	"encoding/json"
	"os"
	"reflect"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
)

func pinWith(bump *docs.Bump, err *docs.Error) docs.Pin {
	return docs.Pin{Name: "kubernetes", Layer: "k8s", Version: "1.35.3", Newer: []docs.Newer{{Version: "1.36.2"}}, Bump: bump, Error: err}
}

func TestAnswerOfReadsEachVerdictFromThePinThatBehindGives(t *testing.T) {
	refusal := docs.Refusal{Code: "DependencyPackUpgradeNotSupported", Message: "SENTINEL", Clusters: []string{"payments"}}
	for _, c := range []struct {
		name    string
		pin     docs.Pin
		verdict Verdict
		newest  string
		status  int
		refuses int
	}{
		{"no error", pinWith(&docs.Bump{To: "1.37.0", PaletteValidation: &docs.Validation{Accepts: true}}, nil), VerdictNoError, "1.37.0", 0, 0},
		{"a refusal", pinWith(&docs.Bump{To: "1.36.2", PaletteValidation: &docs.Validation{Errors: []docs.Refusal{refusal}}}, nil), VerdictRefuses, "1.36.2", 0, 1},
		{"a request that failed", pinWith(&docs.Bump{To: "1.36.2", PaletteValidationError: "HTTP 403: SENTINEL"}, nil), VerdictNoAnswer, "1.36.2", 403, 0},
		{"a failure with no status", pinWith(&docs.Bump{To: "1.36.2", PaletteValidationError: "the link failed"}, nil), VerdictNoAnswer, "1.36.2", 0, 0},
		{"a pin with an error", pinWith(nil, &docs.Error{Kind: docs.KindAPI, Status: 502}), VerdictNoAnswer, "1.36.2", 502, 0},
		{"a pin with no bump", pinWith(nil, nil), VerdictNoAnswer, "1.36.2", 0, 0},
	} {
		t.Run(c.name, func(t *testing.T) {
			a := AnswerOf("aws-stack@1.1.0", "k8s/kubernetes", c.pin)
			if a.Verdict != c.verdict || a.Newest != c.newest || a.Status != c.status || len(a.Refusals) != c.refuses || a.Refusals == nil {
				t.Errorf("%+v", a)
			}
			if a.Profile != "aws-stack@1.1.0" || a.Key != "k8s/kubernetes" || a.Answered() != (c.verdict != VerdictNoAnswer) {
				t.Errorf("%+v", a)
			}
		})
	}
}

// Issue #867. Strata checks two rules of the update that no route of Palette checks before it: a move of
// Kubernetes that skips a minor version, and a required variable with no default value that the move adds.
// The answer keeps each check of the bump beside the verdict of Palette, also beside a request that failed.
// A pin with an error, or with no bump, has no check.
func TestAnAnswerKeepsEachCheckOfStrataBesideTheVerdictOfPalette(t *testing.T) {
	skip := &docs.MinorSkip{Code: "K8sUpgradeNotSupported", Pack: "kubernetes", From: "1.35.3", To: "1.37.0", NextMinor: "1.36", NextStep: "1.36.2"}
	variables := &docs.RequiredVariables{Code: "InvalidVariable", Names: []string{"cost_center", "owner"}}
	both := &docs.UpdateChecks{RequiredVariables: variables, SkippedMinor: skip}
	refusal := docs.Refusal{Code: "DependencyPackUpgradeNotSupported", Message: "SENTINEL"}
	for _, c := range []struct {
		name    string
		pin     docs.Pin
		verdict Verdict
		checks  *docs.UpdateChecks
	}{
		{"a skip beside no error", pinWith(&docs.Bump{To: "1.37.0", PaletteValidation: &docs.Validation{Accepts: true}, UpdateChecks: &docs.UpdateChecks{SkippedMinor: skip}}, nil),
			VerdictNoError, &docs.UpdateChecks{SkippedMinor: skip}},
		{"a variable beside a refusal", pinWith(&docs.Bump{To: "1.37.0", PaletteValidation: &docs.Validation{Errors: []docs.Refusal{refusal}}, UpdateChecks: &docs.UpdateChecks{RequiredVariables: variables}}, nil),
			VerdictRefuses, &docs.UpdateChecks{RequiredVariables: variables}},
		{"both beside a request that failed", pinWith(&docs.Bump{To: "1.37.0", PaletteValidationError: "HTTP 403: SENTINEL", UpdateChecks: both}, nil), VerdictNoAnswer, both},
		{"no check beside no error", pinWith(&docs.Bump{To: "1.36.2", PaletteValidation: &docs.Validation{Accepts: true}}, nil), VerdictNoError, nil},
		{"a pin with an error", pinWith(&docs.Bump{To: "1.37.0", UpdateChecks: both}, &docs.Error{Kind: docs.KindAPI, Status: 502}), VerdictNoAnswer, nil},
		{"a pin with no bump", pinWith(nil, nil), VerdictNoAnswer, nil},
	} {
		t.Run(c.name, func(t *testing.T) {
			a := AnswerOf("aws-stack@1.1.0", "k8s/kubernetes", c.pin)
			if a.Verdict != c.verdict || !reflect.DeepEqual(a.UpdateChecks, c.checks) {
				t.Errorf("verdict %s, checks %+v, want %s and %+v", a.Verdict, a.UpdateChecks, c.verdict, c.checks)
			}
		})
	}
	// The detail tier sends the checks as strata gives them. An answer with no check has no field, as the document of strata.
	for _, c := range []struct {
		checks *docs.UpdateChecks
		want   string
	}{
		{nil, `"status":0}`},
		{both, `"status":0,"updateChecks":{"requiredVariables":{"code":"InvalidVariable","names":["cost_center","owner"]},"skippedMinor":{"code":"K8sUpgradeNotSupported","pack":"kubernetes","from":"1.35.3","to":"1.37.0","nextMinor":"1.36","nextStep":"1.36.2"}}}`},
		{&docs.UpdateChecks{SkippedMinor: &docs.MinorSkip{Code: "K8sUpgradeNotSupported", Pack: "kubernetes", From: "1.35.3", To: "1.37.0", NextMinor: "1.36"}},
			`"status":0,"updateChecks":{"skippedMinor":{"code":"K8sUpgradeNotSupported","pack":"kubernetes","from":"1.35.3","to":"1.37.0","nextMinor":"1.36"}}}`},
	} {
		raw, err := json.Marshal(Answer{Refusals: []docs.Refusal{}, UpdateChecks: c.checks})
		if err != nil {
			t.Fatal(err)
		}
		if !strings.HasSuffix(string(raw), c.want) {
			t.Errorf("%s does not end with %s", raw, c.want)
		}
	}
}

// The document of a real `behind --pin`, from the contract script: the record pins kubernetes 1.34.9, and the move to 1.36.2
// skips a minor version. The answer keeps the check of strata beside the answer of Palette, with the next step.
func TestTheAnswerOfARealMoveThatSkipsAMinorVersionNamesTheNextStep(t *testing.T) {
	raw, err := os.ReadFile("../docs/testdata/ask-skip.json")
	if err != nil {
		t.Fatal(err)
	}
	var doc docs.Behind
	if err := json.Unmarshal(raw, &doc); err != nil {
		t.Fatal(err)
	}
	if len(doc.Pins) != 1 {
		t.Fatalf("%d pins", len(doc.Pins))
	}
	a := AnswerOf("aws-stack@1.1.20", "k8s/kubernetes", doc.Pins[0])
	if a.Verdict != VerdictNoError || a.UpdateChecks == nil || a.UpdateChecks.SkippedMinor == nil || a.UpdateChecks.RequiredVariables != nil {
		t.Fatalf("answer %+v", a)
	}
	s := a.UpdateChecks.SkippedMinor
	if s.Code != "K8sUpgradeNotSupported" || s.Pack != "kubernetes" || s.From != "1.34.9" || s.To != a.Newest || s.NextMinor != "1.35" || s.NextStep != "1.35.6" {
		t.Errorf("the skip %+v", s)
	}
}
