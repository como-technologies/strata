package model

import (
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

var t0 = time.Date(2026, 11, 3, 14, 0, 0, 0, time.UTC)

// allKinds lists every kind of finding. A test that walks the kinds uses it, so a new kind fails the
// tests that did not learn it.
var allKinds = []Kind{PublishedChanged, Overwritten, Renamed, Gone, ClusterDiffers, ImageIDChanged, ImageMoved, ImageSplit}

// pk makes a pack of a record: layer/name.
func pk(layer, name, version string) docs.RecordPack {
	return docs.RecordPack{Key: layer + "/" + name, Layer: layer, Name: name, Version: version, Type: "spectro", RegistryUID: "reg1",
		RegistryProviderType: "spectro", Images: []string{"a/b:1", "a/c@sha256:abc"}, Hash: docs.PackHash{UpstreamRaw: "sha256:" + name, UpstreamSource: "packRef.digest"}}
}

// rec makes a record that the cluster runs, with a `verify` row of the given status. The compare with
// the cluster side read well and found no change.
func rec(name, version, status string, packs ...docs.RecordPack) Record {
	r := Record{File: name + ".json.gz", Row: &docs.VerifyRow{Status: status, Name: name, Version: version, RootRecorded: "r1", RootLive: "r1"}, Cluster: ClusterSide{Runs: true, Diff: &docs.Diff{Same: true, SameContent: true}}}
	r.Doc.Schema = docs.SnapshotSchema
	r.Doc.Tool.Version = "0.28.0"
	r.Doc.Source.Name, r.Doc.Source.Version, r.Doc.Source.Type = name, version, "cluster"
	r.Doc.Packs = packs
	return r
}

// drifted makes a `verify` row that drifted, with a diff that changes one pack.
func drifted(r Record, v *docs.Verify, keys ...string) Record {
	r.Row.Status = docs.VerifyDrifted
	r.Row.RootLive = "r2"
	r.Row.MaxImpact = "in-place"
	r.Row.Diff = &docs.Diff{Verify: v, Summary: docs.DiffSummary{MaxImpact: "in-place"}}
	for _, k := range keys {
		r.Row.Diff.Packs = append(r.Row.Diff.Packs, changed(k, "charts.a.b", "charts.a.c"))
	}
	return r
}

// changed makes a changed pack with the given changed paths.
func changed(key string, paths ...string) docs.Pack {
	p := docs.Pack{Key: key, Status: docs.PackChanged, Authored: true}
	for _, path := range paths {
		p.Values.Diff = append(p.Values.Diff, docs.Leaf{Op: "changed", Path: path, Old: "OLD-SENTINEL", New: "NEW-SENTINEL"})
	}
	return p
}

// onCluster gives the record a compare with the cluster side.
func onCluster(r Record, d docs.Diff) Record {
	r.Cluster = ClusterSide{Runs: true, Diff: &d}
	return r
}

func notOnCluster(r Record) Record {
	r.Cluster = ClusterSide{}
	return r
}

func content(t *testing.T, records ...Record) Content {
	t.Helper()
	var profiles []string
	for _, r := range records {
		if r.Cluster.Runs {
			profiles = append(profiles, r.subject())
		}
	}
	return Content{At: t0, Strata: "0.29.0", Cluster: testkit.Cluster("payments", profiles...), Meta: baseline.Meta{Baseline: baseline.Approved, Revision: "4e1c7a2"}, Records: records}
}

func goodClock(every time.Duration, at time.Time) Clock {
	return Clock{Every: every, Last: &at}
}

// build builds a summary at t0 plus one minute, with good clocks.
func build(c *Content, v *Versions) Summary {
	return Build(Input{Now: t0.Add(time.Minute), ClusterUID: "c1", StartedAt: t0, Files: 2, FilesKnown: true, Content: c, Versions: v,
		ContentClock: goodClock(15*time.Minute, t0), VersionsClock: goodClock(6*time.Hour, t0)})
}

func ptr[T any](v T) *T { return &v }
