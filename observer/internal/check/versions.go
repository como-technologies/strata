package check

import (
	"context"
	"slices"

	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
)

// versionsCycle runs one versions cycle: ask `behind` for the pins of the records, and compare each
// pinned version with the newest one. files is -1 when the directory was not read. With no record,
// it runs no `behind`, because the command needs a file.
func (o *Observer) versionsCycle(ctx context.Context) (versions model.Versions, files int, err error) {
	set, err := baseline.Load(o.cfg.Records)
	if err != nil {
		return versions, -1, failure(docs.KindIO)
	}
	if len(set.Entries) == 0 {
		versions.At = o.now()
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
	args := append([]string{"behind"}, names...)
	args = append(args, "--no-impact", "--json", "--compact", "-q")
	res, err := o.run(ctx, dir, exitsWithDocument, args...)
	if err != nil {
		return err
	}
	doc, err := decode[docs.Behind](res.Stdout)
	if err != nil {
		return err
	}
	// The kind of a failed cycle comes from pins[].error, because a pin that fails is a row.
	if len(doc.Pins) > 0 && !slices.ContainsFunc(doc.Pins, func(p docs.Pin) bool { return p.Error == nil }) {
		e := doc.Pins[0].Error
		return &runner.RunError{Kind: e.Kind, Status: e.Status}
	}
	versions.Strata, versions.Pins = doc.Tool.Version, doc.Pins
	versions.Moves, err = o.moves(ctx, dir, doc)
	versions.At = o.now()
	return err
}

// moves compares the pinned version of each pin that is behind with its newest version. A move that
// the cache knows costs no request. A compare that fails is not cached, and it fails the cycle: the
// next cycle asks again for that move only.
func (o *Observer) moves(ctx context.Context, dir string, doc docs.Behind) (map[string]model.Move, error) {
	moves := map[string]model.Move{}
	used := map[string]bool{}
	for _, pin := range doc.Pins {
		key := model.MoveKey(pin)
		if !pin.Behind || key == "" || pin.Error != nil {
			continue
		}
		cacheKey := doc.Tool.Version + " " + key
		move, ok := o.cachedMove(cacheKey)
		if !ok {
			var err error
			if move, err = o.compareMove(ctx, dir, pin); err != nil {
				return moves, err
			}
			o.storeMove(cacheKey, move)
		}
		moves[key], used[cacheKey] = move, true
	}
	o.pruneMoves(used)
	return moves, nil
}

func (o *Observer) compareMove(ctx context.Context, dir string, pin docs.Pin) (model.Move, error) {
	ref := "pack:" + pin.RegistryUID + "/" + pin.Name + "@"
	res, err := o.run(ctx, dir, exitsOfACompare, "compare", ref+pin.Version, ref+pin.Newer[0].Version, "--json", "--compact", "-q")
	if err != nil {
		return model.Move{}, err
	}
	diff, err := decode[docs.Diff](res.Stdout)
	if err != nil {
		return model.Move{}, err
	}
	// The counts stay. The changed values go out of scope here, and the state never holds them.
	return model.MoveOf(diff), nil
}
