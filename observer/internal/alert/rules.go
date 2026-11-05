// Package alert sends a message to a webhook when a finding opens and when it clears.
//
// It is a stopgap. Palette has no event for a published version that changed, so the observer sends
// one. The package reads the summary tier only: SubjectsOf is the one function that takes a type of
// internal/model, and the rest of the package works on Subject, which holds names, times, and fixed
// words. So a message holds no value. The package never imports internal/docs, and a test makes sure
// of it.
package alert

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"maps"
	"os"
	"slices"
	"time"
)

// Level is the part of the cluster that a rule is about. A lower level overrides a higher one.
type Level string

// The levels of a rule. A rule at the level pack or profile names what it applies to in match.
const (
	LevelCluster Level = "cluster"
	LevelProfile Level = "profile"
	LevelPack    Level = "pack"
)

// levels lists the levels from the lowest to the highest. A rule of an earlier level wins.
var levels = []Level{LevelPack, LevelProfile, LevelCluster}

// News is the kind of news that a rule is about.
type News string

// The kinds of news. Drift holds the published kinds and the cluster kinds of a finding. Image holds
// image-moved and image-split. Lost follows the page state lost, and Newer follows a newer version.
const (
	NewsDrift News = "drift"
	NewsImage News = "image"
	NewsLost  News = "lost"
	NewsNewer News = "newer"
)

var newsKinds = []News{NewsDrift, NewsImage, NewsLost, NewsNewer}

// Send says when a message goes. A rule with no Send is a mute.
type Send string

// The ways to send. A digest goes at most once in each period of the flag --digest-every.
const (
	SendNow    Send = "now"
	SendDigest Send = "digest"
)

// Channel is a place that a message goes to. The address is in a file, so that it can be a Secret.
type Channel struct {
	URLFile string `json:"urlFile"`
}

// Rule is one line of the file of the rules. A mute has MuteUntil and no Send. Any other rule has Send
// and To.
type Rule struct {
	Level     Level  `json:"level"`
	Match     string `json:"match"`
	News      News   `json:"news"`
	Send      Send   `json:"send"`
	To        string `json:"to"`
	MuteUntil string `json:"muteUntil"`
}

// Rules is the file of the rules, after the checks of Parse.
type Rules struct {
	Channels map[string]Channel
	List     []Rule
}

// mute says that the rule holds back messages and sends none.
func (r Rule) mute() bool {
	return r.Send == ""
}

// muteEnd gives the first instant that the mute does not hold. The mute holds through the whole day
// that MuteUntil names, in UTC.
func (r Rule) muteEnd() time.Time {
	day, err := time.Parse(time.DateOnly, r.MuteUntil)
	if err != nil {
		return time.Time{}
	}
	return day.AddDate(0, 0, 1)
}

// live says that the rule counts at this time. A mute that has ended counts as no mute.
func (r Rule) live(now time.Time) bool {
	return !r.mute() || now.Before(r.muteEnd())
}

// covers says that the rule is about this subject. A profile rule names a profile or name@version.
// A pack rule names the name or the key of a pack.
func (r Rule) covers(s Subject) bool {
	switch r.Level {
	case LevelProfile:
		return s.Profile == r.Match || profileName(s.Profile) == r.Match
	case LevelPack:
		return slices.ContainsFunc(s.Packs, func(key string) bool { return key == r.Match || packName(key) == r.Match })
	}
	return true
}

// decide finds the rule that decides what happens to a subject: the first live rule of the lowest level
// that is about the news and covers the subject. The second result is false when no rule does.
func (rs Rules) decide(s Subject, now time.Time) (int, bool) {
	for _, level := range levels {
		for i, r := range rs.List {
			if r.Level == level && r.News == s.News && r.live(now) && r.covers(s) {
				return i, true
			}
		}
	}
	return 0, false
}

// Names lists the channels, sorted.
func (rs Rules) Names() []string {
	return slices.Sorted(maps.Keys(rs.Channels))
}

// FileError is a fault in the file of the rules. Line is the line where the fault starts, counted from 1.
type FileError struct {
	Line int
	Msg  string
}

func (e *FileError) Error() string {
	return fmt.Sprintf("line %d: %s", e.Line, e.Msg)
}

// lineAt gives the line of a byte offset, counted from 1.
func lineAt(raw []byte, offset int) int {
	return 1 + bytes.Count(raw[:min(max(offset, 0), len(raw))], []byte("\n"))
}

func fail(line int, msg string) *FileError {
	return &FileError{Line: line, Msg: msg}
}

// Parse reads the file of the rules and refuses what the observer cannot use: a file that is no JSON,
// a field that it does not know, a level, news, or way to send that is not in the list, a rule for a
// channel that the file does not hold, a mute with no end date, and a newer version that pages a person.
// Each fault names its line.
func Parse(raw []byte) (Rules, error) {
	if err := json.Unmarshal(raw, new(json.RawMessage)); err != nil {
		var syntax *json.SyntaxError
		if errors.As(err, &syntax) {
			return Rules{}, fail(lineAt(raw, int(syntax.Offset)), "the file is not JSON")
		}
		return Rules{}, fail(1, "the file is not JSON")
	}
	p := &reader{raw: raw, dec: json.NewDecoder(bytes.NewReader(raw))}
	p.dec.DisallowUnknownFields()
	rules, err := p.file()
	if err != nil {
		return Rules{}, err
	}
	return rules, nil
}

