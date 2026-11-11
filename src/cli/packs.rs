//! `strata packs`: list the versions that a registry offers for a pack.

use std::process::ExitCode;

use strata::error::Result;
use strata::ops;

use super::args::PacksArgs;
use super::{connect_in, now_utc};

pub(crate) fn cmd_packs(a: &PacksArgs) -> Result<ExitCode> {
    let progress = a.out.progress();
    let options = ops::PackOptions {
        registry: a.registry.clone(),
        cloud: a.cloud.clone(),
        latest: a.latest,
    };
    let listing = ops::registry_packs(
        &a.name,
        &options,
        |scope| connect_in(progress, Some(&a.project), scope),
        &now_utc(),
        &progress,
    )?;
    print!("{}", a.out.renderer().render_packs(&listing)?);
    Ok(ExitCode::SUCCESS)
}
