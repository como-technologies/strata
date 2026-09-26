//! `verify`: a record against what Palette holds now, one record or many.

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::{Palette, PaletteApi};
use crate::diff::{
    self, AuditEntry, ClusterProfileOverwritten, Diff, ImpactLevel, Verify, VerifyStatus,
};
use crate::error::{Error, Result};
use crate::progress::Progress;
use crate::snapshot::{APP_KIND, Snapshot, SubjectKind, Tool};

/// The outcome of a verification.
#[derive(Debug)]
pub struct Verification {
    /// The recorded snapshot compared against the live profile, with `verify` filled in.
    pub diff: Diff,
    /// The live profile as fetched.
    pub live: Snapshot,
    /// True when Palette holds the recorded content: the roots are equal, and no uid names
    /// another version now.
    pub unchanged: bool,
    /// The outcome: `unchanged`, `drifted`, or `unverified` when the content is unchanged and a
    /// pack has no upstream digest. The exit status follows it.
    pub status: VerifyStatus,
}

/// What `verify` checks beyond the content.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VerifyOptions {
    /// For a profile or cluster record that drifted, read the writes to it from the audit log of
    /// Palette since the record was taken, into `verify.who`.
    pub who: bool,
    /// Accept a pack with no upstream digest: the record is `unchanged` and not `unverified`. The
    /// report still names the pack.
    pub allow_no_digest: bool,
}

/// The outcome of one record, from whether its content is unchanged and which of its packs have
/// no upstream digest. Drift wins: a record that drifted is `drifted`, whatever its packs.
fn settle(unchanged: bool, diff: &mut Diff, options: &VerifyOptions) -> VerifyStatus {
    let status = if !unchanged {
        VerifyStatus::Drifted
    } else if !diff.upstream_unknown.is_empty() && !options.allow_no_digest {
        VerifyStatus::Unverified
    } else {
        VerifyStatus::Unchanged
    };
    if let Some(v) = diff.verify.as_mut() {
        v.status = status;
        if status == VerifyStatus::Unverified {
            v.upstream_unknown = diff.upstream_unknown.clone();
        }
    }
    status
}

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

/// Re-fetch what a snapshot was taken from and compare: the profile, or for a cluster snapshot
/// the cluster, with the same profile selection.
pub fn verify<A: PaletteApi>(
    recorded: &Snapshot,
    palette: &Palette<A>,
    checked_at: &str,
    progress: &Progress,
) -> Result<Verification> {
    verify_with(
        recorded,
        palette,
        checked_at,
        &VerifyOptions::default(),
        progress,
    )
}

