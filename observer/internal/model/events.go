package model

import (
	"fmt"
	"slices"
	"strings"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
)

// eventKinds names the event of each kind of finding. A finding is always a drift.
var eventKinds = map[Kind]struct{ reason, message string }{
	PublishedChanged: {"DriftFound", "The published version differs from its record."},
	Overwritten:      {"Overwritten", "Another version replaced the recorded version in place."},
	Renamed:          {"Renamed", "The recorded version has another name now."},
	Gone:             {"Gone", "Palette no longer holds the recorded version."},
	ClusterDiffers:   {"ClusterDiffers", "The cluster runs other content than the record."},
	ImageIDChanged:   {"ImageIDChanged", "A pack names another machine image."},
}

// The reasons of the events that no kind of finding names.
const (
	reasonProfileUpdated = "ProfileUpdated"
	reasonNewVersion     = "NewVersion"
	reasonOldCopy        = "OldCopy"
	reasonAuditUnread    = "AuditUnread"
)

// eventsOf gives the events of the last 7 days, the newest first. A finding makes a drift event at
// its first sight. Each write of its `who` makes an info event at the time of the write. A finding
// whose audit log strata could not read makes one info event that says so, at its last sight: the
// event states what is true now. A newer version makes an event at its listing. A registry copy
// that is older makes one at its last sync. An event that is older than the window is not in the
// list.
func eventsOf(findings []Finding, versions *Versions, now time.Time) []Event {
	out := []Event{}
	add := func(e Event) {
		if within(e.Time, now) {
			e.Time = utc(e.Time)
			out = append(out, e)
		}
	}
	for _, f := range findings {
		k := eventKinds[f.Kind]
		add(Event{Type: EventDrift, Reason: k.reason, Time: f.FirstSeen, Object: f.Profile, Message: k.message})
		for _, w := range f.Who {
			if t, ok := parseTime(w.Time); ok {
				add(Event{Type: EventInfo, Reason: reasonProfileUpdated, Time: t, Object: f.Profile, Message: whoMessage(w)})
			}
		}
		if f.WhoUnread {
			add(Event{Type: EventInfo, Reason: reasonAuditUnread, Time: f.LastSeen, Object: f.Profile, Message: whoUnreadMessage})
		}
	}
	if versions != nil {
		for _, e := range versionEvents(versions) {
			add(e)
		}
	}
	sortEvents(out)
	return out
}

// whoUnreadMessage is the event of a finding whose audit log strata could not read. It holds no text
// of the error: the log of the observer holds it.
const whoUnreadMessage = "The audit log could not be read, so this does not say who made the change."

// whoMessage tells one write of the audit log: the actor, the type of the actor, and the action.
func whoMessage(w docs.Who) string {
	actor := w.Actor
	if w.ActorType != "" {
		actor += " (" + w.ActorType + ")"
	}
	return fmt.Sprintf("%s made this change: %s.", actor, w.Action)
}

// versionEvents gives an event for each version that Palette lists, and for each registry whose copy
// is older. The object is the profiles that the pins name.
func versionEvents(vs *Versions) []Event {
	var out []Event
	type copyOf struct {
		at       time.Time
		profiles []string
	}
	copies := map[string]*copyOf{}
	for _, pin := range vs.Pins {
		profiles := pinProfiles(pin)
		if pin.Behind && len(pin.Newer) > 0 {
			if t, ok := parseTime(pin.Newer[0].Listed); ok {
				out = append(out, Event{Type: EventNewer, Reason: reasonNewVersion, Time: t, Object: strings.Join(profiles, ", "),
					Message: fmt.Sprintf("%s %s is listed.", pin.Name, pin.Newer[0].Version)})
			}
		}
		if pin.RegistrySync == nil {
			continue
		}
		t, ok := parseTime(pin.RegistrySync.LastSyncedTime)
		if !ok {
			continue
		}
		c := copies[registryName(pin)]
		if c == nil {
			c = &copyOf{at: t}
			copies[registryName(pin)] = c
		}
		c.profiles = append(c.profiles, profiles...)
	}
	for registry, c := range copies {
		slices.Sort(c.profiles)
		out = append(out, Event{Type: EventNotChecked, Reason: reasonOldCopy, Time: c.at, Object: strings.Join(slices.Compact(c.profiles), ", "),
			Message: fmt.Sprintf("Current against a copy of %s from %s.", registry, c.at.UTC().Format("Jan 2"))})
	}
	return out
}

// pinProfiles lists the profiles that pin a pack, sorted.
func pinProfiles(pin docs.Pin) []string {
	out := make([]string, 0, len(pin.PinnedBy))
	for _, by := range pin.PinnedBy {
		out = append(out, by.Subject)
	}
	slices.Sort(out)
	return out
}

func sortEvents(events []Event) {
	slices.SortStableFunc(events, func(a, b Event) int {
		if c := b.Time.Compare(a.Time); c != 0 {
			return c
		}
		return strings.Compare(a.Object+a.Reason, b.Object+b.Reason)
	})
}

// parseTime reads a time of a document of strata. A time that does not parse gives false.
func parseTime(s string) (time.Time, bool) {
	t, err := time.Parse(time.RFC3339, s)
	return t, err == nil
}

// within says that a time is in the last 7 days.
func within(t, now time.Time) bool {
	return now.Sub(t) <= newWindow
}
