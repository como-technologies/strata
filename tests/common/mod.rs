//! Raw-API-shaped fixtures for the end-to-end tests: three profile states written as the
//! profile.json, manifests.json and variables.json files that `snapshot --from-raw` reads.
//!
//!   a   baseline
//!   a2  same content as a, but the kubernetes values YAML is reordered and re-indented
//!   a-reversed  the packs of a in the other order: another root, because Palette keeps the order
//!   a3  a re-pinned from another registry: same packs, values, and manifests, but registryUid,
//!       packUid, and every digest differ, so each pack shows IDENTITY UPSTREAM and nothing else
//!   a4  a3 with one kubernetes value edited (podSubnet): a registry move plus an AUTHORED change
//!   a5  a3 with every pack of type oci: a move to an OCI registry, where the type follows the
//!       registry
//!   a6  a with every pack a manifest: type manifest and no registry
//!   a7  a with the OS pack pinned under another tag name for the same version: the same build
//!   b   the next version: k8s bump + values edits, cilium same tag but new digest, prometheus
//!       manifest edited + one added, cert-manager added, external-dns removed, metrics-server
//!       values unparseable on both sides but different, variables changed, two consumers.
//!   addons  a second, add-on profile (lab-addons@2.0.0) with one pack and one variable
//!
//! Clusters, written as the cluster.json, manifests.json and variables.json that a cluster
//! snapshot reads, each assembled from the profile fixtures it runs:
//!
//!   cluster-a          lab-cluster (`CLUSTER_UID`) running a
//!   cluster-a-staging  cluster-a, with the value "staging" for `cluster_env`
//!   cluster-b          lab-cluster (`CLUSTER_UID`) running b
//!   cluster-merged     lab-cluster (`CLUSTER_UID`) running b and addons
//!   cluster-merged-reversed  the same, with the two templates listed the other way round
//!
//! A cluster gives each variable its default value, or the value in its `values`; a sensitive
//! variable always gets `SECRET_VALUE`. No document must contain it.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::{Value, json};
use strata::closed_set::ClosedSet;
use strata::closed_set::unknown::send::{ADDRESS_SETTING, SETTING};
use strata::model::pack_type::PackType;

mod answer;
pub mod closed_set;
mod code_span;
mod download;
mod git;
mod palette;
mod process;
mod stand_in;
mod table;
mod toml;
mod walk;

pub use answer::*;
pub use closed_set::schema_values;
pub use code_span::*;
pub use download::*;
pub use git::isolate_git;
pub use palette::*;
pub use process::*;
pub use stand_in::*;
pub use table::*;
pub use toml::*;
pub use walk::walk;

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
                {"uid": UID_A, "version": "1.2.0"},
                {"uid": UID_B, "version": "1.3.0"},
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

/// The uid of the fixture b, `lab-profile@1.3.0`.
pub const UID_B: &str = "64f0000000000000000000b2";

/// The uid of the fixture addons, `lab-addons@2.0.0`.
pub const UID_ADDONS: &str = "64f0000000000000000000d4";

/// A cluster variable value that must never reach a snapshot.
pub const SECRET_VALUE: &str = "s3cr3t-value";

/// The uid of the profile version that the tests of a draft serve.
pub const DRAFT_PROFILE_UID: &str = "64f0000000000000000000d1";

/// The uid of the manifest `rules` in the tests of a draft.
pub const RULES_UID: &str = "64f00000000000000000a0e1";

/// The content of the manifest `rules` in the published template of the tests of a draft.
pub const PUBLISHED_RULES: &str =
    "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: rules\ndata:\n  level: warn";

/// The content of the manifest `rules` in the draft template of the tests of a draft.
pub const DRAFT_RULES: &str =
    "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: rules\ndata:\n  level: error";

/// The options of `same-release --allow no-digest`: the exact policy, and a pack with no upstream
/// digest can pass.
pub fn no_digest_allowed() -> strata::ops::SameReleaseOptions {
    strata::ops::SameReleaseOptions {
        allow_no_digest: true,
        ..strata::ops::SameReleaseOptions::default()
    }
}

/// The JSON document in the file at `path`.
pub fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

/// Write `doc`, a record or a `behind --since` report, to the file at `path`.
pub fn write_record(path: &Path, doc: &Value) {
    fs::write(path, serde_json::to_string_pretty(doc).unwrap()).unwrap();
}

/// Write `snap`, a record, to the file at `path`: compact JSON when `compact` is true, else
/// pretty JSON.
pub fn write_snapshot(path: &Path, snap: &strata::snapshot::Snapshot, compact: bool) {
    fs::write(path, snap.to_json(compact).unwrap()).unwrap();
}

/// Write `doc`, a candidate, to the file at `path` as YAML.
pub fn write_yaml(path: &Path, doc: &Value) {
    fs::write(path, serde_yaml::to_string(doc).unwrap()).unwrap();
}

/// The schema of the `fields` of the tier `tier`, in `record_schema`, the schema of a record.
pub fn tier_fields(record_schema: &Value, tier: strata::snapshot::Tier) -> &Value {
    use strata::closed_set::ClosedSet;
    let tiers = &record_schema["$defs"]["ClusterSource"]["properties"]["tiers"];
    &tiers["properties"][tier.as_str()]["allOf"][1]["properties"]["fields"]
}

/// True when `schema`, the schema of a text field, accepts the text `value`. It reads only the
/// parts that strata writes for a text: `anyOf`, `type`, `maxLength`, and `format`.
pub fn text_matches_schema(schema: &Value, value: &str) -> bool {
    if let Some(options) = schema["anyOf"].as_array() {
        return options.iter().any(|o| text_matches_schema(o, value));
    }
    let short_enough = schema["maxLength"]
        .as_u64()
        .is_none_or(|max| value.chars().count() as u64 <= max);
    let is_time =
        schema["format"] != "date-time" || chrono::DateTime::parse_from_rfc3339(value).is_ok();
    schema["type"] == "string" && short_enough && is_time
}

