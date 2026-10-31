#!/usr/bin/env bash
# The pending update test (#70, #78): when Palette says that an update waits for cluster A, what
# a cluster record, `list clusters --applied`, and `compare` say about it, and what a record of
# `cluster:<uid>@pending` holds.
#
#   examples/live/pending.sh [FROM_STAGE]
#
#   1  at the baseline: a record of cluster A says that no update waits, and a record of
#      cluster:<uid>@pending has the root of what the cluster runs; consumers --list-only gives
#      the clusters that the walk gives
#   2  an edit in place of a manifest of the add-on version: an update waits, and the record, the
#      listing, and compare name it; the record of the release profile alone says none waits;
#      compare of the cluster with cluster:<uid>@pending names the pack that the update changes;
#      compare --repave asks Palette about the update, and Palette says no repave (#276)
#   3  the update pushed to cluster A without resolving its notification, and then accepted with
#      it: none waits after either, and the cluster runs the root of the pending record
#   4  an edit in place of a value of the release version: an update waits, and compare with
#      cluster:<uid>@pending names the value; the undo of the edit closes it with no update of the
#      cluster; then an apply of the baseline
#   5  a bump of the add-on version number alone, in place with the flag off: none waits; then
#      the version number back
#   6  one more kubeadm command in the Kubernetes pack of the release version: an update waits, and
#      compare --repave with cluster:<uid>@pending carries the answer of Palette, a repave for that
#      pack and that key; the undo of the edit closes the update, and cluster A takes none (#276)
#
# Needs the live example at its baseline with cluster A (just live-up), the strata release binary,
# jq, and the key as verify.sh needs it. Each stage undoes its change, so the script leaves the
# baseline. Results go under runs/<UTC time>/pending/, or OUT; FROM_STAGE resumes a run in OUT.
# Exit 0 only when no check fails.
set -uo pipefail
script=pending
from=${1:-1}
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
case "$from" in 1|2|3|4|5|6) ;; *) usage; exit 2 ;; esac
[ -x "$strata" ] || die "no strata binary at $strata"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/pending}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

summary() {
  write_summary "pending update test" ", cluster \`${C:-}\`," \
    "release \`${R:-}\`, add-on \`${AD:-}\`."
}
trap summary EXIT

load_outputs
C=$(out cluster_uid) R=$(out release_profile_uid) AD=$(out addon_profile_uid)
addon=$(out addon_profile_name)
[ -n "$C" ] || die "no cluster A; run just live-up"

# pending FILE: the updatesPending of the cluster record in FILE, as text
pending() { jq -r '.source.cluster | if has("updatesPending") then .updatesPending | tostring else "absent" end' "$1"; }
# record NAME [REF]: a record of cluster A, or of REF
record() { "$strata" snapshot "${2:-cluster:$C}" -o "$1.json"; }
# settled WANT: wait up to five minutes for cluster A to say WANT, true or false
settled() {
  for _ in $(seq 1 20); do
    "$strata" snapshot "cluster:$C" -o settle.json 2> /dev/null && [ "$(pending settle.json)" = "$1" ] && return 0
    sleep 15
  done
  return 1
}

if stage 1 "the baseline"; then
  require_baseline
  check base-record 0 'record cluster A at the baseline' -- record base
  check base-none 0 '#70: the record says that no update waits: updatesPending false' -- \
    test "$(pending base.json)" = false
  check base-profile-updates 0 '#70: profileUpdates lists each profile of the cluster also when none waits: it is no flag, so strata reads the notifications' -- \
    bash -c "\"$live/api.sh\" get /v1/spectroclusters/$C/profileUpdates | jq -e '.profiles | length > 0'"
  check base-at-pending 0 '#78: record cluster:<uid>@pending' -- record base-pending "cluster:$C@pending"
  check base-pending-same 0 '#78: with no update waiting, the pending record has the root of what the cluster runs' -- \
    test "$(jq -r .root base-pending.json)" = "$(jq -r .root base.json)"
  check base-pending-flag 0 '#78: the record says pending' -- jq -e '.source.cluster.pending == true' base-pending.json
  check base-consumers 1 '#78: consumers of the add-on version reads the list and each cluster' -- \
    "$strata" consumers "$AD" --json
  check base-list-only 1 '#78: consumers --list-only reads the list that Palette computes alone' -- \
    "$strata" consumers "$AD" --list-only --json
  check base-list-only-same 0 '#78: the list gives the same clusters as the walk, and says that the walk was not done' -- \
    bash -c "jq -e '.clustersRead.skipped == true' base-list-only.out && [ \"\$(jq -c '[.consumers[].uid] | sort' base-list-only.out)\" = \"\$(jq -c '[.consumers[].uid] | sort' base-consumers.out)\" ] && jq -e --arg c $C '[.consumers[].uid] | index(\$c)' base-list-only.out"
