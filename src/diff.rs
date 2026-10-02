//! The diff document (schema strata-diff/1): two snapshots compared. Self-describing on
//! purpose: every enum is spelled out, a legend explains the flags and impact levels, and a summary
//! block answers the common questions without walking the packs.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::canon::canonical_json;
pub use crate::impact::{Flags, Impact, ImpactLevel, impact, presence_impact};
use crate::layer::Layer;
use crate::snapshot::{ClusterVariable, Pack, Snapshot, Source, Tool};

/// Schema id written into every diff document.
pub const DIFF_SCHEMA: &str = "strata-diff/1";

/// Two snapshots compared.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Diff {
    /// The schema id of this document
    #[schemars(extend("const" = DIFF_SCHEMA))]
    pub schema: String,
    /// Which tool wrote the document.
    pub tool: Tool,
    /// When, RFC 3339 UTC.
    pub generated_at: String,
    /// What the flags, statuses, ops and impact levels mean
    pub legend: Legend,
    /// The two profiles compared; A is the base, B the candidate
    pub a: Source,
    /// The candidate side.
    pub b: Source,
    /// True when the roots are equal: same release
    pub same: bool,
    /// True when the roots are equal, or differ only by the `packUid`s that Palette mints
    /// (`packs[].mintedUidOnly`) and the tag names packs are pinned under
    /// for the same build (`packs[].tagRenamed`): the same content. `same-release` accepts such a
    /// pair.
    #[serde(default)]
    pub same_content: bool,
    /// True when both sides name the same profile uid and version but their roots differ: the
    /// version was edited in place, or, between two clusters, changed on one of them.
    #[serde(default)]
    pub edited_in_place: bool,
    /// Each profile uid that the two sides name under different versions: the version was
    /// overwritten in place, by a legacy version bump or a rename, whatever the roots. A side
    /// that records a cluster names every profile the cluster runs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overwritten_in_place: Vec<OverwrittenProfile>,
    /// Root of A.
    pub root_a: String,
    /// Root of B.
    pub root_b: String,
    /// Counts and the maximum impact.
    pub summary: Summary,
    /// Profile variables compared.
    pub variables: VariablesDiff,
    /// The values the two clusters give their profile variables, compared; present only when both
    /// sides are clusters. A difference here does not change `same`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster_variables: Option<ClusterVariablesDiff>,
    /// Between two cluster records that carry tiers: each tier that both carry, and the labels,
    /// with what differs in each. A difference here does not change `same`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cluster_tiers: Vec<TierDiff>,
    /// For each side that records a cluster with its `running` tier: each pack whose version, as
    /// the cluster reconciled it, differs from the version that its profile declares.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub running_behind: Vec<RunningBehind>,
    /// True when packs present on both sides appear in a different order
    pub order_changed: bool,
    /// One entry per pack key present on either side, sorted by layer then name
    pub packs: Vec<PackDiff>,
    /// Present only for `verify`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify: Option<Verify>,
    /// The key of each pack, on either side, that has no upstream digest, in pack order. For each
    /// one, a change of its archive does not show, and an equal root does not say the archive is
    /// the same.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upstream_unknown: Vec<String>,
    /// The user macros that either side uses, by name: see [`Snapshot::macros`]. The root covers
    /// their names and not their values, so an edit of a macro does not show here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub macros: Vec<String>,
    /// Between two cluster records that each hold the digests of their macro values: each macro
    /// whose value differs, or that one side resolves and the other does not. The roots can be
    /// equal: the content names the macro, and Palette gives it another value.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub macros_changed: Vec<String>,
    /// With `compare --repave`: what Palette answers when asked if the change repaves the
    /// cluster side, beside the level of the impact table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette_repave: Option<PaletteRepave>,
    /// With `compare --validate`: what Palette answers when asked if it accepts the packs of the
    /// change on the cluster side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette_validation: Option<PaletteValidation>,
}

/// Plain-language meaning of every flag and enum value, for readers without the docs.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Legend {
    /// The IDENTITY flag.
    pub identity: String,
    /// The UPSTREAM flag.
    pub upstream: String,
    /// The AUTHORED flag.
    pub authored: String,
    /// The IMAGES flag.
    #[serde(default)]
    pub images: String,
    /// The CHARTS flag.
    #[serde(default)]
    pub charts: String,
    /// Each impact level.
    pub impact: BTreeMap<String, String>,
    /// Each pack status.
    pub status: BTreeMap<String, String>,
    /// Each leaf op.
    pub op: BTreeMap<String, String>,
}

impl Legend {
    /// The legend every document carries.
    pub fn standard() -> Legend {
        let m = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        Legend {
            identity: "The pack reference changed: name, layer, type, tag, version, packUid or registryUid. The release points at a different pack build.".into(),
            upstream: "The digest on the pack reference changed: pack content moved under the hood, possibly under the same version. Which field fed it is hash.upstreamSource in the snapshot.".into(),
            authored: "Profile-level configuration changed: the pack's values YAML (compared canonically, so formatting does not count), its attached manifests, or its params.".into(),
            images: "The image list that the values declare changed, pack.content.images: an image added, removed, or at another tag. A detail of AUTHORED. The list holds a tag and not a digest, so an image rebuilt under one tag does not show.".into(),
            charts: "The chart list that the values declare changed, pack.content.charts: a chart added, removed, or at another version. A detail of AUTHORED.".into(),
            impact: m(&[
                ("repave", "Nodes are replaced on a cluster of the kubeadm engine: OS image or Kubernetes version change. compare --repave asks Palette."),
                ("may-repave", "Node or control plane configuration changed on the OS or Kubernetes layer; Palette may repave."),
                ("cluster-wide", "CNI or CSI layer changed; affects every node and may repave."),
                ("in-place", "Add-on pack upgraded or reconfigured in place."),
                ("sync", "Only the upstream digest changed; applied on the next profile sync."),
                ("none", "No change, or only the tag name of a pack changed for the same build."),
            ]),
            status: m(&[
                ("unchanged", "Same node hash on both sides."),
                ("changed", "Node hash differs; see identity, upstream and authored."),
                ("added", "Present only in B."),
                ("removed", "Present only in A."),
            ]),
            op: m(&[
                ("changed", "The leaf exists on both sides with different values."),
                ("added", "The leaf exists only in B."),
                ("removed", "The leaf exists only in A."),
            ]),
        }
    }
}

