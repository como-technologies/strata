package alert

import (
	"errors"
	"strings"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/testkit"
)

// planAddress is the file of the address of the channel platform-oncall of the file of the plan.
const planAddress = "/alert-channels/platform-oncall"

// theFile is the file of the plan of the item: a webhook for the team on call, rules at each level, and
// a mute with an end date.
const theFile = `{
  "channels": {"platform-oncall": {"urlFile": "` + planAddress + `"}},
  "rules": [
    {"level": "cluster", "news": "drift", "send": "now", "to": "platform-oncall"},
    {"level": "profile", "match": "fred-identity", "news": "drift", "send": "now", "to": "platform-oncall"},
    {"level": "cluster", "news": "newer", "send": "digest", "to": "platform-oncall"},
    ` + muteBarney + `
  ]
}`

// muteBarney mutes each newer version of barney-base until 2026-12-01.
const muteBarney = `{"level": "profile", "match": "barney-base", "news": "newer", "muteUntil": "2026-12-01"}`

func TestTheFileOfThePlanParses(t *testing.T) {
	rules, err := Parse([]byte(theFile))
	if err != nil {
		t.Fatal(err)
	}
	if len(rules.List) != 4 || rules.List[1].Match != "fred-identity" || !rules.List[3].mute() || rules.Channels[platformOncall].URLFile != planAddress {
		t.Errorf("rules %+v", rules)
	}
	if got := rules.Names(); len(got) != 1 || got[0] != platformOncall {
		t.Errorf("channels %v", got)
	}
}

// The rules of the tests of the parse that send to the channel a.
const (
	driftToA  = `{"level": "cluster", "news": "drift", "send": "now", "to": "a"}`
	lostToA   = `{"level": "cluster", "news": "lost", "send": "now", "to": "a"}`
	digestToA = `{"level": "profile", "match": "fred-identity", "news": "drift", "send": "digest", "to": "a"}`
)

// muteLost gives a rule that mutes the loss of contact until the date. muteDex gives a rule that mutes each drift of
// fred-identity until the date.
func muteLost(until string) string {
	return `{"level": "cluster", "news": "lost", "muteUntil": "` + until + `"}`
}

func muteDex(until string) string {
	return `{"level": "profile", "match": "fred-identity", "news": "drift", "muteUntil": "` + until + `"}`
}

