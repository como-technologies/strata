package check

import (
	"context"
	"os"
	"slices"
	"time"

	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
)

// membership links a cluster of a set to its set. The set gives each member its records, the row of `verify` of
// each record, and the versions of its projects. The member reads its own cluster, and keeps its own findings.
type membership struct {
	// home is the project of the key api-key. A record with no project is checked with that key.
	home string
	// keys gives the keys of the set at the last read. It takes the lock of the keys of the set, and never the lock
	// of the set, so a member that holds its own lock can call it.
	keys func() []runner.Key
}

// setRead is what the set gives a member for one content cycle: the records, and the row of `verify` of each one by
// the uid of the record. Strata is the version of the binary that checked the records.
type setRead struct {
	records baseline.Set
	rows    map[string]docs.VerifyRow
	strata  string
}

// projectOf gives the project whose key checks a record: the project of the record, or home for a record that a
// request at tenant scope took.
func (m *membership) projectOf(r docs.Record) string {
	if r.Source.ProjectUID == "" {
		return m.home
	}
	return r.Source.ProjectUID
}

// keyed gives a context whose runs use the key of the project of the record of subject in the last good content.
// A project with no key gets a key with no secret, so that strata refuses the run: the key of another project must
// never read it. A cluster with no member link keeps ctx.
func (o *Observer) keyed(ctx context.Context, subject string) context.Context {
	if o.member == nil {
		return ctx
	}
	o.mu.Lock()
	project := o.member.home
	if r, ok := o.recordOf(subject); ok {
		project = o.member.projectOf(r.Doc)
	}
	o.mu.Unlock()
	key, ok := keyOfProject(o.member.keys(), project)
	if !ok {
		key = runner.Key{Project: project}
	}
	return runner.WithKey(ctx, key)
}

// recordOf finds the record of a profile in the last good content. The caller holds the lock.
func (o *Observer) recordOf(subject string) (model.Record, bool) {
	if o.content == nil {
		return model.Record{}, false
	}
	i := slices.IndexFunc(o.content.Records, func(r model.Record) bool {
		return model.Subject(r.Doc.Source.Name, r.Doc.Source.Version) == subject
	})
	if i < 0 {
		return model.Record{}, false
	}
	return o.content.Records[i], true
}

// runs says that the cluster runs the profile of a record. A member of a set knows the record of a profile by its
// uid, because two projects can hold one name and version. The observer of one cluster knows it by its name and
// its version.
func (o *Observer) runs(read docs.ClusterRead, r docs.Record) bool {
	return slices.ContainsFunc(read.Source.Cluster.Profiles, func(p docs.ClusterProfile) bool {
		if o.member != nil {
			return p.UID != "" && p.UID == r.Source.UID
		}
		return p.Name == r.Source.Name && p.Version == r.Source.Version
	})
}

// memberCycle reads the cluster of a member: the snapshot of the cluster, and the compare of each record that it
// runs. The set checked the records already, one time for each project. The raw answers of the cluster go when
// the read ends, before the set reads the next cluster: the work volume is in memory.
func (o *Observer) memberCycle(ctx context.Context, in setRead) (model.Content, int, error) {
	var content model.Content
	err := o.withCycleDir(func(dir string) error {
		content.Meta, content.Unreadable, content.Member = in.records.Meta, in.records.Unreadable, o.cfg.ClusterUID
		o.readPods(ctx, &content)
		o.expect(ctx, 1) // the read of the cluster
		read, err := o.readCluster(ctx, dir)
		if err != nil {
			return err
		}
		content.Cluster, content.Strata = read, in.strata
		var entries []baseline.Entry
		for _, e := range in.records.Entries {
			if o.runs(read, e.Record) && !slices.ContainsFunc(entries, func(x baseline.Entry) bool { return x.Record.Source.UID == e.Record.Source.UID }) {
				entries = append(entries, e)
			}
		}
		names, err := writeRecords(dir, entries)
		if err != nil {
			return err
		}
		rows := map[string]docs.VerifyRow{}
		for i, e := range entries {
			if row, ok := in.rows[e.Record.Source.UID]; ok {
				rows[names[i]] = row
			}
		}
		content.Records, err = o.compareRecords(ctx, dir, entries, names, rows, read)
		content.At = o.now()
		return err
	})
	return content, in.records.Files(), err
}

// failMember counts a content cycle of a member that the set could not start, such as for a refused key, with the
// kind and the status of err. The last good answer stays.
func (o *Observer) failMember(err error) {
	o.mu.Lock()
	defer o.mu.Unlock()
	o.ended[cycleContent] = o.now()
	o.fail(cycleContent, &o.state.Content, time.Now(), 0, err)
}

// keepVersions gives a member the versions of the projects of its records, with the clock of the worst of those
// projects. details holds the pack of each move that the set compared, by the key of the cache. A move that the
// versions do not hold leaves the memory, with the answers of Palette about it.
func (o *Observer) keepVersions(v *model.Versions, clock ClockState, details map[string]docs.Pack) {
	o.mu.Lock()
	defer o.mu.Unlock()
	o.versions, o.versionsAt, o.state.Versions = v, nil, clock
	if v != nil && !v.At.IsZero() {
		o.versionsAt = &v.At
	}
	used := map[string]bool{}
	for k, p := range details {
		used[k] = true
		o.details[k] = p
	}
	for k := range o.details {
		if !used[k] {
			delete(o.details, k)
		}
	}
	for k := range o.asks {
		if !used[k.move] {
			delete(o.asks, k)
		}
	}
	o.save()
}

// removeState removes the directory of the state of a member, when its cluster leaves the set.
func (o *Observer) removeState() {
	if err := os.RemoveAll(o.cfg.State); err != nil {
		o.log.Error("the state of a cluster stays", "kind", docs.KindIO)
	}
}

// memberRecord gives the uid of the record of subject that a member of a set runs, or empty text with no member
// link. Two projects can hold one name and version, so a member finds its record by the uid.
func (o *Observer) memberRecord(subject string) string {
	if o.member == nil {
		return ""
	}
	o.mu.Lock()
	defer o.mu.Unlock()
	r, _ := o.recordOf(subject)
	return r.Doc.Source.UID
}

// nameMember keeps the name and the project of the cluster of a member in its state, so that a restart lists the
// cluster before its first search.
func (o *Observer) nameMember(name string, project model.ProjectRef) {
	o.mu.Lock()
	defer o.mu.Unlock()
	next := &MemberState{Name: name, Project: project}
	if o.state.Member != nil && *o.state.Member == *next {
		return
	}
	o.state.Member = next
	o.save()
}