/// [`verify`], with the checks that `options` adds.
pub fn verify_with<A: PaletteApi>(
    recorded: &Snapshot,
    palette: &Palette<A>,
    checked_at: &str,
    options: &VerifyOptions,
    progress: &Progress,
) -> Result<Verification> {
    // a pack record names a registry pack, not a profile or a cluster
    if let Some(p) = &recorded.source.pack {
        if options.who {
            progress
                .note("note: Palette keeps no audit log for a registry pack; --who reads nothing");
        }
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
            who: None,
            upstream_unknown: Vec::new(),
            macros_changed: Vec::new(),
        };
        let mut diff = diff::compute(recorded, &live, checked_at, Some(verify));
        let status = settle(unchanged, &mut diff, options);
        return Ok(Verification {
            diff,
            live,
            unchanged,
            status,
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
        Some(c) => {
            let mut raw = match c.kind {
                SubjectKind::EdgeHost => palette.fetch_edge_host(uid, progress)?,
                SubjectKind::ClusterGroup => palette.fetch_cluster_group(uid, progress)?,
                SubjectKind::Cluster => palette.fetch_cluster(uid, progress)?,
            };
            // a record of the update that waits is checked on what waits now
            if c.pending {
                raw = palette.with_pending(raw)?;
            }
            // a record that holds the digests of the macro values is checked on them too
            if c.macro_values.is_some() {
                raw.resolved_values = Some(palette.resolved_values(uid)?);
            }
            // and one that holds the shape tier on its shape
            if c.tiers.contains_key("shape") {
                raw.cloud_config = Some(palette.cloud_config(&raw.cluster)?);
            }
            let live = Snapshot::from_cluster(
                &raw,
                c.profile.as_deref(),
                &recorded.source.reference,
                checked_at,
            )?;
            let live = if c.pending { live.mark_pending() } else { live };
            live.with_kind(c.kind)
        }
        None if recorded.source.kind == APP_KIND => {
            let mut live =
                Snapshot::from_raw(&palette.fetch_app_profile(uid, progress)?, uid, checked_at)?;
            live.source.consumers = None;
            live
        }
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
    let macros_changed = crate::diff::macros_changed(recorded, &live);
    let unchanged = recorded.root == live.root
        && overwritten_by.is_none()
        && profiles_overwritten.is_empty()
        && macros_changed.is_empty();
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
        who: None,
        upstream_unknown: Vec::new(),
        macros_changed: macros_changed.clone(),
    };
    let mut diff = diff::compute(recorded, &live, checked_at, Some(verify));
    let values_changed = diff
        .cluster_variables
        .as_ref()
        .map_or(0, |c| c.values.len());
    let who = if options.who && !unchanged {
        // the kinds of the audit log, as Palette names them
        let kind = match &recorded.source.cluster {
            Some(c) if c.kind == SubjectKind::EdgeHost => "edgehost",
            Some(_) => "spectrocluster",
            None => "clusterprofile",
        };
        progress.note(&format!(
            "reading the audit log of {kind} {uid} since {}",
            recorded.source.fetched_at
        ));
        let mut entries: Vec<AuditEntry> = palette
            .audits(kind, uid, &recorded.source.fetched_at)?
            .iter()
            .map(AuditEntry::from_palette)
            .collect();
        entries.reverse(); // Palette lists the newest first
        Some(entries)
    } else {
        None
    };
    if let Some(v) = diff.verify.as_mut() {
        v.cluster_values_changed = values_changed;
        v.who = who;
    }
    let status = settle(unchanged, &mut diff, options);
    Ok(Verification {
        diff,
        live,
        unchanged,
        status,
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
    /// Same content, and a pack has no upstream digest: a change of its archive would not show.
    Unverified,
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
    /// No record drifted or failed, and at least one is unverified: exit 1.
    Unverified,
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
    /// Records whose content is unchanged and that hold a pack with no upstream digest; only when
    /// it is not `0`.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub unverified: usize,
    /// `error` if any record failed, else `drifted` if any drifted, else `unverified` if any is,
    /// else `unchanged`.
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

fn is_zero(n: &usize) -> bool {
    *n == 0
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
    /// The exit status: 0 all unchanged, 1 any drift or any unverified record, 2 any error.
    pub fn exit_code(&self) -> u8 {
        match self.summary.status {
            SummaryStatus::Unchanged => 0,
            SummaryStatus::Drifted | SummaryStatus::Unverified => 1,
            SummaryStatus::Error => 2,
        }
    }
}

/// Verify several records. Each record is checked in the project it was taken in, or in
/// `project` when given; `connect` is called once per distinct scope. A record that cannot be
/// read or checked is an `error` entry, and the other records are still checked. `options` is as
/// in [`verify_with`].
pub fn verify_many<A, F>(
    paths: &[PathBuf],
    project: Option<&str>,
    mut connect: F,
    checked_at: &str,
    options: &VerifyOptions,
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
        records.push(
            match verify_with(&recorded, palette, checked_at, options, progress) {
                Ok(v) => VerifiedRecord {
                    path: path.display().to_string(),
                    status: match v.status {
                        VerifyStatus::Unchanged => RecordStatus::Unchanged,
                        VerifyStatus::Drifted => RecordStatus::Drifted,
                        VerifyStatus::Unverified => RecordStatus::Unverified,
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
                    // an unverified record keeps its diff too: it names the packs
                    diff: (v.status != VerifyStatus::Unchanged).then_some(v.diff),
                    error: None,
                },
                Err(e) => VerifiedRecord::failed(path, Some(&recorded), e),
            },
        );
    }
    let count = |s: RecordStatus| records.iter().filter(|r| r.status == s).count();
    let (unchanged, drifted, unverified, errors) = (
        count(RecordStatus::Unchanged),
        count(RecordStatus::Drifted),
        count(RecordStatus::Unverified),
        count(RecordStatus::Error),
    );
    let status = if errors > 0 {
        SummaryStatus::Error
    } else if drifted > 0 {
        SummaryStatus::Drifted
    } else if unverified > 0 {
        SummaryStatus::Unverified
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
                ("unverified", "The live profile has the same content as the record, and a pack has no upstream digest, so a change of its archive would not show. The diff names the packs. --allow-no-digest accepts them."),
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
            unverified,
            status,
        },
        records,
    }
}
