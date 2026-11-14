// Package testkit holds the helpers that the tests of more than one package of the observer use.
// The helpers are a script that stands for the strata binary, a gzip file, a cluster read, and the
// documents that the commands promise. They also read a JSON file and the documents of the real
// binary, and they encode a value. They are also a wait, a count of the calls that run at one time,
// and a webhook that keeps each request. The package also holds the values of the test data that the
// tests of more than one package use. Only test files import it.
package testkit

import (
	"bytes"
	"compress/gzip"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
)

// The smallest document that a command of a cycle promises, as one line of JSON.
const (
	// VerifyUnchanged is the answer of `verify` for one record, record-0.json, that did not change.
	VerifyUnchanged = `{"schema":"` + docs.VerifySummarySchema + `","tool":{"version":"0.29.0"},"records":[{"path":"record-0.json","status":"unchanged","name":"aws-stack","version":"1.1.0"}]}`
	// ClusterPayments is the snapshot of the cluster c1, named payments, that runs aws-stack@1.1.0.
	ClusterPayments = `{"schema":"` + docs.SnapshotSchema + `","source":{"cluster":{"uid":"c1","name":"payments","profiles":[{"name":"aws-stack","version":"1.1.0","type":"cluster"}]}}}`
	// CompareSame is the answer of `compare` for two sides that are the same.
	CompareSame = `{"schema":"` + docs.DiffSchema + `","same":true,"sameContent":true}`
)

// The values of the test data that the tests of more than one package hold. The tests of docs hold their own copy,
// because testkit imports docs.
const (
	// ClusterUID is the uid of a cluster: of quarry-1 in the dev data, and of the cluster of the tests of the seed.
	ClusterUID = "7b1f3c9e-5a42-4d8e-9a67-0c2e4f1a8b35"
	// AddonsProfile is the second profile that the cluster payments runs.
	AddonsProfile = "platform-addons@1.0.0"
	// RecordFile is the file of the record aws-stack@1.1.0.
	RecordFile = "aws-stack@1.1.0.json.gz"
	// ApprovedMeta is the meta file of records that a pipeline approved.
	ApprovedMeta = `{"baseline":"approved","revision":"4e1c7a2"}`
	// FredCompositions is the key of the pack that the move of barney-addons changes.
	FredCompositions = "addon/fred-compositions"
	// SyncedAt is the time of the last sync of a registry.
	SyncedAt = "2026-10-30T09:00:00Z"
	// Digest1 and Digest2 are two digests of an image.
	Digest1 = "sha256:1111111111111111111111111111111111111111111111111111111111111111"
	Digest2 = "sha256:2222222222222222222222222222222222222222222222222222222222222222"
	// VendorRef is the image of the pack VendorPack that moves to another digest.
	VendorRef  = "vendor.example/product:2026.2"
	VendorPack = "addon/vendor-product"
	// NoImage is an image that no pod runs.
	NoImage = "nobody.example/none:0"
	// ImageItem is the path of the first image of a pack, and ImagePath is the path of its name.
	ImageItem = "pack.content.images[0]"
	ImagePath = ImageItem + ".image"
	// The codes of Palette: CodeDependency refuses a move, CodeMinorSkip refuses a move that skips a minor version, and
	// CodeVariable refuses a required variable with no value.
	CodeDependency = "DependencyPackUpgradeNotSupported"
	CodeMinorSkip  = "K8sUpgradeNotSupported"
	CodeVariable   = "InvalidVariable"
	// AuditSentinel is in the reason of a read of the audit log that failed. No route and no log holds it.
	AuditSentinel = "AUDIT-REASON-SENTINEL"
	// AuditUnread is the message of an event when the observer could not read the audit log.
	AuditUnread = "The audit log could not be read, so this does not say who made the change."
	// DexAsk is the route of the question to Palette for the move of dex in the dev data.
	DexAsk = "/v1/packs/fred-identity@2.3.0/addon/dex/ask"
	// AsksRoute is the route of the list of answers of Palette. NoAnswers is the list of an observer that holds none, in
	// the JSON of the route. The list is empty, and not null.
	AsksRoute = "/v1/asks"
	NoAnswers = `"asks":[]`
	// NoErrorVerdict and RefusesVerdict are the verdicts of an answer of the kind no-error and refuses, in the JSON of an
	// answer.
	NoErrorVerdict = `"verdict":"no-error"`
	RefusesVerdict = `"verdict":"refuses"`
	// KubeHost and KubePort are the variables of the address of the API server that the kubelet gives a pod.
	KubeHost = "KUBERNETES_SERVICE_HOST"
	KubePort = "KUBERNETES_SERVICE_PORT"
	// RecordsScript is the script that writes the records of a cluster, from the directory of a package of internal/.
	RecordsScript = "../../../scripts/ci/observer-records.sh"
	// JSONType is the Content-Type of a document of JSON.
	JSONType = "application/json"
	// HeaderPolicy and HeaderNoSniff are two headers that the server sets on each answer.
	HeaderPolicy  = "Content-Security-Policy"
	HeaderNoSniff = "X-Content-Type-Options"
)

