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
use serde_json::{json, Map, Value};

use crate::api::{Palette, PaletteApi, RawProfile};
use crate::error::{Error, Result};
use crate::progress::Progress;

/// The `source.template` of a snapshot built from a candidate file.
pub const TEMPLATE: &str = "candidate";

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
    #[serde(default)]
    pub registry: Option<String>,
    /// The registry for layers that name none, by uid.
    #[serde(default)]
    pub registry_uid: Option<String>,
    /// The layers, in profile order.
    pub layers: Vec<CandidateLayer>,
    /// Profile variable definitions, as Palette returns them.
    #[serde(default)]
    pub variables: Vec<Value>,
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
    #[serde(default)]
    pub layer: Option<String>,
    /// The tag to pin: one of the registry's tags for the version, or the version itself. Defaults
    /// to the tag equal to the version, else the first tag of that version.
    #[serde(default)]
    pub tag: Option<String>,
    /// The registry, by name; overrides the top-level registry.
    #[serde(default)]
    pub registry: Option<String>,
    /// The registry, by uid; overrides the top-level registry.
    #[serde(default)]
    pub registry_uid: Option<String>,
    /// The values YAML. Leave out both this and `valuesFile` for the pack's default values.
    #[serde(default)]
    pub values: Option<String>,
    /// A file with the values YAML, relative to the candidate file.
    #[serde(default)]
    pub values_file: Option<String>,
    /// Pack params.
    #[serde(default)]
    pub params: Map<String, Value>,
    /// Manifests attached to the pack.
    #[serde(default)]
    pub manifests: Vec<CandidateManifest>,
}

/// A manifest attached to a layer: `content` inline or `contentFile`, not both.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateManifest {
    /// Manifest name.
    pub name: String,
    /// The manifest YAML.
    #[serde(default)]
    pub content: Option<String>,
    /// A file with the manifest YAML, relative to the candidate file.
    #[serde(default)]
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

/// A candidate file read and checked, with every file it points at read.
#[derive(Debug, Clone)]
pub struct LoadedCandidate {
    /// The candidate file.
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
    let invalid = |message: String| Error::CandidateInvalid {
        path: path.to_path_buf(),
        message,
    };
    let candidate: Candidate = serde_yaml::from_str(&text).map_err(|e| invalid(e.to_string()))?;
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
                return Err(invalid(format!("{whom} sets both values and valuesFile")))
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
                    )))
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

fn text(v: &Value, ptr: &str) -> Option<String> {
    v.pointer(ptr).and_then(Value::as_str).map(str::to_string)
}

/// The registry's summary of `name` at `version` in `registry`, paging through the listing.
fn find_version<A: PaletteApi>(
    palette: &Palette<A>,
    name: &str,
    version: &str,
    registry: &str,
) -> Result<Option<Value>> {
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
        let found = items.iter().find(|it| {
            text(it, "/spec/name").as_deref() == Some(name)
                && text(it, "/spec/registryUid").as_deref() == Some(registry)
                && text(it, "/spec/version").as_deref() == Some(version)
        });
        if let Some(it) = found {
            return Ok(Some(it.clone()));
        }
        offset += items.len();
        let total = page
            .pointer("/listmeta/count")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        if items.len() < 100 || offset >= total {
            return Ok(None);
        }
    }
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
        "resolving candidate {}@{} against the registry",
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
                    let listing = palette.api().get_json("/v1/registries/metadata")?;
                    names = Some(
                        listing
                            .get("items")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(|it| Some((text(it, "/name")?, text(it, "/uid")?)))
                            .collect(),
                    );
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
        project_uid: None,
    })
}
