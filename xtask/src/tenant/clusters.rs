//! The clusters of a project: their documents, their cloud configs, and the changes that a person
//! makes.

use serde_json::{Map, Value, json};
use strata::closed_set::ClosedSet;
use strata::model::cluster_state::ClusterState;
use strata::model::scope::ProfileScope;

use super::edit::with_reclaim_policy;
use super::macros::Pinner;
use super::profiles::template_of;
use super::routes::{
    PROFILES, TAG_FILTER_LISTING, cloud_config_route, cloud_config_uid, cluster_route,
    cluster_variables_route, group_route, manifests_route, namespaces_route, profile_route,
    rbacs_route, variables_route,
};
use super::{TENANT, Tenant};

impl Tenant {
    /// Delete a cluster at the time `at`, as Palette does: the document stays, with the state
    /// `Deleted`, `metadata.isDeleted`, `at` in `metadata.deletionTimestamp`, and the annotation
    /// `deleted`.
    ///
    /// The listing and the search drop it.
    pub fn delete_cluster(&self, project: &str, uid: &str, at: &str) {
        let path = cluster_route(uid);
        let mut doc = self.read(project, &path);
        doc["status"]["state"] = json!(ClusterState::Deleted.as_str());
        doc["metadata"]["isDeleted"] = json!(true);
        doc["metadata"]["deletionTimestamp"] = json!(at);
        doc["metadata"]["lastModifiedTimestamp"] = json!(at);
        doc["metadata"]["annotations"]["deleted"] = json!("true");
        self.write(project, &path, &doc);
        self.listings();
    }

    /// Take the cluster `cluster` out of the consumer list of the profile `profile`, as Palette
    /// does when it cannot compute the list. The cluster still runs the profile.
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
    /// changes the group. The profiles that it pins stay.
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
        let path = TAG_FILTER_LISTING;
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
            &json!({"metadata": {"uid": cloud_config_uid(uid)}, "spec": spec}),
        );
    }

    /// Create a cluster that runs `profiles` in order, with its own copy of what each profile
    /// holds now. A later edit of a profile does not reach the cluster.
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
        self.write_cluster_documents(project, uid, name, profiles);
        self.write_cluster_resources(project, uid, name);
        self.write_cluster_manifests(project, uid, profiles);
        self.write_cluster_variables(project, uid, profiles, values);
        self.resolve_macros(project, Pinner::Cluster, uid);
        self.write_profile_updates(project, uid);
        // the cluster now holds what each profile holds, so no update waits for it
        self.close_notifications(project, uid);
        self.bind_profiles(project, uid, name, profiles);
        self.listings();
    }

    /// Write the document of the cluster, with its own copy of each profile, and its cloud config.
    fn write_cluster_documents(&self, project: &str, uid: &str, name: &str, profiles: &[&str]) {
        let templates: Vec<Value> = profiles
            .iter()
            .map(|p| template_of(&self.read(project, &profile_route(p))))
            .collect();
        self.write(
            project,
            &cluster_route(uid),
            &cluster_document(project, uid, name, &templates),
        );
        self.write(
            project,
            &cloud_config_route("aws", uid),
            &aws_cloud_config(uid, name),
        );
    }

    /// Write the role bindings and the namespaces of the cluster, which are on routes of their own.
    /// The cluster has one binding of a group to a cluster role, and one namespace with a quota.
    fn write_cluster_resources(&self, project: &str, uid: &str, name: &str) {
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
    }

    /// Write the manifests of the profiles of the cluster.
    fn write_cluster_manifests(&self, project: &str, uid: &str, profiles: &[&str]) {
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
    }

    /// Write the variables of the profiles of the cluster. `values` gives a value to each variable
    /// that it names; each other variable keeps its default.
    fn write_cluster_variables(
        &self,
        project: &str,
        uid: &str,
        profiles: &[&str],
        values: &[(&str, &str)],
    ) {
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
    }

    /// Name the cluster as a consumer of its profiles, and remove it from the consumers of the
    /// profiles that it left.
    fn bind_profiles(&self, project: &str, uid: &str, name: &str, profiles: &[&str]) {
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
    }

    /// Change the pack values on a cluster, as a person who edits the cluster does. The profile
    /// keeps what it held, so only the cluster's copy differs.
    pub fn on_cluster_reclaim(&self, project: &str, uid: &str, policy: &str) {
        let path = cluster_route(uid);
        let mut doc = self.read(project, &path);
        if let Some(templates) = doc["spec"]["clusterProfileTemplates"].as_array_mut() {
            for template in templates {
                for pack in template["packs"].as_array_mut().into_iter().flatten() {
                    if pack["name"] == "csi-aws-ebs"
                        && let Some(values) = pack["values"].as_str()
                    {
                        pack["values"] = json!(with_reclaim_policy(values, policy));
                    }
                }
            }
        }
        self.write(project, &path, &doc);
    }

    /// Give a cluster variable a new value, as a person who changes it in Palette does.
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
}

/// The document of a cluster that runs `templates`, as Palette gives it for a cluster that it
/// built.
fn cluster_document(project: &str, uid: &str, name: &str, templates: &[Value]) -> Value {
    // Palette reports more of a cluster than its profiles. The cluster has its own hashes of the
    // spec that it applied and the pack version that it reconciled. It also has its policy, its
    // labels, and its cloud config
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
    let config_uid = cloud_config_uid(uid);
    // Palette gives a cluster that it built the type PureManage. A chapter changes the field
    // for an imported cluster.
    json!({
        "apiVersion": "v1", "kind": "SpectroCluster",
        "metadata": {"name": name, "uid": uid,
            "annotations": {"projectUid": project, "scope": ProfileScope::Project.as_str()},
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
    })
}

/// The cloud config of an AWS cluster: one pool for the control plane, and one for the workers.
fn aws_cloud_config(uid: &str, name: &str) -> Value {
    json!({
        "metadata": {"uid": cloud_config_uid(uid), "name": format!("{name}-aws-config")},
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
    })
}

/// A 64-bit FNV-1a hash as hex. It stands in for the hashes that Palette computes of a cluster,
/// and it is the same on every machine and every Rust version.
fn fnv_hex(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}
