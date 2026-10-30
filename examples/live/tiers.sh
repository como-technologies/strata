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
#   5  a role binding on cluster A, through the API, then its subjects edited: compare of a record
#      from before with cluster A names the policy tier and the binding, with exit 0, and verify
#      of the record reports the tier (#106); then the bindings of cluster A as before. It needs
#      no cluster B: `tiers.sh 5` runs it alone
#   6  a second key, from STRATA_LIVE_LIMITED_KEY, of an active user with no role in the project: it
#      cannot read the role bindings of cluster A. Palette answers HTTP 500 with the code
#      ResourceAccessDenied, and not HTTP 403. The stage records the status and the code, and what
#      strata makes of the answer (#276). It writes nothing. `tiers.sh 6` runs it alone, and no
#      other FROM_STAGE runs it. With no variable set, it says "could not check" and exits 2
#
# Needs the live example at its baseline with cluster A (just live-up), the strata release binary,
# jq, and the key as verify.sh needs it. Billable: cluster B for about 30 minutes; stages 5 and 6
# add no machine. Results go under
# runs/<UTC time>/tiers/, or OUT; FROM_STAGE resumes a run in OUT. Exit 0 only when no check fails.
set -uo pipefail
script=tiers
from=${1:-1}
case "$from" in 1|2|3|4|5|6) ;; *) sed -n '2,/^set -uo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 2 ;; esac
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

# rbacs: the role bindings of cluster A, as Palette gives them. A 404 is a cluster with none.
rbacs() {
  local err
  err=$(mktemp)
  if "$live/api.sh" get "/v1/spectroclusters/$C/config/rbacs" 2> "$err"; then
    rm -f "$err"
  elif grep -q 'HTTP 404' "$err"; then
    rm -f "$err"
    echo '{"items": []}'
  else
    cat "$err" >&2
    rm -f "$err"
    return 2
  fi
}
# put_rbacs [SUBJECT]: PUT the role bindings of cluster A as they were before stage 5, and, with
# SUBJECT, one more binding of the user SUBJECT to the cluster role view. The PUT replaces each
# binding of the cluster.
put_rbacs() {
  local body
  body=$(jq -c --arg s "${1:-}" '{rbacs: ([.items[]? | {spec: {bindings: .spec.bindings}}]
    + if $s == "" then [] else [{spec: {bindings: [{type: "ClusterRoleBinding",
      role: {kind: "ClusterRole", name: "view"}, subjects: [{type: "User", name: $s}]}]}}] end)}' rbac-before.out) \
    || return 2
  "$live/api.sh" put "/v1/spectroclusters/$C/config/rbacs" "$body"
}

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
  check a-arch 0 "#105: shape names the cloud type, and the architecture of each machine pool" -- \
    jq -e '.source.cluster.tiers.shape.fields | (.cloudType | length > 0)
      and all(.pools[]; .machinePoolProperties.archType | IN("amd64", "arm64"))' a.json
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

if stage 5 "the subjects of a role binding of cluster A"; then
  [ "$from" = 5 ] && require_baseline
  check rbac-before 0 "#106: the role bindings of cluster A before the stage" -- rbacs
  check rbac-add 0 "#106: cluster A gets a binding of barney@example.com to the cluster role view, through the API" -- \
    put_rbacs barney@example.com
  check rbac-record 0 "#106: a record of cluster A with the binding" -- "$strata" snapshot "cluster:$C" -o rbac.json
  check rbac-in-record 0 "#106: the policy tier holds the binding, with its role and its subject" -- \
    jq -e '.source.cluster.tiers.policy.fields.rbacs | any(.[]; .role.name == "view"
      and any(.subjects[]; .name == "barney@example.com" and .type == "User"))' rbac.json
  check rbac-edit 0 "#106: the subject of the binding becomes fred@example.com, through the API" -- \
    put_rbacs fred@example.com
  check rbac-compare 0 "#106: compare of the record with cluster A: one release, exit 0" -- \
    "$strata" compare rbac.json "cluster:$C"
  check rbac-compare-json 0 "the same comparison as JSON" -- "$strata" compare rbac.json "cluster:$C" --json
  check rbac-compare-policy 0 "#106: the policy tier differs, at the subjects of the binding" -- \
    jq -e '.clusterTiers[] | select(.tier == "policy") | .same == false
      and any(.diff[].path; startswith("rbacs") and contains("subjects"))' rbac-compare-json.out
  check rbac-compare-says 0 "#106: the report names the new subject of the binding" -- \
    grep -Eq 'policy\.rbacs\[[0-9]+\]\.subjects\[name=fred@example\.com\]' rbac-compare.out
  check rbac-verify 0 "#106: verify of the record: the verdict follows the release, exit 0" -- "$strata" verify rbac.json
  check rbac-verify-says 0 "#106: and it reports the policy tier" -- grep -q 'policy CHANGED' rbac-verify.out
  check rbac-undo 0 "the role bindings of cluster A as before the stage" -- put_rbacs
  check rbac-after-read 0 "the role bindings of cluster A after the stage" -- rbacs
  check rbac-after 0 "the bindings that Palette gives now are the bindings from before" -- \
    bash -c "diff <(jq -S '[.items[]?.spec.bindings]' rbac-before.out) <(jq -S '[.items[]?.spec.bindings]' rbac-after-read.out)"
fi

# limited_read: read the role bindings of cluster A with the second key. api.sh exits 2 and prints
# the status and the body of the refusal; the key never leaves the environment of that one call.
limited_read() { PALETTE_API_KEY=$STRATA_LIVE_LIMITED_KEY "$live/api.sh" get "/v1/spectroclusters/$C/config/rbacs"; }
# refusal_code: the error code in the body of the refusal that limited-read.err holds
refusal_code() { sed -n 's/^api.sh: GET [^ ]* answered HTTP [0-9]*: //p' limited-read.err | jq -r '.code // empty'; }
# limited_record: a record of cluster A that strata reads with the second key
limited_record() { PALETTE_API_KEY=$STRATA_LIVE_LIMITED_KEY "$strata" snapshot "cluster:$C" -o limited-record.json; }

# stage 6 runs only when its number is given: it needs a second key
if [ "$from" = 6 ] && stage 6 "a key that cannot read the role bindings of cluster A"; then
  [ -n "${STRATA_LIVE_LIMITED_KEY:-}" ] \
    || die "could not check: set STRATA_LIVE_LIMITED_KEY to a key that cannot read config/rbacs of cluster A"
  check limited-control 0 "#276: the key of the example reads the role bindings of cluster A" -- rbacs
  check limited-read 2 "#276: the second key does not read them: api.sh exits 2 for the refusal of Palette" -- limited_read
  check limited-status 0 "#276: the status of the refusal is HTTP 500, and not 403" -- grep -q 'answered HTTP 500: ' limited-read.err
  check limited-code ? "#276: the error code of the body that Palette answers" -- refusal_code
  check limited-code-says 0 "#276: the code is ResourceAccessDenied" -- grep -qx ResourceAccessDenied limited-code.out
  check limited-message 0 "#276: the message names the project: Project '<uid>' access denied" -- \
    grep -q "Project '$PALETTE_PROJECT_UID' access denied" limited-read.err
  check limited-record ? "#276: what strata does with the answer: a record of cluster A with the second key" -- limited_record
  check limited-record-errors ? "#276: the errors of the policy tier of that record, if strata wrote one" -- \
    jq '.source.cluster.tiers.policy.errors' limited-record.json
fi

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]
