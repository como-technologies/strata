package check

import (
	"context"
	"slices"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// twoProjects is a set of three clusters in two projects: payments and ledger run web of home, and search runs db of
// home. db is a profile of the tenant that a request of home recorded. The selector prod selects the three.
func twoProjects(t *testing.T) *setRig {
	t.Helper()
	r := newSetRig(t, prod)
	r.setRecord(webHome, webName, webV1, home)
	r.setRecord(dbHome, dbName, webV1, "")
	r.cluster(cPayments, home, testkit.PaymentsName, []string{webHome}, prod)
	r.cluster(cLedger, home, "ledger", []string{webHome}, prod)
	r.cluster(cSearch, other, "search", []string{dbHome}, prod)
	return r
}

// rowOf finds the row of a cluster in the summary of a set.
func rowOf(t *testing.T, sum model.SetSummary, uid string) model.ClusterRow {
	t.Helper()
	i := slices.IndexFunc(sum.Clusters, func(c model.ClusterRow) bool { return c.UID == uid })
	if i < 0 {
		t.Fatalf("the set has no row of %s: %+v", uid, sum.Clusters)
	}
	return sum.Clusters[i]
}

// contentOnce runs one content cycle of the set, and fails the test on an error.
func contentOnce(t *testing.T, s *Set) {
	t.Helper()
	if err := s.RunContent(context.Background()); err != nil {
		t.Fatal(err)
	}
}

// checkedFiles gives the record files that a run of `verify` checks.
func checkedFiles(c fleetCall) []string {
	var files []string
	for _, a := range c.args[1:] {
		if !strings.HasPrefix(a, "-") {
			files = append(files, a)
		}
	}
	return files
}

func TestTwoProjectsWithTwoKeysGiveTheClustersOfEachProjectAndEachRunHasTheKeyOfItsProject(t *testing.T) {
	r := twoProjects(t)
	s := r.set()
	contentOnce(t, s)
	sum := s.Clusters()
	if sum.Counts.Clusters != 3 || sum.Page != model.PageOK {
		t.Fatalf("counts %+v, page %s", sum.Counts, sum.Page)
	}
	for uid, project := range map[string]string{cPayments: home, cLedger: home, cSearch: other} {
		if row := rowOf(t, sum, uid); row.Project.UID != project || row.Status != model.RowOK || row.Profiles != 1 {
			t.Errorf("%s: %+v", uid, row)
		}
	}
	secrets := map[string]string{home: homeKey, other: otherKey}
	// each search names its project, and each run has the key of the project that it reads
	for _, c := range r.f.runsOf(runner.CmdList) {
		if want := after(c.args, runner.FlagProject); !c.keyed || c.key.Project != want || c.key.Secret != secrets[want] {
			t.Errorf("%v ran with the key %+v", c.args, c.key)
		}
	}
	for _, c := range r.f.runsOf(runner.CmdSnapshot) {
		uid, _, _ := strings.Cut(strings.TrimPrefix(c.args[1], "cluster:"), "/")
		if want := r.f.clusters[uid].project; c.key.Project != want || c.key.Secret != secrets[want] {
			t.Errorf("the read of %s ran with the key of %s, want %s", uid, c.key.Project, want)
		}
	}
}

// The key of each project checks only the records of that project. A record that a request at tenant scope took has
// no project, and the key api-key checks it.
func TestARunGetsOnlyTheKeyOfItsProject(t *testing.T) {
	r := twoProjects(t)
	r.setRecord("u-queue-other", "queue", webV1, other)
	s := r.set()
	contentOnce(t, s)
	verifies := r.f.runsOf(runner.CmdVerify)
	if len(verifies) != 2 {
		t.Fatalf("verify ran %d times, want one time for each project", len(verifies))
	}
	for _, c := range verifies {
		want := map[string]int{home: 2, other: 1}[c.key.Project]
		if !c.keyed || len(checkedFiles(c)) != want {
			t.Errorf("the key of %s checked %v, want %d records", c.key.Project, checkedFiles(c), want)
		}
	}
	for _, c := range r.f.calls {
		if !c.keyed || c.key.Secret == "" {
			t.Errorf("%v ran with no key of a project", c.args)
		}
	}
}

func TestOneProfileVersionOnThreeClustersIsOneCheckOfVerify(t *testing.T) {
	r := newSetRig(t, prod)
	r.setRecord(webHome, webName, webV1, home)
	for _, uid := range []string{cPayments, cLedger, cSearch} {
		r.cluster(uid, home, uid, []string{webHome}, prod)
	}
	s := r.set()
	contentOnce(t, s)
	verifies := r.f.runsOf(runner.CmdVerify)
	if len(verifies) != 1 || len(checkedFiles(verifies[0])) != 1 {
		t.Fatalf("verify ran %v, want one run on one record", verifies)
	}
	if compares := len(r.f.runsOf(runner.CmdCompare)); compares != 3 {
		t.Errorf("%d compares, want one for each cluster", compares)
	}
	shared := s.Shared()
	if len(shared) != 1 || shared[0].Clusters != 3 || shared[0].Copies.Same != 3 || shared[0].Copies.Of != 3 || len(shared[0].Projects) != 1 {
		t.Errorf("shared %+v", shared)
	}
}

func TestARefusedKeyLeavesOnlyTheClustersOfItsProjectNotCheckedWithTheStatus(t *testing.T) {
	r := twoProjects(t)
	s := r.set()
	contentOnce(t, s)
	r.f.refused[other] = true
	if err := s.RunContent(context.Background()); err != nil {
		t.Fatalf("a refusal of one key failed the cycle: %v", err)
	}
	sum := s.Clusters()
	row := rowOf(t, sum, cSearch)
	if row.Status != model.RowNotChecked || row.LastError == nil || row.LastError.Kind != docs.KindAPI || row.LastError.Status != 403 {
		t.Errorf("the cluster of the refused key: %+v", row)
	}
	for _, uid := range []string{cPayments, cLedger} {
		if row := rowOf(t, sum, uid); row.Status != model.RowOK || row.LastError != nil {
			t.Errorf("%s: %+v", uid, row)
		}
	}
	i := slices.IndexFunc(sum.Projects, func(p model.Project) bool { return p.UID == other })
	if i < 0 || sum.Projects[i].LastError == nil || sum.Projects[i].LastError.Status != 403 {
		t.Errorf("projects %+v", sum.Projects)
	}
	if sum.Page != model.PageOK {
		t.Errorf("the page is %s: a refusal of one key is the state of its project only", sum.Page)
	}
}

// The observer checks each cluster, and verify cannot verify the record of web. So the check of payments and ledger
// has one cause of Not checked. Their rows keep the status of their content and count the cause.
func TestACheckedClusterWithACauseOfNotCheckedKeepsTheStatusOfItsContentAndCountsTheCause(t *testing.T) {
	r := twoProjects(t)
	r.f.unverified[webHome] = true
	s := r.set()
	contentOnce(t, s)
	sum := s.Clusters()
	for uid, causes := range map[string]int{cPayments: 1, cLedger: 1, cSearch: 0} {
		if row := rowOf(t, sum, uid); row.Status != model.RowOK || row.NotChecked != causes || row.CheckedAt == nil || row.LastError != nil {
			t.Errorf("%s: %+v, want the status ok with %d causes", uid, row, causes)
		}
	}
	if sum.Counts.NotChecked != 0 {
		t.Errorf("counts %+v: the observer checked each cluster", sum.Counts)
	}
}

// The second read of ledger fails, and the first one ended well. The row of ledger is not checked, with the error and
// the time of its last good check. The other clusters keep the status of their content.
func TestAReadOfAClusterThatFailsLeavesOnlyItsRowNotCheckedWithTheError(t *testing.T) {
	r := twoProjects(t)
	s := r.set()
	contentOnce(t, s)
	r.f.broken[cLedger] = true
	contentOnce(t, s)
	sum := s.Clusters()
	row := rowOf(t, sum, cLedger)
	if row.Status != model.RowNotChecked || row.CheckedAt == nil || row.LastError == nil || row.LastError.Kind != docs.KindAPI || row.LastError.Status != 500 {
		t.Errorf("the cluster whose read failed: %+v", row)
	}
	for _, uid := range []string{cPayments, cSearch} {
		if row := rowOf(t, sum, uid); row.Status != model.RowOK || row.LastError != nil {
			t.Errorf("%s: %+v", uid, row)
		}
	}
	if sum.Counts.NotChecked != 1 || sum.Page != model.PageOK {
		t.Errorf("counts %+v, page %s", sum.Counts, sum.Page)
	}
}

func TestEachKeyRefusedMakesTheCycleFailAndThePageLost(t *testing.T) {
	r := twoProjects(t)
	r.f.refused[home], r.f.refused[other] = true, true
	s := r.set()
	for range 3 {
		if err := s.RunContent(context.Background()); err == nil {
			t.Fatal("a cycle with each key refused ended well")
		}
	}
	if sum := s.Clusters(); sum.Page != model.PageLost || sum.Content.LastError == nil || sum.Content.LastError.Status != 403 {
		t.Errorf("page %s, content %+v", sum.Page, sum.Content)
	}
}

func TestASelectorThatSelectsNoClusterCountsNoneAndIsRead(t *testing.T) {
	r := twoProjects(t)
	r.cfg.Selectors = append(r.cfg.Selectors, Selector{Text: nothing, Labels: []string{"env=nowhere"}})
	s := r.set()
	contentOnce(t, s)
	want := []model.Selector{{Selector: prod, Clusters: 3}, {Selector: nothing}}
	if got := s.Clusters().Selectors; !slices.Equal(got, want) {
		t.Errorf("selectors %+v, want %+v", got, want)
	}
}

func TestTwoProjectsWithOneNameAndVersionKeepTwoRecordsByTheirUids(t *testing.T) {
	r := newSetRig(t, prod)
	r.setRecord(webHome, webName, webV1, home)
	r.setRecord(webOther, webName, webV1, other)
	r.cluster(cPayments, home, testkit.PaymentsName, []string{webHome}, prod)
	r.cluster(cSearch, other, "search", []string{webOther}, prod)
	// only the version of other changed after its record
	r.f.drifted[webOther] = true
	s := r.set()
	contentOnce(t, s)
	if row := rowOf(t, s.Clusters(), cSearch); row.Status != model.RowDrift || row.Drift != 1 {
		t.Errorf("search: %+v", row)
	}
	if row := rowOf(t, s.Clusters(), cPayments); row.Status != model.RowOK {
		t.Errorf("payments: %+v", row)
	}
	pv, _ := s.Member(cPayments)
	sv, _ := s.Member(cSearch)
	pp, sp := pv.Summary().Profiles, sv.Summary().Profiles
	if len(pp) != 1 || pp[0].UID != webHome || len(sp) != 1 || sp[0].UID != webOther || sp[0].Status != model.StatusDrift {
		t.Errorf("payments %+v, search %+v", pp, sp)
	}
	if shared := s.Shared(); len(shared) != 2 || shared[0].UID == shared[1].UID {
		t.Fatalf("shared %+v", shared)
	}
	// the id of a finding holds the uid of the cluster and of the record, so the two versions never share one
	f := sv.Summary().Findings
	if len(f) != 1 || f[0].ID != model.FindingID(model.PublishedChanged, cSearch+"/"+webOther) || f[0].ID == model.FindingID(model.PublishedChanged, webName+"@"+webV1) {
		t.Errorf("findings %+v", f)
	}
}

func TestAClusterThatLeavesTheSetLeavesThePageAndTheAlertsWithNoClearMessage(t *testing.T) {
	r := twoProjects(t)
	r.f.differs[cLedger+"/"+webHome] = true
	sender := &capture{}
	s := r.set()
	withRules(t, r.rig, s.Observer, sender, everyone)
	contentOnce(t, s)
	if got := sender.texts; len(got) != 1 || !strings.Contains(got[0], "strata · ledger · Drift") {
		t.Fatalf("the sender got %q", got)
	}
	// the label of ledger goes, so the search no longer selects it
	r.f.selected[prod] = []string{cPayments, cSearch}
	contentOnce(t, s)
	sum := s.Clusters()
	if slices.ContainsFunc(sum.Clusters, func(c model.ClusterRow) bool { return c.UID == cLedger }) || sum.Counts.Clusters != 2 {
		t.Errorf("clusters %+v", sum.Clusters)
	}
	if _, ok := s.Member(cLedger); ok {
		t.Error("the set still serves the routes of ledger")
	}
	if got := sender.texts; len(got) != 1 {
		t.Errorf("the sender got %q: no check showed that the finding of ledger is fixed", got)
	}
	if len(s.state.Alerts.Items) != 0 {
		t.Errorf("the store keeps %+v", s.state.Alerts.Items)
	}
	testkit.HoldsAll(t, "the log", r.log.String(), leftSet, "clusterUid="+cLedger)
}
