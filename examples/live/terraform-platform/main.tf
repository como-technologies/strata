# The platform cluster wilma (#700): the cluster profile of the lab and seven add-on profiles, one
# for each team of a platform group. Each profile pins the community packs of one team, and a
# manifest layer holds the glue that the team writes: glue.tf. The pins are in terraform.tfvars of
# this folder, and `examples/live/platform.sh pins` writes them from the tenant.
#
# The state is apart from the live example: `just live-platform-up` builds it, and
# `just live-platform-down` removes it. A normal live window costs the same as before. The cluster
# needs the release profile of the lab, so `just live-profiles` or `just live-up` runs first, and
# `just live-down` runs after `just live-platform-down`.
#
# The variables of the cases in variables.tf change one thing each. platform.sh sets them, and
# the defaults are the baseline of wilma. Billable while it exists.

locals {
  tags = ["purpose:strata-live-platform", "managed-by:opentofu"]

  # The packs of each profile, in the order of the profile. The table of #700 gives them.
  teams = {
    security      = ["cert-manager", "vault", "external-secrets-operator", "kyverno"]
    network       = ["envoy-gateway", "external-dns"]
    data          = ["cloudnative-pg"]
    identity      = ["keycloak"]
    gitops        = ["argo-cd"]
    observability = ["prometheus-operator", "grafana", "loki", "fluentbit"]
    control       = ["crossplane"]
  }
  # The profiles that declare the profile variable domain.
  domain_teams = ["network", "identity", "gitops", "observability"]
  # A manifest layer of glue installs after each pack of priority 0, because it needs their CRDs.
  # Keycloak installs after the glue, because the glue makes its database. A pack waits for each
  # pack of a lower priority.
  glue_priority     = "10"
  keycloak_priority = "20"
  # The lab values of the SSO and of the Vault server in dev mode. Each is a value of the lab, and
  # no secret: the zone is private, and the records of the cases hold them.
  vault_token = "strata-live-${var.cluster_name}"
  sso_secret  = { argo-cd = "strata-live-${var.cluster_name}-argo-cd", grafana = "strata-live-${var.cluster_name}-grafana" }
  realm       = "wilma"
  domain      = "{{.spectro.var.domain}}"
  issuer      = "https://keycloak.${local.domain}/realms/${local.realm}"
}

data "spectrocloud_project" "this" {
  name = var.palette_project
}

data "spectrocloud_cloudaccount_aws" "this" {
  name = var.cloud_account_name
}

data "spectrocloud_cluster_profile" "release" {
  name    = var.release_profile_name
  version = var.release_profile_version
}

data "spectrocloud_registry" "this" {
  for_each = toset([for p in values(var.packs) : p.registry])
  name     = each.value
}

data "spectrocloud_pack" "pin" {
  for_each     = var.packs
  name         = each.key
  version      = each.value.version
  registry_uid = data.spectrocloud_registry.this[each.value.registry].id
}

# The other version of cert-manager for case 2, from the registry of the pin.
data "spectrocloud_pack" "other_cert_manager" {
  count        = var.network_cert_manager == "" ? 0 : 1
  name         = "cert-manager"
  version      = var.network_cert_manager
  registry_uid = data.spectrocloud_registry.this[var.packs["cert-manager"].registry].id
}

# ---- the values -----------------------------------------------------------------------------------
#
# Most packs keep the values that the registry lists. Six packs change some values, each chart of a
# Helm registry gets a namespace, and the glue reads the namespace of six. The values of a chart are
# under charts.<chart> in a pack of Spectro Cloud, and at the top in a chart of a Helm registry. A
# patch goes into the values of the chart, three levels deep. A map of a patch merges into the map
# of the chart. Each other value replaces the value of the chart. The text that a patch gives is
# YAML with no comments.

