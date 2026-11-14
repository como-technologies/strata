package web

import (
	"fmt"
	htmlpkg "html"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"slices"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/testkit"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

/* The tests below make sure that each command which the page shows pastes as the arguments that the page means. A name in a
command comes from Palette, so it is text of a person. The tests give the page names that a shell reads as code, take each command
from the markup, and run it with bash. A stand-in for strata writes its arguments to a file. A name that holds `touch M-x` must
leave no file M-x. They need node and bash, like the other tests of the page. */

// shellNames are the kinds of name that a shell does not read as one word. The value holds the tag of the name, and a command
// substitution writes a file with that tag, so a test sees the substitution run. A plain name needs no quote.
var shellNames = []struct {
	kind  string
	value func(tag string) string
	plain bool
}{
	{"a normal name", func(tag string) string { return "plain-" + tag }, true},
	{"an apostrophe", func(tag string) string { return "it's-" + tag }, false},
	{"a space", func(tag string) string { return "two words-" + tag }, false},
	{"a command substitution", func(tag string) string { return "a$(touch M-" + tag + ")b" }, false},
	{"a backtick", func(tag string) string { return "a`touch M-" + tag + "`b" }, false},
	{"every kind at once", func(tag string) string {
		return "a 'b' $(touch M-" + tag + ") `touch N-" + tag + "`;touch O-" + tag + ";"
	}, false},
}

// standIn is a tool that writes its arguments to the file ARGUMENTS_FILE, each one followed by a NUL byte.
const standIn = "#!/usr/bin/env bash\nprintf '%s\\0' \"$@\" > \"$ARGUMENTS_FILE\"\n"

// sink reads its standard input and writes nothing, as kubectl does for a command that the page gives.
const sink = "#!/usr/bin/env bash\ncat > /dev/null\n"

// jsonText gives a text as it stands inside a JSON string.
func jsonText(t *testing.T, s string) string {
	t.Helper()
	raw := testkit.MustJSON(t, s)
	return string(raw[1 : len(raw)-1])
}

// renamedSet writes a copy of a set in which each pair of swaps puts a new text in place of an old one, in every file. An old text
// that starts with a quote is a whole JSON string. The other old texts change wherever they stand.
func renamedSet(t *testing.T, from devdata.State, swaps [][2]string) string {
	t.Helper()
	rename := func(raw []byte) []byte {
		text := string(raw)
		for _, s := range swaps {
			to := jsonText(t, s[1])
			if strings.HasPrefix(s[0], `"`) {
				to = `"` + to + `"`
			}
			text = strings.ReplaceAll(text, s[0], to)
		}
		return []byte(text)
	}
	edit := map[string]func([]byte) []byte{"alerts.json": rename}
	for _, name := range devdata.Files {
		edit[name] = rename
	}
	return writeSet(t, from, edit)
}

var commandOf = regexp.MustCompile(codeOf)

// commandAfter gives the first command in the markup that follows a text.
func commandAfter(t *testing.T, markup, text string) string {
	t.Helper()
	at := strings.Index(markup, text)
	if at < 0 {
		t.Fatalf("the markup has no %q", text)
	}
	m := commandOf.FindStringSubmatch(markup[at:])
	if m == nil {
		t.Fatalf("no command follows %q", text)
	}
	return htmlpkg.UnescapeString(m[1])
}

// argumentsOf runs a command of the page with bash in dir, with the stand-ins of bin first on the PATH, and it gives the arguments that
// the stand-in for the tool got. It gives nil, and it fails the test, when the command fails.
func argumentsOf(t *testing.T, bin, dir, command string) []string {
	t.Helper()
	file := filepath.Join(t.TempDir(), "arguments")
	t.Setenv("ARGUMENTS_FILE", file)
	if code, out := runBash(t, dir, bin, command); code != 0 {
		t.Errorf(commandExited, command, code, out)
		return nil
	}
	raw, err := os.ReadFile(file)
	if err != nil {
		t.Errorf("the command %q ran no stand-in: %v", command, err)
		return nil
	}
	return strings.Split(strings.TrimSuffix(string(raw), "\x00"), "\x00")
}

// writeTool writes an executable file, and the folders above it.
func writeTool(t *testing.T, path, body string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	testkit.WriteExec(t, path, body)
}

// needBash skips the test when bash is not on the PATH.
func needBash(t *testing.T) {
	t.Helper()
	if _, err := exec.LookPath("bash"); err != nil {
		t.Skip("bash is not on the PATH")
	}
}

// noMarkers fails when a command substitution of a name wrote a file in dir: the name ran as code.
func noMarkers(t *testing.T, dir string) {
	t.Helper()
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	var made []string
	for _, e := range entries {
		if regexp.MustCompile(`^[MNO]-`).MatchString(e.Name()) {
			made = append(made, e.Name())
		}
	}
	if len(made) != 0 {
		t.Errorf("a name ran as code and wrote %q", made)
	}
}

// Each command of the card "Check this yourself", of the splash for a text above the cap, and of the splash for a page with no
// records pastes as one argument for each name. A name with a quote, a space, a command substitution, or a backtick never runs and
// never splits. A name that needs no quote stays as it is.
func TestEachCommandThatThePageShowsPastesAsTheArgumentsItMeans(t *testing.T) {
	needBash(t)
	dex := registryUidOf(t, loadSet(t, devdata.StateOK), "dex")
	k8s := registryUidOf(t, loadSet(t, devdata.StateTextTooLarge), "kubernetes")
	for _, c := range shellNames {
		t.Run(c.kind, func(t *testing.T) {
			bin, dir := checkWork(t, "")
			writeTool(t, filepath.Join(bin, "strata"), standIn)
			writeTool(t, filepath.Join(bin, "kubectl"), sink)
			writeTool(t, filepath.Join(dir, testkit.RecordsTool), standIn)
			profile, cluster, pack, kubernetes, uid := c.value("p"), c.value("c"), c.value("k"), c.value("m"), c.value("u")
			subject := profile + "@2.3.0"
			// check runs a command. A plain name leaves no quote in it, and another name has one, when the command holds a name.
			names := []string{profile, cluster, pack, kubernetes, uid}
			check := func(where, command string, want []string) {
				t.Helper()
				named := slices.ContainsFunc(want, func(arg string) bool {
					return slices.ContainsFunc(names, func(name string) bool { return strings.Contains(arg, name) })
				})
				if (c.plain || !named) && strings.Contains(command, "'") {
					t.Errorf("%s: the command %q has a quote that no name needs", where, command)
				}
				if !c.plain && named && !strings.Contains(command, "'") {
					t.Errorf("%s: the command %q has no quote around the name", where, command)
				}
				if got := argumentsOf(t, bin, dir, command); got != nil && !slices.Equal(got, want) {
					t.Errorf("%s: "+gaveArgs, where, command, got, want)
				}
			}

			// The card: the page of a profile and the page of a pack, with a move to a newer version.
			ok := renamedSet(t, devdata.StateOK, [][2]string{{"fred-identity", profile}, {"quarry-1", cluster}, {`"dex"`, pack}, {`"addon/dex"`, "addon/" + pack}})
			pages := pagesOf(t, ok, "#p-"+encodeURIComponent(subject), packPage(subject, "addon", pack))
			want := map[string][]string{
				qPaletteHas:                  {"verify", "record.json"},
				"Who changed it?":            {"verify", "record.json", "--who"},
				"What changed?":              {"compare", "record.json", subject, "--full"},
				qClusterRuns:                 {"compare", "record.json", "cluster:" + cluster + "/" + profile},
				"What does the move change?": {"compare", "pack:" + dex + "/" + pack + "@2.42.0", "pack:" + dex + "/" + pack + "@2.44.0", "--full"},
				"Which newer versions does Palette list?": {"behind", "record.json", "--pin", "addon/" + pack},
			}
			seen := map[string]bool{}
			for i, page := range pages {
				for j, r := range termRows(t, page) {
					if j == 0 {
						// The record is read by its key, which has only safe characters.
						key := regexp.MustCompile(`[^A-Za-z0-9._-]`).ReplaceAllString(profile+"_2.3.0.json.gz", "_")
						if r.command != fmt.Sprintf(recordRead, key) {
							t.Errorf("page %d: the first command is %q", i, r.command)
						}
						continue
					}
					w, known := want[r.question]
					if !known {
						t.Errorf("page %d: the card asks %q, and the test does not know it", i, r.question)
						continue
					}
					seen[r.question] = true
					check(fmt.Sprintf("page %d, %s", i, r.question), r.command, w)
				}
			}
			if len(seen) != len(want) {
				t.Errorf("the cards held %d of the %d questions", len(seen), len(want))
			}

			// The splash of a text above the cap: the published side of dex, and the move of kubernetes.
			large := renamedSet(t, devdata.StateTextTooLarge, [][2]string{{"fred-identity", profile}, {`"dex"`, pack}, {`"addon/dex"`, "addon/" + pack}, {`"kubernetes"`, kubernetes}, {`"k8s/kubernetes"`, "k8s/" + kubernetes}})
			splashes := pagesOf(t, large, packPage(subject, "addon", pack), packPage(k8sSubject, "k8s", kubernetes))
			check("the splash of dex", commandAfter(t, splashes[0], tooLargeTitle), []string{"compare", "record.json", subject, "--full"})
			check("the splash of kubernetes", commandAfter(t, splashes[1], tooLargeTitle),
				[]string{"compare", "pack:" + k8s + "/" + kubernetes + "@1.35.3", "pack:" + k8s + "/" + kubernetes + "@1.36.2", "--full"})

			// The splash of a page with no records names the cluster by its uid.
			first := renamedSet(t, devdata.StateNoRecords, [][2]string{{testkit.ClusterUID, uid}})
			check("the splash with no records", commandAfter(t, pagesOf(t, first, "#overview")[0], noApproval), []string{"--first-seen", uid})

			noMarkers(t, dir)
		})
	}
}
