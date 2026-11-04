package model

import (
	"time"

	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/docs"
)

// Record is one record of the baseline, with the answers that a content cycle got about it.
type Record struct {
	File    string
	Doc     docs.Record
	Row     *docs.VerifyRow
	Cluster ClusterSide
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
}

// Versions is what one good versions cycle read. Moves holds a Move for each pin that is behind,
// by MoveKey.
type Versions struct {
	At     time.Time
	Strata string
	Pins   []docs.Pin
	Moves  map[string]Move
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
}

// Subject names a profile as strata does in pinnedBy: name@version.
func Subject(name, version string) string {
	return name + "@" + version
}

func (r Record) subject() string {
	return Subject(r.Doc.Source.Name, r.Doc.Source.Version)
}

// MoveKey names the move of a pin that is behind: <registryUid>/<name>@<version>-><newest>. It is
// empty for a pin with no newer version.
func MoveKey(p docs.Pin) string {
	if len(p.Newer) == 0 {
		return ""
	}
	return p.RegistryUID + "/" + p.Name + "@" + p.Version + "->" + p.Newer[0].Version
}

// MoveOf reads the counts of a move from the compare of the pinned version and the newest one. The
// changed values are counted, and then dropped.
func MoveOf(d docs.Diff) Move {
	m := Move{Impact: d.Summary.MaxImpact, Flags: []string{}, Images: d.Summary.ImagesChanged, Charts: d.Summary.ChartsChanged}
	if len(d.Packs) == 0 {
		return m
	}
	m.Flags = d.Packs[0].Flags()
	m.Fold = docs.FoldOf(d.Packs[0].Values)
	return m
}
