//! The rules of Palette that only the create of a profile version checks.
//!
//! `POST /v1/clusterprofiles/validate/packs` does not check these rules, so a team learns of a
//! fault only when the create refuses it. Strata fills each gap with a check before any request.
//! Each message names the code that Palette gives at the create. When a route of Palette answers a
//! rule before the create, strata removes its check. The book lists each rule in "The gaps that
//! strata fills".

use super::{Candidate, CandidateLayer, MANIFEST, MANIFEST_LAYER, PACK_TYPE, loose_semver};

/// The longest name that Palette takes for a manifest or a manifest pack.
const NAME_LIMIT: usize = 32;

/// The cloud types that Palette knows. Each other cloud type is a custom cloud.
const KNOWN_CLOUDS: &[&str] = &[
    "all",
    "baremetal",
    "maas",
    "aws",
    "gcp",
    "azure",
    "vsphere",
    "eks",
    "aks",
    "apache-cloudstack",
    "edge-native",
    "nested",
    "gke",
    "generic",
];

/// What strata says after each refusal of this module.
pub(super) const GAP: &str = "Palette checks this only at the create, and no route of Palette answers it \
                   before the create. Strata removes this check when a route answers it";

/// A rule that Palette checks only at the create of a profile version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CreateRule {
    /// The version parses as semver.
    Version,
    /// Each manifest, and each manifest pack, has a DNS-1123 name of 32 characters at most.
    Name,
    /// Each pack name is in the profile once.
    PackTwice,
    /// Each manifest name is in its pack once.
    ManifestTwice,
    /// The type of the profile takes the layer of each pack.
    TypeLayer,
    /// A `cluster` or `infra` profile has its core layers.
    CoreLayers,
    /// Each pack supports the cloud of the profile.
    Cloud,
}

impl CreateRule {
    /// The code of the error that Palette gives at the create.
    fn code(self) -> &'static str {
        match self {
            CreateRule::Version => "VersionParseFailure",
            CreateRule::Name => "InvalidName",
            CreateRule::PackTwice => "DuplicatePacksForbidden",
            CreateRule::ManifestTwice => "DuplicatePackManifestsForbidden",
            CreateRule::TypeLayer => "ClusterProfilePackNotAllowed",
            CreateRule::CoreLayers => "ErrPackCoreLayersMissing",
            CreateRule::Cloud => "PackNotSupported",
        }
    }

    /// The message of a refusal: `problem`, the code of Palette, and why strata checks it.
    fn refuse(self, problem: &str) -> String {
        format!("{problem} ({}). {GAP}", self.code())
    }
}

/// The numbers of the core of a loose semantic version: the text before `-` and `+`, with no
/// `v` at the start.
fn core_numbers(v: &str) -> impl Iterator<Item = &str> {
    let v = v.strip_prefix('v').unwrap_or(v);
    let v = v.split_once('+').map_or(v, |(a, _)| a);
    let core = v.split_once('-').map_or(v, |(a, _)| a);
    core.split('.')
}

/// Whether Palette takes `v` as the version of a profile. Palette takes an optional `v`, one to
/// three numbers, a pre-release, and build data. So `1.1` and `v1.2.3` pass, and
/// `2.2.2.develop` does not. Each number must also fit in a 64-bit integer.
fn palette_version(v: &str) -> bool {
    loose_semver(v) && core_numbers(v).all(|n| n.parse::<i64>().is_ok())
}

/// Whether Palette takes `name` as the name of a manifest or of a manifest pack: a DNS-1123
/// label of 32 characters at most.
fn palette_name(name: &str) -> bool {
    let b = name.as_bytes();
    let inner = |c: &u8| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-';
    name.len() <= NAME_LIMIT
        && b.first().is_some_and(|c| *c != b'-' && inner(c))
        && b.last().is_some_and(|c| *c != b'-' && inner(c))
        && b.iter().all(inner)
}

