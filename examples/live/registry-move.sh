#!/usr/bin/env bash
# The registry-move check (#24): a real move of the CSI pack to another registry, made the way a
# team promotes one with the Spectro Cloud provider's immutable-clusterprofiles flag, as a version bump
# that keeps the old version.
#
#   examples/live/registry-move.sh
#
#   1  hold skip_destroy for the flag (lib.sh), and create the move profile at 1.0.0 with every pack
#      from the pack registry
#   2  bump it to 1.0.1 with the CSI pack from the OCI registry (move_registry_name); the flag
#      clones 1.0.0 and keeps it
#   3  run same-release on the two records, exactly and with --allow registry-move
#   4  edit 1.0.1's CSI values in place through the API and run the check again
#   5  delete both versions and check the baseline
#
# What varies: the move profile's CSI registry, then its values. What holds: the flag (on), the
# other packs, and the baseline the run starts from and returns to (lib.sh). Nothing runs the move
# profile, so nothing here is billable. Needs the live example at its baseline (baseline.sh), the
# strata release binary, and the key as verify.sh needs it. Results go under
# runs/<UTC time>/registry-move/, or OUT. Each check compares an exit code with the expected one.
# Exit 0 only when no check fails.
set -uo pipefail
script=registry-move
flag=true
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/registry-move}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

summary() {
  write_summary "registry-move check" ". Move profile 1.0.0 \`${M1:-}\`, 1.0.1 \`${M2:-}\`."
}
trap summary EXIT

load_outputs
require_baseline
hold_versions
PALETTE_PROJECT_UID=$(out project_uid)
export PALETTE_PROJECT_UID

check plan-no-change 0 "OpenTofu plans no change once skip_destroy is held for the flag" -- tf_plan "$flag"

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
  jq -e --arg csi "$(out csi_pack_name)" '[.packs[] | select(.name == $csi) | .type] == ["oci"]' move2.json

echo "== 3: same-release on the two records" | tee -a steps.log
check compare-move 1 "every field that differs between the two records" -- \
  "$strata" compare move1.json move2.json --json
check same-release-exact 1 "an exact check fails: the registry moved" -- \
  "$strata" same-release move1.json move2.json
check same-release-registry-move 0 "#24: the registry-move policy passes a real move, which changes the pack type" -- \
  "$strata" same-release --allow registry-move move1.json move2.json --json

echo "== 4: edit 1.0.1 in place and check again" | tee -a steps.log
check api-edit-move2 0 "edit 1.0.1's CSI values through the API" -- \
  "$live/api.sh" edit "$M2" "$(out csi_reclaim_delete)" "$(out csi_reclaim_delete | sed 's/Delete/Retain/')"
check move2-edited 0 "record the edited 1.0.1" -- "$strata" snapshot "$M2" -o move2-edited.json
check same-release-after-edit 1 "#24: the policy fails the pair after an in-place edit" -- \
  "$strata" same-release --allow registry-move move1.json move2-edited.json --json
check edit-is-authored 0 "the reason names the AUTHORED change" -- \
  jq -e '[.pairs[0].reasons[] | select(contains("AUTHORED"))] | length == 1' same-release-after-edit.out

echo "== 5: delete both versions and check the baseline" | tee -a steps.log
# skip_destroy keeps both versions in Palette when this removes the move profile from the state
apply_baseline
check delete-move2 0 "delete 1.0.1" -- "$live/api.sh" delete "$M2"
check delete-move1 0 "delete 1.0.0" -- "$live/api.sh" delete "$M1"
check baseline-after 0 "the live example is back at its baseline" -- "$live/baseline.sh"

echo
echo "$failures failed; details in $run"
[ "$failures" -eq 0 ]
