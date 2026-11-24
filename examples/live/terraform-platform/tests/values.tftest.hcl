# The values that Palette requires of the packs of wilma (#1004). The providers of Palette and AWS
# are mocks, so the test sends no request. `just live-platform-values` runs it.
#
# Palette refuses a profile with ClusterProfileCreationValidationFailure in two cases. A chart of a
# Helm registry gives no value to `pack.namespace`: Palette starts the values of each such chart
# with `pack.namespace: ""`. Or prometheus-operator gives Grafana no password. Each run gives each
# pin the values of one form.
#
# A chart of the registry Bitnami names its images in docker.io/bitnami, which Docker Hub no longer
# serves (#700). The third run tests the images of bitnamilegacy, and the output images. The fourth
# run tests an image map with no registry, as in the values of kyverno.
#
# The next three runs test pebbles in a second project (#701), and wilma with no second project.
# They also test the move of Vault of case 2 of platform-boundary.sh. The last run tests that each
# profile whose glue names the variable domain declares it.

mock_provider "spectrocloud" {}
mock_provider "spectrocloud" {
  alias = "second"
}
mock_provider "aws" {}

variables {
  palette_project    = "project"
  cloud_account_name = "account"
  region             = "us-east-2"
  az                 = "us-east-2a"
  ssh_key_name       = "key"
  aws_profile        = "profile"
  packs = {
    cert-manager              = { registry = "Public Repo", version = "1.0.0", type = "spectro" }
    vault                     = { registry = "Palette Registry", version = "1.0.0", type = "spectro" }
    external-secrets-operator = { registry = "Public Repo", version = "1.0.0", type = "spectro" }
    kyverno                   = { registry = "Public Repo", version = "1.0.0", type = "spectro" }
    envoy-gateway             = { registry = "Bitnami", version = "1.0.0", type = "helm" }
    external-dns              = { registry = "Bitnami", version = "1.0.0", type = "helm" }
    cloudnative-pg            = { registry = "Bitnami", version = "1.0.0", type = "helm" }
    keycloak                  = { registry = "Bitnami", version = "1.0.0", type = "helm" }
    argo-cd                   = { registry = "Public Repo", version = "1.0.0", type = "spectro" }
    prometheus-operator       = { registry = "Public Repo", version = "1.0.0", type = "spectro" }
    grafana                   = { registry = "Bitnami", version = "1.0.0", type = "helm" }
    loki                      = { registry = "Palette Community Registry", version = "1.0.0", type = "spectro" }
    fluentbit                 = { registry = "Public Repo", version = "1.0.0", type = "spectro" }
    crossplane                = { registry = "Public Repo", version = "1.0.0", type = "spectro" }
  }
}

# Each pin has the values of a chart of a Helm registry.
run "each_chart_of_a_helm_registry_gets_a_namespace" {
  command = apply

  override_data {
    target = data.spectrocloud_pack.pin
    values = { values = "pack:\n  namespace: \"\"\nreplicaCount: 1\n" }
  }

  assert {
    condition     = yamldecode(local.pack_values["envoy-gateway"]).pack.namespace == "envoy-gateway"
    error_message = "envoy-gateway gets no pack.namespace"
  }
  assert {
    condition     = yamldecode(local.pack_values["external-dns"]).pack.namespace == "external-dns"
    error_message = "external-dns gets no pack.namespace"
  }
  assert {
    condition     = yamldecode(local.pack_values["cloudnative-pg"]).pack.namespace == "cloudnative-pg"
    error_message = "cloudnative-pg gets no pack.namespace"
  }
  assert {
    condition     = yamldecode(local.pack_values["keycloak"]).pack.namespace == "keycloak"
    error_message = "keycloak gets no pack.namespace"
  }
  assert {
    condition     = yamldecode(local.pack_values["grafana"]).pack.namespace == "grafana"
    error_message = "grafana gets no pack.namespace"
  }
  assert {
    condition     = yamldecode(local.pack_values["keycloak"]).pack["spectrocloud.com/install-priority"] == local.keycloak_priority
    error_message = "keycloak loses its install priority"
  }
}