// PageTypes is the Content-Type of each kind of file that the server of the page serves.
var PageTypes = map[string]string{
	".html":  "text/html; charset=utf-8",
	".js":    "text/javascript; charset=utf-8",
	".css":   "text/css; charset=utf-8",
	".woff2": "font/woff2",
	".svg":   "image/svg+xml",
	".md":    "text/markdown; charset=utf-8",
}

// The documents of the real binary in the test data of internal/docs, which ReadDoc reads.
const (
	DriftSummary    = "verify-summary-drift.json"
	MoveKubernetes  = "move-kubernetes.json"
	MoveCertManager = "move-cert-manager.json"
	MoveCilium      = "move-cni-cilium-oss.json"
)

// The failures of the checks that the tests of more than one package make. A check that reads a type of model, api,
// runner, or seed stays in the tests of each package. testkit cannot import a package whose tests import testkit.
const (
	ProfileCounts      = "profile %q, counts %+v"
	ResultAndError     = "result %+v, error %v"
	ProfileCopy        = "profile %q, clusterCopy %+v"
	FindingsNotChecked = "findings %+v, notChecked %+v"
	FindingsWant       = "findings %v, want %v"
	// NoDiff is the failure of a search for the diff of a profile, a pack, and a side.
	NoDiff = "no diff for %s %s %s"
	// HeadersMissing is the failure of a check of the headers that the server sets on the answer for a file.
	HeadersMissing = "%s: the headers of the server are missing"
	// FirstCycles names the wait for the first cycle of each timer of the observer.
	FirstCycles = "the first cycle of each timer"
)

// AskFlow checks the click "Ask Palette" on the move of dex of the dev data ok. get sends a GET to a path of the
// server of the set. post sends a POST, with the header of the page when page is true. Each gives the status and the
// body. The list of answers is empty before the click. The server refuses a POST that the page did not send. The
// click gives no error with the newest version, a second click gives the same answer, and the list then holds it. The
// server finds no move with no reply, and no profile that is not there.
func AskFlow(t *testing.T, get func(path string) (int, string), post func(path string, page bool) (int, string)) {
	t.Helper()
	if code, body := get(AsksRoute); code != http.StatusOK || body != "{"+NoAnswers+"}\n" {
		t.Fatalf("the list before the click: %d %s", code, body)
	}
	if code, _ := post(DexAsk, false); code != http.StatusForbidden {
		t.Errorf("a POST that the page did not send gave %d", code)
	}
	code, body := post(DexAsk, true)
	if code != http.StatusOK || !strings.Contains(body, NoErrorVerdict) || !strings.Contains(body, `"newest":"2.44.0"`) {
		t.Fatalf("the click gave %d %s", code, body)
	}
	if again, bodyAgain := post(DexAsk, true); again != http.StatusOK || bodyAgain != body {
		t.Errorf("the second click gave %d %s", again, bodyAgain)
	}
	if code, list := get(AsksRoute); code != http.StatusOK || strings.Count(list, `"verdict"`) != 1 || !strings.Contains(list, strings.TrimSpace(body)) {
		t.Errorf("the list after the click: %d %s", code, list)
	}
	for _, path := range []string{"/v1/packs/dino-storage@1.1.0/addon/longhorn/ask", "/v1/packs/nobody@1.0.0/addon/dex/ask"} {
		if code, _ := post(path, true); code != http.StatusNotFound {
			t.Errorf("%s gave %d", path, code)
		}
	}
}

