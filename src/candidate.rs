//! A profile version that does not exist in Palette yet, described by a file.
//!
//! The file names the pack of each layer by name, version, and registry, and holds the values
//! and manifests that the profile will have. Strata reads each pack from the registry for what
//! Palette adds at the publish: the pack uid, the tag, the digest, and the default values. It
//! builds the same documents that a published profile gives, so the candidate has the same root
//! as the published version. A `manifest` layer holds manifests from no registry. Strata makes no
//! request for it, and its pack has no uid until Palette mints one at the create.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::api::{Palette, PaletteApi, RawProfile};
use crate::canon::{Canon, canon_yaml};
use crate::error::{Error, Result};
use crate::progress::Progress;
use crate::reference::LATEST;
use crate::snapshot::{PackSource, PresetOption, Snapshot};

mod export;
mod gaps;

/// The `source.template` of a snapshot built from a candidate file.
pub const TEMPLATE: &str = "candidate";

/// The `source.template` of a snapshot of one pack version from a registry.
pub const PACK_TEMPLATE: &str = "registry";

/// The `source.type` of such a snapshot.
pub const PACK_TYPE: &str = "pack";

/// A candidate profile version. YAML or JSON. Quote versions that YAML would read as numbers,
/// such as `"22.04"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Candidate {
    /// Profile name.
    pub name: String,
    /// Profile version.
    pub version: String,
    /// Profile type: cluster, infra, add-on, system.
    #[serde(default = "default_type", rename = "type")]
    pub kind: String,
    /// Cloud type, for example `aws`.
    #[serde(default)]
    pub cloud_type: String,
    /// The registry for layers that name none, by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
    /// The registry for layers that name none, by uid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_uid: Option<String>,
    /// The layers, in profile order.
    pub layers: Vec<CandidateLayer>,
    /// Profile variable definitions, as Palette returns them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<Value>,
}

impl Candidate {
    /// The candidate with the content of a profile record: the same packs in order, each with its
    /// tag, registry uid, values, params, and manifests spelled out, and the same variables, so
    /// `snapshot --from-candidate` turns it back into the record's root while the registry still
    /// serves those pack versions. A cluster record is refused: it is not one profile version.
    pub fn from_snapshot(snap: &Snapshot) -> Result<Candidate> {
        // a candidate describes a profile version to publish: a cluster holds a copy of one, a
        // pack is one layer of one, and an app profile has tiers, so none makes a candidate
        if snap.source.cluster.is_some()
            || snap.source.pack.is_some()
            || snap.source.kind == crate::snapshot::APP_KIND
        {
            return Err(Error::ProfileRefRequired {
                reference: snap.source.reference.clone(),
            });
        }
        let nonempty = |s: &str| Some(s.to_string()).filter(|s| !s.is_empty());
        Ok(Candidate {
            name: snap.source.name.clone(),
            version: snap.source.version.clone(),
            kind: snap.source.kind.clone(),
            cloud_type: snap.source.cloud_type.clone(),
            registry: None,
            registry_uid: None,
            layers: snap
                .packs
                .iter()
                .map(|p| CandidateLayer {
                    name: p.name.clone(),
                    version: p.version.clone(),
                    layer: nonempty(&p.layer),
                    // a pack from a registry takes its type from the registry; a manifest pack has
                    // no registry, so the file says what it is
                    kind: Some(p.kind.clone()).filter(|k| k == MANIFEST),
                    tag: nonempty(&p.tag),
                    registry: None,
                    registry_uid: nonempty(&p.registry_uid),
                    values: Some(p.values.clone()),
                    values_file: None,
                    params: p.params.as_object().cloned().unwrap_or_default(),
                    manifests: p
                        .manifests
                        .iter()
                        .map(|m| CandidateManifest {
                            name: m.name.clone(),
                            content: Some(m.content.clone()),
                            content_file: None,
                        })
                        .collect(),
                })
                .collect(),
            variables: snap.variables.clone(),
        })
    }
}

/// The candidate as YAML.
pub fn to_yaml(candidate: &Candidate) -> Result<String> {
    serde_yaml::to_string(candidate).map_err(|e| Error::CandidateInvalid {
        path: PathBuf::from(&candidate.name),
        message: e.to_string(),
    })
}

/// Write `candidate` as YAML to `path`. With `files_dir`, each layer's values go to
/// `values/<pack>.yaml` and each manifest to `manifests/<pack>/<manifest>.yaml` under it, and the
/// candidate references them by paths relative to its own directory, as [`load`] reads them.
pub fn write(candidate: &Candidate, path: &Path, files_dir: Option<&Path>) -> Result<()> {
    let mut c = candidate.clone();
    if let Some(dir) = files_dir {
        let base = match path.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => PathBuf::from("."),
        };
        for layer in &mut c.layers {
            let pack = file_name(&layer.name);
            if let Some(values) = layer.values.take() {
                let file = dir.join("values").join(format!("{pack}.yaml"));
                crate::ops::write_file(&file, &values)?;
                layer.values_file = Some(reference(&base, &file)?);
            }
            for m in &mut layer.manifests {
                if let Some(content) = m.content.take() {
                    let file = dir
                        .join("manifests")
                        .join(&pack)
                        .join(format!("{}.yaml", file_name(&m.name)));
                    crate::ops::write_file(&file, &content)?;
                    m.content_file = Some(reference(&base, &file)?);
                }
            }
        }
    }
    crate::ops::write_file(path, &to_yaml(&c)?)
}

/// A name made safe as one path segment.
fn file_name(name: &str) -> String {
    name.replace(['/', '\\'], "_")
}

/// `to` as a path relative to the directory `from`, with `/` separators.
fn reference(from: &Path, to: &Path) -> Result<String> {
    let absolute = |p: &Path| {
        std::path::absolute(p).map_err(|source| Error::Io {
            path: p.to_path_buf(),
            source,
        })
    };
    let (from, to) = (absolute(from)?, absolute(to)?);
    let common = from
        .components()
        .zip(to.components())
        .take_while(|(a, b)| a == b)
        .count();
    let mut parts: Vec<String> = from
        .components()
        .skip(common)
        .map(|_| "..".to_string())
        .collect();
    parts.extend(
        to.components()
            .skip(common)
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    Ok(parts.join("/"))
}

fn default_type() -> String {
    "cluster".to_string()
}

/// One layer of the candidate: a pack version and the configuration the profile gives it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateLayer {
    /// Pack name.
    pub name: String,
    /// Pack version. For a `manifest` layer, the version that Palette gives its own manifest
    /// pack: `strata candidate` copies it from the record.
    pub version: String,
    /// Layer: os, k8s, cni, csi, addon. Defaults to the registry's. Palette takes the layer
    /// from the registry, so a layer from a registry takes only that layer. A `manifest` layer
    /// takes only `addon`, the layer that Palette gives each manifest pack.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    /// `manifest` for a layer of manifests from no registry: it names no registry, and it needs
    /// one manifest or more. Leave it out for a pack from a registry: the registry gives the type.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The tag to pin: one of the registry's tags for the version, or the version itself. Defaults
    /// to the tag equal to the version, else the first tag of that version. A `manifest` layer
    /// takes any tag, and defaults to the version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// The registry, by name; overrides the top-level registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
    /// The registry, by uid; overrides the top-level registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_uid: Option<String>,
    /// The values YAML. Leave out both this and `valuesFile` for the pack's default values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<String>,
    /// A file with the values YAML, relative to the candidate file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values_file: Option<String>,
    /// Pack params.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub params: Map<String, Value>,
    /// Manifests attached to the pack.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manifests: Vec<CandidateManifest>,
}

/// A manifest attached to a layer: `content` inline or `contentFile`, not both.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateManifest {
    /// Manifest name.
    pub name: String,
    /// The manifest YAML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// A file with the manifest YAML, relative to the candidate file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_file: Option<String>,
}

/// How a layer names its registry.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RegistryRef {
    Uid(String),
    Name(String),
}

impl RegistryRef {
    fn text(&self) -> &str {
        match self {
            RegistryRef::Uid(s) | RegistryRef::Name(s) => s,
        }
    }
}

/// The `type` of a layer of manifests that comes from no registry.
pub const MANIFEST: &str = "manifest";

/// The layer that Palette gives each manifest pack: the layer of its own manifest pack. It drops
/// the layer of the request.
const MANIFEST_LAYER: &str = "addon";

/// The layer of the pack that Palette makes for a layer from a registry that it does not sync.
const UNSYNCED_LAYER: &str = "addon";