// reader walks the file with a decoder, so that each fault has a line.
type reader struct {
	raw []byte
	dec *json.Decoder
}

// line gives the line of the next token. The decoder stops before a comma, so the reader skips the
// commas and the white space first.
func (p *reader) line() int {
	at := int(p.dec.InputOffset())
	for at < len(p.raw) && bytes.IndexByte([]byte(" \t\r\n,"), p.raw[at]) >= 0 {
		at++
	}
	return lineAt(p.raw, at)
}

// open reads the start of an object or an array.
func (p *reader) open(want json.Delim, msg string) error {
	line := p.line()
	if tok, err := p.dec.Token(); err != nil || tok != want {
		return fail(line, msg)
	}
	return nil
}

func (p *reader) file() (Rules, error) {
	rules := Rules{Channels: map[string]Channel{}}
	if err := p.open('{', "the file must hold one object"); err != nil {
		return rules, err
	}
	var lines []int
	for p.dec.More() {
		line := p.line()
		key, err := p.dec.Token()
		name, _ := key.(string)
		switch {
		case err != nil:
			return rules, fail(line, "the file is not JSON")
		case name == "channels":
			err = p.channels(&rules)
		case name == "rules":
			rules.List, lines, err = p.rules()
		default:
			err = fail(line, fmt.Sprintf("unknown field %q", name))
		}
		if err != nil {
			return rules, err
		}
	}
	return rules, rules.check(lines)
}

func (p *reader) channels(rules *Rules) error {
	if err := p.open('{', "channels must be an object"); err != nil {
		return err
	}
	for p.dec.More() {
		line := p.line()
		key, err := p.dec.Token()
		name, _ := key.(string)
		var c Channel
		if err != nil || name == "" {
			return fail(line, "a channel needs a name")
		}
		if err := p.dec.Decode(&c); err != nil {
			return fail(line, fmt.Sprintf("channel %q: %s", name, decodeFault(err)))
		}
		if c.URLFile == "" {
			return fail(line, fmt.Sprintf("channel %q needs urlFile", name))
		}
		rules.Channels[name] = c
	}
	_, err := p.dec.Token()
	return err
}

// rules reads the array of rules. lines holds the line where each rule starts.
func (p *reader) rules() ([]Rule, []int, error) {
	if err := p.open('[', "rules must be an array"); err != nil {
		return nil, nil, err
	}
	var list []Rule
	var lines []int
	for p.dec.More() {
		line := p.line()
		var r Rule
		if err := p.dec.Decode(&r); err != nil {
			return nil, nil, fail(line, fmt.Sprintf("rule %d: %s", len(list)+1, decodeFault(err)))
		}
		list, lines = append(list, r), append(lines, line)
	}
	_, err := p.dec.Token()
	return list, lines, err
}

// decodeFault says what is wrong with a value in words that name no value of the file.
func decodeFault(err error) string {
	var typeErr *json.UnmarshalTypeError
	if errors.As(err, &typeErr) {
		return fmt.Sprintf("the field %s has the wrong type", typeErr.Field)
	}
	return err.Error()
}

// check makes sure that each rule is complete and that no two rules are about the same thing.
func (rs Rules) check(lines []int) error {
	seen := map[[3]string]bool{}
	for i, r := range rs.List {
		if msg := rs.fault(r); msg != "" {
			return fail(lines[i], fmt.Sprintf("rule %d: %s", i+1, msg))
		}
		key := [3]string{string(r.Level), r.Match, string(r.News)}
		if seen[key] {
			return fail(lines[i], fmt.Sprintf("rule %d: another rule has the same level, match, and news", i+1))
		}
		seen[key] = true
	}
	return nil
}

// fault says what is wrong with one rule, or gives "" when nothing is.
func (rs Rules) fault(r Rule) string {
	switch {
	case !slices.Contains(levels, r.Level):
		return "level must be cluster, profile, or pack"
	case !slices.Contains(newsKinds, r.News):
		return "news must be drift, image, lost, or newer"
	case r.Level == LevelCluster && r.Match != "":
		return "a cluster rule has no match"
	case r.Level != LevelCluster && r.Match == "":
		return "a profile or pack rule needs match"
	case r.News == NewsLost && r.Level != LevelCluster:
		return "the news lost is about the cluster, so its level must be cluster"
	}
	return rs.faultOfSend(r)
}

func (rs Rules) faultOfSend(r Rule) string {
	if r.mute() {
		if _, err := time.Parse(time.DateOnly, r.MuteUntil); err != nil {
			return "a mute needs an end date: muteUntil as YYYY-MM-DD"
		}
		if r.To != "" {
			return "a mute has no to"
		}
		return ""
	}
	_, known := rs.Channels[r.To]
	switch {
	case r.Send != SendNow && r.Send != SendDigest:
		return "send must be now or digest"
	case r.MuteUntil != "":
		return "a rule that sends has no muteUntil: write a second rule for the mute"
	case !known:
		return "to must name a channel of the file"
	case r.News == NewsNewer && r.Send == SendNow:
		return "a newer version never pages a person, so send must be digest"
	}
	return ""
}

// Load reads the file of the rules at path. A fault in the file names the path and the line.
func Load(path string) (Rules, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return Rules{}, fmt.Errorf("%s: the file cannot be read", path)
	}
	rules, err := Parse(raw)
	if err != nil {
		return Rules{}, fmt.Errorf("%s: %w", path, err)
	}
	return rules, nil
}
