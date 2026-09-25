#!/usr/bin/env bash
# What the live example still has in its cloud, read-only: aws-inventory.sh or gcp-inventory.sh for
# clusters A and B, in the cloud that terraform.tfvars names. It reads the variables and not the
# outputs, so it works after `just live-down`. Exit 0 when nothing is left, 1 while anything is, 2
# when the cloud cannot be asked.
#
#   examples/live/inventory.sh
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
vars=$(echo '{cloud = var.cloud, region = var.region, project = var.gcp_project, a = var.cluster_name, b = var.second_cluster_name}' \
  | "$here/tf.sh" console 2>&1) || { echo "inventory: cannot read the variables: $vars" >&2; exit 2; }
get() { sed -n "s/^ *\"*$1\"* *= *\"\(.*\)\"$/\1/p" <<<"$vars"; }
cloud=$(get cloud)

code=0
for name in "$(get a)" "$(get b)"; do
  echo "== $cloud, cluster $name"
  case "$cloud" in
    aws) "$here/aws-inventory.sh" "$name" "$(get region)" ;;
    gcp) "$here/gcp-inventory.sh" "$(get project)" "$name" ;;
    *) echo "inventory: no inventory for the cloud '$cloud'" >&2; exit 2 ;;
  esac
  c=$?
  [ "$c" -gt "$code" ] && code=$c
done
exit $code