/// Whether `v` is a semantic version in the loose form that Palette parses: an optional `v`,
/// one to three numbers with dots between them, an optional pre-release after `-`, and optional
/// build data after `+`. Each identifier of the pre-release and of the build data has letters,
/// digits, and `-` only.
fn loose_semver(v: &str) -> bool {
    let ids = |s: &str| {
        s.split('.')
            .all(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
    };
    let v = v.strip_prefix('v').unwrap_or(v);
    let (v, build) = v.split_once('+').map_or((v, None), |(a, b)| (a, Some(b)));
    let (core, pre) = v.split_once('-').map_or((v, None), |(a, b)| (a, Some(b)));
    let numbers: Vec<&str> = core.split('.').collect();
    numbers.len() <= 3
        && numbers
            .iter()
            .all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        && pre.is_none_or(ids)
        && build.is_none_or(ids)
}

/// A layer with its registry settled and its files read. A `manifest` layer has no registry.
#[derive(Debug, Clone, PartialEq)]
struct LoadedLayer {
    registry: Option<RegistryRef>,
    values: Option<String>,
    manifests: Vec<(String, String)>,
}

/// A candidate read and checked, with every file it points at read.
#[derive(Debug, Clone)]
pub struct LoadedCandidate {
    /// The candidate file, or the text that names a candidate that came from no file. A problem
    /// names it, and the record it gives holds it as the ref.
    pub path: PathBuf,
    /// Its contents.
    pub candidate: Candidate,
    layers: Vec<LoadedLayer>,
}

fn pick_registry(
    uid: &Option<String>,
    name: &Option<String>,
    invalid: impl Fn(String) -> Error,
    whom: &str,
) -> Result<Option<RegistryRef>> {
    match (uid, name) {
        (Some(_), Some(_)) => Err(invalid(format!(
            "{whom} sets both registry and registryUid"
        ))),
        (Some(u), None) => Ok(Some(RegistryRef::Uid(u.clone()))),
        (None, Some(n)) => Ok(Some(RegistryRef::Name(n.clone()))),
        (None, None) => Ok(None),
    }
}

/// Read and check a candidate file, and read the files it points at. Problems here need no
/// network: a file that does not parse or breaks a rule is `CandidateInvalid`, and a values or
/// manifest file that cannot be read is `Io` naming that file.
///
/// A file with `spec.template.packs` is a profile export of Palette. Strata reads it as the
/// candidate that it holds. The page "The candidate file" of the book gives the field of the
/// export that gives each field of the candidate.
pub fn load(path: &Path) -> Result<LoadedCandidate> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let invalid = |message: String| Error::CandidateInvalid {
        path: path.to_path_buf(),
        message,
    };
    let doc: Value = serde_yaml::from_str(&text).map_err(|e| invalid(e.to_string()))?;
    let candidate: Candidate = if export::is_export(&doc) {
        export::to_candidate(&doc).map_err(invalid)?
    } else {
        // parse the text again, and not `doc`: an error of the YAML parser names the line
        serde_yaml::from_str(&text).map_err(|e| invalid(e.to_string()))?
    };
    check(candidate, path)
}

/// Check a candidate that is in memory, and read the files it points at, relative to the
/// directory of `path`. `path` names the candidate: in a problem, and as the ref of the record.
/// [`load`] reads a file and then does this; a caller that builds the candidate itself, as
/// [`Candidate::from_snapshot`] does, starts here and writes no file.
pub fn check(candidate: Candidate, path: &Path) -> Result<LoadedCandidate> {
    let invalid = |message: String| Error::CandidateInvalid {
        path: path.to_path_buf(),
        message,
    };
    if candidate.name.is_empty() || candidate.version.is_empty() {
        return Err(invalid("name and version must not be empty".to_string()));
    }
    if candidate.layers.is_empty() {
        return Err(invalid("a candidate needs at least one layer".to_string()));
    }
    let base = path.parent().unwrap_or(Path::new("."));
    let read = |rel: &str| -> Result<String> {
        let p = base.join(rel);
        std::fs::read_to_string(&p).map_err(|source| Error::Io { path: p, source })
    };
    let top = pick_registry(
        &candidate.registry_uid,
        &candidate.registry,
        invalid,
        "the candidate",
    )?;
    let mut layers = Vec::with_capacity(candidate.layers.len());
    for (i, l) in candidate.layers.iter().enumerate() {
        let whom = format!("layer {} ({})", i + 1, l.name);
        let registry = match l.kind.as_deref() {
            None => Some(
                pick_registry(&l.registry_uid, &l.registry, invalid, &whom)?
                    .or_else(|| top.clone())
                    .ok_or_else(|| {
                        invalid(format!(
                            "{whom} has no registry; set registry or registryUid on the layer or \
                             at the top"
                        ))
                    })?,
            ),
            // Palette makes the pack of a manifest layer from its own manifest pack: no registry
            // holds it, and the top-level registry does not apply to it
            Some(MANIFEST) => {
                if l.registry.is_some() || l.registry_uid.is_some() {
                    return Err(invalid(format!(
                        "{whom} is a manifest layer and names a registry; remove registry and \
                         registryUid from it"
                    )));
                }
                if l.manifests.is_empty() {
                    return Err(invalid(format!(
                        "{whom} is a manifest layer with no manifest; give it one manifest or more"
                    )));
                }
                // Palette takes the layer from its own manifest pack, and drops the layer of the
                // request, so another layer gives a record that the published version never has
                if let Some(layer) = l.layer.as_deref().filter(|&x| x != MANIFEST_LAYER) {
                    return Err(invalid(format!(
                        "{whom} is a manifest layer with layer {layer}; Palette gives each manifest \
                         pack the layer {MANIFEST_LAYER}, so remove layer or set it to \
                         {MANIFEST_LAYER}"
                    )));
                }
                None
            }
            Some(other) => {
                return Err(invalid(format!(
                    "{whom} has type {other}; a layer can give only the type manifest, and the \
                     registry gives the type of each other pack"
                )));
            }
        };
        let values = match (&l.values, &l.values_file) {
            (Some(_), Some(_)) => {
                return Err(invalid(format!("{whom} sets both values and valuesFile")));
            }
            (Some(v), None) => Some(v.clone()),
            (None, Some(f)) => Some(read(f)?),
            (None, None) => None,
        };
        let mut manifests = Vec::with_capacity(l.manifests.len());
        for m in &l.manifests {
            let content = match (&m.content, &m.content_file) {
                (Some(c), None) => c.clone(),
                (None, Some(f)) => read(f)?,
                _ => {
                    return Err(invalid(format!(
                        "manifest {} of {whom} needs exactly one of content and contentFile",
                        m.name
                    )));
                }
            };
            manifests.push((m.name.clone(), content));
        }
        layers.push(LoadedLayer {
            registry,
            values,
            manifests,
        });
    }
    gaps::check_file(&candidate).map_err(invalid)?;
    Ok(LoadedCandidate {
        path: path.to_path_buf(),
        candidate,
        layers,
    })
}

/// Read `/v1/packs?filters=spec.name=<name>` and give each entry to `each`. Palette serves this
/// listing 100 entries at a time, so one pack name can take several requests. `each` returns false
/// to stop the walk.
fn each_pack_named<A: PaletteApi>(
    palette: &Palette<A>,
    name: &str,
    mut each: impl FnMut(Value) -> bool,
) -> Result<()> {
    let mut offset = 0;
    loop {
        let page = palette.api().get_json(&format!(
            "/v1/packs?filters=spec.name={name}&limit=100&offset={offset}"
        ))?;
        let items = page
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let n = items.len();
        for it in items {
            if !each(it) {
                return Ok(());
            }
        }
        offset += n;
        let total = page
            .pointer("/listmeta/count")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        if n < 100 || offset >= total {
            return Ok(());
        }
    }
}

/// Every version of `name` the pack listing returns, across registries. The listing caps `limit`
/// at 100, so this pages until it has them all.
pub(crate) fn versions_of<A: PaletteApi>(palette: &Palette<A>, name: &str) -> Result<Vec<Value>> {
    let mut out: Vec<Value> = Vec::new();
    each_pack_named(palette, name, |it| {
        if text(&it, "/spec/name").as_deref() == Some(name) {
            out.push(it);
        }
        true
    })?;
    Ok(out)
}

/// The provider type of each OCI registry, `pack`, `helm`, or `zarf`, by uid, from
/// `GET /v1/registries/oci/summary`. The pack listing does not carry it. Empty when Palette does
/// not answer: a record then names no provider type, and is otherwise the same.
fn oci_provider_types<A: PaletteApi>(palette: &Palette<A>) -> BTreeMap<String, String> {
    let Ok(listing) = palette.api().get_json("/v1/registries/oci/summary") else {
        return BTreeMap::new();
    };
    listing
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|it| Some((text(it, "/metadata/uid")?, text(it, "/spec/providerType")?)))
        .collect()
}

/// The entry of the registry `uid` in the registry listing at `path`, where `ptr` points to the
/// uid of each entry. None when the listing does not have the registry, and when Palette answers
/// the listing with a 4xx.
fn registry_entry<A: PaletteApi>(
    palette: &Palette<A>,
    path: &str,
    ptr: &str,
    uid: &str,
) -> Result<Option<Value>> {
    let listing = match palette.api().get_json(path) {
        Ok(l) => l,
        Err(Error::Api { status, .. }) if (400..500).contains(&status) => return Ok(None),
        Err(e) => return Err(e),
    };
    Ok(listing
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|it| text(it, ptr).as_deref() == Some(uid))
        .cloned())
}

