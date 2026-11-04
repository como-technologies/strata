package check

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
)

const registryUID = "5eecc89d0b150045ae661cef"

func readDoc(t *testing.T, name string, v any) {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join("..", "docs", "testdata", name))
	if err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(raw, v); err != nil {
		t.Fatal(err)
	}
}

func move(name, from, to string) string {
	ref := "pack:" + registryUID + "/" + name + "@"
	return ref + from + " " + ref + to
}

// withBehind makes a world that answers `behind` with the real document of the stand-in: four pins
// are behind, and one of them is floating. The compare of each move answers with a real document.
func withBehind(t *testing.T) (*rig, *world) {
	t.Helper()
	r := newRig(t)
	r.record("aws-stack", "1.0.0", "k8s/kubernetes", "cni/cni-cilium-oss", "csi/csi-aws-ebs", "os/ubuntu-aws", "addon/cert-manager")
	r.meta(`{"baseline":"approved","revision":"4e1c7a2"}`)
	w := &world{moves: map[string]docs.Diff{}, behindExit: 1}
	readDoc(t, "behind.json", &w.behind)
	var kubernetes, cilium, other docs.Diff
	readDoc(t, "move-kubernetes.json", &kubernetes)
	readDoc(t, "move-cni-cilium-oss.json", &cilium)
	readDoc(t, "move-cert-manager.json", &other)
	w.moves[move("kubernetes", "1.35.3", "1.36.2")] = kubernetes
	w.moves[move("cni-cilium-oss", "1.19.6", "1.20.1")] = cilium
	w.moves[move("csi-aws-ebs", "1.63.1", "1.65.0")] = other
	w.moves[move("ubuntu-aws", "20.04", "24.04")] = other
	w.moveErrs = map[string]error{}
	return r, w
}

func TestTheVersionsCycleComparesEachMoveAndFillsTheRows(t *testing.T) {
	r, w := withBehind(t)
	o := r.observer(w)
	if err := o.RunVersions(context.Background()); err != nil {
		t.Fatal(err)
	}
	s := o.Summary()
	if len(s.Newer) != 4 || w.countMoves() != 4 || s.Counts.Newer != 4 || s.Counts.NewerProfiles != 1 || s.StrataVersion != "0.28.0" {
		t.Fatalf("rows %d, moves %d, counts %+v, strata %q", len(s.Newer), w.countMoves(), s.Counts, s.StrataVersion)
	}
	byName := map[string]model.Newer{}
	for _, n := range s.Newer {
		byName[n.Name] = n
	}
	k := byName["kubernetes"]
	if k.Newest != "1.36.2" || k.Impact != "repave" || k.Images != 8 || !slices.Equal(k.Flags, []string{docs.FlagIdentity, docs.FlagUpstream, docs.FlagAuthored, docs.FlagImages}) ||
		k.Fold.Settings+k.Fold.Follows+k.Fold.Repeats != 11 || !slices.Equal(k.Drops, []string{"arm64"}) || !slices.Equal(k.Profiles, []string{"aws-stack@1.0.0"}) {
		t.Errorf("kubernetes %+v", k)
	}
	if u := byName["ubuntu-aws"]; !u.Floating || u.Version != "20.04" || u.Newest != "24.04" {
		t.Errorf("a floating pin compares its pinned version with the newest: %+v", u)
	}
	if s.Versions.ReadAt == nil || s.Versions.FailedRuns != 0 {
		t.Errorf("versions %+v", s.Versions)
	}
}

func TestTheCacheAsksEachMoveOnce(t *testing.T) {
	r, w := withBehind(t)
	o := r.observer(w)
	for range 3 {
		if err := o.RunVersions(context.Background()); err != nil {
			t.Fatal(err)
		}
	}
	if got := w.countMoves(); got != 4 {
		t.Fatalf("%d compares of two packs after three cycles", got)
	}
	if got := w.count(runner.CmdBehind); got != 3 {
		t.Errorf("%d behind", got)
	}
}

