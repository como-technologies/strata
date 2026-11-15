package web

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"slices"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/testkit"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

/* The test below runs each command of the card "Check this yourself" against the stand-in for Palette, with the strata binary
of this commit. scripts/observer-contract.sh sets the variables. Without them, `go test` skips the test, and it needs no network.

STRATA_BIN              the strata binary.
STRATA_CHECK_RECORD     the record file that the first row of the card must give back, as the pod holds it.
STRATA_CHECK_API        a directory with the answer of each route of a real server, for the first run: a cluster with a finding.
STRATA_CHECK_REGISTRY   the uid of a registry of the stand-in, for the second run: the move of a pack to its newest version. */

// fakeKubectl writes a kubectl that answers the one call of the first row, `get secret strata-observer-records-N -o json`, from
// the files of a directory. It refuses any other call, so the page cannot give a command that this test does not run for real.
func fakeKubectl(t *testing.T, bin, secrets string) {
	t.Helper()
	script := `#!/bin/sh
[ "$#" = 7 ] && [ "$1" = -n ] && [ "$2" = strata-observer ] && [ "$3" = get ] && [ "$4" = secret ] && [ "$6" = -o ] && [ "$7" = json ] \
  || { echo "kubectl: the page gave a call that this test does not know: $*" >&2; exit 1; }
cat "` + secrets + `/$5.json"
`
	testkit.WriteExec(t, filepath.Join(bin, "kubectl"), script)
}

// runBash runs a command of the card in a directory, with the stand-in for kubectl first on the PATH. It gives the exit code and
// the output.
func runBash(t *testing.T, dir, bin, command string) (int, string) {
	t.Helper()
	cmd := exec.Command("bash", "-c", command)
	cmd.Dir = dir
	cmd.Env = append(os.Environ(), "PATH="+bin+string(os.PathListSeparator)+os.Getenv("PATH"))
	var out bytes.Buffer
	cmd.Stdout, cmd.Stderr = &out, &out
	err := cmd.Run()
	if exit, ok := err.(*exec.ExitError); ok {
		return exit.ExitCode(), out.String()
	}
	if err != nil {
		t.Fatalf("%s: %v", command, err)
	}
	return 0, out.String()
}

