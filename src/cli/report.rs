//! `strata report`: print the values that Palette gave and this release does not know, as a report
//! or as a link that opens a new issue.

use std::process::ExitCode;

use strata::error::Result;
use strata::ops;
use strata::render::{MarkdownRenderer, Renderer};

use super::args::ReportArgs;

pub(crate) fn cmd_report(a: &ReportArgs) -> Result<ExitCode> {
    let log = ops::unknown_log()?;
    let options = ops::ReportOptions {
        all: a.all,
        sent: a.sent,
    };
    let report = ops::unknown_report(&log, options)?;
    if a.link {
        let body = MarkdownRenderer.render_report(&report)?;
        if let Some(link) = ops::report_link(&report, &body)? {
            println!("{link}");
            return Ok(ExitCode::SUCCESS);
        }
    }
    print!("{}", a.out.renderer().render_report(&report)?);
    Ok(ExitCode::SUCCESS)
}
