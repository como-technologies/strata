package check

import (
	"cmp"
	"context"
	"slices"
	"strings"
	"sync"

	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
)

// versionsCycle runs one versions cycle. It asks `behind` for the pins of each record and compares
// each pinned version with the newest one. It also compares the record of each profile version that is
// behind with the newest version of its profile. files is -1 when the directory was not read. With no record,
// it runs no `behind`, because the command needs a file. It then reads no version, so At stays zero.
func (o *Observer) versionsCycle(ctx context.Context) (versions model.Versions, files int, err error) {
	set, err := baseline.Load(o.cfg.Records)
	if err != nil {
		return versions, -1, failure(docs.KindIO)
	}
	files = set.Files()
	set.Entries = append(set.Entries, o.keptFirstSeen(set)...)
	if len(set.Entries) == 0 {
		return versions, files, nil
	}
	err = o.withCycleDir(func(dir string) error {
		return o.readVersions(ctx, dir, set, &versions)
	})
	return versions, files, err
}

func (o *Observer) readVersions(ctx context.Context, dir string, set baseline.Set, versions *model.Versions) error {
	names, err := writeRecords(dir, set.Entries)
	if err != nil {
		return err
	}
	doc, err := o.behind(ctx, dir, names)
	if err != nil {
		return err
	}
	// The kind of a failed cycle comes from pins[].error, because a pin that fails is a row.
	if len(doc.Pins) > 0 && !slices.ContainsFunc(doc.Pins, func(p docs.Pin) bool { return p.Error == nil }) {
		e := doc.Pins[0].Error
		return &runner.RunError{Kind: e.Kind, Status: e.Status}
	}
	versions.Strata, versions.Pins, versions.Profiles = doc.Tool.Version, doc.Pins, doc.Profiles
	versions.Moves, versions.Unread, err = o.moves(ctx, dir, doc)
	if err == nil {
		versions.ProfileMoves, versions.ProfileUnread, err = o.profileMoves(ctx, dir, recordFiles(set.Entries, names), doc.Profiles)
	}
	versions.At = o.now()
	return err
}

// recordFiles maps the subject of each record to the file of the record in the directory of the
// cycle. names holds the file of each entry, in order. A subject keeps the file of its first record.
func recordFiles(entries []baseline.Entry, names []string) map[string]string {
	files := map[string]string{}
	for i, e := range entries {
		subject := model.Subject(e.Record.Source.Name, e.Record.Source.Version)
		if _, ok := files[subject]; !ok {
			files[subject] = names[i]
		}
	}
	return files
}

// behind runs `behind` on each record, as many at once as the pool of processes lets. So the progress
// of the cycle counts the records, and the reads of the registries of many records overlap. A record
// that fails stops the cycle with its error, as one `behind` of all the records did. mergeBehind joins
// the answers.
func (o *Observer) behind(ctx context.Context, dir string, names []string) (docs.Behind, error) {
	o.expect(ctx, len(names))
	answers := make([]docs.Behind, len(names))
	errs := make([]error, len(names))
	var wg sync.WaitGroup
	for i, name := range names {
		wg.Add(1)
		go func() {
			defer wg.Done()
			res, err := o.run(ctx, dir, exitsWithDocument, runner.CmdBehind, name, runner.FlagNoImpact, "--json", "--compact", "-q")
			if err == nil {
				answers[i], err = decode[docs.Behind](res.Stdout, docs.BehindSchema)
			}
			errs[i] = err
		}()
	}
	wg.Wait()
	for _, err := range errs {
		if err != nil {
			return docs.Behind{}, err
		}
	}
	return mergeBehind(answers), nil
}

// pinKey names one pin as strata does: the pack name, the registry, the pinned version, and the tag.
type pinKey struct{ name, registryUID, version, tag string }

// mergeBehind joins the answers of `behind` on single records into the answer of one `behind` on all of
// them. It has one pin for each pack name, registry, pinned version, and tag, in that order, with each
// record that pins it. When one answer could not read a pin and another could, the pin keeps the read
// that has no error. The profile versions join in the same way: see mergeProfiles.
func mergeBehind(answers []docs.Behind) docs.Behind {
	var out docs.Behind
	at := map[pinKey]int{}
	for _, a := range answers {
		if out.Schema == "" {
			out.Schema, out.Tool = a.Schema, a.Tool
		}
		out.Profiles = mergeProfiles(out.Profiles, a.Profiles)
		for _, p := range a.Pins {
			k := pinKey{p.Name, p.RegistryUID, p.Version, p.Tag}
			i, found := at[k]
			if !found {
				at[k] = len(out.Pins)
				out.Pins = append(out.Pins, p)
				continue
			}
			carriers := joinCarriers(out.Pins[i].Carriers, p.Carriers)
			joinRead(&out.Pins[i], p, func(p docs.Pin) bool { return p.Error != nil }, func(p *docs.Pin) *[]docs.PinnedBy { return &p.PinnedBy })
			out.Pins[i].Carriers = carriers
		}
	}
	slices.SortStableFunc(out.Pins, func(a, b docs.Pin) int {
		return cmp.Or(strings.Compare(a.Name, b.Name), strings.Compare(a.RegistryUID, b.RegistryUID),
			strings.Compare(a.Version, b.Version), strings.Compare(a.Tag, b.Tag))
	})
	return out
}

