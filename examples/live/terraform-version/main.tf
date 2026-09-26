# One version of an add-on profile, in a state of its own, for examples/live/version-states.sh
# (#81). Each version gets its own tofu workspace, so a new version is a new state and a new
# resource, and the state of the version before it is not touched. No feature_preview flag.
terraform {
  required_version = ">= 1.6"
  required_providers {
    spectrocloud = {
      source  = "spectrocloud/spectrocloud"
      version = "0.30.1"
    }
  }
}

# Auth: SPECTROCLOUD_APIKEY rides the environment; version-states.sh sets it as tf.sh does.
provider "spectrocloud" {
  project_name = var.palette_project
}

variable "palette_project" {
  description = "Palette project the profile lands in"
  type        = string
}

variable "name" {
  description = "The name of the profile: one name, a version in each state"
  type        = string
  default     = "strata-live-versions"
}

variable "profile_version" {
  description = "The version this state holds"
  type        = string
}

variable "value" {
  description = "The value that the manifest of this version carries"
  type        = string
}

resource "spectrocloud_cluster_profile" "version" {
  name        = var.name
  version     = var.profile_version
  description = "strata live example: one version in one state"
  cloud       = "all"
  type        = "add-on"

  pack {
    name   = "strata-live-versions"
    type   = "manifest"
    tag    = "1.0.0"
    values = ""
    manifest {
      name    = "strata-live-versions"
      content = <<-YAML
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: strata-live-versions
          namespace: default
        data:
          value: "${var.value}"
      YAML
    }
  }
}

output "uid" {
  value = spectrocloud_cluster_profile.version.id
}
