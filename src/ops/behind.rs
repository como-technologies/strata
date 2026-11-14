//! `behind`: the pack versions that a set of records pins, against the newer versions that the
//! registries offer.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::registry::{Registries, RegistrySyncNote};
use crate::api::{Palette, PaletteApi};
use crate::closed_set::Open;
use crate::diff::RegistrySync;
use crate::error::{Error, ErrorDocument, Result};
use crate::json::is_zero;
use crate::model::pack_state::PackState;
use crate::snapshot::{Pack, Snapshot, Tool, load_document};
use crate::term::progress::Progress;
use crate::timestamp::Rfc3339;

use self::bump::{BumpQuestions, read_bump};
use self::carriers::read_carriers;
pub use self::carriers::{Carrier, PinCarrier};
pub use self::change::{Bump, BumpPack};
use self::documents::ProfileDocuments;
use self::newer::{newer_versions, next_patch};
use self::profiles::{Held, ProfileKey, check_profile, collect};
pub use self::profiles::{NewerProfile, ProfilePin};
use super::packs::{Architectures, PackBuild, PackOptions, registry_packs_in};
use super::scope_of;

mod bump;
mod carriers;
mod change;
mod documents;
mod newer;
mod profiles;

/// The schema id of the document `behind` writes.
pub const BEHIND_SCHEMA: &str = "strata-behind/1";

/// The document `behind` writes: `strata-behind/1`. One entry for each pack version that the
/// records pin from a registry, and one for each profile version that they hold.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BehindReport {
    /// The schema id of this document
    #[schemars(extend("const" = BEHIND_SCHEMA))]
    pub schema: String,
    /// Which tool wrote the document.
    pub tool: Tool,
    /// When, RFC 3339 UTC.
    #[schemars(with = "Rfc3339")]
    pub generated_at: String,
    /// The records read, in the order given.
    pub records: Vec<String>,
    /// With `--since`: when the previous report was written. `arrived` on each pin is against it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<Rfc3339>")]
    pub since: Option<String>,
    /// One entry for each pack, registry, pinned version, and tag, in the order of the pack name.
    pub pins: Vec<Pin>,
    /// One entry for each profile version that the records hold, in the order of the profile name
    /// and then of the version. Absent when the records hold none, and with `--pin`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<ProfilePin>,
    /// The counts.
    pub summary: BehindSummary,
}

impl BehindReport {
    /// The exit status: 2 when a pin or a profile version could not be checked, 1 when one is
    /// behind, else 0.
    pub fn exit_code(&self) -> u8 {
        let s = &self.summary;
        if s.errors > 0 || s.profile_errors > 0 {
            2
        } else {
            u8::from(s.behind > 0 || s.profiles_behind > 0)
        }
    }

    /// Read the report of a previous run, the file that `--since` names.
    ///
    /// # Errors
    ///
    /// - [`Error::Usage`] when the file is not a `strata-behind/1` document. It is not JSON, it
    ///   has no schema id, or its id names another document, such as a record.
    /// - [`Error::UnsupportedSchema`] when its id names another version of the report.
    /// - [`Error::SnapshotSchema`] when its fields do not match `strata-behind/1`.
    /// - [`Error::Io`] when strata cannot read the file.
    pub fn load(path: &Path) -> Result<BehindReport> {
        load_document(path, BEHIND_SCHEMA, || Error::Usage {
            message: format!(
                "--since {} is not a {BEHIND_SCHEMA} document: give the JSON of a previous run",
                path.display()
            ),
        })
    }
}

/// The counts of a report.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BehindSummary {
    /// The pins.
    pub pins: usize,
    /// The pins with a newer version to move to.
    pub behind: usize,
    /// The pins with a floating tag. Each one is behind as an exact pin is: also when the version
    /// that its tag names is above the pinned one.
    pub floating: usize,
    /// The pins that strata could not check.
    pub errors: usize,
    /// With `--since`: the versions that arrived since the previous report, across the pins.
    pub arrived: usize,
    /// The profile versions that the records hold. Absent when they hold none.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub profiles: usize,
    /// The profile versions with a newer published version of their profile. Absent for none.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub profiles_behind: usize,
    /// The profile versions that strata could not check. Absent for none.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub profile_errors: usize,
}

