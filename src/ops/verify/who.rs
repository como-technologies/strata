//! `verify --who`: the writes to the object of a record, from the audit log of Palette.

use crate::api::{AuditKind, Palette, PaletteApi};
use crate::diff::{AuditEntry, AuditResource, Diff};
use crate::error::{Result, reason_words};
use crate::snapshot::{Snapshot, SubjectKind};
use crate::term::progress::Progress;

/// Add what `--who` finds to the verdict of `diff`: `verify.who`, the writes since the record.
///
/// The verdict comes from the diff, and the audit log adds to it. A read that fails, such as a 403
/// or a 5xx, leaves `verify.who` absent and sets `verify.whoError`. The status and the exit follow
/// the diff, as they do without `--who`.
pub(super) fn add_who<A: PaletteApi>(
    recorded: &Snapshot,
    diff: &mut Diff,
    palette: &Palette<A>,
    progress: &Progress,
) {
    let read = who_wrote(recorded, diff, palette, progress);
    let Some(v) = diff.verify.as_mut() else {
        return;
    };
    match read {
        Ok(entries) => v.who = Some(entries),
        Err(e) => {
            let reason = e.reason();
            progress.note(&unread_note(&reason));
            v.who_error = Some(reason);
        }
    }
}

/// The note for an audit log that strata could not read. It gives the words of `reason`, the
/// reason of `verify.whoError`: see [`reason_words`].
fn unread_note(reason: &str) -> String {
    format!(
        "note: the audit log is unavailable ({}); the verdict names no writer",
        reason_words(reason)
    )
}

/// The writes to the object of `recorded` since the record was taken, oldest first. It reads the
/// audit log of the object, and that of each profile that `diff` shows as overwritten. One read
/// that fails fails the whole list: a part of the list would read as the whole list.
fn who_wrote<A: PaletteApi>(
    recorded: &Snapshot,
    diff: &Diff,
    palette: &Palette<A>,
    progress: &Progress,
) -> Result<Vec<AuditEntry>> {
    let uid = recorded.source.shown_uid();
    let since = &recorded.source.fetched_at;
    let mut entries = writes(palette, audit_kind(recorded), uid, since, progress)?;
    let overwritten = diff
        .verify
        .as_ref()
        .map(|v| v.profiles_overwritten.as_slice())
        .unwrap_or_default();
    // the write that overwrote a version that a cluster runs is an entry of the profile
    for o in overwritten {
        let mut more = writes(
            palette,
            Some(AuditKind::ClusterProfile),
            &o.uid,
            since,
            progress,
        )?;
        // the line names the profile, also when Palette gives no resource on the entry
        for e in &mut more {
            let r = e.resource.get_or_insert_with(|| AuditResource {
                kind: AuditKind::ClusterProfile.as_str().to_string(),
                name: String::new(),
                uid: o.uid.clone(),
            });
            if r.name.is_empty() {
                r.name = o.name.clone();
            }
        }
        entries.extend(more);
    }
    // oldest first over all the logs; each time has the same form, so the text sorts it
    entries.sort_by(|a, b| a.time.cmp(&b.time));
    Ok(entries)
}

/// The kind of the object of a record in the audit log of Palette, or `None` for a cluster.
/// strata reads the log of a cluster by its uid alone. Palette logs the create and the delete of
/// a virtual cluster under `virtualCluster`, not under `spectrocluster`.
fn audit_kind(recorded: &Snapshot) -> Option<AuditKind> {
    match &recorded.source.cluster {
        Some(c) => match c.kind.known() {
            Some(SubjectKind::EdgeHost) => Some(AuditKind::EdgeHost),
            Some(SubjectKind::ClusterGroup) => Some(AuditKind::ClusterGroup),
            // a cluster, and a kind that a later release wrote: the entries of the uid alone
            Some(SubjectKind::Cluster) | None => None,
        },
        None if recorded.source.is_app() => Some(AuditKind::AppProfile),
        None => Some(AuditKind::ClusterProfile),
    }
}

/// The writes to one object that the audit log of Palette holds since `since`, oldest first.
/// With no `kind`, the entries of each kind for the uid.
fn writes<A: PaletteApi>(
    palette: &Palette<A>,
    kind: Option<AuditKind>,
    uid: &str,
    since: &str,
    progress: &Progress,
) -> Result<Vec<AuditEntry>> {
    let object = kind.map_or_else(|| uid.to_string(), |k| format!("{} {uid}", k.as_str()));
    progress.note(&format!("reading the audit log of {object} since {since}"));
    let mut entries: Vec<AuditEntry> = palette
        .audits(kind, uid, since)?
        .iter()
        .map(AuditEntry::from_palette)
        .collect();
    entries.reverse(); // Palette lists the newest first
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Method;
    use crate::api::codes::Denial;
    use crate::api::routes::{EACH_SCOPE, access_denied_message, error_body};
    use crate::error::Error;

    #[test]
    fn the_note_of_an_audit_log_that_palette_refuses_gives_the_status_the_code_and_the_message() {
        let message = access_denied_message("audit.list", EACH_SCOPE);
        // Palette ends a body with a newline
        let refused = Error::Api {
            method: Method::Get,
            path: "/v1/audits".into(),
            status: 403,
            body: format!("{}\n", error_body(Denial::Route.as_str(), &message)),
            retry_after: None,
            key_proved: false,
        };
        let note = unread_note(&refused.reason());
        assert_eq!(
            note,
            format!(
                "note: the audit log is unavailable (HTTP 403: AccessDenied: {message}); the \
                 verdict names no writer"
            )
        );
        assert!(!note.contains('{'), "no JSON of the body: {note}");
        // a request that did not complete gives its message
        let dropped = Error::Transport {
            method: Method::Get,
            path: "/v1/audits".into(),
            message: "connection reset".into(),
        };
        assert_eq!(
            unread_note(&dropped.reason()),
            "note: the audit log is unavailable (GET /v1/audits: connection reset); the verdict \
             names no writer"
        );
    }
}
