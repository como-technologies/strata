//! The object of a record as Palette holds it now: a cluster, an edge host, a cluster group, an
//! app profile, or a profile version.

use serde_json::Value;

use crate::api::{Palette, PaletteApi};
use crate::cluster_state::ClusterState;
use crate::error::{Error, Result, found};
use crate::json::{flag, opt_flag, opt_text, str_at};
use crate::snapshot::{ClusterSource, Snapshot, SubjectKind, Tier};
use crate::term::progress::Progress;

use super::consumers::{ACTIVE_STATE, INACTIVE_STATE};
use super::{HolderReads, holder_snapshot};

/// The state of a cluster, an edge host, or a cluster group, from its document: `status.state`,
/// or, for a cluster group, `active` or `inactive` from `status.isActive`.
fn subject_state(kind: SubjectKind, doc: &Value) -> Option<String> {
    match kind {
        SubjectKind::ClusterGroup => opt_flag(doc, "/status/isActive")
            .map(|a| if a { ACTIVE_STATE } else { INACTIVE_STATE }.to_string()),
        _ => opt_text(doc, "/status/state"),
    }
}

/// True when Palette keeps the document of a deleted object: the state `Deleted`, or
/// `metadata.isDeleted`. Palette deletes a cluster so, and keeps its spec.
fn is_deleted(doc: &Value) -> bool {
    ClusterState::Deleted.is(str_at(doc, "/status/state")) || flag(doc, "/metadata/isDeleted")
}

/// The live snapshot of the holder of a cluster record, from its document `doc`. It holds the
/// update that waits, the macro values, and the shape, as the record holds them. `kind` is the
/// kind of the holder that the record names.
fn live_holder<A: PaletteApi>(
    recorded: &Snapshot,
    c: &crate::snapshot::ClusterSource,
    kind: SubjectKind,
    doc: Value,
    palette: &Palette<A>,
    checked_at: &str,
    progress: &Progress,
) -> Result<Snapshot> {
    let raw = palette.subject_raw(kind, doc, progress)?;
    // the record decides what the live record reads: strata checks each part that the record holds
    let reads = HolderReads {
        profile: c.profile.as_deref(),
        pending: c.pending,
        macros: c.macro_values.is_some(),
        shape: c.tier(Tier::Shape).is_some(),
        members: false,
        raw_dir: None,
    };
    holder_snapshot(
        palette,
        kind,
        raw,
        &c.uid,
        &reads,
        &recorded.source.reference,
        checked_at,
    )
}

/// The kind of the object of the cluster record `c`, which tells strata how to read the object.
///
/// # Errors
///
/// [`Error::Usage`] for a kind that a later release wrote: this release cannot read such an
/// object from Palette. Strata sends no request.
pub(super) fn known_kind(c: &ClusterSource) -> Result<SubjectKind> {
    c.kind.known().ok_or_else(|| Error::Usage {
        message: format!(
            "a later release of strata wrote this record of the {} {}; this strata cannot read \
             a {} from Palette, so use that release for the record",
            c.kind, c.name, c.kind
        ),
    })
}

/// What Palette holds now for the object of a record, and what its document says beside the
/// content.
pub(super) struct LiveObject {
    /// The live record. It is the recorded one, empty, when the object is gone.
    pub(super) snapshot: Snapshot,
    /// Palette no longer has the object.
    pub(super) gone: bool,
    /// When Palette deleted the object, when its document says.
    pub(super) deleted_at: Option<String>,
    /// The state of the object, when its document gives one.
    pub(super) state: Option<String>,
    /// When Palette last changed the object. Its document carries the time, so it costs no
    /// request.
    pub(super) last_modified: Option<String>,
}

impl LiveObject {
    /// The object is there, and `snapshot` is what Palette holds.
    fn present(snapshot: Snapshot, last_modified: Option<String>) -> LiveObject {
        LiveObject {
            snapshot,
            gone: false,
            deleted_at: None,
            state: None,
            last_modified,
        }
    }

    /// Palette no longer has the object of `recorded`.
    fn gone(recorded: &Snapshot, checked_at: &str) -> LiveObject {
        LiveObject {
            snapshot: recorded.gone(checked_at),
            gone: true,
            deleted_at: None,
            state: None,
            last_modified: None,
        }
    }
}

/// Read the object of the record `recorded` from Palette: a cluster, an edge host, a cluster
/// group, an app profile, or a profile version. An object that Palette no longer has is a drift
/// and not an error: a retire deletes a version.
pub(super) fn read_live<A: PaletteApi>(
    recorded: &Snapshot,
    palette: &Palette<A>,
    checked_at: &str,
    progress: &Progress,
) -> Result<LiveObject> {
    let uid = recorded.source.shown_uid();
    match &recorded.source.cluster {
        Some(c) => live_holder_object(recorded, c, palette, checked_at, progress),
        None if recorded.source.is_app() => {
            match found(palette.fetch_app_profile(uid, progress))? {
                Some(raw) => Ok(LiveObject::present(
                    Snapshot::from_app_profile(&raw, uid, checked_at)?,
                    opt_text(&raw.profile, "/metadata/lastModifiedTimestamp"),
                )),
                None => Ok(LiveObject::gone(recorded, checked_at)),
            }
        }
        None => match found(palette.fetch_profile(uid, progress))? {
            Some(mut raw) => {
                // strata also checks the macro values of a record that holds their digests
                if recorded.source.macro_values.is_some() {
                    raw.resolved_values = Some(palette.profile_resolved_values(uid)?);
                }
                Ok(LiveObject::present(
                    Snapshot::from_raw(&raw, uid, checked_at)?,
                    opt_text(&raw.profile, "/metadata/lastModifiedTimestamp"),
                ))
            }
            None => Ok(LiveObject::gone(recorded, checked_at)),
        },
    }
}

/// Read the cluster, the edge host, or the cluster group of the cluster record `c`.
fn live_holder_object<A: PaletteApi>(
    recorded: &Snapshot,
    c: &ClusterSource,
    palette: &Palette<A>,
    checked_at: &str,
    progress: &Progress,
) -> Result<LiveObject> {
    let uid = recorded.source.shown_uid();
    let kind = known_kind(c)?;
    // the document by uid alone: a 404 is a gone object, and not a name to look up
    progress.note(&format!("fetching {} {uid}", kind.noun()));
    let doc = found(palette.subject_document(kind, uid))?.unwrap_or(Value::Null);
    let state = subject_state(kind, &doc);
    let last_modified = opt_text(&doc, "/metadata/lastModifiedTimestamp");
    if doc.is_null() || is_deleted(&doc) {
        return Ok(LiveObject {
            deleted_at: opt_text(&doc, "/metadata/deletionTimestamp"),
            state,
            last_modified,
            ..LiveObject::gone(recorded, checked_at)
        });
    }
    let snapshot = live_holder(recorded, c, kind, doc, palette, checked_at, progress)?;
    Ok(LiveObject {
        state,
        ..LiveObject::present(snapshot, last_modified)
    })
}
