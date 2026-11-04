package model

import (
	"slices"
	"strings"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
)

// lostRuns is the number of failed runs in a row after which a lost key or link shows.
const lostRuns = 2

// stalePeriods is the number of periods of the content cycle after which the last good cycle is old.
const stalePeriods = 2

// PageOf gives the state of the page. The first row of the table that matches wins:
// no-records, lost, stale, first-load, and ok. It reads the content clock only, because a good
// content cycle shows that the key and the link work. filesKnown is false before the first read of
// the directory of records.
func PageOf(filesKnown bool, files int, content Clock, now time.Time) Page {
	switch {
	case filesKnown && files == 0:
		return PageNoRecords
	case content.FailedRuns >= lostRuns && content.LastError.isLost():
		return PageLost
	case content.FailedRuns >= lostRuns && content.Last == nil:
		return PageStale
	case content.Last != nil && content.Every > 0 && now.Sub(*content.Last) > stalePeriods*content.Every:
		return PageStale
	case content.Last == nil:
		return PageFirstLoad
	}
	return PageOK
}

// isLost says that the key or the link to Palette does not work: a refusal of the key, a link that
// does not carry, an answer that is no JSON, or a timeout. Another kind says that the cycle is
// wrong, so the page is stale.
func (e *ErrorRef) isLost() bool {
	if e == nil {
		return false
	}
	switch e.Kind {
	case docs.KindAPI:
		return e.Status == 401 || e.Status == 403
	case docs.KindTransport, docs.KindNotJSON, docs.KindTimeout:
		return true
	}
	return false
}

// eventKinds names the event of each kind of finding. A finding is always a drift.
var eventKinds = map[Kind]struct{ reason, message string }{
	PublishedChanged: {"DriftFound", "The published version differs from its record."},
	Overwritten:      {"Overwritten", "Another version replaced the recorded version in place."},
	Renamed:          {"Renamed", "The recorded version has another name now."},
	Gone:             {"Gone", "Palette no longer holds the recorded version."},
	ClusterDiffers:   {"ClusterDiffers", "The cluster runs other content than the record."},
	ImageIDChanged:   {"ImageIDChanged", "A pack names another machine image."},
}

// eventType is the type of each event: the observer reports drift only.
const eventType = "drift"

// eventsOf gives one event for each finding, the newest first.
func eventsOf(findings []Finding) []Event {
	out := make([]Event, 0, len(findings))
	for _, f := range findings {
		e := eventKinds[f.Kind]
		out = append(out, Event{Type: eventType, Reason: e.reason, Time: f.FirstSeen, Object: f.Profile, Message: e.message})
	}
	sortEvents(out)
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

// limitsOf counts what a check does not cover. The images and the Helm packs are those of the
// records that the cluster runs. The masked paths are those of each diff that the cycle holds.
func limitsOf(c *Content) Limits {
	var l Limits
	for _, r := range c.Records {
		if r.Cluster.Runs {
			for _, p := range r.Doc.Packs {
				l.ImagesNamedByATag += tagImages(p.Images)
				if p.Type == docs.PackHelm || p.RegistryProviderType == docs.PackHelm {
					l.HelmPacks++
				}
			}
		}
		if r.Row != nil {
			l.MaskedPaths += maskedPaths(r.Row.Diff)
		}
		l.MaskedPaths += maskedPaths(r.Cluster.Diff)
	}
	return l
}

func tagImages(images []string) int {
	n := 0
	for _, ref := range images {
		if !strings.Contains(ref, "@sha256") {
			n++
		}
	}
	return n
}

func maskedPaths(d *docs.Diff) int {
	if d == nil {
		return 0
	}
	n := 0
	for _, p := range d.Packs {
		n += len(p.Masked)
	}
	return n
}
