package docs

import (
	"bytes"
	"encoding/json"
	"os"
	"strings"
	"testing"
)

const sentinel = "SENTINEL-MUST-NOT-LEAVE"

func read(t *testing.T, name string) []byte {
	t.Helper()
	b, err := os.ReadFile("testdata/" + name)
	if err != nil {
		t.Fatal(err)
	}
	return b
}

// plant writes the sentinel at a path of a generic copy of a document. A key of "*" takes each
// item of a list.
func plant(t *testing.T, node any, path []string, v any) int {
	t.Helper()
	n := 0
	switch cur := node.(type) {
	case map[string]any:
		if len(path) == 1 {
			cur[path[0]] = v
			return 1
		}
		next, ok := cur[path[0]]
		list, isList := next.([]any)
		switch {
		case !ok && path[1] == "*", isList && len(list) == 0:
			// A list that is absent or empty gets one item, so that the sentinel has a place to go.
			next = []any{map[string]any{}}
			cur[path[0]] = next
		case !ok:
			next = map[string]any{}
			cur[path[0]] = next
		}
		n += plant(t, next, path[1:], v)
	case []any:
		for _, item := range cur {
			n += plant(t, item, path[1:], v)
		}
	}
	return n
}

func TestFieldsOutsideTheAllowListDoNotSurvive(t *testing.T) {
	var doc map[string]any
	if err := json.Unmarshal(read(t, "verify-summary-drift.json"), &doc); err != nil {
		t.Fatal(err)
	}
	for _, p := range []string{
		"records.*.diff.a.cluster.variables",
		"records.*.diff.b.cluster.variables",
		"records.*.diff.clusterVariables.values",
		"records.*.diff.variables.diff",
		"records.*.diff.packs.*.manifests.*.content",
		"records.*.diff.packs.*.paramChanges.*.old",
		"records.*.diff.verify.who.*.userMsg",
		"records.*.clusterVariables",
	} {
		if plant(t, doc, strings.Split(p, "."), sentinel) == 0 {
			t.Fatalf("the test planted nothing at %s", p)
		}
	}
	raw, err := json.Marshal(doc)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Contains(raw, []byte(sentinel)) {
		t.Fatal("the test document holds no sentinel")
	}
	var got VerifySummary
	if err := json.Unmarshal(raw, &got); err != nil {
		t.Fatal(err)
	}
	out, err := json.Marshal(got)
	if err != nil {
		t.Fatal(err)
	}
	if bytes.Contains(out, []byte(sentinel)) {
		t.Fatalf("a field outside the allow-list survived: %s", out)
	}
}

func TestTheDriftDocumentDecodes(t *testing.T) {
	var got VerifySummary
	if err := json.Unmarshal(read(t, "verify-summary-drift.json"), &got); err != nil {
		t.Fatal(err)
	}
	if got.Schema != VerifySummarySchema || len(got.Records) != 1 {
		t.Fatalf("schema %q, %d records", got.Schema, len(got.Records))
	}
	r := got.Records[0]
	if r.Status != VerifyDrifted || r.Diff == nil || r.Diff.Verify == nil {
		t.Fatalf("status %q, diff %v", r.Status, r.Diff)
	}
	if len(r.Diff.Verify.Who) != 1 || r.Diff.Verify.Who[0].Actor == "" {
		t.Fatalf("who: %+v", r.Diff.Verify.Who)
	}
	var changed *Pack
	for i := range r.Diff.Packs {
		if r.Diff.Packs[i].Status == PackChanged {
			changed = &r.Diff.Packs[i]
		}
	}
	if changed == nil || !changed.Authored || changed.Values.Diff[0].Old != "Delete" {
		t.Fatalf("changed pack: %+v", changed)
	}
}

// The schema ids of the constants are those that the documents of the binary state. The contract test
// makes the same check on the documents of this commit.
func TestTheSchemaIdsAreThoseOfTheDocumentsOfTheBinary(t *testing.T) {
	for _, c := range []struct {
		file string
		doc  Document
		want string
	}{
		{"verify-summary-drift.json", &VerifySummary{}, VerifySummarySchema},
		{"behind.json", &Behind{}, BehindSchema},
		{"move-kubernetes.json", &Diff{}, DiffSchema},
	} {
		if err := json.Unmarshal(read(t, c.file), c.doc); err != nil {
			t.Fatal(err)
		}
		if got := c.doc.SchemaID(); got != c.want {
			t.Errorf("%s states %q, want %q", c.file, got, c.want)
		}
	}
}

