//! The registries of Palette, read one time for each command.
//!
//! [`Registries`] holds the listings of the registries that a command reads. The registries that
//! the key can see are `GET /v1/registries/metadata`, and the summary of each kind is
//! `GET /v1/registries/{kind}/summary`. It reads each listing one time, when a check first needs
//! it. From it come the registry that a name or a uid names, the name of a uid, and the provider
//! type of an OCI registry. Also the registries that Palette does not sync, and the last sync of
//! a registry.
//!
//! The state that the listing gives a pack version, such as `deleted` or `disabled`, is here too.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use serde_json::Value;

use crate::api::{Palette, PaletteApi, RegistryKind};
use crate::diff::{RegistrySync, SyncRecovery};
use crate::error::{Error, Result, served, unread_reason};
use crate::json::{opt_text, str_at, take_items, text};
use crate::pack_type::PackType;
use crate::term::progress::Progress;

/// The state that Palette gives to the recovery of a sync when no sync waits for a recovery.
const SYNC_RECOVERED: &str = "Recovered";

/// The registry listings of one command, each read one time, when a check first needs it.
pub(crate) struct Registries<'a, A: PaletteApi> {
    palette: &'a Palette<A>,
    /// The entries of `GET /v1/registries/metadata`, after the first read.
    metadata: Option<Vec<Value>>,
    /// The entries of the summary listing of each kind, after the first read. `None` for a kind
    /// whose listing Palette does not have.
    summaries: BTreeMap<RegistryKind, Option<Vec<Value>>>,
}

impl<'a, A: PaletteApi> Registries<'a, A> {
    /// Listings that no check has read yet. `palette` reads each one.
    pub(crate) fn new(palette: &'a Palette<A>) -> Self {
        Registries {
            palette,
            metadata: None,
            summaries: BTreeMap::new(),
        }
    }

