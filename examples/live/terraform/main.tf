# strata live example: one AWS cluster that runs profile version 1.0.0, and a version 1.0.1 of the
# same profile that nothing runs. 1.0.1 differs by one value, the EBS storage class reclaimPolicy,
# so every strata check has a known answer. An add-on profile runs on the cluster next to it. A
# third profile, attached to nothing, holds one pack from a Helm registry and one from an OCI
# registry, for their pack-ref digests. The variables of examples/live/in-place.sh add a second
# cluster, edit the add-on profile in place, bump its version, and move a profile's CSI pack to an
# OCI registry. examples/live/README.md lists the checks.
#
# Billable while it exists: tear it down with `just live-down`.

locals {
  profile_name = "${var.name_prefix}-profile"
  tags         = ["purpose:strata-live-example", "managed-by:terraform"]

  release_packs = [
    for p in [data.spectrocloud_pack.os, data.spectrocloud_pack.k8s, data.spectrocloud_pack.cni, data.spectrocloud_pack.csi] :
    { name = p.name, tag = p.version, uid = p.id, values = p.values }
  ]
  # The one difference between the two versions.
  next_csi_values = replace(data.spectrocloud_pack.csi.values, "reclaimPolicy: Delete", "reclaimPolicy: Retain")
  # With edit_next_in_place, a second change applied to 1.0.1 after it is published.
  next_csi_values_edited = replace(local.next_csi_values, "volumeBindingMode: WaitForFirstConsumer", "volumeBindingMode: Immediate")
  next_packs = [
    for p in local.release_packs :
    p.name == data.spectrocloud_pack.csi.name
    ? merge(p, { values = var.edit_next_in_place ? local.next_csi_values_edited : local.next_csi_values })
    : p
  ]

  # The add-on profile's one layer: a ConfigMap, so it installs anywhere and an edit to it changes
  # nothing a running cluster depends on, unlike a storage class field, which Kubernetes does not
  # let change. With edit_addon_in_place, one data value changes in place.
  addon_manifest_base = <<-EOT
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: strata-live
      namespace: default
    data:
      purpose: strata live example
      edited: "no"
  EOT
  addon_manifest      = var.edit_addon_in_place ? replace(local.addon_manifest_base, "edited: \"no\"", "edited: \"yes\"") : local.addon_manifest_base
  # The add-on profile version the clusters run: a pinned uid, else the resource's current one.
  cluster_addon_uid = var.cluster_addon_profile_uid != "" ? var.cluster_addon_profile_uid : spectrocloud_cluster_profile.addon.id

  # The registry-move profile: the release packs, with the CSI pack from the OCI registry when
  # move_csi_to_oci is set.
  move_packs = var.registry_move ? [
    for p in local.release_packs :
    p.name == data.spectrocloud_pack.csi.name && var.move_csi_to_oci
    ? merge(p, { uid = data.spectrocloud_pack.csi_oci[0].id, type = "oci", registry_uid = data.spectrocloud_registry.move[0].id })
    : merge(p, { type = "spectro", registry_uid = data.spectrocloud_registry.packs.id })
  ] : []
}

data "spectrocloud_project" "this" {
  name = var.palette_project
}

data "spectrocloud_cloudaccount_aws" "this" {
  name = var.cloud_account_name
}

data "spectrocloud_registry" "packs" {
  name = var.registry_name
}

data "spectrocloud_pack" "os" {
  name         = "ubuntu-aws"
  version      = var.os_version
  registry_uid = data.spectrocloud_registry.packs.id
}

data "spectrocloud_pack" "k8s" {
  name         = "kubernetes"
  version      = var.k8s_version
  registry_uid = data.spectrocloud_registry.packs.id
}

data "spectrocloud_pack" "cni" {
  name         = "cni-cilium-oss"
  version      = var.cni_version
  registry_uid = data.spectrocloud_registry.packs.id
}

data "spectrocloud_pack" "csi" {
  name         = "csi-aws-ebs"
  version      = var.csi_version
  registry_uid = data.spectrocloud_registry.packs.id
}

resource "spectrocloud_cluster_profile" "release" {
  name        = local.profile_name
  version     = "1.0.0"
  description = "strata live example: the version the cluster runs"
  cloud       = "aws"
  type        = "cluster"

  dynamic "pack" {
    for_each = local.release_packs
    content {
      name   = pack.value.name
      tag    = pack.value.tag
      uid    = pack.value.uid
      values = pack.value.values
    }
  }
}

