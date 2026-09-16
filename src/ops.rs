//! The operations the CLI exposes, without argument parsing, printing, or exit codes.

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::{Palette, PaletteApi, RawProfile};
use crate::diff::{self, Diff, ImpactLevel, Verify, VerifyStatus};
use crate::error::{Error, Result};
use crate::progress::Progress;
use crate::reference::Ref;
use crate::snapshot::{Snapshot, Tool};

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
    }
}

/// Build a snapshot from raw API responses saved on disk.
pub fn snapshot_from_dir(dir: &Path, reference: &str, fetched_at: &str) -> Result<Snapshot> {
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

/// Re-fetch the profile a snapshot was taken from and compare.
pub fn verify<A: PaletteApi>(
    recorded: &Snapshot,
    palette: &Palette<A>,
    checked_at: &str,
    progress: &Progress,
) -> Result<Verification> {
    let uid = recorded.source.uid.as_str();
    if uid.is_empty() {
        return Err(Error::NoUid);
    }
    progress.note(&format!(
        "verifying {}@{} ({uid}) against live; recorded {}",
        recorded.source.name, recorded.source.version, recorded.source.fetched_at
    ));
    let raw = palette.fetch_profile(uid, progress)?;
    let live = Snapshot::from_raw(&raw, uid, checked_at)?.with_project(palette.project_uid());
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
