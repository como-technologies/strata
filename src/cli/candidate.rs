//! `strata candidate`: write a candidate file from a profile version or a record.

use std::process::ExitCode;

use strata::error::Result;
use strata::{candidate, ops, render};

use super::args::CandidateArgs;
use super::{connect, now_utc};

pub(crate) fn cmd_candidate(a: &CandidateArgs) -> Result<ExitCode> {
    let progress = a.quiet.progress();
    let snap =
        ops::snapshot_for_candidate(&a.r#ref, || connect(progress, None), &now_utc(), &progress)?;
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
