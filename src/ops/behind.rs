//! `behind`: the pack versions that a set of records pins, against the newer versions that the
//! registries offer.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::{Palette, PaletteApi};
use crate::error::{Error, Result};
use crate::impact::ImpactLevel;
use crate::progress::Progress;
use crate::snapshot::{Snapshot, Tool};
use crate::version;

use super::packs::{PackBuild, PackOptions, registry_packs, supports};

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
    pub generated_at: String,
    /// The records read, in the order given.
    pub records: Vec<String>,
    /// With `--since`: when the previous report was written. `arrived` on each pin is against it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    pub pinned_state: Option<String>,
    /// Each record that pins it.
    pub pinned_by: Vec<PinnedBy>,
    /// The newer versions that the registry offers, newest first. A version that the registry
    /// has disabled or deleted is not one to move to, and a version for none of the clouds of the
    /// pinned version is not either. A version that supports fewer architectures than the pinned
    /// version is one, and it names what it drops.
    pub newer: Vec<Newer>,
    /// True when `newer` is not empty.
    pub behind: bool,
    /// For a pin that is behind: what a move to the newest version does, from `compare` of the
    /// two pack versions. Absent with `--no-impact`, or when strata could not compare them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bump: Option<Bump>,
    /// With `--since`: the newer versions that the previous report did not know. Absent when the
    /// previous report had no pin of this pack in this registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrived: Option<Vec<String>>,
    /// Why strata could not check the pin, when it could not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
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
    pub state: Option<String>,
    /// True for the version that Palette calls the latest: see [`PackBuild::palette_latest`].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub palette_latest: bool,
    /// When Palette first listed the version; empty when the listing gives no time.
    #[serde(default, skip_serializing_if = "String::is_empty")]
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
    pub impact: ImpactLevel,
    /// The change flags: `IDENTITY`, `UPSTREAM`, `AUTHORED`, `IMAGES`, `CHARTS`.
    pub flags: Vec<String>,
}

/// What `behind` reads.
#[derive(Debug, Clone, Default)]
pub struct BehindOptions {
    /// Compare each pin that is behind with the newest version, for its impact: two pack reads
    /// for each such pin.
    pub impact: bool,
    /// The report of the previous run: each pin names the versions that arrived since.
    pub since: Option<BehindReport>,
}

/// What a registry offers for one pack: the registry name, and its versions.
type Offered = (String, Vec<PackBuild>);

/// A floating tag names no one version: `1.x`, `1.30.x`, `LTS__24.x`.
pub fn is_floating(tag: &str) -> bool {
    tag.split(['.', '_', '-']).any(|part| part == "x")
}