// OncallRules gives the text of a file of alert rules with one channel, oncall, whose address is in the file address.
// rules are the JSON objects of the list of rules.
func OncallRules(address, rules string) string {
	return `{"channels": {"oncall": {"urlFile": "` + address + `"}}, "rules": [` + rules + `]}`
}

// Unknown is the error of a fake of the strata binary for a command that the fake does not know.
func Unknown(args []string) error {
	return fmt.Errorf("the fake does not know %q", args[0])
}

// preamble starts each script of Strata. It sets cmd to the command, ref to the first argument after
// it, and out to the value of -o.
const preamble = `
cmd=$1
shift
ref=$1
out=
prev=
for a in "$@"; do
  if [ "$prev" = -o ]; then out=$a; fi
  prev=$a
done
`

// Script writes an executable script named strata to a new directory of the test, and gives its path.
// body is the shell code after the line that names the shell.
func Script(t *testing.T, body string) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "strata")
	if err := os.WriteFile(path, []byte("#!/bin/sh\n"+body+"\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	return path
}

// Strata writes a script that stands for the strata binary, and gives its path. body is the shell
// code after the lines that set $cmd to the command, $ref to the first argument after it, and $out to
// the value of -o. The arguments after the command stay in $1 and the next.
func Strata(t *testing.T, body string) string {
	t.Helper()
	return Script(t, preamble+body)
}

// WriteGzip writes body, compressed, to the file name of dir.
func WriteGzip(t *testing.T, dir, name, body string) {
	t.Helper()
	var buf bytes.Buffer
	w := gzip.NewWriter(&buf)
	if _, err := w.Write([]byte(body)); err != nil {
		t.Fatal(err)
	}
	if err := w.Close(); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, name), buf.Bytes(), 0o600); err != nil {
		t.Fatal(err)
	}
}

// Cluster makes the read of the cluster c1 that runs the given profiles, each as name@version. The
// cluster has an update pending.
func Cluster(name string, profiles ...string) docs.ClusterRead {
	var c docs.ClusterRead
	c.Source.Cluster.UID, c.Source.Cluster.Name, c.Source.Cluster.UpdatesPending = "c1", name, true
	for _, p := range profiles {
		n, v, _ := strings.Cut(p, "@")
		c.Source.Cluster.Profiles = append(c.Source.Cluster.Profiles, docs.ClusterProfile{Name: n, Version: v, Type: "cluster"})
	}
	return c
}

// DecodeFile reads the JSON file at path into v, and fails the test on an error.
func DecodeFile(t *testing.T, path string, v any) {
	t.Helper()
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(raw, v); err != nil {
		t.Fatalf("%s: %v", path, err)
	}
}

// ReadDoc reads the document name of the test data of internal/docs into v. The contract script copies
// each one there from the real binary. A test of a package of internal/ calls it.
func ReadDoc(t *testing.T, name string, v any) {
	t.Helper()
	DecodeFile(t, filepath.Join("..", "docs", "testdata", name), v)
}

// MustJSON encodes v, and fails the test on an error.
func MustJSON(t *testing.T, v any) []byte {
	t.Helper()
	raw, err := json.Marshal(v)
	if err != nil {
		t.Fatal(err)
	}
	return raw
}

// OutFile gives the file that a command writes: the argument after -o.
func OutFile(args []string) (string, error) {
	i := slices.Index(args, "-o")
	if i < 0 || i+1 >= len(args) {
		return "", errors.New("the command has no -o and a file")
	}
	return args[i+1], nil
}

// WaitLimit is the longest time that a test waits for an event.
const WaitLimit = 10 * time.Second

// WaitFor calls ready every 20 ms until it is true, and fails the test after WaitLimit. what names
// the event for the message of the failure.
func WaitFor(t *testing.T, what string, ready func() bool) {
	t.Helper()
	deadline := time.Now().Add(WaitLimit)
	for !ready() {
		if time.Now().After(deadline) {
			t.Fatalf("%s did not happen", what)
		}
		time.Sleep(20 * time.Millisecond)
	}
}
