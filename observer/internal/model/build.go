package model

import (
	"slices"
	"strings"
	"time"

	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/docs"
)

// newWindow is the time that a version counts as new after its listing. It is also the window of
// the list of events.
const newWindow = 7 * 24 * time.Hour

// Build joins the last good content cycle and the last good versions cycle into the summary. The
// page state is computed here, at each call, from the clocks and the time. It writes no value of
// a variable and no old or new value, because it reads none: see the types of docs.
func Build(in Input) Summary {
	now := utc(in.Now)
	s := Summary{
		Page:     PageOf(in.FilesKnown, in.Files, in.ContentClock, now),
		Content:  ContentClock{CheckedAt: utcPtr(in.ContentClock.Last), Health: in.ContentClock.health()},
		Versions: VersionsClock{ReadAt: utcPtr(in.VersionsClock.Last), Health: in.VersionsClock.health()},
		Records:  Records{StartedAt: utc(in.StartedAt)},
		Findings: []Finding{}, Profiles: []Profile{}, Newer: []Newer{}, NotChecked: []NotChecked{}, Events: []Event{},
	}
	s.Cluster.UID = in.ClusterUID
	s.Records.Baseline = baseline.Unknown
	c := newCauses()
	c.fromContent(in.Content)
	c.fromVersions(in.Versions)
	v := newView(in, c)
	if in.Content != nil {
		s.fromContent(in, v)
	}
	if in.Versions != nil {
		s.StrataVersion = in.Versions.Strata
		s.Newer = newerRows(in.Versions, v, now)
	}
	if in.Content != nil && in.Content.Strata != "" {
		s.StrataVersion = in.Content.Strata
	}
	s.NotChecked = c.list()
	s.Events = eventsOf(v.findings, in.Versions, now)
	s.Counts = s.counts(in, v)
	s.Counts.Newer = len(s.Newer)
	s.Counts.NewerProfiles = distinctProfiles(s.Newer)
	for _, p := range s.Profiles {
		if p.NewerVersion != nil {
			s.Counts.NewerProfileVersions++
		}
	}
	return s
}

func (c Clock) health() Health {
	return Health{Every: ShortDuration(c.Every), FailedRuns: c.FailedRuns, LastError: c.LastError, Running: c.Running}
}

// view holds what the rows need from the two cycles, so that a row does not search for it.
type view struct {
	causes *causes
	// registries maps the uid of a registry to its name.
	registries map[string]string
	// newest maps a pack of a profile to the newest version that a pin names.
	newest map[string]string
	// newerProfiles holds the profiles that a pin that is behind names.
	newerProfiles map[string]bool
	// newerVersions maps a profile to the newest published version of its profile, when it is newer.
	newerVersions map[string]NewerVersion
	// findingProfiles and findingPacks hold the profiles and the packs that a finding names.
	// packlessProfiles holds the profiles with a finding that names no pack, such as overwritten.
	findingProfiles  map[string]bool
	findingPacks     map[string]bool
	packlessProfiles map[string]bool
	findings         []Finding
}

func newView(in Input, c *causes) *view {
	v := &view{causes: c, registries: map[string]string{}, newest: map[string]string{}, newerProfiles: map[string]bool{},
		newerVersions: map[string]NewerVersion{}, findingProfiles: map[string]bool{}, findingPacks: map[string]bool{}, packlessProfiles: map[string]bool{}}
	if in.Versions != nil {
		v.readNewerVersions(in.Versions, utc(in.Now))
		for _, pin := range in.Versions.Pins {
			if pin.Registry != "" {
				v.registries[pin.RegistryUID] = pin.Registry
			}
			if !pin.Behind || len(pin.Newer) == 0 {
				continue
			}
			for _, by := range pin.PinnedBy {
				v.newest[packID(by.Subject, PackKey(pin))] = pin.Newer[0].Version
				v.newerProfiles[by.Subject] = true
			}
		}
	}
	if in.Content != nil {
		v.findings = FindingsOf(*in.Content, in.FirstDigests)
		stamp(v.findings, in.FirstSeen, utc(in.Content.At))
		for _, f := range v.findings {
			// A finding of the pods is not a drift of the content: it changes no status of a profile or a pack.
			if f.Kind.IsImage() {
				continue
			}
			v.findingProfiles[f.Profile] = true
			if len(f.Packs) == 0 {
				v.packlessProfiles[f.Profile] = true
			}
			for _, key := range f.Packs {
				v.findingPacks[packID(f.Profile, key)] = true
			}
		}
	}
	return v
}

