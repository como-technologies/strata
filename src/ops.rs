//! The operations the CLI exposes, without argument parsing, printing, or exit codes.

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::{Palette, PaletteApi, RawCluster, RawProfile};
use crate::candidate::LoadedCandidate;
use crate::diff::{
    self, ClusterProfileOverwritten, Diff, ImpactLevel, PackDiff, Status, Verify, VerifyStatus,
};
use crate::error::{Error, Result};
use crate::progress::Progress;
use crate::reference::Ref;
use crate::snapshot::{AppliedProfile, Pack, Snapshot, Tool};
use crate::version;

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
        Ref::Pack {
            registry,
            name,
            version,
        } => {
            let palette = connect()?;
            Ok(crate::candidate::pack_snapshot(
                &palette,
                registry.as_deref(),
                name,
                version,
                &reference.to_string(),
                fetched_at,
                progress,
            )?
            .with_project(palette.project_uid()))
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

/// Build a snapshot of a candidate file: read and check it (no network), then resolve its packs
/// through the [`Palette`] that `connect` returns. The snapshot has `source.template` `candidate`
/// and no uid.
pub fn snapshot_from_candidate<A, F>(
    path: &Path,
    connect: F,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot>
where
    A: PaletteApi,
    F: FnOnce() -> Result<Palette<A>>,
{
    snapshot_from_loaded(
        &crate::candidate::load(path)?,
        connect,
        fetched_at,
        progress,
    )
}

/// Build a snapshot of a candidate that is checked and in memory: resolve its packs through the
/// [`Palette`] that `connect` returns. [`snapshot_from_candidate`] reads a file and then does
/// this; a caller with a [`Candidate`](crate::candidate::Candidate) value gives it to
/// [`candidate::check`](crate::candidate::check) and starts here.
pub fn snapshot_from_loaded<A, F>(
    loaded: &LoadedCandidate,
    connect: F,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot>
where
    A: PaletteApi,
    F: FnOnce() -> Result<Palette<A>>,
{
    let palette = connect()?;
    let raw = crate::candidate::resolve(loaded, &palette, progress)?;
    let mut snap = Snapshot::from_raw(&raw, &loaded.path.display().to_string(), fetched_at)?;
    snap.source.template = crate::candidate::TEMPLATE.to_string();
    // Palette does not hold a candidate, so no request asked which clusters use it
    snap.source.consumers = None;
    Ok(snap.with_project(palette.project_uid()))
}

/// Build a snapshot from raw API responses saved on disk, in the project they were fetched in. A
/// directory with `cluster.json` is a cluster's; a `reference` of the form
/// `cluster:<uid>/<profile>` then selects the profile.
pub fn snapshot_from_dir(dir: &Path, reference: &str, fetched_at: &str) -> Result<Snapshot> {
    if dir.join(RawCluster::MARKER).is_file() {
        let profile = match Ref::parse_with(reference, |_| false) {
            Ref::Cluster { profile, .. } => profile,
            _ => None,
        };
        let raw = RawCluster::load_dir(dir)?;
        // the responses were read when they were saved; this run read a directory
        let at = raw
            .fetched_at
            .clone()
            .unwrap_or_else(|| fetched_at.to_string());
        return Ok(
            Snapshot::from_cluster(&raw, profile.as_deref(), reference, &at)?
                .with_project(raw.project_uid.as_deref()),
        );
    }
    let raw = RawProfile::load_dir(dir)?;
    let fetched_at = &raw
        .fetched_at
        .clone()
        .unwrap_or_else(|| fetched_at.to_string());
    Ok(Snapshot::from_raw(&raw, reference, fetched_at)?.with_project(raw.project_uid.as_deref()))
}

/// Write `text` to `path`, creating the directory it names when that directory does not exist.
/// Every file strata writes goes through here, so `-o releases/1.2.0/dev.json` needs no `mkdir`.
pub fn write_file(path: &Path, text: &str) -> Result<()> {
    let io = |source| Error::Io {
        path: path.to_path_buf(),
        source,
    };
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    std::fs::write(path, text).map_err(io)
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
/// Each profile a cluster runs whose uid no longer names the version the cluster's copy names,
/// read from the profile documents: one request per profile. A profile that is gone is listed with
/// no version; any other error stops the check.
fn profiles_overwritten<A: PaletteApi>(
    running: &[crate::snapshot::AppliedProfile],
    palette: &Palette<A>,
) -> Result<Vec<ClusterProfileOverwritten>> {
    let mut out = Vec::new();
    for p in running {
        let now = match palette.profile_document(&p.uid) {
            Ok(doc) => doc
                .pointer("/spec/version")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            Err(Error::Api { status: 404, .. }) => String::new(),
            Err(e) => return Err(e),
        };
        if now != p.version {
            out.push(ClusterProfileOverwritten {
                uid: p.uid.clone(),
                name: p.name.clone(),
                cluster_version: p.version.clone(),
                profile_version: now,
            });
        }
    }
    Ok(out)
}

/// the cluster, with the same profile selection.
pub fn verify<A: PaletteApi>(
    recorded: &Snapshot,
    palette: &Palette<A>,
    checked_at: &str,
    progress: &Progress,
) -> Result<Verification> {
    // a pack record names a registry pack, not a profile or a cluster
    if let Some(p) = &recorded.source.pack {
        progress.note(&format!(
            "verifying {} against its registry; recorded {}",
            recorded.source.title(),
            recorded.source.fetched_at
        ));
        let live = crate::candidate::pack_snapshot(
            palette,
            Some(&p.registry_uid),
            &p.name,
            &p.version,
            &recorded.source.reference,
            checked_at,
            progress,
        )?
        .with_project(palette.project_uid());
        let unchanged = recorded.root == live.root;
        let verify = Verify {
            status: if unchanged {
                VerifyStatus::Unchanged
            } else {
                VerifyStatus::Drifted
            },
            uid: p.pack_uid.clone(),
            recorded_at: recorded.source.fetched_at.clone(),
            checked_at: checked_at.to_string(),
            overwritten_by: None,
            profiles_overwritten: Vec::new(),
            cluster_values_changed: 0,
        };
        let diff = diff::compute(recorded, &live, checked_at, Some(verify));
        return Ok(Verification {
            diff,
            live,
            unchanged,
        });
    }
    let uid = recorded.source.shown_uid();
    if uid.is_empty() {
        return Err(Error::NoUid);
    }
    progress.note(&format!(
        "verifying {} (uid {uid}) against live; recorded {}",
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
    let overwritten_by = (recorded.source.cluster.is_none()
        && !live.source.version.is_empty()
        && (&live.source.name, &live.source.version)
            != (&recorded.source.name, &recorded.source.version))
        .then(|| format!("{}@{}", live.source.name, live.source.version));
    let profiles_overwritten = match &live.source.cluster {
        Some(c) => profiles_overwritten(&c.profiles, palette)?,
        None => Vec::new(),
    };
    let unchanged =
        recorded.root == live.root && overwritten_by.is_none() && profiles_overwritten.is_empty();
    let verify = Verify {
        status: if unchanged {
            VerifyStatus::Unchanged
        } else {
            VerifyStatus::Drifted
        },
        uid: uid.to_string(),
        recorded_at: recorded.source.fetched_at.clone(),
        checked_at: checked_at.to_string(),
        overwritten_by,
        profiles_overwritten,
        cluster_values_changed: 0,
    };
    let mut diff = diff::compute(recorded, &live, checked_at, Some(verify));
    let values_changed = diff
        .cluster_variables
        .as_ref()
        .map_or(0, |c| c.values.len());
    if let Some(v) = diff.verify.as_mut() {
        v.cluster_values_changed = values_changed;
    }
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
    /// For a cluster record, the values the cluster gives its variables that differ from the
    /// record, whatever the status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster_variables: Option<crate::diff::ClusterVariablesDiff>,
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
            cluster_variables: None,
            error: Some(e.to_json()),
        }
    }
}

/// The document `verify` writes for several records: `strata-verify-summary/1`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VerifySummary {
    /// The schema id of this document
    #[schemars(extend("const" = VERIFY_SUMMARY_SCHEMA))]
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
                cluster_variables: v.diff.cluster_variables.clone().filter(|c| c.changed),
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
        tool: Tool::current(),
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

/// The string at `ptr` in `v`, or empty.
fn text(v: &Value, ptr: &str) -> String {
    v.pointer(ptr)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// The uids of the clusters a profile document says use it: `status.inUseClusterUids`, then any
/// uid that only `status.inUseClusters` names, in order and without repeats.
fn consumer_uids(doc: &Value) -> Vec<String> {
    let mut uids: Vec<String> = doc
        .pointer("/status/inUseClusterUids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    let named = doc
        .pointer("/status/inUseClusters")
        .and_then(Value::as_array)
        .into_iter()
        .flatten();
    for n in named {
        let u = text(n, "/uid");
        if !u.is_empty() && !uids.contains(&u) {
            uids.push(u);
        }
    }
    uids
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
    /// That project's name. A name is a convenience, so it is empty when the project listing
    /// could not be read, and the reports fall back to the uid.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub project_name: String,
    /// The cluster's `status.state`, for example `Running`, or `unknown` when it could not be read.
    pub state: String,
    /// The version the cluster's own copy of the profile names, from the cluster document; empty
    /// when the cluster could not be read. It differs from `profile.version` when the version the
    /// cluster runs was overwritten in place, by a legacy version bump or a rename.
    #[serde(default)]
    pub version: String,
    /// Why the cluster could not be read, when it could not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
    /// True when the consumer list of the profile did not name this cluster, and the cluster
    /// document did: the second reading found it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unlisted: bool,
}

/// The second reading of `consumers`: the clusters the key can list, each read for the profiles
/// it applies. Palette computes the consumer list of a profile when it is asked, in the projects
/// the key can read, and gives an empty list on an error too. The second reading finds a consumer
/// that the list omits.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClustersRead {
    /// Cluster documents read: each cluster the listing names and the consumer list did not.
    pub read: usize,
    /// Clusters the listing names that could not be read. Each one can be a consumer.
    pub unread: usize,
    /// Why the cluster listing could not be read, when it could not. Then `read` is 0, and the
    /// consumer list of the profile stands alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

/// The document `consumers` writes: `strata-consumers/1`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConsumersReport {
    /// The schema id of this document
    #[schemars(extend("const" = CONSUMERS_SCHEMA))]
    pub schema: String,
    /// Which tool wrote the document.
    pub tool: Tool,
    /// When, RFC 3339 UTC.
    pub generated_at: String,
    /// The profile version.
    pub profile: ConsumedProfile,
    /// The number of clusters that use it. Exit status 0 when zero, 1 otherwise.
    pub count: usize,
    /// The consumers whose copy names another version than `profile.version`: the version they run
    /// was overwritten in place.
    #[serde(default)]
    pub overwritten: usize,
    /// One entry per cluster: those Palette lists, in its order, then those the second reading
    /// found.
    pub consumers: Vec<Consumer>,
    /// The second reading: the clusters read for the profiles they apply.
    #[serde(default)]
    pub clusters_read: ClustersRead,
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
        Ref::Cluster { .. } | Ref::Pack { .. } => return Err(refused()),
        Ref::Snapshot(path) => {
            let recorded = Snapshot::load(path)?;
            // a pack record is not a profile version either, and NoUid below would name `verify`,
            // a command that is not running
            if recorded.source.cluster.is_some() || recorded.source.pack.is_some() {
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

    let mut clients: HashMap<String, Palette<A>> = HashMap::new();
    let home = connect(profile_scope.as_deref())?;
    let uid = match (uid_hint, reference) {
        (Some(uid), _) => uid,
        (None, Ref::NameVersion { name, version }) => home.resolve(name, version)?,
        (None, _) => unreachable!("every other ref carries a uid"),
    };
    progress.note(&format!("listing consumers of profile {uid}"));
    let doc = home.profile_document(&uid)?;

    let named: Vec<Value> = doc
        .pointer("/status/inUseClusters")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let uids = consumer_uids(&doc);

    let mut list = Vec::with_capacity(uids.len());
    for cluster in uids {
        let known = named.iter().find(|n| text(n, "/uid") == cluster);
        let mut c = Consumer {
            name: known.map(|n| text(n, "/name")).unwrap_or_default(),
            project_uid: known.map(|n| text(n, "/projectUid")).unwrap_or_default(),
            project_name: String::new(),
            state: UNKNOWN_STATE.to_string(),
            version: String::new(),
            error: None,
            unlisted: false,
            uid: cluster,
        };
        let doc = client_for(&mut clients, &home, &c.project_uid, &mut connect)
            .and_then(|p| p.cluster_document(&c.uid));
        match doc {
            Ok(d) => {
                let or_keep = |current: String, found: String| {
                    if found.is_empty() { current } else { found }
                };
                c.name = or_keep(c.name, text(&d, "/metadata/name"));
                c.project_uid =
                    or_keep(c.project_uid, text(&d, "/metadata/annotations/projectUid"));
                c.state = or_keep(c.state, text(&d, "/status/state"));
                c.version = d
                    .pointer("/spec/clusterProfileTemplates")
                    .and_then(Value::as_array)
                    .and_then(|ts| ts.iter().find(|t| text(t, "/uid") == uid))
                    .map(|t| text(t, "/profileVersion"))
                    .unwrap_or_default();
            }
            Err(e) => c.error = Some(e.to_json()),
        }
        list.push(c);
    }
    // Palette computes the list above when it is asked, in the projects the key can read, and
    // gives an empty list on an error too. So each cluster the key can list is read for the
    // profiles it applies, and a cluster that applies the version joins the list (#66).
    let clusters_read =
        second_reading(&mut list, &uid, &home, &mut clients, &mut connect, progress);
    // a uid names no project to a reader, so give each row the project's name as well
    name_projects_of(
        &mut list,
        |c| &c.project_uid,
        |c| &mut c.project_name,
        &home,
    );

    let version = text(&doc, "/spec/version");
    let overwritten = list
        .iter()
        .filter(|c| !c.version.is_empty() && c.version != version)
        .count();
    Ok(ConsumersReport {
        schema: CONSUMERS_SCHEMA.to_string(),
        tool: Tool::current(),
        generated_at: generated_at.to_string(),
        profile: ConsumedProfile {
            uid: text(&doc, "/metadata/uid"),
            name: text(&doc, "/metadata/name"),
            version,
        },
        count: list.len(),
        overwritten,
        consumers: list,
        clusters_read,
    })
}

/// The second reading of `consumers`. The listing is read at tenant scope, so a cluster in any
/// project the key can read is in it; a key that cannot list at tenant scope lists its own scope.
/// A cluster the consumer list named is not read again. A cluster that applies `uid` joins
/// `list`, marked `unlisted`, with what its document and the listing say of it.
fn second_reading<A, F>(
    list: &mut Vec<Consumer>,
    uid: &str,
    home: &Palette<A>,
    clients: &mut HashMap<String, Palette<A>>,
    connect: &mut F,
    progress: &Progress,
) -> ClustersRead
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    progress.note("reading each cluster for the profiles it applies");
    let listing = connect(Some(TENANT_SCOPE))
        .and_then(|tenant| tenant.cluster_listing())
        .or_else(|_| home.cluster_listing());
    let listing = match listing {
        Ok(listing) => listing,
        Err(e) => {
            return ClustersRead {
                error: Some(e.to_json()),
                ..ClustersRead::default()
            };
        }
    };
    let mut clusters_read = ClustersRead::default();
    for listed in list_clusters(&listing) {
        if list.iter().any(|c| c.uid == listed.uid) {
            continue;
        }
        let doc = client_for(clients, home, &listed.project_uid, connect)
            .and_then(|p| p.cluster_document(&listed.uid));
        let doc = match doc {
            Ok(doc) => doc,
            Err(_) => {
                clusters_read.unread += 1;
                continue;
            }
        };
        clusters_read.read += 1;
        let copy = doc
            .pointer("/spec/clusterProfileTemplates")
            .and_then(Value::as_array)
            .and_then(|ts| ts.iter().find(|t| text(t, "/uid") == uid));
        if let Some(copy) = copy {
            let or_listed = |found: String, listed: &str| {
                if found.is_empty() {
                    listed.to_string()
                } else {
                    found
                }
            };
            list.push(Consumer {
                uid: listed.uid.clone(),
                name: or_listed(text(&doc, "/metadata/name"), &listed.name),
                project_uid: or_listed(
                    text(&doc, "/metadata/annotations/projectUid"),
                    &listed.project_uid,
                ),
                project_name: String::new(),
                state: or_listed(text(&doc, "/status/state"), &listed.state),
                version: text(copy, "/profileVersion"),
                error: None,
                unlisted: true,
            });
        }
    }
    clusters_read
}

/// Schema id of the document `list` writes.
pub const LIST_SCHEMA: &str = "strata-list/1";
/// The schema id of the document `packs` writes.
pub const PACKS_SCHEMA: &str = "strata-packs/1";

/// The scope `list` passes to `connect` for tenant scope: no project, whatever the environment
/// sets.
pub const TENANT_SCOPE: &str = "";

/// A kind of thing `list` reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    /// Profile versions, each a `<name>@<version>` ref.
    Profiles,
    /// Clusters, each a `cluster:<uid>` ref.
    Clusters,
    /// Projects, each a uid for `PALETTE_PROJECT_UID` or `--project`.
    Projects,
}

/// What `list` reads, and in which scope.
#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    /// The kinds to list. Empty is profiles and clusters, or profiles alone when `pack` is set.
    pub kinds: Vec<ListKind>,
    /// The project to list profiles and clusters in, in place of the configured scope.
    pub project: Option<String>,
    /// Read each profile document for its type, consumer count, and packs: one request per
    /// profile version.
    pub packs: bool,
    /// Keep only the profiles with a pack whose name contains this text, ignoring case. Implies
    /// `packs`.
    pub pack: Option<String>,
    /// Read each cluster document for the profiles the cluster applies: one request per cluster.
    pub applied: bool,
    /// Keep only the clusters whose name or uid contains this text, ignoring case. Implies
    /// `applied`.
    pub cluster: Option<String>,
}

impl ListOptions {
    fn wants(&self, kind: ListKind) -> bool {
        if !self.kinds.is_empty() {
            return self.kinds.contains(&kind);
        }
        match kind {
            ListKind::Profiles => self.cluster.is_none(),
            ListKind::Clusters => self.pack.is_none(),
            ListKind::Projects => false,
        }
    }

    /// True when the clusters are read for the profiles they apply: `--applied`, or `--cluster`,
    /// which names one cluster to answer what it runs.
    fn reads_clusters(&self) -> bool {
        self.applied || self.cluster.is_some()
    }
}

/// One pack of a listed profile.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListedPack {
    /// Palette's layer name, for example `cni`.
    pub layer: String,
    /// Pack name, for example `cni-cilium-oss`.
    pub name: String,
    /// The tag the profile pins.
    pub tag: String,
    /// Pack version.
    pub version: String,
}

/// One profile version.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListedProfile {
    /// `<name>@<version>`, for `snapshot`, `compare`, and `consumers`.
    #[serde(rename = "ref")]
    pub reference: String,
    /// Profile name.
    pub name: String,
    /// Profile version.
    pub version: String,
    /// Profile uid.
    pub uid: String,
    /// Cloud type, for example `aws`.
    pub cloud_type: String,
    /// With packs: the profile type, for example `cluster` or `add-on`.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub profile_type: Option<String>,
    /// With packs: the number of clusters that use the version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumers: Option<usize>,
    /// With packs: the packs in the order the profile holds them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packs: Option<Vec<ListedPack>>,
    /// With packs: why the profile document could not be read, when it could not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

/// One cluster.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListedCluster {
    /// `cluster:<uid>`, for `snapshot` and `compare`.
    #[serde(rename = "ref")]
    pub reference: String,
    /// Cluster name.
    pub name: String,
    /// Cluster uid.
    pub uid: String,
    /// The cluster's `status.state`, for example `Running`.
    pub state: String,
    /// Cloud type, for example `aws`.
    pub cloud_type: String,
    /// The project the cluster is in; empty for a tenant cluster. The ref works only in this
    /// project, which a tenant-scope listing can differ from.
    pub project_uid: String,
    /// That project's name. A name is a convenience, so it is empty when the project listing
    /// could not be read, and the reports fall back to the uid.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub project_name: String,
    /// With applied: the profile versions the cluster applies, in the order the cluster document
    /// lists them. That order is no install order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profiles: Option<Vec<AppliedProfile>>,
    /// With applied: why the cluster document could not be read, when it could not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

/// One project.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListedProject {
    /// Project name.
    pub name: String,
    /// Project uid.
    pub uid: String,
}

/// The document `list` writes: `strata-list/1`. A kind that was not listed is absent; a kind
/// that was listed and holds nothing is empty.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    /// The schema id of this document
    #[schemars(extend("const" = LIST_SCHEMA))]
    pub schema: String,
    /// Which tool wrote the document.
    pub tool: Tool,
    /// When, RFC 3339 UTC.
    pub generated_at: String,
    /// The project the profiles and clusters were listed in; empty for tenant scope.
    pub project_uid: String,
    /// That project's name; empty for tenant scope or when it could not be read.
    pub project_name: String,
    /// With `--pack`: the text the pack names were matched against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<String>,
    /// With `--cluster`: the text the cluster names and uids were matched against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster: Option<String>,
    /// Profile versions, by name and then natural version order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profiles: Option<Vec<ListedProfile>>,
    /// Clusters, by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clusters: Option<Vec<ListedCluster>>,
    /// Every project the key can see, by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projects: Option<Vec<ListedProject>>,
}

/// List what a ref can name: profile versions and clusters in one scope, and the projects the
/// key can see. Profiles and clusters are read through `connect(options.project)`, where `None`
/// is the configured scope. Projects are read through `connect(Some(TENANT_SCOPE))`, because in a
/// project Palette lists only that project. With `packs` or `pack`, each profile document is read
/// as well; a profile that cannot be read stays in the list with its error, and a listing that
/// cannot be read is an error.
pub fn list<A, F>(
    options: &ListOptions,
    mut connect: F,
    generated_at: &str,
    progress: &Progress,
) -> Result<Listing>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    let home = connect(options.project.as_deref())?;
    let project_uid = home.project_uid().unwrap_or("").to_string();

    let profiles = if options.wants(ListKind::Profiles) {
        progress.note("listing profiles");
        Some(list_profiles(&home, options, progress)?)
    } else {
        None
    };
    let clusters = if options.wants(ListKind::Clusters) {
        progress.note("listing clusters");
        let mut clusters = list_clusters(&home.cluster_listing()?);
        if let Some(text) = &options.cluster {
            let text = text.to_lowercase();
            clusters.retain(|c| {
                c.name.to_lowercase().contains(&text) || c.uid.to_lowercase().contains(&text)
            });
        }
        if options.reads_clusters() {
            read_applied(&mut clusters, &home, &mut connect, progress);
        }
        // a uid names no project to a reader, so give each line the project's name as well
        name_projects_of(
            &mut clusters,
            |c| &c.project_uid,
            |c| &mut c.project_name,
            &home,
        );
        Some(clusters)
    } else {
        None
    };
    let projects = if options.wants(ListKind::Projects) {
        progress.note("listing projects");
        let tenant = connect(Some(TENANT_SCOPE))?;
        Some(list_projects(&tenant.project_listing()?))
    } else {
        None
    };

    // The scope's name: from the project listing when it was read, else from the scope itself,
    // where Palette lists that one project. A name is a convenience, so a failure leaves it empty.
    let project_name = if project_uid.is_empty() {
        String::new()
    } else {
        let named = match &projects {
            Some(p) => p.clone(),
            None => home
                .project_listing()
                .map(|doc| list_projects(&doc))
                .unwrap_or_default(),
        };
        named
            .into_iter()
            .find(|p| p.uid == project_uid)
            .map(|p| p.name)
            .unwrap_or_default()
    };

    Ok(Listing {
        schema: LIST_SCHEMA.to_string(),
        tool: Tool::current(),
        generated_at: generated_at.to_string(),
        project_uid,
        project_name,
        pack: options.pack.clone(),
        cluster: options.cluster.clone(),
        profiles,
        clusters,
        projects,
    })
}

