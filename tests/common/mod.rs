//! Raw-API-shaped fixtures for the end-to-end tests: three profile states written as the
//! profile.json, manifests.json and variables.json files that `snapshot --from-raw` reads.
//!
//!   a   baseline
//!   a2  same content as a, but the kubernetes values YAML is reordered and re-indented
//!   a3  a re-pinned from another registry: same packs, values, and manifests, but registryUid,
//!       packUid, and every digest differ, so each pack shows IDENTITY UPSTREAM and nothing else
//!   a4  a3 with one kubernetes value edited (podSubnet): a registry move plus an AUTHORED change
//!   b   the next version: k8s bump + values edits, cilium same tag but new digest, prometheus
//!       manifest edited + one added, cert-manager added, external-dns removed, metrics-server
//!       values unparseable on both sides but different, variables changed, two consumers.
//!   addons  a second, add-on profile (lab-addons@2.0.0) with one pack and one variable
//!
//! Clusters, written as the cluster.json, manifests.json and variables.json that a cluster
//! snapshot reads, each assembled from the profile fixtures it runs:
//!
//!   cluster-a          lab-cluster (CLUSTER_UID) running a
//!   cluster-a-staging  cluster-a, with the value "staging" for cluster_env
//!   cluster-b          lab-cluster (CLUSTER_UID) running b
//!   cluster-merged     lab-cluster (CLUSTER_UID) running b and addons
//!
//! A cluster gives each variable its default value, or the value in its `values`; a sensitive
//! variable always gets SECRET_VALUE, which no document may contain.

use std::fs;
use std::path::Path;

use serde_json::{json, Value};

// comment-only values, as the real ubuntu-aws pack ships: canonical form must be null
const OS_VALUES: &str = "\n# Spectro Golden images include most of the hardening standards recommended by CIS benchmarking\n\n# Uncomment below section to\n# 1. Include custom files to be copied over to the nodes and/or\n# 2. Execute list of commands before or after kubeadm init/join is executed\n#\n# kubeadmconfig:\n#   preKubeadmCommands:\n#   - echo \"Executing pre kube admin config commands\"\n#   - update-ca-certificates";

const K8S_VALUES_A: &str = "pack:\n  k8sHardening: true\n  palette:\n    config:\n      oidc:\n        identityProvider: none\nkubeadmconfig:\n  apiServer:\n    extraArgs:\n      audit-log-maxage: \"30\"\n      secure-port: \"6443\"\n  clusterConfiguration:\n    networking:\n      podSubnet: 192.168.0.0/16";

// same as A, keys reordered and re-indented
const K8S_VALUES_A2: &str = "kubeadmconfig:\n    clusterConfiguration:\n        networking:\n            podSubnet: 192.168.0.0/16\n    apiServer:\n        extraArgs:\n            secure-port: \"6443\"\n            audit-log-maxage: \"30\"\npack:\n    palette:\n        config:\n            oidc:\n                identityProvider: none\n    k8sHardening: true";

const K8S_VALUES_B: &str = "pack:\n  k8sHardening: false\n  palette:\n    config:\n      oidc:\n        identityProvider: noauth\nkubeadmconfig:\n  apiServer:\n    extraArgs:\n      audit-log-maxage: \"90\"\n      secure-port: \"6443\"\n      profiling: \"false\"\n  clusterConfiguration:\n    networking:\n      podSubnet: 192.168.0.0/16";

const CNI_VALUES: &str = "pack:\n  content:\n    images:\n      - image: quay.io/cilium/cilium:v1.15.3\ncharts:\n  cilium:\n    hubble:\n      enabled: true";

const CSI_VALUES: &str = "charts:\n  aws-ebs-csi-driver:\n    storageClasses:\n      - name: spectro-storage-class\n        annotations:\n          storageclass.kubernetes.io/is-default-class: \"true\"";

const PROM_VALUES: &str = "charts:\n  kube-prometheus-stack:\n    grafana:\n      adminPassword: \"{{ .spectro.var.grafana_password }}\"";

const RULES_A: &str = "apiVersion: monitoring.coreos.com/v1\nkind: PrometheusRule\nmetadata:\n  name: extra-rules\nspec:\n  groups:\n    - name: node\n      rules:\n        - alert: NodeDown\n          expr: up == 0\n          for: 5m";

