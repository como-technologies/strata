//! The world: the one JSON document that every program of a rule reads. It holds what a run
//! knows before any check runs, and each kind of check adds its own answer to a copy of it.
//!
//! ```text
//! {
//!   "cloud":    "eks",                 the engine: aws, eks, gcp, gke, azure, aks, maas, edge, vsphere
//!   "release":  "4.10.3" | null,       the Palette release, when given
//!   "source":   "cluster.json",        where the entity came from
//!   "entity":   { metadata, spec },    the body of POST /v1/spectroclusters/{cloud}
//!   "before":   { ... } | null,        the entity before a change, for a change of a cluster
//!   "profiles": [ record, ... ],       one strata record for each profile of the entity
//!   "packs":    { "<packUid>": {...} } the pack of each layer, GET /v1/packs/{uid}, when live
//!   "account":  { ... } | null         the cloud account, with no secret in it
//!   "accountKind": "aws",              the kind of cloud account of the engine: eks has aws
//!   "routeCloud": "edge-native"        the cloud type of the engine in a route: edge has edge-native
//! }
//! ```
//!
//! `accountKind` and `routeCloud` come from the engine, by [`account_kind`] and [`route_cloud`], so
//! that no rule writes that mapping again.
//!
//! A check adds `response` (Palette), `exec` (a command), or `cluster`, `summary`, `events`,
//! and `elapsed` (the watch). A fixture of a test is a world with the answers written in, under
//! `_responses` and `_exec`; see [`crate::probe::FixtureBackend`].

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Map, Value, json};
use strata::api::{Palette, PaletteApi, RawCluster, profile_variables};
use strata::cloud::CLOUD_EDGE_NATIVE;
use strata::json::{arr, opt_str, opt_text, str_at, text_map, value_at, value_or_null};
use strata::reference::Ref;
use strata::snapshot::{Snapshot, SubjectKind};
use strata::term::progress::Progress;

use crate::error::{Error, Result};

/// The engines a cluster can be built on. Each is a cloud type of Palette, except `edge`, which
/// Palette names [`CLOUD_EDGE_NATIVE`]. OpenStack is not one: Palette removed it in 2026, and the
/// API has no route for it.
///
/// A rule names an engine too: see [`crate::rule::engines`].
pub const CLOUDS: &[&str] = &[
    "aws", "eks", "gcp", "gke", "azure", "aks", "maas", EDGE, "vsphere",
];

/// The engine of an edge cluster. Palette names its cloud type [`CLOUD_EDGE_NATIVE`].
const EDGE: &str = "edge";

/// The fields of a flat machine pool of a cloud config that belong to `poolConfig` of the
/// entity. Every other field is the cloud part of the pool.
const POOL_CONFIG_FIELDS: &[&str] = &[
    "name",
    "size",
    "minSize",
    "maxSize",
    "isControlPlane",
    "useControlPlaneAsWorker",
    "labels",
    "taints",
    "additionalLabels",
    "additionalAnnotations",
    "additionalTags",
    "nodeRepaveInterval",
    "updateStrategy",
    "skipK8sUpgrade",
    "overrideKubeadmConfiguration",
    "overrideHealthCheckConfiguration",
    "overrideClusterAPIConfig",
    "machinePoolProperties",
];

/// The fields of a cloud account document that hold a secret. A world never carries one.
const SECRET_FIELDS: &[&str] = &[
    "accessKey",
    "secretKey",
    "secretToken",
    "jsonCredentials",
    "clientSecret",
    "apiKey",
    "password",
    "hostPairingKey",
    "hostAuthToken",
];

/// The fields that a file on disk must not carry beside the secrets: an external id is not a
/// credential, and the account validate route needs it, so the world keeps it at run time and
/// `world.json` does not.
const OUTPUT_ONLY_FIELDS: &[&str] = &["externalId"];

/// The inputs of a run, before any check.
#[derive(Debug, Clone, Default)]
pub struct World {
    /// The engine: `aws`, `eks`, `gcp`, `gke`, `azure`, `aks`, `maas`, `edge`, `vsphere`.
    pub cloud: String,
    /// The Palette release, when given. A tenant key cannot read it from Palette.
    pub release: Option<String>,
    /// Where the entity came from, for the report.
    pub source: String,
    /// The body of the create.
    pub entity: Value,
    /// The entity before the change, for a change of a cluster.
    pub before: Option<Value>,
    /// One strata record for each profile the entity names, in the order of the entity.
    pub profiles: Vec<Value>,
    /// The pack of each layer of each profile, by pack uid, when a key was there to read them.
    pub packs: BTreeMap<String, Value>,
    /// The cloud account of the entity, with no secret, when a key was there to read it.
    pub account: Option<Value>,
    /// The cluster uid, for a world of an existing cluster: the watch reads it.
    pub cluster_uid: Option<String>,
}

impl World {
    /// The document the programs read.
    pub fn to_value(&self) -> Value {
        let mut world = json!({
            "cloud": self.cloud,
            "release": self.release,
            "source": self.source,
            "entity": self.entity,
            "before": self.before,
            "profiles": self.profiles,
            "packs": self.packs,
            "account": self.account,
            "clusterUid": self.cluster_uid,
        });
        add_engine_clouds(&mut world);
        world
    }

