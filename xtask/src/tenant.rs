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
}

impl Edit {
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
            let clusters = self.documents_in(project, "v1/spectroclusters");
            every_cluster.extend(clusters.iter().cloned());
            let items: Vec<Value> = profiles.iter().map(profile_row).collect();
            self.write(
                project,
                "v1/dashboard/clusterprofiles/metadata",
                &json!({"items": items}),
            );
            let rows: Vec<Value> = clusters.iter().map(cluster_row).collect();
            self.write(project, "v1/dashboard/spectroclusters/meta", &json!(rows));
        }
        self.write(
            "tenant",
            "v1/dashboard/clusterprofiles/metadata",
            &json!({"items": []}),
        );
        let rows: Vec<Value> = every_cluster.iter().map(cluster_row).collect();
        self.write("tenant", "v1/dashboard/spectroclusters/meta", &json!(rows));
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
        let content = format!(
            "apiVersion: v1\nkind: Namespace\nmetadata:\n  name: payments\n  labels:\n    \
             pod-security.kubernetes.io/enforce: {label}"
        );
        self.write(
            project,
            &format!("v1/clusterprofiles/{uid}"),
            &json!({
                "apiVersion": "v1", "kind": "ClusterProfile",
                "metadata": {"name": "platform-addons", "uid": uid},
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
                "metadata": {"name": "platform-addons", "uid": uid},
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

    /// Change a published profile in place, as an edit through the API does.
    pub fn edit(&self, project: &str, uid: &str, edits: &[Edit]) {
        let path = format!("v1/clusterprofiles/{uid}");
        let mut doc = self.read(project, &path);
        for edit in edits {
            edit.apply(&mut doc);
        }
        self.write(project, &path, &doc);
        self.listings();
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
        let templates: Vec<Value> = docs
            .iter()
            .map(|d| {
                json!({
                    "uid": d["metadata"]["uid"], "name": d["metadata"]["name"],
                    "profileVersion": d["spec"]["version"],
                    "type": d["spec"]["published"]["type"],
                    "cloudType": d["spec"]["published"]["cloudType"],
                    "packs": d["spec"]["published"]["packs"],
                })
            })
            .collect();
        self.write(
            project,
            &format!("v1/spectroclusters/{uid}"),
            &json!({
                "apiVersion": "v1", "kind": "SpectroCluster",
                "metadata": {"name": name, "uid": uid,
                    "annotations": {"projectUid": project, "scope": "project"}},
                "spec": {"cloudType": "aws", "clusterProfileTemplates": templates},
                "status": {"state": "Running"}
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
                     "annotations": doc["metadata"]["annotations"]},
        "spec": {"cloudType": doc["spec"]["cloudType"]},
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
