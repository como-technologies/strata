//! The lines of `verify`: the verdict, the subject, who changed the object, and each record.

use super::changes::values_changed;
use super::notes::{
    ARCHIVE_CHANGE_HIDDEN, invalid_pack_sentences, no_upstream_digest, outside_root_sentence,
};
use super::same_release::record_word;
use super::{count_of, entry_error, registry_sync_suffix};
use crate::diff::{Diff, ImpactLevel, Verify, VerifyStatus};
use crate::ops::{
    ACTIVE_STATE, IN_USE_STATE, RUNNING_STATE, RecordStatus, VerifiedRecord, VerifySummary,
};
use crate::snapshot::{Naming, Source};
use crate::text::name_or_uid;

/// `name@version`, or `-` when the record could not be read.
pub(crate) fn record_label(name: &str, version: &str) -> String {
    if name.is_empty() {
        "-".to_string()
    } else {
        format!("{name}@{version}")
    }
}

/// For a cluster record whose running profile versions are gone from Palette: `cluster c runs
/// name@1.2.0, but uid u now names 1.3.0: the version was overwritten in place`, or `but uid u no
/// longer exists`, one clause per profile.
fn profiles_overwritten_sentence(v: &Verify, recorded: &Source) -> Option<String> {
    if v.profiles_overwritten.is_empty() {
        return None;
    }
    let cluster = recorded
        .cluster
        .as_ref()
        .map(|c| c.name.as_str())
        .unwrap_or_default();
    Some(
        v.profiles_overwritten
            .iter()
            .map(|o| {
                let runs = format!(
                    "cluster {cluster} runs {}@{}, but uid {}",
                    o.name, o.cluster_version, o.uid
                );
                if o.profile_version.is_empty() {
                    format!("{runs} no longer exists")
                } else {
                    format!(
                        "{runs} now names {}: the version was overwritten in place",
                        o.profile_version
                    )
                }
            })
            .collect::<Vec<_>>()
            .join("; "),
    )
}

/// The verify part of the diff of a record that did not stay unchanged.
fn record_verify(r: &crate::ops::VerifiedRecord) -> Option<&Verify> {
    r.diff.as_ref().and_then(|d| d.verify.as_ref())
}

/// What a verdict says that a record holds: `the draft` for a record of `<ref>@draft`, which the
/// verify compares with the published version, and `the copy` for each other record.
fn recorded_noun(recorded: &Source) -> &'static str {
    if recorded.is_draft() {
        "the draft"
    } else {
        "the copy"
    }
}

/// The verdict of a record that verifies: `holds the draft recorded <at>` for a record of
/// `<ref>@draft`, as the publish of the draft gives it, and `is unchanged since <at>` for each
/// other record.
fn unchanged_sentence(draft: bool, at: &str) -> String {
    if draft {
        format!("holds the draft recorded {at}")
    } else {
        format!("is unchanged since {at}")
    }
}

/// What a verdict states.
///
/// The cases are in the order that each report tests them: the first case that holds applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VerdictCase {
    /// The root did not move.
    Unchanged,
    /// The content is the recorded content, and a pack has no upstream digest.
    Unverified,
    /// A cluster runs a version that its uid no longer names.
    ProfilesOverwritten,
    /// Palette no longer has the object.
    Gone,
    /// The uid names the same version under another name.
    Renamed,
    /// The uid names another version: Palette overwrote the recorded one in place.
    OverwrittenBy,
    /// The root did not move, and a macro value or a machine image moved.
    OutsideRoot,
    /// The root moved.
    RootMoved,
}

/// The case of a verdict.
///
/// `same` is whether the root did not move, from [`Diff::same`](crate::diff::Diff::same).
fn verdict_case(v: &Verify, same: bool) -> VerdictCase {
    match v.status {
        VerifyStatus::Unchanged => VerdictCase::Unchanged,
        VerifyStatus::Unverified => VerdictCase::Unverified,
        VerifyStatus::Drifted if !v.profiles_overwritten.is_empty() => {
            VerdictCase::ProfilesOverwritten
        }
        VerifyStatus::Drifted if v.gone => VerdictCase::Gone,
        VerifyStatus::Drifted if v.renamed_to.is_some() => VerdictCase::Renamed,
        VerifyStatus::Drifted if v.overwritten_by.is_some() => VerdictCase::OverwrittenBy,
        // The verify names a macro whose value moved also when the root moved. The verdict
        // states the root then: the content differs, and the report names the macro in a note.
        VerifyStatus::Drifted
            if same && (!v.macros_changed.is_empty() || !v.image_ids_changed.is_empty()) =>
        {
            VerdictCase::OutsideRoot
        }
        VerifyStatus::Drifted => VerdictCase::RootMoved,
    }
}

