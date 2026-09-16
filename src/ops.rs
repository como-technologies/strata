//! The operations the CLI exposes, without argument parsing, printing, or exit codes.

use std::path::Path;

use crate::api::{Palette, PaletteApi, RawProfile};
use crate::diff::{self, Diff, Verify, VerifyStatus};
use crate::error::{Error, Result};
use crate::progress::Progress;
use crate::reference::Ref;
use crate::snapshot::Snapshot;

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
            Snapshot::from_raw(&raw, &reference.to_string(), fetched_at)
        }
        Ref::Uid(uid) => {
            let palette = connect()?;
            let raw = palette.fetch_profile(uid, progress)?;
            Snapshot::from_raw(&raw, uid, fetched_at)
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
    let live = Snapshot::from_raw(&raw, uid, checked_at)?;
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
