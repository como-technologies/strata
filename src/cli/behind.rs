//! `strata behind`: report the pack versions that records pin, and the newer versions.

use std::process::ExitCode;

use strata::error::Result;
use strata::ops;

use super::args::BehindArgs;
use super::{connect, now_utc};

pub(crate) fn cmd_behind(a: &BehindArgs) -> Result<ExitCode> {
    let progress = a.out.progress();
    let paths = ops::expand_records(&a.records)?;
    let since = match &a.since {
        Some(path) => Some(ops::BehindReport::load(path)?),
        None => None,
    };
    let r = ops::behind(
        &paths,
        &ops::BehindOptions {
            impact: !a.no_impact,
            since,
            pre_release: a.pre_release,
        },
        |scope| connect(progress, scope),
        &now_utc(),
        &progress,
    )?;
    print!("{}", a.out.renderer().render_behind(&r)?);
    Ok(ExitCode::from(r.exit_code()))
}
