package baseline

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

const marker = "SENTINEL-MUST-NOT-LEAVE"

func write(t *testing.T, dir, name, body string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(dir, name), []byte(body), 0o600); err != nil {
		t.Fatal(err)
	}
}

const aws = `{"schema":"` + docs.SnapshotSchema + `","tool":{"version":"0.28.0"},"source":{"uid":"u1","name":"aws-stack","version":"1.1.0","type":"cluster"},"root":"r1","packs":[]}`

func TestTheRecordsAndTheMetaAreRead(t *testing.T) {
	dir := t.TempDir()
	testkit.WriteGzip(t, dir, "aws-stack.json.gz", aws)
	write(t, dir, "meta.json", `{"baseline":"approved","revision":"4e1c7a2"}`)
	set, err := Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	if set.Meta != (Meta{Baseline: Approved, Revision: "4e1c7a2"}) {
		t.Errorf("meta %+v", set.Meta)
	}
	if len(set.Entries) != 1 || set.Entries[0].File != "aws-stack.json.gz" || set.Entries[0].Record.Source.Name != "aws-stack" {
		t.Fatalf("entries %+v", set.Entries)
	}
	if string(set.Entries[0].Raw) != aws {
		t.Errorf("raw %s", set.Entries[0].Raw)
	}
}

func TestTheMetaOfAFirstSeenBaselineNamesTheCluster(t *testing.T) {
	dir := t.TempDir()
	write(t, dir, "meta.json", `{"baseline":"first-seen","firstSeenFrom":"64f1"}`)
	set, err := Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	if set.Meta != (Meta{Baseline: FirstSeen, FirstSeenFrom: "64f1"}) {
		t.Errorf("meta %+v", set.Meta)
	}
}

func TestABaselineWithNoMetaOrABadMetaIsUnknown(t *testing.T) {
	for name, body := range map[string]string{
		"absent":        "",
		"not json":      "{",
		"other value":   `{"baseline":"guess"}`,
		"no baseline":   `{"revision":"x"}`,
		"empty content": `{}`,
	} {
		dir := t.TempDir()
		if body != "" {
			write(t, dir, "meta.json", body)
		}
		set, err := Load(dir)
		if err != nil || set.Meta.Baseline != Unknown {
			t.Errorf("%s: meta %+v, error %v", name, set.Meta, err)
		}
	}
}

func TestAFileThatIsNoProfileRecordDoesNotStopTheOthers(t *testing.T) {
	dir := t.TempDir()
	testkit.WriteGzip(t, dir, "a-good.json.gz", aws)
	write(t, dir, "b-plain.json.gz", aws)
	testkit.WriteGzip(t, dir, "c-other-schema.json.gz", `{"schema":"`+docs.DiffSchema+`"}`)
	testkit.WriteGzip(t, dir, "d-cluster.json.gz", `{"schema":"`+docs.SnapshotSchema+`","source":{"cluster":{"uid":"c1"}}}`)
	testkit.WriteGzip(t, dir, "e-not-json.json.gz", `not json`)
	testkit.WriteGzip(t, dir, "f-good.json.gz", aws)
	set, err := Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	if got := len(set.Entries); got != 2 {
		t.Errorf("%d records", got)
	}
	want := "b-plain.json.gz c-other-schema.json.gz d-cluster.json.gz e-not-json.json.gz"
	if got := strings.Join(set.Unreadable, " "); got != want {
		t.Errorf("unreadable %q, want %q", got, want)
	}
	if set.Files() != 6 {
		t.Errorf("files %d", set.Files())
	}
}

func TestOnlyAFileWithTheSuffixOfARecordIsRead(t *testing.T) {
	dir := t.TempDir()
	write(t, dir, "README", "x")
	write(t, dir, "aws.json", aws)
	if err := os.Mkdir(filepath.Join(dir, "..data"), 0o700); err != nil {
		t.Fatal(err)
	}
	set, err := Load(dir)
	if err != nil || set.Files() != 0 {
		t.Fatalf("set %+v, error %v", set, err)
	}
}

func TestAMountedSecretWithLinksIsRead(t *testing.T) {
	dir := t.TempDir()
	data := filepath.Join(dir, "..2026_11_03")
	if err := os.Mkdir(data, 0o700); err != nil {
		t.Fatal(err)
	}
	testkit.WriteGzip(t, data, "aws-stack.json.gz", aws)
	if err := os.Symlink("..2026_11_03", filepath.Join(dir, "..data")); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(filepath.Join("..data", "aws-stack.json.gz"), filepath.Join(dir, "aws-stack.json.gz")); err != nil {
		t.Fatal(err)
	}
	set, err := Load(dir)
	if err != nil || len(set.Entries) != 1 {
		t.Fatalf("set %+v, error %v", set, err)
	}
}

func TestEachLoadReadsTheDirectoryAgain(t *testing.T) {
	dir := t.TempDir()
	first, err := Load(dir)
	if err != nil || first.Files() != 0 {
		t.Fatalf("first %+v, error %v", first, err)
	}
	testkit.WriteGzip(t, dir, "aws-stack.json.gz", aws)
	second, err := Load(dir)
	if err != nil || len(second.Entries) != 1 {
		t.Fatalf("second %+v, error %v", second, err)
	}
}

func TestADirectoryThatDoesNotExistIsAnError(t *testing.T) {
	if _, err := Load(filepath.Join(t.TempDir(), "nope")); err == nil {
		t.Fatal("no error")
	}
}

func TestARecordDecodesOnlyIntoTheAllowList(t *testing.T) {
	dir := t.TempDir()
	body := strings.Replace(aws, `"root":"r1"`, `"root":"r1","variables":[{"name":"x","value":"`+marker+`"}]`, 1)
	testkit.WriteGzip(t, dir, "aws-stack.json.gz", body)
	set, err := Load(dir)
	if err != nil || len(set.Entries) != 1 {
		t.Fatalf("set %+v, error %v", set, err)
	}
	out, err := json.Marshal(set.Entries[0].Record)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(out), marker) {
		t.Fatalf("the marker passed the allow-list: %s", out)
	}
}

func TestARecordAboveTheLimitIsUnreadable(t *testing.T) {
	dir := t.TempDir()
	testkit.WriteGzip(t, dir, "big.json.gz", `{"schema":"`+docs.SnapshotSchema+`","pad":"`+strings.Repeat("a", maxRecordBytes)+`"}`)
	set, err := Load(dir)
	if err != nil || len(set.Entries) != 0 || len(set.Unreadable) != 1 {
		t.Fatalf("set %+v, error %v", set, err)
	}
}
