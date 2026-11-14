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
#   7  an override on a machine pool of cluster A, through the API (the item of #515): both
#      overrides, overrideClusterAPIConfig and overrideHealthCheckConfiguration, apply to the worker
#      pool, as a YAML comment that sets no value. Palette holds them in the pool of the cloud
#      config; a new record keeps them in the shape tier; compare of a record from before names
#      them, with exit 0. Then the pool as before. `tiers.sh 7` runs it alone, and no other
#      FROM_STAGE runs it
#   8  a third key, from STRATA_LIVE_NARROW_KEY. Its user has a role in the project that does not
#      give cluster.get, such as Cluster Profile Viewer. It cannot read the role bindings, the
#      namespaces, or the repave of cluster A. The stage records the status and the code of each
#      answer. The code of Palette shows HTTP 500 with the code OperationForbidden (#665). It
#      writes nothing. Then `strata list` with that key: the code of Palette shows HTTP 500 with the
#      code ResourceAccessDenied for the clusters, so the listing names them as not read, keeps the
#      profiles, and exits 2, with no retry (#620). `tiers.sh 8` runs it alone, and no other
#      FROM_STAGE runs it. With no variable set, it says "could not check" and exits 2
#   9  a key that no tenant has, which the stage makes: the stage reads with it, and the error of
#      strata list with that key holds no copy of it (#620). The stage writes nothing, and the key
#      is no secret. `tiers.sh 9` runs it alone, and no other FROM_STAGE runs it
#
# Needs the live example at its baseline with cluster A (just live-up), the strata release binary,
# jq, and the key as verify.sh needs it. Billable: cluster B for about 30 minutes; stages 5 to 9
# add no machine. Results go under
# runs/<UTC time>/tiers/, or OUT; FROM_STAGE resumes a run in OUT. Exit 0 only when no check fails.
set -uo pipefail
script=tiers
from=${1:-1}
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
case "$from" in 1|2|3|4|5|6|7|8|9) ;; *) usage; exit 2 ;; esac
[ -x "$strata" ] || die "no strata binary at $strata"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/tiers}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

summary() {
  write_summary "cluster tiers test" ", cluster A \`${C:-}\`," \
    "cluster B \`${B:-}\`."
}
trap summary EXIT

load_outputs
C=$(out cluster_uid) R=$(out release_profile_uid)
release=$(out profile_name)
[ -n "$C" ] || die "no cluster A; run just live-up"
# the role bindings of cluster A: stage 5 reads and puts them, and stages 6 and 8 read them with
# another key
rbacs_route=/v1/spectroclusters/$C/config/rbacs
export PALETTE_PROJECT_UID=$(out project_uid)

# tier_state FILE TIER: the state of one tier in a compare --json, same or changed
tier_state() { jq -r --arg t "$2" '.clusterTiers[] | select(.tier == $t) | if .same then "same" else "changed" end' "$1"; }

# rbacs: the role bindings of cluster A, as Palette gives them. A 404 is a cluster with none.
rbacs() {
  local answer status
  answer=$("$live/api.sh" get-status "$rbacs_route") || return 2
  status=$(sed -n 1p <<< "$answer")
  case "$status" in
    "HTTP 2"*) sed 1d <<< "$answer" ;;
    "HTTP 404") echo '{"items": []}' ;;
    *) echo "tiers.sh: GET $rbacs_route answered $status: $(sed 1d <<< "$answer" | head -c 300)" >&2
       return 2 ;;
  esac
}
# put_rbacs [SUBJECT]: PUT the role bindings of cluster A as they were before stage 5. With SUBJECT,
# the PUT also holds one more binding of the user SUBJECT to the cluster role view. The PUT
# replaces each binding of the cluster.
put_rbacs() {
  local body
  body=$(jq -c --arg s "${1:-}" '{rbacs: ([.items[]? | {spec: {bindings: .spec.bindings}}]
    + if $s == "" then [] else [{spec: {bindings: [{type: "ClusterRoleBinding",
      role: {kind: "ClusterRole", name: "view"}, subjects: [{type: "User", name: $s}]}]}}] end)}' rbac-before.out) \
    || return 2
  "$live/api.sh" put "$rbacs_route" "$body"
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