/// Assemble a cluster that runs the profile fixtures `profiles`, in order, the way Palette returns
/// it. The cluster has applied templates, per-profile pack manifests, and per-profile variables
/// with values. A variable gets its value from `values` by name, else its default. A sensitive
/// variable gets `SECRET_VALUE`.
fn write_cluster(root: &Path, dir: &str, profiles: &[&str], values: &[(&str, &str)]) {
    let out = root.join(dir);
    fs::create_dir_all(&out).unwrap();
    let (mut templates, mut manifests, mut variables) = (vec![], vec![], vec![]);
    for name in profiles {
        let prof = read_json(&root.join(name).join("profile.json"));
        let man = read_json(&root.join(name).join("manifests.json"));
        let vars = read_json(&root.join(name).join("variables.json"));
        templates.push(cluster_template(&prof));
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

/// Copy the three raw files from `from` to `to`, and apply each text replacement to each file.
pub fn rewrite(from: &Path, to: &Path, replacements: &[(&str, &str)]) {
    fs::create_dir_all(to).unwrap();
    for name in ["profile.json", "manifests.json", "variables.json"] {
        let mut text = fs::read_to_string(from.join(name)).unwrap();
        for (old, new) in replacements {
            text = text.replace(old, new);
        }
        fs::write(to.join(name), text).unwrap();
    }
}

/// Writes the profile fixtures a/ to a6/, b/ and addons/, and the cluster fixtures
/// cluster-a/, cluster-b/ and cluster-merged/, under `root`.
pub fn make_fixtures(root: &Path) {
    let packs_a = |k8s_values: &str| {
        vec![
            PackFixture::public("ubuntu-aws", "os", "22.04", "pk-os-2204", "", OS_VALUES)
                .pack_ref(&[]),
            PackFixture::public(
                "kubernetes",
                "k8s",
                "1.29.4",
                "pk-k8s-1294",
                "sha256:k8s1294",
                k8s_values,
            )
            .pack_ref(&[]),
            PackFixture::public(
                "cni-cilium-oss",
                "cni",
                "1.15.3",
                "pk-cni-1153",
                "sha256:cni-aaaa",
                CNI_VALUES,
            )
            .pack_ref(&[]),
            PackFixture::public(
                "csi-aws-ebs",
                "csi",
                "1.28.0",
                "pk-csi-1280",
                "sha256:csi-1280",
                CSI_VALUES,
            )
            .pack_ref(&[]),
            PackFixture::public(
                "prometheus-operator",
                "addon",
                "55.8.3",
                "pk-prom-5583",
                "sha256:prom-5583",
                PROM_VALUES,
            )
            .pack_ref(&[]),
            PackFixture::public(
                "external-dns",
                "addon",
                "0.14.0",
                "pk-edns-0140",
                "sha256:edns-0140",
                EXTDNS_VALUES,
            )
            .pack_ref(&[]),
            PackFixture::public(
                "metrics-server",
                "addon",
                "0.7.0",
                "pk-ms-070",
                "sha256:ms-070",
                METRICS_A,
            )
            .pack_ref(&[]),
        ]
    };
    let manifests_a = |k8s_values: &str| {
        vec![
            PackFixture::public(
                "ubuntu-aws",
                "os",
                "22.04",
                "pk-os-2204",
                "sha256:os-2204-registry",
                OS_VALUES,
            )
            .pack_manifests(vec![]),
            PackFixture::public(
                "kubernetes",
                "k8s",
                "1.29.4",
                "pk-k8s-1294",
                "sha256:k8s1294",
                k8s_values,
            )
            .pack_manifests(vec![]),
            PackFixture::public(
                "cni-cilium-oss",
                "cni",
                "1.15.3",
                "pk-cni-1153",
                "sha256:cni-aaaa",
                CNI_VALUES,
            )
            .pack_manifests(vec![]),
            PackFixture::public(
                "csi-aws-ebs",
                "csi",
                "1.28.0",
                "pk-csi-1280",
                "sha256:csi-1280",
                CSI_VALUES,
            )
            .pack_manifests(vec![]),
            PackFixture::public(
                "prometheus-operator",
                "addon",
                "55.8.3",
                "pk-prom-5583",
                "sha256:prom-5583",
                PROM_VALUES,
            )
            .pack_manifests(vec![manifest(
                "extra-rules",
                "m-rules-1",
                "sha256:rules-a",
                RULES_A,
            )]),
            PackFixture::public(
                "external-dns",
                "addon",
                "0.14.0",
                "pk-edns-0140",
                "sha256:edns-0140",
                EXTDNS_VALUES,
            )
            .pack_manifests(vec![]),
            PackFixture::public(
                "metrics-server",
                "addon",
                "0.7.0",
                "pk-ms-070",
                "sha256:ms-070",
                METRICS_A,
            )
            .pack_manifests(vec![]),
        ]
    };
    let vars_a = json!({"variables": [
        variable("cluster_env", "Environment", "dev", false),
        variable("grafana_password", "", "", true),
    ]});
    write_profile(
        &root.join("a"),
        UID_A,
        "1.2.0",
        packs_a(K8S_VALUES_A),
        manifests_a(K8S_VALUES_A),
        vars_a.clone(),
        vec![],
    );
    // The packs of a in the other order: Palette keeps the order given, so this is another root.
    let mut reversed = packs_a(K8S_VALUES_A);
    reversed.reverse();
    write_profile(
        &root.join("a-reversed"),
        UID_A,
        "1.2.0",
        reversed,
        manifests_a(K8S_VALUES_A),
        vars_a.clone(),
        vec![],
    );
    write_profile(
        &root.join("a2"),
        UID_A,
        "1.2.0",
        packs_a(K8S_VALUES_A2),
        manifests_a(K8S_VALUES_A2),
        vars_a,
        vec![],
    );
    // a3 and a4 are text transforms of a: the API shapes stay, only the identifiers move.
    // Manifest digests are not hashed, so rewriting every digest changes UPSTREAM alone.
    let (public, prod) = (
        format!("\"{PUBLIC_REGISTRY}\""),
        format!("\"{PROD_REGISTRY}\""),
    );
    rewrite(
        &root.join("a"),
        &root.join("a3"),
        &[
            (public.as_str(), prod.as_str()),
            ("\"pk-", "\"pkp-"),
            ("sha256:", "sha256:prod-"),
        ],
    );
    rewrite(
        &root.join("a3"),
        &root.join("a4"),
        &[("podSubnet: 192.168.0.0/16", "podSubnet: 10.0.0.0/16")],
    );
    rewrite(
        &root.join("a3"),
        &root.join("a5"),
        &[("\"type\": \"spectro\"", "\"type\": \"oci\"")],
    );
    rewrite(
        &root.join("a"),
        &root.join("a6"),
        &[
            ("\"type\": \"spectro\"", "\"type\": \"manifest\""),
            ("\"registryUid\": \"reg-public\"", "\"registryUid\": \"\""),
        ],
    );
    // The tag is the OS pack's alone: the version stays 22.04, and so do the uid and the digest.
    rewrite(
        &root.join("a"),
        &root.join("a7"),
        &[("\"tag\": \"22.04\"", "\"tag\": \"LTS__22.4\"")],
    );

    let packs_b = vec![
        PackFixture::public("ubuntu-aws", "os", "22.04", "pk-os-2204", "", OS_VALUES).pack_ref(&[]),
        PackFixture::public(
            "kubernetes",
            "k8s",
            "1.30.2",
            "pk-k8s-1302",
            "sha256:k8s1302",
            K8S_VALUES_B,
        )
        .pack_ref(&[]),
        PackFixture::public(
            "cni-cilium-oss",
            "cni",
            "1.15.3",
            "pk-cni-1153",
            "sha256:cni-bbbb",
            CNI_VALUES,
        )
        .pack_ref(&[]),
        PackFixture::public(
            "csi-aws-ebs",
            "csi",
            "1.28.0",
            "pk-csi-1280",
            "sha256:csi-1280",
            CSI_VALUES,
        )
        .pack_ref(&[]),
        PackFixture::public(
            "prometheus-operator",
            "addon",
            "55.8.3",
            "pk-prom-5583",
            "sha256:prom-5583",
            PROM_VALUES,
        )
        .pack_ref(&[]),
        PackFixture::public(
            "cert-manager",
            "addon",
            "1.14.0",
            "pk-cm-1140",
            "sha256:cm-1140",
            CERTMGR_VALUES,
        )
        .pack_ref(&[]),
        PackFixture::public(
            "metrics-server",
            "addon",
            "0.7.0",
            "pk-ms-070",
            "sha256:ms-070",
            METRICS_B,
        )
        .pack_ref(&[]),
    ];
    let manifests_b = vec![
        PackFixture::public(
            "ubuntu-aws",
            "os",
            "22.04",
            "pk-os-2204",
            "sha256:os-2204-registry",
            OS_VALUES,
        )
        .pack_manifests(vec![]),
        PackFixture::public(
            "kubernetes",
            "k8s",
            "1.30.2",
            "pk-k8s-1302",
            "sha256:k8s1302",
            K8S_VALUES_B,
        )
        .pack_manifests(vec![]),
        PackFixture::public(
            "cni-cilium-oss",
            "cni",
            "1.15.3",
            "pk-cni-1153",
            "sha256:cni-bbbb",
            CNI_VALUES,
        )
        .pack_manifests(vec![]),
        PackFixture::public(
            "csi-aws-ebs",
            "csi",
            "1.28.0",
            "pk-csi-1280",
            "sha256:csi-1280",
            CSI_VALUES,
        )
        .pack_manifests(vec![]),
        PackFixture::public(
            "prometheus-operator",
            "addon",
            "55.8.3",
            "pk-prom-5583",
            "sha256:prom-5583",
            PROM_VALUES,
        )
        .pack_manifests(vec![
            manifest("extra-rules", "m-rules-2", "sha256:rules-b", RULES_B),
            manifest("dashboards", "m-dash-1", "sha256:dash-b", DASH_B),
        ]),
        PackFixture::public(
            "cert-manager",
            "addon",
            "1.14.0",
            "pk-cm-1140",
            "sha256:cm-1140",
            CERTMGR_VALUES,
        )
        .pack_manifests(vec![]),
        PackFixture::public(
            "metrics-server",
            "addon",
            "0.7.0",
            "pk-ms-070",
            "sha256:ms-070",
            METRICS_B,
        )
        .pack_manifests(vec![]),
    ];
    let vars_b = json!({"variables": [
        variable("cluster_env", "Environment", "staging", false),
        variable("grafana_password", "", "", true),
        variable("db_password", "", "", true),
    ]});
    write_profile(
        &root.join("b"),
        UID_B,
        "1.3.0",
        packs_b,
        manifests_b,
        vars_b,
        vec!["cluster-aaa", "cluster-bbb"],
    );

    write_profile(
        &root.join("addons"),
        UID_ADDONS,
        "2.0.0",
        vec![
            PackFixture::public(
                "nginx-ingress",
                "addon",
                "4.10.0",
                "pk-ngx-4100",
                "sha256:ngx-4100",
                "charts:\n  ingress-nginx:\n    controller:\n      replicaCount: 2",
            )
            .pack_ref(&[]),
        ],
        vec![
            PackFixture::public(
                "nginx-ingress",
                "addon",
                "4.10.0",
                "pk-ngx-4100",
                "sha256:ngx-4100",
                "charts:\n  ingress-nginx:\n    controller:\n      replicaCount: 2",
            )
            .pack_manifests(vec![]),
        ],
        json!({"variables": [variable("ingress_class", "Ingress class", "nginx", false)]}),
        vec![],
    );
    rename_profile(&root.join("addons"), "lab-addons");
    retype_profile(&root.join("addons"), "add-on");

    write_cluster(root, "cluster-a", &["a"], &[]);
    write_cluster(
        root,
        "cluster-a-staging",
        &["a"],
        &[("cluster_env", "staging")],
    );
    write_cluster(root, "cluster-b", &["b"], &[]);
    write_cluster(root, "cluster-merged", &["b", "addons"], &[]);
    // The same cluster, with its templates listed the other way round, as Palette lists them
    // for another cluster built from one definition.
    write_cluster(root, "cluster-merged-reversed", &["addons", "b"], &[]);
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

/// Give the profile in `dir` another type; `write_profile` gives every profile the type cluster.
fn retype_profile(dir: &Path, kind: &str) {
    let path = dir.join("profile.json");
    let mut v = read_json(&path);
    v["spec"]["published"]["type"] = json!(kind);
    fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
}

// ---- the shell script helpers ------------------------------------------------------------------

/// A script by its path under scripts/, such as `ci/merge-gate.sh` or `dev/tag-release.sh`.
pub fn script(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join(name)
}

/// A script of the live example, by its name under examples/live/.
pub fn live_script(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/live")
        .join(name)
}

/// Copy each file of `files`, a path from the root of the repository, to the same path under
/// `tree`, and make it executable. A test that runs a script in a tree of its own copies each file
/// that the script sources too, such as `examples/live/common.sh`.
pub fn copy_scripts(tree: &Path, files: &[&str]) {
    for file in files {
        let to = tree.join(file);
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        executable(&to, &repo_file(file));
    }
}

/// A stand-in that logs its name and its arguments to the file `FAKE_LOG`, prints `FAKE_OUT`, and
/// exits with `FAKE_EXIT`. With `--out DIR`, it also writes `FAKE_OUT` to `DIR/report.md`, as
/// `strata preflight` writes its report.
pub const LOGGING_TOOL: &str = r#"#!/usr/bin/env bash
echo "$(basename "$0") $*" >> "$FAKE_LOG"
prev=""
for a in "$@"; do
  [ "$prev" != --out ] || { mkdir -p "$a"; printf '%s' "${FAKE_OUT-}" > "$a/report.md"; }
  prev=$a
done
printf '%s' "${FAKE_OUT-}"
exit "${FAKE_EXIT:-0}"
"#;

/// A stand-in for `kubectl port-forward`, named `fake-port-forward` on the `PATH`. Each stand-in
/// `kubectl` of a live script runs it for a port-forward.
///
/// It prints the line of a port-forward that is ready, and stops. `FAKE_FORWARD_LATE` gives a wait
/// in seconds before the line. The wait is real, because the stand-in `sleep` of a stage returns at
/// once. With `FAKE_FORWARD_FAILS`, it prints an error and no line, and exits with 1.
pub const FAKE_PORT_FORWARD: &str = r#"#!/usr/bin/env bash
[ -z "${FAKE_FORWARD_FAILS-}" ] || { echo "error: lost connection to pod" >&2; exit 1; }
[ -z "${FAKE_FORWARD_LATE-}" ] || command -p sleep "$FAKE_FORWARD_LATE"
echo "Forwarding from 127.0.0.1:${PORT:-18080} -> 8080"
"#;

/// The version of a manifest, from the root of the repository: `Cargo.toml` or
/// `crates/strata-preflight/Cargo.toml`.
pub fn version_of(manifest: &str) -> String {
    toml_value(repo_file(manifest).lines(), "version")
        .map(str::to_owned)
        .unwrap_or_else(|| panic!("{manifest} has a version line"))
}

pub fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The JSON Schema that `strata schema <kind>` prints.
pub fn schema(kind: &str) -> Value {
    let r = Run::of(
        strata_command()
            .args(["schema", kind])
            .env("STRATA_QUIET", "1"),
    );
    assert_eq!(r.code, 0, "{}", r.err);
    serde_json::from_str(&r.out).unwrap()
}

/// The reusable workflows that install a release of strata, in `.github/workflows/`.
pub const INSTALLING_WORKFLOWS: [&str; 7] = [
    "comment.yml",
    "merge-gate.yml",
    "preflight.yml",
    "promote-gate.yml",
    "retire-gate.yml",
    "upstream.yml",
    "verify.yml",
];

/// A git user that a machine gives in the environment, in the two forms that git reads. A session
/// container sets its user with `GIT_CONFIG_COUNT`, and `git -c` gives `GIT_CONFIG_PARAMETERS` to
/// the commands that it starts.
pub const MACHINE_GIT_USER: [&[(&str, &str)]; 2] = [
    &[
        ("GIT_CONFIG_COUNT", "1"),
        ("GIT_CONFIG_KEY_0", "user.name"),
        ("GIT_CONFIG_VALUE_0", "other"),
    ],
    &[("GIT_CONFIG_PARAMETERS", "'user.name'='other'")],
];

/// Run git in `dir` with no setting of the machine: see [`isolate_git`].
pub fn git(dir: &Path, args: &[&str]) -> std::process::Output {
    let mut c = Command::new("git");
    c.args(args).current_dir(dir);
    let o = isolate_git(&mut c).output_locked().unwrap();
    assert!(
        o.status.success(),
        "git {args:?}: {}{}",
        text(&o.stderr),
        text(&o.stdout)
    );
    o
}

// ---- the workflow helpers ----------------------------------------------------------------------

/// The text of a file of the repository, by its path from the root.
pub fn repo_file(path: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
        .unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The lines of `text` that are not comments. A line is a comment when `#` is its first character
/// after the indent.
pub fn code_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().filter(|l| !l.trim_start().starts_with('#'))
}

/// `text` without its comment lines, one line after the other.
pub fn without_comments(text: &str) -> String {
    code_lines(text).collect::<Vec<_>>().join("\n")
}

/// The text of a file of the repository, by its path from the root, without its comment lines.
pub fn code_of(path: &str) -> String {
    without_comments(&repo_file(path))
}

/// `text` with each run of white space as one space, as a line break can fall inside a sentence.
pub fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The section of the page `path` of the repository that starts at the line `heading`, such as
/// `## Install`. It ends at the next heading of the same level.
pub fn page_section(path: &str, heading: &str) -> String {
    let page = repo_file(path);
    let level = heading.split(' ').next().unwrap();
    let from = page
        .find(&format!("\n{heading}\n"))
        .unwrap_or_else(|| panic!("{path} has no section {heading:?}"));
    let to = page[from + 1..]
        .find(&format!("\n{level} "))
        .unwrap_or_else(|| panic!("no section follows {heading:?} in {path}"));
    page[from..from + 1 + to].to_string()
}

/// The fenced blocks of the language `lang` in `text`, in order. A block loses the indent of its
/// opening fence, so the block of a list item reads as it does in a terminal.
pub fn code_blocks(text: &str, lang: &str) -> Vec<String> {
    let opening = format!("```{lang}");
    let mut blocks = Vec::new();
    let mut open: Option<(usize, Vec<&str>)> = None;
    for line in text.lines() {
        let body = line.trim_start();
        let indent = line.len() - body.len();
        match open.take() {
            None => {
                if body == opening {
                    open = Some((indent, Vec::new()));
                }
            }
            Some((_, lines)) if body == "```" => blocks.push(lines.join("\n")),
            Some((strip, mut lines)) => {
                lines.push(line.get(strip..).unwrap_or(""));
                open = Some((strip, lines));
            }
        }
    }
    blocks
}

/// The first block of `lang` in `text` that holds `needle`.
pub fn block_with(text: &str, lang: &str, needle: &str) -> String {
    code_blocks(text, lang)
        .into_iter()
        .find(|block| block.contains(needle))
        .unwrap_or_else(|| panic!("no {lang} block holds {needle:?}"))
}

/// The command lines of the recipe `name` of the justfile, trimmed. A `{{variable}}` that the
/// justfile sets to a string in single quotes, such as `rustdoc`, is replaced with its value.
pub fn recipe(name: &str) -> Vec<String> {
    let justfile = repo_file("justfile");
    let variables: Vec<(String, &str)> = justfile
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(" := '")?;
            Some((format!("{{{{{key}}}}}"), value.strip_suffix('\'')?))
        })
        .collect();
    justfile
        .lines()
        .skip_while(|line| !line.starts_with(&format!("{name}:")))
        .skip(1)
        .take_while(|line| line.starts_with(' '))
        .map(|line| {
            variables
                .iter()
                .fold(line.trim().to_string(), |line, (key, value)| {
                    line.replace(key, value)
                })
        })
        .collect()
}

/// The commands that `just --dry-run` prints for the recipe and the arguments `args`. They are
/// the lines of the recipe and of its dependencies, with each variable replaced. No command runs.
/// `just` is the path that [`on_path`] gives.
pub fn just_dry_run(just: &Path, args: &[&str]) -> String {
    let o = Command::new(just)
        .arg("--dry-run")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output_locked()
        .unwrap();
    assert!(o.status.success(), "just {args:?}: {}", text(&o.stderr));
    text(&o.stderr)
}

/// The text of the workflow `file` of `.github/workflows/`.
pub fn workflow_text(file: &str) -> String {
    repo_file(&format!(".github/workflows/{file}"))
}

/// The file names of the workflows in `.github/workflows/`, in order.
pub fn workflow_files() -> Vec<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows");
    let mut files: Vec<String> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "yml" || e == "yaml"))
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    files.sort();
    files
}

