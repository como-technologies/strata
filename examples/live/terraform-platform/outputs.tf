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

# The uid of the version of each profile of a team, by team: 1.0.0, or 1.1.0 of security while
# case 2 of platform-boundary.sh moves Vault.
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
  value = local.packs
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

# ---- pebbles (#701). Each is empty with no second project. ----------------------------------------

# The second project, so that a run with no STRATA_LIVE_SECOND_PROJECT keeps pebbles. It is in the
# state, and in no file of the repository.
output "second_project" {
  value = var.second_project
}

output "second_project_uid" {
  value = try(data.spectrocloud_project.second[0].id, "")
}

output "workload_uid" {
  value = try(spectrocloud_cluster_aws.workload[0].id, "")
}

output "workload_name" {
  value = var.workload_name
}

output "workload_release_uid" {
  value = try(spectrocloud_cluster_profile.workload_release[0].id, "")
}

# The add-on profiles that pebbles runs now, by key.
output "workload_on_cluster" {
  value = local.workload_on_cluster
}

# The profiles of wilma that pebbles runs as a copy, by team. Each has the name, the version, the uid
# of the copy, and the uid on wilma.
output "copies" {
  value = { for t, p in spectrocloud_cluster_profile.copy : t => {
    name      = p.name
    version   = p.version
    uid       = p.id
    wilma_uid = spectrocloud_cluster_profile.team[t].id
  } }
}

output "shared_profile_uid" {
  value = try(spectrocloud_cluster_profile.shared[0].id, "")
}

output "shared_profile_name" {
  value = try(spectrocloud_cluster_profile.shared[0].name, "")
}

# The selector of the set of the observer: the tag of both clusters.
output "set_selector" {
  value = "tag:${replace(local.set_tag, ":", "=")}"
}

# Every profile name that pebbles uses in the second project, for platform.sh down, whether or not
# the profiles exist now.
output "workload_profile_names" {
  value = local.boundary ? concat(["${var.workload_name}-release", "${var.workload_name}-apps"], [for t in local.copied_teams : "${var.cluster_name}-${t}"]) : []
}
