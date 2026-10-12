//! The snapshot document (schema strata-snapshot/1): one profile version with its root,
//! per-pack hashes, canonical values and raw contents. Byte-compatible with the snapshots
//! written by the bash reference implementation.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::api::mask::MASKED;
use crate::api::{RawCluster, RawProfile};
use crate::canon::{canon_yaml_hashed, canonical_json, hash_yaml, sha256_hex};
use crate::error::{Error, Result};
use crate::json::{arr, first_nonempty, opt_text, str_at, strings, text};
pub use crate::repave::PendingRepave;
use crate::repave::is_pending;
pub use crate::validation::ProfileValidation;

/// Schema id written into every snapshot.
pub const SNAPSHOT_SCHEMA: &str = "strata-snapshot/1";

/// The template of a profile that a record reads, as `source.template` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Template {
    /// The published template of a profile version.
    Published,
    /// The draft template of a profile version: of `<ref>@draft`, or of a version that was never
    /// published.
    Draft,
    /// The templates that a cluster applies: a record of a cluster, of a cluster group, or of an
    /// edge host.
    Applied,
    /// A candidate file, resolved against the registries. Palette holds no such template.
    Candidate,
    /// One pack version of a registry: a pack record.
    Registry,
}

impl Template {
    /// The template as the documents name it: `published`, `draft`, `applied`, `candidate`, or
    /// `registry`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Published => "published",
            Self::Draft => "draft",
            Self::Applied => "applied",
            Self::Candidate => "candidate",
            Self::Registry => "registry",
        }
    }
}

impl fmt::Display for Template {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The version of the content digest encoding. It is in the input of the digest, so a change to
/// what the digest covers is a change of this tag, and a digest of one version is never equal to
/// a digest of another. A tool that pins a version by content digest depends on this tag.
pub const CONTENT_DIGEST: &str = "content/1";

/// The annotation of a pack ref that names the machine image of an OS pack, such as an AMI. A
/// bring-your-own-image OS pack holds it. The OS pack `ubuntu-aws` of the Public Repo does not.
pub const IMAGE_ID_ANNOTATION: &str = "imageId";

/// One published profile version: its root, per-pack hashes, canonical values, and raw contents.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// The schema id of this document
    #[schemars(extend("const" = SNAPSHOT_SCHEMA))]
    pub schema: String,
    /// Which tool wrote the snapshot.
    pub tool: Tool,
    /// Where it came from.
    pub source: Source,
    /// Profile variables, normalized and sorted by name
    pub variables: Vec<Value>,
    /// sha256 of the canonical JSON of `variables`
    pub variables_hash: String,
    /// Packs in profile order
    pub packs: Vec<Pack>,
    /// sha256 of canonical JSON {"packs": [[key, node]...], "variables": variablesHash}
    pub root: String,
    /// The content digest, as [`Snapshot::content_digest`] gives it. A record from before this
    /// field has none, so read the digest with the function and not from this field.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    content_digest: String,
}

/// One pack that a profile version takes from a registry: the reference that another tool
/// pins by. See [`Snapshot::pins`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinRef {
    /// Pack name.
    pub name: String,
    /// Resolved version.
    pub version: String,
    /// The digest of the pack in its registry, as Palette stores it. A Helm digest is bare hex.
    pub digest: String,
}

/// Which tool wrote a document.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Tool {
    /// Always "strata".
    pub name: String,
    /// Crate version.
    pub version: String,
    /// "rust" or "bash"; absent in snapshots from the first bash release
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "impl")]
    pub implementation: Option<String>,
}

impl Tool {
    /// This build of strata, as every document it writes records it.
    pub fn current() -> Tool {
        Tool {
            name: crate::TOOL_NAME.to_string(),
            version: crate::TOOL_VERSION.to_string(),
            implementation: Some(crate::TOOL_IMPL.to_string()),
        }
    }
}

/// Where a snapshot came from.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    /// The ref the snapshot was taken from, as given on the command line
    #[serde(rename = "ref")]
    pub reference: String,
    /// Profile uid.
    pub uid: String,
    /// The Palette project the snapshot was taken in; empty for tenant scope. `verify` checks the
    /// record in this project unless told otherwise.
    #[serde(default)]
    pub project_uid: String,
    /// Profile name.
    pub name: String,
    /// Profile version string.
    pub version: String,
    /// Cloud type of the template, for example `aws`.
    pub cloud_type: String,
    /// Profile type: cluster, infra, add-on, system
    #[serde(rename = "type")]
    pub kind: String,
    /// The template that the packs were read from: see [`Template`].
    pub template: Template,
    /// Profile labels.
    #[serde(default)]
    pub tags: BTreeMap<String, String>,
    /// Clusters using this profile version at snapshot time. Absent when no request asked: a
    /// candidate is not in Palette, and a cluster record and a pack record are not a version that
    /// a cluster can use. `0` there would state a count that nothing counted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumers: Option<Consumers>,
    /// True when Palette did not serve the variables endpoint, so the snapshot was taken as if
    /// the version had no variables. The root is the root of a version with no variables, so a
    /// record with this flag does not compare equal to one taken from a Palette that serves the
    /// endpoint. The flag is the only thing that tells the two apart.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub variables_unavailable: bool,
    /// When the snapshot was taken, RFC 3339 UTC.
    pub fetched_at: String,
    /// Present when the snapshot records what a cluster runs rather than a published profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster: Option<ClusterSource>,
    /// Present when the snapshot records one pack version from a registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<PackSource>,
    /// Present after `snapshot --validate`: what Palette answered when strata asked whether it
    /// takes the packs of this content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette_validation: Option<ProfileValidation>,
    /// With `snapshot --macros` on a profile version: each user macro that the recorded content
    /// uses, with a digest. The digest is the `sha256:` of the value that Palette gives the macro
    /// at the scope of the profile, or `unresolved` when Palette gives no value. A record of a
    /// cluster holds them in `cluster.macroValues`. Outside the root; `verify` reads the values
    /// again and compares.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macro_values: Option<BTreeMap<String, String>>,
}

/// The registry pack a snapshot was taken from.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackSource {
    /// Pack name.
    pub name: String,
    /// Pack version.
    pub version: String,
    /// Uid of the registry the pack was read from.
    pub registry_uid: String,
    /// Name of that registry, when Palette lists one.
    pub registry_name: String,
    /// Uid of the pack version in the registry.
    pub pack_uid: String,
}

impl Source {
    /// True for a record of `<ref>@draft`: the draft of a version, which its publish makes the
    /// published template. A record of a version that was never published reads its draft too,
    /// with no `@draft` in its ref, and is not one.
    pub fn is_draft(&self) -> bool {
        self.template == Template::Draft && self.reference.ends_with(crate::reference::DRAFT_SUFFIX)
    }

    /// `name@version`, and `on cluster <name>` for a snapshot of a cluster, or `pending for cluster
    /// <name>` for the update that waits for it.
    pub fn title(&self) -> String {
        let profile = format!("{}@{}", self.name, self.version);
        if self.pack.is_some() {
            return format!("pack {profile}");
        }
        match &self.cluster {
            None => profile,
            Some(c) if self.name.is_empty() && !c.kind.is_cluster() => {
                format!("{} {}", c.kind.noun(), c.name)
            }
            Some(c) if c.kind == SubjectKind::EdgeHost => {
                format!("{profile} on edge host {}", c.name)
            }
            Some(c) if c.kind == SubjectKind::ClusterGroup => {
                format!("{profile} pinned by cluster group {}", c.name)
            }
            Some(c) if self.name.is_empty() && c.pending => format!("cluster {} pending", c.name),
            Some(c) if self.name.is_empty() => format!("cluster {}", c.name),
            Some(c) if c.pending => format!("{profile} pending for cluster {}", c.name),
            Some(c) => format!("{profile} on cluster {}", c.name),
        }
    }

