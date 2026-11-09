// Package seed writes the first-seen records of a cluster into the Secrets of the records, from inside
// the cluster. A job of the install runs it once, so a team gets a baseline with no clone of the
// repository and no build of strata.
//
// The records are the ones that `scripts/ci/observer-records.sh --first-seen` writes, by the same
// rules: one key for each profile version, the gzip of the record, four Secrets filled in order, and
// meta.json in the first one. The script stays the way of a pipeline, which has no observer binary.
// The tests of this package hold both to the same output.
package seed

import (
	"bytes"
	"compress/gzip"
	"context"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"slices"

	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/runner"
)

// Secrets is the number of Secrets of the records, and Limit is the most bytes of base64 in one of
// them, as in the script. MetaKey is the key of the meta file in the first Secret.
const (
	Secrets = 4
	Limit   = 900000
	MetaKey = "meta.json"
)

// Labels are the labels of each Secret of the records.
var Labels = map[string]string{"app.kubernetes.io/name": "strata-observer"}

// Name gives the name of the Secret i of the records.
func Name(i int) string { return fmt.Sprintf("strata-observer-records-%d", i) }

// Strata runs one command of strata in a directory.
type Strata interface {
	Run(ctx context.Context, dir string, args ...string) (runner.Result, error)
}

// Store reads the keys of a Secret and replaces its data.
type Store interface {
	SecretKeys(ctx context.Context, namespace, name string) ([]string, error)
	PutSecret(ctx context.Context, namespace, name string, labels map[string]string, data map[string][]byte) error
}

// Result says what a run did. Kept is true when a Secret already held a record: the run then wrote
// nothing, so a record that a pipeline approved is never replaced.
type Result struct {
	Kept    bool
	Records int
}

// notAFileName matches each character that a key or a file name does not keep, as `tr -c` does in the
// script.
var notAFileName = regexp.MustCompile(`[^A-Za-z0-9._-]`)

// Run writes the first-seen records of the cluster, unless a Secret holds a record. work is a directory
// for the files of strata, and the run removes what it writes there.
func Run(ctx context.Context, strata Strata, store Store, namespace, clusterUID, work string) (Result, error) {
	for i := range Secrets {
		keys, err := store.SecretKeys(ctx, namespace, Name(i))
		if err != nil {
			return Result{}, err
		}
		if slices.ContainsFunc(keys, func(k string) bool { return k != MetaKey }) {
			return Result{Kept: true}, nil
		}
	}
	dir, err := os.MkdirTemp(work, "seed-")
	if err != nil {
		return Result{}, fmt.Errorf("seed: %s", docs.KindIO)
	}
	defer os.RemoveAll(dir)
	records, err := firstSeen(ctx, strata, dir, clusterUID)
	if err != nil {
		return Result{}, err
	}
	shards, err := Shards(records, baseline.Meta{Baseline: baseline.FirstSeen, FirstSeenFrom: clusterUID})
	if err != nil {
		return Result{}, err
	}
	for i, data := range shards {
		if err := store.PutSecret(ctx, namespace, Name(i), Labels, data); err != nil {
			return Result{}, err
		}
	}
	return Result{Records: len(records)}, nil
}

// Record is one record: its key in a Secret, and the JSON of strata.
type Record struct {
	Key  string
	JSON []byte
}

// firstSeen records each profile that the cluster runs now, in the order of the cluster.
func firstSeen(ctx context.Context, strata Strata, dir, clusterUID string) ([]Record, error) {
	if _, err := strata.Run(ctx, dir, runner.CmdSnapshot, "cluster:"+clusterUID, "-o", "cluster.json", "-q"); err != nil {
		return nil, err
	}
	var cluster docs.ClusterRead
	if err := readJSON(filepath.Join(dir, "cluster.json"), &cluster); err != nil {
		return nil, err
	}
	var out []Record
	for _, p := range cluster.Source.Cluster.Profiles {
		ref := p.Name + "@" + p.Version
		file := notAFileName.ReplaceAllString(ref, "_") + ".json"
		if _, err := strata.Run(ctx, dir, runner.CmdSnapshot, ref, "-o", file, "-q"); err != nil {
			return nil, err
		}
		rec, err := recordOf(filepath.Join(dir, file))
		if err != nil {
			return nil, err
		}
		out = append(out, rec)
	}
	if len(out) == 0 {
		return nil, fmt.Errorf("seed: the cluster runs no profile")
	}
	return out, nil
}

// recordOf reads one record of a profile, and gives it its key. A record of a cluster is refused.
func recordOf(path string) (Record, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return Record{}, fmt.Errorf("seed: %s", docs.KindIO)
	}
	var head struct {
		Schema string `json:"schema"`
		Source struct {
			Name    string          `json:"name"`
			Version string          `json:"version"`
			Cluster json.RawMessage `json:"cluster"`
		} `json:"source"`
	}
	if err := json.Unmarshal(raw, &head); err != nil || head.Schema != docs.SnapshotSchema || (len(head.Source.Cluster) > 0 && string(head.Source.Cluster) != "null") {
		return Record{}, fmt.Errorf("seed: %s is not a profile record", filepath.Base(path))
	}
	return Record{Key: notAFileName.ReplaceAllString(head.Source.Name+"_"+head.Source.Version+".json.gz", "_"), JSON: raw}, nil
}

// Shards gives the data of the four Secrets: meta.json in the first, then the gzip of each record in
// the first Secret that has room for its base64. Two records with one key, or records that do not fit,
// are an error.
func Shards(records []Record, meta baseline.Meta) ([]map[string][]byte, error) {
	m, err := json.Marshal(meta)
	if err != nil {
		return nil, fmt.Errorf("seed: %s", docs.KindIO)
	}
	shards := make([]map[string][]byte, Secrets)
	sizes := make([]int, Secrets)
	for i := range shards {
		shards[i] = map[string][]byte{}
	}
	shards[0][MetaKey] = m
	seen := map[string]bool{}
	for _, r := range records {
		if seen[r.Key] {
			return nil, fmt.Errorf("seed: two records are for %s", r.Key)
		}
		seen[r.Key] = true
		gz, err := gzipOf(r.JSON)
		if err != nil {
			return nil, err
		}
		size := base64.StdEncoding.EncodedLen(len(gz))
		i := slices.IndexFunc(sizes, func(used int) bool { return used+size <= Limit })
		if i < 0 {
			return nil, fmt.Errorf("seed: the records do not fit in %d Secrets: %s", Secrets, r.Key)
		}
		shards[i][r.Key] = gz
		sizes[i] += size
	}
	return shards, nil
}

// gzipOf compresses as `gzip -9n` does: the best level, and no name and no time in the header.
func gzipOf(raw []byte) ([]byte, error) {
	var buf bytes.Buffer
	w, err := gzip.NewWriterLevel(&buf, gzip.BestCompression)
	if err != nil {
		return nil, fmt.Errorf("seed: %s", docs.KindIO)
	}
	if _, err := w.Write(raw); err != nil {
		return nil, fmt.Errorf("seed: %s", docs.KindIO)
	}
	if err := w.Close(); err != nil {
		return nil, fmt.Errorf("seed: %s", docs.KindIO)
	}
	return buf.Bytes(), nil
}

func readJSON(path string, v any) error {
	raw, err := os.ReadFile(path)
	if err != nil {
		return fmt.Errorf("seed: %s", docs.KindIO)
	}
	if err := json.Unmarshal(raw, v); err != nil {
		return fmt.Errorf("seed: %s", docs.KindNotJSON)
	}
	return nil
}
