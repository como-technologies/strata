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
//!   cluster-a          lab-cluster (CLUSTER_UID) running a
//!   cluster-a-staging  cluster-a, with the value "staging" for cluster_env
//!   cluster-b          lab-cluster (CLUSTER_UID) running b
//!   cluster-merged     lab-cluster (CLUSTER_UID) running b and addons
//!   cluster-merged-reversed  the same, with the two templates listed the other way round
//!
//! A cluster gives each variable its default value, or the value in its `values`; a sensitive
//! variable always gets SECRET_VALUE, which no document may contain.

#![allow(dead_code)]

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

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

/// The uid of the fixture a, `lab-profile@1.2.0`, and of each fixture made from a.
pub const UID_A: &str = "64f0000000000000000000a1";

/// The uid of the fixture b, `lab-profile@1.3.0`.
pub const UID_B: &str = "64f0000000000000000000b2";

/// The uid of the fixture addons, `lab-addons@2.0.0`.
pub const UID_ADDONS: &str = "64f0000000000000000000d4";

/// A cluster variable value that must never reach a snapshot.
pub const SECRET_VALUE: &str = "s3cr3t-value";

/// The JSON document in the file at `path`.
pub fn read_json(path: &Path) -> Value {
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
/// Copy a raw profile fixture with each replacement applied to its files.
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

pub fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Run git in `dir` with no user or global configuration, so signing settings cannot leak in.
pub fn git(dir: &Path, args: &[&str]) -> std::process::Output {
    let mut c = Command::new("git");
    c.args(args).current_dir(dir);
    let o = isolate_git(&mut c).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", text(&o.stderr));
    o
}

/// Write a file and make it executable, for a stand-in command on a test's PATH.
pub fn executable(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
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
    strata::ops::snapshot_for(
        &strata::reference::Ref::parse(text),
        || Ok(strata::api::Palette::new(api)),
        at,
        &strata::progress::Progress::silent(),
    )
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
        &strata::progress::Progress::silent(),
    )
}

// ---- the routes of Palette ---------------------------------------------------------------------
//
// The crate names each route in a const of its own that is not public, so the tests name the
// route again here, in one place.

/// The profile listing, `GET /v1/dashboard/clusterprofiles/metadata`.
pub const PROFILE_LISTING: &str = "/v1/dashboard/clusterprofiles/metadata";

/// The profile search, `POST /v1/dashboard/clusterprofiles?limit=50`.
pub const PROFILE_SEARCH: &str = "/v1/dashboard/clusterprofiles?limit=50";

/// The cluster listing, `GET /v1/dashboard/spectroclusters/meta`.
pub const CLUSTER_LISTING: &str = "/v1/dashboard/spectroclusters/meta";

/// The project listing, `GET /v1/dashboard/projects/metadata`.
pub const PROJECT_LISTING: &str = "/v1/dashboard/projects/metadata";

/// The cluster search, `POST /v1/dashboard/spectroclusters/search?limit=50`.
pub const CLUSTER_SEARCH: &str = "/v1/dashboard/spectroclusters/search?limit=50";

/// The schema of the cluster search: the properties that a search can filter on.
pub const CLUSTER_SEARCH_SCHEMA: &str = "/v1/dashboard/spectroclusters/search/schema";

/// The edge host search, `POST /v1/dashboard/edgehosts/search?limit=50`.
pub const EDGE_HOST_SEARCH: &str = "/v1/dashboard/edgehosts/search?limit=50";

/// The key of a POST to `route` in the in-memory Palette, for one answer to each body.
pub fn post(route: &str) -> String {
    format!("POST {route}")
}

/// The document of the profile version `uid`, `GET /v1/clusterprofiles/{uid}`.
pub fn profile_route(uid: &str) -> String {
    format!("/v1/clusterprofiles/{uid}")
}

/// The pack manifests of the profile version `uid`.
pub fn manifests_route(uid: &str) -> String {
    format!("/v1/clusterprofiles/{uid}/packs/manifests")
}

/// The variables of the profile version `uid`.
pub fn variables_route(uid: &str) -> String {
    format!("/v1/clusterprofiles/{uid}/variables")
}

/// The pack listing of the pack `name`, as strata asks for its first page.
pub fn pack_listing_route(name: &str) -> String {
    format!("/v1/packs?filters=spec.name={name}&limit=100&offset=0")
}

/// The registry listing, `GET /v1/registries/metadata`.
pub const REGISTRY_LISTING: &str = "/v1/registries/metadata";

