//! The comparison of two packs: the status, the change flags, the params, the images and the
//! charts, the values, and the manifests.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::leaf::{Leaf, Op, leaf_diff};
use super::{Diff, zip_by_key};
use crate::content::canon::canonical_json;
use crate::diff::impact::{Impact, presence_impact};
use crate::legend::Described;
use crate::snapshot::{Pack, Snapshot, UpstreamSource};

/// What happened to one pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Same node hash on both sides.
    Unchanged,
    /// Different node hash.
    Changed,
    /// Only in B.
    Added,
    /// Only in A.
    Removed,
}

impl Described for Status {
    const ALL: &'static [Self] = &[Self::Unchanged, Self::Changed, Self::Added, Self::Removed];

    fn describe(self) -> &'static str {
        match self {
            Self::Unchanged => "Same node hash on both sides.",
            Self::Changed => "Node hash differs; see identity, upstream and authored.",
            Self::Added => "Present only in B.",
            Self::Removed => "Present only in A.",
        }
    }
}

/// A change flag of a pack: a hash of the pack that moved, or a detail of `AUTHORED`.
///
/// The text and the JSON give its name in capitals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum ChangeFlag {
    /// The identity hash moved: the release points at another pack build.
    Identity,
    /// The upstream hash moved: the content digest of the pack.
    Upstream,
    /// The authored hash moved: the values, the attached manifests, or the params.
    Authored,
    /// The image list that the values declare moved: a detail of `AUTHORED`.
    Images,
    /// The chart list that the values declare moved: a detail of `AUTHORED`.
    Charts,
}

impl ChangeFlag {
    /// The name of the flag: `IDENTITY`, `UPSTREAM`, `AUTHORED`, `IMAGES`, or `CHARTS`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Identity => "IDENTITY",
            Self::Upstream => "UPSTREAM",
            Self::Authored => "AUTHORED",
            Self::Images => "IMAGES",
            Self::Charts => "CHARTS",
        }
    }
}

impl fmt::Display for ChangeFlag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One side of a pack comparison.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackSide {
    /// Tag pinned on the profile.
    pub tag: String,
    /// Resolved version.
    pub version: String,
    /// Pack version uid.
    pub pack_uid: String,
    /// Registry uid.
    pub registry_uid: String,
    /// The raw digest that fed the upstream hash
    pub upstream: String,
    /// Which field of Palette fed it: see [`UpstreamSource`].
    pub upstream_source: UpstreamSource,
    /// Hash of the canonical values.
    pub values_hash: String,
    /// Node hash.
    pub node: String,
    /// True when Palette marks the pack ref of this side invalid: see [`Pack::invalid`].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub invalid: bool,
    /// Why Palette marks it invalid, as Palette gives it: see [`Pack::invalid_reason`].
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub invalid_reason: String,
}

/// One pack across the two sides.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackDiff {
    /// `<layer>/<name>`.
    pub key: String,
    /// Palette layer name.
    pub layer: String,
    /// Pack name.
    pub name: String,
    /// What happened to the pack.
    pub status: Status,
    /// Side A; absent when the pack was added.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a: Option<PackSide>,
    /// Side B; absent when the pack was removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b: Option<PackSide>,
    /// The identity hash moved.
    pub identity: bool,
    /// The upstream hash moved.
    pub upstream: bool,
    /// The authored hash moved.
    pub authored: bool,
    /// The params differ.
    pub params_changed: bool,
    /// True when the only difference is a `packUid` that Palette mints. A manifest pack comes
    /// from no registry, and Palette mints its uid for each profile it creates, so two profiles
    /// with the same manifests have different uids. A pack from a registry that Palette does not
    /// sync gets its uid at the create, so the record of a candidate has none. The content is the
    /// same.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub minted_uid_only: bool,
    /// True when `IDENTITY` moved only because the tag name differs: the same name, layer, type,
    /// version, `packUid`, and `registryUid`, pinned under another of the registry's names for
    /// that version, exact (`LTS__24.04`) or floating (`LTS__24.x`). Palette pins the `packUid`
    /// when the profile is saved, so a cluster runs the same build, and the impact leaves the
    /// rename out. `AUTHORED` and `UPSTREAM` still count.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tag_renamed: bool,
    /// What the change does to clusters.
    pub impact: Impact,
    /// The values compared.
    pub values: ValuesDiff,
    /// Manifest changes by name.
    pub manifests: Vec<ManifestChange>,
    /// True when either side of the pack has no upstream digest: a change of its archive does
    /// not show. A manifest pack has no archive, so it is never true for one.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub upstream_unknown: bool,
    /// The path of each value that Palette masks on either side, sorted: see [`Pack::masked`].
    /// The root covers the mask and not the secret, so an equal root does not show an equal
    /// secret.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub masked: Vec<String>,
    /// True when the image list that the values declare differs: a detail of `AUTHORED`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub images: bool,
    /// True when the chart list that the values declare differs: a detail of `AUTHORED`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub charts: bool,
    /// Each image added, removed, or retagged, by repository.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub image_changes: Vec<ArtifactChange>,
    /// Each chart added, removed, or at another version, by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chart_changes: Vec<ArtifactChange>,
    /// Each param added, removed, or with another value, by name: a detail of `AUTHORED`. The
    /// params of an app tier are its properties.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_changes: Vec<ParamChange>,
}

