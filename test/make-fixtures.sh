#!/usr/bin/env bash
# Generates raw-API-shaped fixtures for three profile states under $1:
#   a   baseline
#   a2  same content as a, but values YAML reordered/re-indented (canonicalization check)
#   b   the "next version": k8s bump + values edits, cilium same tag but new digest,
#       prometheus manifest edited + new manifest, cert-manager added, external-dns removed,
#       metrics-server values unparseable on both sides but different, variables changed.
# Shapes follow palette-sdk-go models: V1ClusterProfile, V1ClusterProfilePacksManifests, V1Variables.
set -euo pipefail
root=${1:?usage: make-fixtures.sh OUTDIR}
mkdir -p "$root/a" "$root/a2" "$root/b"

OS_VALUES=$(cat <<'YAML'
kubeadmconfig:
  preKubeadmCommands:
    - echo "Executing pre kube admin config commands"
    - update-ca-certificates
YAML
)
K8S_VALUES_A=$(cat <<'YAML'
pack:
  k8sHardening: true
  palette:
    config:
      oidc:
        identityProvider: none
kubeadmconfig:
  apiServer:
    extraArgs:
      audit-log-maxage: "30"
      secure-port: "6443"
  clusterConfiguration:
    networking:
      podSubnet: 192.168.0.0/16
YAML
)
# same as A, keys reordered and re-indented
K8S_VALUES_A2=$(cat <<'YAML'
kubeadmconfig:
    clusterConfiguration:
        networking:
            podSubnet: 192.168.0.0/16
    apiServer:
        extraArgs:
            secure-port: "6443"
            audit-log-maxage: "30"
pack:
    palette:
        config:
            oidc:
                identityProvider: none
    k8sHardening: true
YAML
)
K8S_VALUES_B=$(cat <<'YAML'
pack:
  k8sHardening: false
  palette:
    config:
      oidc:
        identityProvider: noauth
kubeadmconfig:
  apiServer:
    extraArgs:
      audit-log-maxage: "90"
      secure-port: "6443"
      profiling: "false"
  clusterConfiguration:
    networking:
      podSubnet: 192.168.0.0/16
YAML
)
CNI_VALUES=$(cat <<'YAML'
pack:
  content:
    images:
      - image: quay.io/cilium/cilium:v1.15.3
charts:
  cilium:
    hubble:
      enabled: true
YAML
)
CSI_VALUES=$(cat <<'YAML'
charts:
  aws-ebs-csi-driver:
    storageClasses:
      - name: spectro-storage-class
        annotations:
          storageclass.kubernetes.io/is-default-class: "true"
YAML
)
PROM_VALUES=$(cat <<'YAML'
charts:
  kube-prometheus-stack:
    grafana:
      adminPassword: "{{ .spectro.var.grafana_password }}"
YAML
)
RULES_A=$(cat <<'YAML'
apiVersion: monitoring.coreos.com/v1
kind: PrometheusRule
metadata:
  name: extra-rules
spec:
  groups:
    - name: node
      rules:
        - alert: NodeDown
          expr: up == 0
          for: 5m
YAML
)
RULES_B=$(cat <<'YAML'
apiVersion: monitoring.coreos.com/v1
kind: PrometheusRule
metadata:
  name: extra-rules
spec:
  groups:
    - name: node
      rules:
        - alert: NodeDown
          expr: up == 0
          for: 10m
YAML
)
DASH_B=$(cat <<'YAML'
apiVersion: v1
kind: ConfigMap
metadata:
  name: dashboards
data:
  cluster.json: "{}"
YAML
)
EXTDNS_VALUES=$(cat <<'YAML'
charts:
  external-dns:
    provider: aws
YAML
)
CERTMGR_VALUES=$(cat <<'YAML'
charts:
  cert-manager:
    installCRDs: true
YAML
)
# unquoted go-template makes this invalid YAML on purpose
METRICS_A=$(cat <<'YAML'
charts:
  metrics-server:
    replicas: {{ .spectro.var.replicas }}
    args:
      - --kubelet-insecure-tls
YAML
)
METRICS_B=$(cat <<'YAML'
charts:
  metrics-server:
    replicas: {{ .spectro.var.replicas }}
    args:
      - --kubelet-insecure-tls
      - --metric-resolution=15s
YAML
)

