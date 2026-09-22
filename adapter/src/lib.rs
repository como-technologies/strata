//! The read half of the knurl platform port, on the strata library.
//!
//! knurl knows three roles: leaf, intermediate, and root. This crate is the only place that
//! knows what Palette calls them:
//!
//! | knurl | Palette | strata ref |
//! | --- | --- | --- |
//! | leaf | a pack version in a registry | `pack:<name>@<version>` |
//! | intermediate | a cluster profile version | `<name>@<version>` |
//! | root | nothing: a root is in the forge | none |
//! | instance | a cluster | `cluster:<name>` |
//!
//! strata reads, and it never writes. So this crate gives `PlatformRead` alone.
//!
//! The digests are those that strata prints. The exact digest is the root. The content digest
//! of an intermediate is `Snapshot::content_digest`, and the content digest of a leaf is the
//! digest of the pack in its registry. This crate gives each one the written form of knurl and
//! does not encode it again, so the digest that strata prints is the digest that knurl pins.
//!
//! # The definition of a unit
//!
//! knurl gives a defined unit its pins, as name and version, and its own parameters, as named
//! text. This crate reads one parameter, `candidate`: a strata candidate document, as JSON.
//! It holds what the pins do not: the order of the layers, the tag and the registry of each
//! one, the values, the manifests, and the variables. The order is there because Palette
//! installs the layers in order, so the order is content. [`definition_of`] writes that
//! parameter from a published version.

use std::collections::{BTreeMap, BTreeSet};
use std::marker::PhantomData;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use knurl_core::{
    ContentDigest, Digest, ExactDigest, Health, InstanceName, InstanceObservation, Level, Pin,
    PinObservation, ReportedState, Timestamp, UnitDefinition, UnitName, UnitObservation, UnitRef,
    Version,
};
use knurl_ports::{PlatformRead, PortError, Result};
use serde_json::Value;
use strata::api::{Palette, PaletteApi};
use strata::candidate::{self, Candidate};
use strata::ops;
use strata::progress::Progress;
use strata::reference::Ref;
use strata::snapshot::Snapshot;

/// The parameter of a [`UnitDefinition`] that holds the strata candidate document.
pub const CANDIDATE: &str = "candidate";

/// A function that gives a client of Palette: for a project, or for the scope of the adapter
/// with `None`. The environment owns the project, so a caller makes one adapter value for each
/// environment.
pub trait Connect<A: PaletteApi>:
    Fn(Option<&str>) -> strata::Result<Palette<A>> + Send + Sync
{
}

impl<A: PaletteApi, F> Connect<A> for F where
    F: Fn(Option<&str>) -> strata::Result<Palette<A>> + Send + Sync
{
}

/// Palette, through strata, by the clients that a [`Connect`] gives.
pub struct StrataPlatform<A, F> {
    connect: F,
    api: PhantomData<fn() -> A>,
}

/// The time of one read: for knurl, and for the record of strata.
struct Now {
    stamp: Timestamp,
    text: String,
}

fn now() -> Now {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let text = chrono::DateTime::from_timestamp(seconds as i64, 0)
        .unwrap_or_default()
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    Now {
        stamp: Timestamp(seconds),
        text,
    }
}

/// A failure of strata, with the class that strata gives it and the error as the cause.
fn port_error(error: strata::Error) -> PortError {
    let message = error.to_string();
    let port = match error.class() {
        strata::error::FailureClass::Transient => PortError::transient(message),
        strata::error::FailureClass::Terminal => PortError::terminal(message),
        strata::error::FailureClass::Unknown => PortError::unknown(message),
    };
    port.with_source(error)
}

/// A value that the data of Palette cannot give is a failure that stays.
fn refused(message: String) -> PortError {
    PortError::terminal(message)
}

/// `name version`: how a failure names a unit.
fn label(unit: &UnitRef) -> String {
    format!("{} {}", unit.name, unit.version)
}

/// The strata ref of an intermediate: a profile version.
fn profile_ref(unit: &UnitRef) -> Ref {
    Ref::NameVersion {
        name: unit.name.to_string(),
        version: unit.version.to_string(),
    }
}

/// The strata ref of a leaf: a pack version, in one registry or in any.
fn pack_ref(unit: &UnitRef, registry: Option<String>) -> Ref {
    Ref::Pack {
        registry,
        name: unit.name.to_string(),
        version: unit.version.to_string(),
    }
}

/// A digest of strata or of a registry, in the written form of knurl. A Helm digest is bare
/// hex, and a pack digest has the prefix: both are sha256.
fn digest<K>(text: &str, of: &str) -> Result<Digest<K>> {
    let hex = text.strip_prefix("sha256:").unwrap_or(text);
    format!("sha256:{hex}").parse().map_err(|_| {
        refused(format!(
            "{of} has the digest '{text}', which is not a sha256 digest"
        ))
    })
}

fn unit_name(text: &str) -> Result<UnitName> {
    UnitName::new(text).map_err(|error| refused(error.to_string()))
}

fn version(text: &str) -> Result<Version> {
    Version::new(text).map_err(|error| refused(error.to_string()))
}

