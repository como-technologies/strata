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
//
// The file diffs.json holds the answers of the detail route, with the unified text of the values and of
// the manifests. The set text-too-large is ok with two packs whose text the observer dropped.
//
// The file asks.json holds the answers of Palette that the observer holds. The set asks is ok with one
// answer of each verdict. The file replies.json is optional: it holds the answer that a click on a move
// gives when the set holds none for it. A move with no reply gives "not found", as a move that the
// observer does not know. The set ok holds a reply for each move of its Newer table but one.
//
// The file alerts.json is optional: it holds the answer of /v1/alerts. A set that lacks it, and whose base lacks
// it, is a page of an observer that started without --alerts. The set alerts is ok with rules, and the set
// alerts-unset is ok with that answer written out.
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
	"slices"
	"strings"
	"sync"
	"time"

	"github.com/como-technologies/strata/observer/internal/alert"
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

	// The states of the viewer. Asks holds one answer of Palette of each verdict. Text-too-large holds
	// two packs whose text the observer dropped, so the page names the command for it.
	StateAsks         State = "asks"
	StateTextTooLarge State = "text-too-large"

	// The states of the alerts. Alerts is ok with rules in each state. Alerts-unset is ok with the observer
	// started without --alerts.
	StateAlerts      State = "alerts"
	StateAlertsUnset State = "alerts-unset"
)

// States lists the sets, in the order that the recipe `observer-dev` names them.
var States = []State{StateOK, StateQuiet, StateFirstLoad, StateNoRecords, StateStale, StateLost, StateFirstSeen,
	StateStaleUnchecked, StateLostUnchecked, StateReadError, StateImages, StateImagesSplit, StateImagesOff, StateImagesUnread,
	StateAsks, StateTextTooLarge, StateAlerts, StateAlertsUnset}

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
	fileReplies    = "replies.json"
	fileAlerts     = "alerts.json"
	fileBase       = "base"
)

// Files lists the files that a set holds, or takes from its base. The files replies.json and alerts.json are
// not in the list, because a set can lack them.
var Files = []string{fileCluster, fileFindings, fileProfiles, fileNewer, fileNotChecked, fileEvents, fileDiffs, fileImages, fileAsks}

// Set holds the answer of each route for one state. It is an api.Source. A click on a move changes
// the answers that the set holds, so the methods that read or change them take the lock.
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
	// answer again.
	Asks api.AsksView
	// Replies holds the answer that a click gives for a move that Asks does not hold.
	Replies api.AsksView
	// AlertRules is the answer of /v1/alerts.
	AlertRules alert.View

	mu sync.Mutex
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
	s := Set{AlertRules: alert.Unset()}
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
	raw, err := Read(fsys, state, fileReplies)
	switch {
	case errors.Is(err, fs.ErrNotExist):
	case err != nil:
		return nil, fmt.Errorf("state %s: %w", state, err)
	default:
		if err := Decode(raw, &s.Replies); err != nil {
			return nil, fmt.Errorf("state %s, %s: %w", state, fileReplies, err)
		}
	}
	raw, err = Read(fsys, state, fileAlerts)
	switch {
	case errors.Is(err, fs.ErrNotExist):
	case err != nil:
		return nil, fmt.Errorf("state %s: %w", state, err)
	default:
		if !now.IsZero() {
			raw = shiftTimes(raw, now.Sub(Reference))
		}
		if err := Decode(raw, &s.AlertRules); err != nil {
			return nil, fmt.Errorf("state %s, %s: %w", state, fileAlerts, err)
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
	// Each field of the build that differs gives its value to the two sides. A field that the list does not
	// hold is the same on both sides, so api.PackDiffOf drops it again.
	if len(d.Build) > 0 {
		p.A, p.B = &docs.PackSide{}, &docs.PackSide{}
		for _, b := range d.Build {
			switch b.Field {
			case api.BuildVersion:
				p.A.Version, p.B.Version = b.A, b.B
			case api.BuildTag:
				p.A.Tag, p.B.Tag = b.A, b.B
			case api.BuildPackUID:
				p.A.PackUID, p.B.PackUID = b.A, b.B
			case api.BuildUpstream:
				p.A.Upstream, p.B.Upstream = b.A, b.B
			}
		}
	}
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

// Ask gives the answer for a move, as the observer does when a person clicks. It sends no request. A
// move with an answer of Palette gets that answer again. A move with no answer, or with a request that
// failed, gets its reply, and the set keeps the reply as the answer of the move. A move with no reply
// is not found, and the second result is false.
func (s *Set) Ask(_ context.Context, profile, key string) (model.Answer, bool, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	is := func(a model.Answer) bool { return a.Profile == profile && a.Key == key }
	held := slices.IndexFunc(s.Asks.Asks, is)
	if held >= 0 && s.Asks.Asks[held].Answered() {
		return s.Asks.Asks[held], true, nil
	}
	i := slices.IndexFunc(s.Replies.Asks, is)
	switch {
	case i >= 0 && held >= 0:
		s.Asks.Asks[held] = s.Replies.Asks[i]
	case i >= 0:
		s.Asks.Asks = append(s.Asks.Asks, s.Replies.Asks[i])
	case held >= 0:
		return s.Asks.Asks[held], true, nil
	default:
		return model.Answer{}, false, nil
	}
	return s.Replies.Asks[i], true, nil
}

// Alerts gives the rules of the alerts that the set holds.
func (s *Set) Alerts() alert.View {
	return s.AlertRules
}

// StartCycle answers the click "Check now" as an observer with a free loop does: the cycle starts. The
// set runs no cycle, so its summary does not change.
func (s *Set) StartCycle(name string) (model.Start, bool) {
	if name != "content" && name != "versions" {
		return model.Start{}, false
	}
	return model.Start{Cycle: name, State: model.StartStarted}, true
}

// Answers gives the answers that the set holds.
func (s *Set) Answers() []model.Answer {
	s.mu.Lock()
	defer s.mu.Unlock()
	return slices.Clone(s.Asks.Asks)
}
