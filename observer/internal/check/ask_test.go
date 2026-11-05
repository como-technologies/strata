package check

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// askable runs a versions cycle on the world of withBehind, whose first pin is kubernetes 1.35.3, and
// gives the pin the bump that `behind --pin` would read.
func askable(t *testing.T, bump docs.Bump) (*rig, *world, *Observer) {
	t.Helper()
	r, w := withBehind(t)
	i := slices.IndexFunc(w.behind.Pins, func(p docs.Pin) bool { return p.Name == "kubernetes" })
	if i < 0 {
		t.Fatal("the fixture has no kubernetes pin")
	}
	w.behind.Pins[i].Bump = &bump
	o := r.observer(w)
	if err := o.RunVersions(context.Background()); err != nil {
		t.Fatal(err)
	}
	return r, w, o
}

// pinned gives the arguments of each `behind` that the world got.
func (w *world) behindCalls() [][]string {
	w.mu.Lock()
	defer w.mu.Unlock()
	var out [][]string
	for _, c := range w.calls {
		if c[0] == runner.CmdBehind {
			out = append(out, c)
		}
	}
	return out
}

func TestEachCycleAsksForTheTextAndNoCycleCanSendAPost(t *testing.T) {
	r, w := withBehind(t)
	w.rows = []docs.VerifyRow{unchanged()}
	w.compares = map[int]docs.Diff{}
	o := r.observer(w)
	if _, err := o.Once(context.Background()); err != nil {
		t.Fatal(err)
	}
	if w.countMoves() == 0 || w.count(runner.CmdVerify) == 0 || len(w.behindCalls()) != 1 {
		t.Fatalf("the cycles ran %d moves, %d verifies, %d behinds", w.countMoves(), w.count(runner.CmdVerify), len(w.behindCalls()))
	}
	w.mu.Lock()
	defer w.mu.Unlock()
	for _, c := range w.calls {
		switch c[0] {
		case runner.CmdCompare, runner.CmdVerify:
			if !slices.Contains(c, runner.FlagFull) {
				t.Errorf("%v does not ask for the text", c)
			}
		case runner.CmdBehind:
			if !slices.Contains(c, runner.FlagNoImpact) || slices.Contains(c, runner.FlagPin) {
				t.Errorf("a cycle runs %v, which asks Palette", c)
			}
		}
		for _, banned := range []string{"--validate", "--repave", runner.FlagPin} {
			if slices.Contains(c, banned) && c[0] != runner.CmdBehind {
				t.Errorf("%v holds %s", c, banned)
			}
		}
	}
}

func TestAskRunsBehindForOnePackOnTheRecordOfTheProfileAndKeepsTheAnswer(t *testing.T) {
	r, w, o := askable(t, docs.Bump{To: "1.36.2", PaletteValidation: &docs.Validation{Accepts: true}})
	before := len(w.behindCalls())
	got, ok, err := o.Ask(context.Background(), moveSubject, "k8s/kubernetes")
	if err != nil || !ok || got.Verdict != model.VerdictNoError || got.Newest != "1.36.2" || got.Profile != moveSubject || got.Key != "k8s/kubernetes" {
		t.Fatalf("%+v %v %v", got, ok, err)
	}
	calls := w.behindCalls()
	if len(calls) != before+1 {
		t.Fatalf("the ask ran %d behind", len(calls)-before)
	}
	want := []string{runner.CmdBehind, "record-0.json", runner.FlagPin, "k8s/kubernetes", "--json", "--compact", "-q"}
	if last := calls[len(calls)-1]; !slices.Equal(last, want) {
		t.Errorf("the ask ran %v, want %v", last, want)
	}
	if again, _, _ := o.Ask(context.Background(), moveSubject, "k8s/kubernetes"); !reflect.DeepEqual(again, got) || len(w.behindCalls()) != before+1 {
		t.Errorf("a second ask ran again: %+v", again)
	}
	if names := entries(t, r.work); len(names) != 0 {
		t.Errorf("work holds %v", names)
	}
	if !strings.Contains(r.log.String(), "verdict=no-error") || strings.Contains(r.log.String(), "k8s/kubernetes") {
		t.Errorf("the log of the ask:\n%s", r.log.String())
	}
}

