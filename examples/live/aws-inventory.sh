#!/usr/bin/env bash
# What a live example cluster still has in AWS, read-only. Exit 0 when nothing is left, 1 while
# anything is, 2 when AWS cannot be asked.
#
#   AWS_PROFILE=<profile> examples/live/aws-inventory.sh [CLUSTER_NAME] [REGION]
#
# Defaults: fred, cluster A, in us-east-2. Instances, NAT gateways, VPCs, elastic IPs, and
# volumes carry the Cluster API tag sigs.k8s.io/cluster-api-provider-aws/cluster/<name>; the
# classic load balancer is found by its exact name, <name>-apiserver. Run it after
# `just live-down`, and before any force delete: a force delete drops only Palette's records.
set -uo pipefail
name=${1:-fred}
region=${2:-us-east-2}
tag="sigs.k8s.io/cluster-api-provider-aws/cluster/$name"
total=0

count() { # label command...
  local label=$1 out n
  shift
  out=$("$@" --region "$region" --output text 2>&1) || { echo "aws-inventory: $label: $out" >&2; exit 2; }
  n=$(wc -w <<<"$out")
  printf '%-22s %s\n' "$label" "$n"
  total=$((total + n))
}

count instances aws ec2 describe-instances \
  --filters "Name=tag-key,Values=$tag" "Name=instance-state-name,Values=pending,running,stopping,stopped,shutting-down" \
  --query 'Reservations[].Instances[].InstanceId'
count "nat gateways" aws ec2 describe-nat-gateways \
  --filter "Name=tag-key,Values=$tag" "Name=state,Values=pending,available,deleting" --query 'NatGateways[].NatGatewayId'
count vpcs aws ec2 describe-vpcs --filters "Name=tag-key,Values=$tag" --query 'Vpcs[].VpcId'
count "elastic ips" aws ec2 describe-addresses --filters "Name=tag-key,Values=$tag" --query 'Addresses[].AllocationId'
count volumes aws ec2 describe-volumes --filters "Name=tag-key,Values=$tag" --query 'Volumes[].VolumeId'
count "load balancers" aws elb describe-load-balancers \
  --query "LoadBalancerDescriptions[?LoadBalancerName=='$name-apiserver'].LoadBalancerName"

[ "$total" -eq 0 ]
