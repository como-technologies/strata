#!/usr/bin/env bash
# The managed engine test (#89): the claims that depend on the engine, on an EKS cluster. Amazon
# runs the control plane, and a managed node group runs the workers.
#
#   examples/live/eks.sh
#
#   1  the EKS release version and cluster A: a record of each, their layers and digests, and
#      compare of the two
#   2  the shape tier of cluster A: its managed node group
#   3  what Palette answers to compare --repave, beside the level of the impact table: a
#      Kubernetes version bump, and a change of the values of each infrastructure layer
#   4  a change of the node pool: a new instance type, applied; the shape tier and the machines
#      before and after; then the pool back
#
# Needs the live example with cloud = "eks" in terraform.tfvars and cluster A (just live-up), the
# strata release binary, jq, and the key as verify.sh needs it. Step 4 applies twice. Results go
# under runs/<UTC time>/eks/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=eks
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/eks}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

load_outputs
[ "$(sed -n 's/^cloud *= *"\(.*\)"/\1/p' "$live/terraform/terraform.tfvars")" = eks ] \
  || die "set cloud = \"eks\" in terraform/terraform.tfvars, and run just live-up"
C=$(out cluster_uid)
[ -n "$C" ] || die "no cluster A; run just live-up"
export PALETTE_PROJECT_UID=$(out project_uid)
R=$(out release_profile_uid)
NAME=$(out profile_name)

summary() {
  {
    echo "# managed engine test: EKS"
    echo
    echo "Run \`$(basename "$(dirname "$run")")\` with \`$("$strata" --version)\`, cluster \`$C\`, release \`$R\`."
    echo
    summary_table
  } > summary.md
}
trap summary EXIT

# values_line FILE LAYER: the first line of the values of the pack of LAYER in a record
values_line() { jq -r --arg l "$2" '[.packs[] | select(.layer == $l)][0].values' "$1" | grep -v '^\s*#' | grep -m1 '[a-zA-Z]'; }
pack_of() { jq -r --arg l "$2" '[.packs[] | select(.layer == $l)][0].name' "$1"; }

echo "== 1. the release version and cluster A" | tee -a steps.log
check release 0 "record $NAME@1.0.0, an EKS profile" -- "$strata" snapshot "$R" -o release.json
check release-cloud 0 '#89: the record is of cloud type eks' -- jq -e '.source.cloudType == "eks"' release.json
check release-layers 0 '#89: its packs, in order: an OS, Kubernetes, a CNI, and a CSI layer' -- \
  jq -e '[.packs[].layer] == ["os", "k8s", "cni", "csi"]' release.json
check release-packs ? '#89: the name, the version, and the upstream source of the digest of each pack' -- \
  jq -r '.packs[] | "\(.layer) \(.name) \(.version) \(.hash.upstreamSource)"' release.json
check release-digests ? '#89: each pack has an upstream digest' -- \
  jq -e '[.packs[] | .hash.upstreamSource] | all(. != "none")' release.json
check cluster 0 'record cluster A' -- "$strata" snapshot "cluster:$C" -o cluster.json
check cluster-runs 0 '#89: cluster A runs the recorded release: IDENTICAL' -- \
  "$strata" compare release.json "cluster:$C/$NAME"
check verify-both 0 '#89: both records verify' -- "$strata" verify release.json cluster.json
check consumers 1 '#89: consumers names cluster A' -- "$strata" consumers "$R"

echo "== 2. the shape tier" | tee -a steps.log
check shape 0 'record cluster A with its shape tier' -- "$strata" snapshot "cluster:$C" --shape -o shape-1.json
check shape-pool 0 '#89: the shape tier holds the managed node group' -- \
  jq -e '.source.cluster.tiers.shape.fields.pools | map(.name) | index("worker-pool") != null' shape-1.json
check shape-fields ? '#89: the fields of the node group in the shape tier' -- \
  jq -c '.source.cluster.tiers.shape.fields' shape-1.json
check instances ? '#89: the machines of cluster A, as the live helper counts them' -- instances "$C"

echo "== 3. the answer of Palette to a repave question" | tee -a steps.log
for layer in os k8s cni csi; do
  pack=$(pack_of release.json "$layer")
  first=$(values_line release.json "$layer")
  if [ -z "$first" ]; then
    printf '%-4s  %-32s %s\n' SEEN "value-$layer" "the $layer pack $pack has no values to change" | tee -a steps.log
    printf '%s\t%s\n' "value-$layer" "the $layer pack has no values" >> checks.tsv
    continue
  fi
  check "value-$layer" ? "#89: a new key in the values of the $layer pack $pack: Palette answers (1 repave, 0 no repave)" -- \
    "$live/tenant.sh" repave "$C" "$R" "$R" "$pack" "$first" "strataRepaveTest: \"true\"
$first"
done
bump=$(out k8s_bump_profile_uid)
if [ -n "$bump" ]; then
  check k8s-bump ? '#89: compare --repave of cluster A with the release at another Kubernetes version' -- \
    "$strata" compare "cluster:$C/$NAME" "$bump" --repave
  check k8s-bump-lines 0 'the palette line and the impact of the Kubernetes pack' -- \
    grep -E '^(palette|~ \[k8s|      impact|note )' k8s-bump.out
else
  printf '%-4s  %-32s %s\n' SEEN k8s-bump "no eks_k8s_bump: apply with -var eks_k8s_bump=<version> for this question" | tee -a steps.log
  printf '%s\t%s\n' k8s-bump "no eks_k8s_bump" >> checks.tsv
fi

echo "== 4. a change of the node pool" | tee -a steps.log
before=$(instances "$C")
check pool-change 0 '#89: the node group gets another instance type, through OpenTofu' -- \
  "$live/tf.sh" apply -input=false -auto-approve -no-color -var eks_instance_type=m5.xlarge
wait_running "$C"
after=$(instances "$C")
check pool-machines ? '#89: the machines after the change: new names mean a roll of the node group' -- \
  test "$before" != "$after"
check shape-2 0 'record cluster A with its shape tier after the change' -- "$strata" snapshot "cluster:$C" --shape -o shape-2.json
check shape-differs 1 '#89: compare of the two records names the change of the shape tier' -- \
  "$strata" compare shape-1.json shape-2.json
check shape-differs-says 0 '#89: the shape tier is CHANGED' -- grep -q 'shape CHANGED' shape-differs.out
check pool-back 0 'the node group back' -- "$live/tf.sh" apply -input=false -auto-approve -no-color
wait_running "$C"
check baseline 0 'the example is at its baseline' -- "$live/baseline.sh"

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]
