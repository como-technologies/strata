//! `packs`: the versions a registry offers for a pack.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::registry::{Registries, pack_type_of, registry_layer};
use crate::api::{Palette, PaletteApi};
use crate::closed_set::{ClosedSet, Open};
use crate::error::{Error, Result, found};
use crate::json::{arr, opt_text, strings, text};
use crate::model::cloud::supports;
use crate::model::pack_state::PackState;
use crate::model::pack_type::PackType;
use crate::reference::{LATEST, version};
use crate::snapshot::Tool;
use crate::term::progress::Progress;
use crate::timestamp::Rfc3339;

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
    #[schemars(with = "Rfc3339")]
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
    /// How many versions of the pack the registry offers, before `--latest` or `--cloud` cut the
    /// list. A heading states the registry's inventory, and not the count of rows a flag left.
    pub versions_offered: usize,
    /// True when Palette listed the pack under this registry, and then answered 404 for its tags,
    /// also with the states query. The disabled and the deleted versions then come from the
    /// listing alone, with no tags.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tags_unavailable: bool,
    /// The state that every version of the pack in this registry has, when they all have the
    /// same one: `deprecated`, `disabled`, or `deleted`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<Open<PackState>>,
    /// True when the registry reports no digest for each version of the pack in it, before
    /// `--latest` or `--cloud` cut the list. A record of such a version is `UNVERIFIED`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_digests: bool,
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
    /// The pack type: spectro, helm, or oci. A listing entry with no type is spectro.
    #[serde(rename = "type")]
    pub kind: PackType,
    /// The clouds this version supports.
    pub cloud_types: Vec<String>,
    /// Every tag that points at this version, the exact tag first.
    pub tags: Vec<String>,
    /// True for the version that `pack:<name>@latest` records, of the versions that the listing
    /// keeps. That is the highest version that is not a pre-release, or the highest pre-release
    /// when the registry's copy has no version that is not a pre-release. See [`version::pick`].
    /// The ref picks from each version of the registry. So it can record another version when
    /// `--cloud` cuts the list, and when the highest version has no tag, because the listing omits
    /// that version.
    pub latest: bool,
    /// True for the version that Palette calls the latest of the registry's copy. Palette marks the
    /// highest version by the order of semantic versions, also when it is a pre-release. So it can
    /// differ from `latest` for a pre-release, and for a version that does not parse as a
    /// semantic version.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub palette_latest: bool,
    /// The state that the registry listing gives this version: `deprecated`, `disabled`, or
    /// `deleted`. Palette serves no disabled or deleted pack, and refuses a profile that pins one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<Open<PackState>>,
    /// When Palette first listed this version, `metadata.creationTimestamp` of the listing; empty
    /// when the listing gives no time.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[schemars(with = "Rfc3339")]
    pub listed: String,
}

impl PackBuild {
    /// True when the state of this version is `disabled` or `deleted`: Spectro Cloud retired it,
    /// or marked it `disabled`, and a profile cannot hold it. The rule is
    /// [`PackState::is_retired`].
    pub(crate) fn retired(&self) -> bool {
        self.state
            .as_ref()
            .and_then(Open::known)
            .is_some_and(PackState::is_retired)
    }
}

/// What `packs` was asked for.
#[derive(Debug, Clone, Default)]
pub struct PackOptions {
    /// Only this registry, by name or uid.
    pub registry: Option<String>,
    /// Only the versions that support this cloud, by the rule of [`crate::model::cloud`]. A version
    /// with the cloud type `all`, or with no cloud type, supports each cloud.
    pub cloud: Option<String>,
    /// Only the version that `@latest` names in each registry, and the version that Palette calls
    /// the latest when it is a different one.
    pub latest: bool,
}

/// The annotation of the pack listing that marks the version that Palette calls the latest. At
/// the sync of a registry, Palette gives `"true"` to one version of each pack, by the order of
/// semantic versions, and `"false"` to each other version.
const PALETTE_LATEST: &str = "/metadata/annotations/latestVersion";