locals {
  # Docker Hub no longer serves the images of docker.io/bitnami that the charts of the registry
  # Bitnami name. The repository bitnamilegacy holds each of them with the same tag. So each
  # repository bitnami/ of such a chart becomes bitnamilegacy/, at each level of its values.
  bitnami       = [for k, p in var.packs : k if p.registry == "Bitnami"]
  source_values = { for k, d in data.spectrocloud_pack.pin : k => contains(local.bitnami, k) ? replace(d.values, "/(?m)^( *repository: *)bitnami//", "$${1}bitnamilegacy/") : d.values }
  # Some of these charts refuse an image that their list of images does not name, unless this
  # value is true.
  insecure_images = { global = { security = { allowInsecureImages = true } } }

  sso_realm = {
    realm   = local.realm
    enabled = true
    clients = [for app, host in { argo-cd = "argocd", grafana = "grafana" } : {
      clientId                  = app
      secret                    = local.sso_secret[app]
      protocol                  = "openid-connect"
      publicClient              = false
      standardFlowEnabled       = true
      directAccessGrantsEnabled = false
      redirectUris              = ["https://${host}.${local.domain}/*"]
    }]
  }

  patches = {
    # A server in dev mode is unsealed, so its pod is ready, and External Secrets reads it with the
    # token of the lab.
    vault = { server = { dev = { enabled = true, devRootToken = local.vault_token } } }
    # A Bitnami chart takes the provider as a name; the chart of external-dns takes it as a map.
    external-dns = merge(concat(
      [{
        domainFilters = [local.domain]
        sources       = ["gateway-httproute"]
        txtOwnerId    = var.cluster_name
        policy        = "sync"
      }],
      [for c in [local.chart["external-dns"]] : { provider = "aws", aws = { zoneType = "private", region = var.region } } if !can(keys(c.provider))],
      [for c in [local.chart["external-dns"]] : {
        provider  = { name = "aws" }
        extraArgs = ["--aws-zone-type=private"]
        env       = [{ name = "AWS_DEFAULT_REGION", value = var.region }]
      } if can(keys(c.provider))],
    )...)
    keycloak = merge(
      {
        postgresql = { enabled = false }
        externalDatabase = {
          host                      = "keycloak-db-rw"
          port                      = 5432
          user                      = "keycloak"
          database                  = "keycloak"
          existingSecret            = "keycloak-db-app"
          existingSecretPasswordKey = "password"
        }
        tls = { enabled = true, existingSecret = "keycloak-tls", usePem = true }
      },
      [for r in [local.sso_realm] : { keycloakConfigCli = { enabled = true, configuration = { "wilma.json" = jsonencode(r) } } } if var.sso]...
    )
    grafana = merge({}, [for s in [local.sso_secret.grafana] : {
      grafana = {
        extraEnvVars = [for k, v in {
          GF_SERVER_ROOT_URL                  = "https://grafana.${local.domain}"
          GF_AUTH_GENERIC_OAUTH_ENABLED       = "true"
          GF_AUTH_GENERIC_OAUTH_NAME          = "Keycloak"
          GF_AUTH_GENERIC_OAUTH_CLIENT_ID     = "grafana"
          GF_AUTH_GENERIC_OAUTH_CLIENT_SECRET = s
          GF_AUTH_GENERIC_OAUTH_SCOPES        = "openid profile email"
          GF_AUTH_GENERIC_OAUTH_AUTH_URL      = "${local.issuer}/protocol/openid-connect/auth"
          GF_AUTH_GENERIC_OAUTH_TOKEN_URL     = "${local.issuer}/protocol/openid-connect/token"
          GF_AUTH_GENERIC_OAUTH_API_URL       = "${local.issuer}/protocol/openid-connect/userinfo"
        } : { name = k, value = v }]
      }
    } if var.sso]...)
    argo-cd = merge({}, [for s in [local.sso_secret.argo-cd] : {
      configs = {
        cm = {
          url = "https://argocd.${local.domain}"
          "oidc.config" = yamlencode({
            name            = "Keycloak"
            issuer          = local.issuer
            clientID        = "argo-cd"
            clientSecret    = s
            requestedScopes = ["openid", "profile", "email"]
          })
        }
      }
    } if var.sso]...)
    # Palette requires a password of Grafana in this pack: ClusterProfileCreationValidationFailure.
    prometheus-operator = { grafana = { adminPassword = random_password.grafana.result } }
  }
  # The values of a chart of a Helm registry start with `pack.namespace: ""`. Palette refuses a
  # profile that gives such a pack no namespace: ClusterProfileCreationValidationFailure. So each
  # such chart gets the pack section below, with no patch. Each pack of the registry Bitnami also
  # gets insecure_images.
  helm_charts = [for k, d in data.spectrocloud_pack.pin : k if try(yamldecode(d.values).pack.namespace, null) == ""]
  patched = { for k in setunion(local.helm_charts, local.bitnami, keys(local.patches)) : k => merge(
    try(local.patches[k], {}),
    [for i in [local.insecure_images] : i if contains(local.bitnami, k)]...
  ) }
  # The pack section of a patched pack: the namespace when its values name none, and the priority.
  # A pack with no default namespace here gets its name.
  default_namespaces = { cert-manager = "cert-manager", vault = "vault", external-dns = "external-dns", keycloak = "keycloak", grafana = "grafana", argo-cd = "argocd", prometheus-operator = "monitoring" }
  pack_extra         = { keycloak = { "spectrocloud.com/install-priority" = local.keycloak_priority } }

  decoded    = { for k in setunion(keys(local.default_namespaces), local.helm_charts, local.bitnami) : k => yamldecode(local.source_values[k]) }
  wrapped    = { for k, d in local.decoded : k => can(keys(d.charts)) }
  chart_key  = { for k, d in local.decoded : k => try(keys(d.charts)[0], "") }
  chart      = { for k, d in local.decoded : k => try(d.charts[local.chart_key[k]], d) }
  namespaces = { for k, d in local.decoded : k => try(coalesce(d.pack.namespace), lookup(local.default_namespaces, k, k)) }

  patched_chart = { for p, patch in local.patched : p => {
    for k, v in merge(local.chart[p], patch) : k => try(
      { for k2, v2 in merge(local.chart[p][k], patch[k]) : k2 => try(merge(local.chart[p][k][k2], patch[k][k2]), v2) },
    v)
  } }
  patched_values = { for p, c in local.patched_chart : p => yamlencode(merge(concat(
    [local.decoded[p]],
    [for w in [local.wrapped[p]] : { charts = merge(local.decoded[p].charts, { (local.chart_key[p]) = c }) } if w],
    [for w in [local.wrapped[p]] : c if !w],
    [{ pack = merge(try(local.decoded[p].pack, {}), { namespace = local.namespaces[p] }, try(local.pack_extra[p], {})) }],
  )...)) }
  pack_values = { for k, v in local.source_values : k => lookup(local.patched_values, k, v) }

  # The images that the values of each pin name, for `platform.sh images` before the apply. They are
  # each image of pack.content.images, and each map with a repository and a tag in the first five
  # levels. No patch names an image. The password of Grafana is not known before the first apply, so
  # the images come from source_values, and not from pack_values.
  source_docs = { for k, v in local.source_values : k => yamldecode(v) }
  maps_1      = { for k, d in local.source_docs : k => [for c in try(values(d), []) : c if can(keys(c))] }
  maps_2      = { for k, l in local.maps_1 : k => flatten([for m in l : [for c in values(m) : c if can(keys(c))]]) }
  maps_3      = { for k, l in local.maps_2 : k => flatten([for m in l : [for c in values(m) : c if can(keys(c))]]) }
  maps_4      = { for k, l in local.maps_3 : k => flatten([for m in l : [for c in values(m) : c if can(keys(c))]]) }
  maps_5      = { for k, l in local.maps_4 : k => flatten([for m in l : [for c in values(m) : c if can(keys(c))]]) }
  # A map names its image as a chart of Bitnami does: registry/repository, then @digest, or :tag
  # when the digest is empty. A map with no registry takes the registry that the values give each
  # image of a chart, global.image.registry, as the chart of kyverno does. With no registry, the
  # image has no host, and the container runtime asks Docker Hub.
  global_registry = { for k, d in local.source_docs : k => try(coalesce(concat(
    [try(tostring(d.global.image.registry), null)],
    [for c in try(values(d.charts), []) : try(tostring(c.global.image.registry), null)],
  )...), "") }
  images = { for k, d in local.source_docs : k => sort(distinct(concat(
    [for i in try(d.pack.content.images, []) : i.image if can(tostring(i.image))],
    [for m in concat(local.maps_1[k], local.maps_2[k], local.maps_3[k], local.maps_4[k], local.maps_5[k]) : format("%s%s",
      join("/", compact([try(coalesce(try(tostring(m.registry), null), local.global_registry[k]), ""), m.repository])),
      try(length(tostring(m.digest)) > 0, false) ? "@${m.digest}" : ":${m.tag}",
    ) if try(length(tostring(m.repository)) > 0 && length(tostring(m.tag)) > 0, false)],
  ))) }
}

