#!/usr/bin/env bash
# The macro test (#72): what a project macro does to a published version, to the copy that
# cluster A runs, and to what strata records, when the macro gets another value.
#
#   examples/live/macros.sh [FROM_STAGE]
#
#   1  at the baseline: the add-on version reads no macro, and its record names none; Palette
#      resolves the macros of the version with no cluster, in one map
#   2  a project macro, and the manifest of the add-on version reads it, through the API; cluster
#      A takes the update. Palette says the version has user macros, strata names the macro, and
#      a record of the version or of the cluster with --macros holds the digest of its value
#   3  the macro gets another value: Palette resolves it at once; the record of the version, and a
#      cluster record without the digests, still verify; a record of the version or of the cluster
#      with the digests reports DRIFT and names the macro; no notification opens, and the audit
#      log of the cluster holds no entry
#   4  what the cluster runs: the value inside the cluster, read with its kubeconfig, until it is
#      the new one or PROPAGATION_MINUTES pass (default 20); the runs saw it after 11 and 13
#      minutes
#   5  the manifest back, cluster A takes it, and the macro deleted
#
# Needs the live example at its baseline with cluster A (just live-up), the strata release binary,
# jq, kubectl, and the key as verify.sh needs it. The kubeconfig lives in a temporary file and is
# never saved. Results go under runs/<UTC time>/macros/, or OUT; FROM_STAGE resumes a run in OUT.
# Exit 0 only when no check fails.
set -uo pipefail
script=macros
from=${1:-1}
case "$from" in 1|2|3|4|5) ;; *) sed -n '2,/^set -uo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 2 ;; esac
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"
command -v kubectl > /dev/null || die "kubectl is needed to read the value inside the cluster"

macro=strata_probe
edit_from='edited: "no"'
edit_to="edited: \"{{.spectro.macro.$macro}}\""
minutes=${PROPAGATION_MINUTES:-20}

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/macros}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

summary() {
  {
    echo "# macro test"
    echo
    echo "Run \`$(basename "$(dirname "$run")")\` with \`$("$strata" --version)\`, cluster \`${C:-}\`,"
    echo "add-on \`${AD:-}\`, project macro \`$macro\`."
    echo
    summary_table
  } > summary.md
}
trap summary EXIT

stage() { [ "$from" -le "$1" ] && echo "== stage $1: $2" | tee -a steps.log; }

load_outputs
C=$(out cluster_uid) AD=$(out addon_profile_uid) P=$(out project_uid)
addon=$(out addon_profile_name)
[ -n "$C" ] || die "no cluster A; run just live-up"
export PALETTE_PROJECT_UID=$P

has_macros() { "$live/api.sh" get "/v1/clusterprofiles/$AD" | jq -r '.status.hasUserMacros'; }
resolved() { "$live/api.sh" get "/v1/spectroclusters/$C/packs/resolvedValues" | jq -r --arg k "{{.spectro.macro.$macro}}" '[.profiles[].resolved[$k]] | first // "none"'; }
# profile_resolved: the value of the macro at the scope of the add-on version, with no cluster
profile_resolved() { "$live/api.sh" get "/v1/clusterprofiles/$AD/packs/resolvedValues" | jq -r --arg k "{{.spectro.macro.$macro}}" '.resolved[$k] // "none"'; }
# inside: the value of the ConfigMap that the manifest makes, read inside the cluster
inside() {
  local k
  k=$(mktemp)
  "$live/api.sh" get-kubeconfig "$C" > "$k" 2> /dev/null
  KUBECONFIG=$k timeout 60 kubectl get configmap strata-live -n default -o jsonpath='{.data.edited}' 2>&1
  rm -f "$k"
}
# settled: wait up to five minutes until cluster A says no update waits
settled() {
  for _ in $(seq 1 20); do
    "$strata" snapshot "cluster:$C" 2> /dev/null | jq -e '.source.cluster.updatesPending == false' > /dev/null && return 0
    sleep 15
  done
  return 1
}

if stage 1 "the baseline"; then
  require_baseline
  check base-flag 0 '#72: Palette says the add-on version has no user macros' -- test "$(has_macros)" = false
  check base-record 0 "record $addon" -- "$strata" snapshot "$AD" -o addon-0.json
  check base-no-note 1 '#72: a report of it names no macro' -- \
    grep -q 'the content uses macro' <("$strata" compare addon-0.json "$AD")
  check base-profile-resolved 0 '#111: Palette resolves the macros of the version in one map, and the system macro of the cluster name is empty there' -- \
    bash -c "\"$live/api.sh\" get /v1/clusterprofiles/$AD/packs/resolvedValues | jq -e '(.resolved | type == \"object\") and .resolved[\"{{.spectro.system.cluster.name}}\"] == \"\"'"
fi

