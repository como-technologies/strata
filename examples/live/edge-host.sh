#!/usr/bin/env bash
# The edge host test (#78): a record of the profiles bound to an edge host, and consumers of a
# profile that an edge host uses.
#
#   examples/live/edge-host.sh [FROM_STAGE]
#
#   1  an edge host registered with no device, and the add-on version bound to it: a record of
#      edgehost:<uid> has the root of the add-on version; consumers counts the edge host, also
#      with --list-only; verify of the record is OK
#   2  an edit in place of a manifest of the add-on version: the edge host holds the manifest by
#      uid, so it gets the edit, and verify of its record reports drift; then the undo
#   3  a second edge host bound to the release version, and an edit in place of a value of it:
#      that edge host keeps its copy of the values; then the undo, and the second host deleted
#   4  the edge host deleted: consumers no longer counts it
#
# Needs the live example at its baseline (just live-up), the strata release binary, jq, and the
# key as verify.sh needs it. The edge host has no device, so it costs nothing and runs nothing.
# Results go under runs/<UTC time>/edge-host/, or OUT; FROM_STAGE resumes a run in OUT. Exit 0
# only when no check fails.
set -uo pipefail
script=edge-host
from=${1:-1}
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
case "$from" in 1|2|3|4) ;; *) usage; exit 2 ;; esac
[ -x "$strata" ] || die "no strata binary at $strata"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/edge-host}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

E=strata-edge-78 E2=strata-edge-78-release

summary() {
  write_summary "edge host test" ", edge host \`$E\`," \
    "add-on \`${AD:-}\`."
}
trap summary EXIT

load_outputs
AD=$(out addon_profile_uid) R=$(out release_profile_uid)
[ -n "$AD" ] || die "no add-on profile; run just live-up"
export PALETTE_PROJECT_UID=$(out project_uid)

# host_row FILE: the row of the edge host in a consumers --json
host_row() { jq -e --arg e "$E" '.consumers[] | select(.uid == $e and .kind == "edgehost")' "$1"; }

if stage 1 "an edge host bound to the add-on version"; then
  require_baseline
  check register 0 "register edge host $E with no device, and bind the add-on version to it" -- \
    "$live/api.sh" edge-host-create "$E" "$AD"
  check addon-record 0 'record the add-on version' -- "$strata" snapshot "$AD" -o addon.json
  check host-record 0 '#78: record edgehost:<uid>' -- "$strata" snapshot "edgehost:$E" -o host.json
  check host-flag 0 '#78: the record says edge host, with the uid and the name of the host' -- \
    jq -e --arg e "$E" '.source.cluster | .kind == "edgeHost" and .uid == $e and .name == $e' host.json
  check host-root 0 '#78: the record has the root of the add-on version: the same packs and variable definitions' -- \
    test "$(jq -r .root host.json)" = "$(jq -r .root addon.json)"
  check host-compare 0 '#78: compare of the add-on version with the edge host' -- \
    "$strata" compare addon.json host.json
  check host-by-name 0 '#78: edgehost:<name> finds the host through the search' -- \
    bash -c "\"$strata\" snapshot edgehost:$E | jq -e --arg e $E '.source.cluster.uid == \$e'"
  check host-unknown 2 '#78: an edge host that is not there is an error that lists the edge hosts' -- \
    "$strata" snapshot edgehost:no-such-host
  check host-unknown-says 0 '#78: the error names the edge host of the scope' -- \
    grep -q "no edge host with the uid or name 'no-such-host' in this scope; edge hosts: .*$E" host-unknown.err
  check consumers 1 '#78: consumers of the add-on version' -- "$strata" consumers "$AD" --json
  check consumers-host 0 '#78: consumers counts the edge host, with its kind, its project, and its state' -- \
    bash -c "cd '$run' && $(declare -f host_row); E=$E; host_row consumers.out | jq -e '.projectUid != \"\" and .state != \"\" and .version == \"1.0.0\"'"
  check consumers-list-only 1 '#78: consumers --list-only' -- "$strata" consumers "$AD" --list-only --json
  check consumers-list-only-host 0 '#78: the list of Palette names no edge host, so strata reads the edge hosts also with --list-only' -- \
    bash -c "cd '$run' && $(declare -f host_row); E=$E; host_row consumers-list-only.out"
  check host-verify 0 '#78: verify of the record of the edge host' -- "$strata" verify host.json
  check host-repave 2 '#78: compare --repave refuses an edge host: Palette answers it for a cluster' -- \
    "$strata" compare "edgehost:$E" "$AD" --repave