# The password of Grafana in prometheus-operator. OpenTofu makes it at the first apply, and keeps it
# in the state, so a plan with the defaults changes nothing. No file of the repository holds it.
resource "random_password" "grafana" {
  length  = 24
  special = false
}

# ---- the profiles ---------------------------------------------------------------------------------

locals {
  # One pack block for each pin of a team.
  pins = { for k, p in var.packs : k => {
    name         = k
    type         = p.type
    tag          = p.version
    uid          = data.spectrocloud_pack.pin[k].id
    registry_uid = data.spectrocloud_registry.this[p.registry].id
    values       = local.pack_values[k]
  } }
  # Case 2: cert-manager at another version, in place in wilma-network 1.0.0, and in 1.1.0.
  other_cert_manager = [for d in data.spectrocloud_pack.other_cert_manager : merge(local.pins["cert-manager"], {
    tag    = var.network_cert_manager
    uid    = d.id
    values = d.values
  })]
  team_packs = { for team, packs in local.teams : team => concat(
    [for p in packs : local.pins[p]],
    [for p in local.other_cert_manager : p if team == "network" && var.network_cert_manager_in_place],
  ) }
  # Case 3: wilma-observability 1.1.0 with no fluentbit, and wilma-logging with fluentbit alone.
  moved_packs = {
    observability = [for p in local.teams.observability : local.pins[p] if p != "fluentbit"]
    logging       = [local.pins["fluentbit"]]
  }
  glue_values = "pack:\n  spectrocloud.com/install-priority: \"${local.glue_priority}\"\n"
}

