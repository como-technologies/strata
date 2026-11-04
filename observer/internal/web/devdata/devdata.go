// Package devdata reads the test data of the dashboard: one set of API answers for each state of the
// page. The program `devserver` serves a set, so that a person sees each state with no stand-in and
// no strata binary. A test checks each set against the types of internal/api, so that the data
// cannot drift from the API.
//
// A set is a directory of testdata/api. It holds one file for each route: the answer of the route as
// the API sends it. A set can name another set in a file `base`. A file that the set does not hold
// then comes from the other set. The sets stale, lost, and first-seen use this, because they differ
// from ok in cluster.json only.
package devdata

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"path"
	"regexp"
	"strings"
	"time"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
)

// State names a set of test data.
type State string

// The states of the page that the dev server shows. Quiet and first-seen are states of the data, and
// the others are the values of `page`.
const (
	StateOK        State = "ok"
	StateQuiet     State = "quiet"
	StateFirstLoad State = "first-load"
	StateNoRecords State = "no-records"
	StateStale     State = "stale"
	StateLost      State = "lost"
	StateFirstSeen State = "first-seen"
)

// States lists the sets, in the order that the recipe `observer-dev` names them.
var States = []State{StateOK, StateQuiet, StateFirstLoad, StateNoRecords, StateStale, StateLost, StateFirstSeen}

// Reference is the time that the data is written for. Load moves each time of a set by the
// difference between now and Reference, so that the ages on the page are those of the data.
var Reference = time.Date(2026, 11, 3, 12, 0, 0, 0, time.UTC)

// The files of a set.
const (
	fileCluster    = "cluster.json"
	fileFindings   = "findings.json"
	fileProfiles   = "profiles.json"
	fileNewer      = "newer.json"
	fileNotChecked = "notchecked.json"
	fileEvents     = "events.json"
	fileDiffs      = "diffs.json"
	fileBase       = "base"
)

// Files lists the files that a set holds, or takes from its base.
var Files = []string{fileCluster, fileFindings, fileProfiles, fileNewer, fileNotChecked, fileEvents, fileDiffs}

// Set holds the answer of each route for one state. It is an api.Source.
type Set struct {
	Cluster    api.ClusterView
	Findings   api.FindingsView
	Profiles   api.ProfilesView
	Newer      api.NewerView
	NotChecked api.NotCheckedView
	Events     api.EventsView
	// Diffs holds the answers of the detail route that the set knows.
	Diffs []api.PackDiff
}

// Read gives the content of a file of a state. A file that the set does not hold comes from its base.
func Read(fsys fs.FS, state State, name string) ([]byte, error) {
	raw, err := fs.ReadFile(fsys, path.Join(string(state), name))
	if !errors.Is(err, fs.ErrNotExist) {
		return raw, err
	}
	base, baseErr := fs.ReadFile(fsys, path.Join(string(state), fileBase))
	if baseErr != nil {
		return nil, err
	}
	return fs.ReadFile(fsys, path.Join(strings.TrimSpace(string(base)), name))
}

// timeInJSON finds a time in the form that the API writes it: UTC, to the second.
var timeInJSON = regexp.MustCompile(`"(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z)"`)

// shiftTimes moves each time of a document by d.
func shiftTimes(raw []byte, d time.Duration) []byte {
	return timeInJSON.ReplaceAllFunc(raw, func(m []byte) []byte {
		t, err := time.Parse(time.RFC3339, string(m[1:len(m)-1]))
		if err != nil {
			return m
		}
		return []byte(`"` + t.Add(d).UTC().Format(time.RFC3339) + `"`)
	})
}

// Decode reads a document into v. It refuses a field that v does not have and any data after the
// document, so a set cannot hold a field that the API does not send.
func Decode(raw []byte, v any) error {
	dec := json.NewDecoder(bytes.NewReader(raw))
	dec.DisallowUnknownFields()
	if err := dec.Decode(v); err != nil {
		return err
	}
	if _, err := dec.Token(); !errors.Is(err, io.EOF) {
		return errors.New("data after the document")
	}
	return nil
}

// Load reads the set of a state. With a now that is not zero, it moves each time by now less Reference.
func Load(fsys fs.FS, state State, now time.Time) (*Set, error) {
	var s Set
	for _, f := range []struct {
		name string
		into any
	}{{fileCluster, &s.Cluster}, {fileFindings, &s.Findings}, {fileProfiles, &s.Profiles}, {fileNewer, &s.Newer},
		{fileNotChecked, &s.NotChecked}, {fileEvents, &s.Events}, {fileDiffs, &s.Diffs}} {
		raw, err := Read(fsys, state, f.name)
		if err != nil {
			return nil, fmt.Errorf("state %s: %w", state, err)
		}
		if !now.IsZero() {
			raw = shiftTimes(raw, now.Sub(Reference))
		}
		if err := Decode(raw, f.into); err != nil {
			return nil, fmt.Errorf("state %s, %s: %w", state, f.name, err)
		}
	}
	return &s, nil
}

// Summary joins the answers into the summary that the routes serve parts of.
func (s *Set) Summary() model.Summary {
	profiles := make([]model.Profile, 0, len(s.Profiles.Profiles))
	for _, p := range s.Profiles.Profiles {
		profiles = append(profiles, p.Profile)
	}
	return model.Summary{Cluster: s.Cluster.Cluster, Page: s.Cluster.Page, StrataVersion: s.Cluster.StrataVersion, Records: s.Cluster.Records,
		Content: s.Cluster.Content, Versions: s.Cluster.Versions, Counts: s.Cluster.Counts, Findings: s.Findings.Findings, Profiles: profiles,
		Newer: s.Newer.Newer, NotChecked: s.NotChecked.NotChecked, Limits: s.NotChecked.Limits, Events: s.Events.Events}
}

// PackDiff gives the entry of a pack that the set holds for a side.
func (s *Set) PackDiff(_ context.Context, profile, key string, side model.Side) (docs.Pack, bool, error) {
	for _, d := range s.Diffs {
		if d.Profile == profile && d.Key == key && d.Side == side {
			return packOf(d), true, nil
		}
	}
	return docs.Pack{}, false, nil
}

// packOf turns the answer of the detail route back into the pack that makes it. The three lists of
// values join in one list, and api.PackDiffOf splits it again.
func packOf(d api.PackDiff) docs.Pack {
	p := docs.Pack{Key: d.Key, Status: d.Status, Impact: d.Impact, Manifests: d.Manifests, ParamChanges: d.ParamChanges,
		ImageChanges: d.ImageChanges, ChartChanges: d.ChartChanges, Masked: d.Masked}
	p.Layer, p.Name, _ = strings.Cut(d.Key, "/")
	p.Values.Parseable = d.Values.Parseable
	for _, list := range [][]docs.Leaf{d.Values.Settings, d.Values.FollowsTheVersion, d.Values.RepeatsTheLists} {
		p.Values.Diff = append(p.Values.Diff, list...)
	}
	for _, f := range d.Flags {
		switch f {
		case docs.FlagIdentity:
			p.Identity = true
		case docs.FlagUpstream:
			p.Upstream = true
		case docs.FlagAuthored:
			p.Authored = true
		case docs.FlagImages:
			p.Images = true
		case docs.FlagCharts:
			p.Charts = true
		}
	}
	return p
}
