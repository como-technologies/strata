package check

import (
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
)

func TestDecodeRefusesAnEmptyObjectAndAnotherSchemaAndKeepsTheRightOne(t *testing.T) {
	check := func(name string, got, want error) {
		t.Helper()
		switch {
		case want == nil && got != nil:
			t.Errorf("%s: error %v", name, got)
		case want != nil && (got == nil || kindOf(t, got) != kindOf(t, want)):
			t.Errorf("%s: error %v, want %v", name, got, want)
		}
	}
	schema, unknown := failure(docs.KindSchema), failure(docs.KindUnknown)
	for _, c := range []struct {
		raw  string
		want error
	}{
		{`{}`, schema},
		{`{"schema":""}`, schema},
		{`{"schema":"` + otherSchema + `"}`, schema},
		{`{"schema":"strata-diff/2"}`, schema},
		{`{"schema":"strata-behind/1"}`, schema},
		{`{"schema":"strata-diff/1"}`, nil},
		{`not json`, unknown},
		{`[]`, unknown},
	} {
		_, err := decode[docs.Diff]([]byte(c.raw), docs.DiffSchema)
		check("diff "+c.raw, err, c.want)
	}
	_, err := decode[docs.VerifySummary]([]byte(`{"schema":"strata-verify-summary/1"}`), docs.VerifySummarySchema)
	check("verify", err, nil)
	_, err = decode[docs.Behind]([]byte(`{"schema":"strata-behind/1"}`), docs.BehindSchema)
	check("behind", err, nil)
	_, err = decode[docs.ClusterRead]([]byte(`{"schema":"strata-snapshot/1"}`), docs.SnapshotSchema)
	check("snapshot", err, nil)
	_, err = decode[docs.ClusterRead]([]byte(`{}`), docs.SnapshotSchema)
	check("snapshot with no schema", err, schema)
}