/// The verdict of a record, in parts, so that each report builds its line one way.
///
/// The terminal and the markdown put the subject before the sentence. A row of the summary names
/// the record in its own column.
pub(crate) struct VerdictParts {
    /// The status of the record: see [`VerdictParts::word`].
    status: RecordStatus,
    /// What the verdict states.
    case: VerdictCase,
    /// The sentence after the subject: `is unchanged since <at>`.
    ///
    /// For a gone object, and for a cluster that runs a version its uid no longer names, the
    /// sentence names the subject itself.
    pub(crate) sentence: String,
    /// `; 1 variable value changed`, or nothing.
    ///
    /// A row of the summary puts its detail before it.
    pub(crate) values: String,
    /// The end of the line of one record: the sync of the registry, the state of the object, and
    /// when Palette last changed it.
    pub(crate) tail: String,
}

impl VerdictParts {
    /// The status word of the record: `OK`, `UNVERIFIED`, or `DRIFT`.
    pub(crate) fn word(&self) -> &'static str {
        record_word(self.status)
    }

    /// The line of one record after its status word: `published version x@1.0 is unchanged
    /// since <at>`, with the values and the tail.
    pub(crate) fn line(&self, recorded: &Source) -> String {
        let subject = match self.case {
            VerdictCase::ProfilesOverwritten | VerdictCase::Gone => String::new(),
            _ => format!("{} ", recorded.named(Naming::Verdict)),
        };
        format!("{subject}{}{}{}", self.sentence, self.values, self.tail)
    }
}

/// The parts of the verdict of a record.
///
/// `same` is whether the root did not move, from [`Diff::same`](crate::diff::Diff::same): a macro
/// value that moved is drift
/// outside the root only then. `recorded` is the source of the record, side A of the diff.
/// `code` puts a uid in the form of the report.
pub(crate) fn verdict_parts(
    v: &Verify,
    same: bool,
    recorded: &Source,
    code: fn(&str) -> String,
) -> VerdictParts {
    let at = recorded.fetched_at.as_str();
    let case = verdict_case(v, same);
    // verdict_case picks a case only when the value of that case is there, so no default of
    // this match shows
    let sentence = match case {
        VerdictCase::Unchanged => unchanged_sentence(recorded.is_draft(), at),
        VerdictCase::Unverified => format!(
            "has the content recorded {at}, but {}",
            unverified_sentence(&v.upstream_unknown)
        ),
        VerdictCase::ProfilesOverwritten => {
            profiles_overwritten_sentence(v, recorded).unwrap_or_default()
        }
        VerdictCase::Gone => gone_sentence(v, recorded, code),
        VerdictCase::Renamed => format!(
            "was renamed to {}",
            v.renamed_to.as_deref().unwrap_or_default()
        ),
        VerdictCase::OverwrittenBy => format!(
            "now names {}: the version recorded {at} was overwritten in place",
            v.overwritten_by.as_deref().unwrap_or_default()
        ),
        VerdictCase::OutsideRoot => format!(
            "has the content recorded {at}, but {}",
            outside_root_sentence(&v.macros_changed, &v.image_ids_changed).unwrap_or_default()
        ),
        VerdictCase::RootMoved => {
            format!("differs from {} recorded {at}", recorded_noun(recorded))
        }
    };
    // a gone object has no values to state, and a rename changes none
    let values = match case {
        VerdictCase::Gone | VerdictCase::Renamed => String::new(),
        _ => values_changed_suffix(v.cluster_values_changed),
    };
    VerdictParts {
        status: RecordStatus::from(v.status),
        case,
        sentence,
        values,
        tail: format!(
            "{}{}{}",
            registry_sync_suffix(v.registry_sync.as_ref()),
            state_suffix(v, recorded),
            last_modified_suffix(v)
        ),
    }
}

