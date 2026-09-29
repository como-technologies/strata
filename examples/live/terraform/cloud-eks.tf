# The EKS part of the example: a managed engine on the AWS cloud account, for #89. Amazon runs the
# control plane, and one managed node group runs the workers. It is active when var.cloud is eks.
# The profiles and the checks do not change with the cloud; the release is an EKS profile.

locals {
  eks_cloud = {
    os_pack     = "amazon-linux-eks"
    os_version  = "1.0.0"
    k8s_pack    = "kubernetes-eks"
    k8s_version = "1.35"
    cni_pack    = "cni-aws-vpc-eks"
    cni_version = "1.0"
    csi_pack    = "csi-aws-ebs"
    csi_version = "1.65.0"
    # An older version of the CSI pack, for the pack refs of packs.sh.
    csi_previous_version = "1.63.1"
    # The one line of the CSI values that version 1.0.1 changes, as in cloud-aws.tf.
    csi_reclaim  = "        reclaimPolicy: Delete"
    csi_next     = replace("        reclaimPolicy: Delete", "Delete", "Retain")
    csi_next_key = "reclaimPolicy"
  }
  # EKS needs two availability zones for its control plane.
  eks_azs = length(var.eks_azs) > 0 ? var.eks_azs : ["${var.region}a", "${var.region}b"]
}

# The cloud account of EKS runs is the example's own: a role in the AWS account of aws_profile
# that Palette assumes, with the core policies of Palette and the EKS policy. The role that the
# cloud account of cloud_account_name uses serves other work, so the example does not widen it.
# See docs/src/development/live-example.md, "An EKS cluster".

# The AWS account of Palette and the external id of this tenant, as the console shows them when a
# person adds a cloud account with STS.
data "external" "palette_sts" {
  count   = var.cloud == "eks" ? 1 : 0
  program = ["bash", "-c", "\"$0\" get /v1/clouds/aws/account/sts | jq '{accountId, externalId}'", "${path.module}/../api.sh"]
}

resource "aws_iam_role" "eks" {
  count = var.cloud == "eks" ? 1 : 0
  name  = "${var.name_prefix}-eks-cloud"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Action    = "sts:AssumeRole"
      Principal = { AWS = "arn:aws:iam::${data.external.palette_sts[0].result.accountId}:root" }
      Condition = { StringEquals = { "sts:ExternalId" = data.external.palette_sts[0].result.externalId } }
    }]
  })
  tags = { purpose = "strata-live-example", managed-by = "opentofu" }
}

# The core policies that Palette needs for any AWS cluster. They exist in the account already, as
# customer managed policies, so the role takes them by name.
data "aws_iam_policy" "palette_core" {
  for_each = var.cloud == "eks" ? toset(var.palette_core_policies) : toset([])
  name     = each.value
}

resource "aws_iam_role_policy_attachment" "eks_core" {
  for_each   = data.aws_iam_policy.palette_core
  role       = aws_iam_role.eks[0].name
  policy_arn = each.value.arn
}

# The EKS policy of Palette, as the public documentation gives it:
# https://docs.spectrocloud.com/clusters/public-cloud/aws/required-iam-policies/additional-iam-policies-specific-use-cases/
resource "aws_iam_policy" "eks_controllers" {
  count  = var.cloud == "eks" ? 1 : 0
  name   = "${var.name_prefix}-PaletteControllersEKSPolicy"
  policy = file("${path.module}/iam/eks-controllers.json")
}

resource "aws_iam_role_policy_attachment" "eks_controllers" {
  count      = var.cloud == "eks" ? 1 : 0
  role       = aws_iam_role.eks[0].name
  policy_arn = aws_iam_policy.eks_controllers[0].arn
}

# IAM takes a moment before Palette can assume a new role.
resource "time_sleep" "eks_role" {
  count           = var.cloud == "eks" ? 1 : 0
  create_duration = "30s"
  depends_on      = [aws_iam_role_policy_attachment.eks_core, aws_iam_role_policy_attachment.eks_controllers]
}

resource "spectrocloud_cloudaccount_aws" "eks" {
  count       = var.cloud == "eks" ? 1 : 0
  name        = "${var.name_prefix}-eks"
  context     = "project"
  type        = "sts"
  arn         = aws_iam_role.eks[0].arn
  external_id = data.external.palette_sts[0].result.externalId
  depends_on  = [time_sleep.eks_role]
}

resource "spectrocloud_cluster_eks" "cluster" {
  for_each         = var.cloud == "eks" ? local.clusters : {}
  name             = each.value
  cloud_account_id = spectrocloud_cloudaccount_aws.eks[0].id
  tags             = local.tags

  cluster_profile {
    id = local.cluster_release_uid
  }

  cluster_profile {
    id        = local.cluster_addon_uid
    variables = var.addon_purpose == "" ? null : { purpose = var.addon_purpose }
  }

  dynamic "cluster_profile" {
    for_each = var.extra_addon_profile_uids
    content {
      id = cluster_profile.value
    }
  }

  cloud_config {
    region       = var.region
    ssh_key_name = var.ssh_key_name
    azs          = local.eks_azs
  }

  machine_pool {
    name          = "worker-pool"
    count         = var.eks_worker_count
    instance_type = var.eks_instance_type
    disk_size_gb  = 60
    azs           = local.eks_azs
  }

  timeouts {
    create = "60m"
    delete = "45m"
  }

  # Provider 0.30.1 does not read cloud_config.azs back from Palette (flattenClusterConfigsEKS
  # sets no azs), and azs forces a new resource. Without this line each plan after the create
  # replaces the cluster, whatever else it changes.
  lifecycle {
    ignore_changes = [cloud_config[0].azs]
  }
}

# The release version with another Kubernetes version and nothing else, for the repave question of
# a version bump on EKS. Set var.eks_k8s_bump to the version.
resource "spectrocloud_cluster_profile" "k8s_bump" {
  count        = var.cloud == "eks" && var.eks_k8s_bump != "" ? 1 : 0
  name         = local.profile_name
  version      = "1.0.9"
  description  = "strata live example: the release with the Kubernetes pack at ${var.eks_k8s_bump}"
  cloud        = var.cloud
  type         = "cluster"
  skip_destroy = var.immutable_profiles

  dynamic "pack" {
    for_each = local.release_packs
    content {
      name   = pack.value.name
      tag    = pack.value.name == data.spectrocloud_pack.k8s.name ? data.spectrocloud_pack.k8s_bump[0].version : pack.value.tag
      uid    = pack.value.name == data.spectrocloud_pack.k8s.name ? data.spectrocloud_pack.k8s_bump[0].id : pack.value.uid
      values = pack.value.name == data.spectrocloud_pack.k8s.name ? data.spectrocloud_pack.k8s_bump[0].values : pack.value.values
    }
  }

  depends_on = [spectrocloud_cluster_profile.next]

  lifecycle {
    create_before_destroy = true
  }
}

data "spectrocloud_pack" "k8s_bump" {
  count        = var.cloud == "eks" && var.eks_k8s_bump != "" ? 1 : 0
  name         = local.cloud.k8s_pack
  version      = var.eks_k8s_bump
  registry_uid = data.spectrocloud_registry.packs.id
}