/// The workflow `file` of `.github/workflows/`, parsed.
pub fn workflow(file: &str) -> serde_yaml::Value {
    serde_yaml::from_str(&workflow_text(file)).unwrap_or_else(|e| panic!("{file}: not YAML: {e}"))
}

/// The names of the strings in a YAML sequence.
pub fn strings(value: &serde_yaml::Value) -> Vec<&str> {
    value
        .as_sequence()
        .unwrap_or_else(|| panic!("not a sequence: {value:?}"))
        .iter()
        .map(|v| v.as_str().unwrap_or_else(|| panic!("not a string: {v:?}")))
        .collect()
}

/// The steps of each job of the workflow `doc`, in order.
pub fn steps(doc: &serde_yaml::Value) -> impl Iterator<Item = &serde_yaml::Value> {
    doc["jobs"]
        .as_mapping()
        .expect("a workflow has jobs")
        .values()
        .flat_map(|job| job["steps"].as_sequence().into_iter().flatten())
}

/// The `run` lines of the steps of the job `job` of the workflow `file`. `job` is the key of the
/// job, or its `name`.
pub fn runs_of(file: &str, job: &str) -> Vec<String> {
    let doc = workflow(file);
    let jobs = doc["jobs"]
        .as_mapping()
        .unwrap_or_else(|| panic!("{file} has no jobs"));
    let found = jobs
        .iter()
        .find(|(key, value)| key.as_str() == Some(job) || value["name"].as_str() == Some(job))
        .unwrap_or_else(|| panic!("{file} has no job {job}"))
        .1;
    found["steps"]
        .as_sequence()
        .unwrap_or_else(|| panic!("the job {job} of {file} has steps"))
        .iter()
        .filter_map(|step| step["run"].as_str().map(str::to_string))
        .collect()
}

