package alert

import (
	"cmp"
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
	// Cluster and ClusterUID name the cluster of a subject of a set of clusters. They are empty with no --clusters, where
	// each message names the one cluster.
	Cluster    string `json:"cluster,omitempty"`
	ClusterUID string `json:"clusterUid,omitempty"`
}

// lostID is the id of the loss of contact. A finding has an id of 16 hex digits, so the two never meet.
const lostID = "lost"

// newerPrefix starts the id of a newer version, which is the key of the move and the profile.
const newerPrefix = "newer:"

// SubjectsOf lists what is open in a summary: each finding, each newer version in each profile, and the
// loss of contact when the page says lost. It is the entry point of this file, which is the one file of
// the package that reads the types of the summary in internal/model.
func SubjectsOf(sum model.Summary) []Subject {
	out := clusterSubjects(sum, "")
	if sum.Page == model.PageLost {
		out = append(out, lostSubject(sum.Content))
	}
	return out
}

// MemberSubjects lists what is open in the summary of one cluster of a set: each finding, and each newer version in
// each profile. Each subject names its cluster, and the id of a newer version holds the uid of the cluster: two
// clusters can run one profile version. The loss of contact belongs to the whole set: see SetLost.
func MemberSubjects(sum model.Summary) []Subject {
	out := clusterSubjects(sum, sum.Cluster.UID+"/")
	for i := range out {
		out[i].Cluster, out[i].ClusterUID = cmp.Or(sum.Cluster.Name, sum.Cluster.UID), sum.Cluster.UID
	}
	return out
}

// SetLost gives the loss of contact of a set, when the page of the set says lost: no check of any project ended
// well. The second result is false when the page is not lost.
func SetLost(sum model.SetSummary) (Subject, bool) {
	if sum.Page != model.PageLost {
		return Subject{}, false
	}
	return lostSubject(sum.Content), true
}

// clusterSubjects lists each finding and each newer version in each profile of the summary of one cluster. scope
// starts the id of a newer version after its prefix: empty with no set.
func clusterSubjects(sum model.Summary, scope string) []Subject {
	out := []Subject{}
	for _, f := range sum.Findings {
		out = append(out, findingSubject(f))
	}
	keys := packKeys(sum.Profiles)
	for _, n := range sum.Newer {
		for _, profile := range n.Profiles {
			out = append(out, newerSubject(n, profile, keys, scope))
		}
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

func newerSubject(n model.Newer, profile string, keys map[string]string, scope string) Subject {
	key := keys[profile+"|"+n.Name+"|"+n.Version]
	if key == "" {
		key = n.Name
	}
	return Subject{ID: newerPrefix + scope + n.Key() + "|" + profile, News: NewsNewer, Profile: profile, Packs: []string{key},
		Detail: fmt.Sprintf("%s@%s has a newer version, %s@%s.", n.Name, n.Version, n.Name, n.Newest)}
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
