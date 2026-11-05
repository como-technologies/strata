package alert

import (
	"errors"
	"strings"
	"testing"
	"time"
)

// theFile is the file of the plan of the item: a webhook for the team on call, rules at each level, and
// a mute with an end date.
const theFile = `{
  "channels": {"platform-oncall": {"urlFile": "/alert-channels/platform-oncall"}},
  "rules": [
    {"level": "cluster", "news": "drift", "send": "now", "to": "platform-oncall"},
    {"level": "profile", "match": "fred-identity", "news": "drift", "send": "now", "to": "platform-oncall"},
    {"level": "cluster", "news": "newer", "send": "digest", "to": "platform-oncall"},
    {"level": "profile", "match": "barney-base", "news": "newer", "muteUntil": "2026-12-01"}
  ]
}`

func TestTheFileOfThePlanParses(t *testing.T) {
	rules, err := Parse([]byte(theFile))
	if err != nil {
		t.Fatal(err)
	}
	if len(rules.List) != 4 || rules.List[1].Match != "fred-identity" || !rules.List[3].mute() || rules.Channels["platform-oncall"].URLFile != "/alert-channels/platform-oncall" {
		t.Errorf("rules %+v", rules)
	}
	if got := rules.Names(); len(got) != 1 || got[0] != "platform-oncall" {
		t.Errorf("channels %v", got)
	}
}

// fileWith puts a rule in a file that has the channel platform-oncall.
func fileWith(rule string) string {
	return "{\n\"channels\": {\"platform-oncall\": {\"urlFile\": \"f\"}},\n\"rules\": [\n" + rule + "\n]\n}"
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
		{"a rule with a field that is not known", fileWith(`{"level": "cluster", "news": "drift", "sned": "now", "to": "platform-oncall"}`), 4, "rule 1"},
		{"a field of the wrong type", fileWith(`{"level": "cluster", "news": 4, "send": "now", "to": "platform-oncall"}`), 4, "the wrong type"},
		{"a level that is not in the list", fileWith(`{"level": "node", "news": "drift", "send": "now", "to": "platform-oncall"}`), 4, "level must be"},
		{"news that is not in the list", fileWith(`{"level": "cluster", "news": "health", "send": "now", "to": "platform-oncall"}`), 4, "news must be"},
		{"a profile rule with no match", fileWith(`{"level": "profile", "news": "drift", "send": "now", "to": "platform-oncall"}`), 4, "needs match"},
		{"a cluster rule with a match", fileWith(`{"level": "cluster", "match": "x", "news": "drift", "send": "now", "to": "platform-oncall"}`), 4, "no match"},
		{"lost below the cluster", fileWith(`{"level": "profile", "match": "x", "news": "lost", "send": "now", "to": "platform-oncall"}`), 4, "about the cluster"},
		{"a way to send that is not in the list", fileWith(`{"level": "cluster", "news": "drift", "send": "soon", "to": "platform-oncall"}`), 4, "send must be"},
		{"a rule for a channel that the file lacks", fileWith(`{"level": "cluster", "news": "drift", "send": "now", "to": "nobody"}`), 4, "name a channel"},
		{"a mute with no end date", fileWith(`{"level": "profile", "match": "barney-base", "news": "newer"}`), 4, "end date"},
		{"a mute with an end that is no date", fileWith(`{"level": "profile", "match": "barney-base", "news": "newer", "muteUntil": "soon"}`), 4, "end date"},
		{"a mute with a channel", fileWith(`{"level": "profile", "match": "barney-base", "news": "newer", "muteUntil": "2026-12-01", "to": "platform-oncall"}`), 4, "no to"},
		{"a rule that sends and mutes", fileWith(`{"level": "cluster", "news": "drift", "send": "now", "to": "platform-oncall", "muteUntil": "2026-12-01"}`), 4, "second rule"},
		{"a newer version that pages a person", fileWith(`{"level": "cluster", "news": "newer", "send": "now", "to": "platform-oncall"}`), 4, "never pages"},
		{"two rules for one thing", fileWith("{\"level\": \"cluster\", \"news\": \"drift\", \"send\": \"now\", \"to\": \"platform-oncall\"},\n{\"level\": \"cluster\", \"news\": \"drift\", \"send\": \"digest\", \"to\": \"platform-oncall\"}"), 5, "rule 2"},
		{"a channel with no file", "{\n\"channels\": {\"a\": {}}\n}", 2, "needs urlFile"},
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
	_, err := Parse([]byte(fileWith(`{"level": "SECRET-LEVEL", "news": "drift", "send": "now", "to": "platform-oncall"}`)))
	if err == nil || strings.Contains(err.Error(), "SECRET") {
		t.Errorf("error %v", err)
	}
}

var now = time.Date(2026, 11, 4, 12, 0, 0, 0, time.UTC)

func TestALowerLevelOverridesAHigherOne(t *testing.T) {
	rules, err := Parse([]byte(`{"channels": {"a": {"urlFile": "f"}}, "rules": [
		{"level": "cluster", "news": "drift", "send": "now", "to": "a"},
		{"level": "pack", "match": "dex", "news": "drift", "muteUntil": "2026-12-01"},
		{"level": "profile", "match": "fred-identity", "news": "drift", "send": "digest", "to": "a"}]}`))
	if err != nil {
		t.Fatal(err)
	}
	cases := []struct {
		name string
		s    Subject
		want int
		ok   bool
	}{
		{"the cluster rule takes any other profile", Subject{News: NewsDrift, Profile: "barney-base@1.5.0", Packs: []string{"addon/kyverno"}}, 0, true},
		{"the profile rule overrides the cluster rule", Subject{News: NewsDrift, Profile: "fred-identity@2.3.0", Packs: []string{"addon/cert"}}, 2, true},
		{"the pack rule overrides the profile rule", Subject{News: NewsDrift, Profile: "fred-identity@2.3.0", Packs: []string{"addon/dex"}}, 1, true},
		{"a pack rule takes a finding that names the pack among others", Subject{News: NewsDrift, Profile: "barney-base@1.5.0", Packs: []string{"addon/cert", "addon/dex"}}, 1, true},
		{"a rule is about its own news", Subject{News: NewsImage, Profile: "fred-identity@2.3.0", Packs: []string{"addon/dex"}}, 0, false},
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
	rules, err := Parse([]byte(`{"channels": {"a": {"urlFile": "f"}}, "rules": [
		{"level": "cluster", "news": "drift", "send": "now", "to": "a"},
		{"level": "profile", "match": "fred-identity", "news": "drift", "muteUntil": "2026-12-01"}]}`))
	if err != nil {
		t.Fatal(err)
	}
	s := Subject{News: NewsDrift, Profile: "fred-identity@2.3.0"}
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
	r := Rule{Level: LevelProfile, Match: "fred-identity@2.3.0"}
	if !r.covers(Subject{Profile: "fred-identity@2.3.0"}) || r.covers(Subject{Profile: "fred-identity@2.4.0"}) {
		t.Error("a rule with name@version covers that version only")
	}
	r.Match = "fred-identity"
	if !r.covers(Subject{Profile: "fred-identity@2.4.0"}) || r.covers(Subject{Profile: "fred-identity-two@1.0.0"}) {
		t.Error("a rule with a name covers each version of that profile only")
	}
	p := Rule{Level: LevelPack, Match: "addon/dex"}
	if !p.covers(Subject{Packs: []string{"addon/dex"}}) || p.covers(Subject{Packs: []string{"os/dex"}}) {
		t.Error("a pack rule with a key covers that key only")
	}
}
