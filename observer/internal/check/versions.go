package check

import (
	"context"
	"slices"
	"sync"

	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
)

// versionsCycle runs one versions cycle. It asks `behind` for the pins of the records and compares
// each pinned version with the newest one. files is -1 when the directory was not read. With no record,
// it runs no `behind`, because the command needs a file. It then reads no version, so At stays zero.
func (o *Observer) versionsCycle(ctx context.Context) (versions model.Versions, files int, err error) {
	set, err := baseline.Load(o.cfg.Records)
	if err != nil {
		return versions, -1, failure(docs.KindIO)
	}
	if len(set.Entries) == 0 {
		return versions, set.Files(), nil
	}
	err = o.withCycleDir(func(dir string) error {
		return o.readVersions(ctx, dir, set, &versions)
	})
	return versions, set.Files(), err
}

func (o *Observer) readVersions(ctx context.Context, dir string, set baseline.Set, versions *model.Versions) error {
	names, err := writeRecords(dir, set.Entries)
	if err != nil {
		return err
	}
	o.expect(ctx, 1) // behind
	args := append([]string{runner.CmdBehind}, names...)
	args = append(args, runner.FlagNoImpact, "--json", "--compact", "-q")
	res, err := o.run(ctx, dir, exitsWithDocument, args...)
	if err != nil {
		return err
	}
	doc, err := decode[docs.Behind](res.Stdout, docs.BehindSchema)
	if err != nil {
		return err
	}
	// The kind of a failed cycle comes from pins[].error, because a pin that fails is a row.
	if len(doc.Pins) > 0 && !slices.ContainsFunc(doc.Pins, func(p docs.Pin) bool { return p.Error == nil }) {
		e := doc.Pins[0].Error
		return &runner.RunError{Kind: e.Kind, Status: e.Status}
	}
	versions.Strata, versions.Pins = doc.Tool.Version, doc.Pins
	versions.Moves, versions.Unread, err = o.moves(ctx, dir, doc)
	versions.At = o.now()
	return err
}

// moves compares the pinned version of each pin that is behind with its newest version. A move that
// the cache knows costs no request. The other moves compare at the same time, as many as the pool of
// processes lets. A compare that fails is one row of the list with no counts: it is in the second
// result, it is not cached, and the next cycle asks again for that move only. The other pins stay in
// the cycle. Only a stop of ctx ends the cycle, and then the error is the one of ctx.
//
// Each move of the cycle counts as used, also when its compare fails: the answer of Palette about a
// move lives as long as the move, and a failed compare must not drop it.
func (o *Observer) moves(ctx context.Context, dir string, doc docs.Behind) (map[string]model.Move, map[string]bool, error) {
	moves, unread := map[string]model.Move{}, map[string]bool{}
	used := map[string]bool{}
	type job struct {
		pin           docs.Pin
		key, cacheKey string
		diff          docs.Diff
		err           error
	}
	var jobs []*job
	for _, pin := range doc.Pins {
		key := model.MoveKey(pin)
		if !pin.Behind || key == "" || pin.Error != nil {
			continue
		}
		cacheKey := doc.Tool.Version + " " + key
		used[cacheKey] = true
		if move, ok := o.cachedMove(cacheKey); ok {
			moves[key] = move
			continue
		}
		jobs = append(jobs, &job{pin: pin, key: key, cacheKey: cacheKey})
	}
	o.expect(ctx, len(jobs))
	var wg sync.WaitGroup
	for _, j := range jobs {
		wg.Add(1)
		go func() {
			defer wg.Done()
			j.diff, j.err = o.compareMove(ctx, dir, j.pin)
		}()
	}
	wg.Wait()
	if ctx.Err() != nil {
		return moves, unread, ctx.Err()
	}
	for _, j := range jobs {
		if j.err != nil {
			unread[j.key] = true
			continue
		}
		// The counts go to the state. The pack, with its changed values, stays in memory.
		move := model.MoveOf(j.diff)
		o.storeMove(j.cacheKey, move)
		if pack, ok := movePack(j.diff); ok {
			o.storeDetail(j.cacheKey, pack)
		}
		moves[j.key] = move
	}
	o.pruneMoves(used)
	return moves, unread, nil
}

// compareMove compares the pinned version of a pin with its newest version. The caller keeps the counts
// and the changed values apart.
func (o *Observer) compareMove(ctx context.Context, dir string, pin docs.Pin) (docs.Diff, error) {
	ref := "pack:" + pin.RegistryUID + "/" + pin.Name + "@"
	res, err := o.run(ctx, dir, exitsOfACompare, runner.CmdCompare, ref+pin.Version, ref+pin.Newer[0].Version, runner.FlagFull, "--json", "--compact", "-q")
	if err != nil {
		return docs.Diff{}, err
	}
	diff, err := decode[docs.Diff](res.Stdout, docs.DiffSchema)
	diff.CapText(maxText)
	return diff, err
}
