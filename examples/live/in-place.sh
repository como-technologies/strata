#!/usr/bin/env bash
# The in-place edit test (#27, #23): what an in-place edit and a version bump of the add-on profile
# do to the clusters that run it. Run it once with the Terraform provider's immutable-clusterprofiles
# flag off and once with it on; the two runs differ in nothing else.
#
#   examples/live/in-place.sh off|on [FROM_STAGE]
#
#   1  record cluster A and the release and add-on versions, and what Terraform plans with the flag:
#      no change, and an in-place edit of the add-on version
#   2  edit the add-on version in place through the API, create cluster B, and record both
#      clusters without applying anything to cluster A
#   3  push the add-on's content to cluster A without resolving its notification, then accept the
#      waiting update, and wait until cluster A matches cluster B
#   4  undo the edit and bump the add-on version with the flag; record both clusters and both
#      versions
#   5  delete cluster B and restore the baseline
#
# What varies: the flag, in every Terraform plan and every apply that changes a profile. What holds:
# the edit (api.sh edit in both runs), the clusters (one definition), the steps and their order,
# and the baseline the run starts from and returns to (lib.sh). Terraform's own handling of the
# edit is a check, not a step: with the flag off it plans an update in place, with it on it
# refuses.
#
# Needs the live example at its baseline (baseline.sh), the strata release binary, jq, and the key
# as verify.sh needs it. Billable: cluster B takes about 20 minutes to create and 10 to 30 to
# delete. Results go under runs/<UTC time>/in-place/<off|on>/, or OUT; FROM_STAGE resumes a run in
# OUT. Each check compares an exit code with the expected one; `?` records it without judging.
# With OFF_RUN set to an off run's directory, an on run also compares its bumped version with the
# off run's. When an apply fails or cluster B does not reach Running, diagnostics go under the run's
# diagnostics/. Exit 0 only when no check fails.
set -uo pipefail
script=in-place
mode=${1:-}
from=${2:-1}
case "$mode" in
  off) flag=false ;;
  on) flag=true ;;
  *) sed -n '2,/^set -uo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"

# Expected exits that differ with the flag, and what Terraform says about the edit.
if [ "$mode" = on ]; then
  off0_on1=1 off1_on0=0 off2_on1=1
  plan_edit_says='immutable-clusterprofiles: cluster profile versions are immutable'
else
  off0_on1=0 off1_on0=1 off2_on1=2
  plan_edit_says='spectrocloud_cluster_profile.addon will be updated in-place'
fi

edit_from='edited: "no"'
edit_to='edited: "yes"'

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/in-place/$mode}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

# summary.md from steps.log, written on any exit, so a resumed run keeps its earlier stages
summary() {
  {
    echo "# in-place edit test, flag $mode"
    echo
    echo "Run \`$(basename "$(dirname "$(dirname "$run")")")\` with \`$("$strata" --version)\`. Cluster A \`${C:-}\`,"
    echo "cluster B \`${B:-}\`, release \`${P:-}\`, add-on \`${AN:-}\` uids \`${AD1:-}\` and \`${AD2:-}\`."
    echo
    summary_table
  } > summary.md
}
trap summary EXIT

# ids.env keeps the uids across stages, so a run can resume.
save() { echo "$1=$2" >> ids.env; }
[ -f ids.env ] && . ./ids.env
stage() { [ "$from" -le "$1" ] && echo "== stage $1: $2" | tee -a steps.log; }

load_outputs
if [ "$from" -le 1 ]; then
  require_baseline
  : > ids.env
  PALETTE_PROJECT_UID=$(out project_uid)
  C=$(out cluster_uid)
  N=$(out profile_name)
  P=$(out release_profile_uid)
  AN=$(out addon_profile_name)
  AD1=$(out addon_profile_uid)
  for v in PALETTE_PROJECT_UID C N P AN AD1; do
    [ -n "${!v}" ] || die "the terraform output behind $v is empty"
    save "$v" "${!v}"
  done
fi
export PALETTE_PROJECT_UID

if stage 1 "record the baseline"; then
  check a1 0 "record cluster A" -- "$strata" snapshot "cluster:$C" --raw-dir raw/a1 -o a1.json
  check a1-addon 0 "record cluster A's add-on selection" -- "$strata" snapshot "cluster:$C/$AN" --raw-dir raw/a1-addon -o a1-addon.json
  check release1 0 "record the release version" -- "$strata" snapshot "$P" --raw-dir raw/release1 -o release1.json
  check addon1 0 "record the add-on version" -- "$strata" snapshot "$AD1" --raw-dir raw/addon1 -o addon1.json
  check addon1-is-1.0.0 0 "the add-on version is 1.0.0" -- jq -e '.source.version == "1.0.0"' addon1.json
  check a1-updates 0 "what waits for cluster A" -- "$live/tenant.sh" updates "$C"
  check a-owns-manifest-copy 0 "cluster A runs its own copy of the add-on manifest, not the profile's" -- \
    jq -e -n --slurpfile c a1-addon.json --slurpfile p addon1.json \
      '$c[0].packs[0].manifests[0].uid != $p[0].packs[0].manifests[0].uid'
  check plan-no-change "$off0_on1" "Terraform plans no change at the baseline; provider 0.29.7 refuses that plan with the flag on (lib.sh)" -- \
    tf_plan "$flag"
  check plan-edit "$off2_on1" "#27: Terraform's plan for an in-place edit of the add-on version: an update in place without the flag, a refusal with it" -- \
    tf_plan "$flag" -var edit_addon_in_place=true
  check plan-edit-says 0 "the plan says which" -- command grep -q "$plan_edit_says" plan-edit.out plan-edit.err