fi

if stage 2 "an edit in place of the add-on version"; then
  check edit 0 "edit the manifest of the add-on version in place through the API" -- \
    "$live/api.sh" edit "$AD" "$addon_from" "$addon_to"
  sleep 20
  check edit-host-verify 0 '#81: the edge host keeps its own copy of the manifest: verify of its record passes' -- \
    "$strata" verify host.json
  check edit-host-vs-version 1 '#81: the version differs from the copy of the host: the edit is in the version alone' -- \
    "$strata" compare "edgehost:$E" "$AD"
  check edit-host-record 0 'record the edge host after the edit' -- "$strata" snapshot "edgehost:$E" -o host-edited.json
  check edit-host-raw 0 'the template that Palette gives for the edge host, after the edit' -- \
    "$live/api.sh" get "/v1/edgehosts/$E"
  check edit-host-manifest-uid 0 '#78: the template of the host names the manifest by uid, and holds no content' -- \
    jq -e '[.spec.clusterProfileTemplates[].packs[].manifests[]] | length > 0 and all(.uid != "" and ((.content // "") == ""))' edit-host-raw.out
  check edit-host-own-uid 0 '#81: that uid is a copy of its own, and not the uid of the manifest of the version' -- \
    bash -c "comm -12 <(jq -r '.spec.clusterProfileTemplates[].packs[].manifests[]?.uid' edit-host-raw.out | sort) \
      <(\"$live/api.sh\" get /v1/clusterprofiles/$AD/packs/manifests | jq -r '.spec.packs[].spec.manifests[]?.uid' | sort) | grep -q . && exit 1 || exit 0"
  check edit-host-copy 0 '#81: the route of the copy gives the content from before the edit' -- \
    bash -c "for u in \$(jq -r '.spec.clusterProfileTemplates[].packs[].manifests[]?.uid' edit-host-raw.out); do \"$live/api.sh\" get /v1/edgehosts/$E/pack/manifests/\$u; done | jq -rs 'map(.spec.published.content) | join(\"\")' | grep -qF '$addon_from'"
  check edit-undo 0 'undo the edit' -- "$live/api.sh" edit "$AD" "$addon_to" "$addon_from"
fi

if stage 3 "an edit in place of a value of the release version"; then
  check release-register 0 "register edge host $E2, and bind the release version to it" -- \
    "$live/api.sh" edge-host-create "$E2" "$R"
  check release-host-record 0 "#78: record edgehost:$E2" -- "$strata" snapshot "edgehost:$E2" -o host-release.json
  check release-edit 0 '#78: edit the reclaim policy of the CSI pack of the release version in place' -- \
    "$live/api.sh" edit "$R" "$(out csi_reclaim_delete)" "$(out csi_reclaim_delete | sed 's/Delete/Retain/')"
  sleep 20
  check release-host-verify 0 '#78: the edge host keeps its copy of the pack values: verify of its record is OK' -- \
    "$strata" verify host-release.json
  check release-host-raw 0 'the template that Palette gives for the second host, after the edit' -- \
    "$live/api.sh" get "/v1/edgehosts/$E2"
  check release-host-kept 0 '#78: the host holds reclaimPolicy Delete, and the profile holds Retain' -- \
    bash -c "jq -r '.spec.clusterProfileTemplates[].packs[].values // \"\"' release-host-raw.out | grep -Eq '^ +reclaimPolicy: Delete'"
  check release-undo 0 'put the value back through the API' -- \
    "$live/api.sh" edit "$R" "$(out csi_reclaim_delete | sed 's/Delete/Retain/')" "$(out csi_reclaim_delete)"
  check release-delete 0 "delete edge host $E2" -- "$live/api.sh" edge-host-delete "$E2"
  # the round trip through the API leaves the values text other than OpenTofu wrote it
  apply_baseline
fi

if stage 4 "the edge host deleted"; then
  check delete 0 "delete edge host $E" -- "$live/api.sh" edge-host-delete "$E"
  check after 1 'consumers of the add-on version: cluster A still uses it' -- "$strata" consumers "$AD" --json
  check after-no-host 0 '#78: consumers no longer counts the edge host' -- \
    jq -e --arg e "$E" '[.consumers[] | select(.uid == $e)] | length == 0' after.out
fi

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]
