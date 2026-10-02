#!/usr/bin/env bash
# The cluster group test (#78): a record of the profiles that a cluster group pins for its virtual
# clusters, and compare of the group with a member.
#
#   examples/live/cluster-group.sh [FROM_STAGE]
#
#   1  a cluster group on cluster A that pins the add-on version, and one virtual cluster in it: a
#      record of clustergroup:<uid> holds the pin and names the member; compare of the group with
#      the member reads only the pinned profile of the member, and is IDENTICAL
#   2  an edit in place of a manifest of the add-on version: the group and the member each keep
#      their own copy of the manifest, under a uid of its own, and the version differs; then the
#      undo
#   3  the group and the member removed: the baseline
#
# Needs the live example at its baseline with cluster A (just live-up), the strata release binary,
# jq, and the key as verify.sh needs it. The virtual cluster runs in cluster A, so it adds no
# machine. Results go under runs/<UTC time>/cluster-group/, or OUT; FROM_STAGE resumes a run in
# OUT. Exit 0 only when no check fails.
set -uo pipefail
script=cluster-group
from=${1:-1}
case "$from" in 1|2|3) ;; *) sed -n '2,/^set -uo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 2 ;; esac
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
  check search-nested-member 0 '#181: it answers the virtual cluster of the group' -- \
    jq -e --arg m "$M" '[.items[].metadata.uid] | index($m)' search-nested.out
  check member-consumer-search 0 '#149: consumers read the second reading from the search' -- \
    jq -e '.clustersRead.resolved == "search"' member-consumer.out
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

if stage 3 "the baseline"; then
  apply_baseline
  check gone 0 'the group and the member are gone' -- \
    test -z "$(out cluster_group_uid)$(out member_cluster_uid)"
fi

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]
