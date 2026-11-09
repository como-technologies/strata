package alert

import (
	"bytes"
	"context"
	"errors"
	"log/slog"
	"strings"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/model"
)

type message struct{ channel, text string }

// fake is a sender that keeps each message that went. A channel in fail refuses its messages.
type fake struct {
	sent []message
	fail map[string]error
}

func (f *fake) Send(_ context.Context, channel, text string) error {
	if err := f.fail[channel]; err != nil {
		return err
	}
	f.sent = append(f.sent, message{channel, text})
	return nil
}

// bench holds an engine with its sender and its store, and moves them one cycle at a time.
type bench struct {
	t     *testing.T
	e     *Engine
	send  *fake
	store Store
	log   *bytes.Buffer
	at    time.Time
}

func newBench(t *testing.T, rules string) *bench {
	t.Helper()
	parsed, err := Parse([]byte(rules))
	if err != nil {
		t.Fatal(err)
	}
	b := &bench{t: t, send: &fake{fail: map[string]error{}}, store: NewStore(), log: &bytes.Buffer{}, at: now}
	b.e = New(parsed, b.send, 7*24*time.Hour, slog.New(slog.NewTextHandler(b.log, nil)))
	return b
}

// cycle is one content cycle: it plans, delivers, and settles, as the observer does.
func (b *bench) cycle(subjects []Subject, known Known, held ...string) {
	b.t.Helper()
	heldSet := map[string]bool{}
	for _, id := range held {
		heldSet[id] = true
	}
	b.e.Plan(&b.store, subjects, known, heldSet, "quarry-1", b.at)
	queue := append([]Queued(nil), b.store.Outbox...)
	b.store.Settle(queue, b.e.Deliver(context.Background(), queue), b.at)
}

func (b *bench) later(d time.Duration) { b.at = b.at.Add(d) }

func (b *bench) texts() []string {
	var out []string
	for _, m := range b.send.sent {
		out = append(out, m.text)
	}
	return out
}

const oncall = `{"channels": {"oncall": {"urlFile": "f"}, "planning": {"urlFile": "g"}}, "rules": [
	{"level": "cluster", "news": "drift", "send": "now", "to": "oncall"},
	{"level": "cluster", "news": "lost", "send": "now", "to": "oncall"},
	{"level": "cluster", "news": "newer", "send": "digest", "to": "planning"}]}`

var firstSight = now.Add(-time.Hour)

var drift = Subject{ID: "d1", News: NewsDrift, Kind: model.PublishedChanged, Profile: "fred-identity@2.3.0", Packs: []string{"addon/dex"},
	Detail: model.PublishedChanged.Sentence(), Since: &firstSight}

var content = Known{Content: true}

func TestOneMessageWhenAFindingOpensNoneOnTheNextCycleAndOneWhenItClears(t *testing.T) {
	b := newBench(t, oncall)
	b.cycle([]Subject{drift}, content)
	if got := b.texts(); len(got) != 1 || !strings.HasPrefix(got[0], "strata · quarry-1 · Drift\nfred-identity@2.3.0: ") {
		t.Fatalf("the open message %q", got)
	}
	b.later(15 * time.Minute)
	b.cycle([]Subject{drift}, content)
	if len(b.send.sent) != 1 {
		t.Fatalf("the next cycle sent %d messages", len(b.send.sent))
	}
	b.later(15 * time.Minute)
	b.cycle(nil, content)
	got := b.texts()
	if len(got) != 2 || !strings.HasPrefix(got[1], "strata · quarry-1 · Drift cleared\nfred-identity@2.3.0\nA check proved that this is gone, at 2026-11-04T12:30:00Z.") {
		t.Fatalf("the clear message %q", got)
	}
	b.later(15 * time.Minute)
	b.cycle(nil, content)
	if len(b.send.sent) != 2 || len(b.store.Items) != 0 {
		t.Errorf("a cleared finding sent %d messages in all and left %d items", len(b.send.sent), len(b.store.Items))
	}
}

func TestAFailedReadSendsNoClearAndAHeldFindingStays(t *testing.T) {
	b := newBench(t, oncall)
	b.cycle([]Subject{drift}, content)
	b.later(time.Hour)
	// A content cycle that failed has no finding in its summary and knows none.
	b.cycle(nil, Known{})
	// A good cycle that could not read the part of the record that gives this finding holds it.
	b.cycle(nil, content, "d1")
	if len(b.send.sent) != 1 || len(b.store.Items) != 1 {
		t.Fatalf("a read that failed cleared the finding: %d messages, %d items", len(b.send.sent), len(b.store.Items))
	}
	b.cycle(nil, content)
	if len(b.send.sent) != 2 {
		t.Errorf("a read that works did not clear it: %q", b.texts())
	}
}