    /// The digests of the macro values that the record holds: those of the cluster for a cluster
    /// record, else those of the profile. `None` when the record was taken without `--macros`.
    pub fn macro_values(&self) -> Option<&BTreeMap<String, String>> {
        match &self.cluster {
            Some(c) => c.macro_values.as_ref(),
            None => self.macro_values.as_ref(),
        }
    }

    /// The uid a reader looks up: the cluster's for a cluster snapshot, else the profile's.
    pub fn shown_uid(&self) -> &str {
        match (&self.pack, &self.cluster) {
            (Some(p), _) => &p.pack_uid,
            (None, Some(c)) => &c.uid,
            (None, None) => &self.uid,
        }
    }
}

/// The `source.kind` of a record of an app profile: its tiers are its packs.
pub const APP_KIND: &str = "app";

/// The kind of object that a cluster record was taken from. Each holds profiles in
/// `spec.clusterProfileTemplates`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum SubjectKind {
    /// A cluster: the copies of the profiles that it runs.
    #[default]
    Cluster,
    /// An edge host: the profiles bound to it.
    EdgeHost,
    /// A cluster group: the profiles that it pins for its virtual clusters.
    ClusterGroup,
}

impl SubjectKind {
    /// True for a cluster.
    pub fn is_cluster(&self) -> bool {
        *self == SubjectKind::Cluster
    }

    /// The noun that names the kind in a report.
    pub fn noun(&self) -> &'static str {
        match self {
            SubjectKind::Cluster => "cluster",
            SubjectKind::EdgeHost => "edge host",
            SubjectKind::ClusterGroup => "cluster group",
        }
    }
}

/// A virtual cluster of a cluster group.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupMember {
    /// Cluster uid.
    pub uid: String,
    /// Cluster name.
    pub name: String,
    /// The project of the cluster; empty when Palette does not name it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub project_uid: String,
}

/// The cluster a snapshot was taken from.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterSource {
    /// Cluster uid.
    pub uid: String,
    /// Cluster name.
    pub name: String,
    /// The applied profile selected with `cluster:<uid>/<profile>`; absent when the snapshot
    /// covers every applied profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// The applied profiles this snapshot covers, in the canonical order of
    /// [`AppliedProfile::of_cluster`]: the cluster or infra profile first, then the others by
    /// name.
    pub profiles: Vec<AppliedProfile>,
    /// The values the cluster gives the variables of those profiles, in profile order and then by
    /// name. Outside the root: two clusters on one release can differ here. A sensitive variable
    /// is listed without its value.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<ClusterVariable>,
    /// Whether Palette says that an update waits for the cluster: an open notification of an
    /// update of a profile that this record covers. Absent when the notifications were not read.
    /// Outside the root: it is Palette's statement about the copy, not the copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updates_pending: Option<bool>,
    /// Why the notifications could not be read, when `updatesPending` is absent for that reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updates_pending_error: Option<String>,
    /// Each change that waits, as the open notifications name it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_updates: Vec<PendingUpdate>,
    /// Whether Palette says that a repave of the cluster waits for approval: `status.repave.state`
    /// is `Pending`. Absent for an edge host and a cluster group, and in an older record. Outside
    /// the root: it is a statement of Palette about the cluster, not the copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repave_pending: Option<bool>,
    /// The repave that waits, with the reasons that Palette gives for it. Absent when no repave
    /// waits, and when strata could not read it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_repave: Option<PendingRepave>,
    /// Why strata could not read the repave that waits, when `pendingRepave` is absent for that
    /// reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_repave_error: Option<String>,
    /// With `snapshot --macros`: each user macro the recorded content uses, and the `sha256:` of
    /// the value Palette gives it for this cluster, or `unresolved`. A value can be a secret, so
    /// the record holds its digest and not the value. Outside the root; `verify` reads the values
    /// again and compares.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macro_values: Option<BTreeMap<String, String>>,
    /// True for a record of `cluster:<uid>@pending`: the profiles that the cluster would run after
    /// it accepts the update that waits, and not the copies that it runs now.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pending: bool,
    /// What the record was taken from: absent for a cluster, `edgeHost` for the profiles bound to
    /// an edge host, and `clusterGroup` for the profiles that a cluster group pins for its
    /// virtual clusters. `uid` and `name` are those of that object.
    #[serde(default, skip_serializing_if = "SubjectKind::is_cluster")]
    pub kind: SubjectKind,
    /// For a cluster group: its virtual clusters, which run the profiles that it pins. Outside
    /// the root.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<GroupMember>,
    /// The labels of the cluster, as Palette gives them in `metadata.labels`. Outside the root.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
    /// More of the cluster than its release, by tier: `applied`, `running`, `policy`, and, with
    /// `snapshot --shape`, `shape`. A cluster group carries `policy` alone: the config that it
    /// gives each virtual cluster. Each tier has its own hash. A record names the tiers that it
    /// carries by carrying them, so a record without one still verifies. Outside the root.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tiers: BTreeMap<Tier, ClusterTier>,
}

/// A tier of a cluster record: a part of the cluster beside its release, with a hash of its own.
/// The variants are in the order of their names. So the map of the tiers of a record, and its
/// JSON, keep the order of a record of strata 0.21.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// The hashes of the spec that Palette applied to the cluster.
    Applied,
    /// The labels of the cluster. Only `compare` gives this tier: a record holds the labels in
    /// `labels`, and carries no tier of this name.
    Labels,
    /// The config of the cluster, or the config that a cluster group gives each virtual cluster.
    Policy,
    /// The pack version that the cluster reconciled, for each pack of a covered profile.
    Running,
    /// The machine pools and the cloud part of the cluster, with `snapshot --shape`.
    Shape,
}

impl Tier {
    /// The tier as the documents name it: `applied`, `labels`, `policy`, `running`, or `shape`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Labels => "labels",
            Self::Policy => "policy",
            Self::Running => "running",
            Self::Shape => "shape",
        }
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One tier of a cluster record: the fields that Palette gives for it, and their hash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterTier {
    /// sha256 of the canonical JSON of `fields`.
    pub hash: String,
    /// What the tier covers, as Palette gives it.
    pub fields: Value,
    /// Each field that strata could not read, with why: `rbacs` and `HTTP 403: <body>`. `fields`
    /// does not hold such a field, and `compare` and `verify` do not compare it. The errors are
    /// not part of the hash either. A record of strata 0.19 holds the field as null.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub errors: BTreeMap<String, String>,
}

impl ClusterTier {
    /// A tier of `fields` that strata read in full.
    fn of(fields: Value) -> ClusterTier {
        ClusterTier::with_errors(fields, BTreeMap::new())
    }

    /// A tier of `fields` less each field that `errors` names: a field that strata could not
    /// read, with why.
    ///
    /// An error is not a value of the cluster, so the record does not hold the field, and the
    /// hash does not cover it.
    fn with_errors(mut fields: Value, errors: BTreeMap<String, String>) -> ClusterTier {
        if let Some(m) = fields.as_object_mut() {
            for key in errors.keys() {
                m.remove(key);
            }
        }
        ClusterTier {
            hash: sha256_hex(&canonical_json(&fields)),
            fields,
            errors,
        }
    }
}