/// `both sides are name@version (uid u): the version was edited in place`, for a diff whose sides
/// name one profile version with different content.
pub(super) fn edited_in_place_note(d: &Diff) -> Option<String> {
    // a version that Palette no longer has was deleted, and not edited
    let gone = d.verify.as_ref().is_some_and(|v| v.gone);
    (d.edited_in_place && !gone).then(|| {
        let clusters = d.a.cluster.is_some() || d.b.cluster.is_some();
        format!(
            "both sides are {}@{} (uid {}): the version was edited in place{}",
            d.a.name,
            d.a.version,
            d.a.uid,
            if clusters {
                ", or changed on a cluster"
            } else {
                ""
            }
        )
    })
}

fn record_values_changed(r: &crate::ops::VerifiedRecord) -> usize {
    r.cluster_variables.as_ref().map_or(0, |c| c.values.len())
}

/// What `verify --who` found: `who: 2 writes in the audit log since the record`, or `who: no
/// audit entry since the record`. `None` without `--who`.
pub(crate) fn who_heading(v: &Verify) -> Option<String> {
    let who = v.who.as_ref()?;
    Some(if who.is_empty() {
        "who: no audit entry since the record".to_string()
    } else {
        format!(
            "who: {} in the audit log since the record",
            count_of(who.len(), "write")
        )
    })
}

/// One write: `2026-09-18T14:02:11.482Z  John Doe (user)  update`, then the kind and the name of
/// the object when the write touched another object than `uid`, the object of the record, then
/// `: <summary>` and `; note: <userMsg>` when Palette gives them.
pub(crate) fn who_entry(e: &crate::diff::AuditEntry, uid: &str) -> String {
    let mut line = format!("{}  {} ({})  {}", e.time, e.actor, e.actor_type, e.action);
    if let Some(r) = e.resource.as_ref().filter(|r| r.uid != uid) {
        line.push(' ');
        line.push_str(&r.kind);
        line.push(' ');
        line.push_str(name_or_uid(&r.name, &r.uid));
    }
    if !e.summary.is_empty() {
        line.push_str(": ");
        line.push_str(&e.summary);
    }
    if !e.user_msg.is_empty() {
        line.push_str("; note: ");
        line.push_str(&e.user_msg);
    }
    line
}

/// `; Palette last changed it <time>`: the end of a `DRIFT` verdict, from `verify.lastModified`.
/// Nothing for another verdict, or when Palette gives no time.
fn last_modified_suffix(v: &Verify) -> String {
    match &v.last_modified {
        Some(at) if v.status == crate::diff::VerifyStatus::Drifted => {
            format!("; Palette last changed it {at}")
        }
        _ => String::new(),
    }
}

/// What the summary line of a drifted record adds with `--who`: the last write, or that the log
/// holds none.
fn who_suffix(r: &crate::ops::VerifiedRecord) -> String {
    let Some(who) = r
        .diff
        .as_ref()
        .and_then(|d| d.verify.as_ref())
        .and_then(|v| v.who.as_ref())
    else {
        return String::new();
    };
    match who.last() {
        Some(e) => format!(
            "; last write: {} ({}), {} at {}",
            e.actor, e.actor_type, e.action, e.time
        ),
        None => "; no audit entry since the record".to_string(),
    }
}

/// The sentence after a record's status, and the state of its object when it is not the state
/// of one that runs: `; the state is Deleting`.
pub(crate) fn record_detail(r: &crate::ops::VerifiedRecord) -> String {
    let gone = record_verify(r).is_some_and(|v| v.gone);
    let mut detail = match r.state.as_deref() {
        Some(state) if !gone && !normal_state(state) => {
            format!("{}; the state is {state}", record_sentence(r))
        }
        _ => record_sentence(r),
    };
    for sentence in invalid_pack_sentences(&r.invalid_packs) {
        detail.push_str("; ");
        detail.push_str(&sentence);
    }
    detail
}