// checkWork makes the directory where the commands run: a bin with strata and kubectl, and a folder to work in.
func checkWork(t *testing.T, strata string) (bin, dir string) {
	t.Helper()
	root := t.TempDir()
	bin, dir = filepath.Join(root, "bin"), filepath.Join(root, "run")
	for _, d := range []string{bin, dir} {
		if err := os.MkdirAll(d, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	if strata == "" {
		return bin, dir
	}
	abs, err := filepath.Abs(strata)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(abs, filepath.Join(bin, "strata")); err != nil {
		t.Fatal(err)
	}
	return bin, dir
}

// secretsOf runs the records script of the repository on a record, and it writes the four Secrets that it makes as the files that
// the stand-in for kubectl reads. It gives the directory of the files.
func secretsOf(t *testing.T, record string) string {
	t.Helper()
	yaml, err := exec.Command(testkit.RecordsScript, "--revision", "test", record).Output()
	if err != nil {
		t.Fatalf("the records script: %v", err)
	}
	secrets := filepath.Join(t.TempDir(), "secrets")
	if err := os.MkdirAll(secrets, 0o755); err != nil {
		t.Fatal(err)
	}
	written := testkit.RecordSecrets(t, string(yaml))
	if len(written) != 4 {
		t.Fatalf("the records script wrote %d Secrets, want 4", len(written))
	}
	for name, data := range written {
		testkit.WriteIn(t, secrets, name+".json", string(testkit.MustJSON(t, map[string]any{"data": data})))
	}
	return secrets
}

// recordKey gives the key of the record of the profile name at version in the Secrets, as the page writes it: a character
// outside A-Z a-z 0-9 . _ - becomes an underscore.
func recordKey(name, version string) string {
	return regexp.MustCompile(`[^A-Za-z0-9._-]`).ReplaceAllString(name+"_"+version+".json.gz", "_")
}

// The command of the first row reads a record out of the Secrets that the records script writes. This test runs the script and the
// command with the tools of the pod (bash, jq, base64, and gzip), and a stand-in for kubectl. The key of a record has the name and
// the version of its profile, and a character outside A-Z a-z 0-9 . _ - becomes an underscore, as the page writes it.
func TestTheCommandOfTheFirstRowReadsARecordOutOfTheSecretsOfTheRecordsScript(t *testing.T) {
	testkit.NeedTools(t, "bash", "jq", "base64", "gzip", "gunzip")
	body := testkit.Record(docs.SnapshotSchema, "fred identity!", "2.3.0", `,"cluster":null`, `,"packs":[1,2,3]`)
	record := testkit.WriteIn(t, t.TempDir(), "release.json", body)
	bin, dir := checkWork(t, "")
	fakeKubectl(t, bin, secretsOf(t, record))
	key := recordKey("fred identity!", "2.3.0")
	if key != "fred_identity__2.3.0.json.gz" {
		t.Fatalf("the key is %q", key)
	}
	code, out := runBash(t, dir, bin, fmt.Sprintf(recordRead, key))
	got, err := os.ReadFile(filepath.Join(dir, "record.json"))
	if code != 0 || err != nil || string(got) != body {
		t.Fatalf("the command exited with %d and wrote %q (%v):\n%s", code, got, err, out)
	}
	// A key that no Secret holds gives an empty file and an error of gunzip, so the person sees that the record is not there.
	code, out = runBash(t, dir, bin, fmt.Sprintf(recordRead, "nobody_1.0.0.json.gz"))
	if got, _ := os.ReadFile(filepath.Join(dir, "record.json")); code == 0 || len(got) != 0 {
		t.Errorf("a key that is not there gave exit %d and %q:\n%s", code, got, out)
	}
	// The stand-in refuses any other call, so the test would fail if the page gave another command for the Secrets.
	if code, _ := runBash(t, dir, bin, "kubectl -n other get secret "+recordsSecret+"0 -o json"); code == 0 {
		t.Error("the stand-in for kubectl answered a call for another namespace")
	}
}

// commandExited is the failure of a command of the page that exited with a code that says it failed.
const commandExited = "the command %q exited with %d:\n%s"

// An exit code of a command of the card: 0 says no change, and 1 says that the command found one. A code above 1 is a failure.
func mustAnswer(t *testing.T, command string, code int, out string) {
	t.Helper()
	if code > 1 {
		t.Errorf(commandExited, command, code, out)
	}
	if strings.TrimSpace(out) == "" {
		t.Errorf("the command %q printed nothing", command)
	}
}

func TestEachCommandOfCheckThisYourselfRunsAgainstTheStandIn(t *testing.T) {
	const checkRegistry = "STRATA_CHECK_REGISTRY"
	strata := os.Getenv("STRATA_BIN")
	record, apiDir, registry := os.Getenv("STRATA_CHECK_RECORD"), os.Getenv("STRATA_CHECK_API"), os.Getenv(checkRegistry)
	if strata == "" || record == "" || (apiDir == "" && registry == "") {
		t.Skip("STRATA_BIN, STRATA_CHECK_RECORD, and STRATA_CHECK_API or " + checkRegistry + " are not set: " + testkit.RunContract)
	}
	want := testkit.ReadFile(t, record)

	// The first run: the profile page and the pack pages of a real server. The first row reads the record out of the Secrets
	// that the real records script writes, and the other rows run against the stand-in.
	if apiDir != "" {
		t.Run("a cluster with a finding", func(t *testing.T) {
			bin, dir := checkWork(t, strata)
			fakeKubectl(t, bin, secretsOf(t, record))

			var profiles api.ProfilesView
			if testkit.DecodeFile(t, filepath.Join(apiDir, "profiles.json"), &profiles); len(profiles.Profiles) == 0 {
				t.Fatal("profiles.json holds no profile")
			}
			// The Secrets hold one record, so the commands of the profile of that record are the ones that can run.
			var source struct {
				Source struct{ Name, Version string }
			}
			testkit.Decode(t, record, want, &source)
			var hashes []string
			for _, x := range profiles.Profiles {
				if x.Name != source.Source.Name || x.Version != source.Source.Version {
					continue
				}
				subject := model.Subject(x.Name, x.Version)
				hashes = append(hashes, "#p-"+encodeURIComponent(subject))
				for _, k := range x.Packs {
					hashes = append(hashes, packPage(subject, k.Layer, k.Name))
				}
			}
			pages := showIn(t, filepath.Dir(apiDir), devdata.State(filepath.Base(apiDir)), hashes...)
			var commands []string
			for i, page := range pages {
				for _, r := range termRows(t, page) {
					if !slices.Contains(commands, r.command) {
						commands = append(commands, r.command)
					}
				}
				if i == 0 && len(termRows(t, page)) < 4 {
					t.Errorf("the page of %s has fewer than 4 rows", hashes[i])
				}
			}
			// The commands of the card for a profile with a finding: the record, the verdict, who, the changes, and the cluster.
			for _, prefix := range []string{"for i in 0 1 2 3;", verifyRecord, verifyWho, compareRecord, compareCluster} {
				if !slices.ContainsFunc(commands, func(c string) bool { return strings.HasPrefix(c, prefix) }) {
					t.Errorf("no page gives a command that starts with %q", prefix)
				}
			}
			// The first command writes record.json, and it is the record of the pod, byte for byte.
			code, out := runBash(t, dir, bin, commands[0])
			if code != 0 {
				t.Fatalf("the first command exited with %d:\n%s", code, out)
			}
			got := testkit.ReadFile(t, filepath.Join(dir, "record.json"))
			noProblems(t, "the record that the first command writes", []string{copyProblem("record.json", want, got)})
			for _, c := range commands[1:] {
				code, out := runBash(t, dir, bin, c)
				mustAnswer(t, c, code, out)
			}
		})
	}

	// The second run: the move of a pack to its newest version, on a stand-in that serves a registry. The page is the page of the
	// set ok, with the registry uid of the stand-in.
	if registry != "" {
		t.Run("a move to a newer version", func(t *testing.T) {
			bin, dir := checkWork(t, strata)
			set := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"newer.json": editJSON(t, func(v *api.NewerView) {
				for i := range v.Newer {
					v.Newer[i].RegistryUID = registry
				}
			})})
			var rows []termRow
			for _, r := range termRows(t, pagesOf(t, set, k8sHash)[0]) {
				if strings.Contains(r.command, "compare pack:") || strings.Contains(r.command, "behind") {
					rows = append(rows, r)
				}
			}
			if len(rows) != 2 {
				t.Fatalf("the page of kubernetes gives %d rows for the move, want 2: %+v", len(rows), rows)
			}
			testkit.WriteIn(t, dir, "record.json", string(want))
			for _, r := range rows {
				code, out := runBash(t, dir, bin, r.command)
				mustAnswer(t, r.command, code, out)
			}
		})
	}
}
