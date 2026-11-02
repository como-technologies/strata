//! `strata report`: print the values that Palette gave and this release does not know, as a report
//! or as a link that opens a new issue.

use std::process::ExitCode;

use strata::error::Result;
use strata::ops;
use strata::render::{MarkdownRenderer, Renderer};

use super::args::ReportArgs;

pub(crate) fn cmd_report(a: &ReportArgs) -> Result<ExitCode> {
    let log = ops::unknown_log()?;
    let read = ops::unknown_report(&log, ops::ReportOptions { all: a.all })?;
    let report = &read.report;
    let link = if a.link {
        let body = MarkdownRenderer.render_report(report)?;
        ops::report_link(report, &body)?
    } else {
        None
    };
    match link {
        Some(link) => println!("{link}"),
        None => print!("{}", a.out.renderer().render_report(report)?),
    }
    // the mark comes last: a report that fails to print, or a link that is too long, marks nothing
    if a.sent {
        ops::mark_reported(&log, read.lines)?;
    }
    Ok(ExitCode::SUCCESS)
}
