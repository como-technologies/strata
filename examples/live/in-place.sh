#!/usr/bin/env bash
# The in-place edit test (#27, #23): what an in-place edit and a version bump of the add-on profile
# do to the clusters that run it, with the Terraform provider's immutable-clusterprofiles flag off
# or on.
#
#   examples/live/in-place.sh off|on [FROM_STAGE]
#
#   1  record cluster A, the release and add-on profile versions, and what waits for cluster A
#   2  edit the add-on version in place and create cluster B, then record both clusters without
#      applying anything to cluster A. With the flag on, Terraform must refuse the edit, so it is
#      made through the API.
#   3  push the add-on's content to cluster A without resolving its notification, then accept the
#      waiting update, and wait until cluster A matches cluster B
#   4  undo the edit and bump the add-on version; record both clusters and both versions
#   5  wait for cluster B to be Running, delete it, and put the add-on profile back where
#      `verify.sh` expects it
#
# Needs the live example built with the add-on profile at 1.0.0 and no cluster B, the strata
# release binary, jq, and the key as verify.sh needs it. Billable: cluster B takes about 20 minutes
# to create and 10 to 30 to delete. Results go under runs/<UTC time>/in-place/<off|on>/, or OUT;
# FROM_STAGE resumes a run in OUT. Each check compares an exit code with the expected one; `?`
# records the answer without judging it. With OFF_RUN set to an off run's directory, an on run also
# checks whether the clone of the bumped version carried anything over. Exit 0 only when no check
# fails.
#
# With the flag on, provider 0.29.7 refuses every plan that includes an existing profile whose
# version stays the same, even one that changes nothing: Palette stores the packs a little
# differently from the configuration (no trailing newline on values and manifests, pack type and
# registry fields filled in), and that provider's check counts it as a change. Stage 1 records the
# refusal as expected until the provider release that fixes it. Meanwhile the bump in stage 4
# targets the add-on profile, and the applies that only create or delete cluster B run with the
# flag off, which governs nothing but profiles, and keep skip_destroy on.
set -uo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
strata=${STRATA_BIN:-$repo/target/release/strata}
mode=${1:-}
from=${2:-1}
case "$mode" in
  off) flag=false ;;
  on) flag=true ;;
  *) sed -n '2,/^set -uo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
[ -x "$strata" ] || { echo "in-place: no strata binary at $strata" >&2; exit 2; }

# Expected exits that differ with the flag.
if [ "$mode" = on ]; then off0_on1=1; off1_on0=0; else off0_on1=0; off1_on0=1; fi

edit_from='edited: "no"'
edit_to='edited: "yes"'

run=${OUT:-$here/runs/$(date -u +%Y%m%dT%H%M%SZ)/in-place/$mode}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

out() { "$here/tf.sh" output -json | jq -r --arg k "$1" '.[$k].value // empty'; }
# apply [-var ...]: terraform apply with this run's flag; the output goes to apply-<n>.txt
# apply_without_flag [-var ...]: the same with the flag off and skip_destroy kept on
applies=$(ls apply-*.txt 2>/dev/null | wc -l)
terraform_apply() {
  local with=$1
  shift
  applies=$((applies + 1))
  echo "terraform apply -var immutable_profiles=$with $*" | tee -a steps.log
  "$here/tf.sh" apply -input=false -auto-approve -no-color -var "immutable_profiles=$with" "$@" \
    > "apply-$applies.txt" 2>&1
  local code=$?
  grep -E '^(Plan|Apply complete|Error)' "apply-$applies.txt" | tee -a steps.log
  [ "$code" = 0 ] || { echo "in-place: terraform apply failed; see $run/apply-$applies.txt" >&2; exit 2; }
}
apply() { terraform_apply "$flag" "$@"; }
apply_without_flag() { terraform_apply false -var keep_profile_versions=true "$@"; }

failures=0
rows=()
# check <id> <expected exit or ?> <what it shows> -- command...
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
  printf '%-4s  %-32s expected %s, got %s\n' "$verdict" "$id" "$want" "$got" | tee -a steps.log
}

