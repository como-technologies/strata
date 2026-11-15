// Package testkit holds the helpers that the tests of more than one package of the observer use.
// The helpers are a script that stands for the strata binary, a gzip file, a cluster read, and the
// documents that the commands promise. They also read a JSON file and the documents of the real
// binary, and they encode a value. They are also a wait, a count of the calls that run at one time,
// and a webhook that keeps each request. Other helpers write a file of alert rules, a record, a meta
// file, and the parts of a script of a fake of strata. The package also holds the values of the test
// data and the messages of a failure that the tests of more than one package use. Only test files
// import it.
package testkit

import (
	"bytes"
	"compress/gzip"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"reflect"
	"regexp"
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
	// ClusterHead starts the snapshot of a cluster, before its uid.
	ClusterHead = `{"schema":"` + docs.SnapshotSchema + `","source":{"cluster":{"uid":"`
	// CompareSame is the answer of `compare` for two sides that are the same.
	CompareSame = `{"schema":"` + docs.DiffSchema + `","same":true,"sameContent":true}`
	// KubernetesBehind is the answer of `behind` for the record of aws-stack@1.1.0. Its pack kubernetes 1.35.3 is behind
	// 1.36.2.
	KubernetesBehind = `{"schema":"` + docs.BehindSchema + `","tool":{"version":"0.29.0"},"pins":[{"name":"kubernetes","layer":"k8s","registry":"Public Repo","registryUid":"reg1","version":"1.35.3","tag":"1.35.3","pinnedBy":[{"subject":"` + AWSStackProfile + `"}],"newer":[{"version":"1.36.2","listed":"2026-11-01T00:00:00Z","drops":[]}],"behind":true}]}`
)

// ClusterPayments is the snapshot of the cluster payments, that runs aws-stack@1.1.0.
var ClusterPayments = ClusterJSON(PaymentsUID, PaymentsName, ProfileJSON("aws-stack", "1.1.0", "cluster"))

// ClusterJSON gives the snapshot of the cluster uid, named name, that runs the profiles, each from ProfileJSON.
func ClusterJSON(uid, name string, profiles ...string) string {
	return ClusterHead + uid + `","name":"` + name + `","profiles":[` + strings.Join(profiles, ",") + `]}}}`
}

// ProfileJSON gives a profile that a snapshot of a cluster names, with its type, such as cluster or add-on.
func ProfileJSON(name, version, typ string) string {
	return `{"name":"` + name + `","version":"` + version + `","type":"` + typ + `"}`
}

