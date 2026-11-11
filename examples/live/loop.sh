#!/usr/bin/env bash
# The everyday loop (#81): one change of one team, run end to end against the tenant, with the
# records in a git repository that the script builds, as docs/src/walkthrough.md tells it.
#
#   examples/live/loop.sh [FROM_STAGE]
#
#   1  main records the release version and cluster A that runs it
#   2  a change: the next version as a candidate on a branch, and the question to Palette if it
#      accepts the packs of the candidate. Then the comment of the pull request, and the merge gate
#   3  the publish: the published next version is the candidate that the reviewer read, and behind
#      names it as a newer version of the release (#707). The stage
#      asks Palette if the move of cluster A to the next version repaves, and if Palette accepts
#      the packs. pr-comment.sh --repave puts the answer in the comment of the pull request of the
#      move. A cluster ref that selects no profile holds the release alone (#615)
#   4  before a build: strata preflight runs the static tier and the tier of Palette on the body
#      of a create on the release. The comment of the pull request holds the static report. The
#      stage creates nothing
#   5  the nightly jobs: every record verifies, and the script records the fleet of a label. The
#      registry has newer versions of what the records pin. The stage asks Palette whether it
#      accepts the move to each of them
#   6  behind is not different: a value that a person overrides on cluster A opens no
#      notification. The record of the cluster reports the drift. The stage undoes the override
#   7  an edit of a published version in place: the nightly job opens an issue. --who names the
#      writer in the command and in the issue of the nightly job. Cluster A keeps its copy while
#      an update waits for it. The stage undoes the edit
#   8  an incident: which versions carry a pack, and which clusters run the release
#   9  the retire: nothing runs the next version, so the gate is open. Then strata report reads the
#      log of the unknown values that the run wrote
#
# Needs the live example at its baseline with cluster A (just live-up), the strata release binary,
# git, jq, and the key as verify.sh needs it. Each change is undone, so the script leaves the
# example at its baseline. Stages 2 to 4 only ask Palette: Palette keeps nothing of the questions
# and of the body. Results go under runs/<UTC time>/loop/, or OUT; FROM_STAGE resumes a run in OUT.
# Exit 0 only when no check fails.
set -uo pipefail
script=loop
from=${1:-1}
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
case "$from" in 1|2|3|4|5|6|7|8|9) ;; *) usage; exit 2 ;; esac
[ -x "$strata" ] || die "no strata binary at $strata"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/loop}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1
# the log of the unknown values goes to the run directory, not to the state folder of the user (an
# absolute path, as XDG_STATE_HOME needs)
export XDG_STATE_HOME=$PWD/state

load_outputs
C=$(out cluster_uid)
[ -n "$C" ] || die "no cluster A; run just live-up"
export PALETTE_PROJECT_UID=$(out project_uid)
REL=$(out release_profile_uid)
NEXT=$(out next_profile_uid)
AD=$(out addon_profile_uid)
NAME=$(out profile_name)

summary() {
  write_summary "the everyday loop" ", cluster \`$C\`," \
    "release \`$REL\`, next \`$NEXT\`, add-on \`$AD\`."
}

# undo: each change of this script back, when a step before failed
undo() {
  "$strata" verify repo/runs/a.json > /dev/null 2>&1 \
    || "$live/api.sh" apply "$C" >> steps.log 2>&1
  "$strata" verify repo/releases/addon.json > /dev/null 2>&1 \
    || "$live/api.sh" edit "$AD" "$addon_to" "$addon_from" >> steps.log 2>&1
}
trap 'undo; summary' EXIT

# the git of the repository: one author, and no hook or setting of the machine
isolate_git loop
export STRATA_BIN=$strata
ci=$(cd "$live/../.." && pwd)/scripts/ci
# in REPO: run a command in the repository of the team, where git and the scripts of CI run
in_repo() { (cd repo && "$@"); }

