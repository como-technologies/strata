#!/usr/bin/env bash
# The drift report test (#25): scripts/verify-report.sh, the scheduled verify's reporter, run around
# an in-place edit and a version bump of the add-on profile. Run it once with the Terraform
# provider's immutable-clusterprofiles flag off and once with it on; the two runs differ in nothing
# else.
#
#   examples/live/drift-report.sh off|on [FROM_STAGE]
#
#   1  record the release, the add-on version, and cluster A; report them, which opens nothing; and
#      what Terraform plans for an in-place edit with the flag
#   2  edit the add-on version in place through the API and report twice: one issue opens, and
#      the second report updates it
#   3  undo the edit and report: the issue closes
#   4  bump the add-on version with the flag, keeping cluster A on the first add-on uid, and report
#   5  restore the baseline and report: every issue the run opened is closed
#
# What varies: the flag, in the plan and in the bump. What holds: the edit (api.sh edit in both
# runs), the records, the steps and their order, the issue label, and the baseline the run starts
# from and returns to (lib.sh). The flag guards one client, Terraform; the report guards the
# tenant, so the edit through the API opens an issue whatever the flag says. Without the flag the
# bump overwrites the first add-on uid in place, and the report opens an issue for the add-on
# record and one for cluster A's record, whose add-on version is gone (#27, #30); with it, the bump
# leaves both records verifying.
#
# The issues go to REPORT_REPO (default: this repository, as gh sees it) under the label
# strata-live-drift, which nothing else uses; the run starts only when no issue with that label is
# open, and closes each one it opens. Needs the live example at its baseline (baseline.sh), the
# strata release binary, jq, gh logged in with issue write access, and the key as verify.sh needs
# it. Results go under runs/<UTC time>/drift-report/<off|on>/, or OUT; FROM_STAGE resumes a run in
# OUT. Exit 0 only when no check fails.
set -uo pipefail
script=drift-report
mode=${1:-}
from=${2:-1}
case "$mode" in
  off) flag=false ;;
  on) flag=true ;;
  *) sed -n '2,/^set -uo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"
reporter=$(cd "$live/../.." && pwd)/scripts/verify-report.sh
repo=${REPORT_REPO:-$(cd "$live" && gh repo view --json nameWithOwner --jq .nameWithOwner)} \
  || die "gh cannot see a repository for the issues; set REPORT_REPO"
label=strata-live-drift

# Expected exits and counts that differ with the flag.
if [ "$mode" = on ]; then
  off2_on1=1 off1_on0=0 bump_issues=0
  plan_edit_says='immutable-clusterprofiles: cluster profile versions are immutable'
else
  off2_on1=2 off1_on0=1 bump_issues=2
  plan_edit_says='spectrocloud_cluster_profile.addon will be updated in-place'
fi
edit_from='edited: "no"'
edit_to='edited: "yes"'

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/drift-report/$mode}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

summary() {
  {
    echo "# drift report test, flag $mode"
    echo
    echo "Run \`$(basename "$(dirname "$(dirname "$run")")")\` with \`$("$strata" --version)\`. Cluster A \`${C:-}\`, add-on \`${AD1:-}\`,"
    echo "issues in \`$repo\` labeled \`$label\`."
    echo
    summary_table
  } > summary.md
}
trap summary EXIT

save() { echo "$1=$2" >> ids.env; }
[ -f ids.env ] && . ./ids.env
stage() { [ "$from" -le "$1" ] && echo "== stage $1: $2" | tee -a steps.log; }

