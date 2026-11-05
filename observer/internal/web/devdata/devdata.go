// Package devdata reads the test data of the dashboard: one set of API answers for each state of the
// page. The program `devserver` serves a set, so that a person sees each state with no stand-in and
// no strata binary. A test checks each set against the types of internal/api, so that the data
// cannot drift from the API.
//
// A set is a directory of testdata/api. It holds one file for each route: the answer of the route as
// the API sends it. A set can name another set in a file `base`. A file that the set does not hold
// then comes from the other set. The sets stale, lost, and first-seen use this, because they differ
// from ok in cluster.json only. The sets stale-unchecked and lost-unchecked differ from first-load in
// cluster.json only.
//
// The file images.json is the document of the pods, strata-observed/1. The set ok has pods that the
// observer reads. The sets quiet, first-load, no-records, and read-error have pods that it does not read.
// The set images has one image that moved, and images-split has one image with two digests. Each of the
// two holds the finding of the image, its event, and its profile with the finding. The set images-off is
// ok with the read of the pods off. The set images-unread is ok with a read of the pods that never ended
// well, and it holds the cause pods-not-read.
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

// The states of the page that the dev server shows. Quiet, first-seen, and read-error are states of the
// data, and the others are the values of `page`. The sets stale-unchecked and lost-unchecked are a page
// that no content check has ended well for, as after a restart in an outage.
const (
	StateOK        State = "ok"
	StateQuiet     State = "quiet"
	StateFirstLoad State = "first-load"
	StateNoRecords State = "no-records"
	StateStale     State = "stale"
	StateLost      State = "lost"
	StateFirstSeen State = "first-seen"

	StateStaleUnchecked State = "stale-unchecked"
	StateLostUnchecked  State = "lost-unchecked"
	StateReadError      State = "read-error"

	// The states of the pods. Images and images-split hold a finding of an image. Images-off and
	// images-unread hold none: the observer does not read pods, or it could not read them.
	StateImages       State = "images"
	StateImagesSplit  State = "images-split"
	StateImagesOff    State = "images-off"
	StateImagesUnread State = "images-unread"
)

// States lists the sets, in the order that the recipe `observer-dev` names them.
var States = []State{StateOK, StateQuiet, StateFirstLoad, StateNoRecords, StateStale, StateLost, StateFirstSeen,
	StateStaleUnchecked, StateLostUnchecked, StateReadError, StateImages, StateImagesSplit, StateImagesOff, StateImagesUnread}

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
	fileImages     = "images.json"
	fileAsks       = "asks.json"
	fileBase       = "base"
)

// Files lists the files that a set holds, or takes from its base.
var Files = []string{fileCluster, fileFindings, fileProfiles, fileNewer, fileNotChecked, fileEvents, fileDiffs, fileImages, fileAsks}

// Set holds the answer of each route for one state. It is an api.Source.
type Set struct {
	Cluster    api.ClusterView
	Findings   api.FindingsView
	Profiles   api.ProfilesView
	Newer      api.NewerView
	NotChecked api.NotCheckedView
	Events     api.EventsView
	// Images is the document of the pods, strata-observed/1.
	Images model.Observed
	// Diffs holds the answers of the detail route that the set knows.
	Diffs []api.PackDiff
	// Asks holds the answers of Palette that the set has given. A click on a move that it holds gives the
	// answer again, and a click on another move finds no move.
	Asks api.AsksView
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
		{fileNotChecked, &s.NotChecked}, {fileEvents, &s.Events}, {fileDiffs, &s.Diffs}, {fileImages, &s.Images}, {fileAsks, &s.Asks}} {
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

// Observed gives the document of the pods.
func (s *Set) Observed() model.Observed {
	return s.Images
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
	p.Values.Parseable, p.Values.Unified = d.Values.Parseable, d.Values.Unified
	p.TextTooLarge = d.TextTooLarge
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

// Ask gives the answer that the set holds for a move. It sends no request.
func (s *Set) Ask(_ context.Context, profile, key string) (model.Answer, bool, error) {
	for _, a := range s.Asks.Asks {
		if a.Profile == profile && a.Key == key {
			return a, true, nil
		}
	}
	return model.Answer{}, false, nil
}

// Answers gives the answers that the set holds.
func (s *Set) Answers() []model.Answer {
	return s.Asks.Asks
}
