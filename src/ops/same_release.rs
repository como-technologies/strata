//! `same-release`: whether records in promotion order are one release.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::closed_set::impl_closed_set;
use crate::closed_set::legend::{Described, legend};
use crate::diff::{self, ChangeFlag, Diff, ImageIdChange, MaskedPack, PackDiff, Renumber, Status};
use crate::render::changes::{VALUES_UNPARSED, manifest_change};
use crate::render::notes::no_upstream_digest;
use crate::snapshot::{Pack, Snapshot, Tool};
use crate::timestamp::Rfc3339;

/// Schema id of the document `same-release` writes.
pub const CHAIN_SCHEMA: &str = "strata-chain/1";

/// What counts as the same release between two records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Policy {
    /// Equal roots only.
    #[default]
    Exact,
    /// Equal roots, or packs re-pinned from another registry with the same tag, version, type,
    /// values, manifests, and params, and variables unchanged.
    RegistryMove,
}

impl_closed_set!(Policy {
    Exact => "exact",
    RegistryMove => "registry-move",
});

impl Described for Policy {
    fn describe(self) -> &'static str {
        match self {
            Self::Exact => "Only equal roots pass.",
            Self::RegistryMove => {
                "Equal roots pass, and so does a pair whose only difference is packs re-pinned from another registry."
            }
        }
    }
}

/// What `same-release` accepts beside equal roots: the policy, and each value of `--allow`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SameReleaseOptions {
    /// Equal roots only, or a registry move too: `--allow registry-move`.
    pub policy: Policy,
    /// Accept a pack with no upstream digest: `--allow no-digest`. Without it, such a pair is
    /// `unknown-upstream`.
    pub allow_no_digest: bool,
    /// Accept a pack whose version alone changed: `--allow renumber`. See [`Renumber`].
    pub allow_renumber: bool,
}

/// The outcome for one consecutive pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PairStatus {
    /// Equal roots, and no pack names another machine image.
    Identical,
    /// The roots differ only by the `packUid`s that Palette mints, or by the tag name of a pack for
    /// the same build. The content is the same under either policy.
    SameContent,
    /// Roots differ only because packs were re-pinned from another registry; accepted by policy.
    RegistryMove,
    /// The roots differ only because the version of packs changed, with the same values,
    /// manifests, params, and images: see [`Renumber`]. `--allow renumber` accepts it.
    Renumber,
    /// Not the same release; `reasons` says why.
    Different,
    /// The roots would pass, and a pack on either side has no upstream digest, so an equal root
    /// does not show an equal archive. `--allow no-digest` accepts it.
    UnknownUpstream,
}

impl PairStatus {
    /// True for the statuses that count as the same release.
    pub fn passed(self) -> bool {
        !matches!(self, PairStatus::Different | PairStatus::UnknownUpstream)
    }
}

impl_closed_set!(PairStatus {
    Identical => "identical",
    SameContent => "same-content",
    RegistryMove => "registry-move",
    Renumber => "renumber",
    Different => "different",
    UnknownUpstream => "unknown-upstream",
});

impl Described for PairStatus {
    fn describe(self) -> &'static str {
        match self {
            Self::Identical => {
                "The two records have the same root, and no pack names another machine image in imageId: the same release."
            }
            Self::SameContent => {
                "The roots differ only by the packUid that Palette mints, or by the tag name of a pack for the same build. Palette mints the packUid of a manifest pack and of a pack from a registry that it does not sync. The content is the same, so the release is the same under either policy."
            }
            Self::RegistryMove => {
                "The roots differ only because a pack moved to another registry, with the same tag, version, values, and manifests. The registry-move policy accepts this."
            }
            Self::Renumber => {
                "The roots differ only because the version of packs changed, with the same values, manifests, params, and images. --allow renumber accepts this. Strata does not compare the archives: archivesNotCompared names each pack whose archive the pipeline must check."
            }
            Self::Different => {
                "This is not the same release. The field reasons lists each difference."
            }
            Self::UnknownUpstream => {
                "The roots pass, and a pack on either side has no upstream digest, so an equal root does not show an equal archive. --allow no-digest accepts it."
            }
        }
    }
}

/// One record in the chain.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChainRecord {
    /// The ref as given on the command line.
    pub label: String,
    /// Profile name.
    pub name: String,
    /// Profile version.
    pub version: String,
    /// Profile uid.
    pub uid: String,
    /// The project the record was taken in; empty for tenant scope.
    pub project_uid: String,
    /// The root: one sha256 digest of the whole release.
    pub root: String,
}

