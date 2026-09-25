//! `same-release`: whether records in promotion order are one release.

use std::collections::{BTreeMap, HashMap};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::diff::{self, Diff, PackDiff, Status};
use crate::snapshot::{Pack, Snapshot, Tool};

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

/// The outcome for one consecutive pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PairStatus {
    /// Equal roots.
    Identical,
    /// Roots differ only by the `packUid`s Palette mints for each profile's manifest packs: the
    /// same content, under either policy.
    SameContent,
    /// Roots differ only because packs were re-pinned from another registry; accepted by policy.
    RegistryMove,
    /// Not the same release; `reasons` says why.
    Different,
}

impl PairStatus {
    /// True for the statuses that count as the same release.
    pub fn passed(self) -> bool {
        self != PairStatus::Different
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
    /// Why the pair is not the same release, one sentence per difference; empty when it passed.
    /// Under the exact policy a registry move is listed too, so the reader can decide to allow it.
    pub reasons: Vec<String>,
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
    pub generated_at: String,
    /// What the statuses and policies mean.
    pub legend: ChainLegend,
    /// The policy applied to every pair.
    pub policy: Policy,
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
        return "values changed (not parseable as YAML)".to_string();
    }
    if let Some(m) = pd.manifests.first() {
        return format!("manifest {} {:?}", m.name, m.op).to_lowercase();
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

/// The differences between two records that the policy does not accept, plus the number of
/// clean registry re-pins and of packs pinned under another tag name with nothing else changed.
fn pair_reasons(
    d: &Diff,
    a: &Snapshot,
    b: &Snapshot,
    policy: Policy,
) -> (Vec<String>, usize, usize) {
    let pa: HashMap<&str, &Pack> = a.packs.iter().map(|p| (p.key.as_str(), p)).collect();
    let pb: HashMap<&str, &Pack> = b.packs.iter().map(|p| (p.key.as_str(), p)).collect();
    let mut reasons = Vec::new();
    let mut repinned = 0;
    let mut retagged = 0;
    if d.variables.changed {
        reasons.push("variables changed".to_string());
    }
    for pd in &d.packs {
        match pd.status {
            Status::Unchanged => {}
            Status::Added => reasons.push(format!("pack {} added", pd.key)),
            Status::Removed => reasons.push(format!("pack {} removed", pd.key)),
            Status::Changed => {
                let (x, y) = (pa[pd.key.as_str()], pb[pd.key.as_str()]);
                // a manifest pack's packUid is minted for each profile: not part of the release
                let minted = x.registry_uid.is_empty()
                    && y.registry_uid.is_empty()
                    && x.pack_uid != y.pack_uid
                    && (&x.name, &x.layer, &x.kind, &x.tag, &x.version)
                        == (&y.name, &y.layer, &y.kind, &y.tag, &y.version);
                // The type follows the registry (spectro, helm, oci), so a move may change it; a
                // pack from no registry, a manifest, did not move.
                let moved = !x.registry_uid.is_empty()
                    && !y.registry_uid.is_empty()
                    && x.registry_uid != y.registry_uid
                    && x.tag == y.tag
                    && x.version == y.version;
                // the same build under another of the registry's names for its version
                if pd.tag_renamed && !pd.upstream && !pd.authored {
                    retagged += 1;
                }
                if pd.identity && !minted && !pd.tag_renamed {
                    if moved {
                        if !pd.authored {
                            repinned += 1;
                        }
                        if policy == Policy::Exact {
                            reasons.push(format!(
                                "pack {} moved registry {} -> {}",
                                pd.key, x.registry_uid, y.registry_uid
                            ));
                        }
                    } else {
                        reasons.push(format!(
                            "pack {} IDENTITY: {}",
                            pd.key,
                            identity_detail(x, y)
                        ));
                    }
                }
                if pd.authored {
                    reasons.push(format!("pack {} AUTHORED: {}", pd.key, authored_detail(pd)));
                }
                if pd.upstream && (!pd.identity || pd.tag_renamed) {
                    reasons.push(format!(
                        "pack {} UPSTREAM in the same registry: {} -> {}",
                        pd.key, x.hash.upstream_raw, y.hash.upstream_raw
                    ));
                }
            }
        }
    }
    if d.order_changed {
        reasons.push("pack order changed".to_string());
    }
    if reasons.is_empty() && !d.same_content && policy == Policy::Exact {
        reasons.push("roots differ".to_string());
    }
    (reasons, repinned, retagged)
}

/// Compare consecutive records, in the order given, and decide whether they are one release
/// under `policy`. Fewer than two records is an empty chain that passes.
pub fn same_release(records: &[(String, Snapshot)], policy: Policy, generated_at: &str) -> Chain {
    let pairs: Vec<Pair> = records
        .windows(2)
        .map(|w| {
            let ((la, a), (lb, b)) = (&w[0], &w[1]);
            let d = diff::compute(a, b, generated_at, None);
            let (reasons, packs_repinned, packs_retagged) = pair_reasons(&d, a, b, policy);
            let status = if d.same {
                PairStatus::Identical
            } else if d.same_content {
                PairStatus::SameContent
            } else if policy == Policy::RegistryMove && reasons.is_empty() {
                PairStatus::RegistryMove
            } else {
                PairStatus::Different
            };
            Pair {
                a: la.clone(),
                b: lb.clone(),
                status,
                root_a: a.root.clone(),
                root_b: b.root.clone(),
                packs_repinned,
                packs_retagged,
                reasons,
            }
        })
        .collect();
    let passed = pairs.iter().filter(|p| p.status.passed()).count();
    let m = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    };
    Chain {
        schema: CHAIN_SCHEMA.to_string(),
        tool: Tool::current(),
        generated_at: generated_at.to_string(),
        legend: ChainLegend {
            status: m(&[
                (
                    "identical",
                    "The two records have the same root: the same release.",
                ),
                (
                    "same-content",
                    "The roots differ only by the packUid Palette mints for each profile's manifest packs, or by the tag name a pack is pinned under for the same build: the same content, so the same release under either policy.",
                ),
                (
                    "registry-move",
                    "The roots differ only because packs were re-pinned from another registry with the same tag, version, values, and manifests; accepted by the registry-move policy.",
                ),
                (
                    "different",
                    "Not the same release; reasons lists each difference.",
                ),
            ]),
            policy: m(&[
                ("exact", "Only equal roots pass."),
                (
                    "registry-move",
                    "Equal roots pass, and so does a pair whose only difference is packs re-pinned from another registry.",
                ),
            ]),
        },
        policy,
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
