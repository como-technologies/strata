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
//!   "routeCloud": "edge-native",       the cloud type of the engine in a route: edge has edge-native
//!   "edgeHosts": [ {pool, host} ],     each edge host of each pool, with the poolConfig of its pool
//!   "validateBody": { metadata, spec } the body that the checks send to the validate route
//! }
//! ```
//!
//! `accountKind` and `routeCloud` come from the engine, by [`account_kind`] and [`route_cloud`], so
//! that no rule writes that mapping again. `edgeHosts` and `validateBody` come from the entity.
//! The rules that read each host share one program, and so do the rules that ask the validate
//! route.
//!
//! A check adds `response` (Palette), `exec` (a command), or `cluster`, `summary`, `events`,
//! and `elapsed` (the watch). A fixture of a test is a world with the answers written in, under
//! `_responses` and `_exec`; see [`crate::probe::FixtureBackend`].

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Map, Value, json};
use strata::api::{Palette, PaletteApi, RawCluster, profile_variables};
use strata::closed_set::ClosedSet;
use strata::cloud::{CLOUD_EDGE_NATIVE, Cloud};
use strata::json::{arr, opt_str, opt_text, str_at, text_map, text_or, value_at, value_or_null};
use strata::reference::Ref;
use strata::snapshot::{AppliedProfile, Snapshot, SubjectKind};
use strata::term::progress::Progress;

use crate::error::{Error, Result};

/// An engine: the kind of cluster that a run checks.
///
/// Each engine is a cloud type of Palette, except `edge`, which Palette names
/// [`CLOUD_EDGE_NATIVE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// A cloud type of Palette: the engine has its text.
    Cloud(Cloud),
    /// An edge cluster. Its engine is `edge`.
    Edge,
}

impl Engine {
    /// The text of the engine, as a person and a rule file write it.
    pub fn as_str(self) -> &'static str {
        match self {
            Engine::Cloud(cloud) => cloud.as_str(),
            Engine::Edge => EDGE,
        }
    }

    /// The engine that `text` names, or none.
    pub fn parse(text: &str) -> Option<Engine> {
        ENGINES.into_iter().find(|e| e.as_str() == text)
    }

    /// The cloud type that a route of Palette names for the engine: `edge` is
    /// [`CLOUD_EDGE_NATIVE`].
    pub fn cloud(self) -> Cloud {
        match self {
            Engine::Cloud(cloud) => cloud,
            Engine::Edge => Cloud::EdgeNative,
        }
    }
}

/// The engines a cluster can be built on, in the order of the help screen. OpenStack is not one:
/// Palette removed it in 2026, and the API has no route for it.
///
/// A rule names an engine too: see [`crate::rule::engines`].
pub const ENGINES: [Engine; 9] = [
    Engine::Cloud(Cloud::Aws),
    Engine::Cloud(Cloud::Eks),
    Engine::Cloud(Cloud::Gcp),
    Engine::Cloud(Cloud::Gke),
    Engine::Cloud(Cloud::Azure),
    Engine::Cloud(Cloud::Aks),
    Engine::Cloud(Cloud::Maas),
    Engine::Edge,
    Engine::Cloud(Cloud::Vsphere),
];

/// The text of each engine: `aws`, `eks`, and so on, then `edge` and `vsphere` in their place.
pub fn clouds() -> impl Iterator<Item = &'static str> {
    ENGINES.into_iter().map(Engine::as_str)
}

