#!/usr/bin/env bash
# Remove what a stuck delete of a live example cluster leaves in AWS: the recovery of #89, as a
# script, scoped to the resources that carry the Cluster API tag of the cluster name. Nothing
# without the tag is touched, and nothing is deleted before --yes.
#
#   AWS_PROFILE=<profile> examples/live/aws-cleanup.sh [CLUSTER_NAME] [REGION] [--yes]
#
# Defaults: fred, in us-east-2. Without --yes it prints what it would delete and exits 0. With
# --yes it deletes, in the order that AWS accepts: the EKS cluster of the name (after its node
# groups, which examples/live/eks-nodegroups.sh --delete removes first), the NAT gateways, the
# elastic IPs, the network interfaces of the VPC that are available, the internet gateway, the
# subnets, the route tables that are not the main one, the security groups that are not the
# default one, and the VPC. It waits for the NAT gateways and the EKS cluster, whose deletes take
# minutes. Run examples/live/aws-inventory.sh after: 0 on every line. Then
# examples/live/api.sh delete-cluster <uid> --force removes the records of Palette, and
# `just live-down` the rest of the example. Exit 0 when everything is gone, 1 while something is
# left, 2 when AWS cannot be asked.
set -uo pipefail
name=${1:-fred}
region=${2:-us-east-2}
yes=""
[ "${3:-}" = "--yes" ] && yes=1
[ "${1:-}" = "--yes" ] && { yes=1; name=fred; }
[ "${2:-}" = "--yes" ] && { yes=1; region=us-east-2; }
tag="sigs.k8s.io/cluster-api-provider-aws/cluster/$name"
aws() { command aws --region "$region" --output text "$@"; }
ids() { # ids LABEL COMMAND...: the ids one query gives, on one line, or exit 2 when AWS cannot answer
  local label=$1 out
  shift
  out=$("$@" 2>&1) || { echo "aws-cleanup: $label: $out" >&2; exit 2; }
  tr -s '[:space:]' ' ' <<<"$out" | sed 's/^ //; s/ $//'
}
say() { printf '%-20s %s\n' "$1" "${2:-none}"; }
act() { # act WHAT COMMAND...: print, and run with --yes
  local what=$1
  shift
  if [ -n "$yes" ]; then
    echo "delete $what"
    "$@" > /dev/null 2>&1 || echo "aws-cleanup: $what: the delete failed, and the pass below counts it" >&2
  else
    echo "would delete $what"
  fi
}

cluster=$(ids "eks cluster" aws eks list-clusters --query "clusters[?@ == '$name']")
nats=$(ids "nat gateways" aws ec2 describe-nat-gateways --filter "Name=tag-key,Values=$tag" "Name=state,Values=pending,available,deleting" --query 'NatGateways[].NatGatewayId')
eips=$(ids "elastic ips" aws ec2 describe-addresses --filters "Name=tag-key,Values=$tag" --query 'Addresses[].AllocationId')
igws=$(ids "internet gateways" aws ec2 describe-internet-gateways --filters "Name=tag-key,Values=$tag" --query 'InternetGateways[].InternetGatewayId')
subnets=$(ids subnets aws ec2 describe-subnets --filters "Name=tag-key,Values=$tag" --query 'Subnets[].SubnetId')
rtbs=$(ids "route tables" aws ec2 describe-route-tables --filters "Name=tag-key,Values=$tag" --query 'RouteTables[?Associations[0].Main != `true`].RouteTableId')
sgs=$(ids "security groups" aws ec2 describe-security-groups --filters "Name=tag-key,Values=$tag" --query "SecurityGroups[?GroupName != 'default'].GroupId")
vpcs=$(ids vpcs aws ec2 describe-vpcs --filters "Name=tag-key,Values=$tag" --query 'Vpcs[].VpcId')
say "eks cluster" "$cluster"; say "nat gateways" "$nats"; say "elastic ips" "$eips"; say "internet gateways" "$igws"
say "subnets" "$subnets"; say "route tables" "$rtbs"; say "security groups" "$sgs"; say "vpcs" "$vpcs"
[ -n "$yes" ] || { echo "aws-cleanup: nothing deleted; add --yes"; exit 0; }

# the EKS control plane: its node groups must be gone first
if [ -n "$cluster" ]; then
  groups=$(ids "node groups" aws eks list-nodegroups --cluster-name "$name" --query nodegroups)
  [ -z "$groups" ] || { echo "aws-cleanup: the cluster $name still has node groups ($groups); run examples/live/eks-nodegroups.sh --delete first" >&2; exit 1; }
  act "eks cluster $name" aws eks delete-cluster --name "$name"
fi
# the NAT gateways go first, and take minutes; the addresses and the interfaces follow them
for n in $nats; do act "nat gateway $n" aws ec2 delete-nat-gateway --nat-gateway-id "$n"; done
if [ -n "$nats" ]; then
  echo "wait for the NAT gateways"
  # shellcheck disable=SC2086
  aws ec2 wait nat-gateway-deleted --nat-gateway-ids $nats > /dev/null 2>&1 || sleep 60
fi
for e in $eips; do act "elastic ip $e" aws ec2 release-address --allocation-id "$e"; done
for v in $vpcs; do
  enis=$(ids "network interfaces" aws ec2 describe-network-interfaces --filters "Name=vpc-id,Values=$v" "Name=status,Values=available" --query 'NetworkInterfaces[].NetworkInterfaceId')
  for i in $enis; do act "network interface $i" aws ec2 delete-network-interface --network-interface-id "$i"; done
  for g in $igws; do
    act "detach internet gateway $g" aws ec2 detach-internet-gateway --internet-gateway-id "$g" --vpc-id "$v"
  done
done
for g in $igws; do act "internet gateway $g" aws ec2 delete-internet-gateway --internet-gateway-id "$g"; done
for s in $subnets; do act "subnet $s" aws ec2 delete-subnet --subnet-id "$s"; done
for r in $rtbs; do act "route table $r" aws ec2 delete-route-table --route-table-id "$r"; done
for g in $sgs; do act "security group $g" aws ec2 delete-security-group --group-id "$g"; done
if [ -n "$cluster" ]; then
  echo "wait for the EKS cluster"
  aws eks wait cluster-deleted --name "$name" > /dev/null 2>&1 || true
fi
for v in $vpcs; do act "vpc $v" aws ec2 delete-vpc --vpc-id "$v"; done
echo "aws-cleanup: done; the inventory says what is left:"
"$(dirname "$0")/aws-inventory.sh" "$name" "$region"