# pack ref (V1PackRef on the profile template)
ref() { # name layer tag version packUid digest values
  jq -n --arg n "$1" --arg l "$2" --arg t "$3" --arg v "$4" --arg u "$5" --arg d "$6" --arg vals "$7" \
    '{name:$n, layer:$l, type:"spectro", tag:$t, version:$v, packUid:$u, registryUid:"reg-public",
      digest:(if $d == "" then null else $d end), values:$vals, manifests:[], params:{}, presets:[]}
     | with_entries(select(.value != null))'
}
# pack manifests entry (V1ClusterProfilePackManifests from /packs/manifests)
mf() { # name layer version packUid registryDigest values manifestsJson
  jq -n --arg n "$1" --arg l "$2" --arg v "$3" --arg u "$4" --arg d "$5" --arg vals "$6" --argjson ms "$7" \
    '{metadata:{name:$n, uid:$u}, spec:{name:$n, layer:$l, version:$v, digest:$d, registryUid:"reg-public", values:$vals, manifests:$ms}, status:{}}'
}
manifest() { jq -n --arg n "$1" --arg u "$2" --arg d "$3" --arg c "$4" '{name:$n, uid:$u, digest:$d, content:$c}'; }

write_profile() { # dir uid version packsJson manifestsJson variablesJson [inUseClusterUidsJson]
  local dir=$1
  jq -n --arg uid "$2" --arg ver "$3" --argjson packs "$4" --argjson inuse "${7:-[]}" \
    '{apiVersion:"v1", kind:"ClusterProfile",
      metadata:{name:"lab-profile", uid:$uid, labels:{"purpose":"test"}},
      spec:{version:$ver, published:{cloudType:"aws", type:"cluster", packs:$packs},
            versions:[{uid:"64f0000000000000000000a1", version:"1.2.0"}, {uid:"64f0000000000000000000b2", version:"1.3.0"}]},
      status:{isPublished:true, inUseClusterUids:$inuse}}' > "$dir/profile.json"
  jq -n --arg uid "$2" --argjson packs "$5" \
    '{metadata:{name:"lab-profile", uid:$uid}, spec:{packs:$packs}}' > "$dir/manifests.json"
  printf '%s\n' "$6" > "$dir/variables.json"
}

# ------------------------------------------------------------------ A
PACKS_A=$(jq -s . \
  <(ref ubuntu-aws       os    22.04   22.04   pk-os-2204   ""              "$OS_VALUES") \
  <(ref kubernetes       k8s   1.29.4  1.29.4  pk-k8s-1294  sha256:k8s1294  "$K8S_VALUES_A") \
  <(ref cni-cilium-oss   cni   1.15.3  1.15.3  pk-cni-1153  sha256:cni-aaaa "$CNI_VALUES") \
  <(ref csi-aws-ebs      csi   1.28.0  1.28.0  pk-csi-1280  sha256:csi-1280 "$CSI_VALUES") \
  <(ref prometheus-operator addon 55.8.3 55.8.3 pk-prom-5583 sha256:prom-5583 "$PROM_VALUES") \
  <(ref external-dns     addon 0.14.0  0.14.0  pk-edns-0140 sha256:edns-0140 "$EXTDNS_VALUES") \
  <(ref metrics-server   addon 0.7.0   0.7.0   pk-ms-070    sha256:ms-070   "$METRICS_A"))
MANIFESTS_A=$(jq -s . \
  <(mf ubuntu-aws     os  22.04  pk-os-2204  sha256:os-2204-registry "$OS_VALUES" '[]') \
  <(mf kubernetes     k8s 1.29.4 pk-k8s-1294 sha256:k8s1294 "$K8S_VALUES_A" '[]') \
  <(mf cni-cilium-oss cni 1.15.3 pk-cni-1153 sha256:cni-aaaa "$CNI_VALUES" '[]') \
  <(mf csi-aws-ebs    csi 1.28.0 pk-csi-1280 sha256:csi-1280 "$CSI_VALUES" '[]') \
  <(mf prometheus-operator addon 55.8.3 pk-prom-5583 sha256:prom-5583 "$PROM_VALUES" "$(jq -s . <(manifest extra-rules m-rules-1 sha256:rules-a "$RULES_A"))") \
  <(mf external-dns   addon 0.14.0 pk-edns-0140 sha256:edns-0140 "$EXTDNS_VALUES" '[]') \
  <(mf metrics-server addon 0.7.0 pk-ms-070 sha256:ms-070 "$METRICS_A" '[]'))