# read_with VAR PATH: GET PATH with the key in the variable VAR, such as STRATA_LIVE_LIMITED_KEY.
# api.sh get-status prints the status and the body of the answer. The key never leaves the
# environment of that one call, and no argument holds it.
read_with() { PALETTE_API_KEY=${!1} "$live/api.sh" get-status "$2"; }
# refusal_code FILE: the error code in the body of the refusal that FILE holds after its status line
refusal_code() { sed 1d "$1" | jq -r '.code // empty'; }
# limited_record: a record of cluster A that strata reads with the second key
limited_record() { PALETTE_API_KEY=$STRATA_LIVE_LIMITED_KEY "$strata" snapshot "cluster:$C" -o limited-record.json; }

# stage 6 runs only when its number is given: it needs a second key
if [ "$from" = 6 ] && stage 6 "a key that cannot read the role bindings of cluster A"; then
  [ -n "${STRATA_LIVE_LIMITED_KEY:-}" ] \
    || die "could not check: set STRATA_LIVE_LIMITED_KEY to a key that cannot read config/rbacs of cluster A"
  check limited-control 0 "#276: the key of the example reads the role bindings of cluster A" -- rbacs
  check limited-read 0 "#276: the second key does not read them: api.sh get-status prints the refusal of Palette" -- \
    read_with STRATA_LIVE_LIMITED_KEY "$rbacs_route"
  check limited-status 0 "#276: the status of the refusal is HTTP 500, and not 403" -- grep -qx 'HTTP 500' limited-read.out
  check limited-code ? "#276: the error code of the body that Palette answers" -- refusal_code limited-read.out
  check limited-code-says 0 "#276: the code is ResourceAccessDenied" -- grep -qx ResourceAccessDenied limited-code.out
  check limited-message 0 "#276: the message names the project: Project '<uid>' access denied" -- \
    grep -q "Project '$PALETTE_PROJECT_UID' access denied" limited-read.out
  check limited-record ? "#276: what strata does with the answer: a record of cluster A with the second key" -- limited_record
  check limited-record-errors ? "#276: the errors of the policy tier of that record, if strata wrote one" -- \
    jq '.source.cluster.tiers.policy.errors' limited-record.json
fi

# pool_route: the route of the cloud config of cluster A, /v1/cloudconfigs/<cloud type>/<config uid>
pool_route() {
  "$live/api.sh" get "/v1/spectroclusters/$C" \
    | jq -er '"/v1/cloudconfigs/\(.spec.cloudConfigRef.kind // .spec.cloudType)/\(.spec.cloudConfigRef.uid)"'
}
# put_pool FILE: PUT the machine pool worker-pool of the cloud config with the entity in FILE. The
# entity is a pair {cloudConfig, poolConfig}, as the create body holds a pool
put_pool() { "$live/api.sh" put "$pool_cc/machinePools/worker-pool" "$(cat "$1")"; }
# The text of both overrides: a YAML comment. It sets no value, so no machine of the pool changes.
pool_override=$'# strata tiers.sh stage 7: this text sets no value\n'