resource "spectrocloud_cluster_profile" "team" {
  for_each     = local.team_packs
  name         = "${var.cluster_name}-${each.key}"
  version      = "1.0.0"
  description  = "strata live platform: the packs of the team ${each.key}, and its glue"
  cloud        = "all"
  type         = "add-on"
  skip_destroy = var.immutable_profiles

  dynamic "pack" {
    for_each = each.value
    content {
      name         = pack.value.name
      type         = pack.value.type
      tag          = pack.value.tag
      uid          = pack.value.uid
      registry_uid = pack.value.registry_uid
      values       = pack.value.values
    }
  }

  pack {
    name   = "${var.cluster_name}-${each.key}-glue"
    type   = "manifest"
    tag    = "1.0.0"
    values = local.glue_values
    manifest {
      name    = "${var.cluster_name}-${each.key}-glue"
      content = local.glue[each.key]
    }
  }

  dynamic "profile_variables" {
    for_each = contains(local.domain_teams, each.key) ? [1] : []
    content {
      variable {
        name          = "domain"
        display_name  = "Domain"
        format        = "string"
        description   = "The domain of the platform: the private zone of external-dns"
        default_value = var.domain
        required      = false
      }
    }
  }

  lifecycle {
    create_before_destroy = true
  }
}

# Case 2: a new version of wilma-network with cert-manager at another version. No cluster runs it.
resource "spectrocloud_cluster_profile" "network_next" {
  count        = var.network_cert_manager == "" ? 0 : 1
  name         = "${var.cluster_name}-network"
  version      = "1.1.0"
  description  = "strata live platform: wilma-network with a second cert-manager; no cluster runs it"
  cloud        = "all"
  type         = "add-on"
  skip_destroy = var.immutable_profiles

  dynamic "pack" {
    for_each = concat([for p in local.teams.network : local.pins[p]], local.other_cert_manager)
    content {
      name         = pack.value.name
      type         = pack.value.type
      tag          = pack.value.tag
      uid          = pack.value.uid
      registry_uid = pack.value.registry_uid
      values       = pack.value.values
    }
  }

  pack {
    name   = "${var.cluster_name}-network-glue"
    type   = "manifest"
    tag    = "1.0.0"
    values = local.glue_values
    manifest {
      name    = "${var.cluster_name}-network-glue"
      content = local.glue.network
    }
  }

  profile_variables {
    variable {
      name          = "domain"
      display_name  = "Domain"
      format        = "string"
      description   = "The domain of the platform: the private zone of external-dns"
      default_value = var.domain
      required      = false
    }
  }

  depends_on = [spectrocloud_cluster_profile.team]

  lifecycle {
    create_before_destroy = true
  }
}