func TestAFailedSendKeepsTheMessageAndSendsItOnceAfterItWorks(t *testing.T) {
	b := newBench(t, oncall)
	b.send.fail["oncall"] = &SendError{Channel: "oncall", Kind: KindStatus, Status: 503}
	b.cycle([]Subject{drift}, content)
	b.later(15 * time.Minute)
	b.cycle([]Subject{drift}, content)
	if len(b.send.sent) != 0 || len(b.store.Outbox) != 1 || b.store.Items["d1"].Sent != nil {
		t.Fatalf("sent %d, waiting %d, item %+v", len(b.send.sent), len(b.store.Outbox), b.store.Items["d1"])
	}
	for _, want := range []string{"channel=oncall", "verdict=failed", "kind=status", "status=503"} {
		if !strings.Contains(b.log.String(), want) {
			t.Errorf("the log lacks %q: %s", want, b.log.String())
		}
	}
	delete(b.send.fail, "oncall")
	b.later(15 * time.Minute)
	b.cycle([]Subject{drift}, content)
	b.later(15 * time.Minute)
	b.cycle([]Subject{drift}, content)
	if len(b.send.sent) != 1 || len(b.store.Outbox) != 0 || b.store.Items["d1"].Sent == nil {
		t.Errorf("sent %d, waiting %d, item %+v", len(b.send.sent), len(b.store.Outbox), b.store.Items["d1"])
	}
}

func TestAFailedChannelKeepsTheOrderOfItsMessagesAndOtherChannelsGoOn(t *testing.T) {
	b := newBench(t, oncall)
	other := Subject{ID: "d2", News: NewsDrift, Kind: model.ClusterDiffers, Profile: "barney-base@1.5.0", Detail: "x"}
	b.send.fail["oncall"] = errors.New("down")
	b.cycle([]Subject{drift, other}, content)
	delete(b.send.fail, "oncall")
	b.cycle([]Subject{drift, other}, content)
	got := b.texts()
	if len(got) != 2 || !strings.Contains(got[0], "fred-identity") || !strings.Contains(got[1], "barney-base") {
		t.Errorf("the messages %q", got)
	}
}

func TestAFindingThatClearsBeforeItsMessageWentSendsNeither(t *testing.T) {
	b := newBench(t, oncall)
	b.send.fail["oncall"] = errors.New("down")
	b.cycle([]Subject{drift}, content)
	delete(b.send.fail, "oncall")
	b.cycle(nil, content)
	b.cycle(nil, content)
	if len(b.send.sent) != 0 || len(b.store.Outbox) != 0 {
		t.Errorf("sent %q, waiting %d", b.texts(), len(b.store.Outbox))
	}
}

func TestTheLossOfContactOpensOnAFailedCycleAndClearsOnlyAfterAGoodOne(t *testing.T) {
	b := newBench(t, oncall)
	lost := Subject{ID: lostID, News: NewsLost, Detail: "The observer cannot reach Palette. The last 2 content checks failed: api, HTTP 401."}
	b.cycle([]Subject{lost}, Known{})
	b.cycle([]Subject{lost}, Known{})
	b.cycle(nil, Known{})
	if got := b.texts(); len(got) != 1 || !strings.HasPrefix(got[0], "strata · quarry-1 · Lost contact\nThe observer cannot reach Palette.") {
		t.Fatalf("messages %q", got)
	}
	b.cycle(nil, content)
	if got := b.texts(); len(got) != 2 || !strings.Contains(got[1], "Lost contact cleared\nThe observer reached Palette again") {
		t.Errorf("messages %q", got)
	}
}

func TestAMuteSendsNothingAndTheMessageOfTheFindingGoesWhenTheMuteEnds(t *testing.T) {
	b := newBench(t, `{"channels": {"oncall": {"urlFile": "f"}}, "rules": [
		{"level": "cluster", "news": "drift", "send": "now", "to": "oncall"},
		{"level": "profile", "match": "fred-identity", "news": "drift", "muteUntil": "2026-11-05"}]}`)
	b.cycle([]Subject{drift}, content)
	b.later(24 * time.Hour)
	b.cycle([]Subject{drift}, content)
	if len(b.send.sent) != 0 || len(b.store.Items) != 0 {
		t.Fatalf("a mute let %d messages go", len(b.send.sent))
	}
	b.later(24 * time.Hour)
	b.cycle([]Subject{drift}, content)
	if len(b.send.sent) != 1 {
		t.Errorf("after the mute the finding sent %d messages", len(b.send.sent))
	}
}