/// The GitLab drop-in for the jobs of strata, from the root of the repository.
pub const GITLAB_DROP_IN: &str = "examples/ci/gitlab/strata.yml";

/// The GitLab drop-in [`GITLAB_DROP_IN`], parsed.
pub fn gitlab_drop_in() -> serde_yaml::Value {
    serde_yaml::from_str(&repo_file(GITLAB_DROP_IN))
        .unwrap_or_else(|e| panic!("{GITLAB_DROP_IN}: not YAML: {e}"))
}

/// The value of the variable `name` in the `variables` of the GitLab drop-in, such as the release
/// of `STRATA_VERSION` that its jobs install.
///
/// `gitlab` is the drop-in from [`gitlab_drop_in`]. A caller that reads many variables parses
/// the file one time.
pub fn gitlab_drop_in_variable<'a>(gitlab: &'a serde_yaml::Value, name: &str) -> &'a str {
    gitlab["variables"][name]
        .as_str()
        .unwrap_or_else(|| panic!("{GITLAB_DROP_IN} gives {name} no text value"))
}

/// The variables that `scripts/ci/install-strata.sh` reads a token from. A test of the installer
/// removes each one, so that no request gets a token of the machine.
pub const INSTALLER_TOKENS: [&str; 3] = ["STRATA_TOKEN", "GITHUB_TOKEN", "GH_TOKEN"];

