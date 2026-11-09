package api

import (
	"net/http"
	"testing"

	"github.com/como-technologies/strata/observer/internal/model"
)

// pathCycle is the click "Check now" on the content cycle.
const pathCycle = "/v1/cycles/content"

// cycles is a Source that answers the click "Check now" with start, and keeps each name that it got.
type cycles struct {
	stub
	start model.Start
	names *[]string
}

func (c cycles) StartCycle(name string) (model.Start, bool) {
	*c.names = append(*c.names, name)
	if name != "content" && name != "versions" {
		return model.Start{}, false
	}
	s := c.start
	s.Cycle = name
	return s, true
}

func cyclesWith(state string, retry int) (http.Handler, *[]string) {
	names := &[]string{}
	return handlerOf(cycles{start: model.Start{State: state, RetryAfter: retry}, names: names}), names
}

// A cycle that starts, or that runs already, gets 202 with its state. A cycle that ended less than a
// minute ago gets 429 with the seconds to wait, in the body and in the header Retry-After.
func TestTheClickCheckNowAnswers202ForACycleThatStartsOrRunsAnd429WithRetryAfterForAWait(t *testing.T) {
	for _, tc := range []struct {
		state, retryAfter string
		retry, code       int
	}{
		{model.StartStarted, "", 0, http.StatusAccepted},
		{model.StartRunning, "", 0, http.StatusAccepted},
		{model.StartWait, "42", 42, http.StatusTooManyRequests},
	} {
		h, names := cyclesWith(tc.state, tc.retry)
		rec := click(h, "/v1/cycles/versions")
		var got model.Start
		decode(t, rec, &got)
		if rec.Code != tc.code || got != (model.Start{Cycle: "versions", State: tc.state, RetryAfter: tc.retry}) {
			t.Errorf("%s: status %d, body %+v", tc.state, rec.Code, got)
		}
		if rec.Header().Get("Retry-After") != tc.retryAfter {
			t.Errorf("%s: Retry-After %q", tc.state, rec.Header().Get("Retry-After"))
		}
		if len(*names) != 1 || (*names)[0] != "versions" {
			t.Errorf("%s: the source got %v", tc.state, *names)
		}
	}
}

// A name that is no cycle gets 404. A POST that the page did not send, or that comes to a Host that is
// not loopback, starts nothing.
func TestTheClickCheckNowRefusesAnotherNameAndAPostThatThePageDidNotSend(t *testing.T) {
	h, names := cyclesWith(model.StartStarted, 0)
	if rec := click(h, "/v1/cycles/pods"); rec.Code != http.StatusNotFound {
		t.Errorf("another name gave %d", rec.Code)
	}
	*names = nil
	for _, tc := range []struct {
		host, site string
		code       int
	}{
		{"127.0.0.1:8080", "", http.StatusForbidden},
		{"127.0.0.1:8080", "cross-site", http.StatusForbidden},
		{"evil.example", sameOrigin, http.StatusMisdirectedRequest},
	} {
		if rec := clickWith(h, pathCycle, tc.host, tc.site); rec.Code != tc.code {
			t.Errorf("host %s, site %q: %d", tc.host, tc.site, rec.Code)
		}
	}
	if len(*names) != 0 {
		t.Errorf("a refused request reached the source: %v", *names)
	}
}
