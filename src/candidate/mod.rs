//! A profile version that does not exist in Palette yet, described by a file.
//!
//! The file names the pack of each layer by name, version, and registry, and holds the values
//! and manifests that the profile will have. Strata reads each pack from the registry for what
//! Palette adds at the publish: the pack uid, the tag, the digest, and the default values. It
//! builds the same documents that a published profile gives, so the candidate has the same root
//! as the published version. A `manifest` layer holds manifests from no registry. Strata makes no
//! request for it, and its pack has no uid until Palette mints one at the create.

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::api::RawProfile;
use crate::content::layer::Layer;
use crate::error::{Error, Result};
use crate::pack_type::PackType;
use crate::profile_type::ProfileType;
use crate::snapshot::{PresetOption, Snapshot};
use rules::invalid_at;

mod export;
mod gaps;
pub(crate) mod pack;
mod presets;
pub(crate) mod resolve;
mod rules;

pub use pack::pack_snapshot;
pub use presets::warn_preset_options;
pub use resolve::resolve;
pub use rules::{check, load};

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
        if !snap.source.is_profile_version() {
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
                    kind: p.is_manifest().then_some(LayerKind::Manifest),
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
    serde_yaml::to_string(candidate)
        .map_err(|e| invalid_at(Path::new(&candidate.name))(e.to_string()))
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
                crate::file::write_file(&file, &values)?;
                layer.values_file = Some(reference(&base, &file)?);
            }
            for m in &mut layer.manifests {
                if let Some(content) = m.content.take() {
                    let file = dir
                        .join("manifests")
                        .join(&pack)
                        .join(format!("{}.yaml", file_name(&m.name)));
                    crate::file::write_file(&file, &content)?;
                    m.content_file = Some(reference(&base, &file)?);
                }
            }
        }
    }
    crate::file::write_file(path, &to_yaml(&c)?)
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
    ProfileType::Cluster.as_str().to_string()
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
    pub kind: Option<LayerKind>,
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

/// The type that a layer of a candidate gives: `manifest`. A pack from a registry gives none:
/// the registry gives its type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LayerKind {
    /// A layer of manifests from no registry. Palette makes its pack from its own manifest pack.
    Manifest,
}

impl<'de> Deserialize<'de> for LayerKind {
    /// Read `manifest`.
    ///
    /// Another text is an error that says what a layer can give: the registry
    /// gives the type of each other pack.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text == PackType::Manifest.as_str() {
            Ok(LayerKind::Manifest)
        } else {
            Err(serde::de::Error::custom(format!(
                "{text} is not a type that a layer gives; a layer can give only the type {}, \
                 and the registry gives the type of each other pack",
                PackType::Manifest.as_str()
            )))
        }
    }
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

/// The layer that Palette gives each pack that it makes itself.
///
/// The packs are a manifest pack, and the pack of a layer from a registry that it does not sync. It
/// drops the layer of the request.
const MINTED_LAYER: Layer = Layer::Addon;

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

/// A candidate resolved against the registry: the documents of the profile that it describes,
/// and what strata reads from the registry beside them.
#[derive(Debug, Clone)]
pub struct ResolvedCandidate {
    /// The documents that a published profile with the content of the candidate gives. They hold
    /// only what Palette gives, and no field of strata.
    pub raw: RawProfile,
    /// The options of the pack template that the values of each pack select: one list for each pack
    /// reference of the profile, in its order.
    ///
    /// The list is empty for a manifest layer, and for a layer of a registry that Palette does not
    /// sync. No listing gives their template.
    pub preset_options: Vec<Vec<PresetOption>>,
}

impl ResolvedCandidate {
    /// The record of the candidate: [`Snapshot::from_raw`] of its documents, with the preset
    /// options of each pack. `reference` and `fetched_at` are as for [`Snapshot::from_raw`].
    ///
    /// # Errors
    ///
    /// The errors of [`Snapshot::from_raw`].
    pub fn into_snapshot(self, reference: &str, fetched_at: &str) -> Result<Snapshot> {
        let mut snap = Snapshot::from_raw(&self.raw, reference, fetched_at)?;
        // the record keeps the packs in the order of the profile document, so each list is the
        // list of its pack
        for (pack, options) in snap.packs.iter_mut().zip(self.preset_options) {
            pack.preset_options = options;
        }
        Ok(snap)
    }
}

#[cfg(test)]
mod tests;