if stage 1 "main records the release and the cluster"; then
  require_baseline
  mkdir -p repo/releases repo/runs && git -C repo init -q -b main && git -C repo remote add origin . || exit 2
  check record-release 0 "record $NAME@1.0.0, the release" -- in_repo "$strata" snapshot "$REL" -o releases/1.0.0.json
  check record-addon 0 "record the add-on version" -- in_repo "$strata" snapshot "$AD" -o releases/addon.json
  check record-cluster 0 'record cluster A' -- in_repo "$strata" snapshot "cluster:$C" -o runs/a.json
  check commit-main 0 'main holds the records' -- \
    in_repo bash -c 'git add releases runs && git commit -qm "the release, the add-on, and cluster A"'
  check cluster-runs-release 0 '#81: cluster A runs the recorded release' -- \
    "$strata" compare repo/releases/1.0.0.json "cluster:$C/$REL"
fi

if stage 2 "a change, in a pull request"; then
  git -C repo checkout -q -b next || exit 2
  check candidate 0 "#81: the change: $NAME@1.0.1 written as a candidate file" -- \
    in_repo "$strata" candidate "$NEXT" -o change.yaml
  check candidate-record 0 'the candidate as a record, before Palette has it' -- \
    in_repo "$strata" snapshot --from-candidate change.yaml -o candidate.json
  # the name route refuses a name and a version that Palette holds, and it holds the next version
  # already: the question goes out under a name that the project lacks
  sed "s/^name: .*/name: $NAME-ask/" repo/change.yaml > repo/ask.yaml
  check ask-candidate 0 '#385: Palette accepts the packs of the candidate, and the record holds the answer' -- \
    in_repo bash -c "\"$strata\" snapshot --from-candidate ask.yaml --validate -o ask.json && jq -e '.source.paletteValidation.accepts' ask.json"
  check comment 0 '#81: the comment of the pull request, and the label decision' -- \
    in_repo bash -c "\"$ci/pr-comment.sh\" releases/1.0.0.json candidate.json --dry-run > comment.md"
  check comment-says 0 '#81: the comment names the change and its impact' -- \
    grep -Eq '^\*\*Result:\*\* CHANGED · max impact: `[a-z-]+`' repo/comment.md
  check commit-branch 0 'the branch holds the candidate' -- \
    in_repo bash -c 'git add change.yaml candidate.json && git commit -qm "next: the change"'
  check merge-gate 0 '#81: the merge gate is clear: the records after the merge verify' -- \
    in_repo "$ci/merge-gate.sh" 'releases/*.json' 'runs/*.json' --base main
  git -C repo checkout -q main && git -C repo merge -q --no-ff --no-edit next || exit 2
fi

