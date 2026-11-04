package model

import (
	"crypto/sha256"
	"encoding/hex"
	"slices"
	"strconv"
	"strings"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
)

// idLength is the number of hex digits in the id of a finding.
const idLength = 16

// FindingID is the first 16 hex digits of the SHA-256 of the kind and the subject.
func FindingID(kind Kind, subject string) string {
	sum := sha256.Sum256([]byte(string(kind) + "\n" + subject))
	return hex.EncodeToString(sum[:])[:idLength]
}

// FindingsOf gives one finding for each cause, in the order of the profile and the kind. The
// severity is a table in the page, and a finding never lowers a drift. It leaves the times empty.
func FindingsOf(c Content) []Finding {
	out := []Finding{}
	for _, r := range c.Records {
		for _, f := range recordFindings(r) {
			f.ReBaselineNeeded = ReBaselineNeeded(r.Doc.Tool.Version, c.Strata)
			out = append(out, f)
		}
	}
	slices.SortStableFunc(out, func(a, b Finding) int {
		if c := strings.Compare(a.Profile, b.Profile); c != 0 {
			return c
		}
		return strings.Compare(string(a.Kind), string(b.Kind))
	})
	return out
}

// recordFindings reads the verify row and the compare of one record.
func recordFindings(r Record) []Finding {
	subject := r.subject()
	var published *Finding
	if r.Row != nil && r.Row.Status == docs.VerifyDrifted {
		f := publishedFinding(subject, *r.Row)
		published = &f
	}
	var out []Finding
	if d := r.Cluster.Diff; r.Cluster.Runs && d != nil {
		if !d.Same && !d.SameContent {
			f := newFinding(ClusterDiffers, subject, changedKeys(*d), d.Summary.MaxImpact)
			// One edit that reached the cluster is one cause. The roots tell it.
			if published != nil && published.Kind == PublishedChanged && r.Row.RootLive != "" && r.Row.RootLive == d.RootB {
				published.AlsoOnCluster = true
			} else {
				out = append(out, f)
			}
		}
		if len(d.ImageIDsChanged) > 0 {
			keys := make([]string, 0, len(d.ImageIDsChanged))
			for _, k := range d.ImageIDsChanged {
				keys = append(keys, k.Key)
			}
			out = append(out, newFinding(ImageIDChanged, subject, keys, d.Summary.MaxImpact))
		}
	}
	if published != nil {
		out = append(out, *published)
	}
	return out
}

func publishedFinding(subject string, row docs.VerifyRow) Finding {
	impact := row.MaxImpact
	var v *docs.Verify
	var keys []string
	if row.Diff != nil {
		v = row.Diff.Verify
		keys = changedKeys(*row.Diff)
		if impact == "" {
			impact = row.Diff.Summary.MaxImpact
		}
	}
	kind := PublishedChanged
	switch {
	case v != nil && v.OverwrittenBy != "":
		kind = Overwritten
	case v != nil && v.RenamedTo != "":
		kind = Renamed
	case v != nil && v.Gone:
		kind = Gone
	}
	f := newFinding(kind, subject, nil, impact)
	if kind == PublishedChanged {
		f.Packs = keys
	}
	if v != nil {
		f.Who = append(f.Who, v.Who...)
	}
	return f
}

func newFinding(kind Kind, subject string, packs []string, impact string) Finding {
	if packs == nil {
		packs = []string{}
	}
	return Finding{ID: FindingID(kind, subject), Kind: kind, Profile: subject, Packs: packs, Impact: impact, Who: []docs.Who{}}
}

// changedKeys lists the packs that a diff holds as changed, added, or removed.
func changedKeys(d docs.Diff) []string {
	keys := []string{}
	for _, p := range d.Packs {
		if p.Status != docs.PackUnchanged {
			keys = append(keys, p.Key)
		}
	}
	return keys
}

// ReBaselineNeeded says that the minor version of the strata that wrote a record is lower than the
// minor version of the binary. A new minor version can move a root with no change of content. It is
// the one compare of versions in the observer: `behind` gives the order of pack versions. A version
// that does not parse gives false.
func ReBaselineNeeded(recorded, binary string) bool {
	rMajor, rMinor, ok := majorMinor(recorded)
	if !ok {
		return false
	}
	bMajor, bMinor, ok := majorMinor(binary)
	if !ok {
		return false
	}
	return rMajor < bMajor || (rMajor == bMajor && rMinor < bMinor)
}

func majorMinor(version string) (int, int, bool) {
	parts := strings.SplitN(version, ".", 3)
	if len(parts) < 2 {
		return 0, 0, false
	}
	major, err := strconv.Atoi(parts[0])
	if err != nil {
		return 0, 0, false
	}
	minor, err := strconv.Atoi(parts[1])
	if err != nil {
		return 0, 0, false
	}
	return major, minor, true
}

// stamp gives each finding its times. A finding that the state does not know starts at seen.
func stamp(findings []Finding, firstSeen map[string]time.Time, seen time.Time) {
	for i := range findings {
		findings[i].LastSeen = seen
		findings[i].FirstSeen = seen
		if t, ok := firstSeen[findings[i].ID]; ok {
			findings[i].FirstSeen = t
		}
	}
}
