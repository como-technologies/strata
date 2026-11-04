// Package baseline reads the records that the observer compares the cluster with.
//
// The directory is a mounted Secret. The kubelet updates it, so a cycle reads it again each time.
// A file that the observer cannot use is named in Set.Unreadable, and it does not stop the others.
package baseline

import (
	"bytes"
	"compress/gzip"
	"encoding/json"
	"errors"
	"io"
	"os"
	"path/filepath"
	"strings"

	"github.com/como-technologies/strata/observer/internal/docs"
)

// The values of Meta.Baseline.
const (
	Approved  = "approved"
	FirstSeen = "first-seen"
	Unknown   = "unknown"
)

// recordSuffix ends the name of each record file.
const recordSuffix = ".json.gz"

// metaFile names the file that says where the records come from.
const metaFile = "meta.json"

// maxRecordBytes limits a record after the unzip, so that a bad file cannot fill the memory.
const maxRecordBytes = 64 << 20

// Meta says where the records come from. Baseline is approved, first-seen, or unknown.
type Meta struct {
	Baseline      string `json:"baseline"`
	Revision      string `json:"revision,omitempty"`
	FirstSeenFrom string `json:"firstSeenFrom,omitempty"`
}

// Entry is one record. Raw is the file after the unzip, for the strata process of a cycle.
type Entry struct {
	File   string
	Record docs.Record
	Raw    []byte
}

// Set is the content of the directory.
type Set struct {
	Meta    Meta
	Entries []Entry
	// Unreadable names each file that is no record of strata-snapshot/1 for a profile.
	Unreadable []string
}

// Files counts the record files, also the ones that the observer cannot use.
func (s Set) Files() int {
	return len(s.Entries) + len(s.Unreadable)
}

// Load reads each record file of dir, and meta.json when it is there. It fails only when dir cannot
// be listed. The names come in the order of the file system listing, which is sorted.
func Load(dir string) (Set, error) {
	names, err := os.ReadDir(dir)
	if err != nil {
		return Set{}, err
	}
	set := Set{Meta: readMeta(dir)}
	for _, n := range names {
		if !strings.HasSuffix(n.Name(), recordSuffix) {
			continue
		}
		entry, err := readRecord(filepath.Join(dir, n.Name()))
		if err != nil {
			set.Unreadable = append(set.Unreadable, n.Name())
			continue
		}
		entry.File = n.Name()
		set.Entries = append(set.Entries, entry)
	}
	return set, nil
}

var errNotARecord = errors.New("baseline: not a profile record of strata-snapshot/1")

func readRecord(path string) (Entry, error) {
	f, err := os.Open(path)
	if err != nil {
		return Entry{}, err
	}
	defer f.Close()
	zr, err := gzip.NewReader(f)
	if err != nil {
		return Entry{}, err
	}
	raw, err := io.ReadAll(io.LimitReader(zr, maxRecordBytes+1))
	if err != nil {
		return Entry{}, err
	}
	if len(raw) > maxRecordBytes {
		return Entry{}, errNotARecord
	}
	var r docs.Record
	if err := json.NewDecoder(bytes.NewReader(raw)).Decode(&r); err != nil {
		return Entry{}, err
	}
	if r.Schema != docs.SnapshotSchema || r.Source.Cluster != nil {
		return Entry{}, errNotARecord
	}
	return Entry{Record: r, Raw: raw}, nil
}

// readMeta gives Unknown when the file is absent, is not JSON, or names another baseline.
func readMeta(dir string) Meta {
	raw, err := os.ReadFile(filepath.Join(dir, metaFile))
	if err != nil {
		return Meta{Baseline: Unknown}
	}
	var m Meta
	if err := json.Unmarshal(raw, &m); err != nil || (m.Baseline != Approved && m.Baseline != FirstSeen) {
		return Meta{Baseline: Unknown}
	}
	return m
}