/// The two digests of a strata record.
fn digests(snapshot: &Snapshot, of: &str) -> Result<(ExactDigest, ContentDigest)> {
    Ok((
        digest(&snapshot.root, of)?,
        digest(&snapshot.content_digest(), of)?,
    ))
}

/// One registry's copy of a leaf: the root of the object there, and the archive it serves.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Copy {
    registry: String,
    root: String,
    archive: String,
}

impl<A: PaletteApi, F: Connect<A>> StrataPlatform<A, F> {
    /// An adapter that reads Palette through the clients that `connect` gives.
    pub fn new(connect: F) -> Self {
        Self {
            connect,
            api: PhantomData,
        }
    }

    /// The strata record of a unit: all that the observation leaves out. A root is a terminal
    /// failure, because Palette holds no root.
    pub fn snapshot(&self, unit: &UnitRef) -> Result<Snapshot> {
        self.snapshot_at(unit, &now().text)
    }

    fn snapshot_at(&self, unit: &UnitRef, at: &str) -> Result<Snapshot> {
        let reference = match unit.level {
            Level::Intermediate => profile_ref(unit),
            Level::Leaf => pack_ref(unit, None),
            Level::Root => {
                return Err(refused(format!(
                    "Palette holds no root unit, so it cannot give {}: a root is in the forge",
                    label(unit)
                )));
            }
        };
        ops::snapshot_for(&reference, || (self.connect)(None), at, &Progress::silent())
            .map_err(port_error)
    }

    /// The two digests of a leaf. An address has no registry, and a real tenant has a pack in
    /// two or more of them. A raise copies a pack by digest, so the content is the same in each
    /// one, and then this is one leaf. Two kinds of registry can also hold two archives under
    /// one name and version. Then knurl cannot say which one it means, and the failure names
    /// each registry and its digest.
    fn leaf(&self, unit: &UnitRef, at: &str) -> Result<(ExactDigest, ContentDigest)> {
        let of = label(unit);
        let read = |registry: Option<String>| {
            let reference = pack_ref(unit, registry);
            ops::snapshot_for(&reference, || (self.connect)(None), at, &Progress::silent())
        };
        let archive = |snapshot: &Snapshot| match snapshot.packs.first() {
            Some(pack) => Ok(pack.hash.upstream_raw.clone()),
            None => Err(refused(format!("{of} has no pack"))),
        };
        let registries = match read(None) {
            Ok(one) => return Ok((digest(&one.root, &of)?, digest(&archive(&one)?, &of)?)),
            Err(strata::Error::PackInSeveralRegistries { registries, .. }) => registries,
            Err(error) => return Err(port_error(error)),
        };

        let mut copies = Vec::with_capacity(registries.len());
        for registry in registries {
            let snapshot = read(Some(registry.clone())).map_err(port_error)?;
            copies.push(Copy {
                registry,
                root: snapshot.root.clone(),
                archive: archive(&snapshot)?,
            });
        }
        copies.sort();
        let archives: BTreeSet<&str> = copies.iter().map(|copy| copy.archive.as_str()).collect();
        if archives.len() > 1 {
            let each: Vec<String> = copies
                .iter()
                .map(|copy| format!("{} has {}", copy.registry, copy.archive))
                .collect();
            return Err(refused(format!(
                "{of} is {} archives under one version: {}. An address has no registry, so knurl cannot say which one it means",
                archives.len(),
                each.join(", ")
            )));
        }
        // Several objects hold the one leaf. The exact digest covers each of them, so it
        // changes when a registry gets a copy or loses one.
        let mut exact = ExactDigest::builder().number(copies.len() as u64);
        for copy in &copies {
            exact = exact.text(&copy.registry).text(&copy.root);
        }
        Ok((exact.finish(), digest(&copies[0].archive, &of)?))
    }

    /// The digests and the pins of an intermediate: what its record holds.
    fn intermediate(
        &self,
        unit: &UnitRef,
        at: &str,
    ) -> Result<(ExactDigest, ContentDigest, BTreeMap<UnitName, Pin>)> {
        let snapshot = self.snapshot_at(unit, at)?;
        let mut pins = BTreeMap::new();
        for pin in snapshot.pins() {
            let held = Pin {
                version: version(&pin.version)?,
                digest: digest(&pin.digest, &pin.name)?,
            };
            pins.insert(unit_name(&pin.name)?, held);
        }
        let (exact, content) = digests(&snapshot, &label(unit))?;
        Ok((exact, content, pins))
    }
}

/// The definition of a published profile version, in the form that [`PlatformRead::identity_of`]
/// reads: each layer as a pin, and the candidate document as the one parameter.
pub fn definition_of(snapshot: &Snapshot) -> Result<UnitDefinition> {
    let candidate = Candidate::from_snapshot(snapshot).map_err(port_error)?;
    let mut pins = BTreeMap::new();
    for layer in &candidate.layers {
        pins.insert(unit_name(&layer.name)?, version(&layer.version)?);
    }
    let document = serde_json::to_string(&candidate).map_err(|error| {
        refused(format!(
            "the candidate of {} is not JSON: {error}",
            candidate.name
        ))
    })?;
    Ok(UnitDefinition {
        name: unit_name(&candidate.name)?,
        version: version(&candidate.version)?,
        level: Level::Intermediate,
        pins,
        parameters: BTreeMap::from([(CANDIDATE.to_string(), document)]),
    })
}