# summary.md from steps.log, which every invocation of this run appends to, so a resumed run keeps
# the checks of its earlier stages; written on any exit.
summary() {
  {
    echo "# in-place edit test, flag $mode"
    echo
    echo "Run \`$(basename "$(dirname "$(dirname "$run")")")\` with \`$("$strata" --version)\`. Cluster A \`${C:-}\`,"
    echo "cluster B \`${B:-}\`, release \`${P:-}\`, add-on \`${AN:-}\` uids \`${AD1:-}\` and \`${AD2:-}\`."
    echo
    echo "| result | check | expected exit | got | what it shows |"
    echo "|---|---|---|---|---|"
    grep -E '^(PASS|FAIL|SEEN) ' steps.log | while read -r verdict id _ want _ got; do
      what=$(grep -oE "check $id [^ ]+ \"[^\"]*\"" "$here/in-place.sh" | head -1 | sed -E 's/^check [^ ]+ [^ ]+ "(.*)"$/\1/')
      echo "| $verdict | \`$id\` | ${want%,} | $got | $what |"
    done
  } > summary.md
}
trap summary EXIT

# ids.env keeps the uids across stages, so a run can resume.
save() { echo "$1=$2" >> ids.env; }
[ -f ids.env ] && . ./ids.env

stage() { [ "$from" -le "$1" ] && echo "== stage $1: $2" | tee -a steps.log; }

if [ "$from" -le 1 ]; then
  : > ids.env
  PALETTE_PROJECT_UID=$(out project_uid)
  C=$(out cluster_uid)
  N=$(out profile_name)
  P=$(out release_profile_uid)
  AN=$(out addon_profile_name)
  AD1=$(out addon_profile_uid)
  for v in PALETTE_PROJECT_UID C N P AN AD1; do
    [ -n "${!v}" ] || { echo "in-place: the terraform output behind $v is empty" >&2; exit 2; }
    save "$v" "${!v}"
  done
  [ -z "$(out cluster_b_uid)" ] || { echo "in-place: cluster B exists; run stage 5 of the last run" >&2; exit 2; }
fi
export PALETTE_PROJECT_UID

if stage 1 "record the baseline"; then
  check a1 0 "record cluster A" -- "$strata" snapshot "cluster:$C" --raw-dir raw/a1 -o a1.json
  check a1-addon 0 "record cluster A's add-on selection" -- "$strata" snapshot "cluster:$C/$AN" --raw-dir raw/a1-addon -o a1-addon.json
  check release1 0 "record the release version" -- "$strata" snapshot "$P" --raw-dir raw/release1 -o release1.json
  check addon1 0 "record the add-on version" -- "$strata" snapshot "$AD1" --raw-dir raw/addon1 -o addon1.json
  check addon1-is-1.0.0 0 "the add-on version is 1.0.0" -- jq -e '.source.version == "1.0.0"' addon1.json
  check a1-updates 0 "what waits for cluster A" -- "$here/tenant.sh" updates "$C"
  check a-owns-manifest-copy 0 "cluster A runs its own copy of the add-on manifest, not the profile's" -- \
    jq -e -n --slurpfile c a1-addon.json --slurpfile p addon1.json \
      '$c[0].packs[0].manifests[0].uid != $p[0].packs[0].manifests[0].uid'
  if [ "$mode" = on ]; then
    check plan-refuses-no-change 1 "expected with provider 0.29.7: with the flag on, Terraform refuses a plan that changes nothing" -- \
      "$here/tf.sh" plan -input=false -no-color -var immutable_profiles=true
  fi
fi

