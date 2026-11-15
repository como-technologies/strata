package web

import (
	"strings"
	"testing"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/testkit"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

/* Issue #867. Strata checks two rules of the update that no route of Palette checks before it: a move of Kubernetes that skips a
minor version, and a required variable with no default value that the move adds. The column "Palette" and the card "Move to" show
each check that strata gives beside the answer of Palette. */

// The words of the column "Palette" and of the card "Move to" for the checks of the set update-checks.
const (
	skipLine        = "Skips a minor version"
	variableLine    = "Adds a " + variableWords
	variablesLine   = "Adds 2 " + variableWords + "s"
	skipNote        = "The move skips a minor version:"
	skipHead        = skipNote + " " + testkit.KubernetesFrom + " → kubernetes@1.37.0"
	skipRule        = "Kubernetes moves one minor version at each upgrade."
	skipStep        = skipRule + " Move to " + testkit.KubernetesTo + " first."
	variableNote    = "The move adds a " + variableWords + ":"
	oneVariable     = variableNote + " gateway_owner"
	variableRefuses = paletteRefuses + " the update with " + testkit.CodeVariable + "."
	oneValue        = "It has no default value. Give it a value at the update, or " + variableRefuses
	variablesNote   = "The move adds 2 " + variableWords + "s:"
	twoVariables    = variablesNote + " identity_owner, issuer_url"
	twoValues       = "They have no default value. Give each one a value at the update, or " + variableRefuses
	oneRule         = "Strata checks this rule" + paletteNoCheck + "it."
	twoRules        = "Strata checks these rules" + paletteNoCheck + "them."
	// paletteNoCheck is in the last sentence of the note of the checks.
	paletteNoCheck = ", because the question to Palette does not check "
	// variableWords are in each line of a required variable, on the column and on the card.
	variableWords = "required variable"
	// noStepLine is the next step of a skip when strata names no version of the next minor version.
	noStepLine = skipRule + " Move to a version of 1.36 first."
	// The words of the card "Move to" of a profile for the checks of the set profile-checks (#897). No click asks Palette about
	// the move of a profile, so the last sentence of the note says that no question checks the rules.
	baseVariables   = variablesNote + " aws_region, node_owner"
	oneProfileRule  = "Strata checks this rule" + noQuestionCheck + "it."
	twoProfileRules = "Strata checks these rules" + noQuestionCheck + "them."
	noQuestionCheck = ", because no question to Palette checks "
	// checkNoteStart starts the amber note of the checks on a card, with a required variable first.
	checkNoteStart = `<div class="note a small"><div class="mt3"><b>`
)

// checkWords are the words that only a check of strata shows, on the column and on the card.
var checkWords = []string{skipLine, skipNote, variableWords, oneRule, twoRules, oneProfileRule, twoProfileRules}

// cellWith gives a cell of the column "Palette" with the line of each check under the word of the answer.
func cellWith(cell string, lines ...string) string {
	var checks strings.Builder
	for _, l := range lines {
		checks.WriteString(`<div class="small">` + l + `</div>`)
	}
	return strings.TrimSuffix(cell, "</td>") + checks.String() + "</td>"
}

// checkAnswers gives the answers of the set update-checks for the move of kubernetes, with its skip, and for the move of traefik,
// with its required variable.
func checkAnswers(t *testing.T) (k8s, traefik model.Answer) {
	t.Helper()
	asks := loadSet(t, devdata.StateUpdateChecks).Asks.Asks
	k8s, traefik = asks[0], asks[1]
	if k8s.Key != testkit.KubernetesKey || k8s.UpdateChecks == nil || k8s.UpdateChecks.SkippedMinor == nil ||
		traefik.Key != testkit.TraefikKey || traefik.UpdateChecks == nil || traefik.UpdateChecks.RequiredVariables == nil {
		t.Fatalf("the set %s changed: %+v", devdata.StateUpdateChecks, asks)
	}
	return k8s, traefik
}

// The column "Palette" gives the word of the answer, then the short words of each check. The card gives the answer of Palette, then
// a note with each check, what Palette does at the update, and the next step. A request that failed keeps its checks.
func TestTheCardAndTheColumnShowEachCheckOfStrataWithTheAnswerOfPalette(t *testing.T) {
	screens := render(t, devdata.StateUpdateChecks, false)
	table := pageOf(t, screens, "#newer", "")
	for _, c := range []struct{ hash, cell string }{
		{k8sHash, cellWith(noErrorCell, skipLine)},
		{traefikHash, cellWith(refusesCell, variableLine)},
		{dexHash, cellWith(noAnswerCell, variablesLine)},
	} {
		mustHold(t, c.hash, rowTo(t, table, c.hash), c.cell)
	}

	k8s := textOf(cardOf(t, pageOf(t, screens, k8sHash, ""), gaveNoError))
	mustHoldInOrder(t, "the skip", k8s, noErrorCovers, skipHead, skipStep, oneRule)
	mustNotHold(t, "the skip", k8s, variableWords, askThisMove)

	traefik := textOf(cardOf(t, pageOf(t, screens, traefikHash, ""), paletteRefuses))
	mustHoldInOrder(t, "the variable", traefik, refusesWithCode, nothingChanged, oneVariable, oneValue, oneRule)
	mustNotHold(t, "the variable", traefik, skipNote, askAgain)

	dex := pageOf(t, screens, dexHash, "")
	words := textOf(cardOf(t, dex, gaveNoAnswer))
	mustHoldInOrder(t, "the variables", words, gave403, twoVariables, twoValues, oneRule, askAgain)
	// The checks are a note of their own, in the tone of a refusal. The answer that failed stays neutral.
	mustHold(t, "the variables", dex, neutralNote+gave403+`</b> This is not a finding.</div>`+checkNoteStart+variablesNote+`</b>`)
}

// A move with no check shows no word of a check, on any screen of the sets ok and asks.
func TestAnAnswerWithNoCheckShowsNoWordOfACheck(t *testing.T) {
	for _, state := range []devdata.State{devdata.StateOK, devdata.StateAsks} {
		for _, s := range render(t, state, false) {
			mustNotHold(t, string(state)+" "+s.Hash+" "+s.Variant, s.HTML, checkWords...)
		}
	}
}

// The checks come in the order of strata: the required variables, then the skip. With two checks, the note names the rules. A skip
// with no version of the next minor version in the registry names the minor version.
func TestTwoChecksComeInTheOrderOfStrataAndASkipWithNoStepNamesTheMinorVersion(t *testing.T) {
	k8s, traefik := checkAnswers(t)
	skip := k8s.UpdateChecks.SkippedMinor
	noStep := *skip
	noStep.NextStep = ""
	variable := *traefik.UpdateChecks.RequiredVariables
	variable.Names = []string{"cluster_owner"}
	for _, c := range []struct {
		name   string
		checks docs.UpdateChecks
		words  []string
		cell   string
	}{
		{"both", docs.UpdateChecks{RequiredVariables: &variable, SkippedMinor: skip},
			[]string{variableNote + " cluster_owner", oneValue, skipHead, skipStep, twoRules}, cellWith(noErrorCell, variableLine, skipLine)},
		{"no step", docs.UpdateChecks{SkippedMinor: &noStep},
			[]string{skipHead, noStepLine, oneRule}, cellWith(noErrorCell, skipLine)},
	} {
		t.Run(c.name, func(t *testing.T) {
			dir := writeSet(t, devdata.StateUpdateChecks, map[string]func([]byte) []byte{"asks.json": editJSON(t, func(v *api.AsksView) {
				v.Asks[0].UpdateChecks = &c.checks
			})})
			pages := pagesOf(t, dir, k8sHash, "#newer")
			words := textOf(cardOf(t, pages[0], gaveNoError))
			mustHoldInOrder(t, "the card", words, c.words...)
			mustHold(t, "the column", rowTo(t, pages[1], k8sHash), c.cell)
		})
	}
}

// A click gives the answer with its checks, and the card and the column show them at once.
func TestAClickShowsTheChecksOfTheAnswerAtOnce(t *testing.T) {
	k8s, _ := checkAnswers(t)
	// The set ok, with one reply: the answer of update-checks for the move of kubernetes.
	dir := writeSet(t, devdata.StateOK, map[string]func([]byte) []byte{"replies.json": wholeFile(string(testkit.MustJSON(t, api.AsksView{Asks: []model.Answer{k8s}})))})
	r := clickIn(t, dir, scratch, k8sHash, "0", k8sClick)
	mustNotHold(t, "before", r.Before, skipNote, gaveNoError)
	mustHold(t, "after", textOf(r.After), noErrorCovers, skipHead, skipStep, oneRule)
	table := clickIn(t, dir, scratch, "#newer", "0", k8sClick)
	mustHold(t, "the table", rowTo(t, table.After, k8sHash), cellWith(noErrorCell, skipLine))
}

// The checks are words of the detail tier, and they raise no severity. The screens that do not show a move have the nav and the
// tiles of the set ok, and no word of a check.
func TestTheChecksChangeNoScreenThatShowsNoMove(t *testing.T) {
	sameScreensWithNoMove(t, render(t, devdata.StateOK, false), render(t, devdata.StateUpdateChecks, false), checkWords...)
}

// A code, a version, and the name of a variable of a check with the text of a tag show as text on the card. render.js does not
// poison the pack of a skip.
func TestACheckWithMarkupShowsAsText(t *testing.T) {
	var reached int
	for _, s := range render(t, devdata.StateUpdateChecks, true) {
		where := s.Hash + " " + s.Variant
		checkMarkup(t, s)
		if strings.Contains(s.HTML, variablesNote) {
			reached++
			mustHold(t, where, s.HTML, `<span class="m">issuer_url`+injection, `<span class="m">`+testkit.CodeVariable+injection)
		}
		if strings.Contains(s.HTML, skipNote) {
			reached++
			mustHold(t, where, s.HTML, `@1.36.2`+injection)
		}
		mustNotHold(t, where, s.HTML, "<img")
	}
	if reached == 0 {
		t.Error("no screen of the poisoned set shows a check")
	}
}

// The page of the service quotes the words of each check in its second table of "Ask Palette about a move", and the use case names
// them. Each word is a word of the scripts.
func TestTheBookQuotesTheWordsOfEachCheck(t *testing.T) {
	script := readScripts(t)
	section := sectionOf(t, servicePage, askSection)
	at := strings.Index(section, `The card "Move to" adds an amber note`)
	if at < 0 {
		t.Fatal("the section has no table of the checks")
	}
	rows := tableRows(t, section[at:])
	if len(rows) != 2 {
		t.Fatalf("the table of the checks has %d rows", len(rows))
	}
	useCase := unwrapped(sectionOf(t, useCasePage, "## 5. "+askMove))
	for _, row := range rows {
		mustHold(t, "the scripts", script, words(row[0]), words(row[1]))
		mustHold(t, "the use case", useCase, `"`+row[0]+`"`)
	}
}

/* Issue #897. The compare of a record with the newest version of its profile gives the same checks, with no request. The card
"Move to" of a profile names each one in an amber note above its table of packs. The set profile-checks holds the move of
barney-base, with two required variables and a skip, and the move of wilma-gateway, with one required variable. */

// The section of servicePage about the card "Move to" of a profile.
const profileMoveSection = "## The move to a newer profile version"

// The note of barney-base names its two required variables and what Palette does at the update. It names its skip with the rule of
// Kubernetes and the minor version of the next step. The compare reads no registry, so the skip names no version. The note of
// wilma-gateway names one required variable. No click asks Palette about the move of a profile, so the card has no word of a
// question. A move that changes no pack keeps its note.
func TestTheCardMoveToOfAProfileNamesEachCheckOfTheMove(t *testing.T) {
	pages := shown(t, devdata.StateProfileChecks, barneyHash, wilmaHash)
	barney := cardOf(t, pages[0], moveCardHead)
	mustHoldInOrder(t, "the move of barney-base", textOf(barney), baseVariables, twoValues, skipHead, noStepLine, twoProfileRules)
	mustHoldInOrder(t, "the move of barney-base", barney, `<div class="body">`+checkNoteStart+variablesNote+`</b>`, `<div class="tw"><table>`)
	wilma := textOf(cardOf(t, pages[1], moveCardHead))
	mustHoldInOrder(t, "the move of wilma-gateway", wilma, oneVariable, oneValue, oneProfileRule)
	mustNotHold(t, "the move of wilma-gateway", wilma, skipNote)
	for where, words := range map[string]string{"barney-base": textOf(barney), "wilma-gateway": wilma} {
		mustNotHold(t, "the move of "+where, words, paletteNoCheck, askThisMove)
	}

	dir := writeSet(t, devdata.StateProfileChecks, map[string]func([]byte) []byte{"profiles.json": editJSON(t, func(v *api.ProfilesView) {
		for i := range v.Profiles {
			if n := v.Profiles[i].NewerVersion; n != nil && n.Move != nil {
				n.Move.Packs = []model.MovePack{}
			}
		}
	})})
	noPack := textOf(cardOf(t, pagesOf(t, dir, wilmaHash)[0], moveCardHead))
	mustHoldInOrder(t, "a move that changes no pack", noPack, oneVariable, oneProfileRule, "The move changes no pack.")
}

// A code, a version, and the name of a variable of a check on the move of a profile show as text on the card.
func TestACheckOfTheMoveOfAProfileWithMarkupShowsAsText(t *testing.T) {
	var reached int
	for _, s := range render(t, devdata.StateProfileChecks, true) {
		if s.Hash != barneyHash || s.Variant != "" {
			continue
		}
		reached++
		checkMarkup(t, s)
		mustHold(t, s.Hash, s.HTML, `<span class="m">node_owner`+injection, `<span class="m">`+testkit.CodeVariable+injection,
			`@1.37.0`+injection, "a version of 1.36"+injection)
		mustNotHold(t, s.Hash, s.HTML, "<img")
	}
	if reached == 0 {
		t.Error("no screen of the poisoned set is the page of barney-base")
	}
}

// The page of the service quotes the words of each check of the card "Move to" of a profile, and the last sentence of its note.
// Each word is a word of the scripts. The use case says that the card shows the checks.
func TestTheBookQuotesTheWordsOfEachCheckOfTheMoveOfAProfile(t *testing.T) {
	script := readScripts(t)
	section := sectionOf(t, servicePage, profileMoveSection)
	rows := tableRows(t, section)
	if len(rows) != 2 {
		t.Fatalf("the table of the checks has %d rows", len(rows))
	}
	for _, row := range rows {
		mustHold(t, "the scripts", script, words(row[0]))
	}
	mustHold(t, "the section", unwrapped(section), `"`+oneProfileRule+`"`)
	mustHold(t, "the scripts", script, oneProfileRule, twoProfileRules)
	mustHold(t, "the use case", unwrapped(sectionOf(t, useCasePage, "## 5. "+askMove)), `card "Move to" of a profile`)
}