/// The state that Palette gives to the recovery of a sync when no sync waits for a recovery.
const SYNC_RECOVERED: &str = "Recovered";

/// The last sync of the registry `uid`, which holds a pack of type `pack_type`, from
/// `status.sync` of the registry in the summary listing of its kind: `spectro` reads
/// `GET /v1/registries/pack/summary`, `helm` the same route under `helm`, and `oci` the same
/// route under `oci`. A summary listing gives the sync as Palette stores it. None for a registry
/// that does not sync, for another pack type, for a registry that the listing does not have, and
/// when Palette answers the listing with a 4xx.
pub(crate) fn registry_sync<A: PaletteApi>(
    palette: &Palette<A>,
    uid: &str,
    pack_type: &str,
) -> Result<Option<crate::diff::RegistrySync>> {
    let kind = match pack_type {
        "spectro" => "pack",
        "helm" | "oci" => pack_type,
        _ => return Ok(None),
    };
    let path = format!("/v1/registries/{kind}/summary");
    let Some(entry) = registry_entry(palette, &path, "/metadata/uid", uid)? else {
        return Ok(None);
    };
    let Some(status) = entry.pointer("/status/sync") else {
        return Ok(None);
    };
    if status.get("isSyncSupported").and_then(Value::as_bool) != Some(true) {
        return Ok(None);
    }
    // Palette writes the zero time for a registry that never completed a sync
    let time = |ptr: &str| text(status, ptr).filter(|t| !t.starts_with("0001-"));
    let sync_recovery = text(status, "/syncRecovery/syncRecoveryState")
        .filter(|s| s != SYNC_RECOVERED)
        .map(|state| crate::diff::SyncRecovery {
            sync_recovery_state: state,
            message: text(status, "/syncRecovery/message").unwrap_or_default(),
        });
    Ok(Some(crate::diff::RegistrySync {
        last_synced_time: time("/lastSyncedTime").unwrap_or_default(),
        last_run_time: time("/lastRunTime").unwrap_or_default(),
        status: text(status, "/status").unwrap_or_default(),
        message: text(status, "/message").unwrap_or_default(),
        sync_recovery,
    }))
}

/// The last sync of a registry, read as a note beside an answer.
#[derive(Debug, Clone, Default)]
pub(crate) struct RegistrySyncNote {
    /// The sync, when Palette gives one: see [`registry_sync`].
    pub sync: Option<crate::diff::RegistrySync>,
    /// Why strata could not read the sync: Palette answered with a 5xx, or the request did not
    /// complete. `sync` is then `None`.
    pub error: Option<String>,
}

/// The last sync of the registry `uid`, which holds a pack of type `pack_type`, as a note beside
/// an answer that stands on its own: the verdict of a pack record, or a pin of `behind`. A 5xx
/// of Palette and a request that does not complete do not stop the answer: the note names the
/// reason, and `progress` gets a line. Each other error is an error of the run.
pub(crate) fn registry_sync_note<A: PaletteApi>(
    palette: &Palette<A>,
    uid: &str,
    pack_type: &str,
    progress: &Progress,
) -> Result<RegistrySyncNote> {
    match registry_sync(palette, uid, pack_type) {
        Ok(sync) => Ok(RegistrySyncNote { sync, error: None }),
        Err(e @ (Error::Api { .. } | Error::Transport { .. })) => {
            let reason = match &e {
                Error::Api { status, body, .. } => format!("HTTP {status}: {body}"),
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

/// The type of the pack that Palette makes for a layer from a registry that it does not sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnsyncedKind {
    /// A pack from a private Helm registry.
    Helm,
    /// A pack from an OCI registry.
    Oci,
}

impl UnsyncedKind {
    /// The pack type as Palette writes it: `helm` or `oci`.
    fn as_str(self) -> &'static str {
        match self {
            UnsyncedKind::Helm => "helm",
            UnsyncedKind::Oci => "oci",
        }
    }
}

/// How Palette makes the pack of a layer from a registry that it does not sync.
#[derive(Debug, Clone)]
struct Unsynced {
    /// The pack type.
    kind: UnsyncedKind,
    /// The provider type of an OCI registry: `helm` or `zarf`.
    provider: Option<String>,
}

/// Whether Palette syncs the registry `uid`. It does not sync a Helm registry that is private,
/// or an OCI registry of the provider type `helm` or `zarf` with sync off, so the pack listing
/// has no pack of it. Palette makes the pack of such a layer when it creates the profile.
/// `GET /v1/registries/metadata` gives the kind and `isPrivate`, and
/// `GET /v1/registries/oci/summary` gives the provider type and `status.sync.isSyncSupported`.
/// None for a registry that syncs, and when Palette answers either listing with a 4xx.
fn unsynced<A: PaletteApi>(palette: &Palette<A>, uid: &str) -> Result<Option<Unsynced>> {
    let Some(meta) = registry_entry(palette, "/v1/registries/metadata", "/uid", uid)? else {
        return Ok(None);
    };
    match text(&meta, "/kind").as_deref() {
        Some("helm") => Ok(meta
            .get("isPrivate")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            .then_some(Unsynced {
                kind: UnsyncedKind::Helm,
                provider: None,
            })),
        Some("oci") => {
            let Some(reg) =
                registry_entry(palette, "/v1/registries/oci/summary", "/metadata/uid", uid)?
            else {
                return Ok(None);
            };
            // Palette syncs each OCI registry of the provider type pack
            let provider = text(&reg, "/spec/providerType").unwrap_or_default();
            let syncs = provider == "pack"
                || reg
                    .pointer("/status/sync/isSyncSupported")
                    .and_then(Value::as_bool)
                    == Some(true);
            Ok((!syncs).then_some(Unsynced {
                kind: UnsyncedKind::Oci,
                provider: Some(provider).filter(|p| !p.is_empty()),
            }))
        }
        _ => Ok(None),
    }
}

/// The registries this key can see, name and uid, in the order Palette lists them.
pub(crate) fn registries<A: PaletteApi>(palette: &Palette<A>) -> Result<Vec<(String, String)>> {
    let listing = palette.api().get_json("/v1/registries/metadata")?;
    Ok(listing
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|it| Some((text(it, "/name")?, text(it, "/uid")?)))
        .collect())
}

/// Record one pack version as a snapshot of a single layer, with the values the registry ships.
/// Its pack hashes the same way as that pack inside a profile, so comparing two of these says
/// what changed between two versions of a pack: the version and uid in `IDENTITY`, the content
/// digest in `UPSTREAM`, and the default values in `AUTHORED`.
///
/// Without `registry`, strata uses the registry that has the pack version, and refuses when
/// several do. `version` may be [`LATEST`].
#[allow(clippy::too_many_arguments)]
pub fn pack_snapshot<A: PaletteApi>(
    palette: &Palette<A>,
    registry: Option<&str>,
    name: &str,
    version: &str,
    reference: &str,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot> {
    let visible = registries(palette)?;
    let entries = versions_of(palette, name)?;
    let not_found = |registry: String| Error::PackNotFound {
        name: name.to_string(),
        version: version.to_string(),
        tag: None,
        registry,
    };
    let of = |e: &Value, ptr: &str| text(e, ptr).unwrap_or_default();

    let registry_uid = match registry {
        Some(r) => visible
            .iter()
            .find(|(n, u)| n == r || u == r)
            .map(|(_, u)| u.clone())
            .ok_or_else(|| Error::RegistryNotFound {
                registry: r.to_string(),
                available: visible.iter().map(|(n, _)| n.clone()).collect(),
            })?,
        None => {
            // the registries that have it, in the order Palette lists them
            let holders: Vec<&(String, String)> = visible
                .iter()
                .filter(|(_, uid)| {
                    entries.iter().any(|e| {
                        of(e, "/spec/registryUid") == *uid
                            && (version == LATEST || of(e, "/spec/version") == version)
                    })
                })
                .collect();
            match holders.as_slice() {
                [(_, uid)] => uid.clone(),
                [] => return Err(not_found("any visible registry".to_string())),
                several => {
                    // by name, so the message reads the same on every run: Palette lists the
                    // registries in the order it likes, and a live run saw both orders
                    let mut registries: Vec<String> =
                        several.iter().map(|(n, _)| n.clone()).collect();
                    registries.sort();
                    return Err(Error::PackInSeveralRegistries {
                        name: name.to_string(),
                        version: version.to_string(),
                        registries,
                    });
                }
            }
        }
    };
    let registry_name = visible
        .iter()
        .find(|(_, u)| *u == registry_uid)
        .map(|(n, _)| n.clone())
        .unwrap_or_default();
    let named = |uid: &str| -> String {
        if registry_name.is_empty() {
            uid.to_string()
        } else {
            registry_name.clone()
        }
    };

    let here: Vec<String> = entries
        .iter()
        .filter(|e| of(e, "/spec/registryUid") == registry_uid)
        .map(|e| of(e, "/spec/version"))
        .collect();
    let resolved = if version == LATEST {
        here.iter()
            .max_by(|a, b| crate::version::compare(a, b))
            .cloned()
            .ok_or_else(|| not_found(named(&registry_uid)))?
    } else if here.iter().any(|v| v == version) {
        version.to_string()
    } else {
        return Err(not_found(named(&registry_uid)));
    };
    progress.note(&format!(
        "recording pack {name}@{resolved} from registry {}",
        named(&registry_uid)
    ));
    // one layer, the registry's own values: the same path a candidate file takes
    let candidate = Candidate {
        name: name.to_string(),
        version: resolved.clone(),
        kind: PACK_TYPE.to_string(),
        cloud_type: String::new(),
        registry: None,
        registry_uid: Some(registry_uid.clone()),
        layers: vec![CandidateLayer {
            name: name.to_string(),
            version: resolved.clone(),
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
    let loaded = LoadedCandidate {
        path: PathBuf::from(reference),
        candidate,
        layers: vec![LoadedLayer {
            registry: Some(RegistryRef::Uid(registry_uid.clone())),
            values: None,
            manifests: Vec::new(),
        }],
    };
    let raw = resolve(&loaded, palette, progress)?;
    let mut snap = Snapshot::from_raw(&raw, reference, fetched_at)?;
    snap.source.kind = PACK_TYPE.to_string();
    snap.source.template = PACK_TEMPLATE.to_string();
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

fn text(v: &Value, ptr: &str) -> Option<String> {
    v.pointer(ptr).and_then(Value::as_str).map(str::to_string)
}

/// The options of the pack template that `values` select, sorted by parameter. `summary` is the
/// entry of the pack version in the listing, which gives the template in
/// `spec.template.parameters.inputParameters`. On a create and an update of a profile, Palette
/// reads the value at the `targetKey` of each input parameter as text, and when the text names
/// one of its `options`, writes the presets of that option into the values. The listing gives
/// each option with no presets, so strata records the selection and does not change the values.
pub(crate) fn preset_options(summary: &Value, values: &str) -> Vec<PresetOption> {
    let params = summary
        .pointer("/spec/template/parameters/inputParameters")
        .and_then(Value::as_array);
    let Some(params) = params.filter(|p| !p.is_empty()) else {
        return Vec::new();
    };
    let Canon::Parsed(parsed) = canon_yaml(values) else {
        return Vec::new();
    };
    let mut flat = BTreeMap::new();
    if let Value::Object(top) = &parsed {
        flatten(top, "", &mut flat);
    }
    // by name, as Palette keys the parameters: a later parameter of one name replaces an earlier
    let mut selected: BTreeMap<String, PresetOption> = BTreeMap::new();
    for p in params {
        let (Some(name), Some(key), Some(options)) = (
            text(p, "/name"),
            text(p, "/targetKey").filter(|k| !k.is_empty()),
            p.get("options").and_then(Value::as_object),
        ) else {
            continue;
        };
        let at = flat
            .get(key.as_str())
            .copied()
            .map(as_text)
            .unwrap_or_default();
        if options.contains_key(&at) {
            selected.insert(
                name.clone(),
                PresetOption {
                    parameter: name,
                    target_key: key,
                    option: at,
                },
            );
        }
    }
    selected.into_values().collect()
}

/// Each key of a values map as Palette spells it, a dotted path, with its value. A list is a key
/// of its own, and each of its items is a key with the index. A map is not a key: its keys are.
fn flatten<'v>(map: &'v Map<String, Value>, prefix: &str, out: &mut BTreeMap<String, &'v Value>) {
    for (k, v) in map {
        let key = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}.{k}")
        };
        match v {
            Value::Object(inner) => flatten(inner, &key, out),
            Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    let at = format!("{key}.{i}");
                    match item {
                        Value::Object(inner) => flatten(inner, &at, out),
                        _ => {
                            out.insert(at, item);
                        }
                    }
                }
                out.insert(key, v);
            }
            _ => {
                out.insert(key, v);
            }
        }
    }
}

/// A value as Palette reads it for an option: a string as it is, a number or a boolean as its
/// text, and nothing for null, a list, or a map.
fn as_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => match (n.as_i64(), n.as_u64(), n.as_f64()) {
            (Some(i), _, _) => i.to_string(),
            (_, Some(u), _) => u.to_string(),
            (_, _, Some(f)) => f.to_string(),
            _ => n.to_string(),
        },
        _ => String::new(),
    }
}

