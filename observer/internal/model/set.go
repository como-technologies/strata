package model

import (
	"cmp"
	"maps"
	"slices"
	"strings"
	"time"

	"github.com/como-technologies/strata/observer/internal/baseline"
)

// ClusterStatus is the state of one cluster of a set on the screen of the clusters.
type ClusterStatus string

// The states of a cluster of a set, from the worst to the best. Drift says that a finding names a published
// version or the pods of the cluster. Differs says that each finding names the copy of the cluster: it runs other
// content than its record. Not checked says that the observer could not check the cluster. It never raises a
// severity. A cause of Not checked limits a check that ended well, so it changes no state.
const (
	RowDrift      = ClusterStatus(StatusDrift)
	RowDiffers    = ClusterStatus(CopyDiffers)
	RowNotChecked = ClusterStatus(StatusNotChecked)
	RowOK         = ClusterStatus(StatusOK)
)

// clusterOrder is the order of the rows of the clusters: the worst state first.
var clusterOrder = []ClusterStatus{RowDrift, RowDiffers, RowNotChecked, RowOK}

// ProjectRef names a project of a set: its uid, and its name when a search gave it.
type ProjectRef struct {
	UID  string `json:"uid"`
	Name string `json:"name,omitempty"`
}

// Project is one project of a set, with a key. LastError is the refusal of the last search of its clusters, or of the
// check of its records. Only the clusters of this project are then not checked.
type Project struct {
	ProjectRef
	LastError *ErrorRef `json:"lastError,omitempty"`
}

// Selector is one selector of the flag --clusters and the number of clusters that it selected at the last search.
// Unread says that a search of the selector failed, so the count is not known.
type Selector struct {
	Selector string `json:"selector"`
	Clusters int    `json:"clusters"`
	Unread   bool   `json:"unread,omitempty"`
}

// ClusterRow is one cluster of a set on the screen of the clusters. Drift counts its findings, Newer its newer
// versions, and NotChecked its causes of Not checked. CheckedAt is the end of its last good check. Here says that the
// observer runs on this cluster, so only this cluster can show the pods.
type ClusterRow struct {
	UID        string        `json:"uid"`
	Name       string        `json:"name"`
	Project    ProjectRef    `json:"project"`
	Profiles   int           `json:"profiles"`
	Status     ClusterStatus `json:"status"`
	Drift      int           `json:"drift"`
	Newer      int           `json:"newer"`
	NotChecked int           `json:"notChecked"`
	CheckedAt  *time.Time    `json:"checkedAt"`
	Page       Page          `json:"page"`
	Here       bool          `json:"here,omitempty"`
	LastError  *ErrorRef     `json:"lastError,omitempty"`
}

// SetCounts holds the numbers of the tiles of the screen of the clusters. They count all the clusters, and the
// clusters with a drift or a copy that differs. They also count the clusters with a newer version, and the clusters
// that are not checked.
type SetCounts struct {
	Clusters   int `json:"clusters"`
	Drift      int `json:"drift"`
	Newer      int `json:"newer"`
	NotChecked int `json:"notChecked"`
	Findings   int `json:"findings"`
}

// SetSummary is the answer of the observer for a set of clusters: the route /v1/clusters, and the output of `once`
// with --clusters. The state of the page is never better than the state of the worst cluster: see SetOf.
type SetSummary struct {
	Page          Page          `json:"page"`
	StrataVersion string        `json:"strataVersion"`
	Records       Records       `json:"records"`
	Content       ContentClock  `json:"content"`
	Versions      VersionsClock `json:"versions"`
	Counts        SetCounts     `json:"counts"`
	Projects      []Project     `json:"projects"`
	Selectors     []Selector    `json:"selectors"`
	Clusters      []ClusterRow  `json:"clusters"`
}

// Member is what the summary of a set reads of one cluster: its summary, its project, and whether the observer runs
// on it. LastError is the refusal of the key of its project, when the last search of the project failed.
type Member struct {
	Summary   Summary
	Project   ProjectRef
	Here      bool
	LastError *ErrorRef
}

// SetInput is everything that SetOf reads. The clocks are the clocks of the cycles of the set.
type SetInput struct {
	Now           time.Time
	StartedAt     time.Time
	Files         int
	FilesKnown    bool
	Meta          baseline.Meta
	Strata        string
	ContentClock  Clock
	VersionsClock Clock
	Members       []Member
	Projects      []Project
	Selectors     []Selector
}

// SetOf builds the summary of a set. A row of each cluster comes first, the worst state first. The page is lost
// when no check of any project ended well. A refusal of one key leaves the page as it is: only the clusters of that
// project are not checked.
func SetOf(in SetInput) SetSummary {
	now := utc(in.Now)
	s := SetSummary{
		Page:          PageOf(in.FilesKnown, in.Files, in.ContentClock, now),
		StrataVersion: in.Strata,
		Records:       Records{Count: in.Files, Baseline: in.Meta.Baseline, Revision: in.Meta.Revision, StartedAt: utc(in.StartedAt)},
		Content:       ContentClock{CheckedAt: utcPtr(in.ContentClock.Last), Health: in.ContentClock.health()},
		Versions:      VersionsClock{ReadAt: utcPtr(in.VersionsClock.Last), Health: in.VersionsClock.health()},
		Projects:      slices.Clone(in.Projects),
		Selectors:     slices.Clone(in.Selectors),
		Clusters:      make([]ClusterRow, 0, len(in.Members)),
	}
	if s.Records.Baseline == "" {
		s.Records.Baseline = baseline.Unknown
	}
	for _, m := range in.Members {
		row := RowOf(m)
		s.Clusters = append(s.Clusters, row)
		s.Counts.Clusters++
		s.Counts.Findings += row.Drift
		switch row.Status {
		case RowDrift, RowDiffers:
			s.Counts.Drift++
		case RowNotChecked:
			s.Counts.NotChecked++
		}
		if row.Newer > 0 {
			s.Counts.Newer++
		}
	}
	slices.SortStableFunc(s.Clusters, func(a, b ClusterRow) int {
		return cmp.Or(slices.Index(clusterOrder, a.Status)-slices.Index(clusterOrder, b.Status), b.Drift-a.Drift,
			strings.Compare(a.Name, b.Name), strings.Compare(a.UID, b.UID))
	})
	if s.Projects == nil {
		s.Projects = []Project{}
	}
	if s.Selectors == nil {
		s.Selectors = []Selector{}
	}
	return s
}

