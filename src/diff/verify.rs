//! The verify report: the state of the live object, the sync of its registries, and the audit
//! log.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ImageIdChange;
use crate::json::{is_zero, str_at};
use crate::snapshot::Snapshot;

/// The outcome of `verify`, present only in its documents.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Verify {
    /// Unchanged or drifted.
    pub status: VerifyStatus,
    /// Profile uid that was re-fetched.
    pub uid: String,
    /// When the record was taken.
    pub recorded_at: String,
    /// When the live profile was fetched.
    pub checked_at: String,
    /// For a profile record whose uid now names another version: that `name@version`. The
    /// recorded version was overwritten in place, so `status` is drifted even when the roots
    /// match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overwritten_by: Option<String>,
    /// For a profile record whose uid now names the same version under another name: that
    /// `name@version`. The version was renamed, so `status` is drifted even when the roots match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renamed_to: Option<String>,
    /// Palette no longer has the object of the record: its uid is gone, or, for a cluster, the
    /// document has the state `Deleted` or `metadata.isDeleted`. `status` is drifted, and each
    /// pack of the record is removed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gone: bool,
    /// For a gone object whose document Palette keeps: when it was deleted, from
    /// `metadata.deletionTimestamp`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
    /// For a record of a cluster, an edge host, or a cluster group: the state of the object now,
    /// as Palette gives it. `status.state` of a cluster (such as `Running`, `Deleting`, `Error`) or
    /// of an edge host (`ready`, `unpaired`, `in-use`); `active` or `inactive` from
    /// `status.isActive` of a cluster group. Absent when Palette gives none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// For a cluster record: each profile the cluster runs whose uid now names another version
    /// than the cluster's copy, or no longer exists. The cluster may run exactly what it ran, but
    /// the version it runs is gone from Palette, so `status` is drifted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles_overwritten: Vec<ClusterProfileOverwritten>,
    /// For a cluster record: how many values the cluster gives its variables differ from the
    /// record. They do not change `status`.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub cluster_values_changed: usize,
    /// With `--who`, for a drifted record: each write to the object of the record that the audit
    /// log of Palette holds from `recordedAt`, oldest first. For a cluster record, also each write
    /// to a profile of `profilesOverwritten`: the write that overwrote the version is an entry of
    /// the profile, not of the cluster. Empty when the log holds none. Absent without `--who`, for
    /// an unchanged record, and for a pack record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub who: Option<Vec<AuditEntry>>,
    /// When Palette last changed the object of the record, from `metadata.lastModifiedTimestamp`
    /// of the document that `verify` reads: the profile, the app profile, the cluster, the edge
    /// host, or the cluster group. It costs no request. Absent when Palette gives no time, when
    /// the object is gone, and for a pack record: the pack listing gives the create time there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    /// For an `unverified` record: the key of each pack with no upstream digest.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upstream_unknown: Vec<String>,
    /// For a record that holds the digests of its macro values: each macro whose value is another
    /// one now. The status is `drifted`, also when the roots are equal.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub macros_changed: Vec<String>,
    /// When the roots are equal: each pack that names another machine image now, in the
    /// annotation `imageId`. The status is `drifted`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub image_ids_changed: Vec<ImageIdChange>,
    /// Each pack that Palette marks invalid now, with its reason: the registry deleted its tag.
    /// It does not change `status`: the content is still the recorded content.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalid_packs: Vec<InvalidPack>,
    /// For a pack record: the last sync of the registry, as Palette gives it. Palette holds a copy
    /// of each registry, and the verdict states that copy. Absent when the registry does not sync,
    /// or when Palette does not answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_sync: Option<RegistrySync>,
    /// For a pack record: why strata could not read the last sync of the registry, when Palette
    /// answered with a 5xx or the request did not complete. `registrySync` is then absent. It does
    /// not change `status`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_sync_error: Option<String>,
}

impl Verify {
    /// The verdict of the record `recorded` against `live`, with no drift named yet. The status
    /// is `unchanged` or `drifted`, as `unchanged` says. The verdict holds the two times, and
    /// each pack that Palette marks invalid now. Each other field is empty: `verify` sets what
    /// it found.
    pub(crate) fn new(
        unchanged: bool,
        uid: &str,
        recorded: &Snapshot,
        live: &Snapshot,
        checked_at: &str,
    ) -> Verify {
        Verify {
            status: if unchanged {
                VerifyStatus::Unchanged
            } else {
                VerifyStatus::Drifted
            },
            uid: uid.to_string(),
            recorded_at: recorded.source.fetched_at.clone(),
            checked_at: checked_at.to_string(),
            overwritten_by: None,
            renamed_to: None,
            gone: false,
            deleted_at: None,
            state: None,
            profiles_overwritten: Vec::new(),
            cluster_values_changed: 0,
            who: None,
            last_modified: None,
            upstream_unknown: Vec::new(),
            macros_changed: Vec::new(),
            image_ids_changed: Vec::new(),
            invalid_packs: invalid_packs(live),
            registry_sync: None,
            registry_sync_error: None,
        }
    }
}

/// The status that Palette gives to a sync of a registry that completed.
const SYNC_COMPLETED: &str = "Completed";
/// The status that Palette gives to a sync of a registry that runs.
const SYNC_IN_PROGRESS: &str = "InProgress";
/// The status that Palette gives to a sync of a registry that failed.
const SYNC_FAILED: &str = "Failed";