// The values of the test data that the tests of more than one package hold. The tests of docs hold their own copy,
// because testkit imports docs.
const (
	// ClusterUID is the uid of a cluster: of quarry-1 in the dev data, and of the cluster of the tests of the seed.
	ClusterUID = "7b1f3c9e-5a42-4d8e-9a67-0c2e4f1a8b35"
	// PaymentsName and PaymentsUID are the name and the uid of the cluster payments, whose read Payments gives.
	// AWSStackProfile is the profile that it runs, and AddonsProfile is its second profile in some tests.
	PaymentsName    = "payments"
	PaymentsUID     = "c1"
	AWSStackProfile = "aws-stack@1.1.0"
	AddonsProfile   = "platform-addons@1.0.0"
	// EdgeProfile is a profile that a cluster runs and that no record holds.
	EdgeProfile = "fred-edge@1.0.0"
	// RecordFile is the file of the record of AWSStackProfile, which AWSStack gives. MetaFile is the meta file of the
	// records, which WriteMeta writes.
	RecordFile = AWSStackProfile + ".json.gz"
	MetaFile   = "meta.json"
	// ContentEvery and VersionsEvery are the periods of the content cycle and of the versions cycle of an observer of a
	// test. DigestEvery is the period of the digest of its alerts.
	ContentEvery  = 15 * time.Minute
	VersionsEvery = 6 * time.Hour
	DigestEvery   = 7 * 24 * time.Hour
	// StateFile is the file of the state of an observer, which StateText reads.
	StateFile = "state.json"
	// AddressMarker is in the address of the webhook of a test. It stands for the token that a real address holds, and
	// it must reach no route, no log, and no file of the state. HooksExample is the host and the path of the address of
	// a webhook that no test sends to.
	AddressMarker = "TOKEN-SECRET"
	HooksExample  = "hooks.example/"
	// NoPacks is the member of a record that holds no pack, after a comma.
	NoPacks = `,"packs":[]`
	// Revision and NewRevision are two revisions of the records that a pipeline approved, in the meta file that
	// ApprovedMeta gives. NewRevision comes after Revision.
	Revision    = "4e1c7a2"
	NewRevision = "5f2d8b1"
	// FredCompositions is the key of the pack that the move of barney-addons changes.
	FredCompositions = "addon/fred-compositions"
	// SyncedAt is the time of the last sync of a registry.
	SyncedAt = "2026-10-30T09:00:00Z"
	// Digest1 and Digest2 are two digests of an image.
	Digest1 = "sha256:1111111111111111111111111111111111111111111111111111111111111111"
	Digest2 = "sha256:2222222222222222222222222222222222222222222222222222222222222222"
	// VendorRef is the image of the pack VendorPack that moves to another digest, and VendorImage is its name with no tag.
	VendorImage = "vendor.example/product"
	VendorRef   = VendorImage + ":2026.2"
	VendorPack  = "addon/vendor-product"
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
	// DexProfile is the profile of the pack dex in the dev data and in the tests of more than one package, and DexKey is
	// the key of the pack.
	DexProfile = "fred-identity@2.3.0"
	DexKey     = "addon/dex"
	// AsksRoute is the route of the list of answers of Palette. NoAnswers is the list of an observer that holds none, in
	// the JSON of the route. The list is empty, and not null.
	AsksRoute = "/v1/asks"
	NoAnswers = `"asks":[]`
	// NoErrorVerdict, RefusesVerdict, and NoAnswerVerdict are the verdicts of an answer of the kind no-error, refuses, and
	// no-answer, in the JSON of an answer.
	NoErrorVerdict  = `"verdict":"no-error"`
	RefusesVerdict  = `"verdict":"refuses"`
	NoAnswerVerdict = `"verdict":"no-answer"`
	// KubeHost and KubePort are the variables of the address of the API server that the kubelet gives a pod.
	KubeHost = "KUBERNETES_SERVICE_HOST"
	KubePort = "KUBERNETES_SERVICE_PORT"
	// RecordsTool is the script that writes the records of a cluster, from the root of the repository. RecordsScript is
	// its path from the directory of a package of internal/.
	RecordsTool   = "scripts/ci/observer-records.sh"
	RecordsScript = "../../../" + RecordsTool
	// ImageMoved names an image that runs another digest: the badge of the page and the title of the alert.
	ImageMoved = "Image moved"
	// FaviconLight and FaviconDark are the icons of the page for the light and the dark scheme.
	FaviconLight = "favicon-light.svg"
	FaviconDark  = "favicon-dark.svg"
	// JSONType is the Content-Type of a document of JSON.
	JSONType = "application/json"
	// LoopbackHost is the Host of a request to the server of the page through a port-forward. FetchSite is the header
	// that a browser sets on a fetch, and SameOrigin is its value on a fetch of the page itself. The server refuses a
	// POST with no SameOrigin.
	LoopbackHost = "127.0.0.1:8080"
	FetchSite    = "Sec-Fetch-Site"
	SameOrigin   = "same-origin"
	// HeaderPolicy and HeaderNoSniff are two headers that the server sets on each answer.
	HeaderPolicy  = "Content-Security-Policy"
	HeaderNoSniff = "X-Content-Type-Options"
	// NginxImage is the image of nginx, with its full name and no tag.
	NginxImage = "docker.io/library/nginx"
	// DexExpiry is the path of the settings of the expiry of the tokens of dex, in the values of its pack.
	DexExpiry = "charts.dex.config.expiry"
	// KubernetesMove is the key of the move of kubernetes from 1.35.3 to 1.36.2, after the uid of its registry. DexMove
	// is the key of the move of dex from 2.42.0 to 2.44.0.
	KubernetesMove = "/" + KubernetesFrom + "->1.36.2"
	DexMove        = "/" + DexFrom + "->2.44.0"
	// KubernetesKey is the key of the pack kubernetes, and KubernetesDiff ends the route of its diff in a profile.
	KubernetesKey  = "k8s/kubernetes"
	KubernetesDiff = "/" + KubernetesKey + "/diff"
	// DexValuesHead is the head of the diff of the values of dex in the dev data. NoEOL is the note of a diff for a side
	// with no newline at its end.
	DexValuesHead = "--- a: " + DexKey + " values\n+++ b: " + DexKey + " values\n"
	NoEOL         = `\ No newline at end of file`
)