if stage 2 "a macro that the add-on reads"; then
  check macro-set 0 "#72: the project macro $macro gets the value one" -- "$live/api.sh" macro-set "$macro" one
  check macro-edit 0 "#72: the manifest of $addon reads the macro, through the API" -- \
    "$live/api.sh" edit "$AD" "$edit_from" "$edit_to"
  check macro-flag 0 '#72: Palette says the version has user macros now' -- test "$(has_macros)" = true
  check macro-record 0 "record $addon" -- "$strata" snapshot "$AD" -o addon-1.json
  check macro-names 0 '#72: a report of the record names the macro' -- \
    grep -q "note       the content uses macro $macro: the root covers its name and not its value" <("$strata" compare addon-1.json "$AD")
  check macro-version 0 '#111: record the add-on version with the digests of its macro values' -- \
    "$strata" snapshot "$AD" --macros -o addon-macros-1.json
  check macro-version-digest 0 '#111: the record holds the sha256 of the value at the scope of the version, and not the value' -- \
    jq -e --arg m "$macro" '(.source.macroValues[$m] | startswith("sha256:")) and (tostring | contains("\"one\"") | not)' addon-macros-1.json
  check macro-profile-resolved-1 0 '#111: resolvedValues of the version gives the value one' -- test "$(profile_resolved)" = one
  check macro-accept 0 '#72: cluster A takes the update' -- "$live/api.sh" apply "$C"
  check macro-settled 0 'no update waits for cluster A' -- settled
  check macro-cluster 0 '#72: record cluster A with the digests of its macro values' -- \
    "$strata" snapshot "cluster:$C" --macros -o cluster-1.json
  check macro-cluster-plain 0 'record cluster A without them' -- "$strata" snapshot "cluster:$C" -o cluster-plain.json
  check macro-digest 0 '#72: the record holds the sha256 of the value, and not the value' -- \
    jq -e --arg m "$macro" '(.source.cluster.macroValues[$m] | startswith("sha256:")) and (tostring | contains("\"one\"") | not)' cluster-1.json
  check macro-resolved-1 0 '#72: resolvedValues gives the value one' -- test "$(resolved)" = one
fi

if stage 3 "the macro gets another value"; then
  audits_before=$("$live/api.sh" get "/v1/audits?resourceKind=spectrocluster&resourceUid=$C&limit=1" | jq -r '.items[0].metadata.uid // ""')
  check change 0 "#72: the macro gets the value two" -- "$live/api.sh" macro-set "$macro" two
  sleep 20
  check change-resolved 0 '#72: resolvedValues gives the value two at once' -- test "$(resolved)" = two
  check change-profile-resolved 0 '#111: resolvedValues of the version gives the value two at once' -- test "$(profile_resolved)" = two
  check change-version 0 '#72: the record of the version still verifies' -- "$strata" verify addon-1.json
  check change-plain 0 '#72: a cluster record without the digests still verifies' -- "$strata" verify cluster-plain.json
  check change-version-macros 1 '#111: the record of the version with the digests reports DRIFT' -- "$strata" verify addon-macros-1.json
  check change-version-macros-says 0 '#111: the verdict names the macro' -- \
    grep -q "^DRIFT: .* but macro $macro has another value now" change-version-macros.out
  check change-cluster 1 '#72: the cluster record with the digests reports DRIFT' -- "$strata" verify cluster-1.json
  check change-cluster-says 0 '#72: the verdict names the macro' -- \
    grep -q "^DRIFT: .* but macro $macro has another value now" change-cluster.out
  check change-compare 1 '#72: compare of the two cluster records exits 1 with equal roots' -- \
    bash -c "\"$strata\" snapshot cluster:$C --macros -o cluster-2.json && \"$strata\" compare cluster-1.json cluster-2.json"
  check change-compare-says 0 '#72: and names the macro' -- grep -q "macro $macro has another value on each side" change-compare.out
  check change-no-notification 0 '#72: no notification opens for the cluster' -- \
    bash -c "\"$live/api.sh\" get /v1/notifications/spectrocluster/$C | jq -e '[.items[]? | select(.action.isDone == false)] | length == 0'"
  check change-no-audit 0 '#72: the audit log of the cluster holds no new entry' -- \
    test "$("$live/api.sh" get "/v1/audits?resourceKind=spectrocluster&resourceUid=$C&limit=1" | jq -r '.items[0].metadata.uid // ""')" = "$audits_before"
fi

if stage 4 "what the cluster runs"; then
  changed_at=$(date -u +%s)
  seen=""
  for _ in $(seq 1 $((minutes * 2))); do
    seen=$(inside)
    echo "$(date -u +%H:%M:%S) inside the cluster: $seen" >> propagation.log
    [ "$seen" = two ] && break
    sleep 30
  done
  echo "after $(( ($(date -u +%s) - changed_at) / 60 )) minutes: $seen" >> propagation.log
  check propagation 0 "#72: Palette writes the new value inside the cluster, with no update to accept, within $minutes minutes (propagation.log)" -- test "$seen" = two
fi

if stage 5 "undo"; then
  check undo-edit 0 "the manifest of $addon back" -- "$live/api.sh" edit "$AD" "$edit_to" "$edit_from"
  check undo-accept 0 'cluster A takes it' -- "$live/api.sh" apply "$C"
  check undo-settled 0 'no update waits for cluster A' -- settled
  check undo-macro 0 "the macro $macro deleted" -- "$live/api.sh" macro-delete "$macro"
  check undo-flag 0 '#72: Palette says the version has no user macros again' -- test "$(has_macros)" = false
  check undo-baseline 0 'the example is at its baseline' -- "$live/baseline.sh"
fi

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]
