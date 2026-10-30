//! The pack of a record: its fields, its hashes, its manifests, and its read from a pack
//! reference of Palette.

use std::collections::BTreeMap;
use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::{IMAGE_ID_ANNOTATION, PROVIDER_TYPE_ANNOTATION};
use crate::api::mask::MASKED;
use crate::closed_set::{ClosedSet, Open, impl_closed_set};
use crate::content::canon::{canon_yaml_hashed, canonical_json, hash_yaml, sha256_hex};
use crate::content::layer::Layer;
use crate::json::{arr, first_nonempty, flag, opt_str, opt_text, str_at, text, value_at};
use crate::oci_provider::OciProvider;
use crate::pack_type::PackType;

/// One layer of the profile: a pack with its configuration and its hashes.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Pack {
    /// `<layer>/<name>`, the key of the pack's node hash in the root
    pub key: String,
    /// Palette layer name: os, k8s, cni, csi, addon.
    pub layer: String,
    /// Pack name.
    pub name: String,
    /// Pack type: spectro, helm, manifest, oci.
    #[serde(rename = "type")]
    pub kind: String,
    /// Tag as pinned on the profile.
    pub tag: String,
    /// Resolved version.
    pub version: String,
    /// Uid of the pack version in the registry.
    pub pack_uid: String,
    /// Uid of the registry.
    pub registry_uid: String,
    /// The digest Palette stamps on the pack ref of the profile template
    pub ref_digest: String,
    /// The digest of the pack in its registry, from the packs/manifests endpoint
    pub registry_digest: String,
    /// For a pack from an OCI registry: the provider type of that registry, `pack`, `helm`, or
    /// `zarf`, as the pack ref of the profile gives it in its annotations. Palette follows a new
    /// digest in a different way for each. Outside the root: the registry uid is in it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_provider_type: Option<Open<OciProvider>>,
    /// The annotation `imageId` of the pack ref, when Palette gives it. A record keeps no other
    /// annotation, because an annotation can hold a secret. Outside the root: a record written
    /// before this field has none, and a record of strata 0.17 to 0.19 holds each annotation.
    /// [`Pack::image_id`] reads `imageId`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub annotations: BTreeMap<String, String>,
    /// True when Palette marks the pack ref invalid, from `isInvalid`: the registry deleted the
    /// tag of the pack, and Palette builds no cluster from the version. Outside the root: the
    /// flag is a statement of Palette about the registry, and the content of the pack stays.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub invalid: bool,
    /// Why Palette marks the pack invalid, from `inValidReason`, as Palette gives it. Empty when
    /// the pack is not invalid.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub invalid_reason: String,
    /// Pack params as set on the profile.
    pub params: Value,
    /// Raw values YAML as returned by the API
    pub values: String,
    /// The values parsed as YAML and converted to JSON; null when unparseable
    pub values_canon: Value,
    /// sha256 of canonical JSON of valuesCanon, or of the trimmed text when unparseable
    pub values_hash: String,
    /// Why the values did not parse as YAML, when they did not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values_parse_error: Option<String>,
    /// Attached manifests sorted by name
    pub manifests: Vec<Manifest>,
    /// The images that the values declare, `pack.content.images[].image`, sorted: a reference
    /// with a tag, and no digest. Outside the root: the values are in it. A record written before
    /// this field gets it from its values when it is read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
    /// The Helm charts that the values declare, `pack.content.charts[]`, sorted by name. Outside
    /// the root, as `images`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub charts: Vec<PackChart>,
    /// For a pack that strata reads from the registry, of a candidate or of a `pack:` record:
    /// each option of the pack template that the values select, sorted by parameter. Palette can
    /// write the presets of such an option into the values, and no route shows them. Outside the
    /// root: the values are in it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preset_options: Vec<PresetOption>,
    /// The path of each value that Palette masks, sorted: each value that is the text
    /// `********`. Palette writes that text over a value whose schema format is `password`, so
    /// the root covers the mask and not the secret. Outside the root: the values are in it. A
    /// record written before this field gets it from its values when it is read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub masked: Vec<String>,
    /// The three sub-hashes and the node hash.
    pub hash: PackHash,
}

/// One option of a pack template that the values of the pack select: the value at the target
/// key of an input parameter names the option.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PresetOption {
    /// The name of the input parameter.
    pub parameter: String,
    /// The key in the values that the parameter reads, as a dotted path.
    pub target_key: String,
    /// The option that the value at that key names.
    pub option: String,
}