// The profiles and the keys of the packs of the test data that the tests of more than one package hold, with the
// versions of a pack in a move.
const (
	// BaseProfile and GatewayProfile are two profiles of the dev data. BaseProfile holds the pack kubernetes, and
	// GatewayProfile holds the pack traefik, whose key is TraefikKey. GatewayMoved is the version of the profile that the
	// cluster no longer runs.
	BaseProfile    = "barney-base@1.5.0"
	GatewayProfile = "wilma-gateway@3.1.2"
	GatewayMoved   = "wilma-gateway@3.0.4"
	TraefikKey     = "addon/traefik"
	// LonghornProfile is the profile of the pack longhorn in the dev data, and LonghornKey is the key of the pack.
	LonghornProfile = "dino-storage@1.1.0"
	LonghornKey     = "addon/longhorn"
	// NobodyProfile is a profile that no record holds, and NoneKey and NopeKey are keys of no pack.
	NobodyProfile = "nobody@1.0.0"
	NoneKey       = "addon/none"
	NopeKey       = "k8s/nope"
	// AWSStackOld is a version of aws-stack before AWSStackProfile, and AWSStackNewer is a version after it.
	AWSStackOld   = "aws-stack@1.0.0"
	AWSStackNewer = "aws-stack@1.1.1"
	// BarneyAddons is the version of barney-addons that a cluster runs, and BarneyAddonsOld is the version before it.
	BarneyAddons    = "barney-addons@1.3.1"
	BarneyAddonsOld = "barney-addons@1.3.0"
	// OtherProfile is a second profile that holds a pack of the first profile of a test.
	OtherProfile = "other@1.0.0"
	// GazooProfile is the profile of the pack velero in the dev data.
	GazooProfile = "gazoo-backup@1.2.0"
	// CSIKey, CertManagerKey, UbuntuKey, UbuntuAWSKey, and VeleroKey are the keys of the packs csi-aws-ebs,
	// cert-manager, ubuntu, ubuntu-aws, and velero. AddonKubernetesKey has the name of kubernetes in another layer.
	CSIKey             = "csi/csi-aws-ebs"
	CertManagerKey     = "addon/cert-manager"
	UbuntuKey          = "os/ubuntu"
	UbuntuAWSKey       = UbuntuKey + "-aws"
	VeleroKey          = "addon/velero"
	AddonKubernetesKey = "addon/kubernetes"
	// KubernetesFrom and KubernetesTo are the versions of the pack kubernetes in the move KubernetesMove. DexFrom and
	// DexTo are the versions of the pack dex in the move DexMove.
	KubernetesFrom = "kubernetes@1.35.3"
	KubernetesTo   = "kubernetes@1.36.2"
	DexFrom        = "dex@2.42.0"
	DexTo          = "dex@2.44.0"
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

// The documents of the real binary in the test data of internal/docs, which ReadDoc reads. The test data of
// internal/api also holds a document of `behind` in BehindDoc.
const (
	DriftSummary    = "verify-summary-drift.json"
	BehindDoc       = "behind.json"
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
	FindingsCounts     = "findings %+v, counts %+v"
	// ErrorWantOK is the failure of a check of a call that must give an error, or none.
	ErrorWantOK = "error %v, want ok %v"
	// PackCompares counts the compares of two packs, and RanTogether the most processes that ran at one time.
	PackCompares = "%d compares of two packs"
	RanTogether  = "%d processes ran together"
	// NoDiff is the failure of a search for the diff of a profile, a pack, and a side.
	NoDiff = "no diff for %s %s %s"
	// HeadersMissing is the failure of a check of the headers that the server sets on the answer for a file.
	HeadersMissing = "%s: the headers of the server are missing"
	// RunContract is the step that sets the variables of a contract test, which skips with no variable.
	RunContract = "run scripts/observer-contract.sh"
	// FirstCycles names the wait for the first cycle of each timer of the observer.
	FirstCycles = "the first cycle of each timer"
	// ExitStderr is the failure of a check of the exit code of a command, with the text of its stderr.
	ExitStderr = "exit %d, stderr %q"
	// CyclesFailed is the failure of a test whose cycles of an observer failed.
	CyclesFailed = "the cycles failed: %v"
)

// ExitsTwo fails the test when a command did not exit 2, with nothing on stdout and each of the words on stderr. code,
// stdout, and stderr are what the command gave.
func ExitsTwo(t *testing.T, code int, stdout, stderr string, words ...string) {
	t.Helper()
	ok := code == 2 && stdout == ""
	for _, w := range words {
		ok = ok && strings.Contains(stderr, w)
	}
	if !ok {
		t.Errorf("exit %d, stdout %q, stderr %q", code, stdout, stderr)
	}
}

// Serve sends a request of the method for path to the handler h, with the Host host, and gives the answer. A site
// that is not empty is the value of the header FetchSite.
func Serve(h http.Handler, method, path, host, site string) *httptest.ResponseRecorder {
	req := httptest.NewRequest(method, path, nil)
	req.Host = host
	if site != "" {
		req.Header.Set(FetchSite, site)
	}
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, req)
	return rec
}