/// Two consecutive records compared.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Pair {
    /// Label of the first record.
    pub a: String,
    /// Label of the second record.
    pub b: String,
    /// The outcome.
    pub status: PairStatus,
    /// Root of the first record.
    pub root_a: String,
    /// Root of the second record.
    pub root_b: String,
    /// Packs re-pinned from another registry with nothing else changed.
    pub packs_repinned: usize,
    /// Packs pinned under another tag name for the same build, with nothing else changed.
    #[serde(default)]
    pub packs_retagged: usize,
    /// With `--allow renumber`: each pack whose version alone changed, in pack order. The values,
    /// the manifests, the params, and the images stay: see [`Renumber`]. A pair that fails for
    /// another reason lists them too.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub renumbers: Vec<Renumber>,
    /// The key of each pack of `renumbers` whose archive can differ: its digest changed, or Palette
    /// gives none. Strata does not compare the files of the two archives, so the pipeline must
    /// check them. A manifest pack has no archive.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub archives_not_compared: Vec<String>,
    /// Why the pair is not the same release, one sentence per difference; empty when it passed.
    /// Under the exact policy a registry move is listed too, so the reader can decide to allow it.
    pub reasons: Vec<String>,
    /// The key of each pack, on either side, with no upstream digest; also when it is allowed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upstream_unknown: Vec<String>,
    /// The key of each pack whose only difference is a `packUid` that Palette mints. That is a
    /// manifest pack, or a pack from a registry that Palette does not sync. `compare` marks it as
    /// `packs[].mintedUidOnly`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub minted_uid_only: Vec<String>,
    /// Each pack, on either side, with a value that Palette masks. The root covers the mask and
    /// not the secret, so an equal root does not show an equal secret. It does not change
    /// `status`: the mask is what Palette serves.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub masked: Vec<MaskedPack>,
    /// When both records hold the digests of their macro values: each macro whose value differs,
    /// as [`Diff::macros_changed`]. It does not change `status`: the release is the text, and
    /// each project gives the macro its own value.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub macros_changed: Vec<String>,
    /// When a record holds no digests of its macro values: each user macro that the content of
    /// either record uses. Strata does not compare their values.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub macros_not_compared: Vec<String>,
}

/// Counts over the pairs.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ChainCounts {
    /// Consecutive pairs compared: one fewer than the records.
    pub pairs: usize,
    /// Pairs that are the same release under the policy.
    pub passed: usize,
    /// Pairs that are not.
    pub failed: usize,
}

/// Plain-language meaning of each status and policy.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChainLegend {
    /// Each pair `status`.
    pub status: BTreeMap<String, String>,
    /// Each `policy`.
    pub policy: BTreeMap<String, String>,
}

/// The document `same-release` writes: `strata-chain/1`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Chain {
    /// The schema id of this document
    #[schemars(extend("const" = CHAIN_SCHEMA))]
    pub schema: String,
    /// Which tool wrote the document.
    pub tool: Tool,
    /// When, RFC 3339 UTC.
    #[schemars(with = "Rfc3339")]
    pub generated_at: String,
    /// What the statuses and policies mean.
    pub legend: ChainLegend,
    /// The policy applied to every pair.
    pub policy: Policy,
    /// True when a pack with no upstream digest is accepted: `--allow no-digest`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_no_digest: bool,
    /// True when a pack whose version alone changed is accepted: `--allow renumber`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_renumber: bool,
    /// True when every pair passed: the records are one release.
    pub same: bool,
    /// The counts.
    pub summary: ChainCounts,
    /// The records, in the order given.
    pub records: Vec<ChainRecord>,
    /// Consecutive pairs, in order.
    pub pairs: Vec<Pair>,
}

impl Chain {
    /// The exit status: 0 when every pair passed, 1 otherwise.
    pub fn exit_code(&self) -> u8 {
        u8::from(!self.same)
    }
}

/// The first changed leaf, manifest, or params note of a pack with `AUTHORED`.
fn authored_detail(pd: &PackDiff) -> String {
    if let Some(first) = pd.values.diff.first() {
        let more = pd.values.diff.len() - 1;
        return if more > 0 {
            format!("{} (+{more} more)", first.path)
        } else {
            first.path.clone()
        };
    }
    if pd.values.changed && !pd.values.parseable {
        return VALUES_UNPARSED.to_string();
    }
    if let Some(m) = pd.manifests.first() {
        return manifest_change(m, str::to_string);
    }
    if pd.params_changed {
        return "params changed".to_string();
    }
    "authored hash differs".to_string()
}