/// Counts that answer the common questions first.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// Packs only in B.
    pub packs_added: usize,
    /// Packs only in A.
    pub packs_removed: usize,
    /// Packs on both sides with a different node hash.
    pub packs_changed: usize,
    /// Packs on both sides with the same node hash.
    pub packs_unchanged: usize,
    /// Packs whose identity hash moved
    pub identity_changes: usize,
    /// Packs whose upstream hash moved.
    pub upstream_changes: usize,
    /// Packs whose authored hash moved.
    pub authored_changes: usize,
    /// True when the variables hash differs.
    pub variables_changed: bool,
    /// The images that changed across the packs on both sides: each one added, removed, or
    /// retagged.
    #[serde(default)]
    pub images_changed: usize,
    /// The charts that changed across the packs on both sides.
    #[serde(default)]
    pub charts_changed: usize,
    /// The most severe impact level across the packs that changed, and the packs that only one
    /// side has
    pub max_impact: ImpactLevel,
}

/// The values two clusters give their profile variables, compared.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterVariablesDiff {
    /// True when any value differs.
    pub changed: bool,
    /// The values that differ.
    pub values: Vec<ClusterValueChange>,
}

/// One tier of two cluster records, compared.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TierDiff {
    /// `applied`, `running`, `policy`, `shape`, or `labels`. Between two clusters, `applied`
    /// compares `spcInfraHash` alone: Palette's `spcHash` is its own for each cluster.
    pub tier: String,
    /// True when the hashes are equal.
    pub same: bool,
    /// The fields that differ, when the tier differs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diff: Vec<Leaf>,
}

/// A pack that a cluster runs at another version than its profile declares.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunningBehind {
    /// `a` or `b`: the side whose cluster it is.
    pub side: String,
    /// The pack name.
    pub pack: String,
    /// The version that the profile declares.
    pub declared: String,
    /// The version that the cluster reconciled.
    pub running: String,
}

/// The tiers that two cluster records both carry, and their labels, compared.
fn tiers_diff(a: &Snapshot, b: &Snapshot) -> Vec<TierDiff> {
    let (Some(x), Some(y)) = (&a.source.cluster, &b.source.cluster) else {
        return Vec::new();
    };
    if x.tiers.is_empty() || y.tiers.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<TierDiff> = x
        .tiers
        .iter()
        .filter_map(|(name, t)| {
            let u = y.tiers.get(name)?;
            // Palette's spcHash is its own for each cluster, and equal for no two: between two
            // clusters, applied compares spcInfraHash alone
            let (f, g) = if name == "applied" && x.uid != y.uid {
                let infra = |v: &Value| json!({"spcInfraHash": v.get("spcInfraHash")});
                (infra(&t.fields), infra(&u.fields))
            } else {
                (t.fields.clone(), u.fields.clone())
            };
            let same = f == g;
            Some(TierDiff {
                tier: name.clone(),
                same,
                diff: if same { Vec::new() } else { leaf_diff(&f, &g) },
            })
        })
        .collect();
    let labels = |m: &std::collections::BTreeMap<String, String>| {
        serde_json::to_value(m).unwrap_or(Value::Null)
    };
    let (la, lb) = (labels(&x.labels), labels(&y.labels));
    out.push(TierDiff {
        tier: "labels".to_string(),
        same: la == lb,
        diff: leaf_diff(&la, &lb),
    });
    out
}

/// The packs of a cluster record whose reconciled version differs from the declared one.
fn running_behind(side: &str, s: &Snapshot) -> Vec<RunningBehind> {
    let Some(tier) = s
        .source
        .cluster
        .as_ref()
        .and_then(|c| c.tiers.get("running"))
    else {
        return Vec::new();
    };
    let running = tier.fields.as_array().cloned().unwrap_or_default();
    s.packs
        .iter()
        .filter_map(|p| {
            let r = running
                .iter()
                .find(|r| r.get("name").and_then(Value::as_str) == Some(p.name.as_str()))?;
            let version = r.get("version").and_then(Value::as_str).unwrap_or_default();
            (!version.is_empty() && !p.version.is_empty() && version != p.version).then(|| {
                RunningBehind {
                    side: side.to_string(),
                    pack: p.name.clone(),
                    declared: p.version.clone(),
                    running: version.to_string(),
                }
            })
        })
        .collect()
}

/// One value that differs between two clusters.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterValueChange {
    /// What happened.
    pub op: Op,
    /// Profile name.
    pub profile: String,
    /// Variable name.
    pub name: String,
    /// Value in A; absent for an addition or a sensitive variable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old: Option<String>,
    /// Value in B; absent for a removal or a sensitive variable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new: Option<String>,
    /// True when the variable is sensitive on either side; its values are never compared.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sensitive: bool,
}

/// A profile uid that the two sides of a comparison name under different versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OverwrittenProfile {
    /// The profile uid.
    pub uid: String,
    /// The profile name on side A.
    pub name: String,
    /// The version side A names under the uid.
    pub version_a: String,
    /// The profile name on side B, when it differs from side A's.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name_b: String,
    /// The version side B names under the uid.
    pub version_b: String,
}

/// The profiles a snapshot names, as (uid, name, version): every profile a cluster runs, or the
/// one profile a profile record holds.
fn named_profiles(s: &Snapshot) -> Vec<(&str, &str, &str)> {
    match &s.source.cluster {
        Some(c) => c
            .profiles
            .iter()
            .map(|p| (p.uid.as_str(), p.name.as_str(), p.version.as_str()))
            .collect(),
        None if s.source.uid.is_empty() => Vec::new(),
        None => vec![(
            s.source.uid.as_str(),
            s.source.name.as_str(),
            s.source.version.as_str(),
        )],
    }
}

