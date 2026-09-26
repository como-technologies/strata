#!/usr/bin/env bash
# The one-state-per-version test (#81): the other answer to a version bump that overwrites a
# version. Each version of a profile is its own resource in its own OpenTofu state, a workspace of
# terraform-version/, with no feature_preview flag. A new version is then a new state and a new
# resource, and the state of the version before it is not touched.
#
#   examples/live/version-states.sh
#
#   1  version 1.0.0 in its own state, and a record of it
#   2  version 1.0.1 in a second state: a new uid, and the record of 1.0.0 still verifies
#   3  the state of 1.0.0 plans no change; a change of its content, at the same version, plans an
#      update in place: the provider keeps that path open, and verify is what finds it
#   4  with cluster A: a cluster runs 1.0.0, and a destroy of its state is refused; after the
#      cluster lets it go, the destroy removes it
#   5  the states destroyed, and no version of the name left
#   6  a short version number and one with a v prefix, through the provider: the version that
#      Palette stores
#
# Needs the live example with cluster A (just live-up), the strata release binary, tofu, jq, and
# the key as verify.sh needs it. It leaves nothing behind. Results go under
# runs/<UTC time>/version-states/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=version-states
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/version-states}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

load_outputs
C=$(out cluster_uid)
[ -n "$C" ] || die "no cluster A; run just live-up"
export PALETTE_PROJECT_UID=$(out project_uid)
project=$(sed -n 's/^palette_project *= *"\(.*\)"/\1/p' "$live/terraform/terraform.tfvars")
[ -n "$project" ] || die "no palette_project in terraform/terraform.tfvars"
NAME=strata-live-versions
# tofu in the one-version module; baseline.sh and the other scripts keep the example's module
vtf() { TF_DIR=$live/terraform-version "$live/tf.sh" "$@"; }

summary() {
  {
    echo "# one state per version"
    echo
    echo "Run \`$(basename "$(dirname "$run")")\` with \`$("$strata" --version)\`, profile \`$NAME\`, cluster \`$C\`."
    echo
    summary_table
  } > summary.md
}

# state VERSION: select the workspace of the version, and make it on first use
state() { vtf workspace select -or-create "v$1" > /dev/null; }
# tv VERSION VALUE ACTION ARG...: tofu ACTION in the state of VERSION, with its variables
tv() {
  local v=$1 value=$2 action=$3
  shift 3
  state "$v" || return 2
  vtf "$action" -input=false -no-color -var "palette_project=$project" \
    -var "profile_version=$v" -var "value=$value" "$@"
}
uid_of() { state "$1" && vtf output -raw uid; }

# cleanup: the cluster lets 1.0.0 go, and each state is destroyed; each step changes nothing where
# there is nothing to undo
cleanup() {
  if [ -n "${U1:-}" ] && "$live/api.sh" get "/v1/spectroclusters/$C" | jq -e --arg u "$U1" \
      '[.spec.clusterProfileTemplates[].uid] | index($u) != null' > /dev/null 2>&1; then
    "$live/api.sh" detach "$C" "$U1" >> steps.log 2>&1
  fi
  tv 1.0.1 two destroy -auto-approve >> destroy.log 2>&1
  tv 1.0.0 one destroy -auto-approve >> destroy.log 2>&1
  tv 1.2 short destroy -auto-approve >> destroy.log 2>&1
  tv v1.2.3 prefixed destroy -auto-approve >> destroy.log 2>&1
  vtf workspace select default > /dev/null 2>&1
  vtf workspace delete v1.0.0 > /dev/null 2>&1
  vtf workspace delete v1.0.1 > /dev/null 2>&1
  vtf workspace delete v1.2 > /dev/null 2>&1
  vtf workspace delete vv1.2.3 > /dev/null 2>&1
  summary
}
trap cleanup EXIT

check init 0 'tofu init of the one-version module' -- vtf init -input=false -no-color

echo "== 1. version 1.0.0, in its own state" | tee -a steps.log
check apply-1 0 "#81: apply $NAME@1.0.0 in the state v1.0.0" -- tv 1.0.0 one apply -auto-approve
U1=$(uid_of 1.0.0)
check record-1 0 "record $NAME@1.0.0" -- "$strata" snapshot "$U1" -o v1.json