func TestTheCacheOfTheMovesSurvivesARestartAndHoldsNoValue(t *testing.T) {
	r, w := withBehind(t)
	if err := r.observer(w).RunVersions(context.Background()); err != nil {
		t.Fatal(err)
	}
	if err := r.restart(w).RunVersions(context.Background()); err != nil {
		t.Fatal(err)
	}
	if got := w.countMoves(); got != 4 {
		t.Errorf("%d compares of two packs after a restart", got)
	}
	raw, err := os.ReadFile(filepath.Join(r.state, "state.json"))
	if err != nil {
		t.Fatal(err)
	}
	// The real compare of the Kubernetes pack changes the value v1.13.1 of dns.imageTag.
	for _, banned := range []string{"v1.13.1", "v1.14.2", "imageTag", "apiserver"} {
		if strings.Contains(string(raw), banned) {
			t.Errorf("the state holds %q", banned)
		}
	}
	if !strings.Contains(string(raw), "0.28.0 "+registryUID+"/kubernetes@1.35.3->1.36.2") {
		t.Errorf("the key holds no version of the binary: %s", raw)
	}
}

func TestANewBinaryAsksAgainAndTheOldKeysGo(t *testing.T) {
	r, w := withBehind(t)
	if err := r.observer(w).RunVersions(context.Background()); err != nil {
		t.Fatal(err)
	}
	w.behind.Tool.Version = "0.29.0"
	o := r.restart(w)
	if err := o.RunVersions(context.Background()); err != nil {
		t.Fatal(err)
	}
	if got := w.countMoves(); got != 8 {
		t.Errorf("%d compares of two packs", got)
	}
	state, err := loadState(r.state)
	if err != nil {
		t.Fatal(err)
	}
	for key := range state.Moves {
		if !strings.HasPrefix(key, "0.29.0 ") {
			t.Errorf("the key %q stays", key)
		}
	}
	if len(state.Moves) != 4 {
		t.Errorf("%d moves in the state", len(state.Moves))
	}
}

func TestACycleFailsWhenEachPinHasAnErrorAndTakesTheirKind(t *testing.T) {
	r, w := withBehind(t)
	for i := range w.behind.Pins {
		w.behind.Pins[i].Error = &docs.Error{Kind: docs.KindAPI, Status: 401}
	}
	w.behindExit = 2
	o := r.observer(w)
	_ = o.RunVersions(context.Background())
	_ = o.RunVersions(context.Background())
	s := o.Summary()
	if s.Versions.FailedRuns != 2 || s.Versions.LastError.Kind != docs.KindAPI || s.Versions.LastError.Status != 401 || w.countMoves() != 0 {
		t.Errorf("versions %+v", s.Versions)
	}
}

func TestAPinWithAnErrorBesideGoodPinsIsACause(t *testing.T) {
	r, w := withBehind(t)
	w.behind.Pins[0].Error = &docs.Error{Kind: docs.KindAPI, Status: 404}
	w.behindExit = 2
	o := r.observer(w)
	if err := o.RunVersions(context.Background()); err != nil {
		t.Fatal(err)
	}
	s := o.Summary()
	if s.Versions.FailedRuns != 0 || len(s.NotChecked) != 1 || s.NotChecked[0].Cause != model.PinError || s.NotChecked[0].Packs[0] != "addon/cert-manager" {
		t.Errorf("versions %+v, notChecked %+v", s.Versions, s.NotChecked)
	}
}

func TestAnOlderRegistryCopyIsACauseForEachRegistry(t *testing.T) {
	r, w := withBehind(t)
	w.behind.Pins[0].RegistrySync = &docs.RegistrySync{LastSyncedTime: "2026-10-30T09:00:00Z"}
	o := r.observer(w)
	if err := o.RunVersions(context.Background()); err != nil {
		t.Fatal(err)
	}
	rows := o.Summary().NotChecked
	if len(rows) != 1 || rows[0].Cause != model.OlderRegistryCopy || rows[0].Registry != "Public Repo" || rows[0].Packs[0] != "addon/cert-manager" {
		t.Errorf("notChecked %+v", rows)
	}
}
