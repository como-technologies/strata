//! `strata same-release`: make sure that two or more records are one release.

use std::process::ExitCode;

use strata::error::Result;
use strata::ops;
use strata::ops::{Policy, SameReleaseOptions};
use strata::reference::Ref;

use super::args::{Allow, SameReleaseArgs};
use super::{connect, now_utc};

pub(crate) fn cmd_same_release(a: &SameReleaseArgs) -> Result<ExitCode> {
    let progress = a.out.progress();
    let now = now_utc();
    let mut records = Vec::with_capacity(a.records.len());
    let content = ops::SnapshotOptions::default();
    for r in &a.records {
        let snap = ops::snapshot_for(
            &Ref::parse(r),
            &content,
            || connect(progress, None),
            &now,
            &progress,
        )?;
        records.push((r.clone(), snap));
    }
    let options = SameReleaseOptions {
        policy: if a.allow.contains(&Allow::RegistryMove) {
            Policy::RegistryMove
        } else {
            Policy::Exact
        },
        allow_no_digest: a.allow.contains(&Allow::NoDigest),
        allow_renumber: a.allow.contains(&Allow::Renumber),
    };
    let c = ops::same_release_with(&records, options, &now);
    print!("{}", a.out.renderer().render_chain(&c)?);
    Ok(ExitCode::from(c.exit_code()))
}