# stage 7 runs only when the user gives its number: it writes a machine pool of cluster A
if [ "$from" = 7 ] && stage 7 "an override on a machine pool of cluster A"; then
  require_baseline
  check pool-route 0 "the route of the cloud config of cluster A" -- pool_route
  pool_cc=$(< pool-route.out)
  check pool-before 0 "a record of cluster A with the shape tier, before the override" -- \
    "$strata" snapshot "cluster:$C" --shape -o pool-before.json
  check pool-before-none 0 "the worker pool has neither override: Palette omits a field with no value" -- \
    jq -e '.source.cluster.tiers.shape.fields.pools[] | select(.name == "worker-pool")
      | (has("overrideClusterAPIConfig") or has("overrideHealthCheckConfiguration")) | not' pool-before.json
  # the entity of the pool in the form of the create body: strata-preflight splits the pool of the
  # cloud config into poolConfig and cloudConfig. Its exit follows the rules, so no check judges it.
  check pool-world ? "the create body of cluster A, from strata-preflight with the static tier" -- \
    "$strata" preflight "cluster:$C" --tier static --quiet --no-color --out pool-world --world
  check pool-entity 0 "the entity of the worker pool: poolConfig and cloudConfig" -- \
    jq -e '.spec.machinepoolconfig[] | select(.poolConfig.name == "worker-pool")' pool-world/entity.json
  check pool-body 0 "the entity with both overrides in its poolConfig" -- \
    jq -e --arg v "$pool_override" '.poolConfig += {overrideClusterAPIConfig: $v, overrideHealthCheckConfiguration: $v}' pool-entity.out
  check pool-set 0 "the worker pool of cluster A gets both overrides, through the API" -- put_pool pool-body.out
  check pool-read 0 "the cloud config of cluster A after the override" -- "$live/api.sh" get "$pool_cc"
  check pool-in-cloud-config 0 "Palette holds both overrides in the worker pool of spec.machinePoolConfig" -- \
    jq -e --arg v "$pool_override" '.spec.machinePoolConfig[] | select(.name == "worker-pool")
      | .overrideClusterAPIConfig == $v and .overrideHealthCheckConfiguration == $v' pool-read.out
  check pool-cluster-level ? "whether spec.clusterConfig holds an override now: the value of each, null when absent" -- \
    jq '.spec.clusterConfig | {overrideClusterAPIConfig, overrideHealthCheckConfiguration}' pool-read.out
  check pool-record 0 "a record of cluster A with the shape tier, after the override" -- \
    "$strata" snapshot "cluster:$C" --shape -o pool-after.json
  check pool-in-record 0 "the shape tier keeps both overrides on the worker pool, and on no other pool or the cloud part" -- \
    jq -e --arg v "$pool_override" '.source.cluster.tiers.shape.fields
      | (.pools[] | select(.name == "worker-pool") | .overrideClusterAPIConfig == $v and .overrideHealthCheckConfiguration == $v)
        and ([.pools[] | select(.name != "worker-pool") | has("overrideClusterAPIConfig") or has("overrideHealthCheckConfiguration")] | any | not)
        and (.cloud | has("overrideHealthCheckConfiguration") | not)' pool-after.json
  check pool-compare 0 "compare of the record from before with the record after: the release is the same, exit 0" -- \
    "$strata" compare pool-before.json pool-after.json
  check pool-compare-json 0 "the same comparison as JSON" -- "$strata" compare pool-before.json pool-after.json --json
  check pool-compare-shape 0 "the shape tier differs at both overrides of the worker pool" -- \
    jq -e '.clusterTiers[] | select(.tier == "shape") | .same == false
      and ([.diff[].path] | index("pools[name=worker-pool].overrideClusterAPIConfig") != null
        and index("pools[name=worker-pool].overrideHealthCheckConfiguration") != null)' pool-compare-json.out
  check pool-compare-says 0 "the report names the override of the pool" -- \
    grep -q '+ shape.pools\[name=worker-pool\].overrideHealthCheckConfiguration' pool-compare.out
  check pool-verify 0 "verify of the record from before: the verdict follows the release, exit 0" -- \
    "$strata" verify pool-before.json
  check pool-verify-says 0 "and it reports the shape tier" -- grep -q 'shape CHANGED' pool-verify.out
  check pool-undo 0 "the worker pool of cluster A as before the stage" -- put_pool pool-entity.out
  check pool-after-read 0 "the cloud config of cluster A after the undo" -- "$live/api.sh" get "$pool_cc"
  check pool-undone 0 "the worker pool has neither override again" -- \
    jq -e '.spec.machinePoolConfig[] | select(.name == "worker-pool")
      | (has("overrideClusterAPIConfig") or has("overrideHealthCheckConfiguration")) | not' pool-after-read.out
  check pool-restored 0 "compare of the record from before with cluster A: the shape tier is the same, exit 0" -- \
    "$strata" compare pool-before.json "cluster:$C"
  check pool-restored-says 0 "and it names no change of the shape tier" -- \
    bash -c "! grep -q 'shape CHANGED' pool-restored.out"
