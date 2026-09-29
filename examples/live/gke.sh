#!/usr/bin/env bash
# The managed engine test on GKE (#90): the claims that depend on the engine, on a GKE cluster.
# Google runs the control plane, and a node pool runs the workers.
#
#   examples/live/gke.sh
#
#   1  the GKE release version and cluster A: a record of each, their layers and digests, and
#      compare of the two
#   2  the shape tier of cluster A: its node pool
#   3  what Palette answers to compare --repave, beside the level of the impact table: a
#      Kubernetes version bump, and a change of the values of each infrastructure layer
#   4  with GKE_POOL_CHANGE=1 only: a change of the node pool, a new machine type, applied; the
#      shape tier before and after, and the GKE node pools. Palette deletes the GKE node pool and
#      makes none: the cluster keeps no node, and must be deleted. See "A GKE cluster" in
#      live-example.md
#
# Needs the live example with cloud = "gke" in terraform.tfvars and cluster A (just live-up), the
# strata release binary, jq, gcloud with read access to the project, and the key as verify.sh
# needs it. Results go under runs/<UTC time>/gke/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=gke
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/gke}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

load_outputs
[ "$(sed -n 's/^cloud *= *"\(.*\)"/\1/p' "$live/terraform/terraform.tfvars")" = gke ] \
  || die "set cloud = \"gke\" in terraform/terraform.tfvars, and run just live-up"
C=$(out cluster_uid)
[ -n "$C" ] || die "no cluster A; run just live-up"
export PALETTE_PROJECT_UID=$(out project_uid)
# the provider ends its create before the cluster is Running
wait_running "$C"
R=$(out release_profile_uid)
NAME=$(out profile_name)
gcp_project=$(sed -n 's/^gcp_project *= *"\(.*\)"/\1/p' "$live/terraform/terraform.tfvars")
gcp_region=$(sed -n 's/^gcp_region *= *"\(.*\)"/\1/p' "$live/terraform/terraform.tfvars")
gcp_region=${gcp_region:-us-east1}