records="records-$mode"
# report: the scheduled report over this run's records
report() { STRATA_BIN="$strata" "$reporter" "$records/*.json" --repo "$repo" --label "$label"; }
# open_issues: the open issues with the label, one line each: the number and the marker line
open_issues() {
  gh issue list --repo "$repo" --label "$label" --state open --limit 1000 --json number,body \
    | jq -r '.[] | "\(.number)\t\(.body | split("\n")[0] | rtrimstr("\r"))"'
}
# issues_are RECORD...: the open issues with the label are exactly one per RECORD, by marker
issues_are() {
  local want got
  want=$(for r in "$@"; do echo "<!-- strata-verify: $records/$r.json -->"; done | sort)
  got=$(open_issues | tee /dev/stderr | cut -f2 | sort)
  [ "$got" = "$want" ]
}
# said FILE ACTION COUNT: the report's summary line in FILE names ACTION ("opened", "updated",
# "closed") exactly COUNT times
said() { [ "$(grep -o "$2 #[0-9]* for " "$1" | wc -l)" = "$3" ]; }

load_outputs
if [ "$from" -le 1 ]; then
  require_baseline
  [ -z "$(open_issues)" ] || die "issues labeled $label are open in $repo; close them before a run"
  : > ids.env
  PALETTE_PROJECT_UID=$(out project_uid)
  C=$(out cluster_uid)
  P=$(out release_profile_uid)
  AN=$(out addon_profile_name)
  AD1=$(out addon_profile_uid)
  for v in PALETTE_PROJECT_UID C P AN AD1; do
    [ -n "${!v}" ] || die "the terraform output behind $v is empty"
    save "$v" "${!v}"
  done
fi
export PALETTE_PROJECT_UID

if stage 1 "record, and report the baseline"; then
  mkdir -p "$records"
  check record-release 0 "record the release version" -- "$strata" snapshot "$P" -o "$records/release.json"
  check record-addon 0 "record the add-on version" -- "$strata" snapshot "$AD1" -o "$records/addon.json"
  check record-cluster-a 0 "record cluster A" -- "$strata" snapshot "cluster:$C" -o "$records/cluster-a.json"
  check report-baseline 0 "#25: the report over the three records at the baseline" -- report
  check report-baseline-says 0 "every record verifies, and no issue opens" -- \
    bash -c 'grep -q "3 records: 3 unchanged, 0 drifted, 0 could not be checked$" report-baseline.err'
  check plan-edit "$off2_on1" "Terraform's plan for an in-place edit of the add-on version: an update in place without the flag, a refusal with it" -- \
    tf_plan "$flag" -var edit_addon_in_place=true
  check plan-edit-says 0 "the plan says which" -- command grep -q "$plan_edit_says" plan-edit.out plan-edit.err
fi

if stage 2 "edit the add-on version in place and report"; then
  check api-edit 0 "edit the add-on version in place through the API, which Palette accepts whatever the flag" -- \
    "$live/api.sh" edit "$AD1" "$edit_from" "$edit_to"
  check report-edit 1 "#25: the report after the edit exits 1" -- report
  check report-edit-opened 0 "#25: it opens one issue, for the add-on record; cluster A's copy is untouched" -- \
    bash -c 'grep -q "3 records: 2 unchanged, 1 drifted, 0 could not be checked; opened #[0-9]* for '"$records"'/addon.json$" report-edit.err'
  check issues-after-edit 0 "#25: one open issue, for the add-on record" -- issues_are addon
  EDIT_ISSUE=$(open_issues | cut -f1)
  save EDIT_ISSUE "$EDIT_ISSUE"
  check issue-has-report 0 "#25: the issue holds the report of the drift" -- \
    bash -c "gh issue view $EDIT_ISSUE --repo $repo --json body --jq .body | tee issue-edit-body.md | grep -q 'reports drift'"
  check report-edit-again 1 "#25: a second report while the edit stands" -- report
  check report-edit-again-updated 0 "#25: it updates the issue and opens none" -- \
    bash -c "grep -q 'updated #$EDIT_ISSUE for $records/addon.json$' report-edit-again.err && ! grep -q opened report-edit-again.err"
  check issues-after-edit-again 0 "#25: still exactly one open issue" -- issues_are addon
fi

if stage 3 "undo the edit and report"; then
  check api-undo 0 "undo the edit through the API" -- "$live/api.sh" edit "$AD1" "$edit_to" "$edit_from"
  check report-undo 0 "#25: the report after the undo exits 0" -- report
  check report-undo-closed 0 "#25: it closes the issue" -- \
    bash -c "grep -q '3 records: 3 unchanged, 0 drifted, 0 could not be checked; closed #$EDIT_ISSUE for $records/addon.json$' report-undo.err"
  check issue-edit-closed 0 "#25: the issue is closed on the host" -- \
    bash -c "[ \"\$(gh issue view $EDIT_ISSUE --repo $repo --json state --jq .state)\" = CLOSED ]"
  check issues-after-undo 0 "no issue is open" -- issues_are
fi

if stage 4 "bump the add-on version and report"; then
  apply_profile spectrocloud_cluster_profile.addon -var addon_version=1.0.1 -var "cluster_addon_profile_uid=$AD1"
  check report-bump "$off1_on0" "#25, #27: the report after the bump: drift only without the flag" -- report
  check report-bump-opened 0 "#25, #27: the issues it opens: the add-on record and cluster A's without the flag, none with it" -- \
    said report-bump.err opened "$bump_issues"
  if [ "$mode" = off ]; then
    check issues-after-bump 0 "#27, #30: one open issue each for the add-on record and cluster A's record" -- issues_are addon cluster-a
    check bump-issue-names-overwrite 0 "#27: the add-on record's issue says the version was overwritten in place" -- \
      bash -c "gh issue view \$(gh issue list --repo $repo --label $label --state open --json number,body \
        --jq '.[] | select(.body | startswith(\"<!-- strata-verify: $records/addon.json -->\")) | .number') \
        --repo $repo --json body --jq .body | tee issue-bump-body.md | grep -q 'overwritten in place'"
  else
    check issues-after-bump 0 "no issue is open: the flag kept the first add-on version" -- issues_are
  fi
fi

if stage 5 "restore the baseline and report"; then
  restore_addon "$AD1" "$records/addon.json"
  check report-restored 0 "#25: the report after the restore exits 0" -- report
  check report-restored-closed 0 "#25: it closes each issue the bump opened" -- said report-restored.err closed "$bump_issues"
  check issues-after-restore 0 "no issue is open" -- issues_are
  check baseline-after 0 "the live example is back at its baseline" -- "$live/baseline.sh"
fi

echo
echo "$failures failed; details in $run"
[ "$failures" -eq 0 ]