/// The tiers of a cluster record, from the raw documents in `raw`.
///
/// `raw` holds the document of the cluster or the cluster group. For a cluster, it also holds the
/// role bindings, the namespaces, and the cloud config when strata read them. `covered` limits
/// `running` to the profiles that the record covers. The record carries a tier only when Palette
/// gives the part of the document that the tier reads.
pub fn cluster_tiers(raw: &RawCluster, covered: &[AppliedProfile]) -> BTreeMap<Tier, ClusterTier> {
    let cluster = &raw.cluster;
    let mut tiers = BTreeMap::new();
    // Palette's own hashes of the spec that it applied to the cluster
    if let Some(apply) = cluster.pointer("/status/spcApply") {
        tiers.insert(
            Tier::Applied,
            ClusterTier::of(json!({
                "spcHash": text(apply, "/spcHash"),
                "spcInfraHash": text(apply, "/spcInfraHash"),
            })),
        );
    }
    // the pack version that the cluster reconciled, for each pack of a covered profile
    if cluster
        .pointer("/status/packs")
        .is_some_and(Value::is_array)
    {
        tiers.insert(Tier::Running, running_tier(cluster, covered));
    }
    if let Some(config) = cluster.pointer("/spec/clusterConfig") {
        tiers.insert(
            Tier::Policy,
            ClusterTier::with_errors(
                crate::policy::fields(raw, config),
                crate::policy::errors(raw),
            ),
        );
    }
    // the document of a cluster group has spec.clustersConfig, and no spec.clusterConfig
    if let Some(config) = cluster.pointer("/spec/clustersConfig") {
        tiers.insert(
            Tier::Policy,
            ClusterTier::of(crate::policy::group_fields(config)),
        );
    }
    if let Some(cc) = &raw.cloud_config {
        tiers.insert(
            Tier::Shape,
            ClusterTier::of(crate::shape::fields(cluster, cc)),
        );
    }
    tiers
}

fn running_tier(cluster: &Value, covered: &[AppliedProfile]) -> ClusterTier {
    let mut running: Vec<Value> = arr(cluster, "/status/packs")
        .iter()
        .filter(|p| covered.iter().any(|c| c.uid == text(p, "/profileUid")))
        .map(|p| {
            json!({"name": text(p, "/name"), "version": text(p, "/version"), "profileUid": text(p, "/profileUid")})
        })
        .collect();
    running.sort_by_key(|p| (text(p, "/profileUid"), text(p, "/name")));
    ClusterTier::of(Value::Array(running))
}

/// The digest of each macro in `names` that `resolved` gives for one of `profiles`:
/// `sha256:<hex>` of the value, or `unresolved` when no profile has it. `resolved` is the answer
/// for a cluster or a cluster group, with one map for each profile.
pub fn macro_digests(
    resolved: &Value,
    profiles: &[AppliedProfile],
    names: &[String],
) -> BTreeMap<String, String> {
    let maps: Vec<&Value> = arr(resolved, "/profiles")
        .iter()
        .filter(|p| profiles.iter().any(|a| str_at(p, "/uid") == a.uid))
        .map(|p| &p["resolved"])
        .collect();
    digests_in(&maps, names)
}

/// The digest of each macro in `names` that the answer for one profile version gives: its one
/// `resolved` map, as in [`macro_digests`].
fn profile_macro_digests(resolved: &Value, names: &[String]) -> BTreeMap<String, String> {
    digests_in(&[&resolved["resolved"]], names)
}

/// The digest of each macro in `names`, from the first of `maps` that gives it a text value.
fn digests_in(maps: &[&Value], names: &[String]) -> BTreeMap<String, String> {
    names
        .iter()
        .map(|name| {
            let key = macro_placeholder(name);
            let digest = maps
                .iter()
                .find_map(|m| m.get(&key).and_then(Value::as_str))
                .map(|v| format!("sha256:{}", sha256_hex(v)))
                .unwrap_or_else(|| "unresolved".to_string());
            (name.clone(), digest)
        })
        .collect()
}

/// One change that waits for a cluster, from an open notification of Palette.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PendingUpdate {
    /// The uid of the profile whose change waits.
    pub profile_uid: String,
    /// The name of that profile, as the cluster's copy names it.
    pub profile: String,
    /// What Palette says changed, such as `Manifest 'x' is updated in the pack 'x(x)'`.
    pub message: String,
    /// When Palette made the notification.
    pub since: String,
}

/// The notification type of an update of a profile that waits for a cluster.
const PROFILE_UPDATE_ACTION: &str = "NotifyActionClusterProfileUpdate";

impl PendingUpdate {
    /// The open updates in `notifications` that concern one of `covered`, oldest first, and one
    /// line for each change a notification names. Palette marks a notification done when the
    /// cluster's copy takes the change, whether or not the request that pushed it resolved it.
    pub fn read(notifications: &Value, covered: &[AppliedProfile]) -> Vec<PendingUpdate> {
        let mut out = Vec::new();
        for n in notifications.as_array().into_iter().flatten().rev() {
            let open = str_at(n, "/action/actionType") == PROFILE_UPDATE_ACTION
                && n.pointer("/action/isDone").and_then(Value::as_bool) != Some(true);
            if !open {
                continue;
            }
            let events = n.pointer("/action/events").and_then(Value::as_object);
            for e in events.into_iter().flat_map(|m| m.values()) {
                let uid = text(e, "/profileUid");
                let Some(p) = covered.iter().find(|p| p.uid == uid) else {
                    continue;
                };
                out.push(PendingUpdate {
                    profile_uid: uid,
                    profile: p.name.clone(),
                    message: text(e, "/message"),
                    since: text(n, "/metadata/creationTimestamp"),
                });
            }
        }
        out
    }
}

/// The value a cluster gives one profile variable.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterVariable {
    /// Name of the profile the variable belongs to.
    pub profile: String,
    /// Uid of that profile.
    pub profile_uid: String,
    /// Variable name.
    pub name: String,
    /// The value; absent for a sensitive variable, which no document records, not even hashed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// True for a sensitive variable.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sensitive: bool,
}

impl ClusterVariable {
    /// The values the cluster in `raw` gives the variables of `profiles`, in that order and then
    /// by name.
    fn read(raw: &RawCluster, profiles: &[AppliedProfile]) -> Vec<ClusterVariable> {
        let entries = raw.variables.as_array().cloned().unwrap_or_default();
        let mut out = Vec::new();
        for p in profiles {
            let mut of_profile: Vec<ClusterVariable> = entries
                .iter()
                .filter(|e| text(e, "/profileUid") == p.uid)
                .flat_map(|e| arr(e, "/variables"))
                .map(|v| {
                    let sensitive = v.get("isSensitive").and_then(Value::as_bool) == Some(true);
                    ClusterVariable {
                        profile: p.name.clone(),
                        profile_uid: p.uid.clone(),
                        name: text(v, "/name"),
                        value: (!sensitive).then(|| text(v, "/value")),
                        sensitive,
                    }
                })
                .collect();
            of_profile.sort_by(|a, b| a.name.cmp(&b.name));
            out.extend(of_profile);
        }
        out
    }
}

/// One cluster profile version as applied to a cluster.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct AppliedProfile {
    /// Profile uid.
    pub uid: String,
    /// Profile name.
    pub name: String,
    /// Profile version.
    pub version: String,
    /// Profile type: cluster, infra, add-on, system.
    #[serde(rename = "type")]
    pub kind: String,
}

impl AppliedProfile {
    /// The profiles a cluster document applies, in canonical order: the cluster or infra
    /// profile first, then the others by name. Palette lists them in
    /// `spec.clusterProfileTemplates` of `GET /v1/spectroclusters/{uid}` in an order that
    /// differs between two clusters built from one definition, so that order says nothing.
    pub fn of_cluster(doc: &Value) -> Vec<AppliedProfile> {
        AppliedProfile::templates_of(doc)
            .into_iter()
            .map(|(_, applied)| applied)
            .collect()
    }