/// Whether `text` holds `word` as a whole word: the character after it is not a letter, a digit,
/// or one of `joins`. `--thresh` is not in `--threshold`, and `$NAME` is not in `$NAME_2`,
/// although the longer text contains the shorter one.
pub fn whole_word(text: &str, word: &str, joins: &[char]) -> bool {
    text.match_indices(word).any(|(i, _)| {
        text[i + word.len()..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_alphanumeric() && !joins.contains(&c))
    })
}

// ---- the snapshot helpers ----------------------------------------------------------------------

/// A snapshot of the reference `text` from the in-memory Palette `api`, fetched at `t0`, with no
/// progress output.
pub fn snapshot_of(
    api: &strata::api::MemoryApi,
    text: &str,
) -> strata::Result<strata::snapshot::Snapshot> {
    snapshot_of_at(api, text, "t0")
}

/// [`snapshot_of`], fetched at the time `at`.
pub fn snapshot_of_at(
    api: &strata::api::MemoryApi,
    text: &str,
    at: &str,
) -> strata::Result<strata::snapshot::Snapshot> {
    snapshot_of_ref(api, &strata::reference::Ref::parse(text), at)
}

/// [`snapshot_of_at`], for a ref that the test has built and not parsed.
pub fn snapshot_of_ref(
    api: &strata::api::MemoryApi,
    reference: &strata::reference::Ref,
    at: &str,
) -> strata::Result<strata::snapshot::Snapshot> {
    strata::ops::snapshot_for(
        reference,
        &strata::ops::SnapshotOptions::default(),
        || Ok(strata::api::Palette::new(api)),
        at,
        &strata::term::progress::Progress::silent(),
    )
}

/// The options of `behind` that compare each bump, and ask Palette about the bump of each pin.
pub const WITH_IMPACT: strata::ops::BehindOptions = strata::ops::BehindOptions {
    impact: true,
    since: None,
    pin: None,
    pre_release: false,
};

/// `behind` on `api` itself and not on a copy, so that a test reads what `behind` posted. The
/// report has the time `2026-09-26T00:00:00Z`, with no progress output.
pub fn behind_of(
    api: &strata::api::MemoryApi,
    records: &[std::path::PathBuf],
    options: &strata::ops::BehindOptions,
) -> strata::ops::BehindReport {
    strata::ops::behind(
        records,
        options,
        |_| Ok(strata::api::Palette::new(api)),
        "2026-09-26T00:00:00Z",
        &strata::term::progress::Progress::silent(),
    )
    .unwrap()
}

/// A snapshot of the candidate file `file`, resolved against the in-memory Palette `api` and
/// fetched at `t0`, with no validation and no progress output.
pub fn candidate_snapshot(
    file: &Path,
    api: &strata::api::MemoryApi,
) -> strata::Result<strata::snapshot::Snapshot> {
    strata::ops::snapshot_from_candidate(
        file,
        false,
        || Ok(strata::api::Palette::new(api)),
        "t0",
        &strata::term::progress::Progress::silent(),
    )
}

/// Serve the raw files of the profile fixture `name` under `raw_root` as the version `uid`.
pub fn serve_fixture(
    api: strata::api::MemoryApi,
    raw_root: &Path,
    name: &str,
    uid: &str,
) -> strata::api::MemoryApi {
    let d = raw_root.join(name);
    serve_profile(
        api,
        uid,
        read_json(&d.join("profile.json")),
        read_json(&d.join("manifests.json")),
        read_json(&d.join("variables.json")),
    )
}

/// The digest that Palette gives a manifest content, as the tests make one: none for no content.
pub fn manifest_digest(content: &str) -> String {
    if content.is_empty() {
        String::new()
    } else {
        format!("sha1:{}", content.len())
    }
}

/// One manifest of a pack, as `GET /v1/clusterprofiles/{uid}/packs/{pack}/manifests/{manifest}`
/// gives it.
///
/// Each template has its content and the digest of that content.
pub fn manifest_entity(uid: &str, name: &str, published: &str, draft: &str) -> Value {
    let data = |content: &str| json!({"content": content, "digest": manifest_digest(content)});
    json!({"metadata": {"name": name, "uid": uid},
           "spec": {"published": data(published), "draft": data(draft)}})
}

/// The content of the manifest `name` in a record, or None when no pack of it has the manifest.
pub fn manifest_content<'a>(snap: &'a strata::snapshot::Snapshot, name: &str) -> Option<&'a str> {
    snap.packs
        .iter()
        .flat_map(|p| &p.manifests)
        .find(|m| m.name == name)
        .map(|m| m.content.as_str())
}

