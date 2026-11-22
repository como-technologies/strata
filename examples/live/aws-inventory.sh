#!/usr/bin/env bash
# What a live example cluster still has in AWS, read-only. Exit 0 when nothing is left, 1 while
# anything is, 2 when AWS cannot be asked.
#
#   AWS_PROFILE=<profile> examples/live/aws-inventory.sh [CLUSTER_NAME] [REGION] [OPTION...]
#
# Defaults: fred, cluster A, in us-east-2. An EKS cluster of the name counts as well. Instances,
# NAT gateways, elastic IPs, volumes, security groups, subnets, route tables, internet gateways,
# and the VPC carry the Cluster API tag sigs.k8s.io/cluster-api-provider-aws/cluster/<name>; the
# classic load balancer is found by its exact name, <name>-apiserver, and network interfaces, which
# are not all tagged, inside the VPC.
# A deletion removes the security groups, route tables, subnets, internet gateway, and VPC last,
# minutes after the instances and the NAT gateway, so only 0 on every line means AWS is empty. Run
# it after `just live-down`, and before any force delete: a force delete drops only Palette's
# records.
#
# The options count what a cluster with workloads adds, such as wilma of platform.sh:
#
#   --kubernetes-tags     each object with the tag kubernetes.io/cluster/<name>, such as a load balancer
#                         of a Service
#   --csi-volumes         each volume that the EBS CSI driver made in the region, for any cluster
#   --zone NAME           each hosted zone of the name NAME
#   --role-policy R:P     the inline policy P of the IAM role R
set -uo pipefail
name=${1:-fred}
region=${2:-us-east-2}
shift $(($# < 2 ? $# : 2))
k8s_tags= csi_volumes= zone= role_policy=
while [ $# -gt 0 ]; do
  case "$1" in
    --kubernetes-tags) k8s_tags=1; shift ;;
    --csi-volumes) csi_volumes=1; shift ;;
    --zone) [ $# -ge 2 ] || { echo "aws-inventory: --zone needs a name" >&2; exit 2; }; zone=${2%.}; shift 2 ;;
    --role-policy) [ $# -ge 2 ] || { echo "aws-inventory: --role-policy needs ROLE:POLICY" >&2; exit 2; }; role_policy=$2; shift 2 ;;
    *) echo "aws-inventory: unknown argument $1" >&2; exit 2 ;;
  esac
done
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
count "elastic ips" aws ec2 describe-addresses --filters "Name=tag-key,Values=$tag" --query 'Addresses[].AllocationId'
count volumes aws ec2 describe-volumes --filters "Name=tag-key,Values=$tag" --query 'Volumes[].VolumeId'
count "load balancers" aws elb describe-load-balancers \
  --query "LoadBalancerDescriptions[?LoadBalancerName=='$name-apiserver'].LoadBalancerName"
count "security groups" aws ec2 describe-security-groups --filters "Name=tag-key,Values=$tag" --query 'SecurityGroups[].GroupId'
count subnets aws ec2 describe-subnets --filters "Name=tag-key,Values=$tag" --query 'Subnets[].SubnetId'
count "route tables" aws ec2 describe-route-tables --filters "Name=tag-key,Values=$tag" --query 'RouteTables[].RouteTableId'
count "internet gateways" aws ec2 describe-internet-gateways --filters "Name=tag-key,Values=$tag" \
  --query 'InternetGateways[].InternetGatewayId'
vpcs=$(aws ec2 describe-vpcs --filters "Name=tag-key,Values=$tag" --query 'Vpcs[].VpcId' --region "$region" --output text 2>&1) \
  || { echo "aws-inventory: vpcs: $vpcs" >&2; exit 2; }
if [ -n "$(tr -d '[:space:]' <<<"$vpcs")" ]; then
  count "network interfaces" aws ec2 describe-network-interfaces \
    --filters "Name=vpc-id,Values=$(tr -s '[:space:]' ',' <<<"$vpcs" | sed 's/^,//; s/,$//')" \
    --query 'NetworkInterfaces[].NetworkInterfaceId'
else
  printf '%-22s %s\n' "network interfaces" 0
fi
n=$(wc -w <<<"$vpcs")
printf '%-22s %s\n' vpcs "$n"
total=$((total + n))
# an EKS cluster of the name (cloud = "eks"); 0 for a cluster on instances
count "eks clusters" aws eks list-clusters --query "clusters[?@ == '$name']"
[ -z "$k8s_tags" ] || count "kubernetes tags" aws resourcegroupstaggingapi get-resources \
  --tag-filters "Key=kubernetes.io/cluster/$name" --query 'ResourceTagMappingList[].ResourceARN'
[ -z "$csi_volumes" ] || count "csi volumes" aws ec2 describe-volumes \
  --filters "Name=tag-key,Values=ebs.csi.aws.com/cluster" --query 'Volumes[].VolumeId'
[ -z "$zone" ] || count "hosted zones" aws route53 list-hosted-zones-by-name --dns-name "$zone" \
  --query "HostedZones[?Name == '$zone.'].Id"
[ -z "$role_policy" ] || count "role policies" aws iam list-role-policies --role-name "${role_policy%%:*}" \
  --query "PolicyNames[?@ == '${role_policy#*:}']"

[ "$total" -eq 0 ]
