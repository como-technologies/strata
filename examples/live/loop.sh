#!/usr/bin/env bash
# The everyday loop (#81): one change of one team, run end to end against the tenant, with the
# records in a git repository that the script builds, as docs/src/loop.md tells it.
#
#   examples/live/loop.sh
#
#   1  main records the release version and cluster A that runs it
#   2  a change: the next version as a candidate on a branch, the comment of the pull request,
#      and the merge gate
#   3  the publish: the published next version is the candidate that the reviewer read
#   4  the nightly jobs: every record verifies, the fleet of a label is recorded, and the
#      registry has newer versions of what the records pin
#   5  behind is not different: a value that a person overrides on cluster A opens no
#      notification, and the record of the cluster reports the drift; the override is undone
#   6  an edit of a published version in place: the nightly job opens an issue, --who names the
#      writer, and cluster A keeps its copy while an update waits for it; the edit is undone
#   7  an incident: which versions carry a pack, and which clusters run the release
#   8  the retire: nothing runs the next version, so the gate is open
#
# Needs the live example at its baseline with cluster A (just live-up), the strata release binary,
# git, jq, and the key as verify.sh needs it. Each change is undone, so the script leaves the
# example at its baseline. Results go under runs/<UTC time>/loop/, or OUT. Exit 0 only when no
# check fails.
set -uo pipefail
script=loop
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/loop}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

load_outputs
C=$(out cluster_uid)
[ -n "$C" ] || die "no cluster A; run just live-up"
export PALETTE_PROJECT_UID=$(out project_uid)
REL=$(out release_profile_uid)
NEXT=$(out next_profile_uid)
AD=$(out addon_profile_uid)
NAME=$(out profile_name)
edit_from='edited: "no"'
edit_to='edited: "yes"'

summary() {
  {
    echo "# the everyday loop"
    echo
    echo "Run \`$(basename "$(dirname "$run")")\` with \`$("$strata" --version)\`, cluster \`$C\`,"
    echo "release \`$REL\`, next \`$NEXT\`, add-on \`$AD\`."
    echo
    summary_table
  } > summary.md
}

# undo: each change of this script back, when a step before failed
undo() {
  "$strata" verify repo/runs/a.json > /dev/null 2>&1 \
    || "$live/api.sh" apply "$C" >> steps.log 2>&1
  "$strata" verify repo/releases/addon.json > /dev/null 2>&1 \
    || "$live/api.sh" edit "$AD" "$edit_to" "$edit_from" >> steps.log 2>&1
}
trap 'undo; summary' EXIT

# the git of the repository: one author, and no hook or setting of the machine
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null
export GIT_AUTHOR_NAME=loop GIT_AUTHOR_EMAIL=loop@example.com
export GIT_COMMITTER_NAME=loop GIT_COMMITTER_EMAIL=loop@example.com
export STRATA_BIN=$strata
ci=$(cd "$live/../.." && pwd)/scripts/ci
# in REPO: run a command in the repository of the team, where git and the scripts of CI run
in_repo() { (cd repo && "$@"); }

echo "== 1. main records the release and the cluster" | tee -a steps.log
require_baseline
mkdir -p repo/releases repo/runs && git -C repo init -q -b main && git -C repo remote add origin . || exit 2
check record-release 0 "record $NAME@1.0.0, the release" -- in_repo "$strata" snapshot "$REL" -o releases/1.0.0.json
check record-addon 0 "record the add-on version" -- in_repo "$strata" snapshot "$AD" -o releases/addon.json
check record-cluster 0 'record cluster A' -- in_repo "$strata" snapshot "cluster:$C" -o runs/a.json
check commit-main 0 'main holds the records' -- \
  in_repo bash -c 'git add releases runs && git commit -qm "the release, the add-on, and cluster A"'
check cluster-runs-release 0 '#81: cluster A runs the recorded release' -- \
  "$strata" compare repo/releases/1.0.0.json "cluster:$C/$REL"

echo "== 2. a change, in a pull request" | tee -a steps.log
git -C repo checkout -q -b next || exit 2
check candidate 0 "#81: the change: $NAME@1.0.1 written as a candidate file" -- \
  in_repo "$strata" candidate "$NEXT" -o change.yaml
check candidate-record 0 'the candidate as a record, before Palette has it' -- \
  in_repo "$strata" snapshot --from-candidate change.yaml -o candidate.json
check comment 0 '#81: the comment of the pull request, and the label decision' -- \
  in_repo bash -c "\"$ci/pr-comment.sh\" releases/1.0.0.json candidate.json --dry-run > comment.md"
check comment-says 0 '#81: the comment names the change and its impact' -- \
  grep -Eq '^\*\*Result:\*\* CHANGED · max impact: `[a-z-]+`' repo/comment.md
check commit-branch 0 'the branch holds the candidate' -- \
  in_repo bash -c 'git add change.yaml candidate.json && git commit -qm "next: the change"'