// ---- the refs and the listings -----------------------------------------------------------------

/// The ref of the cluster `uid`.
///
/// With `profile`, the ref names one profile of the cluster. With `pending` true, it names the
/// update that waits for the cluster.
pub fn cluster_ref(uid: &str, profile: Option<&str>, pending: bool) -> strata::reference::Ref {
    strata::reference::Ref::Cluster {
        uid: uid.to_string(),
        profile: profile.map(str::to_string),
        pending,
    }
}

/// The answer of Palette to the cluster listing for a key whose roles in the project `lab` do not
/// give `cluster.list`: HTTP 500, not 403, with the code of [`Denial::Project`].
///
/// [`Denial::Project`]: strata::api::codes::Denial::Project
pub fn clusters_denied() -> String {
    error_body(
        strata::api::codes::Denial::Project.as_str(),
        CLUSTERS_DENIED_MESSAGE,
    )
}

/// The answer of Palette to [`TAG_FILTER_LISTING`] for a user with no role that gives
/// `filter.list`: HTTP 403 with the code of [`Denial::Route`].
///
/// [`Denial::Route`]: strata::api::codes::Denial::Route
pub fn filters_denied() -> String {
    error_body(
        strata::api::codes::Denial::Route.as_str(),
        FILTERS_DENIED_MESSAGE,
    )
}

/// The listing that `options` asks for, from the in-memory Palette `api`, read at `t1` with no
/// progress output.
pub fn listing(
    api: &strata::api::MemoryApi,
    options: &strata::ops::ListOptions,
) -> strata::Result<strata::ops::Listing> {
    strata::ops::list(
        options,
        |_| Ok(strata::api::Palette::new(api)),
        "t1",
        &strata::term::progress::Progress::silent(),
    )
}

// ---- the renderers and the summaries -------------------------------------------------------------

/// The terminal renderer, with no color.
pub fn terminal() -> Box<dyn strata::render::Renderer> {
    strata::render::renderer(
        strata::render::Format::Terminal,
        strata::term::color::Paint::off(),
        false,
    )
}

/// The markdown renderer.
pub fn markdown() -> Box<dyn strata::render::Renderer> {
    strata::render::renderer(
        strata::render::Format::Markdown,
        strata::term::color::Paint::off(),
        false,
    )
}

/// Run `strata verify` on `record` against the in-memory Palette `api`, read at `t1`.
///
/// The record goes in a file first, as `verify` reads one.
pub fn verify_summary(
    record: &strata::snapshot::Snapshot,
    api: &strata::api::MemoryApi,
) -> strata::ops::VerifySummary {
    verify_summary_with(record, api, &strata::ops::VerifyOptions::default())
}

/// [`verify_summary`], with the options `options`.
pub fn verify_summary_with(
    record: &strata::snapshot::Snapshot,
    api: &strata::api::MemoryApi,
    options: &strata::ops::VerifyOptions,
) -> strata::ops::VerifySummary {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("record.json");
    write_snapshot(&path, record, true);
    strata::ops::verify_many(
        &[path],
        None,
        |_| Ok(strata::api::Palette::new(api.clone())),
        "t1",
        options,
        &strata::term::progress::Progress::silent(),
    )
}

// ---- the processes -----------------------------------------------------------------------------

/// The strata binary that cargo built for the tests. It is private. A test starts it with
/// [`strata_command`], or gives it to a script with [`give_strata`]. So no run of the tests can
/// send the report to the endpoint of the release.
fn strata_bin() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_strata"))
}

/// The address that refuses a connection: nothing listens on the port 9 of this machine.
pub const REFUSED: &str = "http://127.0.0.1:9";

/// Give `command` the guard of the send of the report: `STRATA_REPORT=0`, and a
/// `STRATA_REPORT_URL` that refuses a connection.
///
/// The release names the address of the real endpoint, so a run that sends posts there. The
/// setting `0` makes the run send nothing, whatever `~/.palette.env` or a saved answer say, because
/// the environment has priority. The address is the second guard. A test that enables the send and
/// gives no address of its own posts to [`REFUSED`], and never to the real endpoint. A test that
/// sends sets `STRATA_REPORT=1` and `STRATA_REPORT_URL` after this call, with the address of its
/// stand-in.
pub fn without_report(command: &mut Command) -> &mut Command {
    command.env(SETTING, "0").env(ADDRESS_SETTING, REFUSED)
}

/// The strata binary as a command that can send no report: see [`without_report`].
pub fn strata_command() -> Command {
    let mut command = Command::new(strata_bin());
    without_report(&mut command);
    command
}

/// Give `command` the path of the strata binary in the variable `name`, for a script that runs it,
/// and the guard of [`without_report`]. The script passes the guard to each strata that it starts.
pub fn give_strata<'a>(command: &'a mut Command, name: &str) -> &'a mut Command {
    without_report(command).env(name, strata_bin())
}

/// Run `examples/fetch-examples.sh` offline into a new directory, from the fetched pack data that the
/// repository tracks. Gives the directory and what the script printed.
///
/// The script runs the binary of the tests, or `wrapper` when there is one. The wrapper gets the
/// binary of the tests in `REAL_STRATA`. `env` holds the settings of the machine, which the guard
/// of [`without_report`] does not cover.
pub fn fetch_examples(wrapper: Option<&Path>, env: &[(&str, &str)]) -> (tempfile::TempDir, Output) {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = tempfile::tempdir().unwrap();
    for entry in fs::read_dir(repo.join("examples/packs")).unwrap() {
        let pack = entry.unwrap().path();
        let to = dir.path().join("packs").join(pack.file_name().unwrap());
        fs::create_dir_all(&to).unwrap();
        for file in ["_entity.json", "_summaries.json"] {
            fs::copy(pack.join(file), to.join(file)).unwrap();
        }
    }
    let mut fetch = Command::new("bash");
    match wrapper {
        None => give_strata(&mut fetch, "STRATA_BIN"),
        Some(wrapper) => give_strata(&mut fetch, "REAL_STRATA").env("STRATA_BIN", wrapper),
    };
    let o = fetch
        .arg(repo.join("examples/fetch-examples.sh"))
        .env("EXAMPLES_DIR", dir.path())
        .env("OFFLINE", "1")
        .envs(env.iter().copied())
        .output_locked()
        .unwrap();
    (dir, o)
}

/// Make the directory `bin` under `dir`, and put the stand-in commands `commands` in it: each is a
/// name and the body of its script. Gives the directory, for [`path_with`].
pub fn fake_bin(dir: &Path, commands: &[(&str, &str)]) -> std::path::PathBuf {
    let bin = dir.join("bin");
    fs::create_dir_all(&bin).unwrap();
    for (name, body) in commands {
        executable(&bin.join(name), body);
    }
    bin
}

