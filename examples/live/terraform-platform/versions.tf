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
    random = {
      source  = "hashicorp/random"
      version = "~> 3.6"
    }
  }
}

# The private zone of external-dns and the policy that lets the nodes write to it are in the
# account that aws_profile names.
provider "aws" {
  region  = var.region
  profile = var.aws_profile
}

# Auth: SPECTROCLOUD_APIKEY rides the environment; examples/live/tf.sh sets it from
# PALETTE_API_KEY or ~/.palette.env. Never in tfvars.
provider "spectrocloud" {
  project_name = var.palette_project
  feature_preview = {
    "immutable-clusterprofiles" = var.immutable_profiles
  }
}
