//! `strata list`: list the profile versions, clusters, projects, saved tag filters, and
//! registries.

use std::process::ExitCode;

use strata::error::Result;
use strata::ops;
use strata::ops::ListOptions;

use super::args::ListArgs;
use super::{connect, now_utc, project_arg};

pub(crate) fn cmd_list(a: &ListArgs) -> Result<ExitCode> {
    let progress = a.out.progress();
    // each old name reads one kind deeper, as before, so a script that uses it gets the same
    // answer until its removal
    for (old, given, part) in [
        ("--packs", a.packs, "versions"),
        ("--applied", a.applied, "clusters"),
    ] {
        if given {
            progress.note(&format!(
                "{old} is the old name of --detail for the {part} alone. Release 0.35.0 \
                 removes {old}"
            ));
        }
    }
    let options = ListOptions {
        kinds: a.kinds.clone(),
        project: project_arg(progress, &a.project)?,
        packs: a.detail || a.packs,
        pack: a.pack.clone(),
        applied: a.detail || a.applied,
        cluster: a.cluster.clone(),
        query: ops::ClusterQuery {
            tags: a.tag.clone(),
            filter: a.filter.clone(),
        },
    };
    let l = ops::list(
        &options,
        |scope| connect(progress, scope),
        &now_utc(),
        &progress,
    )?;
    print!("{}", a.out.renderer().render_list(&l)?);
    Ok(ExitCode::SUCCESS)
}
