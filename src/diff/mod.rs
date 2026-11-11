//! The diff document (schema strata-diff/1): two snapshots compared.
//!
//! The document describes itself. It names each value of each enum in full. A legend explains the
//! flags and the impact levels. A summary block answers the common questions without a walk of the
//! packs.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::closed_set::legend::legend;
use crate::json::flag;
use crate::model::cluster_template::ClusterTemplateRef;
pub use crate::model::repave::PaletteRepaveReason;
pub use crate::model::validation::{PaletteRefusal, PaletteValidation};
use crate::snapshot::{Snapshot, Source, Tool};
use crate::timestamp::Rfc3339;
use cluster::{cluster_values_diff, overwritten_in_place, running_behind, tiers_diff};
pub use impact::{Flags, Impact, ImpactLevel, impact, presence_impact};
use pack::pack_diffs;

mod cluster;
pub mod impact;
pub(crate) mod leaf;
pub(crate) mod pack;
pub(crate) mod renumber;
pub(crate) mod unified;
mod variables;
mod verify;

pub use cluster::{
    ClusterValueChange, ClusterVariablesDiff, OverwrittenProfile, RunningBehind, Side, TierDiff,
};
pub use leaf::{Leaf, Op, Segment, leaf_diff};
pub use pack::{
    ArtifactChange, ChangeFlag, ManifestChange, MaskedPack, PackDiff, PackSide, ParamChange,
    Status, ValuesDiff, VersionMove,
};
pub use renumber::Renumber;
pub use variables::{
    VariableChange, VariableSummary, VariablesByName, VariablesDiff, group_variables,
};
pub(crate) use verify::invalid_packs;
pub use verify::{
    AuditEntry, AuditResource, ClusterProfileOverwritten, InvalidPack, RegistrySync, SyncRecovery,
    Verify, VerifyStatus,
};

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
    #[schemars(with = "Rfc3339")]
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
    /// Which way the version of the profile moves from side A to side B. Absent when a side has
    /// no version, and when the two versions have the same precedence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_move: Option<VersionMove>,
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
        let required = flag(answer, "/isRepaveRequired");
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

/// Each pack on both sides whose `imageId` differs, in the pack order of A.
///
/// Strata does not compare a side with no `imageId`. A record written before the annotations holds
/// none, and a pack with none leaves the choice of the image to Palette.
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

/// Each key of either side, in key order, one time, with its value on side A and on side B.
///
/// A side that does not hold the key has `None` there. At least one side holds it. A side that
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

impl Summary {
    /// The counts of `packs`, with the answer of the comparison of the variables.
    fn of(packs: &[PackDiff], variables_changed: bool) -> Summary {
        let count = |keep: &dyn Fn(&PackDiff) -> bool| packs.iter().filter(|p| keep(p)).count();
        let with_status = |status: Status| count(&|p| p.status == status);
        Summary {
            packs_added: with_status(Status::Added),
            packs_removed: with_status(Status::Removed),
            packs_changed: with_status(Status::Changed),
            packs_unchanged: with_status(Status::Unchanged),
            identity_changes: count(&|p| p.identity),
            upstream_changes: count(&|p| p.upstream),
            authored_changes: count(&|p| p.authored),
            variables_changed,
            images_changed: packs.iter().map(|p| p.image_changes.len()).sum(),
            charts_changed: packs.iter().map(|p| p.chart_changes.len()).sum(),
            packs_invalid: count(&|p| p.invalid_sides().is_some()),
            max_impact: packs
                .iter()
                .map(|p| p.impact.level)
                .max()
                .unwrap_or_default(),
        }
    }
}

/// The key of each pack of `own` that `other` holds too, in the order of `own`.
fn shared_keys<'a>(own: &'a Snapshot, other: &Snapshot) -> Vec<&'a str> {
    let held: BTreeSet<&str> = other.packs.iter().map(|p| p.key.as_str()).collect();
    own.packs
        .iter()
        .map(|p| p.key.as_str())
        .filter(|k| held.contains(k))
        .collect()
}

/// True when the packs that both records hold are in another order on side B.
fn pack_order_changed(a: &Snapshot, b: &Snapshot) -> bool {
    shared_keys(a, b) != shared_keys(b, a)
}

/// True when the records hold the same content, though their roots can differ. No variable moved,
/// the packs keep their order, and each pack is unchanged. A pack can differ only by a uid that
/// Palette mints or by a renamed tag.
fn no_content_moved(variables_changed: bool, order_changed: bool, packs: &[PackDiff]) -> bool {
    !variables_changed
        && !order_changed
        && packs.iter().all(|p| {
            p.status == Status::Unchanged
                || p.minted_uid_only
                || (p.tag_renamed && !p.upstream && !p.authored)
        })
}

/// The names of the macros that either record uses, sorted, one time each.
fn macros_of_both(a: &Snapshot, b: &Snapshot) -> Vec<String> {
    let mut names = a.macros();
    names.extend(b.macros());
    names.sort();
    names.dedup();
    names
}

/// True when the two records are one profile version whose content changed in place: same uid,
/// same version, same kind of template.
///
/// A version and its draft differ by what the publish changes, and not by an edit in place.
fn edited_in_place(a: &Snapshot, b: &Snapshot) -> bool {
    a.root != b.root
        && !a.source.uid.is_empty()
        && a.source.uid == b.source.uid
        && a.source.version == b.source.version
        && a.source.is_draft() == b.source.is_draft()
}

/// The comparison of the values of two clusters. An edge host and a cluster group give no values,
/// so only two clusters compare.
fn cluster_values_between(a: &Snapshot, b: &Snapshot) -> Option<ClusterVariablesDiff> {
    match (&a.source.cluster, &b.source.cluster) {
        (Some(x), Some(y)) if x.kind.is_cluster() && y.kind.is_cluster() => {
            Some(cluster_values_diff(&x.variables, &y.variables))
        }
        _ => None,
    }
}

/// Compare two snapshots. `verify` is attached when this is the second half of a verification.
pub fn compute(a: &Snapshot, b: &Snapshot, generated_at: &str, verify: Option<Verify>) -> Diff {
    let packs = pack_diffs(a, b);
    let variables = VariablesDiff::between(a, b);
    let order_changed = pack_order_changed(a, b);
    let same = a.root == b.root;
    Diff {
        schema: DIFF_SCHEMA.to_string(),
        tool: Tool::current(),
        generated_at: generated_at.to_string(),
        legend: Legend::standard(),
        a: a.source.clone(),
        b: b.source.clone(),
        same,
        same_content: same || no_content_moved(variables.changed, order_changed, &packs),
        edited_in_place: edited_in_place(a, b),
        overwritten_in_place: overwritten_in_place(a, b),
        macros: macros_of_both(a, b),
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
        summary: Summary::of(&packs, variables.changed),
        variables,
        cluster_variables: cluster_values_between(a, b),
        cluster_tiers: tiers_diff(a, b),
        running_behind: {
            let mut v = running_behind(Side::A, a);
            v.extend(running_behind(Side::B, b));
            v
        },
        version_move: VersionMove::of(&a.source.version, &b.source.version),
        order_changed,
        packs,
        verify,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
