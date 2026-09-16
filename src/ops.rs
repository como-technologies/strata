//! The operations the CLI exposes, without argument parsing, printing, or exit codes.

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::{Palette, PaletteApi, RawCluster, RawProfile};
use crate::diff::{self, Diff, ImpactLevel, PackDiff, Status, Verify, VerifyStatus};
use crate::error::{Error, Result};
use crate::progress::Progress;
use crate::reference::Ref;
use crate::snapshot::{Pack, Snapshot, Tool};

/// Turn a ref into a snapshot. A snapshot file is loaded; anything else is fetched through the
/// [`Palette`] that `connect` returns, so comparing two files never needs credentials.
pub fn snapshot_for<A, F>(
    reference: &Ref,
    connect: F,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot>
where
    A: PaletteApi,
    F: FnOnce() -> Result<Palette<A>>,
{
    match reference {
        Ref::Snapshot(path) => Snapshot::load(path),
        Ref::NameVersion { name, version } => {
            let palette = connect()?;
            let uid = palette.resolve(name, version)?;
            let raw = palette.fetch_profile(&uid, progress)?;
            Ok(
                Snapshot::from_raw(&raw, &reference.to_string(), fetched_at)?
                    .with_project(palette.project_uid()),
            )
        }
        Ref::Uid(uid) => {
            let palette = connect()?;
            let raw = palette.fetch_profile(uid, progress)?;
            Ok(Snapshot::from_raw(&raw, uid, fetched_at)?.with_project(palette.project_uid()))
        }
        Ref::Cluster { uid, profile } => {
            let palette = connect()?;
            let raw = palette.fetch_cluster(uid, progress)?;
            Ok(Snapshot::from_cluster(
                &raw,
                profile.as_deref(),
                &reference.to_string(),
                fetched_at,
            )?
            .with_project(palette.project_uid()))
        }
    }
}

/// Build a snapshot from raw API responses saved on disk. A directory with `cluster.json` is a
/// cluster's; a `reference` of the form `cluster:<uid>/<profile>` then selects the profile.
pub fn snapshot_from_dir(dir: &Path, reference: &str, fetched_at: &str) -> Result<Snapshot> {
    if dir.join(RawCluster::MARKER).is_file() {
        let profile = match Ref::parse_with(reference, |_| false) {
            Ref::Cluster { profile, .. } => profile,
            _ => None,
        };
        let raw = RawCluster::load_dir(dir)?;
        return Snapshot::from_cluster(&raw, profile.as_deref(), reference, fetched_at);
    }
    let raw = RawProfile::load_dir(dir)?;
    Snapshot::from_raw(&raw, reference, fetched_at)
}

/// Compare two snapshots.
pub fn compare(a: &Snapshot, b: &Snapshot, generated_at: &str) -> Diff {
    diff::compute(a, b, generated_at, None)
}

/// The outcome of a verification.
#[derive(Debug)]
pub struct Verification {
    /// The recorded snapshot compared against the live profile, with `verify` filled in.
    pub diff: Diff,
    /// The live profile as fetched.
    pub live: Snapshot,
    /// True when the roots are equal.
    pub unchanged: bool,
}

/// Re-fetch what a snapshot was taken from and compare: the profile, or for a cluster snapshot
/// the cluster, with the same profile selection.
pub fn verify<A: PaletteApi>(
    recorded: &Snapshot,
    palette: &Palette<A>,
    checked_at: &str,
    progress: &Progress,
) -> Result<Verification> {
    let uid = recorded.source.shown_uid();
    if uid.is_empty() {
        return Err(Error::NoUid);
    }
    progress.note(&format!(
        "verifying {} ({uid}) against live; recorded {}",
        recorded.source.title(),
        recorded.source.fetched_at
    ));
    let live = match &recorded.source.cluster {
        Some(c) => Snapshot::from_cluster(
            &palette.fetch_cluster(uid, progress)?,
            c.profile.as_deref(),
            &recorded.source.reference,
            checked_at,
        )?,
        None => Snapshot::from_raw(&palette.fetch_profile(uid, progress)?, uid, checked_at)?,
    }
    .with_project(palette.project_uid());
    let unchanged = recorded.root == live.root;
    let verify = Verify {
        status: if unchanged {
            VerifyStatus::Unchanged
        } else {
            VerifyStatus::Drifted
        },
        uid: uid.to_string(),
        recorded_at: recorded.source.fetched_at.clone(),
        checked_at: checked_at.to_string(),
    };
    let diff = diff::compute(recorded, &live, checked_at, Some(verify));
    Ok(Verification {
        diff,
        live,
        unchanged,
    })
}

/// Schema id of the document `verify` writes for several records.
pub const VERIFY_SUMMARY_SCHEMA: &str = "strata-verify-summary/1";

/// The outcome for one record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RecordStatus {
    /// Same root as the live profile.
    Unchanged,
    /// The live profile differs from the record.
    Drifted,
    /// The record could not be checked.
    Error,
}