/// The processor architectures that the listing names for each version of one pack in one registry,
/// sorted, by version: `amd64`, `arm64`. A version with no architecture in the listing has an empty
/// list. `packs` is not a catalog, so the document does not show them: `behind` reads them, and says
/// what a move to a newer version drops.
pub(crate) type Architectures = BTreeMap<String, Vec<String>>;

/// The annotation of the pack listing that names the processor architectures of a version, as
/// one text with commas: `amd64`, `amd64,arm64`, or `arm64,amd64`. Not each version has it, and
/// the versions of one pack can differ.
const ARCHITECTURE: &str = "/spec/annotations/architecture";

/// The architectures that the listing entry `entry` names, sorted, with no repeat. Empty when the
/// entry names none.
fn architectures_of(entry: &Value) -> Vec<String> {
    let mut found: Vec<String> = text(entry, ARCHITECTURE)
        .split(',')
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(str::to_string)
        .collect();
    found.sort();
    found.dedup();
    found
}

/// The value of the `states` query that asks Palette for the tags of the retired versions of a
/// pack: each state that a listing gives. With it, Palette answers with the versions in these
/// states only.
fn listed_states() -> String {
    let listed = PackState::ALL.iter().filter(|s| s.listed());
    listed.map(|s| s.as_str()).collect::<Vec<_>>().join(",")
}

/// Join the tag rows of the `answers` of one registry, and keep each row one time.
///
/// A deprecated version is in the answer with no query and in the answer with the states query,
/// so its rows come two times.
fn merged_tags(answers: &[Value]) -> Vec<Value> {
    let mut seen = std::collections::BTreeSet::new();
    answers
        .iter()
        .flat_map(|a| arr(a, "/tags"))
        .filter(|t| seen.insert((text(t, "/packUid"), text(t, "/version"), text(t, "/tag"))))
        .cloned()
        .collect()
}

/// List what the registries offer for one pack.
///
/// Reads the paged listing for the versions, their digests, and their types. Then it reads the
/// answer of each registry for the tags that point at a version. Palette does not serve a version
/// that has no tag in that answer. So the listing of strata omits it, unless Spectro Cloud
/// retired it as disabled or deleted. A registry with no version left is not in the listing.
///
/// `name` is the name of the pack. `connect` gives a client for a project uid, with `None` for the
/// configured scope. `generated_at` is the time of the listing, in RFC 3339.
///
/// # Errors
///
/// [`Error::RegistryNotFound`] when `--registry` names no registry that the key can see.
/// [`Error::PackNameNotFound`] when no registry has the pack, and [`Error::PackNotFound`] when
/// the one registry of `--registry` has none. Each error of `connect`, and of a request.
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
    registry_packs_in(
        &mut Registries::new(&palette),
        name,
        options,
        generated_at,
        progress,
    )
    .map(|(listing, _)| listing)
}

/// [`registry_packs`] with the registry listings of the command, so that a later read of the same
/// command reads them again from memory.
///
/// It gives the architectures that the listing names for each version too: the part of the listing
/// that the document does not show. They are by registry uid, and then by version.
pub(super) fn registry_packs_in<A: PaletteApi>(
    listings: &mut Registries<'_, A>,
    name: &str,
    options: &PackOptions,
    generated_at: &str,
    progress: &Progress,
) -> Result<(PackListing, BTreeMap<String, Architectures>)> {
    let palette = listings.palette();
    // the registry listing gives the names, and the reason of an error that finds no pack
    let wanted = match &options.registry {
        Some(r) => vec![listings.by_ref(r)?],
        None => listings.names()?,
    };
    progress.note(&format!("reading the versions of {name}"));
    let entries = crate::candidate::pack::versions_of(palette, name)?;

    let mut registries = Vec::new();
    let mut architectures: BTreeMap<String, Architectures> = BTreeMap::new();
    let mut had_the_pack = false;
    for (registry, uid) in &wanted {
        let mine: Vec<&Value> = entries
            .iter()
            .filter(|e| text(e, "/spec/registryUid") == *uid)
            .collect();
        if mine.is_empty() {
            continue;
        }
        had_the_pack = true;
        let (copy, listed) =
            pack_registry(palette, name, options, (registry, uid), mine, progress)?;
        registries.extend(copy);
        if !listed.is_empty() {
            architectures.entry(uid.clone()).or_default().extend(listed);
        }
    }
    // no registry has the pack at all: strata cannot answer. A cloud that keeps nothing is an
    // answer, and an empty one
    if !had_the_pack {
        return Err(missing_pack(listings, name, options, &wanted));
    }
    let listing = PackListing {
        schema: PACKS_SCHEMA.to_string(),
        tool: Tool::current(),
        generated_at: generated_at.to_string(),
        name: name.to_string(),
        cloud: options.cloud.clone(),
        registries,
    };
    Ok((listing, architectures))
}

