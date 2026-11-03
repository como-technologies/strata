//! `strata report`: the report of the unknown values that the log holds, and the link that opens a
//! new issue with it.
//!
//! `strata report` sends nothing. The report is text for the user to read, and the link is an
//! address that the user opens in a browser. [`send_unknown_report`] is the one function that
//! posts the report. The end of a run calls it only when the user chose the send. For the send,
//! see [`crate::closed_set::unknown::send`]. See [`crate::closed_set::unknown`] for the log.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::REPOSITORY;
use crate::closed_set::unknown::report::Report;
use crate::closed_set::unknown::report::{MOST_ENTRIES, MOST_VALUES};
use crate::closed_set::unknown::{log, send};
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

/// The report of the log, and the mark that the user can ask for after strata sent the report.
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
/// user knows when the user sent the report. The caller prints the report, and then calls
/// [`mark_reported`] with the field `lines`. A mark that is past the end of the log, such as after
/// a new log, counts for nothing: the report then holds every line.
///
/// # Errors
///
/// `LogInvalid` when a line of the log is not a line of `strata-unknown/1`: the function reports
/// nothing. `Io` when strata cannot read the log.
pub fn unknown_report(log: &Path, options: ReportOptions) -> Result<UnsentReport> {
    let (lines, skip) = read_unsent(log, options.all)?;
    Ok(UnsentReport {
        report: Report::of(lines.iter().skip(skip)),
        lines: lines.len(),
    })
}

/// Mark the first `lines` lines of the log at `log` as sent. A later report omits them.
///
/// Call it after strata sent the report, so that a report that fails marks nothing. `lines` is the
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
    let skip = marked_lines(log, lines.len(), all);
    Ok((lines, skip))
}

/// How many of the `lines` lines of the log at `log` the user marked as sent. `all` counts none. A
/// mark past the end of the log belongs to a log that the user removed, so it counts for nothing.
fn marked_lines(log: &Path, lines: usize, all: bool) -> usize {
    let marked = log::sent_count(&log::sent_path(log));
    if all || marked > lines { 0 } else { marked }
}

/// How many of `lines` come before the first line of a value that waits. `waiting` holds the
/// numbers of the entries of `report` that wait at the endpoint, from 1. All of `lines` when
/// none waits.
fn lines_before_waiting(lines: &[log::Line], report: &Report, waiting: &[usize]) -> usize {
    let held: BTreeSet<(&str, &str)> = waiting
        .iter()
        .filter_map(|number| report.unknown.get(number.checked_sub(1)?))
        .map(|entry| (entry.sighting.set.as_str(), entry.sighting.value.as_str()))
        .collect();
    lines
        .iter()
        .position(|line| held.contains(&(line.sighting.set.as_str(), line.sighting.value.as_str())))
        .unwrap_or(lines.len())
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

/// What [`send_unknown_report`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sent {
    /// How many lines the send marked as sent: the lines of the reports that the endpoint took.
    pub marked: usize,
    /// How many lines wait at the endpoint for now, with the lines after them. The hour has no
    /// room for the values of these lines. The next send holds them again.
    pub waiting: usize,
    /// The numbers of the lines of the log that are not valid, from 1. The send skipped them.
    pub skipped: Vec<usize>,
}

/// Post the report of the lines that the user did not mark as sent to the endpoint `endpoint`.
/// After each report that the endpoint took, mark its lines as sent. A log with no such line posts
/// nothing.
///
/// This is the one place where strata marks a line by itself: the endpoint took the report, so
/// the run sent the lines. A report has [`MOST_ENTRIES`] entries or fewer and [`MOST_VALUES`]
/// values or fewer, so a long log goes in more than one report. A report that fails marks nothing,
/// and the next send holds its lines again.
///
/// A line that is not valid is no line of the report, and it stops nothing. The mark counts the
/// valid lines only, and the field `skipped` of the result names the others. When the values of
/// some entries wait at the endpoint, the send marks the lines before the first line of such a
/// value, and stops. The next send holds that line and the lines after it. So a line after it that
/// the endpoint took counts again.
///
/// # Errors
///
/// `Io` as [`unknown_report`], and `Io` when strata cannot write the mark. `ReportSend` when the
/// endpoint did not take a report: see [`send::post`].
pub fn send_unknown_report(log: &Path, endpoint: &str) -> Result<Sent> {
    let log::Valid { lines, invalid } = log::read_valid(log)?;
    let mut done = marked_lines(log, lines.len(), false);
    let mut sent = Sent {
        skipped: invalid,
        ..Sent::default()
    };
    while done < lines.len() {
        let rest = &lines[done..];
        let take = first_chunk(rest);
        let report = Report::of(&rest[..take]);
        let waiting = send::post(endpoint, &report, done)?;
        let taken = lines_before_waiting(&rest[..take], &report, &waiting);
        if taken > 0 {
            done += taken;
            sent.marked += taken;
            log::mark_sent(&log::sent_path(log), done)?;
        }
        if taken < take {
            sent.waiting = lines.len() - done;
            break;
        }
    }
    Ok(sent)
}

/// The address of a new issue of strata. The address holds the title and the text `body`.
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