/// One pack version that the records pin from one registry.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Pin {
    /// The pack name.
    pub name: String,
    /// The layer of the pack in the records.
    pub layer: String,
    /// The registry name, as the registry listing gives it; empty when strata could not read it.
    pub registry: String,
    /// The registry uid.
    pub registry_uid: String,
    /// The pinned version.
    pub version: String,
    /// The pinned tag.
    pub tag: String,
    /// True when the tag is a floating tag, such as `1.x`. Palette moves a profile that keeps the
    /// values of the pack to the version that the tag names. A profile with values of its own
    /// stays on the pinned version until a person applies the update.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub floating: bool,
    /// For a floating tag: the version that the tag names now, from the tags of the registry.
    /// When it is above the pinned version, the pin is behind it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<String>,
    /// The state that the registry gives the pinned version: `deprecated`, `disabled`, or
    /// `deleted`; `absent` when the registry no longer lists it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_state: Option<Open<PackState>>,
    /// Each record that pins it.
    pub pinned_by: Vec<PinnedBy>,
    /// The newer versions that the registry offers, newest first. A version that the registry
    /// has disabled or deleted is not one to move to. A version for none of the clouds of the
    /// pinned version is not one to move to either. A pre-release is not one to move to from a
    /// version that is not a pre-release, unless [`BehindOptions::pre_release`] asks for it. A
    /// version that supports fewer architectures than the pinned version is one, and it names
    /// what it drops.
    pub newer: Vec<Newer>,
    /// True when `newer` is not empty.
    pub behind: bool,
    /// For a pin that is behind a newer minor version: the newest version in `newer` of the minor
    /// version of the pin. A pin of `1.36.2` that is behind `1.37.0` and `1.36.4` gives `1.36.4`. A
    /// team can take that patch before the move to the newest version. Absent when the pin is
    /// current, when its version does not parse, and when the newest version is of its minor
    /// version. Also absent when `newer` has no version of its minor version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_patch: Option<String>,
    /// For a pin that is behind: what a move to the newest version does, from `compare` of the
    /// two pack versions, and whether Palette accepts it. Absent with `--no-impact`, or when strata
    /// could not compare them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bump: Option<Bump>,
    /// For a pin that is behind: each profile version that a record of the pin holds, that has a
    /// newer version, and whose template holds the pack. Each one names the newest of its newer
    /// versions that pins a newer version of the pack. Absent when there is none, and with `--pin`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub carriers: Vec<PinCarrier>,
    /// With `--since`: the newer versions that the previous report did not know. Absent when the
    /// previous report had no pin of this pack in this registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrived: Option<Vec<String>>,
    /// Why strata could not check the pin, when it could not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<ErrorDocument>")]
    pub error: Option<Value>,
    /// For a pin that is current: the last sync of its registry, when that sync did not
    /// complete. The copy of the registry in Palette is then the copy of an earlier sync, and
    /// `current` is against that copy. Absent for a pin that is behind, when the last sync
    /// completed, when the registry does not sync, and when Palette does not answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_sync: Option<RegistrySync>,
    /// For a pin that is current: why strata could not read the last sync of its registry.
    /// Palette answered with a 5xx, or the request did not complete. The pin stays current.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_sync_error: Option<String>,
}

/// A record that pins a pack version, or that holds a profile version.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PinnedBy {
    /// The path of the record.
    pub path: String,
    /// What the record holds, as its title: `aws-stack@1.1.1`, `aws-stack@1.1.1 on cluster
    /// payments`, or `4 profiles on cluster payments`.
    pub subject: String,
    /// For a record of more than one profile, such as a cluster: the title with only the profile
    /// that holds the pack, or that is the profile version, as
    /// `monitoring@1.4.0 on cluster payments`. Absent for a record of one profile, and when the
    /// record does not name the profile of the pack.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

