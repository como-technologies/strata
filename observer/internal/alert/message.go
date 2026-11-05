package alert

import (
	"fmt"
	"strings"
	"time"

	"github.com/como-technologies/strata/observer/internal/model"
)

// The words of a message. A message holds the title, one line for each subject, and no value.
const (
	titleDrift = "Drift"
	titleMoved = "Image moved"
	titleSplit = "Image split"
	titleNewer = "Newer version"
	titleLost  = "Lost contact"
)

// digestLimit is the most subjects that one digest lists. A webhook refuses a text that is too long.
const digestLimit = 30

// title gives the words after the cluster in the first line of a message.
func (s Subject) title() string {
	switch {
	case s.News == NewsLost:
		return titleLost
	case s.News == NewsNewer:
		return titleNewer
	case s.Kind == model.ImageSplit:
		return titleSplit
	case s.News == NewsImage:
		return titleMoved
	}
	return titleDrift
}

// line is the sentence of a subject: the profile, and then what happened.
func (s Subject) line() string {
	if s.Profile == "" {
		return s.Detail
	}
	return s.Profile + ": " + s.Detail
}

// header is the first line of every message.
func header(cluster, title string) string {
	return fmt.Sprintf("strata · %s · %s", cluster, title)
}

// openText is the message for a subject that opened.
func openText(cluster string, s Subject) string {
	lines := []string{header(cluster, s.title()), s.line()}
	if len(s.Packs) > 0 && s.News != NewsNewer {
		lines = append(lines, "Packs: "+strings.Join(s.Packs, ", "))
	}
	if !s.Since.IsZero() {
		lines = append(lines, "First seen: "+s.Since.UTC().Format(time.RFC3339))
	}
	return strings.Join(lines, "\n")
}

// clearText is the message for a subject that cleared. A check proved it: the observer clears nothing
// after a failed read.
func clearText(cluster string, s Subject, at time.Time) string {
	body := "A check proved that this is gone, at " + at.UTC().Format(time.RFC3339) + "."
	if s.News == NewsLost {
		body = "The observer reached Palette again, at " + at.UTC().Format(time.RFC3339) + "."
	}
	lines := []string{header(cluster, s.title()+" cleared")}
	if s.Profile != "" {
		lines = append(lines, s.Profile)
	}
	return strings.Join(append(lines, body), "\n")
}

// digestText is the message that lists the open subjects of a digest rule.
func digestText(cluster string, subjects []Subject) string {
	lines := []string{header(cluster, "Digest"), fmt.Sprintf("%d open %s:", len(subjects), plural(len(subjects)))}
	for i, s := range subjects {
		if i == digestLimit {
			lines = append(lines, fmt.Sprintf("and %d more.", len(subjects)-digestLimit))
			break
		}
		lines = append(lines, "- "+s.title()+" · "+s.line())
	}
	return strings.Join(lines, "\n")
}

func plural(n int) string {
	if n == 1 {
		return "item"
	}
	return "items"
}

// sampleSubject is the subject of the sample message on the page.
var sampleSubject = Subject{ID: "sample", News: NewsDrift, Kind: model.PublishedChanged, Profile: "fred-identity@2.3.0", Packs: []string{"addon/dex"},
	Detail: model.PublishedChanged.Sentence(), Since: time.Date(2026, 11, 3, 6, 9, 0, 0, time.UTC)}

// Sample gives the message that a finding makes, for a made-up profile. The page shows it, so a team
// sees what the channel gets.
func Sample(cluster string) string {
	return openText(cluster, sampleSubject)
}