// joinCarriers joins the carriers of one pin from two answers. Each answer of `behind` on one record
// names the profile versions of that record. The join keeps one carrier for each profile version, and
// a read with no error takes the place of a read that failed.
func joinCarriers(out, more []docs.PinCarrier) []docs.PinCarrier {
	out = slices.Clone(out)
	for _, c := range more {
		i := slices.IndexFunc(out, func(o docs.PinCarrier) bool { return o.Profile == c.Profile && o.Version == c.Version })
		switch {
		case i < 0:
			out = append(out, c)
		case out[i].Error != nil && c.Error == nil:
			out[i] = c
		}
	}
	return out
}

// uniquePinnedBy keeps the first of each subject, in order.
func uniquePinnedBy(by []docs.PinnedBy) []docs.PinnedBy {
	seen := map[string]bool{}
	out := by[:0]
	for _, b := range by {
		if !seen[b.Subject] {
			seen[b.Subject] = true
			out = append(out, b)
		}
	}
	return out
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
	return o.compareFull(ctx, dir, ref+pin.Version, ref+pin.Newer[0].Version)
}

// profileMoves compares the record of each profile version that is behind with the newest version of
// its profile, by the uid of that version. files maps a subject to its record in dir. The compares run
// at the same time, as many as the pool of processes lets. A compare that fails is in the second
// result, and the page then says that strata could not compare the two. The other versions stay in
// the cycle. Only a stop of ctx ends the cycle, and then the error is the one of ctx.
//
// A move of a profile has no cache, because a published version can change in place. So each cycle
// compares it again. The diff stays in memory for the detail tier, and the state never holds it.
func (o *Observer) profileMoves(ctx context.Context, dir string, files map[string]string, profiles []docs.ProfilePin) (map[string]docs.Diff, map[string]bool, error) {
	type job struct {
		subject, record, uid string
		diff                 docs.Diff
		err                  error
	}
	var jobs []*job
	for _, p := range profiles {
		subject := model.Subject(p.Name, p.Version)
		newest, ok := model.NewestProfile(p)
		record, found := files[subject]
		if ok && found && newest.UID != "" {
			jobs = append(jobs, &job{subject: subject, record: record, uid: newest.UID})
		}
	}
	o.expect(ctx, len(jobs))
	var wg sync.WaitGroup
	for _, j := range jobs {
		wg.Add(1)
		go func() {
			defer wg.Done()
			j.diff, j.err = o.compareFull(ctx, dir, j.record, j.uid)
		}()
	}
	wg.Wait()
	if ctx.Err() != nil {
		return nil, nil, ctx.Err()
	}
	moves, unread := map[string]docs.Diff{}, map[string]bool{}
	for _, j := range jobs {
		if j.err != nil {
			unread[j.subject] = true
			continue
		}
		moves[j.subject] = j.diff
	}
	return moves, unread, nil
}

// mergeProfiles adds the profile versions of one answer to the ones of the answers before it. It has
// one entry for each name@version, the subject of the observer, in the order of the answers. When one
// answer could not read a version and another could, the entry keeps the read that has no error. The
// entry keeps each record that holds the version.
func mergeProfiles(out, more []docs.ProfilePin) []docs.ProfilePin {
	for _, p := range more {
		i := slices.IndexFunc(out, func(q docs.ProfilePin) bool { return q.Name == p.Name && q.Version == p.Version })
		if i < 0 {
			out = append(out, p)
			continue
		}
		joinRead(&out[i], p, func(p docs.ProfilePin) bool { return p.Error != nil }, func(p *docs.ProfilePin) *[]docs.PinnedBy { return &p.PinnedBy })
	}
	return out
}

// joinRead joins the read of another answer into an entry with the same key. The read with no error
// takes the place of a read that failed. The entry keeps each record that pins it or holds it, once.
// failed says that a read has an error, and records gives the records of an entry.
func joinRead[T any](entry *T, read T, failed func(T) bool, records func(*T) *[]docs.PinnedBy) {
	by := append(slices.Clone(*records(entry)), *records(&read)...)
	if failed(*entry) && !failed(read) {
		*entry = read
	}
	*records(entry) = uniquePinnedBy(by)
}