// Get sends a GET for path to the handler h, with the Host LoopbackHost and no header FetchSite, and gives the answer.
func Get(h http.Handler, path string) *httptest.ResponseRecorder {
	return Serve(h, http.MethodGet, path, LoopbackHost, "")
}

// AskPath gives the route of the question to Palette for the move of the pack key of the profile.
func AskPath(profile, key string) string {
	return "/v1/packs/" + profile + "/" + key + "/ask"
}

// DexAsk is the route of the question to Palette for the move of dex in the dev data.
var DexAsk = AskPath(DexProfile, DexKey)

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
	for _, path := range []string{AskPath(LonghornProfile, LonghornKey), AskPath(NobodyProfile, DexKey)} {
		if code, _ := post(path, true); code != http.StatusNotFound {
			t.Errorf("%s gave %d", path, code)
		}
	}
}

// Channel gives the member of the object channels of a file of alert rules: the channel name, whose address is in the
// file address.
func Channel(name, address string) string {
	return `"` + name + `": {"urlFile": "` + address + `"}`
}

// Rules gives the text of a file of alert rules with the channels, each from Channel, and the rules, each a JSON
// object. Each rule starts a line, and the first one is on line 4.
func Rules(channels []string, rules ...string) string {
	return "{\n\"channels\": {" + strings.Join(channels, ", ") + "},\n\"rules\": [\n" + strings.Join(rules, ",\n") + "\n]\n}"
}

// OneChannel gives the text of a file of alert rules with one channel, name, whose address is in the file address.
func OneChannel(name, address string, rules ...string) string {
	return Rules([]string{Channel(name, address)}, rules...)
}

// OncallRules gives the text of a file of alert rules with one channel, oncall, whose address is in the file address.
func OncallRules(address string, rules ...string) string {
	return OneChannel("oncall", address, rules...)
}

// The rules of the alerts that the tests of more than one package give. Each one sends a drift, a loss of contact, or
// a change of an image of the cluster at once to the channel oncall.
const (
	DriftNow = `{"level": "cluster", "news": "drift", "send": "now", "to": "oncall"}`
	LostNow  = `{"level": "cluster", "news": "lost", "send": "now", "to": "oncall"}`
	ImageNow = `{"level": "cluster", "news": "image", "send": "now", "to": "oncall"}`
)

// A BadRule is a rule that the parse of a file of rules refuses. It holds the name of the case and the words of the
// error.
type BadRule struct{ Name, Rule, Want string }

// The rules that the tests of the parse and of the start refuse. A mute needs an end date, and a newer version never
// pages a person.
var (
	NoEndDate = BadRule{"a mute with no end date", `{"level": "profile", "match": "barney-base", "news": "newer"}`, "end date"}
	NewerNow  = BadRule{"a newer version that pages a person", `{"level": "cluster", "news": "newer", "send": "now", "to": "oncall"}`, "never pages"}
)