/// Warn once for each option of a pack template that the values of a pack of `snap` select:
/// Palette can write the presets of that option into the values when it publishes the version,
/// and the root of `snap` does not have them.
pub fn warn_preset_options(snap: &Snapshot, progress: &Progress) {
    for p in &snap.packs {
        for o in &p.preset_options {
            progress.warn(&format!(
                "{}: the values select the option {} of {} ({}). Palette can write the presets of \
                 that option into the values, and no route of Palette shows them. So the root \
                 can differ from the version that Palette publishes",
                p.name, o.option, o.parameter, o.target_key
            ));
        }
    }
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
    system_state(summary).or_else(|| {
        (text(summary, DISABLED_MARK).as_deref() == Some("true")).then(|| "disabled".to_string())
    })
}

/// The state that Spectro Cloud gives a version that it retired: `deprecated`, `disabled`, or
/// `deleted`.
fn system_state(summary: &Value) -> Option<String> {
    text(summary, "/spec/annotations/system_state").filter(|s| !s.is_empty())
}

/// The state of a pack version that Palette no longer serves: `disabled` or `deleted` in
/// `system_state`. Palette serves a version with the [`DISABLED_MARK`] alone, and its validate
/// route refuses a profile that holds it. So strata does not refuse that version itself.
pub(crate) fn retired_state(summary: &Value) -> Option<String> {
    system_state(summary).filter(|s| s == "disabled" || s == "deleted")
}

/// The registry's summary of `name` at `version` in `registry`, paging through the listing.
fn find_version<A: PaletteApi>(
    palette: &Palette<A>,
    name: &str,
    version: &str,
    registry: &str,
) -> Result<Option<Value>> {
    let mut found = None;
    each_pack_named(palette, name, |it| {
        let hit = text(&it, "/spec/name").as_deref() == Some(name)
            && text(&it, "/spec/registryUid").as_deref() == Some(registry)
            && text(&it, "/spec/version").as_deref() == Some(version);
        if hit {
            found = Some(it);
        }
        !hit
    })?;
    Ok(found)
}

/// The fields of one pack that a published profile gives in two documents: the pack reference
/// of the profile and the entry of the manifests document.
struct PackFields<'a> {
    layer: &'a str,
    kind: &'a str,
    tag: &'a str,
    version: &'a str,
    pack_uid: &'a str,
    registry_uid: &'a str,
    digest: &'a str,
    values: &'a str,
}

/// One layer as a published profile gives it.
struct ResolvedLayer {
    /// The pack reference of the profile.
    pack_ref: Value,
    /// The entry of the manifests document.
    entry: Value,
}

/// The pack reference and the manifests entry of the layer `l` with the fields `f`. The pack
/// reference has the digest only when it is not empty. The entry holds `manifests`.
fn pack_documents(l: &CandidateLayer, f: &PackFields, manifests: Vec<Value>) -> (Value, Value) {
    let mut pack_ref = json!({
        "name": l.name, "layer": f.layer, "type": f.kind, "tag": f.tag, "version": f.version,
        "packUid": f.pack_uid, "registryUid": f.registry_uid, "values": f.values,
        "manifests": [], "params": l.params, "presets": [],
    });
    if !f.digest.is_empty() {
        pack_ref["digest"] = json!(f.digest);
    }
    let entry = json!({
        "metadata": {"name": l.name, "uid": f.pack_uid},
        "spec": {
            "name": l.name, "layer": f.layer, "version": f.version, "digest": f.digest,
            "registryUid": f.registry_uid, "values": f.values, "manifests": manifests,
        },
        "status": {},
    });
    (pack_ref, entry)
}

/// The state of one resolve: the candidate, the client, and the answers of Palette that strata
/// reads one time and uses again for each layer.
struct Resolver<'r, A: PaletteApi> {
    loaded: &'r LoadedCandidate,
    palette: &'r Palette<A>,
    progress: &'r Progress,
    /// Registry names to uids, in the order Palette lists them. Strata reads them one time, and
    /// only when a layer names a registry.
    names: Option<Vec<(String, String)>>,
    /// The provider type of each OCI registry. Strata reads them one time, and only for an OCI
    /// pack.
    oci_types: Option<BTreeMap<String, String>>,
    /// Whether Palette syncs each registry. Strata reads it only for a layer that the listing
    /// does not have.
    unsynced_of: BTreeMap<String, Option<Unsynced>>,
}

