//! `packs`: the versions a registry offers for a pack.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::{Palette, PaletteApi};
use crate::error::{Error, Result};
use crate::progress::Progress;
use crate::snapshot::Tool;
use crate::version;

use super::text;

/// The schema id of the document `packs` writes.
pub const PACKS_SCHEMA: &str = "strata-packs/1";

/// The document `packs` writes: `strata-packs/1`. One entry per registry that has the pack, and
/// one row per version, newest first.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackListing {
    /// The schema id of this document
    #[schemars(extend("const" = PACKS_SCHEMA))]
    pub schema: String,
    /// Which tool wrote the document.
    pub tool: Tool,
    /// When, RFC 3339 UTC.
    pub generated_at: String,
    /// The pack that was asked for.
    pub name: String,
    /// With `--cloud`: the cloud the versions were kept for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud: Option<String>,
    /// The registries that have the pack, in the order Palette lists them.
    pub registries: Vec<PackRegistry>,
}

/// One registry's copy of the pack.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackRegistry {
    /// The registry name.
    pub registry: String,
    /// Its uid.
    pub registry_uid: String,
    /// The layer the pack belongs to, as the registry gives it.
    pub layer: String,
    /// The versions, newest first. `--latest` keeps one of them.
    pub versions: Vec<PackBuild>,
    /// How many versions of the pack the registry holds, before `--latest` or `--cloud` cut the
    /// list. A heading states the registry's inventory, and not the count of rows a flag left.
    pub versions_offered: usize,
    /// True when Palette listed the pack under this registry and then answered 404 for the
    /// registry's entity: the versions come from the listing alone, with no tags.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tags_unavailable: bool,
    /// The state that every version of the pack in this registry has, when they all have the
    /// same one: `deprecated`, `disabled`, or `deleted`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

/// One version of a pack in one registry.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackBuild {
    /// The version.
    pub version: String,
    /// The pack uid of this build.
    pub pack_uid: String,
    /// The digest the registry reports, empty when it reports none.
    pub digest: String,
    /// The pack type: spectro, helm, oci, or manifest.
    #[serde(rename = "type")]
    pub kind: String,
    /// The clouds this version supports.
    pub cloud_types: Vec<String>,
    /// Every tag that points at this version, the exact tag first.
    pub tags: Vec<String>,
    /// True for the newest version of the registry's copy.
    pub latest: bool,
    /// The state that the registry listing gives this version: `deprecated`, `disabled`, or
    /// `deleted`. Palette serves no disabled or deleted pack, and refuses a profile that pins one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// When Palette first listed this version, `metadata.creationTimestamp` of the listing; empty
    /// when the listing gives no time.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub listed: String,
}

/// What `packs` was asked for.
#[derive(Debug, Clone, Default)]
pub struct PackOptions {
    /// Only this registry, by name or uid.
    pub registry: Option<String>,
    /// Only the versions that support this cloud.
    pub cloud: Option<String>,
    /// Only the newest version of each registry.
    pub latest: bool,
}