// readNewerVersions keeps the newest published version of each profile that is behind, with the move
// to it. A profile version that strata could not check has none: the page then says nothing about a
// newer version.
func (v *view) readNewerVersions(vs *Versions, now time.Time) {
	for _, p := range vs.Profiles {
		newest, ok := NewestProfile(p)
		if !ok {
			continue
		}
		n := NewerVersion{Version: newest.Version, Created: newest.Created, New: listedWithin(newest.Created, now)}
		subject := Subject(p.Name, p.Version)
		if d, ok := vs.ProfileMoves[subject]; ok {
			move := ProfileMoveOf(d)
			n.Move = &move
		}
		n.Unread = vs.ProfileUnread[subject]
		for _, by := range p.PinnedBy {
			v.newerVersions[by.Subject] = n
		}
	}
}

func (v *view) registry(uid string) string {
	if name := v.registries[uid]; name != "" {
		return name
	}
	return uid
}

// fromContent fills the parts of the summary that only a content cycle gives.
func (s *Summary) fromContent(in Input, v *view) {
	c := in.Content
	cluster := c.Cluster.Source.Cluster
	s.Cluster.Name = cluster.Name
	if cluster.UID != "" {
		s.Cluster.UID = cluster.UID
	}
	s.Records.Count = len(c.Records)
	s.Records.Baseline = c.Meta.Baseline
	s.Records.Revision = c.Meta.Revision
	s.Records.FirstSeenFrom = c.Meta.FirstSeenFrom
	s.Findings = v.findings
	if len(v.findings) > 0 {
		updates, repave := cluster.UpdatesPending, cluster.RepavePending
		s.Cluster.UpdatesPending, s.Cluster.RepavePending = &updates, &repave
	}
	for _, r := range c.Records {
		s.Profiles = append(s.Profiles, profileRow(r, v))
	}
	for _, p := range noRecordProfiles(c) {
		s.Profiles = append(s.Profiles, Profile{Name: p.Name, Version: p.Version, Type: p.Type, Status: StatusNoRecord, Packs: []Pack{}})
	}
	s.Limits = limitsOf(c)
}

func profileRow(r Record, v *view) Profile {
	subject := r.subject()
	p := Profile{Name: r.Doc.Source.Name, Version: r.Doc.Source.Version, Type: r.Doc.Source.Type, Scope: ScopeTenant, Packs: []Pack{}}
	if r.Doc.Source.ProjectUID != "" {
		p.Scope = ScopeProject
	}
	if r.Row != nil {
		p.Published = &Published{Status: r.Row.Status, RootRecorded: r.Row.RootRecorded, RootLive: r.Row.RootLive}
	}
	p.ClusterCopy = copyOf(r.Cluster)
	if newer, ok := v.newerVersions[subject]; ok {
		p.NewerVersion = &newer
	}
	for _, rp := range r.Doc.Packs {
		p.Packs = append(p.Packs, packRow(r, rp, v))
	}
	switch {
	case v.findingProfiles[subject]:
		p.Status = StatusDrift
	// a newer version of a pack, or of the profile itself: the page gives each its own words
	case v.newerProfiles[subject], p.NewerVersion != nil:
		p.Status = StatusNewer
	case v.causes.profiles[subject]:
		p.Status = StatusNotChecked
	default:
		p.Status = StatusOK
	}
	return p
}

func copyOf(side ClusterSide) *ClusterCopy {
	switch {
	case !side.Runs:
		return &ClusterCopy{Status: CopyNotOnThisCluster}
	case side.Diff == nil:
		return &ClusterCopy{Status: CopyNotRead}
	case side.Diff.Same:
		return &ClusterCopy{Status: CopySame, RootB: side.Diff.RootB}
	case side.Diff.SameContent:
		return &ClusterCopy{Status: CopySameContent, RootB: side.Diff.RootB}
	}
	return &ClusterCopy{Status: CopyDiffers, RootB: side.Diff.RootB}
}

func packRow(r Record, rp docs.RecordPack, v *view) Pack {
	id := packID(r.subject(), rp.Key)
	p := Pack{Key: rp.Key, Name: rp.Name, Layer: rp.Layer, Version: rp.Version, Registry: v.registry(rp.RegistryUID), Kind: rp.RegistryProviderType,
		Upstream: rp.Hash.UpstreamRaw, UpstreamSource: rp.Hash.UpstreamSource, Images: len(rp.Images), Newer: v.newest[id]}
	if p.Kind == "" {
		p.Kind = rp.Type
	}
	p.Flags, p.Fold = changesOf(r, rp.Key)
	switch {
	// One finding names the profile and no pack. A pack that has changes then belongs to it.
	case v.findingPacks[id], v.packlessProfiles[r.subject()] && p.Fold != nil:
		p.Status = StatusDrift
	case p.Newer != "":
		p.Status = StatusNewer
	case v.causes.packs[id]:
		p.Status = StatusNotChecked
	default:
		p.Status = StatusOK
	}
	return p
}