/// The error for a pack that no registry of `wanted` has.
///
/// Palette lists a pack of a registry that it does not sync only while a profile pins the pack.
/// So the error names each such registry: it says why the listing has nothing. When the read of
/// the registries fails, the error is that failure.
fn missing_pack<A: PaletteApi>(
    listings: &mut Registries<'_, A>,
    name: &str,
    options: &PackOptions,
    wanted: &[(String, String)],
) -> Error {
    let unsynced = match listings.unsynced_names(wanted) {
        Ok(unsynced) => unsynced,
        Err(error) => return error,
    };
    match (&options.registry, wanted) {
        // --registry named one registry, and wanted holds only that one
        (Some(_), [(registry, _)]) => Error::PackNotFound {
            name: name.to_string(),
            version: String::new(),
            tag: None,
            registry: registry.clone(),
            unsynced,
        },
        _ => Error::PackNameNotFound {
            name: name.to_string(),
            unsynced,
        },
    }
}

/// The copy of the pack in one registry, and the architectures of its versions. `mine` holds the
/// listing entries of the registry, and at least one. The copy is `None` when no version is left
/// after the flags of `options`.
fn pack_registry<A: PaletteApi>(
    palette: &Palette<A>,
    name: &str,
    options: &PackOptions,
    (registry, uid): (&str, &str),
    mine: Vec<&Value>,
    progress: &Progress,
) -> Result<(Option<PackRegistry>, Architectures)> {
    progress.note(&format!("reading the tags of {name} in {registry}"));
    let answers = tag_answers(palette, name, uid, &mine)?;
    let tags_unavailable = answers.is_empty();
    let layer = registry_layer(mine[0], &answers).unwrap_or_default();
    let tags = merged_tags(&answers);
    // Palette applies the FIPS rule of the tenant to its registry route, and not to the
    // listing. So a version with no tag in the answer is a version that Palette does not
    // serve. Omit it, unless Spectro Cloud retired it as disabled or deleted. A 404 is
    // the same answer: the route serves no version, for example when no version of the pack
    // is FIPS compliant
    let mine: Vec<&Value> = mine
        .into_iter()
        .filter(|e| has_tag(&tags, e) || crate::api::registry::retired_state(e).is_some())
        .collect();
    // every version of this pack that the registry offers, counted before --cloud and
    // --latest cut the list. The heading states the inventory, not the rows that survived a
    // flag
    let versions_offered = mine
        .iter()
        .filter_map(|e| opt_text(e, "/spec/version"))
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let no_digests = mine.iter().all(|e| text(e, "/spec/digest").is_empty());
    let state = common_state(&mine);
    let (versions, architectures) = pack_builds(&mine, &tags, options);
    let copy = (!versions.is_empty()).then(|| PackRegistry {
        registry: registry.to_string(),
        registry_uid: uid.to_string(),
        layer,
        versions,
        versions_offered,
        tags_unavailable,
        state,
        no_digests,
    });
    Ok((copy, architectures))
}

/// The answers of the tag route of one registry for the pack `name`, with the tag rows. The
/// answer is empty when Palette answers 404.
///
/// The registry's own answer holds the tags, which the listing does not. With no query, the
/// answer does not include the disabled and the deleted versions, and Palette answers 404 when it
/// has no version. So a registry with a retired version in `mine` gets a second request with the
/// states query. A registry that answers 404 to both keeps only the disabled and the deleted
/// versions of the listing, with no tags. The other registries still answer.
fn tag_answers<A: PaletteApi>(
    palette: &Palette<A>,
    name: &str,
    uid: &str,
    mine: &[&Value],
) -> Result<Vec<Value>> {
    let mut answers = vec![found(palette.pack_tags(name, uid, None))?];
    if mine
        .iter()
        .any(|e| crate::api::registry::retired_state(e).is_some())
    {
        answers.push(found(palette.pack_tags(name, uid, Some(&listed_states())))?);
    }
    Ok(answers.into_iter().flatten().collect())
}