# Each pin has the values of a pack of Spectro Cloud: a namespace, and a Grafana with no password.
run "prometheus_operator_gets_a_password_of_grafana" {
  command = apply

  override_data {
    target = data.spectrocloud_pack.pin
    values = { values = "pack:\n  namespace: \"monitoring\"\ncharts:\n  kube-prometheus-stack:\n    grafana:\n      adminPassword: \"\"\n" }
  }

  assert {
    condition     = length(yamldecode(local.pack_values["prometheus-operator"]).charts.kube-prometheus-stack.grafana.adminPassword) == 24
    error_message = "prometheus-operator gives Grafana no password"
  }
  assert {
    condition     = local.pack_values["cert-manager"] == data.spectrocloud_pack.pin["cert-manager"].values
    error_message = "cert-manager does not keep the values of its registry"
  }
}

# Each pin has the values of a chart of Bitnami. They name an image at the top and an image one level
# down. They also have a map with a digest, a map with no tag, and the list of images of a pack.
run "each_chart_of_bitnami_names_the_images_of_bitnamilegacy" {
  command = apply

  override_data {
    target = data.spectrocloud_pack.pin
    values = {
      values = <<-EOT
        pack:
          namespace: ""
          content:
            images:
              - image: quay.io/team/tool:v1
        global:
          imageRegistry: ""
          security:
            allowInsecureImages: false
        image:
          registry: docker.io
          repository: bitnami/grafana
          tag: 12.1.1-debian-12-r0
          digest: ""
        volumePermissions:
          enabled: false
          image:
            registry: docker.io
            repository: bitnami/os-shell
            tag: 12-debian-12-r50
            digest: ""
        sidecar:
          image:
            registry: quay.io
            repository: team/sidecar
            tag: ""
        exporter:
          image:
            registry: quay.io
            repository: team/exporter
            tag: "2"
            digest: sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
      EOT
    }
  }

  assert {
    condition = output.images["grafana"] == tolist([
      "docker.io/bitnamilegacy/grafana:12.1.1-debian-12-r0",
      "docker.io/bitnamilegacy/os-shell:12-debian-12-r50",
      "quay.io/team/exporter@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
      "quay.io/team/tool:v1",
    ])
    error_message = "the images of grafana are not each image of bitnamilegacy: ${jsonencode(output.images["grafana"])}"
  }
  assert {
    condition     = yamldecode(local.pack_values["grafana"]).global == { imageRegistry = "", security = { allowInsecureImages = true } }
    error_message = "grafana does not take an image that its list of images does not name"
  }
  assert {
    condition     = yamldecode(local.pack_values["keycloak"]).global.security.allowInsecureImages && yamldecode(local.pack_values["keycloak"]).externalDatabase.host == "keycloak-db-rw"
    error_message = "keycloak loses its patch or the value of the images"
  }
  assert {
    condition     = contains(output.images["cert-manager"], "docker.io/bitnami/grafana:12.1.1-debian-12-r0") && !yamldecode(local.pack_values["cert-manager"]).global.security.allowInsecureImages
    error_message = "a pack of Public Repo changes the images of its values"
  }
}

# Each pin has the values of a pack of kyverno. The chart gives the registry of its images once, at
# global.image.registry, and most of its image maps have a registry with no value. The list of
# images of the pack names one of them with its host.
run "a_map_with_no_registry_takes_the_registry_of_its_chart" {
  command = apply

  override_data {
    target = data.spectrocloud_pack.pin
    values = {
      values = <<-EOT
        pack:
          namespace: "kyverno"
          content:
            images:
              - image: us-docker.pkg.dev/palette-images/packs/kyverno/1.19.2/kyverno:v1.19.2
        charts:
          kyverno:
            global:
              image:
                registry: us-docker.pkg.dev
            admissionController:
              container:
                image:
                  registry: ~
                  repository: palette-images/packs/kyverno/1.19.2/kyverno
                  tag: v1.19.2
            cleanupController:
              image:
                repository: palette-images/packs/kyverno/1.19.2/kyverno-cleanup-controller
                tag: v1.19.2
            webhooksCleanup:
              image:
                registry: quay.io
                repository: team/kubectl
                tag: v1
      EOT
    }
  }

  assert {
    condition = output.images["kyverno"] == tolist([
      "quay.io/team/kubectl:v1",
      "us-docker.pkg.dev/palette-images/packs/kyverno/1.19.2/kyverno-cleanup-controller:v1.19.2",
      "us-docker.pkg.dev/palette-images/packs/kyverno/1.19.2/kyverno:v1.19.2",
    ])
    error_message = "an image of kyverno does not take the registry of its chart: ${jsonencode(output.images["kyverno"])}"
  }
}