/// List what the registries offer for one pack.
///
/// Reads the paged listing for the versions, their digests and their types, and each registry's
/// own answer for the tags that point at a version. A registry that does not have the pack is not
/// in the answer; no registry with it at all is `pack-not-found`.
pub fn registry_packs<A, F>(
    name: &str,
    options: &PackOptions,
    mut connect: F,
    generated_at: &str,
    progress: &Progress,
) -> Result<PackListing>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    let palette = connect(None)?;
    let visible = crate::candidate::registries(&palette)?;
    let wanted = match &options.registry {
        Some(r) => {
            let found = visible
                .iter()
                .find(|(n, u)| n == r || u == r)
                .ok_or_else(|| Error::RegistryNotFound {
                    registry: r.to_string(),
                    available: visible.iter().map(|(n, _)| n.clone()).collect(),
                })?;
            vec![found.clone()]
        }
        None => visible.clone(),
    };
    progress.note(&format!("reading the versions of {name}"));
    let entries = crate::candidate::versions_of(&palette, name)?;

    let mut registries = Vec::new();
    let mut had_the_pack = false;
    for (registry, uid) in wanted {
        let mut versions: Vec<PackBuild> = Vec::new();
        let mine: Vec<&Value> = entries
            .iter()
            .filter(|e| text(e, "/spec/registryUid") == uid)
            .collect();
        if mine.is_empty() {
            continue;
        }
        had_the_pack = true;
        // every version of this pack that the registry holds, counted before --cloud and --latest
        // cut the list: the heading states the inventory, not the rows that survived a flag
        let versions_offered = mine
            .iter()
            .filter_map(|e| Some(text(e, "/spec/version")).filter(|v| !v.is_empty()))
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        // one state for the registry, when every version of the pack in it has the same one
        let states: std::collections::BTreeSet<Option<String>> = mine
            .iter()
            .map(|e| crate::candidate::pack_state(e))
            .collect();
        let state = match states.into_iter().collect::<Vec<_>>().as_slice() {
            [Some(one)] => Some(one.clone()),
            _ => None,
        };
        // the registry's own answer holds the tags, which the listing does not. Palette serves no
        // disabled or deleted pack, so it can list a pack under a registry and then answer 404
        // for it; that registry keeps the versions of the listing, with no tags, and the other
        // registries still answer
        progress.note(&format!("reading the tags of {name} in {registry}"));
        let (entity, tags_unavailable) = match palette
            .api()
            .get_json(&format!("/v1/packs/{name}/registries/{uid}"))
        {
            Ok(entity) => (entity, false),
            Err(Error::Api { status: 404, .. }) => (Value::Null, true),
            Err(e) => return Err(e),
        };
        let layer = Some(text(&entity, "/layer"))
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| text(mine[0], "/spec/layer"));
        let tags = entity
            .get("tags")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        for e in mine {
            let version = text(e, "/spec/version");
            if versions.iter().any(|v: &PackBuild| v.version == version) {
                continue; // the paged listing repeats a version
            }
            let cloud_types: Vec<String> = e
                .pointer("/spec/cloudTypes")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|c| c.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            if let Some(cloud) = &options.cloud
                && !cloud_types.iter().any(|c| c == cloud)
            {
                continue;
            }
            // a tag row names its version, and names the build it points at; match on either,
            // because a registry that answers with one and not the other still says enough
            let pack_uid = text(e, "/metadata/uid");
            let mut rows: Vec<(usize, String)> = tags
                .iter()
                .filter(|t| {
                    let v = text(t, "/version");
                    if v.is_empty() {
                        text(t, "/packUid") == pack_uid
                    } else {
                        v == version
                    }
                })
                .map(|t| {
                    let above = t
                        .pointer("/parentTags")
                        .and_then(Value::as_array)
                        .map_or(0, Vec::len);
                    (above, text(t, "/tag"))
                })
                .collect();
            // the exact tag first, then the floating tags above it. A tag row names every tag
            // above it, so the row with the most of them is the most exact. A registry that
            // names none leaves the tags of one version in name order.
            rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
            let for_version: Vec<String> = rows.into_iter().map(|(_, tag)| tag).collect();
            versions.push(PackBuild {
                version,
                pack_uid,
                digest: text(e, "/spec/digest"),
                kind: text(e, "/spec/type"),
                cloud_types,
                tags: for_version,
                latest: false,
                state: crate::candidate::pack_state(e),
                listed: text(e, "/metadata/creationTimestamp"),
            });
        }
        versions.sort_by(|a, b| version::compare(&b.version, &a.version));
        if let Some(first) = versions.first_mut() {
            first.latest = true;
        }
        if options.latest {
            versions.truncate(1);
        }
        if !versions.is_empty() {
            registries.push(PackRegistry {
                registry,
                registry_uid: uid,
                layer,
                versions,
                versions_offered,
                tags_unavailable,
                state,
            });
        }
    }
    // no registry has the pack at all: strata cannot answer. A cloud that keeps nothing is an
    // answer, and an empty one.
    if !had_the_pack {
        if options.registry.is_none() {
            return Err(Error::PackNameNotFound {
                name: name.to_string(),
            });
        }
        return Err(Error::PackNotFound {
            name: name.to_string(),
            version: String::new(),
            tag: None,
            registry: options.registry.clone().unwrap_or_default(),
        });
    }
    Ok(PackListing {
        schema: PACKS_SCHEMA.to_string(),
        tool: Tool::current(),
        generated_at: generated_at.to_string(),
        name: name.to_string(),
        cloud: options.cloud.clone(),
        registries,
    })
}
