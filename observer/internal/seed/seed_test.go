package seed

import (
	"bytes"
	"compress/gzip"
	"context"
	"encoding/base64"
	"encoding/json"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/runner"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// profiles are the profiles that the fake cluster runs, in its order.
var profiles = [][2]string{{"aws-stack", "1.0.0"}, {"platform-addons", "2.3.0"}}

func clusterJSON() string {
	return testkit.ClusterJSON(testkit.ClusterUID, "fred", testkit.ProfileJSON(profiles[0][0], profiles[0][1], "cluster"), testkit.ProfileJSON(profiles[1][0], profiles[1][1], "add-on"))
}

// fakeStrata writes the documents of a snapshot into the file of -o, as strata does.
type fakeStrata struct{ calls [][]string }

func (f *fakeStrata) Run(_ context.Context, dir string, args ...string) (runner.Result, error) {
	f.calls = append(f.calls, args)
	out := args[slices.Index(args, "-o")+1]
	doc := clusterJSON()
	if ref := args[1]; !strings.HasPrefix(ref, "cluster:") {
		name, version, _ := strings.Cut(ref, "@")
		doc = testkit.Snapshot(name, version)
	}
	return runner.Result{}, os.WriteFile(filepath.Join(dir, out), []byte(doc), 0o600)
}

// memStore holds the four Secrets in memory.
type memStore struct{ data map[string]map[string][]byte }

func newStore() *memStore {
	s := &memStore{data: map[string]map[string][]byte{}}
	for i := range Secrets {
		s.data[Name(i)] = map[string][]byte{}
	}
	return s
}

func (s *memStore) SecretKeys(_ context.Context, _, name string) ([]string, error) {
	var keys []string
	for k := range s.data[name] {
		keys = append(keys, k)
	}
	return keys, nil
}

func (s *memStore) PutSecret(_ context.Context, _, name string, _ map[string]string, data map[string][]byte) error {
	s.data[name] = data
	return nil
}

func gunzip(t *testing.T, b []byte) string {
	t.Helper()
	r, err := gzip.NewReader(bytes.NewReader(b))
	if err != nil {
		t.Fatal(err)
	}
	out, err := io.ReadAll(r)
	if err != nil {
		t.Fatal(err)
	}
	return string(out)
}

// A run with empty Secrets writes the first-seen records: meta.json and one key for each profile of the
// cluster, in the first Secret. A second run keeps them, and writes nothing.
func TestARunWritesTheFirstSeenRecordsOnceAndThenKeepsThem(t *testing.T) {
	strata, store := &fakeStrata{}, newStore()
	res, err := Run(context.Background(), strata, store, "strata-observer", testkit.ClusterUID, t.TempDir())
	if err != nil || res.Kept || res.Records != 2 {
		t.Fatalf(testkit.ResultAndError, res, err)
	}
	first := store.data[Name(0)]
	if got := string(first[MetaKey]); got != testkit.FirstSeenMeta(testkit.ClusterUID) {
		t.Errorf(MetaKey+" is %s", got)
	}
	for _, p := range profiles {
		key := p[0] + "_" + p[1] + ".json.gz"
		if got := gunzip(t, first[key]); got != testkit.Snapshot(p[0], p[1]) {
			t.Errorf("%s holds %s", key, got)
		}
	}
	calls := len(strata.calls)
	res, err = Run(context.Background(), strata, store, "strata-observer", testkit.ClusterUID, t.TempDir())
	if err != nil || !res.Kept || len(strata.calls) != calls {
		t.Errorf("the second run: %+v, error %v, %d new calls of strata", res, err, len(strata.calls)-calls)
	}
}

// A Secret that holds meta.json alone holds no record, so a run writes the records.
func TestASecretWithMetaAloneIsNoRecord(t *testing.T) {
	store := newStore()
	store.data[Name(0)][MetaKey] = []byte(testkit.FirstSeenMeta(testkit.ClusterUID))
	if res, err := Run(context.Background(), &fakeStrata{}, store, "ns", testkit.ClusterUID, t.TempDir()); err != nil || res.Kept {
		t.Errorf(testkit.ResultAndError, res, err)
	}
}

// A record that does not fit in the first Secret goes to the next one, and records that fit in no
// Secret, or two records with one key, are an error.
func TestTheRecordsFillTheSecretsInOrder(t *testing.T) {
	big := Record{Key: "big_1.0.0.json.gz", JSON: randomish(500000)}
	small := Record{Key: "small_1.0.0.json.gz", JSON: []byte(`{}`)}
	other := Record{Key: "other_1.0.0.json.gz", JSON: randomish(500000)}
	shards, err := Shards([]Record{big, other, small}, metaFirstSeen())
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := shards[0][big.Key]; !ok {
		t.Error("the first record is not in the first Secret")
	}
	if _, ok := shards[1][other.Key]; !ok {
		t.Error("a record that does not fit in the first Secret is not in the second")
	}
	if _, ok := shards[0][small.Key]; !ok {
		t.Error("a small record does not fill the room of the first Secret")
	}
	if _, err := Shards([]Record{small, small}, metaFirstSeen()); err == nil {
		t.Error("two records with one key are no error")
	}
	var many []Record
	for i := range 5 {
		many = append(many, Record{Key: string(rune('a'+i)) + ".json.gz", JSON: randomish(500000)})
	}
	if _, err := Shards(many, metaFirstSeen()); err == nil {
		t.Error("records that fit in no Secret are no error")
	}
}

// The seed and scripts/ci/observer-records.sh --first-seen write the same Secrets for one cluster: the
// same keys in the same Secrets, the same meta.json, and the same records. The bytes of gzip can differ
// between two programs, so the test compares the records after gunzip.
func TestTheSeedWritesWhatTheScriptOfThePipelineWrites(t *testing.T) {
	testkit.NeedTools(t, "bash", "jq", "gzip", "base64")
	var cases strings.Builder
	cases.WriteString("cluster:*) printf '%s' '" + clusterJSON() + "' > \"$out\" ;;\n")
	for _, p := range profiles {
		cases.WriteString(p[0] + "@" + p[1] + ") printf '%s' '" + testkit.Snapshot(p[0], p[1]) + "' > \"$out\" ;;\n")
	}
	fake := testkit.Strata(t, "case $ref in\n"+cases.String()+"esac\n")
	cmd := exec.Command("bash", testkit.RecordsScript, "--first-seen", testkit.ClusterUID)
	cmd.Env = append(os.Environ(), "STRATA_BIN="+fake)
	out, err := cmd.Output()
	if err != nil {
		t.Fatalf("the script: %v", err)
	}
	fromScript := secretsOfYAML(t, string(out))
	store := newStore()
	if _, err := Run(context.Background(), &fakeStrata{}, store, "strata-observer", testkit.ClusterUID, t.TempDir()); err != nil {
		t.Fatal(err)
	}
	for i := range Secrets {
		got, want := store.data[Name(i)], fromScript[Name(i)]
		if len(got) != len(want) {
			t.Errorf("%s: the seed writes %d keys, the script %d", Name(i), len(got), len(want))
		}
		for k, w := range want {
			g, ok := got[k]
			switch {
			case !ok:
				t.Errorf("%s: the seed has no %s", Name(i), k)
			case k == MetaKey:
				testkit.SamePlain(t, Name(i)+": the meta file of the seed", json.RawMessage(g), json.RawMessage(w))
			case gunzip(t, g) != gunzip(t, w):
				t.Errorf("%s: %s differs from the record of the script", Name(i), k)
			}
		}
	}
}

// secretsOfYAML reads the Secrets that the script prints, with testkit.RecordSecrets, and decodes the data.
func secretsOfYAML(t *testing.T, out string) map[string]map[string][]byte {
	t.Helper()
	got := map[string]map[string][]byte{}
	for name, secret := range testkit.RecordSecrets(t, out) {
		data := map[string][]byte{}
		for key, text := range secret {
			b, err := base64.StdEncoding.DecodeString(text)
			if err != nil {
				t.Fatalf("%s %s: %v", name, key, err)
			}
			data[key] = b
		}
		got[name] = data
	}
	return got
}

func metaFirstSeen() baseline.Meta {
	return baseline.Meta{Baseline: baseline.FirstSeen, FirstSeenFrom: testkit.ClusterUID}
}

// randomish gives n bytes that gzip cannot shrink much, from a fixed seed, so a test can fill a Secret.
func randomish(n int) []byte {
	b := make([]byte, n)
	x := uint32(2463534242)
	for i := range b {
		x ^= x << 13
		x ^= x >> 17
		x ^= x << 5
		b[i] = byte(x)
	}
	return b
}
