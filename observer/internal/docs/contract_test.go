package docs

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"reflect"
	"strings"
	"testing"
)

// The contract test runs against the strata binary of this commit. scripts/observer-contract.sh
// sets STRATA_BIN and STRATA_DOCS. Without them, `go test` skips it, and it needs no network.

// notSet ends the reason of a skip of the contract test.
const notSet = " is not set: run scripts/observer-contract.sh"

// schemaProps finds the properties of a JSON Schema node. It follows $ref, anyOf, oneOf, and items.
func schemaProps(root, node map[string]any, out map[string]map[string]any) {
	if ref, ok := node["$ref"].(string); ok {
		name := strings.TrimPrefix(ref, "#/$defs/")
		if def, ok := root["$defs"].(map[string]any)[name].(map[string]any); ok {
			schemaProps(root, def, out)
		}
	}
	for _, key := range []string{"anyOf", "oneOf", "allOf"} {
		if list, ok := node[key].([]any); ok {
			for _, item := range list {
				schemaProps(root, item.(map[string]any), out)
			}
		}
	}
	if items, ok := node["items"].(map[string]any); ok {
		schemaProps(root, items, out)
	}
	if props, ok := node["properties"].(map[string]any); ok {
		for k, v := range props {
			out[k] = v.(map[string]any)
		}
	}
}

// missing lists each JSON name of a Go type that the schema does not have.
func missing(root, node map[string]any, t reflect.Type, path string, seen map[reflect.Type]bool) []string {
	for t.Kind() == reflect.Pointer || t.Kind() == reflect.Slice {
		t = t.Elem()
	}
	if t.Kind() != reflect.Struct || seen[t] {
		return nil
	}
	seen[t] = true
	defer delete(seen, t)
	props := map[string]map[string]any{}
	schemaProps(root, node, props)
	var out []string
	for i := range t.NumField() {
		f := t.Field(i)
		name, _, _ := strings.Cut(f.Tag.Get("json"), ",")
		child, ok := props[name]
		// A field that the observer sets itself is not a field of the document.
		if name == "-" {
			continue
		}
		// The fields of an error other than kind and message belong to the kind, so the schema does
		// not name them. See docs/src/reference/errors.md.
		if !ok && name == "status" && strings.HasSuffix(path, ".error") {
			continue
		}
		if !ok {
			out = append(out, path+"."+name)
			continue
		}
		out = append(out, missing(root, child, f.Type, path+"."+name, seen)...)
	}
	return out
}

func TestEachAllowedFieldIsInTheSchemaOfTheBinary(t *testing.T) {
	bin := contractEnv(t, "STRATA_BIN")
	for _, c := range []struct {
		kind string
		typ  any
	}{
		{"verify-summary", VerifySummary{}},
		{"diff", Diff{}},
		{"behind", Behind{}},
		{"snapshot", Record{}},
		{"snapshot", ClusterRead{}},
		{"list", ClusterList{}},
	} {
		out, err := exec.Command(bin, "schema", c.kind).Output()
		if err != nil {
			t.Fatalf("strata schema %s: %v", c.kind, err)
		}
		var root map[string]any
		decode(t, "strata schema "+c.kind, out, &root)
		for _, m := range missing(root, root, reflect.TypeOf(c.typ), c.kind, map[reflect.Type]bool{}) {
			t.Errorf("not in the schema: %s", m)
		}
	}
}

// contractDir gives the directory of the documents of the real binary that the contract script sets in STRATA_DOCS.
// With no directory, the test skips.
func contractDir(t *testing.T) string {
	t.Helper()
	return contractEnv(t, "STRATA_DOCS")
}

// contractEnv gives the variable name that the contract script sets. When it is not set, the test skips.
func contractEnv(t *testing.T, name string) string {
	t.Helper()
	value := os.Getenv(name)
	if value == "" {
		t.Skip(name + notSet)
	}
	return value
}

// decodeFile reads the JSON file name of dir into v. It does the job of testkit.DecodeFile for the tests of docs, because
// testkit imports docs.
func decodeFile(t *testing.T, dir, name string, v any) {
	t.Helper()
	b, err := os.ReadFile(dir + "/" + name)
	if err != nil {
		t.Fatal(err)
	}
	decode(t, name, b, v)
}

// A cluster side holds the value of a variable in clear. This test fails with no such value, so it
// cannot pass by an empty document.
func TestAClearValueDoesNotPassTheAllowList(t *testing.T) {
	dir := contractDir(t)
	const value = "team-payments"
	const name = "compare-values.json"
	var raw json.RawMessage
	decodeFile(t, dir, name, &raw)
	if !strings.Contains(string(raw), value) {
		t.Fatal("the document holds no clear value, so this test proves nothing")
	}
	var d Diff
	decode(t, name, raw, &d)
	holdsNone(t, "the diff after the allow-list", string(encode(t, d)), value)
}

func TestTheKindOfAFailedCheckIsInTheRow(t *testing.T) {
	dir := contractDir(t)
	var down VerifySummary
	decodeFile(t, dir, "verify-summary-down.json", &down)
	if len(down.Records) != 1 || down.Records[0].Status != VerifyError || down.Records[0].Error == nil || down.Records[0].Error.Kind != KindTransport {
		t.Fatalf("records: %+v", down.Records)
	}
}