/// The profile uids that `a` and `b` name under different versions.
fn overwritten_in_place(a: &Snapshot, b: &Snapshot) -> Vec<OverwrittenProfile> {
    let named_b = named_profiles(b);
    named_profiles(a)
        .into_iter()
        .filter(|(uid, _, _)| !uid.is_empty())
        .filter_map(|(uid, name, version)| {
            let (_, name_b, version_b) = named_b.iter().find(|(u, _, _)| *u == uid)?;
            (version != *version_b).then(|| OverwrittenProfile {
                uid: uid.to_string(),
                name: name.to_string(),
                version_a: version.to_string(),
                name_b: if *name_b == name {
                    String::new()
                } else {
                    name_b.to_string()
                },
                version_b: version_b.to_string(),
            })
        })
        .collect()
}

/// Profile variables compared by name.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VariablesDiff {
    /// True when the hashes differ.
    pub changed: bool,
    /// Variables hash of A.
    pub hash_a: String,
    /// Variables hash of B.
    pub hash_b: String,
    /// Leaf changes with paths like `<variable name>.<field>`
    pub diff: Vec<Leaf>,
    /// The same changes grouped per variable, the form the text reports print
    #[serde(default)]
    pub by_name: VariablesByName,
}

/// Variable changes grouped by variable name.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct VariablesByName {
    /// Variables only in B.
    pub added: Vec<VariableSummary>,
    /// Variables only in A.
    pub removed: Vec<VariableSummary>,
    /// Variables on both sides with different fields.
    pub changed: Vec<VariableChange>,
}

/// One profile variable in brief: its name and the traits a reviewer cares about.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct VariableSummary {
    /// Variable name.
    pub name: String,
    /// The variable must be set.
    pub required: bool,
    /// The value is a secret (`isSensitive` on the variable).
    pub sensitive: bool,
    /// The variable is not shown in the UI.
    pub hidden: bool,
    /// The value cannot change after the cluster is created.
    pub immutable: bool,
}

impl VariableSummary {
    /// The traits that are set, in the order the reports print them.
    pub fn traits(&self) -> Vec<&'static str> {
        [
            (self.required, "required"),
            (self.sensitive, "sensitive"),
            (self.hidden, "hidden"),
            (self.immutable, "immutable"),
        ]
        .into_iter()
        .filter_map(|(on, word)| on.then_some(word))
        .collect()
    }

    fn from_value(v: &Value) -> VariableSummary {
        let flag = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
        VariableSummary {
            name: v
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            required: flag("required"),
            sensitive: flag("isSensitive"),
            hidden: flag("hidden"),
            immutable: flag("immutable"),
        }
    }
}

/// One variable present on both sides with different fields.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VariableChange {
    /// Variable name.
    pub name: String,
    /// The fields that differ, as leaves with paths relative to the variable, for example `defaultValue`
    pub fields: Vec<Leaf>,
}

/// What happened to one leaf or manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Op {
    /// Present on both sides with different values.
    Changed,
    /// Present only in B.
    Added,
    /// Present only in A.
    Removed,
}