/// Which identity field moved, the most telling first.
fn identity_detail(x: &Pack, y: &Pack) -> String {
    for (field, a, b) in [
        ("version", &x.version, &y.version),
        ("tag", &x.tag, &y.tag),
        ("type", &x.kind, &y.kind),
        ("registryUid", &x.registry_uid, &y.registry_uid),
        ("packUid", &x.pack_uid, &y.pack_uid),
    ] {
        if a != b {
            return format!("{field} {a} -> {b}");
        }
    }
    "identity hash differs".to_string()
}

/// What the difference between two records says about their release.
#[derive(Default)]
struct PairReasons {
    /// The differences that the options do not accept.
    reasons: Vec<String>,
    /// The packs that a clean registry re-pin moved.
    repinned: usize,
    /// The packs pinned under another tag name with nothing else changed.
    retagged: usize,
    /// The packs whose version alone changed, when `--allow renumber` accepts them.
    renumbers: Vec<Renumber>,
    /// The key of each accepted renumber whose archive can differ.
    archives_not_compared: Vec<String>,
}

impl PairReasons {
    /// Add the reasons and the counts for the changed pack `pd`. `x` and `y` are the pack in the
    /// first record and in the second record.
    fn changed_pack(&mut self, pd: &PackDiff, x: &Pack, y: &Pack, options: SameReleaseOptions) {
        // a packUid that Palette mints is not part of the release
        let minted = diff::pack::minted_uid_pair(x, y);
        // The type follows the registry (spectro, helm, oci), so a move may change it; a
        // pack from no registry, a manifest, did not move.
        let moved = !x.registry_uid.is_empty()
            && !y.registry_uid.is_empty()
            && x.registry_uid != y.registry_uid
            && x.tag == y.tag
            && x.version == y.version;
        // the same build under another of the registry's names for its version
        if pd.tag_renamed && !pd.upstream && !pd.authored {
            self.retagged += 1;
        }
        if pd.identity && !minted && !pd.tag_renamed {
            if moved {
                if !pd.authored {
                    self.repinned += 1;
                }
                if options.policy == Policy::Exact {
                    self.reasons.push(format!(
                        "pack {} moved registry {} -> {}",
                        pd.key, x.registry_uid, y.registry_uid
                    ));
                }
            } else if options.allow_renumber && diff::renumber::renumbered(x, y) {
                self.renumbered(pd, x, y);
            } else {
                self.reasons.push(format!(
                    "pack {} {}: {}",
                    pd.key,
                    ChangeFlag::Identity,
                    identity_detail(x, y)
                ));
            }
        }
        if pd.authored {
            self.reasons.push(format!(
                "pack {} {}: {}",
                pd.key,
                ChangeFlag::Authored,
                authored_detail(pd)
            ));
        }
        if pd.upstream && (!pd.identity || pd.tag_renamed) {
            self.reasons.push(format!(
                "pack {} {} in the same registry: {} -> {}",
                pd.key,
                ChangeFlag::Upstream,
                x.hash.upstream_raw,
                y.hash.upstream_raw
            ));
        }
    }

    /// Accept the renumber of the pack `pd`, from `x` in the first record to `y` in the second.
    /// The versions can change their numbers: the user asks for the renumber.
    ///
    /// The new build of a pack has a new digest, because its `pack.json` holds the new version.
    /// The digest alone cannot show that only that file changed, and strata reads no archive. So a
    /// renumber whose digest changed, or that has no digest, keeps the archive to the pipeline.
    fn renumbered(&mut self, pd: &PackDiff, x: &Pack, y: &Pack) {
        self.renumbers.push(Renumber::of(x, y));
        if pd.upstream || pd.upstream_unknown {
            self.archives_not_compared.push(pd.key.clone());
        }
    }
}

/// The differences between the records `a` and `b` that `options` does not accept. It also counts
/// the clean registry re-pins and the packs pinned under another tag name with nothing else
/// changed, and it lists each accepted renumber. `d` is their comparison.
fn pair_reasons(d: &Diff, a: &Snapshot, b: &Snapshot, options: SameReleaseOptions) -> PairReasons {
    let (pa, pb) = (a.packs_by_key(), b.packs_by_key());
    let mut out = PairReasons::default();
    if d.variables.changed {
        out.reasons.push("variables changed".to_string());
    }
    for pd in &d.packs {
        match pd.status {
            Status::Unchanged => {}
            Status::Added => out.reasons.push(format!("pack {} added", pd.key)),
            Status::Removed => out.reasons.push(format!("pack {} removed", pd.key)),
            Status::Changed => {
                out.changed_pack(pd, pa[pd.key.as_str()], pb[pd.key.as_str()], options);
            }
        }
    }
    if d.order_changed {
        out.reasons.push("pack order changed".to_string());
    }
    out.reasons
        .extend(d.image_ids_changed.iter().map(ImageIdChange::sentence));
    if out.reasons.is_empty()
        && out.renumbers.is_empty()
        && !d.same_content
        && options.policy == Policy::Exact
    {
        out.reasons.push("roots differ".to_string());
    }
    out
}

