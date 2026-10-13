//! The diff document (schema strata-diff/1): two snapshots compared. Self-describing on
//! purpose: every enum is spelled out, a legend explains the flags and impact levels, and a summary
//! block answers the common questions without walking the packs.

use std::collections::BTreeMap;
use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::canon::canonical_json;
use crate::cluster_template::ClusterTemplateRef;
pub use crate::impact::{Flags, Impact, ImpactLevel, impact, presence_impact};
use crate::json::{is_zero, str_at, text};
use crate::layer::Layer;
use crate::legend::{Described, legend};
pub use crate::repave::PaletteRepaveReason;
use crate::snapshot::{
    ClusterTier, ClusterVariable, Pack, Snapshot, Source, Tier, Tool, UpstreamSource,
};
pub use crate::validation::{PaletteRefusal, PaletteValidation};

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
    /// (`packs[].mintedUidOnly`) and by the tag name of a pack for the same build
    /// (`packs[].tagRenamed`): the same content. `same-release` accepts such a pair.
    #[serde(default)]
    pub same_content: bool,
    /// True when both sides name the same profile uid and version but their roots differ: an edit
    /// in place changed the version, or, between two clusters, one cluster changed. It is false
    /// for a version and a record of its draft.
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
    /// Between two cluster records of one kind that carry tiers: each tier that both carry, and
    /// the labels, with what differs in each. A difference here does not change `same`.
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
    /// Between two records that each hold the digests of their macro values: each macro
    /// whose value differs, or that one side resolves and the other does not. The roots can be
    /// equal: the content names the macro, and Palette gives it another value.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub macros_changed: Vec<String>,
    /// Each pack that names a machine image in the annotation `imageId` on both sides, with two
    /// different images. The root does not cover the annotations, so the roots can be equal.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub image_ids_changed: Vec<ImageIdChange>,
    /// With `compare --repave`: what Palette answers when strata asks whether the change repaves
    /// the cluster side, beside the level of the impact table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette_repave: Option<PaletteRepave>,
    /// With `compare --validate`: what Palette answers when strata asks whether it accepts the
    /// packs of the change on the cluster side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette_validation: Option<PaletteValidation>,
}

impl Diff {
    /// True when the roots are equal, no macro has another value, and no pack names another
    /// machine image. `compare` exits `0` only then.
    pub fn nothing_differs(&self) -> bool {
        self.same && self.macros_changed.is_empty() && self.image_ids_changed.is_empty()
    }
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
        Legend {
            identity: "The pack reference changed: name, layer, type, tag, version, packUid or registryUid. The release points at a different pack build.".into(),
            upstream: "The digest on the pack reference changed: pack content moved under the hood, possibly under the same version. Which field fed it is hash.upstreamSource in the snapshot.".into(),
            authored: "Profile-level configuration changed: the pack's values YAML (compared canonically, so formatting does not count), its attached manifests, or its params.".into(),
            images: "The image list that the values declare changed, pack.content.images: an image added, removed, or at another tag. A detail of AUTHORED. The list holds a tag and not a digest, so an image rebuilt under one tag does not show.".into(),
            charts: "The chart list that the values declare changed, pack.content.charts: a chart added, removed, or at another version. A detail of AUTHORED.".into(),
            impact: legend::<ImpactLevel>(),
            status: legend::<Status>(),
            op: legend::<Op>(),
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
    /// The packs that Palette marks invalid on one side or on the two sides. They do not change
    /// `same`.
    #[serde(default)]
    pub packs_invalid: usize,
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
    /// The tier compared. Between two clusters, `applied` compares `spcInfraHash` alone:
    /// Palette's `spcHash` is its own for each cluster.
    pub tier: Tier,
    /// True when the hashes are equal.
    pub same: bool,
    /// The fields that differ, when the tier differs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diff: Vec<Leaf>,
}

/// A side of a comparison: `a` or `b`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    /// Side A: the base of a comparison, and the record of a verify.
    A,
    /// Side B: the candidate of a comparison, and what Palette holds now in a verify.
    B,
}