/// One scalar (or empty container) that differs.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Leaf {
    /// What happened.
    pub op: Op,
    /// Dotted path; array items as `[i]`, or `[name=<value>]` when every item has a unique name
    pub path: String,
    /// The same path as segments: strings for keys, integers for indexes, {"name": ...} for named items
    pub segments: Vec<Value>,
    /// Value in A, absent for additions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old: Option<Value>,
    /// Value in B, absent for removals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new: Option<Value>,
}

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
    /// Which field fed it: `packRef.digest`, `packSummary.digest`, or `none`.
    pub upstream_source: String,
    /// Hash of the canonical values.
    pub values_hash: String,
    /// Node hash.
    pub node: String,
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
    /// with the same manifests carry different ones. A pack from a registry that Palette does not
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
fn param_changes(a: &Value, b: &Value) -> Vec<ParamChange> {
    let empty = serde_json::Map::new();
    let x = a.as_object().unwrap_or(&empty);
    let y = b.as_object().unwrap_or(&empty);
    let names: std::collections::BTreeSet<&String> = x.keys().chain(y.keys()).collect();
    names
        .into_iter()
        .filter_map(|name| {
            let (old, new) = (x.get(name), y.get(name));
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
fn artifact_changes(a: &[(String, String)], b: &[(String, String)]) -> Vec<ArtifactChange> {
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
fn image_pairs(images: &[String], version: &str) -> Vec<(String, String)> {
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

fn chart_pairs(charts: &[crate::snapshot::PackChart]) -> Vec<(String, String)> {
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

/// Palette's answer to `POST /v1/spectroclusters/{uid}/validate/repave`, for a change from what a
/// cluster runs to a profile version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaletteRepave {
    /// The cluster that Palette was asked about.
    pub cluster_uid: String,
    /// `isRepaveRequired`: Palette would replace the nodes.
    pub required: bool,
    /// Each reason Palette gives, in its order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<PaletteRepaveReason>,
    /// True when the impact table and Palette say the same: both a repave, or both no repave. A
    /// level of `may-repave` agrees with either answer.
    pub agrees: bool,
}

/// One reason Palette gives for a repave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaletteRepaveReason {
    /// For example `PackValuesUpdated`.
    pub code: String,
    /// Palette's sentence.
    pub message: String,
    /// The pack the reason is about.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pack: String,
    /// The config keys that differ, such as `kubeadmconfig.postKubeadmCommands`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<String>,
}

impl PaletteRepave {
    /// Read Palette's answer, and set `agrees` from the most severe level of the impact table.
    pub fn from_palette(cluster_uid: &str, answer: &Value, level: ImpactLevel) -> PaletteRepave {
        let required = answer
            .get("isRepaveRequired")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let reasons = answer
            .get("reasons")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|r| PaletteRepaveReason {
                code: r
                    .get("code")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                message: r
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                pack: r
                    .pointer("/pack/current/name")
                    .or_else(|| r.pointer("/pack/target/name"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                keys: r
                    .pointer("/pack/diffConfigKeys")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|k| k.as_str().map(str::to_string))
                    .collect(),
            })
            .collect();
        let agrees = match level {
            ImpactLevel::Repave => required,
            ImpactLevel::MayRepave => true,
            _ => !required,
        };
        PaletteRepave {
            cluster_uid: cluster_uid.to_string(),
            required,
            reasons,
            agrees,
        }
    }
}

/// Palette's answer to `POST /v1/spectroclusters/{uid}/validate/packs`, for a change from what a
/// cluster runs to a profile version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaletteValidation {
    /// The cluster that Palette was asked about.
    pub cluster_uid: String,
    /// True when Palette gives no error.
    pub accepts: bool,
    /// Each error that Palette gives, sorted by pack, code, and message: Palette sorts the
    /// profiles in no fixed order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<PaletteRefusal>,
}

/// One error that Palette gives for a pack.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaletteRefusal {
    /// The pack that the error is about. Empty when Palette refuses the whole request.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pack: String,
    /// For example `PackDependencyNotFound`.
    pub code: String,
    /// Palette's sentence.
    pub message: String,
}

impl PaletteRefusal {
    /// `<code> <pack>: <message>`, with no pack for a refusal of the whole request.
    pub fn line(&self) -> String {
        if self.pack.is_empty() {
            format!("{}: {}", self.code, self.message)
        } else {
            format!("{} {}: {}", self.code, self.pack, self.message)
        }
    }

    /// The errors of a `v1ConstraintValidatorResponse`: `results[]`, each with the `name` of its
    /// pack and `errors[]` of `code` and `message`.
    pub fn from_constraints(answer: &Value) -> Vec<PaletteRefusal> {
        let text = |v: &Value, key: &str| {
            v.get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        answer
            .get("results")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|r| {
                let pack = text(r, "name");
                r.get("errors")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(move |e| PaletteRefusal {
                        pack: pack.clone(),
                        code: text(e, "code"),
                        message: text(e, "message"),
                    })
            })
            .collect()
    }
}

impl PaletteValidation {
    /// Read Palette's answer: the errors under `profiles[].packs` and under `machinePools`.
    /// Palette gives the error of a pack under each profile that holds a pack of that name, so
    /// strata keeps one copy.
    pub fn from_palette(cluster_uid: &str, answer: &Value) -> PaletteValidation {
        let errors = answer
            .get("profiles")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|p| p.get("packs"))
            .chain(answer.get("machinePools"))
            .flat_map(PaletteRefusal::from_constraints)
            .collect();
        PaletteValidation::refused(cluster_uid, errors)
    }

    /// The answer with these errors, sorted and with no copy. It accepts when there is none.
    pub fn refused(cluster_uid: &str, mut errors: Vec<PaletteRefusal>) -> PaletteValidation {
        errors.sort();
        errors.dedup();
        PaletteValidation {
            cluster_uid: cluster_uid.to_string(),
            accepts: errors.is_empty(),
            errors,
        }
    }
}

/// Each macro whose value digest differs between two cluster records, by name: see
/// [`Diff::macros_changed`]. Empty unless both records hold the digests.
pub fn macros_changed(a: &Snapshot, b: &Snapshot) -> Vec<String> {
    let digests = |s: &Snapshot| {
        s.source
            .cluster
            .as_ref()
            .and_then(|c| c.macro_values.clone())
    };
    let (Some(x), Some(y)) = (digests(a), digests(b)) else {
        return Vec::new();
    };
    let names: std::collections::BTreeSet<&String> = x.keys().chain(y.keys()).collect();
    names
        .into_iter()
        .filter(|n| x.get(*n) != y.get(*n))
        .cloned()
        .collect()
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

/// The outcome of `verify`, present only in its documents.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Verify {
    /// Unchanged or drifted.
    pub status: VerifyStatus,
    /// Profile uid that was re-fetched.
    pub uid: String,
    /// When the record was taken.
    pub recorded_at: String,
    /// When the live profile was fetched.
    pub checked_at: String,
    /// For a profile record whose uid now names another version: that `name@version`. The
    /// recorded version was overwritten in place, so `status` is drifted even when the roots
    /// match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overwritten_by: Option<String>,
    /// For a profile record whose uid now names the same version under another name: that
    /// `name@version`. The version was renamed, so `status` is drifted even when the roots match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renamed_to: Option<String>,
    /// Palette no longer has the object of the record: its uid is gone, or, for a cluster, the
    /// document has the state `Deleted` or `metadata.isDeleted`. `status` is drifted, and each
    /// pack of the record is removed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gone: bool,
    /// For a gone object whose document Palette keeps: when it was deleted, from
    /// `metadata.deletionTimestamp`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
    /// For a record of a cluster, an edge host, or a cluster group: the state of the object now,
    /// as Palette gives it. `status.state` of a cluster (such as `Running`, `Deleting`, `Error`) or
    /// of an edge host (`ready`, `unpaired`, `in-use`); `active` or `inactive` from
    /// `status.isActive` of a cluster group. Absent when Palette gives none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// For a cluster record: each profile the cluster runs whose uid now names another version
    /// than the cluster's copy, or no longer exists. The cluster may run exactly what it ran, but
    /// the version it runs is gone from Palette, so `status` is drifted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles_overwritten: Vec<ClusterProfileOverwritten>,
    /// For a cluster record: how many values the cluster gives its variables differ from the
    /// record. They do not change `status`.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub cluster_values_changed: usize,
    /// With `--who`, for a drifted record: each write to the object of the record that the audit
    /// log of Palette holds from `recordedAt`, oldest first. For a cluster record, also each write
    /// to a profile of `profilesOverwritten`: the write that overwrote the version is an entry of
    /// the profile, not of the cluster. Empty when the log holds none. Absent without `--who`, for
    /// an unchanged record, and for a pack record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub who: Option<Vec<AuditEntry>>,
    /// When Palette last changed the object of the record, from `metadata.lastModifiedTimestamp`
    /// of the document that `verify` reads: the profile, the app profile, the cluster, the edge
    /// host, the cluster group, or the pack in its registry. It costs no request. Absent when
    /// Palette gives no time, or when the object is gone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    /// For an `unverified` record: the key of each pack with no upstream digest.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upstream_unknown: Vec<String>,
    /// For a cluster record that holds the digests of its macro values: each macro whose value is
    /// another one now. The status is `drifted`, also when the roots are equal.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub macros_changed: Vec<String>,
    /// For a pack record: the last sync of the registry, as Palette gives it. Palette holds a copy
    /// of each registry, and the verdict states that copy. Absent when the registry does not sync,
    /// or when Palette does not answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_sync: Option<RegistrySync>,
    /// For a pack record: why strata could not read the last sync of the registry, when Palette
    /// answered with a 5xx or the request did not complete. `registrySync` is then absent. It does
    /// not change `status`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_sync_error: Option<String>,
}

/// The last sync of a registry: `status.sync` of the registry in Palette, `v1RegistrySyncStatus`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySync {
    /// When the last sync that completed ended, as Palette gives it. Empty when Palette gives no
    /// time.
    pub last_synced_time: String,
    /// The state of the last sync: `Completed`, `InProgress`, or `Failed`, as Palette gives it.
    pub status: String,
    /// What Palette says about the last sync, such as the cause of a failure. Empty when it says
    /// nothing.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
}

