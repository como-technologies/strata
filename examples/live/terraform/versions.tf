terraform {
  required_version = ">= 1.6"
  required_providers {
    spectrocloud = {
      source  = "spectrocloud/spectrocloud"
      version = "0.30.1"
    }
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
    time = {
      source  = "hashicorp/time"
      version = "~> 0.12"
    }
  }
}

# The example's own OCI pack registry is an ECR repository in the account that aws_profile names.
# The clusters use Palette's cloud account and need no AWS credential here.
provider "aws" {
  region  = var.region
  profile = var.aws_profile
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