// Quarry starts the head of a message of the cluster quarry-1 of the dev data, and QuarryDrift is the head of a drift.
// NoReach starts the detail of a loss of contact with Palette. LostCleared is in the message when the observer reaches
// Palette again.
const (
	Quarry      = "strata · quarry-1 · "
	QuarryDrift = Quarry + "Drift"
	NoReach     = "The observer cannot reach Palette."
	LostCleared = "Lost contact cleared"
)

// NoReachAfter is the detail of a loss of contact after n content checks that each failed with an HTTP 401.
func NoReachAfter(n int) string {
	return fmt.Sprintf("%s The last %d content checks failed: %s, HTTP 401.", NoReach, n, docs.KindAPI)
}

// The failures of the check of the messages of a finding that opens, stays for one more cycle, and clears.
const (
	OpenMessage  = "the open message %q"
	NextCycle    = "the next cycle sent %d messages"
	ClearMessage = "the clear message %q"
)

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
	WriteExec(t, path, "#!/bin/sh\n"+body+"\n")
	return path
}

// Strata writes a script that stands for the strata binary, and gives its path. body is the shell
// code after the lines that set $cmd to the command, $ref to the first argument after it, and $out to
// the value of -o. The arguments after the command stay in $1 and the next.
func Strata(t *testing.T, body string) string {
	t.Helper()
	return Script(t, preamble+body)
}

// WriteFile writes body to the file at path, which only the user can read and write, and fails the test on an error.
func WriteFile(t *testing.T, path, body string) {
	t.Helper()
	writeMode(t, path, body, 0o600)
}

// writeMode writes body to the file at path with the mode, and fails the test on an error.
func writeMode(t *testing.T, path, body string, mode os.FileMode) {
	t.Helper()
	if err := os.WriteFile(path, []byte(body), mode); err != nil {
		t.Fatal(err)
	}
}

// WriteIn writes body to the file name of dir with WriteFile, and gives its path.
func WriteIn(t *testing.T, dir, name, body string) string {
	t.Helper()
	path := filepath.Join(dir, name)
	WriteFile(t, path, body)
	return path
}

// WriteExec writes body to the file at path as a program that each user can run, and fails the test on an error.
func WriteExec(t *testing.T, path, body string) {
	t.Helper()
	writeMode(t, path, body, 0o755)
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
	WriteIn(t, dir, name, buf.String())
}

// Cluster makes the read of the cluster PaymentsUID that runs the given profiles, each as name@version. The
// cluster has an update pending.
func Cluster(name string, profiles ...string) docs.ClusterRead {
	var c docs.ClusterRead
	c.Source.Cluster.UID, c.Source.Cluster.Name, c.Source.Cluster.UpdatesPending = PaymentsUID, name, true
	for _, p := range profiles {
		n, v, _ := strings.Cut(p, "@")
		c.Source.Cluster.Profiles = append(c.Source.Cluster.Profiles, docs.ClusterProfile{Name: n, Version: v, Type: "cluster"})
	}
	return c
}

// Payments makes the read of the cluster payments, which runs AWSStackProfile and then the more profiles.
func Payments(more ...string) docs.ClusterRead {
	return Cluster(PaymentsName, append([]string{AWSStackProfile}, more...)...)
}

// RigDirs holds the directories of an observer of the tests of api and of check: its records, its work, and its
// state. testkit cannot give the check.Config of the observer, because the tests of check import testkit. So each rig
// writes the config from RigDirs, PaymentsUID, ContentEvery, and VersionsEvery.
type RigDirs struct{ Records, Work, State string }

// NewRigDirs makes the directories of an observer in new directories of the test. The observer makes the state.
func NewRigDirs(t *testing.T) RigDirs {
	t.Helper()
	return RigDirs{Records: t.TempDir(), Work: t.TempDir(), State: filepath.Join(t.TempDir(), "state")}
}

// WriteMeta writes body to the meta file of the records in dir.
func WriteMeta(t *testing.T, dir, body string) {
	t.Helper()
	WriteIn(t, dir, MetaFile, body)
}

// StateText gives the text of the state file that an observer wrote in the directory state.
func StateText(t *testing.T, state string) string {
	t.Helper()
	return string(ReadFile(t, filepath.Join(state, StateFile)))
}

// StateHoldsNone fails when the state file in the directory state holds one of the words.
func StateHoldsNone(t *testing.T, state string, words ...string) {
	t.Helper()
	HoldsNone(t, "the state", StateText(t, state), words...)
}

