# The AWS part of the example: the cloud account, the clusters, and the packs that only AWS
# uses. It is active when var.cloud is aws. The profiles and the checks do not change with the
# cloud.

locals {
  aws_cloud = {
    os_pack     = "ubuntu-aws"
    k8s_version = "1.36.2"
    csi_pack    = "csi-aws-ebs"
    csi_version = "1.65.0"
    # An older version of the CSI pack, for the pack refs of packs.sh.
    csi_previous_version = "1.63.1"
    # The one line of the CSI values that version 1.0.1 changes, as the pack writes it, with its
    # indentation: the values also hold the comments "# reclaimPolicy: Delete" and
    # "#   reclaimPolicy: Retain", which an edit must not change.
    csi_reclaim = "        reclaimPolicy: Delete"
  }
}

data "spectrocloud_cloudaccount_aws" "this" {
  count = var.cloud == "aws" ? 1 : 0
  name  = var.cloud_account_name
}

resource "spectrocloud_cluster_aws" "cluster" {
  for_each         = var.cloud == "aws" ? local.clusters : {}
  name             = each.value
  cloud_account_id = data.spectrocloud_cloudaccount_aws.this[0].id
  tags             = local.tags

  cluster_profile {
    id = local.cluster_release_uid
  }

  cluster_profile {
    id = local.cluster_addon_uid
    # The value the cluster gives the add-on profile's variable, when cluster.sh sets one.
    variables = var.addon_purpose == "" ? null : { purpose = var.addon_purpose }
  }

  # The second add-on profile, for the install order of add-on packs across profiles.
  dynamic "cluster_profile" {
    for_each = var.second_addon ? [spectrocloud_cluster_profile.addon2[0].id] : []
    content {
      id = cluster_profile.value
    }
  }

  cloud_config {
    region       = var.region
    ssh_key_name = var.ssh_key_name
  }

  machine_pool {
    name          = "cp-pool"
    count         = 1
    control_plane = true
    instance_type = var.instance_type
    azs           = [var.az]
  }

  machine_pool {
    name          = "worker-pool"
    count         = var.worker_count
    instance_type = var.instance_type
    azs           = [var.az]
  }

  timeouts {
    create = "60m"
    delete = "45m"
  }
}

# The two resources this one replaced, so an existing cluster moves in the state instead of being
# replaced.
moved {
  from = spectrocloud_cluster_aws.this
  to   = spectrocloud_cluster_aws.cluster["a"]
}

moved {
  from = spectrocloud_cluster_aws.b[0]
  to   = spectrocloud_cluster_aws.cluster["b"]
}
