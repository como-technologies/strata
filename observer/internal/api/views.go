package api

import (
	"slices"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
)

// The views of the summary tier. Each one is a part of model.Summary, with the same field names, and
// none of them holds a value. A test walks the types and fails when one holds a docs.Leaf.

// ClusterView is the answer of /v1/cluster: the fields of the summary up to the counts.
type ClusterView struct {
	Cluster       model.Cluster       `json:"cluster"`
	Page          model.Page          `json:"page"`
	StrataVersion string              `json:"strataVersion"`
	Records       model.Records       `json:"records"`
	Content       model.ContentClock  `json:"content"`
	Versions      model.VersionsClock `json:"versions"`
	Counts        model.Counts        `json:"counts"`
}

// ClusterOf takes the fields of /v1/cluster from the summary.
func ClusterOf(s model.Summary) ClusterView {
	return ClusterView{Cluster: s.Cluster, Page: s.Page, StrataVersion: s.StrataVersion, Records: s.Records,
		Content: s.Content, Versions: s.Versions, Counts: s.Counts}
}

// FindingsView is the answer of /v1/findings.
type FindingsView struct {
	Findings []model.Finding `json:"findings"`
}

// ProfileView is a profile with its findings and the writes that the audit log holds for them.
type ProfileView struct {
	model.Profile
	Findings []model.Finding `json:"findings"`
	Who      []docs.Who      `json:"who"`
}

// ProfilesView is the answer of /v1/profiles.
type ProfilesView struct {
	Profiles []ProfileView `json:"profiles"`
}

// ProfilesOf takes the profiles of the summary, each with its findings and its who.
func ProfilesOf(s model.Summary) ProfilesView {
	out := ProfilesView{Profiles: make([]ProfileView, 0, len(s.Profiles))}
	for _, p := range s.Profiles {
		out.Profiles = append(out.Profiles, profileView(p, s.Findings))
	}
	return out
}

func profileView(p model.Profile, findings []model.Finding) ProfileView {
	subject := model.Subject(p.Name, p.Version)
	v := ProfileView{Profile: p, Findings: []model.Finding{}, Who: []docs.Who{}}
	for _, f := range findings {
		if f.Profile == subject {
			v.Findings = append(v.Findings, f)
			v.Who = append(v.Who, f.Who...)
		}
	}
	return v
}

// findProfile finds a profile by name@version. A name with no version finds the profile when only one
// profile has the name.
func findProfile(profiles []model.Profile, word string) (model.Profile, bool) {
	if i := slices.IndexFunc(profiles, func(p model.Profile) bool { return model.Subject(p.Name, p.Version) == word }); i >= 0 {
		return profiles[i], true
	}
	var found []model.Profile
	for _, p := range profiles {
		if p.Name == word {
			found = append(found, p)
		}
	}
	if len(found) == 1 {
		return found[0], true
	}
	return model.Profile{}, false
}

// NewerView is the answer of /v1/newer. Each row holds the counts of its move, and no value.
type NewerView struct {
	Newer []model.Newer `json:"newer"`
}

// NotCheckedView is the answer of /v1/notchecked: the causes and the limits.
type NotCheckedView struct {
	NotChecked []model.NotChecked `json:"notChecked"`
	Limits     model.Limits       `json:"limits"`
}

// EventsView is the answer of /v1/events: the events of the last 7 days.
type EventsView struct {
	Events []model.Event `json:"events"`
}

// PackDiff is the answer of the detail route. It is the only type of the API that holds docs.Leaf.
type PackDiff struct {
	Profile      string          `json:"profile"`
	Key          string          `json:"key"`
	Side         model.Side      `json:"side"`
	Status       string          `json:"status"`
	Impact       docs.Impact     `json:"impact"`
	Flags        []string        `json:"flags"`
	Values       ValueLists      `json:"values"`
	ImageChanges []docs.Change   `json:"imageChanges"`
	ChartChanges []docs.Change   `json:"chartChanges"`
	Masked       []string        `json:"masked"`
	Manifests    []docs.Manifest `json:"manifests"`
	ParamChanges []docs.Named    `json:"paramChanges"`
	// Build lists each field of the identity of the pack, and its upstream digest, that differs.
	Build []BuildChange `json:"build"`
	// TextTooLarge says that the pack has text, and the observer dropped it: the text is above the cap.
	TextTooLarge bool `json:"textTooLarge"`
}

// ValueLists holds each changed value in the list of its group. The groups are those of the fold that
// the summary counts.
type ValueLists struct {
	Parseable         bool        `json:"parseable"`
	Unified           string      `json:"unified"`
	Settings          []docs.Leaf `json:"settings"`
	FollowsTheVersion []docs.Leaf `json:"followsTheVersion"`
	RepeatsTheLists   []docs.Leaf `json:"repeatsTheLists"`
}

// PackDiffOf makes the answer of the detail route. A list that the diff does not hold is empty and
// not null.
func PackDiffOf(profile, key string, side model.Side, p docs.Pack) PackDiff {
	settings, follows, repeats := model.Split(p.Values)
	return PackDiff{
		Profile: profile, Key: key, Side: side, Status: p.Status, Impact: p.Impact, Flags: p.Flags(),
		Values:       ValueLists{Parseable: p.Values.Parseable, Unified: p.Values.Unified, Settings: settings, FollowsTheVersion: follows, RepeatsTheLists: repeats},
		ImageChanges: nonNil(p.ImageChanges), ChartChanges: nonNil(p.ChartChanges), Masked: nonNil(p.Masked),
		Manifests: nonNil(p.Manifests), ParamChanges: nonNil(p.ParamChanges), Build: buildOf(p), TextTooLarge: p.TextTooLarge,
	}
}

// BuildChange is one field of the identity of a pack, or its upstream digest, with its value on each side.
type BuildChange struct {
	Field string `json:"field"`
	A     string `json:"a"`
	B     string `json:"b"`
}

// The fields of the build, in the order that the page shows them.
const (
	BuildVersion  = "version"
	BuildTag      = "tag"
	BuildPackUID  = "packUid"
	BuildUpstream = "upstream"
)

// buildOf lists the fields of the build that differ between the two sides. A pack with one side, an
// added or a removed pack, gives an empty list.
func buildOf(p docs.Pack) []BuildChange {
	out := []BuildChange{}
	if p.A == nil || p.B == nil {
		return out
	}
	for _, f := range []BuildChange{
		{BuildVersion, p.A.Version, p.B.Version}, {BuildTag, p.A.Tag, p.B.Tag},
		{BuildPackUID, p.A.PackUID, p.B.PackUID}, {BuildUpstream, p.A.Upstream, p.B.Upstream},
	} {
		if f.A != f.B {
			out = append(out, f)
		}
	}
	return out
}

func nonNil[T any](s []T) []T {
	if s == nil {
		return []T{}
	}
	return s
}

// AsksView is the answer of /v1/asks: the answer of Palette to each question that a person asked since
// the observer started. It is a detail route, because a refusal holds the message of Palette.
type AsksView struct {
	Asks []model.Answer `json:"asks"`
}