resource "spectrocloud_cluster_profile" "next" {
  name        = local.profile_name
  version     = "1.0.1"
  description = "strata live example: a newer version nothing runs"
  cloud       = "aws"
  type        = "cluster"

  dynamic "pack" {
    for_each = local.next_packs
    content {
      name   = pack.value.name
      tag    = pack.value.tag
      uid    = pack.value.uid
      values = pack.value.values
    }
  }

  depends_on = [spectrocloud_cluster_profile.release]
}

data "spectrocloud_registry" "helm" {
  name = var.helm_registry_name
}

data "spectrocloud_registry" "oci" {
  name = var.oci_registry_name
}

data "spectrocloud_pack" "helm" {
  name         = var.helm_chart
  version      = var.helm_chart_version
  registry_uid = data.spectrocloud_registry.helm.id
}

data "spectrocloud_pack" "oci" {
  name         = var.oci_pack
  version      = var.oci_pack_version
  registry_uid = data.spectrocloud_registry.oci.id
}

resource "spectrocloud_cluster_profile" "registries" {
  name        = "${var.name_prefix}-registries"
  version     = "1.0.0"
  description = "strata live example: a Helm pack and an OCI pack; nothing runs it"
  cloud       = "all"
  type        = "add-on"

  pack {
    name         = data.spectrocloud_pack.helm.name
    type         = "helm"
    tag          = data.spectrocloud_pack.helm.version
    uid          = data.spectrocloud_pack.helm.id
    registry_uid = data.spectrocloud_registry.helm.id
    values       = data.spectrocloud_pack.helm.values
  }

  pack {
    name         = data.spectrocloud_pack.oci.name
    type         = "oci"
    tag          = data.spectrocloud_pack.oci.version
    uid          = data.spectrocloud_pack.oci.id
    registry_uid = data.spectrocloud_registry.oci.id
    values       = data.spectrocloud_pack.oci.values
  }
}

resource "spectrocloud_cluster_profile" "addon" {
  name        = "${var.name_prefix}-addon"
  version     = var.addon_version
  description = "strata live example: an add-on profile the clusters run next to the release"
  cloud       = "all"
  type        = "add-on"
  # With immutable-clusterprofiles, a version bump keeps the old version in Palette.
  skip_destroy = var.immutable_profiles || var.keep_profile_versions

  pack {
    name   = "strata-live-config"
    type   = "manifest"
    tag    = "1.0.0"
    values = ""
    manifest {
      name    = "strata-live-config"
      content = local.addon_manifest
    }
  }

  lifecycle {
    create_before_destroy = true
  }
}

data "spectrocloud_registry" "move" {
  count = var.registry_move ? 1 : 0
  name  = var.move_registry_name
}

data "spectrocloud_pack" "csi_oci" {
  count        = var.registry_move ? 1 : 0
  name         = "csi-aws-ebs"
  version      = var.csi_version
  registry_uid = data.spectrocloud_registry.move[0].id
}

resource "spectrocloud_cluster_profile" "move" {
  count        = var.registry_move ? 1 : 0
  name         = "${var.name_prefix}-move"
  version      = var.move_version
  description  = "strata live example: the release packs, for a real registry move; nothing runs it"
  cloud        = "aws"
  type         = "cluster"
  skip_destroy = var.immutable_profiles || var.keep_profile_versions

  dynamic "pack" {
    for_each = local.move_packs
    content {
      name         = pack.value.name
      tag          = pack.value.tag
      uid          = pack.value.uid
      type         = pack.value.type
      registry_uid = pack.value.registry_uid
      values       = pack.value.values
    }
  }

  lifecycle {
    create_before_destroy = true
  }
}

resource "spectrocloud_cluster_aws" "this" {
  name             = var.cluster_name
  cloud_account_id = data.spectrocloud_cloudaccount_aws.this.id
  tags             = local.tags

  cluster_profile {
    id = spectrocloud_cluster_profile.release.id
  }

  cluster_profile {
    id = local.cluster_addon_uid
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

# Cluster B for examples/live/in-place.sh: the same profiles as the cluster, created after an
# in-place edit of the add-on profile. Billable while it exists. Created without waiting for it to
# finish: Palette records the profiles a cluster runs when the cluster is created, and that record
# is what strata reads; an add-on that never installs would otherwise hold the apply for an hour.
resource "spectrocloud_cluster_aws" "b" {
  count            = var.second_cluster ? 1 : 0
  name             = var.second_cluster_name
  cloud_account_id = data.spectrocloud_cloudaccount_aws.this.id
  tags             = local.tags
  skip_completion  = true

  cluster_profile {
    id = spectrocloud_cluster_profile.release.id
  }

  cluster_profile {
    id = local.cluster_addon_uid
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
