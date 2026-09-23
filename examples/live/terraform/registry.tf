# The example's own OCI pack registry (#66): an ECR repository in the account that aws_profile
# names, and a Palette registry that reads it. examples/live/registry.sh pushes a pack into it,
# pushes the same version again, and pushes a new version, which no shared registry may see.
# live-down removes it all: the repository with its images, the user that reads it, and the
# registry.

locals {
  pack_name     = "strata-live-pack"
  registry_host = "${data.aws_caller_identity.this.account_id}.dkr.ecr.${var.region}.amazonaws.com"
  registry_tags = {
    purpose      = "strata-live-example"
    "managed-by" = "opentofu"
  }
}

data "aws_caller_identity" "this" {}

# Palette reads a pack at <base content path>/spectro-packs/archive/<name>:<version>, and ECR
# holds one repository per name.
resource "aws_ecr_repository" "pack" {
  name                 = "${var.name_prefix}/spectro-packs/archive/${local.pack_name}"
  image_tag_mutability = "MUTABLE" # a push of one version again is what registry.sh tests
  force_delete         = true      # the images go with the repository at live-down
  tags                 = local.registry_tags
}

# The user Palette reads the repository with. The actions are the ones the Palette documentation
# lists for an ECR pack registry: read only.
resource "aws_iam_user" "registry" {
  name = "${var.name_prefix}-registry"
  tags = local.registry_tags
}

resource "aws_iam_access_key" "registry" {
  user = aws_iam_user.registry.name
}

data "aws_iam_policy_document" "registry_read" {
  statement {
    actions = [
      "ecr:BatchCheckLayerAvailability",
      "ecr:BatchGetImage",
      "ecr:DescribeImageReplicationStatus",
      "ecr:DescribeImageScanFindings",
      "ecr:DescribeImages",
      "ecr:DescribeRegistry",
      "ecr:DescribeRepositories",
      "ecr:GetAuthorizationToken",
      "ecr:GetDownloadUrlForLayer",
      "ecr:GetLifecyclePolicy",
      "ecr:GetLifecyclePolicyPreview",
      "ecr:GetRegistryPolicy",
      "ecr:GetRepositoryPolicy",
      "ecr:ListImages",
      "ecr:ListTagsForResource",
    ]
    resources = ["*"]
  }
}

resource "aws_iam_user_policy" "registry_read" {
  name   = "read-packs"
  user   = aws_iam_user.registry.name
  policy = data.aws_iam_policy_document.registry_read.json
}

# A new access key takes some seconds to work everywhere in AWS. Palette checks the key when the
# registry is created, and refuses one it cannot use yet, so the registry waits.
resource "time_sleep" "key" {
  depends_on      = [aws_iam_access_key.registry, aws_iam_user_policy.registry_read]
  create_duration = "30s"
}

# The Palette registry over the repository.
resource "spectrocloud_registry_oci" "pack" {
  name               = "${var.name_prefix}-registry"
  type               = "ecr"
  endpoint           = local.registry_host
  base_content_path  = var.name_prefix
  provider_type      = "pack"
  is_private         = true
  is_synchronization = true

  credentials {
    credential_type = "secret"
    access_key      = aws_iam_access_key.registry.id
    secret_key      = aws_iam_access_key.registry.secret
    # The provider reads this block back whether or not it was given, so a plan without it
    # changes the registry every time.
    tls_config {
      insecure_skip_verify = false
    }
  }

  depends_on = [time_sleep.key, aws_ecr_repository.pack]
}