impl<A: PaletteApi> Resolver<'_, A> {
    /// A `CandidateInvalid` error for the candidate file with `message`.
    fn invalid(&self, message: String) -> Error {
        Error::CandidateInvalid {
            path: self.loaded.path.clone(),
            message,
        }
    }

    /// The uid of the registry that `registry_ref` names.
    fn registry_uid(&mut self, registry_ref: &RegistryRef) -> Result<String> {
        match registry_ref {
            RegistryRef::Uid(u) => Ok(u.clone()),
            RegistryRef::Name(n) => {
                if self.names.is_none() {
                    self.names = Some(registries(self.palette)?);
                }
                let visible = self.names.as_deref().unwrap_or_default();
                visible
                    .iter()
                    .find(|(name, _)| name == n)
                    .map(|(_, uid)| uid.clone())
                    .ok_or_else(|| Error::RegistryNotFound {
                        registry: n.clone(),
                        available: visible.iter().map(|(name, _)| name.clone()).collect(),
                    })
            }
        }
    }

    /// Resolve the layer `i` of the candidate.
    fn layer(&mut self, i: usize) -> Result<ResolvedLayer> {
        let l = &self.loaded.candidate.layers[i];
        let loaded_layer = &self.loaded.layers[i];
        let manifests: Vec<Value> = loaded_layer
            .manifests
            .iter()
            .map(|(name, content)| json!({"name": name, "uid": "", "digest": "", "content": content}))
            .collect();
        let Some(registry_ref) = &loaded_layer.registry else {
            return Ok(manifest_layer(l, loaded_layer, manifests));
        };
        let registry = self.registry_uid(registry_ref)?;
        match find_version(self.palette, &l.name, &l.version, &registry)? {
            Some(summary) => self.registry_layer(i, registry_ref, &registry, &summary, manifests),
            None => self.unsynced_layer(i, registry_ref, &registry, manifests),
        }
    }

    /// Resolve the layer `i`, which the listing of the registry does not have. Palette lists no
    /// pack of a registry that it does not sync, and makes the pack at the create. A registry
    /// that syncs and lists no such version does not have it.
    fn unsynced_layer(
        &mut self,
        i: usize,
        registry_ref: &RegistryRef,
        registry: &str,
        manifests: Vec<Value>,
    ) -> Result<ResolvedLayer> {
        let l = &self.loaded.candidate.layers[i];
        let loaded_layer = &self.loaded.layers[i];
        let how = match self.unsynced_of.entry(registry.to_string()) {
            std::collections::btree_map::Entry::Occupied(e) => e.get().clone(),
            std::collections::btree_map::Entry::Vacant(e) => {
                e.insert(unsynced(self.palette, registry)?).clone()
            }
        };
        let Some(how) = how else {
            return Err(Error::PackNotFound {
                name: l.name.clone(),
                version: l.version.clone(),
                tag: None,
                registry: registry_ref.text().to_string(),
            });
        };
        let not_synced = |message: String| {
            self.invalid(format!(
                "layer {} ({}) is from registry {}, which Palette does not sync; {message}",
                i + 1,
                l.name,
                registry_ref.text()
            ))
        };
        // the pack that Palette makes has the layer addon and the tag as its version, and a
        // profile takes both from it
        if let Some(layer) = l.layer.as_deref().filter(|&x| x != UNSYNCED_LAYER) {
            return Err(not_synced(format!(
                "Palette gives the pack that it makes the layer {UNSYNCED_LAYER}, not \
                 {layer}, so remove layer or set it to {UNSYNCED_LAYER}"
            )));
        }
        let tag = l.tag.as_deref().unwrap_or(&l.version);
        if tag != l.version {
            return Err(not_synced(format!(
                "Palette gives the pack that it makes the tag as its version, so set version \
                 to {tag}, or remove tag"
            )));
        }
        gaps::check_pack_tag(tag).map_err(not_synced)?;
        self.progress.note(&format!(
            "{}@{}: registry {} does not sync, so Palette makes the pack when it creates the \
             profile; the record has no pack uid and no digest",
            l.name,
            l.version,
            registry_ref.text()
        ));
        let values = loaded_layer.values.clone().unwrap_or_default();
        let fields = PackFields {
            layer: UNSYNCED_LAYER,
            kind: how.kind.as_str(),
            tag,
            version: tag,
            pack_uid: "",
            registry_uid: registry,
            digest: "",
            values: &values,
        };
        let (mut pack_ref, entry) = pack_documents(l, &fields, manifests);
        if let Some(t) = &how.provider {
            pack_ref["annotations"] = json!({ "ociRegistryProviderType": t });
        }
        // the pack that Palette makes has no cloud type, so each cloud takes it
        Ok(ResolvedLayer { pack_ref, entry })
    }

    /// Resolve the layer `i` from `summary`, the entry of its version in the listing of the
    /// registry, and from the tags and the values that the registry gives for the pack.
    fn registry_layer(
        &mut self,
        i: usize,
        registry_ref: &RegistryRef,
        registry: &str,
        summary: &Value,
        manifests: Vec<Value>,
    ) -> Result<ResolvedLayer> {
        let c = &self.loaded.candidate;
        let l = &c.layers[i];
        let loaded_layer = &self.loaded.layers[i];
        // Palette serves no disabled or deleted pack: the registry answers 404 for it, and a
        // profile that pins one is refused. Say so before that read.
        if let Some(state) = retired_state(summary) {
            // a `pack:` ref comes here with the uid of the registry; the error gives its name
            let registry = match registry_ref {
                RegistryRef::Name(n) => n.clone(),
                RegistryRef::Uid(u) => registries(self.palette)?
                    .into_iter()
                    .find(|(_, uid)| uid == u)
                    .map_or_else(|| u.clone(), |(name, _)| name),
            };
            return Err(Error::PackUnavailable {
                name: l.name.clone(),
                version: l.version.clone(),
                registry,
                state,
            });
        }
        // only the create checks that the pack supports the cloud of the profile
        let clouds: Vec<String> = summary
            .pointer("/spec/cloudTypes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        gaps::check_cloud(&c.cloud_type, i, l, &clouds).map_err(|m| self.invalid(m))?;
        let pack_uid = text(summary, "/metadata/uid").unwrap_or_default();
        let entity = self
            .palette
            .api()
            .get_json(&format!("/v1/packs/{}/registries/{registry}", l.name))?;
        let tags: Vec<String> = entity
            .get("tags")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|t| text(t, "/packUid").as_deref() == Some(pack_uid.as_str()))
            .filter_map(|t| text(t, "/tag"))
            .collect();
        // Palette stores the tag a profile was authored with, which can be the version itself
        // rather than one of the registry's tag names (a Terraform-built profile pins `24.04`
        // where the registry says `LTS__24.x`). Accept both; anything else is a typo.
        let tag = match &l.tag {
            Some(t) if tags.contains(t) || *t == l.version => t.clone(),
            Some(t) => {
                return Err(Error::PackNotFound {
                    name: l.name.clone(),
                    version: l.version.clone(),
                    tag: Some(t.clone()),
                    registry: registry_ref.text().to_string(),
                });
            }
            None => tags
                .iter()
                .find(|t| **t == l.version)
                .or_else(|| tags.first())
                .cloned()
                .unwrap_or_else(|| l.version.clone()),
        };
        let pack_values = entity
            .get("packValues")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|v| text(v, "/packUid").as_deref() == Some(pack_uid.as_str()))
            .and_then(|v| text(v, "/values"));
        let values = loaded_layer
            .values
            .clone()
            .or_else(|| text(summary, "/spec/values"))
            .or(pack_values)
            .unwrap_or_default();
        // Palette gives the pack the layer of the registry and drops the layer of the request with
        // no error. So another layer in the file gives a root that the published version does not
        // have.
        let registry_layer = text(summary, "/spec/layer").or_else(|| text(&entity, "/layer"));
        if let (Some(given), Some(theirs)) = (l.layer.as_deref(), registry_layer.as_deref())
            && given != theirs
        {
            return Err(self.invalid(format!(
                "layer {} ({}) has the layer {given}, and its registry gives the layer {theirs}; \
                 Palette takes the layer from the registry, so remove layer or set it to {theirs}",
                i + 1,
                l.name
            )));
        }
        let layer = registry_layer
            .or_else(|| l.layer.clone())
            .unwrap_or_default();
        let kind = text(summary, "/spec/type").unwrap_or_else(|| "spectro".to_string());
        let digest = text(summary, "/spec/digest").unwrap_or_default();
        let fields = PackFields {
            layer: &layer,
            kind: &kind,
            tag: &tag,
            version: &l.version,
            pack_uid: &pack_uid,
            registry_uid: registry,
            digest: &digest,
            values: &values,
        };
        let (mut pack_ref, entry) = pack_documents(l, &fields, manifests);
        let selected = preset_options(summary, &values);
        if !selected.is_empty() {
            pack_ref["presetOptions"] = json!(selected);
        }
        // the provider type of an OCI registry, as a published profile carries it on the pack
        // reference
        if kind == "oci" {
            let palette = self.palette;
            let types = self
                .oci_types
                .get_or_insert_with(|| oci_provider_types(palette));
            if let Some(t) = types.get(registry) {
                pack_ref["annotations"] = json!({ "ociRegistryProviderType": t });
            }
        }
        Ok(ResolvedLayer { pack_ref, entry })
    }
}