func TestAskReadsARefusalAndAFailedRequestAsTheirVerdicts(t *testing.T) {
	refusal := docs.Refusal{Pack: "kubernetes", Code: "DependencyPackUpgradeNotSupported", Message: "SENTINEL-MESSAGE", Clusters: []string{"payments"}}
	for _, c := range []struct {
		name    string
		bump    docs.Bump
		verdict model.Verdict
		status  int
	}{
		{"a refusal", docs.Bump{To: "1.36.2", PaletteValidation: &docs.Validation{Errors: []docs.Refusal{refusal}}}, model.VerdictRefuses, 0},
		{"a request that failed with 403", docs.Bump{To: "1.36.2", PaletteValidationError: "HTTP 403: SENTINEL-BODY"}, model.VerdictNoAnswer, 403},
		{"a request with no status", docs.Bump{To: "1.36.2", PaletteValidationError: "1 record of project p2 was not asked"}, model.VerdictNoAnswer, 0},
		{"a pin with no question", docs.Bump{To: "1.36.2"}, model.VerdictNoAnswer, 0},
	} {
		t.Run(c.name, func(t *testing.T) {
			r, _, o := askable(t, c.bump)
			got, ok, err := o.Ask(context.Background(), moveSubject, "k8s/kubernetes")
			if err != nil || !ok || got.Verdict != c.verdict || got.Status != c.status {
				t.Fatalf("%+v %v %v", got, ok, err)
			}
			if c.verdict == model.VerdictRefuses && (len(got.Refusals) != 1 || got.Refusals[0].Code != refusal.Code) {
				t.Errorf("refusals %+v", got.Refusals)
			}
			if strings.Contains(r.log.String(), "SENTINEL") {
				t.Errorf("the log holds the message:\n%s", r.log.String())
			}
		})
	}
}

func TestAskGivesNoAnswerForARunThatFailsAndTheNextAskTriesAgain(t *testing.T) {
	_, w, o := askable(t, docs.Bump{To: "1.36.2", PaletteValidation: &docs.Validation{Accepts: true}})
	w.mu.Lock()
	w.behindErr = &runner.RunError{Kind: docs.KindAPI, Status: 500}
	w.mu.Unlock()
	got, ok, err := o.Ask(context.Background(), moveSubject, "k8s/kubernetes")
	if err != nil || !ok || got.Verdict != model.VerdictNoAnswer || got.Status != 500 {
		t.Fatalf("%+v %v %v", got, ok, err)
	}
	w.mu.Lock()
	w.behindErr = nil
	w.mu.Unlock()
	if got, _, _ = o.Ask(context.Background(), moveSubject, "k8s/kubernetes"); got.Verdict != model.VerdictNoError {
		t.Errorf("%+v", got)
	}
}

func TestAskKnowsNoMoveForAProfileAPackOrAPinThatIsCurrent(t *testing.T) {
	_, w, o := askable(t, docs.Bump{To: "1.36.2", PaletteValidation: &docs.Validation{Accepts: true}})
	before := len(w.behindCalls())
	for _, c := range []struct{ subject, key string }{
		{"other@1.0.0", "k8s/kubernetes"},
		{moveSubject, "k8s/nope"},
		{moveSubject, "addon/kubernetes"},
		{moveSubject, ""},
	} {
		if _, ok, err := o.Ask(context.Background(), c.subject, c.key); ok || err != nil {
			t.Errorf("%v: %v %v", c, ok, err)
		}
	}
	if len(w.behindCalls()) != before {
		t.Error("an ask for a move that is not known ran behind")
	}
	if len(o.Answers()) != 0 {
		t.Errorf("answers %+v", o.Answers())
	}
}

func TestAskWaitsForTheWorkerAndEndsWithTheContext(t *testing.T) {
	_, _, o := askable(t, docs.Bump{To: "1.36.2", PaletteValidation: &docs.Validation{Accepts: true}})
	if err := o.acquire(context.Background()); err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, _, err := o.Ask(ctx, moveSubject, "k8s/kubernetes"); !errors.Is(err, context.Canceled) {
		t.Errorf("error %v", err)
	}
	o.release()
}