// RowOf gives the row of one cluster of a set. A finding makes the row a drift, or a copy that differs when each
// finding names the copy of the cluster. The cluster is not checked when the key of its project was refused, when
// its last read failed, or when its page is not ok. A cause of Not checked does not change the status: the row
// counts it in NotChecked.
func RowOf(m Member) ClusterRow {
	sum := m.Summary
	row := ClusterRow{UID: sum.Cluster.UID, Name: sum.Cluster.Name, Project: m.Project, Profiles: sum.Counts.Profiles,
		Drift: len(sum.Findings), Newer: sum.Counts.Newer + sum.Counts.NewerProfileVersions, NotChecked: sum.Counts.NotChecked,
		CheckedAt: sum.Content.CheckedAt, Page: sum.Page, Here: m.Here, LastError: m.LastError}
	if row.LastError == nil {
		row.LastError = sum.Content.LastError
	}
	switch {
	case len(sum.Findings) > 0 && slices.ContainsFunc(sum.Findings, func(f Finding) bool { return !slices.Contains(clusterKinds, f.Kind) }):
		row.Status = RowDrift
	case len(sum.Findings) > 0:
		row.Status = RowDiffers
	case m.LastError != nil, sum.Content.LastError != nil, sum.Page != PageOK, sum.Content.CheckedAt == nil:
		row.Status = RowNotChecked
	default:
		row.Status = RowOK
	}
	return row
}

// Shared is one profile version that the clusters of a set run: the screen of the shared versions. Clusters counts
// the clusters that run it, and Projects names the project of each, by uid. Published is the answer of `verify` for its record, and it is
// absent when no record holds the version.
type Shared struct {
	UID       string     `json:"uid"`
	Name      string     `json:"name"`
	Version   string     `json:"version"`
	Scope     string     `json:"scope,omitempty"`
	Clusters  int        `json:"clusters"`
	Projects  []string   `json:"projects"`
	Published *Published `json:"published,omitempty"`
	Copies    Copies     `json:"copies"`
}

// Copies counts the copies of the clusters of one profile version. Same counts the copies that match the record,
// also by content, of Of copies. Differ names each cluster whose copy differs from the record. NotRead counts the
// copies that the last check could not read.
type Copies struct {
	Same    int          `json:"same"`
	Of      int          `json:"of"`
	Differ  []ClusterRef `json:"differ"`
	NotRead int          `json:"notRead"`
}

// ClusterRef names one cluster of a set.
type ClusterRef struct {
	UID  string `json:"uid"`
	Name string `json:"name"`
}

// SharedOf lists each profile version that a cluster of the set runs, by uid, in the order of the name and then of
// the uid. The observer orders no version: the page sorts a column of versions. A profile version that a cluster does not run is not a copy of it. A version with no record has no
// Published, and its copies are not read.
func SharedOf(members []Member) []Shared {
	byUID := map[string]*Shared{}
	projects := map[string]map[string]bool{}
	var order []string
	for _, m := range members {
		for _, p := range m.Summary.Profiles {
			if p.UID == "" || p.Status == StatusNotOnThisCluster {
				continue
			}
			sh, ok := byUID[p.UID]
			if !ok {
				sh = &Shared{UID: p.UID, Name: p.Name, Version: p.Version, Scope: p.Scope, Published: p.Published, Copies: Copies{Differ: []ClusterRef{}}}
				byUID[p.UID], projects[p.UID] = sh, map[string]bool{}
				order = append(order, p.UID)
			}
			sh.Clusters++
			projects[p.UID][m.Project.UID] = true
			sh.Copies.Of++
			switch {
			case p.ClusterCopy == nil, p.ClusterCopy.Status == CopyNotRead:
				sh.Copies.NotRead++
			case p.ClusterCopy.Status == CopySame, p.ClusterCopy.Status == CopySameContent:
				sh.Copies.Same++
			case p.ClusterCopy.Status == CopyDiffers:
				sh.Copies.Differ = append(sh.Copies.Differ, ClusterRef{UID: m.Summary.Cluster.UID, Name: m.Summary.Cluster.Name})
			}
		}
	}
	out := make([]Shared, 0, len(order))
	for _, uid := range order {
		sh := *byUID[uid]
		sh.Projects = slices.Sorted(maps.Keys(projects[uid]))
		out = append(out, sh)
	}
	slices.SortStableFunc(out, func(a, b Shared) int {
		return cmp.Or(strings.Compare(a.Name, b.Name), strings.Compare(a.UID, b.UID))
	})
	return out
}