if stage 3 "the publish"; then
  check record-next 0 "record $NAME@1.0.1, as the pipeline published it" -- \
    in_repo "$strata" snapshot "$NEXT" -o releases/1.0.1.json
  check same-release 0 '#81: Palette holds the content that the reviewer read' -- \
    in_repo "$strata" same-release candidate.json releases/1.0.1.json
  check commit-publish 0 'main records the published version' -- \
    in_repo bash -c 'git add releases && git commit -qm "record 1.0.1"'
  # the publish of the next version: Palette lists it with the release, so behind names it for the
  # record of the release and for the record of cluster A, which runs the release
  check behind-profile 1 "#707: behind on the records of the release and of cluster A exits 1: $NAME has a newer version" -- \
    "$strata" behind repo/releases/1.0.0.json repo/runs/a.json --no-impact --json
  check behind-profile-names 0 "#707: behind names $NAME@1.0.1 as a newer version of $NAME@1.0.0, for the two records" -- \
    jq -e --arg name "$NAME" '[.profiles[] | select(.name == $name and .version == "1.0.0")]
      | length == 1 and (.[0].pinnedBy | length) == 2 and any(.[0].newer[]; .version == "1.0.1")' behind-profile.out
  # Palette answers for a version that it holds: the move of cluster A to the next version. Both
  # commands exit 1, because the roots differ; the answers are in the report.
  check ask-repave 1 '#385: compare --repave: Palette answers if the move of cluster A to the next version repaves' -- \
    "$strata" compare "cluster:$C/$REL" "$NEXT" --repave --json
  check ask-repave-answer 0 '#385: the report holds the answer of Palette: paletteRepave.required' -- \
    jq -e '.paletteRepave | (.required | type) == "boolean"' ask-repave.out
  check ask-validate 1 '#385: compare --validate: Palette answers if it accepts the packs of the move' -- \
    "$strata" compare "cluster:$C/$REL" "$NEXT" --validate --json
  check ask-validate-answer 0 '#385: the report holds the answer of Palette: paletteValidation.accepts' -- \
    jq -e '.paletteValidation | (.accepts | type) == "boolean"' ask-validate.out
  # cluster A runs the release and the add-on. A cluster ref that selects no profile holds only the
  # profile of the other side
  check cluster-plain 0 '#615: compare of the release with cluster:<uid> holds the release alone, so cluster A runs it' -- \
    "$strata" compare repo/releases/1.0.0.json "cluster:$C"
  check ask-repave-plain 1 '#615: compare --repave with cluster:<uid> asks Palette about the move of the release' -- \
    "$strata" compare "cluster:$C" "$NEXT" --repave --json
  check ask-repave-plain-says 0 '#615: the cluster side is cluster:<uid>/<release>, and the report names no pack of the add-on' -- \
    jq -e --arg ref "cluster:$C/$NAME" --slurpfile addon repo/releases/addon.json \
      '([$addon[0].packs[].name] as $ad | .a.ref == $ref and ([.packs[].name] - $ad == [.packs[].name]))' ask-repave-plain.out
  # the pull request that moves cluster A asks the same question: its comment holds the answer
  # of Palette, and the label follows it
  check ask-repave-comment 0 '#157: pr-comment.sh --repave asks Palette about the move of cluster A, in the comment of the pull request' -- \
    in_repo bash -c "\"$ci/pr-comment.sh\" \"cluster:$C/$REL\" \"$NEXT\" --repave --dry-run > repave-comment.md"
  check ask-repave-comment-says 0 '#157: the comment holds the line of Palette, and the note says that Palette decided the label' -- \
    bash -c "grep -Eq '^\*\*Palette:\*\* (no )?repave' repo/repave-comment.md && grep -Eq 'label [^ ]+: Palette says (no )?repave' ask-repave-comment.err"
fi