    /// The client that reads the listings.
    pub(crate) fn palette(&self) -> &'a Palette<A> {
        self.palette
    }

    /// The entries of `GET /v1/registries/metadata`: each registry that the key can see, with
    /// its `uid`, `name`, `kind`, and `isPrivate`, in the order Palette lists them.
    ///
    /// # Errors
    ///
    /// Each error of the request.
    pub(crate) fn metadata(&mut self) -> Result<&[Value]> {
        match &mut self.metadata {
            Some(entries) => Ok(entries.as_slice()),
            none => Ok(none
                .insert(take_items(self.palette.registry_metadata()?))
                .as_slice()),
        }
    }

    /// The entries of the summary listing of the registries of `kind`,
    /// `GET /v1/registries/{kind}/summary`. Each has its last sync in `status.sync`, and an OCI
    /// registry has its provider type in `spec.providerType`. `None` when Palette has no such
    /// listing: it answers HTTP 404 or 405.
    ///
    /// # Errors
    ///
    /// Each other error of the request. A 401, a 403, or a 429 is an error too, and not a
    /// listing with no entry: a record of a pack from an OCI registry holds the provider type,
    /// so a key that cannot read the listing must not change the root with no error.
    pub(crate) fn summary(&mut self, kind: RegistryKind) -> Result<Option<&[Value]>> {
        let entries = match self.summaries.entry(kind) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(v) => v.insert(served(self.palette.registry_summary(kind))?.map(take_items)),
        };
        Ok(entries.as_deref())
    }

    /// The name and the uid of each registry that the key can see, in the order Palette lists
    /// them.
    pub(crate) fn names(&mut self) -> Result<Vec<(String, String)>> {
        Ok(self
            .metadata()?
            .iter()
            .filter_map(|it| Some((opt_text(it, "/name")?, opt_text(it, "/uid")?)))
            .collect())
    }

    /// The registry that `reference` names, by name or by uid: its name and its uid.
    ///
    /// # Errors
    ///
    /// [`Error::RegistryNotFound`], with the names of the registries that the key can see, when
    /// none has the name or the uid. Each error of the request.
    pub(crate) fn by_ref(&mut self, reference: &str) -> Result<(String, String)> {
        let names = self.names()?;
        names
            .iter()
            .find(|(n, u)| n == reference || u == reference)
            .cloned()
            .ok_or_else(|| Error::RegistryNotFound {
                registry: reference.to_string(),
                available: names.iter().map(|(n, _)| n.clone()).collect(),
            })
    }

    /// The name of the registry `uid`, or `None` when the key cannot see the registry.
    pub(crate) fn name_of(&mut self, uid: &str) -> Result<Option<String>> {
        Ok(registry_entry(self.metadata()?, "/uid", uid).and_then(|it| opt_text(it, "/name")))
    }

    /// The provider type of the OCI registry `uid`, `pack`, `helm`, or `zarf`, from
    /// `spec.providerType` of the OCI summary listing. The pack listing does not carry it. `None`
    /// when Palette has no such listing, when the listing does not have the registry,
    /// and when the entry names no type.
    pub(crate) fn oci_provider_type(&mut self, uid: &str) -> Result<Option<String>> {
        Ok(self
            .summary(RegistryKind::Oci)?
            .and_then(|regs| registry_entry(regs, "/metadata/uid", uid))
            .and_then(|reg| opt_text(reg, "/spec/providerType")))
    }

    /// How Palette makes the pack of a layer from the registry `uid`, when it does not sync the
    /// registry. It does not sync a Helm registry that is private, or an OCI registry of the
    /// provider type `helm` or `zarf` with sync off. So the pack listing has no pack of it, and
    /// Palette makes the pack of such a layer when it creates the profile. The metadata gives the
    /// kind and `isPrivate`, and the OCI summary gives the provider type and
    /// `status.sync.isSyncSupported`. `None` for a registry that syncs, for a registry that the
    /// key cannot see, and when Palette has no OCI summary.
    pub(crate) fn unsynced(&mut self, uid: &str) -> Result<Option<Unsynced>> {
        let Some(meta) = registry_entry(self.metadata()?, "/uid", uid) else {
            return Ok(None);
        };
        let kind = RegistryKind::of_kind(str_at(meta, "/kind"));
        let private = meta
            .get("isPrivate")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        match kind {
            Some(RegistryKind::Helm) => Ok(private.then_some(Unsynced {
                kind: RegistryKind::Helm,
                provider: None,
            })),
            Some(RegistryKind::Oci) => {
                let Some(regs) = self.summary(RegistryKind::Oci)? else {
                    return Ok(None);
                };
                let Some(reg) = registry_entry(regs, "/metadata/uid", uid) else {
                    return Ok(None);
                };
                // Palette syncs each OCI registry of the provider type pack
                let provider = opt_text(reg, "/spec/providerType");
                let syncs = provider.as_deref() == Some("pack")
                    || reg
                        .pointer("/status/sync/isSyncSupported")
                        .and_then(Value::as_bool)
                        == Some(true);
                Ok((!syncs).then_some(Unsynced {
                    kind: RegistryKind::Oci,
                    provider,
                }))
            }
            _ => Ok(None),
        }
    }

    /// The names of the registries of `registries` (name and uid) that Palette does not sync.
    ///
    /// An error that finds no pack names them: Palette lists a pack of such a registry only while
    /// a profile pins the pack. The rule of [`Registries::unsynced`] decides for each registry.
    pub(crate) fn unsynced_names<'r>(
        &mut self,
        registries: impl IntoIterator<Item = &'r (String, String)>,
    ) -> Result<Vec<String>> {
        let mut names = Vec::new();
        for (name, uid) in registries {
            if self.unsynced(uid)?.is_some() {
                names.push(name.clone());
            }
        }
        Ok(names)
    }

    /// The last sync of the registry `uid`, which holds a pack of type `pack_type`. It comes from
    /// `status.sync` of the registry in the summary listing of its kind. The type `spectro` reads
    /// `GET /v1/registries/pack/summary`, and `helm` and `oci` read the same route under their
    /// own kind. A summary listing gives the sync as Palette stores it. `None` for a registry
    /// that does not sync, for a manifest pack, and for a type that is no pack type: `pack_type`
    /// is then `None`. Also `None` for a registry that the listing does not have, and when
    /// Palette has no such listing.
    pub(crate) fn sync(
        &mut self,
        uid: &str,
        pack_type: Option<PackType>,
    ) -> Result<Option<RegistrySync>> {
        let Some(kind) = pack_type.and_then(RegistryKind::of_pack_type) else {
            return Ok(None);
        };
        let Some(entries) = self.summary(kind)? else {
            return Ok(None);
        };
        let Some(status) =
            registry_entry(entries, "/metadata/uid", uid).and_then(|e| e.pointer("/status/sync"))
        else {
            return Ok(None);
        };
        if status.get("isSyncSupported").and_then(Value::as_bool) != Some(true) {
            return Ok(None);
        }
        // Palette writes the zero time for a registry that never completed a sync
        let time = |ptr: &str| {
            opt_text(status, ptr)
                .filter(|t| !t.starts_with("0001-"))
                .unwrap_or_default()
        };
        let sync_recovery = opt_text(status, "/syncRecovery/syncRecoveryState")
            .filter(|s| s != SYNC_RECOVERED)
            .map(|state| SyncRecovery {
                sync_recovery_state: state,
                message: text(status, "/syncRecovery/message"),
            });
        Ok(Some(RegistrySync {
            last_synced_time: time("/lastSyncedTime"),
            last_run_time: time("/lastRunTime"),
            status: text(status, "/status"),
            message: text(status, "/message"),
            sync_recovery,
        }))
    }

    /// The last sync of the registry `uid`, which holds a pack of type `pack_type`, as a note
    /// beside an answer that stands on its own. The answer is the verdict of a pack record, or a
    /// pin of `behind`. An error of Palette, such as a 403 or a 5xx, and a request that does
    /// not complete do not stop the answer. The note then names the reason, and `progress` gets
    /// a line. Each other error is an error of the run.
    pub(crate) fn sync_note(
        &mut self,
        uid: &str,
        pack_type: Option<PackType>,
        progress: &Progress,
    ) -> Result<RegistrySyncNote> {
        match self.sync(uid, pack_type) {
            Ok(sync) => Ok(RegistrySyncNote { sync, error: None }),
            Err(e @ (Error::Api { .. } | Error::Transport { .. })) => {
                let reason = match &e {
                    Error::Api { status, body, .. } => unread_reason(*status, body),
                    _ => e.to_string(),
                };
                progress.note(&format!(
                    "note: the sync status of registry {uid} is unavailable ({reason})"
                ));
                Ok(RegistrySyncNote {
                    sync: None,
                    error: Some(reason),
                })
            }
            Err(e) => Err(e),
        }
    }
}

