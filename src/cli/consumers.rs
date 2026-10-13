//! `strata consumers`: list the objects that use a profile version.

use std::process::ExitCode;

use strata::error::Result;
use strata::ops;
use strata::reference::Ref;

use super::args::ConsumersArgs;
use super::{connect, now_utc};

pub(crate) fn cmd_consumers(a: &ConsumersArgs) -> Result<ExitCode> {
    let progress = a.out.progress();
    let r = ops::consumers_with(
        &Ref::parse(&a.r#ref),
        &ops::ConsumersOptions {
            list_only: a.list_only,
        },
        |scope| connect(progress, scope),
        &now_utc(),
        &progress,
    )?;
    print!("{}", a.out.renderer().render_consumers(&r)?);
    Ok(ExitCode::from(r.exit_code()))
}
