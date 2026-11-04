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
	if len(down.Records) != 1 || down.Records[0].Status != "error" || down.Records[0].Error == nil || down.Records[0].Error.Kind != "transport" {
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
	if len(drift.Records) == 0 || drift.Records[0].Status != "drifted" || drift.Records[0].Diff.Verify.Who == nil {
		t.Errorf("drift: %+v", drift.Records)
	}
	var behind Behind
	decodeFile(t, dir, "behind.json", &behind)
	if len(behind.Pins) == 0 {
		t.Error("behind has no pin")
	}
	var move Diff
	decodeFile(t, dir, "move-cni-cilium-oss.json", &move)
	if got := FoldOf(move.Packs[0].Values); got.Settings+got.Follows+got.Repeats != len(move.Packs[0].Values.Diff) {
		t.Errorf("fold: %+v", got)
	}
	for _, name := range []string{"compare-payments-aws-stack.json", "compare-payments-platform-addons.json"} {
		var d Diff
		decodeFile(t, dir, name, &d)
		if !d.Same {
			t.Errorf("%s: the two forms of the cluster side differ: rootA %s, rootB %s", name, d.RootA, d.RootB)
		}
	}
	var c ClusterRead
	decodeFile(t, dir, "cluster-payments.json", &c)
	if len(c.Source.Cluster.Profiles) != 2 {
		t.Errorf("payments runs %d profiles", len(c.Source.Cluster.Profiles))
	}
	var r Record
	decodeFile(t, dir, "record-aws-stack.json", &r)
	if r.Root == "" || r.Source.Name != "aws-stack" {
		t.Errorf("record: %+v", r.Source)
	}
}