    /// Each template of `spec.clusterProfileTemplates` with the profile it applies, in
    /// canonical order. Two profiles of one type are by name, then by version, then by uid.
    fn templates_of(doc: &Value) -> Vec<(Value, AppliedProfile)> {
        let mut pairs: Vec<(Value, AppliedProfile)> = arr(doc, "/spec/clusterProfileTemplates")
            .iter()
            .map(|t| (t.clone(), AppliedProfile::from_template(t)))
            .collect();
        pairs.sort_by(|(_, a), (_, b)| {
            a.rank()
                .cmp(&b.rank())
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.version.cmp(&b.version))
                .then_with(|| a.uid.cmp(&b.uid))
        });
        pairs
    }

    /// The cluster or infra profile comes first: a cluster runs it before any add-on.
    fn rank(&self) -> u8 {
        match self.kind.as_str() {
            "cluster" | "infra" => 0,
            _ => 1,
        }
    }

    fn from_template(t: &Value) -> AppliedProfile {
        AppliedProfile {
            uid: text(t, "/uid"),
            name: text(t, "/name"),
            version: text(t, "/profileVersion"),
            kind: text(t, "/type"),
        }
    }
}

/// Clusters using a profile version.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct Consumers {
    /// Cluster uids.
    pub clusters: Vec<String>,
    /// Number of clusters.
    pub count: usize,
}

/// One layer of the profile: a pack with its configuration and its hashes.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Pack {
    /// `<layer>/<name>`, the key of the pack's node hash in the root
    pub key: String,
    /// Palette layer name: os, k8s, cni, csi, addon.
    pub layer: String,
    /// Pack name.
    pub name: String,
    /// Pack type: spectro, helm, manifest, oci.
    #[serde(rename = "type")]
    pub kind: String,
    /// Tag as pinned on the profile.
    pub tag: String,
    /// Resolved version.
    pub version: String,
    /// Uid of the pack version in the registry.
    pub pack_uid: String,
    /// Uid of the registry.
    pub registry_uid: String,
    /// The digest Palette stamps on the pack ref of the profile template
    pub ref_digest: String,
    /// The digest of the pack in its registry, from the packs/manifests endpoint
    pub registry_digest: String,
    /// For a pack from an OCI registry: the provider type of that registry, `pack`, `helm`, or
    /// `zarf`, as the pack ref of the profile gives it in its annotations. Palette follows a new
    /// digest in a different way for each. Outside the root: the registry uid is in it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_provider_type: Option<String>,
    /// The annotation `imageId` of the pack ref, when Palette gives it. A record keeps no other
    /// annotation, because an annotation can hold a secret. Outside the root: a record written
    /// before this field has none, and a record of strata 0.17 to 0.19 holds each annotation.
    /// [`Pack::image_id`] reads `imageId`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub annotations: BTreeMap<String, String>,
    /// True when Palette marks the pack ref invalid, from `isInvalid`: the registry deleted the
    /// tag of the pack, and Palette builds no cluster from the version. Outside the root: the
    /// flag is a statement of Palette about the registry, and the content of the pack stays.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub invalid: bool,
    /// Why Palette marks the pack invalid, from `inValidReason`, as Palette gives it. Empty when
    /// the pack is not invalid.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub invalid_reason: String,
    /// Pack params as set on the profile.
    pub params: Value,
    /// Raw values YAML as returned by the API
    pub values: String,
    /// The values parsed as YAML and converted to JSON; null when unparseable
    pub values_canon: Value,
    /// sha256 of canonical JSON of valuesCanon, or of the trimmed text when unparseable
    pub values_hash: String,
    /// Why the values did not parse as YAML, when they did not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values_parse_error: Option<String>,
    /// Attached manifests sorted by name
    pub manifests: Vec<Manifest>,
    /// The images that the values declare, `pack.content.images[].image`, sorted: a reference
    /// with a tag, and no digest. Outside the root: the values are in it. A record written before
    /// this field gets it from its values when it is read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
    /// The Helm charts that the values declare, `pack.content.charts[]`, sorted by name. Outside
    /// the root, as `images`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub charts: Vec<PackChart>,
    /// For a pack that strata reads from the registry, of a candidate or of a `pack:` record:
    /// each option of the pack template that the values select, sorted by parameter. Palette can
    /// write the presets of such an option into the values, and no route shows them. Outside the
    /// root: the values are in it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preset_options: Vec<PresetOption>,
    /// The path of each value that Palette masks, sorted: each value that is the text
    /// `********`. Palette writes that text over a value whose schema format is `password`, so
    /// the root covers the mask and not the secret. Outside the root: the values are in it. A
    /// record written before this field gets it from its values when it is read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub masked: Vec<String>,
    /// The three sub-hashes and the node hash.
    pub hash: PackHash,
}

/// One option of a pack template that the values of the pack select: the value at the target
/// key of an input parameter names the option.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PresetOption {
    /// The name of the input parameter.
    pub parameter: String,
    /// The key in the values that the parameter reads, as a dotted path.
    pub target_key: String,
    /// The option that the value at that key names.
    pub option: String,
}

/// One Helm chart that the values of a pack declare.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackChart {
    /// The chart name.
    pub name: String,
    /// The chart version.
    pub version: String,
    /// The repository of the chart.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub repo: String,
}

/// The images and the charts that parsed values declare, from `pack.content`: each sorted, and
/// each image once.
pub fn pack_content(values: &Value) -> (Vec<String>, Vec<PackChart>) {
    let content = values.pointer("/pack/content").unwrap_or(&Value::Null);
    let mut images: Vec<String> = arr(content, "/images")
        .iter()
        .filter_map(|i| {
            i.get("image")
                .and_then(Value::as_str)
                .or_else(|| i.as_str())
        })
        .map(str::to_string)
        .filter(|i| !i.is_empty())
        .collect();
    images.sort();
    images.dedup();
    let mut charts: Vec<PackChart> = arr(content, "/charts")
        .iter()
        .map(|c| PackChart {
            name: text(c, "/name"),
            version: text(c, "/version"),
            repo: text(c, "/repo"),
        })
        .filter(|c| !c.name.is_empty())
        .collect();
    charts.sort();
    charts.dedup();
    (images, charts)
}

/// The path of each value of parsed `values` that Palette masks, sorted, in the form of
/// [`crate::diff::leaf_diff`]: each value that is the text `********`.
fn masked_paths(values: &Value) -> Vec<String> {
    crate::diff::paths_of_text(values, MASKED)
}

/// A manifest attached to a pack on the profile.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// Manifest name.
    pub name: String,
    /// Manifest uid.
    pub uid: String,
    /// Digest as reported by Palette.
    pub digest: String,
    /// Raw YAML content.
    pub content: String,
    /// sha256 of the canonical content, or of the trimmed text when unparseable.
    pub content_hash: String,
    /// Why the content did not parse as YAML, when it did not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_parse_error: Option<String>,
}

/// The field of Palette that gave the upstream digest of a pack, as `hash.upstreamSource` names
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum UpstreamSource {
    /// The digest on the pack ref of the profile template.
    #[serde(rename = "packRef.digest")]
    PackRef,
    /// The digest of the entry of `packs/manifests`, when the pack ref gives none.
    #[serde(rename = "packSummary.digest")]
    PackSummary,
    /// Palette gave no digest, so `upstream` is the hash of nothing: see
    /// [`Pack::upstream_unknown`].
    #[serde(rename = "none")]
    NoDigest,
}

