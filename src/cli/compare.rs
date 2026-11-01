//! `strata compare`: compare two profile versions, records, or other objects.

use std::process::ExitCode;

use strata::error::Result;
use strata::ops;
use strata::reference::Ref;

use super::args::CompareArgs;
use super::{connect, exit_for, now_utc};

pub(crate) fn cmd_compare(a: &CompareArgs) -> Result<ExitCode> {
    let progress = a.out.progress();
    let now = now_utc();
    let (ra, rb) = (Ref::parse(&a.a), Ref::parse(&a.b));
    a.questions().check(&ra, &rb)?;
    let (sa, sb) = ops::snapshot_pair(&ra, &rb, || connect(progress, None), &now, &progress)?;
    let mut d = ops::compare(&sa, &sb, &now);
    if a.repave || a.validate {
        // the cluster side names its project; the questions go to that project
        let scope = [&sa, &sb]
            .into_iter()
            .find(|s| s.source.cluster.is_some())
            .and_then(|s| ops::scope_of(&s.source.project_uid));
        let palette = connect(progress, scope)?;
        if a.repave {
            d.palette_repave = Some(ops::palette_repave(
                &palette,
                &sa,
                &sb,
                d.summary.max_impact,
            )?);
        }
        if a.validate {
            d.palette_validation = Some(ops::palette_validation(&palette, &sa, &sb)?);
        }
    }
    print!("{}", a.out.renderer().render(&d, &sa, &sb, a.full)?);
    Ok(exit_for(d.nothing_differs()))
}
