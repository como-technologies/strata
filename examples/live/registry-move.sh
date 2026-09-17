#!/usr/bin/env bash
# The registry-move check (#24): a real move of the CSI pack to another registry, made the way a
# team promotes one with the Terraform provider's immutable-clusterprofiles flag, as a version bump
# that keeps the old version.
#
#   examples/live/registry-move.sh
#
#   1  create the move profile at 1.0.0 with every pack from the pack registry
#   2  bump it to 1.0.1 with the CSI pack from the OCI registry (move_registry_name); the flag
#      clones 1.0.0 and keeps it
#   3  run same-release on the two records, exactly and with --allow registry-move
#   4  edit 1.0.1's CSI values in place through the API and run the check again
#   5  delete both versions
#
# Needs the live example built, with no cluster B, the strata release binary, and the key as
# verify.sh needs it. Nothing runs the move profile, so nothing here is billable. Results go under
# runs/<UTC time>/registry-move/, or OUT. Each check compares an exit code with the expected one;
# `?` records the answer without judging it. Exit 0 only when no check fails.
#
# Provider 0.29.7 refuses every flag-on plan that includes an existing profile whose version stays
# the same, even one that changes nothing (see in-place.sh). The first check records that as
# expected until the provider release that fixes it, and every apply here targets the move profile.
set -uo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
strata=${STRATA_BIN:-$repo/target/release/strata}
[ -x "$strata" ] || { echo "registry-move: no strata binary at $strata" >&2; exit 2; }

run=${OUT:-$here/runs/$(date -u +%Y%m%dT%H%M%SZ)/registry-move}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

out() { "$here/tf.sh" output -json | jq -r --arg k "$1" '.[$k].value // empty'; }
applies=0
apply() {
  applies=$((applies + 1))
  echo "terraform apply $*" | tee -a steps.log
  "$here/tf.sh" apply -input=false -auto-approve -no-color -var immutable_profiles=true "$@" \
    -target=spectrocloud_cluster_profile.move > "apply-$applies.txt" 2>&1
  local code=$?
  grep -E '^(Plan|Apply complete|Error)' "apply-$applies.txt" | tee -a steps.log
  [ "$code" = 0 ] || { echo "registry-move: terraform apply failed; see $run/apply-$applies.txt" >&2; exit 2; }
}

failures=0
rows=()
check() {
  local id=$1 want=$2 what=$3
  shift 4
  "$@" > "$id.out" 2> "$id.err"
  local got=$? verdict=PASS
  if [ "$want" = "?" ]; then
    verdict=SEEN
  elif [ "$got" != "$want" ]; then
    verdict=FAIL
    failures=$((failures + 1))
  fi
  rows+=("| $verdict | \`$id\` | $want | $got | $what |")
  printf '%-4s  %-30s expected %s, got %s\n' "$verdict" "$id" "$want" "$got" | tee -a steps.log
}

PALETTE_PROJECT_UID=$(out project_uid)
export PALETTE_PROJECT_UID
[ -z "$(out cluster_b_uid)" ] || { echo "registry-move: cluster B exists; finish in-place.sh first" >&2; exit 2; }

check plan-refuses-no-change 1 "expected with provider 0.29.7: with the flag on, Terraform refuses a plan that changes nothing" -- \
  "$here/tf.sh" plan -input=false -no-color -var immutable_profiles=true

echo "== 1: create the move profile at 1.0.0" | tee -a steps.log
apply -var registry_move=true
M1=$(out move_profile_uid)
check move1 0 "record 1.0.0, every pack from the pack registry" -- "$strata" snapshot "$M1" -o move1.json

echo "== 2: bump it to 1.0.1 with the CSI pack from the OCI registry" | tee -a steps.log
apply -var registry_move=true -var move_version=1.0.1 -var move_csi_to_oci=true
M2=$(out move_profile_uid)
check move-new-uid 1 "the flag made 1.0.1 a new profile version" -- test "$M2" = "$M1"
check move1-kept 0 "1.0.0 is still there, unchanged" -- "$strata" verify move1.json
check move2 0 "record 1.0.1" -- "$strata" snapshot "$M2" -o move2.json
check move2-csi-is-oci 0 "1.0.1's CSI pack comes from the OCI registry" -- \
  jq -e '[.packs[] | select(.name == "csi-aws-ebs") | .type] == ["oci"]' move2.json

echo "== 3: same-release on the two records" | tee -a steps.log
check compare-move 1 "every field that differs between the two records" -- \
  "$strata" compare move1.json move2.json --json
check same-release-exact 1 "an exact check fails: the registry moved" -- \
  "$strata" same-release move1.json move2.json
check same-release-registry-move 0 "#24: the registry-move policy passes a real move, which changes the pack type" -- \
  "$strata" same-release --allow registry-move move1.json move2.json --json

echo "== 4: edit 1.0.1 in place and check again" | tee -a steps.log
check api-edit-move2 0 "edit 1.0.1's CSI values through the API" -- \
  "$here/api.sh" edit "$M2" "reclaimPolicy: Delete" "reclaimPolicy: Retain"
check move2-edited 0 "record the edited 1.0.1" -- "$strata" snapshot "$M2" -o move2-edited.json
check same-release-after-edit 1 "#24: the policy fails the pair after an in-place edit" -- \
  "$strata" same-release --allow registry-move move1.json move2-edited.json --json
check edit-is-authored 0 "the reason names the AUTHORED change" -- \
  jq -e '[.pairs[0].reasons[] | select(contains("AUTHORED"))] | length == 1' same-release-after-edit.out

echo "== 5: delete both versions" | tee -a steps.log
apply -var registry_move=false
check delete-move2 0 "delete 1.0.1" -- "$here/api.sh" delete "$M2"
check delete-move1 0 "delete 1.0.0" -- "$here/api.sh" delete "$M1"

{
  echo "# registry-move check"
  echo
  echo "Run \`$(basename "$(dirname "$run")")\` with \`$("$strata" --version)\`. Move profile 1.0.0 \`$M1\`, 1.0.1 \`$M2\`."
  echo
  echo "| result | check | expected exit | got | what it shows |"
  echo "|---|---|---|---|---|"
  printf '%s\n' "${rows[@]}"
} > summary.md

echo
echo "$failures failed; details in $run"
[ "$failures" -eq 0 ]
