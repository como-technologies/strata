# A cluster group on cluster A that pins the add-on version for its virtual clusters, and one
# virtual cluster in it, while cluster_group is set, for examples/live/cluster-group.sh. The
# virtual cluster runs in cluster A, so it adds no machine.

resource "spectrocloud_cluster_group" "group" {
  count   = var.cluster_group ? 1 : 0
  name    = "${var.name_prefix}-group"
  context = "project"

  clusters {
    cluster_uid = local.cluster_uids["a"]
    host_dns    = "*"
  }

  config {
    host_endpoint_type       = "LoadBalancer"
    cpu_millicore            = var.group_cpu_millicore
    memory_in_mb             = 12288
    storage_in_gb            = 10
    oversubscription_percent = 120
  }

  cluster_profile {
    id = spectrocloud_cluster_profile.addon.id
  }
}

resource "spectrocloud_virtual_cluster" "member" {
  count             = var.cluster_group ? 1 : 0
  name              = "${var.name_prefix}-member"
  cluster_group_uid = spectrocloud_cluster_group.group[0].id

  # Palette refuses a limit of less than 3 cores. The agents of Palette in the virtual cluster
  # take about 3.2 GiB of the memory limit, and the virtual cluster 1 GiB more
  resources {
    max_cpu       = 3
    max_mem_in_mb = 8192
    min_cpu       = 0
    min_mem_in_mb = 0
  }
}
