//! `strata verify`: make sure that each record agrees with Palette now.

use std::process::ExitCode;

use strata::error::Result;
use strata::snapshot::Snapshot;
use strata::{diff, ops};

use super::args::VerifyArgs;
use super::{connect, exit_for, expand_records, is_pattern, now_utc, project_arg};

pub(crate) fn cmd_verify(a: &VerifyArgs) -> Result<ExitCode> {
    let progress = a.out.progress();
    let paths = expand_records(&a.snapshots)?;
    let project = project_arg(progress, &a.project, &a.project_name)?;
    let bulk = a.summary || a.snapshots.len() > 1 || a.snapshots.iter().any(|s| is_pattern(s));
    let options = ops::VerifyOptions {
        who: a.who,
        allow_no_digest: a.allow_no_digest,
    };
    if bulk {
        let s = ops::verify_many(
            &paths,
            project.as_deref(),
            |scope| connect(progress, scope),
            &now_utc(),
            &options,
            &progress,
        );
        print!("{}", a.out.renderer().render_verify_summary(&s)?);
        return Ok(ExitCode::from(s.exit_code()));
    }
    let recorded = Snapshot::load(&paths[0])?;
    let scope = project
        .as_deref()
        .or(ops::scope_of(&recorded.source.project_uid));
    let palette = connect(progress, scope)?;
    let v = ops::verify_with(&recorded, &palette, &now_utc(), &options, &progress)?;
    let renderer = a.out.renderer();
    print!("{}", renderer.render(&v.diff, &recorded, &v.live, false)?);
    if let Some(line) = renderer.verdict(&v.diff) {
        println!("{line}");
    }
    Ok(exit_for(v.status == diff::VerifyStatus::Unchanged))
}
