package check

import (
	"context"
	"encoding/json"
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
	if len(s.Newer) != 4 || w.countMoves() != 4 || s.Counts.Newer != 4 || s.Counts.NewerProfiles != 1 || s.StrataVersion != w.behind.Tool.Version {
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
	if !strings.Contains(string(raw), w.behind.Tool.Version+" "+registryUID+"/kubernetes@1.35.3->1.36.2") {
		t.Errorf("the key holds no version of the binary: %s", raw)
	}
}

func TestANewBinaryAsksAgainAndTheOldKeysGo(t *testing.T) {
	r, w := withBehind(t)
	if err := r.observer(w).RunVersions(context.Background()); err != nil {
		t.Fatal(err)
	}
	w.behind.Tool.Version = "99.0.0"
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
		if !strings.HasPrefix(key, "99.0.0 ") {
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

// Issue #708. The versions cycle compares the record of each profile version that is behind with the
// newest version of its profile, by the uid of that version. The row holds the move, and the newest
// side of the detail tier gives a pack of the diff with its values. A compare that fails names the
// newer version with no move, and the other versions keep theirs. Each cycle compares again, because
// a published version can change in place, and the state holds no move of a profile.
func TestTheVersionsCycleComparesTheRecordOfAProfileWithItsNewestVersion(t *testing.T) {
	r, w := withBehind(t)
	r.record("barney-addons", "1.3.1", "addon/fred-compositions")
	r.record("barney-base", "1.5.0", "csi/csi-aws-ebs")
	r.record("wilma-gateway", "3.1.2", "addon/cert-manager")
	held := func(name, version, newest, uid string) docs.ProfilePin {
		p := docs.ProfilePin{Name: name, Version: version, PinnedBy: []docs.PinnedBy{{Subject: name + "@" + version}}}
		if newest != "" {
			p.Behind, p.Newer = true, []docs.NewerProfile{{Version: newest, UID: uid}}
		}
		return p
	}
	w.behind.Profiles = []docs.ProfilePin{held("barney-addons", "1.3.1", "1.4.0", "u-140"), held("barney-base", "1.5.0", "1.6.0", "u-160"), held("wilma-gateway", "3.1.2", "", "")}
	fred := docs.Pack{Key: "addon/fred-compositions", Status: docs.PackChanged, Identity: true, A: &docs.PackSide{Version: "0.1.0"}, B: &docs.PackSide{Version: "0.2.0"},
		Values: docs.Values{Changed: true, Diff: []docs.Leaf{{Op: "changed", Path: "compositions", Old: sentinel, New: "x"}}}}
	w.profileMoves = map[string]docs.Diff{"barney-addons@1.3.1 u-140": {Summary: docs.DiffSummary{MaxImpact: "in-place"}, Packs: []docs.Pack{fred}}}
	w.profileMoveErrs = map[string]error{"barney-base@1.5.0 u-160": failure(docs.KindAPI)}
	// the cluster runs the three versions: a version that it does not run gets no newer version
	w.cluster = testkit.Cluster("payments", "barney-addons@1.3.1", "barney-base@1.5.0", "wilma-gateway@3.1.2")
	o := r.observer(w)
	ctx := context.Background()
	// the rows of the profiles come from a content cycle
	if err := o.RunContent(ctx); err != nil {
		t.Fatal(err)
	}
	if err := o.RunVersions(ctx); err != nil {
		t.Fatal(err)
	}
	byName := map[string]model.Profile{}
	for _, p := range o.Summary().Profiles {
		byName[p.Name] = p
	}
	addons, base, gateway := byName["barney-addons"].NewerVersion, byName["barney-base"].NewerVersion, byName["wilma-gateway"].NewerVersion
	want := model.MovePack{Key: "addon/fred-compositions", Status: docs.PackChanged, From: "0.1.0", To: "0.2.0", Flags: []string{docs.FlagIdentity}, ValuesChanged: true}
	if addons == nil || addons.Move == nil || addons.Move.Impact != "in-place" || len(addons.Move.Packs) != 1 || !reflect.DeepEqual(addons.Move.Packs[0], want) {
		t.Fatalf("barney-addons: %+v", addons)
	}
	if base == nil || base.Move != nil || !base.Unread || gateway != nil {
		t.Errorf("barney-base %+v, wilma-gateway %+v", base, gateway)
	}
	if got := w.countProfileMoves(); got != 2 {
		t.Errorf("%d compares of a record with a profile version", got)
	}
	pack, ok, err := o.PackDiff(ctx, "barney-addons@1.3.1", "addon/fred-compositions", model.SideNewest)
	if err != nil || !ok || pack.Values.Diff[0].Old != sentinel {
		t.Errorf("the newest side: %+v %v %v", pack, ok, err)
	}
	for _, c := range []struct{ name, subject, key string }{
		{"a version whose compare failed", "barney-base@1.5.0", "csi/csi-aws-ebs"},
		{"a version that is current", "wilma-gateway@3.1.2", "addon/cert-manager"},
		{"a pack that is not in the move", "barney-addons@1.3.1", "addon/nope"},
	} {
		if _, ok, err := o.PackDiff(ctx, c.subject, c.key, model.SideNewest); ok || err != nil {
			t.Errorf("%s: %v %v", c.name, ok, err)
		}
	}
	if err := o.RunVersions(ctx); err != nil {
		t.Fatal(err)
	}
	if got := w.countProfileMoves(); got != 4 {
		t.Errorf("%d compares after two cycles", got)
	}
	raw, err := os.ReadFile(filepath.Join(r.state, "state.json"))
	if err != nil {
		t.Fatal(err)
	}
	for _, banned := range []string{sentinel, "u-140", "fred-compositions"} {
		if strings.Contains(string(raw), banned) {
			t.Errorf("the state holds %q", banned)
		}
	}
}
