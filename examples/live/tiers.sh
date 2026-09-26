#!/usr/bin/env bash
# The cluster tiers test (#76): what a cluster record keeps of a real cluster beside its release,
# and what compare of two cluster records says.
#
#   examples/live/tiers.sh [FROM_STAGE]
#
#   1  record cluster A with --shape: the tiers that it carries, the labels, and no pack whose
#      reconciled version differs from its profile
#   2  cluster B from the same definition: the two records have one release, and the run records
#      which tiers are the same
#   3  another timezone and one more label on B, through the API: compare names the tier and the
#      field, and a record of B from before still verifies
#   4  B removed, and the baseline
#
# Needs the live example at its baseline with cluster A (just live-up), the strata release binary,
# jq, and the key as verify.sh needs it. Billable: cluster B for about 30 minutes. Results go under
# runs/<UTC time>/tiers/, or OUT; FROM_STAGE resumes a run in OUT. Exit 0 only when no check fails.
set -uo pipefail
script=tiers
from=${1:-1}
case "$from" in 1|2|3|4) ;; *) sed -n '2,/^set -uo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 2 ;; esac
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/tiers}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

summary() {
  {
    echo "# cluster tiers test"
    echo
    echo "Run \`$(basename "$(dirname "$run")")\` with \`$("$strata" --version)\`, cluster A \`${C:-}\`,"
    echo "cluster B \`${B:-}\`."
    echo
    summary_table
  } > summary.md
}
trap summary EXIT

stage() { [ "$from" -le "$1" ] && echo "== stage $1: $2" | tee -a steps.log; }

load_outputs
C=$(out cluster_uid) R=$(out release_profile_uid)
release=$(out profile_name)
[ -n "$C" ] || die "no cluster A; run just live-up"
export PALETTE_PROJECT_UID=$(out project_uid)

# tier_state FILE TIER: the state of one tier in a compare --json, same or changed
tier_state() { jq -r --arg t "$2" '.clusterTiers[] | select(.tier == $t) | if .same then "same" else "changed" end' "$1"; }

if stage 1 "cluster A"; then
  require_baseline
  check a-record 0 "#76: a record of cluster A, with the shape tier" -- \
    "$strata" snapshot "cluster:$C" --shape --raw-dir raw/a -o a.json
  check a-tiers 0 "#76: the record carries applied, policy, running, and shape, and the labels of the cluster" -- \
    jq -e '.source.cluster | (.tiers | keys) == ["applied", "policy", "running", "shape"] and (.labels | length > 0)' a.json
  check a-applied 0 "#76: applied holds the two hashes of Palette" -- \
    jq -e '.source.cluster.tiers.applied.fields | (.spcHash | length > 0) and (.spcInfraHash | length > 0)' a.json
  check a-running 0 "#76: running holds one entry for each pack of the record" -- \
    jq -e '(.source.cluster.tiers.running.fields | length) == (.packs | length)' a.json
  check a-shape 0 "#76: shape holds two machine pools and the region, and no subnet id" -- \
    jq -e --arg region "$(out region)" '.source.cluster.tiers.shape.fields | (.pools | length) == 2 and .cloud.region == $region
      and ([.. | objects | has("subnetIds")] | any | not)' a.json
  check a-behind 0 "#76: no pack of cluster A runs another version than its profile declares" -- \
    bash -c "\"$strata\" compare cluster:$C/$release $R --json | jq -e '(.runningBehind // []) == []'"
fi

if stage 2 "cluster B from the same definition"; then
  apply -var second_cluster=true
  B=$(out cluster_b_uid)
  [ -n "$B" ] || die "no cluster B after the apply"
  wait_running "$B"
  echo "B=$B" > ids.env
  check a-record-again 0 "a record of cluster A now" -- "$strata" snapshot "cluster:$C" --shape -o a.json
  check b-record 0 "#76: a record of cluster B, with the shape tier" -- \
    "$strata" snapshot "cluster:$B" --shape --raw-dir raw/b -o b-first.json
  check same-release 0 "#76: compare of the two records: one release" -- "$strata" compare a.json b-first.json
  check same-release-json 0 "the same comparison as JSON" -- "$strata" compare a.json b-first.json --json
  for t in applied policy running shape labels; do
    check "same-$t" 0 "#76: the $t tier of the two clusters is the same; applied reads spcInfraHash alone" -- \
      test "$(tier_state same-release-json.out "$t")" = same
  done
fi

if stage 3 "another timezone and one more label on cluster B"; then
  [ -n "${B:-}" ] || . ./ids.env
  labels=$(jq -c '.source.cluster.labels + {"team": "search"}' b-first.json)
  check b-timezone 0 "cluster B gets the timezone America/Chicago, through the API" -- \
    "$live/api.sh" patch "/v1/spectroclusters/$B/clusterConfig/timezone" '{"timezone": "America/Chicago"}'
  check b-labels 0 "cluster B gets the label team=search, through the API" -- \
    "$live/api.sh" patch "/v1/spectroclusters/$B/metadata" "{\"metadata\": {\"labels\": $labels}}"
  check b-record-again 0 "a record of cluster B after the change" -- \
    "$strata" snapshot "cluster:$B" --shape -o b.json
  check differ 0 "#76: compare of A and B: still one release, exit 0" -- "$strata" compare a.json b.json
  check differ-json 0 "the same comparison as JSON" -- "$strata" compare a.json b.json --json
  check differ-policy 0 "#76: the policy tier differs, at timezone" -- \
    jq -e '.clusterTiers[] | select(.tier == "policy") | .same == false and ([.diff[].path] | index("timezone"))' differ-json.out
  check differ-labels 0 "#76: the labels differ, at team" -- \
    jq -e '.clusterTiers[] | select(.tier == "labels") | .same == false and ([.diff[].path] == ["team"])' differ-json.out
  check differ-says 0 "#76: the report names the fields" -- \
    grep -q '~ policy.timezone: "Etc/UTC" -> "America/Chicago"' differ.out
  check b-verify 0 "#76: the record of B from before still verifies: the verdict follows the release" -- \
    "$strata" verify b-first.json
  check b-verify-says 0 "#76: and it names the changed tier" -- grep -q 'policy CHANGED' b-verify.out
fi

if stage 4 "cluster B removed"; then
  [ -n "${B:-}" ] || . ./ids.env
  apply
  "$live/tenant.sh" cluster "$B" > /dev/null 2>&1 || true
  check undo-baseline 0 "the example is at its baseline" -- "$live/baseline.sh"
fi

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]