    /// A world from a file: the body of a create, as JSON or YAML. The cloud comes from `cloud`,
    /// else from an envelope `{cloudType|cloud, entity|spec}`, else from the shape of
    /// `spec.cloudConfig`. A file that names no cloud is an error that says to give `--cloud`.
    pub fn from_file(path: &Path, cloud: Option<&str>) -> Result<World> {
        let text = std::fs::read_to_string(path).map_err(|e| crate::error::io(path, e))?;
        let doc: Value = parse_json_or_yaml(&text).map_err(|m| Error::Input {
            message: format!("{}: {m}", path.display()),
        })?;
        let (envelope_cloud, entity) = unwrap_envelope(doc);
        let cloud = match cloud.map(normalize_cloud).or(envelope_cloud) {
            Some(c) => c,
            None => infer_cloud(&entity).ok_or_else(|| Error::Input {
                message: format!(
                    "{}: the entity names no cloud, and its shape fits more than one; give --cloud, one of {}",
                    path.display(),
                    CLOUDS.join(", ")
                ),
            })?,
        };
        if !CLOUDS.contains(&cloud.as_str()) {
            return Err(Error::Input {
                message: format!(
                    "'{cloud}' is not a cloud strata-preflight knows: {}",
                    CLOUDS.join(", ")
                ),
            });
        }
        if entity.pointer("/spec").is_none() {
            return Err(Error::Input {
                message: format!(
                    "{}: not a cluster entity: no `spec`. Give the body of POST /v1/spectroclusters/{cloud}",
                    path.display()
                ),
            });
        }
        Ok(World {
            cloud,
            source: path.display().to_string(),
            entity,
            ..World::default()
        })
    }

    /// A world from an existing cluster, as Palette holds it now: the cluster document, its cloud
    /// config, its profiles, and the values of its variables become the body of a create. `id` is
    /// the uid of the cluster, or its name when no cluster has that uid.
    ///
    /// The value of a sensitive variable is the mask that Palette gives. The world sends it as it
    /// is, so the validate route reads a mask and not the secret.
    ///
    /// # Errors
    ///
    /// [`Error::Palette`] with the error of a request, such as a cluster that the scope does not
    /// have, or a name that two clusters have. [`Error::Input`] when the document names no cloud
    /// type.
    pub fn from_cluster<A: PaletteApi>(
        palette: &Palette<A>,
        id: &str,
        progress: &Progress,
    ) -> Result<World> {
        let cluster = palette.subject_by_id(SubjectKind::Cluster, id, progress)?;
        let uid = opt_str(&cluster, "/metadata/uid").unwrap_or(id);
        let cloud_type = RawCluster::cloud_type(&cluster);
        if cloud_type.is_empty() {
            return Err(Error::Input {
                message: format!(
                    "cluster {uid}: the document names no cloud type, in \
                     spec.cloudConfigRef.kind or in spec.cloudType"
                ),
            });
        }
        let cloud = normalize_cloud(cloud_type);
        let config = palette.cloud_config(&cluster)?;
        let account_uid = opt_str(&config, "/spec/cloudAccountRef/uid")
            .or_else(|| opt_str(&cluster, "/spec/cloudAccountRef/uid"));
        // the flag is true for a Palette with no variables route, and the profiles then send none
        let (variables, _) = palette.cluster_variables(uid, progress)?;
        let entity = entity_of(&cluster, &config, account_uid, &variables);
        Ok(World {
            cloud,
            source: Ref::Cluster {
                uid: uid.to_string(),
                profile: None,
                pending: false,
            }
            .to_string(),
            entity,
            cluster_uid: Some(uid.to_string()),
            ..World::default()
        })
    }

    /// Apply a change file to the entity: the entity as it is becomes `before`, and the change
    /// merges over it. An object merges key by key; `machinepoolconfig` merges pool by pool, on
    /// the pool name; `profiles` merges profile by profile, on the uid; `variables` merges
    /// variable by variable, on the name; any other list replaces.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when strata cannot read the file at `path`. [`Error::Input`] when the file is
    /// neither JSON nor YAML.
    pub fn with_change(mut self, path: &Path) -> Result<World> {
        let text = std::fs::read_to_string(path).map_err(|e| crate::error::io(path, e))?;
        let doc: Value = parse_json_or_yaml(&text).map_err(|m| Error::Input {
            message: format!("{}: {m}", path.display()),
        })?;
        let (_, change) = unwrap_envelope(doc);
        let before = self.entity.clone();
        let mut after = before.clone();
        merge_change(&mut after, &change);
        self.before = Some(before);
        self.entity = after;
        self.source = format!("{} with {}", self.source, path.display());
        Ok(self)
    }

    /// The profile uids the entity names, in order.
    pub fn profile_uids(&self) -> Vec<String> {
        arr(&self.entity, "/spec/profiles")
            .iter()
            .filter_map(|p| opt_text(p, "/uid"))
            .collect()
    }

    /// Add the records of the profiles from files: each file is a strata record, matched to a
    /// profile of the entity by its uid. A record that matches no profile is kept out, and a
    /// profile with no record is reported by the rules that need one.
    pub fn with_records(mut self, paths: &[std::path::PathBuf]) -> Result<World> {
        let mut records = Vec::new();
        for path in paths {
            let snap = Snapshot::load(path)?;
            records.push(snap);
        }
        for uid in self.profile_uids() {
            if let Some(snap) = records.iter().find(|s| s.source.uid == uid) {
                self.profiles.push(record_value(snap)?);
            }
        }
        Ok(self)
    }