/// The entry of the registry `uid` in `entries`, the items of a registry listing, where `ptr`
/// points to the uid of each entry: `/uid` in the metadata listing, `/metadata/uid` in a summary.
pub(crate) fn registry_entry<'e>(entries: &'e [Value], ptr: &str, uid: &str) -> Option<&'e Value> {
    entries.iter().find(|it| str_at(it, ptr) == uid)
}

/// How Palette makes the pack of a layer from a registry that it does not sync.
#[derive(Debug, Clone)]
pub(crate) struct Unsynced {
    /// The kind of the registry, which gives the pack its type: `helm` or `oci`.
    pub kind: RegistryKind,
    /// The provider type of an OCI registry: `helm` or `zarf`.
    pub provider: Option<String>,
}

/// The last sync of a registry, read as a note beside an answer.
#[derive(Debug, Clone)]
pub(crate) struct RegistrySyncNote {
    /// The sync, when Palette gives one: see [`Registries::sync`].
    pub sync: Option<RegistrySync>,
    /// Why strata has no sync: Palette answered with an error, such as a 403 or a 5xx, or the
    /// request did not complete. `sync` is then `None`.
    pub error: Option<String>,
}

/// The annotation of a pack version that a profile must not hold, with the value `true`.
/// Spectro Cloud sets it on a version in its registries, and the version keeps no `system_state`.
/// Palette refuses a profile that holds such a version with `DisabledPacksForbidden`, also on
/// its validate route.
const DISABLED_MARK: &str = "/spec/annotations/disabled";