// ReadFile reads the file at path, and fails the test on an error.
func ReadFile(t *testing.T, path string) []byte {
	t.Helper()
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return raw
}

// OncallFile writes the file of the address of the channel oncall with AddressFile.
func OncallFile(t *testing.T, address string) string {
	t.Helper()
	return AddressFile(t, "oncall", address)
}

// AddressFile writes the file of the address of the channel, which holds address, to a new directory of the test, and
// gives its path. The file has the name of the channel, as a Secret of the channel gives it.
func AddressFile(t *testing.T, channel, address string) string {
	t.Helper()
	return WriteIn(t, t.TempDir(), channel, address)
}

// DecodeFile reads the JSON file at path into v, and fails the test on an error.
func DecodeFile(t *testing.T, path string, v any) {
	t.Helper()
	Decode(t, path, ReadFile(t, path), v)
}

// Decode reads the JSON raw into v, and fails the test on an error. what names the JSON in the failure.
func Decode(t *testing.T, what string, raw []byte, v any) {
	t.Helper()
	if err := json.Unmarshal(raw, v); err != nil {
		t.Fatalf("%s is no JSON that the test can read: %v: %s", what, err, raw)
	}
}

// SamePlain fails when got and want encode to other JSON, compared as plain values. what names got in the failure.
func SamePlain(t *testing.T, what string, got, want any) {
	t.Helper()
	if g, w := Plain(t, got), Plain(t, want); !reflect.DeepEqual(g, w) {
		t.Errorf("%s gives\n%v\nwant\n%v", what, g, w)
	}
}