// skipFault gives the fault of the answer to the question about a move that skips a minor version, or "" when it has
// none. The answer has one pin with a bump, and the bump holds the check of the skip with the next step.
func skipFault(skip Behind) string {
	if len(skip.Pins) != 1 || skip.Pins[0].Bump == nil || skip.Pins[0].Bump.UpdateChecks == nil {
		return fmt.Sprintf("the question about the skip gives %d pins, and the first has no bump with checks: %+v", len(skip.Pins), skip.Pins)
	}
	pin := skip.Pins[0]
	if s := pin.Bump.UpdateChecks.SkippedMinor; s == nil || s.From != pin.Version || s.To != pin.Bump.To || s.NextMinor == "" || s.NextStep == "" {
		return fmt.Sprintf("the bump of the skip holds the check %+v", s)
	}
	return ""
}

// The check of the skip gives a fault, and no panic, for each part of the answer that is missing. It gives none for a
// full answer.
func TestTheCheckOfTheSkipGivesAFaultForEachPartThatIsMissing(t *testing.T) {
	full := MinorSkip{From: "1.33.1", To: "1.35.0", NextMinor: "1.34", NextStep: "1.34.2"}
	// pinWith gives an answer with one pin of the version full.From, with the bump.
	pinWith := func(bump *Bump) Behind { return Behind{Pins: []Pin{{Version: full.From, Bump: bump}}} }
	for _, c := range []struct {
		name  string
		skip  Behind
		fault bool
	}{
		{"no pin", Behind{}, true},
		{"no bump", pinWith(nil), true},
		{"a bump with no checks", pinWith(&Bump{To: full.To}), true},
		{"checks with no skip", pinWith(&Bump{To: full.To, UpdateChecks: &UpdateChecks{}}), true},
		{"a skip with no next step", pinWith(&Bump{To: full.To, UpdateChecks: &UpdateChecks{SkippedMinor: &MinorSkip{From: full.From, To: full.To, NextMinor: full.NextMinor}}}), true},
		{"a full skip", pinWith(&Bump{To: full.To, UpdateChecks: &UpdateChecks{SkippedMinor: &full}}), false},
	} {
		t.Run(c.name, func(t *testing.T) {
			if fault := skipFault(c.skip); (fault != "") != c.fault {
				t.Errorf("the fault %q, want one %v", fault, c.fault)
			}
		})
	}
}

func TestTheDocumentsOfTheBinaryDecode(t *testing.T) {
	dir := contractDir(t)
	var drift VerifySummary
	decodeFile(t, dir, driftSummary, &drift)
	if drift.Schema != VerifySummarySchema {
		t.Errorf("verify states %q", drift.Schema)
	}
	if len(drift.Records) == 0 || drift.Records[0].Status != VerifyDrifted || drift.Records[0].Diff.Verify.Who == nil {
		t.Errorf("drift: %+v", drift.Records)
	}
	var behind Behind
	decodeFile(t, dir, behindFile, &behind)
	if behind.Schema != BehindSchema {
		t.Errorf("behind states %q", behind.Schema)
	}
	if len(behind.Pins) == 0 {
		t.Error("behind has no pin")
	}
	var move Diff
	decodeFile(t, dir, "move-cni-cilium-oss.json", &move)
	if move.Schema != DiffSchema {
		t.Errorf("compare states %q", move.Schema)
	}
	if len(move.Packs) != 1 || len(move.Packs[0].Values.Diff) == 0 {
		t.Errorf("the compare of a move holds %d packs, and the first holds no changed value", len(move.Packs))
	}
	// The observer runs the compare of a move with --full: the text of the values comes with it.
	if text := move.Packs[0].Values.Unified; !strings.HasPrefix(text, "--- a: ") || !strings.Contains(text, "\n@@ ") {
		t.Errorf("the compare of a move holds no unified text of the values: %q", text)
	}
	var ask Behind
	decodeFile(t, dir, "ask-behind.json", &ask)
	if len(ask.Pins) != 1 || ask.Pins[0].Bump == nil || ask.Pins[0].Bump.To == "" {
		t.Errorf("the question about one move gives %d pins, and the first has no bump: %+v", len(ask.Pins), ask.Pins)
	}
	// The question about a move of Kubernetes that skips a minor version: the bump holds the check of strata, with
	// the next step (#867).
	var skip Behind
	decodeFile(t, dir, "ask-skip.json", &skip)
	if fault := skipFault(skip); fault != "" {
		t.Error(fault)
	}
	// The compare of a record with the newest version of its profile, as the versions cycle runs it (#897). The move adds
	// a required variable, and the compare holds the check of strata.
	var profile Diff
	decodeFile(t, dir, "profile-move.json", &profile)
	if c := profile.UpdateChecks; profile.Schema != DiffSchema || c == nil || c.RequiredVariables == nil || c.RequiredVariables.Code == "" || len(c.RequiredVariables.Names) == 0 {
		t.Errorf("the compare of the move of a profile holds the checks %+v", c)
	}
	for _, name := range []string{"compare-payments-aws-stack.json", "compare-payments-platform-addons.json"} {
		var d Diff
		decodeFile(t, dir, name, &d)
		if d.Schema != DiffSchema {
			t.Errorf("%s states %q", name, d.Schema)
		}
		if !d.Same {
			t.Errorf("%s: the two forms of the cluster side differ: rootB %s", name, d.RootB)
		}
	}
	var c ClusterRead
	decodeFile(t, dir, "cluster-payments.json", &c)
	if c.Schema != SnapshotSchema {
		t.Errorf("the snapshot of a cluster states %q", c.Schema)
	}
	// the seed gives payments the release and three add-on profiles
	if len(c.Source.Cluster.Profiles) != 4 {
		t.Errorf("payments runs %d profiles", len(c.Source.Cluster.Profiles))
	}
	var r Record
	decodeFile(t, dir, "record-aws-stack.json", &r)
	if r.Source.Name != "aws-stack" {
		t.Errorf("record: %+v", r.Source)
	}
}