check merge-gate 0 '#81: the merge gate is clear: the records after the merge verify' -- \
  in_repo "$ci/merge-gate.sh" 'releases/*.json' 'runs/*.json' --base main
git -C repo checkout -q main && git -C repo merge -q --no-ff --no-edit next || exit 2

echo "== 3. the publish" | tee -a steps.log
check record-next 0 "record $NAME@1.0.1, as the pipeline published it" -- \
  in_repo "$strata" snapshot "$NEXT" -o releases/1.0.1.json
check same-release 0 '#81: Palette holds the content that the reviewer read' -- \
  in_repo "$strata" same-release candidate.json releases/1.0.1.json
check commit-publish 0 'main records the published version' -- \
  in_repo bash -c 'git add releases && git commit -qm "record 1.0.1"'

echo "== 4. the nightly jobs" | tee -a steps.log
check nightly 0 '#81: verify-report.sh: every record verifies, and each issue would close' -- \
  in_repo "$ci/verify-report.sh" 'releases/*.json' 'runs/*.json' --dry-run
check fleet 0 '#81: the fleet of a label, one record for each cluster' -- \
  in_repo "$strata" snapshot tag:purpose=strata-live-example --dir fleet --prune
check fleet-has-a 0 '#81: the fleet holds cluster A' -- \
  bash -c "jq -e --arg c $C '.source.cluster.uid == \$c' repo/fleet/*.json | grep -q true"
check upstream 1 '#81: upstream-report.sh: the registry has newer versions of what the records pin' -- \
  in_repo "$ci/upstream-report.sh" 'releases/*.json' --dry-run --no-impact

echo "== 5. behind is not different" | tee -a steps.log
check override 0 '#81: a person overrides a value on cluster A, through the API' -- \
  "$live/api.sh" override "$C" "$edit_from" "$edit_to"
check override-record 0 'record cluster A after the override' -- \
  "$strata" snapshot "cluster:$C" -o override.json
check override-not-behind 0 '#81: Palette says that no update waits for cluster A: an override opens no notification' -- \
  jq -e '.source.cluster.updatesPending == false' override.json
check override-drift 1 '#81: the record of cluster A reports the drift' -- "$strata" verify repo/runs/a.json
check override-release 0 '#81: the release record still verifies: the profile did not change' -- \
  "$strata" verify repo/releases/1.0.0.json repo/releases/addon.json
check override-undo 0 'undo the override: the content of the profiles, put on cluster A again' -- "$live/api.sh" apply "$C"
check override-gone 0 'the record of cluster A verifies again' -- "$strata" verify repo/runs/a.json

echo "== 6. an edit of a published version in place" | tee -a steps.log
check edit 0 '#81: a person edits the add-on version in place, through the API' -- \
  "$live/api.sh" edit "$AD" "$edit_from" "$edit_to"
check nightly-drift 1 '#81: the nightly job would open an issue for the add-on record' -- \
  in_repo "$ci/verify-report.sh" 'releases/*.json' 'runs/*.json' --dry-run
check nightly-names 0 '#81: the issue is for releases/addon.json' -- \
  grep -q 'would open or update the issue for releases/addon.json' nightly-drift.err
check who 1 '#81: --who names who wrote the edit' -- "$strata" verify repo/releases/addon.json --who
check who-says 0 '#81: the last write is an update by a person' -- \
  grep -Eq '^  [0-9TZ:.-]+  .+ \(user\)  update(: .+)?$' <(grep -E '^  [0-9]{4}-' who.out | tail -1)
check cluster-kept 0 '#81: cluster A keeps its copy: its record verifies' -- "$strata" verify repo/runs/a.json
check behind-record 0 'record cluster A with the edit waiting' -- "$strata" snapshot "cluster:$C" -o behind.json
check behind 0 '#81: Palette says that an update waits for cluster A' -- \
  jq -e '.source.cluster.updatesPending == true' behind.json
check edit-undo 0 'undo the edit' -- "$live/api.sh" edit "$AD" "$edit_to" "$edit_from"
check edit-gone 0 'the add-on record verifies again' -- "$strata" verify repo/releases/addon.json

echo "== 7. an incident" | tee -a steps.log
pack=$(jq -r '[.packs[] | select(.layer == "csi")][0].name' repo/releases/1.0.0.json)
check incident-list 0 "#81: the versions that carry $pack" -- "$strata" list --pack "$pack" --json
check incident-list-has 0 '#81: the release is one of them' -- \
  jq -e --arg u "$REL" '[.profiles[].uid] | index($u) != null' incident-list.out
check incident-consumers 1 '#81: cluster A runs the release' -- "$strata" consumers "$REL"
check incident-compare 0 '#81: and it runs exactly the recorded release' -- \
  "$strata" compare repo/releases/1.0.0.json "cluster:$C/$REL"

echo "== 8. the retire" | tee -a steps.log
check retire 0 "#81: nothing runs $NAME@1.0.1: the retire gate is open" -- "$strata" consumers "$NEXT"
check baseline 0 'the example is at its baseline' -- "$live/baseline.sh"

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]
