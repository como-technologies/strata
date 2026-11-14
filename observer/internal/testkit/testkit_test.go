package testkit

import (
	"log/slog"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
)

func TestOutFileGivesTheArgumentAfterTheFlagAndAnErrorWhenThereIsNone(t *testing.T) {
	for _, c := range []struct {
		args []string
		want string
		ok   bool
	}{
		{[]string{"snapshot", "a", "-o", "out.json", "-q"}, "out.json", true},
		{[]string{"snapshot", "a", "-q"}, "", false},
		{[]string{"snapshot", "a", "-o"}, "", false},
	} {
		got, err := OutFile(c.args)
		if got != c.want || (err == nil) != c.ok {
			t.Errorf("%v: got %q, %v", c.args, got, err)
		}
	}
}

func TestClusterRunsTheProfilesThatTheCallGives(t *testing.T) {
	c := Cluster("payments", "aws-stack@1.1.0", "platform-addons@1.0.0")
	got := c.Source.Cluster
	if got.UID != "c1" || got.Name != "payments" || !got.UpdatesPending || len(got.Profiles) != 2 {
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
	ReadDoc(t, "verify-summary-drift.json", &s)
	if s.Schema != docs.VerifySummarySchema || len(s.Records) == 0 {
		t.Errorf("the document gives the schema %q and %d records", s.Schema, len(s.Records))
	}
}

func TestMustJSONEncodesAValue(t *testing.T) {
	if got := string(MustJSON(t, map[string]int{"a": 1})); got != `{"a":1}` {
		t.Errorf("got %s", got)
	}
}