/// The summary of the OCI registries, `GET /v1/registries/oci/summary`.
pub const OCI_REGISTRY_SUMMARY: &str = "/v1/registries/oci/summary";

/// One manifest `manifest` of the pack `pack` of the profile version `uid`, with the content of
/// each template.
pub fn manifest_route(uid: &str, pack: &str, manifest: &str) -> String {
    format!("/v1/clusterprofiles/{uid}/packs/{pack}/manifests/{manifest}")
}

// ---- the profile documents ---------------------------------------------------------------------

/// A template of a profile version: the cloud type, the type of the profile, and its packs.
pub fn template(cloud: &str, kind: &str, packs: Value) -> Value {
    json!({"cloudType": cloud, "type": kind, "packs": packs})
}

/// A template with no pack: the published template of a version that nobody published, or the
/// draft of a version with no draft.
pub fn no_template() -> Value {
    json!({"packs": []})
}

/// The document of the profile version `uid`, as `GET /v1/clusterprofiles/{uid}` gives it.
///
/// `published` and `draft` are its two templates. `status.isPublished` is true when the published
/// template has a pack. With `draft` None, the document has no draft template at all.
pub fn profile_document(
    uid: &str,
    name: &str,
    version: &str,
    published: Value,
    draft: Option<Value>,
) -> Value {
    let is_published = published["packs"].as_array().is_some_and(|p| !p.is_empty());
    let mut doc = json!({
        "metadata": {"name": name, "uid": uid},
        "spec": {"version": version, "published": published},
        "status": {"isPublished": is_published, "inUseClusterUids": []},
    });
    if let Some(draft) = draft {
        doc["spec"]["draft"] = draft;
    }
    doc
}

/// Serve the three documents of the profile version `uid`: its document, its pack manifests,
/// and its variables.
pub fn serve_profile(
    api: strata::api::MemoryApi,
    uid: &str,
    document: Value,
    manifests: Value,
    variables: Value,
) -> strata::api::MemoryApi {
    api.with(profile_route(uid), document)
        .with(manifests_route(uid), manifests)
        .with(variables_route(uid), variables)
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

/// The answer of `packs/manifests` for a version whose packs have no manifest.
pub fn no_manifests() -> Value {
    json!({"spec": {"packs": []}})
}

/// The answer of `variables` for a version with no variable.
pub fn no_variables() -> Value {
    json!({"variables": []})
}

/// One manifest of a pack, as `GET /v1/clusterprofiles/{uid}/packs/{pack}/manifests/{manifest}`
/// gives it: the content of each template, with the digest that Palette gives a content, and no
/// digest for no content.
pub fn manifest_entity(uid: &str, name: &str, published: &str, draft: &str) -> Value {
    let data = |content: &str| {
        let digest = if content.is_empty() {
            String::new()
        } else {
            format!("sha1:{}", content.len())
        };
        json!({"content": content, "digest": digest})
    };
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

/// The ref of the cluster `uid`: of its profile `profile` when one is named, and of the update
/// that waits for it when `pending` is true.
pub fn cluster_ref(uid: &str, profile: Option<&str>, pending: bool) -> strata::reference::Ref {
    strata::reference::Ref::Cluster {
        uid: uid.to_string(),
        profile: profile.map(str::to_string),
        pending,
    }
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
        &strata::progress::Progress::silent(),
    )
}

// ---- the processes -----------------------------------------------------------------------------

/// The strata binary that cargo built for the tests.
pub fn strata_bin() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_strata"))
}

/// `PATH` with the directory `bin` first, so that a stand-in command in it runs in place of the
/// real one.
pub fn path_with(bin: &Path) -> String {
    format!("{}:{}", bin.display(), std::env::var("PATH").unwrap())
}

/// The lines of the file at `path`, or none when there is no file.
pub fn lines_of(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

/// Give `command` no user or global git configuration and a fixed author, so that a signing
/// setting of the machine cannot reach the test.
pub fn isolate_git(command: &mut Command) -> &mut Command {
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
}

/// Write the record of the raw profile fixture `name` under `raw` to `out` with the strata
/// binary, as `snapshot --from-raw` writes one.
pub fn snapshot_from_raw(raw: &Path, name: &str, out: &Path) {
    let o = Command::new(strata_bin())
        .args(["snapshot", "--from-raw"])
        .arg(raw.join(name))
        .arg("-o")
        .arg(out)
        .env("STRATA_QUIET", "1")
        .output()
        .unwrap();
    assert!(o.status.success(), "snapshot {name}: {}", text(&o.stderr));
}
