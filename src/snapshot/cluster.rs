//! The record of a cluster: its source, its tiers, the profiles that it applies, its variables,
//! and its pending updates.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};

use super::{
    CLUSTER_FIELD, SNAPSHOT_SCHEMA, Source, SubjectKind, macro_placeholder, parse_document,
};
use crate::api::{RawCluster, profile_variables};
use crate::closed_set::{ClosedSet, Open, impl_closed_set};
use crate::content::canon::{canonical_json, sha256_digest, sha256_hex};
use crate::json::{arr, flag, object_at, str_at, text};
use crate::model::profile_type::ProfileType;
use crate::model::repave::PendingRepave;
use crate::timestamp::Rfc3339OrEmpty;

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
    /// Why strata could not read a part of the cluster that is beside the copy. The key names the
    /// part, and the field of the cluster is then absent. The reason is the HTTP error of Palette,
    /// such as `HTTP 403: forbidden`. The tiers have their own `errors`, in the same shape. Outside
    /// the root.
    ///
    /// A record of strata 0.24 and before holds `updatesPendingError` and `pendingRepaveError`
    /// instead. Strata reads them into this map.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[schemars(schema_with = "read_errors_schema")]
    pub errors: BTreeMap<Open<ClusterRead>, String>,
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
    #[serde(default, skip_serializing_if = "Open::is_cluster")]
    pub kind: Open<SubjectKind>,
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
    #[schemars(schema_with = "super::tier_schema::tiers")]
    pub tiers: BTreeMap<Open<Tier>, ClusterTier>,
}

/// A part of a cluster that is beside its copy, and that strata reads with a request of its own.
///
/// [`ClusterSource::errors`] names a part that strata could not read by one of these values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ClusterRead {
    /// The open notifications of an update of a profile that the record covers. The field
    /// `updatesPending` is then absent.
    UpdatesPending,
    /// The repave of the cluster that waits for approval. The field `pendingRepave` is then
    /// absent.
    PendingRepave,
}

impl_closed_set!(ClusterRead {
    UpdatesPending => "updatesPending",
    PendingRepave => "pendingRepave",
});

/// The two fields of a record of strata 0.24 and before that held a read error. Each has the key of
/// [`ClusterSource::errors`] that holds the error now. Do not remove one: a record of an older
/// release needs it.
const OLDER_ERRORS: [(&str, ClusterRead); 2] = [
    ("updatesPendingError", ClusterRead::UpdatesPending),
    ("pendingRepaveError", ClusterRead::PendingRepave),
];

/// Move the older read errors of `cluster`, the JSON of a cluster record, into its `errors`.
///
/// An entry that `errors` holds already stays. An older field that holds null is no error. An
/// older field that is not text moves too, so the read of [`ClusterSource`] refuses it and names
/// the entry. [`take_record`] refuses it first, and names the older field.
///
/// The move works on the JSON and not on a struct with `#[serde(flatten)]`. Serde loses the path
/// of a field inside a flatten, so the error names the parent.
pub(super) fn move_older_errors(cluster: &mut Value) {
    let Some(fields) = cluster.as_object_mut() else {
        return;
    };
    for (older, read) in OLDER_ERRORS {
        let Some(why) = fields.remove(older).filter(|why| !why.is_null()) else {
            continue;
        };
        if let Value::Object(errors) = fields
            .entry("errors")
            .or_insert_with(|| Value::Object(Default::default()))
        {
            errors.entry(read.as_str()).or_insert(why);
        }
    }
}