/// The first `tool` on the `PATH` of the test, if there is one. The runner of GitHub has `kubectl` and
/// `just`. A test that needs one fails there when it is missing. A machine without it skips the test.
pub fn on_path(tool: &str) -> Option<std::path::PathBuf> {
    let found = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join(tool))
            .find(|c| c.is_file())
    });
    assert!(
        found.is_some() || std::env::var_os("CI").is_none(),
        "the runner of CI must have {tool}"
    );
    found
}

/// `PATH` with the directory `bin` first, so that a stand-in command in it runs in place of the
/// real one.
pub fn path_with(bin: &Path) -> String {
    format!("{}:{}", bin.display(), std::env::var("PATH").unwrap())
}

/// Run the shell text `command` in `dir`, as a reader runs the commands of a page. A stand-in
/// `kubectl` writes its arguments to a file and its standard input to another: read them with
/// [`kubectl_args`] and [`kubectl_input`].
pub fn run_with_kubectl(dir: &Path, command: &str, envs: &[(&str, &str)]) -> Output {
    let bin = fake_bin(
        dir,
        &[(
            "kubectl",
            "#!/usr/bin/env bash\necho \"$*\" >> kubectl.args\ncat > kubectl.in\n",
        )],
    );
    Command::new("bash")
        .arg("-c")
        .arg(format!("set -euo pipefail\n{command}"))
        .current_dir(dir)
        .env("PATH", path_with(&bin))
        .envs(envs.iter().copied())
        .output_locked()
        .unwrap()
}

/// The arguments of each call of the stand-in `kubectl` of [`run_with_kubectl`], one call on each
/// line.
pub fn kubectl_args(dir: &Path) -> String {
    written(dir, "kubectl.args")
}

/// The text that the stand-in `kubectl` of [`run_with_kubectl`] got on standard input in its last
/// call.
pub fn kubectl_input(dir: &Path) -> String {
    written(dir, "kubectl.in")
}

/// The file `name` that a stand-in command wrote in `dir`.
fn written(dir: &Path, name: &str) -> String {
    fs::read_to_string(dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// A `jq` that acts as jq 1.6, which the jq of Debian 12 is: `jq -e` exits with 0 when its input
/// is empty. jq 1.7 and later exit with 4. A call with `-s` or `-n` always has an input value, so
/// it gets no fault. Each other call goes to the next `jq` in `PATH`. So a test shows the fault
/// with the jq of the CI and of the session image.
///
/// The input is the last argument when it is a file, and else the standard input.
pub const JQ_1_6: &str = r#"#!/usr/bin/env bash
PATH=${PATH#"$(dirname "$0")":}
exit_status= whole=
for a in "$@"; do
  case "$a" in --exit-status) exit_status=1 ;; --slurp|--null-input) whole=1 ;; --*) continue ;; esac
  case "$a" in -*e*) exit_status=1 ;; esac
  case "$a" in -*[sn]*) whole=1 ;; esac
done
[ -n "$exit_status" ] && [ -z "$whole" ] || exec jq "$@"
input=${!#}
if [ ! -f "$input" ]; then
  input=$(mktemp)
  trap 'rm -f "$input"' EXIT
  cat > "$input"
fi
grep -q '[^[:space:]]' "$input" || exit 0
jq "$@" < "$input"
"#;

/// A `jq` that acts as the jq of `ubuntu-latest`: `rtrimstr` stops with the error `endswith()
/// requires string inputs` when its input is not a string. jq 1.7.1 returns the input, so a
/// filter that passes a null to `rtrimstr` runs there and stops on the runner. GitHub gives `null`
/// as the body of an issue that a person opens with no text.
///
/// The stand-in puts the definition in a file `.jq` of a new `HOME`, and jq reads that file before
/// the program. Each other part of a call goes to the next `jq` in `PATH`.
pub const JQ_STRICT_TRIM: &str = r#"#!/usr/bin/env bash
PATH=${PATH#"$(dirname "$0")":}
home=$(mktemp -d)
trap 'rm -rf "$home"' EXIT
echo 'def rtrimstr($x): if endswith($x) and ($x|length) > 0 then .[:-($x|length)] else . end;' > "$home/.jq"
HOME=$home jq "$@"
"#;

/// The lines of the file at `path`. The file must be there: a log that is missing is a fault of
/// the test, not "no call".
pub fn lines_of(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

/// The settings of the machine that name a project or a tenant. They are the variables of strata
/// and those of the Terraform provider, which strata also reads. It also holds the variable `CI`,
/// because a pipeline sets it.
const MACHINE_SETTINGS: [&str; 5] = [
    "PALETTE_PROJECT_UID",
    "PALETTE_PROJECT",
    "SPECTROCLOUD_APIKEY",
    "SPECTROCLOUD_HOST",
    "CI",
];

/// Give `command` an address that refuses connections and the home `home`. It gets no setting of
/// the machine that names a project, and no send of the report: see [`without_report`].
///
/// The run makes no retry, and it gets no setting that names a tenant either. So a request fails
/// at once, and the run reads only the `.palette.env` of `home`, not the one of the person who
/// runs the tests.
pub fn isolate_palette<'a>(command: &'a mut Command, home: &Path) -> &'a mut Command {
    for key in MACHINE_SETTINGS {
        command.env_remove(key);
    }
    without_report(command)
        .env("HOME", home)
        .env("PALETTE_API_KEY", "k")
        .env("PALETTE_API", REFUSED)
        .env("STRATA_RETRIES", "0")
}

/// What one run of the binary gave: the exit status, standard output, and standard error.
pub struct Run {
    pub code: i32,
    pub out: String,
    pub err: String,
}

impl Run {
    /// Run `command`, and keep what it gave.
    pub fn of(command: &mut Command) -> Run {
        let o = command.output_locked().expect("binary runs");
        Run {
            code: o.status.code().unwrap_or(-1),
            out: text(&o.stdout),
            err: text(&o.stderr),
        }
    }

    /// The JSON document of the output with no `generatedAt`, the one field that two runs do not
    /// share. An output that is not JSON gives `null`.
    pub fn document_without_time(&self) -> Value {
        let mut document: Value = serde_json::from_str(&self.out).unwrap_or(Value::Null);
        if let Some(fields) = document.as_object_mut() {
            fields.remove("generatedAt");
        }
        document
    }
}

/// Run `command` on `args` against an address that refuses connections, with a home of its own,
/// as [`isolate_palette`] gives it. So the path in a transport error is the first request strata
/// made.
///
/// `command` is the binary in the environment that the run starts from. That is the environment
/// of the tests, or a home and a project of a machine that a test gives. `envs` comes last: a
/// `HOME` in it replaces the empty home. With `quiet` false, the warnings that a person sees are
/// on.
pub fn against_closed_port(
    mut command: Command,
    args: &[&str],
    envs: &[(&str, &str)],
    quiet: bool,
) -> Run {
    let home = tempfile::tempdir().unwrap();
    isolate_palette(&mut command, home.path())
        .args(args)
        .env("STRATA_QUIET", "1");
    if !quiet {
        command.env_remove("STRATA_QUIET");
    }
    command.envs(envs.iter().copied());
    Run::of(&mut command)
}

/// The names of the commands of the binary, as the help of clap lists them under `Commands:`.
pub fn command_names() -> Vec<String> {
    let help = Run::of(strata_command().arg("--help"));
    assert_eq!(help.code, 0, "{}", help.err);
    help.out
        .lines()
        .skip_while(|line| *line != "Commands:")
        .skip(1)
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

/// Write the record of the raw profile fixture `name` under `raw` to `out` with the strata
/// binary, as `snapshot --from-raw` writes one.
pub fn snapshot_from_raw(raw: &Path, name: &str, out: &Path) {
    let o = strata_command()
        .args(["snapshot", "--from-raw"])
        .arg(raw.join(name))
        .arg("-o")
        .arg(out)
        .env("STRATA_QUIET", "1")
        .output_locked()
        .unwrap();
    assert!(o.status.success(), "snapshot {name}: {}", text(&o.stderr));
}

/// The `kind` (a body or a comment) that a stand-in kept from the call with the 1-based number
/// `call`, as `<kind>-<call>.md` under `dir`. [`FAKE_ISSUES_GH`] and the stand-in curl of the
/// GitLab tests write the files.
pub fn handed_in(dir: &Path, kind: &str, call: usize) -> String {
    fs::read_to_string(dir.join(format!("{kind}-{call}.md")))
        .unwrap_or_else(|_| panic!("no {kind} from call {call}"))
}

/// A `gh` for issues. It logs each call, and keeps each body file that the script gives it as
/// `body-<n>.md` in call order. It lists the open issues from issues.json. It numbers a new issue
/// from 41 up.
pub const FAKE_ISSUES_GH: &str = r#"#!/usr/bin/env bash
set -euo pipefail
echo "$*" >> "$FAKE_VERIFY/gh.log"
n=$(wc -l < "$FAKE_VERIFY/gh.log")
prev=""
for a in "$@"; do
  [ "$prev" = "--body-file" ] && cp "$a" "$FAKE_VERIFY/body-$n.md"
  [ "$prev" = "--comment" ] && printf '%s' "$a" > "$FAKE_VERIFY/comment-$n.md"
  prev="$a"
done
case "$*" in
  *" 99 "*) echo "GraphQL: Could not resolve to an issue with the number of 99." >&2; exit 1 ;;
  "issue close"*) echo "✓ Closed issue o/r#$3" >&2 ;;
  "label create"*) echo "✓ Label \"$3\" created in o/r" >&2 ;;
  "issue list"*) cat "$FAKE_VERIFY/issues.json" 2>/dev/null || echo "[]" ;;
  "issue create"*) c=$(cat "$FAKE_VERIFY/created" 2>/dev/null || echo 40); c=$((c + 1)); echo "$c" > "$FAKE_VERIFY/created"
                   echo "https://github.com/o/r/issues/$c" ;;
  "label list"*) cat "$FAKE_VERIFY/labels.json" 2>/dev/null || echo "[]" ;;
  "repo view"*) echo '{"nameWithOwner":"o/r"}' ;;
