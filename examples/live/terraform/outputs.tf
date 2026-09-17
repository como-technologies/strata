# The scripts in examples/live read these by name.

output "project_uid" {
  value = data.spectrocloud_project.this.id
}

output "cluster_uid" {
  value = spectrocloud_cluster_aws.cluster["a"].id
}

output "profile_name" {
  value = local.profile_name
}

output "release_profile_uid" {
  value = spectrocloud_cluster_profile.release.id
}

output "next_profile_uid" {
  value = spectrocloud_cluster_profile.next.id
}

output "registries_profile_uid" {
  value = spectrocloud_cluster_profile.registries.id
}

output "addon_profile_uid" {
  value = spectrocloud_cluster_profile.addon.id
}

output "addon_profile_name" {
  value = spectrocloud_cluster_profile.addon.name
}

output "cluster_b_uid" {
  value = try(spectrocloud_cluster_aws.cluster["b"].id, "")
}

output "move_profile_uid" {
  value = try(spectrocloud_cluster_profile.move[0].id, "")
}

# Every profile name the example uses, for just live-down and baseline.sh, whether or not the
# optional profiles exist now.
output "profile_names" {
  value = [local.profile_name, "${var.name_prefix}-registries", "${var.name_prefix}-addon", "${var.name_prefix}-move"]
}

output "cluster_names" {
  value = { a = var.cluster_name, b = var.second_cluster_name }
}

output "tagged_profile_uids" {
  value = { for version, p in spectrocloud_cluster_profile.tagged : version => p.id }
}

output "region" {
  value = var.region
}