impl PinnedBy {
    /// Add the record at `path`, whose title is `subject`, to the records `by`. `owner` is the
    /// title of the record with only the profile that holds the entry. A record that `by` names
    /// already is not added again.
    fn add(by: &mut Vec<PinnedBy>, path: &Path, subject: &str, owner: Option<String>) {
        let path = path.display().to_string();
        if !by.iter().any(|b| b.path == path) {
            by.push(PinnedBy {
                path,
                subject: subject.to_string(),
                owner,
            });
        }
    }
}

/// One newer version of a pack.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Newer {
    /// The version.
    pub version: String,
    /// `deprecated`, when the registry says so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<Open<PackState>>,
    /// True for the version that Palette calls the latest: see [`PackBuild::palette_latest`].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub palette_latest: bool,
    /// When Palette first listed the version; empty when the listing gives no time.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[schemars(with = "Rfc3339")]
    pub listed: String,
    /// The architectures of the pinned version that this version does not support: a move to it
    /// drops them. Empty when the listing names the architectures of only one of the two.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drops: Vec<String>,
}

/// What `behind` reads.
#[derive(Debug, Clone, Default)]
pub struct BehindOptions {
    /// Compare each pin that is behind with the newest version, for its impact: two pack reads
    /// for each such pin. Also ask Palette whether it accepts the move, with one request for each
    /// record that pins it. A record of a cluster also costs a read of the cluster document.
    /// Compare each profile version that is behind with the newest version of its profile too.
    /// That reads the pack manifests and the variables of the two versions.
    pub impact: bool,
    /// The report of the previous run: each pin names the versions that arrived since.
    pub since: Option<BehindReport>,
    /// Report only the pins of the pack with this key, `<layer>/<name>`. Then strata reads no other
    /// pack, and compares and asks Palette about the move of these pins only. Strata reads the
    /// records first, and refuses a key that none of them pins from a registry.
    pub pin: Option<String>,
    /// Also name a pre-release as a newer version of a pin that is not a pre-release. Without it,
    /// a pre-release is a newer version only for a pin that is one itself.
    pub pre_release: bool,
}

/// What a registry offers for one pack.
struct Offered {
    /// The name of the registry.
    registry: String,
    /// The versions of the pack in the registry.
    builds: Vec<PackBuild>,
    /// The architectures that the listing names for those versions.
    architectures: Architectures,
}

/// A floating tag names no one version: `1.x`, `1.30.x`, `LTS__24.x`.
pub fn is_floating(tag: &str) -> bool {
    tag.split(['.', '_', '-']).any(|part| part == "x")
}

impl Pin {
    /// The pin that the pack `p` of a record makes, with no read of its registry yet.
    fn of(p: &Pack) -> Pin {
        Pin {
            name: p.name.clone(),
            layer: p.layer.clone(),
            registry: String::new(),
            registry_uid: p.registry_uid.clone(),
            version: p.version.clone(),
            tag: p.tag.clone(),
            floating: is_floating(&p.tag),
            resolved: None,
            pinned_state: None,
            pinned_by: Vec::new(),
            newer: Vec::new(),
            behind: false,
            next_patch: None,
            bump: None,
            carriers: Vec::new(),
            arrived: None,
            error: None,
            registry_sync: None,
            registry_sync_error: None,
        }
    }
}

/// One pin in the records: a pack name, a registry, a pinned version, and a tag. The order of the
/// fields is the order of the pins in the report.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PinKey {
    name: String,
    registry_uid: String,
    version: String,
    tag: String,
}

