#!/usr/bin/env bash
# The order test: what a reorder is to Palette, and what strata says of it. Each stage needs the
# live example at its baseline with cluster A (just live-up). Stages 1 to 3 edit two published
# versions in place and leave them so: run just live-down after them.
#
#   just live-up && just live-order          # stages 1 to 3
#   just live-up && just live-order-append   # stage 4
#
#   examples/live/order.sh [4]
#
#   1  the registries version, two add-on packs that nothing runs: a PUT with the packs in the
#      other order, then the document, the audit log, and verify
#   2  the release version, four infrastructure packs that cluster A runs: the same PUT, and
#      whether Palette accepts another order of those layers
#   3  cluster A: a PUT of its profiles in the other order, then the document, the audit log,
#      and verify on the cluster record, which orders the profiles itself
#   4  the stored order of the profiles of cluster A after an add and after a new version of its
#      infrastructure profile (#844), with cluster B as the control. The stage makes 1.0.2, a
#      release version with a new uid and the content of 1.0.0. It adds an add-on profile of one
#      ConfigMap to cluster A, with a PUT that gives that profile first. Then it sends the PATCH
#      of an upgrade from 1.0.0 to 1.0.2. After each step, it reads spec.clusterProfileTemplates
#      of each cluster, and spec.clusterProfileTemplate of the summary of each cluster.
#
# Stage 4 runs only when you give its number. It builds cluster B from the same definition, and
# changes nothing on it. At its end, it puts cluster A back, deletes the profile, and removes
# cluster B and 1.0.2. When it stops early, run just live-down. It is billable: it keeps cluster
# B until its end.
#
# Needs tofu, jq, and curl with the Palette API key in PALETTE_API_KEY or ~/.palette.env. Results
# go under runs/<UTC time>/order/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=order
flag=false
from=${1:-1}
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
case "$from" in 1|4) ;; *) usage; exit 2 ;; esac
[ -x "$strata" ] || die "no strata binary at $strata; run just build"
for tool in jq curl; do
  command -v "$tool" > /dev/null || die "$tool is not installed"
done
run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/order}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1
load_outputs
C=$(out cluster_uid)
P=$(out release_profile_uid)
R=$(out registries_profile_uid)
PALETTE_PROJECT_UID=$(out project_uid)
export PALETTE_PROJECT_UID
for v in C P R PALETTE_PROJECT_UID; do
  [ -n "${!v}" ] || die "the tofu output behind $v is empty; run just live-up"
done
summary() {
  local stage4=""
  [ -z "${B:-}" ] || stage4=" Stage 4: cluster B \`$B\`, release 1.0.2 \`${P2:-}\`, added profile \`${X:-}\`."
  write_summary "order test" ", cluster \`$C\`," \
    "release version \`$P\`, registries version \`$R\`.$stage4"
}
trap summary EXIT

# audits UID: the audit entries of one object
audits() { "$live/api.sh" get "/v1/audits?resourceUid=$1&limit=100"; }
audit_count() { jq '.items | length' "$1"; }
# grew BEFORE AFTER: the audit log has more entries than before
grew() { [ "$(audit_count "$2")" -gt "$(audit_count "$1")" ]; }
# pack_names FILE: the pack names of a profile document, in its order
pack_names() { jq -c '[.spec.published.packs[].name]' "$1"; }
# record_pack_names RECORD: the pack names of a record, in its order
record_pack_names() { jq -c '[.packs[].name]' "$1"; }
# reversed A B: the two JSON arrays are each other reversed, and not equal
reversed() { [ "$1" != "$2" ] && [ "$(jq -c 'reverse' <<<"$1")" = "$2" ]; }
# profile_uids FILE: the profile uids of a cluster document, in its order
profile_uids() { jq -c '[.spec.clusterProfileTemplates[].uid]' "$1"; }
same() { [ "$1" = "$2" ]; }

if [ "$from" = 1 ]; then
  check release-before 0 "a record of the release version" -- "$strata" snapshot "$P" -o release.json
  check registries-before 0 "a record of the registries version" -- "$strata" snapshot "$R" -o registries.json
  check cluster-before 0 "a record of cluster A, its profiles merged in the order strata gives them" -- \
    "$strata" snapshot "cluster:$C" -o cluster.json
  check registries-doc-before 0 "the registries document" -- "$live/api.sh" get "/v1/clusterprofiles/$R"
  check release-doc-before 0 "the release document" -- "$live/api.sh" get "/v1/clusterprofiles/$P"
  check cluster-doc-before 0 "the cluster document" -- "$live/api.sh" get "/v1/spectroclusters/$C"
  check registries-audits-before 0 "the audit entries of the registries version" -- audits "$R"
  check release-audits-before 0 "the audit entries of the release version" -- audits "$P"
  check cluster-audits-before 0 "the audit entries of the cluster" -- audits "$C"
fi

# ---- 1. the registries version -------------------------------------------------------------------

