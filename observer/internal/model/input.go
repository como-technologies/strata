package model

import (
	"time"

	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/docs"
)

// Record is one record of the baseline, with the answers that a content cycle got about it.
// RowCarried and SideCarried say that the read of the last cycle failed for that part. Row or
// Cluster.Diff is then the last good answer: see Content.Carry.
type Record struct {
	File                    string
	Doc                     docs.Record
	Row                     *docs.VerifyRow
	Cluster                 ClusterSide
	RowCarried, SideCarried bool
}

// ClusterSide is the compare of a record and the side of the cluster that has the profile of the
// record. Runs is false when the cluster does not run this version of the profile. Runs with no
// Diff means that the cluster side was not read.
type ClusterSide struct {
	Runs bool
	Diff *docs.Diff
}

// Content is what one good content cycle read. At is the end of the cycle.
type Content struct {
	At         time.Time
	Strata     string
	Cluster    docs.ClusterRead
	Meta       baseline.Meta
	Records    []Record
	Unreadable []string
	// Pods is what the read of the pods gave. It is empty when the observer does not read pods.
	Pods PodRead
}

// Versions is what one good versions cycle read. Moves holds a Move for each pin that is behind,
// by MoveKey. Unread holds the MoveKey of each pin that is behind and whose compare failed: its row
// has no counts.
type Versions struct {
	At     time.Time
	Strata string
	Pins   []docs.Pin
	Moves  map[string]Move
	Unread map[string]bool
}

// Clock is the state of one timer.
type Clock struct {
	Every      time.Duration
	Last       *time.Time
	FailedRuns int
	LastError  *ErrorRef
}

// Input is everything that Build reads. Content and Versions are nil until a good cycle ended.
// Files is the number of record files at the last read of the directory, and FilesKnown is false
// before the first read.
type Input struct {
	Now           time.Time
	ClusterUID    string
	StartedAt     time.Time
	Files         int
	FilesKnown    bool
	Content       *Content
	ContentClock  Clock
	Versions      *Versions
	VersionsClock Clock
	FirstSeen     map[string]time.Time
	// ReadPods says that the observer reads pods. FirstDigests holds the first digest of each image,
	// by ImageKey.
	ReadPods     bool
	FirstDigests map[string]string
}

// Subject names a profile as strata does in pinnedBy: name@version.
func Subject(name, version string) string {
	return name + "@" + version
}

func (r Record) subject() string {
	return Subject(r.Doc.Source.Name, r.Doc.Source.Version)
}

// PackKey names the pack of a pin as a record does: <layer>/<name>.
func PackKey(p docs.Pin) string {
	return p.Layer + "/" + p.Name
}

// MoveKey names the move of a pin that is behind: <registryUid>/<name>@<version>-><newest>. It is
// empty for a pin with no newer version.
func MoveKey(p docs.Pin) string {
	if len(p.Newer) == 0 {
		return ""
	}
	return moveKey(p.RegistryUID, p.Name, p.Version, p.Newer[0].Version)
}

// Key names the move of a row in the way that MoveKey names the move of its pin.
func (n Newer) Key() string {
	return moveKey(n.RegistryUID, n.Name, n.Version, n.Newest)
}

func moveKey(registryUID, name, version, newest string) string {
	return registryUID + "/" + name + "@" + version + "->" + newest
}

// MoveOf reads the counts of a move from the compare of the pinned version and the newest one. The
// changed values are counted, and then dropped.
func MoveOf(d docs.Diff) Move {
	m := Move{Impact: d.Summary.MaxImpact, Flags: []string{}, Images: d.Summary.ImagesChanged, Charts: d.Summary.ChartsChanged}
	if len(d.Packs) == 0 {
		return m
	}
	m.Flags = d.Packs[0].Flags()
	m.ImpactText = d.Packs[0].Impact.Text
	m.Fold = FoldOf(d.Packs[0].Values.Paths())
	return m
}