/// A pack that a cluster runs at another version than its profile declares.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunningBehind {
    /// The side whose cluster it is.
    pub side: Side,
    /// The pack name.
    pub pack: String,
    /// The version that the profile declares.
    pub declared: String,
    /// The version that the cluster reconciled.
    pub running: String,
}

/// `fields` less each field that strata could not read in the tier `a` or `b`.
///
/// An error of Palette is not a change of the cluster.
fn read_fields(mut fields: Value, a: &ClusterTier, b: &ClusterTier) -> Value {
    if let Some(map) = fields.as_object_mut() {
        for key in a.errors.keys().chain(b.errors.keys()) {
            map.remove(key);
        }
    }
    fields
}

/// The tiers that two cluster records both carry, and their labels, compared.
fn tiers_diff(a: &Snapshot, b: &Snapshot) -> Vec<TierDiff> {
    let (Some(x), Some(y)) = (&a.source.cluster, &b.source.cluster) else {
        return Vec::new();
    };
    // the tiers of a cluster group hold what the group gives its virtual clusters, and not what
    // a cluster runs. A group and a cluster have no tier to compare.
    if x.kind != y.kind || x.tiers.is_empty() || y.tiers.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<TierDiff> = x
        .tiers
        .iter()
        .filter_map(|(name, t)| {
            let u = y.tiers.get(name)?;
            // Palette's spcHash is its own for each cluster, and equal for no two: between two
            // clusters, applied compares spcInfraHash alone
            let (f, g) = match name {
                Tier::Applied if x.uid != y.uid => {
                    let infra = |v: &Value| json!({"spcInfraHash": v.get("spcInfraHash")});
                    (infra(&t.fields), infra(&u.fields))
                }
                // a record from before strata read the shape by cloud type holds fewer fields
                Tier::Shape => crate::shape::comparable(&t.fields, &u.fields),
                // and one from before strata read the role bindings holds their references
                Tier::Policy => crate::policy::comparable(&t.fields, &u.fields),
                _ => (t.fields.clone(), u.fields.clone()),
            };
            let (f, g) = (read_fields(f, t, u), read_fields(g, t, u));
            let same = f == g;
            Some(TierDiff {
                tier: *name,
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
        tier: Tier::Labels,
        same: la == lb,
        diff: leaf_diff(&la, &lb),
    });
    out
}

/// The packs of a cluster record whose reconciled version differs from the declared one.
fn running_behind(side: Side, s: &Snapshot) -> Vec<RunningBehind> {
    let Some(tier) = s
        .source
        .cluster
        .as_ref()
        .and_then(|c| c.tiers.get(&Tier::Running))
    else {
        return Vec::new();
    };
    let running = tier.fields.as_array().cloned().unwrap_or_default();
    s.packs
        .iter()
        .filter_map(|p| {
            let r = running.iter().find(|r| str_at(r, "/name") == p.name)?;
            let version = str_at(r, "/version");
            (!version.is_empty() && !p.version.is_empty() && version != p.version).then(|| {
                RunningBehind {
                    side,
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
            name: text(v, "/name"),
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

impl Described for Op {
    const ALL: &'static [Self] = &[Self::Changed, Self::Added, Self::Removed];

    fn describe(self) -> &'static str {
        match self {
            Self::Changed => "The leaf exists on both sides with different values.",
            Self::Added => "The leaf exists only in B.",
            Self::Removed => "The leaf exists only in A.",
        }
    }
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
    /// The flags that moved, in the order of the legend. A renamed tag is `IDENTITY`, as the
    /// field `identity` says.
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

/// Each key of either side, in key order, one time, with its value on side A and on side B. A
/// side that does not hold the key has `None` there. At least one side holds it. A side that
/// gives a key twice keeps its last value, as a map does.
fn zip_by_key<K: Ord, V>(
    a: impl IntoIterator<Item = (K, V)>,
    b: impl IntoIterator<Item = (K, V)>,
) -> impl ExactSizeIterator<Item = (K, Option<V>, Option<V>)> {
    let mut sides: BTreeMap<K, (Option<V>, Option<V>)> = BTreeMap::new();
    for (k, v) in a {
        sides.entry(k).or_default().0 = Some(v);
    }
    for (k, v) in b {
        sides.entry(k).or_default().1 = Some(v);
    }
    sides.into_iter().map(|(k, (x, y))| (k, x, y))
}

/// The params that differ between two packs, by name.
fn param_changes(a: &Value, b: &Value) -> Vec<ParamChange> {
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
    /// The cluster that strata asked Palette about.
    pub cluster_uid: String,
    /// `isRepaveRequired`: Palette would replace the nodes.
    pub required: bool,
    /// Each reason Palette gives, in its order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<PaletteRepaveReason>,
    /// True when the impact table and Palette say the same: both a repave, or both no repave. A
    /// level of `may-repave` agrees with either answer.
    pub agrees: bool,
    /// The cluster template that Palette answered for, when the cluster is in one. On the route of
    /// such a cluster, Palette compares nothing and answers no repave. It answers on the route of
    /// the template, by profile uid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster_template: Option<ClusterTemplateRef>,
}

impl PaletteRepave {
    /// Read Palette's answer, and set `agrees` from the most severe level of the impact table.
    pub fn from_palette(cluster_uid: &str, answer: &Value, level: ImpactLevel) -> PaletteRepave {
        let required = answer
            .get("isRepaveRequired")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let reasons = PaletteRepaveReason::read_all(answer, "/reasons");
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
            cluster_template: None,
        }
    }
}

/// Whether strata compares the macro values of two records: both records hold the digests.
pub(crate) fn macros_compared(a: &Snapshot, b: &Snapshot) -> bool {
    a.source.macro_values().is_some() && b.source.macro_values().is_some()
}

/// Each macro whose value digest differs between two records, by name: see
/// [`Diff::macros_changed`]. Empty unless `macros_compared` is true: both records hold the
/// digests.
pub fn macros_changed(a: &Snapshot, b: &Snapshot) -> Vec<String> {
    if !macros_compared(a, b) {
        return Vec::new();
    }
    let none = BTreeMap::new();
    let x = a.source.macro_values().unwrap_or(&none);
    let y = b.source.macro_values().unwrap_or(&none);
    zip_by_key(x, y)
        .filter(|(_, p, q)| p != q)
        .map(|(n, _, _)| n.clone())
        .collect()
}

/// One pack whose two sides name another machine image: see [`Diff::image_ids_changed`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImageIdChange {
    /// `<layer>/<name>` of the pack.
    pub key: String,
    /// The `imageId` of side A.
    pub a: String,
    /// The `imageId` of side B.
    pub b: String,
}

impl ImageIdChange {
    /// The sentence that names the pack and its two images: `pack os/ubuntu-aws names image
    /// ami-1 on one side and ami-2 on the other: the root does not cover the annotations`.
    pub(crate) fn sentence(&self) -> String {
        format!(
            "pack {} names image {} on one side and {} on the other: the root does not cover the annotations",
            self.key, self.a, self.b
        )
    }
}

/// Each pack on both sides whose `imageId` differs, in the pack order of A. Strata does not
/// compare a side with no `imageId`. A record written before the annotations holds none, and a
/// pack with none leaves the choice of the image to Palette.
pub fn image_ids_changed(a: &Snapshot, b: &Snapshot) -> Vec<ImageIdChange> {
    let images: BTreeMap<&str, &str> = b
        .packs
        .iter()
        .filter_map(|p| Some((p.key.as_str(), p.image_id()?)))
        .collect();
    a.packs
        .iter()
        .filter_map(|p| {
            let (x, y) = (p.image_id()?, *images.get(p.key.as_str())?);
            (x != y).then(|| ImageIdChange {
                key: p.key.clone(),
                a: x.to_string(),
                b: y.to_string(),
            })
        })
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
    /// host, or the cluster group. It costs no request. Absent when Palette gives no time, when
    /// the object is gone, and for a pack record: the pack listing gives the create time there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    /// For an `unverified` record: the key of each pack with no upstream digest.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upstream_unknown: Vec<String>,
    /// For a record that holds the digests of its macro values: each macro whose value is another
    /// one now. The status is `drifted`, also when the roots are equal.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub macros_changed: Vec<String>,
    /// When the roots are equal: each pack that names another machine image now, in the
    /// annotation `imageId`. The status is `drifted`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub image_ids_changed: Vec<ImageIdChange>,
    /// Each pack that Palette marks invalid now, with its reason: the registry deleted its tag.
    /// It does not change `status`: the content is still the recorded content.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalid_packs: Vec<InvalidPack>,
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

impl Verify {
    /// The verdict of the record `recorded` against `live`, with no drift named yet. The status
    /// is `unchanged` or `drifted`, as `unchanged` says. The verdict holds the two times, and
    /// each pack that Palette marks invalid now. Each other field is empty: `verify` sets what
    /// it found.
    pub(crate) fn new(
        unchanged: bool,
        uid: &str,
        recorded: &Snapshot,
        live: &Snapshot,
        checked_at: &str,
    ) -> Verify {
        Verify {
            status: if unchanged {
                VerifyStatus::Unchanged
            } else {
                VerifyStatus::Drifted
            },
            uid: uid.to_string(),
            recorded_at: recorded.source.fetched_at.clone(),
            checked_at: checked_at.to_string(),
            overwritten_by: None,
            renamed_to: None,
            gone: false,
            deleted_at: None,
            state: None,
            profiles_overwritten: Vec::new(),
            cluster_values_changed: 0,
            who: None,
            last_modified: None,
            upstream_unknown: Vec::new(),
            macros_changed: Vec::new(),
            image_ids_changed: Vec::new(),
            invalid_packs: invalid_packs(live),
            registry_sync: None,
            registry_sync_error: None,
        }
    }
}

/// The status that Palette gives to a sync of a registry that completed.
const SYNC_COMPLETED: &str = "Completed";
/// The status that Palette gives to a sync of a registry that runs.
const SYNC_IN_PROGRESS: &str = "InProgress";
/// The status that Palette gives to a sync of a registry that failed.
const SYNC_FAILED: &str = "Failed";

/// The last sync of a registry: `status.sync` of the registry in Palette, `v1RegistrySyncStatus`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySync {
    /// When the last sync that completed started, as Palette gives it in `lastSyncedTime`. Empty
    /// when no sync completed.
    pub last_synced_time: String,
    /// When the last sync started, also a sync that runs or that failed, as Palette gives it in
    /// `lastRunTime`. Empty when Palette gives no time.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_run_time: String,
    /// The state of the last sync: `Completed`, `InProgress`, or `Failed`, as Palette gives it.
    pub status: String,
    /// What Palette says about the last sync, such as the cause of a failure. Empty when it says
    /// nothing.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
    /// The recovery of a sync that stopped before its end, as Palette gives it in
    /// `syncRecovery`. Absent when Palette gives none, and when the recovery is `Recovered`: then
    /// no sync waits for a recovery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_recovery: Option<SyncRecovery>,
}

impl RegistrySync {
    /// Whether the last sync of the registry completed: the copy of the registry in Palette is
    /// then the copy of the last run.
    pub fn completed(&self) -> bool {
        self.status == SYNC_COMPLETED
    }

    /// Whether a sync of the registry runs now.
    pub fn in_progress(&self) -> bool {
        self.status == SYNC_IN_PROGRESS
    }

    /// Whether the last sync of the registry failed.
    pub fn failed(&self) -> bool {
        self.status == SYNC_FAILED
    }
}

/// The recovery of a sync of a registry that stopped before its end: `v1SyncRecovery`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SyncRecovery {
    /// The state of the recovery: `Interrupted` or `Recovering`, as Palette gives it.
    pub sync_recovery_state: String,
    /// What Palette says about the recovery. Empty when it says nothing.
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
        // no space around a name or a message in the record
        let text = |ptr: &str| str_at(item, ptr).trim().to_string();
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

/// The path of each leaf of `v` that is the text `text`, sorted, in the form of [`leaf_diff`].
/// A document that is the text itself has no path, so it gives none.
pub(crate) fn paths_of_text(v: &Value, text: &str) -> Vec<String> {
    let mut leaves = Leaves::new();
    flatten(v, &mut Vec::new(), &mut leaves);
    leaves
        .into_iter()
        .filter(|(path, (_, x))| !path.is_empty() && x.as_str() == Some(text))
        .map(|(path, _)| path)
        .collect()
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
    let mut out = Vec::new();
    for (path, x, y) in zip_by_key(la, lb) {
        match (x, y) {
            (Some((segments, x)), Some((_, y))) => {
                if x != y {
                    out.push(Leaf {
                        op: Op::Changed,
                        path,
                        segments,
                        old: Some(x),
                        new: Some(y),
                    });
                }
            }
            (Some((segments, x)), None) => out.push(Leaf {
                op: Op::Removed,
                path,
                segments,
                old: Some(x),
                new: None,
            }),
            (None, Some((segments, y))) => out.push(Leaf {
                op: Op::Added,
                path,
                segments,
                old: None,
                new: Some(y),
            }),
            (None, None) => {}
        }
    }
    out
}

/// The masked paths of the two sides of one pack, sorted, each one time.
fn masked_union(x: &Pack, y: &Pack) -> Vec<String> {
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

fn side(p: &Pack) -> PackSide {
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

/// Each variable of a normalized list, with its name. Two variables of one name are one, the
/// last.
fn named(vars: &[Value]) -> impl Iterator<Item = (String, &Value)> {
    vars.iter().map(|v| (text(v, "/name"), v))
}

/// The variables of a normalized list as one document, by name: see [`named`].
fn by_name(vars: &[Value]) -> Value {
    Value::Object(named(vars).map(|(n, v)| (n, v.clone())).collect())
}

/// Group two normalized variable lists by name: added, removed, and changed with the fields that
/// differ. Names are sorted.
pub fn group_variables(a: &[Value], b: &[Value]) -> VariablesByName {
    let mut g = VariablesByName::default();
    for (name, x, y) in zip_by_key(named(a), named(b)) {
        match (x, y) {
            (None, Some(y)) => g.added.push(VariableSummary::from_value(y)),
            (Some(x), None) => g.removed.push(VariableSummary::from_value(x)),
            (Some(x), Some(y)) => {
                let fields = leaf_diff(x, y);
                if !fields.is_empty() {
                    g.changed.push(VariableChange { name, fields });
                }
            }
            (None, None) => {}
        }
    }
    g
}

/// The values two clusters give their variables, matched by profile name and variable name.
fn cluster_values_diff(a: &[ClusterVariable], b: &[ClusterVariable]) -> ClusterVariablesDiff {
    fn keyed(vs: &[ClusterVariable]) -> impl Iterator<Item = ((&str, &str), &ClusterVariable)> {
        vs.iter()
            .map(|v| ((v.profile.as_str(), v.name.as_str()), v))
    }
    let values: Vec<ClusterValueChange> = zip_by_key(keyed(a), keyed(b))
        .filter_map(|((profile, name), x, y)| {
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
                profile: profile.to_string(),
                name: name.to_string(),
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

/// The diff of a pack that one side holds: `added`, with the pack on side B, or removed, with the
/// pack on side A. No hash moved, and the values and the manifests have no change to show.
fn presence_pack_diff(key: &str, pack: &Pack, added: bool) -> PackDiff {
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

/// Compare two snapshots. `verify` is attached when this is the second half of a verification.
pub fn compute(a: &Snapshot, b: &Snapshot, generated_at: &str, verify: Option<Verify>) -> Diff {
    let pa: BTreeMap<&str, &Pack> = a.packs.iter().map(|p| (p.key.as_str(), p)).collect();
    let pb: BTreeMap<&str, &Pack> = b.packs.iter().map(|p| (p.key.as_str(), p)).collect();
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

    let mut summary = Summary::default();
    let sides = zip_by_key(pa, pb);
    let mut packs = Vec::with_capacity(sides.len());
    for (k, x, y) in sides {
        let pd = match (x, y) {
            (None, Some(y)) => {
                summary.packs_added += 1;
                presence_pack_diff(k, y, true)
            }
            (Some(x), None) => {
                summary.packs_removed += 1;
                presence_pack_diff(k, x, false)
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
                        version: x.version != y.version,
                        upstream,
                        authored,
                    },
                );
                if tag_renamed && imp.level == ImpactLevel::None {
                    imp.text = "none (tag renamed, same pack build)".to_string();
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
                    masked: masked_union(x, y),
                    images: !image_changes.is_empty(),
                    charts: !chart_changes.is_empty(),
                    image_changes,
                    chart_changes,
                    param_changes: param_changes(&x.params, &y.params),
                }
            }
            (None, None) => unreachable!("a key comes from one of the sides"),
        };
        if pd.impact.level > summary.max_impact {
            summary.max_impact = pd.impact.level;
        }
        summary.packs_invalid += usize::from(pd.invalid_sides().is_some());
        packs.push(pd);
    }
    packs.sort_by(|p, q| {
        (Layer::parse(&p.layer).rank(), &p.name).cmp(&(Layer::parse(&q.layer).rank(), &q.name))
    });

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
        // a version and its draft differ by what the publish changes, and not by an edit in place
        edited_in_place: a.root != b.root
            && !a.source.uid.is_empty()
            && a.source.uid == b.source.uid
            && a.source.version == b.source.version
            && a.source.is_draft() == b.source.is_draft(),
        overwritten_in_place: overwritten_in_place(a, b),
        macros: {
            let mut names = a.macros();
            names.extend(b.macros());
            names.sort();
            names.dedup();
            names
        },
        macros_changed: macros_changed(a, b),
        image_ids_changed: image_ids_changed(a, b),
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
            let mut v = running_behind(Side::A, a);
            v.extend(running_behind(Side::B, b));
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

    #[test]
    fn zip_by_key_gives_each_key_one_time_in_order_with_the_value_of_each_side() {
        let a = [("b", 1), ("a", 2), ("b", 3)];
        let b = [("c", 4), ("a", 5)];
        let zipped: Vec<_> = zip_by_key(a, b).collect();
        assert_eq!(
            zipped,
            [
                ("a", Some(2), Some(5)),
                ("b", Some(3), None),
                ("c", None, Some(4))
            ],
            "the keys are in order, and a key twice on one side keeps its last value"
        );
        assert_eq!(zip_by_key(a, b).len(), 3);
        assert!(
            zip_by_key(Vec::<(&str, u8)>::new(), Vec::new())
                .next()
                .is_none()
        );
    }

    #[test]
    fn paths_of_text_gives_each_leaf_that_is_the_text_in_the_form_of_leaf_diff() {
        let v = json!({
            "auth": {"password": "********", "user": "fred"},
            "users": [{"name": "barney", "token": "********"}],
            "keys": ["********", "*********"],
            "count": 8
        });
        assert_eq!(
            paths_of_text(&v, "********"),
            ["auth.password", "keys[0]", "users[name=barney].token"]
        );
        assert!(paths_of_text(&json!("********"), "********").is_empty());
        assert!(paths_of_text(&Value::Null, "********").is_empty());
    }

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
