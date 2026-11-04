// Package model holds the summary of the observer, and the rules that build it from the documents
// of strata.
//
// The summary is the one type that the observer shows. It holds no old value, no new value, and no
// value of a variable, and its types do not use docs.Leaf. A finding names a profile, a pack, and a
// path, and never what the path holds.
package model

import (
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
)

// Summary is the answer of the observer. `once` prints it, and each summary route serves a part.
type Summary struct {
	Cluster       Cluster       `json:"cluster"`
	Page          Page          `json:"page"`
	StrataVersion string        `json:"strataVersion"`
	Records       Records       `json:"records"`
	Content       ContentClock  `json:"content"`
	Versions      VersionsClock `json:"versions"`
	Counts        Counts        `json:"counts"`
	Findings      []Finding     `json:"findings"`
	Profiles      []Profile     `json:"profiles"`
	Newer         []Newer       `json:"newer"`
	NotChecked    []NotChecked  `json:"notChecked"`
	Limits        Limits        `json:"limits"`
	Events        []Event       `json:"events"`
}

// Cluster names the cluster. The flags of Palette appear only when a finding exists: Palette shows
// them on its own page, and the observer does not repeat a status that Palette shows.
type Cluster struct {
	UID            string `json:"uid"`
	Name           string `json:"name"`
	UpdatesPending *bool  `json:"updatesPending,omitempty"`
	RepavePending  *bool  `json:"repavePending,omitempty"`
}

// Records says what the observer compares with. StartedAt is the first start with this state.
type Records struct {
	Count         int       `json:"count"`
	Baseline      string    `json:"baseline"`
	Revision      string    `json:"revision,omitempty"`
	FirstSeenFrom string    `json:"firstSeenFrom,omitempty"`
	StartedAt     time.Time `json:"startedAt"`
}

// ErrorRef is the kind and the status of a failed cycle, and never a message.
type ErrorRef struct {
	Kind   string `json:"kind"`
	Status int    `json:"status,omitempty"`
}

// Health is the state of one timer.
type Health struct {
	Every      string    `json:"every"`
	FailedRuns int       `json:"failedRuns"`
	LastError  *ErrorRef `json:"lastError"`
}

// ContentClock is the timer of the content cycle. CheckedAt is the end of the last good cycle.
type ContentClock struct {
	CheckedAt *time.Time `json:"checkedAt"`
	Health
}

// VersionsClock is the timer of the versions cycle. ReadAt is the end of the last good cycle.
type VersionsClock struct {
	ReadAt *time.Time `json:"readAt"`
	Health
}

// Counts holds the numbers that the page shows at the top.
type Counts struct {
	Findings      int `json:"findings"`
	Drift         int `json:"drift"`
	Newer         int `json:"newer"`
	NewerProfiles int `json:"newerProfiles"`
	NotChecked    int `json:"notChecked"`
	Packs         int `json:"packs"`
	PacksInFull   int `json:"packsInFull"`
	Profiles      int `json:"profiles"`
}

// Finding is one cause of drift in one profile.
type Finding struct {
	ID               string     `json:"id"`
	Kind             Kind       `json:"kind"`
	Profile          string     `json:"profile"`
	Packs            []string   `json:"packs"`
	Impact           string     `json:"impact"`
	AlsoOnCluster    bool       `json:"alsoOnCluster"`
	ReBaselineNeeded bool       `json:"reBaselineNeeded"`
	FirstSeen        time.Time  `json:"firstSeen"`
	LastSeen         time.Time  `json:"lastSeen"`
	Who              []docs.Who `json:"who"`
}

// Profile is one profile of the cluster or of the baseline.
type Profile struct {
	Name        string       `json:"name"`
	Version     string       `json:"version"`
	Type        string       `json:"type"`
	Scope       string       `json:"scope,omitempty"`
	Status      Status       `json:"status"`
	Published   *Published   `json:"published,omitempty"`
	ClusterCopy *ClusterCopy `json:"clusterCopy,omitempty"`
	Packs       []Pack       `json:"packs"`
}

// Published is the answer of `verify` for the record.
type Published struct {
	Status       string `json:"status"`
	RootRecorded string `json:"rootRecorded,omitempty"`
	RootLive     string `json:"rootLive,omitempty"`
}

// ClusterCopy is the answer of the compare of the record and the cluster side.
type ClusterCopy struct {
	Status CopyStatus `json:"status"`
	RootB  string     `json:"rootB,omitempty"`
}

// Pack is one pack of a record.
type Pack struct {
	Key            string     `json:"key"`
	Name           string     `json:"name"`
	Layer          string     `json:"layer"`
	Version        string     `json:"version"`
	Status         Status     `json:"status"`
	Registry       string     `json:"registry"`
	Kind           string     `json:"kind"`
	Upstream       string     `json:"upstream"`
	UpstreamSource string     `json:"upstreamSource"`
	Images         int        `json:"images"`
	Newer          string     `json:"newer,omitempty"`
	Flags          []string   `json:"flags"`
	ChangedPaths   []string   `json:"changedPaths"`
	Fold           *docs.Fold `json:"fold,omitempty"`
}

// Move is what the observer keeps of the comparison of a pinned version and the newest version. The
// counts go to the state. The changed values never do.
type Move struct {
	Impact string    `json:"impact"`
	Flags  []string  `json:"flags"`
	Fold   docs.Fold `json:"fold"`
	Images int       `json:"images"`
	Charts int       `json:"charts"`
}

// Newer is one pack that a newer version exists for.
type Newer struct {
	Name        string   `json:"name"`
	Registry    string   `json:"registry"`
	RegistryUID string   `json:"registryUid"`
	Version     string   `json:"version"`
	Newest      string   `json:"newest"`
	Listed      string   `json:"listed"`
	New         bool     `json:"new"`
	Floating    bool     `json:"floating"`
	PinnedState string   `json:"pinnedState"`
	Drops       []string `json:"drops"`
	Profiles    []string `json:"profiles"`
	Move
}

// NotChecked is one cause that a check did not cover some profiles or packs.
type NotChecked struct {
	Cause    Cause    `json:"cause"`
	Profiles []string `json:"profiles"`
	Packs    []string `json:"packs"`
	Registry string   `json:"registry"`
}

// Limits counts what a check does not cover.
type Limits struct {
	ImagesNamedByATag int `json:"imagesNamedByATag"`
	HelmPacks         int `json:"helmPacks"`
	MaskedPaths       int `json:"maskedPaths"`
}

// Event is one line for the list of events, in the way of an event of Kubernetes.
type Event struct {
	Type    string    `json:"type"`
	Reason  string    `json:"reason"`
	Time    time.Time `json:"time"`
	Object  string    `json:"object"`
	Message string    `json:"message"`
}