func TestAFileThatCannotBeUsedStopsWithTheLineOfTheFault(t *testing.T) {
	cases := []struct {
		name string
		file string
		line int
		want string
	}{
		{"text that is no JSON", "{\n\"rules\": [\n{\"level\": \n]}", 4, "not JSON"},
		{"a file that is no object", "[1]", 1, "one object"},
		{"a field of the file that is not known", "{\n\"channel\": {}\n}", 2, "unknown field"},
		{"a rule with a field that is not known", testkit.OneChannel(platformOncall, "f", `{"level": "cluster", "news": "drift", "sned": "now", "to": "platform-oncall"}`), 4, "rule 1"},
		{"a field of the wrong type", testkit.OneChannel(platformOncall, "f", `{"level": "cluster", "news": 4, "send": "now", "to": "platform-oncall"}`), 4, "the wrong type"},
		{"a level that is not in the list", testkit.OneChannel(platformOncall, "f", `{"level": "node", "news": "drift", "send": "now", "to": "platform-oncall"}`), 4, "level must be"},
		{"news that is not in the list", testkit.OneChannel(platformOncall, "f", `{"level": "cluster", "news": "health", "send": "now", "to": "platform-oncall"}`), 4, "news must be"},
		{"a profile rule with no match", testkit.OneChannel(platformOncall, "f", `{"level": "profile", "news": "drift", "send": "now", "to": "platform-oncall"}`), 4, "needs match"},
		{"a cluster rule with a match", testkit.OneChannel(platformOncall, "f", `{"level": "cluster", "match": "x", "news": "drift", "send": "now", "to": "platform-oncall"}`), 4, "no match"},
		{"lost below the cluster", testkit.OneChannel(platformOncall, "f", `{"level": "profile", "match": "x", "news": "lost", "send": "now", "to": "platform-oncall"}`), 4, "about the cluster"},
		{"a way to send that is not in the list", testkit.OneChannel(platformOncall, "f", `{"level": "cluster", "news": "drift", "send": "soon", "to": "platform-oncall"}`), 4, "send must be"},
		{"a rule for a channel that the file lacks", testkit.OneChannel(platformOncall, "f", `{"level": "cluster", "news": "drift", "send": "now", "to": "nobody"}`), 4, "name a channel"},
		{testkit.NoEndDate.Name, testkit.OncallRules("f", testkit.NoEndDate.Rule), 4, testkit.NoEndDate.Want},
		{"a mute with an end that is no date", testkit.OneChannel(platformOncall, "f", `{"level": "profile", "match": "barney-base", "news": "newer", "muteUntil": "soon"}`), 4, "end date"},
		{"a mute with a channel", testkit.OneChannel(platformOncall, "f", `{"level": "profile", "match": "barney-base", "news": "newer", "muteUntil": "2026-12-01", "to": "platform-oncall"}`), 4, "no to"},
		{"a rule that sends and mutes", testkit.OneChannel(platformOncall, "f", `{"level": "cluster", "news": "drift", "send": "now", "to": "platform-oncall", "muteUntil": "2026-12-01"}`), 4, "second rule"},
		{testkit.NewerNow.Name, testkit.OncallRules("f", testkit.NewerNow.Rule), 4, testkit.NewerNow.Want},
		{"two rules for one thing", testkit.OncallRules("f", testkit.DriftNow, `{"level": "cluster", "news": "drift", "send": "digest", "to": "oncall"}`), 5, "rule 2"},
		{"a channel with no file", testkit.Rules([]string{`"a": {}`}), 2, "needs urlFile"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			_, err := Parse([]byte(c.file))
			var fe *FileError
			if !errors.As(err, &fe) {
				t.Fatalf("error %v", err)
			}
			if fe.Line != c.line || !strings.Contains(fe.Msg, c.want) {
				t.Errorf("line %d, %q; want line %d with %q", fe.Line, fe.Msg, c.line, c.want)
			}
		})
	}
}

func TestAFaultNamesNoValueOfTheFile(t *testing.T) {
	_, err := Parse([]byte(testkit.OneChannel(platformOncall, "f", `{"level": "SECRET-LEVEL", "news": "drift", "send": "now", "to": "platform-oncall"}`)))
	if err == nil || strings.Contains(err.Error(), "SECRET") {
		t.Errorf("error %v", err)
	}
}

var now = time.Date(2026, 11, 4, 12, 0, 0, 0, time.UTC)

func TestALowerLevelOverridesAHigherOne(t *testing.T) {
	rules, err := Parse([]byte(testkit.OneChannel("a", "f", driftToA, `{"level": "pack", "match": "dex", "news": "drift", "muteUntil": "2026-12-01"}`, digestToA)))
	if err != nil {
		t.Fatal(err)
	}
	cases := []struct {
		name string
		s    Subject
		want int
		ok   bool
	}{
		{"the cluster rule takes any other profile", Subject{News: NewsDrift, Profile: testkit.BaseProfile, Packs: []string{"addon/kyverno"}}, 0, true},
		{"the profile rule overrides the cluster rule", Subject{News: NewsDrift, Profile: testkit.DexProfile, Packs: []string{"addon/cert"}}, 2, true},
		{"the pack rule overrides the profile rule", Subject{News: NewsDrift, Profile: testkit.DexProfile, Packs: []string{testkit.DexKey}}, 1, true},
		{"a pack rule takes a finding that names the pack among others", Subject{News: NewsDrift, Profile: testkit.BaseProfile, Packs: []string{"addon/cert", testkit.DexKey}}, 1, true},
		{"a rule is about its own news", Subject{News: NewsImage, Profile: testkit.DexProfile, Packs: []string{testkit.DexKey}}, 0, false},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			got, ok := rules.decide(c.s, now)
			if ok != c.ok || (ok && got != c.want) {
				t.Errorf("rule %d, %v; want %d, %v", got, ok, c.want, c.ok)
			}
		})
	}
}

