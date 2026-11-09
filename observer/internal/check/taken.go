package check

import (
	"context"
	"slices"
	"strings"

	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
	"github.com/como-technologies/strata/observer/internal/seed"
)

// The records that the observer takes itself. The seed writes the first-seen records once, for the
// profiles that the cluster runs at the install, and the observer has no right on Secrets. So while
// meta.json says first-seen, the observer records each profile version that the cluster runs and that
// no record holds, as the seed does, and keeps the record in memory. A restart takes it again. The
// records of the Secrets win: a record there for the same profile version, or records that a pipeline
// approved, drop what the observer took.

// keptFirstSeen gives the records that the observer took, for the profile versions that set does not
// hold. It drops each one that set holds now, and all of them when set is not first-seen.
func (o *Observer) keptFirstSeen(set baseline.Set) []baseline.Entry {
	o.mu.Lock()
	defer o.mu.Unlock()
	if set.Meta.Baseline != baseline.FirstSeen {
		clear(o.taken)
		return nil
	}
	held := subjectsOf(set.Entries)
	var out []baseline.Entry
	for subject, e := range o.taken {
		if held[subject] {
			delete(o.taken, subject)
			continue
		}
		out = append(out, e)
	}
	slices.SortFunc(out, func(a, b baseline.Entry) int { return strings.Compare(a.File, b.File) })
	return out
}

// recordNewProfiles records each profile version that the cluster runs and that no record holds, while
// set is first-seen, and starts the next content cycle at once to check it. It drops a record that
// it took for a profile version that the cluster no longer runs. A snapshot that fails leaves that
// profile with no record, and the next cycle tries again: the content cycle itself stays good.
func (o *Observer) recordNewProfiles(ctx context.Context, dir string, set baseline.Set, read docs.ClusterRead) {
	if set.Meta.Baseline != baseline.FirstSeen {
		return
	}
	held := subjectsOf(set.Entries)
	runs := map[string]bool{}
	var added int
	for _, p := range read.Source.Cluster.Profiles {
		subject := model.Subject(p.Name, p.Version)
		runs[subject] = true
		if held[subject] {
			continue
		}
		rec, err := seed.Snapshot(ctx, poolStrata{o}, dir, p.Name+"@"+p.Version)
		if err != nil {
			ref := errorRefOf(err)
			o.log.Info("first-seen record", "verdict", "failed", "kind", ref.Kind, "status", ref.Status)
			continue
		}
		e, err := baseline.EntryOf(rec.Key, rec.JSON)
		if err != nil {
			o.log.Info("first-seen record", "verdict", "failed", "kind", docs.KindSchema)
			continue
		}
		o.mu.Lock()
		o.taken[subject] = e
		o.mu.Unlock()
		added++
	}
	o.mu.Lock()
	for subject := range o.taken {
		if !runs[subject] {
			delete(o.taken, subject)
		}
	}
	o.mu.Unlock()
	if added > 0 {
		o.log.Info("first-seen record", "verdict", "taken", "records", added)
		o.kick(o.kicks[cycleContent])
	}
}

// subjectsOf gives the profile version, name@version, of each record.
func subjectsOf(entries []baseline.Entry) map[string]bool {
	out := make(map[string]bool, len(entries))
	for _, e := range entries {
		out[model.Subject(e.Record.Source.Name, e.Record.Source.Version)] = true
	}
	return out
}

// poolStrata runs a command of strata through the pool of processes of the observer, as a write: only
// exit 0 is an answer.
type poolStrata struct{ o *Observer }

func (p poolStrata) Run(ctx context.Context, dir string, args ...string) (runner.Result, error) {
	return p.o.run(ctx, dir, exitsOfAWrite, args...)
}
