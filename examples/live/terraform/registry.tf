# The example's own OCI pack registry (#66): an ECR repository in the account that aws_profile
# names, and a Palette registry that reads it. examples/live/registry.sh pushes a pack into it,
# pushes the same version again, and pushes a new version, which no shared registry may see.
# live-down removes it all: the repository with its images, the user that reads it, and the
# registry.

locals {
  pack_name     = "strata-live-pack"
  dep_name      = "strata-live-dep"
  chart_name    = "strata-live-chart"
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

# The same account holds the example's own OCI Helm registry: one chart, at
# <base content path>/<chart>, as helm push puts it.
# The second pack of the pack registry: the one that strata-live-pack 1.1.0 requires (registry.sh,
# step 6).
resource "aws_ecr_repository" "dep" {
  name                 = "${var.name_prefix}/spectro-packs/archive/${local.dep_name}"
  image_tag_mutability = "MUTABLE"
  force_delete         = true
  tags                 = local.registry_tags
}

resource "aws_ecr_repository" "chart" {
  name                 = "${var.name_prefix}-helm/${local.chart_name}"
  image_tag_mutability = "MUTABLE"
  force_delete         = true
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

  depends_on = [time_sleep.key, aws_ecr_repository.pack, aws_ecr_repository.dep]
}

# The Palette registry over the chart repository: the same repository host, read as Helm charts.
# Palette keeps the digest it listed first for a chart version, so registry.sh removes this
# registry and makes it again before it pushes: count 0, then 1. A -replace cannot do it: the
# profiles' create_before_destroy reaches this resource, and the new registry would be made under
# the name of the old one.
moved {
  from = spectrocloud_registry_oci.helm
  to   = spectrocloud_registry_oci.helm[0]
}

resource "spectrocloud_registry_oci" "helm" {
  count              = var.owned_helm_registry ? 1 : 0
  name               = "${var.name_prefix}-helm-registry"
  type               = "ecr"
  endpoint           = local.registry_host
  base_content_path  = "${var.name_prefix}-helm"
  provider_type      = "helm"
  is_private         = true
  is_synchronization = true

  credentials {
    credential_type = "secret"
    access_key      = aws_iam_access_key.registry.id
    secret_key      = aws_iam_access_key.registry.secret
    tls_config {
      insecure_skip_verify = false
    }
  }

  depends_on = [time_sleep.key, aws_ecr_repository.chart]
}