impl PinKey {
    /// The pin that the pack `p` of a record makes.
    fn of(p: &Pack) -> PinKey {
        PinKey {
            name: p.name.clone(),
            registry_uid: p.registry_uid.clone(),
            version: p.version.clone(),
            tag: p.tag.clone(),
        }
    }
}

/// What `behind` has read for all the scopes: each pack of each registry, and the last sync of
/// each registry, each read one time.
#[derive(Default)]
struct Reads {
    /// The pack name and the registry uid, to what the registry offers.
    offers: BTreeMap<(String, String), Result<Offered>>,
    /// The registry uid, to its last sync.
    syncs: BTreeMap<String, RegistrySyncNote>,
}

/// A pin of the records, with the project of its first record and the records that pin it.
struct Pinned {
    pin: Pin,
    /// The project of the first record that pins it.
    project: String,
    /// The records that pin it, as indexes into the records read: the question to Palette about a
    /// bump reads them.
    holders: Vec<usize>,
}

/// The records that `behind` reads, and the pins and the profile versions in them.
struct Records {
    /// The records, in the order given.
    loaded: Vec<Snapshot>,
    /// The pins, by key.
    pins: BTreeMap<PinKey, Pinned>,
    /// The profile versions, by key. Empty with `--pin`.
    profiles: BTreeMap<ProfileKey, Held>,
}

/// What `behind` checks in one scope: the pins and the profile versions of the scope.
#[derive(Default)]
struct ScopeWork {
    pins: Vec<(PinKey, Pinned)>,
    profiles: Vec<(ProfileKey, Held)>,
}

/// What the check of each pin shares.
struct Check<'a> {
    /// The records that the pins come from.
    loaded: &'a [Snapshot],
    /// Compare each pin that is behind with the newest version: see [`BehindOptions::impact`].
    impact: bool,
    /// Name a pre-release as a newer version of a pin that is not one: see
    /// [`BehindOptions::pre_release`].
    pre_release: bool,
    generated_at: &'a str,
    progress: &'a Progress,
}

/// Report the pack versions that `records` pin, and the newer versions that the registries offer.
///
/// Also report the profile versions that `records` hold, and the newer published versions of each
/// profile.
///
/// `records` are the paths of the record files. `generated_at` is the time of the report, in
/// RFC 3339.
///
/// Strata reads each pin and each profile version in the project of the first record that holds
/// it. It calls `connect` one time for each project, and keeps one cache of the registry listings
/// for each project. It reads each pack of a registry one time. A pin or a profile version in a
/// project that `connect` cannot reach has the error of `connect`. When strata cannot read a
/// record, the whole run fails: strata cannot say what the record pins. With `options.pin`,
/// strata reads no profile version.
///
/// With `options.impact`, strata asks Palette about the bump of a pin in the project of the pin.
/// It does not ask a record of another project that pins the pack, and the bump says so. It also
/// compares each profile version that is behind with the newest version of its profile. Strata
/// reads the document of each profile version one time in each project, also for the names of the
/// clusters in a refusal of Palette.
///
/// # Errors
///
/// The error of [`Snapshot::load`] for a record that strata cannot read, such as [`Error::Io`] or
/// [`Error::NotSnapshot`]. A request that fails for one pin gives the error of that pin in the
/// report, and not an error of the run.
pub fn behind<A, F>(
    records: &[PathBuf],
    options: &BehindOptions,
    connect: F,
    generated_at: &str,
    progress: &Progress,
) -> Result<BehindReport>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    let read = read_records(records, options.pin.as_deref())?;
    let check = Check {
        loaded: &read.loaded,
        impact: options.impact,
        pre_release: options.pre_release,
        generated_at,
        progress,
    };
    let (mut out, profiles) = check_scopes(&check, read.pins, read.profiles, connect)?;
    let arrived = options
        .since
        .as_ref()
        .map_or(0, |previous| mark_arrived(&mut out, previous));
    let summary = BehindSummary {
        pins: out.len(),
        behind: out.iter().filter(|p| p.behind).count(),
        floating: out.iter().filter(|p| p.floating).count(),
        errors: out.iter().filter(|p| p.error.is_some()).count(),
        arrived,
        profiles: profiles.len(),
        profiles_behind: profiles.iter().filter(|p| p.behind).count(),
        profile_errors: profiles.iter().filter(|p| p.error.is_some()).count(),
    };
    Ok(BehindReport {
        schema: BEHIND_SCHEMA.to_string(),
        tool: Tool::current(),
        generated_at: generated_at.to_string(),
        records: records.iter().map(|p| p.display().to_string()).collect(),
        since: options.since.as_ref().map(|p| p.generated_at.clone()),
        pins: out,
        profiles,
        summary,
    })
}

