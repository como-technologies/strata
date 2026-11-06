//! The read half of the knurl platform port, on the strata library.
//!
//! knurl knows three roles: leaf, intermediate, and root. This crate is the only place that
//! knows what Palette calls them:
//!
//! | knurl | Palette | strata ref |
//! | --- | --- | --- |
//! | leaf | a pack version in a registry | `pack:<name>@<version>` |
//! | intermediate | a cluster profile version | `<name>@<version>` |
//! | root | nothing: Palette holds no root unit | none |
//! | instance | a cluster | `cluster:<name>` |
//!
//! strata reads, and it never writes. So this crate gives `PlatformRead` alone.
//!
//! The port is `async`, and strata blocks. The adapter moves each read to a blocking task, so
//! the thread that polls the future stays free. It uses tokio for that. The port of knurl names
//! no runtime, and a caller runs the future on a tokio runtime.
//!
//! The digests are those that strata prints. The exact digest is the root. The content digest
//! of an intermediate is `Snapshot::content_digest`, and the content digest of a leaf is the
//! digest of the pack in its registry. This crate gives each one the written form of knurl and
//! does not encode it again, so the digest that strata prints is the digest that knurl pins.
//!
//! # The definition of a unit
//!
//! knurl gives a defined unit its pins, as name and version in order, and its own parameters,
//! as named text. This crate reads one parameter, `candidate`: a strata candidate document, as
//! JSON. It holds what the pins do not: the tag and the registry of each layer, the values, the
//! manifests, and the variables. The pins give the order of the layers. Palette stores the
//! layers in the order given and keeps it, so the order is content, and knurl keeps it in the
//! pin set. [`definition_of`] writes that parameter from a published version.
//!
//! # The address of a leaf
//!
//! An address has no registry, and a tenant can hold one pack version in two kinds of registry
//! as two archives. The address can hold the content digest, and then the adapter gives the
//! archive with that digest. A pin holds the digest, so what pins the leaf can say which one it
//! means. With no digest, two archives are a terminal failure that names each one.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;
use std::marker::PhantomData;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use knurl_core::{
    Content, ContentDigest, Digest, ExactDigest, Health, InstanceName, InstanceObservation, Level,
    PendingChange, Pin, PinObservation, Pins, ReportedState, Timestamp, UnitDefinition, UnitName,
    UnitObservation, UnitRef, Version,
};
use knurl_ports::{PlatformRead, PortError, Result};
use serde_json::Value;
use strata::api::{Palette, PaletteApi};
use strata::candidate::{self, Candidate, CandidateLayer};
use strata::json::{arr, str_at};
use strata::ops;
use strata::reference::Ref;
use strata::snapshot::{AppliedProfile, PendingUpdate, Snapshot, SubjectKind};
use strata::term::progress::Progress;
use strata::text::name_or_uid;

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
    connect: Arc<F>,
    api: PhantomData<fn() -> A>,
}

