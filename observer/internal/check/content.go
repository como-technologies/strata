package check

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"sync"

	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
)

// The exits of a strata command that are an answer. `verify` and `behind` exit 2 with a document
// when a row has an error. The others give a document with 0 or 1, or write a file with 0.
var (
	exitsWithDocument = []int{0, 1, 2}
	exitsOfACompare   = []int{0, 1}
	exitsOfAWrite     = []int{0}
)

// failure makes the error of a cycle: a kind and a status, and never a message.
func failure(kind string) error {
	return &runner.RunError{Kind: kind}
}

// withCycleDir makes a directory under work, with mode 0700, and removes it when fn ends, also with
// an error. A cluster side holds the clear values of the variables, so work is a volume in memory.
func (o *Observer) withCycleDir(fn func(dir string) error) error {
	dir, err := os.MkdirTemp(o.cfg.Work, cyclePrefix)
	if err != nil {
		return failure(docs.KindIO)
	}
	defer func() {
		if err := os.RemoveAll(dir); err != nil {
			o.log.Error("the directory of a cycle stays", "kind", docs.KindIO)
		}
	}()
	return fn(dir)
}

// writeRecords writes each record to the directory of the cycle, after the unzip, and gives the
// file names in the order of the entries.
func writeRecords(dir string, entries []baseline.Entry) ([]string, error) {
	names := make([]string, 0, len(entries))
	for i, e := range entries {
		name := fmt.Sprintf("record-%d.json", i)
		if err := os.WriteFile(filepath.Join(dir, name), e.Raw, 0o600); err != nil {
			return nil, failure(docs.KindIO)
		}
		names = append(names, name)
	}
	return names, nil
}

// decode reads a document of strata into an allow-listed type. A document that does not decode is
// of the kind unknown, because it is not the document that the command promises. A document that
// states another schema id, or none, is of the kind schema: `--strata` can name another binary, and
// the fields of its document would not mean what the observer reads.
func decode[T docs.Document](raw []byte, schema string) (T, error) {
	var doc T
	if err := json.Unmarshal(raw, &doc); err != nil {
		return doc, failure(docs.KindUnknown)
	}
	if doc.SchemaID() != schema {
		var none T
		return none, failure(docs.KindSchema)
	}
	return doc, nil
}

// rowsByPath maps the rows of `verify` to the files that it read.
func rowsByPath(doc docs.VerifySummary) map[string]docs.VerifyRow {
	rows := make(map[string]docs.VerifyRow, len(doc.Records))
	for _, r := range doc.Records {
		rows[r.Path] = r
	}
	return rows
}

// firstRowError gives the first error of a row, or nil.
func firstRowError(rows []docs.VerifyRow) *docs.Error {
	for _, r := range rows {
		if r.Error != nil {
			return r.Error
		}
	}
	return nil
}

// allRowsFailed says that each row has an error. A row with an error beside good rows is a cause
// in "Not checked", and not a failed cycle.
func allRowsFailed(rows []docs.VerifyRow) bool {
	return len(rows) > 0 && !slices.ContainsFunc(rows, func(r docs.VerifyRow) bool { return r.Error == nil })
}

// kindOfARowError gives the error of a failed command with the kind of a row of `verify`. A
// `snapshot` writes a plain line, so its kind is unknown, and the rows know the cause.
func kindOfARowError(err error, rows []docs.VerifyRow) error {
	var re *runner.RunError
	if !errors.As(err, &re) || re.Kind != docs.KindUnknown {
		return err
	}
	if rowErr := firstRowError(rows); rowErr != nil {
		return &runner.RunError{Kind: rowErr.Kind, Status: rowErr.Status}
	}
	return err
}

// contentCycle runs one content cycle. It runs `verify` on the records and reads the cluster. Then it
// compares each record that the cluster runs with its side of the cluster. files is -1 when the
// directory was not read.
func (o *Observer) contentCycle(ctx context.Context) (content model.Content, files int, err error) {
	set, err := baseline.Load(o.cfg.Records)
	if err != nil {
		return content, -1, failure(docs.KindIO)
	}
	files = set.Files()
	set.Entries = append(set.Entries, o.keptFirstSeen(set)...)
	err = o.withCycleDir(func(dir string) error {
		if err := o.readContent(ctx, dir, set, &content); err != nil {
			return err
		}
		o.recordNewProfiles(ctx, dir, set, content.Cluster)
		return nil
	})
	return content, files, err
}

func (o *Observer) readContent(ctx context.Context, dir string, set baseline.Set, content *model.Content) error {
	content.Meta, content.Unreadable = set.Meta, set.Unreadable
	o.readPods(ctx, content)
	names, err := writeRecords(dir, set.Entries)
	if err != nil {
		return err
	}
	var doc docs.VerifySummary
	o.expect(ctx, 1) // the read of the cluster
	if len(names) > 0 {
		o.expect(ctx, 1)
		if doc, err = o.verify(ctx, dir, names); err != nil {
			return err
		}
	}
	content.Strata = doc.Tool.Version
	read, err := o.readCluster(ctx, dir)
	if err != nil {
		return kindOfARowError(err, doc.Records)
	}
	content.Cluster = read
	content.Records, err = o.compareRecords(ctx, dir, set.Entries, names, rowsByPath(doc), read)
	content.At = o.now()
	return err
}