/// The outcome over every record; also the exit status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SummaryStatus {
    /// Every record is unchanged: exit 0.
    #[default]
    Unchanged,
    /// At least one record drifted and none failed: exit 1.
    Drifted,
    /// At least one record could not be checked: exit 2.
    Error,
}

/// Plain-language meaning of each record status.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VerifyLegend {
    /// Each `status` value.
    pub status: BTreeMap<String, String>,
}

/// Counts over the records, and the status that decides the exit code.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VerifyCounts {
    /// Records checked, including the ones that failed.
    pub records: usize,
    /// Records whose live profile matches.
    pub unchanged: usize,
    /// Records whose live profile differs.
    pub drifted: usize,
    /// Records that could not be checked.
    pub errors: usize,
    /// `error` if any record failed, else `drifted` if any drifted, else `unchanged`.
    pub status: SummaryStatus,
}

/// One record checked against its live profile.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedRecord {
    /// The record file, as given.
    pub path: String,
    /// What happened.
    pub status: RecordStatus,
    /// Profile name from the record; empty when the file could not be read.
    pub name: String,
    /// Profile version from the record.
    pub version: String,
    /// Profile uid from the record.
    pub uid: String,
    /// The project the record was checked in; empty for tenant scope.
    pub project_uid: String,
    /// When the record was taken.
    pub recorded_at: String,
    /// The root in the record.
    pub root_recorded: String,
    /// The root of the live profile, when it was fetched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_live: Option<String>,
    /// The most severe impact among the drifted packs, for drifted records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_impact: Option<ImpactLevel>,
    /// Packs whose node hash differs, for drifted records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packs_changed: Option<usize>,
    /// The full comparison, for drifted records only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<Diff>,
    /// The error object, for records that could not be checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

impl VerifiedRecord {
    fn failed(path: &Path, recorded: Option<&Snapshot>, e: Error) -> VerifiedRecord {
        let src = recorded.map(|s| &s.source);
        let field = |f: fn(&crate::snapshot::Source) -> &String| {
            src.map(|s| f(s).clone()).unwrap_or_default()
        };
        VerifiedRecord {
            path: path.display().to_string(),
            status: RecordStatus::Error,
            name: field(|s| &s.name),
            version: field(|s| &s.version),
            uid: field(|s| &s.uid),
            project_uid: field(|s| &s.project_uid),
            recorded_at: field(|s| &s.fetched_at),
            root_recorded: recorded.map(|s| s.root.clone()).unwrap_or_default(),
            root_live: None,
            max_impact: None,
            packs_changed: None,
            diff: None,
            error: Some(e.to_json()),
        }
    }
}

/// The document `verify` writes for several records: `strata-verify-summary/1`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VerifySummary {
    /// Always "strata-verify-summary/1"
    pub schema: String,
    /// Which tool wrote the document.
    pub tool: Tool,
    /// When, RFC 3339 UTC.
    pub generated_at: String,
    /// What each status means.
    pub legend: VerifyLegend,
    /// The counts and the overall status.
    pub summary: VerifyCounts,
    /// One entry per record, in the order given.
    pub records: Vec<VerifiedRecord>,
}