/// Read each record of `records`, and collect the pins and the profile versions in them. A pin
/// is in the project of the first record that pins it. With `only`, the key of a pack, the
/// records give the pins of that pack alone, and no profile version.
///
/// # Errors
///
/// [`Error::Usage`] when `only` is a key that no record pins from a registry. The error comes
/// before any request.
fn read_records(records: &[PathBuf], only: Option<&str>) -> Result<Records> {
    let mut loaded: Vec<Snapshot> = Vec::with_capacity(records.len());
    let mut pins: BTreeMap<PinKey, Pinned> = BTreeMap::new();
    let mut profiles: BTreeMap<ProfileKey, Held> = BTreeMap::new();
    for path in records {
        let snap = Snapshot::load(path)?;
        if only.is_none() {
            collect(&mut profiles, &snap, path);
        }
        let subject = snap.source.title();
        let from_registry = snap.packs.iter().filter(|p| !p.registry_uid.is_empty());
        for p in from_registry.filter(|p| only.is_none_or(|key| p.key == key)) {
            let pinned = pins.entry(PinKey::of(p)).or_insert_with(|| Pinned {
                pin: Pin::of(p),
                project: snap.source.project_uid.clone(),
                holders: Vec::new(),
            });
            if pinned.holders.last() != Some(&loaded.len()) {
                pinned.holders.push(loaded.len());
            }
            let owner = snap.source.title_of_pack(&p.name);
            PinnedBy::add(&mut pinned.pin.pinned_by, path, &subject, owner);
        }
        loaded.push(snap);
    }
    if let Some(key) = only
        && pins.is_empty()
    {
        return Err(Error::Usage {
            message: format!("--pin {key}: no record pins this pack from a registry"),
        });
    }
    Ok(Records {
        loaded,
        pins,
        profiles,
    })
}

/// Check each pin of `pins` and each profile version of `profiles` in its scope, and then give
/// each pin that is behind its carriers. Give the pins in the order of their keys, and the profile
/// versions in the order of theirs.
///
/// A cache of the registry listings borrows its client. So strata groups the pins and the profile
/// versions by scope: each scope connects one time, and keeps its cache for all its pins. Each
/// scope keeps its connection and its documents to the end, because the carriers of a pin read the
/// profile versions of each scope: see [`read_carriers`].
fn check_scopes<A, F>(
    check: &Check<'_>,
    pins: BTreeMap<PinKey, Pinned>,
    profiles: BTreeMap<ProfileKey, Held>,
    mut connect: F,
) -> Result<(Vec<Pin>, Vec<ProfilePin>)>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    let (works, connections): (Vec<_>, Vec<_>) = by_scope(pins, profiles)
        .into_iter()
        .map(|(scope, work)| {
            let connection = connect(scope.as_deref());
            ((scope, work), connection)
        })
        .unzip();
    let mut documents: BTreeMap<Option<String>, ProfileDocuments<'_, A>> = BTreeMap::new();
    let mut reads = Reads::default();
    let mut checked: BTreeMap<PinKey, Pinned> = BTreeMap::new();
    let mut held: BTreeMap<ProfileKey, Held> = BTreeMap::new();
    for ((scope, work), connection) in works.into_iter().zip(&connections) {
        let palette = match connection {
            Ok(palette) => palette,
            Err(e) => {
                fail_scope(work, &e.to_json(), &mut checked, &mut held);
                continue;
            }
        };
        let docs = documents
            .entry(scope.clone())
            .or_insert_with(|| ProfileDocuments::new(palette));
        for (key, mut h) in work.profiles {
            check_profile(docs, &mut h.profile, check);
            held.insert(key, h);
        }
        check_scope(
            check,
            docs,
            scope.as_deref(),
            work.pins,
            &mut reads,
            &mut checked,
        )?;
    }
    read_carriers(check, &mut checked, &held, &mut documents);
    Ok((
        checked.into_values().map(|p| p.pin).collect(),
        held.into_values().map(|h| h.profile).collect(),
    ))
}

