#!/usr/bin/env bash
# The cluster group test (#78): a record of the profiles that a cluster group pins for its virtual
# clusters, and compare of the group with a member.
#
#   examples/live/cluster-group.sh [FROM_STAGE]
#
#   1  a cluster group on cluster A that pins the add-on version, and one virtual cluster in it: a
#      record of clustergroup:<uid> holds the pin and names the member; compare of the group with
#      the member reads only the pinned profile of the member, and is IDENTICAL; with --macros,
#      the record holds the digests of the macro values of the group
#   2  an edit in place of a manifest of the add-on version: the group and the member each keep
#      their own copy of the manifest, under a uid of its own, and the version differs; then the
#      undo
#   3  the CPU limit of the group changes, through OpenTofu: verify of a record from before
#      reports the policy tier, and compare of it with the group names limitConfig.cpuMilliCore
#      and the tier, each with exit 0 (#110); then the limit back
#   4  the group and the member removed: the baseline
#
# Needs the live example at its baseline with cluster A (just live-up), the strata release binary,
# jq, and the key as verify.sh needs it. The virtual cluster runs in cluster A, so it adds no
# machine. Results go under runs/<UTC time>/cluster-group/, or OUT; FROM_STAGE resumes a run in
# OUT. Exit 0 only when no check fails.
set -uo pipefail
script=cluster-group
from=${1:-1}
case "$from" in 1|2|3|4) ;; *) sed -n '2,/^set -uo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 2 ;; esac
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/cluster-group}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

summary() {
  {
    echo "# cluster group test"
    echo
    echo "Run \`$(basename "$(dirname "$run")")\` with \`$("$strata" --version)\`, group \`${G:-}\`,"
    echo "member \`${M:-}\`, add-on \`${AD:-}\`."
    echo
    summary_table
  } > summary.md
}
trap summary EXIT

stage() { [ "$from" -le "$1" ] && echo "== stage $1: $2" | tee -a steps.log; }

load_outputs
C=$(out cluster_uid) AD=$(out addon_profile_uid) AN=$(out addon_profile_name)
[ -n "$C" ] || die "no cluster A; run just live-up"
export PALETTE_PROJECT_UID=$(out project_uid)