func (o *Observer) verify(ctx context.Context, dir string, names []string) (docs.VerifySummary, error) {
	args := append([]string{runner.CmdVerify}, names...)
	args = append(args, "--summary", "--who", runner.FlagFull, "--json", "--compact", "-q")
	res, err := o.run(ctx, dir, exitsWithDocument, args...)
	if err != nil {
		return docs.VerifySummary{}, err
	}
	doc, err := decode[docs.VerifySummary](res.Stdout, docs.VerifySummarySchema)
	if err != nil {
		return doc, err
	}
	for _, row := range doc.Records {
		if row.Diff != nil {
			row.Diff.CapText(maxText)
		}
	}
	if allRowsFailed(doc.Records) {
		e := firstRowError(doc.Records)
		return doc, &runner.RunError{Kind: e.Kind, Status: e.Status}
	}
	o.logUnreadAudits(doc.Records)
	return doc, nil
}

// noAnswer is what the log says when the reason of strata holds no status.
const noAnswer = "no answer"

// unreadReason gives the part of the reason of strata that a log line can hold: `HTTP <status>`, or
// `no answer`. The body of an answer of Palette and the message of a failed request are output of a
// process, so they never leave this function.
func unreadReason(whoError string) string {
	if status := docs.StatusOf(whoError); status != 0 {
		return fmt.Sprintf("HTTP %d", status)
	}
	return noAnswer
}

// logUnreadAudits writes one line for each record whose audit log strata could not read, such as for a
// key without the permission. The verdict of the record stays. The summary carries a flag and not
// the reason. The line names the profile and the status, and never the body of the answer.
func (o *Observer) logUnreadAudits(rows []docs.VerifyRow) {
	for _, r := range rows {
		if r.Diff != nil && r.Diff.Verify != nil && r.Diff.Verify.WhoError != "" {
			o.log.Warn("the audit log could not be read", "profile", model.Subject(r.Name, r.Version), "reason", unreadReason(r.Diff.Verify.WhoError))
		}
	}
}

// readCluster takes the cluster in a snapshot, with the raw answers of Palette in raw/ for the sides.
func (o *Observer) readCluster(ctx context.Context, dir string) (docs.ClusterRead, error) {
	if _, err := o.run(ctx, dir, exitsOfAWrite, runner.CmdSnapshot, "cluster:"+o.cfg.ClusterUID, "--raw-dir", "raw", "-o", "cluster.json", "-q"); err != nil {
		return docs.ClusterRead{}, err
	}
	raw, err := os.ReadFile(filepath.Join(dir, "cluster.json"))
	if err != nil {
		return docs.ClusterRead{}, failure(docs.KindIO)
	}
	return decode[docs.ClusterRead](raw, docs.SnapshotSchema)
}

// runsProfile says that the cluster runs the version of the profile of a record.
func runsProfile(read docs.ClusterRead, r docs.Record) bool {
	return slices.ContainsFunc(read.Source.Cluster.Profiles, func(p docs.ClusterProfile) bool {
		return p.Name == r.Source.Name && p.Version == r.Source.Version
	})
}

// compareRecords gives each record its `verify` row and, when the cluster runs it, its `compare` with
// the cluster side. The side comes from the raw answers, so it makes no request. The records compare
// at the same time, as many as the pool of processes lets, and keep their order. When a side or a
// compare fails, the record gets no Diff and the page says "not read". When all of them fail, the
// cycle fails, because the cluster side is then not readable at all.
func (o *Observer) compareRecords(ctx context.Context, dir string, entries []baseline.Entry, names []string, rows map[string]docs.VerifyRow, read docs.ClusterRead) ([]model.Record, error) {
	out := make([]model.Record, len(entries))
	errs := make([]error, len(entries))
	var wg sync.WaitGroup
	for i, e := range entries {
		out[i] = model.Record{File: e.File, Doc: e.Record}
		if row, ok := rows[names[i]]; ok {
			out[i].Row = &row
		}
		if out[i].Cluster.Runs = runsProfile(read, e.Record); out[i].Cluster.Runs {
			o.expect(ctx, 2) // the side, and its compare
			wg.Add(1)
			go func() {
				defer wg.Done()
				out[i].Cluster.Diff, errs[i] = o.compareSide(ctx, dir, names[i], i, e.Record.Source.Name)
			}()
		}
	}
	wg.Wait()
	var attempted, failed int
	var first error
	for i := range out {
		if !out[i].Cluster.Runs {
			continue
		}
		attempted++
		if errs[i] != nil {
			failed++
			if first == nil {
				first = errs[i]
			}
		}
	}
	if attempted > 0 && failed == attempted {
		return nil, first
	}
	return out, nil
}

// compareSide reads the side of one profile from the raw answers and compares the record with it.
func (o *Observer) compareSide(ctx context.Context, dir, record string, index int, profile string) (*docs.Diff, error) {
	side := fmt.Sprintf("side-%d.json", index)
	if _, err := o.run(ctx, dir, exitsOfAWrite, runner.CmdSnapshot, "cluster:"+o.cfg.ClusterUID+"/"+profile, "--from-raw", "raw", "-o", side, "-q"); err != nil {
		return nil, err
	}
	diff, err := o.compareFull(ctx, dir, record, side)
	if err != nil {
		return nil, err
	}
	return &diff, nil
}

// compareFull compares the refs a and b with `--full`, and caps the text of each pack at maxText.
func (o *Observer) compareFull(ctx context.Context, dir, a, b string) (docs.Diff, error) {
	res, err := o.run(ctx, dir, exitsOfACompare, runner.CmdCompare, a, b, runner.FlagFull, "--json", "--compact", "-q")
	if err != nil {
		return docs.Diff{}, err
	}
	diff, err := decode[docs.Diff](res.Stdout, docs.DiffSchema)
	diff.CapText(maxText)
	return diff, err
}