func TestAMuteHoldsBackTheClearOfAMessageThatWent(t *testing.T) {
	b := newBench(t, `{"channels": {"oncall": {"urlFile": "f"}}, "rules": [
		{"level": "cluster", "news": "drift", "send": "now", "to": "oncall"},
		{"level": "profile", "match": "fred-identity", "news": "drift", "muteUntil": "2026-12-01"}]}`)
	b.store.Items["d1"] = Item{Subject: drift, To: "oncall", Sent: &now}
	b.cycle(nil, content)
	if len(b.send.sent) != 0 || len(b.store.Items) != 0 {
		t.Errorf("sent %q, items %d", b.texts(), len(b.store.Items))
	}
}

func TestADigestGoesOncePerPeriodListsTheOpenItemsAndHasNoClear(t *testing.T) {
	b := newBench(t, oncall)
	newer := func(id, profile string) Subject {
		return Subject{ID: id, News: NewsNewer, Profile: profile, Packs: []string{"dex"}, Detail: "dex@2.42.0 has a newer version, dex@2.44.0."}
	}
	subjects := []Subject{newer("n1", "fred-identity@2.3.0"), newer("n2", "barney-base@1.5.0")}
	known := Known{Content: true, Newer: true}
	b.cycle(subjects, known)
	if len(b.send.sent) != 1 || b.send.sent[0].channel != "planning" {
		t.Fatalf("messages %+v", b.send.sent)
	}
	text := b.send.sent[0].text
	if !strings.HasPrefix(text, "strata · quarry-1 · Digest\n2 open items:\n- Newer version · barney-base@1.5.0: ") || !strings.Contains(text, "\n- Newer version · fred-identity@2.3.0: ") {
		t.Errorf("the digest %q", text)
	}
	if it := b.store.Items["n1"]; it.Sent == nil {
		t.Error("a listed item is not sent")
	}
	b.later(6 * 24 * time.Hour)
	b.cycle(subjects, known)
	if len(b.send.sent) != 1 {
		t.Fatalf("the digest went before its period: %d messages", len(b.send.sent))
	}
	b.later(24 * time.Hour)
	b.cycle(subjects[:1], known)
	if len(b.send.sent) != 2 || !strings.Contains(b.send.sent[1].text, "1 open item:") {
		t.Fatalf("messages %q", b.texts())
	}
	b.later(8 * 24 * time.Hour)
	b.cycle(nil, known)
	if len(b.send.sent) != 2 || len(b.store.Items) != 0 {
		t.Errorf("a newer version that went sent a message when it cleared: %q", b.texts())
	}
}

func TestADigestWithNothingToListWaitsForTheFirstItem(t *testing.T) {
	b := newBench(t, oncall)
	known := Known{Content: true, Newer: true}
	b.cycle(nil, known)
	b.later(30 * 24 * time.Hour)
	b.cycle([]Subject{{ID: "n1", News: NewsNewer, Profile: "fred-identity@2.3.0", Detail: "x"}}, known)
	if len(b.send.sent) != 1 {
		t.Errorf("messages %q", b.texts())
	}
}

func TestANewerVersionIsNotClearedByACycleThatDidNotReadVersions(t *testing.T) {
	b := newBench(t, oncall)
	n := Subject{ID: "n1", News: NewsNewer, Profile: "fred-identity@2.3.0", Detail: "x"}
	b.cycle([]Subject{n}, Known{Content: true, Newer: true})
	b.cycle(nil, Known{Content: true})
	if len(b.store.Items) != 1 {
		t.Error("a cycle that read no version cleared a newer version")
	}
}

func TestTheStoreDropsTheMessagesOfAChannelThatTheRulesLackAndKeepsAtMostOneHundred(t *testing.T) {
	b := newBench(t, oncall)
	b.store.Outbox = []Queued{{Channel: "gone", Text: "x"}, {Channel: "oncall", Text: "y"}}
	b.send.fail["oncall"] = errors.New("down")
	b.cycle(nil, content)
	if len(b.store.Outbox) != 1 || b.store.Outbox[0].Channel != "oncall" {
		t.Errorf("outbox %+v", b.store.Outbox)
	}
	for i := 0; i < 3*maxQueued; i++ {
		b.store.queue(Queued{Channel: "oncall", Text: "z"})
	}
	if len(b.store.Outbox) != maxQueued {
		t.Errorf("the outbox holds %d messages", len(b.store.Outbox))
	}
}

