package check

import (
	"cmp"
	"context"
	"maps"
	"slices"

	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
)

// RunVersions runs one versions cycle of the set: `behind` one time for each key, on the records of its project.
// The cycle takes the versions slot for one project at a time, so a click "Ask Palette" waits for one project. A
// cycle fails only when the read of each project failed.
func (s *Set) RunVersions(ctx context.Context) error {
	err := s.runCycle(ctx, cycleVersions, s.versionsTurn, &s.state.Versions,
		func(ctx context.Context) (int, error) { return s.versionsCycle(ctx) },
		func() {
			at := s.now()
			s.versionsAt = &at
		})
	s.giveVersions()
	return err
}

func (s *Set) versionsCycle(ctx context.Context) (int, error) {
	keys, err := readKeys(s.scfg.Keys, s.scfg.Home)
	if err != nil {
		s.log.Error("the keys cannot be read", "kind", docs.KindIO)
		return -1, failure(docs.KindIO)
	}
	s.setKeys(keys)
	records, err := baseline.Load(s.cfg.Records)
	if err != nil {
		return -1, failure(docs.KindIO)
	}
	used := map[string]bool{}
	var good int
	var first error
	for _, k := range keys {
		entries := s.entriesOf(records, k.Project)
		if len(entries) == 0 {
			continue
		}
		v, err := s.projectVersions(ctx, k, baseline.Set{Meta: records.Meta, Entries: entries}, used)
		if ctx.Err() != nil {
			return records.Files(), ctx.Err()
		}
		s.mu.Lock()
		p := s.project(k.Project)
		if err != nil {
			ref := errorRefOf(err)
			p.clock.FailedRuns++
			p.clock.LastError = &ref
			first = cmp.Or(first, err)
		} else {
			p.versions, p.clock = v, ClockState{}
			good++
		}
		s.mu.Unlock()
	}
	if first == nil {
		s.pruneMoves(used)
	}
	if good == 0 && first != nil {
		return records.Files(), first
	}
	return records.Files(), nil
}

// projectVersions reads the versions of the records of one project with its key, in the versions slot.
func (s *Set) projectVersions(ctx context.Context, k runner.Key, set baseline.Set, used map[string]bool) (*model.Versions, error) {
	if err := s.acquire(ctx, s.versionsSlot); err != nil {
		return nil, err
	}
	defer s.release(s.versionsSlot)
	var v model.Versions
	err := s.withCycleDir(func(dir string) error {
		return s.readVersions(runner.WithKey(ctx, k), dir, set, &v, used)
	})
	return &v, err
}

// giveVersions gives each member the versions of the records that its cluster runs, from the last good read of each
// of their projects. A member with no good content yet runs no record that the set knows, so it gets none. The
// pack of each move goes with the versions, for the detail tier of the member.
func (s *Set) giveVersions() {
	for _, m := range s.memberList() {
		subjects := m.obs.runSubjects(s.projectOf)
		s.mu.Lock()
		byProject := map[string]*model.Versions{}
		clock := ClockState{}
		for p := range subjects {
			if sp, ok := s.projects[p]; ok {
				byProject[p] = sp.versions
				if sp.clock.FailedRuns > clock.FailedRuns {
					clock = sp.clock
				}
			}
		}
		v := versionsFor(byProject, subjects)
		details := map[string]docs.Pack{}
		if v != nil {
			for _, pin := range v.Pins {
				key := v.Strata + " " + model.MoveKey(pin)
				if p, ok := s.details[key]; ok {
					details[key] = p
				}
			}
		}
		s.mu.Unlock()
		m.obs.keepVersions(v, clock, details)
	}
}

// runSubjects gives the subjects of the records that the cluster of a member runs, name@version, by the project
// whose key checks each one. projectOf gives that project.
func (o *Observer) runSubjects(projectOf func(docs.Record) string) map[string]map[string]bool {
	o.mu.Lock()
	defer o.mu.Unlock()
	out := map[string]map[string]bool{}
	if o.content == nil {
		return out
	}
	for _, r := range o.content.Records {
		if !r.Cluster.Runs {
			continue
		}
		p := projectOf(r.Doc)
		if out[p] == nil {
			out[p] = map[string]bool{}
		}
		out[p][model.Subject(r.Doc.Source.Name, r.Doc.Source.Version)] = true
	}
	return out
}

// versionsFor gives the versions of the records of one cluster: the pins and the profile versions that its records
// hold, from the versions of the project of each record. byProject holds the last good versions of each project, and
// subjects the subjects of the records of the cluster in each project. A pin of two projects is one pin, as one
// `behind` of all the records gives it: see mergeBehind. It gives nil when no project has versions.
func versionsFor(byProject map[string]*model.Versions, subjects map[string]map[string]bool) *model.Versions {
	out := &model.Versions{Moves: map[string]model.Move{}, Unread: map[string]bool{}, ProfileMoves: map[string]docs.Diff{}, ProfileUnread: map[string]bool{}}
	var answers []docs.Behind
	for _, p := range slices.Sorted(maps.Keys(subjects)) {
		v := byProject[p]
		if v == nil {
			continue
		}
		keep := subjects[p]
		held := func(by []docs.PinnedBy) []docs.PinnedBy {
			return slices.DeleteFunc(slices.Clone(by), func(b docs.PinnedBy) bool { return !keep[b.Subject] })
		}
		answer := docs.Behind{Schema: docs.BehindSchema, Tool: docs.Tool{Version: v.Strata}}
		for _, pin := range v.Pins {
			if pin.PinnedBy = held(pin.PinnedBy); len(pin.PinnedBy) == 0 {
				continue
			}
			pin.Carriers = slices.DeleteFunc(slices.Clone(pin.Carriers), func(c docs.PinCarrier) bool { return !keep[model.Subject(c.Profile, c.Version)] })
			answer.Pins = append(answer.Pins, pin)
			if key := model.MoveKey(pin); key != "" {
				if m, ok := v.Moves[key]; ok {
					out.Moves[key] = m
				}
				if v.Unread[key] {
					out.Unread[key] = true
				}
			}
		}
		for _, pp := range v.Profiles {
			if pp.PinnedBy = held(pp.PinnedBy); len(pp.PinnedBy) == 0 {
				continue
			}
			answer.Profiles = append(answer.Profiles, pp)
			subject := model.Subject(pp.Name, pp.Version)
			if d, ok := v.ProfileMoves[subject]; ok {
				out.ProfileMoves[subject] = d
			}
			if v.ProfileUnread[subject] {
				out.ProfileUnread[subject] = true
			}
		}
		answers = append(answers, answer)
		if v.At.After(out.At) {
			out.At = v.At
		}
		if out.Strata == "" {
			out.Strata = v.Strata
		}
	}
	if len(answers) == 0 {
		return nil
	}
	merged := mergeBehind(answers)
	out.Pins, out.Profiles = merged.Pins, merged.Profiles
	return out
}