# The run gives a second project, so the state holds pebbles. pebbles runs the packs of the release
# profile of the lab, and the profile of its team. It also runs a copy of two profiles of wilma, and
# the profile of the tenant. Both clusters have the tag of the set.
run "pebbles_runs_a_copy_of_two_profiles_of_wilma_and_the_profile_of_the_tenant" {
  command = apply

  variables {
    second_project = "second"
  }

  override_data {
    target = data.spectrocloud_pack.pin
    values = { values = "pack:\n  namespace: \"monitoring\"\n" }
  }
  override_data {
    target = data.spectrocloud_cluster_profile.release
    values = { pack = [{ name = "ubuntu-aws", type = "spectro", tag = "22.04", uid = "os-1", values = "os: 1\n", registry_uid = "", manifest = [] }] }
  }

  assert {
    condition     = length(spectrocloud_cluster_aws.workload) == 1 && spectrocloud_cluster_aws.workload[0].name == "pebbles"
    error_message = "the state holds no pebbles"
  }
  assert {
    condition = alltrue([for t in ["data", "control"] : spectrocloud_cluster_profile.copy[t].name == spectrocloud_cluster_profile.team[t].name
      && spectrocloud_cluster_profile.copy[t].version == spectrocloud_cluster_profile.team[t].version
    && jsonencode(spectrocloud_cluster_profile.copy[t].pack) == jsonencode(spectrocloud_cluster_profile.team[t].pack)])
    error_message = "a copy differs from its profile on wilma"
  }
  assert {
    condition     = spectrocloud_cluster_profile.shared[0].context == "tenant" && spectrocloud_cluster_profile.shared[0].name == "wilma-shared"
    error_message = "the profile wilma-shared is not of the tenant"
  }
  assert {
    condition = alltrue([for c in [spectrocloud_cluster_aws.wilma, spectrocloud_cluster_aws.workload[0]] :
    contains([for p in c.cluster_profile : p.id], spectrocloud_cluster_profile.shared[0].id) && contains(c.tags, "hub:wilma")])
    error_message = "a cluster does not run the profile of the tenant, or has no tag hub:wilma"
  }
  assert {
    condition = toset([for p in spectrocloud_cluster_aws.workload[0].cluster_profile : p.id]) == toset(concat(
      [spectrocloud_cluster_profile.workload_release[0].id, spectrocloud_cluster_profile.workload_team[0].id, spectrocloud_cluster_profile.shared[0].id],
      [for t in ["data", "control"] : spectrocloud_cluster_profile.copy[t].id],
    ))
    error_message = "pebbles does not run its five profiles"
  }
  assert {
    condition     = spectrocloud_cluster_profile.workload_release[0].pack[0].uid == "os-1" && spectrocloud_cluster_profile.workload_release[0].pack[0].values == "os: 1\n"
    error_message = "the cluster profile of pebbles does not hold the packs of the release profile of the lab"
  }
  assert {
    condition     = alltrue([for p in concat(local.workload_packs, flatten([for t in local.copied_teams : local.teams[t]])) : contains(keys(output.images), p)])
    error_message = "a pack of pebbles is not a pin, so platform.sh images does not resolve its images"
  }
  assert {
    condition     = [for d in split("---\n", local.glue.workload) : yamldecode(d).spec.provider.vault.server if yamldecode(d).kind == "ClusterSecretStore"] == ["https://vault.{{.spectro.var.domain}}"]
    error_message = "the store of secrets of pebbles does not read Vault on wilma"
  }
  assert {
    condition     = [for d in split("---\n", local.glue.security) : yamldecode(d).spec.hostnames if yamldecode(d).kind == "HTTPRoute"] == [["vault.{{.spectro.var.domain}}"]]
    error_message = "wilma has no route to Vault"
  }
  assert {
    condition     = spectrocloud_cluster_profile.workload_team[0].profile_variables[0].variable[0].default_value == "wilma.strata-live.internal"
    error_message = "the profile of the team of pebbles does not give Grafana the domain of wilma"
  }
  assert {
    condition     = output.set_selector == "tag:hub=wilma" && output.workload_profile_names == tolist(["pebbles-release", "pebbles-apps", "wilma-data", "wilma-control"])
    error_message = "the outputs of pebbles: ${output.set_selector} ${jsonencode(output.workload_profile_names)}"
  }
}