/// A copy reads the same place through the same clients.
impl<A, F> Clone for StrataPlatform<A, F> {
    fn clone(&self) -> Self {
        Self {
            connect: Arc::clone(&self.connect),
            api: PhantomData,
        }
    }
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
    Now {
        stamp: Timestamp(seconds),
        text: strata::timestamp::utc_text(seconds as i64),
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

/// The health of a cluster, from its summary: `status.health.state` is `Healthy` or
/// `UnHealthy`. An unhealthy cluster names its first condition. Any other state, or none, is
/// Unknown: the port does not invent a health.
fn health_of(summary: &Value) -> Health {
    match str_at(summary, "/status/health/state") {
        "Healthy" => Health::Healthy,
        "UnHealthy" => {
            let reason = arr(summary, "/status/health/conditions")
                .iter()
                .find_map(|condition| {
                    match (str_at(condition, "/type"), str_at(condition, "/message")) {
                        ("", "") => None,
                        (kind, "") => Some(kind.to_string()),
                        (_, message) => Some(message.to_string()),
                    }
                })
                .unwrap_or_else(|| "UnHealthy".to_string());
            Health::Unhealthy(reason)
        }
        _ => Health::Unknown,
    }
}

/// `name version`: how a failure names a unit. A [`UnitRef`] and a [`UnitDefinition`] each have a
/// name and a version, so the caller gives both.
fn label(name: &impl Display, version: &impl Display) -> String {
    format!("{name} {version}")
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

/// Whether Palette says a change waits for the cluster: an open notification of an update of a
/// profile it runs, as a cluster record reads it. `Unknown` when the notifications were not read.
fn pending_of(raw: &strata::api::RawCluster) -> Result<PendingChange> {
    let Some(notifications) = &raw.notifications else {
        return Ok(PendingChange::Unknown);
    };
    let applied = AppliedProfile::of_cluster(&raw.cluster);
    let mut waiting = BTreeSet::new();
    for update in PendingUpdate::read(notifications, &applied) {
        waiting.insert(unit_name(&update.profile)?);
    }
    Ok(if waiting.is_empty() {
        PendingChange::Clear
    } else {
        PendingChange::Waiting(waiting)
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
            connect: Arc::new(connect),
            api: PhantomData,
        }
    }

    /// The strata record of a unit: all that the observation omits. A root is a terminal
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
                    "Palette holds no root unit, so it cannot give {}",
                    label(&unit.name, &unit.version)
                )));
            }
        };
        self.read_snapshot(&reference, at).map_err(port_error)
    }

    /// The record of `reference` at the time `at`, with the options that every read of the
    /// adapter uses, and no progress message.
    fn read_snapshot(&self, reference: &Ref, at: &str) -> strata::Result<Snapshot> {
        ops::snapshot_for(
            reference,
            &ops::SnapshotOptions::default(),
            || (self.connect)(None),
            at,
            &Progress::silent(),
        )
    }

    /// The two digests of a leaf. An address has no registry, and a real tenant has a pack in
    /// two or more of them. A raise copies a pack by digest, so the content is the same in each
    /// one, and then this is one leaf. Two kinds of registry can also hold two archives under
    /// one name and version. Then the digest of the address chooses one. With no digest, the
    /// failure names each registry and its archive.
    fn leaf(&self, unit: &UnitRef, at: &str) -> Result<(ExactDigest, ContentDigest)> {
        let of = label(&unit.name, &unit.version);
        let read = |registry: Option<String>| self.read_snapshot(&pack_ref(unit, registry), at);
        // A registry that Palette does not sync lists no digest for a pack. Then there is no
        // content to pin by, and the leaf is refused (knurl D52).
        let archive = |snapshot: &Snapshot| match snapshot.packs.first() {
            Some(pack) if pack.hash.upstream_raw.is_empty() => Err(refused(format!(
                "{of}: the registry lists no digest for this pack, so it cannot be pinned by content"
            ))),
            Some(pack) => Ok(pack.hash.upstream_raw.clone()),
            None => Err(refused(format!("{of} has no pack"))),
        };
        let registries = match read(None) {
            // One object. The digest of the address chooses nothing, and it refuses nothing:
            // the caller compares.
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
        let each = || -> String {
            let each: Vec<String> = copies
                .iter()
                .map(|copy| format!("{} has {}", copy.registry, copy.archive))
                .collect();
            each.join(", ")
        };
        let held: Vec<&Copy> = match (archives.len(), unit.digest) {
            // One archive, in each registry: one leaf.
            (1, _) => copies.iter().collect(),
            (count, None) => {
                return Err(refused(format!(
                    "{of} is {count} archives under one version: {}. An address with no digest cannot say which one it means",
                    each()
                )));
            }
            (count, Some(wanted)) => {
                let held: Vec<&Copy> = copies
                    .iter()
                    .filter(|copy| digest::<Content>(&copy.archive, &of).ok() == Some(wanted))
                    .collect();
                if held.is_empty() {
                    return Err(refused(format!(
                        "{of} is {count} archives under one version, and none is {wanted}: {}",
                        each()
                    )));
                }
                held
            }
        };
        // Several objects hold the one content. The exact digest covers each of them, so it
        // changes when a registry gets a copy or loses one.
        let mut exact = ExactDigest::builder().number(held.len() as u64);
        for copy in &held {
            exact = exact.text(&copy.registry).text(&copy.root);
        }
        Ok((exact.finish(), digest(&held[0].archive, &of)?))
    }

    /// The digests and the pins of an intermediate: what its record holds. The pins are in
    /// the order of the layers, as Palette stores them.
    fn intermediate(
        &self,
        unit: &UnitRef,
        at: &str,
    ) -> Result<(ExactDigest, ContentDigest, Pins<Pin>)> {
        let snapshot = self.snapshot_at(unit, at)?;
        let mut pins = Pins::new();
        for pin in snapshot.pins() {
            let held = Pin {
                version: version(&pin.version)?,
                digest: digest(&pin.digest, &pin.name)?,
            };
            pins.insert(unit_name(&pin.name)?, held);
        }
        let (exact, content) = digests(&snapshot, &label(&unit.name, &unit.version))?;
        Ok((exact, content, pins))
    }
}