// changesOf joins the flags of a pack and folds its changed values, from the two diffs of a record.
// They are the `verify` diff and the `compare` with the cluster. The fold counts the joined paths, so
// a path that the two diffs share counts once. A path can hold the name of a list item, which is a
// value of the cluster, so the paths stay here and only their count leaves.
func changesOf(r Record, key string) ([]string, *Fold) {
	flags, paths := []string{}, []string{}
	var diffs []*docs.Diff
	if r.Row != nil {
		diffs = append(diffs, r.Row.Diff)
	}
	diffs = append(diffs, r.Cluster.Diff)
	for _, d := range diffs {
		if d == nil {
			continue
		}
		for _, p := range d.Packs {
			if p.Key != key || p.Status == docs.PackUnchanged {
				continue
			}
			for _, f := range p.Flags() {
				if !slices.Contains(flags, f) {
					flags = append(flags, f)
				}
			}
			paths = append(paths, p.Values.Paths()...)
		}
	}
	if len(flags) == 0 && len(paths) == 0 {
		return flags, nil
	}
	slices.Sort(paths)
	fold := FoldOf(slices.Compact(paths))
	return flags, &fold
}

// newerRows gives one row for each pin that is behind, in the order of the pins.
func newerRows(vs *Versions, v *view, now time.Time) []Newer {
	rows := []Newer{}
	for _, pin := range vs.Pins {
		if !pin.Behind || len(pin.Newer) == 0 {
			continue
		}
		newest := pin.Newer[0]
		move, ok := vs.Moves[MoveKey(pin)]
		if !ok || move.Flags == nil {
			move.Flags = []string{}
		}
		drops := newest.Drops
		if drops == nil {
			drops = []string{}
		}
		rows = append(rows, Newer{Name: pin.Name, Registry: v.registry(pin.RegistryUID), RegistryUID: pin.RegistryUID, Version: pin.Version,
			Newest: newest.Version, Listed: newest.Listed, New: listedWithin(newest.Listed, now), Floating: pin.Floating,
			PinnedState: pin.PinnedState, Drops: drops, Profiles: pinProfiles(pin.PinnedBy), Steps: stepsOf(pin), Move: move})
	}
	return rows
}

// stepsOf lists the newer versions of a pin from the oldest to the newest. Strata gives them newest first.
func stepsOf(pin docs.Pin) []Step {
	steps := make([]Step, 0, len(pin.Newer))
	for i := len(pin.Newer) - 1; i >= 0; i-- {
		steps = append(steps, Step{Version: pin.Newer[i].Version, Listed: pin.Newer[i].Listed})
	}
	return steps
}

// listedWithin says that a version came in the last 7 days. A time that does not parse is not new.
func listedWithin(listed string, now time.Time) bool {
	t, ok := parseTime(listed)
	return ok && within(t, now)
}

func distinctProfiles(rows []Newer) int {
	seen := map[string]bool{}
	for _, r := range rows {
		for _, p := range r.Profiles {
			seen[p] = true
		}
	}
	return len(seen)
}

// counts reads the numbers of the page from the rows. A pack counts for the records that the
// cluster runs, so a record of another cluster adds none.
func (s Summary) counts(in Input, v *view) Counts {
	n := Counts{Findings: len(s.Findings), NotChecked: len(s.NotChecked), Profiles: len(s.Profiles)}
	for _, p := range s.Profiles {
		if p.Status == StatusDrift {
			n.Drift++
		}
	}
	if in.Content == nil {
		return n
	}
	for _, r := range in.Content.Records {
		if !r.Cluster.Runs {
			continue
		}
		for _, p := range r.Doc.Packs {
			n.Packs++
			if !v.causes.packs[packID(r.subject(), p.Key)] {
				n.PacksInFull++
			}
		}
	}
	return n
}

// utc gives a time in UTC, to the second, as the page shows it.
func utc(t time.Time) time.Time {
	return t.UTC().Truncate(time.Second)
}

func utcPtr(t *time.Time) *time.Time {
	if t == nil {
		return nil
	}
	u := utc(*t)
	return &u
}

// ShortDuration writes 15m0s as 15m and 6h0m0s as 6h.
func ShortDuration(d time.Duration) string {
	s := d.String()
	if strings.HasSuffix(s, "m0s") {
		s = strings.TrimSuffix(s, "0s")
	}
	if strings.HasSuffix(s, "h0m") {
		s = strings.TrimSuffix(s, "0m")
	}
	return s
}
