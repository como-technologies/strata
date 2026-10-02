//! The tenant that the stand-in for Palette serves.
//!
//! Each chapter of the book builds the tenant that one page needs, from the real pack data in
//! `examples/`, and changes it between two commands in the way that Palette changes: an edit in
//! place, a version bump, a cluster that takes an update.
//!
//! A document here answers one request. `Tenant::doc` gives the file that answers a path, and
//! every helper writes one or more of those files.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

/// The uid of the Public Repo pack registry, as Palette gives it.
pub const PUBLIC_REPO: &str = "5eecc89d0b150045ae661cef";
/// The uid of the OCI pack registry that the live run of 20260917T150025Z found.
pub const PALETTE_REGISTRY: &str = "64ec000000000000000000a1";
const OCI_CSI_PACK: &str = "64ec000000000000000000b1";
const OCI_CSI_DIGEST: &str =
    "sha256:a746e14689085b14531566f9c9cfb2bc7716e96a394547ef50a1c52eab510c9e";

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
            self.write(
                pair.1,
                "v1/dashboard/projects/metadata",
                &json!({"items": [item]}),
            );
        }
        self.write(
            "tenant",
            "v1/dashboard/projects/metadata",
            &json!({"items": items}),
        );
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
            .filter(|name| name != "tenant")
            .collect();
        scopes.sort();
        for project in &scopes {
            let profiles = self.documents_in(project, "v1/clusterprofiles");
            // a deleted cluster leaves the listing and the search, and keeps its document
            let clusters: Vec<Value> = self
                .documents_in(project, "v1/spectroclusters")
                .into_iter()
                .filter(|d| d["status"]["state"] != "Deleted")
                .collect();
            every_cluster.extend(clusters.iter().cloned());
            let items: Vec<Value> = profiles.iter().map(profile_row).collect();
            self.write(
                project,
                "v1/dashboard/clusterprofiles/metadata",
                &json!({"items": items}),
            );
            let rows: Vec<Value> = clusters
                .iter()
                .filter(|c| !nested(c))
                .map(cluster_row)
                .collect();
            self.write(project, "v1/dashboard/spectroclusters/meta", &json!(rows));
        }
        self.write(
            "tenant",
            "v1/dashboard/clusterprofiles/metadata",
            &json!({"items": []}),
        );
        // the listing leaves out each virtual cluster; the search holds it, when a condition asks
        let rows: Vec<Value> = every_cluster
            .iter()
            .filter(|c| !nested(c))
            .map(cluster_row)
            .collect();
        self.write("tenant", "v1/dashboard/spectroclusters/meta", &json!(rows));
        // the search of the tenant: every cluster, which the server filters by the body of a
        // request, and the schema that says the search takes tags
        let projects: Value =
            std::fs::read_to_string(self.doc("tenant", "v1/dashboard/projects/metadata"))
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
        // search by profile does not find a virtual cluster through its group.
        let mut group_pins: Map<String, Value> = Map::new();
        for project in &scopes {
            for group in self.documents_in(project, "v1/clustergroups") {
                let gid = group["metadata"]["uid"].as_str().unwrap_or_default();
                let pinned: Vec<Value> = group["spec"]["clusterProfileTemplates"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|t| t["uid"].clone())
                    .collect();
                let members = self.read(
                    project,
                    &format!("v1/dashboard/clustergroups/{gid}/virtualClusters@limit=50"),
                );
                for m in members["items"].as_array().into_iter().flatten() {
                    let muid = m["metadata"]["uid"].as_str().unwrap_or_default();
                    let entry = group_pins.entry(muid.to_string()).or_insert(json!([]));
                    if let Some(list) = entry.as_array_mut() {
                        list.extend(pinned.iter().cloned());
                    }
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
                let pins = group_pins
                    .get(c["metadata"]["uid"].as_str().unwrap_or_default())
                    .cloned();
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
                )
            })
            .collect();
        self.write(
            "tenant",
            "v1/dashboard/spectroclusters/search@limit=50",
            &json!({"items": found, "listmeta": {"continue": ""}}),
        );
        self.write(
            "tenant",
            "v1/dashboard/spectroclusters/search/schema",
            &json!({"schema": {"properties": [
                {"name": "clusterName", "type": "string"}, {"name": "clusterState", "type": "string"},
                {"name": "tags", "type": "string"}, {"name": "environment", "type": "string"},
                {"name": "clusterProfiles", "type": "string"}]}}),
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
        let from = self.repo.join("examples/profiles/raw").join(example);
        let text = std::fs::read_to_string(from.join("profile.json"))
            .unwrap_or_else(|e| panic!("{}: {e}", from.display()));
        let mut doc: Value = serde_json::from_str(&text).expect("an example profile is JSON");
        doc["metadata"]["uid"] = json!(uid);
        doc["spec"]["version"] = json!(version);
        doc["status"]["inUseClusterUids"] = json!([]);
        doc["status"]["inUseClusters"] = json!([]);
        for edit in edits {
            edit.apply(&mut doc);
        }
        self.write(project, &format!("v1/clusterprofiles/{uid}"), &doc);

        let text = std::fs::read_to_string(from.join("manifests.json")).unwrap_or_default();
        let mut manifests: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({}));
        manifests["metadata"]["uid"] = json!(uid);
        for edit in edits {
            edit.apply_manifests(&mut manifests);
        }
        self.write(
            project,
            &format!("v1/clusterprofiles/{uid}/packs/manifests"),
            &manifests,
        );

        let text = std::fs::read_to_string(from.join("variables.json")).unwrap_or_default();
        let variables: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({}));
        self.write(
            project,
            &format!("v1/clusterprofiles/{uid}/variables"),
            &variables,
        );
        self.listings();
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
            &format!("v1/clusterprofiles/{uid}"),
            &json!({
                "apiVersion": "v1", "kind": "ClusterProfile",
                "metadata": {"name": name, "uid": uid},
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
            &format!("v1/clusterprofiles/{uid}/packs/manifests"),
            &json!({
                "metadata": {"name": name, "uid": uid},
                "spec": {"packs": [{"metadata": {"name": "namespace-policy", "uid": pack},
                    "spec": {"name": "namespace-policy", "layer": "addon", "version": "1.0.0",
                        "manifests": [{"name": "namespace-policy", "content": content}]},
                    "status": {}}]}
            }),
        );
        self.write(
            project,
            &format!("v1/clusterprofiles/{uid}/variables"),
            &json!({"variables": []}),
        );
        self.listings();
    }

    /// Change a published profile in place, as an edit through the API does, and log the write
    /// in the audit log, as Palette does. The document takes the time of the write in
    /// `metadata.lastModifiedTimestamp`.
    pub fn edit(&self, project: &str, uid: &str, edits: &[Edit]) {
        let path = format!("v1/clusterprofiles/{uid}");
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
            let copy: Vec<Value> = self.read(project, &format!("v1/spectroclusters/{cluster}"))
                ["spec"]["clusterProfileTemplates"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|t| t["uid"] == json!(uid))
                .and_then(|t| t["packs"].as_array().cloned())
                .unwrap_or_default();
            self.write_profile_updates(project, &cluster);
            let path = format!("v1/notifications/spectrocluster/{cluster}");
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
        let profiles: Vec<Value> = self.read(project, &format!("v1/spectroclusters/{cluster}"))
            ["spec"]["clusterProfileTemplates"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|t| {
                let uid = t["uid"].as_str().unwrap_or_default();
                template_of(&self.read(project, &format!("v1/clusterprofiles/{uid}")))
            })
            .collect();
        self.write(
            project,
            &format!("v1/spectroclusters/{cluster}/profileUpdates"),
            &json!({ "profiles": profiles }),
        );
    }

    /// An edge host `uid`, `name`, with `profiles` bound to it: its own copy of each profile as
    /// the profile is now. A live run gives the rules: the host keeps its copy of the values,
    /// and holds each manifest by uid alone, so strata reads the manifests of the profile.
    pub fn edge_host(&self, project: &str, uid: &str, name: &str, profiles: &[&str]) {
        let templates = self.holder_templates(project, uid, profiles, |_, copy| {
            format!("v1/edgehosts/{uid}/pack/manifests/{copy}")
        });
        self.write(
            project,
            &format!("v1/edgehosts/{uid}"),
            &json!({
                "metadata": {"uid": uid, "name": name,
                    "annotations": {"projectUid": project, "scope": "project"}},
                "spec": {"clusterProfileTemplates": templates},
                "status": {"state": "ready"},
            }),
        );
        // the search gives a summary of each template: uid, name, and version
        let items: Vec<Value> = self
            .documents_in(project, "v1/edgehosts")
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
        self.write(project, "v1/dashboard/edgehosts/search@limit=50", &search);
        self.write("tenant", "v1/dashboard/edgehosts/search@limit=50", &search);
    }

    /// A cluster template `uid`, `name`, that links `profiles`. Palette names each template that
    /// links a version in `status.inUseClusterTemplates` of the profile document, with the fields
    /// of `v1ProjectResourceReference`, and refuses the delete of the version while one does.
    pub fn cluster_template(&self, project: &str, uid: &str, name: &str, profiles: &[&str]) {
        for p in profiles {
            let path = format!("v1/clusterprofiles/{p}");
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
    /// clusters `members` as those virtual clusters.
    pub fn cluster_group(
        &self,
        project: &str,
        uid: &str,
        name: &str,
        profiles: &[&str],
        members: &[&str],
    ) {
        let templates = self.holder_templates(project, uid, profiles, |pack, copy| {
            format!("v1/clusterprofiles/{{profile}}/packs/{pack}/manifests/{copy}")
        });
        self.write(
            project,
            &format!("v1/clustergroups/{uid}"),
            &json!({
                "metadata": {"uid": uid, "name": name},
                "spec": {"type": "hostCluster", "clusterProfileTemplates": templates},
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
                let doc = self.read(project, &format!("v1/spectroclusters/{m}"));
                json!({"metadata": {"uid": m, "name": doc["metadata"]["name"],
                    "annotations": {"projectUid": project}}})
            })
            .collect();
        self.write(
            project,
            &format!("v1/dashboard/clustergroups/{uid}/virtualClusters@limit=50"),
            &json!({"items": items, "listmeta": {"continue": ""}}),
        );
    }

    /// The app profile `shop@<version>`, `uid`: a helm tier `web` with the property `replicas`,
    /// and a manifest tier `config`. A second call for the same uid is a plain update, as
    /// Palette takes one: the uid and the version stay.
    pub fn app_profile(&self, project: &str, uid: &str, version: &str, replicas: &str) {
        self.write(
            project,
            "v1/dashboard/appProfiles/metadata",
            &json!({"appProfiles": [{"metadata": {"name": "shop", "uid": uid},
                "spec": {"version": version}}]}),
        );
        self.write(
            project,
            &format!("v1/appProfiles/{uid}"),
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
            &format!("v1/appProfiles/{uid}/tiers"),
            &json!({"spec": {"appTiers": [
                {"metadata": {"name": "web", "uid": format!("{uid}1")},
                 "spec": {"type": "helm", "version": "2.1.0", "registryUid": PUBLIC_REPO,
                    "sourceAppTierUid": "64ed000000000000000000a1",
                    "values": "replicaCount: 2\nimage:\n  tag: 2.1.0\n",
                    "properties": [{"name": "replicas", "type": "number", "value": replicas}]}},
                {"metadata": {"name": "config", "uid": format!("{uid}2")},
                 "spec": {"type": "manifest", "version": "1.0.0",
                    "manifests": [{"kind": "manifest", "name": "config",
                        "uid": format!("{uid}3")}]}},
            ]}}),
        );
        self.write(
            project,
            &format!("v1/appProfiles/{uid}/tiers/{uid}2/manifests/{uid}3"),
            &json!({"metadata": {"name": "config", "uid": format!("{uid}3")},
                "spec": {"published": {
                    "content": "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: shop\ndata:\n  mode: live\n"}}}),
        );
    }

    /// The app deployment `name`, `uid`, of the app profile `profile` at its version now, in the
    /// state `Deployed`. Palette names it in `status.inUseApps` of the app profile.
    pub fn app_deployment(&self, project: &str, uid: &str, name: &str, profile: &str) {
        let path = format!("v1/appProfiles/{profile}");
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
        let path = format!("v1/notifications/spectrocluster/{cluster}");
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
        let path = format!("v1/audits@resourceKind={kind}&resourceUid={uid}&limit=100");
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

    /// Delete a cluster as Palette does: the document stays, with the state `Deleted`,
    /// `metadata.isDeleted`, the time in `metadata.deletionTimestamp`, and the annotation
    /// `deleted`. The listing and the search drop it.
    pub fn delete_cluster(&self, project: &str, uid: &str, at: &str) {
        let path = format!("v1/spectroclusters/{uid}");
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
        let path = format!("v1/clusterprofiles/{profile}");
        let mut doc = self.read(project, &path);
        for key in ["inUseClusterUids", "inUseClusters"] {
            if let Some(rows) = doc["status"][key].as_array_mut() {
                rows.retain(|r| r != &json!(cluster) && r["uid"] != json!(cluster));
            }
        }
        self.write(project, &path, &doc);
    }

    /// Change one field of the document of a cluster, or of its cloud config, at a JSON pointer:
    /// what Palette reports changes, as a change on the cluster does, and its profiles do not.
    pub fn cluster_field(&self, project: &str, uid: &str, pointer: &str, value: Value) {
        let (path, pointer) = match pointer.strip_prefix("/cloudconfig") {
            Some(rest) => (format!("v1/cloudconfigs/aws/{uid}c"), rest.to_string()),
            None => (format!("v1/spectroclusters/{uid}"), pointer.to_string()),
        };
        let mut doc = self.read(project, &path);
        // an existing field is replaced; a new key of an existing object is added
        let (parent, key) = pointer.rsplit_once('/').unwrap_or(("", pointer.as_str()));
        match doc.pointer_mut(&pointer) {
            Some(slot) => *slot = value,
            None => match doc.pointer_mut(parent).and_then(Value::as_object_mut) {
                Some(object) => {
                    object.insert(key.to_string(), value);
                }
                None => panic!("{path} has no object at {parent} for {key}"),
            },
        }
        self.write(project, &path, &doc);
        // a label is in the listing and the search too
        self.listings();
    }

    /// A saved tag filter of the tenant, `name`, that selects the clusters with the label
    /// `key=value`.
    pub fn tag_filter(&self, name: &str, uid: &str, key: &str, value: &str) {
        let path = "v1/filters/metadata@filterType=tag";
        let mut items: Vec<Value> = std::fs::read_to_string(self.doc("tenant", path))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v["items"].as_array().cloned())
            .unwrap_or_default();
        items.retain(|i| i["metadata"]["uid"] != uid);
        items.push(json!({"metadata": {"name": name, "uid": uid}, "spec": {"filterType": "tag"}}));
        self.write("tenant", path, &json!({"items": items}));
        self.write(
            "tenant",
            &format!("v1/filters/tag/{uid}"),
            &json!({"metadata": {"name": name, "uid": uid},
                "spec": {"filterGroup": {"conjunction": "and", "filters": [
                    {"key": key, "values": [value], "operator": "eq", "negation": false}]}}}),
        );
    }

    /// Create a cluster that runs `profiles` in order, with its own copy of what each profile
    /// holds now, so a later edit of a profile does not reach the cluster. Run it again for the
    /// same cluster to apply the update that waits for it.
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
            .map(|p| self.read(project, &format!("v1/clusterprofiles/{p}")))
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
        self.write(
            project,
            &format!("v1/spectroclusters/{uid}"),
            &json!({
                "apiVersion": "v1", "kind": "SpectroCluster",
                "metadata": {"name": name, "uid": uid,
                    "annotations": {"projectUid": project, "scope": "project"},
                    "labels": {"managed-by": "opentofu"}},
                "spec": {"cloudType": "aws", "clusterProfileTemplates": templates,
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
                    "spcApply": {"spcHash": fnv_hex(&json!(templates).to_string()),
                        "spcInfraHash": fnv_hex(&json!(infra).to_string())},
                    "packs": running,
                    "fips": {"mode": "none"}}
            }),
        );
        self.write(
            project,
            &format!("v1/cloudconfigs/aws/{config_uid}"),
            &json!({
                "metadata": {"uid": config_uid, "name": format!("{name}-aws-config")},
                "spec": {
                    "clusterConfig": {"region": "us-east-2", "sshKeyName": "platform",
                        "vpcId": format!("vpc-{uid}")},
                    "machinePoolConfig": [
                        {"name": "cp-pool", "isControlPlane": true, "instanceType": "t3.xlarge",
                            "size": 1, "azs": ["us-east-2a"], "subnetIds": {"us-east-2a": format!("subnet-{uid}")}},
                        {"name": "worker-pool", "isControlPlane": false, "instanceType": "t3.xlarge",
                            "size": 2, "azs": ["us-east-2a"], "subnetIds": {"us-east-2a": format!("subnet-{uid}")}},
                    ]
                }
            }),
        );

        let manifests: Vec<Value> = profiles
            .iter()
            .map(|p| {
                let d = self.read(project, &format!("v1/clusterprofiles/{p}/packs/manifests"));
                json!({"metadata": d["metadata"], "spec": d["spec"]})
            })
            .collect();
        self.write(
            project,
            &format!("v1/spectroclusters/{uid}/profiles/packs/manifests"),
            &json!({"profiles": manifests}),
        );

        let chosen: Map<String, Value> = values
            .iter()
            .map(|(k, v)| ((*k).to_string(), json!(v)))
            .collect();
        let variables: Vec<Value> = profiles
            .iter()
            .map(|p| {
                let d = self.read(project, &format!("v1/clusterprofiles/{p}/variables"));
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
        self.write(
            project,
            &format!("v1/spectroclusters/{uid}/variables"),
            &json!(variables),
        );

        self.resolve_macros(project, uid);
        self.write_profile_updates(project, uid);
        // the cluster now holds what each profile holds, so no update waits for it
        let mut items = self.notifications(project, uid);
        for n in &mut items {
            n["action"]["isDone"] = json!(true);
        }
        self.write_notifications(
            project,
            &format!("v1/notifications/spectrocluster/{uid}"),
            items,
        );

        // a cluster that moved to other versions is no longer a consumer of the ones it left
        for doc in self.documents_in(project, "v1/clusterprofiles") {
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
            self.write(project, &format!("v1/clusterprofiles/{p}"), &doc);
        }
        for p in profiles {
            let path = format!("v1/clusterprofiles/{p}");
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
        let path = format!("v1/spectroclusters/{uid}");
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
        self.write(
            project,
            &format!("v1/spectroclusters/{cluster}/validate/repave"),
            &json!({"isRepaveRequired": required, "reasons": reasons}),
        );
    }

    /// Give a project macro a value, as `PATCH /v1/projects/{uid}/macros` does, and let each
    /// cluster of the project resolve it again: Palette answers `packs/resolvedValues` with the
    /// value the macro has now.
    pub fn set_macro(&self, project: &str, name: &str, value: &str) {
        let path = format!("v1/projects/{project}/macros");
        let mut doc = if self.doc(project, &path).is_file() {
            self.read(project, &path)
        } else {
            json!({"macros": []})
        };
        let macros = doc["macros"].as_array_mut().expect("macros is a list");
        macros.retain(|m| m["name"] != name);
        macros.push(json!({"name": name, "value": value}));
        self.write(project, &path, &doc);
        let dir = self.doc(project, "v1/spectroclusters/x").with_file_name("");
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let file = entry.file_name().to_string_lossy().to_string();
            if let Some(uid) = file.strip_suffix(".json") {
                self.resolve_macros(project, uid);
            }
        }
    }

    /// The `packs/resolvedValues` answer of one cluster: for each profile it runs, each project
    /// macro and its value, and the system macro of the cluster name, as Palette gives them.
    fn resolve_macros(&self, project: &str, cluster: &str) {
        let doc = self.read(project, &format!("v1/spectroclusters/{cluster}"));
        let macros_path = format!("v1/projects/{project}/macros");
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
            &format!("v1/spectroclusters/{cluster}/packs/resolvedValues"),
            &json!({"profiles": profiles}),
        );
    }

    /// Give a cluster variable a new value, as a person changing it in Palette does.
    pub fn set_value(&self, project: &str, uid: &str, variable: &str, value: &str) {
        let path = format!("v1/spectroclusters/{uid}/variables");
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
            "v1/registries/metadata",
            &json!({"items": [
                {"name": "Public Repo", "uid": PUBLIC_REPO, "kind": "pack"},
                {"name": "Palette Registry", "uid": PALETTE_REGISTRY, "kind": "oci"}]}),
        );
        // the last sync of each registry, as v1RegistrySyncStatus gives it
        let synced = json!({"isSyncSupported": true, "status": "Completed",
            "lastRunTime": "2026-09-17T09:00:00.000Z", "lastSyncedTime": "2026-09-17T09:00:00.000Z"});
        self.write(
            project,
            &format!("v1/registries/pack/{PUBLIC_REPO}/sync/status"),
            &synced,
        );
        self.write(
            project,
            &format!("v1/registries/oci/{PALETTE_REGISTRY}/basic/sync/status"),
            &synced,
        );
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
            self.write(
                project,
                &format!("v1/packs/{name}/registries/{PUBLIC_REPO}"),
                &entity,
            );
            // The CSI pack is in the OCI registry too, as the live run of 20260917T150025Z found.
            // A pack ref that names no registry then has to say so.
            if name == "csi-aws-ebs" {
                add_csi_to_oci(&mut listing);
                self.write(
                    project,
                    &format!("v1/packs/{name}/registries/{PALETTE_REGISTRY}"),
                    &csi_entity_in_oci(&entity),
                );
            }
            self.write(
                project,
                &format!("v1/packs@filters=spec.name={name}&limit=100&offset=0"),
                &listing,
            );
        }
    }
}