/// The state that the listing gives a pack version, and nothing for a pack in use.
///
/// The state is the `system_state`: `deprecated`, `disabled`, or `deleted`. It is `disabled` for
/// a version with the [`DISABLED_MARK`] and no `system_state`.
pub(crate) fn pack_state(summary: &Value) -> Option<String> {
    system_state(summary)
        .or_else(|| (str_at(summary, DISABLED_MARK) == "true").then(|| "disabled".to_string()))
}

/// The state that Spectro Cloud gives a version that it retired: `deprecated`, `disabled`, or
/// `deleted`.
fn system_state(summary: &Value) -> Option<String> {
    opt_text(summary, "/spec/annotations/system_state")
}

/// True when `state` is a state of a version that Palette no longer serves: `disabled` or
/// `deleted`. [`retired_state`] and [`PackBuild::retired`](crate::ops::PackBuild::retired) keep
/// this one rule.
pub(crate) fn is_retired(state: &str) -> bool {
    matches!(state, "disabled" | "deleted")
}

/// The state of a pack version that Palette no longer serves: `disabled` or `deleted` in
/// `system_state`. Palette serves a version with the [`DISABLED_MARK`] alone, and its validate
/// route refuses a profile that holds it. So strata does not refuse that version itself.
pub(crate) fn retired_state(summary: &Value) -> Option<String> {
    system_state(summary).filter(|s| is_retired(s))
}

/// The type of the pack of `summary`, an entry of the pack listing, from its `spec.type`.
///
/// A registry gives each pack the type of its kind: `spectro`, `helm`, or `oci`. The API of
/// Palette names one more type, `manifest`, for a pack from no registry. An entry with no type,
/// or with a text that is none of the four, is a pack of a pack registry: `spectro`. A record, a
/// listing of `packs`, and a sync note of `behind` all read the type here, so they agree.
pub(crate) fn pack_type_of(summary: &Value) -> PackType {
    PackType::parse(str_at(summary, "/spec/type")).unwrap_or(PackType::Spectro)
}

