#!/usr/bin/env bash
# The cluster test: two answers that only a cluster gives, and that no other run covers. It needs
# the live example at its baseline, with cluster A (just live-up). It ends with that cluster gone,
# and the example where just live-profiles leaves it.
#
#   just live-up && just live-cluster
#
#   1  the value the cluster gives the variable of the add-on profile changes: what the cluster
#      variables endpoint, the record of the cluster's copy, and the audit log show
#   2  the cluster is deleted through the API, not the provider: what the cluster listing,
#      consumers, and verify of a record from before show while it goes, and after it is gone
#   3  the OpenTofu state is brought in line with the tenant
#
# Needs tofu, jq, and curl with the Palette API key in PALETTE_API_KEY or ~/.palette.env. Results
# go under runs/<UTC time>/cluster/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=cluster
flag=false
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata; run just build"
for tool in jq curl; do
  command -v "$tool" > /dev/null || die "$tool is not installed"
done
run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/cluster}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1
load_outputs
C=$(out cluster_uid)
AD=$(out addon_profile_uid)
AN=$(out addon_profile_name)
PALETTE_PROJECT_UID=$(out project_uid)
export PALETTE_PROJECT_UID
for v in C AD AN PALETTE_PROJECT_UID; do
  [ -n "${!v}" ] || die "the tofu output behind $v is empty; run just live-up"
done
summary() {
  {
    echo "# cluster test"
    echo
    echo "Run \`$(basename "$(dirname "$run")")\` with \`$("$strata" --version)\`, cluster \`$C\`,"
    echo "add-on profile \`$AN\` \`$AD\`."
    echo
    summary_table
  } > summary.md
}
trap summary EXIT

# audits: the audit entries of the cluster
audits() { "$live/api.sh" get "/v1/audits?resourceKind=spectrocluster&resourceUid=$C&limit=100"; }
# audit_count FILE: how many entries an audits answer holds
audit_count() { jq '.items | length' "$1"; }
# newest_audit FILE: the newest entry of an audits answer
newest_audit() { jq '.items | sort_by(.metadata.creationTimestamp) | last' "$1"; }
# value_of FILE: the value the cluster variables endpoint gives the variable purpose
value_of() { jq -r '[.. | objects | select(.name? == "purpose")] | first | .value // ""' "$1"; }
# listing: the cluster listing strata reads for consumers. It answers an array at the top level,
# or an object with items; entries gives the array either way.
listing() { "$live/api.sh" get /v1/dashboard/spectroclusters/meta; }
entries() { jq --arg c "$C" '(if type == "array" then . else (.items // []) end) | map(select(.metadata.uid == $c))' "$1"; }
# in_listing FILE: exit 0 when the listing holds cluster A
in_listing() { [ "$(entries "$1" | jq length)" = 1 ]; }
not_in_listing() { ! in_listing "$1"; }
# state_in_listing FILE: the state the listing gives cluster A, or `absent`
state_in_listing() { entries "$1" | jq -r 'first | .status.state // "absent"'; }
# consumer_of FILE: what consumers --json says of cluster A
consumer_of() { jq --arg c "$C" '[.. | objects | select(.uid? == $c)]' "$1"; }
has_consumer() { [ "$(consumer_of "$1" | jq length)" != 0 ]; }
no_consumer() { ! has_consumer "$1"; }
# wait_gone: wait up to forty minutes until the cluster document is gone, or reports Deleted;
# print each state on the way
wait_gone() {
  local doc state last=""
  for _ in $(seq 1 80); do
    if doc=$("$live/api.sh" get "/v1/spectroclusters/$C" 2> /dev/null); then
      state=$(jq -r '.status.state // ""' <<<"$doc")
      [ "$state" = "$last" ] || { echo "$(date -u +%TZ) $state"; last=$state; }
      [ "$state" = Deleted ] && return 0
    else
      echo "$(date -u +%TZ) the document is gone"
      return 0
    fi
    sleep 30
  done
  return 1
}
state_has_no_cluster() { ! "$live/tf.sh" state list | command grep -qE "spectrocloud_cluster_(aws|gcp)"; }

# ---- 1. the value of the variable changes --------------------------------------------------------

echo "== 1. the value the cluster gives the add-on variable changes" | tee -a steps.log
check variables-before 0 "the cluster variables endpoint, before" -- "$live/api.sh" get "/v1/spectroclusters/$C/variables"
echo "value before: '$(value_of variables-before.out)'" | tee -a steps.log
check copy-before 0 "a record of the copy of the add-on profile that the cluster holds" -- \
  "$strata" snapshot "cluster:$C/$AN" -o copy-before.json
