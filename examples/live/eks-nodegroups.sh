#!/usr/bin/env bash
# The EKS node groups of cluster A (#89), from AWS and not from Palette.
#
#   examples/live/eks-nodegroups.sh [--delete]
#
# Prints one line for each node group: its name, instance type, status, and nodes. A change of the
# instance type of the node pool makes Palette create a node group of the new type and keep the
# old one, which Palette then no longer tracks: a delete of the cluster removes the new group
# alone, and the EKS control plane cannot go while the old one exists. --delete deletes each node
# group of the cluster that carries the tag purpose=strata-live-example, and waits until it is
# gone: a step of the recovery of such a cluster, which does not end the wait of Palette alone.
#
# Needs the AWS CLI with the profile of aws_profile. Exit 0 when the cluster has no node group or
# each one is listed or deleted; 2 when AWS cannot answer.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
tfvars=$here/terraform/terraform.tfvars
value() { sed -n "s/^$1 *= *\"\\(.*\\)\"/\\1/p" "$tfvars"; }
region=$(value region)
profile=$(value aws_profile)
cluster=$(value cluster_name)
cluster=${cluster:-fred}
export AWS_PROFILE=${AWS_PROFILE:-$profile}

groups=$(aws eks list-nodegroups --region "$region" --cluster-name "$cluster" --query nodegroups --output text 2>/dev/null) || {
  echo "eks-nodegroups: no EKS cluster $cluster in $region"
  exit 0
}
for g in $groups; do
  aws eks describe-nodegroup --region "$region" --cluster-name "$cluster" --nodegroup-name "$g" \
    --query 'nodegroup.[nodegroupName, instanceTypes[0], status, scalingConfig.desiredSize, tags.purpose]' --output text \
    || exit 2
done
[ "${1:-}" = --delete ] || exit 0
for g in $groups; do
  purpose=$(aws eks describe-nodegroup --region "$region" --cluster-name "$cluster" --nodegroup-name "$g" \
    --query 'nodegroup.tags.purpose' --output text) || exit 2
  [ "$purpose" = strata-live-example ] || { echo "eks-nodegroups: $g is not the example's; left alone"; continue; }
  aws eks delete-nodegroup --region "$region" --cluster-name "$cluster" --nodegroup-name "$g" > /dev/null || exit 2
  echo "eks-nodegroups: deleting $g"
done
for g in $groups; do
  aws eks wait nodegroup-deleted --region "$region" --cluster-name "$cluster" --nodegroup-name "$g" 2>/dev/null
done
echo "eks-nodegroups: the node groups are gone"
