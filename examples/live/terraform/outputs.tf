# The scripts in examples/live read these by name.

output "project_uid" {
  value = data.spectrocloud_project.this.id
}

output "cluster_uid" {
  value = try(spectrocloud_cluster_aws.cluster["a"].id, "")
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
  value = [local.profile_name, "${var.name_prefix}-registries", "${var.name_prefix}-addon", "${var.name_prefix}-move", "${var.name_prefix}-owned", "${var.name_prefix}-owned-helm", "${var.name_prefix}-addon2", "${var.name_prefix}-dep"]
}

# The example's own OCI Helm registry, and the chart profile, for examples/live/registry.sh.
output "owned_helm_registry_uid" {
  value = try(spectrocloud_registry_oci.helm[0].id, "")
}

output "owned_helm_registry_name" {
  value = try(spectrocloud_registry_oci.helm[0].name, "")
}

output "owned_chart_repository" {
  value = aws_ecr_repository.chart.repository_url
}

output "owned_chart_name" {
  value = local.chart_name
}

output "owned_helm_profile_uid" {
  value = try(spectrocloud_cluster_profile.owned_helm[0].id, "")
}

# The cluster template and the second add-on profile, for examples/live/template.sh.
output "template_uid" {
  value = try(spectrocloud_cluster_config_template.release[0].id, "")
}

output "addon2_profile_uid" {
  value = try(spectrocloud_cluster_profile.addon2[0].id, "")
}

output "addon2_profile_name" {
  value = "${var.name_prefix}-addon2"
}

# The example's own OCI pack registry, for examples/live/registry.sh.
output "owned_registry_uid" {
  value = spectrocloud_registry_oci.pack.id
}

output "owned_registry_name" {
  value = spectrocloud_registry_oci.pack.name
}

output "owned_registry_host" {
  value = local.registry_host
}

output "owned_repository" {
  value = aws_ecr_repository.pack.repository_url
}

output "owned_pack_name" {
  value = local.pack_name
}

output "owned_profile_uid" {
  value = try(spectrocloud_cluster_profile.owned[0].id, "")
}

# The second pack of the pack registry, and the profile of the dependency check.
output "dep_pack_name" {
  value = local.dep_name
}

output "owned_dep_repository" {
  value = aws_ecr_repository.dep.repository_url
}

output "dep_profile_uid" {
  value = try(spectrocloud_cluster_profile.dep[0].id, "")
}

output "aws_profile" {
  value = var.aws_profile
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
