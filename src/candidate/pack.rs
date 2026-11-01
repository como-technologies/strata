//! The versions of a pack in the registries, and the record of one pack version.

use std::path::PathBuf;

use serde_json::{Map, Value};

use super::resolve::resolve_with;
use super::{Candidate, CandidateLayer, LoadedCandidate, LoadedLayer, RegistryRef};
use crate::api::registry::Registries;
use crate::api::{Palette, PaletteApi};
use crate::closed_set::ClosedSet;
use crate::error::{Error, Result};
use crate::json::{str_at, text};
use crate::model::profile_type::ProfileType;
use crate::reference::{LATEST, name_version};
use crate::snapshot::{PackSource, Snapshot, Template};
use crate::term::progress::Progress;
use crate::text::name_or_uid;

/// Read `/v1/packs?filters=spec.name=<name>` and give each entry to `each`. Palette serves this
/// listing 100 entries at a time, so one pack name can take several requests. `each` returns false
/// to stop the walk.
fn each_pack_named<A: PaletteApi>(
    palette: &Palette<A>,
    name: &str,
    each: impl FnMut(Value) -> bool,
) -> Result<()> {
    palette.each_offset_page(
        crate::api::PACK_LISTING_PAGE,
        |offset| palette.pack_listing_page(name, offset),
        each,
    )
}

/// Every version of `name` the pack listing returns, across registries. The listing caps `limit`
/// at 100, so this pages until it has them all.
pub(crate) fn versions_of<A: PaletteApi>(palette: &Palette<A>, name: &str) -> Result<Vec<Value>> {
    let mut out: Vec<Value> = Vec::new();
    each_pack_named(palette, name, |it| {
        if str_at(&it, "/spec/name") == name {
            out.push(it);
        }
        true
    })?;
    Ok(out)
}