/// One Helm chart that the values of a pack declare.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackChart {
    /// The chart name.
    pub name: String,
    /// The chart version.
    pub version: String,
    /// The repository of the chart.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub repo: String,
}

/// The images and the charts that parsed values declare, from `pack.content`: each sorted, and
/// each image once.
pub fn pack_content(values: &Value) -> (Vec<String>, Vec<PackChart>) {
    let content = value_at(values, "/pack/content");
    let mut images: Vec<String> = arr(content, "/images")
        .iter()
        .filter_map(|i| opt_str(i, "/image").or_else(|| i.as_str()))
        .map(str::to_string)
        .filter(|i| !i.is_empty())
        .collect();
    images.sort();
    images.dedup();
    let mut charts: Vec<PackChart> = arr(content, "/charts")
        .iter()
        .map(|c| PackChart {
            name: text(c, "/name"),
            version: text(c, "/version"),
            repo: text(c, "/repo"),
        })
        .filter(|c| !c.name.is_empty())
        .collect();
    charts.sort();
    charts.dedup();
    (images, charts)
}

/// The path of each value of parsed `values` that Palette masks, sorted, in the form of
/// [`crate::diff::leaf_diff`]: each value that is the text `********`.
pub(super) fn masked_paths(values: &Value) -> Vec<String> {
    crate::diff::leaf::paths_of_text(values, MASKED)
}

/// A manifest attached to a pack on the profile.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// Manifest name.
    pub name: String,
    /// Manifest uid.
    pub uid: String,
    /// Digest as reported by Palette.
    pub digest: String,
    /// Raw YAML content.
    pub content: String,
    /// sha256 of the canonical content, or of the trimmed text when unparseable.
    pub content_hash: String,
    /// Why the content did not parse as YAML, when it did not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_parse_error: Option<String>,
}

/// The field of Palette that gave the upstream digest of a pack, as `hash.upstreamSource` names
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum UpstreamSource {
    /// The digest on the pack ref of the profile template.
    #[serde(rename = "packRef.digest")]
    PackRef,
    /// The digest of the entry of `packs/manifests`, when the pack ref gives none.
    #[serde(rename = "packSummary.digest")]
    PackSummary,
    /// Palette gave no digest, so `upstream` is the hash of nothing: see
    /// [`Pack::upstream_unknown`].
    #[serde(rename = "none")]
    NoDigest,
}

impl_closed_set!(UpstreamSource {
    PackRef => "packRef.digest",
    PackSummary => "packSummary.digest",
    NoDigest => "none",
});

impl fmt::Display for UpstreamSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The three sub-hashes and the node hash of one pack.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackHash {
    /// sha256 over name, layer, type, tag, version, packUid, registryUid
    pub identity: String,
    /// sha256 of upstreamRaw
    pub upstream: String,
    /// The digest that fed `upstream`
    pub upstream_raw: String,
    /// The field of Palette that gave `upstream`.
    pub upstream_source: Open<UpstreamSource>,
    /// sha256 over valuesHash, [manifest name, contentHash]..., params
    pub authored: String,
    /// sha256(identity + upstream + authored)
    pub node: String,
}

impl Pack {
    /// True when the upstream of the pack is unknown: Palette gave no digest for it, so the
    /// `upstream` hash is the hash of nothing and a change of its archive cannot show. A manifest
    /// pack has no archive: its content is the manifests of the profile, which `authored` covers.
    pub fn upstream_unknown(&self) -> bool {
        self.hash.upstream_source == UpstreamSource::NoDigest && !self.is_manifest()
    }

    /// The type of the pack. None for a type that is not a pack type: the type of a tier of an
    /// app profile, such as `container`.
    pub(crate) fn pack_type(&self) -> Option<PackType> {
        PackType::parse(&self.kind)
    }

    /// True for a manifest pack: the manifests of a layer, from no registry.
    pub(crate) fn is_manifest(&self) -> bool {
        self.pack_type() == Some(PackType::Manifest)
    }

    /// The machine image that the pack names in its annotation `imageId`, when it names one.
    pub fn image_id(&self) -> Option<&str> {
        self.annotations
            .get(IMAGE_ID_ANNOTATION)
            .map(String::as_str)
            .filter(|id| !id.is_empty())
    }

    /// Build one pack from its ref on the profile template and the matching entry of the
    /// packs/manifests response (matched by name; missing entries leave those fields empty).
    pub fn from_api(pk: &Value, manifests_packs: &[Value]) -> Pack {
        Self::from_api_with(pk, manifests_packs, None)
    }