/// The verdict on two consecutive records `(label, record)`, under `options`.
fn build_pair(
    (la, a): &(String, Snapshot),
    (lb, b): &(String, Snapshot),
    options: SameReleaseOptions,
    generated_at: &str,
) -> Pair {
    let d = diff::compute(a, b, generated_at, None);
    let PairReasons {
        mut reasons,
        repinned: packs_repinned,
        retagged: packs_retagged,
        renumbers,
        archives_not_compared,
    } = pair_reasons(&d, a, b, options);
    // The root does not cover the annotations, so two records with one root can boot their nodes
    // from two machine images.
    let mut status = if !d.image_ids_changed.is_empty() {
        PairStatus::Different
    } else if d.same {
        PairStatus::Identical
    } else if d.same_content {
        PairStatus::SameContent
    } else if reasons.is_empty() && !renumbers.is_empty() {
        PairStatus::Renumber
    } else if options.policy == Policy::RegistryMove && reasons.is_empty() {
        PairStatus::RegistryMove
    } else {
        PairStatus::Different
    };
    if !d.upstream_unknown.is_empty() && !options.allow_no_digest {
        reasons.push(format!(
            "{}, so an equal root does not show an equal archive; --allow no-digest accepts it",
            no_upstream_digest(&d.upstream_unknown)
        ));
        if status.passed() {
            status = PairStatus::UnknownUpstream;
        }
    }
    Pair {
        a: la.clone(),
        b: lb.clone(),
        status,
        root_a: a.root.clone(),
        root_b: b.root.clone(),
        packs_repinned,
        packs_retagged,
        renumbers,
        archives_not_compared,
        reasons,
        minted_uid_only: d
            .packs
            .iter()
            .filter(|p| p.minted_uid_only)
            .map(|p| p.key.clone())
            .collect(),
        masked: d.masked_packs(),
        upstream_unknown: d.upstream_unknown,
        macros_changed: d.macros_changed,
        macros_not_compared: if diff::macros_compared(a, b) {
            Vec::new()
        } else {
            d.macros
        },
    }
}

/// Compare consecutive records, in the order given, and decide whether they are one release
/// under `policy`. Fewer than two records is an empty chain that passes.
///
/// Each of `records` is the path of a record, as the report names it, and the record itself.
/// `generated_at` is the time of the report, in RFC 3339.
pub fn same_release(records: &[(String, Snapshot)], policy: Policy, generated_at: &str) -> Chain {
    let options = SameReleaseOptions {
        policy,
        ..SameReleaseOptions::default()
    };
    same_release_with(records, options, generated_at)
}

/// [`same_release`] with each value of `--allow`: see [`SameReleaseOptions`].
///
/// With `allow_no_digest`, a pair that holds a pack with no upstream digest can pass. Without it,
/// such a pair is `unknown-upstream`. A move from a registry that gives a digest to one that gives
/// none loses what the root can show. With `allow_renumber`, a pair whose packs differ only by
/// their versions is `renumber`. `generated_at` is the time of the report, in RFC 3339.
pub fn same_release_with(
    records: &[(String, Snapshot)],
    options: SameReleaseOptions,
    generated_at: &str,
) -> Chain {
    let pairs: Vec<Pair> = records
        .windows(2)
        .map(|w| build_pair(&w[0], &w[1], options, generated_at))
        .collect();
    let passed = pairs.iter().filter(|p| p.status.passed()).count();
    Chain {
        schema: CHAIN_SCHEMA.to_string(),
        tool: Tool::current(),
        generated_at: generated_at.to_string(),
        legend: ChainLegend {
            status: legend::<PairStatus>(),
            policy: legend::<Policy>(),
        },
        policy: options.policy,
        allow_no_digest: options.allow_no_digest,
        allow_renumber: options.allow_renumber,
        same: passed == pairs.len(),
        summary: ChainCounts {
            pairs: pairs.len(),
            passed,
            failed: pairs.len() - passed,
        },
        records: records
            .iter()
            .map(|(label, s)| ChainRecord {
                label: label.clone(),
                name: s.source.name.clone(),
                version: s.source.version.clone(),
                uid: s.source.uid.clone(),
                project_uid: s.source.project_uid.clone(),
                root: s.root.clone(),
            })
            .collect(),
        pairs,
    }
}