/// The entries of a listing: its `items` array, or the document itself when it is the array.
fn items(doc: &Value) -> impl Iterator<Item = &Value> {
    doc.get("items")
        .or(Some(doc))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn list_profiles<A: PaletteApi>(
    palette: &Palette<A>,
    options: &ListOptions,
    progress: &Progress,
) -> Result<Vec<ListedProfile>> {
    let mut profiles: Vec<ListedProfile> = items(&palette.profile_listing()?)
        .map(|it| {
            let name = text(it, "/metadata/name");
            let version = text(it, "/spec/version");
            ListedProfile {
                reference: format!("{name}@{version}"),
                uid: text(it, "/metadata/uid"),
                cloud_type: text(it, "/spec/cloudType"),
                name,
                version,
                profile_type: None,
                consumers: None,
                packs: None,
                error: None,
            }
        })
        .collect();
    profiles.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| version::compare(&a.version, &b.version))
            .then_with(|| a.uid.cmp(&b.uid))
    });
    if !options.packs && options.pack.is_none() {
        return Ok(profiles);
    }

    for p in &mut profiles {
        progress.note(&format!("reading profile {}", p.reference));
        let doc = match palette.profile_document(&p.uid) {
            Ok(doc) => doc,
            Err(e) => {
                p.error = Some(e.to_json());
                continue;
            }
        };
        // A profile that was never published has only a draft, as a snapshot reads it.
        let template = ["/spec/published", "/spec/draft"]
            .into_iter()
            .filter_map(|ptr| doc.pointer(ptr))
            .find(|t| {
                t.get("packs")
                    .and_then(Value::as_array)
                    .is_some_and(|a| !a.is_empty())
            })
            .or_else(|| doc.pointer("/spec/published"));
        let packs = template
            .and_then(|t| t.get("packs"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|pk| ListedPack {
                layer: text(pk, "/layer"),
                name: text(pk, "/name"),
                tag: text(pk, "/tag"),
                version: text(pk, "/version"),
            })
            .collect();
        p.profile_type = template.map(|t| text(t, "/type")).filter(|t| !t.is_empty());
        p.consumers = Some(consumer_uids(&doc).len());
        p.packs = Some(packs);
    }
    if let Some(needle) = &options.pack {
        let needle = needle.to_lowercase();
        // A profile that could not be read cannot be ruled out, so it stays with its error.
        profiles.retain(|p| match &p.packs {
            Some(packs) => packs
                .iter()
                .any(|pk| pk.name.to_lowercase().contains(&needle)),
            None => p.error.is_some(),
        });
    }
    Ok(profiles)
}

