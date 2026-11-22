package baseline

import (
	"path/filepath"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// awsFile is the record file of the tests of the mark.
const awsFile = testkit.AWSStackOld + ".json.gz"

// Mark changes when meta.json or a record file changes, and not when another file changes. A directory
// that cannot be listed gives the empty text.
func TestMarkChangesWithTheRecordsAndMetaJSONAndNotWithAnotherFile(t *testing.T) {
	dir := t.TempDir()
	testkit.WriteGzip(t, dir, awsFile, `{"schema":"`+docs.SnapshotSchema+`"}`)
	first := Mark(dir)
	if first == "" {
		t.Fatal("no mark")
	}
	testkit.WriteIn(t, dir, "notes.txt", "x")
	if Mark(dir) != first {
		t.Error("a file that is no record changed the mark")
	}
	testkit.WriteMeta(t, dir, testkit.ApprovedMeta(testkit.Revision))
	second := Mark(dir)
	if second == first {
		t.Error("a new " + metaFile + " kept the mark")
	}
	testkit.WriteGzip(t, dir, awsFile, `{"schema":"`+docs.SnapshotSchema+`"`+testkit.NoPacks+`}`)
	if Mark(dir) == second {
		t.Error("a changed record kept the mark")
	}
	if Mark(filepath.Join(dir, "nope")) != "" {
		t.Error("a directory that is not there has a mark")
	}
}

// The const of testkit that copies a const of baseline holds the value of that const.
func TestEachCopyInTestkitHoldsTheValueOfItsConst(t *testing.T) {
	if testkit.MetaFile != metaFile {
		t.Errorf("testkit holds %q, and baseline holds %q", testkit.MetaFile, metaFile)
	}
}
