package model

import (
	"slices"
	"strings"

	"github.com/como-technologies/strata/observer/internal/docs"
)

// causes collects the section "Not checked". A cause groups by a name of its own: a pack, a
// registry, a profile, a record, or a pin. It never raises a severity.
type causes struct {
	rows  map[string]*NotChecked
	order []string
	// profiles and packs say which profile and which pack a cause names. A pack is "subject|key".
	profiles map[string]bool
	packs    map[string]bool
}

func newCauses() *causes {
	return &causes{rows: map[string]*NotChecked{}, profiles: map[string]bool{}, packs: map[string]bool{}}
}

// packID names a pack of a profile.
func packID(subject, key string) string {
	return subject + "|" + key
}

// add puts a profile and some packs under the cause of a group. It makes the row at the first call.
func (c *causes) add(cause Cause, group, registry, subject string, keys ...string) {
	id := string(cause) + "\n" + group
	row, ok := c.rows[id]
	if !ok {
		row = &NotChecked{Cause: cause, Profiles: []string{}, Packs: []string{}, Registry: registry}
		c.rows[id] = row
		c.order = append(c.order, id)
	}
	if !slices.Contains(row.Profiles, subject) {
		row.Profiles = append(row.Profiles, subject)
	}
	c.profiles[subject] = true
	for _, k := range keys {
		if !slices.Contains(row.Packs, k) {
			row.Packs = append(row.Packs, k)
		}
		c.packs[packID(subject, k)] = true
	}
}

// list gives the rows in the order of the table of causes, and then of the group.
func (c *causes) list() []NotChecked {
	slices.SortFunc(c.order, func(a, b string) int {
		if d := rank(c.rows[a].Cause) - rank(c.rows[b].Cause); d != 0 {
			return d
		}
		return strings.Compare(a, b)
	})
	out := make([]NotChecked, 0, len(c.order))
	for _, id := range c.order {
		row := *c.rows[id]
		slices.Sort(row.Profiles)
		slices.Sort(row.Packs)
		out = append(out, row)
	}
	return out
}

func rank(c Cause) int {
	return slices.Index(causeOrder, c)
}

// fromContent adds the causes that a content cycle shows. The packs of a record are all named when a
// read of the cycle failed for it, because none of them was checked in full. A read that failed is a
// cause also when the record holds the last good answer of that part.
func (c *causes) fromContent(content *Content) {
	if content == nil {
		return
	}
	for _, name := range content.Unreadable {
		c.add(ReadError, name, "", name)
	}
	for _, r := range content.Records {
		subject := r.subject()
		if r.readError() {
			c.add(ReadError, subject, "", subject, recordKeys(r)...)
			continue
		}
		keys := unknownKeys(r)
		// A record that is unverified and names no pack still shows its profile.
		if r.Row.Status == docs.VerifyUnverified && len(keys) == 0 {
			c.add(NoUpstreamDigest, "", "", subject)
		}
		for _, key := range keys {
			c.add(NoUpstreamDigest, key, "", subject, key)
		}
	}
	for _, p := range noRecordProfiles(content) {
		subject := Subject(p.Name, p.Version)
		c.add(NoRecord, subject, "", subject)
	}
}

// fromVersions adds the causes that a versions cycle shows. A pin whose move was not read is a
// pin-error too: the page knows that a newer version exists, and it cannot count what the move changes.
func (c *causes) fromVersions(v *Versions) {
	if v == nil {
		return
	}
	for _, pin := range v.Pins {
		key := PackKey(pin)
		registry := registryName(pin)
		if pin.Error != nil || v.Unread[MoveKey(pin)] {
			group := pin.RegistryUID + "/" + key + "@" + pin.Version
			for _, by := range pin.PinnedBy {
				c.add(PinError, group, "", by.Subject, key)
			}
		}
		if pin.Error == nil && pin.RegistrySync != nil {
			for _, by := range pin.PinnedBy {
				c.add(OlderRegistryCopy, registry, registry, by.Subject, key)
			}
		}
	}
}

func registryName(pin docs.Pin) string {
	if pin.Registry != "" {
		return pin.Registry
	}
	return pin.RegistryUID
}

func recordKeys(r Record) []string {
	keys := make([]string, 0, len(r.Doc.Packs))
	for _, p := range r.Doc.Packs {
		keys = append(keys, p.Key)
	}
	return keys
}

// unknownKeys lists the packs whose upstream digest strata could not read, from the diff of the
// row. A row of another status has none.
func unknownKeys(r Record) []string {
	if r.Row == nil || r.Row.Diff == nil {
		return nil
	}
	d := r.Row.Diff
	keys := append([]string{}, d.UpstreamUnknown...)
	if d.Verify != nil {
		keys = append(keys, d.Verify.UpstreamUnknown...)
	}
	for _, p := range d.Packs {
		if p.UpstreamUnknown {
			keys = append(keys, p.Key)
		}
	}
	slices.Sort(keys)
	return slices.Compact(keys)
}

// noRecordProfiles lists the profiles that the cluster runs and that no record holds.
func noRecordProfiles(c *Content) []docs.ClusterProfile {
	var out []docs.ClusterProfile
	for _, p := range c.Cluster.Source.Cluster.Profiles {
		subject := Subject(p.Name, p.Version)
		if !slices.ContainsFunc(c.Records, func(r Record) bool { return r.subject() == subject }) {
			out = append(out, p)
		}
	}
	return out
}