fi

if stage 2 "edit the add-on version in place and create cluster B"; then
  check api-edit 0 "#27: edit the add-on version in place through the API" -- \
    "$live/api.sh" edit "$AD1" "$edit_from" "$edit_to"
  # The configuration now matches the edited version, so this creates cluster B and changes no profile.
  apply_clusters -var edit_addon_in_place=true -var second_cluster=true
  B=$(out cluster_b_uid)
  save B "$B"
  check a2 0 "record cluster A again, applying nothing" -- "$strata" snapshot "cluster:$C" --raw-dir raw/a2 -o a2.json
  check a2-addon 0 "record cluster A's add-on selection" -- "$strata" snapshot "cluster:$C/$AN" --raw-dir raw/a2-addon -o a2-addon.json
  check b2-addon 0 "record cluster B's add-on selection" -- "$strata" snapshot "cluster:$B/$AN" --raw-dir raw/b2-addon -o b2-addon.json
  check addon2 0 "record the edited add-on version" -- "$strata" snapshot "$AD1" --raw-dir raw/addon2 -o addon2.json
  check edit-kept-manifest-uid 0 "the edit kept the add-on manifest's uid, so cluster copies still name it" -- \
    jq -e -n --slurpfile a addon1.json --slurpfile b addon2.json \
      '$a[0].packs[0].manifests[0].uid == $b[0].packs[0].manifests[0].uid'
  check a2-updates 0 "what waits for cluster A" -- "$live/tenant.sh" updates "$C"
  check b-owns-manifest-copy 0 "cluster B was created with its own copy of the add-on manifest" -- \
    jq -e -n --slurpfile c b2-addon.json --slurpfile p addon2.json \
      '$c[0].packs[0].manifests[0].uid != $p[0].packs[0].manifests[0].uid'
  check a-kept-old-content 0 "#27: cluster A keeps its copy from before the edit" -- \
    "$strata" compare a1-addon.json a2-addon.json
  check a-vs-b 1 "#27: cluster A differs from cluster B, created from the edited version" -- \
    "$strata" compare a2-addon.json b2-addon.json --json
  check a-vs-b-edited-in-place 0 "#27: the comparison says the version was edited in place" -- \
    jq -e '.editedInPlace' a-vs-b.out
  check verify-a1 0 "#27: cluster A's record from before the edit still verifies" -- "$strata" verify a1.json
  check verify-release1 0 "#23: the release record is untouched by the add-on edit" -- "$strata" verify release1.json
  check verify-addon1 1 "#23: the add-on record reports DRIFT after its in-place edit" -- "$strata" verify addon1.json
  check addon1-vs-a2 0 "#27: the add-on record from before the edit matches cluster A's selection" -- \
    "$strata" compare addon1.json "cluster:$C/$AN"
  check consumers-addon 1 "both clusters use the add-on version" -- "$strata" consumers "$AD1" --json
fi

if stage 3 "apply the add-on profile to cluster A"; then
  # A resumed stage 3 does not push again once the update was accepted: the answer to the push
  # without resolving means something only before that.
  if ! command grep -q '^PASS  apply-to-a ' steps.log; then
    check apply-keeping-notification 0 "push the add-on's content to cluster A without resolving its notification" -- \
      "$live/api.sh" apply "$C" --keep-notification
    sleep 180
    check a-changed-without-resolve 1 "#27: three minutes after that push, cluster A's copy has taken the edit" -- \
      "$strata" compare a2-addon.json "cluster:$C/$AN"
    check apply-to-a 0 "#27: accept the waiting update on cluster A, resolving the notification" -- \
      "$live/api.sh" apply "$C"
  fi
  for _ in $(seq 1 30); do
    "$strata" snapshot "cluster:$C/$AN" -o a3-addon.json 2>/dev/null \
      && "$strata" compare a3-addon.json b2-addon.json >/dev/null 2>&1 && break
    sleep 30
  done
  check a3 0 "record cluster A after the apply" -- "$strata" snapshot "cluster:$C" --raw-dir raw/a3 -o a3.json
  check a3-updates 0 "what waits for cluster A" -- "$live/tenant.sh" updates "$C"
  check a3-matches-b 0 "#27: after the apply, cluster A runs what cluster B runs" -- \
    "$strata" compare a3-addon.json b2-addon.json
  check verify-a1-after-apply 1 "#27: once the update is applied, cluster A's record from before the edit reports DRIFT" -- "$strata" verify a1.json
