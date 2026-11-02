//! `strata report`: the report of the unknown values that the log holds, and the link that opens a
//! new issue with it.
//!
//! `strata report` sends nothing. The report is text for the user to read, and the link is an
//! address that the user opens in a browser. [`send_unknown_report`] is the one function that
//! posts the report, and the end of a run calls it only when the user chose the send: see
//! [`crate::closed_set::unknown::send`]. See [`crate::closed_set::unknown`] for the log.

use std::path::{Path, PathBuf};

use crate::REPOSITORY;
use crate::closed_set::unknown::Report;
use crate::closed_set::unknown::{log, send};
use crate::error::{Error, Result};
use crate::text::query_value;

/// The title of the new issue that the link opens.
const ISSUE_TITLE: &str = "Values that Palette gave and strata does not know";

/// The most characters of a link. A web server refuses an address of about 8,000 characters or
/// more, so the link stays below that.
const LONGEST_LINK: usize = 6000;

/// Which lines of the log `strata report` reads, and what it does with them afterward.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReportOptions {
    /// `--all`: read each line of the log, also the lines that the user marked as sent.
    pub all: bool,
    /// `--sent`: mark each line that the report reads as sent. A later report omits them.
    pub sent: bool,
}

/// The log of the user: see [`log::path`].
///
/// # Errors
///
/// `Usage` when strata knows no state folder, so that it keeps no log.
pub fn unknown_log() -> Result<PathBuf> {
    log::path().ok_or_else(|| Error::Usage {
        message: "strata knows no state folder, so it keeps no log of unknown values: set XDG_STATE_HOME or HOME"
            .to_string(),
    })
}

/// The report of the log at `log`: the lines after the mark of the last `--sent`, or every line
/// with `all`.
///
/// Strata never marks a line by itself, because only the user knows that the report went out. A
/// mark that is past the end of the log, such as after a new log, counts for nothing: the
/// report then holds every line.
///
/// # Errors
///
/// `LogInvalid` when a line of the log is not a line of `strata-unknown/1`: nothing is reported
/// and nothing is marked. `Io` when strata cannot read the log, or cannot write the mark.
pub fn unknown_report(log: &Path, options: ReportOptions) -> Result<Report> {
    let (report, total) = report_and_total(log, options.all)?;
    if options.sent {
        log::mark_sent(&log::sent_path(log), total)?;
    }
    Ok(report)
}

/// The report of the log at `log`, and the number of lines that the log holds. `all` reads the
/// lines that the user marked as sent too.
fn report_and_total(log: &Path, all: bool) -> Result<(Report, usize)> {
    let lines = log::read(log)?;
    let marked = log::sent_count(&log::sent_path(log));
    // a mark past the end of the log belongs to a log that the user removed
    let skip = if all || marked > lines.len() {
        0
    } else {
        marked
    };
    Ok((Report::of(lines.iter().skip(skip)), lines.len()))
}

/// Post the report of the lines that the user did not mark as sent to the endpoint `endpoint`,
/// and mark those lines as sent after the endpoint took the report. A report with no line posts
/// nothing.
///
/// This is the one place where strata marks a line by itself: the endpoint took the report, so
/// the lines are sent. A send that fails marks nothing, and the next send holds the lines again.
///
/// # Errors
///
/// `LogInvalid` or `Io` as [`unknown_report`], and `Io` when strata cannot write the mark.
/// `ReportSend` when the endpoint did not take the report: see [`send::post`].
pub fn send_unknown_report(log: &Path, endpoint: &str) -> Result<()> {
    let (report, total) = report_and_total(log, false)?;
    if report.unknown.is_empty() {
        return Ok(());
    }
    send::post(endpoint, &report)?;
    log::mark_sent(&log::sent_path(log), total)
}

/// The address of a new issue of strata, with the title and the text `body` filled in.
///
/// The user reads each field in the browser before the user sends it. `body` is the report as
/// markdown. `None` when the report is empty, because there is nothing to send.
///
/// # Errors
///
/// `Usage` when the link is too long for an address. The message gives the
/// address of the new issue, so that the user can paste the markdown report there.
pub fn report_link(report: &Report, body: &str) -> Result<Option<String>> {
    if report.unknown.is_empty() {
        return Ok(None);
    }
    let new_issue = format!("{REPOSITORY}/issues/new");
    let link = format!(
        "{new_issue}?title={}&body={}",
        query_value(ISSUE_TITLE),
        query_value(body)
    );
    if link.len() > LONGEST_LINK {
        return Err(Error::Usage {
            message: format!(
                "the report is too long for a link: {} characters, and the most is {LONGEST_LINK}. \
                 Run `strata report --format markdown`, and paste the text in a new issue at {new_issue}",
                link.len()
            ),
        });
    }
    Ok(Some(link))
}
