package main

import (
	"bytes"
	"compress/gzip"
	"encoding/json"
	"os"
	"path/filepath"
	"slices"
	"strconv"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
)

func TestVersionPrintsTheName(t *testing.T) {
	var out, errs bytes.Buffer
	if code := run([]string{"version"}, &out, &errs); code != 0 {
		t.Fatalf("exit %d, stderr %q", code, errs.String())
	}
	if !strings.HasPrefix(out.String(), "strata-observer ") {
		t.Fatalf("stdout %q", out.String())
	}
}

func TestAnUnknownCommandExitsTwo(t *testing.T) {
	var out, errs bytes.Buffer
	if code := run([]string{"nope"}, &out, &errs); code != 2 {
		t.Fatalf("exit %d", code)
	}
	if !strings.Contains(errs.String(), "usage:") {
		t.Fatalf("stderr %q", errs.String())
	}
}

// fakeStrata writes a script that stands for the strata binary. compare is the line that
// `compare` prints. verifyExit is the exit code of `verify`.
func fakeStrata(t *testing.T, compare string, verifyExit int, behindFails bool) string {
	t.Helper()
	body := `#!/bin/sh
cmd=$1
shift
out=
while [ $# -gt 0 ]; do
  if [ "$1" = -o ]; then out=$2; fi
  shift
done
case $cmd in
verify)
  if [ ` + strconv.Itoa(verifyExit) + ` = 2 ]; then echo '{"error":{"kind":"transport","message":"SENTINEL"}}' >&2; exit 2; fi
  echo '{"schema":"strata-verify-summary/1","tool":{"version":"0.29.0"},"records":[{"path":"record-0.json","status":"unchanged","name":"aws-stack","version":"1.1.0"}]}' ;;
snapshot) echo '{"source":{"cluster":{"uid":"c1","name":"payments","profiles":[{"name":"aws-stack","version":"1.1.0","type":"cluster"}]}}}' > "$out" ;;
compare) echo '` + compare + `'; case '` + compare + `' in *'"same":true'*) exit 0 ;; *) exit 1 ;; esac ;;
behind)
  if [ ` + strconv.FormatBool(behindFails) + ` = true ]; then echo '{"error":{"kind":"api","status":404}}' >&2; exit 2; fi
  echo '{"schema":"strata-behind/1","tool":{"version":"0.29.0"},"pins":[]}' ;;
esac
`
	bin := filepath.Join(t.TempDir(), "strata")
	if err := os.WriteFile(bin, []byte(body), 0o755); err != nil {
		t.Fatal(err)
	}
	return bin
}

// onceArgs makes the directories of a run of `once` with one record.
func onceArgs(t *testing.T, bin string) []string {
	t.Helper()
	records := t.TempDir()
	record := `{"schema":"strata-snapshot/1","tool":{"version":"0.28.0"},"source":{"uid":"u1","name":"aws-stack","version":"1.1.0","type":"cluster"},"packs":[]}`
	var buf bytes.Buffer
	zw := gzip.NewWriter(&buf)
	if _, err := zw.Write([]byte(record)); err != nil {
		t.Fatal(err)
	}
	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(records, "aws-stack.json.gz"), buf.Bytes(), 0o600); err != nil {
		t.Fatal(err)
	}
	return []string{"once", "--strata", bin, "--cluster-uid", "c1", "--records", records, "--work", t.TempDir(), "--state", filepath.Join(t.TempDir(), "state")}
}

func TestOnceExitsZeroWithNoFindingAndPrintsTheSummary(t *testing.T) {
	var out, errs bytes.Buffer
	code := run(onceArgs(t, fakeStrata(t, `{"same":true,"sameContent":true}`, 0, false)), &out, &errs)
	if code != 0 {
		t.Fatalf("exit %d, stderr %q", code, errs.String())
	}
	var s model.Summary
	if err := json.Unmarshal(out.Bytes(), &s); err != nil {
		t.Fatalf("stdout %q: %v", out.String(), err)
	}
	if s.Page != model.PageOK || s.Cluster.Name != "payments" || len(s.Findings) != 0 || s.Records.Count != 1 {
		t.Errorf("summary %+v", s)
	}
}