// Plain gives the plain values of the JSON of v, such as a map[string]any, so that a test can compare two documents. A
// json.RawMessage gives the values of the text that it holds.
func Plain(t *testing.T, v any) any {
	t.Helper()
	raw, ok := v.(json.RawMessage)
	if !ok {
		raw = MustJSON(t, v)
	}
	var out any
	Decode(t, "the document", raw, &out)
	return out
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

// The words of a check that the tests of more than one package hold.
const (
	// NoNewerProfile names a profile with no newer version, in the name of a case or of a check.
	NoNewerProfile = "a profile with no newer version"
	// NotJSONAnswer names an answer of the kind not-json: a case of the read of the pods, and the words of the page.
	NotJSONAnswer = "an answer that is not JSON"
	// KubernetesValues names the values of the pack kubernetes in a check of a diff.
	KubernetesValues = "the values of kubernetes"
)

// The parts of the script of a fake of strata that the tests of more than one package write. VerifyFirst starts the
// cases of the command with the branch of verify. VerifyAndSnapshot ends that branch: verify prints VerifyUnchanged.
// It then gives the branch of snapshot, which writes ClusterPayments to the file of -o. PacksFirst starts the branch
// of compare with the case of a compare of two packs.
const (
	VerifyFirst = "case $cmd in\nverify)\n"
	PacksFirst  = "compare)\n  case $ref in\n  pack:*)\n"
)

var VerifyAndSnapshot = "  echo '" + VerifyUnchanged + "' ;;\nsnapshot) echo '" + ClusterPayments + "' > \"$out\" ;;\n"

// ErrorRow is the row of `verify` for a record whose check failed with the kind of an error and the HTTP status. A
// status of 0 is none.
func ErrorRow(kind string, status int) docs.VerifyRow {
	return docs.VerifyRow{Status: docs.VerifyError, Error: &docs.Error{Kind: kind, Status: status}}
}

// NotReadRow is the row of `verify` for a record that Palette refused to answer for, with a 429.
func NotReadRow() docs.VerifyRow {
	return ErrorRow(docs.KindAPI, 429)
}

// ErrorLine gives the line of the script of a fake of strata that writes the error document of strata to its stderr.
// The document holds the kind and the other fields, such as `"status":401`.
func ErrorLine(kind, fields string) string {
	return `echo '{"error":{"kind":"` + kind + `",` + fields + `}}' >&2`
}

// ApprovedMeta gives the meta file of records that a pipeline approved at the revision. The word of the baseline is
// baseline.Approved, and that of FirstSeenMeta is baseline.FirstSeen. testkit cannot import baseline, because the
// tests of baseline import testkit.
func ApprovedMeta(revision string) string {
	return `{"baseline":"approved","revision":"` + revision + `"}`
}

// FirstSeenMeta gives the meta file of records that the observer took from the cluster from.
func FirstSeenMeta(from string) string {
	return `{"baseline":"first-seen","firstSeenFrom":"` + from + `"}`
}

// Record gives a record of strata with the schema id, and the name and the version of its profile. source holds the
// other members of the source, and rest the other members of the record, each after a comma.
func Record(schema, name, version, source, rest string) string {
	return `{"schema":"` + schema + `","source":{"name":"` + name + `","version":"` + version + `"` + source + `}` + rest + `}`
}

// ProfileRecord gives a record that strata 0.28.0 took of the cluster profile name@version, whose uid is u-<name>.
// rest holds the other members of the record, each after a comma.
func ProfileRecord(name, version, rest string) string {
	return Record(docs.SnapshotSchema, name, version, `,"uid":"u-`+name+`","type":"cluster"`, `,"tool":{"version":"0.28.0"}`+rest)
}

// AWSStack gives the record of aws-stack@1.1.0, the profile that the cluster payments runs, with the root r1. rest
// holds the other members of the record, each after a comma.
func AWSStack(rest string) string {
	return ProfileRecord("aws-stack", "1.1.0", rootR1+rest)
}

// rootR1 is the member of the root of a record whose root is r1, after a comma.
const rootR1 = `,"root":"r1"`

// Snapshot gives the record that a fake of strata writes for a snapshot of the profile name@version. Its root is
// r-<name>, and it holds no pack.
func Snapshot(name, version string) string {
	return ProfileRecord(name, version, `,"root":"r-`+name+`"`+NoPacks)
}

// RigPack gives a pack of the record of a rig of the tests of api and of check. The pack has the key layer/name, the
// version, the registry reg1, and the images.
func RigPack(key, version string, images ...string) map[string]any {
	layer, name, _ := strings.Cut(key, "/")
	return map[string]any{"key": key, "layer": layer, "name": name, "version": version, "registryUid": "reg1", "images": images}
}

// RigRecord gives the record of name@version that a rig of the tests of api and of check writes. Its root is r1, it
// holds the packs, and its one variable, team, holds the value.
func RigRecord(t *testing.T, name, version string, packs []map[string]any, value string) string {
	t.Helper()
	variables := MustJSON(t, []map[string]string{{"name": "team", "value": value}})
	return ProfileRecord(name, version, rootR1+`,"packs":`+string(MustJSON(t, packs))+`,"variables":`+string(variables))
}

// WriteRecord writes the record of RigRecord, compressed, to the file name@version.json.gz of dir.
func WriteRecord(t *testing.T, dir, name, version string, packs []map[string]any, value string) {
	t.Helper()
	WriteGzip(t, dir, name+"@"+version+".json.gz", RigRecord(t, name, version, packs, value))
}

var (
	secretName = regexp.MustCompile(`^  name: (\S+)$`)
	secretData = regexp.MustCompile(`^  ([^ :]+): (\S+)$`)
)

// HunkHead reads the head of a hunk of a unified text. It gives the first line and the count of lines of the old side,
// then of the new side. A head with no count names one line.
var HunkHead = regexp.MustCompile(`^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@$`)

// RecordSecrets reads the Secrets in YAML that the records script prints. It gives the data of each Secret by its
// name: each key with its text in base64. A Secret with no data gives an empty map. It fails the test when the YAML
// holds no Secret.
func RecordSecrets(t *testing.T, yaml string) map[string]map[string]string {
	t.Helper()
	out := map[string]map[string]string{}
	for _, doc := range strings.Split(yaml, "---\n") {
		name, data, inData := "", map[string]string{}, false
		for _, line := range strings.Split(doc, "\n") {
			switch {
			case secretName.MatchString(line):
				name = secretName.FindStringSubmatch(line)[1]
			case line == "data:":
				inData = true
			case inData && secretData.MatchString(line):
				m := secretData.FindStringSubmatch(line)
				data[m[1]] = m[2]
			}
		}
		if name != "" {
			out[name] = data
		}
	}
	if len(out) == 0 {
		t.Fatalf("the records script printed no Secret:\n%s", yaml)
	}
	return out
}
