# The GCP part of the example: the cloud account, the clusters, and the packs that only GCP uses.
# It is active when var.cloud is gcp. The profiles and the checks do not change with the cloud.

locals {
  gcp_cloud = {
    os_pack     = "ubuntu-gcp"
    os_version  = "24.04"
    k8s_pack    = "kubernetes"
    cni_pack    = "cni-cilium-oss"
    cni_version = "1.20.1"
    # The newest Kubernetes version with a GCP image that Spectro Cloud shares: the image of
    # 1.36.2 answers 403 on compute.images.useReadOnly, and no machine starts.
    k8s_version = "1.35.6"
    csi_pack    = "csi-gcp-driver"
    csi_version = "1.26.4"
    # An older version of the CSI pack, for the pack refs of packs.sh.
    csi_previous_version = "1.25.2"
    # The one line of the CSI values that version 1.0.1 changes, as the pack writes it, with its
    # indentation.
    csi_reclaim  = "    reclaimPolicy: \"Delete\""
    csi_next     = replace("    reclaimPolicy: \"Delete\"", "Delete", "Retain")
    csi_next_key = "reclaimPolicy"
  }
}

data "spectrocloud_cloudaccount_gcp" "this" {
  count = var.cloud == "gcp" ? 1 : 0
  name  = var.gcp_cloud_account_name
}

resource "spectrocloud_cluster_gcp" "cluster" {
  for_each         = var.cloud == "gcp" ? local.clusters : {}
  name             = each.value
  cloud_account_id = data.spectrocloud_cloudaccount_gcp.this[0].id
  tags             = local.tags

  dynamic "cluster_profile" {
    for_each = var.cluster_in_template ? [] : [local.cluster_release_uid]
    content {
      id = cluster_profile.value
    }
  }

  dynamic "cluster_profile" {
    for_each = var.cluster_in_template ? [] : [local.cluster_addon_uid]
    content {
      id = cluster_profile.value
      # The value the cluster gives the add-on profile's variable, when cluster.sh sets one.
      variables = var.addon_purpose == "" ? null : { purpose = var.addon_purpose }
    }
  }

  # A cluster in the cluster template takes the profiles of the template (template.sh, #276).
  dynamic "cluster_template" {
    for_each = var.cluster_in_template ? [spectrocloud_cluster_config_template.release[0].id] : []
    content {
      id = cluster_template.value
      cluster_profile {
        id = local.cluster_release_uid
      }
      cluster_profile {
        id = local.cluster_addon_uid
      }
    }
  }

  # The second add-on profile, for the install order of add-on packs across profiles.
  dynamic "cluster_profile" {
    for_each = var.second_addon ? [spectrocloud_cluster_profile.addon2[0].id] : []
    content {
      id = cluster_profile.value
    }
  }

  # More add-on profiles that a script made through the API, such as the profiles that pin a chart
  # of the registry host.
  dynamic "cluster_profile" {
    for_each = var.extra_addon_profile_uids
    content {
      id = cluster_profile.value
    }
  }

  cloud_config {
    project = var.gcp_project
    region  = var.gcp_region
  }

  machine_pool {
    name          = "cp-pool"
    count         = 1
    control_plane = true
    instance_type = var.gcp_instance_type
    azs           = [var.gcp_zone]
  }

  machine_pool {
    name          = "worker-pool"
    count         = var.worker_count
    instance_type = var.gcp_instance_type
    azs           = [var.gcp_zone]
  }

  timeouts {
    create = "60m"
    delete = "45m"
  }
}