/// The definition of a published profile version, in the form that [`PlatformRead::identity_of`]
/// reads: each layer as a pin, in order, and the candidate document as the one parameter.
pub fn definition_of(snapshot: &Snapshot) -> Result<UnitDefinition> {
    let candidate = Candidate::from_snapshot(snapshot).map_err(port_error)?;
    let mut pins = Pins::new();
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

/// The strata candidate of a definition: the document of the parameter, with the name and the
/// version of the definition, and the layers in the order of its pins, each at the pinned
/// version. The pins own the order and the version. The document gives the rest of each layer,
/// by name.
fn candidate_of(unit: &UnitDefinition) -> Result<Candidate> {
    let bad = |why: String| {
        refused(format!(
            "the definition of {}: {why}",
            label(&unit.name, &unit.version)
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
    if unit.pins.is_empty() {
        return Err(bad("it has no pins".into()));
    }
    let mut rest: BTreeMap<String, CandidateLayer> = candidate
        .layers
        .drain(..)
        .map(|layer| (layer.name.clone(), layer))
        .collect();
    let mut layers = Vec::with_capacity(unit.pins.len());
    for (name, version) in &unit.pins {
        let mut layer = rest
            .remove(name.as_str())
            .ok_or_else(|| bad(format!("'{CANDIDATE}' has no layer '{name}'")))?;
        layer.version = version.to_string();
        layers.push(layer);
    }
    if let Some(extra) = rest.keys().next() {
        return Err(bad(format!(
            "the layer '{extra}' of '{CANDIDATE}' has no pin"
        )));
    }
    candidate.layers = layers;
    Ok(candidate)
}

/// Each read of the port, on this thread. The port runs each one on a blocking task.
impl<A: PaletteApi + 'static, F: Connect<A> + 'static> StrataPlatform<A, F> {
    /// Runs `read` on a blocking task, and waits for its answer. strata blocks, and the port is
    /// `async`, so the thread that polls the future stays free (knurl D43). A panic in the read
    /// is a panic here.
    async fn blocking<T: Send + 'static>(
        &self,
        read: impl FnOnce(&Self) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let this = self.clone();
        match tokio::task::spawn_blocking(move || read(&this)).await {
            Ok(answer) => answer,
            Err(join) if join.is_panic() => std::panic::resume_unwind(join.into_panic()),
            Err(join) => Err(PortError::unknown(format!("the read was canceled: {join}"))),
        }
    }

    fn observe_unit_blocking(&self, unit: &UnitRef) -> Result<UnitObservation> {
        let now = now();
        let (exact, content, pins) = match unit.level {
            // The content of a leaf is the archive in the registry, so its digest is the
            // content digest. It pins nothing that Palette holds.
            Level::Leaf => {
                let (exact, content) = self.leaf(unit, &now.text)?;
                (exact, content, Pins::new())
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

    fn observe_instance_blocking(&self, instance: &InstanceName) -> Result<InstanceObservation> {
        let now = now();
        let palette = (self.connect)(None).map_err(port_error)?;
        let raw = palette
            .fetch_subject(SubjectKind::Cluster, instance.as_str(), &Progress::silent())
            .map_err(port_error)?;
        let reference = Ref::Cluster {
            uid: instance.to_string(),
            profile: None,
            pending: false,
        }
        .to_string();
        // Each one is the copy that the cluster holds, and not the published profile. Palette
        // lists the profiles of a cluster in an order that differs between two clusters built
        // from one definition, so the order says nothing. The copies are by name (knurl D49).
        let mut copies =
            Snapshot::each_from_cluster(&raw, &reference, &now.text).map_err(port_error)?;
        copies.sort_by(|a, b| a.source.name.cmp(&b.source.name));
        let mut pins = Pins::new();
        for copy in copies {
            let (exact, content) = digests(&copy, &copy.source.name)?;
            let held = PinObservation::new(version(&copy.source.version)?, exact, content);
            pins.insert(unit_name(&copy.source.name)?, held);
        }
        let state = str_at(&raw.cluster, "/status/state");
        // The health is on the cluster summary, and not on the cluster document. A Palette that
        // does not serve the summary, with HTTP 404 or 405, gives no health, and the port says
        // Unknown for that.
        let uid = str_at(&raw.cluster, "/metadata/uid");
        let health = match palette.fetch_cluster_summary(uid) {
            Ok(summary) => health_of(&summary),
            Err(error) if error.is_missing_route() => Health::Unknown,
            Err(error) => return Err(port_error(error)),
        };
        Ok(InstanceObservation::new(
            instance.clone(),
            ReportedState(state.to_string()),
            health,
            pins,
            now.stamp,
        )
        .with_pending(pending_of(&raw)?))
    }

    fn pinned_by_blocking(&self, unit: &UnitRef) -> Result<BTreeSet<InstanceName>> {
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
            .map(|cluster| name_or_uid(&cluster.name, &cluster.uid))
            .map(|name| InstanceName::new(name).map_err(|error| refused(error.to_string())))
            .collect()
    }

    fn identity_of_blocking(&self, unit: &UnitDefinition) -> Result<ContentDigest> {
        let candidate = candidate_of(unit)?;
        // The candidate is in memory, and its values and manifests are in it, so no file is
        // read. The name is the ref of the record, as the path of a candidate file would be.
        let named = Ref::NameVersion {
            name: unit.name.to_string(),
            version: unit.version.to_string(),
        }
        .to_string();
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
            &label(&unit.name, &unit.version),
        )
    }
}

impl<A: PaletteApi + 'static, F: Connect<A> + 'static> PlatformRead for StrataPlatform<A, F> {
    async fn observe_unit(&self, unit: &UnitRef) -> Result<UnitObservation> {
        let unit = unit.clone();
        self.blocking(move |this| this.observe_unit_blocking(&unit))
            .await
    }

    async fn observe_instance(&self, instance: &InstanceName) -> Result<InstanceObservation> {
        let instance = instance.clone();
        self.blocking(move |this| this.observe_instance_blocking(&instance))
            .await
    }

    async fn pinned_by(&self, unit: &UnitRef) -> Result<BTreeSet<InstanceName>> {
        let unit = unit.clone();
        self.blocking(move |this| this.pinned_by_blocking(&unit))
            .await
    }

    async fn identity_of(&self, unit: &UnitDefinition) -> Result<ContentDigest> {
        let unit = unit.clone();
        self.blocking(move |this| this.identity_of_blocking(&unit))
            .await
    }
}