VARS_A='{"variables":[{"name":"cluster_env","displayName":"Environment","defaultValue":"dev","format":"string","inputType":"text","required":true,"hidden":false,"immutable":false,"isSensitive":false,"options":[]},{"name":"grafana_password","defaultValue":"","format":"string","inputType":"text","required":true,"isSensitive":true,"hidden":false,"immutable":false,"options":[]}]}'
write_profile "$root/a" 64f0000000000000000000a1 1.2.0 "$PACKS_A" "$MANIFESTS_A" "$VARS_A"

# ------------------------------------------------------------------ A2: identical content, reordered YAML
PACKS_A2=$(jq --arg v "$K8S_VALUES_A2" 'map(if .name == "kubernetes" then .values = $v else . end)' <<<"$PACKS_A")
MANIFESTS_A2=$(jq --arg v "$K8S_VALUES_A2" 'map(if .spec.name == "kubernetes" then .spec.values = $v else . end)' <<<"$MANIFESTS_A")
write_profile "$root/a2" 64f0000000000000000000a1 1.2.0 "$PACKS_A2" "$MANIFESTS_A2" "$VARS_A"

# ------------------------------------------------------------------ B
PACKS_B=$(jq -s . \
  <(ref ubuntu-aws       os    22.04   22.04   pk-os-2204   ""              "$OS_VALUES") \
  <(ref kubernetes       k8s   1.30.2  1.30.2  pk-k8s-1302  sha256:k8s1302  "$K8S_VALUES_B") \
  <(ref cni-cilium-oss   cni   1.15.3  1.15.3  pk-cni-1153  sha256:cni-bbbb "$CNI_VALUES") \
  <(ref csi-aws-ebs      csi   1.28.0  1.28.0  pk-csi-1280  sha256:csi-1280 "$CSI_VALUES") \
  <(ref prometheus-operator addon 55.8.3 55.8.3 pk-prom-5583 sha256:prom-5583 "$PROM_VALUES") \
  <(ref cert-manager     addon 1.14.0  1.14.0  pk-cm-1140   sha256:cm-1140  "$CERTMGR_VALUES") \
  <(ref metrics-server   addon 0.7.0   0.7.0   pk-ms-070    sha256:ms-070   "$METRICS_B"))
MANIFESTS_B=$(jq -s . \
  <(mf ubuntu-aws     os  22.04  pk-os-2204  sha256:os-2204-registry "$OS_VALUES" '[]') \
  <(mf kubernetes     k8s 1.30.2 pk-k8s-1302 sha256:k8s1302 "$K8S_VALUES_B" '[]') \
  <(mf cni-cilium-oss cni 1.15.3 pk-cni-1153 sha256:cni-bbbb "$CNI_VALUES" '[]') \
  <(mf csi-aws-ebs    csi 1.28.0 pk-csi-1280 sha256:csi-1280 "$CSI_VALUES" '[]') \
  <(mf prometheus-operator addon 55.8.3 pk-prom-5583 sha256:prom-5583 "$PROM_VALUES" "$(jq -s . <(manifest extra-rules m-rules-2 sha256:rules-b "$RULES_B") <(manifest dashboards m-dash-1 sha256:dash-b "$DASH_B"))") \
  <(mf cert-manager   addon 1.14.0 pk-cm-1140 sha256:cm-1140 "$CERTMGR_VALUES" '[]') \
  <(mf metrics-server addon 0.7.0 pk-ms-070 sha256:ms-070 "$METRICS_B" '[]'))
VARS_B='{"variables":[{"name":"cluster_env","displayName":"Environment","defaultValue":"staging","format":"string","inputType":"text","required":true,"hidden":false,"immutable":false,"isSensitive":false,"options":[]},{"name":"grafana_password","defaultValue":"","format":"string","inputType":"text","required":true,"isSensitive":true,"hidden":false,"immutable":false,"options":[]},{"name":"db_password","defaultValue":"","format":"string","inputType":"text","required":true,"isSensitive":true,"hidden":false,"immutable":false,"options":[]}]}'
write_profile "$root/b" 64f0000000000000000000b2 1.3.0 "$PACKS_B" "$MANIFESTS_B" "$VARS_B" '["cluster-aaa","cluster-bbb"]'

echo "fixtures written under $root"