if stage 1 "the packs of the registries version, in the other order"; then
  check reorder-addon 0 "#66: a PUT of the version with its packs in the other order is accepted" -- \
    "$live/api.sh" reorder "$R"
  check registries-doc-after 0 "the registries document, after" -- "$live/api.sh" get "/v1/clusterprofiles/$R"
  check addon-doc-reversed 0 "#66: Palette stores the new order" -- \
    reversed "$(pack_names registries-doc-before.out)" "$(pack_names registries-doc-after.out)"
  check registries-audits-after 0 "the audit entries, after" -- audits "$R"
  check addon-audited 0 "#66: the audit log has a new entry for the version" -- \
    grew registries-audits-before.out registries-audits-after.out
  check addon-verify 1 "#66: the record from before reports DRIFT" -- "$strata" verify registries.json
  check addon-verify-says 0 "the report names the order, and every pack is unchanged" -- \
    command grep -q "note: pack order changed between the two profiles" addon-verify.out
  check registries-after 0 "a record of the version, after" -- "$strata" snapshot "$R" -o registries-after.json
  check addon-record-reversed 0 "the new record lists the packs in the new order" -- \
    reversed "$(record_pack_names registries.json)" "$(record_pack_names registries-after.json)"
fi

# ---- 2. the release version -----------------------------------------------------------------------

if stage 2 "the packs of the release version, in the other order"; then
  check reorder-release 0 "#66: Palette accepts the four infrastructure layers in another order, on a version a cluster runs" -- \
    "$live/api.sh" reorder "$P"
  check release-doc-after 0 "the release document, after" -- "$live/api.sh" get "/v1/clusterprofiles/$P"
  echo "release packs before: $(pack_names release-doc-before.out), after: $(pack_names release-doc-after.out)" | tee -a steps.log
  check release-doc-reversed 0 "#66: Palette stores the new order: the CSI layer first, the OS layer last" -- \
    reversed "$(pack_names release-doc-before.out)" "$(pack_names release-doc-after.out)"
  check release-audits-after 0 "the audit entries, after" -- audits "$P"
  check release-audited 0 "#66: the audit log has a new entry for the version" -- \
    grew release-audits-before.out release-audits-after.out
  check release-verify 1 "#66: the record from before reports DRIFT" -- "$strata" verify release.json
  check release-verify-says 0 "the report names the order, and every pack is unchanged" -- \
    command grep -q "note: pack order changed between the two profiles" release-verify.out
fi

# ---- 3. the profiles of cluster A -----------------------------------------------------------------

if stage 3 "the profiles of cluster A, in the other order"; then
  check reorder-cluster 0 "a PUT of the cluster's profiles in the other order is accepted" -- \
    "$live/api.sh" reorder-cluster "$C"
  check cluster-doc-after 0 "the cluster document, after" -- "$live/api.sh" get "/v1/spectroclusters/$C"
  echo "cluster profiles before: $(profile_uids cluster-doc-before.out), after: $(profile_uids cluster-doc-after.out)" | tee -a steps.log
  check cluster-order-kept 0 "#66: Palette keeps the stored order of a cluster's profiles: a PUT that only reorders them changes nothing" -- \
    same "$(profile_uids cluster-doc-before.out)" "$(profile_uids cluster-doc-after.out)"
  check cluster-audits-after 0 "the audit entries, after" -- audits "$C"
  check cluster-audited 0 "the audit log has an entry for the PUT all the same: an update of the cluster" -- \
    grew cluster-audits-before.out cluster-audits-after.out
  check cluster-verify 0 "#66: the cluster record verifies: strata orders the profiles itself" -- \
    "$strata" verify cluster.json
fi

# ---- 4. an add and a new version of the infrastructure profile ------------------------------------

# the release version that stage 4 makes, and the name of the profile that it adds
upgrade=1.0.2
added=strata-live-order
# orders STEP: the document and the summary of cluster A and of cluster B, after the step STEP
orders() {
  check "order-a-$1" 0 "the document of cluster A, $1" -- "$live/api.sh" get "/v1/spectroclusters/$C"
  check "summary-a-$1" 0 "the summary of cluster A, $1" -- "$live/api.sh" get "/v1/dashboard/spectroclusters/$C"
  check "order-b-$1" 0 "the document of cluster B, $1" -- "$live/api.sh" get "/v1/spectroclusters/$B"
  check "summary-b-$1" 0 "the summary of cluster B, $1" -- "$live/api.sh" get "/v1/dashboard/spectroclusters/$B"
  echo "$1: cluster A $(profile_uids "order-a-$1.out"), cluster B $(profile_uids "order-b-$1.out")" | tee -a steps.log
}
# added_last BEFORE AFTER UID: AFTER is BEFORE with UID at the end
added_last() { same "$(jq -c --arg u "$3" '. + [$u]' <<<"$1")" "$2"; }
# replaced_last BEFORE AFTER OLD NEW: AFTER is BEFORE without OLD, with NEW at the end
replaced_last() { same "$(jq -c --arg o "$3" --arg n "$4" 'map(select(. != $o)) + [$n]' <<<"$1")" "$2"; }
# template FILE: the uid of spec.clusterProfileTemplate of the summary FILE
template() { jq -r '.spec.clusterProfileTemplate.uid // "none"' "$1"; }
# summary_first STEP...: in each read of each step, the summary of each cluster gives the profile at
# position 0 of the document as spec.clusterProfileTemplate
summary_first() {
  local step c first bad=0
  for step in "$@"; do
    for c in a b; do
      first=$(jq -r '.spec.clusterProfileTemplates[0].uid // "none"' "order-$c-$step.out")
      echo "cluster $c, $step: position 0 $first, clusterProfileTemplate $(template "summary-$c-$step.out")"
      same "$first" "$(template "summary-$c-$step.out")" || bad=1
    done
  done
  return "$bad"
}
# control_kept: cluster B gives the order and the clusterProfileTemplate of its first read in each read
control_kept() {
  local step bad=0
  for step in added upgraded; do
    same "$(profile_uids order-b-before.out)" "$(profile_uids "order-b-$step.out")" || bad=1
    same "$(template summary-b-before.out)" "$(template "summary-b-$step.out")" || bad=1
  done
  echo "cluster B: $(profile_uids order-b-before.out), clusterProfileTemplate $(template summary-b-before.out)"
  return "$bad"
}

