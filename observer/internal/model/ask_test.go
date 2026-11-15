package model

import (
	"reflect"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

func pinWith(bump *docs.Bump, err *docs.Error) docs.Pin {
	p := pin("kubernetes", "k8s", "1.35.3", "1.36.2")
	p.Bump, p.Error = bump, err
	return p
}

// The checks of the update that strata gives for the move from the pin of pinWith to kubernetes 1.37.0.
var (
	skipped    = &docs.MinorSkip{Code: testkit.CodeMinorSkip, Pack: "kubernetes", From: "1.35.3", To: "1.37.0", NextMinor: "1.36", NextStep: "1.36.2"}
	required   = &docs.RequiredVariables{Code: testkit.CodeVariable, Names: []string{"cost_center", "owner"}}
	bothChecks = &docs.UpdateChecks{RequiredVariables: required, SkippedMinor: skipped}
)

// The verdict comes from the pin that `behind --pin` gives. Issue #867: strata checks two rules of the update that no
// route of Palette checks before it. They are a move of Kubernetes that skips a minor version, and a required variable
// with no default value that the move adds. The answer keeps each check of the bump beside the verdict of Palette, also
// beside a request that failed. A pin with an error, or with no bump, has no check.
func TestAnswerOfReadsEachVerdictFromThePinThatBehindGives(t *testing.T) {
	refusal := docs.Refusal{Code: testkit.CodeDependency, Message: "SENTINEL", Clusters: []string{testkit.PaymentsName}}
	for _, c := range []struct {
		name    string
		pin     docs.Pin
		verdict Verdict
		newest  string
		status  int
		refuses int
		checks  *docs.UpdateChecks
	}{
		{"no error", pinWith(&docs.Bump{To: "1.37.0", PaletteValidation: &docs.Validation{Accepts: true}, UpdateChecks: &docs.UpdateChecks{SkippedMinor: skipped}}, nil),
			VerdictNoError, "1.37.0", 0, 0, &docs.UpdateChecks{SkippedMinor: skipped}},
		{"a refusal", pinWith(&docs.Bump{To: "1.36.2", PaletteValidation: &docs.Validation{Errors: []docs.Refusal{refusal}}, UpdateChecks: &docs.UpdateChecks{RequiredVariables: required}}, nil),
			VerdictRefuses, "1.36.2", 0, 1, &docs.UpdateChecks{RequiredVariables: required}},
		{"a request that failed", pinWith(&docs.Bump{To: "1.37.0", PaletteValidationError: "HTTP 403: SENTINEL", UpdateChecks: bothChecks}, nil), VerdictNoAnswer, "1.37.0", 403, 0, bothChecks},
		{"a failure with no status", pinWith(&docs.Bump{To: "1.36.2", PaletteValidationError: "the link failed"}, nil), VerdictNoAnswer, "1.36.2", 0, 0, nil},
		{"a pin with an error", pinWith(&docs.Bump{To: "1.37.0", UpdateChecks: bothChecks}, &docs.Error{Kind: docs.KindAPI, Status: 502}), VerdictNoAnswer, "1.36.2", 502, 0, nil},
		{"a pin with no bump", pinWith(nil, nil), VerdictNoAnswer, "1.36.2", 0, 0, nil},
	} {
		t.Run(c.name, func(t *testing.T) {
			a := AnswerOf(testkit.AWSStackProfile, testkit.KubernetesKey, c.pin)
			if a.Verdict != c.verdict || a.Newest != c.newest || a.Status != c.status || len(a.Refusals) != c.refuses || a.Refusals == nil {
				t.Errorf("%+v", a)
			}
			if a.Profile != testkit.AWSStackProfile || a.Key != testkit.KubernetesKey || a.Answered() != (c.verdict != VerdictNoAnswer) {
				t.Errorf("%+v", a)
			}
			if !reflect.DeepEqual(a.UpdateChecks, c.checks) {
				t.Errorf("checks %+v, want %+v", a.UpdateChecks, c.checks)
			}
		})
	}
}

// The detail tier sends the checks as strata gives them. An answer with no check has no field, as the document of strata.
func TestAnAnswerSendsTheChecksAsStrataGivesThem(t *testing.T) {
	// The skip of bothChecks with no next step: no version of the next minor version is in the registry.
	noStep := *skipped
	noStep.NextStep = ""
	const skipJSON = `{"code":"` + testkit.CodeMinorSkip + `","pack":"kubernetes","from":"1.35.3","to":"1.37.0","nextMinor":"1.36"`
	for _, c := range []struct {
		checks *docs.UpdateChecks
		want   string
	}{
		{nil, `"status":0}`},
		{bothChecks, `"status":0,"updateChecks":{"requiredVariables":{"code":"` + testkit.CodeVariable + `","names":["cost_center","owner"]},"skippedMinor":` + skipJSON + `,"nextStep":"1.36.2"}}}`},
		{&docs.UpdateChecks{SkippedMinor: &noStep}, `"status":0,"updateChecks":{"skippedMinor":` + skipJSON + `}}}`},
	} {
		raw := testkit.MustJSON(t, Answer{Refusals: []docs.Refusal{}, UpdateChecks: c.checks})
		if !strings.HasSuffix(string(raw), c.want) {
			t.Errorf("%s does not end with %s", raw, c.want)
		}
	}
}

// The document of a real `behind --pin`, from the contract script: the record pins kubernetes 1.34.9, and the move to 1.36.2
// skips a minor version. The answer keeps the check of strata beside the answer of Palette, with the next step.
func TestTheAnswerOfARealMoveThatSkipsAMinorVersionNamesTheNextStep(t *testing.T) {
	var doc docs.Behind
	testkit.ReadDoc(t, "ask-skip.json", &doc)
	if len(doc.Pins) != 1 {
		t.Fatalf("%d pins", len(doc.Pins))
	}
	a := AnswerOf("aws-stack@1.1.20", testkit.KubernetesKey, doc.Pins[0])
	if a.Verdict != VerdictNoError || a.UpdateChecks == nil || a.UpdateChecks.SkippedMinor == nil || a.UpdateChecks.RequiredVariables != nil {
		t.Fatalf("answer %+v", a)
	}
	s := a.UpdateChecks.SkippedMinor
	if s.Code != skipped.Code || s.Pack != "kubernetes" || s.From != "1.34.9" || s.To != a.Newest || s.NextMinor != "1.35" || s.NextStep != "1.35.6" {
		t.Errorf("the skip %+v", s)
	}
}
