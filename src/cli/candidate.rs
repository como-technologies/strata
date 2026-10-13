//! `strata candidate`: write a candidate file from a profile version or a record.

use std::process::ExitCode;

use strata::error::{Error, Result};
use strata::reference::Ref;
use strata::{candidate, ops, render};

use super::args::CandidateArgs;
use super::{connect, now_utc, progress};

pub(crate) fn cmd_candidate(a: &CandidateArgs) -> Result<ExitCode> {
    let progress = progress(a.quiet);
    let reference = Ref::parse(&a.r#ref);
    // a record can hold a profile version too: Candidate::from_snapshot says
    if !reference.is_profile_version() && !matches!(reference, Ref::Snapshot(_)) {
        return Err(Error::ProfileRefRequired {
            reference: a.r#ref.clone(),
        });
    }
    let snap = ops::snapshot_for(
        &reference,
        &ops::SnapshotOptions::default(),
        || connect(progress, None),
        &now_utc(),
        &progress,
    )?;
    let c = candidate::Candidate::from_snapshot(&snap)?;
    match &a.output {
        Some(path) => {
            candidate::write(&c, path, a.files_dir.as_deref())?;
            // the root is the published version's, which the candidate was written from. The
            // candidate file carries no digests, so its own root depends on what the registry
            // holds: `snapshot --from-candidate` is what measures it.
            progress.note(&format!(
                "wrote {} from {} (root {})",
                path.display(),
                snap.source.reference,
                render::short(&snap.root)
            ));
        }
        None => print!("{}", candidate::to_yaml(&c)?),
    }
    Ok(ExitCode::SUCCESS)
}
