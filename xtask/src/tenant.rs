//! The tenant that the stand-in for Palette serves.
//!
//! Each chapter of the book builds the tenant that one page needs, from the real pack data in
//! `examples/`, and changes it between two commands in the way that Palette changes: an edit in
//! place, a version bump, a cluster that takes an update.
//!
//! A document here answers one request. `Tenant::doc` gives the file that answers a path, and
//! every helper writes one or more of those files.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

/// The directory of the documents that Palette serves at tenant scope, with no `ProjectUid`.
pub const TENANT: &str = "tenant";

/// The uid of the Public Repo pack registry, as Palette gives it.
pub const PUBLIC_REPO: &str = "5eecc89d0b150045ae661cef";
/// The uid of the OCI pack registry that the live run of 20260917T150025Z found.
pub const PALETTE_REGISTRY: &str = "64ec000000000000000000a1";
const OCI_CSI_PACK: &str = "64ec000000000000000000b1";
/// The uid of a private Helm registry, which Palette does not sync.
pub const PRIVATE_CHARTS: &str = "64ed000000000000000000a1";
/// The chart that a profile pins from it.
const CHART: &str = "podinfo";
/// The host cluster of each cluster group. The book lists no host cluster.
const GROUP_HOST: &str = "64f5000000000000000000b1";
/// The values that a cluster group gives each virtual cluster: YAML with comments, as Palette
/// gives it.
const GROUP_VALUES: &str = "# Sync describes how to sync resources to the host cluster.\nsync:\n  toHost:\n    services:\n      enabled: true\n    ingresses:\n      enabled: false\n# The external database of each virtual cluster.\ndatabase:\n  user: vcluster\n  password: s3cr3t\n";
const OCI_CSI_DIGEST: &str =
    "sha256:a746e14689085b14531566f9c9cfb2bc7716e96a394547ef50a1c52eab510c9e";

// The routes of Palette, as the server names their files: the path, and the query after an `@`.
// A route that two helpers write has one const or one fn here, and the server reads the same
// one.

/// The projects that the key can see, `GET /v1/dashboard/projects/metadata`.
pub const PROJECT_LISTING: &str = "v1/dashboard/projects/metadata";
/// The profile search, `POST /v1/dashboard/clusterprofiles`.
///
/// The server keeps the items of the scope that the body names.
pub const PROFILE_SEARCH_PATH: &str = "v1/dashboard/clusterprofiles";
/// The cluster search, `POST /v1/dashboard/spectroclusters/search`.
///
/// The server keeps the clusters that the body selects.
pub const CLUSTER_SEARCH_PATH: &str = "v1/dashboard/spectroclusters/search";
/// The audit log, `GET /v1/audits`.
///
/// The server answers an empty log for a resource with no file.
pub const AUDITS: &str = "v1/audits";
/// The profile documents of a scope, `v1/clusterprofiles/{uid}`.
const PROFILES: &str = "v1/clusterprofiles";
/// The cluster documents of a project, `v1/spectroclusters/{uid}`.
const CLUSTERS: &str = "v1/spectroclusters";
/// The cluster group documents of a project, `v1/clustergroups/{uid}`.
const GROUPS: &str = "v1/clustergroups";
/// The edge host documents of a project, `v1/edgehosts/{uid}`.
const EDGE_HOSTS: &str = "v1/edgehosts";
/// The app profile documents of a project, `v1/appProfiles/{uid}`.
const APP_PROFILES: &str = "v1/appProfiles";
/// The pack registries of a scope, `GET /v1/registries/metadata`.
const REGISTRY_LISTING: &str = "v1/registries/metadata";

/// The file of a listing or a search that strata reads by pages: `path` with the page of
/// strata.
///
/// One page holds every item of the stand-in.
fn paged(path: &str) -> String {
    format!("{path}@limit=50")
}

/// The document of the profile version `uid`, `GET /v1/clusterprofiles/{uid}`.
fn profile_route(uid: &str) -> String {
    format!("{PROFILES}/{uid}")
}

/// The manifests of the profile version `uid`, `GET /v1/clusterprofiles/{uid}/packs/manifests`.
fn manifests_route(uid: &str) -> String {
    format!("{}/packs/manifests", profile_route(uid))
}

/// The variables of the profile version `uid`, `GET /v1/clusterprofiles/{uid}/variables`.
fn variables_route(uid: &str) -> String {
    format!("{}/variables", profile_route(uid))
}

/// One manifest of the pack `pack` of the profile version `uid`,
/// `GET /v1/clusterprofiles/{uid}/packs/{pack}/manifests/{manifest}`.
fn manifest_route(uid: &str, pack: &str, manifest: &str) -> String {
    format!("{}/packs/{pack}/manifests/{manifest}", profile_route(uid))
}

/// The document of the cluster `uid`, `GET /v1/spectroclusters/{uid}`.
fn cluster_route(uid: &str) -> String {
    format!("{CLUSTERS}/{uid}")
}

/// The variables of the cluster `uid`, `GET /v1/spectroclusters/{uid}/variables`.
fn cluster_variables_route(uid: &str) -> String {
    format!("{}/variables", cluster_route(uid))
}

/// The role bindings of the cluster `uid`, `GET /v1/spectroclusters/{uid}/config/rbacs`.
pub fn rbacs_route(uid: &str) -> String {
    format!("{}/config/rbacs", cluster_route(uid))
}

/// The namespaces of the cluster `uid`, `GET /v1/spectroclusters/{uid}/config/namespaces`.
fn namespaces_route(uid: &str) -> String {
    format!("{}/config/namespaces", cluster_route(uid))
}

/// The repave check of the cluster `uid`, `POST /v1/spectroclusters/{uid}/validate/repave`.
fn repave_route(uid: &str) -> String {
    format!("{}/validate/repave", cluster_route(uid))
}

/// The notifications of the cluster `uid`, `GET /v1/notifications/spectrocluster/{uid}`.
fn notifications_route(uid: &str) -> String {
    format!("v1/notifications/spectrocluster/{uid}")
}

/// The document of the cluster group `uid`, `GET /v1/clustergroups/{uid}`.
fn group_route(uid: &str) -> String {
    format!("{GROUPS}/{uid}")
}

/// The virtual clusters of the cluster group `uid`, as the server names the file of
/// `GET /v1/dashboard/clustergroups/{uid}/virtualClusters`.
fn virtual_clusters_file(uid: &str) -> String {
    paged(&format!("v1/dashboard/clustergroups/{uid}/virtualClusters"))
}

/// The document of the app profile `uid`, `GET /v1/appProfiles/{uid}`.
fn app_profile_route(uid: &str) -> String {
    format!("{APP_PROFILES}/{uid}")
}

/// The macros of the project `project`, `GET /v1/projects/{project}/macros`.
fn macros_route(project: &str) -> String {
    format!("v1/projects/{project}/macros")
}

/// The versions of the pack `name` in each registry, as the server names the file of
/// `GET /v1/packs?filters=spec.name={name}&limit=100&offset=0`: the first page of the listing.
fn pack_listing(name: &str) -> String {
    format!("v1/packs@filters=spec.name={name}&limit=100&offset=0")
}

/// The pack `name` in the registry `registry`, `GET /v1/packs/{name}/registries/{registry}`.
fn pack_entity_route(name: &str, registry: &str) -> String {
    format!("v1/packs/{name}/registries/{registry}")
}

/// The audit log of the resource `uid` of the kind `kind`, as the server names the file of
/// `GET /v1/audits?resourceKind={kind}&resourceUid={uid}&limit=100`.
///
/// The server leaves the time out of the query.
fn audit_log_file(kind: &str, uid: &str) -> String {
    format!("{AUDITS}@resourceKind={kind}&resourceUid={uid}&limit=100")
}

/// The cloud config of the cluster `uid` for the cloud type `kind`.
///
/// Palette serves the cloud config of each cloud type at its own path. The stand-in gives the
/// config the uid of the cluster with a `c` at the end.
fn cloud_config_route(kind: &str, uid: &str) -> String {
    format!("v1/cloudconfigs/{kind}/{uid}c")
}