/// The one state of the registry, when every version of the pack in `mine` has the same one.
fn common_state(mine: &[&Value]) -> Option<Open<PackState>> {
    let states: std::collections::BTreeSet<Option<Open<PackState>>> = mine
        .iter()
        .map(|e| crate::api::registry::pack_state(e))
        .collect();
    match states.into_iter().collect::<Vec<_>>().as_slice() {
        [Some(one)] => Some(one.clone()),
        _ => None,
    }
}

/// The versions of one registry's copy, newest first, and their architectures.
///
/// `--cloud` drops a version before strata reads its architectures. `--latest` cuts the list
/// after that, so `behind` still sees what a version drops.
fn pack_builds(
    mine: &[&Value],
    tags: &[Value],
    options: &PackOptions,
) -> (Vec<PackBuild>, Architectures) {
    let mut versions: Vec<PackBuild> = Vec::new();
    let mut architectures = Architectures::new();
    for &e in mine {
        let version = text(e, "/spec/version");
        if versions.iter().any(|v| v.version == version) {
            continue; // the paged listing repeats a version
        }
        let cloud_types = strings(e, "/spec/cloudTypes");
        if let Some(cloud) = &options.cloud
            && !supports(&cloud_types, cloud)
        {
            continue;
        }
        architectures.insert(version.clone(), architectures_of(e));
        versions.push(PackBuild {
            version,
            pack_uid: text(e, "/metadata/uid"),
            digest: text(e, "/spec/digest"),
            kind: pack_type_of(e),
            cloud_types,
            tags: tag_names(tags, e),
            latest: false,
            palette_latest: text(e, PALETTE_LATEST) == "true",
            state: crate::api::registry::pack_state(e),
            listed: text(e, "/metadata/creationTimestamp"),
        });
    }
    versions.sort_by(|a, b| version::compare(&b.version, &a.version));
    // the version that `@latest` records: `version::pick` is the one rule of both
    let latest =
        version::pick(versions.iter(), |v| v.version.as_str(), LATEST).map(|v| v.version.clone());
    for v in &mut versions {
        v.latest = latest.as_deref() == Some(v.version.as_str());
    }
    if options.latest {
        versions.retain(|v| v.latest || v.palette_latest);
    }
    (versions, architectures)
}

/// The tags that point at the listing entry `e`: the exact tag first, then the floating tags
/// above it.
///
/// A tag row names every tag above it, so the row with the most of them is the most exact. A
/// registry that names none leaves the tags of one version in name order.
fn tag_names(tags: &[Value], e: &Value) -> Vec<String> {
    let mut rows: Vec<(usize, String)> = tag_rows(tags, e)
        .into_iter()
        .map(|t| (arr(t, "/parentTags").len(), text(t, "/tag")))
        .collect();
    rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    rows.into_iter().map(|(_, tag)| tag).collect()
}

/// The tag rows of the registry answer that point at the listing entry `e`.
fn tag_rows<'a>(tags: &'a [Value], e: &Value) -> Vec<&'a Value> {
    tags.iter().filter(|t| points_at(t, e)).collect()
}

/// True when a tag row of the registry answer points at the listing entry `e`.
fn has_tag(tags: &[Value], e: &Value) -> bool {
    tags.iter().any(|t| points_at(t, e))
}

/// True when the tag row `t` points at the listing entry `e`. A tag row names its version and the
/// build that it points at. Match on either: a registry that answers with one of the two still
/// says enough.
fn points_at(t: &Value, e: &Value) -> bool {
    let v = text(t, "/version");
    if v.is_empty() {
        text(t, "/packUid") == text(e, "/metadata/uid")
    } else {
        v == text(e, "/spec/version")
    }
}

#[cfg(test)]
mod tests;