/// Take the field `source.cluster` out of `document`, the JSON of a record, with the older read
/// errors moved: see [`move_older_errors`]. None when the record has no cluster.
///
/// [`Snapshot::load`](super::Snapshot::load) reads the cluster apart from the rest of the record.
/// So an error names the field inside the cluster, as `source.cluster.profiles[0].version`.
///
/// # Errors
///
/// [`Error::SnapshotSchema`](crate::error::Error::SnapshotSchema) when an older read error is not
/// text. The move renames the field, so this function checks it first. The error names the key
/// that the file holds, as `source.cluster.pendingRepaveError`.
pub(super) fn take_record(
    path: &Path,
    document: &mut Value,
) -> crate::error::Result<Option<Value>> {
    let Some(mut cluster) = document
        .get_mut("source")
        .and_then(|source| source.get_mut("cluster"))
        .map(Value::take)
        .filter(|cluster| !cluster.is_null())
    else {
        return Ok(None);
    };
    for (older, _) in OLDER_ERRORS {
        if let Some(why) = cluster.get(older).filter(|why| !why.is_null()) {
            let field = format!("{CLUSTER_FIELD}.{older}");
            parse_document::<String>(path, SNAPSHOT_SCHEMA, why.clone(), &field)?;
        }
    }
    move_older_errors(&mut cluster);
    Ok(Some(cluster))
}

/// Read the field `cluster` of [`Source`] from a record of any release.
///
/// A record of strata 0.24 and before holds the read errors of the cluster in two fields. A later
/// record holds them in [`ClusterSource::errors`]. This function reads both into `errors`. So a
/// read of a [`Snapshot`](super::Snapshot) with serde gives the same answer as
/// [`Snapshot::load`](super::Snapshot::load).
pub(super) fn deserialize_cluster<'de, D>(
    deserializer: D,
) -> Result<Option<ClusterSource>, D::Error>
where
    D: Deserializer<'de>,
{
    let Some(mut cluster) = Option::<Value>::deserialize(deserializer)? else {
        return Ok(None);
    };
    move_older_errors(&mut cluster);
    serde_json::from_value(cluster)
        .map(Some)
        .map_err(serde::de::Error::custom)
}

/// The schema of [`ClusterSource::errors`]: a map from a part to the error of Palette. A key can
/// also name a part of a later release.
fn read_errors_schema(generator: &mut SchemaGenerator) -> Schema {
    json_schema!({
        "type": "object",
        "propertyNames": generator.subschema_for::<Open<ClusterRead>>(),
        "additionalProperties": {"type": "string"},
    })
}

impl ClusterSource {
    /// The tier `tier` of the record, when the record carries it.
    pub fn tier(&self, tier: Tier) -> Option<&ClusterTier> {
        self.tiers.get(&Open::Known(tier))
    }

    /// Why strata could not read the notifications, when `updates_pending` is absent for that
    /// reason.
    pub fn updates_pending_error(&self) -> Option<&str> {
        self.read_error(ClusterRead::UpdatesPending)
    }

    /// Why strata could not read the repave that waits, when `pending_repave` is absent for that
    /// reason.
    pub fn pending_repave_error(&self) -> Option<&str> {
        self.read_error(ClusterRead::PendingRepave)
    }

    /// Why strata could not read the part `read` of the cluster, when it could not.
    fn read_error(&self, read: ClusterRead) -> Option<&str> {
        self.errors.get(&read.into()).map(String::as_str)
    }
}

/// The parts of the cluster in `raw` that strata could not read and that are beside the copy: the
/// notifications and the repave that waits. Each one has the reason. The role bindings and the
/// namespaces are in the `errors` of the tier `policy`.
pub(crate) fn cluster_errors(raw: &RawCluster) -> BTreeMap<Open<ClusterRead>, String> {
    unread([
        (
            ClusterRead::UpdatesPending.into(),
            &raw.errors.notifications,
        ),
        (ClusterRead::PendingRepave.into(), &raw.errors.repave),
    ])
}

/// The entries of `reasons` that hold a reason, as a map from the part to why strata could not
/// read it.
pub(super) fn unread<K: Ord, const N: usize>(
    reasons: [(K, &Option<String>); N],
) -> BTreeMap<K, String> {
    reasons
        .into_iter()
        .filter_map(|(key, why)| Some((key, why.clone()?)))
        .collect()
}