func TestAnItemWhoseOpenMessageWasDroppedAsksAgain(t *testing.T) {
	b := newBench(t, oncall)
	b.store.Items["d1"] = Item{Subject: drift, To: "oncall"}
	b.cycle([]Subject{drift}, content)
	if len(b.send.sent) != 1 {
		t.Errorf("messages %q", b.texts())
	}
}

func TestACycleWithNoRuleForTheNewsSendsNothing(t *testing.T) {
	b := newBench(t, `{"channels": {"oncall": {"urlFile": "f"}}, "rules": [{"level": "cluster", "news": "lost", "send": "now", "to": "oncall"}]}`)
	b.cycle([]Subject{drift}, content)
	b.cycle(nil, content)
	if len(b.send.sent) != 0 || len(b.store.Items) != 0 {
		t.Errorf("messages %q", b.texts())
	}
}

func TestTheViewShowsFiringPendingInactiveAndMuted(t *testing.T) {
	b := newBench(t, `{"channels": {"oncall": {"urlFile": "f"}, "planning": {"urlFile": "g"}}, "rules": [
		{"level": "cluster", "news": "drift", "send": "now", "to": "oncall"},
		{"level": "cluster", "news": "image", "send": "now", "to": "oncall"},
		{"level": "cluster", "news": "newer", "send": "digest", "to": "planning"},
		{"level": "profile", "match": "barney-base", "news": "newer", "muteUntil": "2026-12-01"},
		{"level": "profile", "match": "wilma-gateway", "news": "newer", "muteUntil": "2026-11-01"}]}`)
	n := Subject{ID: "n1", News: NewsNewer, Profile: "fred-identity@2.3.0", Detail: "x"}
	muted := Subject{ID: "n2", News: NewsNewer, Profile: "barney-base@1.5.0", Detail: "x"}
	b.cycle([]Subject{drift, n, muted}, Known{Content: true, Newer: true})
	// The digest went, so the newer version fires. A second newer version waits for the next digest.
	later := Subject{ID: "n3", News: NewsNewer, Profile: "betty-observability@4.2.1", Detail: "x"}
	subjects := []Subject{drift, n, muted, later}
	want := map[int]State{0: StateFiring, 1: StateInactive, 2: StateFiring, 3: StateMuted, 4: StateInactive}
	v := b.e.View(b.store, subjects, "quarry-1", b.at)
	for i, r := range v.Rules {
		if r.State != want[i] {
			t.Errorf("rule %d is %s, want %s", i, r.State, want[i])
		}
	}
	// A subject that no message covers yet is pending.
	v = b.e.View(NewStore(), []Subject{drift}, "quarry-1", b.at)
	if v.Rules[0].State != StatePending {
		t.Errorf("a finding with no message is %s", v.Rules[0].State)
	}
	if !v.Set || v.DigestEvery != "168h" || len(v.Channels) != 2 || v.Channels[0] != "oncall" || v.Sample != Sample("quarry-1") {
		t.Errorf("view %+v", v)
	}
}

func TestTheUnsetViewHasNoRuleAndNoChannel(t *testing.T) {
	if v := Unset(); v.Set || v.Rules == nil || v.Channels == nil || len(v.Rules) != 0 {
		t.Errorf("view %+v", v)
	}
}

// rulesNow gives the bench other rules, as a team does when it applies a new file and the pod restarts. The
// store stays.
func (b *bench) rulesNow(rules string) {
	b.t.Helper()
	parsed, err := Parse([]byte(rules))
	if err != nil {
		b.t.Fatal(err)
	}
	b.e = New(parsed, b.send, 7*24*time.Hour, slog.New(slog.NewTextHandler(b.log, nil)))
}