/// Fill in each row's project name from the project listing. The name is a convenience, so a
/// listing that cannot be read leaves every name empty and the reports print the uid alone. No
/// request is made when no row names a project.
fn name_projects_of<T, A: PaletteApi>(
    rows: &mut [T],
    uid_of: impl Fn(&T) -> &String,
    name_of: impl Fn(&mut T) -> &mut String,
    home: &Palette<A>,
) {
    if rows.iter().all(|r| uid_of(r).is_empty()) {
        return;
    }
    let Ok(doc) = home.project_listing() else {
        return;
    };
    let names = list_projects(&doc);
    for r in rows.iter_mut() {
        if let Some(p) = names.iter().find(|p| p.uid == *uid_of(r)).cloned() {
            *name_of(r) = p.name;
        }
    }
}

fn list_clusters(doc: &Value) -> Vec<ListedCluster> {
    let mut clusters: Vec<ListedCluster> = items(doc)
        .map(|it| {
            let uid = text(it, "/metadata/uid");
            ListedCluster {
                reference: format!("{}{uid}", crate::reference::CLUSTER_PREFIX),
                name: text(it, "/metadata/name"),
                state: text(it, "/status/state"),
                cloud_type: text(it, "/spec/cloudType"),
                project_uid: text(it, "/metadata/annotations/projectUid"),
                project_name: String::new(),
                uid,
                profiles: None,
                error: None,
            }
        })
        .collect();
    clusters.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.uid.cmp(&b.uid)));
    clusters
}