/// The sides of a pack that Palette marks invalid: see [`PackDiff::invalid_sides`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InvalidSides<'a> {
    /// Only side A, with its reason.
    A(&'a str),
    /// Only side B, with its reason.
    B(&'a str),
    /// The two sides, with the reason of side B.
    Both(&'a str),
}

impl PackDiff {
    /// The flags that moved, in the order of the legend.
    ///
    /// A renamed tag is `IDENTITY`, as the field `identity` says.
    pub fn flags(&self) -> Vec<ChangeFlag> {
        [
            (self.identity, ChangeFlag::Identity),
            (self.upstream, ChangeFlag::Upstream),
            (self.authored, ChangeFlag::Authored),
            (self.images, ChangeFlag::Images),
            (self.charts, ChangeFlag::Charts),
        ]
        .into_iter()
        .filter_map(|(moved, flag)| moved.then_some(flag))
        .collect()
    }

    /// The sides that Palette marks invalid, each with the reason that Palette gives. None when
    /// neither side is invalid.
    pub(crate) fn invalid_sides(&self) -> Option<InvalidSides<'_>> {
        fn reason(s: &Option<PackSide>) -> Option<&str> {
            s.as_ref()
                .filter(|s| s.invalid)
                .map(|s| s.invalid_reason.as_str())
        }
        match (reason(&self.a), reason(&self.b)) {
            (Some(_), Some(b)) => Some(InvalidSides::Both(b)),
            (Some(a), None) => Some(InvalidSides::A(a)),
            (None, Some(b)) => Some(InvalidSides::B(b)),
            (None, None) => None,
        }
    }
}

/// One pack that Palette marks invalid: see [`Verify::invalid_packs`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvalidPack {
    /// `<layer>/<name>` of the pack.
    pub key: String,
    /// Why Palette marks it invalid, as Palette gives it. Empty when Palette gives no reason.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
}

/// Each pack of `s` that Palette marks invalid, in pack order.
pub(crate) fn invalid_packs(s: &Snapshot) -> Vec<InvalidPack> {
    s.packs
        .iter()
        .filter(|p| p.invalid)
        .map(|p| InvalidPack {
            key: p.key.clone(),
            reason: p.invalid_reason.clone(),
        })
        .collect()
}

/// One pack with a value that Palette masks: see [`Pack::masked`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MaskedPack {
    /// `<layer>/<name>` of the pack.
    pub key: String,
    /// The path of each value that Palette masks, on either side, sorted.
    pub paths: Vec<String>,
}

impl Diff {
    /// Each pack with a value that Palette masks on either side, in pack order.
    pub(crate) fn masked_packs(&self) -> Vec<MaskedPack> {
        self.packs
            .iter()
            .filter(|p| !p.masked.is_empty())
            .map(|p| MaskedPack {
                key: p.key.clone(),
                paths: p.masked.clone(),
            })
            .collect()
    }
}

/// One param of a pack, compared.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ParamChange {
    /// What happened.
    pub op: Op,
    /// The param name.
    pub name: String,
    /// The value in A.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old: Option<Value>,
    /// The value in B.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new: Option<Value>,
}

/// The params that differ between two packs, by name.
pub(super) fn param_changes(a: &Value, b: &Value) -> Vec<ParamChange> {
    let empty = serde_json::Map::new();
    let x = a.as_object().unwrap_or(&empty);
    let y = b.as_object().unwrap_or(&empty);
    zip_by_key(x, y)
        .filter_map(|(name, old, new)| {
            let op = match (old, new) {
                (Some(o), Some(n)) if canonical_json(o) == canonical_json(n) => return None,
                (Some(_), Some(_)) => Op::Changed,
                (Some(_), None) => Op::Removed,
                (None, Some(_)) => Op::Added,
                (None, None) => return None,
            };
            Some(ParamChange {
                op,
                name: name.clone(),
                old: old.cloned(),
                new: new.cloned(),
            })
        })
        .collect()
}

