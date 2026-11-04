package model

import "github.com/como-technologies/strata/observer/internal/docs"

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

// HeldIDs gives the ids of the findings that this content cannot clear. These are the findings that
// each part of a record could give when the cycle could not read that part. A failed read of the pods
// holds the image findings of each record in the same way. The caller keeps the first sight of each, also
// when no finding exists, because no answer is there to say that the drift is gone.
func HeldIDs(c Content) []string {
	var ids []string
	for _, r := range c.Records {
		subject := r.subject()
		if r.rowUnread() {
			for _, kind := range publishedKinds {
				ids = append(ids, FindingID(kind, subject))
			}
		}
		if r.sideUnread() {
			for _, kind := range clusterKinds {
				ids = append(ids, FindingID(kind, subject))
			}
		}
		if c.Pods.Err != nil {
			for _, kind := range imageKinds {
				ids = append(ids, FindingID(kind, subject))
			}
		}
	}
	return ids
}
