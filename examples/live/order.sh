#!/usr/bin/env bash
# The order test: what a reorder is to Palette, and what strata says of it. It needs the live
# example at its baseline with cluster A (just live-up). It edits two published versions in
# place and leaves them so: run just live-down after it.
#
#   just live-up && just live-order
#
#   1  the registries version, two add-on packs that nothing runs: a PUT with the packs in the
#      other order, then the document, the audit log, and verify
#   2  the release version, four infrastructure packs that cluster A runs: the same PUT, and
#      whether Palette accepts another order of those layers
#   3  cluster A: a PUT of its profiles in the other order, then the document, the audit log,
#      and verify on the cluster record, which orders the profiles itself
#
# Needs tofu, jq, and curl with the Palette API key in PALETTE_API_KEY or ~/.palette.env. Results
# go under runs/<UTC time>/order/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=order
flag=false
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
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
  write_summary "order test" ", cluster \`$C\`," \
    "release version \`$P\`, registries version \`$R\`."
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

# ---- 1. the registries version -------------------------------------------------------------------

echo "== 1. the packs of the registries version, in the other order" | tee -a steps.log
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

# ---- 2. the release version -----------------------------------------------------------------------

echo "== 2. the packs of the release version, in the other order" | tee -a steps.log
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

# ---- 3. the profiles of cluster A -----------------------------------------------------------------

echo "== 3. the profiles of cluster A, in the other order" | tee -a steps.log
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

echo
echo "$script: $(command grep -c PASS steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]
