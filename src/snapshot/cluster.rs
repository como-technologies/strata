//! The record of a cluster: its source, its tiers, the profiles that it applies, its variables,
//! and its pending updates.

use std::collections::BTreeMap;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};

use super::{SubjectKind, macro_placeholder};
use crate::api::{RawCluster, profile_variables};
use crate::closed_set::{ClosedSet, Open, impl_closed_set};
use crate::content::canon::{canonical_json, sha256_hex};
use crate::json::{arr, flag, object_at, str_at, text};
use crate::profile_type::ProfileType;
use crate::repave::PendingRepave;
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

/// The cluster of a record of any release. It has the fields of [`ClusterSource`], and the two
/// fields of strata 0.24 and before that held a read error. Each older field has the key of
/// [`ClusterSource::errors`] that holds the error now. Do not remove a field: a record of an
/// older release needs it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClusterRecord {
    #[serde(flatten)]
    cluster: ClusterSource,
    updates_pending_error: Option<String>,
    pending_repave_error: Option<String>,
}

impl From<ClusterRecord> for ClusterSource {
    /// The cluster with the older read errors in `errors`. An entry that `errors` holds already
    /// stays. An older field that holds null is no error.
    fn from(record: ClusterRecord) -> ClusterSource {
        let ClusterRecord {
            mut cluster,
            updates_pending_error,
            pending_repave_error,
        } = record;
        for (why, read) in [
            (updates_pending_error, ClusterRead::UpdatesPending),
            (pending_repave_error, ClusterRead::PendingRepave),
        ] {
            if let Some(why) = why {
                cluster.errors.entry(read.into()).or_insert(why);
            }
        }
        cluster
    }
}

