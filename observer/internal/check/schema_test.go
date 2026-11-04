package check

import (
	"context"
	"errors"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
)

// otherSchema is the id of a document that another binary can write. `--strata` can name any binary,
// so a document with this id must make no finding.
const otherSchema = "other-tool-document/1"

func kindOf(t *testing.T, err error) string {
	t.Helper()
	var re *runner.RunError
	if !errors.As(err, &re) {
		t.Fatalf("error %v holds no kind", err)
	}
	return re.Kind
}

func TestADocumentWithAnotherSchemaIsAFailedReadWithTheKindSchemaAndMakesNoFinding(t *testing.T) {
	// Each document would make a finding if the cycle read it: a drifted row, or a pack that changed.
	cases := []struct {
		name  string
		setup func(t *testing.T, w *world)
	}{
		{"verify", func(t *testing.T, w *world) { w.rows[0], w.verifySchema = drift(t), otherSchema }},
		{"the snapshot of the cluster", func(t *testing.T, w *world) { w.cluster.Schema = otherSchema }},
		{"compare", func(t *testing.T, w *world) {
			w.compares = map[int]docs.Diff{0: {Schema: otherSchema, RootB: "e06b", Packs: []docs.Pack{changedPack("csi/csi-aws-ebs")}}}
		}},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			r, w := oneRecord(t, unchanged())
			c.setup(t, w)
			o := r.observer(w)
			err := o.RunContent(context.Background())
			if err == nil || kindOf(t, err) != docs.KindSchema {
				t.Fatalf("error %v", err)
			}
			s := o.Summary()
			if len(s.Findings) != 0 || s.Counts.Drift != 0 || s.Content.FailedRuns != 1 || s.Content.LastError == nil || s.Content.LastError.Kind != docs.KindSchema {
				t.Errorf("findings %+v, content %+v", s.Findings, s.Content)
			}
		})
	}
}

func TestACompareWithAnotherSchemaBesideGoodSidesIsAProfileThatTheCycleDidNotRead(t *testing.T) {
	r, w := oneRecord(t, unchanged())
	r.record("platform-addons", "1.0.0", "addon/cert-manager")
	w.rows = []docs.VerifyRow{unchanged(), unchanged()}
	w.cluster = cluster(t, "payments", "aws-stack@1.1.0", "platform-addons@1.0.0")
	// The compare would make a finding, if the cycle read it.
	w.compares = map[int]docs.Diff{0: {Schema: otherSchema, RootB: "e06b", Packs: []docs.Pack{changedPack("csi/csi-aws-ebs")}}}
	o := r.observer(w)
	if err := o.RunContent(context.Background()); err != nil {
		t.Fatal(err)
	}
	s := o.Summary()
	if len(s.Findings) != 0 || s.Profiles[0].ClusterCopy.Status != model.CopyNotRead || s.Profiles[1].ClusterCopy.Status != model.CopySame {
		t.Errorf("findings %+v, profiles %+v", s.Findings, s.Profiles)
	}
}

func TestABehindWithAnotherSchemaIsAFailedReadWithTheKindSchema(t *testing.T) {
	r, w := withBehind(t)
	w.behind.Schema = otherSchema
	o := r.observer(w)
	err := o.RunVersions(context.Background())
	if err == nil || kindOf(t, err) != docs.KindSchema {
		t.Fatalf("error %v", err)
	}
	s := o.Summary()
	if len(s.Newer) != 0 || s.Versions.FailedRuns != 1 || s.Versions.LastError == nil || s.Versions.LastError.Kind != docs.KindSchema {
		t.Errorf("newer %+v, versions %+v", s.Newer, s.Versions)
	}
}

// A compare that fails is one row of the list with no counts, and it does not fail the cycle. A compare
// with another schema fails as any other compare does, so the row has no counts of the other document.
func TestAMoveWithAnotherSchemaIsAMoveThatWasNotRead(t *testing.T) {
	r, w := withBehind(t)
	kubernetes := move("kubernetes", "1.35.3", "1.36.2")
	d := w.moves[kubernetes]
	d.Schema = otherSchema
	w.moves[kubernetes] = d
	o := r.observer(w)
	if err := o.RunVersions(context.Background()); err != nil {
		t.Fatalf("a move with another schema failed the cycle: %v", err)
	}
	s := o.Summary()
	if s.Versions.ReadAt == nil || s.Versions.FailedRuns != 0 || s.Versions.LastError != nil || len(s.Newer) != 4 {
		t.Fatalf("versions %+v, newer %d", s.Versions, len(s.Newer))
	}
	for _, n := range s.Newer {
		if hasCounts := n.Impact != "" && n.Images > 0; (n.Name == "kubernetes") == hasCounts {
			t.Errorf("%s: impact %q, images %d", n.Name, n.Impact, n.Images)
		}
	}
	if len(s.NotChecked) != 1 || s.NotChecked[0].Cause != model.PinError || s.NotChecked[0].Packs[0] != "k8s/kubernetes" {
		t.Errorf("notChecked %+v", s.NotChecked)
	}
}