const RULES_B: &str = "apiVersion: monitoring.coreos.com/v1\nkind: PrometheusRule\nmetadata:\n  name: extra-rules\nspec:\n  groups:\n    - name: node\n      rules:\n        - alert: NodeDown\n          expr: up == 0\n          for: 10m";

const DASH_B: &str =
    "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: dashboards\ndata:\n  cluster.json: \"{}\"";

const EXTDNS_VALUES: &str = "charts:\n  external-dns:\n    provider: aws";

const CERTMGR_VALUES: &str = "charts:\n  cert-manager:\n    installCRDs: true";

// unquoted go-template makes this invalid YAML on purpose
const METRICS_A: &str = "charts:\n  metrics-server:\n    replicas: {{ .spectro.var.replicas }}\n    args:\n      - --kubelet-insecure-tls";

const METRICS_B: &str = "charts:\n  metrics-server:\n    replicas: {{ .spectro.var.replicas }}\n    args:\n      - --kubelet-insecure-tls\n      - --metric-resolution=15s";

/// A pack ref on the profile template (V1PackRef).
fn pack_ref(
    name: &str,
    layer: &str,
    tag: &str,
    version: &str,
    uid: &str,
    digest: &str,
    values: &str,
) -> Value {
    let mut v = json!({
        "name": name, "layer": layer, "type": "spectro", "tag": tag, "version": version,
        "packUid": uid, "registryUid": "reg-public", "values": values,
        "manifests": [], "params": {}, "presets": [],
    });
    if !digest.is_empty() {
        v["digest"] = json!(digest);
    }
    v
}

/// A pack entry from /packs/manifests (V1ClusterProfilePackManifests).
fn pack_manifests(
    name: &str,
    layer: &str,
    version: &str,
    uid: &str,
    digest: &str,
    values: &str,
    manifests: Vec<Value>,
) -> Value {
    json!({
        "metadata": {"name": name, "uid": uid},
        "spec": {"name": name, "layer": layer, "version": version, "digest": digest,
                 "registryUid": "reg-public", "values": values, "manifests": manifests},
        "status": {},
    })
}

fn manifest(name: &str, uid: &str, digest: &str, content: &str) -> Value {
    json!({"name": name, "uid": uid, "digest": digest, "content": content})
}

fn variable(name: &str, display: &str, default: &str, sensitive: bool) -> Value {
    json!({
        "name": name, "displayName": display, "defaultValue": default, "format": "string",
        "inputType": "text", "required": true, "hidden": false, "immutable": false,
        "isSensitive": sensitive, "options": [],
    })
}

fn write_profile(
    dir: &Path,
    uid: &str,
    version: &str,
    packs: Vec<Value>,
    manifests: Vec<Value>,
    variables: Value,
    in_use: Vec<&str>,
) {
    fs::create_dir_all(dir).unwrap();
    let profile = json!({
        "apiVersion": "v1", "kind": "ClusterProfile",
        "metadata": {"name": "lab-profile", "uid": uid, "labels": {"purpose": "test"}},
        "spec": {
            "version": version,
            "published": {"cloudType": "aws", "type": "cluster", "packs": packs},
            "versions": [
                {"uid": "64f0000000000000000000a1", "version": "1.2.0"},
                {"uid": "64f0000000000000000000b2", "version": "1.3.0"},
            ],
        },
        "status": {"isPublished": true, "inUseClusterUids": in_use},
    });
    let manifests =
        json!({"metadata": {"name": "lab-profile", "uid": uid}, "spec": {"packs": manifests}});
    fs::write(
        dir.join("profile.json"),
        serde_json::to_string_pretty(&profile).unwrap(),
    )
    .unwrap();
    fs::write(
        dir.join("manifests.json"),
        serde_json::to_string_pretty(&manifests).unwrap(),
    )
    .unwrap();
    fs::write(
        dir.join("variables.json"),
        serde_json::to_string_pretty(&variables).unwrap(),
    )
    .unwrap();
}

/// The uid every cluster fixture carries: the fixtures are one cluster at different moments.
pub const CLUSTER_UID: &str = "c1a5000000000000000000c1";

