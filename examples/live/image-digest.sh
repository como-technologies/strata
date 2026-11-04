#!/usr/bin/env bash
# The image digest test (#630): what a cluster pulls when only the image list names a digest, and
# when the workload reads one.
#
#   examples/live/image-digest.sh [FROM_STAGE]
#
#   1  at the baseline: the digest that the registry serves for the image, and a record of the
#      add-on version
#   2  the add-on manifest makes a Deployment that names the image by its tag. The values of the
#      pack list the image with its digest, in pack.content.images. A record of the version holds
#      the digest, and compare names the list. Cluster A takes the update. The result is the image
#      field of the Deployment inside the cluster, and the image of its pod
#   3  the manifest names the image with its digest, where the workload reads it. Cluster A takes
#      the update. The result is the same two images
#   4  undo: the manifest and the values back, cluster A takes them, and the Deployment is gone
#
# What varies: the place of the digest, in the list of the values or in the manifest. What holds: the
# image, the cluster, and the baseline that the run starts from and returns to (lib.sh). The
# manifest stands for a chart: the example has no chart that cluster A runs. The workload reads the
# manifest, as it reads the values of a chart. A check with `?` records a result without judging
# it, because the run finds the result.
#
# Needs the live example at its baseline with cluster A (just live-up), the strata release binary,
# jq, kubectl, crane, and the key of verify.sh. The kubeconfig lives in a temporary file and is
# never saved. The nodes need egress to the registry of the image. Results go under
# runs/<UTC time>/image-digest/, or OUT; FROM_STAGE resumes a run in OUT. Exit 0 only when no check
# fails.
set -uo pipefail
script=image-digest
from=${1:-1}
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
case "$from" in 1|2|3|4) ;; *) usage; exit 2 ;; esac
[ -x "$strata" ] || die "no strata binary at $strata"
command -v kubectl > /dev/null || die "kubectl is needed to read the image inside the cluster"
command -v crane > /dev/null || die "crane is needed to read the digest of the image"

image=registry.k8s.io/pause:3.10.2
pack=strata-live-config
deploy=strata-live-image
minutes=${PROPAGATION_MINUTES:-20}
digest=$(crane digest "$image") || die "crane cannot read the digest of $image"
case "$digest" in sha256:*) ;; *) die "crane gave '$digest' for $image, not a digest" ;; esac

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/image-digest}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

summary() {
  write_summary "image digest test" ", cluster \`${C:-}\`," \
    "add-on \`${AD:-}\`, image \`$image\`, digest \`$digest\`."
}
trap summary EXIT

load_outputs
C=$(out cluster_uid) AD=$(out addon_profile_uid) P=$(out project_uid)
addon=$(out addon_profile_name)
[ -n "$C" ] || die "no cluster A; run just live-up"
export PALETTE_PROJECT_UID=$P

# workload IMAGE: the document that the add-on manifest gets, a Deployment of one pod
workload() {
  cat << EOF
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: $deploy
  namespace: default
spec:
  replicas: 1
  selector:
    matchLabels:
      app: $deploy
  template:
    metadata:
      labels:
        app: $deploy
    spec:
      containers:
        - name: pause
          image: $1
          imagePullPolicy: IfNotPresent
EOF
}
# the manifest text after stage 2, where the Deployment names the tag, and after stage 3, where it
# names the digest
with_tag="$addon_from"$'\n'"$(workload "$image")"
with_digest="$addon_from"$'\n'"$(workload "$image@$digest")"
# list_values IMAGE: the values of the pack, with IMAGE in the list pack.content.images
list_values() { printf 'pack:\n  content:\n    images:\n      - image: %s\n' "$1"; }