/// One write to an object, as the audit log of Palette holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    /// When Palette logged the write, as Palette gives it.
    pub time: String,
    /// Who wrote: the name of a user, else the email or the uid of the user; or the name of a
    /// Palette service, such as `system` or `cluster-agent`.
    pub actor: String,
    /// `user` or `service`, as Palette gives it.
    pub actor_type: String,
    /// The uid of the user; empty for a service.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub actor_uid: String,
    /// `create`, `update`, `delete`, `activity`, or `action`, as Palette gives it.
    pub action: String,
    /// What Palette says changed, when it says it, such as the manifest that an edit changed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
    /// The note that a person adds to the entry with `PATCH /v1/audits/{uid}/userMsg`, such as
    /// the ticket of the change. Empty when the entry has no note.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user_msg: String,
    /// The object that the write touched, from `spec.resource` of the entry. Absent when Palette
    /// gives none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<AuditResource>,
    /// The uid of the entry: `GET /v1/audits/{uid}` reads it again. Palette gives no request id.
    pub audit_uid: String,
}

/// The object of an audit entry: `spec.resource` of the entry, `v1AuditResourceReference`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditResource {
    /// The kind of the object in the log: `clusterprofile`, `appprofile`, `spectrocluster`,
    /// `edgehost`, or `clustergroup`, as Palette gives it.
    pub kind: String,
    /// The name of the object, as Palette gives it. Empty when it gives none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The uid of the object.
    pub uid: String,
}

impl AuditEntry {
    /// Read one item of `GET /v1/audits`.
    pub fn from_palette(item: &Value) -> AuditEntry {
        let text = |ptr: &str| {
            item.pointer(ptr)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        let actor_type = text("/spec/actor/actorType");
        let full_name = format!(
            "{} {}",
            text("/spec/actor/user/firstName"),
            text("/spec/actor/user/lastName")
        );
        let actor = [
            full_name.trim().to_string(),
            text("/spec/actor/user/emailId"),
            text("/spec/actor/user/uid"),
            text("/spec/actor/serviceName"),
        ]
        .into_iter()
        .find(|s| !s.is_empty())
        .unwrap_or_else(|| actor_type.clone());
        AuditEntry {
            time: text("/metadata/creationTimestamp"),
            actor,
            actor_type,
            actor_uid: text("/spec/actor/user/uid"),
            action: text("/spec/actionType"),
            // a summary can span lines, as a list of the values that changed does
            summary: text("/spec/contentMsg")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
            user_msg: text("/spec/userMsg"),
            resource: item
                .pointer("/spec/resource")
                .filter(|r| r.is_object())
                .map(|_| AuditResource {
                    kind: text("/spec/resource/kind"),
                    name: text("/spec/resource/name"),
                    uid: text("/spec/resource/uid"),
                }),
            audit_uid: text("/metadata/uid"),
        }
    }
}

/// A profile a cluster runs whose uid no longer names the version the cluster's copy names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterProfileOverwritten {
    /// The profile uid.
    pub uid: String,
    /// The profile name, as the cluster's copy names it.
    pub name: String,
    /// The version the cluster's copy names.
    pub cluster_version: String,
    /// The version the uid names now; empty when the uid no longer exists.
    pub profile_version: String,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// Whether a published version still matches its record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VerifyStatus {
    /// Same root.
    Unchanged,
    /// Different root: the published version was edited, or the record is stale.
    Drifted,
    /// Same content, and a pack has no upstream digest, so a change of its archive would not
    /// show. `--allow-no-digest` makes it `unchanged`.
    Unverified,
}

enum Seg {
    Key(String),
    Index(usize),
    Named(String),
}

fn render_path(segs: &[Seg]) -> String {
    let mut out = String::new();
    for seg in segs {
        match seg {
            Seg::Key(k) => {
                if !out.is_empty() {
                    out.push('.');
                }
                out.push_str(k);
            }
            Seg::Index(i) => out.push_str(&format!("[{i}]")),
            Seg::Named(n) => out.push_str(&format!("[name={n}]")),
        }
    }
    out
}

fn segments_json(segs: &[Seg]) -> Vec<Value> {
    segs.iter()
        .map(|s| match s {
            Seg::Key(k) => Value::String(k.clone()),
            Seg::Index(i) => Value::from(*i),
            Seg::Named(n) => json!({ "name": n }),
        })
        .collect()
}

/// When every item is an object with a unique string `name`, address items by name so that an
/// insertion shows as one addition rather than shifted neighbors.
fn name_keys(items: &[Value]) -> Option<Vec<String>> {
    let mut names = Vec::with_capacity(items.len());
    for it in items {
        let n = it.as_object()?.get("name")?.as_str()?;
        if names.iter().any(|x: &String| x == n) {
            return None;
        }
        names.push(n.to_string());
    }
    Some(names)
}

type Leaves = BTreeMap<String, (Vec<Value>, Value)>;

