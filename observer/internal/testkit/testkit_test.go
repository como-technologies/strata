package testkit

import (
	"encoding/json"
	"log/slog"
	"net/url"
	"path"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/runner"
)

func TestOutFileGivesTheArgumentAfterTheFlagAndAnErrorWhenThereIsNone(t *testing.T) {
	for _, c := range []struct {
		args []string
		want string
		ok   bool
	}{
		{[]string{runner.CmdSnapshot, "a", "-o", "out.json", "-q"}, "out.json", true},
		{[]string{runner.CmdSnapshot, "a", "-q"}, "", false},
		{[]string{runner.CmdSnapshot, "a", "-o"}, "", false},
	} {
		got, err := OutFile(c.args)
		if got != c.want || (err == nil) != c.ok {
			t.Errorf("%v: got %q, %v", c.args, got, err)
		}
	}
}

// With the variable CI set, a tool that is not on the PATH, such as node for the render tests of internal/web, fails
// the test. Else it skips. This test needs no tool.
func TestWithCISetAMissingToolFailsTheTest(t *testing.T) {
	for _, c := range []struct {
		name, ci      string
		failed, skips bool
	}{{"no CI", "", false, true}, {"CI is true", "true", true, false}, {"CI is 1", "1", true, false}} {
		t.Run(c.name, func(t *testing.T) {
			var failed, skipped bool
			WhenMissing(c.ci, "node", func(...any) { failed = true }, func(...any) { skipped = true })
			if failed != c.failed || skipped != c.skips {
				t.Errorf("failed %v and skipped %v, want %v and %v", failed, skipped, c.failed, c.skips)
			}
		})
	}
}

func TestClusterRunsTheProfilesThatTheCallGives(t *testing.T) {
	c := Payments(AddonsProfile)
	got := c.Source.Cluster
	if got.UID != PaymentsUID || got.Name != PaymentsName || !got.UpdatesPending || len(got.Profiles) != 2 {
		t.Fatalf("cluster %+v", got)
	}
	if p := got.Profiles[1]; p.Name != "platform-addons" || p.Version != "1.0.0" || p.Type != "cluster" {
		t.Errorf("profile %+v", p)
	}
}

func TestListenAddrReadsTheAddressOfTheLineListeningOfTheLog(t *testing.T) {
	var log strings.Builder
	logger := slog.New(slog.NewTextHandler(&log, nil))
	logger.Info("serving test data", "state", "ok")
	if _, ok := listenAddr(log.String()); ok {
		t.Errorf("a log with no line listening gave an address: %s", log.String())
	}
	logger.Info("listening", "addr", "[::1]:41234")
	if got, ok := listenAddr(log.String()); !ok || got != "[::1]:41234" {
		t.Errorf("got %q, %v from %s", got, ok, log.String())
	}
}

func TestReadDocReadsADocumentOfTheRealBinaryFromTheTestDataOfDocs(t *testing.T) {
	var s docs.VerifySummary
	ReadDoc(t, DriftSummary, &s)
	if s.Schema != docs.VerifySummarySchema || len(s.Records) == 0 {
		t.Errorf("the document gives the schema %q and %d records", s.Schema, len(s.Records))
	}
}

func TestMustJSONEncodesAValue(t *testing.T) {
	if got := string(MustJSON(t, map[string]int{"a": 1})); got != `{"a":1}` {
		t.Errorf("got %s", got)
	}
}

func TestUnknownNamesTheCommandThatTheFakeDoesNotKnow(t *testing.T) {
	if err := Unknown([]string{runner.CmdCompare, "a"}); err == nil || err.Error() != `the fake does not know "`+runner.CmdCompare+`"` {
		t.Errorf("got %v", err)
	}
}

// rulesDoc is the part of a file of alert rules that the tests of Rules and OncallRules read.
type rulesDoc struct {
	Channels map[string]struct{ URLFile string }
	Rules    []map[string]string
}

func TestOncallRulesIsAFileOfRulesInJSON(t *testing.T) {
	var got rulesDoc
	if err := json.Unmarshal([]byte(OncallRules("/a/oncall", `{"level": "cluster"}`)), &got); err != nil || got.Channels["oncall"].URLFile != "/a/oncall" || len(got.Rules) != 1 {
		t.Errorf("got %+v, %v", got, err)
	}
}

func TestRulesHoldsEachChannelAndPutsEachRuleOnALineFromLine4(t *testing.T) {
	text := Rules([]string{Channel("a", "/a"), Channel("b", "/b")}, DriftNow, LostNow)
	var got rulesDoc
	if err := json.Unmarshal([]byte(text), &got); err != nil || got.Channels["a"].URLFile != "/a" || got.Channels["b"].URLFile != "/b" || len(got.Rules) != 2 {
		t.Errorf("got %+v, %v", got, err)
	}
	if lines := strings.Split(text, "\n"); len(lines) < 5 || lines[3] != DriftNow+"," || lines[4] != LostNow {
		t.Errorf("the lines %q", lines)
	}
}

// DiffPath puts the side in the query, and an empty side gives no query. The path of the diff and the path of the
// question of AskPath start with the same route of the pack.
func TestDiffPathPutsTheSideInTheQueryAndAnEmptySideGivesNone(t *testing.T) {
	for _, side := range []string{"x", ""} {
		u, err := url.Parse(DiffPath(DexProfile, DexKey, side))
		if err != nil || u.Query().Get("side") != side || (u.RawQuery == "") != (side == "") || path.Dir(u.Path) != path.Dir(DexAsk) {
			t.Errorf("the side %q gives %v, %v", side, u, err)
		}
	}
}
