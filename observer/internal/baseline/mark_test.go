package baseline

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// awsFile is the record file of the tests of the mark.
const awsFile = "aws-stack@1.0.0.json.gz"

// Mark changes when meta.json or a record file changes, and not when another file changes. A directory
// that cannot be listed gives the empty text.
func TestMarkChangesWithTheRecordsAndMetaJSONAndNotWithAnotherFile(t *testing.T) {
	dir := t.TempDir()
	testkit.WriteGzip(t, dir, awsFile, `{"schema":"`+docs.SnapshotSchema+`"}`)
	first := Mark(dir)
	if first == "" {
		t.Fatal("no mark")
	}
	if err := os.WriteFile(filepath.Join(dir, "notes.txt"), []byte("x"), 0o600); err != nil {
		t.Fatal(err)
	}
	if Mark(dir) != first {
		t.Error("a file that is no record changed the mark")
	}
	if err := os.WriteFile(filepath.Join(dir, metaFile), []byte(`{"baseline":"approved"}`), 0o600); err != nil {
		t.Fatal(err)
	}
	second := Mark(dir)
	if second == first {
		t.Error("a new meta.json kept the mark")
	}
	testkit.WriteGzip(t, dir, awsFile, `{"schema":"`+docs.SnapshotSchema+`","packs":[]}`)
	if Mark(dir) == second {
		t.Error("a changed record kept the mark")
	}
	if Mark(filepath.Join(dir, "nope")) != "" {
		t.Error("a directory that is not there has a mark")
	}
}