/// The text of each engine, as a list for a message.
pub fn cloud_list() -> String {
    clouds().collect::<Vec<_>>().join(", ")
}

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
        add_derived_fields(&mut world);
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
                    cloud_list()
                ),
            })?,
        };
        if Engine::parse(&cloud).is_none() {
            return Err(Error::Input {
                message: format!(
                    "'{cloud}' is not a cloud strata-preflight knows: {}",
                    cloud_list()
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
        // a Palette with no variables route gives an empty answer, and the profiles then send none
        let variables = palette.cluster_variables(uid, progress)?.answer;
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

/// The cloud account kind of an engine: a managed engine uses the account of its cloud. Text that
/// names no engine stays as it is.
pub fn account_kind(cloud: &str) -> &str {
    Engine::parse(cloud).map_or(cloud, |engine| engine.cloud().account().as_str())
}

/// The name Palette gives an engine in a route: `edge` is [`CLOUD_EDGE_NATIVE`]. Text that names
/// no engine stays as it is.
pub fn route_cloud(cloud: &str) -> &str {
    Engine::parse(cloud).map_or(cloud, |engine| engine.cloud().as_str())
}

/// The suffix that the body of the validate route gives to the name of an existing cluster. The
/// route refuses a name that Palette holds already, with `ResourceAlreadyExists`, and the cluster
/// holds its own.
const VALIDATE_NAME_SUFFIX: &str = "-preflight";

/// Put the fields that the world derives in the world `world`: `accountKind` and `routeCloud`
/// from its `cloud`, then `edgeHosts` and `validateBody` from its `entity`. [`World::to_value`]
/// writes them, and the loader of a fixture, which is a world on disk, adds them here. A world
/// with no `cloud` gets no field of the engine.
pub(crate) fn add_derived_fields(world: &mut Value) {
    if let Some(cloud) = opt_text(world, "/cloud") {
        world["accountKind"] = json!(account_kind(&cloud));
        world["routeCloud"] = json!(route_cloud(&cloud));
    }
    world["edgeHosts"] = edge_hosts(value_at(world, "/entity"));
    world["validateBody"] = validate_body(world);
}

/// Each edge host of each pool of `entity`, with the pool that holds it: `{pool, host}`, where
/// `pool` is the `poolConfig` of the pool. The order is that of the entity. The rules that read
/// each host of the cluster read this one list.
fn edge_hosts(entity: &Value) -> Value {
    arr(entity, "/spec/machinepoolconfig")
        .iter()
        .flat_map(|pool| {
            let config = value_or_null(pool, "/poolConfig");
            arr(pool, "/cloudConfig/edgeHosts")
                .iter()
                .map(move |host| json!({"pool": config, "host": host}))
        })
        .collect()
}

/// The body that the checks send to the validate route of the cluster: the entity. A world of an
/// existing cluster has the name of the cluster with [`VALIDATE_NAME_SUFFIX`]; nothing else
/// changes. Each check that asks the route sends this one body, so the run sends one request.
fn validate_body(world: &Value) -> Value {
    let mut body = value_or_null(world, "/entity");
    if !value_at(world, "/clusterUid").is_null() {
        let name = text_or(&body, "/metadata/name", "cluster");
        body["metadata"]["name"] = json!(format!("{name}{VALIDATE_NAME_SUFFIX}"));
    }
    body
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
    infer_engine(entity).map(|engine| engine.as_str().to_string())
}

/// The engine that the shape of `spec.cloudConfig` fits, when it fits one alone.
fn infer_engine(entity: &Value) -> Option<Engine> {
    let spec = entity.get("spec")?;
    let cc = spec.get("cloudConfig");
    let has = |k: &str| cc.and_then(|c| c.get(k)).is_some_and(|v| !v.is_null());
    let pool_has = |k: &str| {
        arr(spec, "/machinepoolconfig")
            .iter()
            .any(|p| p.pointer(&format!("/cloudConfig/{k}")).is_some())
    };
    if has("managedClusterConfig") {
        return Some(Engine::Cloud(Cloud::Gke));
    }
    if has("project") {
        return Some(Engine::Cloud(Cloud::Gcp));
    }
    if has("subscriptionId") || has("location") && has("resourceGroup") {
        return Some(Engine::Cloud(
            if pool_has("isSystemNodePool")
                || spec
                    .pointer("/machinepoolconfig/0/managedPoolConfig")
                    .is_some()
            {
                Cloud::Aks
            } else {
                Cloud::Azure
            },
        ));
    }
    if has("domain") || pool_has("resourcePool") {
        return Some(Engine::Cloud(Cloud::Maas));
    }
    if pool_has("edgeHosts")
        || has("controlPlaneEndpoint")
            && !has("placement")
            && cc.and_then(|c| c.get("ntpServers")).is_some()
            && spec.get("cloudAccountUid").is_none()
    {
        return Some(Engine::Edge);
    }
    if has("placement") || pool_has("placements") {
        return Some(Engine::Cloud(Cloud::Vsphere));
    }
    if has("endpointAccess")
        || has("encryptionConfig")
        || has("addons")
        || spec.get("fargateProfiles").is_some()
        || pool_has("amiType")
        || pool_has("awsLaunchTemplate")
    {
        return Some(Engine::Cloud(Cloud::Eks));
    }
    // a plain AWS entity and a plain EKS entity share a shape: region, vpcId, sshKeyName
    None
}

/// The entry of `packValues` in the body of a create for `pack`, a pack of a template: the pack as
/// the template holds it, and the name and the uid of each of its manifests.
fn pack_entry(pack: &Value) -> Value {
    let mut entry = AppliedProfile::template_pack_value(pack);
    entry["manifests"] = arr(pack, "/manifests")
        .iter()
        .map(|m| json!({"name": value_at(m, "/name"), "uid": value_at(m, "/uid")}))
        .collect();
    entry
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
                "packValues": arr(t, "/packs").iter().map(pack_entry).collect::<Vec<_>>(),
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
mod tests;
