package model

import (
	"github.com/como-technologies/strata/observer/internal/docs"
)

// Verdict is what Palette said to the question about one move. A move that nobody asked about has no
// Answer: the page reads "not asked" from that.
type Verdict string

// The verdicts. No error is not "accepts": Palette checks the default values of the new version, and
// it does not check your own values. No answer is a request that failed, such as a 403 for a key that
// can only read. It is no finding, and it raises no severity.
const (
	VerdictNoError  Verdict = "no-error"
	VerdictRefuses  Verdict = "refuses"
	VerdictNoAnswer Verdict = "no-answer"
)

// Answer is the reply of Palette to the question about the move of one pack of one profile. Only the
// detail tier carries it, because a refusal holds the message of Palette and the names of clusters. The
// summary and the state hold none. Status is the HTTP status of a request that failed, and 0 when the
// failure has none.
type Answer struct {
	Profile  string         `json:"profile"`
	Key      string         `json:"key"`
	Newest   string         `json:"newest"`
	Verdict  Verdict        `json:"verdict"`
	Refusals []docs.Refusal `json:"refusals"`
	Status   int            `json:"status"`
}

// Answered says that Palette gave an answer, with an error or without one. A second question would
// give the same answer, so it is not sent. A request that failed is not an answer.
func (a Answer) Answered() bool {
	return a.Verdict != VerdictNoAnswer
}

// AnswerOf reads the answer for the move of a pack of a profile from the pin that `behind --pin` gave
// back. A pin with an error, with no bump, or with no answer of Palette is a request that failed. The
// words of the failure never leave this function: only the status does.
func AnswerOf(profile, key string, pin docs.Pin) Answer {
	a := Answer{Profile: profile, Key: key, Verdict: VerdictNoAnswer, Refusals: []docs.Refusal{}}
	if len(pin.Newer) > 0 {
		a.Newest = pin.Newer[0].Version
	}
	switch {
	case pin.Error != nil:
		a.Status = pin.Error.Status
	case pin.Bump == nil:
	case pin.Bump.PaletteValidation == nil:
		a.Newest = pin.Bump.To
		a.Status = docs.StatusOf(pin.Bump.PaletteValidationError)
	case pin.Bump.PaletteValidation.Accepts:
		a.Newest, a.Verdict = pin.Bump.To, VerdictNoError
	default:
		a.Newest, a.Verdict = pin.Bump.To, VerdictRefuses
		a.Refusals = append(a.Refusals, pin.Bump.PaletteValidation.Errors...)
	}
	return a
}