# Case 3: the profiles of the move. The content of fluentbit does not change.
resource "spectrocloud_cluster_profile" "moved" {
  for_each     = { for k, v in local.moved_packs : k => v if var.move_fluentbit }
  name         = "${var.cluster_name}-${each.key}"
  version      = each.key == "observability" ? "1.1.0" : "1.0.0"
  description  = "strata live platform: ${each.key} after the move of fluentbit"
  cloud        = "all"
  type         = "add-on"
  skip_destroy = var.immutable_profiles

  dynamic "pack" {
    for_each = each.value
    content {
      name         = pack.value.name
      type         = pack.value.type
      tag          = pack.value.tag
      uid          = pack.value.uid
      registry_uid = pack.value.registry_uid
      values       = pack.value.values
    }
  }

  pack {
    name   = "${var.cluster_name}-${each.key}-glue"
    type   = "manifest"
    tag    = "1.0.0"
    values = local.glue_values
    manifest {
      name    = "${var.cluster_name}-${each.key}-glue"
      content = local.glue[each.key == "observability" ? "observability_moved" : "logging"]
    }
  }

  dynamic "profile_variables" {
    for_each = each.key == "observability" ? [1] : []
    content {
      variable {
        name          = "domain"
        display_name  = "Domain"
        format        = "string"
        description   = "The domain of the platform: the private zone of external-dns"
        default_value = var.domain
        required      = false
      }
    }
  }

  depends_on = [spectrocloud_cluster_profile.team]

  lifecycle {
    create_before_destroy = true
  }
}

# ---- the cluster ----------------------------------------------------------------------------------

locals {
  # The add-on profiles that wilma runs, by team, for the variables of the cases.
  on_cluster = { for team, uid in merge(
    { for team, p in spectrocloud_cluster_profile.team : team => p.id
    if !(team == "data" && !var.data_profile) && !(team == "observability" && var.move_fluentbit) },
    { for team, p in spectrocloud_cluster_profile.moved : team => p.id },
  ) : team => uid if var.addons }
}

resource "spectrocloud_cluster_aws" "wilma" {
  name             = var.cluster_name
  cloud_account_id = data.spectrocloud_cloudaccount_aws.this.id
  tags             = local.tags

  cluster_profile {
    id = data.spectrocloud_cluster_profile.release.id
  }

  dynamic "cluster_profile" {
    for_each = local.on_cluster
    content {
      id = cluster_profile.value
      # The value that wilma gives the variable domain, when case 4 sets one.
      variables = contains(local.domain_teams, cluster_profile.key) && var.cluster_domain != "" ? { domain = var.cluster_domain } : null
    }
  }

  cloud_config {
    region       = var.region
    ssh_key_name = var.ssh_key_name
  }

  machine_pool {
    name          = "cp-pool"
    count         = 1
    control_plane = true
    instance_type = var.instance_type
    azs           = [var.az]
  }

  machine_pool {
    name          = "worker-pool"
    count         = var.worker_count
    instance_type = var.instance_type
    azs           = [var.az]
  }

  timeouts {
    create = "60m"
    delete = "45m"
  }
}

# ---- the private zone of external-dns -------------------------------------------------------------
#
# The zone is in the VPC of wilma, which Palette makes with the cluster. The nodes write to it
# through the policy below, which this state adds to the role of the nodes and removes. A destroy
# removes the zone and the policy before the cluster.

data "aws_vpc" "wilma" {
  filter {
    name   = "tag-key"
    values = ["sigs.k8s.io/cluster-api-provider-aws/cluster/${var.cluster_name}"]
  }
  depends_on = [spectrocloud_cluster_aws.wilma]
}

resource "aws_route53_zone" "private" {
  name          = var.domain
  comment       = "strata live platform: the private zone of external-dns on ${var.cluster_name}"
  force_destroy = true
  vpc {
    vpc_id = data.aws_vpc.wilma.id
  }
  tags = { purpose = "strata-live-platform" }
}

resource "aws_iam_role_policy" "external_dns" {
  name = var.node_policy_name
  role = var.node_role_name
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      { Effect = "Allow", Action = ["route53:ChangeResourceRecordSets", "route53:ListResourceRecordSets"], Resource = [aws_route53_zone.private.arn] },
      { Effect = "Allow", Action = ["route53:ListHostedZones", "route53:ListTagsForResource"], Resource = ["*"] },
    ]
  })
}