/// A cluster variable value that must never reach a snapshot.
pub const SECRET_VALUE: &str = "s3cr3t-value";

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

/// Assemble a cluster that runs the profile fixtures `profiles`, in order, the way Palette returns
/// it: applied templates on the cluster, per-profile pack manifests, and per-profile variables
/// with values: `values` by name, else the default, and SECRET_VALUE for a sensitive variable.
fn write_cluster(root: &Path, dir: &str, profiles: &[&str], values: &[(&str, &str)]) {
    let out = root.join(dir);
    fs::create_dir_all(&out).unwrap();
    let (mut templates, mut manifests, mut variables) = (vec![], vec![], vec![]);
    for name in profiles {
        let prof = read_json(&root.join(name).join("profile.json"));
        let man = read_json(&root.join(name).join("manifests.json"));
        let vars = read_json(&root.join(name).join("variables.json"));
        let published = &prof["spec"]["published"];
        templates.push(json!({
            "uid": prof["metadata"]["uid"], "name": prof["metadata"]["name"],
            "profileVersion": prof["spec"]["version"], "type": published["type"],
            "cloudType": published["cloudType"], "packs": published["packs"],
        }));
        manifests.push(json!({"metadata": man["metadata"], "spec": man["spec"]}));
        let with_values: Vec<Value> = vars["variables"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                let mut v = v.clone();
                let name = v["name"].as_str().unwrap_or("");
                v["value"] = if v["isSensitive"] == json!(true) {
                    json!(SECRET_VALUE)
                } else {
                    match values.iter().find(|(n, _)| *n == name) {
                        Some((_, value)) => json!(value),
                        None => v["defaultValue"].clone(),
                    }
                };
                v
            })
            .collect();
        variables.push(json!({"profileUid": prof["metadata"]["uid"], "variables": with_values}));
    }
    let cluster = json!({
        "apiVersion": "v1", "kind": "SpectroCluster",
        "metadata": {"name": "lab-cluster", "uid": CLUSTER_UID},
        "spec": {"cloudType": "aws", "clusterProfileTemplates": templates},
        "status": {"state": "Running"},
    });
    for (file, v) in [
        ("cluster.json", cluster),
        ("manifests.json", json!({"profiles": manifests})),
        ("variables.json", Value::Array(variables)),
    ] {
        fs::write(out.join(file), serde_json::to_string_pretty(&v).unwrap()).unwrap();
    }
}

/// Copy the three raw files from `from` to `to`, applying each text replacement to every file.
fn rewrite(from: &Path, to: &Path, replacements: &[(&str, &str)]) {
    fs::create_dir_all(to).unwrap();
    for name in ["profile.json", "manifests.json", "variables.json"] {
        let mut text = fs::read_to_string(from.join(name)).unwrap();
        for (old, new) in replacements {
            text = text.replace(old, new);
        }
        fs::write(to.join(name), text).unwrap();
    }
}