impl VerifySummary {
    /// The exit status: 0 all unchanged, 1 any drift, 2 any error.
    pub fn exit_code(&self) -> u8 {
        match self.summary.status {
            SummaryStatus::Unchanged => 0,
            SummaryStatus::Drifted => 1,
            SummaryStatus::Error => 2,
        }
    }
}

/// Verify several records. Each record is checked in the project it was taken in, or in
/// `project` when given; `connect` is called once per distinct scope. A record that cannot be
/// read or checked is an `error` entry, and the other records are still checked.
pub fn verify_many<A, F>(
    paths: &[PathBuf],
    project: Option<&str>,
    mut connect: F,
    checked_at: &str,
    progress: &Progress,
) -> VerifySummary
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    let mut clients: HashMap<Option<String>, Palette<A>> = HashMap::new();
    let mut records = Vec::with_capacity(paths.len());
    for path in paths {
        let recorded = match Snapshot::load(path) {
            Ok(s) => s,
            Err(e) => {
                records.push(VerifiedRecord::failed(path, None, e));
                continue;
            }
        };
        let scope: Option<String> = project
            .map(str::to_string)
            .or_else(|| Some(recorded.source.project_uid.clone()).filter(|p| !p.is_empty()));
        let palette = match clients.entry(scope.clone()) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(v) => match connect(scope.as_deref()) {
                Ok(p) => v.insert(p),
                Err(e) => {
                    records.push(VerifiedRecord::failed(path, Some(&recorded), e));
                    continue;
                }
            },
        };
        records.push(match verify(&recorded, palette, checked_at, progress) {
            Ok(v) => VerifiedRecord {
                path: path.display().to_string(),
                status: if v.unchanged {
                    RecordStatus::Unchanged
                } else {
                    RecordStatus::Drifted
                },
                name: recorded.source.name.clone(),
                version: recorded.source.version.clone(),
                uid: recorded.source.uid.clone(),
                project_uid: scope.clone().unwrap_or_default(),
                recorded_at: recorded.source.fetched_at.clone(),
                root_recorded: recorded.root.clone(),
                root_live: Some(v.live.root.clone()),
                max_impact: (!v.unchanged).then_some(v.diff.summary.max_impact),
                packs_changed: (!v.unchanged).then_some(v.diff.summary.packs_changed),
                diff: (!v.unchanged).then_some(v.diff),
                error: None,
            },
            Err(e) => VerifiedRecord::failed(path, Some(&recorded), e),
        });
    }
    let count = |s: RecordStatus| records.iter().filter(|r| r.status == s).count();
    let (unchanged, drifted, errors) = (
        count(RecordStatus::Unchanged),
        count(RecordStatus::Drifted),
        count(RecordStatus::Error),
    );
    let status = if errors > 0 {
        SummaryStatus::Error
    } else if drifted > 0 {
        SummaryStatus::Drifted
    } else {
        SummaryStatus::Unchanged
    };
    VerifySummary {
        schema: VERIFY_SUMMARY_SCHEMA.to_string(),
        tool: Tool {
            name: crate::TOOL_NAME.to_string(),
            version: crate::TOOL_VERSION.to_string(),
            implementation: Some(crate::TOOL_IMPL.to_string()),
        },
        generated_at: checked_at.to_string(),
        legend: VerifyLegend {
            status: [
                ("unchanged", "The live profile has the same root as the record."),
                ("drifted", "The live profile differs from the record: the published version was edited in place, or the record is stale. The diff is attached."),
                ("error", "The record could not be checked: the file is not a record, or Palette could not be reached. The error is attached."),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        },
        summary: VerifyCounts {
            records: records.len(),
            unchanged,
            drifted,
            errors,
            status,
        },
        records,
    }
}

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
    /// The Merkle root.
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
    /// Always "strata-chain/1"
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
/// clean registry re-pins.
fn pair_reasons(d: &Diff, a: &Snapshot, b: &Snapshot, policy: Policy) -> (Vec<String>, usize) {
    let pa: HashMap<&str, &Pack> = a.packs.iter().map(|p| (p.key.as_str(), p)).collect();
    let pb: HashMap<&str, &Pack> = b.packs.iter().map(|p| (p.key.as_str(), p)).collect();
    let mut reasons = Vec::new();
    let mut repinned = 0;
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
                let moved = x.registry_uid != y.registry_uid
                    && x.tag == y.tag
                    && x.version == y.version
                    && x.kind == y.kind;
                if pd.identity {
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
                if pd.upstream && !pd.identity {
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
    if reasons.is_empty() && !d.same && policy == Policy::Exact {
        reasons.push("roots differ".to_string());
    }
    (reasons, repinned)
}

/// Compare consecutive records, in the order given, and decide whether they are one release
/// under `policy`. Fewer than two records is an empty chain that passes.
pub fn same_release(records: &[(String, Snapshot)], policy: Policy, generated_at: &str) -> Chain {
    let pairs: Vec<Pair> = records
        .windows(2)
        .map(|w| {
            let ((la, a), (lb, b)) = (&w[0], &w[1]);
            let d = diff::compute(a, b, generated_at, None);
            let (reasons, packs_repinned) = pair_reasons(&d, a, b, policy);
            let status = if d.same {
                PairStatus::Identical
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
        tool: Tool {
            name: crate::TOOL_NAME.to_string(),
            version: crate::TOOL_VERSION.to_string(),
            implementation: Some(crate::TOOL_IMPL.to_string()),
        },
        generated_at: generated_at.to_string(),
        legend: ChainLegend {
            status: m(&[
                ("identical", "The two records have the same root: the same release."),
                ("registry-move", "The roots differ only because packs were re-pinned from another registry with the same tag, version, values, and manifests; accepted by the registry-move policy."),
                ("different", "Not the same release; reasons lists each difference."),
            ]),
            policy: m(&[
                ("exact", "Only equal roots pass."),
                ("registry-move", "Equal roots pass, and so does a pair whose only difference is packs re-pinned from another registry."),
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

/// Schema id of the document `consumers` writes.
pub const CONSUMERS_SCHEMA: &str = "strata-consumers/1";

/// The state a consumer shows when its cluster could not be read.
pub const UNKNOWN_STATE: &str = "unknown";

/// The profile version whose consumers are listed.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ConsumedProfile {
    /// Profile uid.
    pub uid: String,
    /// Profile name.
    pub name: String,
    /// Profile version.
    pub version: String,
}

/// One cluster that uses the profile version.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Consumer {
    /// Cluster uid.
    pub uid: String,
    /// Cluster name; empty when neither the profile nor the cluster could name it.
    pub name: String,
    /// The project the cluster is in; empty for tenant scope or when unknown.
    pub project_uid: String,
    /// The cluster's `status.state`, for example `Running`, or `unknown` when it could not be read.
    pub state: String,
    /// Why the cluster could not be read, when it could not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

/// The document `consumers` writes: `strata-consumers/1`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConsumersReport {
    /// Always "strata-consumers/1"
    pub schema: String,
    /// Which tool wrote the document.
    pub tool: Tool,
    /// When, RFC 3339 UTC.
    pub generated_at: String,
    /// The profile version.
    pub profile: ConsumedProfile,
    /// The number of clusters that use it. Exit status 0 when zero, 1 otherwise.
    pub count: usize,
    /// One entry per cluster, in the order Palette lists them.
    pub consumers: Vec<Consumer>,
}

impl ConsumersReport {
    /// The exit status: 0 when nothing uses the profile version, 1 when something does, so a
    /// delete step can branch on it.
    pub fn exit_code(&self) -> u8 {
        u8::from(self.count > 0)
    }
}

/// List the clusters that use a profile version, with each cluster's name, project, and state.
/// The profile comes from a uid, `<name>@<version>`, or a profile record (whose uid and project
/// are used; the list is always fetched live). Names and projects come from the profile's
/// `status.inUseClusters`; each cluster is then read in its own project for its state, with one
/// client per project from `connect`. A cluster that cannot be read is listed with state
/// `unknown` and its error; it still counts.
pub fn consumers<A, F>(
    reference: &Ref,
    mut connect: F,
    generated_at: &str,
    progress: &Progress,
) -> Result<ConsumersReport>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    let refused = || Error::ProfileRefRequired {
        reference: reference.to_string(),
    };
    let (profile_scope, uid_hint) = match reference {
        Ref::Cluster { .. } => return Err(refused()),
        Ref::Snapshot(path) => {
            let recorded = Snapshot::load(path)?;
            if recorded.source.cluster.is_some() {
                return Err(refused());
            }
            if recorded.source.uid.is_empty() {
                return Err(Error::NoUid);
            }
            let scope = Some(recorded.source.project_uid.clone()).filter(|p| !p.is_empty());
            (scope, Some(recorded.source.uid))
        }
        Ref::Uid(uid) => (None, Some(uid.clone())),
        Ref::NameVersion { .. } => (None, None),
    };

    let mut clients: HashMap<Option<String>, Palette<A>> = HashMap::new();
    let home = connect(profile_scope.as_deref())?;
    let uid = match (uid_hint, reference) {
        (Some(uid), _) => uid,
        (None, Ref::NameVersion { name, version }) => home.resolve(name, version)?,
        (None, _) => unreachable!("every other ref carries a uid"),
    };
    progress.note(&format!("listing consumers of profile {uid}"));
    let doc = home.profile_document(&uid)?;
    clients.insert(profile_scope, home);

    let text = |v: &Value, ptr: &str| {
        v.pointer(ptr)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let named: Vec<Value> = doc
        .pointer("/status/inUseClusters")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut uids: Vec<String> = doc
        .pointer("/status/inUseClusterUids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    for n in &named {
        let u = text(n, "/uid");
        if !u.is_empty() && !uids.contains(&u) {
            uids.push(u);
        }
    }

    let mut list = Vec::with_capacity(uids.len());
    for cluster in uids {
        let known = named.iter().find(|n| text(n, "/uid") == cluster);
        let mut c = Consumer {
            name: known.map(|n| text(n, "/name")).unwrap_or_default(),
            project_uid: known.map(|n| text(n, "/projectUid")).unwrap_or_default(),
            state: UNKNOWN_STATE.to_string(),
            error: None,
            uid: cluster,
        };
        let scope = Some(c.project_uid.clone()).filter(|p| !p.is_empty());
        let client = match clients.entry(scope.clone()) {
            Entry::Occupied(e) => Ok(e.into_mut()),
            Entry::Vacant(v) => connect(scope.as_deref()).map(|p| v.insert(p)),
        };
        match client.and_then(|p| p.cluster_document(&c.uid)) {
            Ok(d) => {
                let or_keep = |current: String, found: String| {
                    if found.is_empty() {
                        current
                    } else {
                        found
                    }
                };
                c.name = or_keep(c.name, text(&d, "/metadata/name"));
                c.project_uid =
                    or_keep(c.project_uid, text(&d, "/metadata/annotations/projectUid"));
                c.state = or_keep(c.state, text(&d, "/status/state"));
            }
            Err(e) => c.error = Some(e.to_json()),
        }
        list.push(c);
    }

    Ok(ConsumersReport {
        schema: CONSUMERS_SCHEMA.to_string(),
        tool: Tool {
            name: crate::TOOL_NAME.to_string(),
            version: crate::TOOL_VERSION.to_string(),
            implementation: Some(crate::TOOL_IMPL.to_string()),
        },
        generated_at: generated_at.to_string(),
        profile: ConsumedProfile {
            uid: text(&doc, "/metadata/uid"),
            name: text(&doc, "/metadata/name"),
            version: text(&doc, "/spec/version"),
        },
        count: list.len(),
        consumers: list,
    })
}