# spec_image: the image field of the Deployment, read inside the cluster
spec_image() { kube "$C" get deployment "$deploy" -n default -o jsonpath='{.spec.template.spec.containers[0].image}'; }
# pod_image_id: the image that the newest pod of the Deployment runs, as the node reports it
pod_image_id() {
  kube "$C" get pods -n default -l "app=$deploy" --sort-by=.metadata.creationTimestamp \
    -o jsonpath='{.items[-1:].status.containerStatuses[0].imageID}'
}
# wait_image PATTERN: wait up to $minutes minutes until the image field of the Deployment matches the
# shell pattern PATTERN. The log has one line for each look.
wait_image() {
  local seen=""
  for _ in $(seq 1 $((minutes * 2))); do
    seen=$(spec_image)
    echo "$(date -u +%H:%M:%S) image field: $seen" >> propagation.log
    # shellcheck disable=SC2254
    case "$seen" in $1) return 0 ;; esac
    sleep 30
  done
  return 1
}
# wait_pod: wait up to $minutes minutes until the pod of the Deployment reports an image
wait_pod() {
  local seen=""
  for _ in $(seq 1 $((minutes * 2))); do
    seen=$(pod_image_id)
    echo "$(date -u +%H:%M:%S) pod image: $seen" >> propagation.log
    case "$seen" in *sha256:*) return 0 ;; esac
    sleep 30
  done
  return 1
}
# wait_gone: wait up to $minutes minutes until the Deployment is not in the cluster
wait_gone() {
  for _ in $(seq 1 $((minutes * 2))); do
    kube "$C" get deployment "$deploy" -n default > /dev/null 2>&1 || return 0
    sleep 30
  done
  return 1
}
# pod_after_wait: the image of the pod, when the pod reports one within the time
pod_after_wait() { wait_pod && pod_image_id; }
# deployment_absent: cluster A has no Deployment of the example
deployment_absent() { ! kube "$C" get deployment "$deploy" -n default > /dev/null 2>&1; }
# names_a_value_alone: compare names a changed value, and no change of the image list
names_a_value_alone() { grep -q AUTHORED digest-compare.out && ! grep -q IMAGES digest-compare.out; }
# has_digest TEXT: succeeds when TEXT names an image by a digest
has_digest() { case "$1" in *@sha256:*) return 0 ;; *) return 1 ;; esac; }
# undo_manifest: the manifest of the add-on back. The run can stop after stage 2, so the manifest can
# hold the tag and not the digest.
undo_manifest() {
  "$live/api.sh" edit "$AD" "$with_digest" "$addon_from" 2> /dev/null || "$live/api.sh" edit "$AD" "$with_tag" "$addon_from"
}

if stage 1 "the baseline"; then
  require_baseline
  check base-record 0 "record $addon" -- "$strata" snapshot "$AD" -o addon-0.json
  check base-no-deployment 0 "#630: cluster A runs no Deployment $deploy at the baseline" -- deployment_absent
fi

if stage 2 "the digest in the list alone"; then
  check list-values 0 "#630: the values of the pack list $image with its digest" -- \
    "$live/api.sh" set-values "$AD" "$pack" "$(list_values "$image@$digest")"
  check list-manifest 0 "#630: the manifest of $addon makes a Deployment that names the tag" -- \
    "$live/api.sh" edit "$AD" "$addon_from" "$with_tag"
  check list-record 0 "record $addon" -- "$strata" snapshot "$AD" -o addon-list.json
  check list-record-digest 0 "#630: the record holds the digest in the image list of the pack" -- \
    jq -e --arg p "$pack" --arg i "$image@$digest" '[.packs[] | select(.name == $p) | .images[]?] | index($i) != null' addon-list.json
  check list-compare 1 "#630: compare of the baseline record with the version differs" -- \
    "$strata" compare addon-0.json "$AD"
  check list-compare-flag 0 "#630: compare names the list: IMAGES" -- grep -q IMAGES list-compare.out
  check list-accept 0 '#630: cluster A takes the update' -- "$live/api.sh" apply "$C"
  check list-deployed 0 "#630: the Deployment is in cluster A within $minutes minutes (propagation.log)" -- \
    wait_image "${image%:*}*"
  check list-spec ? '#630: the image field of the Deployment' -- spec_image
  check list-spec-digest ? '#630: the field holds the digest (0), or only the tag (1)' -- has_digest "$(cat list-spec.out)"
  check list-pod ? "#630: the image of the pod, as the node reports it, within $minutes minutes" -- pod_after_wait
fi

if stage 3 "the digest where the manifest reads it"; then
  check digest-manifest 0 "#630: the manifest names the image with its digest" -- \
    "$live/api.sh" edit "$AD" $'image: '"$image"$'\n' $'image: '"$image@$digest"$'\n'
  check digest-record 0 "record $addon" -- "$strata" snapshot "$AD" -o addon-digest.json
  check digest-compare 1 "#630: compare of the record of stage 2 with the version differs" -- \
    "$strata" compare addon-list.json "$AD"
  check digest-compare-flag 0 "#630: compare names a changed value, and no change of the list" -- names_a_value_alone
  check digest-accept 0 '#630: cluster A takes the update' -- "$live/api.sh" apply "$C"
  check digest-deployed 0 "#630: the image field of the Deployment holds the digest within $minutes minutes (propagation.log)" -- \
    wait_image "*@sha256:*"
  check digest-spec ? '#630: the image field of the Deployment' -- spec_image
  check digest-pod ? "#630: the image of the pod, as the node reports it, within $minutes minutes" -- pod_after_wait
fi

if stage 4 "undo"; then
  check undo-manifest 0 "the manifest of $addon back" -- undo_manifest
  check undo-values 0 "the values of the pack back" -- "$live/api.sh" set-values "$AD" "$pack" ""
  check undo-accept 0 'cluster A takes it' -- "$live/api.sh" apply "$C"
  check undo-gone 0 "#630: the Deployment is gone from cluster A within $minutes minutes" -- wait_gone
  check undo-baseline 0 'the example is at its baseline' -- "$live/baseline.sh"
fi

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]