func TestAnAnswerStaysInMemoryAndTheStateHoldsNone(t *testing.T) {
	r, _, o := askable(t, docs.Bump{To: "1.36.2", PaletteValidation: &docs.Validation{Errors: []docs.Refusal{{Code: "RefusalCode", Message: "SENTINEL-MESSAGE"}}}})
	if _, _, err := o.Ask(context.Background(), moveSubject, "k8s/kubernetes"); err != nil {
		t.Fatal(err)
	}
	answers := o.Answers()
	if len(answers) != 1 || answers[0].Key != "k8s/kubernetes" {
		t.Fatalf("answers %+v", answers)
	}
	raw, err := os.ReadFile(filepath.Join(r.state, stateFile))
	if err != nil {
		t.Fatal(err)
	}
	if len(o.state.Moves) == 0 || strings.Contains(string(raw), "RefusalCode") || strings.Contains(string(raw), "refuses") {
		t.Errorf("the state of %d moves:\n%s", len(o.state.Moves), raw)
	}
	if left := r.restart(&world{}).Answers(); len(left) != 0 {
		t.Errorf("a restart kept %+v", left)
	}
}

// bigText is one byte above the cap.
func bigText() string {
	return strings.Repeat("x", maxText+1)
}

func TestAPackAboveTheCapKeepsNoTextOnAnySideAndSaysSo(t *testing.T) {
	r, w := withBehind(t)
	// The published side comes from verify, the cluster side from a compare of a record, and the newer side from a move.
	w.rows = []docs.VerifyRow{drift(t)}
	w.compares = map[int]docs.Diff{0: {RootB: "r2", Packs: []docs.Pack{changedPack("k8s/kubernetes")}}}
	for i := range w.rows[0].Diff.Packs {
		w.rows[0].Diff.Packs[i].Values.Unified = bigText()
	}
	w.compares[0].Packs[0].Values.Unified = bigText()
	key := move("kubernetes", "1.35.3", "1.36.2")
	moved := w.moves[key]
	moved.Packs = slices.Clone(moved.Packs)
	moved.Packs[0].Values.Unified = bigText()
	moved.Packs[0].Manifests = []docs.Manifest{{Op: "added", Name: "m", Unified: "small"}}
	w.moves[key] = moved
	w.cluster = testkit.Cluster("payments", moveSubject)
	o := r.observer(w)
	if _, err := o.Once(context.Background()); err != nil {
		t.Fatal(err)
	}
	for _, c := range []struct {
		side model.Side
		key  string
	}{{model.SidePublished, ""}, {model.SideCluster, "k8s/kubernetes"}, {model.SideNewer, "k8s/kubernetes"}} {
		key := c.key
		if key == "" {
			key = w.rows[0].Diff.Packs[0].Key
		}
		pack, ok, err := o.PackDiff(context.Background(), moveSubject, key, c.side)
		if err != nil || !ok {
			t.Fatalf("%s: %v %v", c.side, ok, err)
		}
		if !pack.TextTooLarge || pack.Values.Unified != "" {
			t.Errorf("%s keeps the text: %d bytes, too large %v", c.side, len(pack.Values.Unified), pack.TextTooLarge)
		}
		for _, m := range pack.Manifests {
			if m.Unified != "" || m.Name == "" || m.Op == "" {
				t.Errorf("%s: a manifest keeps %+v", c.side, m)
			}
		}
	}
}

func TestAPackAtTheCapKeepsItsText(t *testing.T) {
	r, w := edited(t)
	text := strings.Repeat("x", maxText)
	w.compares[0].Packs[0].Values.Unified, w.compares[0].Packs[0].Manifests = text, nil
	o := r.observer(w)
	if err := o.RunContent(context.Background()); err != nil {
		t.Fatal(err)
	}
	pack, ok, err := o.PackDiff(context.Background(), "aws-stack@1.1.0", "csi/csi-aws-ebs", model.SideCluster)
	if err != nil || !ok || pack.TextTooLarge || pack.Values.Unified != text {
		t.Errorf("%v %v too large %v, %d bytes", ok, err, pack.TextTooLarge, len(pack.Values.Unified))
	}
}