/// Record one pack version as a snapshot of a single layer, with the values the registry ships.
///
/// Its pack hashes the same way as that pack inside a profile, so comparing two of these says
/// what changed between two versions of a pack: the version and uid in `IDENTITY`, the content
/// digest in `UPSTREAM`, and the default values in `AUTHORED`.
///
/// `registry` is the name or the uid of the registry. Without it, strata uses the registry that
/// has the pack version, and refuses when several do. `version` can be [`LATEST`]. `reference` is
/// the ref that the record gives as its source, and `fetched_at` is the time of the read, in
/// RFC 3339.
///
/// # Errors
///
/// [`Error::PackNotFound`] when the registry does not have the pack version, and
/// [`Error::PackInSeveralRegistries`] when no `registry` is set and more than one has it.
/// [`Error::RegistryNotFound`] and [`Error::RegistryNameAmbiguous`] when `registry` names no
/// registry or several. Each error of [`resolve`](fn@super::resolve), and each error of a request.
pub(crate) fn pack_snapshot<A: PaletteApi>(
    palette: &Palette<A>,
    registry: Option<&str>,
    name: &str,
    version: &str,
    reference: &str,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot> {
    let mut registries = Registries::new(palette);
    pack_snapshot_in(
        &mut registries,
        registry,
        name,
        version,
        reference,
        fetched_at,
        progress,
    )
}

/// [`pack_snapshot`] with the registry listings of the command, so that a later check of the
/// same command reads them again from memory.
pub(crate) fn pack_snapshot_in<A: PaletteApi>(
    registries: &mut Registries<'_, A>,
    registry: Option<&str>,
    name: &str,
    version: &str,
    reference: &str,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot> {
    let palette = registries.palette();
    // the registry listing gives the names, and the reason of an error that finds no pack
    let visible = registries.names()?;
    let entries = versions_of(palette, name)?;
    let registry_uid = match registry {
        Some(r) => registries.by_ref(r)?.1,
        None => holding_registry(registries, &visible, &entries, name, version)?,
    };
    // a registry that the listing does not name has no name: each message gives its uid
    let registry_name = registries.name_of(&registry_uid)?.unwrap_or_default();
    let named = |uid: &str| name_or_uid(&registry_name, uid).to_string();

    let here: Vec<String> = entries
        .iter()
        .filter(|e| str_at(e, "/spec/registryUid") == registry_uid)
        .map(|e| text(e, "/spec/version"))
        .collect();
    let resolved = crate::reference::version::pick(here.iter(), |v| v.as_str(), version).cloned();
    let Some(resolved) = resolved else {
        let asked = visible.iter().filter(|(_, u)| *u == registry_uid);
        return Err(pack_not_found(
            name,
            version,
            named(&registry_uid),
            registries.unsynced_names(asked)?,
        ));
    };
    progress.note(&format!(
        "recording pack {} from registry {}",
        name_version(name, &resolved),
        named(&registry_uid)
    ));
    let loaded = one_layer_candidate(name, &resolved, &registry_uid, reference);
    let mut snap =
        resolve_with(&loaded, registries, progress)?.into_snapshot(reference, fetched_at)?;
    snap.source.kind = ProfileType::Pack.as_str().to_string();
    snap.source.template = Template::Registry.into();
    // a pack version is not a profile version, so no cluster uses it and nothing asked
    snap.source.consumers = None;
    snap.source.pack = Some(PackSource {
        name: name.to_string(),
        version: resolved,
        registry_uid,
        registry_name,
        pack_uid: snap
            .packs
            .first()
            .map(|p| p.pack_uid.clone())
            .unwrap_or_default(),
    });
    Ok(snap)
}

/// The error for a pack version that `registry` does not have. `unsynced` names the registries
/// that Palette does not sync, among the ones that strata asked.
fn pack_not_found(name: &str, version: &str, registry: String, unsynced: Vec<String>) -> Error {
    Error::PackNotFound {
        name: name.to_string(),
        version: version.to_string(),
        tag: None,
        registry,
        unsynced,
    }
}

/// The uid of the one visible registry that has the pack `name` at `version`, from the listing
/// `entries` of the pack. `version` can be [`LATEST`].
///
/// # Errors
///
/// [`Error::PackNotFound`] when no visible registry has it, and [`Error::PackInSeveralRegistries`]
/// when more than one has it.
fn holding_registry<A: PaletteApi>(
    registries: &mut Registries<'_, A>,
    visible: &[(String, String)],
    entries: &[Value],
    name: &str,
    version: &str,
) -> Result<String> {
    // the registries that have it, in the order Palette lists them
    let holders: Vec<&(String, String)> = visible
        .iter()
        .filter(|(_, uid)| {
            entries.iter().any(|e| {
                str_at(e, "/spec/registryUid") == uid
                    && (version == LATEST || str_at(e, "/spec/version") == version)
            })
        })
        .collect();
    match holders.as_slice() {
        [(_, uid)] => Ok(uid.clone()),
        [] => Err(pack_not_found(
            name,
            version,
            "any visible registry".to_string(),
            registries.unsynced_names(visible)?,
        )),
        several => {
            // by name, so the message reads the same on every run: Palette lists the
            // registries in the order it likes, and a live run saw both orders
            let mut registries: Vec<String> = several.iter().map(|(n, _)| n.clone()).collect();
            registries.sort();
            Err(Error::PackInSeveralRegistries {
                name: name.to_string(),
                version: version.to_string(),
                registries,
            })
        }
    }
}

/// A candidate with one layer, the pack `name` at `version` in the registry `registry_uid`, with
/// the values that the registry ships: the same path a candidate file takes. `reference` is the
/// path that the candidate reports.
fn one_layer_candidate(
    name: &str,
    version: &str,
    registry_uid: &str,
    reference: &str,
) -> LoadedCandidate {
    let candidate = Candidate {
        name: name.to_string(),
        version: version.to_string(),
        kind: ProfileType::Pack.as_str().to_string(),
        cloud_type: String::new(),
        registry: None,
        registry_uid: Some(registry_uid.to_string()),
        layers: vec![CandidateLayer {
            name: name.to_string(),
            version: version.to_string(),
            layer: None,
            kind: None,
            tag: None,
            registry: None,
            registry_uid: None,
            values: None,
            values_file: None,
            params: Map::new(),
            manifests: Vec::new(),
        }],
        variables: Vec::new(),
    };
    LoadedCandidate {
        path: PathBuf::from(reference),
        candidate,
        layers: vec![LoadedLayer {
            registry: Some(RegistryRef::Uid(registry_uid.to_string())),
            values: None,
            manifests: Vec::new(),
        }],
    }
}

/// The registry's summary of `name` at `version` in `registry`, paging through the listing.
pub(super) fn find_version<A: PaletteApi>(
    palette: &Palette<A>,
    name: &str,
    version: &str,
    registry: &str,
) -> Result<Option<Value>> {
    let mut found = None;
    each_pack_named(palette, name, |it| {
        let hit = str_at(&it, "/spec/name") == name
            && str_at(&it, "/spec/registryUid") == registry
            && str_at(&it, "/spec/version") == version;
        if hit {
            found = Some(it);
        }
        !hit
    })?;
    Ok(found)
}
