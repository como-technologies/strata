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
    let options = ListOptions {
        kinds: a.kinds.clone(),
        project: project_arg(progress, &a.project)?,
        packs: a.detail,
        pack: a.pack.clone(),
        applied: a.detail,
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
    Ok(ExitCode::from(l.exit_code()))
}