func TestAMuteHoldsThroughTheDayOfItsEndDateAndThenCountsAsNoMute(t *testing.T) {
	rules, err := Parse([]byte(testkit.OneChannel("a", "f", driftToA, muteDex("2026-12-01"))))
	if err != nil {
		t.Fatal(err)
	}
	s := Subject{News: NewsDrift, Profile: testkit.DexProfile}
	for _, c := range []struct {
		at   time.Time
		want int
	}{
		{time.Date(2026, 11, 4, 0, 0, 0, 0, time.UTC), 1},
		{time.Date(2026, 12, 1, 23, 59, 59, 0, time.UTC), 1},
		{time.Date(2026, 12, 2, 0, 0, 0, 0, time.UTC), 0},
		{time.Date(2027, 6, 1, 0, 0, 0, 0, time.UTC), 0},
	} {
		if got, ok := rules.decide(s, c.at); !ok || got != c.want {
			t.Errorf("at %s the rule is %d, %v; want %d", c.at.Format(time.RFC3339), got, ok, c.want)
		}
	}
}

func TestAProfileRuleNamesTheProfileOrNameAndVersion(t *testing.T) {
	r := Rule{Level: LevelProfile, Match: testkit.DexProfile}
	if !r.covers(Subject{Profile: testkit.DexProfile}) || r.covers(Subject{Profile: "fred-identity@2.4.0"}) {
		t.Error("a rule with name@version covers that version only")
	}
	r.Match = "fred-identity"
	if !r.covers(Subject{Profile: "fred-identity@2.4.0"}) || r.covers(Subject{Profile: "fred-identity-two@1.0.0"}) {
		t.Error("a rule with a name covers each version of that profile only")
	}
	p := Rule{Level: LevelPack, Match: testkit.DexKey}
	if !p.covers(Subject{Packs: []string{testkit.DexKey}}) || p.covers(Subject{Packs: []string{"os/dex"}}) {
		t.Error("a pack rule with a key covers that key only")
	}
}

func TestAMuteBesideASendRuleAtOneLevelParsesAndTheLiveMuteWins(t *testing.T) {
	rules, err := Parse([]byte(testkit.OneChannel("a", "f", lostToA, muteLost("2026-12-01"), muteDex("2026-12-01"), digestToA)))
	if err != nil {
		t.Fatal(err)
	}
	lost := Subject{News: NewsLost}
	drift := Subject{News: NewsDrift, Profile: testkit.DexProfile}
	for _, c := range []struct {
		name string
		s    Subject
		at   time.Time
		want int
	}{
		{"lost, in the mute", lost, time.Date(2026, 12, 1, 23, 59, 59, 0, time.UTC), 1},
		{"lost, after the mute", lost, time.Date(2026, 12, 2, 0, 0, 0, 0, time.UTC), 0},
		{"drift of a profile, in the mute, with the send rule after it in the file", drift, now, 2},
		{"drift of a profile, after the mute", drift, time.Date(2026, 12, 2, 0, 0, 0, 0, time.UTC), 3},
	} {
		if got, ok := rules.decide(c.s, c.at); !ok || got != c.want {
			t.Errorf("%s: the rule is %d, %v; want %d", c.name, got, ok, c.want)
		}
	}
}

func TestTwoRulesThatSendForOneThingAreStillRefusedAndAMuteCountsInNoDuplicate(t *testing.T) {
	cases := []struct {
		name  string
		rules []string
		ok    bool
	}{
		{"two rules that send", []string{lostToA, lostToA}, false},
		{"a mute after a rule that sends", []string{lostToA, muteLost("2026-12-01")}, true},
		{"a mute before a rule that sends", []string{muteLost("2026-12-01"), lostToA}, true},
		{"two mutes", []string{muteLost("2026-12-01"), muteLost("2027-01-01")}, true},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			_, err := Parse([]byte(testkit.OneChannel("a", "f", c.rules...)))
			if (err == nil) != c.ok {
				t.Errorf(testkit.ErrorWantOK, err, c.ok)
			}
		})
	}
}
