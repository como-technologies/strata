#!/usr/bin/env bash
# What the live example still has in its cloud, read-only: aws-inventory.sh (aws, eks) or
# gcp-inventory.sh (gcp, gke) for clusters A and B, in the cloud that terraform.tfvars names. It
# reads the variables and not the outputs, so it works after `just live-down`. Exit 0 when nothing
# is left, 1 while anything is, 2 when the cloud cannot be asked.
#
#   examples/live/inventory.sh
#   examples/live/inventory.sh --platform
#
# --platform reads wilma of platform.sh instead, on AWS. `platform.sh inventory` runs it so, with the
# state of terraform-platform/. It also counts what its workloads, its zone, and the policy of its nodes
# leave.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
case "${1:-}" in
  "") query='{cloud = var.cloud, region = var.region, project = var.gcp_project, a = var.cluster_name, b = var.second_cluster_name}' ;;
  --platform)
    query='{region = var.region, a = var.cluster_name, domain = var.domain, role = var.node_role_name, policy = var.node_policy_name}'
    ;;
  *) echo "inventory: the one option is --platform" >&2; exit 2 ;;
esac
vars=$(echo "$query" | "$here/tf.sh" console 2>&1) || { echo "inventory: cannot read the variables: $vars" >&2; exit 2; }
get() { sed -n "s/^ *\"*$1\"* *= *\"\(.*\)\"$/\1/p" <<<"$vars"; }
if [ -n "${1:-}" ]; then
  echo "== aws, cluster $(get a)"
  exec "$here/aws-inventory.sh" "$(get a)" "$(get region)" --kubernetes-tags --csi-volumes \
    --zone "$(get domain)" --role-policy "$(get role):$(get policy)"
fi
cloud=$(get cloud)

code=0
for name in "$(get a)" "$(get b)"; do
  echo "== $cloud, cluster $name"
  case "$cloud" in
    aws | eks) "$here/aws-inventory.sh" "$name" "$(get region)" ;;
    gcp) "$here/gcp-inventory.sh" "$(get project)" "$name" ;;
    gke) "$here/gcp-inventory.sh" "$(get project)" "$name" --gke ;;
    *) echo "inventory: no inventory for the cloud '$cloud'" >&2; exit 2 ;;
  esac
  c=$?
  [ "$c" -gt "$code" ] && code=$c
done
exit $code
