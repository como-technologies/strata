//! `strata consumers`: list the objects that use a profile version.

use std::process::ExitCode;

use strata::error::Result;
use strata::ops;
use strata::reference::Ref;

use super::args::ConsumersArgs;
use super::{connect_for, connect_in, now_utc};

pub(crate) fn cmd_consumers(a: &ConsumersArgs) -> Result<ExitCode> {
    let progress = a.out.progress();
    let reference = Ref::parse(&a.r#ref);
    let r = ops::consumers_with(
        &reference,
        &ops::ConsumersOptions {
            list_only: a.list_only,
        },
        // the scope of the command is the project that the ref names, else the flags
        |scope| match scope {
            None => connect_for(progress, reference.project(), Some(&a.project)),
            Some(_) => connect_in(progress, Some(&a.project), scope),
        },
        &now_utc(),
        &progress,
    )?;
    print!("{}", a.out.renderer().render_consumers(&r)?);
    Ok(ExitCode::from(r.exit_code()))
}