fi

# narrow_routes: the routes of cluster A that stage 8 reads with the third key, as NAME PATH. Each
# one needs the permission cluster.get.
narrow_routes() {
  cat << EOF
rbacs $rbacs_route
namespaces /v1/spectroclusters/$C/config/namespaces
repave /v1/spectroclusters/$C/repave/status
EOF
}
# forbidden_says FILE: the answer that FILE holds is HTTP 500 with the code OperationForbidden, and
# its message names the permission 'cluster.get'
forbidden_says() {
  [ "$(head -n 1 "$1")" = "HTTP 500" ] || return 1
  sed 1d "$1" | jq -e --arg p "'cluster.get'" \
    '.code == "OperationForbidden" and (.message // "" | contains($p))' > /dev/null
}

# narrow_list: strata list with the third key. The run keeps its progress notes, so that standard
# error shows each retry
narrow_list() { env -u STRATA_QUIET PALETTE_API_KEY="$STRATA_LIVE_NARROW_KEY" "$strata" list; }

# stage 8 runs only when its number is given: it needs a third key
if [ "$from" = 8 ] && stage 8 "a key whose role does not give cluster.get"; then
  [ -n "${STRATA_LIVE_NARROW_KEY:-}" ] \
    || die "could not check: set STRATA_LIVE_NARROW_KEY to a key with a role in the project that does not give cluster.get"
  check narrow-control 0 "#665: the key of the example reads the role bindings of cluster A" -- rbacs
  while read -r name path; do
    check "narrow-$name" 0 "#665: GET $path with the third key: api.sh get-status prints the answer of Palette" -- \
      read_with STRATA_LIVE_NARROW_KEY "$path"
    check "narrow-$name-code" ? "#665: the error code of that answer" -- refusal_code "narrow-$name.out"
    check "narrow-$name-says" 0 "#665: the answer is HTTP 500 with the code OperationForbidden, and the message names 'cluster.get'" -- \
      forbidden_says "narrow-$name.out"
  done < <(narrow_routes)
  check narrow-list 2 "#620: strata list with the third key prints what it read, and exits 2 for the clusters" -- narrow_list
  check narrow-list-profiles 0 "#620: the listing keeps the release profile, which the role reads" -- \
    grep -q "^  $release@" narrow-list.out
  check narrow-list-says 0 "#620: the listing names the clusters as not read: HTTP 500 and ResourceAccessDenied" -- \
    grep -q '^clusters: not read (GET /v1/dashboard/spectroclusters/meta failed with HTTP 500: .*ResourceAccessDenied' narrow-list.out
  check narrow-list-once 1 "#620: strata asks for the clusters one time: standard error holds no retry" -- \
    grep -q retry narrow-list.err
fi

# unknown_key: a key that no tenant has. So it is no secret, and a check can look for each copy of
# it
unknown_key="strata-unknown-key-$(printf '%0381d' 0)"
# unknown_list: strata list projects with the key that no tenant has, as text and as JSON
unknown_list() {
  PALETTE_API_KEY=$unknown_key "$strata" list projects
  PALETTE_API_KEY=$unknown_key "$strata" list projects --json
}

# stage 9 runs only when its number is given
if [ "$from" = 9 ] && stage 9 "a key that no tenant has"; then
  check unknown-key-read 0 "#620: GET the project listing with a key that no tenant has: api.sh get-status prints the answer" -- \
    read_with unknown_key /v1/dashboard/projects/metadata
  check unknown-key-list 2 "#620: strata list projects with that key, as text and as JSON, exits 2 with the error" -- unknown_list
  check unknown-key-masked 1 "#620: the errors of strata hold no copy of the key" -- \
    grep -qF "$unknown_key" unknown-key-list.out unknown-key-list.err
fi

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]