if stage 4 "before a build"; then
  engine=$(var cloud)
  engine=${engine:-aws}
  case "$engine" in aws|eks|gcp|gke) ;; *) die "no body of a create for the cloud '$engine'" ;; esac
  create_body "$engine" "$REL" strata-loop-build build.json
  check build-body 0 "#385: the body of the create names the release profile and the cloud account $account_name" -- \
    jq -e --arg a "$A" --arg r "$REL" '.spec.cloudAccountUid == $a and .spec.profiles[0].uid == $r' build.json
  check preflight-static 0 '#385: the static tier of preflight finds no fault in the body, with no request' -- \
    "$strata" preflight build.json --cloud "$engine" --tier static --offline --quiet
  check preflight-comment 0 '#158: pr-comment.sh --preflight posts the markdown report of the static tier as the comment of the pull request' -- \
    bash -c "\"$strata\" preflight build.json --cloud \"$engine\" --tier static --offline --quiet --format markdown > preflight-report.md && \"$ci/pr-comment.sh\" --preflight preflight-report.md --dry-run > preflight-comment.md"
  check preflight-comment-says 0 '#158: the comment starts with the marker of its own and holds the verdict' -- \
    bash -c "head -n 1 preflight-comment.md | grep -qx '<!-- strata-preflight -->' && grep -q '^\*\*OK: ' preflight-comment.md"
  # the job of a pipeline runs preflight.sh, as the drop-ins do. Offline, the log and the report of
  # the job say so. With the key of the lab where strata finds it, the job reads Palette.
  check preflight-job 0 '#618: preflight.sh --offline runs the static tier of the body as the job of a pull request' -- \
    env STRATA_PREFLIGHT_SOURCE=build.json STRATA_PREFLIGHT_CLOUD="$engine" STRATA_PREFLIGHT_TIER=static \
    STRATA_PREFLIGHT_OUT=preflight-job "$ci/preflight.sh" --offline
  check preflight-job-says 0 '#618: the log of the job says that the run is offline, and why, and the report says offline after the tiers' -- \
    bash -c "grep -qx 'preflight: the run is offline, so it reads nothing from Palette: --offline' preflight-job.err && grep -q 'tiers static, offline\.' preflight-job.out"
  check preflight-job-key 0 '#618: with the key of the lab where strata finds it, preflight.sh reads Palette' -- \
    env STRATA_PREFLIGHT_SOURCE=build.json STRATA_PREFLIGHT_CLOUD="$engine" STRATA_PREFLIGHT_TIER=static,palette \
    STRATA_PREFLIGHT_OUT=preflight-job-key "$ci/preflight.sh"
  check preflight-job-key-says 0 '#618: the log and the report of the job with the key do not say offline' -- \
    bash -c "! grep -q 'the run is offline' preflight-job-key.err && grep -q 'tiers static, palette\.' preflight-job-key.out"
  # the tier of Palette calls the validate routes of Palette, and each of them creates nothing
  check preflight-live 0 '#385: the tier of Palette finds no fault in the body, and --strict is the gate' -- \
    "$strata" preflight build.json --cloud "$engine" --tier static,palette --no-exec --strict --quiet
  # the check of the cluster templates asks Palette for each profile of the body: the body names
  # the release profile alone, so one request
  check preflight-each 0 '#144: the check of the cluster templates asks Palette for each profile of the body' -- \
    bash -c "\"$strata\" preflight build.json --cloud \"$engine\" --tier palette --rule template-outside --json --quiet | jq -e '.checks[0] | .verdict == \"passed\" and (.requests | length) == 1'"
  # the instance type route lists every type of the region, so one request answers all the pools
  # of the body. The type of the release body is offered in its zone
  check preflight-offering 0 '#143: the instance type route offers the type of each pool in its zone, in one request' -- \
    bash -c "\"$strata\" preflight build.json --cloud \"$engine\" --tier palette --rule instance-offering --json --quiet | jq -e '.checks[0] | .verdict == \"passed\" and (.ran | contains(\"/instancetypes?cloudAccountUid=\"))'"
  # the key pair of the body: on aws the property route of Palette reads it, for iaas-pod-identity.
  # On eks the key pair route reads it, for ssh-key-pair. A body on gcp or gke has no key pair
  case "$engine" in
    aws) key_rule=iaas-pod-identity ;;
    eks) key_rule=ssh-key-pair ;;
    *) key_rule= ;;
  esac
  if [ -n "$key_rule" ]; then
    check preflight-key-pair 0 "#433: $key_rule passes: the account of the lab holds the key pair of the body" -- \
      bash -c "\"$strata\" preflight build.json --cloud \"$engine\" --tier palette --rule $key_rule --json --quiet | jq -e '.checks[0] | .verdict == \"passed\"'"
  fi
  jq '.spec.cloudAccountUid = "000000000000000000000000"' build.json > build-fault.json
  check preflight-fault 1 '#385: a body with a cloud account that the project lacks fails the gate' -- \
    "$strata" preflight build-fault.json --cloud "$engine" --tier static,palette --no-exec --strict --quiet
fi

if stage 5 "the nightly jobs"; then
  check nightly 0 '#81: verify-report.sh: every record verifies, and each issue would close' -- \
    in_repo "$ci/verify-report.sh" 'releases/*.json' 'runs/*.json' --dry-run
  check fleet 0 '#81: the fleet of a label, one record for each cluster' -- \
    in_repo "$strata" snapshot tag:purpose=strata-live-example --dir fleet --prune
  check fleet-has-a 0 '#81: the fleet holds cluster A' -- \
    bash -c "jq -e --arg c $C '.source.cluster.uid == \$c' repo/fleet/*.json | grep -q true"
  check upstream 1 '#81: upstream-report.sh: the registry has newer versions of what the records pin' -- \
    in_repo "$ci/upstream-report.sh" 'releases/*.json' --dry-run --no-impact
  check upstream-asks 1 '#166: upstream-report.sh asks Palette whether it accepts the bump of each pin that is behind' -- \
    in_repo "$ci/upstream-report.sh" 'releases/*.json' --dry-run
  check upstream-asks-says 0 '#166: the bump column of an issue holds the answer of Palette' -- \
    grep -Eq '<br>palette (accepts|refuses: )' upstream-asks.out
