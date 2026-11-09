package model

import (
	"encoding/json"
	"slices"
	"strings"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
)

func at(d time.Duration) string {
	return t0.Add(d).Format(time.RFC3339)
}

// eventsAt builds the summary at t0 plus one minute, and gives its events.
func eventsAt(c *Content, v *Versions, firstSeen map[string]time.Time) []Event {
	return Build(Input{Now: t0.Add(time.Minute), ClusterUID: "c1", StartedAt: t0, Files: 2, FilesKnown: true, Content: c, Versions: v, FirstSeen: firstSeen,
		ContentClock: goodClock(15*time.Minute, t0), VersionsClock: goodClock(6*time.Hour, t0)}).Events
}

func reasons(events []Event) []string {
	var out []string
	for _, e := range events {
		out = append(out, string(e.Type)+" "+e.Reason+" "+e.Object)
	}
	return out
}

func TestTheEventsOfTheLastSevenDaysCarryFourReasons(t *testing.T) {
	who := &docs.Verify{Who: []docs.Who{{Time: at(-6 * time.Hour), Actor: "Betty Rubble", ActorType: "user", Action: "update"}}}
	c := content(t, drifted(rec("fred-identity", "2.3.0", docs.VerifyUnchanged, pk("addon", "dex", "2.42.0")), who, "addon/dex"))
	kubernetes := pin("traefik", "addon", "37.0.0", "38.1.0", "wilma-gateway@3.1.2")
	kubernetes.Newer[0].Listed = at(-48 * time.Hour)
	bitnami := pin("coredns", "addon", "1.0.0", "", "rubble-dns@1.0.0", "hoppy-autoscale@1.0.0")
	bitnami.Registry = "Bitnami"
	bitnami.RegistrySync = &docs.RegistrySync{LastSyncedTime: "2026-10-30T09:00:00Z"}
	v := &Versions{At: t0, Strata: "0.29.0", Pins: []docs.Pin{kubernetes, bitnami}, Moves: map[string]Move{}}
	events := eventsAt(&c, v, map[string]time.Time{FindingID(PublishedChanged, "fred-identity@2.3.0"): t0.Add(-5 * time.Hour)})
	want := []string{
		"drift DriftFound fred-identity@2.3.0",
		"info ProfileUpdated fred-identity@2.3.0",
		"newer NewVersion wilma-gateway@3.1.2",
		"not-checked OldCopy hoppy-autoscale@1.0.0, rubble-dns@1.0.0",
	}
	if got := reasons(events); !slices.Equal(got, want) {
		t.Fatalf("events %q\nwant %q", got, want)
	}
	if events[1].Message != "Betty Rubble (user) made this change: update." || events[2].Message != "traefik@38.1.0 is listed." ||
		events[3].Message != "Current against a copy of Bitnami from Oct 30." {
		t.Errorf("messages %+v", events)
	}
	if !events[0].Time.Equal(t0.Add(-5*time.Hour)) || !events[1].Time.Equal(t0.Add(-6*time.Hour)) {
		t.Errorf("times %v %v", events[0].Time, events[1].Time)
	}
}

func TestTheEventsAreOrderedByTimeTheNewestFirst(t *testing.T) {
	who := &docs.Verify{Who: []docs.Who{{Time: at(-6 * time.Hour), Actor: "Betty", ActorType: "user", Action: "update"}}}
	c := content(t, drifted(rec("fred-identity", "2.3.0", docs.VerifyUnchanged, pk("addon", "dex", "2.42.0")), who, "addon/dex"))
	p := pin("traefik", "addon", "37.0.0", "38.1.0", "wilma-gateway@3.1.2")
	p.Newer[0].Listed = at(-48 * time.Hour)
	events := eventsAt(&c, &Versions{At: t0, Pins: []docs.Pin{p}}, map[string]time.Time{FindingID(PublishedChanged, "fred-identity@2.3.0"): t0.Add(-5 * time.Hour)})
	for i := 1; i < len(events); i++ {
		if events[i].Time.After(events[i-1].Time) {
			t.Errorf("event %d is newer than event %d: %+v", i, i-1, events)
		}
	}
	if len(events) != 3 {
		t.Errorf("events %+v", events)
	}
}