/// Resolve a manifest layer. No registry holds it, so it needs no request. Palette mints its
/// pack uid at the create, so the candidate has none, and it has no digest.
fn manifest_layer(
    l: &CandidateLayer,
    loaded_layer: &LoadedLayer,
    manifests: Vec<Value>,
) -> ResolvedLayer {
    let values = loaded_layer.values.clone().unwrap_or_default();
    let fields = PackFields {
        layer: l.layer.as_deref().unwrap_or(MANIFEST_LAYER),
        kind: MANIFEST,
        tag: l.tag.as_deref().unwrap_or(&l.version),
        version: &l.version,
        pack_uid: "",
        registry_uid: "",
        digest: "",
        values: &values,
    };
    let (pack_ref, entry) = pack_documents(l, &fields, manifests);
    ResolvedLayer { pack_ref, entry }
}

/// Look each layer's pack up in its registry and build the three documents a published profile
/// with this content returns: `GET /v1/registries/metadata` for registry names,
/// `GET /v1/packs?filters=spec.name=<name>` for the pack uid, digest, layer, type, and default
/// values, and `GET /v1/packs/<name>/registries/<uid>` for the tags. A `manifest` layer needs
/// none of these. When the listing does not have the version, and the registry is one that
/// Palette does not sync, the pack is the one that Palette makes at the create: no pack uid, no
/// digest, the layer `addon`, and the tag as the version.
pub fn resolve<A: PaletteApi>(
    loaded: &LoadedCandidate,
    palette: &Palette<A>,
    progress: &Progress,
) -> Result<RawProfile> {
    let c = &loaded.candidate;
    progress.note(&format!(
        "resolving {}@{} against the registry",
        c.name, c.version
    ));
    let mut resolver = Resolver {
        loaded,
        palette,
        progress,
        names: None,
        oci_types: None,
        unsynced_of: BTreeMap::new(),
    };
    let mut refs = Vec::with_capacity(c.layers.len());
    let mut entries = Vec::with_capacity(c.layers.len());
    for i in 0..c.layers.len().min(loaded.layers.len()) {
        let resolved = resolver.layer(i)?;
        refs.push(resolved.pack_ref);
        entries.push(resolved.entry);
    }
    // each pack reference has the layer that Palette gives the pack, so check the layers again
    let known: Vec<Option<&str>> = refs
        .iter()
        .map(|r| r.get("layer").and_then(Value::as_str))
        .collect();
    gaps::check_layers(c, &known).map_err(|message| resolver.invalid(message))?;
    Ok(RawProfile {
        profile: json!({
            "metadata": {"name": c.name, "uid": ""},
            "spec": {
                "version": c.version,
                "published": {"cloudType": c.cloud_type, "type": c.kind, "packs": refs},
            },
            "status": {},
        }),
        manifests: json!({"metadata": {"name": c.name, "uid": ""}, "spec": {"packs": entries}}),
        variables: json!({ "variables": c.variables }),
        variables_unavailable: false,
        fetched_at: None,
        project_uid: None,
        manifest_entities: None,
        resolved_values: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::MemoryApi;

    /// A summary listing of registries that holds the one registry `uid` with the sync `sync`.
    fn summary(uid: &str, sync: Value) -> Value {
        json!({"items": [{"metadata": {"uid": uid}, "status": {"sync": sync}}]})
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
            .with("/v1/registries/oci/summary", summary("reg-oci", completed))
            // the route of one registry is not read
            .failing("/v1/registries/oci/reg-oci/basic/sync/status", 500, "{}");
        let palette = Palette::new(api);
        for (uid, pack_type) in [
            ("reg-pack", "spectro"),
            ("reg-helm", "helm"),
            ("reg-oci", "oci"),
        ] {
            let sync = registry_sync(&palette, uid, pack_type).unwrap().unwrap();
            assert_eq!(sync.last_synced_time, "2026-09-30T00:00:11.000Z");
            // a recovery that is Recovered is no recovery that waits
            assert_eq!(sync.sync_recovery, None);
        }
        // a manifest pack has no registry; the listing of the kind does not have the registry
        assert!(
            registry_sync(&palette, "reg-oci", "manifest")
                .unwrap()
                .is_none()
        );
        assert!(
            registry_sync(&palette, "reg-pack", "oci")
                .unwrap()
                .is_none()
        );
        // Palette answers no listing
        let none = Palette::new(MemoryApi::new());
        assert!(registry_sync(&none, "reg-oci", "oci").unwrap().is_none());
    }

    /// The profile `platform-addons` as the export of Palette writes it: a pack from the Public
    /// Repo, and a manifest pack with an empty registry and no values.
    const EXPORT: &str = r#"metadata:
  name: platform-addons
  description: ""
  labels: {}
spec:
  version: 1.0.0
  template:
    type: add-on
    cloudType: all
    packs:
    - name: cert-manager
      type: spectro
      layer: addon
      version: 1.21.1
      tag: 1.21.1
      values: |-
        pack:
          namespace: cert-manager
      registry:
        metadata:
          uid: 5eecc89d0b150045ae661cef
          name: Public Repo
          kind: pack
          isPrivate: false
          providerType: ""
          isSyncSupported: true
    - name: namespace-policy
      type: manifest
      layer: addon
      version: 1.0.0
      tag: 1.0.0
      registry:
        metadata:
          uid: ""
          name: ""
          kind: ""
          isPrivate: false
          providerType: ""
          isSyncSupported: false
      manifests:
      - name: namespace-policy
        content: |-
          kind: Namespace
  variables:
  - name: owner
    displayName: Owner
    format: string
    required: true
"#;

    /// The candidate file of the same content as [`EXPORT`].
    const SAME_CANDIDATE: &str = r#"name: platform-addons
version: 1.0.0
type: add-on
cloudType: all
layers:
- name: cert-manager
  version: 1.21.1
  layer: addon
  tag: 1.21.1
  registryUid: 5eecc89d0b150045ae661cef
  values: |-
    pack:
      namespace: cert-manager
- name: namespace-policy
  version: 1.0.0
  layer: addon
  type: manifest
  tag: 1.0.0
  values: ''
  manifests:
  - name: namespace-policy
    content: |-
      kind: Namespace
variables:
- name: owner
  displayName: Owner
  format: string
  required: true
"#;

    #[test]
    fn an_export_of_palette_with_a_manifest_pack_loads_as_the_candidate_file_of_the_same_content() {
        let dir = tempfile::tempdir().unwrap();
        let (export, file) = (dir.path().join("export.yaml"), dir.path().join("c.yaml"));
        std::fs::write(&export, EXPORT).unwrap();
        std::fs::write(&file, SAME_CANDIDATE).unwrap();
        let (from_export, from_file) = (load(&export).unwrap(), load(&file).unwrap());
        assert_eq!(from_export.candidate, from_file.candidate);
        assert_eq!(from_export.layers, from_file.layers);
        assert_eq!(
            from_export.layers[1].registry, None,
            "a manifest pack has no registry"
        );
        // the same file as JSON, as the export gives it with format=json
        let doc: Value = serde_yaml::from_str(EXPORT).unwrap();
        let json_export = dir.path().join("export.json");
        std::fs::write(&json_export, serde_json::to_string(&doc).unwrap()).unwrap();
        assert_eq!(load(&json_export).unwrap().candidate, from_file.candidate);
    }

    #[test]
    fn an_export_whose_pack_has_no_registry_is_candidate_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let export = dir.path().join("export.yaml");
        std::fs::write(
            &export,
            EXPORT
                .replace("uid: 5eecc89d0b150045ae661cef", "uid: \"\"")
                .replace("name: Public Repo", "name: \"\""),
        )
        .unwrap();
        match load(&export).unwrap_err() {
            Error::CandidateInvalid { message, .. } => assert_eq!(
                message,
                "layer 1 (cert-manager) has no registry; set registry or registryUid on the layer \
                 or at the top"
            ),
            other => panic!("not candidate-invalid: {other}"),
        }
    }

    /// A candidate with one `manifest` layer, as `strata candidate` writes it for a manifest pack.
    fn manifest_candidate() -> Candidate {
        serde_yaml::from_str(
            "name: platform-addons\nversion: 1.0.0\ntype: add-on\ncloudType: all\n\
             layers:\n- name: namespace-policy\n  version: 1.0.0\n  type: manifest\n  \
             manifests:\n  - name: namespace-policy\n    content: 'kind: Namespace'\n",
        )
        .unwrap()
    }

    fn refusal(c: Candidate) -> String {
        match check(c, Path::new("c.yaml")).unwrap_err() {
            Error::CandidateInvalid { message, .. } => message,
            other => panic!("not candidate-invalid: {other}"),
        }
    }

    #[test]
    fn a_manifest_layer_needs_no_registry_and_resolves_with_no_request() {
        // the registry at the top is for the packs from a registry, not for a manifest layer
        let mut c = manifest_candidate();
        c.registry = Some("Public Repo".into());
        let loaded = check(c, Path::new("c.yaml")).unwrap();
        // the API knows no path: each request fails
        let raw = resolve(
            &loaded,
            &Palette::new(MemoryApi::new()),
            &Progress::silent(),
        )
        .unwrap();
        let pack = &raw.profile["spec"]["published"]["packs"][0];
        assert_eq!(pack["type"], "manifest");
        assert_eq!(
            pack["layer"], "addon",
            "the layer of Palette's manifest pack"
        );
        assert_eq!(pack["tag"], "1.0.0", "the tag defaults to the version");
        assert_eq!(pack["packUid"], "", "Palette mints the uid at the create");
        assert_eq!(pack["registryUid"], "");
        assert!(pack.get("digest").is_none());
        let entry = &raw.manifests["spec"]["packs"][0]["spec"];
        assert_eq!(entry["manifests"][0]["content"], "kind: Namespace");
    }

    #[test]
    fn a_manifest_layer_that_names_a_registry_is_candidate_invalid() {
        let mut by_name = manifest_candidate();
        by_name.layers[0].registry = Some("Public Repo".into());
        let mut by_uid = manifest_candidate();
        by_uid.layers[0].registry_uid = Some("r1".into());
        for c in [by_name, by_uid] {
            let message = refusal(c);
            assert!(
                message.contains("layer 1 (namespace-policy)")
                    && message.contains("names a registry"),
                "{message}"
            );
        }
    }

    #[test]
    fn a_manifest_layer_with_no_manifest_or_another_type_is_candidate_invalid() {
        let mut empty = manifest_candidate();
        empty.layers[0].manifests.clear();
        let message = refusal(empty);
        assert!(
            message.contains("layer 1 (namespace-policy)") && message.contains("no manifest"),
            "{message}"
        );
        let mut helm = manifest_candidate();
        helm.layers[0].kind = Some("helm".into());
        let message = refusal(helm);
        assert!(
            message.contains("layer 1 (namespace-policy) has type helm"),
            "{message}"
        );
    }

    #[test]
    fn a_manifest_layer_takes_only_the_layer_that_palette_gives_a_manifest_pack() {
        let mut k8s = manifest_candidate();
        k8s.layers[0].layer = Some("k8s".into());
        let message = refusal(k8s);
        assert!(
            message.contains("layer 1 (namespace-policy) is a manifest layer with layer k8s"),
            "{message}"
        );
        // the layer that Palette gives, written out, is the same as no layer
        let mut addon = manifest_candidate();
        addon.layers[0].layer = Some("addon".into());
        assert!(check(addon, Path::new("c.yaml")).is_ok());
    }

    /// A cluster candidate that Palette takes, with each layer given: the core layers and one
    /// add-on with one manifest.
    fn stack() -> Candidate {
        serde_yaml::from_str(
            "name: aws-stack\nversion: 1.1.1\ncloudType: aws\nregistryUid: r1\nlayers:\n\
             - {name: ubuntu-aws, version: '24.04', layer: os}\n\
             - {name: kubernetes, version: 1.33.6, layer: k8s, values: 'a: 1'}\n\
             - {name: cni-cilium-oss, version: 1.18.4, layer: cni}\n\
             - {name: csi-aws-ebs, version: 1.48.0, layer: csi}\n\
             - name: cert-manager\n  version: 1.19.1\n  layer: addon\n  manifests:\n  \
             - {name: namespace-labels, content: 'kind: Namespace'}\n",
        )
        .unwrap()
    }

    /// The refusal of `c`, with the rule of Palette that it names, and the reason that strata
    /// checks it.
    fn refused(c: Candidate, rule: &str) -> String {
        let message = refusal(c);
        assert!(message.contains(&format!("({rule}). ")), "{message}");
        assert!(message.ends_with(gaps::GAP), "{message}");
        message
    }

    #[test]
    fn the_stack_is_a_candidate_that_palette_takes() {
        assert!(check(stack(), Path::new("c.yaml")).is_ok());
    }

    #[test]
    fn a_version_that_semver_of_palette_does_not_parse_is_candidate_invalid() {
        // Palette takes an optional v and one to three numbers
        for ok in [
            "1.1",
            "v1.2.3",
            "7",
            "1.2.3-rc.1",
            "1.2.3+build.5",
            "1.2.3-rc-1.x+b",
        ] {
            let mut c = stack();
            c.version = ok.into();
            assert!(check(c, Path::new("c.yaml")).is_ok(), "{ok}");
        }
        for bad in [
            "2.2.2.develop",
            "1.2.3.4",
            "chart-v1.2.3",
            "latest",
            "1.2.3-",
            "1.2.3-a..b",
            "1..2",
            "V1.2.3",
            "99999999999999999999.0.0",
        ] {
            let mut c = stack();
            c.version = bad.into();
            let message = refused(c, "VersionParseFailure");
            assert!(message.contains(&format!("version {bad} ")), "{message}");
        }
    }

    #[test]
    fn a_pack_twice_is_candidate_invalid() {
        let mut c = stack();
        let again = c.layers[4].clone();
        c.layers.push(again);
        let message = refused(c, "DuplicatePacksForbidden");
        assert!(message.contains("the pack cert-manager"), "{message}");
    }

    #[test]
    fn a_manifest_name_twice_in_one_pack_is_candidate_invalid() {
        let mut c = stack();
        let again = c.layers[4].manifests[0].clone();
        c.layers[4].manifests.push(again);
        let message = refused(c, "DuplicatePackManifestsForbidden");
        assert!(
            message.contains("layer 5 (cert-manager)") && message.contains("namespace-labels"),
            "{message}"
        );
        // the same name in two packs is not a repeat
        let mut c = stack();
        c.layers[1].manifests = c.layers[4].manifests.clone();
        assert!(check(c, Path::new("c.yaml")).is_ok());
    }

    #[test]
    fn a_name_that_is_not_a_dns_label_is_candidate_invalid() {
        for bad in [
            "Namespace Labels",
            "-labels",
            "labels-",
            "labels_x",
            &"a".repeat(33),
        ] {
            let mut c = stack();
            c.layers[4].manifests[0].name = bad.to_string();
            let message = refused(c, "InvalidName");
            assert!(
                message.contains("a manifest of layer 5 (cert-manager)"),
                "{message}"
            );
        }
        let mut c = stack();
        c.layers[4].manifests[0].name = "a".repeat(32);
        assert!(check(c, Path::new("c.yaml")).is_ok());
        // the name of a manifest pack too, and not the name of a pack from a registry
        let mut c = manifest_candidate();
        c.layers[0].name = "Namespace-Policy".into();
        let message = refused(c, "InvalidName");
        assert!(
            message.contains("layer 1 (Namespace-Policy) is a manifest layer"),
            "{message}"
        );
        let mut c = stack();
        c.layers[4].name = "Cert-Manager".into();
        assert!(check(c, Path::new("c.yaml")).is_ok());
    }

    #[test]
    fn a_layer_that_the_type_of_the_profile_does_not_take_is_candidate_invalid() {
        let mut c = stack();
        c.kind = "add-on".into();
        let message = refused(c, "ClusterProfilePackNotAllowed");
        assert!(
            message.contains("layer 1 (ubuntu-aws) has the layer os")
                && message.contains("type add-on takes only the layers addon, cni, and csi"),
            "{message}"
        );
        // an add-on profile takes a cni and a csi layer
        let mut c = stack();
        c.kind = "add-on".into();
        c.layers.drain(0..2);
        assert!(check(c, Path::new("c.yaml")).is_ok());
        let mut c = stack();
        c.kind = "infra".into();
        let message = refused(c, "ClusterProfilePackNotAllowed");
        assert!(
            message.contains("layer 5 (cert-manager) has the layer addon"),
            "{message}"
        );
        // a manifest layer has the layer addon
        let mut c = manifest_candidate();
        c.kind = "infra".into();
        refused(c, "ClusterProfilePackNotAllowed");
    }

    #[test]
    fn a_cluster_or_infra_profile_with_no_core_layer_is_candidate_invalid() {
        let mut c = stack();
        c.layers.remove(0);
        let message = refused(c, "ErrPackCoreLayersMissing");
        assert!(
            message.contains("type cluster needs the layers os, k8s, cni, and csi")
                && message.contains("has no layer os"),
            "{message}"
        );
        let mut c = stack();
        c.kind = "infra".into();
        c.layers.truncate(2);
        let message = refused(c, "ErrPackCoreLayersMissing");
        assert!(message.contains("has no layer cni and csi"), "{message}");
        // edge-native needs no csi
        let mut c = stack();
        c.cloud_type = "edge-native".into();
        c.layers.remove(3);
        assert!(check(c, Path::new("c.yaml")).is_ok());
        // a layer that the file does not give comes from the registry: resolve checks it
        let mut c = stack();
        c.layers.remove(0);
        c.layers[0].layer = None;
        assert!(check(c, Path::new("c.yaml")).is_ok());
    }

    #[test]
    fn a_candidate_that_the_validate_route_checks_gives_a_record() {
        // `--validate` asks Palette for these rules, so strata does not copy them. They are values
        // and a manifest that are not YAML, more than 200 packs, and more than 200 manifests
        let mut c = stack();
        c.layers[1].values = Some("replicas: {{ .spectro.var.replicas }}".into());
        c.layers[4].manifests[0].content = Some("- kind: Namespace".into());
        assert!(check(c, Path::new("c.yaml")).is_ok());
        let mut c = stack();
        let addon = c.layers[4].clone();
        c.layers.extend((0..197).map(|i| CandidateLayer {
            name: format!("addon-{i}"),
            ..addon.clone()
        }));
        assert_eq!(c.layers.len(), 202);
        assert!(check(c, Path::new("c.yaml")).is_ok());
    }

    /// A registry that serves each layer of [`stack`] with the layer `layers[i]` and the cloud
    /// types `clouds[i]`.
    fn served(layers: &[&str], clouds: &[Value]) -> Palette<MemoryApi> {
        let mut api = MemoryApi::new();
        for (i, l) in stack().layers.iter().enumerate() {
            let uid = format!("pk-{i}");
            let summary = json!({"metadata": {"uid": uid},
                "spec": {"name": l.name, "version": l.version, "registryUid": "r1",
                         "layer": layers[i], "type": "spectro", "values": "",
                         "cloudTypes": clouds[i]}});
            api = api
                .with(
                    format!("/v1/packs?filters=spec.name={}&limit=100&offset=0", l.name),
                    json!({"items": [summary], "listmeta": {"count": 1}}),
                )
                .with(
                    format!("/v1/packs/{}/registries/r1", l.name),
                    json!({"tags": [{"tag": l.version, "packUid": uid}]}),
                );
        }
        Palette::new(api)
    }

    const STACK_LAYERS: [&str; 5] = ["os", "k8s", "cni", "csi", "addon"];

    fn resolved(c: Candidate, palette: &Palette<MemoryApi>) -> Result<RawProfile> {
        resolve(
            &check(c, Path::new("c.yaml")).unwrap(),
            palette,
            &Progress::silent(),
        )
    }

    #[test]
    fn a_pack_that_does_not_support_the_cloud_of_the_profile_is_candidate_invalid() {
        let aws = json!(["aws"]);
        let mut clouds = vec![aws.clone(); 5];
        assert!(resolved(stack(), &served(&STACK_LAYERS, &clouds)).is_ok());
        clouds[2] = json!(["vsphere", "edge-native"]);
        let palette = served(&STACK_LAYERS, &clouds);
        let err = resolved(stack(), &palette).unwrap_err();
        let Error::CandidateInvalid { message, .. } = &err else {
            panic!("not candidate-invalid: {err}");
        };
        assert!(
            message.contains("layer 3 (cni-cilium-oss)")
                && message.contains("the clouds vsphere and edge-native")
                && message.contains("the cloud aws (PackNotSupported)"),
            "{message}"
        );
        // the cloud all, and a candidate with no cloud, take each pack
        for cloud in ["all", ""] {
            let mut c = stack();
            c.cloud_type = cloud.into();
            assert!(resolved(c, &palette).is_ok(), "{cloud:?}");
        }
        // a pack of each cloud, a pack with no cloud type, and a custom pack on a custom cloud
        clouds[2] = json!(["all"]);
        clouds[3] = json!([]);
        assert!(resolved(stack(), &served(&STACK_LAYERS, &clouds)).is_ok());
        let mut c = stack();
        c.cloud_type = "my-cloud".into();
        assert!(resolved(c, &served(&STACK_LAYERS, &vec![json!(["custom"]); 5])).is_ok());
    }

    #[test]
    fn resolve_checks_the_layers_that_the_registry_gives() {
        // the file gives no layer for kubernetes, and the registry gives k8s
        let mut c = stack();
        c.kind = "add-on".into();
        c.layers
            .retain(|l| l.name == "kubernetes" || l.name == "cert-manager");
        c.layers[0].layer = None;
        assert!(check(c.clone(), Path::new("c.yaml")).is_ok());
        let clouds = vec![json!(["aws"]); 5];
        let err = resolved(c, &served(&STACK_LAYERS, &clouds)).unwrap_err();
        assert!(
            err.to_string()
                .contains("layer 1 (kubernetes) has the layer k8s"),
            "{err}"
        );
        // and a core layer that is not there, which only the registry shows
        let mut c = stack();
        c.layers[0].layer = None;
        let mut layers = STACK_LAYERS;
        layers[0] = "addon";
        let err = resolved(c, &served(&layers, &clouds)).unwrap_err();
        assert!(err.to_string().contains("has no layer os"), "{err}");
    }

    #[test]
    fn a_layer_from_a_registry_takes_only_the_layer_of_the_registry() {
        let palette = served(&STACK_LAYERS, &vec![json!(["aws"]); 5]);
        // kubernetes first, so that it is layer 1; with no layer on ubuntu-aws, the check of the
        // file does not know each layer, and does not check the core layers
        let first = |layer: Option<&str>| {
            let mut c = stack();
            c.layers.swap(0, 1);
            c.layers[0].layer = layer.map(str::to_string);
            c.layers[1].layer = None;
            c
        };
        let err = resolved(first(Some("addon")), &palette).unwrap_err();
        let Error::CandidateInvalid { message, .. } = &err else {
            panic!("not candidate-invalid: {err}");
        };
        assert!(
            message.contains("layer 1 (kubernetes) has the layer addon")
                && message.contains("its registry gives the layer k8s")
                && message.contains("Palette takes the layer from the registry"),
            "{message}"
        );
        // the layer of the registry, or no layer, gives the pack the layer of the registry
        let records: Vec<RawProfile> = [Some("k8s"), None]
            .into_iter()
            .map(|layer| resolved(first(layer), &palette).unwrap())
            .collect();
        for raw in &records {
            assert_eq!(raw.profile["spec"]["published"]["packs"][0]["layer"], "k8s");
            assert_eq!(raw.manifests["spec"]["packs"][0]["spec"]["layer"], "k8s");
        }
        assert_eq!(records[0].profile, records[1].profile);
        // a wrong layer that the type of the profile takes, and that leaves each core layer
        let mut c = stack();
        c.layers[4].layer = Some("cni".into());
        assert!(check(c.clone(), Path::new("c.yaml")).is_ok());
        let err = resolved(c, &palette).unwrap_err();
        assert!(
            err.to_string().contains(
                "layer 5 (cert-manager) has the layer cni, and its registry gives the layer addon"
            ),
            "{err}"
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
        let sync = registry_sync(&never, "reg-helm", "helm").unwrap().unwrap();
        assert_eq!(sync.last_synced_time, "");
        assert_eq!(sync.last_run_time, "2026-10-05T16:03:58.598Z");
        assert!(sync.in_progress());
        assert_eq!(
            sync.sync_recovery,
            Some(crate::diff::SyncRecovery {
                sync_recovery_state: "Recovering".into(),
                message: "sync interrupted".into(),
            })
        );
    }

    #[test]
    fn palette_takes_a_loose_semantic_version_or_a_text_with_three_numbers_as_the_version() {
        for tag in [
            "1.0.0",
            "v1.2.3",
            "1",
            "1.2",
            "1.2.3-rc.1+b5",
            "01.2.3",
            "chart-1.2.3",
            "1.2.3.4",
            "1.2.3-",
        ] {
            assert!(gaps::check_pack_tag(tag).is_ok(), "{tag}");
        }
        for tag in [
            "",
            "stable",
            "latest",
            "1.2.x",
            "LTS__24.x",
            "1.2-rc..1",
            "v",
        ] {
            assert!(gaps::check_pack_tag(tag).is_err(), "{tag}");
        }
        // only the create checks the tag, so the refusal names the code and the gap
        let message = gaps::check_pack_tag("stable").unwrap_err();
        assert!(message.contains("(InvalidPackVersion). "), "{message}");
        assert!(message.ends_with(gaps::GAP), "{message}");
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