/// Read the field `cluster` of [`Source`](super::Source) from a record of any release.
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
    Ok(Option::<ClusterRecord>::deserialize(deserializer)?.map(ClusterSource::from))
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
pub fn cluster_tiers(
    raw: &RawCluster,
    covered: &[AppliedProfile],
) -> BTreeMap<Open<Tier>, ClusterTier> {
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
            ClusterTier::of(crate::snapshot::policy::group_fields(config)),
        );
    }
    if let Some(cc) = &raw.cloud_config {
        tiers.insert(
            Tier::Shape.into(),
            ClusterTier::of(crate::snapshot::shape::fields(cluster, cc)),
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
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_document_applies_the_profile_of_a_uid_with_the_version_that_its_template_names() {
        let doc = json!({"spec": {"clusterProfileTemplates": [
            {"uid": "p1", "name": "lab", "profileVersion": "1.2.0", "type": "cluster",
             "version": 3},
            {"uid": "p2", "name": "addons", "profileVersion": "2.0.0", "type": "add-on",
             "version": 1},
        ]}});
        let applied = AppliedProfile::applied(&doc, "p2").expect("the document applies p2");
        assert_eq!(
            (
                applied.uid.as_str(),
                applied.name.as_str(),
                applied.version.as_str(),
                applied.kind.as_str()
            ),
            ("p2", "addons", "2.0.0", "add-on"),
            "the version is the profile version, not the version of the template"
        );
        assert_eq!(
            AppliedProfile::template_of(&doc, "p1").map(|t| t["version"].clone()),
            Some(json!(3))
        );
        assert!(AppliedProfile::applied(&doc, "p3").is_none());
        assert!(AppliedProfile::applied(&json!({}), "p1").is_none());
    }

    #[test]
    fn a_pack_of_a_template_is_an_entry_of_pack_values_with_empty_text_for_a_missing_field() {
        let doc = json!({"spec": {"clusterProfileTemplates": [
            {"uid": "p1", "packs": [
                {"name": "kubernetes", "tag": "1.32", "type": "spectro", "values": "a: 1\n",
                 "manifests": [{"name": "m"}]},
                {"name": "manifest-pack", "type": "manifest"},
            ]},
        ]}});
        let packs = AppliedProfile::template_packs(&doc, "p1");
        assert_eq!(packs.len(), 2);
        assert_eq!(
            AppliedProfile::template_pack_value(&packs[0]),
            json!({"name": "kubernetes", "tag": "1.32", "type": "spectro", "values": "a: 1\n"})
        );
        assert_eq!(
            AppliedProfile::template_pack_value(&packs[1]),
            json!({"name": "manifest-pack", "tag": "", "type": "manifest", "values": ""})
        );
        assert!(AppliedProfile::template_packs(&doc, "p2").is_empty());
    }

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

    /// The field `cluster` of a `Source`, as serde reads it from `record`.
    fn try_read_cluster(record: Value) -> Result<Option<ClusterSource>, serde_json::Error> {
        #[derive(Deserialize)]
        struct Holder {
            #[serde(deserialize_with = "deserialize_cluster")]
            cluster: Option<ClusterSource>,
        }
        serde_json::from_value::<Holder>(json!({"cluster": record})).map(|h| h.cluster)
    }

    fn read_cluster(record: Value) -> Option<ClusterSource> {
        try_read_cluster(record).unwrap()
    }

    /// A cluster object with the fields that a record must hold, and `more`.
    fn cluster_object(more: Value) -> Value {
        let mut cluster = json!({"uid": "c1", "name": "payments", "profiles": []});
        cluster
            .as_object_mut()
            .unwrap()
            .extend(more.as_object().unwrap().clone());
        cluster
    }

    #[test]
    fn the_read_errors_of_an_older_record_move_into_errors_and_an_entry_of_errors_stays() {
        let older = read_cluster(cluster_object(json!({
            "updatesPendingError": "HTTP 404: not found",
            "pendingRepaveError": "HTTP 403: forbidden",
        })))
        .unwrap();
        assert_eq!(
            serde_json::to_value(&older.errors).unwrap(),
            json!({
                "updatesPending": "HTTP 404: not found",
                "pendingRepave": "HTTP 403: forbidden",
            })
        );
        assert_eq!(older.uid, "c1");

        // a record that holds both shapes keeps the entry of the new one
        let both = read_cluster(cluster_object(json!({
            "pendingRepaveError": "HTTP 500: older",
            "errors": {"pendingRepave": "HTTP 403: newer", "updatesPending": "HTTP 404: x"},
        })))
        .unwrap();
        assert_eq!(
            serde_json::to_value(&both.errors).unwrap(),
            json!({"pendingRepave": "HTTP 403: newer", "updatesPending": "HTTP 404: x"})
        );
    }

    #[test]
    fn a_cluster_that_a_record_writes_now_reads_with_the_errors_it_holds_and_writes_the_same() {
        let current = cluster_object(json!({
            "updatesPending": true,
            "errors": {"network": "HTTP 500: later", "pendingRepave": "HTTP 403: forbidden"},
        }));
        let read = read_cluster(current.clone()).unwrap();
        assert_eq!(read.errors.len(), 2);
        assert_eq!(serde_json::to_value(&read).unwrap(), current);

        let none = read_cluster(cluster_object(json!({}))).unwrap();
        assert!(none.errors.is_empty());
        assert_eq!(read_cluster(Value::Null).map(|c| c.uid), None);
    }

    #[test]
    fn an_older_field_that_holds_null_is_no_error() {
        let read = read_cluster(cluster_object(json!({"updatesPendingError": null}))).unwrap();
        assert!(read.errors.is_empty());
    }

    #[test]
    fn an_older_field_that_is_not_text_is_an_error_of_the_record() {
        let err = try_read_cluster(cluster_object(json!({"pendingRepaveError": 7}))).unwrap_err();
        assert!(err.to_string().contains("invalid type: integer"), "{err}");
    }

    #[test]
    fn the_errors_of_a_cluster_name_the_field_that_strata_could_not_read() {
        let reasons = [
            (
                Open::Known(ClusterRead::UpdatesPending),
                &Some("HTTP 404: not found".to_string()),
            ),
            (Open::Known(ClusterRead::PendingRepave), &None),
        ];
        assert_eq!(
            unread(reasons),
            BTreeMap::from([(
                Open::Known(ClusterRead::UpdatesPending),
                "HTTP 404: not found".to_string()
            )])
        );
    }

    #[test]
    fn the_errors_of_a_record_keep_a_part_that_a_later_release_names() {
        let errors: BTreeMap<Open<ClusterRead>, String> = serde_json::from_value(json!({
            "network": "HTTP 500: later",
            "pendingRepave": "HTTP 403: forbidden",
        }))
        .unwrap();
        let known: Vec<Option<ClusterRead>> = errors.keys().map(Open::known).collect();
        assert_eq!(known, [None, Some(ClusterRead::PendingRepave)]);
        assert_eq!(
            serde_json::to_value(&errors).unwrap(),
            json!({"network": "HTTP 500: later", "pendingRepave": "HTTP 403: forbidden"})
        );
    }
}