/// The layer of the pack of `summary`, an entry of the pack listing, else of `tags`, the answers
/// of the registry route of the pack. `None` when neither gives a layer.
///
/// The listing gives the layer of each version. The registry route gives one layer for all the
/// versions of the pack: the layer of one of them. So the layer of the version comes first.
pub(crate) fn registry_layer(summary: &Value, tags: &[Value]) -> Option<String> {
    opt_text(summary, "/spec/layer").or_else(|| tags.iter().find_map(|t| opt_text(t, "/layer")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::MemoryApi;
    use serde_json::json;

    const METADATA: &str = "/v1/registries/metadata";
    const OCI_SUMMARY: &str = "/v1/registries/oci/summary";

    /// A summary listing of registries that holds the one registry `uid` with the sync `sync`.
    fn summary(uid: &str, sync: Value) -> Value {
        json!({"items": [{"metadata": {"uid": uid}, "status": {"sync": sync}}]})
    }

    /// A tenant with a pack registry, a private Helm registry, and an OCI registry of the
    /// provider type zarf with sync off.
    fn tenant() -> MemoryApi {
        MemoryApi::new()
            .with(
                METADATA,
                json!({"items": [
                    {"name": "Public Repo", "uid": "reg-public", "kind": "pack"},
                    {"name": "charts-auth", "uid": "reg-auth", "kind": "helm", "isPrivate": true},
                    {"name": "zarf", "uid": "reg-zarf", "kind": "oci"},
                ]}),
            )
            .with(
                OCI_SUMMARY,
                json!({"items": [{"metadata": {"uid": "reg-zarf"},
                    "spec": {"providerType": "zarf"},
                    "status": {"sync": {"isSyncSupported": false}}}]}),
            )
    }

    #[test]
    fn the_listings_are_read_one_time_and_give_the_registry_of_a_name_or_a_uid() {
        let api = tenant();
        let palette = Palette::new(&api);
        let mut registries = Registries::new(&palette);
        assert_eq!(
            registries.by_ref("zarf").unwrap(),
            ("zarf".to_string(), "reg-zarf".to_string())
        );
        assert_eq!(registries.by_ref("reg-auth").unwrap().0, "charts-auth");
        let err = registries.by_ref("nope").unwrap_err();
        assert!(matches!(err, Error::RegistryNotFound { .. }), "{err}");
        assert!(
            err.to_string().contains("Public Repo, charts-auth, zarf"),
            "the names in the order of the listing: {err}"
        );
        assert_eq!(
            registries.name_of("reg-public").unwrap().as_deref(),
            Some("Public Repo")
        );
        assert_eq!(registries.name_of("reg-gone").unwrap(), None);
        assert_eq!(registries.names().unwrap().len(), 3);
        assert_eq!(
            registries.oci_provider_type("reg-zarf").unwrap().as_deref(),
            Some("zarf")
        );
        assert_eq!(registries.oci_provider_type("reg-public").unwrap(), None);
        let unsynced = registries.unsynced("reg-zarf").unwrap().unwrap();
        assert_eq!(
            (unsynced.kind, unsynced.provider.as_deref()),
            (RegistryKind::Oci, Some("zarf"))
        );
        assert_eq!(
            registries.unsynced("reg-auth").unwrap().unwrap().kind,
            RegistryKind::Helm
        );
        assert!(registries.unsynced("reg-public").unwrap().is_none());
        let names = registries.names().unwrap();
        assert_eq!(
            registries.unsynced_names(&names).unwrap(),
            vec!["charts-auth", "zarf"]
        );
        assert_eq!(api.calls(METADATA), 1, "one read of the metadata");
        assert_eq!(api.calls(OCI_SUMMARY), 1, "one read of the OCI summary");
    }

    #[test]
    fn a_summary_that_palette_does_not_serve_has_no_entry_and_each_other_error_is_an_error() {
        let api = MemoryApi::new()
            .with(METADATA, json!({"items": []}))
            .failing("/v1/registries/pack/summary", 500, "{}")
            .failing("/v1/registries/helm/summary", 405, "{}");
        let palette = Palette::new(&api);
        let mut registries = Registries::new(&palette);
        // the OCI summary is not registered: a 404
        assert!(registries.summary(RegistryKind::Oci).unwrap().is_none());
        assert!(registries.summary(RegistryKind::Oci).unwrap().is_none());
        assert_eq!(api.calls(OCI_SUMMARY), 1, "a 404 is read one time too");
        assert!(registries.summary(RegistryKind::Helm).unwrap().is_none());
        let err = registries.summary(RegistryKind::Pack).unwrap_err();
        assert!(matches!(err, Error::Api { status: 500, .. }), "{err}");
        // the key or the rate is no listing with no entry: the provider type of an OCI pack is
        // in its record
        for status in [401, 403, 429] {
            let refused = Palette::new(MemoryApi::new().failing(OCI_SUMMARY, status, "{}"));
            let err = Registries::new(&refused)
                .oci_provider_type("reg-zarf")
                .unwrap_err();
            assert!(matches!(err, Error::Api { status: s, .. } if s == status), "{err}");
        }
        // a 404 of the metadata is an error: each Palette has the listing
        let none = Palette::new(MemoryApi::new());
        let err = Registries::new(&none).metadata().unwrap_err();
        assert!(matches!(err, Error::Api { status: 404, .. }), "{err}");
    }

    #[test]
    fn the_sync_of_a_registry_is_read_from_the_summary_listing_of_its_kind() {
        let completed = json!({"isSyncSupported": true, "status": "Completed",
            "lastSyncedTime": "2026-09-30T00:00:11.000Z",
            "syncRecovery": {"syncRecoveryState": "Recovered"}});
        let api = MemoryApi::new()
            .with(
                "/v1/registries/pack/summary",
                summary("reg-pack", completed.clone()),
            )
            .with(
                "/v1/registries/helm/summary",
                summary("reg-helm", completed.clone()),
            )
            .with(OCI_SUMMARY, summary("reg-oci", completed))
            // the route of one registry is not read
            .failing("/v1/registries/oci/reg-oci/basic/sync/status", 500, "{}");
        let palette = Palette::new(api);
        let mut registries = Registries::new(&palette);
        for (uid, pack_type) in [
            ("reg-pack", PackType::Spectro),
            ("reg-helm", PackType::Helm),
            ("reg-oci", PackType::Oci),
        ] {
            let sync = registries.sync(uid, Some(pack_type)).unwrap().unwrap();
            assert_eq!(sync.last_synced_time, "2026-09-30T00:00:11.000Z");
            // a recovery that is Recovered is no recovery that waits
            assert_eq!(sync.sync_recovery, None);
        }
        // a manifest pack has no registry, and a tier type is no pack type; the listing of the
        // kind does not have the registry
        for pack_type in [Some(PackType::Manifest), None] {
            assert!(registries.sync("reg-oci", pack_type).unwrap().is_none());
        }
        assert!(
            registries
                .sync("reg-pack", Some(PackType::Oci))
                .unwrap()
                .is_none()
        );
        // Palette answers no listing
        let none = Palette::new(MemoryApi::new());
        assert!(
            Registries::new(&none)
                .sync("reg-oci", Some(PackType::Oci))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_registry_that_never_completed_a_sync_gives_no_time() {
        let never = Palette::new(MemoryApi::new().with(
            "/v1/registries/helm/summary",
            summary(
                "reg-helm",
                json!({"isSyncSupported": true, "status": "InProgress",
                    "lastRunTime": "2026-10-05T16:03:58.598Z",
                    "lastSyncedTime": "0001-01-01T00:00:00.000Z",
                    "syncRecovery": {"syncRecoveryState": "Recovering",
                        "message": "sync interrupted"}}),
            ),
        ));
        let sync = Registries::new(&never)
            .sync("reg-helm", Some(PackType::Helm))
            .unwrap()
            .unwrap();
        assert_eq!(sync.last_synced_time, "");
        assert_eq!(sync.last_run_time, "2026-10-05T16:03:58.598Z");
        assert!(sync.in_progress());
        assert_eq!(
            sync.sync_recovery,
            Some(SyncRecovery {
                sync_recovery_state: "Recovering".into(),
                message: "sync interrupted".into(),
            })
        );
    }

    #[test]
    fn a_version_with_the_disabled_mark_and_no_system_state_is_disabled_and_not_retired() {
        let marked = json!({"spec": {"annotations": {"disabled": "true"}}});
        assert_eq!(pack_state(&marked).as_deref(), Some("disabled"));
        assert_eq!(
            retired_state(&marked),
            None,
            "Palette serves the version, and its validate route refuses the profile"
        );
        let deleted =
            json!({"spec": {"annotations": {"disabled": "true", "system_state": "deleted"}}});
        assert_eq!(pack_state(&deleted).as_deref(), Some("deleted"));
        assert_eq!(retired_state(&deleted).as_deref(), Some("deleted"));
        for summary in [
            json!({"spec": {"annotations": {"disabled": "false"}}}),
            json!({"spec": {"annotations": {"disabled": true}}}),
            json!({"spec": {"annotations": {"system_state": ""}}}),
            json!({"spec": {}}),
        ] {
            assert_eq!(pack_state(&summary), None, "{summary}");
            assert_eq!(retired_state(&summary), None, "{summary}");
        }
    }
}
