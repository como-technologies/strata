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
	"regexp"
	"slices"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/runner"
)

const clusterUID = "7b1f3c9e-5a42-4d8e-9a67-0c2e4f1a8b35"

// profiles are the profiles that the fake cluster runs, in its order.
var profiles = [][2]string{{"aws-stack", "1.0.0"}, {"platform-addons", "2.3.0"}}

func clusterJSON() string {
	return `{"schema":"strata-snapshot/1","source":{"cluster":{"uid":"` + clusterUID + `","name":"fred","profiles":[` +
		`{"name":"aws-stack","version":"1.0.0","type":"cluster"},{"name":"platform-addons","version":"2.3.0","type":"add-on"}]}}}`
}

func recordJSON(name, version string) string {
	return `{"schema":"strata-snapshot/1","source":{"name":"` + name + `","version":"` + version + `"},"root":"r-` + name + `"}`
}

// fakeStrata writes the documents of a snapshot into the file of -o, as strata does.
type fakeStrata struct{ calls [][]string }

func (f *fakeStrata) Run(_ context.Context, dir string, args ...string) (runner.Result, error) {
	f.calls = append(f.calls, args)
	out := args[slices.Index(args, "-o")+1]
	doc := clusterJSON()
	if ref := args[1]; !strings.HasPrefix(ref, "cluster:") {
		name, version, _ := strings.Cut(ref, "@")
		doc = recordJSON(name, version)
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
	res, err := Run(context.Background(), strata, store, "strata-observer", clusterUID, t.TempDir())
	if err != nil || res.Kept || res.Records != 2 {
		t.Fatalf("result %+v, error %v", res, err)
	}
	first := store.data[Name(0)]
	if got := string(first[MetaKey]); got != `{"baseline":"first-seen","firstSeenFrom":"`+clusterUID+`"}` {
		t.Errorf("meta.json is %s", got)
	}
	for _, p := range profiles {
		key := p[0] + "_" + p[1] + ".json.gz"
		if got := gunzip(t, first[key]); got != recordJSON(p[0], p[1]) {
			t.Errorf("%s holds %s", key, got)
		}
	}
	calls := len(strata.calls)
	res, err = Run(context.Background(), strata, store, "strata-observer", clusterUID, t.TempDir())
	if err != nil || !res.Kept || len(strata.calls) != calls {
		t.Errorf("the second run: %+v, error %v, %d new calls of strata", res, err, len(strata.calls)-calls)
	}
}

// A Secret that holds meta.json alone holds no record, so a run writes the records.
func TestASecretWithMetaAloneIsNoRecord(t *testing.T) {
	store := newStore()
	store.data[Name(0)][MetaKey] = []byte(`{"baseline":"first-seen"}`)
	if res, err := Run(context.Background(), &fakeStrata{}, store, "ns", clusterUID, t.TempDir()); err != nil || res.Kept {
		t.Errorf("result %+v, error %v", res, err)
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
	for _, tool := range []string{"bash", "jq", "gzip", "base64"} {
		if _, err := exec.LookPath(tool); err != nil {
			if os.Getenv("CI") != "" {
				t.Fatalf("%s is missing on a runner of CI", tool)
			}
			t.Skipf("%s is missing", tool)
		}
	}
	dir := t.TempDir()
	fake := filepath.Join(dir, "strata")
	var cases strings.Builder
	cases.WriteString("cluster:*) printf '%s' '" + clusterJSON() + "' > \"$out\" ;;\n")
	for _, p := range profiles {
		cases.WriteString(p[0] + "@" + p[1] + ") printf '%s' '" + recordJSON(p[0], p[1]) + "' > \"$out\" ;;\n")
	}
	script := "#!/usr/bin/env bash\nout=\"\"\nprev=\"\"\nfor a in \"$@\"; do [ \"$prev\" = -o ] && out=$a; prev=$a; done\ncase \"$2\" in\n" + cases.String() + "esac\n"
	if err := os.WriteFile(fake, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	cmd := exec.Command("bash", "../../../scripts/ci/observer-records.sh", "--first-seen", clusterUID)
	cmd.Env = append(os.Environ(), "STRATA_BIN="+fake)
	out, err := cmd.Output()
	if err != nil {
		t.Fatalf("the script: %v", err)
	}
	fromScript := secretsOfYAML(t, string(out))
	store := newStore()
	if _, err := Run(context.Background(), &fakeStrata{}, store, "strata-observer", clusterUID, t.TempDir()); err != nil {
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
				var a, b map[string]any
				_ = json.Unmarshal(g, &a)
				_ = json.Unmarshal(w, &b)
				if !jsonEqual(a, b) {
					t.Errorf("%s: meta.json is %s, the script writes %s", Name(i), g, w)
				}
			case gunzip(t, g) != gunzip(t, w):
				t.Errorf("%s: %s differs from the record of the script", Name(i), k)
			}
		}
	}
}

var (
	secretName = regexp.MustCompile(`(?m)^  name: (\S+)$`)
	dataLine   = regexp.MustCompile(`(?m)^  ([A-Za-z0-9._-]+): (\S+)$`)
)

// secretsOfYAML reads the Secrets that the script prints: the name of each, and the decoded data.
func secretsOfYAML(t *testing.T, out string) map[string]map[string][]byte {
	t.Helper()
	got := map[string]map[string][]byte{}
	for _, doc := range strings.Split(out, "---\n") {
		name := secretName.FindStringSubmatch(doc)
		if name == nil {
			continue
		}
		data := map[string][]byte{}
		if _, body, ok := strings.Cut(doc, "\ndata:\n"); ok {
			for _, m := range dataLine.FindAllStringSubmatch(body, -1) {
				b, err := base64.StdEncoding.DecodeString(m[2])
				if err != nil {
					t.Fatalf("%s %s: %v", name[1], m[1], err)
				}
				data[m[1]] = b
			}
		}
		got[name[1]] = data
	}
	return got
}

func jsonEqual(a, b map[string]any) bool {
	x, _ := json.Marshal(a)
	y, _ := json.Marshal(b)
	return bytes.Equal(x, y)
}

func metaFirstSeen() baseline.Meta {
	return baseline.Meta{Baseline: baseline.FirstSeen, FirstSeenFrom: clusterUID}
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
