//! The targets of the files of the unknown values: the log, and the file of the answer to the
//! question of the send.
//!
//! `strata report` and the send read the log. The two reads of the log are not public, and the log
//! target reaches them as a program does. `ops::unknown_report` reads the log with the read that
//! stops at the first line that is not valid. `ops::send_unknown_report` reads it first with the
//! read that skips such a line, and then posts. The answer target calls `read_answer`, which is
//! public.

use strata::Error;
use strata::closed_set::unknown::log::FILE;
use strata::closed_set::unknown::report::{MOST_ENTRIES, Report, SCHEMA};
use strata::closed_set::unknown::send::{
    ANSWER_FILE, Choice, Facts, Step, keep_answer, read_answer, step,
};
use strata::ops::{self, ReportOptions, UnsentReport};
use strata_report_endpoint::accept::parse;

use crate::report::JSON;
use crate::support::{answer, scratch, utf8, write};

/// The address of the endpoint of the send: a text that is no address. So the post of a report
/// fails before it opens a connection, and the target sends nothing. The send reads the log
/// before it posts, so each input reaches the read.
const NO_ADDRESS: &str = "";

/// Make sure that `read`, the report of each line of a valid log, counts each line one time. Make
/// sure that the endpoint takes each part of [`MOST_ENTRIES`] entries of it, as the send posts it.
#[track_caller]
fn check_report(read: &UnsentReport) {
    let counted: usize = read.report.unknown.iter().map(|entry| entry.count).sum();
    assert_eq!(
        counted, read.lines,
        "the report counts another number of lines"
    );
    for part in read.report.unknown.chunks(MOST_ENTRIES) {
        let report = Report {
            schema: SCHEMA.to_string(),
            unknown: part.to_vec(),
        };
        let body = serde_json::to_vec(&report).expect("a report is JSON");
        let taken = parse(JSON, &body)
            .unwrap_or_else(|r| panic!("the endpoint refuses a report of a valid log: {r:?}"));
        assert_eq!(taken, report, "the endpoint reads another report");
    }
}

/// Read `bytes` as the log, as `strata report` and the send read it. Return true when each line of
/// the log is valid.
///
/// The report of a valid log counts each line, and the endpoint takes it. The send of a log with
/// no valid line posts nothing. Then it names the lines that are not valid, and the first of them
/// is the line that the report refuses. The send of a log with a valid line fails, so it marks no
/// line. Then the report of the unsent lines is the whole report.
pub(crate) fn log(bytes: &[u8]) -> bool {
    let dir = scratch();
    write(&dir, FILE, bytes);
    let file = dir.join(FILE);
    let read = ops::unknown_report(&file, ReportOptions { all: true });
    let refused = match &read {
        Err(Error::LogInvalid { line, .. }) => Some(*line),
        _ => None,
    };
    let sent = answer(ops::send_unknown_report(&file, NO_ADDRESS));
    if let Some(sent) = &sent {
        assert_eq!(
            (sent.marked, sent.waiting),
            (0, 0),
            "the send posted nothing"
        );
        assert_eq!(
            sent.skipped.first().copied(),
            refused,
            "the first invalid line"
        );
        assert!(
            sent.skipped.windows(2).all(|pair| pair[0] < pair[1]),
            "the lines that the send skips are in order: {:?}",
            sent.skipped
        );
    }
    let Some(read) = answer(read) else {
        return false;
    };
    check_report(&read);
    assert_eq!(
        sent.is_none(),
        read.lines > 0,
        "the send of a valid line fails"
    );
    let unsent = answer(ops::unknown_report(&file, ReportOptions::default()))
        .expect("a log that reads one time reads again");
    assert_eq!(unsent, read, "a send that failed marked a line");
    true
}

#[test]
fn a_log_of_unknown_values_gives_a_report_that_the_endpoint_takes_or_is_refused_with_a_kind() {
    bolero::check!().for_each(|bytes| {
        log(bytes);
    });
}

/// Read `bytes` as the file of the answer. Return true when the file holds a choice.
///
/// The file holds a choice only when its text is the text of a choice, with blank space around it.
/// Only the yes sends the report, and a file with no choice makes a run in a terminal ask again.
/// The file that `keep_answer` writes for the choice reads as the same choice.
pub(crate) fn answer_file(bytes: &[u8]) -> bool {
    let dir = scratch();
    write(&dir, ANSWER_FILE, bytes);
    let file = dir.join(ANSWER_FILE);
    let choice = read_answer(&file);
    let text = utf8(bytes).map(str::trim);
    assert_eq!(
        choice.map(Choice::as_str),
        text.filter(|t| Choice::ALL.iter().any(|c| c.as_str() == *t)),
        "the answer of the file {bytes:?}"
    );
    let facts = Facts {
        setting: None,
        answer: choice,
        endpoint: Some(NO_ADDRESS.to_string()),
        wrote: true,
        in_ci: false,
        terminal: true,
    };
    let sends = text == Some(Choice::Yes.as_str());
    assert_eq!(
        step(&facts) == Step::Send,
        sends,
        "the file {bytes:?} sends"
    );
    let Some(choice) = choice else {
        assert_eq!(step(&facts), Step::Ask, "a file with no answer asks again");
        return false;
    };
    answer(keep_answer(&file, choice)).expect("strata keeps the answer");
    assert_eq!(
        read_answer(&file),
        Some(choice),
        "the kept answer reads again"
    );
    true
}

#[test]
fn an_answer_file_gives_a_choice_that_it_keeps_and_only_the_yes_sends() {
    bolero::check!().for_each(|bytes| {
        answer_file(bytes);
    });
}