/// The names that are in `names` more than once, each one time, in the order of their first
/// repeat. Palette compares the names as they are.
fn repeated<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for n in names {
        if !seen.insert(n) && !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

/// The layers that a profile of type `kind` takes: each layer for `cluster`; `os`, `k8s`, `cni`,
/// and `csi` for `infra`; `addon`, `cni`, and `csi` for each other type. None for each layer.
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

/// `items` as `a, b, and c`.
fn listed(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [one] => one.to_string(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// Check the rules that the file alone can break, in this order: the version, a pack name twice,
/// the name of a manifest and of a manifest pack, a manifest name twice in one pack, and the
/// layers of the type. The layer of a pack that the file does not give comes from the registry,
/// so [`super::resolve`] checks the layers again.
pub(super) fn check_file(c: &Candidate) -> Result<(), String> {
    if !palette_version(&c.version) {
        return Err(CreateRule::Version.refuse(&format!(
            "version {} is not a version that Palette takes: give one to three numbers, such as \
             1.2.3, with an optional pre-release, such as 1.2.3-rc.1",
            c.version
        )));
    }
    if let [first, ..] = repeated(c.layers.iter().map(|l| l.name.as_str()))[..] {
        return Err(CreateRule::PackTwice.refuse(&format!(
            "the pack {first} is in more than one layer; Palette takes each pack once in a profile"
        )));
    }
    let named = |what: String, name: &str| {
        CreateRule::Name.refuse(&format!(
            "{what} has the name {name:?}; Palette takes only a name of {NAME_LIMIT} characters \
             at most, of lowercase letters, digits, and '-', that starts and ends with a letter or \
             a digit"
        ))
    };
    for (i, l) in c.layers.iter().enumerate() {
        let whom = format!("layer {} ({})", i + 1, l.name);
        if l.kind.as_deref() == Some(MANIFEST) && !palette_name(&l.name) {
            return Err(named(format!("{whom} is a manifest layer and"), &l.name));
        }
        if let Some(m) = l.manifests.iter().find(|m| !palette_name(&m.name)) {
            return Err(named(format!("a manifest of {whom}"), &m.name));
        }
        if let [first, ..] = repeated(l.manifests.iter().map(|m| m.name.as_str()))[..] {
            return Err(CreateRule::ManifestTwice.refuse(&format!(
                "{whom} has more than one manifest named {first}; Palette takes each name once \
                 in a pack"
            )));
        }
    }
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
                    listed(allowed)
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
                listed(core),
                listed(&missing)
            )));
        }
    }
    Ok(())
}

/// Whether a pack whose listing gives the cloud types `clouds` goes in a profile of the cloud
/// `cloud`: the cloud `all`, a pack with no cloud type, a pack of that cloud or of `all`, and a
/// pack of `custom` on a custom cloud.
fn cloud_supported(cloud: &str, clouds: &[String]) -> bool {
    let custom = !KNOWN_CLOUDS.contains(&cloud);
    cloud == "all"
        || clouds.is_empty()
        || clouds
            .iter()
            .any(|c| c == cloud || c == "all" || (custom && c == "custom"))
}

/// Check that `l`, the layer `i` of a profile of the cloud `cloud`, supports that cloud. `clouds`
/// are the cloud types that the listing of the registry gives for its pack. A candidate with no
/// cloud takes each pack.
pub(super) fn check_cloud(
    cloud: &str,
    i: usize,
    l: &CandidateLayer,
    clouds: &[String],
) -> Result<(), String> {
    if cloud.is_empty() || cloud_supported(cloud, clouds) {
        return Ok(());
    }
    Err(CreateRule::Cloud.refuse(&format!(
        "layer {} ({}) is the version {} of a pack for the clouds {}; Palette refuses it in a \
         profile of the cloud {cloud}",
        i + 1,
        l.name,
        l.version,
        listed(&clouds.iter().map(String::as_str).collect::<Vec<_>>()),
    )))
}