func TestBehindDecodes(t *testing.T) {
	var got Behind
	if err := json.Unmarshal(read(t, "behind.json"), &got); err != nil {
		t.Fatal(err)
	}
	behind := 0
	for _, p := range got.Pins {
		if p.Behind {
			behind++
			if p.RegistryUID == "" || p.Newer[0].Listed == "" || p.PinnedBy[0].Subject == "" {
				t.Fatalf("pin: %+v", p)
			}
			// the record holds a profile version with a newer version, which carries the newest version of each pack (#709)
			if len(p.Carriers) != 1 || p.Carriers[0].Profile != "aws-stack" || p.Carriers[0].Carrier == nil ||
				p.Carriers[0].Carrier.PackVersion != p.Newer[0].Version {
				t.Fatalf("the carriers of %s: %+v", p.Name, p.Carriers)
			}
		}
	}
	if behind != 4 {
		t.Fatalf("%d pins behind", behind)
	}
	// the report holds a profile version with a newer published version, so the tests read a report of #707
	if len(got.Profiles) != 1 || !got.Profiles[0].Behind || len(got.Profiles[0].Newer) == 0 || got.Profiles[0].Newer[0].UID == "" {
		t.Fatalf("profiles: %+v", got.Profiles)
	}
}

func TestAVerdictWithAnUnreadAuditLogDecodesTheReason(t *testing.T) {
	const doc = `{"schema":"` + VerifySummarySchema + `","tool":{"version":"0.29.0"},"records":[{"path":"record-0.json","status":"drifted","name":"aws-stack","version":"1.1.0",
		"diff":{"schema":"` + DiffSchema + `","verify":{"status":"drifted","whoError":"HTTP 403: {\"code\":\"AccessDenied\"}"}}}]}`
	var got VerifySummary
	if err := json.Unmarshal([]byte(doc), &got); err != nil {
		t.Fatal(err)
	}
	r := got.Records[0]
	if r.Status != VerifyDrifted || r.Error != nil || r.Diff.Verify.WhoError != `HTTP 403: {"code":"AccessDenied"}` || len(r.Diff.Verify.Who) != 0 {
		t.Fatalf("row %+v, verify %+v", r, r.Diff.Verify)
	}
}

func TestCapTextDropsAllTheTextOfAPackAboveTheLimitAndKeepsTheOthers(t *testing.T) {
	d := Diff{Packs: []Pack{
		{Key: "k8s/big", Values: Values{Unified: strings.Repeat("v", 60)}, Manifests: []Manifest{{Op: "added", Name: "m", Unified: strings.Repeat("m", 41)}}},
		{Key: "k8s/exact", Values: Values{Unified: strings.Repeat("v", 60)}, Manifests: []Manifest{{Op: "added", Name: "m", Unified: strings.Repeat("m", 40)}}},
		{Key: "k8s/none"},
	}}
	d.CapText(100)
	big, exact, none := d.Packs[0], d.Packs[1], d.Packs[2]
	if !big.TextTooLarge || big.Values.Unified != "" || big.Manifests[0].Unified != "" || big.Manifests[0].Op != "added" || big.Manifests[0].Name != "m" {
		t.Errorf("a pack above the cap: %+v", big)
	}
	if exact.TextTooLarge || exact.Values.Unified == "" || exact.Manifests[0].Unified == "" {
		t.Errorf("a pack at the cap keeps its text: %+v", exact)
	}
	if none.TextTooLarge {
		t.Errorf("a pack with no text: %+v", none)
	}
}

func TestTextTooLargeIsNeverReadFromADocumentOrWrittenToOne(t *testing.T) {
	var p Pack
	if err := json.Unmarshal([]byte(`{"key":"k8s/x","TextTooLarge":true,"textTooLarge":true}`), &p); err != nil {
		t.Fatal(err)
	}
	if p.TextTooLarge {
		t.Error("a document set a field that only the observer sets")
	}
	p.TextTooLarge = true
	out, err := json.Marshal(p)
	if err != nil || strings.Contains(strings.ToLower(string(out)), "texttoolarge") {
		t.Errorf("%s %v", out, err)
	}
}

func TestStatusOfReadsTheStatusAtTheStartOfAReasonOnly(t *testing.T) {
	for _, c := range []struct {
		reason string
		want   int
	}{
		{"HTTP 403: SENTINEL body", 403},
		{"HTTP 500", 500},
		{"HTTP 4031", 0},
		{"HTTP 40", 0},
		{"the link failed: HTTP 403", 0},
		{"1 record of project p2 was not asked", 0},
		{"", 0},
	} {
		if got := StatusOf(c.reason); got != c.want {
			t.Errorf("StatusOf(%q) = %d, want %d", c.reason, got, c.want)
		}
	}
}