fi

if stage 2 "an edit of a manifest of the add-on version"; then
  check addon-edit 0 "#70: edit the manifest of $addon in place through the API" -- \
    "$live/api.sh" edit "$AD" "$addon_from" "$addon_to"
  check addon-pending 0 '#70: a record of cluster A says that an update waits' -- settled true
  check addon-record 0 'record cluster A' -- record addon
  check addon-names 0 '#70: pendingUpdates names the add-on profile, and the manifest that changed' -- \
    jq -e --arg u "$AD" '.source.cluster.pendingUpdates | length == 1 and .[0].profileUid == $u and (.[0].message | test("Manifest .* is updated"))' addon.json
  check addon-root 0 '#70: the root is the one at the baseline: the statement is outside the root, and the copy did not change' -- \
    test "$(jq -r .root addon.json)" = "$(jq -r .root base.json)"
  check addon-release-only 0 '#70: a record of the release profile alone says that no update waits for it' -- \
    bash -c "\"$strata\" snapshot cluster:$C/$(out profile_name) | jq -e '.source.cluster.updatesPending == false'"
  check addon-list 0 '#70: list clusters --applied marks the add-on as behind' -- \
    "$strata" list clusters --applied
  check addon-list-says 0 '#70: the line of the add-on says behind' -- \
    grep -Eq "^    $addon@[^ ]+ +$AD +add-on +behind: an update waits$" addon-list.out
  check addon-compare 1 "#70: compare of the add-on version with its copy on cluster A" -- \
    "$strata" compare "$AD" "cluster:$C/$addon"
  check addon-compare-says 0 '#70: compare says that cluster A is behind, and names the profile' -- \
    grep -q "^note       cluster .* is behind: Palette says an update of $addon waits for it, since " addon-compare.out
  check addon-at-pending 0 '#78: record cluster:<uid>@pending' -- record addon-pending "cluster:$C@pending"
  check addon-pending-root 0 '#78: the pending record has another root than the copy that runs' -- \
    test "$(jq -r .root addon-pending.json)" != "$(jq -r .root addon.json)"
  check addon-pending-compare 1 '#78: compare cluster:<uid> cluster:<uid>@pending says what the update changes' -- \
    "$strata" compare "cluster:$C" "cluster:$C@pending"
  check addon-pending-compare-says 0 '#78: it names the pack of the add-on as AUTHORED, and no pack of the release' -- \
    bash -c "grep -Eq '^~ .* AUTHORED' addon-pending-compare.out && [ \$(grep -c '^~ ' addon-pending-compare.out) = 1 ]"
  check addon-pending-verify 0 '#78: verify of the pending record, while the update still waits' -- \
    "$strata" verify addon-pending.json
  check addon-repave 1 '#276: compare cluster:<uid> cluster:<uid>@pending --repave asks Palette about the update that waits' -- \
    "$strata" compare "cluster:$C" "cluster:$C@pending" --repave
  check addon-repave-says 0 '#276: the palette line holds the answer of Palette for a manifest of the add-on: no repave' -- \
    grep -q '^palette    no repave$' addon-repave.out
  check addon-repave-json 0 '#276: --json carries paletteRepave for cluster A: required false, and no reason' -- \
    bash -c "\"$strata\" compare cluster:$C cluster:$C@pending --repave --json | jq -e '.paletteRepave | .required == false and .clusterUid == \"$C\" and ((.reasons // []) | length == 0)'"
fi

if stage 3 "the update accepted on cluster A"; then
  check accept 0 '#70: push the update to cluster A through the API, without resolving its notification' -- \
    "$live/api.sh" apply "$C" --keep-notification
  check accept-none 0 '#70: Palette closes the notification all the same when the copy takes the change: none waits' -- settled false
  check accept-record 0 'record cluster A' -- record accepted
  check accept-as-pending 0 '#78: the cluster now runs what the pending record held: the same root' -- \
    test "$(jq -r .root accepted.json)" = "$(jq -r .root addon-pending.json)"
  check accept-undo 0 "undo the edit of $addon" -- "$live/api.sh" edit "$AD" "$addon_to" "$addon_from"
  check accept-undo-pending 0 '#70: the undo is an edit too: an update waits' -- settled true
  check accept-again 0 'accept it, resolving the notification' -- "$live/api.sh" apply "$C"
  check accept-again-none 0 '#70: none waits' -- settled false