impl UpstreamSource {
    /// The source as the documents name it: `packRef.digest`, `packSummary.digest`, or `none`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PackRef => "packRef.digest",
            Self::PackSummary => "packSummary.digest",
            Self::NoDigest => "none",
        }
    }
}

impl fmt::Display for UpstreamSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The three sub-hashes and the node hash of one pack.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackHash {
    /// sha256 over name, layer, type, tag, version, packUid, registryUid
    pub identity: String,
    /// sha256 of upstreamRaw
    pub upstream: String,
    /// The digest that fed `upstream`
    pub upstream_raw: String,
    /// The field of Palette that gave `upstream`.
    pub upstream_source: UpstreamSource,
    /// sha256 over valuesHash, [manifest name, contentHash]..., params
    pub authored: String,
    /// sha256(identity + upstream + authored)
    pub node: String,
}

/// The content digest: what a version is, without the ids that Palette gives. It covers the
/// variables hash and, for each pack in order, the key, the version, the upstream hash, and the
/// authored hash. It leaves out the tag, the pack uid, the registry uid, and the pack type, so it
/// is equal for two publishes of one definition and for a copy of a pack to another registry.
fn content_of(variables_hash: &str, packs: &[Pack]) -> String {
    sha256_hex(&canonical_json(&json!({
        // The key keeps this digest apart from every other one, and gives the encoding a version.
        "digest": CONTENT_DIGEST,
        "variables": variables_hash,
        "packs": packs
            .iter()
            .map(|p| json!([p.key, p.version, p.hash.upstream, p.hash.authored]))
            .collect::<Vec<_>>(),
    })))
}

/// The root: one sha256 digest over the variables hash and the node hash of each pack, in order.
fn root_of(variables_hash: &str, packs: &[Pack]) -> String {
    sha256_hex(&canonical_json(&json!({
        "variables": variables_hash,
        "packs": packs.iter().map(|p| json!([p.key, p.hash.node])).collect::<Vec<_>>(),
    })))
}

/// The text of `key` in `v`, or `default` when `v` has no text there. An empty text stays empty,
/// so the hash of the variables does not change: this is not [`first_nonempty`].
fn str_or(v: &Value, key: &str, default: &str) -> Value {
    Value::String(
        v.get(key)
            .and_then(Value::as_str)
            .unwrap_or(default)
            .to_string(),
    )
}

fn bool_or(v: &Value, key: &str) -> Value {
    Value::Bool(v.get(key).and_then(Value::as_bool).unwrap_or(false))
}

/// Same field set and defaults as the bash extraction, so hashes agree across implementations.
fn normalize_variable(v: &Value) -> Value {
    let options: Vec<Value> = arr(v, "/options")
        .iter()
        .map(|o| {
            json!({
                "label": str_or(o, "label", ""),
                "value": str_or(o, "value", ""),
                "description": str_or(o, "description", ""),
            })
        })
        .collect();
    json!({
        "name": v.get("name").cloned().unwrap_or(Value::Null),
        "displayName": str_or(v, "displayName", ""),
        "description": str_or(v, "description", ""),
        "format": str_or(v, "format", "string"),
        "defaultValue": str_or(v, "defaultValue", ""),
        "regex": str_or(v, "regex", ""),
        "required": bool_or(v, "required"),
        "immutable": bool_or(v, "immutable"),
        "hidden": bool_or(v, "hidden"),
        "isSensitive": bool_or(v, "isSensitive"),
        "inputType": str_or(v, "inputType", "text"),
        "options": options,
    })
}

/// The manifests of pack `pk` of one template, each as `name`, `uid`, `digest`, and `content`,
/// from the answers of the manifests by uid (see [`RawProfile::manifest_entities`]). A manifest
/// of the published template has its published content. A manifest of the draft has its draft
/// content, and its published content when the draft did not change it: Palette writes the draft
/// content of a manifest at an edit, and empties it at the publish.
fn manifests_of_template(pk: &Value, entities: &Value, template: Template) -> Vec<Value> {
    arr(pk, "/manifests")
        .iter()
        .map(|m| {
            let uid = text(m, "/uid");
            let entity = entities.get(&uid).unwrap_or(&Value::Null);
            let edited = !text(entity, &format!("/spec/{template}/content")).is_empty();
            let from = if edited {
                template
            } else {
                Template::Published
            };
            let content = text(entity, &format!("/spec/{from}/content"));
            json!({
                "name": first_nonempty(&[str_at(m, "/name"), str_at(entity, "/metadata/name")]),
                "uid": uid,
                "digest": text(entity, &format!("/spec/{from}/digest")),
                "content": content,
            })
        })
        .collect()
}

impl Pack {
    /// True when the upstream of the pack is unknown: Palette gave no digest for it, so the
    /// `upstream` hash is the hash of nothing and a change of its archive cannot show. A manifest
    /// pack has no archive: its content is the manifests of the profile, which `authored` covers.
    pub fn upstream_unknown(&self) -> bool {
        self.hash.upstream_source == UpstreamSource::NoDigest && self.kind != "manifest"
    }

    /// The machine image that the pack names in its annotation `imageId`, when it names one.
    pub fn image_id(&self) -> Option<&str> {
        self.annotations
            .get(IMAGE_ID_ANNOTATION)
            .map(String::as_str)
            .filter(|id| !id.is_empty())
    }

    /// Build one pack from its ref on the profile template and the matching entry of the
    /// packs/manifests response (matched by name; missing entries leave those fields empty).
    pub fn from_api(pk: &Value, manifests_packs: &[Value]) -> Pack {
        Self::from_api_with(pk, manifests_packs, None)
    }