/// The pins of `pins` and the profile versions of `profiles`, by the scope of their first record.
fn by_scope(
    pins: BTreeMap<PinKey, Pinned>,
    profiles: BTreeMap<ProfileKey, Held>,
) -> BTreeMap<Option<String>, ScopeWork> {
    let mut scopes: BTreeMap<Option<String>, ScopeWork> = BTreeMap::new();
    for (key, pinned) in pins {
        let work = scopes.entry(scope_of(&pinned.project).map(str::to_string));
        work.or_default().pins.push((key, pinned));
    }
    for (key, held) in profiles {
        let work = scopes.entry(scope_of(&held.project).map(str::to_string));
        work.or_default().profiles.push((key, held));
    }
    scopes
}

/// Give each pin and each profile version of `work` the error of the connection to their scope,
/// `error`.
fn fail_scope(
    work: ScopeWork,
    error: &Value,
    checked: &mut BTreeMap<PinKey, Pinned>,
    held: &mut BTreeMap<ProfileKey, Held>,
) {
    for (key, mut pinned) in work.pins {
        pinned.pin.error = Some(error.clone());
        checked.insert(key, pinned);
    }
    for (key, mut h) in work.profiles {
        h.profile.error = Some(error.clone());
        held.insert(key, h);
    }
}

/// Check the pins of `group`, the pins of one scope. `documents` holds the connection to that
/// scope, and the documents of the profile versions that the scope read.
fn check_scope<A: PaletteApi>(
    check: &Check<'_>,
    documents: &mut ProfileDocuments<'_, A>,
    scope: Option<&str>,
    group: Vec<(PinKey, Pinned)>,
    reads: &mut Reads,
    checked: &mut BTreeMap<PinKey, Pinned>,
) -> Result<()> {
    let Check {
        loaded,
        impact,
        pre_release,
        generated_at,
        progress,
    } = *check;
    let mut listings = Registries::new(documents.palette);
    let mut questions = BumpQuestions::new(documents, scope);
    for (key, mut pinned) in group {
        let pin = &mut pinned.pin;
        read_pin(
            pin,
            &mut listings,
            reads,
            pre_release,
            generated_at,
            progress,
        )?;
        if impact && pin.behind {
            let holding: Vec<&Snapshot> = pinned
                .holders
                .iter()
                .filter_map(|&i| loaded.get(i))
                .collect();
            read_bump(
                pin,
                &holding,
                &mut listings,
                &mut questions,
                generated_at,
                progress,
            );
        }
        checked.insert(key, pinned);
    }
    Ok(())
}

/// Give each pin of `pins` the versions that arrived since the report `previous`, and count them.
/// A pin of a pack that `previous` did not know gets none.
fn mark_arrived(pins: &mut [Pin], previous: &BehindReport) -> usize {
    let mut arrived = 0;
    for pin in pins {
        let before: Vec<&Pin> = previous
            .pins
            .iter()
            .filter(|p| p.name == pin.name && p.registry_uid == pin.registry_uid)
            .collect();
        if before.is_empty() {
            continue;
        }
        let known: BTreeSet<&str> = before
            .iter()
            .flat_map(|p| {
                std::iter::once(p.version.as_str())
                    .chain(p.newer.iter().map(|n| n.version.as_str()))
            })
            .collect();
        let new: Vec<String> = pin
            .newer
            .iter()
            .filter(|n| !known.contains(n.version.as_str()))
            .map(|n| n.version.clone())
            .collect();
        arrived += new.len();
        pin.arrived = Some(new);
    }
    arrived
}

