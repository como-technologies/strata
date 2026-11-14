package web

import (
	"regexp"
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

/* Issue #867. Strata checks two rules of the update that no route of Palette checks before it: a move of Kubernetes that skips a
minor version, and a required variable with no default value that the move adds. The column "Palette" and the card "Move to" show
each check that strata gives beside the answer of Palette. */

// The words of the card for the checks of the set update-checks.
const (
	skipHead     = "The move skips a minor version: kubernetes@1.35.3 → kubernetes@1.37.0"
	skipStep     = "Palette refuses the update with K8sUpgradeNotSupported. Move to kubernetes@1.36.2 first."
	oneVariable  = "The move adds a required variable: gateway_owner"
	oneValue     = "It has no default value. Give it a value at the update, or Palette refuses the update with InvalidVariable."
	twoVariables = "The move adds 2 required variables: identity_owner, issuer_url"
	twoValues    = "They have no default value. Give each one a value at the update, or Palette refuses the update with InvalidVariable."
	oneRule      = "Strata checks this rule, because the question to Palette does not check it."
	twoRules     = "Strata checks these rules, because the question to Palette does not check them."
)

// The column "Palette" gives the word of the answer, then the short words of each check. The card gives the answer of Palette, then
// a note with each check, what Palette does at the update, and the next step. A request that failed keeps its checks.
func TestTheCardAndTheColumnShowEachCheckOfStrataWithTheAnswerOfPalette(t *testing.T) {
	screens := render(t, devdata.StateUpdateChecks, false)
	table := pageOf(t, screens, "#newer", "")
	for _, c := range []struct{ hash, cell string }{
		{k8sHash, `<td class="narrow-hide"><span class="fw">No error</span><div class="small">Skips a minor version</div></td>`},
		{traefikHash, `<td class="narrow-hide"><span class="fw">Refuses</span><div class="small">Adds a required variable</div></td>`},
		{dexHash, `<td class="narrow-hide"><span class="mut">No answer</span> <button class="lnk" data-ask="fred-identity@2.3.0" data-pack="addon/dex">Ask again</button><div class="small">Adds 2 required variables</div></td>`},
	} {
		mustHold(t, c.hash, rowWith(t, table, `data-go="`+c.hash+`"`), c.cell)
	}

	k8s := textOf(cardOf(t, pageOf(t, screens, k8sHash, ""), "Palette gave no error"))
	mustHoldInOrder(t, "the skip", k8s, noErrorCovers, skipHead, skipStep, oneRule)
	mustNotHold(t, "the skip", k8s, "required variable", "Ask Palette about this move")

	traefik := textOf(cardOf(t, pageOf(t, screens, traefikHash, ""), "Palette refuses"))
	mustHoldInOrder(t, "the variable", traefik, "Palette refuses: DependencyPackUpgradeNotSupported", "Nothing on this cluster changed.", oneVariable, oneValue, oneRule)
	mustNotHold(t, "the variable", traefik, "skips a minor version", "Ask again")

	dex := pageOf(t, screens, dexHash, "")
	words := textOf(cardOf(t, dex, "Palette gave no answer"))
	mustHoldInOrder(t, "the variables", words, "Palette gave no answer: HTTP 403.", twoVariables, twoValues, oneRule, "Ask again")
	// The checks are a note of their own, in the tone of a refusal. The answer that failed stays neutral.
	mustHold(t, "the variables", dex, `<div class="note small"><b>Palette gave no answer: HTTP 403.</b> This is not a finding.</div><div class="note a small"><div class="mt3"><b>The move adds 2 required variables:</b> <span class="m">identity_owner</span>, <span class="m">issuer_url</span></div>`)
}

// A move with no check shows no word of a check, on any screen of the sets ok and asks.
func TestAnAnswerWithNoCheckShowsNoWordOfACheck(t *testing.T) {
	check := regexp.MustCompile(`Skips a minor version|skips a minor version|required variable|Strata checks th`)
	for _, state := range []devdata.State{devdata.StateOK, devdata.StateAsks} {
		for _, s := range render(t, state, false) {
			if check.MatchString(s.HTML) {
				t.Errorf("%s %s %s shows %q", state, s.Hash, s.Variant, check.FindString(s.HTML))
			}
		}
	}
}

// The checks come in the order of strata: the required variables, then the skip. With two checks, the note names the rules. A skip
// with no version of the next minor version in the registry names the minor version.
func TestTwoChecksComeInTheOrderOfStrataAndASkipWithNoStepNamesTheMinorVersion(t *testing.T) {
	const skip = `"updateChecks":{"skippedMinor":{"code":"K8sUpgradeNotSupported","pack":"kubernetes","from":"1.35.3","to":"1.37.0","nextMinor":"1.36","nextStep":"1.36.2"}}`
	for _, c := range []struct {
		name, with string
		words      []string
		cell       string
	}{
		{"both", `"updateChecks":{"requiredVariables":{"code":"InvalidVariable","names":["cluster_owner"]},"skippedMinor":{"code":"K8sUpgradeNotSupported","pack":"kubernetes","from":"1.35.3","to":"1.37.0","nextMinor":"1.36","nextStep":"1.36.2"}}`,
			[]string{"The move adds a required variable: cluster_owner", oneValue, skipHead, skipStep, twoRules},
			`<span class="fw">No error</span><div class="small">Adds a required variable</div><div class="small">Skips a minor version</div></td>`},
		{"no step", `"updateChecks":{"skippedMinor":{"code":"K8sUpgradeNotSupported","pack":"kubernetes","from":"1.35.3","to":"1.37.0","nextMinor":"1.36"}}`,
			[]string{skipHead, "Palette refuses the update with K8sUpgradeNotSupported. Move to a version of 1.36 first.", oneRule},
			`<span class="fw">No error</span><div class="small">Skips a minor version</div></td>`},
	} {
		t.Run(c.name, func(t *testing.T) {
			dir := writeSet(t, devdata.StateUpdateChecks, map[string]func([]byte) []byte{"asks.json": func(raw []byte) []byte {
				if !strings.Contains(string(raw), skip) {
					t.Fatal("the set update-checks changed")
				}
				return []byte(strings.Replace(string(raw), skip, c.with, 1))
			}})
			pages := pagesOf(t, dir, k8sHash, "#newer")
			words := textOf(cardOf(t, pages[0], "Palette gave no error"))
			mustHoldInOrder(t, "the card", words, c.words...)
			mustHold(t, "the column", rowWith(t, pages[1], `data-go="`+k8sHash+`"`), c.cell)
		})
	}
}

// A click gives the answer with its checks, and the card and the column show them at once.
func TestAClickShowsTheChecksOfTheAnswerAtOnce(t *testing.T) {
	const noCheck = `{"profile":"barney-base@1.5.0","key":"k8s/kubernetes","newest":"1.36.2","verdict":"no-error","refusals":[],"status":0}`
	const withCheck = `{"profile":"barney-base@1.5.0","key":"k8s/kubernetes","newest":"1.37.0","verdict":"no-error","refusals":[],"status":0,"updateChecks":{"skippedMinor":{"code":"K8sUpgradeNotSupported","pack":"kubernetes","from":"1.35.3","to":"1.37.0","nextMinor":"1.36","nextStep":"1.36.2"}}}`
	dir := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"replies.json": func(raw []byte) []byte {
		if !strings.Contains(string(raw), noCheck) {
			t.Fatal("the replies of the set ok changed")
		}
		return []byte(strings.Replace(string(raw), noCheck, withCheck, 1))
	}})
	const k8s = `{"ask":"barney-base@1.5.0","pack":"k8s/kubernetes"}`
	r := clickIn(t, dir, "scratch", k8sHash, "0", k8s)
	mustNotHold(t, "before", r.Before, "skips a minor version", "Palette gave no error")
	mustHold(t, "after", textOf(r.After), noErrorCovers, skipHead, skipStep, oneRule)
	table := clickIn(t, dir, "scratch", "#newer", "0", k8s)
	mustHold(t, "the table", rowWith(t, table.After, `data-go="`+k8sHash+`"`), `<span class="fw">No error</span><div class="small">Skips a minor version</div></td>`)
}