    /// [`Pack::from_api`], with the manifests of the pack given apart, each as `name`, `uid`,
    /// `digest`, and `content`, in place of those of the packs/manifests entry.
    fn from_api_with(pk: &Value, manifests_packs: &[Value], given: Option<Vec<Value>>) -> Pack {
        let name = text(pk, "/name");
        let mut mm = manifests_packs
            .iter()
            .find(|m| text(m, "/metadata/name") == name || text(m, "/spec/name") == name)
            .cloned()
            .unwrap_or(Value::Null);
        if let Some(given) = given {
            if !mm.is_object() {
                mm = json!({});
            }
            mm["spec"]["manifests"] = Value::Array(given);
        }
        let layer = first_nonempty(&[str_at(pk, "/layer"), "addon"]);
        let kind = first_nonempty(&[str_at(pk, "/type"), "spectro"]);
        let tag = text(pk, "/tag");
        let version = first_nonempty(&[str_at(pk, "/version"), str_at(&mm, "/spec/version")]);
        let pack_uid = text(pk, "/packUid");
        let registry_uid = text(pk, "/registryUid");
        let ref_digest = text(pk, "/digest");
        let registry_digest = text(&mm, "/spec/digest");
        let registry_provider_type = opt_text(pk, "/annotations/ociRegistryProviderType");
        // only `imageId`: another annotation can hold a secret, and strata reads no other one.
        // Palette gives each annotation as text; a value of another type is not an annotation
        let annotations: BTreeMap<String, String> =
            opt_text(pk, &format!("/annotations/{IMAGE_ID_ANNOTATION}"))
                .map(|id| (IMAGE_ID_ANNOTATION.to_string(), id))
                .into_iter()
                .collect();
        // a reason without the flag is not a reason: Palette clears the flag when the tag is back
        let invalid = pk.get("isInvalid").and_then(Value::as_bool) == Some(true);
        let invalid_reason = if invalid {
            text(pk, "/inValidReason")
        } else {
            String::new()
        };
        // a key of strata on the pack ref of a candidate: a pack ref of Palette has none
        let preset_options: Vec<PresetOption> = pk
            .get("presetOptions")
            .and_then(|o| serde_json::from_value(o.clone()).ok())
            .unwrap_or_default();
        let params = pk
            .get("params")
            .cloned()
            .filter(Value::is_object)
            .unwrap_or_else(|| Value::Object(Map::new()));
        let values = first_nonempty(&[str_at(pk, "/values"), str_at(&mm, "/spec/values")]);

        let mut manifests: Vec<Manifest> = arr(&mm, "/spec/manifests")
            .iter()
            .map(|m| {
                let content = text(m, "/content");
                let (content_hash, content_parse_error) = hash_yaml(&content);
                Manifest {
                    name: text(m, "/name"),
                    uid: text(m, "/uid"),
                    digest: text(m, "/digest"),
                    content,
                    content_hash,
                    content_parse_error,
                }
            })
            .collect();
        manifests.sort_by(|a, b| a.name.cmp(&b.name));

        let (values_canon, values_hash, values_parse_error) = canon_yaml_hashed(&values);

        let identity = sha256_hex(&canonical_json(&json!({
            "name": name, "layer": layer, "type": kind, "tag": tag, "version": version,
            "packUid": pack_uid, "registryUid": registry_uid,
        })));
        let (upstream_raw, upstream_source) = if !ref_digest.is_empty() {
            (ref_digest.clone(), UpstreamSource::PackRef)
        } else if !registry_digest.is_empty() {
            (registry_digest.clone(), UpstreamSource::PackSummary)
        } else {
            (String::new(), UpstreamSource::NoDigest)
        };
        let upstream = sha256_hex(&upstream_raw);
        let authored = sha256_hex(&canonical_json(&json!({
            "values": values_hash,
            "manifests": manifests.iter().map(|m| json!([m.name, m.content_hash])).collect::<Vec<_>>(),
            "params": params,
        })));
        let node = sha256_hex(&format!("{identity}{upstream}{authored}"));

        let (images, charts) = pack_content(&values_canon);
        let masked = masked_paths(&values_canon);
        Pack {
            key: format!("{layer}/{name}"),
            layer,
            name,
            kind,
            tag,
            version,
            pack_uid,
            registry_uid,
            ref_digest,
            registry_digest,
            registry_provider_type,
            annotations,
            invalid,
            invalid_reason,
            params,
            values,
            values_canon,
            values_hash,
            values_parse_error,
            manifests,
            images,
            charts,
            preset_options,
            masked,
            hash: PackHash {
                identity,
                upstream,
                upstream_raw,
                upstream_source,
                authored,
                node,
            },
        }
    }
}

/// The user macros that `packs` use, by name and sorted: in the values and the manifests.
fn macros_of(packs: &[Pack]) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    for p in packs {
        names.extend(macro_names(&p.values));
        for m in &p.manifests {
            names.extend(macro_names(&m.content));
        }
    }
    names.into_iter().collect()
}

/// The prefix of a user macro in a value or a manifest: `{{.spectro.macro.NAME}}`. A system
/// macro, `{{.spectro.system.NAME}}`, has another prefix.
const MACRO_PREFIX: &str = ".spectro.macro.";

/// The placeholder of the user macro `name`, as the resolved values of Palette key it:
/// `{{.spectro.macro.NAME}}`.
fn macro_placeholder(name: &str) -> String {
    format!("{{{{{MACRO_PREFIX}{name}}}}}")
}

/// The names of the user macros that `text` uses: each `{{.spectro.macro.NAME}}`, with spaces
/// after `{{` allowed. A name holds letters, digits, `_`, `-`, and `.`. A system macro,
/// `{{.spectro.system.NAME}}`, is not a user macro: Palette gives it from the cluster, and no
/// person edits it.
pub fn macro_names(text: &str) -> std::collections::BTreeSet<String> {
    let mut names = std::collections::BTreeSet::new();
    let mut rest = text;
    while let Some(i) = rest.find("{{") {
        rest = &rest[i + 2..];
        let body = rest.trim_start();
        if let Some(after) = body.strip_prefix(MACRO_PREFIX) {
            let name: String = after
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
                .collect();
            let name = name.trim_end_matches('.').to_string();
            if !name.is_empty() {
                names.insert(name);
            }
        }
    }
    names
}

impl Snapshot {
    /// The user macros that the recorded content uses, by name and sorted: in the values and the
    /// manifests of each pack. The root covers these names and not the values Palette gives
    /// them, so an edit of a macro does not move the root.
    pub fn macros(&self) -> Vec<String> {
        macros_of(&self.packs)
    }

    /// The key of each pack whose upstream is unknown, in pack order: see
    /// [`Pack::upstream_unknown`].
    pub fn upstream_unknown(&self) -> Vec<String> {
        self.packs
            .iter()
            .filter(|p| p.upstream_unknown())
            .map(|p| p.key.clone())
            .collect()
    }

    /// Build a snapshot from the raw API responses of one profile: of its published template, or
    /// of its draft when it has no published template.
    pub fn from_raw(raw: &RawProfile, reference: &str, fetched_at: &str) -> Result<Snapshot> {
        if arr(&raw.profile, "/spec/published/packs").is_empty() {
            Self::from_raw_template(raw, reference, fetched_at, Template::Draft)
        } else {
            Self::from_raw_template(raw, reference, fetched_at, Template::Published)
        }
    }

    /// Build a snapshot of the draft template of one profile: what the next publish makes the
    /// version. A profile with no draft is the error [`Error::NoDraft`].
    pub fn from_raw_draft(raw: &RawProfile, reference: &str, fetched_at: &str) -> Result<Snapshot> {
        let prof = &raw.profile;
        if !crate::api::has_draft(prof) {
            return Err(Error::NoDraft {
                name: text(prof, "/metadata/name"),
                version: text(prof, "/spec/version"),
                uid: text(prof, "/metadata/uid"),
            });
        }
        Self::from_raw_template(raw, reference, fetched_at, Template::Draft)
    }

