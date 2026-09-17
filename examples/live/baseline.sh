#!/usr/bin/env bash
# Check that the live example is at its baseline, the state `just live-up` builds. Read-only.
#
#   examples/live/baseline.sh
#
#   plan          an OpenTofu plan with the default variables changes nothing
#   cluster-a     cluster A is Running
#   no-cluster-b  no cluster by cluster B's name exists
#   versions      each profile name holds exactly the versions OpenTofu manages
#   runs-release  cluster A runs the release profile's current content
#   runs-addon    cluster A runs the add-on profile's current content
#
# verify.sh, in-place.sh, and registry-move.sh run it first and refuse to start from anything
# else. Prints one line per condition. Exit 0 at the baseline, 1 when a condition fails, 2 when it
# cannot tell.
set -uo pipefail
script=baseline
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"

[ -x "$strata" ] || die "no strata binary at $strata"
load_outputs
PALETTE_PROJECT_UID=$(out project_uid)
export PALETTE_PROJECT_UID STRATA_QUIET=1
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

differs=0
say() { # ok|differs NAME DETAIL
  printf '%-8s %-13s %s\n' "$1" "$2" "$3"
  [ "$1" = ok ] || differs=1
}

tf_plan false > "$tmp/plan" 2>&1
case $? in
  0) say ok plan "no changes with the default variables" ;;
  2) say differs plan "$(command grep -E '^\s+# .* (will|must) be' "$tmp/plan" | sed 's/^ *# //' | paste -sd ';' -)" ;;
  *) die "tofu plan failed: $(tail -3 "$tmp/plan")" ;;
esac

C=$(out cluster_uid)
state=$("$live/tenant.sh" cluster "$C" 2>&1)
case "$state" in
  *": Running") say ok cluster-a "$state" ;;
  *) say differs cluster-a "$(tr '\n' ';' <<<"$state")" ;;
esac

B_NAME=$(out cluster_names | jq -r .b)
"$strata" list clusters --json > "$tmp/clusters" 2> "$tmp/clusters.err" || die "strata list clusters failed: $(cat "$tmp/clusters.err")"
b=$(jq -r --arg n "$B_NAME" '[.clusters[] | select(.name == $n) | "\(.uid) \(.state // "")"] | join(", ")' "$tmp/clusters")
if [ -z "$b" ]; then say ok no-cluster-b "no cluster named $B_NAME"; else say differs no-cluster-b "$B_NAME exists: $b"; fi

"$live/tf.sh" show -json > "$tmp/state" 2>/dev/null || die "cannot read the OpenTofu state"
"$strata" list profiles --json > "$tmp/profiles" 2> "$tmp/profiles.err" || die "strata list profiles failed: $(cat "$tmp/profiles.err")"
for name in $(out profile_names | jq -r '.[]'); do
  managed=$(jq -r --arg n "$name" '[.values.root_module.resources[]? | select(.type == "spectrocloud_cluster_profile" and .values.name == $n) | .values.id] | sort | join(" ")' "$tmp/state")
  live_uids=$(jq -r --arg n "$name" '[.profiles[] | select(.name == $n) | .uid] | sort | join(" ")' "$tmp/profiles")
  if [ "$managed" = "$live_uids" ]; then
    say ok versions "$name: ${live_uids:-none}"
  else
    say differs versions "$name: Palette has ${live_uids:-none}, OpenTofu manages ${managed:-none}"
  fi
done

for pair in "runs-release release_profile_uid profile_name" "runs-addon addon_profile_uid addon_profile_name"; do
  set -- $pair
  if "$strata" compare "$(out "$2")" "cluster:$C/$(out "$3")" > "$tmp/$1" 2>&1; then
    say ok "$1" "$(out "$3") is IDENTICAL on cluster A"
  else
    say differs "$1" "$(sed -n 2p "$tmp/$1")"
  fi
done

exit $differs