/// The last sync of a registry: `status.sync` of the registry in Palette, `v1RegistrySyncStatus`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySync {
    /// When the last sync that completed started, as Palette gives it in `lastSyncedTime`. Empty
    /// when no sync completed.
    pub last_synced_time: String,
    /// When the last sync started, also a sync that runs or that failed, as Palette gives it in
    /// `lastRunTime`. Empty when Palette gives no time.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_run_time: String,
    /// The state of the last sync: `Completed`, `InProgress`, or `Failed`, as Palette gives it.
    pub status: String,
    /// What Palette says about the last sync, such as the cause of a failure. Empty when it says
    /// nothing.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
    /// The recovery of a sync that stopped before its end, as Palette gives it in
    /// `syncRecovery`. Absent when Palette gives none, and when the recovery is `Recovered`: then
    /// no sync waits for a recovery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_recovery: Option<SyncRecovery>,
}

impl RegistrySync {
    /// Whether the last sync of the registry completed: the copy of the registry in Palette is
    /// then the copy of the last run.
    pub fn completed(&self) -> bool {
        self.status == SYNC_COMPLETED
    }

    /// Whether a sync of the registry runs now.
    pub fn in_progress(&self) -> bool {
        self.status == SYNC_IN_PROGRESS
    }

    /// Whether the last sync of the registry failed.
    pub fn failed(&self) -> bool {
        self.status == SYNC_FAILED
    }
}

/// The recovery of a sync of a registry that stopped before its end: `v1SyncRecovery`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SyncRecovery {
    /// The state of the recovery: `Interrupted` or `Recovering`, as Palette gives it.
    pub sync_recovery_state: String,
    /// What Palette says about the recovery. Empty when it says nothing.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
}

/// One write to an object, as the audit log of Palette holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    /// When Palette logged the write, as Palette gives it.
    pub time: String,
    /// Who wrote: the name of a user, else the email or the uid of the user; or the name of a
    /// Palette service, such as `system` or `cluster-agent`.
    pub actor: String,
    /// `user` or `service`, as Palette gives it.
    pub actor_type: String,
    /// The uid of the user; empty for a service.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub actor_uid: String,
    /// `create`, `update`, `delete`, `activity`, or `action`, as Palette gives it.
    pub action: String,
    /// What Palette says changed, when it says it, such as the manifest that an edit changed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
    /// The note that a person adds to the entry with `PATCH /v1/audits/{uid}/userMsg`, such as
    /// the ticket of the change. Empty when the entry has no note.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user_msg: String,
    /// The object that the write touched, from `spec.resource` of the entry. Absent when Palette
    /// gives none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<AuditResource>,
    /// The uid of the entry: `GET /v1/audits/{uid}` reads it again. Palette gives no request id.
    pub audit_uid: String,
}

/// The object of an audit entry: `spec.resource` of the entry, `v1AuditResourceReference`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditResource {
    /// The kind of the object in the log: `clusterprofile`, `appprofile`, `spectrocluster`,
    /// `edgehost`, or `clustergroup`, as Palette gives it.
    pub kind: String,
    /// The name of the object, as Palette gives it. Empty when it gives none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The uid of the object.
    pub uid: String,
}

impl AuditEntry {
    /// Read one item of `GET /v1/audits`.
    pub fn from_palette(item: &Value) -> AuditEntry {
        // no space around a name or a message in the record
        let text = |ptr: &str| str_at(item, ptr).trim().to_string();
        let actor_type = text("/spec/actor/actorType");
        let full_name = format!(
            "{} {}",
            text("/spec/actor/user/firstName"),
            text("/spec/actor/user/lastName")
        );
        let actor = [
            full_name.trim().to_string(),
            text("/spec/actor/user/emailId"),
            text("/spec/actor/user/uid"),
            text("/spec/actor/serviceName"),
        ]
        .into_iter()
        .find(|s| !s.is_empty())
        .unwrap_or_else(|| actor_type.clone());
        AuditEntry {
            time: text("/metadata/creationTimestamp"),
            actor,
            actor_type,
            actor_uid: text("/spec/actor/user/uid"),
            action: text("/spec/actionType"),
            // a summary can span lines, as a list of the values that changed does
            summary: text("/spec/contentMsg")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
            user_msg: text("/spec/userMsg"),
            resource: item
                .pointer("/spec/resource")
                .filter(|r| r.is_object())
                .map(|_| AuditResource {
                    kind: text("/spec/resource/kind"),
                    name: text("/spec/resource/name"),
                    uid: text("/spec/resource/uid"),
                }),
            audit_uid: text("/metadata/uid"),
        }
    }
}

/// A profile a cluster runs whose uid no longer names the version the cluster's copy names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterProfileOverwritten {
    /// The profile uid.
    pub uid: String,
    /// The profile name, as the cluster's copy names it.
    pub name: String,
    /// The version the cluster's copy names.
    pub cluster_version: String,
    /// The version the uid names now; empty when the uid no longer exists.
    pub profile_version: String,
}

/// Whether a published version still matches its record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VerifyStatus {
    /// Same root.
    Unchanged,
    /// Different root: the published version was edited, or the record is stale.
    Drifted,
    /// Same content, and a pack has no upstream digest, so a change of its archive would not
    /// show. `--allow-no-digest` makes it `unchanged`.
    Unverified,
}

/// One pack that Palette marks invalid: see [`Verify::invalid_packs`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvalidPack {
    /// `<layer>/<name>` of the pack.
    pub key: String,
    /// Why Palette marks it invalid, as Palette gives it. Empty when Palette gives no reason.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
}

/// Each pack of `s` that Palette marks invalid, in pack order.
pub(crate) fn invalid_packs(s: &Snapshot) -> Vec<InvalidPack> {
    s.packs
        .iter()
        .filter(|p| p.invalid)
        .map(|p| InvalidPack {
            key: p.key.clone(),
            reason: p.invalid_reason.clone(),
        })
        .collect()
}
