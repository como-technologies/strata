# verify.sh reads these by name.

output "project_uid" {
  value = data.spectrocloud_project.this.id
}

output "cluster_uid" {
  value = spectrocloud_cluster_aws.this.id
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
  value = try(spectrocloud_cluster_aws.b[0].id, "")
}

output "move_profile_uid" {
  value = try(spectrocloud_cluster_profile.move[0].id, "")
}