/// The client that reads a cluster of project `project`: the one already connected for that
/// project, a new one, or `home` when the cluster names no project of its own. A cluster with no
/// project is in the scope the operation already works in, so `home` reads it; `consumers` and
/// `list --applied` answered that differently until they shared this.
fn client_for<'a, A, F>(
    clients: &'a mut HashMap<String, Palette<A>>,
    home: &'a Palette<A>,
    project: &str,
    connect: &mut F,
) -> Result<&'a Palette<A>>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    if project.is_empty() || Some(project) == home.project_uid() {
        return Ok(home);
    }
    match clients.entry(project.to_string()) {
        Entry::Occupied(e) => Ok(e.into_mut()),
        Entry::Vacant(v) => connect(Some(project)).map(|p| &*v.insert(p)),
    }
}

/// Read each cluster document for the profiles it applies. A cluster is in its own project, so
/// each project gets one client, as `consumers` reads its clusters. A cluster the request cannot
/// read keeps its row and carries the error.
fn read_applied<A, F>(
    clusters: &mut [ListedCluster],
    home: &Palette<A>,
    connect: &mut F,
    progress: &Progress,
) where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    let mut clients: HashMap<String, Palette<A>> = HashMap::new();
    for c in clusters.iter_mut() {
        progress.note(&format!("reading cluster {}", c.uid));
        let doc = client_for(&mut clients, home, &c.project_uid, connect)
            .and_then(|p| p.cluster_document(&c.uid));
        match doc {
            Ok(d) => c.profiles = Some(AppliedProfile::of_cluster(&d)),
            Err(e) => c.error = Some(e.to_json()),
        }
    }
}

