package model

import (
	"slices"
	"strings"

	"github.com/como-technologies/strata/observer/internal/docs"
)

// hasRow says that the record has a `verify` row that strata could read.
func (r Record) hasRow() bool {
	return r.Row != nil && r.Row.Status != docs.VerifyError
}

// hasSide says that the record has the answer of the compare with the cluster side.
func (r Record) hasSide() bool {
	return r.Cluster.Runs && r.Cluster.Diff != nil
}

// rowUnread says that the last cycle could not read the `verify` row of the record. Its answer, if
// any, comes from an earlier cycle.
func (r Record) rowUnread() bool {
	return r.RowCarried || !r.hasRow()
}

// sideUnread says that the last cycle could not read the cluster side of a record that the cluster
// runs. Its answer, if any, comes from an earlier cycle.
func (r Record) sideUnread() bool {
	return r.SideCarried || r.Cluster.Runs && r.Cluster.Diff == nil
}

// readError says that the last cycle could not read a part of the record.
func (r Record) readError() bool {
	return r.rowUnread() || r.sideUnread()
}

// Carry puts the last good answer into each part of a record whose read failed in this cycle. A
// failed read tells nothing about the drift, so a finding stays until a read that works clears it.
// The record keeps RowCarried or SideCarried, and the page lists it in "Not checked". The read of the
// pods follows the same rule. When it failed, the content keeps the containers of the last good read,
// with the time of that read.
//
// A part gets no answer in three cases. The last cycle held no good answer for it. The last cycle did
// not hold the record. The record changed since then.
//
// A file that the observer cannot read gives no record in this cycle. When the last cycle read a
// record from that file, this record replaces it, with both parts carried. A file with no last record
// is blind: see Content.Blind.
func (c *Content) Carry(last *Content) {
	if last == nil {
		return
	}
	if c.Pods.Err != nil && !c.Pods.read() && last.Pods.read() {
		c.Pods.Containers, c.Pods.At = last.Pods.Containers, last.Pods.At
	}
	for i := range c.Records {
		r := &c.Records[i]
		prev, ok := last.recordOf(r.subject())
		if !ok || prev.Doc.Root != r.Doc.Root {
			continue
		}
		if !r.hasRow() && prev.hasRow() {
			r.Row, r.RowCarried = prev.Row, true
		}
		if r.Cluster.Runs && r.Cluster.Diff == nil && prev.hasSide() {
			r.Cluster.Diff, r.SideCarried = prev.Cluster.Diff, true
		}
	}
	c.carryUnreadable(last)
}

// carryUnreadable puts the last record of each file in Unreadable in place of the file. A profile
// that another file gives in this cycle is not carried a second time. The records keep the order of
// the files, as the directory lists them.
func (c *Content) carryUnreadable(last *Content) {
	carried := false
	for _, file := range c.Unreadable {
		prev, ok := last.recordOfFile(file)
		if !ok {
			continue
		}
		if _, given := c.recordOf(prev.subject()); given {
			continue
		}
		prev.RowCarried, prev.SideCarried = true, true
		c.Records = append(c.Records, prev)
		carried = true
	}
	if carried {
		slices.SortStableFunc(c.Records, func(a, b Record) int { return strings.Compare(a.File, b.File) })
	}
}

// Blind says that a file cannot be read and that no record replaces it. The file can hold the profile
// of any finding that the other records do not give, so the content cannot clear those: see BlindIDs.
func (c Content) Blind() bool {
	return slices.ContainsFunc(c.Unreadable, func(file string) bool {
		_, ok := c.recordOfFile(file)
		return !ok
	})
}

// recordOf finds the record of a profile.
func (c Content) recordOf(subject string) (Record, bool) {
	for _, r := range c.Records {
		if r.subject() == subject {
			return r, true
		}
	}
	return Record{}, false
}

// recordOfFile finds the record that a file gave.
func (c Content) recordOfFile(file string) (Record, bool) {
	for _, r := range c.Records {
		if r.File == file {
			return r, true
		}
	}
	return Record{}, false
}

// idsOf gives the ids of the findings that a profile can have, for the kinds of the lists.
func idsOf(subject string, lists ...[]Kind) []string {
	var ids []string
	for _, kinds := range lists {
		for _, kind := range kinds {
			ids = append(ids, FindingID(kind, subject))
		}
	}
	return ids
}

// HeldIDs gives the ids of the findings that this content cannot clear. These are the findings that
// each part of a record could give when the cycle could not read that part. A failed read of the pods
// holds the image findings of each record in the same way. The caller keeps the first sight of each, also
// when no finding exists, because no answer is there to say that the drift is gone.
func HeldIDs(c Content) []string {
	var ids []string
	for _, r := range c.Records {
		subject := r.subject()
		if r.rowUnread() {
			ids = append(ids, idsOf(subject, publishedKinds)...)
		}
		if r.sideUnread() {
			ids = append(ids, idsOf(subject, clusterKinds)...)
		}
		if c.Pods.Err != nil {
			ids = append(ids, idsOf(subject, imageKinds)...)
		}
	}
	return ids
}

// BlindIDs gives the ids in known that a blind content cannot clear. known holds the ids of the
// findings that earlier cycles saw. A record that the content holds gives an answer for each kind of
// its profile, so its ids are not here. Any other id can belong to the file that cannot be read. A
// content that is not blind holds none.
func BlindIDs(c Content, known []string) []string {
	if !c.Blind() {
		return nil
	}
	decided := map[string]bool{}
	for _, r := range c.Records {
		for _, id := range idsOf(r.subject(), publishedKinds, clusterKinds, imageKinds) {
			decided[id] = true
		}
	}
	var ids []string
	for _, id := range known {
		if !decided[id] {
			ids = append(ids, id)
		}
	}
	return ids
}