// The checks are words of the detail tier, and they raise no severity. The screens that do not show a move have the nav and the
// tiles of the set ok, and no word of a check.
func TestTheChecksChangeNoScreenThatShowsNoMove(t *testing.T) {
	checks := render(t, devdata.StateUpdateChecks, false)
	sameNavAndTiles(t, render(t, devdata.StateOK, false), checks)
	for _, hash := range noMoveScreens {
		mustNotHold(t, hash, pageOf(t, checks, hash, ""), "Skips a minor version", "required variable", "Strata checks th")
	}
}

// A code, a version, and the name of a variable of a check with the text of a tag show as text on the card.
func TestACheckWithMarkupShowsAsText(t *testing.T) {
	var reached int
	for _, s := range render(t, devdata.StateUpdateChecks, true) {
		checkMarkup(t, s)
		if strings.Contains(s.HTML, "The move adds 2 required variables:") {
			reached++
			mustHold(t, s.Hash+" "+s.Variant, s.HTML, `<span class="m">issuer_url`+injection, `<span class="m">InvalidVariable`+injection)
		}
		if strings.Contains(s.HTML, "The move skips a minor version:") {
			reached++
			mustHold(t, s.Hash+" "+s.Variant, s.HTML, `<span class="m">K8sUpgradeNotSupported`+injection, `@1.36.2`+injection)
		}
		mustNotHold(t, s.Hash+" "+s.Variant, s.HTML, "<img")
	}
	if reached == 0 {
		t.Error("no screen of the poisoned set shows a check")
	}
}

// The page of the service quotes the words of each check in its second table of "Ask Palette about a move", and the use case names
// them. Each word is a word of the scripts.
func TestTheBookQuotesTheWordsOfEachCheck(t *testing.T) {
	script := readScripts(t)
	section := sectionOf(t, servicePage, "## Ask Palette about a move")
	at := strings.Index(section, `The card "Move to" adds an amber note`)
	if at < 0 {
		t.Fatal("the section has no table of the checks")
	}
	rows := tableRows(t, section[at:])
	if len(rows) != 2 {
		t.Fatalf("the table of the checks has %d rows", len(rows))
	}
	useCase := strings.Join(strings.Fields(sectionOf(t, useCasePage, "## 5. Ask Palette about a move")), " ")
	for _, row := range rows {
		mustHold(t, "the scripts", script, words(row[0]), words(row[1]))
		mustHold(t, "the use case", useCase, `"`+row[0]+`"`)
	}
}
