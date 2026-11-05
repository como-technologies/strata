package model

import (
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