/// Report the pack versions that `records` pin, and the newer versions that the registries offer.
/// Each record is read in its own project, with a client from `connect`; each pack of a
/// registry is read one time. A record that cannot be read is an error of the whole run: strata
/// cannot say what it pins.
pub fn behind<A, F>(
    records: &[PathBuf],
    options: &BehindOptions,
    mut connect: F,
    generated_at: &str,
    progress: &Progress,
) -> Result<BehindReport>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    // pack name, registry uid, version, tag -> the pin, and the project of its first record
    let mut pins: BTreeMap<(String, String, String, String), (Pin, String)> = BTreeMap::new();
    for path in records {
        let snap = Snapshot::load(path)?;
        let subject = snap.source.title();
        for p in snap.packs.iter().filter(|p| !p.registry_uid.is_empty()) {
            let key = (
                p.name.clone(),
                p.registry_uid.clone(),
                p.version.clone(),
                p.tag.clone(),
            );
            let entry = pins.entry(key).or_insert_with(|| {
                (
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
                    },
                    snap.source.project_uid.clone(),
                )
            });
            let by = PinnedBy {
                path: path.display().to_string(),
                subject: subject.clone(),
            };
            if !entry.0.pinned_by.iter().any(|b| b.path == by.path) {
                entry.0.pinned_by.push(by);
            }
        }
    }

    // each pack of each registry, read one time
    let mut offers: BTreeMap<(String, String), Result<Offered>> = BTreeMap::new();
    let mut out: Vec<(Pin, String)> = Vec::with_capacity(pins.len());
    for ((name, registry_uid, _, _), (mut pin, project)) in pins {
        let scope = Some(project.as_str()).filter(|p| !p.is_empty());
        let offered = offers
            .entry((name.clone(), registry_uid.clone()))
            .or_insert_with(|| {
                registry_packs(
                    &name,
                    &PackOptions {
                        registry: Some(registry_uid.clone()),
                        ..PackOptions::default()
                    },
                    |_| connect(scope),
                    generated_at,
                    progress,
                )
                .map(|l| {
                    l.registries
                        .into_iter()
                        .next()
                        .map(|r| (r.registry, r.versions))
                        .unwrap_or_default()
                })
            });
        let (registry, builds) = match offered {
            Ok(found) => found.clone(),
            Err(e) => {
                pin.error = Some(e.to_json());
                out.push((pin, project));
                continue;
            }
        };
        pin.registry = registry;
        let pinned = builds.iter().find(|b| b.version == pin.version);
        pin.pinned_state = match pinned {
            Some(b) => b.state.clone(),
            None => Some("absent".to_string()),
        };
        // the version that a floating tag names now. A profile with values of its own stays on
        // the pinned version until a person applies the update, so each version above the pin
        // is newer, also the one that the tag names
        if pin.floating {
            pin.resolved = builds
                .iter()
                .find(|b| b.tags.iter().any(|t| t == &pin.tag))
                .map(|b| b.version.clone());
        }
        let clouds: Vec<String> = pinned.map(|b| b.cloud_types.clone()).unwrap_or_default();
        let architectures: Vec<String> =
            pinned.map(|b| b.architectures.clone()).unwrap_or_default();
        let fits = |b: &PackBuild| {
            clouds.is_empty()
                || clouds.iter().any(|c| c == "all")
                || b.cloud_types.is_empty()
                || clouds.iter().any(|c| supports(&b.cloud_types, c))
        };
        pin.newer = builds
            .iter()
            .filter(|b| version::compare(&b.version, &pin.version) == std::cmp::Ordering::Greater)
            .filter(|b| !matches!(b.state.as_deref(), Some("disabled" | "deleted")))
            .filter(|b| fits(b))
            .map(|b| Newer {
                version: b.version.clone(),
                state: b.state.clone(),
                palette_latest: b.palette_latest,
                listed: b.listed.clone(),
                drops: dropped(&architectures, &b.architectures),
            })
            .collect();
        pin.behind = !pin.newer.is_empty();
        out.push((pin, project));
    }

    if options.impact {
        for (pin, project) in out.iter_mut().filter(|(p, _)| p.behind) {
            let to = pin.newer[0].version.clone();
            progress.note(&format!("comparing {} {} with {to}", pin.name, pin.version));
            let scope = Some(project.as_str()).filter(|p| !p.is_empty());
            let bump = connect(scope).and_then(|palette| {
                bump_of(
                    &pin.name,
                    &pin.registry_uid,
                    &pin.version,
                    &to,
                    &palette,
                    generated_at,
                    progress,
                )
            });
            match bump {
                Ok(bump) => pin.bump = Some(bump),
                Err(e) => pin.error = Some(e.to_json()),
            }
        }
    }
    let mut out: Vec<Pin> = out.into_iter().map(|(pin, _)| pin).collect();

    let mut arrived = 0;
    if let Some(previous) = &options.since {
        for pin in &mut out {
            let known: BTreeSet<&str> = previous
                .pins
                .iter()
                .filter(|p| p.name == pin.name && p.registry_uid == pin.registry_uid)
                .flat_map(|p| {
                    std::iter::once(p.version.as_str())
                        .chain(p.newer.iter().map(|n| n.version.as_str()))
                })
                .collect();
            if previous
                .pins
                .iter()
                .any(|p| p.name == pin.name && p.registry_uid == pin.registry_uid)
            {
                let new: Vec<String> = pin
                    .newer
                    .iter()
                    .filter(|n| !known.contains(n.version.as_str()))
                    .map(|n| n.version.clone())
                    .collect();
                arrived += new.len();
                pin.arrived = Some(new);
            }
        }
    }

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

/// The architectures of `pinned` that `newer` does not support: what a move drops. When the
/// listing names the architectures of only one of the two versions, strata cannot say what a
/// move drops, and the answer is empty.
fn dropped(pinned: &[String], newer: &[String]) -> Vec<String> {
    if pinned.is_empty() || newer.is_empty() {
        return Vec::new();
    }
    pinned
        .iter()
        .filter(|a| !newer.contains(a))
        .cloned()
        .collect()
}

/// What a move from `from` to `to` of one pack in one registry does: `compare` of the two pack
/// versions.
fn bump_of<A: PaletteApi>(
    name: &str,
    registry_uid: &str,
    from: &str,
    to: &str,
    palette: &Palette<A>,
    at: &str,
    progress: &Progress,
) -> Result<Bump> {
    let take = |version: &str| {
        crate::candidate::pack_snapshot(
            palette,
            Some(registry_uid),
            name,
            version,
            &format!("pack:{name}@{version}"),
            at,
            progress,
        )
    };
    let (a, b) = (take(from)?, take(to)?);
    let d = super::compare(&a, &b, at);
    let pd = d
        .packs
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| Error::Usage {
            message: format!("the comparison of {name} {from} and {to} holds no such pack"),
        })?;
    Ok(Bump {
        to: to.to_string(),
        impact: d.summary.max_impact,
        flags: crate::render::plain_flags(pd)
            .split_whitespace()
            .map(str::to_string)
            .collect(),
    })
}
