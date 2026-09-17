terraform {
  required_version = ">= 1.5"
  required_providers {
    spectrocloud = {
      source  = "spectrocloud/spectrocloud"
      version = "0.30.1"
    }
  }
}

# Auth: SPECTROCLOUD_APIKEY rides the environment; examples/live/tf.sh sets it from
# PALETTE_API_KEY or ~/.palette.env. Never in tfvars.
provider "spectrocloud" {
  project_name = var.palette_project
  # Off by default, as in the recorded runs; examples/live/in-place.sh runs with it off and on.
  feature_preview = {
    "immutable-clusterprofiles" = var.immutable_profiles
  }
}
