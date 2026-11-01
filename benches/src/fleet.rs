//! A fleet of clusters in an in-memory Palette.
//!
//! Every cluster of a fleet runs the same profile, so the fleet differs from another fleet only in
//! its count. A cluster of the verify fleet runs a stack of three real packs: the OS, Kubernetes,
//! and the storage. A cluster of the consumer fleet states the profile that it runs, and no pack.

use std::path::PathBuf;

use serde_json::{Value, json};
use strata::api::{MemoryApi, RawCluster, ReadErrors};
use strata::snapshot::Snapshot;
use tempfile::TempDir;

use crate::FETCHED_AT;
use crate::profile::{self, PROFILE_NAME, PROFILE_UID, PROFILE_VERSION};
use crate::real;
use crate::routes::{
    cluster_manifests_route, cluster_route, cluster_variables_route, profile_route,
};

/// The uid of the cluster `index`: 24 characters, as Palette writes a uid.
fn cluster_uid(index: usize) -> String {
    format!("c1a5{index:020}")
}

/// The packs that each cluster of the verify fleet runs: the newest OS pack, Kubernetes, and the
/// storage pack, as the refs of the profile.
fn stack() -> Vec<Value> {
    let families = real::families();
    [0, 1, 3]
        .into_iter()
        .map(|place| {
            let family = &families[place];
            let newest = family.at(family.versions.len() - 1);
            profile::fixture(newest, family.name).pack_ref(&[])
        })
        .collect()
}

/// The profile template that a cluster applies, as `spec.clusterProfileTemplates` lists it.
fn template(packs: Value) -> Value {
    json!({
        "uid": PROFILE_UID, "name": PROFILE_NAME, "profileVersion": PROFILE_VERSION,
        "type": "cluster", "cloudType": "aws", "packs": packs,
    })
}

/// A fleet to verify: the records of the clusters on disk, and the Palette that serves them.
pub struct VerifyFleet {
    /// The Palette that serves each cluster and the profile that it runs.
    pub api: MemoryApi,
    /// The record file of each cluster, in the order of the fleet.
    pub records: Vec<PathBuf>,
    // the directory goes with the fleet: the records are files of it
    _dir: TempDir,
}

impl VerifyFleet {
    /// A fleet of `clusters` clusters that run the stack, each with a record that Palette still
    /// holds.
    ///
    /// # Panics
    ///
    /// When the files cannot be written, or the generated responses do not build a record.
    pub fn new(clusters: usize) -> VerifyFleet {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let packs = Value::Array(stack());
        let mut api =
            MemoryApi::new().with(profile_route(PROFILE_UID), profile::document(packs.clone()));
        let mut records = Vec::with_capacity(clusters);
        let mut base: Option<Snapshot> = None;
        for index in 0..clusters {
            let uid = cluster_uid(index);
            let name = format!("cluster-{index}");
            let document = json!({
                "metadata": {"uid": uid, "name": name},
                "spec": {"cloudType": "aws", "clusterProfileTemplates": [template(packs.clone())]},
                "status": {"state": "Running"},
            });
            let manifests = json!({"profiles": [{
                "metadata": {"name": PROFILE_NAME, "uid": PROFILE_UID},
                "spec": {"packs": []},
            }]});
            let variables = json!([]);
            // Each record is the record of the first cluster with its own name and uid. The packs
            // are the same, and the root does not cover the cluster.
            let record = base.get_or_insert_with(|| {
                let raw = RawCluster {
                    cluster: document.clone(),
                    manifests: manifests.clone(),
                    variables: variables.clone(),
                    variables_unavailable: false,
                    project_uid: None,
                    fetched_at: None,
                    notifications: None,
                    repave: None,
                    resolved_values: None,
                    cloud_config: None,
                    rbacs: None,
                    namespaces: None,
                    errors: ReadErrors::default(),
                };
                Snapshot::from_cluster(&raw, None, &format!("cluster:{uid}"), FETCHED_AT)
                    .expect("the generated responses build a record")
            });
            let mut record = record.clone();
            record.source.reference = format!("cluster:{uid}");
            if let Some(cluster) = record.source.cluster.as_mut() {
                cluster.uid = uid.clone();
                cluster.name = name;
            }
            let path = dir.path().join(format!("cluster-{index:04}.json"));
            std::fs::write(&path, record.to_json(true).expect("a record serializes"))
                .expect("the record is written");
            records.push(path);
            api = api
                .with(cluster_route(&uid), document)
                .with(cluster_manifests_route(&uid), manifests)
                .with(cluster_variables_route(&uid), variables);
        }
        VerifyFleet {
            api,
            records,
            _dir: dir,
        }
    }
}

/// A fleet to list: a Palette that serves a profile version and the `clusters` clusters that run
/// it.
pub fn consumer_fleet(clusters: usize) -> MemoryApi {
    let uids: Vec<String> = (0..clusters).map(cluster_uid).collect();
    let named: Vec<Value> = uids
        .iter()
        .enumerate()
        .map(|(index, uid)| json!({"uid": uid, "name": format!("cluster-{index}"), "projectUid": ""}))
        .collect();
    let mut profile = profile::document(Value::Array(stack()));
    profile["status"]["inUseClusterUids"] = json!(uids);
    profile["status"]["inUseClusters"] = Value::Array(named);
    let mut api = MemoryApi::new().with(profile_route(PROFILE_UID), profile);
    for (index, uid) in uids.iter().enumerate() {
        api = api.with(
            cluster_route(uid),
            json!({
                "metadata": {"uid": uid, "name": format!("cluster-{index}")},
                "spec": {"clusterProfileTemplates": [template(json!([]))]},
                "status": {"state": "Running"},
            }),
        );
    }
    api
}
