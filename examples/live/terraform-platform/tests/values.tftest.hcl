# The values that Palette requires of the packs of wilma (#1004). The providers of Palette and AWS
# are mocks, so the test sends no request. `just live-platform-values` runs it.
#
# Palette refuses a profile with ClusterProfileCreationValidationFailure in two cases. A chart of a
# Helm registry gives no value to `pack.namespace`, and the registry starts the values of each such
# chart with `pack.namespace: ""`. Or prometheus-operator gives Grafana no password. Each run gives
# each pin the values of one form.

mock_provider "spectrocloud" {}
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
