package check

import (
	"context"
	"slices"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
)

// PackDiff gives the entry of one pack in the diff of one side of a profile, with the changed
// values. It is the one way that a changed value leaves the observer, and only the detail tier of the
// API calls it. The second result is false when the profile, the pack, or the diff of the side
// is not known.
//
// The published side and the cluster side come from the last good content cycle. The newer side
// comes from the move of the last good versions cycle. The state keeps the counts of a move and not
// its values. So after a restart the memory holds no values for a move that the state knows. Then
// the call asks the worker for one `compare` of the two packs, as a cycle does, and keeps the answer
// in memory. It waits for a cycle that holds the worker, and it ends with the context. No call
// sends a POST.
func (o *Observer) PackDiff(ctx context.Context, subject, key string, side model.Side) (docs.Pack, bool, error) {
	if side == model.SideNewer {
		return o.newerPack(ctx, subject, key)
	}
	o.mu.Lock()
	defer o.mu.Unlock()
	if o.content == nil {
		return docs.Pack{}, false, nil
	}
	for _, r := range o.content.Records {
		if model.Subject(r.Doc.Source.Name, r.Doc.Source.Version) != subject {
			continue
		}
		diff := r.Cluster.Diff
		if side == model.SidePublished {
			diff = nil
			if r.Row != nil {
				diff = r.Row.Diff
			}
		}
		pack, ok := packOf(diff, key)
		return pack, ok, nil
	}
	return docs.Pack{}, false, nil
}

// packOf finds the entry of a pack in a diff.
func packOf(d *docs.Diff, key string) (docs.Pack, bool) {
	if d == nil {
		return docs.Pack{}, false
	}
	i := slices.IndexFunc(d.Packs, func(p docs.Pack) bool { return p.Key == key })
	if i < 0 {
		return docs.Pack{}, false
	}
	return d.Packs[i], true
}

// movePack gives the entry of the pack that a move compares. A compare of two packs holds one entry.
func movePack(d docs.Diff) (docs.Pack, bool) {
	if len(d.Packs) == 0 {
		return docs.Pack{}, false
	}
	return d.Packs[0], true
}

// pinOf finds the pin that a profile pins at a pack and that has a newer version. The key is the key of
// the cache of its move.
func (o *Observer) pinOf(subject, key string) (docs.Pin, string, bool) {
	o.mu.Lock()
	defer o.mu.Unlock()
	if o.versions == nil {
		return docs.Pin{}, "", false
	}
	for _, pin := range o.versions.Pins {
		if pin.Layer+"/"+pin.Name != key || !pin.Behind || pin.Error != nil || model.MoveKey(pin) == "" {
			continue
		}
		if slices.ContainsFunc(pin.PinnedBy, func(by docs.PinnedBy) bool { return by.Subject == subject }) {
			return pin, o.versions.Strata + " " + model.MoveKey(pin), true
		}
	}
	return docs.Pin{}, "", false
}

func (o *Observer) newerPack(ctx context.Context, subject, key string) (docs.Pack, bool, error) {
	pin, cacheKey, ok := o.pinOf(subject, key)
	if !ok {
		return docs.Pack{}, false, nil
	}
	if pack, ok := o.cachedDetail(cacheKey); ok {
		return pack, true, nil
	}
	if err := o.acquire(ctx); err != nil {
		return docs.Pack{}, false, err
	}
	defer o.release()
	// A request that waited for the worker can find the answer that another request got.
	if pack, ok := o.cachedDetail(cacheKey); ok {
		return pack, true, nil
	}
	var pack docs.Pack
	var found bool
	err := o.withCycleDir(func(dir string) error {
		diff, err := o.compareMove(ctx, dir, pin)
		if err != nil {
			return err
		}
		pack, found = movePack(diff)
		return nil
	})
	if err != nil || !found {
		return docs.Pack{}, false, err
	}
	o.storeDetail(cacheKey, pack)
	return pack, true, nil
}