    /// Build a snapshot of one template of a profile: [`Template::Published`] or
    /// [`Template::Draft`].
    fn from_raw_template(
        raw: &RawProfile,
        reference: &str,
        fetched_at: &str,
        template: Template,
    ) -> Result<Snapshot> {
        let prof = &raw.profile;
        let tpl_ptr = format!("/spec/{template}");
        let tpl_packs = arr(prof, &format!("{tpl_ptr}/packs"));
        let mut mpacks = arr(&raw.manifests, "/spec/packs").to_vec();
        if crate::api::has_draft(prof) && template == Template::Published {
            // packs/manifests holds the packs of the draft: keep only the entries of the packs
            // that the published template holds, by pack uid
            let uids: BTreeSet<&str> = tpl_packs.iter().map(|pk| str_at(pk, "/packUid")).collect();
            mpacks.retain(|m| uids.contains(str_at(m, "/metadata/uid")));
        }

        let tags: BTreeMap<String, String> = prof
            .pointer("/metadata/labels")
            .and_then(Value::as_object)
            .map(|o| {
                o.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        let clusters = strings(prof, "/status/inUseClusterUids");
        let variables: Vec<Value> = arr(&raw.variables, "/variables")
            .iter()
            .map(normalize_variable)
            .collect();
        let packs: Vec<Pack> = tpl_packs
            .iter()
            .map(|pk| {
                // a ref that lists no manifest keeps the manifests of the packs/manifests entry
                let given = raw
                    .manifest_entities
                    .as_ref()
                    .filter(|_| !arr(pk, "/manifests").is_empty())
                    .map(|e| manifests_of_template(pk, e, template));
                Pack::from_api_with(pk, &mpacks, given)
            })
            .collect();
        let source = Source {
            reference: reference.to_string(),
            uid: text(prof, "/metadata/uid"),
            project_uid: String::new(),
            name: text(prof, "/metadata/name"),
            version: text(prof, "/spec/version"),
            cloud_type: text(prof, &format!("{tpl_ptr}/cloudType")),
            kind: text(prof, &format!("{tpl_ptr}/type")),
            template,
            variables_unavailable: raw.variables_unavailable,
            tags,
            consumers: Some(Consumers {
                count: clusters.len(),
                clusters,
            }),
            fetched_at: fetched_at.to_string(),
            cluster: None,
            pack: None,
            palette_validation: None,
            macro_values: raw
                .resolved_values
                .as_ref()
                .map(|r| profile_macro_digests(r, &macros_of(&packs))),
        };
        Ok(Snapshot::assemble(source, variables, packs))
    }

    /// The record of `source`, with `variables` sorted by name, `packs` in profile order, and
    /// the root and the content digest over both.
    ///
    /// Each kind of record comes from this function. A change here moves the root of each one, so
    /// it is a change of the schema id.
    fn assemble(source: Source, mut variables: Vec<Value>, packs: Vec<Pack>) -> Snapshot {
        variables.sort_by_key(|v| text(v, "/name"));
        let variables_hash = sha256_hex(&canonical_json(&Value::Array(variables.clone())));
        Snapshot {
            schema: SNAPSHOT_SCHEMA.to_string(),
            tool: Tool::current(),
            source,
            root: root_of(&variables_hash, &packs),
            content_digest: content_of(&variables_hash, &packs),
            variables,
            variables_hash,
            packs,
        }
    }

    /// The content digest: equal for two objects that are the same version, where the root is
    /// not. Palette gives a manifest pack a new uid for each profile, and a pack uid belongs to
    /// one registry, so two publishes of one definition have two roots and one content digest.
    /// The root answers "did this object change". This digest answers "is this the same version".
    pub fn content_digest(&self) -> String {
        content_of(&self.variables_hash, &self.packs)
    }

    /// The packs that this version takes from a registry, each with its version and the digest
    /// of the registry. A manifest pack comes from no registry, so it is authored content and
    /// not a pin.
    pub fn pins(&self) -> Vec<PinRef> {
        self.packs
            .iter()
            .filter(|p| p.kind != "manifest")
            .map(|p| PinRef {
                name: p.name.clone(),
                version: p.version.clone(),
                digest: p.hash.upstream_raw.clone(),
            })
            .collect()
    }

    /// A snapshot for each profile that a cluster applies, from one read of the cluster, in the
    /// canonical order of [`AppliedProfile::of_cluster`]. Each one is the snapshot that
    /// `cluster:<cluster>/<profile>` gives.
    pub fn each_from_cluster(
        raw: &RawCluster,
        reference: &str,
        fetched_at: &str,
    ) -> Result<Vec<Snapshot>> {
        AppliedProfile::of_cluster(&raw.cluster)
            .iter()
            .map(|applied| Snapshot::from_cluster(raw, Some(&applied.uid), reference, fetched_at))
            .collect()
    }

    /// Record the project scope the snapshot was taken in; `None` is tenant scope. The scope is
    /// metadata: it does not enter the root.
    pub fn with_project(mut self, project_uid: Option<&str>) -> Snapshot {
        self.source.project_uid = project_uid.unwrap_or("").to_string();
        self
    }

    /// Build a snapshot of what a cluster runs from its raw API responses. Each applied profile is
    /// hashed as the profile itself would be, so a cluster that runs a recorded version has the
    /// root of that record. With `profile`, only the applied profile of that name (or uid) is
    /// covered; without, every applied profile in order, packs concatenated and variables merged.
    pub fn from_cluster(
        raw: &RawCluster,
        profile: Option<&str>,
        reference: &str,
        fetched_at: &str,
    ) -> Result<Snapshot> {
        // In canonical order, so two clusters built from one definition have one merged root
        // whatever order Palette lists their profiles in.
        let (templates, applied): (Vec<Value>, Vec<AppliedProfile>) =
            AppliedProfile::templates_of(&raw.cluster)
                .into_iter()
                .unzip();
        let chosen: Vec<usize> = match profile {
            None => (0..templates.len()).collect(),
            Some(p) => {
                // `a+b` selects several profiles, as compare does for a cluster group
                let wanted: Vec<&str> = p.split('+').collect();
                let chosen: Vec<usize> = applied
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| {
                        wanted.contains(&a.name.as_str()) || wanted.contains(&a.uid.as_str())
                    })
                    .map(|(i, _)| i)
                    .collect();
                if chosen.is_empty() {
                    return Err(Error::ClusterProfileNotFound {
                        cluster: text(&raw.cluster, "/metadata/uid"),
                        profile: p.to_string(),
                        applied: applied.iter().map(|a| a.name.clone()).collect(),
                    });
                }
                chosen
            }
        };
        let parts = chosen
            .iter()
            .map(|&i| Snapshot::from_raw(&raw.template_raw(&templates[i]), reference, fetched_at))
            .collect::<Result<Vec<Snapshot>>>()?;
        let covered: Vec<AppliedProfile> = chosen.iter().map(|&i| applied[i].clone()).collect();

        let variables: Vec<Value> = parts.iter().flat_map(|p| p.variables.clone()).collect();
        let packs: Vec<Pack> = parts.iter().flat_map(|p| p.packs.clone()).collect();

        let joined = |f: fn(&Source) -> &String| {
            parts
                .iter()
                .map(|p| f(&p.source).as_str())
                .collect::<Vec<_>>()
                .join("+")
        };
        let source = Source {
            reference: reference.to_string(),
            uid: match parts.as_slice() {
                [only] => only.source.uid.clone(),
                _ => String::new(),
            },
            project_uid: String::new(),
            name: joined(|s| &s.name),
            version: joined(|s| &s.version),
            cloud_type: parts
                .first()
                .map(|p| p.source.cloud_type.clone())
                .filter(|c| !c.is_empty())
                .unwrap_or_else(|| text(&raw.cluster, "/spec/cloudType")),
            kind: joined(|s| &s.kind),
            template: Template::Applied,
            variables_unavailable: raw.variables_unavailable,
            tags: BTreeMap::new(),
            consumers: None,
            fetched_at: fetched_at.to_string(),
            pack: None,
            palette_validation: None,
            // a cluster record holds the digests in cluster.macroValues
            macro_values: None,
            cluster: Some({
                let pending_updates = raw
                    .notifications
                    .as_ref()
                    .map(|n| PendingUpdate::read(n, &covered))
                    .unwrap_or_default();
                ClusterSource {
                    uid: text(&raw.cluster, "/metadata/uid"),
                    name: text(&raw.cluster, "/metadata/name"),
                    profile: profile.map(str::to_string),
                    variables: ClusterVariable::read(raw, &covered),
                    updates_pending: raw
                        .notifications
                        .as_ref()
                        .map(|_| !pending_updates.is_empty()),
                    updates_pending_error: raw.errors.notifications.clone(),
                    pending_updates,
                    repave_pending: Some(is_pending(&raw.cluster)),
                    pending_repave: raw.repave.as_ref().map(PendingRepave::read),
                    pending_repave_error: raw.errors.repave.clone(),
                    macro_values: raw
                        .resolved_values
                        .as_ref()
                        .map(|r| macro_digests(r, &covered, &macros_of(&packs))),
                    labels: raw
                        .cluster
                        .pointer("/metadata/labels")
                        .and_then(Value::as_object)
                        .map(|m| {
                            m.iter()
                                .map(|(k, v)| {
                                    (k.clone(), v.as_str().unwrap_or_default().to_string())
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    tiers: cluster_tiers(raw, &covered),
                    pending: false,
                    kind: SubjectKind::Cluster,
                    members: Vec::new(),
                    profiles: covered,
                }
            }),
        };
        Ok(Snapshot::assemble(source, variables, packs))
    }

    /// What Palette holds of a version that it no longer has: the source of the record, fetched
    /// at `checked_at`, with no pack and no variable. `verify` compares a record with it when the
    /// uid of the record is gone, so each pack of the record shows as removed.
    pub fn gone(&self, checked_at: &str) -> Snapshot {
        let mut source = self.source.clone();
        source.fetched_at = checked_at.to_string();
        source.consumers = None;
        Snapshot::assemble(source, Vec::new(), Vec::new())
    }

    /// Mark a record of a cluster as the profiles of the update that waits for it. Whether an
    /// update or a repave waits is a statement about the cluster as it runs now, and not about
    /// these profiles, so the record does not carry it.
    pub fn mark_pending(mut self) -> Snapshot {
        if let Some(c) = self.source.cluster.as_mut() {
            c.pending = true;
            c.updates_pending = None;
            c.updates_pending_error = None;
            c.pending_updates.clear();
            c.repave_pending = None;
            c.pending_repave = None;
            c.pending_repave_error = None;
        }
        self
    }

    /// Mark a record of a cluster as a record of another kind of object that holds profiles. Such
    /// an object gives no values to the variables, so the record holds no cluster values: a
    /// definition with no value is not the value `""`. Palette repaves only a cluster, so the
    /// record holds no repave either.
    pub fn with_kind(mut self, kind: SubjectKind) -> Snapshot {
        if let Some(c) = self.source.cluster.as_mut() {
            c.kind = kind;
            if !kind.is_cluster() {
                c.variables.clear();
                c.repave_pending = None;
                c.pending_repave = None;
                c.pending_repave_error = None;
            }
        }
        self
    }

    /// Read a snapshot file, checking its schema id.
    ///
    /// # Errors
    ///
    /// [`Error::UnsupportedSchema`] for a snapshot of another schema id, [`Error::SnapshotSchema`]
    /// when the fields do not match, [`Error::NotSnapshot`] for a file that is not a snapshot,
    /// and [`Error::Io`] when strata cannot read the file.
    pub fn load(path: &Path) -> Result<Snapshot> {
        let mut snap: Snapshot = load_document(path, SNAPSHOT_SCHEMA, || {
            Error::NotSnapshot(path.to_path_buf())
        })?;
        snap.fill_content();
        Ok(snap)
    }

    /// Give each pack the images, the charts, and the masked paths of its values, when the record
    /// holds none: a record written before those fields has the values, and so the lists.
    pub fn fill_content(&mut self) {
        for p in &mut self.packs {
            if p.images.is_empty() && p.charts.is_empty() {
                (p.images, p.charts) = pack_content(&p.values_canon);
            }
            if p.masked.is_empty() {
                p.masked = masked_paths(&p.values_canon);
            }
        }
    }

    /// Serialize, pretty-printed unless `compact`, with a trailing newline: the document of the
    /// record, as [`JsonRenderer`](crate::render::JsonRenderer) writes each document.
    pub fn to_json(&self, compact: bool) -> Result<String> {
        crate::render::JsonRenderer { compact }.document(self)
    }
}

/// Read the document of the schema id `schema` at `path`, with its id checked.
///
/// A file with the id `schema` gives the document, or [`Error::SnapshotSchema`] when its
/// fields do not match. A file whose id names another version of the same document, such as
/// `strata-snapshot/2` for `strata-snapshot/1`, gives [`Error::UnsupportedSchema`], which names
/// both ids. Each other file gives `not_one`: it is not JSON, it has no id, or its id names
/// another document. The records and the report of `behind --since` follow this one rule.
pub(crate) fn load_document<T: DeserializeOwned>(
    path: &Path,
    schema: &'static str,
    not_one: impl FnOnce() -> Error,
) -> Result<T> {
    let text = crate::ops::read_file(path)?;
    let Ok(v) = serde_json::from_str::<Value>(&text) else {
        return Err(not_one());
    };
    let found = str_at(&v, "/schema");
    if found != schema {
        return Err(if document_name(found) == document_name(schema) {
            Error::UnsupportedSchema {
                path: path.to_path_buf(),
                found: found.to_string(),
                supported: schema,
            }
        } else {
            not_one()
        });
    }
    serde_json::from_value(v).map_err(|source| Error::SnapshotSchema {
        path: path.to_path_buf(),
        schema,
        source,
    })
}

/// The name of the document in the schema id `id`: the text before its last `/`, which each
/// version of the document shares. None when `id` is no schema id.
fn document_name(id: &str) -> Option<&str> {
    id.rsplit_once('/').map(|(name, _)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tier_leaves_a_field_that_strata_could_not_read_out_of_its_fields_and_its_hash() {
        let errors = BTreeMap::from([("rbacs".to_string(), "HTTP 403: forbidden".to_string())]);
        let unread = ClusterTier::with_errors(
            json!({"timezone": "Etc/UTC", "rbacs": null}),
            errors.clone(),
        );
        let read = ClusterTier::of(json!({"timezone": "Etc/UTC"}));
        assert_eq!(unread.fields, read.fields);
        assert_eq!(unread.hash, read.hash);
        assert_eq!(unread.errors, errors);
    }

    /// `as_str` of each value equals its text in the documents, and the text reads back.
    fn names_its_values<T>(values: &[T], as_str: fn(T) -> &'static str)
    where
        T: Copy + PartialEq + fmt::Debug + Serialize + serde::de::DeserializeOwned,
    {
        for v in values {
            let text = Value::String(as_str(*v).to_string());
            assert_eq!(serde_json::to_value(v).unwrap(), text);
            assert_eq!(serde_json::from_value::<T>(text).unwrap(), *v);
        }
    }

    #[test]
    fn each_closed_set_of_a_record_prints_as_the_documents_name_it() {
        names_its_values(
            &[
                Template::Published,
                Template::Draft,
                Template::Applied,
                Template::Candidate,
                Template::Registry,
            ],
            Template::as_str,
        );
        names_its_values(
            &[
                UpstreamSource::PackRef,
                UpstreamSource::PackSummary,
                UpstreamSource::NoDigest,
            ],
            UpstreamSource::as_str,
        );
        names_its_values(
            &[
                Tier::Applied,
                Tier::Labels,
                Tier::Policy,
                Tier::Running,
                Tier::Shape,
            ],
            Tier::as_str,
        );
        // the map of the tiers keeps the order of the names, as a map keyed by text did
        let tiers: BTreeMap<Tier, u8> = [
            (Tier::Shape, 0),
            (Tier::Running, 0),
            (Tier::Policy, 0),
            (Tier::Applied, 0),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            serde_json::to_string(&tiers).unwrap(),
            r#"{"applied":0,"policy":0,"running":0,"shape":0}"#
        );
    }

    #[test]
    fn the_placeholder_of_a_macro_is_what_macro_names_reads() {
        let key = macro_placeholder("region");
        assert_eq!(key, "{{.spectro.macro.region}}");
        assert_eq!(
            macro_names(&key).into_iter().collect::<Vec<_>>(),
            ["region"]
        );
    }
}
