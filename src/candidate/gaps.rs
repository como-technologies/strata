//! The rules of Palette that only the create of a profile version checks.
//!
//! No route that takes a candidate checks these rules, so a team learns of a fault only when the
//! create refuses it. Strata fills each gap with a check before any request. A check gives a
//! [`RuleViolation`]: the [`CreateRule`] with the code that Palette gives at the create, and the
//! problem in the words of the file. When a route of Palette answers a rule before the create,
//! strata removes its check. The book lists each rule in "The gaps that strata fills".

use std::path::Path;

use super::{Candidate, CandidateLayer, LayerKind, MINTED_LAYER, loose_semver};
use crate::api::codes::CreateRule;
use crate::closed_set::ClosedSet;
use crate::cloud::{Cloud, supports};
use crate::content::layer::Layer;
use crate::error::Error;
use crate::profile_type::ProfileType;
use crate::text::and_list;

/// A rule of the create that a candidate breaks: the rule, and the problem in the words of the
/// file.
#[derive(Debug)]
pub(super) struct RuleViolation {
    /// The rule that the candidate breaks.
    pub(super) rule: CreateRule,
    /// The problem, with the layer or the tag that it is about.
    pub(super) message: String,
}

impl RuleViolation {
    /// The [`Error::CandidateInvalid`] of the candidate file `path` for the violation.
    pub(super) fn at(self, path: &Path) -> Error {
        Error::CandidateInvalid {
            path: path.to_path_buf(),
            rule: Some(self.rule),
            message: self.message,
        }
    }
}

/// The layers of an `infra` profile.
const INFRA_LAYERS: [Layer; 4] = [Layer::Os, Layer::K8s, Layer::Cni, Layer::Csi];

/// The layers of an `add-on` profile, and of each type that is not `cluster` or `infra`.
const ADDON_LAYERS: [Layer; 3] = [Layer::Addon, Layer::Cni, Layer::Csi];

/// The core layers of a profile on the cloud `edge-native`.
const EDGE_CORE_LAYERS: [Layer; 3] = [Layer::Os, Layer::K8s, Layer::Cni];

/// The layers that a profile of type `kind` takes, or None for each layer.
///
/// `cluster` takes each layer. `infra` takes `os`, `k8s`, `cni`, and `csi`. Each other type takes
/// `addon`, `cni`, and `csi`. A candidate with no type takes each layer too: the create of
/// Palette refuses it before the layers count.
fn layers_of_type(kind: &str) -> Option<&'static [Layer]> {
    match ProfileType::parse(kind) {
        Some(ProfileType::Cluster | ProfileType::Pack) => None,
        None if kind.is_empty() => None,
        Some(ProfileType::Infra) => Some(&INFRA_LAYERS),
        _ => Some(&ADDON_LAYERS),
    }
}

/// The layers that a `cluster` or `infra` profile must have: `os`, `k8s`, and `cni` on the cloud
/// `edge-native`, and `csi` too on each other cloud. None for each other type.
fn core_layers(kind: &str, cloud: &str) -> Option<&'static [Layer]> {
    if !ProfileType::text_builds_cluster(kind) {
        return None;
    }
    match Cloud::of(cloud) {
        Cloud::EdgeNative => Some(&EDGE_CORE_LAYERS),
        _ => Some(&INFRA_LAYERS),
    }
}

/// Check the layers of the type with the layers that the file gives. The layer of a pack that the
/// file does not give comes from the registry, so [`super::resolve()`] checks the layers again.
pub(super) fn check_file(c: &Candidate) -> Result<(), RuleViolation> {
    let known: Vec<Option<&str>> = c
        .layers
        .iter()
        .map(|l| match l.kind {
            Some(LayerKind::Manifest) => Some(MINTED_LAYER.as_str()),
            None => l.layer.as_deref(),
        })
        .collect();
    check_layers(c, &known)
}