/// Writes the profile fixtures a/, a2/, a3/, a4/, b/ and addons/, and the cluster fixtures
/// cluster-a/, cluster-b/ and cluster-merged/, under `root`.
pub fn make_fixtures(root: &Path) {
    let packs_a = |k8s_values: &str| {
        vec![
            pack_ref(
                "ubuntu-aws",
                "os",
                "22.04",
                "22.04",
                "pk-os-2204",
                "",
                OS_VALUES,
            ),
            pack_ref(
                "kubernetes",
                "k8s",
                "1.29.4",
                "1.29.4",
                "pk-k8s-1294",
                "sha256:k8s1294",
                k8s_values,
            ),
            pack_ref(
                "cni-cilium-oss",
                "cni",
                "1.15.3",
                "1.15.3",
                "pk-cni-1153",
                "sha256:cni-aaaa",
                CNI_VALUES,
            ),
            pack_ref(
                "csi-aws-ebs",
                "csi",
                "1.28.0",
                "1.28.0",
                "pk-csi-1280",
                "sha256:csi-1280",
                CSI_VALUES,
            ),
            pack_ref(
                "prometheus-operator",
                "addon",
                "55.8.3",
                "55.8.3",
                "pk-prom-5583",
                "sha256:prom-5583",
                PROM_VALUES,
            ),
            pack_ref(
                "external-dns",
                "addon",
                "0.14.0",
                "0.14.0",
                "pk-edns-0140",
                "sha256:edns-0140",
                EXTDNS_VALUES,
            ),
            pack_ref(
                "metrics-server",
                "addon",
                "0.7.0",
                "0.7.0",
                "pk-ms-070",
                "sha256:ms-070",
                METRICS_A,
            ),
        ]
    };
    let manifests_a = |k8s_values: &str| {
        vec![
            pack_manifests(
                "ubuntu-aws",
                "os",
                "22.04",
                "pk-os-2204",
                "sha256:os-2204-registry",
                OS_VALUES,
                vec![],
            ),
            pack_manifests(
                "kubernetes",
                "k8s",
                "1.29.4",
                "pk-k8s-1294",
                "sha256:k8s1294",
                k8s_values,
                vec![],
            ),
            pack_manifests(
                "cni-cilium-oss",
                "cni",
                "1.15.3",
                "pk-cni-1153",
                "sha256:cni-aaaa",
                CNI_VALUES,
                vec![],
            ),
            pack_manifests(
                "csi-aws-ebs",
                "csi",
                "1.28.0",
                "pk-csi-1280",
                "sha256:csi-1280",
                CSI_VALUES,
                vec![],
            ),
            pack_manifests(
                "prometheus-operator",
                "addon",
                "55.8.3",
                "pk-prom-5583",
                "sha256:prom-5583",
                PROM_VALUES,
                vec![manifest(
                    "extra-rules",
                    "m-rules-1",
                    "sha256:rules-a",
                    RULES_A,
                )],
            ),
            pack_manifests(
                "external-dns",
                "addon",
                "0.14.0",
                "pk-edns-0140",
                "sha256:edns-0140",
                EXTDNS_VALUES,
                vec![],
            ),
            pack_manifests(
                "metrics-server",
                "addon",
                "0.7.0",
                "pk-ms-070",
                "sha256:ms-070",
                METRICS_A,
                vec![],
            ),
        ]
    };
    let vars_a = json!({"variables": [
        variable("cluster_env", "Environment", "dev", false),
        variable("grafana_password", "", "", true),
    ]});
    write_profile(
        &root.join("a"),
        "64f0000000000000000000a1",
        "1.2.0",
        packs_a(K8S_VALUES_A),
        manifests_a(K8S_VALUES_A),
        vars_a.clone(),
        vec![],
    );
    write_profile(
        &root.join("a2"),
        "64f0000000000000000000a1",
        "1.2.0",
        packs_a(K8S_VALUES_A2),
        manifests_a(K8S_VALUES_A2),
        vars_a,
        vec![],
    );
    // a3 and a4 are text transforms of a: the API shapes stay, only the identifiers move.
    // Manifest digests are not hashed, so rewriting every digest changes UPSTREAM alone.
    rewrite(
        &root.join("a"),
        &root.join("a3"),
        &[
            ("\"reg-public\"", "\"reg-prod\""),
            ("\"pk-", "\"pkp-"),
            ("sha256:", "sha256:prod-"),
        ],
    );
    rewrite(
        &root.join("a3"),
        &root.join("a4"),
        &[("podSubnet: 192.168.0.0/16", "podSubnet: 10.0.0.0/16")],
    );

    let packs_b = vec![
        pack_ref(
            "ubuntu-aws",
            "os",
            "22.04",
            "22.04",
            "pk-os-2204",
            "",
            OS_VALUES,
        ),
        pack_ref(
            "kubernetes",
            "k8s",
            "1.30.2",
            "1.30.2",
            "pk-k8s-1302",
            "sha256:k8s1302",
            K8S_VALUES_B,
        ),
        pack_ref(
            "cni-cilium-oss",
            "cni",
            "1.15.3",
            "1.15.3",
            "pk-cni-1153",
            "sha256:cni-bbbb",
            CNI_VALUES,
        ),
        pack_ref(
            "csi-aws-ebs",
            "csi",
            "1.28.0",
            "1.28.0",
            "pk-csi-1280",
            "sha256:csi-1280",
            CSI_VALUES,
        ),
        pack_ref(
            "prometheus-operator",
            "addon",
            "55.8.3",
            "55.8.3",
            "pk-prom-5583",
            "sha256:prom-5583",
            PROM_VALUES,
        ),
        pack_ref(
            "cert-manager",
            "addon",
            "1.14.0",
            "1.14.0",
            "pk-cm-1140",
            "sha256:cm-1140",
            CERTMGR_VALUES,
        ),
        pack_ref(
            "metrics-server",
            "addon",
            "0.7.0",
            "0.7.0",
            "pk-ms-070",
            "sha256:ms-070",
            METRICS_B,
        ),
    ];
    let manifests_b = vec![
        pack_manifests(
            "ubuntu-aws",
            "os",
            "22.04",
            "pk-os-2204",
            "sha256:os-2204-registry",
            OS_VALUES,
            vec![],
        ),
        pack_manifests(
            "kubernetes",
            "k8s",
            "1.30.2",
            "pk-k8s-1302",
            "sha256:k8s1302",
            K8S_VALUES_B,
            vec![],
        ),
        pack_manifests(
            "cni-cilium-oss",
            "cni",
            "1.15.3",
            "pk-cni-1153",
            "sha256:cni-bbbb",
            CNI_VALUES,
            vec![],
        ),
        pack_manifests(
            "csi-aws-ebs",
            "csi",
            "1.28.0",
            "pk-csi-1280",
            "sha256:csi-1280",
            CSI_VALUES,
            vec![],
        ),
        pack_manifests(
            "prometheus-operator",
            "addon",
            "55.8.3",
            "pk-prom-5583",
            "sha256:prom-5583",
            PROM_VALUES,
            vec![
                manifest("extra-rules", "m-rules-2", "sha256:rules-b", RULES_B),
                manifest("dashboards", "m-dash-1", "sha256:dash-b", DASH_B),
            ],
        ),
        pack_manifests(
            "cert-manager",
            "addon",
            "1.14.0",
            "pk-cm-1140",
            "sha256:cm-1140",
            CERTMGR_VALUES,
            vec![],
        ),
        pack_manifests(
            "metrics-server",
            "addon",
            "0.7.0",
            "pk-ms-070",
            "sha256:ms-070",
            METRICS_B,
            vec![],
        ),
    ];
    let vars_b = json!({"variables": [
        variable("cluster_env", "Environment", "staging", false),
        variable("grafana_password", "", "", true),
        variable("db_password", "", "", true),
    ]});
    write_profile(
        &root.join("b"),
        "64f0000000000000000000b2",
        "1.3.0",
        packs_b,
        manifests_b,
        vars_b,
        vec!["cluster-aaa", "cluster-bbb"],
    );

    write_profile(
        &root.join("addons"),
        "64f0000000000000000000d4",
        "2.0.0",
        vec![pack_ref(
            "nginx-ingress",
            "addon",
            "4.10.0",
            "4.10.0",
            "pk-ngx-4100",
            "sha256:ngx-4100",
            "charts:\n  ingress-nginx:\n    controller:\n      replicaCount: 2",
        )],
        vec![pack_manifests(
            "nginx-ingress",
            "addon",
            "4.10.0",
            "pk-ngx-4100",
            "sha256:ngx-4100",
            "charts:\n  ingress-nginx:\n    controller:\n      replicaCount: 2",
            vec![],
        )],
        json!({"variables": [variable("ingress_class", "Ingress class", "nginx", false)]}),
        vec![],
    );
    rename_profile(&root.join("addons"), "lab-addons");

    write_cluster(root, "cluster-a", &["a"], &[]);
    write_cluster(
        root,
        "cluster-a-staging",
        &["a"],
        &[("cluster_env", "staging")],
    );
    write_cluster(root, "cluster-b", &["b"], &[]);
    write_cluster(root, "cluster-merged", &["b", "addons"], &[]);
}

/// Give the profile in `dir` another name; `write_profile` names every profile lab-profile.
fn rename_profile(dir: &Path, name: &str) {
    for file in ["profile.json", "manifests.json"] {
        let path = dir.join(file);
        let mut v = read_json(&path);
        v["metadata"]["name"] = json!(name);
        fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    }
}