check audits-before 0 "the audit entries of the cluster, before" -- audits
apply -var addon_purpose=changed
wait_running "$C"
check variables-after 0 "the cluster variables endpoint, after" -- "$live/api.sh" get "/v1/spectroclusters/$C/variables"
check value-changed 0 "#66: the endpoint gives the value the cluster gives now" -- test "$(value_of variables-after.out)" = changed
check copy-after 0 "a record of the copy, after" -- "$strata" snapshot "cluster:$C/$AN" -o copy-after.json
check copy-verify 0 "#66: the record from before verifies: the value is the cluster's, and no part of the copy" -- \
  "$strata" verify copy-before.json
check copy-compare 0 "the two records of the copy compare IDENTICAL" -- "$strata" compare copy-before.json copy-after.json
check copy-source-value 0 "#66: the record from after carries the value under source.cluster.variables, outside the root" -- \
  jq -e '[.source.cluster.variables[] | select(.name == "purpose") | .value] == ["changed"]' copy-after.json
check audits-after 0 "the audit entries of the cluster, after" -- audits
check audit-records-change 0 "#66: the audit log has a new entry for the cluster" -- \
  test "$(audit_count audits-after.out)" -gt "$(audit_count audits-before.out)"
# audit_says: the newest entry, and exit 0 when it says who updated the cluster variables
audit_says() {
  newest_audit audits-after.out
  newest_audit audits-after.out | jq -e '.spec.actionMsg | test("updated the cluster variables on the cluster")' > /dev/null
}
check audit-says 0 "#66: the newest entry names who updated the cluster variables on the cluster" -- audit_says

# ---- 2. the cluster is deleted through the API --------------------------------------------------

echo "== 2. the cluster is deleted through the API" | tee -a steps.log
check listing-before 0 "the cluster listing, before" -- listing
check listing-has-cluster 0 "the listing holds the cluster" -- in_listing listing-before.out
check consumers-before 1 "consumers, before: exit 1, the version has a consumer" -- "$strata" consumers "$AD" --json
check consumers-has-cluster 0 "consumers lists the cluster under the add-on version" -- has_consumer consumers-before.out
check record-before-delete 0 "a record of the cluster, before the deletion" -- \
  "$strata" snapshot "cluster:$C/$AN" -o record-before-delete.json
check delete 0 "#66: DELETE /v1/spectroclusters/<uid>" -- "$live/api.sh" delete-cluster "$C"
# the listing while the cluster goes: the first read after the request that no longer says Running
for _ in $(seq 1 12); do
  listing > listing-deleting.out 2> listing-deleting.err || die "the listing failed after the request"
  [ "$(state_in_listing listing-deleting.out)" != Running ] && break
  sleep 10
done
echo "state in the listing after the request: $(state_in_listing listing-deleting.out)" | tee -a steps.log
check listing-deleting 0 "#66: the listing keeps the cluster while it goes, with the state Deleting" -- \
  test "$(state_in_listing listing-deleting.out)" = Deleting
check consumers-deleting 1 "consumers, while the cluster goes: exit 1, the version still has a consumer" -- \
  "$strata" consumers "$AD" --json
check consumers-shows-deleting 0 "#66: consumers lists the cluster with the state Deleting" -- \
  jq -e --arg c "$C" '.consumers | map(select(.uid == $c)) | first | .state == "Deleting"' consumers-deleting.out
wait_gone | tee -a steps.log || die "cluster $C is not gone after forty minutes"
check doc-after 0 "GET /v1/spectroclusters/<uid> still answers after the deletion" -- "$live/api.sh" get "/v1/spectroclusters/$C"
check doc-after-deleted 0 "#66: the document reports the state Deleted" -- jq -e '.status.state == "Deleted"' doc-after.out
check verify-deleted 1 "#139: verify of the record from before the deletion: DRIFT, exit 1" -- \
  "$strata" verify record-before-delete.json
check verify-deleted-says 0 "#139: the verdict says that the cluster no longer exists, with the time of the delete" -- \
  command grep -E "^DRIFT: cluster .* no longer exists: deleted [0-9]{4}-" verify-deleted.out
check listing-after 0 "the cluster listing, after" -- listing
check listing-drops-cluster 0 "#66: the listing no longer holds the cluster" -- not_in_listing listing-after.out
check consumers-after 0 "consumers, after: exit 0, the version has no consumer" -- "$strata" consumers "$AD" --json
check consumers-drops-cluster 0 "#66: consumers no longer lists the cluster" -- no_consumer consumers-after.out

# ---- 3. the state is brought in line ----------------------------------------------------------------

echo "== 3. the OpenTofu state is brought in line" | tee -a steps.log
tf_apply false -var cluster=false
check state-has-no-cluster 0 "the OpenTofu state holds no cluster: the refresh saw the deletion" -- state_has_no_cluster

echo
echo "$script: $(command grep -c PASS steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]