func TestAMuteOrTheLossOfARuleDropsTheMessageOfTheOpenThatWaitsForARetry(t *testing.T) {
	muted := `{"channels": {"oncall": {"urlFile": "f"}}, "rules": [
		{"level": "cluster", "news": "drift", "send": "now", "to": "oncall"},
		{"level": "profile", "match": "fred-identity", "news": "drift", "muteUntil": "2026-12-01"}]}`
	none := `{"channels": {"oncall": {"urlFile": "f"}}, "rules": [{"level": "cluster", "news": "lost", "send": "now", "to": "oncall"}]}`
	for name, rules := range map[string]string{"a mute": muted, "no rule": none} {
		t.Run(name, func(t *testing.T) {
			b := newBench(t, oncall)
			b.send.fail["oncall"] = errors.New("down")
			b.cycle([]Subject{drift}, content)
			if len(b.store.Outbox) != 1 {
				t.Fatalf("outbox %+v", b.store.Outbox)
			}
			b.rulesNow(rules)
			delete(b.send.fail, "oncall")
			b.cycle([]Subject{drift}, content)
			b.cycle([]Subject{drift}, content)
			if len(b.send.sent) != 0 || len(b.store.Outbox) != 0 {
				t.Errorf("sent %q, outbox %+v", b.texts(), b.store.Outbox)
			}
		})
	}
}

func TestAMuteThatEndsLetsTheFindingQueueItsOpenAgain(t *testing.T) {
	b := newBench(t, `{"channels": {"oncall": {"urlFile": "f"}}, "rules": [
		{"level": "cluster", "news": "drift", "send": "now", "to": "oncall"},
		{"level": "profile", "match": "fred-identity", "news": "drift", "muteUntil": "2026-11-04"}]}`)
	b.send.fail["oncall"] = errors.New("down")
	b.rulesNow(`{"channels": {"oncall": {"urlFile": "f"}}, "rules": [{"level": "cluster", "news": "drift", "send": "now", "to": "oncall"}]}`)
	b.cycle([]Subject{drift}, content)
	delete(b.send.fail, "oncall")
	b.cycle([]Subject{drift}, content)
	if len(b.send.sent) != 1 {
		t.Errorf("sent %q", b.texts())
	}
}

func TestTheClearGoesToTheChannelOfTheOpenMessageAndToNoChannelThatTheRulesLack(t *testing.T) {
	b := newBench(t, oncall)
	b.cycle([]Subject{drift}, content)
	b.rulesNow(`{"channels": {"oncall": {"urlFile": "f"}, "other": {"urlFile": "g"}}, "rules": [{"level": "cluster", "news": "drift", "send": "now", "to": "other"}]}`)
	b.cycle(nil, content)
	if len(b.send.sent) != 2 || b.send.sent[1].channel != "oncall" {
		t.Fatalf("messages %+v", b.send.sent)
	}
	again := newBench(t, oncall)
	again.cycle([]Subject{drift}, content)
	again.rulesNow(`{"channels": {"other": {"urlFile": "g"}}, "rules": [{"level": "cluster", "news": "drift", "send": "now", "to": "other"}]}`)
	again.cycle(nil, content)
	if len(again.send.sent) != 1 || len(again.store.Outbox) != 0 {
		t.Errorf("a channel that the rules lack got a message: %+v", again.send.sent)
	}
}

func TestADigestThatWaitsIsMadeAgainFromTheItemsThatAreOpenAndNeverTwice(t *testing.T) {
	b := newBench(t, oncall)
	item := func(id, profile string) Subject {
		return Subject{ID: id, News: NewsNewer, Profile: profile, Packs: []string{"dex"}, Detail: "x"}
	}
	known := Known{Content: true, Newer: true}
	b.send.fail["planning"] = errors.New("down")
	all := []Subject{item("n1", "a@1"), item("n2", "b@1"), item("n3", "c@1")}
	b.cycle(all, known)
	// one item clears while the channel is down, and a second period passes
	b.later(8 * 24 * time.Hour)
	b.cycle([]Subject{all[0], all[2]}, known)
	if len(b.store.Outbox) != 1 || !strings.Contains(b.store.Outbox[0].Text, "2 open items:") || strings.Contains(b.store.Outbox[0].Text, "b@1") {
		t.Fatalf("outbox %+v", b.store.Outbox)
	}
	delete(b.send.fail, "planning")
	b.cycle([]Subject{all[0], all[2]}, known)
	if len(b.send.sent) != 1 || strings.Contains(b.send.sent[0].text, "b@1") || len(b.store.Outbox) != 0 {
		t.Errorf("sent %+v", b.send.sent)
	}
}