summary() {
  {
    echo "# managed engine test: GKE"
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
check release 0 "record $NAME@1.0.0, a GKE profile" -- "$strata" snapshot "$R" -o release.json
check release-cloud 0 '#90: the record is of cloud type gke' -- jq -e '.source.cloudType == "gke"' release.json
check release-layers 0 '#90: its packs, in order: an OS, Kubernetes, a CNI, and a CSI layer' -- \
  jq -e '[.packs[].layer] == ["os", "k8s", "cni", "csi"]' release.json
check release-packs 0 '#90: the name, the version, and the upstream source of the digest of each pack' -- \
  jq -r '.packs[] | "\(.layer) \(.name) \(.version) \(.hash.upstreamSource)"' release.json
check release-digests 0 '#90: each pack has an upstream digest: packRef.digest' -- \
  jq -e '[.packs[] | .hash.upstreamSource] | all(. != "none")' release.json
check cluster 0 'record cluster A' -- "$strata" snapshot "cluster:$C" -o cluster.json
check cluster-runs 0 '#90: cluster A runs the recorded release: IDENTICAL' -- \
  "$strata" compare release.json "cluster:$C/$NAME"
check verify-both 0 '#90: both records verify' -- "$strata" verify release.json cluster.json
check consumers 1 '#90: consumers names cluster A' -- "$strata" consumers "$R"

echo "== 2. the shape tier" | tee -a steps.log
check shape 0 'record cluster A with its shape tier' -- "$strata" snapshot "cluster:$C" --shape -o shape-1.json
check shape-pool 0 '#90: the shape tier holds the node pool' -- \
  jq -e '.source.cluster.tiers.shape.fields.pools | map(.name) | index("worker-pool") != null' shape-1.json
check shape-fields 0 '#90: the fields of the node pool in the shape tier' -- \
  jq -c '.source.cluster.tiers.shape.fields' shape-1.json
check shape-control-plane 0 '#90: the shape tier holds the managed cluster settings and the release channel of the override' -- \
  jq -e '.source.cluster.tiers.shape.fields.cloud | (.managedClusterConfig.location | length > 0)
         and (.overrideClusterAPIConfig | test("releaseChannel: regular"))' shape-1.json
nodes=$(instances "$C")
check instances 0 '#90: the live helper lists the machines of cluster A' -- test -n "$nodes"

echo "== 3. the answer of Palette to a repave question" | tee -a steps.log
for layer in os k8s cni csi; do
  pack=$(pack_of release.json "$layer")
  first=$(values_line release.json "$layer")
  if [ -z "$first" ]; then
    printf '%-4s  %-32s %s\n' SEEN "value-$layer" "the $layer pack $pack has no values to change" | tee -a steps.log
    printf '%s\t%s\n' "value-$layer" "the $layer pack has no values" >> checks.tsv
    continue
  fi
  check "value-$layer" ? "#90: a new key in the values of the $layer pack $pack: Palette answers (1 repave, 0 no repave)" -- \
    "$live/tenant.sh" repave "$C" "$R" "$R" "$pack" "$first" "strataRepaveTest: \"true\"
$first"
done
bump=$(out k8s_bump_profile_uid)
if [ -n "$bump" ]; then
  check k8s-bump 1 '#90: compare --repave of cluster A with the release at another Kubernetes version' -- \
    "$strata" compare "cluster:$C/$NAME" "$bump" --repave
  check k8s-bump-lines 0 'the palette line and the impact of the Kubernetes pack' -- \
    grep -E '^(palette|~ \[k8s|      impact|note )' k8s-bump.out
  check k8s-bump-repave 0 '#90: Palette says repave for a Kubernetes version bump on GKE, and the table agrees' -- \
    bash -c "grep -q '^palette    repave: PackUpgraded kubernetes-gke' k8s-bump.out && ! grep -q 'the impact table says' k8s-bump.out"
else
  printf '%-4s  %-32s %s\n' SEEN k8s-bump "no gke_k8s_bump: apply with -var gke_k8s_bump=<version> for this question" | tee -a steps.log
  printf '%s\t%s\n' k8s-bump "no gke_k8s_bump" >> checks.tsv
fi

echo "== 4. a change of the node pool" | tee -a steps.log
if [ "${GKE_POOL_CHANGE:-}" != 1 ]; then
  printf '%-4s  %-32s %s\n' SEEN pool-change "skipped: GKE_POOL_CHANGE=1 runs it, and leaves a cluster with no node" | tee -a steps.log
  printf '%s\t%s\n' pool-change "skipped without GKE_POOL_CHANGE=1" >> checks.tsv
  check baseline 0 'the example is at its baseline' -- "$live/baseline.sh"
  echo
  echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
  [ "$failures" = 0 ]
  exit
fi
pools() {
  gcloud container node-pools list --cluster "$(out cluster_names | jq -r .a)" --region "$gcp_region" \
    --project "$gcp_project" --format='value(name,config.machineType,status)'
}
check nodepools-before 0 '#90: the GKE node pool before the change, from gcloud' -- pools
check pool-change 0 '#90: the node pool gets another machine type, through OpenTofu' -- \
  "$live/tf.sh" apply -input=false -auto-approve -no-color -var gke_instance_type=e2-standard-8
# Palette gives the new machine pool the GKE node pool name of the old one; the delete of the old
# one removes the GKE node pool, and the controllers that would make the new one ran on its nodes
for _ in $(seq 1 10); do
  [ -z "$(pools 2>/dev/null)" ] && break
  sleep 30
done
check nodepools-gone 0 '#90: GKE has no node pool within 5 minutes of the change' -- test -z "$(pools)"
check shape-2 0 'record cluster A with its shape tier after the change' -- "$strata" snapshot "cluster:$C" --shape -o shape-2.json
check shape-differs 0 '#90: compare of the two records names the change of the shape tier; a tier does not change the exit' -- \
  "$strata" compare shape-1.json shape-2.json
check shape-differs-says 0 '#90: the shape tier is CHANGED' -- grep -q 'shape CHANGED' shape-differs.out
sleep 600
check nodepools-none-made 0 '#90: 10 minutes later GKE still has no node pool: nothing makes the new one' -- test -z "$(pools)"
echo "$script: cluster A has no node now; delete it with just live-down" | tee -a steps.log
echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]