/// One change to a published profile document, as a person makes it through the API.
#[derive(Clone, Debug)]
pub enum Edit {
    /// Set the CSI pack's reclaim policy, which is one value inside its values YAML.
    Reclaim(&'static str),
    /// Give a pack another tag name for the build it already pins.
    Tag {
        /// The pack whose tag changes.
        pack: &'static str,
        /// The tag name to write.
        tag: &'static str,
    },
    /// Take the CSI pack from the OCI registry, as a team that promotes a pack does.
    OciCsi,
    /// Give the version another number, which a bump without the immutability flag does.
    Version(&'static str),
    /// Give the profile another name, as `PATCH /v1/clusterprofiles/{uid}/metadata` does: the
    /// uid and the version stay.
    Name(&'static str),
    /// Add one command to `kubeadmconfig.postKubeadmCommands` of the Kubernetes pack: a key that
    /// Palette repaves on.
    KubeadmCommand(&'static str),
    /// Move a pack to another version: its `packUid`, tag, and version change, as an upgrade of
    /// the pack does. The impact table calls this a repave for the OS and Kubernetes layers.
    Upgrade {
        /// The pack that moves.
        pack: &'static str,
        /// The `packUid` of the other version.
        uid: &'static str,
        /// The other version, also its tag.
        version: &'static str,
    },
    /// Remove the digest of a pack, on its pack reference and in its pack summary, as a registry
    /// that Palette does not sync gives it.
    NoDigest(&'static str),
    /// Make one value of a pack read a project macro: a line `<macro>: "{{.spectro.macro.<macro>}}"`
    /// at the end of the pack's values.
    Macro {
        /// The pack whose values read the macro.
        pack: &'static str,
        /// The macro name, and the key of the new line.
        name: &'static str,
    },
    /// Name a machine image in the annotation `imageId` of a pack ref, as a
    /// bring-your-own-image OS pack does. The values and the pack build stay.
    ImageId {
        /// The pack that names the image.
        pack: &'static str,
        /// The id of the image, such as an AMI.
        id: &'static str,
    },
    /// Mark a pack ref invalid, as Palette does when the registry deletes the tag of the pack.
    /// `isInvalid` is true, and `inValidReason` is the message of the event of the registry. The
    /// values and the pack build stay.
    Invalid {
        /// The pack whose tag the registry deleted.
        pack: &'static str,
        /// The message of the event, `<display name> <tag> tag is deleted`.
        reason: &'static str,
    },
    /// Give a pack a secret value: a line `<key>: '********'` at the end of the pack's values.
    /// The schema of the pack gives the key the format `password`, so Palette serves its mask in
    /// place of the secret, whatever the secret is.
    Masked {
        /// The pack whose values hold the secret.
        pack: &'static str,
        /// The key of the secret.
        key: &'static str,
    },
}

impl Edit {
    /// The part of an edit that lands in the pack summaries of `packs/manifests`.
    fn apply_manifests(&self, manifests: &mut Value) {
        if let Edit::NoDigest(name) = self {
            for pack in manifests["spec"]["packs"]
                .as_array_mut()
                .into_iter()
                .flatten()
            {
                if pack["spec"]["name"] == *name {
                    pack["spec"]["digest"] = json!("");
                }
            }
        }
    }

    fn apply(&self, doc: &mut Value) {
        match self {
            Edit::Reclaim(policy) => {
                let from = if *policy == "Retain" {
                    "Delete"
                } else {
                    "Retain"
                };
                for pack in packs_mut(doc) {
                    if pack["name"] == "csi-aws-ebs"
                        && let Some(values) = pack["values"].as_str()
                    {
                        let next = values.replacen(
                            &format!("reclaimPolicy: {from}"),
                            &format!("reclaimPolicy: {policy}"),
                            1,
                        );
                        pack["values"] = json!(next);
                    }
                }
            }
            Edit::Tag { pack: name, tag } => {
                for pack in packs_mut(doc) {
                    if pack["name"] == *name {
                        pack["tag"] = json!(tag);
                    }
                }
            }
            Edit::OciCsi => {
                for pack in packs_mut(doc) {
                    if pack["name"] == "csi-aws-ebs" {
                        pack["type"] = json!("oci");
                        pack["registryUid"] = json!(PALETTE_REGISTRY);
                        pack["packUid"] = json!(OCI_CSI_PACK);
                        pack["digest"] = json!(OCI_CSI_DIGEST);
                    }
                }
            }
            Edit::Version(version) => doc["spec"]["version"] = json!(version),
            Edit::Name(name) => doc["metadata"]["name"] = json!(name),
            Edit::KubeadmCommand(command) => {
                for pack in packs_mut(doc) {
                    if pack["name"] == "kubernetes"
                        && let Some(values) = pack["values"].as_str()
                    {
                        let anchor = "    - 'chmod 600 /var/lib/kubelet/config.yaml'\n";
                        pack["values"] = json!(values.replacen(
                            anchor,
                            &format!("{anchor}    - '{command}'\n"),
                            1
                        ));
                    }
                }
            }
            Edit::Upgrade {
                pack: name,
                uid,
                version,
            } => {
                for pack in packs_mut(doc) {
                    if pack["name"] == *name {
                        pack["packUid"] = json!(uid);
                        pack["tag"] = json!(version);
                        pack["version"] = json!(version);
                    }
                }
            }
            Edit::NoDigest(name) => {
                for pack in packs_mut(doc) {
                    if pack["name"] == *name {
                        pack["digest"] = json!("");
                    }
                }
            }
            Edit::ImageId { pack: name, id } => {
                for pack in packs_mut(doc) {
                    if pack["name"] == *name {
                        pack["annotations"]["imageId"] = json!(id);
                    }
                }
            }
            Edit::Invalid { pack: name, reason } => {
                for pack in packs_mut(doc) {
                    if pack["name"] == *name {
                        pack["isInvalid"] = json!(true);
                        pack["inValidReason"] = json!(reason);
                    }
                }
            }
            Edit::Masked { pack: name, key } => {
                for pack in packs_mut(doc) {
                    if pack["name"] == *name {
                        let values = pack["values"].as_str().unwrap_or_default();
                        pack["values"] =
                            json!(format!("{}\n{key}: '********'\n", values.trim_end()));
                    }
                }
            }
            Edit::Macro {
                pack: name,
                name: m,
            } => {
                for pack in packs_mut(doc) {
                    if pack["name"] == *name {
                        let values = pack["values"].as_str().unwrap_or_default();
                        pack["values"] = json!(format!(
                            "{}\n{m}: \"{{{{.spectro.macro.{m}}}}}\"\n",
                            values.trim_end()
                        ));
                    }
                }
            }
        }
    }
}

fn packs_mut(doc: &mut Value) -> impl Iterator<Item = &mut Value> {
    doc["spec"]["published"]["packs"]
        .as_array_mut()
        .map(|p| p.iter_mut())
        .into_iter()
        .flatten()
}

/// A refusal in the answer of `validate/packs` for a cluster: the pack `pack` of the profile
/// `profile` requires the pack `required` at `version` or newer.
#[derive(Debug, Clone, Copy)]
pub struct PackRefusal<'a> {
    /// The uid of the profile that holds the pack.
    pub profile: &'a str,
    /// The name of the pack that has the requirement.
    pub pack: &'a str,
    /// The name of the pack that it requires.
    pub required: &'a str,
    /// The minimum version of the required pack.
    pub version: &'a str,
}

/// The tenant directory, and the repository that holds the example pack data.
pub struct Tenant {
    root: PathBuf,
    repo: PathBuf,
}

impl Tenant {
    /// A tenant under `root`, reading its example profiles from `repo`.
    pub fn new(root: &Path, repo: &Path) -> Self {
        Tenant {
            root: root.to_path_buf(),
            repo: repo.to_path_buf(),
        }
    }

    /// The file that answers `GET path` in `project`, or at tenant scope for `"tenant"`. The
    /// directory above it is created, so a caller can write the file at once.
    pub fn doc(&self, project: &str, path: &str) -> PathBuf {
        let file = self.root.join(project).join(format!("{path}.json"));
        if let Some(parent) = file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        file
    }

    /// Write the answer of one request, for a route that no other helper writes: `path` is the
    /// request path without the leading slash, such as `v1/spectroclusters/aws/validate`.
    pub fn put(&self, project: &str, path: &str, value: &Value) {
        self.write(project, path, value);
    }

    fn write(&self, project: &str, path: &str, value: &Value) {
        let file = self.doc(project, path);
        let text = serde_json::to_string_pretty(value).expect("a document is JSON");
        std::fs::write(&file, text + "\n").unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    }

    fn read(&self, project: &str, path: &str) -> Value {
        let file = self.doc(project, path);
        let text =
            std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", file.display()))
    }

    /// Empty the tenant, keeping the directory: the server holds its path.
    pub fn clear(&self) {
        if let Ok(entries) = std::fs::read_dir(&self.root) {
            for entry in entries.flatten() {
                let path = entry.path();
                let _ = if path.is_dir() {
                    std::fs::remove_dir_all(&path)
                } else {
                    std::fs::remove_file(&path)
                };
            }
        }
    }

    /// The projects that the key can see, as `name=uid` pairs.
    pub fn projects(&self, pairs: &[(&str, &str)]) {
        let items: Vec<Value> = pairs
            .iter()
            .map(|(name, uid)| json!({"metadata": {"name": name, "uid": uid}}))
            .collect();
        for (pair, item) in pairs.iter().zip(&items) {
            self.write(pair.1, PROJECT_LISTING, &json!({"items": [item]}));
        }
        self.write(TENANT, PROJECT_LISTING, &json!({"items": items}));
        self.listings();
    }

    /// The profile and cluster listings of each scope, from the documents in the tenant. At tenant
    /// scope Palette lists the tenant's own profiles, and the clusters of every project.
    pub fn listings(&self) {
        let mut every_cluster = Vec::new();
        let mut scopes: Vec<String> = std::fs::read_dir(&self.root)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name != TENANT)
            .collect();
        scopes.sort();
        // Palette lists a profile of the tenant in each project, and serves its document there.
        // So each project gets a copy of the documents of each tenant profile. Its listing and
        // its search name the tenant versions beside its own.
        let tenant_profiles = self.documents_in(TENANT, PROFILES);
        for project in &scopes {
            for doc in &tenant_profiles {
                self.copy_profile(
                    TENANT,
                    project,
                    doc["metadata"]["uid"].as_str().unwrap_or(""),
                );
            }
            let profiles: Vec<Value> = self
                .documents_in(project, PROFILES)
                .into_iter()
                .filter(|d| d["metadata"]["annotations"]["scope"] != "tenant")
                .chain(tenant_profiles.iter().cloned())
                .collect();
            // a deleted cluster leaves the listing and the search, and keeps its document
            let clusters: Vec<Value> = self
                .documents_in(project, CLUSTERS)
                .into_iter()
                .filter(|d| d["status"]["state"] != "Deleted")
                .collect();
            every_cluster.extend(clusters.iter().cloned());
            self.write_listings(project, &profiles, &clusters);
        }
        self.write_listings(TENANT, &tenant_profiles, &every_cluster);
        // the search of the tenant: every cluster, which the server filters by the body of a
        // request, and the schema that says the search takes tags
        let projects: Value = std::fs::read_to_string(self.doc(TENANT, PROJECT_LISTING))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let names: Map<String, Value> = projects["items"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| {
                (
                    p["metadata"]["uid"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    p["metadata"]["name"].clone(),
                )
            })
            .collect();
        // The search reads a summary of each cluster, and the summary of a virtual cluster does
        // not name the profiles that its cluster group pins, as on a live Palette (#184). So a
        // search by profile does not find a virtual cluster through its group. The summary of a
        // virtual cluster names its group and its host cluster in `status.virtual`.
        let mut group_pins: Map<String, Value> = Map::new();
        let mut group_of: Map<String, Value> = Map::new();
        for project in &scopes {
            for group in self.documents_in(project, GROUPS) {
                let gid = group["metadata"]["uid"].as_str().unwrap_or_default();
                let pinned: Vec<Value> = group["spec"]["clusterProfileTemplates"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|t| t["uid"].clone())
                    .collect();
                let host = group["spec"]["clustersConfig"]["hostClustersConfig"][0]["clusterUid"]
                    .as_str()
                    .unwrap_or_default();
                let host_path = cluster_route(host);
                let host_name = if self.doc(project, &host_path).is_file() {
                    self.read(project, &host_path)["metadata"]["name"].clone()
                } else {
                    json!("")
                };
                let virtual_summary = json!({
                    "clusterGroup": {"uid": gid, "name": group["metadata"]["name"],
                                     "kind": "clustergroup"},
                    "hostCluster": {"uid": host, "name": host_name, "kind": "spectrocluster"},
                });
                let members = self.read(project, &virtual_clusters_file(gid));
                for m in members["items"].as_array().into_iter().flatten() {
                    let muid = m["metadata"]["uid"].as_str().unwrap_or_default();
                    let entry = group_pins.entry(muid.to_string()).or_insert(json!([]));
                    if let Some(list) = entry.as_array_mut() {
                        list.extend(pinned.iter().cloned());
                    }
                    group_of.insert(muid.to_string(), virtual_summary.clone());
                }
            }
        }
        let found: Vec<Value> = every_cluster
            .iter()
            .map(|c| {
                let project = c["metadata"]["annotations"]["projectUid"]
                    .as_str()
                    .unwrap_or_default();
                let mut c = c.clone();
                let uid = c["metadata"]["uid"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let pins = group_pins.get(&uid).cloned();
                if let (true, Some(pins)) = (nested(&c), pins)
                    && let Some(ts) = c["spec"]["clusterProfileTemplates"].as_array_mut()
                {
                    ts.retain(|t| !pins.as_array().is_some_and(|p| p.contains(&t["uid"])));
                }
                search_row(
                    &c,
                    names
                        .get(project)
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    group_of.get(&uid).filter(|_| nested(&c)),
                )
            })
            .collect();
        // the search of a project holds the clusters of that project, and the search of the
        // tenant every cluster; each scope has the schema
        let schema = json!({"schema": {"properties": [
            {"name": "clusterName", "type": "string"}, {"name": "clusterState", "type": "string"},
            {"name": "tags", "type": "string"}, {"name": "environment", "type": "string"},
            {"name": "clusterProfiles", "type": "string"}]}});
        for project in &scopes {
            let rows: Vec<Value> = found
                .iter()
                .filter(|r| r["specSummary"]["projectMeta"]["uid"] == json!(project))
                .cloned()
                .collect();
            self.write_cluster_search(project, rows, &schema);
        }
        self.write_cluster_search(TENANT, found, &schema);
    }

    /// The profile listing, the profile search, and the cluster listing of one scope.
    ///
    /// The cluster listing leaves out each virtual cluster; the search holds it, when a condition
    /// asks.
    fn write_listings(&self, scope: &str, profiles: &[Value], clusters: &[Value]) {
        let items: Vec<Value> = profiles.iter().map(profile_row).collect();
        self.write(
            scope,
            "v1/dashboard/clusterprofiles/metadata",
            &json!({"items": items}),
        );
        self.write(
            scope,
            &paged(PROFILE_SEARCH_PATH),
            &profile_search(profiles),
        );
        let rows: Vec<Value> = clusters
            .iter()
            .filter(|c| !nested(c))
            .map(cluster_row)
            .collect();
        self.write(scope, "v1/dashboard/spectroclusters/meta", &json!(rows));
    }

    /// The cluster search of one scope, with `rows` as its clusters, and the schema of the search.
    fn write_cluster_search(&self, scope: &str, rows: Vec<Value>, schema: &Value) {
        self.write(
            scope,
            &paged(CLUSTER_SEARCH_PATH),
            &json!({"items": rows, "listmeta": {"continue": ""}}),
        );
        self.write(scope, &format!("{CLUSTER_SEARCH_PATH}/schema"), schema);
    }

    /// Copy the documents of the profile `uid` from the directory `from` to the directory `to`:
    /// the profile document, and each document under its path, such as its manifests and its
    /// variables.
    fn copy_profile(&self, from: &str, to: &str, uid: &str) {
        let path = profile_route(uid);
        let source = self.doc(from, &path);
        let target = self.doc(to, &path);
        std::fs::copy(&source, &target)
            .unwrap_or_else(|e| panic!("{} -> {}: {e}", source.display(), target.display()));
        copy_dir(
            &self.root.join(from).join(&path),
            &self.root.join(to).join(&path),
        );
    }

    /// The documents directly inside one directory of a project, in name order, so a listing does
    /// not move when a file system gives another order.
    fn documents_in(&self, project: &str, path: &str) -> Vec<Value> {
        let dir = self.root.join(project).join(path);
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "json"))
            .collect();
        files.sort();
        files
            .iter()
            .filter_map(|f| std::fs::read_to_string(f).ok())
            .filter_map(|t| serde_json::from_str(&t).ok())
            .collect()
    }

    /// Publish a copy of the example profile `example`, such as `aws-stack@1.1.1`, in `project`
    /// under `uid` and `version`, with each edit applied.
    pub fn profile(&self, project: &str, uid: &str, version: &str, example: &str, edits: &[Edit]) {
        self.profile_of_scope(project, "project", uid, version, example, edits);
    }

    /// Publish a copy of the example profile `example` at tenant scope, under `uid` and
    /// `version`. Palette lists a profile of the tenant in each project, beside the profiles of
    /// the project. It serves its document there too, so `listings` gives each project a copy.
    pub fn tenant_profile(&self, uid: &str, version: &str, example: &str, edits: &[Edit]) {
        self.profile_of_scope(TENANT, "tenant", uid, version, example, edits);
    }

    /// Publish a copy of `example` in the directory `project`, with the scope `scope` in
    /// `metadata.annotations.scope`, as Palette gives it.
    fn profile_of_scope(
        &self,
        project: &str,
        scope: &str,
        uid: &str,
        version: &str,
        example: &str,
        edits: &[Edit],
    ) {
        let from = self.repo.join("examples/profiles/raw").join(example);
        let text = std::fs::read_to_string(from.join("profile.json"))
            .unwrap_or_else(|e| panic!("{}: {e}", from.display()));
        let mut doc: Value = serde_json::from_str(&text).expect("an example profile is JSON");
        doc["metadata"]["uid"] = json!(uid);
        doc["metadata"]["annotations"]["scope"] = json!(scope);
        doc["spec"]["version"] = json!(version);
        doc["status"]["inUseClusterUids"] = json!([]);
        doc["status"]["inUseClusters"] = json!([]);
        for edit in edits {
            edit.apply(&mut doc);
        }
        self.write(project, &profile_route(uid), &doc);

        let text = std::fs::read_to_string(from.join("manifests.json")).unwrap_or_default();
        let mut manifests: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({}));
        manifests["metadata"]["uid"] = json!(uid);
        for edit in edits {
            edit.apply_manifests(&mut manifests);
        }
        self.write(project, &manifests_route(uid), &manifests);

        let text = std::fs::read_to_string(from.join("variables.json")).unwrap_or_default();
        let variables: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({}));
        self.write(project, &variables_route(uid), &variables);
        self.resolve_profile_macros(project, uid);
        self.listings();
    }

    /// The export of the published profile `uid`, as `GET /v1/clusterprofiles/{uid}/export` gives
    /// it with `format=json`.
    ///
    /// The export holds the name, the description, the labels, the version, the type, and the
    /// cloud. It also holds each pack with its values, its manifests, and the metadata of its
    /// registry, and the variables. Palette omits values that are empty and a pack with no
    /// manifest, and gives a manifest pack an empty registry. The export holds no params and no
    /// pack uid.
    pub fn export(&self, project: &str, uid: &str) {
        let doc = self.read(project, &profile_route(uid));
        let manifests = self.read(project, &manifests_route(uid));
        let variables = self.read(project, &variables_route(uid));
        let registries = self.read(project, REGISTRY_LISTING);
        let registry = |registry_uid: &Value| {
            let row = registries["items"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|r| r["uid"] == *registry_uid);
            match row {
                Some(r) => json!({"uid": r["uid"], "name": r["name"], "kind": r["kind"],
                    "isPrivate": false, "providerType": "", "isSyncSupported": true}),
                None => json!({"uid": "", "name": "", "kind": "", "isPrivate": false,
                    "providerType": "", "isSyncSupported": false}),
            }
        };
        let published = &doc["spec"]["published"];
        let packs: Vec<Value> = published["packs"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| {
                let mut pack = json!({"name": p["name"], "type": p["type"], "layer": p["layer"],
                    "version": p["version"], "tag": p["tag"],
                    "registry": {"metadata": registry(&p["registryUid"])}});
                if p["values"].as_str().is_some_and(|v| !v.is_empty()) {
                    pack["values"] = p["values"].clone();
                }
                let content: Vec<Value> = manifests["spec"]["packs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|m| m["metadata"]["name"] == p["name"])
                    .flat_map(|m| {
                        m["spec"]["manifests"]
                            .as_array()
                            .cloned()
                            .unwrap_or_default()
                    })
                    .map(|m| json!({"name": m["name"], "content": m["content"]}))
                    .collect();
                if !content.is_empty() {
                    pack["manifests"] = json!(content);
                }
                pack
            })
            .collect();
        let export = json!({
            "metadata": {"name": doc["metadata"]["name"],
                "description": doc["metadata"]["annotations"]["description"].as_str().unwrap_or(""),
                "labels": doc["metadata"]["labels"]},
            "spec": {"version": doc["spec"]["version"],
                "template": {"type": published["type"], "cloudType": published["cloudType"],
                    "packs": packs},
                "variables": variables["variables"]},
        });
        self.write(
            project,
            &format!("{}/export@format=json", profile_route(uid)),
            &export,
        );
    }

    /// Publish `platform-addons`, an add-on profile with one manifest pack. Palette gives the
    /// manifest pack of each profile a new pack uid, so two copies of one manifest differ.
    pub fn addons(&self, project: &str, uid: &str, pack: &str, label: &str, version: &str) {
        self.addons_named(project, uid, "platform-addons", pack, label, version);
    }

    /// [`Tenant::addons`] under another profile name.
    pub fn addons_named(
        &self,
        project: &str,
        uid: &str,
        name: &str,
        pack: &str,
        label: &str,
        version: &str,
    ) {
        let content = format!(
            "apiVersion: v1\nkind: Namespace\nmetadata:\n  name: payments\n  labels:\n    \
             pod-security.kubernetes.io/enforce: {label}"
        );
        self.write(
            project,
            &profile_route(uid),
            &json!({
                "apiVersion": "v1", "kind": "ClusterProfile",
                "metadata": {"name": name, "uid": uid, "annotations": {"scope": "project"}},
                "spec": {"version": version, "published": {"cloudType": "all", "type": "add-on",
                    "packs": [{"name": "namespace-policy", "layer": "addon", "type": "manifest",
                        "tag": "1.0.0", "version": "1.0.0", "packUid": pack,
                        "manifests": [{"kind": "manifest", "name": "namespace-policy"}],
                        "presets": [], "schema": []}]}},
                "status": {"isPublished": true, "inUseClusterUids": [], "inUseClusters": []}
            }),
        );
        self.write(
            project,
            &manifests_route(uid),
            &json!({
                "metadata": {"name": name, "uid": uid, "annotations": {"scope": "project"}},
                "spec": {"packs": [{"metadata": {"name": "namespace-policy", "uid": pack},
                    "spec": {"name": "namespace-policy", "layer": "addon", "version": "1.0.0",
                        "manifests": [{"name": "namespace-policy", "content": content}]},
                    "status": {}}]}
            }),
        );
        self.write(project, &variables_route(uid), &json!({"variables": []}));
        self.listings();
    }

    /// A Helm registry that is private, `charts-auth`. Palette does not sync it, so the pack
    /// listing has no chart of it until a profile pins one.
    pub fn private_charts(&self, project: &str) {
        let path = REGISTRY_LISTING;
        let mut listing = self.read(project, path);
        if let Some(items) = listing["items"].as_array_mut() {
            items.push(
                json!({"name": "charts-auth", "uid": PRIVATE_CHARTS, "kind": "helm",
                "isPrivate": true}),
            );
        }
        self.write(project, path, &listing);
        self.write(
            project,
            &pack_listing(CHART),
            &json!({"items": [], "listmeta": {"count": 0}}),
        );
    }

    /// Create and publish the add-on profile `charts-probe`, which pins the chart of
    /// [`Tenant::private_charts`] at `version`. Palette makes the pack at the create, with the uid
    /// `pack`, the layer addon, the tag as the version, and no digest. The listing then has it.
    pub fn chart_profile(&self, project: &str, uid: &str, pack: &str, version: &str) {
        let values = "replicaCount: 2\n";
        self.write(
            project,
            &profile_route(uid),
            &json!({
                "apiVersion": "v1", "kind": "ClusterProfile",
                "metadata": {"name": "charts-probe", "uid": uid},
                "spec": {"version": "1.0.0", "published": {"cloudType": "all", "type": "add-on",
                    "packs": [{"name": CHART, "layer": "addon", "type": "helm", "tag": version,
                        "version": version, "packUid": pack, "registryUid": PRIVATE_CHARTS,
                        "values": values, "presets": [], "schema": []}]}},
                "status": {"isPublished": true, "inUseClusterUids": [], "inUseClusters": []}
            }),
        );
        self.write(
            project,
            &manifests_route(uid),
            &json!({
                "metadata": {"name": "charts-probe", "uid": uid},
                "spec": {"packs": [{"metadata": {"name": CHART, "uid": pack},
                    "spec": {"name": CHART, "layer": "addon", "version": version,
                        "registryUid": PRIVATE_CHARTS, "digest": "", "manifests": []},
                    "status": {}}]}
            }),
        );
        self.write(project, &variables_route(uid), &json!({"variables": []}));
        let made = json!({"metadata": {"uid": pack, "name": CHART},
            "spec": {"name": CHART, "version": version, "registryUid": PRIVATE_CHARTS,
                "layer": "addon", "type": "helm", "digest": ""}});
        self.write(
            project,
            &pack_listing(CHART),
            &json!({"items": [made], "listmeta": {"count": 1}}),
        );
        self.write(
            project,
            &pack_entity_route(CHART, PRIVATE_CHARTS),
            &json!({"name": CHART, "layer": "addon",
                "tags": [{"tag": version, "version": version, "packUid": pack}],
                "packValues": [{"packUid": pack, "values": ""}]}),
        );
        self.listings();
    }

    /// Change a published profile in place, as an edit through the API does, and log the write
    /// in the audit log, as Palette does. The document takes the time of the write in
    /// `metadata.lastModifiedTimestamp`.
    pub fn edit(&self, project: &str, uid: &str, edits: &[Edit]) {
        let path = profile_route(uid);
        let mut doc = self.read(project, &path);
        for edit in edits {
            edit.apply(&mut doc);
        }
        let at = self.log_write(project, "clusterprofile", uid, &doc);
        doc["metadata"]["lastModifiedTimestamp"] = json!(at);
        self.write(project, &path, &doc);
        self.notify_clusters(project, uid, &doc);
        self.listings();
    }

    /// Edit a published profile with no publish, as a PUT of `/v1/clusterprofiles/{uid}` does:
    /// the edits go to `spec.draft`, which starts as a copy of the published template, and
    /// `spec.published` stays. Palette gives no `status.hasDraft`. `packs/manifests` then lists
    /// the packs of the draft, with their values, and the published content of each manifest.
    /// Each manifest with a uid answers its own route, with its draft content and its published
    /// content; an edit of values leaves the draft content of a manifest empty.
    pub fn draft(&self, project: &str, uid: &str, edits: &[Edit]) {
        let path = profile_route(uid);
        let mut doc = self.read(project, &path);
        let mut edited = doc.clone();
        for edit in edits {
            edit.apply(&mut edited);
        }
        doc["spec"]["draft"] = edited["spec"]["published"].clone();
        let at = self.log_write(project, "clusterprofile", uid, &doc);
        doc["metadata"]["lastModifiedTimestamp"] = json!(at);
        self.write(project, &path, &doc);

        let listed = manifests_route(uid);
        let mut manifests = self.read(project, &listed);
        let draft = doc["spec"]["draft"]["packs"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for pack in manifests["spec"]["packs"]
            .as_array_mut()
            .into_iter()
            .flatten()
        {
            if let Some(d) = draft.iter().find(|d| d["name"] == pack["spec"]["name"]) {
                pack["spec"]["values"] = d["values"].clone();
            }
        }
        for edit in edits {
            edit.apply_manifests(&mut manifests);
        }
        self.write(project, &listed, &manifests);
        for pack in manifests["spec"]["packs"].as_array().into_iter().flatten() {
            for m in pack["spec"]["manifests"].as_array().into_iter().flatten() {
                let Some(manifest) = m["uid"].as_str().filter(|u| !u.is_empty()) else {
                    continue;
                };
                self.write(
                    project,
                    &manifest_route(
                        uid,
                        pack["spec"]["name"].as_str().unwrap_or_default(),
                        manifest,
                    ),
                    &json!({"metadata": {"name": m["name"], "uid": manifest},
                        "spec": {"published": {"content": m["content"], "digest": m["digest"]},
                            "draft": {"content": "", "digest": ""}}}),
                );
            }
        }
        self.listings();
    }

    /// Make profile `uid` a version that nobody published, as a create with no publish leaves
    /// it: the packs are in `spec.draft`, `spec.published` holds no pack, and
    /// `status.isPublished` is false. The profile listing still names the version.
    pub fn unpublish(&self, project: &str, uid: &str) {
        let path = profile_route(uid);
        let mut doc = self.read(project, &path);
        doc["spec"]["draft"] = doc["spec"]["published"].clone();
        doc["spec"]["published"] = json!({"packs": []});
        doc["status"]["isPublished"] = json!(false);
        self.write(project, &path, &doc);
        self.listings();
    }

    /// Publish the draft of a profile, as `PATCH /v1/clusterprofiles/{uid}/publish` does: the
    /// draft becomes the published template, and the draft is empty again. The version keeps its
    /// uid and its number, so each cluster that runs it learns of an update, as after an edit in
    /// place.
    pub fn publish(&self, project: &str, uid: &str) {
        let path = profile_route(uid);
        let mut doc = self.read(project, &path);
        doc["spec"]["published"] = doc["spec"]["draft"].clone();
        doc["spec"]["draft"] = json!({"packs": []});
        doc["status"]["isPublished"] = json!(true);
        let at = self.log_write(project, "clusterprofile", uid, &doc);
        doc["metadata"]["lastModifiedTimestamp"] = json!(at);
        self.write(project, &path, &doc);
        self.notify_clusters(project, uid, &doc);
        self.listings();
    }

    /// Tell each cluster that runs profile `uid` whether an update waits for it, as Palette does
    /// after an edit in place. A live run gives the rules: a change of a pack opens one
    /// notification, a change of the version number alone opens none, and an edit that gives the
    /// profile the content of the cluster's copy again closes the open ones. `cluster` closes
    /// them too, when the cluster takes the update.
    fn notify_clusters(&self, project: &str, uid: &str, doc: &Value) {
        let clusters: Vec<String> = doc["status"]["inUseClusterUids"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c.as_str().map(str::to_string))
            .collect();
        let packs = doc["spec"]["published"]["packs"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for cluster in clusters {
            let copy: Vec<Value> =
                self.read(project, &cluster_route(&cluster))["spec"]["clusterProfileTemplates"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|t| t["uid"] == json!(uid))
                    .and_then(|t| t["packs"].as_array().cloned())
                    .unwrap_or_default();
            self.write_profile_updates(project, &cluster);
            let path = notifications_route(&cluster);
            let mut items = self.notifications(project, &cluster);
            if copy == packs {
                for n in &mut items {
                    n["action"]["isDone"] = json!(true);
                }
                self.write_notifications(project, &path, items);
                continue;
            }
            // one event for each pack that the edit changed, as Palette names it
            let events: Map<String, Value> = packs
                .iter()
                .filter(|p| !copy.contains(p))
                .enumerate()
                .map(|(i, p)| {
                    let name = p["name"].as_str().unwrap_or_default();
                    let version = p["version"]
                        .as_str()
                        .or(p["tag"].as_str())
                        .unwrap_or_default();
                    let values_only = copy.iter().any(|c| {
                        c["name"] == p["name"] && c["tag"] == p["tag"] && c["values"] != p["values"]
                    });
                    let (kind, message) = if values_only {
                        (
                            "PackValuesUpdate",
                            format!("{name} {version} values are updated"),
                        )
                    } else {
                        ("PackUpdate", format!("Pack '{name}' is updated"))
                    };
                    (
                        format!("e{i}"),
                        json!({"profileUid": uid, "type": kind, "message": message}),
                    )
                })
                .collect();
            items.insert(
                0,
                json!({
                    "metadata": {
                        "uid": format!("64fc{:020x}", items.len() + 1),
                        "creationTimestamp": format!("2026-09-18T14:{:02}:12.107Z", 2 + items.len()),
                        "name": format!("spectrocluster '{cluster}' notification"),
                    },
                    "type": "NotificationPackUpdate",
                    "action": {
                        "actionType": "NotifyActionClusterProfileUpdate",
                        "actionMessage": "Apply cluster profile pack updates",
                        "isDone": false,
                        "events": events,
                    },
                    "relatedObject": {"kind": "spectrocluster", "uid": cluster},
                }),
            );
            self.write_notifications(project, &path, items);
        }
    }

    /// `GET /v1/spectroclusters/{uid}/profileUpdates`: each profile of the cluster as the profile
    /// is now, in the shape of the cluster's templates. A live run gives the rule: Palette lists
    /// each profile of the cluster also when no update waits.
    fn write_profile_updates(&self, project: &str, cluster: &str) {
        let profiles: Vec<Value> =
            self.read(project, &cluster_route(cluster))["spec"]["clusterProfileTemplates"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|t| {
                    let uid = t["uid"].as_str().unwrap_or_default();
                    template_of(&self.read(project, &profile_route(uid)))
                })
                .collect();
        self.write(
            project,
            &format!("{}/profileUpdates", cluster_route(cluster)),
            &json!({ "profiles": profiles }),
        );
    }

    /// An edge host `uid`, `name`, with `profiles` bound to it: its own copy of each profile as
    /// the profile is now. A live run gives the rules: the host keeps its copy of the values,
    /// and holds each manifest by uid alone, so strata reads the manifests of the profile.
    pub fn edge_host(&self, project: &str, uid: &str, name: &str, profiles: &[&str]) {
        let templates = self.holder_templates(project, uid, profiles, |_, _, copy| {
            format!("{EDGE_HOSTS}/{uid}/pack/manifests/{copy}")
        });
        self.write(
            project,
            &format!("{EDGE_HOSTS}/{uid}"),
            &json!({
                "metadata": {"uid": uid, "name": name,
                    "annotations": {"projectUid": project, "scope": "project"}},
                "spec": {"clusterProfileTemplates": templates},
                "status": {"state": "ready"},
            }),
        );
        // the search gives a summary of each template: uid, name, and version
        let items: Vec<Value> = self
            .documents_in(project, EDGE_HOSTS)
            .iter()
            .map(|h| {
                let summary: Vec<Value> = h["spec"]["clusterProfileTemplates"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|t| {
                        json!({"uid": t["uid"], "name": t["name"],
                        "version": t["profileVersion"], "type": t["type"]})
                    })
                    .collect();
                json!({"metadata": h["metadata"], "spec": {"clusterProfileTemplates": summary},
                    "status": h["status"]})
            })
            .collect();
        let search = json!({"items": items, "listmeta": {"continue": ""}});
        for scope in [project, TENANT] {
            self.write(scope, &paged("v1/dashboard/edgehosts/search"), &search);
        }
    }

    /// A cluster template `uid`, `name`, that links `profiles`. Palette names each template that
    /// links a version in `status.inUseClusterTemplates` of the profile document, with the fields
    /// of `v1ProjectResourceReference`, and refuses the delete of the version while one does.
    pub fn cluster_template(&self, project: &str, uid: &str, name: &str, profiles: &[&str]) {
        for p in profiles {
            let path = profile_route(p);
            let mut doc = self.read(project, &path);
            let mut rows: Vec<Value> = doc["status"]["inUseClusterTemplates"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|t| t["uid"] != json!(uid))
                .collect();
            rows.push(json!({"kind": "clustertemplate", "name": name,
                             "projectUid": project, "uid": uid}));
            doc["status"]["inUseClusterTemplates"] = json!(rows);
            self.write(project, &path, &doc);
        }
    }

    /// A cluster group `uid`, `name`, that pins `profiles` for its virtual clusters, with the
    /// clusters `members` as those virtual clusters. The group gives each virtual cluster the
    /// config of `spec.clustersConfig`, as Palette gives it: limits, an endpoint, and values.
    pub fn cluster_group(
        &self,
        project: &str,
        uid: &str,
        name: &str,
        profiles: &[&str],
        members: &[&str],
    ) {
        let templates = self.holder_templates(project, uid, profiles, manifest_route);
        self.write(
            project,
            &group_route(uid),
            &json!({
                "metadata": {"uid": uid, "name": name},
                "spec": {
                    "type": "hostCluster",
                    "clusterProfileTemplates": templates,
                    "clustersConfig": {
                        "endpointType": "LoadBalancer",
                        "hostClustersConfig": [{"clusterUid": GROUP_HOST, "endpointConfig": {
                            "ingressConfig": {"host": "*", "port": 6443},
                            "loadBalancerConfig": {"externalIPs": null,
                                "loadBalancerSourceRanges": null},
                        }}],
                        "kubernetesDistroType": "vcluster-generic",
                        "limitConfig": {"cpu": 6, "cpuMilliCore": 6000, "memory": 12288,
                            "memoryMiB": 12288, "overSubscription": 120, "storageGiB": 10},
                        "values": GROUP_VALUES,
                    },
                },
            }),
        );
        self.write(
            project,
            "v1/clustergroups/hostCluster/metadata",
            &json!({"items": [{"uid": uid, "name": name, "scope": "project"}]}),
        );
        let items: Vec<Value> = members
            .iter()
            .map(|m| {
                let doc = self.read(project, &cluster_route(m));
                json!({"metadata": {"uid": m, "name": doc["metadata"]["name"],
                    "annotations": {"projectUid": project}}})
            })
            .collect();
        self.write(
            project,
            &virtual_clusters_file(uid),
            &json!({"items": items, "listmeta": {"continue": ""}}),
        );
        self.resolve_group_macros(project, uid);
    }

    /// The app profile `shop@<version>`, `uid`: a helm tier `web` with the property `replicas`,
    /// and a manifest tier `config`. A second call for the same uid is a plain update, as
    /// Palette takes one: the uid and the version stay. `web_order` is the install order of
    /// `web`. Palette gives no `installOrder` for a tier whose order is 0, as for `config`.
    pub fn app_profile(
        &self,
        project: &str,
        uid: &str,
        version: &str,
        replicas: &str,
        web_order: u32,
    ) {
        let mut web = json!({"metadata": {"name": "web", "uid": format!("{uid}1")},
            "spec": {"type": "helm", "version": "2.1.0", "registryUid": PUBLIC_REPO,
                "sourceAppTierUid": "64ed000000000000000000a1",
                "values": "replicaCount: 2\nimage:\n  tag: 2.1.0\n",
                "properties": [{"name": "replicas", "type": "number", "value": replicas}]}});
        if web_order > 0 {
            web["spec"]["installOrder"] = web_order.into();
        }
        self.write(
            project,
            "v1/dashboard/appProfiles/metadata",
            &json!({"appProfiles": [{"metadata": {"name": "shop", "uid": uid},
                "spec": {"version": version}}]}),
        );
        self.write(
            project,
            &app_profile_route(uid),
            &json!({
                "metadata": {"name": "shop", "uid": uid},
                "spec": {"version": version,
                    "versions": [{"uid": uid, "version": version}],
                    "template": {"appTiers": [
                        {"name": "web", "uid": format!("{uid}1"), "type": "helm",
                            "version": "2.1.0"},
                        {"name": "config", "uid": format!("{uid}2"), "type": "manifest",
                            "version": "1.0.0"}]}},
                "status": {"inUseApps": []},
            }),
        );
        self.write(
            project,
            &format!("{}/tiers", app_profile_route(uid)),
            &json!({"spec": {"appTiers": [
                web,
                {"metadata": {"name": "config", "uid": format!("{uid}2")},
                 "spec": {"type": "manifest", "version": "1.0.0",
                    "manifests": [{"kind": "manifest", "name": "config",
                        "uid": format!("{uid}3")}]}},
            ]}}),
        );
        self.write(
            project,
            &format!("{}/tiers/{uid}2/manifests/{uid}3", app_profile_route(uid)),
            &json!({"metadata": {"name": "config", "uid": format!("{uid}3")},
                "spec": {"published": {
                    "content": "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: shop\ndata:\n  mode: live\n"}}}),
        );
    }

    /// The app deployment `name`, `uid`, of the app profile `profile` at its version now, in the
    /// state `Deployed`. Palette names it in `status.inUseApps` of the app profile.
    pub fn app_deployment(&self, project: &str, uid: &str, name: &str, profile: &str) {
        let path = app_profile_route(profile);
        let mut doc = self.read(project, &path);
        let version = doc["spec"]["version"].clone();
        doc["status"]["inUseApps"]
            .as_array_mut()
            .expect("an app profile")
            .push(json!({"kind": "appdeployment", "uid": uid, "name": name,
                "projectUid": project}));
        self.write(project, &path, &doc);
        self.write(
            project,
            &format!("v1/appDeployments/{uid}"),
            &json!({
                "metadata": {"name": name, "uid": uid,
                    "annotations": {"projectUid": project}},
                "spec": {"profile": {"metadata": {"name": doc["metadata"]["name"],
                    "uid": profile, "version": version}}},
                "status": {"state": "Deployed"},
            }),
        );
    }

    /// The notifications of a cluster, newest first; none for a cluster that has no file yet.
    fn notifications(&self, project: &str, cluster: &str) -> Vec<Value> {
        let path = notifications_route(cluster);
        if !self.doc(project, &path).is_file() {
            return Vec::new();
        }
        self.read(project, &path)["items"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    /// Palette gives `items: null`, not an empty list, for a cluster with no notification.
    fn write_notifications(&self, project: &str, path: &str, items: Vec<Value>) {
        let items = if items.is_empty() {
            Value::Null
        } else {
            Value::Array(items)
        };
        self.write(
            project,
            path,
            &json!({"items": items, "listmeta": {"continue": "", "limit": 50}}),
        );
    }

    /// Add one entry to the audit log of a resource: an update by the person of the examples.
    /// The server leaves the time out of the query, so one file holds the log of the resource,
    /// newest first, as Palette lists it. Each entry is one minute after the one before it. It
    /// gives the time of the entry.
    fn log_write(&self, project: &str, kind: &str, uid: &str, doc: &Value) -> String {
        let path = audit_log_file(kind, uid);
        let mut log = if self.doc(project, &path).is_file() {
            self.read(project, &path)
        } else {
            json!({"items": [], "listmeta": {"continue": "", "limit": 100}})
        };
        let items = log["items"].as_array_mut().expect("an audit log has items");
        let n = items.len();
        let name = doc["metadata"]["name"].as_str().unwrap_or_default();
        let at = format!("2026-09-18T14:{:02}:11.482Z", 2 + n);
        items.insert(
            0,
            json!({
                "metadata": {
                    "uid": format!("64fa{:020x}", n + 1),
                    "creationTimestamp": at,
                },
                "spec": {
                    "actionMsg": format!("John Doe updated {name} Profile"),
                    "actionType": "update",
                    "actor": {
                        "actorType": "user",
                        "user": {
                            "emailId": "john.doe@example.com",
                            "firstName": "John",
                            "lastName": "Doe",
                            "uid": "64fb00000000000000000001",
                        },
                    },
                    "resource": {"kind": kind, "label": "Profile", "name": name, "uid": uid},
                },
            }),
        );
        self.write(project, &path, &log);
        at
    }

    /// Delete a cluster at the time `at`, as Palette does: the document stays, with the state
    /// `Deleted`, `metadata.isDeleted`, `at` in `metadata.deletionTimestamp`, and the annotation
    /// `deleted`.
    ///
    /// The listing and the search drop it.
    pub fn delete_cluster(&self, project: &str, uid: &str, at: &str) {
        let path = cluster_route(uid);
        let mut doc = self.read(project, &path);
        doc["status"]["state"] = json!("Deleted");
        doc["metadata"]["isDeleted"] = json!(true);
        doc["metadata"]["deletionTimestamp"] = json!(at);
        doc["metadata"]["lastModifiedTimestamp"] = json!(at);
        doc["metadata"]["annotations"]["deleted"] = json!("true");
        self.write(project, &path, &doc);
        self.listings();
    }

    /// Take the cluster `cluster` out of the consumer list of the profile `profile`, as Palette
    /// does when it cannot compute the list: the cluster still runs the profile.
    pub fn drop_consumer(&self, project: &str, profile: &str, cluster: &str) {
        let path = profile_route(profile);
        let mut doc = self.read(project, &path);
        for key in ["inUseClusterUids", "inUseClusters"] {
            if let Some(rows) = doc["status"][key].as_array_mut() {
                rows.retain(|r| r != &json!(cluster) && r["uid"] != json!(cluster));
            }
        }
        self.write(project, &path, &doc);
    }

    /// Change one field of the document of a cluster, at a JSON pointer.
    ///
    /// What Palette reports changes, as a change on the cluster does, and its profiles do not. A
    /// pointer that starts with `/cloudconfig`, `/rbacs`, or `/namespaces` changes the cloud
    /// config, the role bindings, or the namespaces of the cluster.
    pub fn cluster_field(&self, project: &str, uid: &str, pointer: &str, value: Value) {
        let (path, pointer) = if let Some(rest) = pointer.strip_prefix("/cloudconfig") {
            (self.cloud_config_path(project, uid), rest.to_string())
        } else if let Some(rest) = pointer.strip_prefix("/rbacs") {
            (rbacs_route(uid), rest.to_string())
        } else if let Some(rest) = pointer.strip_prefix("/namespaces") {
            (namespaces_route(uid), rest.to_string())
        } else {
            (cluster_route(uid), pointer.to_string())
        };
        self.set_field(project, &path, &pointer, value);
        // a label is in the listing and the search too
        self.listings();
    }

    /// Change one field of the document of a cluster group at a JSON pointer, as a person
    /// changes the group: the profiles that it pins stay.
    pub fn cluster_group_field(&self, project: &str, uid: &str, pointer: &str, value: Value) {
        self.set_field(project, &group_route(uid), pointer, value);
    }

    /// Set the field at `pointer` of the document at `path`. An existing field is replaced, and a
    /// new key of an existing object is added.
    fn set_field(&self, project: &str, path: &str, pointer: &str, value: Value) {
        let mut doc = self.read(project, path);
        let (parent, key) = pointer.rsplit_once('/').unwrap_or(("", pointer));
        match doc.pointer_mut(pointer) {
            Some(slot) => *slot = value,
            None => match doc.pointer_mut(parent).and_then(Value::as_object_mut) {
                Some(object) => {
                    object.insert(key.to_string(), value);
                }
                None => panic!("{path} has no object at {parent} for {key}"),
            },
        }
        self.write(project, path, &doc);
    }

    /// A saved tag filter of the tenant, `name`, that selects the clusters with the label
    /// `key=value`.
    pub fn tag_filter(&self, name: &str, uid: &str, key: &str, value: &str) {
        let path = "v1/filters/metadata@filterType=tag";
        let mut items: Vec<Value> = std::fs::read_to_string(self.doc(TENANT, path))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v["items"].as_array().cloned())
            .unwrap_or_default();
        items.retain(|i| i["metadata"]["uid"] != uid);
        items.push(json!({"metadata": {"name": name, "uid": uid}, "spec": {"filterType": "tag"}}));
        self.write(TENANT, path, &json!({"items": items}));
        self.write(
            TENANT,
            &format!("v1/filters/tag/{uid}"),
            &json!({"metadata": {"name": name, "uid": uid},
                "spec": {"filterGroup": {"conjunction": "and", "filters": [
                    {"key": key, "values": [value], "operator": "eq", "negation": false}]}}}),
        );
    }

    /// The path of the cloud config of the cluster `uid`, from the kind that its document names.
    fn cloud_config_path(&self, project: &str, uid: &str) -> String {
        let doc = self.read(project, &cluster_route(uid));
        let kind = doc["spec"]["cloudConfigRef"]["kind"]
            .as_str()
            .unwrap_or("aws");
        cloud_config_route(kind, uid)
    }

    /// Move the cluster `uid` to the cloud type `kind`, with `spec` as the spec of its cloud
    /// config. Palette serves the cloud config of each cloud type at its own path.
    pub fn cloud(&self, project: &str, uid: &str, kind: &str, spec: Value) {
        let path = cluster_route(uid);
        let mut doc = self.read(project, &path);
        doc["spec"]["cloudType"] = json!(kind);
        doc["spec"]["cloudConfigRef"]["kind"] = json!(kind);
        self.write(project, &path, &doc);
        self.write(
            project,
            &cloud_config_route(kind, uid),
            &json!({"metadata": {"uid": format!("{uid}c")}, "spec": spec}),
        );
    }

    /// Create a cluster that runs `profiles` in order, with its own copy of what each profile
    /// holds now, so a later edit of a profile does not reach the cluster.
    ///
    /// Run it again for the same cluster to apply the update that waits for it. `values` gives a
    /// value to each profile variable that it names; each other variable keeps its default.
    pub fn cluster(
        &self,
        project: &str,
        uid: &str,
        name: &str,
        profiles: &[&str],
        values: &[(&str, &str)],
    ) {
        let docs: Vec<Value> = profiles
            .iter()
            .map(|p| self.read(project, &profile_route(p)))
            .collect();
        let templates: Vec<Value> = docs.iter().map(template_of).collect();
        // what Palette reports of a cluster beside its profiles: its own hashes of the spec it
        // applied, the pack version that it reconciled, its policy, its labels, and its cloud
        // config
        let running: Vec<Value> = templates
            .iter()
            .flat_map(|t| {
                let profile = t["uid"].clone();
                t["packs"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(move |p| {
                        let version = if p["version"].is_string() {
                            p["version"].clone()
                        } else {
                            p["tag"].clone()
                        };
                        json!({"name": p["name"], "version": version, "profileUid": profile,
                            "condition": {"type": "Ready", "status": "True"}})
                    })
            })
            .collect();
        let infra: Vec<&Value> = templates
            .iter()
            .filter(|t| t["type"] == "cluster" || t["type"] == "infra")
            .collect();
        let config_uid = format!("{uid}c");
        // Palette gives a cluster that it built the type PureManage. A chapter changes the field
        // for an imported cluster.
        self.write(
            project,
            &cluster_route(uid),
            &json!({
                "apiVersion": "v1", "kind": "SpectroCluster",
                "metadata": {"name": name, "uid": uid,
                    "annotations": {"projectUid": project, "scope": "project"},
                    "labels": {"managed-by": "opentofu"}},
                "spec": {"cloudType": "aws", "clusterType": "PureManage",
                    "clusterProfileTemplates": templates,
                    "cloudConfigRef": {"kind": "aws", "uid": config_uid, "name": format!("{name}-aws-config")},
                    "clusterConfig": {
                        "timezone": "Etc/UTC",
                        "lifecycleConfig": {"pause": false},
                        "machineHealthConfig": {},
                        "machineManagementConfig": {"osPatchConfig":
                            {"patchOnBoot": false, "rebootIfRequired": false}},
                        "clusterResources": {"namespaces": [], "rbacs": []},
                    }},
                "status": {"state": "Running",
                    "spcApply": {"actionType": "DownloadAndInstall",
                        "spcHash": fnv_hex(&json!(templates).to_string()),
                        "spcInfraHash": fnv_hex(&json!(infra).to_string())},
                    "packs": running,
                    "fips": {"mode": "none"}}
            }),
        );
        self.write(
            project,
            &cloud_config_route("aws", uid),
            &json!({
                "metadata": {"uid": config_uid, "name": format!("{name}-aws-config")},
                "spec": {
                    "clusterConfig": {"region": "us-east-2", "sshKeyName": "platform",
                        "vpcId": format!("vpc-{uid}")},
                    "machinePoolConfig": [
                        {"name": "cp-pool", "isControlPlane": true, "instanceType": "t3.xlarge",
                            "size": 1, "azs": ["us-east-2a"], "subnetIds": {"us-east-2a": format!("subnet-{uid}")},
                            "machinePoolProperties": {"archType": "amd64"}},
                        {"name": "worker-pool", "isControlPlane": false, "instanceType": "t3.xlarge",
                            "size": 2, "azs": ["us-east-2a"], "subnetIds": {"us-east-2a": format!("subnet-{uid}")},
                            "machinePoolProperties": {"archType": "amd64"}},
                    ]
                }
            }),
        );

        // the role bindings and the namespaces of the cluster are on routes of their own. The
        // cluster has one binding of a group to a cluster role, and one namespace with a quota.
        let related = json!({"kind": "spectrocluster", "name": name, "uid": uid});
        self.write(
            project,
            &rbacs_route(uid),
            &json!({"items": [{
                "metadata": {"uid": format!("{uid}r"), "name": format!("{name}-rbac"),
                    "creationTimestamp": "2026-09-01T10:00:00.000Z"},
                "spec": {"isSystem": false, "relatedObject": related,
                    "bindings": [{"type": "ClusterRoleBinding", "namespace": "",
                        "role": {"kind": "ClusterRole", "name": "view"},
                        "subjects": [{"type": "Group", "name": "platform", "namespace": ""}]}]},
                "status": {}}]}),
        );
        self.write(
            project,
            &namespaces_route(uid),
            &json!({"items": [{
                "metadata": {"uid": format!("{uid}n"), "name": "apps",
                    "creationTimestamp": "2026-09-01T10:00:00.000Z"},
                "spec": {"isRegex": false, "relatedObject": related,
                    "resourceAllocation": {"cpuCores": 2, "memoryMiB": 4096}},
                "status": {}}]}),
        );

        let manifests: Vec<Value> = profiles
            .iter()
            .map(|p| {
                let d = self.read(project, &manifests_route(p));
                json!({"metadata": d["metadata"], "spec": d["spec"]})
            })
            .collect();
        self.write(
            project,
            &format!("{}/profiles/packs/manifests", cluster_route(uid)),
            &json!({"profiles": manifests}),
        );

        let chosen: Map<String, Value> = values
            .iter()
            .map(|(k, v)| ((*k).to_string(), json!(v)))
            .collect();
        let variables: Vec<Value> = profiles
            .iter()
            .map(|p| {
                let d = self.read(project, &variables_route(p));
                let rows: Vec<Value> = d["variables"]
                    .as_array()
                    .map(|v| v.as_slice())
                    .unwrap_or_default()
                    .iter()
                    .map(|v| {
                        let mut row = v.clone();
                        let name = row["name"].as_str().unwrap_or_default().to_string();
                        row["value"] = chosen
                            .get(&name)
                            .cloned()
                            .unwrap_or_else(|| row["defaultValue"].clone());
                        row
                    })
                    .collect();
                json!({"profileUid": p, "variables": rows})
            })
            .collect();
        self.write(project, &cluster_variables_route(uid), &json!(variables));

        self.resolve_macros(project, uid);
        self.write_profile_updates(project, uid);
        // the cluster now holds what each profile holds, so no update waits for it
        let mut items = self.notifications(project, uid);
        for n in &mut items {
            n["action"]["isDone"] = json!(true);
        }
        self.write_notifications(project, &notifications_route(uid), items);

        // a cluster that moved to other versions is no longer a consumer of the ones it left
        for doc in self.documents_in(project, PROFILES) {
            let Some(p) = doc["metadata"]["uid"].as_str() else {
                continue;
            };
            let listed = doc["status"]["inUseClusterUids"]
                .as_array()
                .is_some_and(|u| u.contains(&json!(uid)));
            if !listed || profiles.contains(&p) {
                continue;
            }
            let mut doc = doc.clone();
            for key in ["inUseClusterUids", "inUseClusters"] {
                if let Some(rows) = doc["status"][key].as_array_mut() {
                    rows.retain(|r| r != &json!(uid) && r["uid"] != json!(uid));
                }
            }
            self.write(project, &profile_route(p), &doc);
        }
        for p in profiles {
            let path = profile_route(p);
            let mut doc = self.read(project, &path);
            let mut uids: Vec<Value> = doc["status"]["inUseClusterUids"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|u| u != uid)
                .collect();
            uids.push(json!(uid));
            doc["status"]["inUseClusterUids"] = json!(uids);
            let mut rows: Vec<Value> = doc["status"]["inUseClusters"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|c| c["uid"] != json!(uid))
                .collect();
            rows.push(json!({"kind": "spectrocluster", "name": name,
                             "projectUid": project, "uid": uid}));
            doc["status"]["inUseClusters"] = json!(rows);
            self.write(project, &path, &doc);
        }
        self.listings();
    }

    /// Change the pack values on a cluster, as a person editing the cluster does. The profile
    /// keeps what it held, so only the cluster's copy differs.
    pub fn on_cluster_reclaim(&self, project: &str, uid: &str, policy: &str) {
        let path = cluster_route(uid);
        let mut doc = self.read(project, &path);
        let from = if policy == "Retain" {
            "Delete"
        } else {
            "Retain"
        };
        if let Some(templates) = doc["spec"]["clusterProfileTemplates"].as_array_mut() {
            for template in templates {
                if let Some(packs) = template["packs"].as_array_mut() {
                    for pack in packs {
                        if pack["name"] == "csi-aws-ebs"
                            && let Some(values) = pack["values"].as_str()
                        {
                            let next = values.replacen(
                                &format!("reclaimPolicy: {from}"),
                                &format!("reclaimPolicy: {policy}"),
                                1,
                            );
                            pack["values"] = json!(next);
                        }
                    }
                }
            }
        }
        self.write(project, &path, &doc);
    }

    /// The answer of `POST /v1/spectroclusters/{uid}/validate/repave` for a cluster: whether the
    /// asked change repaves it, and the reason, as a live run gave it for a kubeadm command. The
    /// stand-in gives one answer for every request; it does not compare anything.
    pub fn repave_answer(&self, project: &str, cluster: &str, required: bool) {
        self.write(project, &repave_route(cluster), &repave_answer_of(required));
    }

    /// Put a cluster in the cluster template `template`, as a cluster that the template launched.
    ///
    /// The cluster document names the template in `spec.clusterTemplate`. On `POST
    /// /v1/spectroclusters/{uid}/validate/repave` for the cluster, Palette then compares nothing,
    /// and answers as for a change that does not repave: `isRepaveRequired: false` and no reason.
    pub fn cluster_in_template(&self, project: &str, cluster: &str, template: &str) {
        self.cluster_field(
            project,
            cluster,
            "/spec/clusterTemplate",
            json!({"uid": template, "projectUid": project}),
        );
        self.write(project, &repave_route(cluster), &repave_answer_of(false));
    }

    /// The answer of `POST /v1/spectroclusters/clusterTemplates/{uid}/validate/repave` for a
    /// cluster template: whether the asked change of its profiles repaves its clusters. Palette
    /// gives the answer of the cluster route, for one cluster of the template. The stand-in gives
    /// one answer for every request, with the reason of [`Tenant::repave_answer`].
    pub fn template_repave_answer(&self, project: &str, template: &str, required: bool) {
        self.write(
            project,
            &format!("{CLUSTERS}/clusterTemplates/{template}/validate/repave"),
            &repave_answer_of(required),
        );
    }

    /// Make Palette refuse `GET path` in `project` with HTTP 403, as it does for a user whose role
    /// does not give the permission that the route needs. The body is in the form of the bodies of
    /// Palette, with the code `AccessDenied`.
    pub fn forbid(&self, project: &str, path: &str) {
        let file = self.doc(project, path);
        if file.is_file() {
            std::fs::remove_file(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        }
        let refused = crate::palette::forbidden(&file);
        let body = json!({
            "code": "AccessDenied",
            "message": "cluster.get operation is not permitted, required permissions cluster.get",
        });
        std::fs::write(&refused, body.to_string() + "\n")
            .unwrap_or_else(|e| panic!("{}: {e}", refused.display()));
    }

    /// Hold a repave of a cluster for approval, as Palette does at the next read of the agent
    /// after a macro in `kubeadmconfig.postKubeadmCommands` gets a new value.
    ///
    /// The cluster document says `status.repave.state: Pending`. `GET
    /// /v1/spectroclusters/{uid}/repave/status` gives the source `hubble` and one reason, with the
    /// resolved values on each side. `since` is the time of the state.
    pub fn repave_waits(&self, project: &str, cluster: &str, since: &str) {
        self.cluster_field(
            project,
            cluster,
            "/status/repave",
            json!({"state": "Pending"}),
        );
        let side = |command: &str| {
            json!({
                "name": "kubernetes",
                "values": format!("kubeadmconfig:\n  postKubeadmCommands:\n  - {command}\n"),
            })
        };
        self.write(
            project,
            &format!("{}/repave/status", cluster_route(cluster)),
            &json!({
                "metadata": {"uid": format!("{cluster}r"), "creationTimestamp": since},
                "spec": {
                    "spectroClusterUid": cluster,
                    "source": "hubble",
                    "reasons": [{
                        "code": "PackValuesUpdated",
                        "message": "Pack 'kubernetes' values are updated",
                        "pack": {
                            "current": side("echo one"),
                            "target": side("echo two"),
                            "diffConfigKeys": ["kubeadmconfig.postKubeadmCommands"],
                        },
                    }],
                },
                "status": {"state": "Pending", "repaveTransitionTime": since},
            }),
        );
    }

    /// The answer of `POST /v1/spectroclusters/{uid}/validate/packs` for a cluster. A
    /// [`PackRefusal`] says that a pack of a profile requires another pack at a minimum version,
    /// and the cluster runs an older one. Palette gives the error, with its code and
    /// its message, under the profile, and gives no profile when it accepts. The stand-in gives
    /// one answer for every request; it does not check anything.
    pub fn packs_answer(&self, project: &str, cluster: &str, refusal: Option<PackRefusal>) {
        let profiles: Vec<Value> = refusal
            .map(|r| {
                json!({"uid": r.profile, "packs": {"results": [{
                    "name": r.pack,
                    "displayName": r.pack,
                    "errors": [{
                        "code": "DependencyPackWithMinVersionRequired",
                        "message": format!(
                            "Pack '{}' is required with minimum compatible version '{}'",
                            r.required, r.version
                        ),
                    }],
                }]}})
            })
            .into_iter()
            .collect();
        self.write(
            project,
            &format!("{}/validate/packs", cluster_route(cluster)),
            &json!({ "profiles": profiles }),
        );
    }

    /// The answer of `POST /v1/clusterprofiles/validate/packs`, or of `POST
    /// /v1/clusterprofiles/{uid}/validate/packs` with `profile`. A refusal is `(pack, required,
    /// version)`: `pack` requires `required` at `version` or newer, and the packs of the body do
    /// not hold it. Palette gives the error, with its code and its message, under
    /// `packs.results`, and gives an empty `results` when it accepts. The stand-in gives one
    /// answer for every request; it does not check anything.
    pub fn profile_packs_answer(
        &self,
        project: &str,
        profile: Option<&str>,
        refusal: Option<(&str, &str, &str)>,
    ) {
        let results: Vec<Value> = refusal
            .map(|(pack, required, version)| {
                json!({
                    "name": pack,
                    "displayName": pack,
                    "errors": [{
                        "code": "DependencyPackWithMinVersionRequired",
                        "message": format!(
                            "Pack '{required}' is required with minimum compatible version \
                             '{version}'"
                        ),
                    }],
                })
            })
            .into_iter()
            .collect();
        let path = match profile {
            Some(uid) => format!("{}/validate/packs", profile_route(uid)),
            None => format!("{PROFILES}/validate/packs"),
        };
        self.write(project, &path, &json!({ "packs": { "results": results } }));
    }

    /// The answers of the two routes that check a new profile version before its create, and that
    /// the validate route of the packs does not: `GET /v1/clusterprofiles/validate/name` for
    /// `name` and `version`, and `POST /v1/clusterprofiles/import/validate`. Palette answers the
    /// name route with `204` and no body, and the import route with the profile that it takes. A
    /// refusal is `(status, code, message)`, in the body that Palette gives. The stand-in gives
    /// one answer for every request; it does not check anything.
    pub fn profile_create_checks(
        &self,
        project: &str,
        (name, version): (&str, &str),
        name_refusal: Option<(u16, &str, &str)>,
        import_refusal: Option<(u16, &str, &str)>,
    ) {
        let name_route = format!("{PROFILES}/validate/name@name={name}&version={version}");
        let import_route = format!("{PROFILES}/import/validate");
        let taken = json!({"metadata": {"name": name}, "spec": {"version": version}});
        for (path, refusal, body) in [
            (name_route.as_str(), name_refusal, None),
            (import_route.as_str(), import_refusal, Some(taken)),
        ] {
            let file = self.doc(project, path);
            for status in std::iter::once(None).chain(crate::palette::REFUSALS.map(Some)) {
                let old = status.map_or(file.clone(), |s| crate::palette::refused(&file, s));
                if old.is_file() {
                    std::fs::remove_file(&old).unwrap_or_else(|e| panic!("{}: {e}", old.display()));
                }
            }
            let (target, text) = match refusal {
                Some((status, code, message)) => (
                    crate::palette::refused(&file, status),
                    json!({"code": code, "message": message}).to_string() + "\n",
                ),
                None => (file, body.map_or(String::new(), |b| b.to_string() + "\n")),
            };
            std::fs::write(&target, text).unwrap_or_else(|e| panic!("{}: {e}", target.display()));
        }
    }

    /// Give a project macro a value, as `PATCH /v1/projects/{uid}/macros` does, and let each
    /// cluster, profile version, and cluster group of the project resolve it again: Palette
    /// answers `packs/resolvedValues` with the value the macro has now.
    pub fn set_macro(&self, project: &str, name: &str, value: &str) {
        let path = macros_route(project);
        let mut doc = if self.doc(project, &path).is_file() {
            self.read(project, &path)
        } else {
            json!({"macros": []})
        };
        let macros = doc["macros"].as_array_mut().expect("macros is a list");
        macros.retain(|m| m["name"] != name);
        macros.push(json!({"name": name, "value": value}));
        self.write(project, &path, &doc);
        for uid in self.uids_in(project, CLUSTERS) {
            self.resolve_macros(project, &uid);
        }
        for uid in self.uids_in(project, PROFILES) {
            self.resolve_profile_macros(project, &uid);
        }
        for uid in self.uids_in(project, GROUPS) {
            self.resolve_group_macros(project, &uid);
        }
    }

    /// The uid of each document directly inside one directory of a project.
    fn uids_in(&self, project: &str, path: &str) -> Vec<String> {
        self.documents_in(project, path)
            .iter()
            .filter_map(|d| d["metadata"]["uid"].as_str().map(str::to_string))
            .collect()
    }

    /// The user macros of `project`, by placeholder: each `{{.spectro.macro.<name>}}` and its
    /// value.
    fn user_macros(&self, project: &str) -> Map<String, Value> {
        let macros_path = macros_route(project);
        let macros = if self.doc(project, &macros_path).is_file() {
            self.read(project, &macros_path)["macros"].clone()
        } else {
            json!([])
        };
        let mut resolved = Map::new();
        for m in macros.as_array().into_iter().flatten() {
            let name = m["name"].as_str().unwrap_or_default();
            resolved.insert(format!("{{{{.spectro.macro.{name}}}}}"), m["value"].clone());
        }
        resolved
    }

    /// The `packs/resolvedValues` answer of one profile version, with no cluster: one `resolved`
    /// map with each project macro and the system macro of the profile name. The system macro of
    /// the cluster name has an empty value, as Palette gives it for a profile.
    fn resolve_profile_macros(&self, project: &str, profile: &str) {
        let doc = self.read(project, &profile_route(profile));
        let mut resolved = self.user_macros(project);
        resolved.insert(
            "{{.spectro.system.clusterprofile.name}}".to_string(),
            doc["metadata"]["name"].clone(),
        );
        resolved.insert("{{.spectro.system.cluster.name}}".to_string(), json!(""));
        self.write(
            project,
            &format!("{}/packs/resolvedValues", profile_route(profile)),
            &json!({ "resolved": resolved }),
        );
    }

    /// The `packs/resolvedValues` answer of one cluster group: for each profile it pins, each
    /// project macro, in the shape of the answer for a cluster.
    fn resolve_group_macros(&self, project: &str, group: &str) {
        let doc = self.read(project, &group_route(group));
        let resolved = self.user_macros(project);
        let profiles: Vec<Value> = doc["spec"]["clusterProfileTemplates"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|t| json!({"uid": t["uid"], "resolved": resolved}))
            .collect();
        self.write(
            project,
            &format!("{}/packs/resolvedValues", group_route(group)),
            &json!({ "profiles": profiles }),
        );
    }

    /// The `packs/resolvedValues` answer of one cluster: for each profile it runs, each project
    /// macro and its value, and the system macro of the cluster name, as Palette gives them.
    fn resolve_macros(&self, project: &str, cluster: &str) {
        let doc = self.read(project, &cluster_route(cluster));
        let mut resolved = self.user_macros(project);
        resolved.insert(
            "{{.spectro.system.cluster.name}}".to_string(),
            doc["metadata"]["name"].clone(),
        );
        let profiles: Vec<Value> = doc["spec"]["clusterProfileTemplates"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|t| json!({"uid": t["uid"], "resolved": resolved}))
            .collect();
        self.write(
            project,
            &format!("{}/packs/resolvedValues", cluster_route(cluster)),
            &json!({"profiles": profiles}),
        );
    }

    /// Give a cluster variable a new value, as a person changing it in Palette does.
    pub fn set_value(&self, project: &str, uid: &str, variable: &str, value: &str) {
        let path = cluster_variables_route(uid);
        let mut doc = self.read(project, &path);
        if let Some(profiles) = doc.as_array_mut() {
            for profile in profiles {
                if let Some(rows) = profile["variables"].as_array_mut() {
                    for row in rows {
                        if row["name"] == variable {
                            row["value"] = json!(value);
                        }
                    }
                }
            }
        }
        self.write(project, &path, &doc);
    }

    /// The Public Repo registry and the versions of each pack in `examples/packs/`, as the
    /// registry lookups of a candidate read them.
    pub fn registries(&self, project: &str) {
        self.write(
            project,
            REGISTRY_LISTING,
            &json!({"items": [
                {"name": "Public Repo", "uid": PUBLIC_REPO, "kind": "pack"},
                {"name": "Palette Registry", "uid": PALETTE_REGISTRY, "kind": "oci"}]}),
        );
        // the last sync of each registry, as v1RegistrySyncStatus gives it in the summary listing
        // of its kind. The sync of the OCI registry stopped before its end and recovers, as the
        // Bitnami registry of the tenant gives it: its copy is the copy of the last complete sync
        let synced = json!({"isSyncSupported": true, "status": "Completed",
            "lastRunTime": "2026-09-17T09:00:00.000Z", "lastSyncedTime": "2026-09-17T09:00:00.000Z",
            "syncRecovery": {"syncRecoveryState": "Recovered"}});
        let stopped = json!({"isSyncSupported": true, "status": "InProgress",
            "lastRunTime": "2026-09-17T09:00:00.000Z", "lastSyncedTime": "2026-08-29T00:00:11.032Z",
            "syncRecovery": {"syncRecoveryState": "Recovering", "message": "sync interrupted"}});
        for (kind, uid, sync) in [
            ("pack", PUBLIC_REPO, &synced),
            ("oci", PALETTE_REGISTRY, &stopped),
        ] {
            self.write(
                project,
                &format!("v1/registries/{kind}/summary"),
                &json!({"items": [{"metadata": {"uid": uid}, "status": {"sync": sync}}]}),
            );
        }
        let packs = self.repo.join("examples/packs");
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&packs)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        for dir in dirs {
            let name = dir.file_name().unwrap().to_string_lossy().into_owned();
            let summaries: Value = read_json(&dir.join("_summaries.json"));
            let count = summaries.as_array().map(Vec::len).unwrap_or(0);
            let mut listing = json!({"items": summaries, "listmeta": {"count": count}});
            let entity: Value = read_json(&dir.join("_entity.json"));
            self.write(project, &pack_entity_route(&name, PUBLIC_REPO), &entity);
            // The answer with the states query: the tags of the retired versions, which the
            // answer above does not include. A pack with no disabled or deleted version has no file.
            let retired = dir.join("_entity_retired.json");
            if retired.exists() {
                self.write(
                    project,
                    &format!(
                        "{}@states=deprecated,disabled,deleted",
                        pack_entity_route(&name, PUBLIC_REPO)
                    ),
                    &read_json(&retired),
                );
            }
            // The CSI pack is in the OCI registry too, as the live run of 20260917T150025Z found.
            // A pack ref that names no registry then has to say so.
            if name == "csi-aws-ebs" {
                add_csi_to_oci(&mut listing);
                self.write(
                    project,
                    &pack_entity_route(&name, PALETTE_REGISTRY),
                    &csi_entity_in_oci(&entity),
                );
            }
            self.write(project, &pack_listing(&name), &listing);
        }
    }
}

impl Tenant {
    /// Take one version of a pack out of the listing of the Public Repo, as before the registry
    /// published it. `registries` puts it back.
    pub fn hide_pack_version(&self, project: &str, name: &str, version: &str) {
        let path = pack_listing(name);
        let mut listing = self.read(project, &path);
        if let Some(items) = listing["items"].as_array_mut() {
            items.retain(|i| {
                !(i["spec"]["version"] == version && i["spec"]["registryUid"] == PUBLIC_REPO)
            });
            let count = items.len();
            listing["listmeta"]["count"] = json!(count);
        }
        self.write(project, &path, &listing);
    }

    /// Publish a rebuild of one version of a pack in the Public Repo as `<version>-rev1`, as
    /// Spectro Cloud publishes `1.19.3-rev1` of cert-manager. Palette orders the versions by
    /// semantic version, so the rebuild sorts below its base: the mark `latestVersion` and the
    /// floating tags stay on the base, and the rebuild gets only its exact tag.
    pub fn rebuild_pack_version(&self, project: &str, name: &str, version: &str) {
        let rebuild = format!("{version}-rev1");
        let path = pack_listing(name);
        let mut listing = self.read(project, &path);
        let Some(items) = listing["items"].as_array_mut() else {
            return;
        };
        let Some(mut copy) = items
            .iter()
            .find(|i| i["spec"]["version"] == version && i["spec"]["registryUid"] == PUBLIC_REPO)
            .cloned()
        else {
            return;
        };
        // a pack uid of Palette is 24 hex digits: the rebuild changes the last one
        let base = copy["metadata"]["uid"].as_str().unwrap_or_default();
        let uid = format!("{}f", &base[..base.len().saturating_sub(1)]);
        copy["metadata"]["uid"] = json!(uid);
        copy["metadata"]["annotations"]["latestVersion"] = json!("false");
        copy["spec"]["version"] = json!(rebuild);
        items.push(copy);
        let count = items.len();
        listing["listmeta"]["count"] = json!(count);
        self.write(project, &path, &listing);

        let path = pack_entity_route(name, PUBLIC_REPO);
        let mut entity = self.read(project, &path);
        let above = entity["tags"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|t| t["tag"] == version)
            .map(|t| t["parentTags"].clone())
            .unwrap_or(Value::Null);
        if let Some(tags) = entity["tags"].as_array_mut() {
            tags.push(json!({"packUid": uid, "parentTags": above, "tag": rebuild,
                             "version": rebuild}));
        }
        self.write(project, &path, &entity);
    }

    /// Give one version of a pack in the listing of the Public Repo the architectures
    /// `architectures`, as the annotation `architecture` of the listing names them: `amd64`, or
    /// `amd64,arm64`. The Public Repo names them on most versions, and a newer version can name
    /// fewer than an older one, as `loki` 3.6.7 names `amd64` below 2.10.2 with `amd64,arm64`.
    pub fn pack_architectures(
        &self,
        project: &str,
        name: &str,
        version: &str,
        architectures: &str,
    ) {
        let path = pack_listing(name);
        let mut listing = self.read(project, &path);
        for item in listing["items"].as_array_mut().into_iter().flatten() {
            if item["spec"]["version"] == version && item["spec"]["registryUid"] == PUBLIC_REPO {
                item["spec"]["annotations"]["architecture"] = json!(architectures);
            }
        }
        self.write(project, &path, &listing);
    }

    /// Give one version of a pack in the listing of the Public Repo a template, as
    /// `spec.template` of `v1PackSummarySpec`. Palette gives each option of an input parameter
    /// with its label and dependencies, and with no presets.
    pub fn pack_template(&self, project: &str, name: &str, version: &str, template: &Value) {
        let path = pack_listing(name);
        let mut listing = self.read(project, &path);
        for item in listing["items"].as_array_mut().into_iter().flatten() {
            if item["spec"]["version"] == version && item["spec"]["registryUid"] == PUBLIC_REPO {
                item["spec"]["template"] = template.clone();
            }
        }
        self.write(project, &path, &listing);
    }
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The row of a profile version in the profile listing. The listing gives the cloud type also
/// for a version that nobody published: the cloud type of its draft.
fn profile_row(doc: &Value) -> Value {
    let cloud = ["published", "draft"]
        .iter()
        .map(|t| &doc["spec"][t]["cloudType"])
        .find(|c| !c.is_null())
        .cloned()
        .unwrap_or(Value::Null);
    json!({
        "metadata": {"name": doc["metadata"]["name"], "uid": doc["metadata"]["uid"]},
        "spec": {"version": doc["spec"]["version"], "cloudType": cloud}
    })
}

/// The answer of the profile search, `POST /v1/dashboard/clusterprofiles`, for `profiles`,
/// before the filter of a request. It has one item for each profile name of each scope. An item
/// gives the scope in `metadata.annotations.scope`, and the uid of each version in
/// `specSummary.versions[]`. The server keeps the items of the scope that a request names.
fn profile_search(profiles: &[Value]) -> Value {
    let mut by_name: BTreeMap<(String, String), Vec<&Value>> = BTreeMap::new();
    for doc in profiles {
        let name = doc["metadata"]["name"].as_str().unwrap_or("").to_string();
        let scope = doc["metadata"]["annotations"]["scope"]
            .as_str()
            .unwrap_or("project")
            .to_string();
        by_name.entry((name, scope)).or_default().push(doc);
    }
    let items: Vec<Value> = by_name
        .iter()
        .map(|((name, scope), docs)| {
            let versions: Vec<Value> = docs
                .iter()
                .map(|d| json!({"version": d["spec"]["version"], "uid": d["metadata"]["uid"]}))
                .collect();
            let latest: &Value = docs.last().copied().unwrap_or(&Value::Null);
            json!({
                "metadata": {"name": name, "uid": latest["metadata"]["uid"],
                             "annotations": {"scope": scope}},
                "specSummary": {"version": latest["spec"]["version"], "versions": versions}
            })
        })
        .collect();
    json!({
        "items": items,
        "listmeta": {"continue": "", "count": items.len(), "limit": 50, "offset": 0}
    })
}

/// Copy each file under `from` to the same path under `to`.
fn copy_dir(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).into_iter().flatten().flatten() {
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            std::fs::create_dir_all(to).unwrap_or_else(|e| panic!("{}: {e}", to.display()));
            std::fs::copy(&path, &target)
                .unwrap_or_else(|e| panic!("{} -> {}: {e}", path.display(), target.display()));
        }
    }
}

fn cluster_row(doc: &Value) -> Value {
    json!({
        "metadata": {"name": doc["metadata"]["name"], "uid": doc["metadata"]["uid"],
                     "annotations": doc["metadata"]["annotations"],
                     "labels": doc["metadata"]["labels"]},
        "spec": {"cloudType": doc["spec"]["cloudType"], "clusterType": doc["spec"]["clusterType"]},
        "status": {"state": doc["status"]["state"]}
    })
}

/// True for a virtual cluster: the cloud type `nested`.
fn nested(cluster: &Value) -> bool {
    cluster["spec"]["cloudType"] == "nested"
}

/// A cluster as the search gives it: no spec, and a summary with the cloud, the project, and the
/// uid, name, and type of each profile. A virtual cluster has `status.virtual`, `virtual_summary`:
/// its cluster group and its host cluster.
fn search_row(doc: &Value, project_name: &str, virtual_summary: Option<&Value>) -> Value {
    let profiles: Vec<Value> = doc["spec"]["clusterProfileTemplates"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| json!({"uid": t["uid"], "name": t["name"], "type": t["type"]}))
        .collect();
    let mut status = json!({"state": doc["status"]["state"]});
    if let Some(v) = virtual_summary {
        status["virtual"] = v.clone();
    }
    json!({
        "metadata": {"name": doc["metadata"]["name"], "uid": doc["metadata"]["uid"],
                     "annotations": doc["metadata"]["annotations"],
                     "labels": doc["metadata"]["labels"]},
        "specSummary": {"cloudConfig": {"cloudType": doc["spec"]["cloudType"]},
                        "clusterType": doc["spec"]["clusterType"],
                        "clusterProfileTemplates": profiles,
                        "projectMeta": {"name": project_name,
                                        "uid": doc["metadata"]["annotations"]["projectUid"]}},
        "status": status
    })
}

/// A version is newest by number, and not by text: 1.9.0 is older than 1.10.0.
fn version_key(version: &str) -> Vec<u64> {
    version
        .split('.')
        .map(|p| p.parse::<u64>().unwrap_or(0))
        .collect()
}

/// The newest csi-aws-ebs version, copied into the OCI registry with its own uid and digest.
fn add_csi_to_oci(listing: &mut Value) {
    let items = listing["items"].as_array().cloned().unwrap_or_default();
    let newest = items
        .iter()
        .filter(|i| !i["spec"]["values"].is_null())
        .max_by_key(|i| version_key(i["spec"]["version"].as_str().unwrap_or_default()));
    let Some(newest) = newest else { return };
    let mut copy = newest.clone();
    copy["metadata"]["uid"] = json!(OCI_CSI_PACK);
    copy["spec"]["registryUid"] = json!(PALETTE_REGISTRY);
    copy["spec"]["digest"] = json!(OCI_CSI_DIGEST);
    copy["spec"]["type"] = json!("oci");
    listing["items"]
        .as_array_mut()
        .expect("an array")
        .push(copy);
}

/// The OCI copy holds the newest version, under its exact tag. A tag row carries its version, as
/// run 20260918T020941Z found, and the newest is newest by number, not by text.
fn csi_entity_in_oci(entity: &Value) -> Value {
    let tags = entity["tags"].as_array().cloned().unwrap_or_default();
    let newest = tags
        .iter()
        .filter(|t| {
            let tag = t["tag"].as_str().unwrap_or_default();
            tag.split('.').count() == 3 && tag.split('.').all(|p| p.parse::<u64>().is_ok())
        })
        .max_by_key(|t| version_key(t["version"].as_str().unwrap_or_default()));
    let mut out = entity.clone();
    out["tags"] = match newest {
        Some(t) => json!([{"tag": t["tag"], "version": t["version"], "packUid": OCI_CSI_PACK}]),
        None => json!([]),
    };
    out
}

/// A 64-bit FNV-1a hash as hex: a stand-in for the hashes Palette computes of a cluster, and the
/// same on every machine and every Rust version.
fn fnv_hex(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// An answer of a `validate/repave` route: a repave for a kubeadm command when `required`, as a
/// live run gave it, or no repave and no reason.
fn repave_answer_of(required: bool) -> Value {
    let reasons = if required {
        json!([{
            "code": "PackValuesUpdated",
            "message": "Pack 'kubernetes' values are updated",
            "pack": {
                "current": {"name": "kubernetes"},
                "diffConfigKeys": ["kubeadmconfig.postKubeadmCommands"],
            },
        }])
    } else {
        json!([])
    };
    json!({"isRepaveRequired": required, "reasons": reasons})
}

/// The template of a profile on a cluster, from the profile document: the published profile.
impl Tenant {
    /// The templates of an object that holds profiles and is not a cluster: an edge host or a
    /// cluster group.
    ///
    /// Palette gives the holder its own copy of each manifest, under a uid of its own. The
    /// template names the copy by that uid, with no content. `route(profile, pack, copy)` is
    /// where the API serves a copy.
    fn holder_templates(
        &self,
        project: &str,
        holder: &str,
        profiles: &[&str],
        route: impl Fn(&str, &str, &str) -> String,
    ) -> Vec<Value> {
        let mut n = 0;
        profiles
            .iter()
            .map(|p| {
                let mut t = template_of(&self.read(project, &profile_route(p)));
                let contents = self.read(project, &manifests_route(p));
                for pack in t["packs"].as_array_mut().into_iter().flatten() {
                    let pname = pack["name"].as_str().unwrap_or_default().to_string();
                    let source: Vec<Value> = contents["spec"]["packs"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|q| q["spec"]["name"] == pname.as_str())
                        .flat_map(|q| q["spec"]["manifests"].as_array().cloned().unwrap_or_default())
                        .collect();
                    if source.is_empty() {
                        continue;
                    }
                    let mut refs = Vec::new();
                    for m in source {
                        n += 1;
                        let copy = format!("{}{n:04x}", &holder[..holder.len().min(20)]);
                        self.write(
                            project,
                            &route(p, &pname, &copy),
                            &json!({"metadata": {"uid": copy, "name": m["name"]},
                                "spec": {"published": {"content": m["content"], "digest": m["digest"]}}}),
                        );
                        refs.push(json!({"uid": copy, "name": m["name"]}));
                    }
                    pack["manifests"] = json!(refs);
                }
                t
            })
            .collect()
    }
}

fn template_of(d: &Value) -> Value {
    json!({
        "uid": d["metadata"]["uid"], "name": d["metadata"]["name"],
        "profileVersion": d["spec"]["version"],
        "type": d["spec"]["published"]["type"],
        "cloudType": d["spec"]["published"]["cloudType"],
        "packs": d["spec"]["published"]["packs"],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_is_newest_by_number_and_not_by_text() {
        let mut got = vec!["1.9.0", "1.10.0", "1.63.1"];
        got.sort_by_key(|v| version_key(v));
        assert_eq!(got, ["1.9.0", "1.10.0", "1.63.1"]);
    }

    #[test]
    fn the_reclaim_edit_changes_the_csi_pack_alone() {
        let mut doc = json!({"spec": {"published": {"packs": [
            {"name": "csi-aws-ebs", "values": "a\nreclaimPolicy: Delete\nb"},
            {"name": "kubernetes", "values": "reclaimPolicy: Delete"}]}}});
        Edit::Reclaim("Retain").apply(&mut doc);
        let packs = doc["spec"]["published"]["packs"].as_array().unwrap();
        assert_eq!(packs[0]["values"], "a\nreclaimPolicy: Retain\nb");
        assert_eq!(packs[1]["values"], "reclaimPolicy: Delete");
    }

    #[test]
    fn the_oci_edit_moves_the_csi_pack_to_the_other_registry() {
        let mut doc = json!({"spec": {"published": {"packs": [{"name": "csi-aws-ebs"}]}}});
        Edit::OciCsi.apply(&mut doc);
        let pack = &doc["spec"]["published"]["packs"][0];
        assert_eq!(pack["type"], "oci");
        assert_eq!(pack["registryUid"], PALETTE_REGISTRY);
        assert_eq!(pack["packUid"], OCI_CSI_PACK);
    }

    /// The edits compose in order, as the chapter that writes `OCI_CSI | RETAIN` needs.
    #[test]
    fn the_edits_apply_in_order() {
        let mut doc = json!({"spec": {"version": "1.1.1", "published": {"packs": [
            {"name": "csi-aws-ebs", "values": "reclaimPolicy: Delete"}]}}});
        for edit in [
            Edit::OciCsi,
            Edit::Reclaim("Retain"),
            Edit::Version("1.1.2"),
        ] {
            edit.apply(&mut doc);
        }
        assert_eq!(doc["spec"]["version"], "1.1.2");
        let pack = &doc["spec"]["published"]["packs"][0];
        assert_eq!(pack["type"], "oci");
        assert_eq!(pack["values"], "reclaimPolicy: Retain");
    }
}