fi

if stage 4 "undo the edit and bump the add-on version"; then
  apply_profile spectrocloud_cluster_profile.addon \
    -var addon_version=1.0.1 -var second_cluster=true -var "cluster_addon_profile_uid=$AD1"
  AD2=$(out addon_profile_uid)
  save AD2 "$AD2"
  check addon-uid-kept "$off0_on1" "#27: the bump keeps the add-on uid without the flag and makes a new one with it" -- test "$AD2" = "$AD1"
  check addon-old4 0 "record what the first add-on uid holds now" -- "$strata" snapshot "$AD1" --raw-dir raw/addon-old4 -o addon-old4.json
  check addon-new4 0 "record the bumped add-on version" -- "$strata" snapshot "$AD2" --raw-dir raw/addon-new4 -o addon-new4.json
  check a4-addon 0 "record cluster A's add-on selection" -- "$strata" snapshot "cluster:$C/$AN" --raw-dir raw/a4-addon -o a4-addon.json
  check b4-addon 0 "record cluster B's add-on selection" -- "$strata" snapshot "cluster:$B/$AN" --raw-dir raw/b4-addon -o b4-addon.json
  check verify-addon1-after-bump 1 "#27: the add-on record from before the edit still reports DRIFT" -- \
    "$strata" verify addon1.json --json
  check addon1-overwritten "$off0_on1" "#27: verify says the recorded version was overwritten, only without the flag" -- \
    jq -e '.verify.overwrittenBy != null' verify-addon1-after-bump.out
  check a4-matches-old-version "$off1_on0" "#23, #27: cluster A's selection equals the first add-on uid only with the flag, which keeps the old version" -- \
    "$strata" compare addon-old4.json a4-addon.json
  check a4-vs-old-json "$off1_on0" "the same comparison as JSON" -- \
    "$strata" compare addon-old4.json a4-addon.json --json
  check a4-names-overwrite "$off0_on1" "#29: the comparison names the first add-on uid as overwritten in place, only without the flag" -- \
    jq -e --arg u "$AD1" '(.overwrittenInPlace // []) | map(.uid) == [$u]' a4-vs-old-json.out
  check a4-still-a3 0 "#27: the bump left cluster A as it was" -- "$strata" compare a3-addon.json a4-addon.json
  check verify-a3-after-bump "$off1_on0" "#30: verify of cluster A's record from stage 3 fails only without the flag: the version it runs was overwritten" -- \
    "$strata" verify a3.json --json
  check a3-names-overwritten-profile "$off0_on1" "#30: the verify names the add-on profile, only without the flag" -- \
    jq -e --arg u "$AD1" '[.verify.profilesOverwritten[]?.uid] == [$u]' verify-a3-after-bump.out
  check consumers-old 1 "#27: both clusters are still consumers of the first add-on uid" -- "$strata" consumers "$AD1" --json
  if [ "$mode" = on ] && [ -n "${OFF_RUN:-}" ]; then
    check clone-carried-nothing 1 "#27: the flag's clone is not the same release as the legacy bump: Palette mints the manifest pack's uid for each profile (#32)" -- \
      "$strata" same-release "$OFF_RUN/addon-new4.json" addon-new4.json
    check clone-same-content 0 "#27: the clone has the values, manifests, and variables of the legacy bump of the same configuration" -- \
      jq -e -n --slurpfile a "$OFF_RUN/addon-new4.json" --slurpfile b addon-new4.json \
        '[$a[0].packs[] | .hash.authored] == [$b[0].packs[] | .hash.authored] and $a[0].variablesHash == $b[0].variablesHash'
  fi
fi

if stage 5 "delete cluster B and restore the baseline"; then
  "$live/tenant.sh" cluster "$B" >/dev/null 2>&1 || wait_running "$B"  # exit 0: already deleted
  apply_clusters '-target=spectrocloud_cluster_aws.cluster["b"]' \
    -var addon_version=1.0.1 -var second_cluster=false -var "cluster_addon_profile_uid=$AD1"
  check b-gone 0 "cluster B is deleted" -- "$live/tenant.sh" cluster "$B"
  restore_addon "$AD1" addon1.json
  check a5-matches-addon 0 "cluster A runs the restored add-on version" -- "$strata" compare "$AD1" "cluster:$C/$AN"
  check baseline-after 0 "the live example is back at its baseline" -- "$live/baseline.sh"
fi

echo
echo "$failures failed; details in $run"
[ "$failures" -eq 0 ]
