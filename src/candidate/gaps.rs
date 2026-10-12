//! The rules of Palette that only the create of a profile version checks.
//!
//! No route that takes a candidate checks these rules, so a team learns of a fault only when the
//! create refuses it. Strata fills each gap with a check before any request.
//! Each message names the code that Palette gives at the create. When a route of Palette answers a
//! rule before the create, strata removes its check. The book lists each rule in "The gaps that
//! strata fills".

use super::{Candidate, CandidateLayer, MANIFEST, MANIFEST_LAYER, PACK_TYPE, loose_semver};
use crate::cloud::supports;
use crate::text::and_list;

/// What strata says after each refusal of this module.
pub(super) const GAP: &str = "Palette checks this only at the create, and no route that takes a \
                   candidate answers it before the create. Strata removes this check when a route \
                   answers it";

/// A rule that Palette checks only at the create of a profile version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CreateRule {
    /// The type of the profile takes the layer of each pack.
    TypeLayer,
    /// A `cluster` or `infra` profile has its core layers.
    CoreLayers,
    /// Each pack supports the cloud of the profile.
    Cloud,
    /// The tag of a pack that Palette makes for a registry that it does not sync is a version.
    PackTag,
}

impl CreateRule {
    /// The code of the error that Palette gives at the create.
    fn code(self) -> &'static str {
        match self {
            CreateRule::TypeLayer => "ClusterProfilePackNotAllowed",
            CreateRule::CoreLayers => "ErrPackCoreLayersMissing",
            CreateRule::Cloud => "PackNotSupported",
            CreateRule::PackTag => "InvalidPackVersion",
        }
    }

    /// The message of a refusal: `problem`, the code of Palette, and why strata checks it.
    fn refuse(self, problem: &str) -> String {
        format!("{problem} ({}). {GAP}", self.code())
    }
}

/// The layers that a profile of type `kind` takes, or None for each layer.
///
/// `cluster` takes each layer. `infra` takes `os`, `k8s`, `cni`, and `csi`. Each other type takes
/// `addon`, `cni`, and `csi`.
fn layers_of_type(kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "" | "cluster" | PACK_TYPE => None,
        "infra" => Some(&["os", "k8s", "cni", "csi"]),
        _ => Some(&["addon", "cni", "csi"]),
    }
}

/// The layers that a `cluster` or `infra` profile must have: `os`, `k8s`, and `cni` on the cloud
/// `edge-native`, and `csi` too on each other cloud. None for each other type.
fn core_layers(kind: &str, cloud: &str) -> Option<&'static [&'static str]> {
    match (kind, cloud) {
        ("cluster" | "infra", "edge-native") => Some(&["os", "k8s", "cni"]),
        ("cluster" | "infra", _) => Some(&["os", "k8s", "cni", "csi"]),
        _ => None,
    }
}

/// Check the layers of the type with the layers that the file gives. The layer of a pack that the
/// file does not give comes from the registry, so [`super::resolve`] checks the layers again.
pub(super) fn check_file(c: &Candidate) -> Result<(), String> {
    let known: Vec<Option<&str>> = c
        .layers
        .iter()
        .map(|l| match l.kind.as_deref() {
            Some(MANIFEST) => Some(MANIFEST_LAYER),
            _ => l.layer.as_deref(),
        })
        .collect();
    check_layers(c, &known)
}

/// Check the layer of each pack against the type of the profile, as Palette does at the create and
/// at the publish. `layers` gives, in candidate order, the layer of each pack, and None when it is
/// not known yet: the core layers are then not checked.
pub(super) fn check_layers(c: &Candidate, layers: &[Option<&str>]) -> Result<(), String> {
    if let Some(allowed) = layers_of_type(&c.kind) {
        for (i, (l, layer)) in c.layers.iter().zip(layers).enumerate() {
            if let Some(layer) = layer.filter(|x| !allowed.contains(x)) {
                return Err(CreateRule::TypeLayer.refuse(&format!(
                    "layer {} ({}) has the layer {layer}; a profile of type {} takes only the \
                     layers {}",
                    i + 1,
                    l.name,
                    c.kind,
                    and_list(allowed)
                )));
            }
        }
    }
    if let Some(core) = core_layers(&c.kind, &c.cloud_type)
        && layers.iter().all(Option::is_some)
    {
        let missing: Vec<&str> = core
            .iter()
            .copied()
            .filter(|x| !layers.contains(&Some(*x)))
            .collect();
        if !missing.is_empty() {
            return Err(CreateRule::CoreLayers.refuse(&format!(
                "a profile of type {} needs the layers {}; this one has no layer {}",
                c.kind,
                and_list(core),
                and_list(&missing)
            )));
        }
    }
    Ok(())
}

/// Check that `l`, the layer `i` of a profile of the cloud `cloud`, supports that cloud. `clouds`
/// are the cloud types that the listing of the registry gives for its pack. The rule is
/// [`supports`]. A candidate with no cloud takes each pack.
pub(super) fn check_cloud(
    cloud: &str,
    i: usize,
    l: &CandidateLayer,
    clouds: &[String],
) -> Result<(), String> {
    if cloud.is_empty() || supports(clouds, cloud) {
        return Ok(());
    }
    Err(CreateRule::Cloud.refuse(&format!(
        "layer {} ({}) is the version {} of a pack for the clouds {}; Palette refuses it in a \
         profile of the cloud {cloud}",
        i + 1,
        l.name,
        l.version,
        and_list(&clouds.iter().map(String::as_str).collect::<Vec<_>>()),
    )))
}

/// True when Palette takes `tag` as the version of a pack that it makes. Palette takes a semantic
/// version in a loose form (`v1`, `1.2`, `1.2.3-rc.1+b5`), or a text that holds three numbers with
/// dots between them (`chart-1.2.3`).
fn palette_pack_tag(tag: &str) -> bool {
    loose_semver(tag) || has_three_numbers(tag)
}

/// Whether `text` holds three numbers with dots between them, `\d+\.\d+\.\d+`, at any place.
fn has_three_numbers(text: &str) -> bool {
    let b = text.as_bytes();
    (0..b.len()).any(|start| {
        let mut i = start;
        for part in 0..3 {
            let from = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if i == from {
                return false;
            }
            if part < 2 {
                if b.get(i) != Some(&b'.') {
                    return false;
                }
                i += 1;
            }
        }
        true
    })
}

/// Check `tag`, the tag of a pack from a registry that Palette does not sync. Palette makes the
/// pack at the create, with the tag as its version.
pub(super) fn check_pack_tag(tag: &str) -> Result<(), String> {
    if palette_pack_tag(tag) {
        return Ok(());
    }
    Err(CreateRule::PackTag.refuse(&format!(
        "the tag {tag} is not a semantic version, and it holds no three numbers with dots \
         between them"
    )))
}