/// One image or chart that the values of a pack declare, compared: `changed` is a new tag of
/// one image repository, or a new version of one chart.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactChange {
    /// What happened.
    pub op: Op,
    /// The image repository, or the chart name.
    pub name: String,
    /// The tag or the version in A.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old: Option<String>,
    /// The tag or the version in B.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new: Option<String>,
}

/// An image reference as its repository and its tag: `repo:tag`, or `repo@sha256:...`.
fn split_image(image: &str) -> (&str, &str) {
    if let Some((repo, digest)) = image.split_once('@') {
        return (repo, digest);
    }
    match image.rfind(':') {
        Some(i) if i > image.rfind('/').unwrap_or(0) => (&image[..i], &image[i + 1..]),
        _ => (image, ""),
    }
}

/// Each (name, tag or version) that differs between two lists, by name.
pub(super) fn artifact_changes(
    a: &[(String, String)],
    b: &[(String, String)],
) -> Vec<ArtifactChange> {
    let names: std::collections::BTreeSet<&String> = a.iter().chain(b).map(|(n, _)| n).collect();
    let mut out = Vec::new();
    for name in names {
        let x: Vec<&String> = a
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, v)| v)
            .collect();
        let y: Vec<&String> = b
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, v)| v)
            .collect();
        if x == y {
            continue;
        }
        match (x.as_slice(), y.as_slice()) {
            ([old], [new]) => out.push(ArtifactChange {
                op: Op::Changed,
                name: name.clone(),
                old: Some((*old).clone()),
                new: Some((*new).clone()),
            }),
            _ => {
                for old in x.iter().filter(|v| !y.contains(v)) {
                    out.push(ArtifactChange {
                        op: Op::Removed,
                        name: name.clone(),
                        old: Some((*old).clone()),
                        new: None,
                    });
                }
                for new in y.iter().filter(|v| !x.contains(v)) {
                    out.push(ArtifactChange {
                        op: Op::Added,
                        name: name.clone(),
                        old: None,
                        new: Some((*new).clone()),
                    });
                }
            }
        }
    }
    out
}

/// Each image as (repository, tag). A path segment of the repository that is the version of
/// the pack reads as `<version>`: Palette mirrors the images of a pack version under a path that
/// holds that version, so one image keeps one repository across versions.
pub(super) fn image_pairs(images: &[String], version: &str) -> Vec<(String, String)> {
    images
        .iter()
        .map(|i| {
            let (repo, tag) = split_image(i);
            let repo = if version.is_empty() {
                repo.to_string()
            } else {
                repo.split('/')
                    .map(|seg| if seg == version { "<version>" } else { seg })
                    .collect::<Vec<_>>()
                    .join("/")
            };
            (repo, tag.to_string())
        })
        .collect()
}

pub(super) fn chart_pairs(charts: &[crate::snapshot::PackChart]) -> Vec<(String, String)> {
    charts
        .iter()
        .map(|c| (c.name.clone(), c.version.clone()))
        .collect()
}

/// The values of one pack compared.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValuesDiff {
    /// True when the values hashes differ.
    pub changed: bool,
    /// False when one side's values did not parse as YAML; then only the raw text differs and `diff` is empty
    pub parseable: bool,
    /// Leaves changed.
    pub changed_count: usize,
    /// Leaves added.
    pub added_count: usize,
    /// Leaves removed.
    pub removed_count: usize,
    /// The leaves, sorted by path.
    pub diff: Vec<Leaf>,
}

/// One manifest across the two sides.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManifestChange {
    /// What happened.
    pub op: Op,
    /// Manifest name.
    pub name: String,
    /// Content hash in A.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash_a: Option<String>,
    /// Content hash in B.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash_b: Option<String>,
}

/// The masked paths of the two sides of one pack, sorted, each one time.
pub(super) fn masked_union(x: &Pack, y: &Pack) -> Vec<String> {
    let mut paths: Vec<String> = x.masked.iter().chain(&y.masked).cloned().collect();
    paths.sort();
    paths.dedup();
    paths
}

