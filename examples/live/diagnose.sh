#!/usr/bin/env bash
# Diagnostics for every cluster in the live example's OpenTofu state, for a cluster that will not
# reach Running or will not finish deleting. Read-only.
#
#   examples/live/diagnose.sh [DIR]
#
# For each cluster, `tenant.sh diagnose` into DIR/<cluster name>/ (default
# runs/<UTC time>/diagnostics), and what the inventory of its cloud finds for it: `aws-inventory.sh`
# with AWS_PROFILE set, or `gcp-inventory.sh` with gcloud. in-place.sh and `just live-up` and `live-down` run it when an apply fails or a
# wait gives up. Exit 0 when every cluster was read, 2 when the state names none or one could not
# be read.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
dir=${1:-$here/runs/$(date -u +%Y%m%dT%H%M%SZ)/diagnostics}

state=$("$here/tf.sh" show -json 2>/dev/null) || { echo "diagnose: cannot read the OpenTofu state" >&2; exit 2; }
project=$(jq -r '.values.outputs.project_uid.value // empty' <<<"$state")
export PALETTE_PROJECT_UID=${PALETTE_PROJECT_UID:-$project}
clusters=$(jq -r '.values.root_module.resources[]?
  | select(.type == "spectrocloud_cluster_aws" or .type == "spectrocloud_cluster_gcp")
  | [.values.id, .values.name, (.type | ltrimstr("spectrocloud_cluster_")), .values.cloud_config[0].region, .values.cloud_config[0].project // "-"]
  | join(" ")' <<<"$state")
[ -n "$clusters" ] || { echo "diagnose: the OpenTofu state names no cluster" >&2; exit 2; }

code=0
while read -r uid name cloud region project; do
  "$here/tenant.sh" diagnose "$uid" "$dir/$name" || code=2
  if [ "$cloud" = aws ] && [ -n "${AWS_PROFILE:-}" ] && command -v aws >/dev/null; then
    "$here/aws-inventory.sh" "$name" "$region" > "$dir/$name/aws.txt" 2>&1
  elif [ "$cloud" = gcp ] && command -v gcloud >/dev/null; then
    "$here/gcp-inventory.sh" "$project" "$name" > "$dir/$name/gcp.txt" 2>&1
  fi
done <<<"$clusters"
exit $code
