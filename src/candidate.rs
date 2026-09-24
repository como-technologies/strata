//! A profile version that does not exist in Palette yet, described by a file.
//!
//! The file names each layer's pack by name, version, and registry, and carries the values and
//! manifests the profile will have. Strata looks each pack up in the registry for what Palette
//! fills in on publication (the pack uid, the tag, the digest, and the default values) and builds
//! the same documents a published profile returns, so the candidate hashes exactly as the
//! published version will.

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::api::{Palette, PaletteApi, RawProfile};
use crate::error::{Error, Result};
use crate::progress::Progress;
use crate::reference::LATEST;
use crate::snapshot::{PackSource, Snapshot};

/// The `source.template` of a snapshot built from a candidate file.
pub const TEMPLATE: &str = "candidate";

/// The `source.template` of a snapshot of one pack version from a registry.
pub const PACK_TEMPLATE: &str = "registry";

/// The `source.type` of such a snapshot.
pub const PACK_TYPE: &str = "pack";

/// A candidate profile version. YAML or JSON. Quote versions that YAML would read as numbers,
/// such as `"22.04"`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
        // a candidate describes a profile version to publish: a cluster holds a copy of one, and
        // a pack is one layer of one, so neither makes a candidate
        if snap.source.cluster.is_some() || snap.source.pack.is_some() {
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateLayer {
    /// Pack name.
    pub name: String,
    /// Pack version.
    pub version: String,
    /// Layer: os, k8s, cni, csi, addon. Defaults to the registry's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    /// The tag to pin: one of the registry's tags for the version, or the version itself. Defaults
    /// to the tag equal to the version, else the first tag of that version.
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

/// A layer with its registry settled and its files read.
#[derive(Debug, Clone)]
struct LoadedLayer {
    registry: RegistryRef,
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
pub fn load(path: &Path) -> Result<LoadedCandidate> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let candidate: Candidate =
        serde_yaml::from_str(&text).map_err(|e| Error::CandidateInvalid {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
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
        let registry = pick_registry(&l.registry_uid, &l.registry, invalid, &whom)?
            .or_else(|| top.clone())
            .ok_or_else(|| {
                invalid(format!(
                    "{whom} has no registry; set registry or registryUid on the layer or at the top"
                ))
            })?;
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
    Ok(LoadedCandidate {
        path: path.to_path_buf(),
        candidate,
        layers,
    })
}

/// Every version of `name` the pack listing returns, across registries. The listing caps `limit`
/// at 100, so this pages until it has them all.
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
            registry: RegistryRef::Uid(registry_uid.clone()),
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

/// The state that the registry listing gives a pack version: `deprecated`, `disabled`, or
/// `deleted`, and nothing for a pack in use.
pub(crate) fn pack_state(summary: &Value) -> Option<String> {
    text(summary, "/spec/annotations/system_state").filter(|s| !s.is_empty())
}

/// The state of a pack version that Palette no longer serves: `disabled` or `deleted`.
fn retired_state(summary: &Value) -> Option<String> {
    pack_state(summary).filter(|s| s == "disabled" || s == "deleted")
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

/// Look each layer's pack up in its registry and build the three documents a published profile
/// with this content returns: `GET /v1/registries/metadata` for registry names,
/// `GET /v1/packs?filters=spec.name=<name>` for the pack uid, digest, layer, type, and default
/// values, and `GET /v1/packs/<name>/registries/<uid>` for the tags.
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
    // registry names to uids, in the order Palette lists them; fetched once, and only when needed
    let mut names: Option<Vec<(String, String)>> = None;
    let mut refs = Vec::with_capacity(c.layers.len());
    let mut entries = Vec::with_capacity(c.layers.len());
    for (l, loaded_layer) in c.layers.iter().zip(&loaded.layers) {
        let registry = match &loaded_layer.registry {
            RegistryRef::Uid(u) => u.clone(),
            RegistryRef::Name(n) => {
                if names.is_none() {
                    names = Some(registries(palette)?);
                }
                let visible = names.as_deref().unwrap_or_default();
                visible
                    .iter()
                    .find(|(name, _)| name == n)
                    .map(|(_, uid)| uid.clone())
                    .ok_or_else(|| Error::RegistryNotFound {
                        registry: n.clone(),
                        available: visible.iter().map(|(name, _)| name.clone()).collect(),
                    })?
            }
        };
        let not_found = |tag: Option<String>| Error::PackNotFound {
            name: l.name.clone(),
            version: l.version.clone(),
            tag,
            registry: loaded_layer.registry.text().to_string(),
        };
        let summary = find_version(palette, &l.name, &l.version, &registry)?
            .ok_or_else(|| not_found(None))?;
        // Palette serves no disabled or deleted pack: the registry answers 404 for it, and a
        // profile that pins one is refused. Say so before that read.
        if let Some(state) = retired_state(&summary) {
            // a `pack:` ref comes here with the uid of the registry; the error gives its name
            let registry = match &loaded_layer.registry {
                RegistryRef::Name(n) => n.clone(),
                RegistryRef::Uid(u) => registries(palette)?
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
        let pack_uid = text(&summary, "/metadata/uid").unwrap_or_default();
        let entity = palette
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
            Some(t) => return Err(not_found(Some(t.clone()))),
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
            .or_else(|| text(&summary, "/spec/values"))
            .or(pack_values)
            .unwrap_or_default();
        let layer = l
            .layer
            .clone()
            .or_else(|| text(&summary, "/spec/layer"))
            .or_else(|| text(&entity, "/layer"))
            .unwrap_or_default();
        let kind = text(&summary, "/spec/type").unwrap_or_else(|| "spectro".to_string());
        let digest = text(&summary, "/spec/digest").unwrap_or_default();

        let mut pack_ref = json!({
            "name": l.name, "layer": layer, "type": kind, "tag": tag, "version": l.version,
            "packUid": pack_uid, "registryUid": registry, "values": values,
            "manifests": [], "params": l.params, "presets": [],
        });
        if !digest.is_empty() {
            pack_ref["digest"] = json!(digest);
        }
        refs.push(pack_ref);
        entries.push(json!({
            "metadata": {"name": l.name, "uid": pack_uid},
            "spec": {
                "name": l.name, "layer": layer, "version": l.version, "digest": digest,
                "registryUid": registry, "values": values,
                "manifests": loaded_layer.manifests.iter()
                    .map(|(name, content)| json!({"name": name, "uid": "", "digest": "", "content": content}))
                    .collect::<Vec<_>>(),
            },
            "status": {},
        }));
    }
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
    })
}