if stage 1 "a cluster group, and a virtual cluster in it"; then
  [ "$from" -le 1 ] && require_baseline
  apply -var cluster_group=true
  G=$(out cluster_group_uid) GN=$(out cluster_group_name) M=$(out member_cluster_uid)
  check addon-record 0 'record the add-on version' -- "$strata" snapshot "$AD" -o addon.json
  check group-record 0 '#78: record clustergroup:<uid>' -- "$strata" snapshot "clustergroup:$G" -o group.json
  check group-kind 0 '#78: the record says cluster group, and holds the add-on version alone: the pin' -- \
    jq -e --arg a "$AD" '.source.cluster | .kind == "clusterGroup" and ([.profiles[].uid] == [$a])' group.json
  check group-members 0 '#78: the record names the virtual cluster as a member' -- \
    jq -e --arg m "$M" '[.source.cluster.members[].uid] | index($m)' group.json
  check group-root 0 '#78: the record has the root of the add-on version' -- \
    test "$(jq -r .root group.json)" = "$(jq -r .root addon.json)"
  check group-by-name 0 '#78: clustergroup:<name> finds the group' -- \
    bash -c "\"$strata\" snapshot clustergroup:$GN | jq -e --arg g $G '.source.cluster.uid == \$g'"
  check member-record 0 'record the member, whole' -- "$strata" snapshot "cluster:$M" -o member.json
  check member-runs-pin 0 '#78: the member runs the add-on version that the group pins' -- \
    jq -e --arg a "$AD" '[.source.cluster.profiles[].uid] | index($a)' member.json
  check group-compare 0 '#78: compare of the group with the member reads the pinned profile of the member: IDENTICAL' -- \
    "$strata" compare "clustergroup:$G" "cluster:$M"
  check group-compare-record 0 '#78: the same from the record of the group' -- \
    "$strata" compare group.json "cluster:$M"
  check group-compare-no-values 0 '#78: a group gives no variable values, so the compare states no cluster value' -- \
    bash -c "! grep -q 'cluster values' group-compare.out"
  check group-verify 0 '#78: verify of the record of the group' -- "$strata" verify group.json
  check group-macros 0 '#111: record clustergroup:<uid> with the digests of its macro values' -- \
    "$strata" snapshot "clustergroup:$G" --macros -o group-macros.json
  check group-macro-values 0 '#111: the record holds cluster.macroValues' -- \
    jq -e '.source.cluster.macroValues | type == "object"' group-macros.json
  check group-macros-verify 0 '#111: verify of that record reads the values of the group again' -- "$strata" verify group-macros.json
  check member-consumer 1 '#78: consumers of the add-on version' -- "$strata" consumers "$AD" --json
  check member-consumer-listed 0 '#78: the consumer list of Palette names the virtual cluster' -- \
    jq -e --arg m "$M" '.consumers[] | select(.uid == $m and (.unlisted | not))' member-consumer.out
  check member-not-in-listing 0 '#78: the cluster listing does not name the virtual cluster' -- \
    bash -c "\"$live/api.sh\" get /v1/dashboard/spectroclusters/meta | jq -e --arg m $M '[.. | objects | select(has(\"metadata\")) | .metadata.uid] | index(\$m) | not'"
  # the search with the name of the add-on profile, as the second reading of consumers sends it:
  # Palette changes the name into the uids of the profiles with that name, and a uid matches no
  # name (#149, #181)
  condition='{property: $p, type: "string", condition: {string: {operator: "eq", negation: false,
    ignoreCase: false, match: {conjunction: "or", values: [$v]}}}}'
  jq -nc --arg p clusterProfiles --arg v "$AN" "{filter: {conjunction: \"and\", filterGroups:
    [{conjunction: \"and\", filters: [$condition]}]}, sort: []}" > search-profile.json
  jq -c --arg p environment --arg v nested ".filter.filterGroups[0].filters += [$condition]" \
    search-profile.json > search-nested.json
  check search-profile 0 '#181: the cluster search with clusterProfiles eq the name of the add-on profile' -- \
    "$live/api.sh" post '/v1/dashboard/spectroclusters/search?limit=50' "$(cat search-profile.json)"
  check search-profile-runs 0 '#181: it answers cluster A, which runs the add-on version, and not the virtual cluster' -- \
    jq -e --arg c "$C" --arg m "$M" '([.items[].metadata.uid] | index($c))
      and ([.items[].metadata.uid] | index($m) | not)' search-profile.out
  check search-nested 0 '#181: the same search with environment eq nested' -- \
    "$live/api.sh" post '/v1/dashboard/spectroclusters/search?limit=50' "$(cat search-nested.json)"
  check search-nested-misses-member 0 '#184: it does not answer the virtual cluster, which runs the add-on version through its group' -- \
    jq -e --arg m "$M" '[.items[].metadata.uid] | index($m) | not' search-nested.out
  check member-consumer-search 0 '#149: consumers read the second reading from the search' -- \
    jq -e '.clustersRead.resolved == "search"' member-consumer.out
  # so consumers also reads the virtual clusters of each group that pins the version (#184)
  check member-consumer-group 0 '#184: consumers finds the group that pins the add-on version' -- \
    jq -e '.clustersRead.groups >= 1' member-consumer.out
  check group-virtual-clusters 0 '#184: the virtual clusters of the group, which consumers reads, name the virtual cluster' -- \
    bash -c "\"$live/api.sh\" post '/v1/dashboard/clustergroups/$G/virtualClusters?limit=50' '{\"filter\": {}, \"sort\": []}' | jq -e --arg m $M '[.items[].metadata.uid] | index(\$m)'"
fi

G=${G:-$(out cluster_group_uid)} M=${M:-$(out member_cluster_uid)}

if stage 2 "an edit in place of the add-on version"; then
  check edit 0 "edit the manifest of the add-on version in place through the API" -- \
    "$live/api.sh" edit "$AD" 'edited: "no"' 'edited: "yes"'
  sleep 20
  check edit-group-verify 0 '#81: the group keeps its own copy of the manifest: verify of its record passes' -- \
    "$strata" verify group.json
  check edit-group-compare 0 '#81: the member keeps its copy too, so it still runs what the group pins' -- \
    "$strata" compare "clustergroup:$G" "cluster:$M"
  check edit-group-vs-version 1 '#81: the version differs from the copy of the group: the edit is in the version alone' -- \
    "$strata" compare "clustergroup:$G" "$AD"
  check edit-group-vs-version-says 0 '#81: the compare names the manifest of the add-on pack' -- \
    grep -q '^      ~ manifest ' edit-group-vs-version.out
  check edit-group-raw 0 'the group document after the edit' -- "$live/api.sh" get "/v1/clustergroups/$G"
  check edit-group-manifest-uid 0 '#78: the template of the group names the manifest by uid, and holds no content' -- \
    jq -e '[.spec.clusterProfileTemplates[].packs[].manifests[]] | length > 0 and all(.uid != "" and ((.content // "") == ""))' edit-group-raw.out
  check edit-group-own-uid 0 '#81: that uid is a copy of its own, and not the uid of the manifest of the version' -- \
    bash -c "comm -12 <(jq -r '.spec.clusterProfileTemplates[].packs[].manifests[]?.uid' edit-group-raw.out | sort) \
      <(\"$live/api.sh\" get /v1/clusterprofiles/$AD/packs/manifests | jq -r '.spec.packs[].spec.manifests[]?.uid' | sort) | grep -q . && exit 1 || exit 0"
  check edit-undo 0 'undo the edit' -- "$live/api.sh" edit "$AD" 'edited: "yes"' 'edited: "no"'
fi

if stage 3 "the CPU limit of the group"; then
  check limit-record 0 '#110: a record of the group before the change' -- \
    "$strata" snapshot "clustergroup:$G" -o limit.json
  check limit-before 0 '#110: the policy tier holds the CPU limit of the group, 6000 millicores' -- \
    jq -e '.source.cluster.tiers.policy.fields.limitConfig.cpuMilliCore == 6000' limit.json
  # only the group: a plan of the member also removes the profile that the group pins, and
  # Palette refuses that update (ClusterUpdateValidationFailure)
  apply -var cluster_group=true -var group_cpu_millicore=4000 -target='spectrocloud_cluster_group.group[0]'
  check limit-verify 0 '#110: verify of the record: the pinned profile is unchanged, exit 0' -- "$strata" verify limit.json
  check limit-verify-says 0 '#110: and it reports the policy tier' -- grep -q 'policy CHANGED' limit-verify.out
  check limit-compare 0 '#110: compare of the record with the group: one release, exit 0' -- \
    "$strata" compare limit.json "clustergroup:$G"
  check limit-compare-says 0 '#110: the report names the field' -- \
    grep -q '~ policy.limitConfig.cpuMilliCore: 6000 -> 4000' limit-compare.out
  check limit-compare-json 0 'the same comparison as JSON' -- "$strata" compare limit.json "clustergroup:$G" --json
  check limit-compare-tier 0 '#110: the policy tier differs, at limitConfig.cpuMilliCore' -- \
    jq -e '.clusterTiers[] | select(.tier == "policy") | .same == false
      and ([.diff[].path] | index("limitConfig.cpuMilliCore"))' limit-compare-json.out
  apply -var cluster_group=true -target='spectrocloud_cluster_group.group[0]'
  check limit-undone 0 'the limit is back: compare of the record with the group finds the same policy tier' -- \
    bash -c "\"$strata\" compare limit.json clustergroup:$G --json | jq -e '.clusterTiers[] | select(.tier == \"policy\") | .same'"
fi

if stage 4 "the baseline"; then
  apply_baseline
  check gone 0 'the group and the member are gone' -- \
    test -z "$(out cluster_group_uid)$(out member_cluster_uid)"
  check member-audit-kind 0 '#188: the log of the member by its uid alone has its create and its delete under virtualCluster' -- \
    bash -c "\"$live/api.sh\" get '/v1/audits?resourceUid=$M&limit=100' | jq -e '[.items[] | select(.spec.resource.kind == \"virtualCluster\") | .spec.actionType] | index(\"create\") and index(\"delete\")'"
  check member-audit-not-cluster 0 '#188: a query with spectrocluster and the uid of the member does not find them' -- \
    bash -c "\"$live/api.sh\" get '/v1/audits?resourceKind=spectrocluster&resourceUid=$M&limit=100' | jq -e '[(.items // [])[] | select(.spec.actionType == \"create\" or .spec.actionType == \"delete\")] | length == 0'"
fi

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]