fn list_projects(doc: &Value) -> Vec<ListedProject> {
    let mut projects: Vec<ListedProject> = items(doc)
        .map(|it| ListedProject {
            name: text(it, "/metadata/name"),
            uid: text(it, "/metadata/uid"),
        })
        .collect();
    projects.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.uid.cmp(&b.uid)));
    projects
}

/// The document `packs` writes: `strata-packs/1`. One entry per registry that has the pack, and
/// one row per version, newest first.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackListing {
    /// The schema id of this document
    #[schemars(extend("const" = PACKS_SCHEMA))]
    pub schema: String,
    /// Which tool wrote the document.
    pub tool: Tool,
    /// When, RFC 3339 UTC.
    pub generated_at: String,
    /// The pack that was asked for.
    pub name: String,
    /// With `--cloud`: the cloud the versions were kept for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud: Option<String>,
    /// The registries that have the pack, in the order Palette lists them.
    pub registries: Vec<PackRegistry>,
}

/// One registry's copy of the pack.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackRegistry {
    /// The registry name.
    pub registry: String,
    /// Its uid.
    pub registry_uid: String,
    /// The layer the pack belongs to, as the registry gives it.
    pub layer: String,
    /// The versions, newest first. `--latest` keeps one of them.
    pub versions: Vec<PackBuild>,
    /// How many versions of the pack the registry holds, before `--latest` or `--cloud` cut the
    /// list. A heading states the registry's inventory, and not the count of rows a flag left.
    pub versions_offered: usize,
    /// True when Palette listed the pack under this registry and then answered 404 for the
    /// registry's entity: the versions come from the listing alone, with no tags.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tags_unavailable: bool,
    /// The state that every version of the pack in this registry has, when they all have the
    /// same one: `deprecated`, `disabled`, or `deleted`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

