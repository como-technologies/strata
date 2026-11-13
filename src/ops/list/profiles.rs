//! The profile versions of a listing: the scope of each one, and with `--packs`, its packs and
//! the registry of each pack.

use std::collections::BTreeMap;

use crate::api::registry::{Registries, registry_name};
use crate::api::{
    PROFILE_TEMPLATE, Palette, PaletteApi, profile_versions, recorded_packs, recorded_template,
};
use crate::error::{Result, served};
use crate::json::{opt_flag, opt_set, text};
use crate::model::scope::ProfileScope;
use crate::reference::{name_version, version};
use crate::term::progress::Progress;

use super::{ConsumerCounts, ListOptions, ListedPack, ListedProfile};

/// The profile versions that `palette` lists, sorted by name and then by version, with what
/// `options` asks for. The names of the registries of the packs come from `registries`.
pub(super) fn list_profiles<A: PaletteApi>(
    palette: &Palette<A>,
    options: &ListOptions,
    registries: &mut Registries<'_, A>,
    progress: &Progress,
) -> Result<Vec<ListedProfile>> {
    let mut profiles: Vec<ListedProfile> = profile_versions(&palette.profile_listing()?)
        .map(|v| ListedProfile {
            reference: name_version(&v.name, &v.version),
            uid: v.uid,
            cloud_type: v.cloud_type,
            name: v.name,
            version: v.version,
            scope: None,
            profile_type: None,
            consumers: None,
            consumers_by_kind: None,
            packs: None,
            published: None,
            error: None,
        })
        .collect();
    profiles.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| version::compare(&a.version, &b.version))
            .then_with(|| a.uid.cmp(&b.uid))
    });
    if !options.packs && options.pack.is_none() {
        scopes_from_search(&mut profiles, palette, progress)?;
        return Ok(profiles);
    }

    for p in &mut profiles {
        progress.note(&format!("reading profile {}", p.reference));
        let doc = match palette.profile_document(&p.uid) {
            Ok(doc) => doc,
            Err(e) => {
                p.error = Some(e.to_json());
                continue;
            }
        };
        p.scope = ProfileScope::of_document(&doc);
        // the template that a record of the version reads
        let template = doc.pointer(&format!("/spec/{}", recorded_template(&doc)));
        let packs = recorded_packs(&doc)
            .iter()
            .map(|pk| ListedPack {
                layer: text(pk, "/layer"),
                name: text(pk, "/name"),
                tag: text(pk, "/tag"),
                version: text(pk, "/version"),
                registry_uid: text(pk, "/registryUid"),
                // the pack ref gives no name, so name_registries reads it from the registry listing
                registry: String::new(),
            })
            .collect();
        p.profile_type = template.and_then(|t| opt_set(t, "/type", PROFILE_TEMPLATE));
        let consumers = ConsumerCounts::of(&doc);
        p.consumers = Some(consumers.count());
        p.consumers_by_kind = Some(consumers);
        p.packs = Some(packs);
        p.published = opt_flag(&doc, "/status/isPublished");
    }
    if let Some(needle) = &options.pack {
        let needle = needle.to_lowercase();
        // A profile that could not be read cannot be ruled out, so it stays with its error.
        profiles.retain(|p| match &p.packs {
            Some(packs) => packs
                .iter()
                .any(|pk| pk.name.to_lowercase().contains(&needle)),
            None => p.error.is_some(),
        });
    }
    name_registries(registries, &mut profiles)?;
    Ok(profiles)
}

/// Give each listed pack the name of its registry, from the registry listing.
///
/// Strata asks only when a pack names a registry. A Palette with no such listing gives no names.
/// Each other error of the listing returns, as the error of a listing does.
fn name_registries<A: PaletteApi>(
    listings: &mut Registries<'_, A>,
    profiles: &mut [ListedProfile],
) -> Result<()> {
    let mut packs: Vec<&mut ListedPack> = profiles
        .iter_mut()
        .flat_map(|p| p.packs.iter_mut().flatten())
        .filter(|pk| !pk.registry_uid.is_empty())
        .collect();
    if packs.is_empty() {
        return Ok(());
    }
    let Some(registries) = listings.listed()? else {
        return Ok(());
    };
    // a registry that the key cannot see has no name, and the row gives its uid
    for pk in &mut packs {
        pk.registry = registry_name(registries, &pk.registry_uid).unwrap_or_default();
    }
    Ok(())
}

/// Give each version its scope from the profile search.
///
/// The listing names each version with no scope. So strata asks the search for the versions of
/// each other scope that the listing holds. In a project, these are the tenant and the system. At
/// tenant scope, it is the system. Each other version is of the scope of the listing. An empty
/// listing needs no search.
///
/// A Palette with no profile search answers HTTP 404 or 405. The scopes then stay unknown,
/// because the listing is still the answer. Each other error returns.
fn scopes_from_search<A: PaletteApi>(
    profiles: &mut [ListedProfile],
    palette: &Palette<A>,
    progress: &Progress,
) -> Result<()> {
    if profiles.is_empty() {
        return Ok(());
    }
    let own = ProfileScope::of_listing(palette.project_uid().unwrap_or(""));
    let mut scopes: BTreeMap<String, ProfileScope> = BTreeMap::new();
    for &scope in own.others_listed() {
        progress.note(&format!("searching the {scope} profiles"));
        let Some(uids) = served(palette.profile_uids_in_scope(scope))? else {
            return Ok(());
        };
        scopes.extend(uids.into_iter().map(|uid| (uid, scope)));
    }
    for p in profiles {
        p.scope = Some(scopes.get(&p.uid).copied().unwrap_or(own));
    }
    Ok(())
}