func TestOnceExitsOneWhenAFindingExists(t *testing.T) {
	var out, errs bytes.Buffer
	code := run(onceArgs(t, fakeStrata(t, `{"same":false,"sameContent":false,"rootB":"x","packs":[{"key":"k8s/kubernetes","status":"changed"}]}`, 0, false)), &out, &errs)
	var s model.Summary
	if err := json.Unmarshal(out.Bytes(), &s); err != nil {
		t.Fatalf("stdout %q: %v", out.String(), err)
	}
	if code != 1 || len(s.Findings) != 1 || s.Findings[0].Kind != model.ClusterDiffers {
		t.Fatalf("exit %d, findings %+v", code, s.Findings)
	}
}

func TestOnceExitsTwoWhenACycleFailsAndStillPrintsTheSummary(t *testing.T) {
	var out, errs bytes.Buffer
	code := run(onceArgs(t, fakeStrata(t, `{"same":true}`, 2, false)), &out, &errs)
	var s model.Summary
	if err := json.Unmarshal(out.Bytes(), &s); err != nil {
		t.Fatalf("stdout %q: %v", out.String(), err)
	}
	if code != 2 || s.Content.FailedRuns != 1 || s.Content.LastError == nil || s.Content.LastError.Kind != docs.KindTransport {
		t.Fatalf("exit %d, content %+v", code, s.Content)
	}
	if strings.Contains(out.String()+errs.String(), "SENTINEL") {
		t.Errorf("the message of strata left the process: %s %s", out.String(), errs.String())
	}
}

func TestOnceRefusesAMissingFlagAndAPeriodThatIsNotPositive(t *testing.T) {
	good := onceArgs(t, "strata")
	without := func(flag string) []string {
		var out []string
		for i := 0; i < len(good); i++ {
			if good[i] == flag {
				i++
				continue
			}
			out = append(out, good[i])
		}
		return out
	}
	for _, c := range []struct {
		name string
		args []string
		want string
	}{
		{"no cluster uid", without("--cluster-uid"), "--cluster-uid is required"},
		{"no records", without("--records"), "--records is required"},
		{"no work", without("--work"), "--work is required"},
		{"no state", without("--state"), "--state is required"},
		{"a period of zero", append(slices.Clone(good), "--content-every", "0s"), "more than zero"},
		{"a flag that does not exist", append(slices.Clone(good), "--nope"), "flag provided but not defined"},
	} {
		t.Run(c.name, func(t *testing.T) {
			var out, errs bytes.Buffer
			if code := run(c.args, &out, &errs); code != 2 || !strings.Contains(errs.String(), c.want) || out.Len() != 0 {
				t.Errorf("exit %d, stdout %q, stderr %q", code, out.String(), errs.String())
			}
		})
	}
}

// The observer sends no POST and has no HTTP client. It asks Palette only through strata.
func TestNoCodeOfTheObserverSendsARequest(t *testing.T) {
	banned := []string{`"net/http"`, `http.MethodPost`, `"POST"`, `"net"`}
	err := filepath.WalkDir(".", func(path string, d os.DirEntry, err error) error {
		if err != nil || d.IsDir() || !strings.HasSuffix(path, ".go") || strings.HasSuffix(path, "_test.go") {
			return err
		}
		raw, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		for _, b := range banned {
			if strings.Contains(string(raw), b) {
				t.Errorf("%s holds %s", path, b)
			}
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
}

func TestOnceExitsOneWhenAFindingExistsAlsoIfTheOtherCycleFailed(t *testing.T) {
	var out, errs bytes.Buffer
	code := run(onceArgs(t, fakeStrata(t, `{"same":false,"sameContent":false,"rootB":"x","packs":[{"key":"k8s/kubernetes","status":"changed"}]}`, 0, true)), &out, &errs)
	var s model.Summary
	if err := json.Unmarshal(out.Bytes(), &s); err != nil {
		t.Fatalf("stdout %q: %v", out.String(), err)
	}
	if code != 1 || len(s.Findings) != 1 || s.Versions.FailedRuns != 1 || s.Versions.LastError == nil || s.Versions.LastError.Status != 404 {
		t.Fatalf("exit %d, findings %+v, versions %+v", code, s.Findings, s.Versions)
	}
}
