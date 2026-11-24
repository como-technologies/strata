# The workload cluster pebbles (#701), in a second project of the tenant. wilma is in the project of
# the lab, so the two clusters cross the boundary of a project. pebbles runs a cluster profile of its
# own, the profile of its team, and a copy of two profiles of wilma. Both clusters run the profile of
# the tenant wilma-shared, and both have the tag of the set (main.tf).
#
# The variable second_project names the project, and platform-lib.sh gives it. With no second
# project, this file makes nothing, and the state holds wilma alone. The run reuses a project of the
# lab, so no apply makes or deletes a project. Billable while pebbles exists.

locals {
  boundary = var.second_project != ""
  # The profiles of wilma that pebbles also runs: a copy of each in the second project, with the same
  # name, version, packs, values, and glue. Their glue needs the CRDs of prometheus-operator, which
  # the profile of the team of pebbles holds.
  copied_teams = local.boundary ? ["data", "control"] : []
  # The packs of the team of pebbles: its metrics, its dashboards, and its secrets. Each is a pin of
  # wilma, so `platform.sh images` resolves its images. Grafana logs in through Keycloak on wilma,
  # because the profile gives the variable domain the domain of wilma.
  workload_packs = ["prometheus-operator", "grafana", "external-secrets-operator"]
  # The add-on profiles that pebbles runs, by key, for platform.sh. The profile of the tenant has the
  # key shared, as on wilma.
  workload_on_cluster = { for k, uid in merge(
    { for p in spectrocloud_cluster_profile.workload_team : "apps" => p.id },
    { for t, p in spectrocloud_cluster_profile.copy : t => p.id },
    { for p in spectrocloud_cluster_profile.shared : "shared" => p.id },
  ) : k => uid if var.addons }
}

data "spectrocloud_project" "second" {
  count = local.boundary ? 1 : 0
  name  = var.second_project
}

data "spectrocloud_cloudaccount_aws" "second" {
  count    = local.boundary ? 1 : 0
  provider = spectrocloud.second
  name     = var.second_cloud_account_name != "" ? var.second_cloud_account_name : var.cloud_account_name
}

# The profile of the tenant on both clusters. Each project of the tenant can use it. Its one
# manifest layer names the platform of the cluster.
resource "spectrocloud_cluster_profile" "shared" {
  count        = local.boundary ? 1 : 0
  name         = "${var.cluster_name}-shared"
  version      = "1.0.0"
  description  = "strata live platform: the profile of the tenant that wilma and pebbles run"
  cloud        = "all"
  type         = "add-on"
  context      = "tenant"
  skip_destroy = var.immutable_profiles

  pack {
    name   = "${var.cluster_name}-shared"
    type   = "manifest"
    tag    = "1.0.0"
    values = ""
    manifest {
      name = "${var.cluster_name}-shared"
      content = yamlencode({
        apiVersion = "v1"
        kind       = "ConfigMap"
        metadata   = { name = "${var.cluster_name}-shared", namespace = "default" }
        data       = { platform = var.cluster_name }
      })
    }
  }

  lifecycle {
    create_before_destroy = true
  }
}

# The cluster profile of pebbles: the packs of the release profile of the lab, with their values.
resource "spectrocloud_cluster_profile" "workload_release" {
  count        = local.boundary ? 1 : 0
  provider     = spectrocloud.second
  name         = "${var.workload_name}-release"
  version      = "1.0.0"
  description  = "strata live platform: the packs of the release profile of the lab, for pebbles"
  cloud        = "aws"
  type         = "cluster"
  skip_destroy = var.immutable_profiles

  dynamic "pack" {
    for_each = data.spectrocloud_cluster_profile.release.pack
    content {
      name   = pack.value.name
      type   = pack.value.type
      tag    = pack.value.tag
      uid    = pack.value.uid
      values = pack.value.values
    }
  }

  lifecycle {
    create_before_destroy = true
  }
}

# The profile of the team of pebbles, and its glue.
resource "spectrocloud_cluster_profile" "workload_team" {
  count        = local.boundary ? 1 : 0
  provider     = spectrocloud.second
  name         = "${var.workload_name}-apps"
  version      = "1.0.0"
  description  = "strata live platform: the packs of the team of pebbles, and its glue"
  cloud        = "all"
  type         = "add-on"
  skip_destroy = var.immutable_profiles

  dynamic "pack" {
    for_each = [for p in local.workload_packs : local.pins[p]]
    content {
      name         = pack.value.name
      type         = pack.value.type
      tag          = pack.value.tag
      uid          = pack.value.uid
      registry_uid = pack.value.registry_uid
      values       = pack.value.values
    }
  }

  pack {
    name   = "${var.workload_name}-apps-glue"
    type   = "manifest"
    tag    = "1.0.0"
    values = local.glue_values
    manifest {
      name    = "${var.workload_name}-apps-glue"
      content = local.glue.workload
    }
  }

  profile_variables {
    variable {
      name          = "domain"
      display_name  = "Domain"
      format        = "string"
      description   = "The domain of the platform: the private zone of external-dns"
      default_value = var.domain
      required      = false
    }
  }

  lifecycle {
    create_before_destroy = true
  }
}

# A copy of each profile of copied_teams in the second project. It is the profile of the team of
# wilma: the same name, version, packs, values, and glue, in another project.
resource "spectrocloud_cluster_profile" "copy" {
  for_each     = toset(local.copied_teams)
  provider     = spectrocloud.second
  name         = "${var.cluster_name}-${each.key}"
  version      = "1.0.0"
  description  = "strata live platform: the packs of the team ${each.key}, and its glue"
  cloud        = "all"
  type         = "add-on"
  skip_destroy = var.immutable_profiles

  dynamic "pack" {
    for_each = local.team_packs[each.key]
    content {
      name         = pack.value.name
      type         = pack.value.type
      tag          = pack.value.tag
      uid          = pack.value.uid
      registry_uid = pack.value.registry_uid
      values       = pack.value.values
    }
  }

  pack {
    name   = "${var.cluster_name}-${each.key}-glue"
    type   = "manifest"
    tag    = "1.0.0"
    values = local.glue_values
    manifest {
      name    = "${var.cluster_name}-${each.key}-glue"
      content = local.glue[each.key]
    }
  }

  lifecycle {
    create_before_destroy = true
  }
}

resource "spectrocloud_cluster_aws" "workload" {
  count            = local.boundary ? 1 : 0
  provider         = spectrocloud.second
  name             = var.workload_name
  cloud_account_id = data.spectrocloud_cloudaccount_aws.second[0].id
  tags             = local.tags

  cluster_profile {
    id = spectrocloud_cluster_profile.workload_release[0].id
  }

  dynamic "cluster_profile" {
    for_each = local.workload_on_cluster
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
    count         = var.workload_worker_count
    instance_type = var.instance_type
    azs           = [var.az]
  }

  timeouts {
    create = "60m"
    delete = "45m"
  }
}
