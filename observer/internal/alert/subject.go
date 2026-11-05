package alert

import (
	"fmt"
	"strings"
	"time"

	"github.com/como-technologies/strata/observer/internal/model"
)

// Subject is one thing that an alert is about: a finding, a newer version of a pack in one profile, or
// the loss of contact with Palette. It holds names, times, and fixed words, and no value.
type Subject struct {
	ID   string     `json:"id"`
	News News       `json:"news"`
	Kind model.Kind `json:"kind,omitempty"`
	// Profile is name@version. It is empty for the loss of contact.
	Profile string   `json:"profile,omitempty"`
	Packs   []string `json:"packs,omitempty"`
	// Detail is one sentence. Since is the first sight of a finding, and nil for the other news.
	Detail string     `json:"detail"`
	Since  *time.Time `json:"since,omitempty"`
}

// lostID is the id of the loss of contact. A finding has an id of 16 hex digits, so the two never meet.
const lostID = "lost"

// newerPrefix starts the id of a newer version, which is the key of the move and the profile.
const newerPrefix = "newer:"

// SubjectsOf lists what is open in a summary: each finding, each newer version in each profile, and the
// loss of contact when the page says lost. It is the one function of the package that reads the types
// of internal/model.
func SubjectsOf(sum model.Summary) []Subject {
	out := []Subject{}
	for _, f := range sum.Findings {
		out = append(out, findingSubject(f))
	}
	keys := packKeys(sum.Profiles)
	for _, n := range sum.Newer {
		for _, profile := range n.Profiles {
			out = append(out, newerSubject(n, profile, keys))
		}
	}
	if sum.Page == model.PageLost {
		out = append(out, lostSubject(sum.Content))
	}
	return out
}

func findingSubject(f model.Finding) Subject {
	news := NewsDrift
	if f.Kind.IsImage() {
		news = NewsImage
	}
	since := f.FirstSeen
	return Subject{ID: f.ID, News: news, Kind: f.Kind, Profile: f.Profile, Packs: f.Packs, Detail: f.Kind.Sentence(), Since: &since}
}

// packKeys maps a pack of a profile, by the profile, the name, and the version, to its key. A row of
// the newer versions names a pack by its name, and a rule of the level pack can name the key.
func packKeys(profiles []model.Profile) map[string]string {
	keys := map[string]string{}
	for _, p := range profiles {
		for _, k := range p.Packs {
			keys[model.Subject(p.Name, p.Version)+"|"+k.Name+"|"+k.Version] = k.Key
		}
	}
	return keys
}

func newerSubject(n model.Newer, profile string, keys map[string]string) Subject {
	key := keys[profile+"|"+n.Name+"|"+n.Version]
	if key == "" {
		key = n.Name
	}
	return Subject{ID: newerPrefix + n.Key() + "|" + profile, News: NewsNewer, Profile: profile, Packs: []string{key},
		Detail: fmt.Sprintf("%s %s has a newer version, %s.", n.Name, n.Version, n.Newest)}
}

func lostSubject(c model.ContentClock) Subject {
	detail := fmt.Sprintf("The observer cannot reach Palette. The last %d content checks failed", c.FailedRuns)
	if c.LastError != nil {
		detail += ": " + c.LastError.Kind
		if c.LastError.Status != 0 {
			detail += fmt.Sprintf(", HTTP %d", c.LastError.Status)
		}
	}
	return Subject{ID: lostID, News: NewsLost, Detail: detail + "."}
}

// profileName cuts the version from name@version.
func profileName(subject string) string {
	name, _, _ := strings.Cut(subject, "@")
	return name
}

// packName cuts the layer from <layer>/<name>.
func packName(key string) string {
	_, name, found := strings.Cut(key, "/")
	if !found {
		return key
	}
	return name
}
