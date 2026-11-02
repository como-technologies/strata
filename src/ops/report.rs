//! `strata report`: the report of the unknown values that the log holds, and the link that opens a
//! new issue with it.
//!
//! `strata report` sends nothing. The report is text for the user to read, and the link is an
//! address that the user opens in a browser. [`send_unknown_report`] is the one function that
//! posts the report, and the end of a run calls it only when the user chose the send: see
//! [`crate::closed_set::unknown::send`]. See [`crate::closed_set::unknown`] for the log.

use std::path::{Path, PathBuf};

use crate::REPOSITORY;
use crate::closed_set::unknown::report::{MOST_ENTRIES, MOST_VALUES};
use crate::closed_set::unknown::{Report, log, send};
use crate::error::{Error, Result};
use crate::text::query_value;

/// The title of the new issue that the link opens.
const ISSUE_TITLE: &str = "Values that Palette gave and strata does not know";

/// The most characters of a link. A web server refuses an address of about 8,000 characters or
/// more, so the link stays below that.
const LONGEST_LINK: usize = 6000;

/// Which lines of the log `strata report` reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReportOptions {
    /// `--all`: read each line of the log, also the lines that the user marked as sent.
    pub all: bool,
}

/// The report of the log, and the mark that the user can ask for after the report went out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsentReport {
    /// The report of the lines that the user did not mark as sent.
    pub report: Report,
    /// How many lines the log held when strata read it. [`mark_reported`] marks them as sent.
    pub lines: usize,
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
/// This function only reads. `strata report` marks a line only with `--sent`, because only the
/// user knows that the report went out. The caller prints the report, and then calls
/// [`mark_reported`] with the field `lines`. A mark that is past the end of the log, such as after
/// a new log, counts for nothing: the report then holds every line.
///
/// # Errors
///
/// `LogInvalid` when a line of the log is not a line of `strata-unknown/1`: nothing is reported.
/// `Io` when strata cannot read the log.
pub fn unknown_report(log: &Path, options: ReportOptions) -> Result<UnsentReport> {
    let (lines, skip) = read_unsent(log, options.all)?;
    Ok(UnsentReport {
        report: Report::of(lines.iter().skip(skip)),
        lines: lines.len(),
    })
}

/// Mark the first `lines` lines of the log at `log` as sent. A later report omits them.
///
/// Call it after the report went out, so that a report that fails marks nothing. `lines` is the
/// field of the [`UnsentReport`] that the report came from: a line that a run wrote after the read
/// stays unmarked.
///
/// # Errors
///
/// `Io` when strata cannot write the mark.
pub fn mark_reported(log: &Path, lines: usize) -> Result<()> {
    log::mark_sent(&log::sent_path(log), lines)
}

/// The lines of the log at `log`, and how many of them the user marked as sent. `all` counts none
/// as sent. A mark past the end of the log belongs to a log that the user removed, so it counts
/// for nothing.
fn read_unsent(log: &Path, all: bool) -> Result<(Vec<log::Line>, usize)> {
    let lines = log::read(log)?;
    let marked = log::sent_count(&log::sent_path(log));
    let skip = if all || marked > lines.len() {
        0
    } else {
        marked
    };
    Ok((lines, skip))
}

/// How many of the first `lines` make a report of [`MOST_ENTRIES`] entries or fewer, with
/// [`MOST_VALUES`] different values or fewer. At least one: a line is one entry.
fn first_chunk(lines: &[log::Line]) -> usize {
    let (mut fits, mut over) = (1, lines.len() + 1);
    while over - fits > 1 {
        let middle = fits + (over - fits) / 2;
        let report = Report::of(&lines[..middle]);
        if report.unknown.len() <= MOST_ENTRIES && report.distinct_values() <= MOST_VALUES {
            fits = middle;
        } else {
            over = middle;
        }
    }
    fits
}

/// Post the report of the lines that the user did not mark as sent to the endpoint `endpoint`.
/// After each report that the endpoint took, mark its lines as sent. A log with no such line posts
/// nothing.
///
/// This is the one place where strata marks a line by itself: the endpoint took the report, so
/// the lines are sent. A report has [`MOST_ENTRIES`] entries or fewer and [`MOST_VALUES`] values or
/// fewer, so a long log goes in more than one report. A report that fails marks nothing, and the
/// next send holds its lines again.
///
/// # Errors
///
/// `LogInvalid` or `Io` as [`unknown_report`], and `Io` when strata cannot write the mark.
/// `ReportSend` when the endpoint did not take a report: see [`send::post`].
pub fn send_unknown_report(log: &Path, endpoint: &str) -> Result<()> {
    let (lines, mut done) = read_unsent(log, false)?;
    while done < lines.len() {
        let rest = &lines[done..];
        let take = first_chunk(rest);
        send::post(endpoint, &Report::of(&rest[..take]), done)?;
        done += take;
        log::mark_sent(&log::sent_path(log), done)?;
    }
    Ok(())
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