esac
"#;

/// The chain of `records` under `options`, with a Palette that serves nothing: for a chain that
/// compares no archive. A download makes the test fail.
pub fn same_release_offline(
    records: &[(String, strata::snapshot::Snapshot)],
    options: strata::ops::SameReleaseOptions,
    generated_at: &str,
) -> strata::ops::Chain {
    let api = strata::api::MemoryApi::new();
    same_release_served(records, options, &api, generated_at)
}

/// The chain of `records` under `options`, with the downloads that `api` serves in each project.
pub fn same_release_served(
    records: &[(String, strata::snapshot::Snapshot)],
    options: strata::ops::SameReleaseOptions,
    api: &strata::api::MemoryApi,
    generated_at: &str,
) -> strata::ops::Chain {
    strata::ops::same_release_with(
        records,
        options,
        |_| Ok(strata::api::Palette::new(api)),
        generated_at,
        &strata::term::progress::Progress::silent(),
    )
    .unwrap()
}

/// The files of the archive of the build of a pack, beside `pack.json`: the path and the content of
/// each. For a Helm chart, they are all the files of the chart. None for a pack whose download
/// holds no archive.
pub type BuildFiles<'a> =
    dyn Fn(&strata::snapshot::Pack) -> Option<Vec<(&'static str, String)>> + 'a;

/// `api`, which also serves the download of the profile version of each of `records`, as Palette
/// writes it: see [`profile_download`].
///
/// The download names each pack of a record that has a registry. The archive of the pack holds
/// `pack.json` and each file that `files` gives for the pack: its path and its content. The
/// archive of a Helm chart holds only the files that `files` gives, as a Helm registry serves the
/// chart: see [`chart_archive`]. A pack for which `files` gives None has no archive in the
/// download, as a pack of a private registry.
pub fn serve_downloads(
    api: strata::api::MemoryApi,
    records: &[(String, strata::snapshot::Snapshot)],
    files: &BuildFiles,
) -> strata::api::MemoryApi {
    serve_pack_files(api, records, &|p| build_archive(p, files))
}

/// The archive of the build of the pack `p`, as [`serve_downloads`] puts it in a download, with
/// the files that `files` gives. None when `files` gives none.
pub fn build_archive(p: &strata::snapshot::Pack, files: &BuildFiles) -> Option<Vec<u8>> {
    let files = files(p)?;
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (*path, text.as_str()))
        .collect();
    let kind = PackType::parse(&p.kind)?;
    if kind == PackType::Helm {
        return Some(chart_archive(&p.name, &files));
    }
    let top = archive_top(kind, &p.name, &p.version, &p.ref_digest);
    Some(pack_archive(&p.name, &p.version, &p.layer, &top, &files))
}

/// `api`, which also serves the download of the profile version of each of `records`. The file
/// of each pack is the one that `file` gives. An example is an archive from [`build_archive`], or
/// an error answer of the host of a chart. A pack for which `file` gives None has no file in the
/// download, as a pack of a private registry.
pub fn serve_pack_files(
    api: strata::api::MemoryApi,
    records: &[(String, strata::snapshot::Snapshot)],
    file: &dyn Fn(&strata::snapshot::Pack) -> Option<Vec<u8>>,
) -> strata::api::MemoryApi {
    records.iter().fold(api, |api, (_, s)| {
        let archives: Vec<Option<Vec<u8>>> = s.packs.iter().map(file).collect();
        let packs: Vec<DownloadPack> = s
            .packs
            .iter()
            .zip(archives)
            .filter_map(|(p, archive)| {
                Some(DownloadPack {
                    name: &p.name,
                    version: &p.version,
                    kind: PackType::parse(&p.kind)?,
                    private: archive.is_none(),
                    archive,
                    manifests: &[],
                })
            })
            .collect();
        let bytes = profile_download(&s.source.uid, &s.source.name, &packs);
        api.with_file(download_route(&s.source.uid), bytes)
    })
}
