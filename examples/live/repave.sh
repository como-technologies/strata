#!/usr/bin/env bash
# The repave test (#73): `compare --repave` asks Palette if a change repaves
# cluster A, and prints the answer beside the level of the impact table.
#
#   examples/live/repave.sh [FROM_STAGE]
#
#   1  at the baseline: a profile version equal to the copy of cluster A: Palette says no repave
#   2  the Kubernetes pack of the release version gets one more kubeadm command: Palette says
#      repave, for that pack and that key, and the impact table says it may repave
#   3  the same change, asked with tenant.sh: strata and the helper give one answer
#   4  a manifest of the add-on version changes: Palette says no repave, and the table says
#      in-place
#   5  a side that is not a cluster is a usage error, and the request changes nothing: cluster A
#      keeps its instances
#
# AWS and GCP run one upgrade engine, so every answer here agrees with the table. A disagreement
# needs another engine, such as Edge, and no live run has one. Needs the live example at its baseline with
# cluster A (just live-up), the strata release binary, jq, and the key as verify.sh needs it. Each
# stage puts its change back and takes no update on cluster A. Results go under
# runs/<UTC time>/repave/, or OUT; FROM_STAGE resumes a run in OUT. Exit 0 only when no
# check fails.
set -uo pipefail
script=repave
from=${1:-1}
case "$from" in 1|2|3|4|5) ;; *) sed -n '2,/^set -uo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 2 ;; esac
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"

kubeadm_from=$'    - \'chmod 600 /var/lib/kubelet/config.yaml\'\n'
kubeadm_to=$'    - \'chmod 600 /var/lib/kubelet/config.yaml\'\n    - \'echo strata-repave-test\'\n'
addon_from='edited: "no"'
addon_to='edited: "yes"'

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/repave}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

summary() {
  {
    echo "# Palette repave test"
    echo
    echo "Run \`$(basename "$(dirname "$run")")\` with \`$("$strata" --version)\`, cluster \`${C:-}\`,"
    echo "release \`${R:-}\`, add-on \`${AD:-}\`."
    echo
    summary_table
  } > summary.md
}
trap summary EXIT

stage() { [ "$from" -le "$1" ] && echo "== stage $1: $2" | tee -a steps.log; }

load_outputs
C=$(out cluster_uid) R=$(out release_profile_uid) AD=$(out addon_profile_uid)
CN=$(out cluster_names | jq -r .a)
[ -n "$C" ] || die "no cluster A; run just live-up"
export PALETTE_PROJECT_UID=$(out project_uid)
release=$(out profile_name)
addon=$(out addon_profile_name)

if stage 1 "no change"; then
  require_baseline
  check none 0 "#73: a profile version equal to the copy of cluster A: the comparison is IDENTICAL, and Palette says no repave" -- \
    "$strata" compare "cluster:$C/$release" "$R" --repave
  check none-says 0 "#73: the output has a palette line: no repave" -- grep -q '^palette    no repave$' none.out
  check none-json 0 "#73: --json carries paletteRepave, required false, and agrees true" -- \
    bash -c "\"$strata\" compare cluster:$C/$release $R --repave --json | jq -e '.paletteRepave | .required == false and .agrees == true and .clusterUid == \"$C\"'"
fi

if stage 2 "a kubeadm command in the Kubernetes pack"; then
  check k8s-edit 0 "the Kubernetes pack of $release gets one more kubeadm command, through the API" -- \
    "$live/api.sh" edit "$R" "$kubeadm_from" "$kubeadm_to"
  check k8s 1 "#73: compare of the copy with the edited version: CHANGED, and Palette says repave" -- \
    "$strata" compare "cluster:$C/$release" "$R" --repave
  check k8s-says 0 "#73: Palette names the pack, the reason, and the key" -- \
    grep -q '^palette    repave: PackValuesUpdated kubernetes (kubeadmconfig.postKubeadmCommands)$' k8s.out
  check k8s-agrees 0 "#73: the impact table says may-repave, and there is no disagreement note" -- \
    bash -c "grep -q 'impact: .*may-repave\|impact: .*repave' k8s.out; ! grep -q 'the impact table says' k8s.out"
  check k8s-json 0 "#73: --json carries the reason, with its key" -- \
    bash -c "\"$strata\" compare cluster:$C/$release $R --repave --json | jq -e '.paletteRepave | .required == true and .agrees == true and (.reasons[0].keys | index(\"kubeadmconfig.postKubeadmCommands\"))'"
  check k8s-reversed 1 "#73: the two sides in the other order give the same answer" -- \
    "$strata" compare "$R" "cluster:$C/$release" --repave
  check k8s-reversed-says 0 "and the same line" -- \
    grep -q '^palette    repave: PackValuesUpdated kubernetes' k8s-reversed.out
fi

if stage 3 "the helper gives the same answer"; then
  check helper 1 "#73: tenant.sh asks Palette for the same change: repave" -- \
    "$live/tenant.sh" repave "$C" "$R" "$R" kubernetes "$kubeadm_from" "$kubeadm_to"
  check helper-says 0 "#73: and names the same key" -- grep -q 'kubeadmconfig.postKubeadmCommands' helper.out
  check k8s-undo 0 "the Kubernetes pack back" -- "$live/api.sh" edit "$R" "$kubeadm_to" "$kubeadm_from"
fi

if stage 4 "a manifest of the add-on"; then
  check addon-edit 0 "the manifest of $addon changes, through the API" -- "$live/api.sh" edit "$AD" "$addon_from" "$addon_to"
  check addon 1 "#73: compare of the copy with the edited add-on: CHANGED, and Palette says no repave" -- \
    "$strata" compare "cluster:$C/$addon" "$AD" --repave
  check addon-says 0 "#73: the palette line says no repave" -- grep -q '^palette    no repave$' addon.out
  check addon-agrees 0 "#73: the table says in-place, and there is no disagreement note" -- \
    bash -c "grep -q 'in-place' addon.out; ! grep -q 'the impact table says' addon.out"
  check addon-undo 0 "the manifest back" -- "$live/api.sh" edit "$AD" "$addon_to" "$addon_from"
fi

if stage 5 "what the option refuses, and what it changes"; then
  before=$(instances)
  check usage 2 "#73: two profile versions and no cluster: a usage error" -- \
    "$strata" compare "$R" "$AD" --repave
  check usage-says 0 "and it says what it needs" -- grep -q 'needs one side that is a cluster' usage.err
  check no-update 0 "the questions took no update: cluster A has no update waiting" -- \
    bash -c "\"$strata\" snapshot cluster:$C | jq -e '.source.cluster.updatesPending == false'"
  after=$(instances)
  check same-nodes 0 "#73: the same machines after each question: nothing was repaved" -- test -n "$before" -a "$before" = "$after"
  check undo-baseline 0 "the example is at its baseline" -- "$live/baseline.sh"
fi

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]