/// A tier of a cluster record: a part of the cluster beside its release, with a hash of its own.
///
/// The variants are in the order of their names. So the map of the tiers of a record, and its JSON,
/// keep the order of a record of strata 0.21.
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

impl_closed_set!(Tier {
    Applied => "applied",
    Labels => "labels",
    Policy => "policy",
    Running => "running",
    Shape => "shape",
});

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
///
/// # Errors
///
/// [`Error::Serialize`](crate::Error::Serialize) for a cluster group whose values have no
/// canonical form, when strata cannot mask a password in them.
pub fn cluster_tiers(
    raw: &RawCluster,
    covered: &[AppliedProfile],
) -> crate::error::Result<BTreeMap<Open<Tier>, ClusterTier>> {
    let cluster = &raw.cluster;
    let mut tiers = BTreeMap::new();
    // Palette's own hashes of the spec that it applied to the cluster
    if let Some(apply) = cluster.pointer("/status/spcApply") {
        tiers.insert(
            Tier::Applied.into(),
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
        tiers.insert(Tier::Running.into(), running_tier(cluster, covered));
    }
    if let Some(config) = cluster.pointer("/spec/clusterConfig") {
        tiers.insert(
            Tier::Policy.into(),
            ClusterTier::with_errors(
                crate::snapshot::policy::fields(raw, config),
                crate::snapshot::policy::errors(raw),
            ),
        );
    }
    // the document of a cluster group has spec.clustersConfig, and no spec.clusterConfig
    if let Some(config) = cluster.pointer("/spec/clustersConfig") {
        tiers.insert(
            Tier::Policy.into(),
            ClusterTier::of(crate::snapshot::policy::group_fields(config)?),
        );
    }
    if let Some(cc) = &raw.cloud_config {
        tiers.insert(
            Tier::Shape.into(),
            ClusterTier::of(crate::snapshot::shape::fields(cluster, cc)),
        );
    }
    Ok(tiers)
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

/// The digest of each macro in `names` that `resolved` gives for one of `profiles`.
///
/// A digest is `sha256:<hex>` of the value, or `unresolved` when no profile has it. `resolved` is
/// the answer for a cluster or a cluster group, with one map for each profile.
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
pub(super) fn profile_macro_digests(
    resolved: &Value,
    names: &[String],
) -> BTreeMap<String, String> {
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
                .map(sha256_digest)
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
    #[schemars(with = "Rfc3339OrEmpty")]
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
        for n in arr(notifications, "").iter().rev() {
            let open = str_at(n, "/action/actionType") == PROFILE_UPDATE_ACTION
                && !flag(n, "/action/isDone");
            if !open {
                continue;
            }
            let events = object_at(n, "/action/events");
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
    pub(super) fn read(raw: &RawCluster, profiles: &[AppliedProfile]) -> Vec<ClusterVariable> {
        let mut out = Vec::new();
        for p in profiles {
            let mut of_profile: Vec<ClusterVariable> = profile_variables(&raw.variables, &p.uid)
                .map(|v| {
                    let sensitive = flag(v, "/isSensitive");
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

    /// True when the record holds the value of the variable in clear, and its name reads as a
    /// secret: see [`name_reads_as_secret`]. A value counts when it is not empty and holds no
    /// template, `{{`. The name is only a guess, so this blocks nothing.
    pub(crate) fn reads_as_secret(&self) -> bool {
        self.value
            .as_deref()
            .is_some_and(|v| !v.is_empty() && !v.contains("{{"))
            && name_reads_as_secret(&self.name)
    }
}

/// The words that make a variable name read as a secret, with no case.
const SECRET_WORDS: [&str; 4] = ["secret", "password", "passwd", "token"];

/// The pairs of words that make a variable name read as a secret, with no case, with one
/// character or none between them: `apikey`, `api_key`, `private-key`.
const SECRET_PAIRS: [(&str, &str); 2] = [("api", "key"), ("private", "key")];

/// True when `name` matches `secret|password|passwd|token|api.?key|private.?key` with no case.
///
/// The pattern comes from #626. On the variables of a real project, it named one variable, and
/// that variable held a credential. Each index is in the lower case text, so each slice is at a
/// `char` boundary.
fn name_reads_as_secret(name: &str) -> bool {
    let name = name.to_lowercase();
    SECRET_WORDS.iter().any(|w| name.contains(w))
        || SECRET_PAIRS.iter().any(|(first, last)| {
            name.match_indices(first).any(|(i, _)| {
                let rest = &name[i + first.len()..];
                let after_one = rest.chars().next().map_or("", |c| &rest[c.len_utf8()..]);
                rest.starts_with(last) || after_one.starts_with(last)
            })
        })
}

/// A variable of a cluster record whose name reads as a secret, and whose value the record holds
/// in clear. It names the variable, and holds no value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SecretLikeVariable {
    /// The name of the profile that declares the variable.
    pub profile: String,
    /// The name of the variable.
    pub name: String,
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
    pub(super) fn templates_of(doc: &Value) -> Vec<(Value, AppliedProfile)> {
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

    /// The profile of `running` that a move to the profile version `target` replaces.
    ///
    /// It is the profile with the uid of `target`, else the profile with its name. Else, when
    /// `target` builds a cluster, it is the profile that builds the cluster: a cluster runs one.
    /// `None` for an add-on that the cluster does not run, because the move adds it.
    pub(crate) fn replaced_by<'a>(
        running: &'a [AppliedProfile],
        target: &Source,
    ) -> Option<&'a AppliedProfile> {
        let builds_cluster = |p: &&AppliedProfile| ProfileType::text_builds_cluster(&p.kind);
        running
            .iter()
            .find(|p| p.uid == target.uid)
            .or_else(|| running.iter().find(|p| p.name == target.name))
            .or_else(|| {
                ProfileType::text_builds_cluster(&target.kind)
                    .then(|| running.iter().find(builds_cluster))
                    .flatten()
            })
    }

    /// The cluster or infra profile comes first: a cluster runs it before any add-on.
    fn rank(&self) -> u8 {
        if ProfileType::text_builds_cluster(&self.kind) {
            0
        } else {
            1
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

    /// The template of the profile `uid` in `spec.clusterProfileTemplates` of a document: a
    /// cluster, a cluster group, or an edge host. `None` when the document does not apply it.
    pub fn template_of<'a>(doc: &'a Value, uid: &str) -> Option<&'a Value> {
        arr(doc, "/spec/clusterProfileTemplates")
            .iter()
            .find(|t| str_at(t, "/uid") == uid)
    }

    /// The packs of the template of the profile `uid`, as the document holds them. No pack when
    /// the document does not apply the profile: see [`AppliedProfile::template_of`].
    pub(crate) fn template_packs<'a>(doc: &'a Value, uid: &str) -> &'a [Value] {
        Self::template_of(doc, uid).map_or(&[], |t| arr(t, "/packs"))
    }

    /// The entry of `packValues` for `pack`, a pack of a template: the name, the tag, the type,
    /// and the values, as the template holds them.
    ///
    /// The body of a create and the body of a validate route hold such an entry for each pack. A
    /// field that the template does not hold is empty text.
    pub fn template_pack_value(pack: &Value) -> Value {
        json!({
            "name": str_at(pack, "/name"),
            "tag": str_at(pack, "/tag"),
            "type": str_at(pack, "/type"),
            "values": str_at(pack, "/values"),
        })
    }

    /// The profile `uid` as the document of a cluster or a cluster group applies it, with the
    /// version that its template names: see [`AppliedProfile::template_of`].
    pub(crate) fn applied(doc: &Value, uid: &str) -> Option<AppliedProfile> {
        Self::template_of(doc, uid).map(Self::from_template)
    }
}

#[cfg(test)]
mod tests;