fi

if stage 4 "an edit of a value of the release version, and its undo"; then
  check values-edit 0 '#70: edit the reclaim policy of the CSI pack of the release version in place' -- \
    "$live/api.sh" edit "$R" "$(out csi_reclaim_delete)" "$(out csi_reclaim_delete | sed 's/Delete/Retain/')"
  check values-pending 0 '#70: an update waits' -- settled true
  check values-record 0 'record cluster A' -- record values
  check values-names 0 '#70: Palette names the pack whose values changed' -- \
    jq -e --arg u "$R" --arg csi "$(out csi_pack_name)" '.source.cluster.pendingUpdates | any(.profileUid == $u and (.message | test($csi + " .* values are updated")))' values.json
  check values-pending-compare 1 '#78: compare cluster:<uid> cluster:<uid>@pending' -- \
    "$strata" compare "cluster:$C" "cluster:$C@pending" --json
  check values-pending-compare-says 0 '#78: the one pack that changes is the CSI pack, and the path is the reclaim policy' -- \
    jq -e --arg csi "$(out csi_pack_name)" --arg key "$(out csi_next_key)" '[.packs[] | select(.status != "unchanged")] | length == 1 and .[0].name == $csi and (.[0].values | tostring | test($key))' values-pending-compare.out
  check values-undo 0 'put the value back through the API' -- \
    "$live/api.sh" edit "$R" "$(out csi_reclaim_delete | sed 's/Delete/Retain/')" "$(out csi_reclaim_delete)"
  check values-undo-none 0 '#70: the undo closes the update with no update of the cluster: none waits' -- settled false
  # the round trip through the API leaves the values text other than OpenTofu wrote it, so its
  # plan is not empty; the apply of the baseline writes the text again
  apply_baseline
  check values-baseline-none 0 '#70: the apply of the same content opens no update: none waits' -- settled false
fi

if stage 5 "a bump of the add-on version number alone"; then
  apply -var addon_version=1.0.1
  check bump-uid-kept 0 '#70: the bump with the flag off keeps the uid of the add-on' -- test "$(out addon_profile_uid)" = "$AD"
  sleep 20
  check bump-record 0 'record cluster A' -- record bump
  check bump-none 0 '#70: a new version number alone opens no update: none waits' -- test "$(pending bump.json)" = false
  apply_baseline
fi

if stage 6 "a kubeadm command in the Kubernetes pack of the release version"; then
  [ "$from" = 6 ] && require_baseline
  check k8s-edit 0 '#276: edit the Kubernetes pack of the release version in place: one more kubeadm command' -- \
    "$live/api.sh" edit "$R" "$kubeadm_from" "$kubeadm_to"
  check k8s-pending 0 '#276: an update waits' -- settled true
  check k8s-repave 1 '#276: compare cluster:<uid> cluster:<uid>@pending --repave: CHANGED' -- \
    "$strata" compare "cluster:$C" "cluster:$C@pending" --repave
  check k8s-repave-says 0 '#276: the palette line holds the answer of Palette: repave, the pack, and the key' -- \
    grep -qxF "$kubeadm_repave" k8s-repave.out
  check k8s-repave-json 0 '#276: --json carries paletteRepave for cluster A: required true, and the key in the reason' -- \
    bash -c "\"$strata\" compare cluster:$C cluster:$C@pending --repave --json | jq -e '.paletteRepave | .required == true and .clusterUid == \"$C\" and (.reasons[0].keys | index(\"kubeadmconfig.postKubeadmCommands\"))'"
  check k8s-undo 0 'put the command back through the API' -- "$live/api.sh" edit "$R" "$kubeadm_to" "$kubeadm_from"
  check k8s-undo-none 0 '#276: the undo closes the update with no update of the cluster: none waits' -- settled false
  apply_baseline
  check k8s-baseline-none 0 '#276: the apply of the baseline opens no update: none waits' -- settled false
fi

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]
