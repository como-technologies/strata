# A cluster template (#66): a maintenance policy, and a template that links the release version
# and the add-on version. Palette locks a profile version that a template links. template.sh
# links the versions before any cluster runs them, tries an edit in place, and then builds a
# cluster outside the template with the linked versions. With cluster_in_template, the cluster
# is built in the template instead, and takes its profiles (#276). Everything here goes with
# live-down.

resource "spectrocloud_cluster_config_policy" "maintenance" {
  count   = var.template ? 1 : 0
  name    = "${var.name_prefix}-maintenance"
  context = "project"
  tags    = ["purpose:strata-live-example", "managed-by:opentofu"]

  schedules {
    name         = "sunday"
    start_cron   = "0 2 * * SUN"
    duration_hrs = 4
  }
}

resource "spectrocloud_cluster_config_template" "release" {
  count      = var.template ? 1 : 0
  name       = "${var.name_prefix}-template"
  cloud_type = var.cloud
  context    = "project"
  tags       = ["purpose:strata-live-example", "managed-by:opentofu"]

  policy {
    id   = spectrocloud_cluster_config_policy.maintenance[0].id
    kind = "maintenance"
  }

  cluster_profile {
    id = spectrocloud_cluster_profile.release.id
  }

  cluster_profile {
    id = spectrocloud_cluster_profile.addon.id
  }
}

# A second add-on profile, one manifest, for the install order of add-on packs across profiles.
# The cluster runs it beside the first when second_addon is set.
resource "spectrocloud_cluster_profile" "addon2" {
  count        = var.second_addon ? 1 : 0
  name         = "${var.name_prefix}-addon2"
  version      = "1.0.0"
  description  = "strata live example: a second add-on profile, for the install order"
  cloud        = "all"
  type         = "add-on"
  skip_destroy = var.immutable_profiles

  pack {
    name   = "strata-live-config2"
    type   = "manifest"
    tag    = "1.0.0"
    values = ""
    manifest {
      name    = "strata-live-config2"
      content = <<-EOT
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: strata-live-2
          namespace: default
        data:
          purpose: strata live example, second add-on
      EOT
    }
  }

  lifecycle {
    create_before_destroy = true
  }
}