impl Tenant {
    /// Take one version of a pack out of the listing of the Public Repo, as before the registry
    /// published it. `registries` puts it back.
    pub fn hide_pack_version(&self, project: &str, name: &str, version: &str) {
        let path = format!("v1/packs@filters=spec.name={name}&limit=100&offset=0");
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
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn profile_row(doc: &Value) -> Value {
    json!({
        "metadata": {"name": doc["metadata"]["name"], "uid": doc["metadata"]["uid"]},
        "spec": {"version": doc["spec"]["version"],
                 "cloudType": doc["spec"]["published"]["cloudType"]}
    })
}

fn cluster_row(doc: &Value) -> Value {
    json!({
        "metadata": {"name": doc["metadata"]["name"], "uid": doc["metadata"]["uid"],
                     "annotations": doc["metadata"]["annotations"],
                     "labels": doc["metadata"]["labels"]},
        "spec": {"cloudType": doc["spec"]["cloudType"]},
        "status": {"state": doc["status"]["state"]}
    })
}

/// True for a virtual cluster: the cloud type `nested`.
fn nested(cluster: &Value) -> bool {
    cluster["spec"]["cloudType"] == "nested"
}

/// A cluster as the search gives it: no spec, and a summary with the cloud, the project, and the
/// uid, name, and type of each profile.
fn search_row(doc: &Value, project_name: &str) -> Value {
    let profiles: Vec<Value> = doc["spec"]["clusterProfileTemplates"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| json!({"uid": t["uid"], "name": t["name"], "type": t["type"]}))
        .collect();
    json!({
        "metadata": {"name": doc["metadata"]["name"], "uid": doc["metadata"]["uid"],
                     "annotations": doc["metadata"]["annotations"],
                     "labels": doc["metadata"]["labels"]},
        "specSummary": {"cloudConfig": {"cloudType": doc["spec"]["cloudType"]},
                        "clusterProfileTemplates": profiles,
                        "projectMeta": {"name": project_name,
                                        "uid": doc["metadata"]["annotations"]["projectUid"]}},
        "status": {"state": doc["status"]["state"]}
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

/// The template of a profile on a cluster, from the profile document: the published profile.
impl Tenant {
    /// The templates of an object that holds profiles without being a cluster: an edge host or a
    /// cluster group. Palette gives the holder its own copy of each manifest, under a uid of its
    /// own, and the template names the copy by that uid, with no content. `route(pack, copy)` is
    /// where the API serves a copy; `{profile}` in it becomes the uid of the profile.
    fn holder_templates(
        &self,
        project: &str,
        holder: &str,
        profiles: &[&str],
        route: impl Fn(&str, &str) -> String,
    ) -> Vec<Value> {
        let mut n = 0;
        profiles
            .iter()
            .map(|p| {
                let mut t = template_of(&self.read(project, &format!("v1/clusterprofiles/{p}")));
                let contents = self.read(project, &format!("v1/clusterprofiles/{p}/packs/manifests"));
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
                            &route(&pname, &copy).replace("{profile}", p),
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