fn flatten(v: &Value, path: &mut Vec<Seg>, out: &mut Leaves) {
    match v {
        Value::Object(m) if !m.is_empty() => {
            for (k, x) in m {
                path.push(Seg::Key(k.clone()));
                flatten(x, path, out);
                path.pop();
            }
        }
        Value::Array(a) if !a.is_empty() => {
            if let Some(names) = name_keys(a) {
                for (x, n) in a.iter().zip(names) {
                    path.push(Seg::Named(n));
                    flatten(x, path, out);
                    path.pop();
                }
            } else {
                for (i, x) in a.iter().enumerate() {
                    path.push(Seg::Index(i));
                    flatten(x, path, out);
                    path.pop();
                }
            }
        }
        _ => {
            out.insert(render_path(path), (segments_json(path), v.clone()));
        }
    }
}

/// Every scalar or empty container that differs between two JSON documents, sorted by path.
pub fn leaf_diff(a: &Value, b: &Value) -> Vec<Leaf> {
    let (mut la, mut lb) = (Leaves::new(), Leaves::new());
    flatten(a, &mut Vec::new(), &mut la);
    flatten(b, &mut Vec::new(), &mut lb);
    // An empty or scalar document on one side against a structured document on the other reads
    // as additions or removals of the structure, not as a change at the root path.
    if la.keys().chain(lb.keys()).any(|k| !k.is_empty()) {
        la.remove("");
        lb.remove("");
    }
    let mut keys: Vec<&String> = la.keys().chain(lb.keys()).collect();
    keys.sort();
    keys.dedup();
    let mut out = Vec::new();
    for k in keys {
        match (la.get(k), lb.get(k)) {
            (Some((segs, x)), Some((_, y))) => {
                if x != y {
                    out.push(Leaf {
                        op: Op::Changed,
                        path: k.clone(),
                        segments: segs.clone(),
                        old: Some(x.clone()),
                        new: Some(y.clone()),
                    });
                }
            }
            (Some((segs, x)), None) => out.push(Leaf {
                op: Op::Removed,
                path: k.clone(),
                segments: segs.clone(),
                old: Some(x.clone()),
                new: None,
            }),
            (None, Some((segs, y))) => out.push(Leaf {
                op: Op::Added,
                path: k.clone(),
                segments: segs.clone(),
                old: None,
                new: Some(y.clone()),
            }),
            (None, None) => {}
        }
    }
    out
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

fn side(p: &Pack) -> PackSide {
    PackSide {
        tag: p.tag.clone(),
        version: p.version.clone(),
        pack_uid: p.pack_uid.clone(),
        registry_uid: p.registry_uid.clone(),
        upstream: p.hash.upstream_raw.clone(),
        upstream_source: p.hash.upstream_source.clone(),
        values_hash: p.values_hash.clone(),
        node: p.hash.node.clone(),
    }
}

fn values_diff(x: &Pack, y: &Pack) -> ValuesDiff {
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

fn manifests_diff(x: &Pack, y: &Pack) -> Vec<ManifestChange> {
    let mx: BTreeMap<&str, &str> = x
        .manifests
        .iter()
        .map(|m| (m.name.as_str(), m.content_hash.as_str()))
        .collect();
    let my: BTreeMap<&str, &str> = y
        .manifests
        .iter()
        .map(|m| (m.name.as_str(), m.content_hash.as_str()))
        .collect();
    let mut names: Vec<&str> = mx.keys().chain(my.keys()).copied().collect();
    names.sort();
    names.dedup();
    let mut out = Vec::new();
    for n in names {
        match (mx.get(n), my.get(n)) {
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

fn by_name(vars: &[Value]) -> Value {
    let mut m = serde_json::Map::new();
    for v in vars {
        let name = v
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        m.insert(name, v.clone());
    }
    Value::Object(m)
}

/// Group two normalized variable lists by name: added, removed, and changed with the fields that
/// differ. Names are sorted.
pub fn group_variables(a: &[Value], b: &[Value]) -> VariablesByName {
    let (ma, mb) = (by_name(a), by_name(b));
    let (ma, mb) = (
        ma.as_object().cloned().unwrap_or_default(),
        mb.as_object().cloned().unwrap_or_default(),
    );
    let mut names: Vec<&String> = ma.keys().chain(mb.keys()).collect();
    names.sort();
    names.dedup();
    let mut g = VariablesByName::default();
    for n in names {
        match (ma.get(n), mb.get(n)) {
            (None, Some(y)) => g.added.push(VariableSummary::from_value(y)),
            (Some(x), None) => g.removed.push(VariableSummary::from_value(x)),
            (Some(x), Some(y)) => {
                let fields = leaf_diff(x, y);
                if !fields.is_empty() {
                    g.changed.push(VariableChange {
                        name: n.clone(),
                        fields,
                    });
                }
            }
            (None, None) => {}
        }
    }
    g
}

/// The values two clusters give their variables, matched by profile name and variable name.
fn cluster_values_diff(a: &[ClusterVariable], b: &[ClusterVariable]) -> ClusterVariablesDiff {
    let keyed = |vs: &[ClusterVariable]| -> BTreeMap<(String, String), ClusterVariable> {
        vs.iter()
            .map(|v| ((v.profile.clone(), v.name.clone()), v.clone()))
            .collect()
    };
    let (ka, kb) = (keyed(a), keyed(b));
    let mut keys: Vec<&(String, String)> = ka.keys().chain(kb.keys()).collect();
    keys.sort();
    keys.dedup();
    let values: Vec<ClusterValueChange> = keys
        .into_iter()
        .filter_map(|k| {
            let (x, y) = (ka.get(k), kb.get(k));
            let op = match (x, y) {
                (Some(x), Some(y)) if x.value == y.value && x.sensitive == y.sensitive => {
                    return None;
                }
                (Some(_), Some(_)) => Op::Changed,
                (None, Some(_)) => Op::Added,
                _ => Op::Removed,
            };
            Some(ClusterValueChange {
                op,
                profile: k.0.clone(),
                name: k.1.clone(),
                old: x.and_then(|v| v.value.clone()),
                new: y.and_then(|v| v.value.clone()),
                sensitive: x.is_some_and(|v| v.sensitive) || y.is_some_and(|v| v.sensitive),
            })
        })
        .collect();
    ClusterVariablesDiff {
        changed: !values.is_empty(),
        values,
    }
}

/// Compare two snapshots. `verify` is attached when this is the second half of a verification.
pub fn compute(a: &Snapshot, b: &Snapshot, generated_at: &str, verify: Option<Verify>) -> Diff {
    let pa: BTreeMap<&str, &Pack> = a.packs.iter().map(|p| (p.key.as_str(), p)).collect();
    let pb: BTreeMap<&str, &Pack> = b.packs.iter().map(|p| (p.key.as_str(), p)).collect();
    let mut keys: Vec<&str> = pa.keys().chain(pb.keys()).copied().collect();
    keys.sort();
    keys.dedup();

    let mut summary = Summary::default();
    let mut packs = Vec::with_capacity(keys.len());
    for k in keys {
        let pd = match (pa.get(k), pb.get(k)) {
            (None, Some(y)) => {
                summary.packs_added += 1;
                let imp = presence_impact(&y.layer, true);
                if imp.level > summary.max_impact {
                    summary.max_impact = imp.level;
                }
                PackDiff {
                    key: k.to_string(),
                    layer: y.layer.clone(),
                    name: y.name.clone(),
                    status: Status::Added,
                    a: None,
                    b: Some(side(y)),
                    identity: false,
                    upstream: false,
                    authored: false,
                    params_changed: false,
                    minted_uid_only: false,
                    tag_renamed: false,
                    impact: imp,
                    values: ValuesDiff::default(),
                    manifests: Vec::new(),
                    upstream_unknown: y.upstream_unknown(),
                    images: false,
                    charts: false,
                    image_changes: Vec::new(),
                    chart_changes: Vec::new(),
                    param_changes: Vec::new(),
                }
            }
            (Some(x), None) => {
                summary.packs_removed += 1;
                let imp = presence_impact(&x.layer, false);
                if imp.level > summary.max_impact {
                    summary.max_impact = imp.level;
                }
                PackDiff {
                    key: k.to_string(),
                    layer: x.layer.clone(),
                    name: x.name.clone(),
                    status: Status::Removed,
                    a: Some(side(x)),
                    b: None,
                    identity: false,
                    upstream: false,
                    authored: false,
                    params_changed: false,
                    minted_uid_only: false,
                    tag_renamed: false,
                    impact: imp,
                    values: ValuesDiff::default(),
                    manifests: Vec::new(),
                    upstream_unknown: x.upstream_unknown(),
                    images: false,
                    charts: false,
                    image_changes: Vec::new(),
                    chart_changes: Vec::new(),
                    param_changes: Vec::new(),
                }
            }
            (Some(x), Some(y)) => {
                let identity = x.hash.identity != y.hash.identity;
                let upstream = x.hash.upstream != y.hash.upstream;
                let authored = x.hash.authored != y.hash.authored;
                let status = if x.hash.node == y.hash.node {
                    Status::Unchanged
                } else {
                    Status::Changed
                };
                match status {
                    Status::Changed => summary.packs_changed += 1,
                    _ => summary.packs_unchanged += 1,
                }
                summary.identity_changes += usize::from(identity);
                summary.upstream_changes += usize::from(upstream);
                summary.authored_changes += usize::from(authored);
                let tag_renamed = identity
                    && x.tag != y.tag
                    && (
                        &x.name,
                        &x.layer,
                        &x.kind,
                        &x.version,
                        &x.pack_uid,
                        &x.registry_uid,
                    ) == (
                        &y.name,
                        &y.layer,
                        &y.kind,
                        &y.version,
                        &y.pack_uid,
                        &y.registry_uid,
                    );
                let mut imp = impact(
                    Layer::parse(&x.layer),
                    Flags {
                        identity: identity && !tag_renamed,
                        upstream,
                        authored,
                    },
                );
                if tag_renamed && imp.level == ImpactLevel::None {
                    imp.text = "none (tag renamed, same pack build)".to_string();
                }
                if imp.level > summary.max_impact {
                    summary.max_impact = imp.level;
                }
                let image_changes = artifact_changes(
                    &image_pairs(&x.images, &x.version),
                    &image_pairs(&y.images, &y.version),
                );
                let chart_changes =
                    artifact_changes(&chart_pairs(&x.charts), &chart_pairs(&y.charts));
                summary.images_changed += image_changes.len();
                summary.charts_changed += chart_changes.len();
                PackDiff {
                    key: k.to_string(),
                    layer: x.layer.clone(),
                    name: x.name.clone(),
                    status,
                    a: Some(side(x)),
                    b: Some(side(y)),
                    identity,
                    upstream,
                    authored,
                    params_changed: canonical_json(&x.params) != canonical_json(&y.params),
                    minted_uid_only: identity && !upstream && !authored && minted_uid_pair(x, y),
                    tag_renamed,
                    impact: imp,
                    values: values_diff(x, y),
                    manifests: manifests_diff(x, y),
                    upstream_unknown: x.upstream_unknown() || y.upstream_unknown(),
                    images: !image_changes.is_empty(),
                    charts: !chart_changes.is_empty(),
                    image_changes,
                    chart_changes,
                    param_changes: param_changes(&x.params, &y.params),
                }
            }
            (None, None) => unreachable!("key came from one of the maps"),
        };
        packs.push(pd);
    }
    packs.sort_by(|p, q| {
        (Layer::parse(&p.layer).rank(), &p.name).cmp(&(Layer::parse(&q.layer).rank(), &q.name))
    });

    let order_a: Vec<&str> = a
        .packs
        .iter()
        .map(|p| p.key.as_str())
        .filter(|k| pb.contains_key(k))
        .collect();
    let order_b: Vec<&str> = b
        .packs
        .iter()
        .map(|p| p.key.as_str())
        .filter(|k| pa.contains_key(k))
        .collect();

    let variables_changed = a.variables_hash != b.variables_hash;
    summary.variables_changed = variables_changed;
    let order_changed = order_a != order_b;
    let same_content = a.root == b.root
        || (!variables_changed
            && !order_changed
            && packs.iter().all(|p| {
                p.status == Status::Unchanged
                    || p.minted_uid_only
                    || (p.tag_renamed && !p.upstream && !p.authored)
            }));

    Diff {
        schema: DIFF_SCHEMA.to_string(),
        tool: Tool::current(),
        generated_at: generated_at.to_string(),
        legend: Legend::standard(),
        a: a.source.clone(),
        b: b.source.clone(),
        same: a.root == b.root,
        same_content,
        edited_in_place: a.root != b.root
            && !a.source.uid.is_empty()
            && a.source.uid == b.source.uid
            && a.source.version == b.source.version,
        overwritten_in_place: overwritten_in_place(a, b),
        macros: {
            let mut names = a.macros();
            names.extend(b.macros());
            names.sort();
            names.dedup();
            names
        },
        macros_changed: macros_changed(a, b),
        palette_repave: None,
        palette_validation: None,
        upstream_unknown: packs
            .iter()
            .filter(|p| p.upstream_unknown)
            .map(|p| p.key.clone())
            .collect(),
        root_a: a.root.clone(),
        root_b: b.root.clone(),
        summary,
        variables: VariablesDiff {
            changed: variables_changed,
            hash_a: a.variables_hash.clone(),
            hash_b: b.variables_hash.clone(),
            diff: if variables_changed {
                leaf_diff(&by_name(&a.variables), &by_name(&b.variables))
            } else {
                Vec::new()
            },
            by_name: if variables_changed {
                group_variables(&a.variables, &b.variables)
            } else {
                VariablesByName::default()
            },
        },
        cluster_variables: match (&a.source.cluster, &b.source.cluster) {
            // an edge host and a cluster group give no values, so only two clusters compare
            (Some(x), Some(y)) if x.kind.is_cluster() && y.kind.is_cluster() => {
                Some(cluster_values_diff(&x.variables, &y.variables))
            }
            _ => None,
        },
        cluster_tiers: tiers_diff(a, b),
        running_behind: {
            let mut v = running_behind("a", a);
            v.extend(running_behind("b", b));
            v
        },
        order_changed,
        packs,
        verify,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaf_diff_reports_changed_added_removed_and_keeps_false_and_null() {
        let a = json!({"x": {"on": true, "n": null, "gone": 1}, "list": [1, 2]});
        let b = json!({"x": {"on": false, "n": null, "new": "v"}, "list": [1, 2, 3]});
        let d = leaf_diff(&a, &b);
        let paths: Vec<(Op, &str)> = d.iter().map(|l| (l.op, l.path.as_str())).collect();
        assert_eq!(
            paths,
            vec![
                (Op::Added, "list[2]"),
                (Op::Removed, "x.gone"),
                (Op::Added, "x.new"),
                (Op::Changed, "x.on"),
            ]
        );
    }

    #[test]
    fn arrays_of_named_objects_are_addressed_by_name() {
        let a = json!({"env": [{"name": "A", "value": "1"}, {"name": "B", "value": "2"}]});
        let b = json!({"env": [{"name": "Z", "value": "0"}, {"name": "A", "value": "1"}, {"name": "B", "value": "3"}]});
        let d = leaf_diff(&a, &b);
        let paths: Vec<(Op, &str)> = d.iter().map(|l| (l.op, l.path.as_str())).collect();
        assert_eq!(
            paths,
            vec![
                (Op::Changed, "env[name=B].value"),
                (Op::Added, "env[name=Z].name"),
                (Op::Added, "env[name=Z].value"),
            ]
        );
        assert_eq!(
            d[0].segments,
            vec![json!("env"), json!({"name": "B"}), json!("value")]
        );
    }

    #[test]
    fn empty_or_null_root_against_structure_shows_only_the_structure() {
        for empty in [json!({}), json!(null), json!([])] {
            let d = leaf_diff(&empty, &json!({"a": {"b": 1}}));
            assert_eq!(d.len(), 1, "from {empty}");
            assert_eq!((d[0].op, d[0].path.as_str()), (Op::Added, "a.b"));
            let d = leaf_diff(&json!({"a": {"b": 1}}), &empty);
            assert_eq!((d[0].op, d[0].path.as_str()), (Op::Removed, "a.b"));
        }
    }

    #[test]
    fn empty_containers_are_leaves() {
        let d = leaf_diff(&json!({"a": {}}), &json!({"a": []}));
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].op, Op::Changed);
    }

    #[test]
    fn variables_are_grouped_by_name_with_relative_field_paths() {
        let var = |name: &str, default: &str, sensitive: bool| {
            json!({"name": name, "defaultValue": default, "required": true, "hidden": false,
                   "immutable": false, "isSensitive": sensitive, "format": "string"})
        };
        let a = vec![var("cluster_env", "dev", false), var("gone", "", true)];
        let b = vec![
            var("cluster_env", "staging", false),
            var("db_password", "", true),
        ];
        let g = group_variables(&a, &b);
        assert_eq!(g.added.len(), 1);
        assert_eq!(g.added[0].name, "db_password");
        assert!(g.added[0].required && g.added[0].sensitive && !g.added[0].hidden);
        assert_eq!(g.removed.len(), 1);
        assert_eq!(g.removed[0].name, "gone");
        assert_eq!(g.changed.len(), 1);
        assert_eq!(g.changed[0].name, "cluster_env");
        assert_eq!(g.changed[0].fields.len(), 1);
        assert_eq!(g.changed[0].fields[0].path, "defaultValue");
        assert_eq!(g.changed[0].fields[0].segments, vec![json!("defaultValue")]);
        assert_eq!(g.changed[0].fields[0].old, Some(json!("dev")));
        assert_eq!(g.changed[0].fields[0].new, Some(json!("staging")));
        assert_eq!(
            g.added[0].traits(),
            vec!["required", "sensitive"],
            "the words the report prints in parentheses"
        );
    }
}