/// The strata candidate of a definition: the document of the parameter, with the name, the
/// version, and the version of each layer from the definition.
fn candidate_of(unit: &UnitDefinition) -> Result<Candidate> {
    let bad = |why: String| {
        refused(format!(
            "the definition of {} {}: {why}",
            unit.name, unit.version
        ))
    };
    let text = unit
        .parameters
        .get(CANDIDATE)
        .ok_or_else(|| bad(format!("it has no parameter '{CANDIDATE}'")))?;
    let mut candidate: Candidate = serde_json::from_str(text)
        .map_err(|e| bad(format!("'{CANDIDATE}' is not a candidate: {e}")))?;
    candidate.name = unit.name.to_string();
    candidate.version = unit.version.to_string();
    if candidate.layers.is_empty() {
        return Err(bad("it has no layers".into()));
    }
    for layer in &mut candidate.layers {
        let pinned = unit
            .pins
            .get(layer.name.as_str())
            .ok_or_else(|| bad(format!("no pin for the layer '{}'", layer.name)))?;
        layer.version = pinned.to_string();
    }
    // Each layer has a pin. When the counts are equal too, each pin has a layer.
    if candidate.layers.len() != unit.pins.len() {
        return Err(bad("a pin has no layer in the candidate document".into()));
    }
    Ok(candidate)
}

impl<A: PaletteApi, F: Connect<A>> PlatformRead for StrataPlatform<A, F> {
    async fn observe_unit(&self, unit: &UnitRef) -> Result<UnitObservation> {
        let now = now();
        let (exact, content, pins) = match unit.level {
            // The content of a leaf is the archive in the registry, so its digest is the
            // content digest. It pins nothing that Palette holds.
            Level::Leaf => {
                let (exact, content) = self.leaf(unit, &now.text)?;
                (exact, content, BTreeMap::new())
            }
            Level::Intermediate | Level::Root => self.intermediate(unit, &now.text)?,
        };
        Ok(UnitObservation::new(
            unit.clone(),
            exact,
            content,
            pins,
            now.stamp,
        ))
    }

    async fn observe_instance(&self, instance: &InstanceName) -> Result<InstanceObservation> {
        let now = now();
        let palette = (self.connect)(None).map_err(port_error)?;
        let raw = palette
            .fetch_cluster(instance.as_str(), &Progress::silent())
            .map_err(port_error)?;
        let reference = format!("cluster:{instance}");
        let mut pins = BTreeMap::new();
        // Each one is the copy that the cluster holds, and not the published profile.
        for copy in Snapshot::each_from_cluster(&raw, &reference, &now.text).map_err(port_error)? {
            let (exact, content) = digests(&copy, &copy.source.name)?;
            let held = PinObservation::new(version(&copy.source.version)?, exact, content);
            pins.insert(unit_name(&copy.source.name)?, held);
        }
        let state = raw
            .cluster
            .pointer("/status/state")
            .and_then(Value::as_str)
            .unwrap_or("");
        // strata reads no health. The port says so, and does not invent one.
        Ok(InstanceObservation::new(
            instance.clone(),
            ReportedState(state.to_string()),
            Health::Unknown,
            pins,
            now.stamp,
        ))
    }

    async fn pinned_by(&self, unit: &UnitRef) -> Result<BTreeSet<InstanceName>> {
        if unit.level != Level::Intermediate {
            return Err(refused(format!(
                "Palette names the clusters of a profile version alone, and {} is a {:?}",
                unit.name, unit.level
            )));
        }
        let report = ops::consumers(
            &profile_ref(unit),
            |project| (self.connect)(project),
            &now().text,
            &Progress::silent(),
        )
        .map_err(port_error)?;
        report
            .consumers
            .iter()
            .map(|cluster| {
                if cluster.name.is_empty() {
                    &cluster.uid
                } else {
                    &cluster.name
                }
            })
            .map(|name| {
                InstanceName::new(name.as_str()).map_err(|error| refused(error.to_string()))
            })
            .collect()
    }

    async fn identity_of(&self, unit: &UnitDefinition) -> Result<ContentDigest> {
        let candidate = candidate_of(unit)?;
        // The candidate is in memory, and its values and manifests are in it, so no file is
        // read. The name is the ref of the record, as the path of a candidate file would be.
        let named = format!("{}@{}", unit.name, unit.version);
        let loaded = candidate::check(candidate, Path::new(&named)).map_err(port_error)?;
        let snapshot = ops::snapshot_from_loaded(
            &loaded,
            || (self.connect)(None),
            &now().text,
            &Progress::silent(),
        )
        .map_err(port_error)?;
        digest(
            &snapshot.content_digest(),
            &format!("{} {}", unit.name, unit.version),
        )
    }
}
