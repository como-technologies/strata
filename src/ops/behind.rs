//! `behind`: the pack versions that a set of records pins, against the newer versions that the
//! registries offer.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::{Palette, PaletteApi};
use crate::closed_set::Open;
use crate::cloud::supports;
use crate::diff::{ChangeFlag, Diff, ImpactLevel, RegistrySync};
use crate::error::{Error, ErrorDocument, Result};
use crate::pack_state::PackState;
use crate::reference::version;
use crate::registry::{Registries, RegistrySyncNote};
use crate::snapshot::{Pack, Snapshot, Tool, load_document};
use crate::term::progress::Progress;
use crate::timestamp::Rfc3339;
use crate::validation::ProfileValidation;

use self::bump::{BumpQuestions, read_bump};
use super::packs::{Architectures, PackBuild, PackOptions, registry_packs_in};
use super::scope_of;

mod bump;

/// The schema id of the document `behind` writes.
pub const BEHIND_SCHEMA: &str = "strata-behind/1";

/// The document `behind` writes: `strata-behind/1`. One entry for each pack version that the
/// records pin from a registry.
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
    /// The counts.
    pub summary: BehindSummary,
}

impl BehindReport {
    /// The exit status: 2 when a pin could not be checked, 1 when a pin is behind, else 0.
    pub fn exit_code(&self) -> u8 {
        if self.summary.errors > 0 {
            2
        } else {
            u8::from(self.summary.behind > 0)
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
    /// has disabled or deleted is not one to move to, and a version for none of the clouds of the
    /// pinned version is not either. A pre-release is not one to move to from a version that is
    /// not a pre-release, unless [`BehindOptions::pre_release`] asks for it. A version that
    /// supports fewer architectures than the pinned version is one, and it names what it drops.
    pub newer: Vec<Newer>,
    /// True when `newer` is not empty.
    pub behind: bool,
    /// For a pin that is behind: what a move to the newest version does, from `compare` of the
    /// two pack versions, and whether Palette accepts it. Absent with `--no-impact`, or when strata
    /// could not compare them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bump: Option<Bump>,
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

/// A record that pins a pack version.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PinnedBy {
    /// The path of the record.
    pub path: String,
    /// What the record holds, as its title: `aws-stack@1.1.1`, or `aws-stack@1.1.1 on cluster
    /// payments`.
    pub subject: String,
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

/// What a move from the pinned version to the newest one does.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Bump {
    /// The version compared: the newest one.
    pub to: String,
    /// The impact level of the change.
    pub impact: Open<ImpactLevel>,
    /// The change flags of the pack, in the order of the legend.
    pub flags: Vec<Open<ChangeFlag>>,
    /// Whether Palette accepts the move, with the errors that it gives and the clusters that they
    /// name. Palette checks the move against the packs of the clusters that run the profile of
    /// each record. Strata asks the records that pin the pack in the project of the pin. Absent
    /// when strata can ask no record of the pin. Also absent when a request failed or strata did
    /// not ask a record, and no answer holds an error. Strata says that Palette accepts only when
    /// it asked each record and each request answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette_validation: Option<ProfileValidation>,
    /// Why a request to Palette failed, when one did. Also the records of another project that
    /// strata did not ask, such as `1 record of project p2 was not asked`. The pin stays behind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette_validation_error: Option<String>,
}

impl Bump {
    /// What a move to the version `to` does to the pack `name`, from the comparison `d`. Palette
    /// is not asked: the fields about its answer are empty.
    ///
    /// `d` compares the pinned version with `to`. None when `d` holds no pack of that name.
    pub fn of(to: &str, d: &Diff, name: &str) -> Option<Bump> {
        let pd = d.packs.iter().find(|p| p.name == name)?;
        Some(Bump {
            to: to.to_string(),
            impact: d.summary.max_impact.into(),
            flags: pd.flags().into_iter().map(Open::from).collect(),
            palette_validation: None,
            palette_validation_error: None,
        })
    }
}

/// What `behind` reads.
#[derive(Debug, Clone, Default)]
pub struct BehindOptions {
    /// Compare each pin that is behind with the newest version, for its impact: two pack reads
    /// for each such pin. Also ask Palette whether it accepts the move, with one request for each
    /// record that pins it. A record of a cluster also costs a read of the cluster document.
    pub impact: bool,
    /// The report of the previous run: each pin names the versions that arrived since.
    pub since: Option<BehindReport>,
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
            bump: None,
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

/// The records that `behind` reads, and the pins in them.
struct Records {
    /// The records, in the order given.
    loaded: Vec<Snapshot>,
    /// The pins, by key.
    pins: BTreeMap<PinKey, Pinned>,
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
/// Strata reads each pin in the project of the first record that pins it. It calls `connect` one
/// time for each project, and keeps one cache of the registry listings for each project. It reads
/// each pack of a registry one time. A pin in a project that `connect` cannot reach has the error
/// of `connect`. When strata cannot read a record, the whole run fails: strata cannot say what the
/// record pins.
///
/// With `options.impact`, strata asks Palette about the bump of a pin in the project of the pin.
/// It does not ask a record of another project that pins the pack, and the bump says so.
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
    let Records { loaded, pins } = read_records(records)?;
    let check = Check {
        loaded: &loaded,
        impact: options.impact,
        pre_release: options.pre_release,
        generated_at,
        progress,
    };
    let mut out = check_pins(&check, pins, connect)?;
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
    };
    Ok(BehindReport {
        schema: BEHIND_SCHEMA.to_string(),
        tool: Tool::current(),
        generated_at: generated_at.to_string(),
        records: records.iter().map(|p| p.display().to_string()).collect(),
        since: options.since.as_ref().map(|p| p.generated_at.clone()),
        pins: out,
        summary,
    })
}

/// Read each record of `records`, and collect the pins in them. A pin is in the project of the
/// first record that pins it.
fn read_records(records: &[PathBuf]) -> Result<Records> {
    let mut loaded: Vec<Snapshot> = Vec::with_capacity(records.len());
    let mut pins: BTreeMap<PinKey, Pinned> = BTreeMap::new();
    for path in records {
        let snap = Snapshot::load(path)?;
        let subject = snap.source.title();
        for p in snap.packs.iter().filter(|p| !p.registry_uid.is_empty()) {
            let pinned = pins.entry(PinKey::of(p)).or_insert_with(|| Pinned {
                pin: Pin::of(p),
                project: snap.source.project_uid.clone(),
                holders: Vec::new(),
            });
            if pinned.holders.last() != Some(&loaded.len()) {
                pinned.holders.push(loaded.len());
            }
            let by = PinnedBy {
                path: path.display().to_string(),
                subject: subject.clone(),
            };
            if !pinned.pin.pinned_by.iter().any(|b| b.path == by.path) {
                pinned.pin.pinned_by.push(by);
            }
        }
        loaded.push(snap);
    }
    Ok(Records { loaded, pins })
}

/// Check each pin of `pins` in its scope, and give the pins in the order of their keys.
///
/// A cache of the registry listings borrows its client. So strata groups the pins by scope: each
/// scope connects one time, and keeps its cache for all its pins.
fn check_pins<A, F>(
    check: &Check<'_>,
    pins: BTreeMap<PinKey, Pinned>,
    mut connect: F,
) -> Result<Vec<Pin>>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    let mut scopes: BTreeMap<Option<String>, Vec<(PinKey, Pinned)>> = BTreeMap::new();
    for (key, pinned) in pins {
        scopes
            .entry(scope_of(&pinned.project).map(str::to_string))
            .or_default()
            .push((key, pinned));
    }
    let mut reads = Reads::default();
    let mut checked: BTreeMap<PinKey, Pin> = BTreeMap::new();
    for (scope, group) in scopes {
        match connect(scope.as_deref()) {
            Ok(palette) => {
                check_scope(
                    check,
                    &palette,
                    scope.as_deref(),
                    group,
                    &mut reads,
                    &mut checked,
                )?;
            }
            Err(e) => {
                for (key, mut pinned) in group {
                    pinned.pin.error = Some(e.to_json());
                    checked.insert(key, pinned.pin);
                }
            }
        }
    }
    Ok(checked.into_values().collect())
}

/// Check the pins of `group`, the pins of one scope, with the connection `palette` to that scope.
fn check_scope<A: PaletteApi>(
    check: &Check<'_>,
    palette: &Palette<A>,
    scope: Option<&str>,
    group: Vec<(PinKey, Pinned)>,
    reads: &mut Reads,
    checked: &mut BTreeMap<PinKey, Pin>,
) -> Result<()> {
    let Check {
        loaded,
        impact,
        pre_release,
        generated_at,
        progress,
    } = *check;
    let mut listings = Registries::new(palette);
    let mut questions = BumpQuestions::new(palette, scope);
    for (
        key,
        Pinned {
            mut pin, holders, ..
        },
    ) in group
    {
        read_pin(
            &mut pin,
            &mut listings,
            reads,
            pre_release,
            generated_at,
            progress,
        )?;
        if impact && pin.behind {
            let holding: Vec<&Snapshot> = holders.iter().filter_map(|&i| loaded.get(i)).collect();
            read_bump(
                &mut pin,
                &holding,
                &mut listings,
                &mut questions,
                generated_at,
                progress,
            );
        }
        checked.insert(key, pin);
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

/// The versions above the version of `pin` that a move can go to, newest first. `pinned` is the
/// build of the pinned version, when the registry still lists it. `pre_release` also names a
/// pre-release above a pin that is not one: see [`is_newer`].
fn newer_versions(
    pin: &Pin,
    offered: &Offered,
    pinned: Option<&PackBuild>,
    pre_release: bool,
) -> Vec<Newer> {
    let clouds: &[String] = pinned.map_or(&[], |b| b.cloud_types.as_slice());
    let architectures_of = |version: &str| -> &[String] {
        offered
            .architectures
            .get(version)
            .map_or(&[], Vec::as_slice)
    };
    let architectures = pinned.map_or(&[][..], |b| architectures_of(&b.version));
    // a newer version fits when it supports one cloud of the pinned version. A pinned version
    // with no cloud type supports each cloud, so each newer version fits it
    let fits =
        |b: &PackBuild| clouds.is_empty() || clouds.iter().any(|c| supports(&b.cloud_types, c));
    offered
        .builds
        .iter()
        .filter(|b| is_newer(&b.version, &pin.version, pre_release))
        .filter(|b| !b.retired())
        .filter(|b| fits(b))
        .map(|b| Newer {
            version: b.version.clone(),
            state: b.state.clone(),
            palette_latest: b.palette_latest,
            listed: b.listed.clone(),
            drops: dropped(architectures, architectures_of(&b.version)),
        })
        .collect()
}

/// True when `candidate` is a newer version than `pin`: it is above `pin` by [`version::compare`].
///
/// A pre-release is below its version, so a pin of `1.0.1` has no newer version in `1.0.1-rc.1`.
/// A pre-release above a pin that is not one is still not a version to move to, unless
/// `pre_release` asks for it. A pin that is a pre-release follows the pre-releases: it has each
/// version above it, and its final version is the first of them.
fn is_newer(candidate: &str, pin: &str, pre_release: bool) -> bool {
    version::compare(candidate, pin).is_gt()
        && (pre_release || version::is_pre_release(pin) || !version::is_pre_release(candidate))
}

/// The architectures of `pinned` that `newer` does not support: what a move drops. When the
/// listing names the architectures of only one of the two versions, strata cannot say what a
/// move drops, and the answer is empty.
fn dropped(pinned: &[String], newer: &[String]) -> Vec<String> {
    if pinned.is_empty() || newer.is_empty() {
        return Vec::new();
    }
    let newer: BTreeSet<&String> = newer.iter().collect();
    pinned
        .iter()
        .filter(|a| !newer.contains(a))
        .cloned()
        .collect()
}
