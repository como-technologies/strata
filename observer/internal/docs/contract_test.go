package docs

import (
	"encoding/json"
	"os"
	"os/exec"
	"reflect"
	"strings"
	"testing"
)

// The contract test runs against the strata binary of this commit. scripts/observer-contract.sh
// sets STRATA_BIN and STRATA_DOCS. Without them, `go test` skips it, and it needs no network.

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
	bin := os.Getenv("STRATA_BIN")
	if bin == "" {
		t.Skip("STRATA_BIN is not set: run scripts/observer-contract.sh")
	}
	for _, c := range []struct {
		kind string
		typ  any
	}{
		{"verify-summary", VerifySummary{}},
		{"diff", Diff{}},
		{"behind", Behind{}},
		{"snapshot", Record{}},
		{"snapshot", ClusterRead{}},
	} {
		out, err := exec.Command(bin, "schema", c.kind).Output()
		if err != nil {
			t.Fatalf("strata schema %s: %v", c.kind, err)
		}
		var root map[string]any
		if err := json.Unmarshal(out, &root); err != nil {
			t.Fatal(err)
		}
		for _, m := range missing(root, root, reflect.TypeOf(c.typ), c.kind, map[reflect.Type]bool{}) {
			t.Errorf("not in the schema: %s", m)
		}
	}
}

func decodeFile(t *testing.T, dir, name string, v any) {
	t.Helper()
	b, err := os.ReadFile(dir + "/" + name)
	if err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(b, v); err != nil {
		t.Fatalf("%s: %v", name, err)
	}
}

// A cluster side holds the value of a variable in clear. This test fails with no such value, so it
// cannot pass by an empty document.
func TestAClearValueDoesNotPassTheAllowList(t *testing.T) {
	dir := os.Getenv("STRATA_DOCS")
	if dir == "" {
		t.Skip("STRATA_DOCS is not set: run scripts/observer-contract.sh")
	}
	const value = "team-payments"
	raw, err := os.ReadFile(dir + "/compare-values.json")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(raw), value) {
		t.Fatal("the document holds no clear value, so this test proves nothing")
	}
	var d Diff
	if err := json.Unmarshal(raw, &d); err != nil {
		t.Fatal(err)
	}
	out, err := json.Marshal(d)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(out), value) {
		t.Fatalf("the clear value passed the allow-list: %s", out)
	}
}

func TestTheKindOfAFailedCheckIsInTheRow(t *testing.T) {
	dir := os.Getenv("STRATA_DOCS")
	if dir == "" {
		t.Skip("STRATA_DOCS is not set: run scripts/observer-contract.sh")
	}
	var down VerifySummary
	decodeFile(t, dir, "verify-summary-down.json", &down)
	if len(down.Records) != 1 || down.Records[0].Status != VerifyError || down.Records[0].Error == nil || down.Records[0].Error.Kind != KindTransport {
		t.Fatalf("records: %+v", down.Records)
	}
}

func TestTheDocumentsOfTheBinaryDecode(t *testing.T) {
	dir := os.Getenv("STRATA_DOCS")
	if dir == "" {
		t.Skip("STRATA_DOCS is not set: run scripts/observer-contract.sh")
	}
	var drift VerifySummary
	decodeFile(t, dir, "verify-summary-drift.json", &drift)
	if drift.Schema != VerifySummarySchema {
		t.Errorf("verify states %q", drift.Schema)
	}
	if len(drift.Records) == 0 || drift.Records[0].Status != VerifyDrifted || drift.Records[0].Diff.Verify.Who == nil {
		t.Errorf("drift: %+v", drift.Records)
	}
	var behind Behind
	decodeFile(t, dir, "behind.json", &behind)
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