/// True when the two sides of one pack have a different `packUid` that Palette mints, and are
/// otherwise the same pack: the same name, layer, type, tag, version, and registry. Palette mints
/// the uid of a manifest pack, from no registry, for each profile. It mints the uid of a pack from
/// a registry that it does not sync at the create, so the record of a candidate has none, and
/// that pack has no digest.
pub(crate) fn minted_uid_pair(x: &Pack, y: &Pack) -> bool {
    let manifest = x.registry_uid.is_empty();
    let unsynced = (x.pack_uid.is_empty() || y.pack_uid.is_empty())
        && x.upstream_unknown()
        && y.upstream_unknown();
    x.pack_uid != y.pack_uid
        && x.registry_uid == y.registry_uid
        && (manifest || unsynced)
        && (&x.name, &x.layer, &x.kind, &x.tag, &x.version)
            == (&y.name, &y.layer, &y.kind, &y.tag, &y.version)
}

pub(super) fn side(p: &Pack) -> PackSide {
    PackSide {
        tag: p.tag.clone(),
        version: p.version.clone(),
        pack_uid: p.pack_uid.clone(),
        registry_uid: p.registry_uid.clone(),
        upstream: p.hash.upstream_raw.clone(),
        upstream_source: p.hash.upstream_source,
        values_hash: p.values_hash.clone(),
        node: p.hash.node.clone(),
        invalid: p.invalid,
        invalid_reason: p.invalid_reason.clone(),
    }
}

pub(super) fn values_diff(x: &Pack, y: &Pack) -> ValuesDiff {
    let changed = x.values_hash != y.values_hash;
    let parseable = x.values_parse_error.is_none() && y.values_parse_error.is_none();
    let diff = if changed && parseable {
        leaf_diff(&x.values_canon, &y.values_canon)
    } else {
        Vec::new()
    };
    let count = |op: Op| diff.iter().filter(|l| l.op == op).count();
    ValuesDiff {
        changed,
        parseable,
        changed_count: count(Op::Changed),
        added_count: count(Op::Added),
        removed_count: count(Op::Removed),
        diff,
    }
}

pub(super) fn manifests_diff(x: &Pack, y: &Pack) -> Vec<ManifestChange> {
    fn hashes(p: &Pack) -> impl Iterator<Item = (&str, &str)> {
        p.manifests
            .iter()
            .map(|m| (m.name.as_str(), m.content_hash.as_str()))
    }
    let mut out = Vec::new();
    for (n, a, b) in zip_by_key(hashes(x), hashes(y)) {
        match (a, b) {
            (Some(a), Some(b)) => {
                if a != b {
                    out.push(ManifestChange {
                        op: Op::Changed,
                        name: n.to_string(),
                        hash_a: Some(a.to_string()),
                        hash_b: Some(b.to_string()),
                    });
                }
            }
            (Some(a), None) => out.push(ManifestChange {
                op: Op::Removed,
                name: n.to_string(),
                hash_a: Some(a.to_string()),
                hash_b: None,
            }),
            (None, Some(b)) => out.push(ManifestChange {
                op: Op::Added,
                name: n.to_string(),
                hash_a: None,
                hash_b: Some(b.to_string()),
            }),
            (None, None) => {}
        }
    }
    out
}

/// The diff of a pack that one side holds: `added`, with the pack on side B, or removed, with the
/// pack on side A. No hash moved, and the values and the manifests have no change to show.
pub(super) fn presence_pack_diff(key: &str, pack: &Pack, added: bool) -> PackDiff {
    let (a, b) = if added {
        (None, Some(side(pack)))
    } else {
        (Some(side(pack)), None)
    };
    PackDiff {
        key: key.to_string(),
        layer: pack.layer.clone(),
        name: pack.name.clone(),
        status: if added {
            Status::Added
        } else {
            Status::Removed
        },
        a,
        b,
        identity: false,
        upstream: false,
        authored: false,
        params_changed: false,
        minted_uid_only: false,
        tag_renamed: false,
        impact: presence_impact(&pack.layer, added),
        values: ValuesDiff::default(),
        manifests: Vec::new(),
        upstream_unknown: pack.upstream_unknown(),
        masked: pack.masked.clone(),
        images: false,
        charts: false,
        image_changes: Vec::new(),
        chart_changes: Vec::new(),
        param_changes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_of_a_change_flag_is_its_name_in_the_documents() {
        for flag in [
            ChangeFlag::Identity,
            ChangeFlag::Upstream,
            ChangeFlag::Authored,
            ChangeFlag::Images,
            ChangeFlag::Charts,
        ] {
            assert_eq!(
                serde_json::to_value(flag).unwrap(),
                Value::from(flag.as_str()),
                "{flag:?}"
            );
        }
    }
}