if [ "$from" = 4 ] && stage 4 "an add and a new version of the infrastructure profile of cluster A, with cluster B as the control"; then
  require_baseline
  hold_versions
  check append-release 0 "the release version 1.0.0, for the tag of its OS pack" -- \
    "$live/api.sh" get "/v1/clusterprofiles/$P"
  os_tag=$(jq -r --arg os "$(out os_pack_name)" '.spec.published.packs[] | select(.name == $os) | .tag' append-release.out)
  [ -n "$os_tag" ] || die "the release version has no OS pack $(out os_pack_name); see $run/append-release.out"
  # 1.0.2 pins the OS pack under the tag of 1.0.0, so it has the content of 1.0.0 and a new uid
  apply -var second_cluster=true -var "tag_only_versions={\"$upgrade\"=\"$os_tag\"}"
  B=$(out cluster_b_uid)
  P2=$(out tagged_profile_uids | jq -r --arg v "$upgrade" '.[$v] // empty')
  [ -n "$B" ] && [ -n "$P2" ] || die "the apply gave no cluster B or no $upgrade"
  wait_running "$B"
  orders before
  cat > added.yaml <<'EOF'
apiVersion: v1
kind: ConfigMap
metadata:
  name: strata-live-order
  namespace: default
data:
  purpose: the profile that order.sh adds to cluster A
EOF
  "$live/api.sh" delete-named "$added" > /dev/null || die "cannot delete the profile $added of an earlier run"
  check append-profile 0 "#844: an add-on profile of one ConfigMap, the profile of the add" -- \
    "$live/api.sh" manifest-profile "$added" "$added" added.yaml
  X=$(cat append-profile.out)
  [ -n "$X" ] || die "no profile to add; see $run/append-profile.err"
  check append-add 0 "#844: Palette takes a PUT of the profiles of cluster A that gives the new profile first" -- \
    "$live/api.sh" attach "$C" "$X" --first
  wait_running "$C"
  orders added
  check added-last 0 "#844: the stored order of cluster A keeps the order from before, and puts the new profile last" -- \
    added_last "$(profile_uids order-a-before.out)" "$(profile_uids order-a-added.out)" "$X"
  check append-upgrade 0 "#844: Palette takes the PATCH of an upgrade of cluster A from 1.0.0 to 1.0.2" -- \
    "$live/api.sh" replace "$C" "$P" "$P2"
  wait_running "$C"
  orders upgraded
  check upgraded-last 0 "#844: the stored order drops 1.0.0, keeps the other profiles in their order, and puts 1.0.2 last" -- \
    replaced_last "$(profile_uids order-a-added.out)" "$(profile_uids order-a-upgraded.out)" "$P" "$P2"
  check summary-first 0 "#844: in each read, the summary gives the profile at position 0 as clusterProfileTemplate" -- \
    summary_first before added upgraded
  check summary-addon 0 "#844: after the upgrade, clusterProfileTemplate of cluster A is an add-on profile" -- \
    jq -e '.spec.clusterProfileTemplate.type == "add-on"' summary-a-upgraded.out
  check control-kept 0 "#844: cluster B, with no change, gives one order and one clusterProfileTemplate in each read" -- \
    control_kept
  check append-back 0 "cluster A gets 1.0.0 again" -- "$live/api.sh" replace "$C" "$P2" "$P"
  wait_running "$C"
  check append-detach 0 "cluster A loses the added profile" -- "$live/api.sh" detach "$C" "$X"
  wait_running "$C"
  check append-delete 0 "the added profile is deleted" -- "$live/api.sh" delete "$X"
  apply_baseline
  check append-baseline 0 "the live example is at its baseline again: no cluster B, and no 1.0.2" -- \
    "$live/baseline.sh"
fi

echo
echo "$script: $(command grep -c PASS steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]