/// Check the layer of each pack against the type of the profile, as Palette does at the create and
/// at the publish.
///
/// `layers` gives, in candidate order, the layer of each pack, and None when it is not known yet:
/// the core layers are then not checked. A layer name that strata does not know is in no list, so
/// the type refuses it. The message names the text of the file.
pub(super) fn check_layers(c: &Candidate, layers: &[Option<&str>]) -> Result<(), RuleViolation> {
    if let Some(allowed) = layers_of_type(&c.kind) {
        for (i, (l, layer)) in c.layers.iter().zip(layers).enumerate() {
            if let Some(layer) = layer.filter(|x| !allowed.contains(&Layer::parse(x))) {
                return Err(RuleViolation {
                    rule: CreateRule::TypeLayer,
                    message: format!(
                        "layer {} ({}) has the layer {layer}; a profile of type {} takes only the \
                         layers {}",
                        i + 1,
                        l.name,
                        c.kind,
                        and_list(allowed)
                    ),
                });
            }
        }
    }
    if let Some(core) = core_layers(&c.kind, &c.cloud_type)
        && layers.iter().all(Option::is_some)
    {
        let present: Vec<Layer> = layers.iter().flatten().map(|x| Layer::parse(x)).collect();
        let missing: Vec<Layer> = core
            .iter()
            .copied()
            .filter(|x| !present.contains(x))
            .collect();
        if !missing.is_empty() {
            return Err(RuleViolation {
                rule: CreateRule::CoreLayers,
                message: format!(
                    "a profile of type {} needs the layers {}; this one has no layer {}",
                    c.kind,
                    and_list(core),
                    and_list(&missing)
                ),
            });
        }
    }
    Ok(())
}

/// Check that `l`, the layer `i` of a profile of the cloud `cloud`, supports that cloud.
///
/// `clouds` are the cloud types that the listing of the registry gives for its pack. The rule is
/// [`supports`]. A candidate with no cloud takes each pack.
pub(super) fn check_cloud(
    cloud: &str,
    i: usize,
    l: &CandidateLayer,
    clouds: &[String],
) -> Result<(), RuleViolation> {
    if cloud.is_empty() || supports(clouds, cloud) {
        return Ok(());
    }
    Err(RuleViolation {
        rule: CreateRule::Cloud,
        message: format!(
            "layer {} ({}) is the version {} of a pack for the clouds {}; Palette refuses it in a \
             profile of the cloud {cloud}",
            i + 1,
            l.name,
            l.version,
            and_list(&clouds.iter().map(String::as_str).collect::<Vec<_>>()),
        ),
    })
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

/// Check `tag`, the tag of a pack from a registry that Palette does not sync.
///
/// Palette makes the pack at the create, with the tag as its version. `whom` names the layer and
/// its registry, as the start of the message.
pub(super) fn check_pack_tag(whom: &str, tag: &str) -> Result<(), RuleViolation> {
    if palette_pack_tag(tag) {
        return Ok(());
    }
    Err(RuleViolation {
        rule: CreateRule::PackTag,
        message: format!(
            "{whom}; the tag {tag} is not a semantic version, and it holds no three numbers with \
             dots between them"
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_type_of_a_profile_gives_the_layers_that_it_takes() {
        for kind in ["cluster", "pack", ""] {
            assert_eq!(layers_of_type(kind), None, "{kind}");
        }
        assert_eq!(layers_of_type("infra"), Some(&INFRA_LAYERS[..]));
        for kind in ["add-on", "system", "app", "other"] {
            assert_eq!(layers_of_type(kind), Some(&ADDON_LAYERS[..]), "{kind}");
        }
    }

    #[test]
    fn the_core_layers_follow_the_type_and_the_cloud() {
        for kind in ["cluster", "infra"] {
            assert_eq!(core_layers(kind, "aws"), Some(&INFRA_LAYERS[..]), "{kind}");
            assert_eq!(
                core_layers(kind, "edge-native"),
                Some(&EDGE_CORE_LAYERS[..]),
                "{kind}"
            );
        }
        for kind in ["add-on", "system", "app", "pack", "", "other"] {
            assert_eq!(core_layers(kind, "aws"), None, "{kind}");
        }
    }
}