    /// [`Pack::from_api`], with the manifests of the pack given apart, each as `name`, `uid`,
    /// `digest`, and `content`, in place of those of the packs/manifests entry.
    pub(super) fn from_api_with(
        pk: &Value,
        manifests_packs: &[Value],
        given: Option<Vec<Value>>,
    ) -> Pack {
        let name = text(pk, "/name");
        let mut mm = manifests_packs
            .iter()
            .find(|m| text(m, "/metadata/name") == name || text(m, "/spec/name") == name)
            .cloned()
            .unwrap_or(Value::Null);
        if let Some(given) = given {
            if !mm.is_object() {
                mm = json!({});
            }
            mm["spec"]["manifests"] = Value::Array(given);
        }
        let layer = first_nonempty(&[str_at(pk, "/layer"), Layer::Addon.as_str()]);
        let kind = first_nonempty(&[str_at(pk, "/type"), PackType::Spectro.as_str()]);
        let tag = text(pk, "/tag");
        let version = first_nonempty(&[str_at(pk, "/version"), str_at(&mm, "/spec/version")]);
        let pack_uid = text(pk, "/packUid");
        let registry_uid = text(pk, "/registryUid");
        let ref_digest = text(pk, "/digest");
        let registry_digest = text(&mm, "/spec/digest");
        let registry_provider_type =
            opt_str(pk, &format!("/annotations/{PROVIDER_TYPE_ANNOTATION}")).map(Open::from_text);
        // only `imageId`: another annotation can hold a secret, and strata reads no other one.
        // Palette gives each annotation as text; a value of another type is not an annotation
        let annotations: BTreeMap<String, String> =
            opt_text(pk, &format!("/annotations/{IMAGE_ID_ANNOTATION}"))
                .map(|id| (IMAGE_ID_ANNOTATION.to_string(), id))
                .into_iter()
                .collect();
        // a reason without the flag is not a reason: Palette clears the flag when the tag is back
        let invalid = flag(pk, "/isInvalid");
        let invalid_reason = if invalid {
            text(pk, "/inValidReason")
        } else {
            String::new()
        };
        let params = pk
            .get("params")
            .cloned()
            .filter(Value::is_object)
            .unwrap_or_else(|| Value::Object(Map::new()));
        let values = first_nonempty(&[str_at(pk, "/values"), str_at(&mm, "/spec/values")]);

        let mut manifests: Vec<Manifest> = arr(&mm, "/spec/manifests")
            .iter()
            .map(|m| {
                let content = text(m, "/content");
                let (content_hash, content_parse_error) = hash_yaml(&content);
                Manifest {
                    name: text(m, "/name"),
                    uid: text(m, "/uid"),
                    digest: text(m, "/digest"),
                    content,
                    content_hash,
                    content_parse_error,
                }
            })
            .collect();
        manifests.sort_by(|a, b| a.name.cmp(&b.name));

        let (values_canon, values_hash, values_parse_error) = canon_yaml_hashed(&values);

        let identity = sha256_hex(&canonical_json(&json!({
            "name": name, "layer": layer, "type": kind, "tag": tag, "version": version,
            "packUid": pack_uid, "registryUid": registry_uid,
        })));
        let (upstream_raw, upstream_source) = if !ref_digest.is_empty() {
            (ref_digest.clone(), UpstreamSource::PackRef)
        } else if !registry_digest.is_empty() {
            (registry_digest.clone(), UpstreamSource::PackSummary)
        } else {
            (String::new(), UpstreamSource::NoDigest)
        };
        let upstream = sha256_hex(&upstream_raw);
        let authored = sha256_hex(&canonical_json(&json!({
            "values": values_hash,
            "manifests": manifests.iter().map(|m| json!([m.name, m.content_hash])).collect::<Vec<_>>(),
            "params": params,
        })));
        let node = sha256_hex(&format!("{identity}{upstream}{authored}"));

        let (images, charts) = pack_content(&values_canon);
        let masked = masked_paths(&values_canon);
        Pack {
            key: format!("{layer}/{name}"),
            layer,
            name,
            kind,
            tag,
            version,
            pack_uid,
            registry_uid,
            ref_digest,
            registry_digest,
            registry_provider_type,
            annotations,
            invalid,
            invalid_reason,
            params,
            values,
            values_canon,
            values_hash,
            values_parse_error,
            manifests,
            images,
            charts,
            // no document of Palette gives a selection: the resolve of a candidate sets it
            preset_options: Vec::new(),
            masked,
            hash: PackHash {
                identity,
                upstream,
                upstream_raw,
                upstream_source: upstream_source.into(),
                authored,
                node,
            },
        }
    }
}