/// Give `pin` the registry name, the state of the pinned version, and the newer versions, from
/// what its registry offers. For a pin that is current, also give it the last sync of the registry.
///
/// `listings` is the cache of the scope of the pin, and `reads` what the other pins read. A pack
/// that the registry cannot give is the error of `pin`.
fn read_pin<A: PaletteApi>(
    pin: &mut Pin,
    listings: &mut Registries<'_, A>,
    reads: &mut Reads,
    pre_release: bool,
    generated_at: &str,
    progress: &Progress,
) -> Result<()> {
    let offered = match offered_for(&mut reads.offers, pin, listings, generated_at, progress) {
        Ok(found) => found,
        Err(e) => {
            pin.error = Some(e.to_json());
            return Ok(());
        }
    };
    pin.registry.clone_from(&offered.registry);
    let builds = &offered.builds;
    let pinned = builds.iter().find(|b| b.version == pin.version);
    pin.pinned_state = match pinned {
        Some(b) => b.state.clone(),
        None => Some(PackState::Absent.into()),
    };
    // the version that a floating tag names now. A profile with values of its own stays on
    // the pinned version until a person applies the update. So each version above the pin
    // is newer, also the one that the tag names
    if pin.floating {
        pin.resolved = builds
            .iter()
            .find(|b| b.tags.iter().any(|t| t == &pin.tag))
            .map(|b| b.version.clone());
    }
    pin.newer = newer_versions(pin, offered, pinned, pre_release);
    pin.behind = !pin.newer.is_empty();
    pin.next_patch = next_patch(&pin.version, &pin.newer);
    // a pin that is current is current against the copy of the registry in Palette. When
    // the last sync did not complete, the copy is the copy of an earlier sync, and the pin
    // says so. A pin that is behind needs no note: a newer version in the copy is a newer
    // version in the registry. The first build gives the kind of the registry. A registry
    // with no build of the pack is not in the listing, so it gets no note
    if !pin.behind
        && let Some(first) = builds.first()
    {
        let note = match reads.syncs.entry(pin.registry_uid.clone()) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(v) => {
                v.insert(listings.sync_note(&pin.registry_uid, Some(first.kind), progress)?)
            }
        };
        pin.registry_sync = note.sync.as_ref().filter(|s| !s.completed()).cloned();
        pin.registry_sync_error = note.error.clone();
    }
    Ok(())
}

/// What the registry of `pin` offers for its pack, from `offers`, or from the registry when no
/// pin read the pack before.
fn offered_for<'r, A: PaletteApi>(
    offers: &'r mut BTreeMap<(String, String), Result<Offered>>,
    pin: &Pin,
    listings: &mut Registries<'_, A>,
    generated_at: &str,
    progress: &Progress,
) -> &'r Result<Offered> {
    offers
        .entry((pin.name.clone(), pin.registry_uid.clone()))
        .or_insert_with(|| {
            registry_packs_in(
                listings,
                &pin.name,
                &PackOptions {
                    registry: Some(pin.registry_uid.clone()),
                    ..PackOptions::default()
                },
                generated_at,
                progress,
            )
            .map(|(listing, mut architectures)| {
                let (registry, builds) = listing
                    .registries
                    .into_iter()
                    .next()
                    .map(|r| (r.registry, r.versions))
                    .unwrap_or_default();
                Offered {
                    registry,
                    builds,
                    architectures: architectures.remove(&pin.registry_uid).unwrap_or_default(),
                }
            })
        })
}