fi

if stage 6 "behind is not different"; then
  check override 0 '#81: a person overrides a value on cluster A, through the API' -- \
    "$live/api.sh" override "$C" "$addon_from" "$addon_to"
  check override-record 0 'record cluster A after the override' -- \
    "$strata" snapshot "cluster:$C" -o override.json
  check override-not-behind 0 '#81: Palette says that no update waits for cluster A: an override opens no notification' -- \
    jq -e '.source.cluster.updatesPending == false' override.json
  check override-drift 1 '#81: the record of cluster A reports the drift' -- "$strata" verify repo/runs/a.json
  check override-release 0 '#81: the release record still verifies: the profile did not change' -- \
    "$strata" verify repo/releases/1.0.0.json repo/releases/addon.json
  check override-undo 0 'undo the override: the content of the profiles, put on cluster A again' -- "$live/api.sh" apply "$C"
  check override-gone 0 'the record of cluster A verifies again' -- "$strata" verify repo/runs/a.json
fi

if stage 7 "an edit of a published version in place"; then
  check edit 0 '#81: a person edits the add-on version in place, through the API' -- \
    "$live/api.sh" edit "$AD" "$addon_from" "$addon_to"
  check nightly-drift 1 '#81: the nightly job would open an issue for the add-on record' -- \
    in_repo "$ci/verify-report.sh" 'releases/*.json' 'runs/*.json' --dry-run
  check nightly-names 0 '#81: the issue is for releases/addon.json' -- \
    grep -q 'would open or update the issue for releases/addon.json' nightly-drift.err
  check who 1 '#81: --who names who wrote the edit' -- "$strata" verify repo/releases/addon.json --who
  check who-says 0 '#81: the last write is an update by a person' -- \
    grep -Eq '^  [0-9TZ:.-]+  .+ \(user\)  update(: .+)?$' <(grep -E '^  [0-9]{4}-' who.out | tail -1)
  check nightly-who 1 '#158: verify-report.sh --who puts the audit log in the issue of the add-on record' -- \
    in_repo "$ci/verify-report.sh" 'releases/*.json' 'runs/*.json' --who --dry-run
  check nightly-who-says 0 '#158: the body of the issue names the last write: an update by a person' -- \
    grep -Eq '^- [0-9]{4}-[0-9]{2}-.+ \(user\)  update' nightly-who.out
  check cluster-kept 0 '#81: cluster A keeps its copy: its record verifies' -- "$strata" verify repo/runs/a.json
  check behind-record 0 'record cluster A with the edit waiting' -- "$strata" snapshot "cluster:$C" -o behind.json
  check behind 0 '#81: Palette says that an update waits for cluster A' -- \
    jq -e '.source.cluster.updatesPending == true' behind.json
  check edit-undo 0 'undo the edit' -- "$live/api.sh" edit "$AD" "$addon_to" "$addon_from"
  check edit-gone 0 'the add-on record verifies again' -- "$strata" verify repo/releases/addon.json
fi

if stage 8 "an incident"; then
  pack=$(jq -r '[.packs[] | select(.layer == "csi")][0].name' repo/releases/1.0.0.json)
  check incident-list 0 "#81: the versions that carry $pack" -- "$strata" list --pack "$pack" --json
  check incident-list-has 0 '#81: the release is one of them' -- \
    jq -e --arg u "$REL" '[.profiles[].uid] | index($u) != null' incident-list.out
  check incident-consumers 1 '#81: cluster A runs the release' -- "$strata" consumers "$REL"
  check incident-compare 0 '#81: and it runs exactly the recorded release' -- \
    "$strata" compare repo/releases/1.0.0.json "cluster:$C/$REL"
fi

if stage 9 "the retire"; then
  check retire 0 "#81: nothing runs $NAME@1.0.1: the retire gate is open" -- "$strata" consumers "$NEXT"
  check report-log 0 "#396: strata report reads the log of the unknown values that this run wrote" -- "$strata" report
  check baseline 0 'the example is at its baseline' -- "$live/baseline.sh"

  echo
  echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
  [ "$failures" = 0 ]
fi