/// One version of a pack in one registry.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackBuild {
    /// The version.
    pub version: String,
    /// The pack uid of this build.
    pub pack_uid: String,
    /// The digest the registry reports, empty when it reports none.
    pub digest: String,
    /// The pack type: spectro, helm, oci, or manifest.
    #[serde(rename = "type")]
    pub kind: String,
    /// The clouds this version supports.
    pub cloud_types: Vec<String>,
    /// Every tag that points at this version, the exact tag first.
    pub tags: Vec<String>,
    /// True for the newest version of the registry's copy.
    pub latest: bool,
    /// The state that the registry listing gives this version: `deprecated`, `disabled`, or
    /// `deleted`. Palette serves no disabled or deleted pack, and refuses a profile that pins one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

/// What `packs` was asked for.
#[derive(Debug, Clone, Default)]
pub struct PackOptions {
    /// Only this registry, by name or uid.
    pub registry: Option<String>,
    /// Only the versions that support this cloud.
    pub cloud: Option<String>,
    /// Only the newest version of each registry.
    pub latest: bool,
}

/// List what the registries offer for one pack.
///
/// Reads the paged listing for the versions, their digests and their types, and each registry's
/// own answer for the tags that point at a version. A registry that does not have the pack is not
/// in the answer; no registry with it at all is `pack-not-found`.
pub fn registry_packs<A, F>(
    name: &str,
    options: &PackOptions,
    mut connect: F,
    generated_at: &str,
    progress: &Progress,
) -> Result<PackListing>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    let palette = connect(None)?;
    let visible = crate::candidate::registries(&palette)?;
    let wanted = match &options.registry {
        Some(r) => {
            let found = visible
                .iter()
                .find(|(n, u)| n == r || u == r)
                .ok_or_else(|| Error::RegistryNotFound {
                    registry: r.to_string(),
                    available: visible.iter().map(|(n, _)| n.clone()).collect(),
                })?;
            vec![found.clone()]
        }
        None => visible.clone(),
    };
    progress.note(&format!("reading the versions of {name}"));
    let entries = crate::candidate::versions_of(&palette, name)?;

    let mut registries = Vec::new();
    let mut had_the_pack = false;
    for (registry, uid) in wanted {
        let mut versions: Vec<PackBuild> = Vec::new();
        let mine: Vec<&Value> = entries
            .iter()
            .filter(|e| text(e, "/spec/registryUid") == uid)
            .collect();
        if mine.is_empty() {
            continue;
        }
        had_the_pack = true;
        // every version of this pack that the registry holds, counted before --cloud and --latest
        // cut the list: the heading states the inventory, not the rows that survived a flag
        let versions_offered = mine
            .iter()
            .filter_map(|e| Some(text(e, "/spec/version")).filter(|v| !v.is_empty()))
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        // one state for the registry, when every version of the pack in it has the same one
        let states: std::collections::BTreeSet<Option<String>> = mine
            .iter()
            .map(|e| crate::candidate::pack_state(e))
            .collect();
        let state = match states.into_iter().collect::<Vec<_>>().as_slice() {
            [Some(one)] => Some(one.clone()),
            _ => None,
        };
        // the registry's own answer holds the tags, which the listing does not. Palette serves no
        // disabled or deleted pack, so it can list a pack under a registry and then answer 404
        // for it; that registry keeps the versions of the listing, with no tags, and the other
        // registries still answer
        progress.note(&format!("reading the tags of {name} in {registry}"));
        let (entity, tags_unavailable) = match palette
            .api()
            .get_json(&format!("/v1/packs/{name}/registries/{uid}"))
        {
            Ok(entity) => (entity, false),
            Err(Error::Api { status: 404, .. }) => (Value::Null, true),
            Err(e) => return Err(e),
        };
        let layer = Some(text(&entity, "/layer"))
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| text(mine[0], "/spec/layer"));
        let tags = entity
            .get("tags")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        for e in mine {
            let version = text(e, "/spec/version");
            if versions.iter().any(|v: &PackBuild| v.version == version) {
                continue; // the paged listing repeats a version
            }
            let cloud_types: Vec<String> = e
                .pointer("/spec/cloudTypes")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|c| c.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            if let Some(cloud) = &options.cloud
                && !cloud_types.iter().any(|c| c == cloud)
            {
                continue;
            }
            // a tag row names its version, and names the build it points at; match on either,
            // because a registry that answers with one and not the other still says enough
            let pack_uid = text(e, "/metadata/uid");
            let mut rows: Vec<(usize, String)> = tags
                .iter()
                .filter(|t| {
                    let v = text(t, "/version");
                    if v.is_empty() {
                        text(t, "/packUid") == pack_uid
                    } else {
                        v == version
                    }
                })
                .map(|t| {
                    let above = t
                        .pointer("/parentTags")
                        .and_then(Value::as_array)
                        .map_or(0, Vec::len);
                    (above, text(t, "/tag"))
                })
                .collect();
            // the exact tag first, then the floating tags above it. A tag row names every tag
            // above it, so the row with the most of them is the most exact. A registry that
            // names none leaves the tags of one version in name order.
            rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
            let for_version: Vec<String> = rows.into_iter().map(|(_, tag)| tag).collect();
            versions.push(PackBuild {
                version,
                pack_uid,
                digest: text(e, "/spec/digest"),
                kind: text(e, "/spec/type"),
                cloud_types,
                tags: for_version,
                latest: false,
                state: crate::candidate::pack_state(e),
            });
        }
        versions.sort_by(|a, b| version::compare(&b.version, &a.version));
        if let Some(first) = versions.first_mut() {
            first.latest = true;
        }
        if options.latest {
            versions.truncate(1);
        }
        if !versions.is_empty() {
            registries.push(PackRegistry {
                registry,
                registry_uid: uid,
                layer,
                versions,
                versions_offered,
                tags_unavailable,
                state,
            });
        }
    }
    // no registry has the pack at all: strata cannot answer. A cloud that keeps nothing is an
    // answer, and an empty one.
    if !had_the_pack {
        if options.registry.is_none() {
            return Err(Error::PackNameNotFound {
                name: name.to_string(),
            });
        }
        return Err(Error::PackNotFound {
            name: name.to_string(),
            version: String::new(),
            tag: None,
            registry: options.registry.clone().unwrap_or_default(),
        });
    }
    Ok(PackListing {
        schema: PACKS_SCHEMA.to_string(),
        tool: Tool::current(),
        generated_at: generated_at.to_string(),
        name: name.to_string(),
        cloud: options.cloud.clone(),
        registries,
    })
}