func TestAnImageFindingIsNotClearedByACycleThatReadNoPod(t *testing.T) {
	b := newBench(t, `{"channels": {"oncall": {"urlFile": "f"}}, "rules": [{"level": "cluster", "news": "image", "send": "now", "to": "oncall"}]}`)
	moved := Subject{ID: "i1", News: NewsImage, Kind: model.ImageMoved, Profile: "wilma-gateway@3.1.2", Packs: []string{"addon/traefik"}, Detail: "x"}
	b.cycle([]Subject{moved}, Known{Content: true, Image: true})
	b.cycle(nil, Known{Content: true})
	if len(b.send.sent) != 1 || len(b.store.Items) != 1 {
		t.Fatalf("a cycle that read no pod cleared the finding: %q", b.texts())
	}
	b.cycle(nil, Known{Content: true, Image: true})
	if len(b.send.sent) != 2 || len(b.store.Items) != 0 {
		t.Errorf("a cycle that read the pods did not clear it: %q", b.texts())
	}
}

func TestAnItemKeepsWhatItsSubjectSaysNowForTheClearAndTheDigest(t *testing.T) {
	b := newBench(t, oncall)
	first := drift
	first.Packs = []string{"addon/dex"}
	b.cycle([]Subject{first}, content)
	second := drift
	second.Packs = []string{"addon/dex", "addon/cert"}
	b.cycle([]Subject{second}, content)
	if got := b.store.Items["d1"].Packs; len(got) != 2 {
		t.Errorf("packs %v", got)
	}
}

func TestADigestThatOneChannelMissedIsMadeAgainForThatChannelOnly(t *testing.T) {
	b := newBench(t, `{"channels": {"planning": {"urlFile": "f"}, "platform": {"urlFile": "g"}}, "rules": [
		{"level": "cluster", "news": "newer", "send": "digest", "to": "planning"},
		{"level": "profile", "match": "barney-base", "news": "newer", "send": "digest", "to": "platform"}]}`)
	item := func(id, profile string) Subject {
		return Subject{ID: id, News: NewsNewer, Profile: profile, Packs: []string{"dex"}, Detail: "x"}
	}
	subjects := []Subject{item("n1", "fred-identity@2.3.0"), item("n2", "barney-base@1.5.0")}
	known := Known{Content: true, Newer: true}
	sentTo := func(channel string) int {
		n := 0
		for _, m := range b.send.sent {
			if m.channel == channel {
				n++
			}
		}
		return n
	}
	b.send.fail["platform"] = errors.New("down")
	for range 3 {
		b.cycle(subjects, known)
		b.later(15 * time.Minute)
	}
	if sentTo("planning") != 1 || sentTo("platform") != 0 || len(b.store.Outbox) != 1 || b.store.Outbox[0].Channel != "platform" {
		t.Fatalf("while platform is down: planning got %d, platform got %d, outbox %+v", sentTo("planning"), sentTo("platform"), b.store.Outbox)
	}
	delete(b.send.fail, "platform")
	for range 2 {
		b.cycle(subjects, known)
		b.later(15 * time.Minute)
	}
	if sentTo("planning") != 1 || sentTo("platform") != 1 || len(b.store.Outbox) != 0 {
		t.Errorf("after platform came back: planning got %d, platform got %d, outbox %+v", sentTo("planning"), sentTo("platform"), b.store.Outbox)
	}
}

func TestAMuteBesideASendRuleAtOneLevelHoldsBackTheMessageUntilItsEndAndThenTheRuleSends(t *testing.T) {
	b := newBench(t, `{"channels": {"oncall": {"urlFile": "f"}}, "rules": [
		{"level": "cluster", "news": "lost", "send": "now", "to": "oncall"},
		{"level": "cluster", "news": "lost", "muteUntil": "2026-11-05"}]}`)
	lost := Subject{ID: lostID, News: NewsLost, Detail: "The observer cannot reach Palette."}
	b.cycle([]Subject{lost}, content)
	if v := b.e.View(b.store, []Subject{lost}, "quarry-1", b.at); len(b.send.sent) != 0 || v.Rules[0].State != StateInactive || v.Rules[1].State != StateMuted {
		t.Fatalf("sent %q, view %+v", b.texts(), v.Rules)
	}
	b.later(24 * time.Hour)
	b.cycle([]Subject{lost}, content)
	if len(b.send.sent) != 0 {
		t.Fatalf("the mute let %q go on its last day", b.texts())
	}
	b.later(24 * time.Hour)
	b.cycle([]Subject{lost}, content)
	if got := b.texts(); len(got) != 1 || !strings.HasPrefix(got[0], "strata · quarry-1 · Lost contact\n") {
		t.Errorf("after the mute the rule sent %q", got)
	}
	if v := b.e.View(b.store, []Subject{lost}, "quarry-1", b.at); v.Rules[0].State != StateFiring || v.Rules[1].State != StateInactive {
		t.Errorf("view %+v", v.Rules)
	}
}
