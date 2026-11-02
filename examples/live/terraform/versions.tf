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
    random = {
      source  = "hashicorp/random"
      version = "~> 3.6"
    }
    external = {
      source  = "hashicorp/external"
      version = "~> 2.3"
    }
  }
}

# The example's own OCI pack registry is an ECR repository in the account that aws_profile names.
# The clusters use the cloud account of Palette and need no AWS credential here. An EKS cluster is
# the exception: its cloud account is a role in the same account. With own_registries false and a
# cloud other than eks, the module makes no AWS resource. The provider then reads no profile and
# calls no account (#531).
locals {
  aws_needed = var.own_registries || var.cloud == "eks"
}

provider "aws" {
  region                      = var.region
  profile                     = local.aws_needed ? var.aws_profile : null
  skip_credentials_validation = !local.aws_needed
  skip_requesting_account_id  = !local.aws_needed
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
