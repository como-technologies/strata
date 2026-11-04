package model

import (
	"slices"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
)

func TestThePageStateFollowsTheFirstRowThatMatches(t *testing.T) {
	every := 15 * time.Minute
	now := t0.Add(time.Hour)
	fresh, old := now.Add(-time.Minute), now.Add(-31*time.Minute)
	e := func(kind string, status int) *ErrorRef { return &ErrorRef{Kind: kind, Status: status} }
	cases := []struct {
		name       string
		filesKnown bool
		files      int
		clock      Clock
		want       Page
	}{
		{"no record file", true, 0, Clock{Every: every, Last: &fresh}, PageNoRecords},
		{"no record file beats a lost key", true, 0, Clock{Every: every, FailedRuns: 5, LastError: e(docs.KindTransport, 0)}, PageNoRecords},
		{"the directory is not read yet", false, 0, Clock{Every: every}, PageFirstLoad},
		{"two 401 with no good cycle", true, 2, Clock{Every: every, FailedRuns: 2, LastError: e(docs.KindAPI, 401)}, PageLost},
		{"a 403", true, 2, Clock{Every: every, FailedRuns: 2, LastError: e(docs.KindAPI, 403)}, PageLost},
		{"a link that fails", true, 2, Clock{Every: every, FailedRuns: 2, LastError: e(docs.KindTransport, 0)}, PageLost},
		{"an answer that is no JSON", true, 2, Clock{Every: every, FailedRuns: 3, LastError: e(docs.KindNotJSON, 0)}, PageLost},
		{"a timeout", true, 2, Clock{Every: every, FailedRuns: 2, LastError: e(docs.KindTimeout, 0)}, PageLost},
		{"lost after a good cycle", true, 2, Clock{Every: every, Last: &fresh, FailedRuns: 2, LastError: e(docs.KindAPI, 401)}, PageLost},
		{"one failed run is not lost", true, 2, Clock{Every: every, FailedRuns: 1, LastError: e(docs.KindAPI, 401)}, PageFirstLoad},
		{"a 500 is not lost", true, 2, Clock{Every: every, FailedRuns: 2, LastError: e(docs.KindAPI, 500)}, PageStale},
		{"a wrong cluster uid", true, 2, Clock{Every: every, FailedRuns: 2, LastError: e(docs.KindUnknown, 0)}, PageStale},
		{"two failed runs after a good cycle that is not old", true, 2, Clock{Every: every, Last: &fresh, FailedRuns: 2, LastError: e(docs.KindUnknown, 0)}, PageOK},
		{"the last good cycle is older than two periods", true, 2, Clock{Every: every, Last: &old}, PageStale},
		{"the last good cycle is two periods old", true, 2, Clock{Every: every, Last: ptr(now.Add(-2 * every))}, PageOK},
		{"no cycle finished", true, 2, Clock{Every: every}, PageFirstLoad},
		{"one failed first run", true, 2, Clock{Every: every, FailedRuns: 1, LastError: e(docs.KindUnknown, 0)}, PageFirstLoad},
		{"a good cycle", true, 2, Clock{Every: every, Last: &fresh}, PageOK},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if got := PageOf(c.filesKnown, c.files, c.clock, now); got != c.want {
				t.Errorf("page %q, want %q", got, c.want)
			}
		})
	}
}

func TestADurationIsShort(t *testing.T) {
	for d, want := range map[time.Duration]string{
		15 * time.Minute: "15m", 6 * time.Hour: "6h", 10 * time.Minute: "10m", 90 * time.Minute: "1h30m", 30 * time.Second: "30s", 0: "0s",
		time.Hour + 5*time.Second: "1h0m5s",
	} {
		if got := shortDuration(d); got != want {
			t.Errorf("%v: %q, want %q", d, got, want)
		}
	}
}

func TestTheWordsOnTheWireAreTheWordsOfThePage(t *testing.T) {
	words := map[string][]string{
		"kind":        {string(PublishedChanged), string(Overwritten), string(Renamed), string(Gone), string(ClusterDiffers), string(ImageIDChanged)},
		"cause":       {string(NoUpstreamDigest), string(OlderRegistryCopy), string(NoRecord), string(ReadError), string(PinError)},
		"status":      {string(StatusDrift), string(StatusNewer), string(StatusNotChecked), string(StatusOK), string(StatusNoRecord)},
		"clusterCopy": {string(CopySame), string(CopySameContent), string(CopyDiffers), string(CopyNotOnThisCluster), string(CopyNotRead)},
		"page":        {string(PageNoRecords), string(PageLost), string(PageStale), string(PageFirstLoad), string(PageOK)},
	}
	want := map[string][]string{
		"kind":        {"published-changed", "overwritten", "renamed", "gone", "cluster-differs", "image-id-changed"},
		"cause":       {"no-upstream-digest", "older-registry-copy", "no-record", "read-error", "pin-error"},
		"status":      {"drift", "newer", "not-checked", "ok", "no-record"},
		"clusterCopy": {"same", "same-content", "differs", "not-on-this-cluster", "not-read"},
		"page":        {"no-records", "lost", "stale", "first-load", "ok"},
	}
	for name, got := range words {
		if !slices.Equal(got, want[name]) {
			t.Errorf("%s: %v, want %v", name, got, want[name])
		}
	}
	if len(causeOrder) != len(want["cause"]) {
		t.Errorf("the order of the causes holds %d causes", len(causeOrder))
	}
}
