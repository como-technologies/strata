#!/usr/bin/env bash
# What a live example cluster still has in GCP, read-only. Exit 0 when nothing is left, 1 while
# anything is, 2 when GCP cannot be asked.
#
#   examples/live/gcp-inventory.sh PROJECT [CLUSTER_NAME]
#
# CLUSTER_NAME defaults to fred, cluster A. The instances, their disks, the load balancer, the
# network and its subnets, the router, and the addresses count when their names start with the
# cluster name, as the cluster API gives them. The firewall rules count when they are on the
# network of the cluster, <name>-network: their names are allow-<name>-.... A deletion removes the
# network last, minutes after the instances, so only 0 on every line means GCP is empty. Run it
# after `just live-down`, and before any force delete: a force delete drops only Palette's records.
set -uo pipefail
project=${1:?usage: gcp-inventory.sh PROJECT [CLUSTER_NAME]}
name=${2:-fred}
total=0

err=$(mktemp)
trap 'rm -f "$err"' EXIT

count() { # label filter command...
  local label=$1 filter=$2 out n
  shift 2
  out=$(gcloud compute "$@" --project "$project" ${filter:+--filter="$filter"} --format='value(name)' 2> "$err") \
    || { echo "gcp-inventory: $label: $(cat "$err")" >&2; exit 2; }
  n=$(wc -w <<<"$out")
  printf '%-22s %s\n' "$label" "$n"
  total=$((total + n))
}

mine="name ~ ^$name"
count instances "$mine" instances list
count disks "$mine" disks list
count "forwarding rules" "$mine" forwarding-rules list
count "backend services" "$mine" backend-services list
count "target proxies" "$mine" target-tcp-proxies list
count "health checks" "$mine" health-checks list
count "instance groups" "$mine" instance-groups list
count "firewall rules" "network ~ /$name-network\$" firewall-rules list
count addresses "$mine" addresses list
count routers "$mine" routers list
count subnets "$mine" networks subnets list
count networks "$mine" networks list

[ "$total" -eq 0 ]