/// The sentence of a row of the summary: the verdict of the record, then what the report of one
/// record shows above its verdict. The last write of `--who` and the sync of the registry end it.
fn record_sentence(r: &VerifiedRecord) -> String {
    let sync = registry_sync_suffix(record_verify(r).and_then(|v| v.registry_sync.as_ref()));
    let verdict = r.diff.as_ref().and_then(|d| Some((d, d.verify.as_ref()?)));
    let detail = match (r.status, verdict) {
        (RecordStatus::Error, _) => entry_error(r.error.as_ref())
            .unwrap_or("could not be checked")
            .to_string(),
        (_, Some((d, v))) => {
            let parts = verdict_parts(v, d.same, &d.a, str::to_string);
            // the report of one record shows the impact of its diff above the verdict, and a row
            // of the summary states it
            let impact = match parts.case {
                VerdictCase::RootMoved => format!(
                    "; max impact {}, {} changed{}",
                    // verify_many gives each record that changed its max impact
                    r.max_impact.map(ImpactLevel::as_str).unwrap_or_default(),
                    count_of(r.packs_changed.unwrap_or(0), "pack"),
                    // Palette keeps the order of the packs, so two orders are two roots: a
                    // reorder alone is drift, and the line says so.
                    if d.order_changed {
                        ", the packs in another order"
                    } else {
                        ""
                    }
                ),
                _ => String::new(),
            };
            format!(
                "{}{impact}{}{}",
                parts.sentence,
                parts.values,
                who_suffix(r)
            )
        }
        // an unchanged record carries no diff, and the document of the summary does not say
        // whether the record holds a draft
        (_, None) => format!(
            "{}{}",
            unchanged_sentence(false, &r.recorded_at),
            values_changed_suffix(record_values_changed(r))
        ),
    };
    detail + &sync
}

/// `3 records: 2 unchanged, 1 drifted, 0 errors`, and `1 record` for one.
pub(crate) fn counts_line(s: &VerifySummary) -> String {
    let c = &s.summary;
    let unverified = if c.unverified > 0 {
        format!(", {} unverified", c.unverified)
    } else {
        String::new()
    };
    format!(
        "{}: {} unchanged, {} drifted{unverified}, {}",
        count_of(c.records, "record"),
        c.unchanged,
        c.drifted,
        count_of(c.errors, "error")
    )
}

/// Why a record is unverified: `pack csi/x has no upstream digest, so a change of its archive
/// does not show; --allow-no-digest accepts it`.
fn unverified_sentence(packs: &[String]) -> String {
    format!(
        "{}, {ARCHIVE_CHANGE_HIDDEN}; --allow-no-digest accepts it",
        no_upstream_digest(packs)
    )
}

/// The state that a cluster, an edge host, or a cluster group has when it runs what it holds.
fn normal_state(state: &str) -> bool {
    matches!(state, RUNNING_STATE | IN_USE_STATE | ACTIVE_STATE)
}

/// `; the cluster is Deleting`: the state of the object of a record, when it is not the state of
/// one that runs what it holds, and the object is not gone. Nothing for a profile record.
fn state_suffix(v: &Verify, recorded: &Source) -> String {
    match (&recorded.cluster, &v.state) {
        (Some(c), Some(state)) if !v.gone && !normal_state(state) => {
            format!("; the {} is {state}", c.kind.noun())
        }
        _ => String::new(),
    }
}

/// What a verdict names when the object of a record is gone: the cluster, the edge host, or the
/// cluster group itself, and not the profile it ran; or the published version.
fn gone_subject(recorded: &Source) -> String {
    match &recorded.cluster {
        Some(c) => format!("{} {}", c.kind.noun(), c.name),
        None => recorded.named(Naming::Verdict),
    }
}

/// `cluster c no longer exists: deleted <time>` when Palette keeps the document of the deleted
/// object, else `... no longer exists: uid u is gone from Palette since the record of <time>`.
/// `code` puts the uid in the form of the report.
fn gone_sentence(v: &Verify, recorded: &Source, code: fn(&str) -> String) -> String {
    let what = gone_subject(recorded);
    match &v.deleted_at {
        Some(at) => format!("{what} no longer exists: deleted {at}"),
        None => format!(
            "{what} no longer exists: uid {} is gone from Palette since the record of {}",
            code(&v.uid),
            recorded.fetched_at
        ),
    }
}

/// `; 1 variable value changed`, or nothing when none did.
fn values_changed_suffix(n: usize) -> String {
    if n == 0 {
        String::new()
    } else {
        format!("; {}", values_changed(n))
    }
}