    /// Read from Palette what the static rules need beside the entity: a record of each profile,
    /// the pack of each layer, and the cloud account with no secret.
    pub fn with_palette<A: PaletteApi>(
        mut self,
        palette: &Palette<A>,
        now: &str,
        progress: &Progress,
    ) -> Result<World> {
        for uid in self.profile_uids() {
            if self
                .profiles
                .iter()
                .any(|p| str_at(p, "/source/uid") == uid)
            {
                continue;
            }
            progress.note(&format!("recording profile {uid}"));
            let snap = strata::ops::snapshot_for(
                &Ref::Uid(uid.clone()),
                &strata::ops::SnapshotOptions::default(),
                || Ok(Palette::new(palette.api())),
                now,
                progress,
            )?;
            for pack in &snap.packs {
                if pack.pack_uid.is_empty() || self.packs.contains_key(&pack.pack_uid) {
                    continue;
                }
                match palette.pack_document(&pack.pack_uid) {
                    Ok(doc) => {
                        self.packs.insert(pack.pack_uid.clone(), doc);
                    }
                    // a pack the key cannot read is not a pack; a rule that needs it says so
                    Err(strata::Error::Api { status, .. }) if unreadable(status) => {}
                    Err(e) => return Err(e.into()),
                }
            }
            self.profiles.push(record_value(&snap)?);
        }
        if let Some(uid) = opt_str(&self.entity, "/spec/cloudAccountUid") {
            match palette.cloud_account(account_kind(&self.cloud), uid) {
                Ok(mut doc) => {
                    redact(&mut doc);
                    self.account = Some(doc);
                }
                // an account that is not there, or that the key cannot read, is what a rule
                // reports; the world says null
                Err(strata::Error::Api { status, .. }) if unreadable(status) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(self)
    }
}

/// True for each 4xx on a read of the world but 401 and 429: the object is not there for this key.
/// A 404 is one case, and the 403 of a project key on an object of the tenant is another. The
/// world holds `null`, and a rule says so. A 401 is the key itself, and a 429 the rate: the run
/// stops with the error that names it.
pub(crate) fn unreadable(status: u16) -> bool {
    (400..500).contains(&status) && !matches!(status, 401 | 429)
}

/// The cloud account kind of a cloud: a managed engine uses the account of its cloud.
pub fn account_kind(cloud: &str) -> &str {
    match cloud {
        "eks" => "aws",
        "gke" => "gcp",
        "aks" => "azure",
        other => route_cloud(other),
    }
}

/// The name Palette gives a cloud in a route: `edge` is [`CLOUD_EDGE_NATIVE`].
pub fn route_cloud(cloud: &str) -> &str {
    if cloud == EDGE {
        CLOUD_EDGE_NATIVE
    } else {
        cloud
    }
}

/// Put `accountKind` and `routeCloud` in the world `world`, from its `cloud`. [`World::to_value`]
/// writes them, and the loader of a fixture, which is a world on disk, adds them here. A world
/// with no `cloud` stays as it is.
pub(crate) fn add_engine_clouds(world: &mut Value) {
    let Some(cloud) = opt_text(world, "/cloud") else {
        return;
    };
    world["accountKind"] = json!(account_kind(&cloud));
    world["routeCloud"] = json!(route_cloud(&cloud));
}

/// The engine of the cloud type `text`, which Palette or a file names: [`EDGE`] for
/// [`CLOUD_EDGE_NATIVE`].
fn normalize_cloud(text: &str) -> String {
    match text.trim().to_ascii_lowercase().as_str() {
        CLOUD_EDGE_NATIVE => EDGE.to_string(),
        other => other.to_string(),
    }
}

fn parse_json_or_yaml(text: &str) -> std::result::Result<Value, String> {
    match serde_json::from_str(text) {
        Ok(v) => Ok(v),
        Err(json_err) => serde_yaml::from_str::<Value>(text)
            .map_err(|yaml_err| format!("not JSON ({json_err}) and not YAML ({yaml_err})")),
    }
}

/// `{cloudType|cloud, entity|spec}` gives its cloud and its entity; a bare entity gives itself.
fn unwrap_envelope(doc: Value) -> (Option<String>, Value) {
    let cloud = opt_str(&doc, "/cloudType")
        .or_else(|| opt_str(&doc, "/cloud"))
        .map(normalize_cloud);
    match doc.get("entity") {
        Some(entity) if cloud.is_some() || entity.get("spec").is_some() => (cloud, entity.clone()),
        _ => (cloud, doc),
    }
}

/// The cloud that the shape of `spec.cloudConfig` fits, when it fits one alone.
pub fn infer_cloud(entity: &Value) -> Option<String> {
    let spec = entity.get("spec")?;
    let cc = spec.get("cloudConfig");
    let has = |k: &str| cc.and_then(|c| c.get(k)).is_some_and(|v| !v.is_null());
    let pool_has = |k: &str| {
        arr(spec, "/machinepoolconfig")
            .iter()
            .any(|p| p.pointer(&format!("/cloudConfig/{k}")).is_some())
    };
    if has("managedClusterConfig") {
        return Some("gke".into());
    }
    if has("project") {
        return Some("gcp".into());
    }
    if has("subscriptionId") || has("location") && has("resourceGroup") {
        return Some(
            if pool_has("isSystemNodePool")
                || spec
                    .pointer("/machinepoolconfig/0/managedPoolConfig")
                    .is_some()
            {
                "aks"
            } else {
                "azure"
            }
            .into(),
        );
    }
    if has("domain") || pool_has("resourcePool") {
        return Some("maas".into());
    }
    if pool_has("edgeHosts")
        || has("controlPlaneEndpoint")
            && !has("placement")
            && cc.and_then(|c| c.get("ntpServers")).is_some()
            && spec.get("cloudAccountUid").is_none()
    {
        return Some("edge".into());
    }
    if has("placement") || pool_has("placements") {
        return Some("vsphere".into());
    }
    if has("endpointAccess")
        || has("encryptionConfig")
        || has("addons")
        || spec.get("fargateProfiles").is_some()
        || pool_has("amiType")
        || pool_has("awsLaunchTemplate")
    {
        return Some("eks".into());
    }
    // a plain AWS entity and a plain EKS entity share a shape: region, vpcId, sshKeyName
    None
}

/// The body of a create, from a cluster document, its cloud config, and the answer of its
/// variables route, `variables`. Each profile sends the name and the value of its variables, as a
/// create sends them.
fn entity_of(
    cluster: &Value,
    config: &Value,
    account_uid: Option<&str>,
    variables: &Value,
) -> Value {
    let pools: Vec<Value> = arr(config, "/spec/machinePoolConfig")
        .iter()
        .map(split_pool)
        .collect();
    let profiles: Vec<Value> = arr(cluster, "/spec/clusterProfileTemplates")
        .iter()
        .map(|t| {
            json!({
                "uid": value_or_null(t, "/uid"),
                "packValues": arr(t, "/packs").iter().map(|p| {
                    json!({
                        "name": value_at(p, "/name"),
                        "tag": value_at(p, "/tag"),
                        "type": value_at(p, "/type"),
                        "values": value_at(p, "/values"),
                        "manifests": arr(p, "/manifests").iter().map(|m| {
                            json!({"name": value_at(m, "/name"), "uid": value_at(m, "/uid")})
                        }).collect::<Vec<_>>(),
                    })
                }).collect::<Vec<_>>(),
                "variables": profile_variables(variables, str_at(t, "/uid")).map(|v| {
                    json!({"name": value_at(v, "/name"), "value": value_at(v, "/value")})
                }).collect::<Vec<_>>(),
            })
        })
        .collect();
    let mut spec = Map::new();
    if let Some(uid) = account_uid {
        spec.insert("cloudAccountUid".into(), json!(uid));
    }
    spec.insert(
        "cloudConfig".into(),
        config
            .pointer("/spec/clusterConfig")
            .cloned()
            .unwrap_or(Value::Object(Map::new())),
    );
    spec.insert("machinepoolconfig".into(), Value::Array(pools));
    spec.insert("profiles".into(), Value::Array(profiles));
    if let Some(cc) = cluster.pointer("/spec/clusterConfig") {
        spec.insert("clusterConfig".into(), cc.clone());
    }
    if let Some(t) = cluster.pointer("/spec/clusterType") {
        spec.insert("clusterType".into(), t.clone());
    }
    json!({
        "metadata": {
            "name": value_at(cluster, "/metadata/name"),
            "labels": text_map(cluster, "/metadata/labels"),
            "annotations": text_map(cluster, "/metadata/annotations"),
        },
        "spec": Value::Object(spec),
    })
}

/// A flat pool of a cloud config as the `{cloudConfig, poolConfig}` pair of the entity. The
/// `subnetIds` map of a cloud config becomes the `subnets` list of the entity.
fn split_pool(flat: &Value) -> Value {
    let mut pool = Map::new();
    let mut cloud = Map::new();
    if let Some(obj) = flat.as_object() {
        for (k, v) in obj {
            if v.is_null() {
                continue;
            }
            // Palette stores `skipK8sUpgrade: "disabled"` on every pool, and refuses the field in
            // a create for an engine that does not support it (GKE: `InvalidParam`). A provider
            // sends it when a person sets it, so the entity carries it when it is enabled alone.
            if k == "skipK8sUpgrade" && v.as_str() != Some("enabled") {
                continue;
            }
            if POOL_CONFIG_FIELDS.contains(&k.as_str()) {
                pool.insert(k.clone(), v.clone());
            } else if k == "subnetIds" {
                let subnets: Vec<Value> = v
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(az, id)| json!({"az": az, "id": id}))
                    .collect();
                cloud.insert("subnets".into(), Value::Array(subnets));
            } else {
                cloud.insert(k.clone(), v.clone());
            }
        }
    }
    json!({"cloudConfig": Value::Object(cloud), "poolConfig": Value::Object(pool)})
}

/// Merge `change` into `into`, as [`World::with_change`] says. A fixture of a test merges its
/// patch the same way.
pub fn merge(into: &mut Value, change: &Value) {
    merge_change(into, change)
}

fn merge_change(into: &mut Value, change: &Value) {
    match (into, change) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in b {
                match (a.get_mut(k), v) {
                    (Some(Value::Array(pools)), Value::Array(changes))
                        if k == "machinepoolconfig" =>
                    {
                        merge_list(pools, changes, |p| opt_str(p, "/poolConfig/name"));
                    }
                    // the profiles of an entity carry `uid`; the records of the world carry it
                    // at `source.uid`
                    (Some(Value::Array(profiles)), Value::Array(changes)) if k == "profiles" => {
                        merge_list(profiles, changes, |p| {
                            opt_str(p, "/uid").or_else(|| opt_str(p, "/source/uid"))
                        });
                    }
                    // the variables of a profile carry a `name`, as the pools do
                    (Some(Value::Array(variables)), Value::Array(changes)) if k == "variables" => {
                        merge_list(variables, changes, |v| opt_str(v, "/name"));
                    }
                    (Some(have), v) if have.is_object() && v.is_object() => merge_change(have, v),
                    (_, v) => {
                        a.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (into, change) => *into = change.clone(),
    }
}

/// Merge items by a key: an item with a known key merges into it, a new key is added.
fn merge_list(have: &mut Vec<Value>, changes: &[Value], key: impl Fn(&Value) -> Option<&str>) {
    for c in changes {
        let k = key(c).map(str::to_string);
        match have
            .iter_mut()
            .find(|h| k.is_some() && key(h).map(str::to_string) == k)
        {
            Some(h) => merge_change(h, c),
            None => have.push(c.clone()),
        }
    }
}

/// A record as JSON.
fn record_value(snap: &Snapshot) -> Result<Value> {
    Ok(serde_json::to_value(snap)?)
}

/// Replace every secret field with `<redacted>`, at any depth.
pub fn redact(doc: &mut Value) {
    redact_fields(doc, SECRET_FIELDS);
}

/// [`redact`], and the fields a file on disk must not carry.
pub fn redact_for_output(doc: &mut Value) {
    redact_fields(doc, SECRET_FIELDS);
    redact_fields(doc, OUTPUT_ONLY_FIELDS);
}

fn redact_fields(doc: &mut Value, fields: &[&str]) {
    match doc {
        Value::Object(o) => {
            for k in fields {
                if o.contains_key(*k) {
                    o.insert(k.to_string(), json!("<redacted>"));
                }
            }
            for v in o.values_mut() {
                redact_fields(v, fields);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|v| redact_fields(v, fields)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_world_of_a_cluster_reads_its_cloud_type_as_the_route_of_its_cloud_config_does() {
        // the document names its cloud config, and no spec.cloudType
        let api = strata::api::MemoryApi::new()
            .with(
                "/v1/spectroclusters/c1",
                json!({"metadata": {"uid": "c1", "name": "barney"},
                    "spec": {"cloudConfigRef": {"kind": "aws", "uid": "cc1"}}}),
            )
            .with("/v1/cloudconfigs/aws/cc1", json!({"spec": {}}));
        let world = World::from_cluster(&Palette::new(api), "c1", &Progress::silent()).unwrap();
        assert_eq!(world.cloud, "aws");
        let none = strata::api::MemoryApi::new().with(
            "/v1/spectroclusters/c1",
            json!({"metadata": {"uid": "c1"}, "spec": {}}),
        );
        let err = World::from_cluster(&Palette::new(none), "c1", &Progress::silent()).unwrap_err();
        assert!(err.to_string().contains("names no cloud type"), "{err}");
    }

    #[test]
    fn a_world_of_a_cluster_by_name_reads_the_cluster_that_the_name_names() {
        let api = strata::api::MemoryApi::new()
            .with(
                "/v1/dashboard/spectroclusters/meta",
                json!([{"metadata": {"name": "barney", "uid": "c1"}}]),
            )
            .with(
                "/v1/spectroclusters/c1",
                json!({"metadata": {"uid": "c1", "name": "barney"},
                    "spec": {"cloudConfigRef": {"kind": "aws", "uid": "cc1"}}}),
            )
            .with("/v1/cloudconfigs/aws/cc1", json!({"spec": {}}));
        let world = World::from_cluster(&Palette::new(api), "barney", &Progress::silent()).unwrap();
        assert_eq!(world.cluster_uid.as_deref(), Some("c1"));
        assert_eq!(world.source, "cluster:c1");
        assert_eq!(str_at(&world.entity, "/metadata/name"), "barney");
    }

    #[test]
    fn a_world_of_a_cluster_tries_an_id_of_24_hex_digits_as_a_uid_and_then_as_a_name() {
        let id = "5f1c0b7a9d3e2a4b6c8d0e1f";
        let document = |uid: &str| {
            json!({"metadata": {"uid": uid, "name": "barney"},
                "spec": {"cloudConfigRef": {"kind": "aws", "uid": "cc1"}}})
        };
        let config = json!({"spec": {}});
        // the id is a uid: one request for the document, and no listing
        let by_uid = strata::api::MemoryApi::new()
            .with(format!("/v1/spectroclusters/{id}"), document(id))
            .with(format!("/v1/spectroclusters/{id}/variables"), json!([]))
            .with("/v1/cloudconfigs/aws/cc1", config.clone());
        let world = World::from_cluster(&Palette::new(&by_uid), id, &Progress::silent()).unwrap();
        assert_eq!(world.cluster_uid.as_deref(), Some(id));
        assert_eq!(by_uid.calls(&format!("/v1/spectroclusters/{id}")), 1);
        assert_eq!(by_uid.calls("/v1/dashboard/spectroclusters/meta"), 0);
        // the id is a name: the uid route answers 404, and the listing gives the uid
        let by_name = strata::api::MemoryApi::new()
            .with(
                "/v1/dashboard/spectroclusters/meta",
                json!([{"metadata": {"name": id, "uid": "c1"}}]),
            )
            .with("/v1/spectroclusters/c1", document("c1"))
            .with("/v1/spectroclusters/c1/variables", json!([]))
            .with("/v1/cloudconfigs/aws/cc1", config);
        let world = World::from_cluster(&Palette::new(&by_name), id, &Progress::silent()).unwrap();
        assert_eq!(world.cluster_uid.as_deref(), Some("c1"));
        assert_eq!(by_name.calls(&format!("/v1/spectroclusters/{id}")), 1);
        assert_eq!(by_name.calls("/v1/dashboard/spectroclusters/meta"), 1);
    }

    #[test]
    fn the_edge_engine_is_the_cloud_type_edge_native_in_a_route_an_account_and_a_file() {
        assert_eq!(route_cloud(EDGE), CLOUD_EDGE_NATIVE);
        assert_eq!(account_kind(EDGE), CLOUD_EDGE_NATIVE);
        assert_eq!(normalize_cloud(" Edge-Native "), EDGE);
        assert_eq!(normalize_cloud(EDGE), EDGE);
        for (engine, account) in [
            ("eks", "aws"),
            ("gke", "gcp"),
            ("aks", "azure"),
            ("aws", "aws"),
        ] {
            assert_eq!(account_kind(engine), account, "{engine}");
            assert_eq!(route_cloud(engine), engine, "{engine}");
        }
        assert!(CLOUDS.contains(&EDGE));
        assert!(!CLOUDS.contains(&CLOUD_EDGE_NATIVE));
    }

    #[test]
    fn the_world_gives_the_account_kind_and_the_route_cloud_of_each_engine() {
        for engine in CLOUDS {
            let world = World {
                cloud: engine.to_string(),
                ..World::default()
            }
            .to_value();
            assert_eq!(world["accountKind"], account_kind(engine), "{engine}");
            assert_eq!(world["routeCloud"], route_cloud(engine), "{engine}");
        }
        let mut no_cloud = json!({"entity": {}});
        add_engine_clouds(&mut no_cloud);
        assert_eq!(no_cloud, json!({"entity": {}}));
    }

    #[test]
    fn a_cloud_config_pool_splits_into_the_two_parts_of_the_entity() {
        let flat = json!({"name": "worker-pool", "isControlPlane": false, "size": 2,
            "instanceType": "t3.xlarge", "azs": ["us-east-2a"],
            "subnetIds": {"us-east-2a": "subnet-1"}, "amiType": "AL2023_x86_64_STANDARD"});
        let e = split_pool(&flat);
        assert_eq!(e["poolConfig"]["name"], "worker-pool");
        assert_eq!(e["poolConfig"]["size"], 2);
        assert_eq!(e["cloudConfig"]["instanceType"], "t3.xlarge");
        assert_eq!(e["cloudConfig"]["subnets"][0]["id"], "subnet-1");
        assert_eq!(e["cloudConfig"]["amiType"], "AL2023_x86_64_STANDARD");
        assert!(e["cloudConfig"].get("name").is_none());
    }

    #[test]
    fn a_disabled_skip_k8s_upgrade_stays_out_of_the_entity() {
        // Palette stores "disabled" on each pool, and the GKE validate route refuses the field
        let off = split_pool(&json!({"name": "w", "size": 3, "skipK8sUpgrade": "disabled"}));
        assert!(off["poolConfig"].get("skipK8sUpgrade").is_none());
        let on = split_pool(&json!({"name": "w", "size": 3, "skipK8sUpgrade": "enabled"}));
        assert_eq!(on["poolConfig"]["skipK8sUpgrade"], "enabled");
    }

    #[test]
    fn a_change_merges_pools_by_name_and_keeps_the_rest() {
        let mut entity = json!({"spec": {"cloudConfig": {"region": "us-east-2"},
            "machinepoolconfig": [
                {"cloudConfig": {"instanceType": "t3.xlarge"}, "poolConfig": {"name": "worker-pool", "size": 2}},
                {"cloudConfig": {"instanceType": "t3.xlarge"}, "poolConfig": {"name": "cp", "size": 1}}]}});
        let change = json!({"spec": {"machinepoolconfig": [
            {"cloudConfig": {"instanceType": "m5.xlarge"}, "poolConfig": {"name": "worker-pool"}}]}});
        merge_change(&mut entity, &change);
        let pools = entity["spec"]["machinepoolconfig"].as_array().unwrap();
        assert_eq!(pools.len(), 2);
        assert_eq!(pools[0]["cloudConfig"]["instanceType"], "m5.xlarge");
        assert_eq!(
            pools[0]["poolConfig"]["size"], 2,
            "a field the change omits stays"
        );
        assert_eq!(pools[1]["poolConfig"]["name"], "cp");
        assert_eq!(entity["spec"]["cloudConfig"]["region"], "us-east-2");
    }

    #[test]
    fn a_change_adds_a_pool_with_an_empty_name_and_does_not_merge_it_into_another() {
        let mut entity = json!({"spec": {"machinepoolconfig": [
            {"cloudConfig": {"instanceType": "t3.xlarge"}, "poolConfig": {"name": "", "size": 2}}]}});
        let change = json!({"spec": {"machinepoolconfig": [
            {"cloudConfig": {"instanceType": "m5.xlarge"}, "poolConfig": {"name": "", "size": 3}}]}});
        merge_change(&mut entity, &change);
        let pools = entity["spec"]["machinepoolconfig"].as_array().unwrap();
        assert_eq!(pools.len(), 2);
        assert_eq!(pools[0]["poolConfig"]["size"], 2);
        assert_eq!(pools[1]["poolConfig"]["size"], 3);
    }

    #[test]
    fn the_entity_of_a_cluster_keeps_the_labels_and_the_annotations_that_are_text() {
        let api = strata::api::MemoryApi::new()
            .with(
                "/v1/spectroclusters/c1",
                json!({"metadata": {"uid": "c1", "name": "barney",
                    "labels": {"env": "dev", "n": 1}, "annotations": {"owner": "ops"}},
                    "spec": {"cloudConfigRef": {"kind": "aws", "uid": "cc1"}}}),
            )
            .with("/v1/cloudconfigs/aws/cc1", json!({"spec": {}}));
        let world = World::from_cluster(&Palette::new(api), "c1", &Progress::silent()).unwrap();
        assert_eq!(
            world.entity["metadata"]["labels"],
            json!({"env": "dev"}),
            "a value that is not text is no label"
        );
        assert_eq!(
            world.entity["metadata"]["annotations"],
            json!({"owner": "ops"})
        );
        let bare = strata::api::MemoryApi::new()
            .with(
                "/v1/spectroclusters/c2",
                json!({"metadata": {"uid": "c2", "name": "wilma"},
                    "spec": {"cloudConfigRef": {"kind": "aws", "uid": "cc2"}}}),
            )
            .with("/v1/cloudconfigs/aws/cc2", json!({"spec": {}}));
        let world = World::from_cluster(&Palette::new(bare), "c2", &Progress::silent()).unwrap();
        assert_eq!(world.entity["metadata"]["labels"], json!({}));
        assert_eq!(world.entity["metadata"]["annotations"], json!({}));
    }

    #[test]
    fn the_cloud_comes_from_the_shape_when_it_fits_one() {
        let gke = json!({"spec": {"cloudConfig": {"project": "p", "region": "r", "managedClusterConfig": {"location": "r"}}}});
        assert_eq!(infer_cloud(&gke).as_deref(), Some("gke"));
        let gcp = json!({"spec": {"cloudConfig": {"project": "p", "region": "r"}}});
        assert_eq!(infer_cloud(&gcp).as_deref(), Some("gcp"));
        let eks =
            json!({"spec": {"cloudConfig": {"region": "r", "endpointAccess": {"public": true}}}});
        assert_eq!(infer_cloud(&eks).as_deref(), Some("eks"));
        let aws = json!({"spec": {"cloudConfig": {"region": "r", "sshKeyName": "k"}}});
        assert_eq!(infer_cloud(&aws), None, "aws and eks share this shape");
        let maas = json!({"spec": {"cloudConfig": {"domain": "maas"}}});
        assert_eq!(infer_cloud(&maas).as_deref(), Some("maas"));
        let edge = json!({"spec": {"machinepoolconfig": [{"cloudConfig": {"edgeHosts": []}}]}});
        assert_eq!(infer_cloud(&edge).as_deref(), Some("edge"));
    }

    #[test]
    fn an_envelope_names_the_cloud() {
        let (cloud, entity) =
            unwrap_envelope(json!({"cloudType": "edge-native", "entity": {"spec": {}}}));
        assert_eq!(cloud.as_deref(), Some("edge"));
        assert!(entity.get("spec").is_some());
        let (cloud, entity) = unwrap_envelope(json!({"spec": {"a": 1}}));
        assert_eq!(cloud, None);
        assert_eq!(entity["spec"]["a"], 1);
    }

    #[test]
    fn a_world_of_a_file_needs_a_cloud_it_can_name() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("c.json");
        std::fs::write(&p, r#"{"spec": {"cloudConfig": {"region": "us-east-2"}}}"#).unwrap();
        let err = World::from_file(&p, None).unwrap_err();
        assert!(err.to_string().contains("give --cloud"), "{err}");
        let w = World::from_file(&p, Some("eks")).unwrap();
        assert_eq!(w.cloud, "eks");
        assert_eq!(
            w.to_value()["entity"]["spec"]["cloudConfig"]["region"],
            "us-east-2"
        );
        let err = World::from_file(&p, Some("mars")).unwrap_err();
        assert!(err.to_string().contains("not a cloud"), "{err}");
    }

    #[test]
    fn a_world_never_carries_a_secret() {
        let mut account = json!({"spec": {"credentialType": "sts", "sts": {"arn": "arn:x", "externalId": "e"},
            "secretKey": "s", "nested": [{"password": "p"}]}});
        redact(&mut account);
        let text = account.to_string();
        assert!(!text.contains("\"s\"") && !text.contains("\"p\""), "{text}");
        assert_eq!(account["spec"]["sts"]["arn"], "arn:x");
        assert_eq!(
            account["spec"]["sts"]["externalId"], "e",
            "the validate route needs it at run time"
        );
        redact_for_output(&mut account);
        assert_eq!(account["spec"]["sts"]["externalId"], "<redacted>");
    }

    #[test]
    fn an_account_the_key_cannot_read_is_null_and_a_bad_key_is_an_error() {
        let base = || World {
            cloud: "aws".into(),
            source: "c.json".into(),
            entity: json!({"metadata": {"name": "fred"}, "spec": {"cloudAccountUid": "acc1"}}),
            ..World::default()
        };
        for status in [403u16, 404] {
            let api =
                strata::api::MemoryApi::new().failing("/v1/cloudaccounts/aws/acc1", status, "{}");
            let w = base()
                .with_palette(&Palette::new(&api), "t", &Progress::silent())
                .unwrap();
            assert!(w.account.is_none(), "{status}: the world says null");
        }
        for status in [401u16, 429] {
            let api =
                strata::api::MemoryApi::new().failing("/v1/cloudaccounts/aws/acc1", status, "{}");
            let err = base()
                .with_palette(&Palette::new(&api), "t", &Progress::silent())
                .unwrap_err()
                .to_string();
            assert!(err.contains(&format!("HTTP {status}")), "{status}: {err}");
        }
    }

    #[test]
    fn a_cluster_becomes_a_create_body() {
        let cluster = json!({"metadata": {"uid": "c1", "name": "fred", "labels": {"env": "lab"}},
            "spec": {"cloudType": "eks", "clusterType": "AlloyAssist", "clusterConfig": {"timezone": "Etc/UTC"},
                "clusterProfileTemplates": [{"uid": "p1", "packs": [{"name": "kubernetes-eks", "tag": "1.32", "type": "spectro", "values": "x", "manifests": []}]}]}});
        let config = json!({"spec": {"cloudAccountRef": {"uid": "acc1"},
            "clusterConfig": {"region": "us-east-2", "vpcId": "", "endpointAccess": {"public": true}},
            "machinePoolConfig": [{"name": "worker-pool", "size": 2, "instanceType": "t3.xlarge", "azs": ["us-east-2a", "us-east-2b"]}]}});
        let e = entity_of(&cluster, &config, Some("acc1"), &json!([]));
        assert_eq!(e["metadata"]["name"], "fred");
        assert_eq!(e["spec"]["cloudAccountUid"], "acc1");
        assert_eq!(e["spec"]["cloudConfig"]["region"], "us-east-2");
        assert_eq!(e["spec"]["machinepoolconfig"][0]["poolConfig"]["size"], 2);
        assert_eq!(
            e["spec"]["machinepoolconfig"][0]["cloudConfig"]["azs"][1],
            "us-east-2b"
        );
        assert_eq!(e["spec"]["profiles"][0]["uid"], "p1");
        assert_eq!(
            e["spec"]["profiles"][0]["packValues"][0]["name"],
            "kubernetes-eks"
        );
        assert_eq!(e["spec"]["clusterType"], "AlloyAssist");
        assert_eq!(e["spec"]["profiles"][0]["variables"], json!([]));
    }

    /// A cluster of two profiles, and the answer of its variables route: the first profile
    /// declares two variables, one of them sensitive, and the second declares none.
    fn cluster_api(variables: serde_json::Value) -> strata::api::MemoryApi {
        strata::api::MemoryApi::new()
            .with(
                "/v1/spectroclusters/c1",
                json!({"metadata": {"uid": "c1", "name": "barney"},
                    "spec": {"cloudConfigRef": {"kind": "aws", "uid": "cc1"},
                        "clusterProfileTemplates": [{"uid": "p1", "packs": []}, {"uid": "p2", "packs": []}]}}),
            )
            .with("/v1/cloudconfigs/aws/cc1", json!({"spec": {}}))
            .with("/v1/spectroclusters/c1/variables", variables)
    }

    #[test]
    fn the_entity_of_a_cluster_carries_the_variable_values_of_each_profile() {
        let api = cluster_api(json!([{"profileUid": "p1", "variables": [
            {"name": "pod_cidr", "value": "10.244.0.0/16", "format": "string", "required": true},
            {"name": "db_password", "value": "********", "isSensitive": true}]}]));
        let world = World::from_cluster(&Palette::new(api), "c1", &Progress::silent()).unwrap();
        let profiles = &world.entity["spec"]["profiles"];
        assert_eq!(
            profiles[0]["variables"],
            json!([
                {"name": "pod_cidr", "value": "10.244.0.0/16"},
                {"name": "db_password", "value": "********"}
            ]),
            "the name and the value, and a sensitive value as Palette masks it"
        );
        assert_eq!(
            profiles[1]["variables"],
            json!([]),
            "a profile with no entry has no variable"
        );
    }

    #[test]
    fn a_cluster_on_a_palette_with_no_variables_route_sends_no_variable() {
        let api = cluster_api(json!([]));
        let older = cluster_api(json!([])).failing("/v1/spectroclusters/c1/variables", 405, "{}");
        for api in [api, older] {
            let world = World::from_cluster(&Palette::new(api), "c1", &Progress::silent()).unwrap();
            assert_eq!(world.entity["spec"]["profiles"][0]["variables"], json!([]));
        }
    }

    #[test]
    fn a_variables_route_that_fails_stops_the_world_of_a_cluster() {
        let api = cluster_api(json!([])).failing("/v1/spectroclusters/c1/variables", 500, "boom");
        let err = World::from_cluster(&Palette::new(api), "c1", &Progress::silent()).unwrap_err();
        assert!(err.to_string().contains("HTTP 500"), "{err}");
    }

    #[test]
    fn a_change_merges_the_variables_of_a_profile_by_name_and_keeps_the_rest() {
        let mut entity = json!({"spec": {"profiles": [{"uid": "p1", "variables": [
            {"name": "pod_cidr", "value": "10.244.0.0/16"},
            {"name": "owner", "value": "platform"}]}]}});
        let change = json!({"spec": {"profiles": [{"uid": "p1", "variables": [
            {"name": "pod_cidr", "value": "10.0.0.0/8"},
            {"name": "region", "value": "eu-west-3"}]}]}});
        merge_change(&mut entity, &change);
        assert_eq!(
            entity["spec"]["profiles"][0]["variables"],
            json!([
                {"name": "pod_cidr", "value": "10.0.0.0/8"},
                {"name": "owner", "value": "platform"},
                {"name": "region", "value": "eu-west-3"}
            ]),
            "a variable of the change replaces its value, a new name is added, the rest stays"
        );
    }
}
