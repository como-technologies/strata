# The GKE part of the example: a managed engine on the GCP cloud account, for #90. Google runs the
# control plane, and one node pool runs the workers. It is active when var.cloud is gke. The
# profiles and the checks do not change with the cloud; the release is a GKE profile.

locals {
  gke_cloud = {
    os_pack     = "cos-gke"
    os_version  = "1.0.0"
    k8s_pack    = "kubernetes-gke"
    k8s_version = "1.34"
    cni_pack    = "cni-vpc-native-gke"
    cni_version = "1.0.0"
    csi_pack    = "gke-csi-gcp-driver"
    csi_version = "1.0.0"
    # The CSI pack has no older version for the pack refs of packs.sh.
    csi_previous_version = "1.0.0"
    # Each pack of the GKE engine has values of comments alone, which the root does not cover: so
    # version 1.0.1 adds one key to the values of the CSI pack.
    csi_reclaim  = "# No customization is supported"
    csi_next     = "# No customization is supported\nstrataNext: \"true\""
    csi_next_key = "strataNext"
  }
}

data "spectrocloud_cloudaccount_gcp" "gke" {
  count = var.cloud == "gke" ? 1 : 0
  name  = var.gcp_cloud_account_name
}

resource "spectrocloud_cluster_gke" "cluster" {
  for_each         = var.cloud == "gke" ? local.clusters : {}
  name             = each.value
  cloud_account_id = data.spectrocloud_cloudaccount_gcp.gke[0].id
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
    project = var.gcp_project
    region  = var.gcp_region
    # GCP refuses a new GKE cluster that is in no release channel, and Palette sets no channel
    # (known issue PCP-7528). The workaround of Palette is this cluster-level CAPI override.
    override_cluster_api_config = <<-EOT
      gcpManagedControlPlane:
        spec:
          releaseChannel: regular
    EOT
  }

  machine_pool {
    name          = "worker-pool"
    count         = var.gke_worker_count
    instance_type = var.gke_instance_type
    disk_size_gb  = 60
  }

  timeouts {
    create = "60m"
    delete = "45m"
  }
}

# The release version with another Kubernetes version and nothing else, for the repave question of
# a version bump on GKE. Set var.gke_k8s_bump to the version.
resource "spectrocloud_cluster_profile" "gke_k8s_bump" {
  count        = var.cloud == "gke" && var.gke_k8s_bump != "" ? 1 : 0
  name         = local.profile_name
  version      = "1.0.9"
  description  = "strata live example: the release with the Kubernetes pack at ${var.gke_k8s_bump}"
  cloud        = var.cloud
  type         = "cluster"
  skip_destroy = var.immutable_profiles

  dynamic "pack" {
    for_each = local.release_packs
    content {
      name   = pack.value.name
      tag    = pack.value.name == data.spectrocloud_pack.k8s.name ? data.spectrocloud_pack.gke_k8s_bump[0].version : pack.value.tag
      uid    = pack.value.name == data.spectrocloud_pack.k8s.name ? data.spectrocloud_pack.gke_k8s_bump[0].id : pack.value.uid
      values = pack.value.name == data.spectrocloud_pack.k8s.name ? data.spectrocloud_pack.gke_k8s_bump[0].values : pack.value.values
    }
  }

  depends_on = [spectrocloud_cluster_profile.next]

  lifecycle {
    create_before_destroy = true
  }
}

data "spectrocloud_pack" "gke_k8s_bump" {
  count        = var.cloud == "gke" && var.gke_k8s_bump != "" ? 1 : 0
  name         = local.cloud.k8s_pack
  version      = var.gke_k8s_bump
  registry_uid = data.spectrocloud_registry.packs.id
}