func TestAnEventOlderThanSevenDaysIsNotInTheList(t *testing.T) {
	who := &docs.Verify{Who: []docs.Who{{Time: at(-8 * 24 * time.Hour), Actor: "Betty", ActorType: "user", Action: "update"}}}
	c := content(t, drifted(rec("a-stack", "1.0.0", docs.VerifyUnchanged, pk("addon", "dex", "2.42.0")), who, "addon/dex"))
	old := pin("traefik", "addon", "37.0.0", "38.1.0", "a-stack@1.0.0")
	old.Newer[0].Listed = at(-8 * 24 * time.Hour)
	stale := pin("coredns", "addon", "1.0.0", "", "a-stack@1.0.0")
	stale.RegistrySync = &docs.RegistrySync{LastSyncedTime: at(-8 * 24 * time.Hour)}
	s := Build(Input{Now: t0.Add(time.Minute), Files: 2, FilesKnown: true, Content: &c, Versions: &Versions{At: t0, Pins: []docs.Pin{old, stale}},
		ContentClock: goodClock(15*time.Minute, t0), VersionsClock: goodClock(6*time.Hour, t0),
		FirstSeen: map[string]time.Time{FindingID(PublishedChanged, "a-stack@1.0.0"): t0.Add(-8 * 24 * time.Hour)}})
	if len(s.Events) != 0 {
		t.Errorf("events %+v", s.Events)
	}
	// The finding, the newer row, and the cause stay: only the list of events has a window.
	if len(s.Findings) != 1 || len(s.Newer) != 1 || len(s.NotChecked) != 1 {
		t.Errorf("findings %d, newer %d, not checked %d", len(s.Findings), len(s.Newer), len(s.NotChecked))
	}
}

func TestAListingOrASyncThatDoesNotParseMakesNoEvent(t *testing.T) {
	p := pin("traefik", "addon", "37.0.0", "38.1.0", "a-stack@1.0.0")
	p.Newer[0].Listed = "yesterday"
	q := pin("coredns", "addon", "1.0.0", "", "a-stack@1.0.0")
	q.RegistrySync = &docs.RegistrySync{LastSyncedTime: ""}
	if events := eventsAt(nil, &Versions{At: t0, Pins: []docs.Pin{p, q}}, nil); len(events) != 0 {
		t.Errorf("events %+v", events)
	}
}

func TestAnEventWithNoActorTypeHasNoEmptyBrackets(t *testing.T) {
	if got := whoMessage(docs.Who{Actor: "Betty", Action: "update"}); strings.Contains(got, "()") || got != "Betty made this change: update." {
		t.Errorf("message %q", got)
	}
}

func TestParseSideKnowsThreeSides(t *testing.T) {
	for word, want := range map[string]Side{"published": SidePublished, "cluster": SideCluster, "newer": SideNewer} {
		if got, ok := ParseSide(word); !ok || got != want {
			t.Errorf("%s: %q %v", word, got, ok)
		}
	}
	for _, word := range []string{"", "Published", "behind", "published ", "../"} {
		if _, ok := ParseSide(word); ok {
			t.Errorf("%q is no side", word)
		}
	}
}

func TestADriftedRecordWhoseAuditLogCouldNotBeReadStillGivesAFindingAndSaysSo(t *testing.T) {
	const reason = "HTTP 403: AUDIT-REASON-SENTINEL"
	r := drifted(rec("fred-identity", "2.3.0", docs.VerifyUnchanged, pk("addon", "dex", "2.42.0")), &docs.Verify{WhoError: reason}, "addon/dex")
	c := content(t, r)
	s := build(&c, nil)
	if len(s.Findings) != 1 || s.Findings[0].Kind != PublishedChanged || !s.Findings[0].WhoUnread || len(s.Findings[0].Who) != 0 {
		t.Fatalf("findings %+v", s.Findings)
	}
	for _, n := range s.NotChecked {
		if n.Cause == ReadError {
			t.Errorf("the drift is a finding, and not a read-error: %+v", n)
		}
	}
	if s.Counts.Drift != 1 || s.Counts.NotChecked != 0 {
		t.Errorf("counts %+v", s.Counts)
	}
	// the notice is as new as the last sight, and the drift is as old as the first
	want := []string{"info AuditUnread fred-identity@2.3.0", "drift DriftFound fred-identity@2.3.0"}
	if got := reasons(s.Events); !slices.Equal(got, want) {
		t.Fatalf("events %q\nwant %q", got, want)
	}
	if got := s.Events[0].Message; got != "The audit log could not be read, so this does not say who made the change." {
		t.Errorf("message %q", got)
	}
	// the summary carries the flag, and never the text of the error
	raw, err := json.Marshal(s)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(raw), "AUDIT-REASON-SENTINEL") || !strings.Contains(string(raw), `"whoUnread":true`) {
		t.Errorf("the summary %s", raw)
	}
}

func TestAFindingWithItsWritesSaysNothingOfAnUnreadAuditLog(t *testing.T) {
	who := &docs.Verify{Who: []docs.Who{{Time: at(-time.Hour), Actor: "Betty Rubble", ActorType: "user", Action: "update"}}}
	c := content(t, drifted(rec("fred-identity", "2.3.0", docs.VerifyUnchanged, pk("addon", "dex", "2.42.0")), who, "addon/dex"))
	s := build(&c, nil)
	if s.Findings[0].WhoUnread {
		t.Errorf("finding %+v", s.Findings[0])
	}
	for _, e := range s.Events {
		if e.Reason == reasonAuditUnread {
			t.Errorf("event %+v", e)
		}
	}
}
