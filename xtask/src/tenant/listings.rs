//! The profile and cluster listings of each scope, and the searches that read the same documents.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Map, Value, json};

use super::routes::{
    CLUSTER_SEARCH_PATH, CLUSTERS, GROUPS, PROFILE_SEARCH_PATH, PROFILES, PROJECT_LISTING,
    cluster_route, paged, profile_route, virtual_clusters_file,
};
use super::{TENANT, Tenant};
use crate::files::files_with_ext;

/// What the cluster search needs of the virtual clusters.
struct VirtualMembers {
    /// The uids of the profiles that the group of a virtual cluster pins, by uid of the cluster.
    pins: Map<String, Value>,
    /// The group and the host cluster of a virtual cluster, by uid of the cluster.
    groups: Map<String, Value>,
}

impl Tenant {
    /// The profile and cluster listings of each scope, from the documents in the tenant. At tenant
    /// scope Palette lists the tenant's own profiles, and the clusters of every project.
    pub fn listings(&self) {
        let mut every_cluster = Vec::new();
        let scopes = self.project_dirs();
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
        self.write_cluster_searches(&scopes, &every_cluster);
    }

    /// The directory of each project, in name order. The directory of the tenant is no project.
    fn project_dirs(&self) -> Vec<String> {
        let mut scopes: Vec<String> = std::fs::read_dir(&self.root)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name != TENANT)
            .collect();
        scopes.sort();
        scopes
    }

    /// The cluster search of each scope, from the clusters `every_cluster`. The search of a
    /// project holds the clusters of that project. The search of the tenant holds every cluster.
    /// Each scope has the schema.
    fn write_cluster_searches(&self, scopes: &[String], every_cluster: &[Value]) {
        // The search of the tenant holds every cluster, which the server filters by the body of
        // a request. The schema says that the search takes tags
        let names = self.project_names();
        let members = self.virtual_members(scopes);
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
                let pins = members.pins.get(&uid).cloned();
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
                    members.groups.get(&uid).filter(|_| nested(&c)),
                )
            })
            .collect();
        // the search of a project holds the clusters of that project, and the search of the
        // tenant every cluster; each scope has the schema
        let schema = json!({"schema": {"properties": [
            {"name": "clusterName", "type": "string"}, {"name": "clusterState", "type": "string"},
            {"name": "tags", "type": "string"}, {"name": "environment", "type": "string"},
            {"name": "clusterProfiles", "type": "string"}]}});
        for project in scopes {
            let rows: Vec<Value> = found
                .iter()
                .filter(|r| r["specSummary"]["projectMeta"]["uid"] == json!(project))
                .cloned()
                .collect();
            self.write_cluster_search(project, rows, &schema);
        }
        self.write_cluster_search(TENANT, found, &schema);
    }

    /// The name of each project, by the uid of the project.
    fn project_names(&self) -> Map<String, Value> {
        let projects: Value = std::fs::read_to_string(self.doc(TENANT, PROJECT_LISTING))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        projects["items"]
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
            .collect()
    }

    /// The group and the host cluster of each virtual cluster, and the profiles that its group
    /// pins.
    fn virtual_members(&self, scopes: &[String]) -> VirtualMembers {
        // The search reads a summary of each cluster. The summary of a virtual cluster does not
        // name the profiles that its cluster group pins, as on a live Palette (#184). So a
        // search by profile does not find a virtual cluster through its group. The summary of a
        // virtual cluster names its group and its host cluster in `status.virtual`.
        let mut virtuals = VirtualMembers {
            pins: Map::new(),
            groups: Map::new(),
        };
        for project in scopes {
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
                    let entry = virtuals.pins.entry(muid.to_string()).or_insert(json!([]));
                    if let Some(list) = entry.as_array_mut() {
                        list.extend(pinned.iter().cloned());
                    }
                    virtuals
                        .groups
                        .insert(muid.to_string(), virtual_summary.clone());
                }
            }
        }
        virtuals
    }

    /// The profile listing, the profile search, and the cluster listing of one scope.
    ///
    /// The cluster listing omits each virtual cluster; the search holds it, when a condition
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

    /// Copy the documents of the profile `uid` from the directory `from` to the directory `to`.
    /// They are the profile document, and each document under its path, such as its manifests and
    /// its variables.
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
    pub(super) fn documents_in(&self, project: &str, path: &str) -> Vec<Value> {
        files_with_ext(&self.root.join(project).join(path), "json")
            .iter()
            .filter_map(|f| std::fs::read_to_string(f).ok())
            .filter_map(|t| serde_json::from_str(&t).ok())
            .collect()
    }
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
pub(super) fn copy_dir(from: &Path, to: &Path) {
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

/// A cluster as the search gives it. The row has no spec, and a summary with the cloud, the
/// project, and the uid, name, and type of each profile. A virtual cluster has `status.virtual`,
/// `virtual_summary`: its cluster group and its host cluster.
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