if stage 2 "edit the add-on version in place and create cluster B"; then
  if [ "$mode" = on ]; then
    check plan-refuses-edit 1 "#27: with the flag on, Terraform refuses an in-place edit" -- \
      "$here/tf.sh" plan -input=false -no-color -var immutable_profiles=true -var edit_addon_in_place=true
    check plan-names-the-flag 0 "the refusal names immutable-clusterprofiles" -- \
      grep -q 'immutable-clusterprofiles: cluster profile versions are immutable' plan-refuses-edit.err
    check api-edit 0 "the API still accepts the in-place edit" -- \
      "$here/api.sh" edit "$AD1" "$edit_from" "$edit_to"
    # The configuration now matches the edited version, so this only creates cluster B.
    apply_without_flag -var edit_addon_in_place=true -var second_cluster=true
  else
    apply -var edit_addon_in_place=true -var second_cluster=true
  fi
  B=$(out cluster_b_uid)
  save B "$B"
  check a2 0 "record cluster A again, applying nothing" -- "$strata" snapshot "cluster:$C" --raw-dir raw/a2 -o a2.json
  check a2-addon 0 "record cluster A's add-on selection" -- "$strata" snapshot "cluster:$C/$AN" --raw-dir raw/a2-addon -o a2-addon.json
  check b2-addon 0 "record cluster B's add-on selection" -- "$strata" snapshot "cluster:$B/$AN" --raw-dir raw/b2-addon -o b2-addon.json
  check addon2 0 "record the edited add-on version" -- "$strata" snapshot "$AD1" --raw-dir raw/addon2 -o addon2.json
  check edit-kept-manifest-uid 0 "the edit kept the add-on manifest's uid, so cluster copies still name it" -- \
    jq -e -n --slurpfile a addon1.json --slurpfile b addon2.json \
      '$a[0].packs[0].manifests[0].uid == $b[0].packs[0].manifests[0].uid'
  check a2-updates 0 "what waits for cluster A" -- "$here/tenant.sh" updates "$C"
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
  if ! grep -q '^PASS  apply-to-a ' steps.log; then
    check apply-keeping-notification 0 "push the add-on's content to cluster A without resolving its notification" -- \
      "$here/api.sh" apply "$C" --keep-notification
    sleep 180
    check a-changed-without-resolve 1 "#27: three minutes after that push, cluster A's copy has taken the edit" -- \
      "$strata" compare a2-addon.json "cluster:$C/$AN"
    check apply-to-a 0 "#27: accept the waiting update on cluster A, resolving the notification" -- \
      "$here/api.sh" apply "$C"
  fi
  for _ in $(seq 1 30); do
    "$strata" snapshot "cluster:$C/$AN" -o a3-addon.json 2>/dev/null \
      && "$strata" compare a3-addon.json b2-addon.json >/dev/null 2>&1 && break
    sleep 30
  done
  check a3 0 "record cluster A after the apply" -- "$strata" snapshot "cluster:$C" --raw-dir raw/a3 -o a3.json
  check a3-updates 0 "what waits for cluster A" -- "$here/tenant.sh" updates "$C"
  check a3-matches-b 0 "#27: after the apply, cluster A runs what cluster B runs" -- \
    "$strata" compare a3-addon.json b2-addon.json
  check verify-a1-after-apply 1 "#27: once the update is applied, cluster A's record from before the edit reports DRIFT" -- "$strata" verify a1.json
fi

if stage 4 "undo the edit and bump the add-on version"; then
  if [ "$mode" = on ]; then
    apply -var addon_version=1.0.1 -var second_cluster=true -var "cluster_addon_profile_uid=$AD1" \
      -target=spectrocloud_cluster_profile.addon
  else
    apply -var addon_version=1.0.1 -var second_cluster=true
  fi
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
  check a4-still-a3 0 "#27: the bump left cluster A as it was" -- "$strata" compare a3-addon.json a4-addon.json
  check consumers-old 1 "#27: both clusters are still consumers of the first add-on uid" -- "$strata" consumers "$AD1" --json
  if [ "$mode" = on ] && [ -n "${OFF_RUN:-}" ]; then
    check clone-carried-nothing ? "#27: the flag's clone equals the legacy bump of the same configuration" -- \
      "$strata" same-release "$OFF_RUN/addon-new4.json" addon-new4.json
  fi
fi

if stage 5 "delete cluster B and restore the add-on profile"; then
  # Palette does not carry out the deletion of a cluster that is still provisioning: it stays in
  # Deleting with every cloud resource in place. Wait for Running, up to an hour, before asking.
  if ! "$here/tenant.sh" cluster "$B" >/dev/null 2>&1; then  # exit 0: already deleted
    for _ in $(seq 1 60); do
      state=$("$here/tenant.sh" cluster "$B" 2>/dev/null)
      case "$state" in *": Running") break ;; esac
      sleep 60
    done
    case "$state" in
      *": Running") ;;
      *) echo "in-place: cluster B is not Running ($state); delete it by hand, then run stage 5 again" >&2; exit 2 ;;
    esac
  fi
  if [ "$mode" = on ]; then
    apply_without_flag -var addon_version=1.0.1 -var second_cluster=false -var "cluster_addon_profile_uid=$AD1"
  else
    apply -var addon_version=1.0.0 -var second_cluster=false
    check reapply-to-a 0 "apply the restored add-on profile to cluster A" -- "$here/api.sh" apply "$C"
    "$strata" snapshot "$AD1" -o addon5.json
    for _ in $(seq 1 30); do
      "$strata" compare addon5.json "cluster:$C/$AN" >/dev/null 2>&1 && break
      sleep 30
    done
    check a5-matches-addon 0 "cluster A runs the restored add-on version" -- \
      "$strata" compare addon5.json "cluster:$C/$AN"
  fi
  check b-gone 0 "cluster B is deleted" -- "$here/tenant.sh" cluster "$B"
fi

echo
echo "$failures failed; details in $run"
[ "$failures" -eq 0 ]
