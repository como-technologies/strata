# The glue of each team: the objects that a real team writes beside its packs, in one manifest
# layer of its profile. The layer installs after each pack of priority 0, so the CRDs of the packs
# exist (main.tf). The glue crosses the profiles. The issuer of wilma-security signs the
# certificates of wilma-network and wilma-identity. wilma-identity holds a Cluster of
# cloudnative-pg from wilma-data. Each ServiceMonitor needs prometheus-operator of
# wilma-observability. The domain is the profile variable domain. The team of pebbles (#701) has a
# layer too: its store of secrets reads Vault on wilma, through the route of Vault on wilma.
#
# Each object is a map here, and yamlencode writes it. A selector or a Service name that matches
# nothing on wilma is no fault of the apply: strata compares the content, not the health.

locals {
  # The packs of each layer of glue: a team, the two profiles of the move of case 3, and the team of
  # pebbles.
  glue_teams = merge(local.teams, {
    observability_moved = [for p in local.teams.observability : p if p != "fluentbit"]
    logging             = ["fluentbit"]
    workload            = local.workload_packs
  })
  glue_namespaces = merge({ for t in keys(local.teams) : t => "${var.cluster_name}-${t}" }, {
    observability_moved = "${var.cluster_name}-observability"
    logging             = "${var.cluster_name}-logging"
    workload            = "${var.workload_name}-apps"
  })

  # The label and the port of the metrics of each pack. Vault in dev mode serves none.
  monitors = {
    cert-manager              = { app = "cert-manager", port = "tcp-prometheus-servicemonitor" }
    external-secrets-operator = { app = "external-secrets", port = "metrics" }
    kyverno                   = { app = "kyverno", port = "metrics-port" }
    envoy-gateway             = { app = "envoy-gateway", port = "http-metrics" }
    external-dns              = { app = "external-dns", port = "http" }
    cloudnative-pg            = { app = "cloudnative-pg", port = "http-metrics" }
    keycloak                  = { app = "keycloak", port = "http" }
    argo-cd                   = { app = "argocd-metrics", port = "http-metrics" }
    grafana                   = { app = "grafana", port = "http" }
    loki                      = { app = "loki", port = "http-metrics" }
    fluentbit                 = { app = "fluent-bit", port = "http" }
    crossplane                = { app = "crossplane", port = "metrics" }
  }

  # route HOST NAMESPACE SERVICE PORT: an HTTPRoute of the Gateway of wilma-network
  routes = {
    # The store of secrets of pebbles reads Vault through this route (#701).
    security      = { host = "vault", namespace = local.namespaces.vault, service = "vault", port = 8200 }
    identity      = { host = "keycloak", namespace = local.namespaces.keycloak, service = "keycloak", port = 80 }
    gitops        = { host = "argocd", namespace = local.namespaces.argo-cd, service = "argo-cd-server", port = 80 }
    observability = { host = "grafana", namespace = local.namespaces.grafana, service = "grafana", port = 3000 }
  }
  route = { for t, r in local.routes : t => {
    apiVersion = "gateway.networking.k8s.io/v1"
    kind       = "HTTPRoute"
    metadata   = { name = r.host, namespace = r.namespace }
    spec = {
      parentRefs = [{ name = var.cluster_name, namespace = local.glue_namespaces.network }]
      hostnames  = ["${r.host}.${local.domain}"]
      rules      = [{ backendRefs = [{ name = r.service, port = r.port }] }]
    }
  } }

  # The token of Vault, and the store of secrets that reads Vault with it, for each layer with a
  # store. wilma reads Vault in the cluster. pebbles reads it at the route of Vault on wilma, and the
  # name of that route is in the private zone of wilma.
  vault_servers = {
    security = "http://vault.${local.namespaces.vault}.svc:8200"
    workload = "https://vault.${local.domain}"
  }
  vault_store = { for t, server in local.vault_servers : t => [
    {
      apiVersion = "v1"
      kind       = "Secret"
      metadata   = { name = "vault-token", namespace = local.glue_namespaces[t] }
      stringData = { token = local.vault_token }
    },
    {
      apiVersion = "external-secrets.io/v1"
      kind       = "ClusterSecretStore"
      metadata   = { name = "${var.cluster_name}-vault" }
      spec = {
        provider = {
          vault = {
            server  = server
            path    = "secret"
            version = "v2"
            auth    = { tokenSecretRef = { name = "vault-token", namespace = local.glue_namespaces[t], key = "token" } }
          }
        }
      }
    },
  ] }

  # The objects of each layer that are not a namespace or a ServiceMonitor.
  glue_objects = {
    security = concat([
      { apiVersion = "cert-manager.io/v1", kind = "ClusterIssuer", metadata = { name = "${var.cluster_name}-selfsigned" }, spec = { selfSigned = {} } },
      {
        apiVersion = "cert-manager.io/v1"
        kind       = "Certificate"
        metadata   = { name = "${var.cluster_name}-ca", namespace = local.namespaces.cert-manager }
        spec = {
          isCA       = true
          commonName = "${var.cluster_name} lab CA"
          secretName = "${var.cluster_name}-ca"
          privateKey = { algorithm = "ECDSA", size = 256 }
          issuerRef  = { kind = "ClusterIssuer", name = "${var.cluster_name}-selfsigned", group = "cert-manager.io" }
        }
      },
      { apiVersion = "cert-manager.io/v1", kind = "ClusterIssuer", metadata = { name = "${var.cluster_name}-ca" }, spec = { ca = { secretName = "${var.cluster_name}-ca" } } },
      ], local.vault_store.security, [
      {
        apiVersion = "kyverno.io/v1"
        kind       = "ClusterPolicy"
        metadata   = { name = "${var.cluster_name}-image-tag" }
        spec = {
          validationFailureAction = "Audit"
          background              = true
          rules = [{
            name = "a-pinned-tag"
            match = { any = [{ resources = {
              kinds             = ["Pod"]
              namespaceSelector = { matchExpressions = [{ key = "kubernetes.io/metadata.name", operator = "NotIn", values = ["kube-system"] }] }
            } }] }
            validate = {
              message = "Each image of a pod of the platform names a tag other than latest."
              pattern = { spec = { containers = [{ image = "!*:latest" }] } }
            }
          }]
        }
      },
      local.route.security,
    ])
    network = [
      {
        apiVersion = "gateway.envoyproxy.io/v1alpha1"
        kind       = "EnvoyProxy"
        metadata   = { name = var.cluster_name, namespace = local.glue_namespaces.network }
        # A Service of type ClusterIP makes no load balancer in AWS, so a destroy leaves none.
        spec = { provider = { type = "Kubernetes", kubernetes = { envoyService = { type = "ClusterIP" } } } }
      },
      {
        apiVersion = "gateway.networking.k8s.io/v1"
        kind       = "GatewayClass"
        metadata   = { name = var.cluster_name }
        spec = {
          controllerName = "gateway.envoyproxy.io/gatewayclass-controller"
          parametersRef  = { group = "gateway.envoyproxy.io", kind = "EnvoyProxy", name = var.cluster_name, namespace = local.glue_namespaces.network }
        }
      },
      {
        apiVersion = "cert-manager.io/v1"
        kind       = "Certificate"
        metadata   = { name = "${var.cluster_name}-wildcard", namespace = local.glue_namespaces.network }
        spec = {
          secretName = "${var.cluster_name}-wildcard"
          dnsNames   = ["*.${local.domain}"]
          issuerRef  = { kind = "ClusterIssuer", name = "${var.cluster_name}-ca", group = "cert-manager.io" }
        }
      },
      {
        apiVersion = "gateway.networking.k8s.io/v1"
        kind       = "Gateway"
        metadata   = { name = var.cluster_name, namespace = local.glue_namespaces.network }
        spec = {
          gatewayClassName = var.cluster_name
          listeners = [{
            name          = "https"
            protocol      = "HTTPS"
            port          = 443
            hostname      = "*.${local.domain}"
            tls           = { mode = "Terminate", certificateRefs = [{ kind = "Secret", name = "${var.cluster_name}-wildcard" }] }
            allowedRoutes = { namespaces = { from = "All" } }
          }]
        }
      },
    ]
    data = []
    identity = [
      { apiVersion = "v1", kind = "Namespace", metadata = { name = local.namespaces.keycloak } },
      {
        apiVersion = "cert-manager.io/v1"
        kind       = "Certificate"
        metadata   = { name = "keycloak-tls", namespace = local.namespaces.keycloak }
        spec = {
          secretName = "keycloak-tls"
          dnsNames   = ["keycloak.${local.domain}"]
          issuerRef  = { kind = "ClusterIssuer", name = "${var.cluster_name}-ca", group = "cert-manager.io" }
        }
      },
      # The database of Keycloak. Its CRD comes from cloudnative-pg in wilma-data: case 6 removes
      # that profile from wilma.
      {
        apiVersion = "postgresql.cnpg.io/v1"
        kind       = "Cluster"
        metadata   = { name = "keycloak-db", namespace = local.namespaces.keycloak }
        spec = {
          instances = 1
          storage   = { size = "1Gi" }
          bootstrap = { initdb = { database = "keycloak", owner = "keycloak" } }
        }
      },
      local.route.identity,
    ]
    gitops              = [local.route.gitops]
    observability       = [local.route.observability]
    observability_moved = [local.route.observability]
    logging             = []
    workload            = local.vault_store.workload
    control = [
      # A platform API that a team offers: a claim of a database, with no composition yet.
      {
        apiVersion = "apiextensions.crossplane.io/v1"
        kind       = "CompositeResourceDefinition"
        metadata   = { name = "xdatabases.platform.${var.cluster_name}.example" }
        spec = {
          group      = "platform.${var.cluster_name}.example"
          names      = { kind = "XDatabase", plural = "xdatabases" }
          claimNames = { kind = "Database", plural = "databases" }
          versions = [{
            name          = "v1alpha1"
            served        = true
            referenceable = true
            schema = { openAPIV3Schema = {
              type       = "object"
              properties = { spec = { type = "object", properties = { size = { type = "string" } } } }
            } }
          }]
        }
      },
    ]
  }

  glue = { for t, packs in local.glue_teams : t => join("---\n", [for o in concat(
    [{ apiVersion = "v1", kind = "Namespace", metadata = { name = local.glue_namespaces[t] } }],
    [for p in packs : {
      apiVersion = "monitoring.coreos.com/v1"
      kind       = "ServiceMonitor"
      metadata   = { name = p, namespace = local.glue_namespaces[t] }
      spec = {
        namespaceSelector = { any = true }
        selector          = { matchLabels = { "app.kubernetes.io/name" = local.monitors[p].app } }
        endpoints         = [{ port = local.monitors[p].port }]
      }
    } if contains(keys(local.monitors), p)],
    local.glue_objects[t],
  ) : yamlencode(o)]) }
}