# With no second project, the state holds wilma alone, with its tag of the set.
run "with_no_second_project_the_state_holds_wilma_alone" {
  command = apply

  override_data {
    target = data.spectrocloud_pack.pin
    values = { values = "pack:\n  namespace: \"monitoring\"\n" }
  }

  assert {
    condition     = length(spectrocloud_cluster_aws.workload) == 0 && length(spectrocloud_cluster_profile.shared) == 0 && length(spectrocloud_cluster_profile.copy) == 0
    error_message = "the state holds a part of pebbles"
  }
  assert {
    condition     = !contains(keys(local.on_cluster), "shared") && contains(spectrocloud_cluster_aws.wilma.tags, "hub:wilma")
    error_message = "wilma runs a profile of the tenant, or has no tag hub:wilma"
  }
  assert {
    condition     = output.workload_uid == "" && output.second_project_uid == "" && length(output.workload_profile_names) == 0 && length(output.copies) == 0
    error_message = "an output names a part of pebbles"
  }
}

# Case 2 of platform-boundary.sh: wilma-security 1.1.0 holds the other vault, and each other pin and
# profile stays.
run "case_2_of_the_boundary_moves_vault_to_wilma_security_1_1_0" {
  command = apply

  variables {
    vault_version = "2.0.0"
  }

  override_data {
    target = data.spectrocloud_pack.pin
    values = { values = "pack:\n  namespace: \"monitoring\"\n" }
  }

  assert {
    condition     = spectrocloud_cluster_profile.team["security"].version == "1.1.0" && alltrue([for t, p in spectrocloud_cluster_profile.team : p.version == "1.0.0" if t != "security"])
    error_message = "the move of Vault changes the version of another profile, or not of wilma-security"
  }
  assert {
    condition     = [for p in spectrocloud_cluster_profile.team["security"].pack : p.tag if p.name == "vault"] == ["2.0.0"] && output.packs["cert-manager"].version == "1.0.0"
    error_message = "wilma-security 1.1.0 does not hold vault 2.0.0, or another pin changes"
  }
  assert {
    condition     = contains([for p in spectrocloud_cluster_aws.wilma.cluster_profile : p.id], spectrocloud_cluster_profile.team["security"].id)
    error_message = "wilma does not run wilma-security 1.1.0"
  }
}

# The run makes each profile of each case. Each layer of glue that names the variable domain is in a
# profile that declares the variable. In a profile with no variable domain, the glue keeps the text
# {{.spectro.var.domain}}. So wilma refused the route of Vault of wilma-security, and wilma stayed
# Provisioning (#701).
run "each_glue_that_names_the_variable_domain_is_in_a_profile_that_declares_it" {
  command = apply

  variables {
    second_project       = "second"
    network_cert_manager = "2.0.0"
    move_fluentbit       = true
  }

  override_data {
    target = data.spectrocloud_pack.pin
    values = { values = "pack:\n  namespace: \"monitoring\"\n" }
  }

  assert {
    condition = alltrue([for c in [
      spectrocloud_cluster_profile.team, spectrocloud_cluster_profile.network_next, spectrocloud_cluster_profile.moved,
      spectrocloud_cluster_profile.copy, spectrocloud_cluster_profile.workload_team, spectrocloud_cluster_profile.shared,
      spectrocloud_cluster_profile.workload_release,
    ] : length(c) > 0])
    error_message = "the run does not make each profile of each case"
  }
  assert {
    condition = alltrue([for p in concat(
      values(spectrocloud_cluster_profile.team), spectrocloud_cluster_profile.network_next, values(spectrocloud_cluster_profile.moved),
      values(spectrocloud_cluster_profile.copy), spectrocloud_cluster_profile.workload_team, spectrocloud_cluster_profile.shared,
      spectrocloud_cluster_profile.workload_release,
      ) : contains(flatten([for v in p.profile_variables : v.variable[*].name]), "domain")
    if strcontains(join("", flatten([for k in p.pack : k.manifest[*].content])), local.domain)])
    error_message = "a layer of glue names ${local.domain} in a profile that declares no variable domain"
  }
}