echo "== 2. version 1.0.1, in a second state" | tee -a steps.log
check apply-2 0 "#81: apply $NAME@1.0.1 in the state v1.0.1" -- tv 1.0.1 two apply -auto-approve
U2=$(uid_of 1.0.1)
check new-uid 0 '#81: the new version has a uid of its own' -- test -n "$U2" -a "$U1" != "$U2"
check old-verifies 0 '#81: the record of 1.0.0 verifies: the bump did not touch it' -- "$strata" verify v1.json
check both-listed 0 '#81: Palette lists both versions of the name' -- \
  bash -c "\"$strata\" list profiles --json | jq -e --arg n $NAME '[.profiles[] | select(.name == \$n) | .version] | sort == [\"1.0.0\", \"1.0.1\"]'"

echo "== 3. the state of 1.0.0" | tee -a steps.log
check plan-same 0 '#81: the state of 1.0.0 plans no change' -- tv 1.0.0 one plan -detailed-exitcode
check plan-edit 2 '#81: a change of the content of 1.0.0, at the same version, plans a change' -- \
  tv 1.0.0 edited plan -detailed-exitcode
check plan-edit-in-place 0 '#81: the plan is an update in place of the published version' -- \
  grep -q 'will be updated in-place' plan-edit.out

echo "== 4. a cluster runs 1.0.0" | tee -a steps.log
check attach 0 "cluster A runs $NAME@1.0.0 too, through the API" -- "$live/api.sh" attach "$C" "$U1"
# Palette lists the consumer a moment after the update of the cluster
for _ in $(seq 1 30); do "$strata" consumers "$U1" > /dev/null 2>&1 && sleep 10 || break; done
check in-use 1 "#81: cluster A uses $NAME@1.0.0" -- "$strata" consumers "$U1"
check destroy-refused 1 '#81: a destroy of the state of 1.0.0 is refused while a cluster uses the version' -- \
  tv 1.0.0 one destroy -auto-approve
check destroy-refused-says 0 'the refusal says that the version is in use' -- grep -qi 'in-use\|in use' destroy-refused.err
check still-there 0 '#81: the version is still there, and its record verifies' -- "$strata" verify v1.json
check detach 0 "cluster A lets $NAME@1.0.0 go" -- "$live/api.sh" detach "$C" "$U1"
for _ in $(seq 1 30); do "$strata" consumers "$U1" > /dev/null 2>&1 && break || sleep 10; done
check free 0 "nothing uses $NAME@1.0.0" -- "$strata" consumers "$U1"

echo "== 5. the states destroyed" | tee -a steps.log
check destroy-1 0 '#81: the destroy of the state of 1.0.0 removes the version' -- tv 1.0.0 one destroy -auto-approve
check old-gone 1 '#81: the record of 1.0.0 reports the version gone' -- "$strata" verify v1.json
check old-gone-says 0 '#81: the verdict says that the version no longer exists' -- grep -q 'no longer exists: uid' old-gone.out
check destroy-2 0 'destroy the state of 1.0.1' -- tv 1.0.1 two destroy -auto-approve
check none-left 0 "no version of $NAME is left" -- \
  bash -c "\"$strata\" list profiles --json | jq -e --arg n $NAME '[.profiles[] | select(.name == \$n)] | length == 0'"

echo "== 6. the version number that Palette stores" | tee -a steps.log
check apply-short 0 "#81: apply $NAME with the version 1.2" -- tv 1.2 short apply -auto-approve
check stored-short 0 '#81: the version that Palette lists for 1.2' -- \
  bash -c "\"$strata\" list profiles --json | jq -r --arg n $NAME '.profiles[] | select(.name == \$n) | .version'"
check replan-short 0 '#81: a plan after it has no change: Palette stored the string as given' -- \
  tv 1.2 short plan -detailed-exitcode
check short-as-given 0 '#81: Palette stores 1.2, and not 1.2.0' -- test "$(tr -d '[:space:]' < stored-short.out)" = 1.2
check destroy-short 0 'destroy it' -- tv 1.2 short destroy -auto-approve
check apply-prefixed 0 "#81: Palette accepts the version v1.2.3" -- tv v1.2.3 prefixed apply -auto-approve
check stored-prefixed 0 '#81: the version that Palette lists for v1.2.3' -- \
  bash -c "\"$strata\" list profiles --json | jq -r --arg n $NAME '.profiles[] | select(.name == \$n) | .version'"
check prefixed-as-given 0 '#81: Palette stores v1.2.3, and not 1.2.3' -- test "$(tr -d '[:space:]' < stored-prefixed.out)" = v1.2.3
check destroy-prefixed 0 'destroy it' -- tv v1.2.3 prefixed destroy -auto-approve
check baseline 0 'the example is at its baseline' -- "$live/baseline.sh"

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]
