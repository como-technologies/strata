# examples/live/platform.sh reads these by name.

output "project_uid" {
  value = data.spectrocloud_project.this.id
}

output "cluster_uid" {
  value = spectrocloud_cluster_aws.wilma.id
}

output "cluster_name" {
  value = var.cluster_name
}

output "release_profile_uid" {
  value = data.spectrocloud_cluster_profile.release.id
}

# The uid of version 1.0.0 of each profile of a team, by team.
output "profile_uids" {
  value = { for team, p in spectrocloud_cluster_profile.team : team => p.id }
}

# The add-on profiles that wilma runs now, by team.
output "on_cluster" {
  value = local.on_cluster
}

output "domain_teams" {
  value = local.domain_teams
}

output "domain" {
  value = var.domain
}

output "network_next_uid" {
  value = try(spectrocloud_cluster_profile.network_next[0].id, "")
}

output "moved_uids" {
  value = { for team, p in spectrocloud_cluster_profile.moved : team => p.id }
}

# The pins, by pack name: the registry, the version, and the type.
output "packs" {
  value = var.packs
}

# The images that the values of each pin name, by pack name. A plan gives them before the apply,
# and `platform.sh images` resolves each one.
output "images" {
  value = local.images
}

# The packs of each team, in the order of the profile.
output "teams" {
  value = local.teams
}

# Every profile name that the platform uses, for platform.sh down, whether or not the profiles of
# the cases exist now.
output "profile_names" {
  value = concat([for team in keys(local.teams) : "${var.cluster_name}-${team}"], ["${var.cluster_name}-logging"])
}

output "zone_id" {
  value = aws_route53_zone.private.zone_id
}

output "zone_name" {
  value = aws_route53_zone.private.name
}

output "node_policy_name" {
  value = aws_iam_role_policy.external_dns.name
}

output "node_role_name" {
  value = var.node_role_name
}
