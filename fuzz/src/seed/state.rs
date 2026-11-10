//! The seeds of the files of the state folder: the log of the unknown values and the file of the
//! answer.
//!
//! The module also holds the seeds of the body of a request to the endpoint, which is the report of
//! the lines of the log.

use std::fs;

use strata::closed_set::unknown::log::{Line, SCHEMA as LINE_SCHEMA};
use strata::closed_set::unknown::report::{Entry, Report, SCHEMA};
use strata::closed_set::unknown::send::{ANSWER_FILE, Choice, keep_answer};

use crate::support::{scratch, utf8};

use super::book_output;

// ---- the report ------------------------------------------------------------------------------

/// A report as `strata report --json` writes it, and the bodies that the endpoint refuses. The
/// fields `palette`, `status`, and `errorCode` were in the report of release 0.27.0, and the check
/// refuses them now.
const REPORT_CASES: [&str; 17] = [
    r#"{"schema":"strata-report/1","unknown":[{"strata":"0.26.0","set":"ProfileType","value":"later-type","route":"/v1/clusterprofiles/{uid}","count":2}]}"#,
    r#"{"schema":"strata-report/1","unknown":[{"strata":"0.26.0","set":"PackState","value":"retired","route":"/v1/packs","count":1},{"strata":"0.27.0","set":"PackState","value":"retired","route":"/v1/packs","count":3}]}"#,
    r#"{"schema":"strata-report/1","unknown":[{"strata":"0.26.0","set":"OciProvider","value":"oras","route":"/v1/registries/oci/summary","count":1},{"strata":"0.26.1","set":"ProfileType","value":"later-type","route":"/v1/dashboard/clusterprofiles/metadata","count":7}]}"#,
    r#"{"schema":"strata-report/1","unknown":[{"strata":"0.27.0-rc.1","set":"PackState","value":"x","route":"/v1/packs","count":1}]}"#,
    r#"{"schema":"strata-report/1","unknown":[{"strata":"0.28.0","set":"PackState","value":"two-words","route":"/v1/packs","count":1}]}"#,
    r#"{"schema":"strata-report/1","unknown":[{"strata":"0.28.0","set":"PackState","value":"retired","route":"/v1/packs","count":2},{"strata":"0.28.0","set":"OciProvider","value":"retired","route":"/v1/registries/oci/summary","count":1}]}"#,
    r#"{"schema":"strata-report/1","unknown":[{"strata":"0.26.0","palette":"4.7.1","set":"ProfileType","value":"later-type","route":"/v1/clusterprofiles/{uid}","count":2}]}"#,
    r#"{"schema":"strata-report/1","unknown":[{"strata":"0.26.0","palette":"4.7.1","set":"OciProvider","value":"oras","route":"/v1/registries/oci/summary","status":404,"errorCode":"RegistryNotFound","count":1}]}"#,
    r#"{"schema":"strata-report/1","unknown":[{"strata":"0.26.0","set":"PackState","value":"pending\u200b","route":"/v1/packs","count":1}]}"#,
    r#"{"schema":"strata-report/1","unknown":[{"strata":"0.28.0","set":"PackState","value":" pending ","route":"/v1/packs","count":1}]}"#,
    r#"{"schema":"strata-report/1","unknown":[]}"#,
    r#"{"schema":"strata-report/2","unknown":[{"strata":"0.26.0","set":"S","value":"v","route":"/v1/packs","count":1}]}"#,
    r#"{"schema":"strata-report/1","unknown":[{"strata":"0.26.0","set":"S","value":"v","route":"/v1/packs","count":1,"uid":"64f0a1b2c3d4e5f6a7b8c9d0"}]}"#,
    r#"{"schema":"strata-report/1","unknown":[{"strata":"0.26.0","set":"S","value":"v","route":"/v1/clusterprofiles/64f0a1b2c3d4e5f6a7b8c9d0","count":1}]}"#,
    r#"{"schema":"strata-report/1","unknown":[{"strata":"0.26.0","set":"S","value":"a\nb","route":"/v1/packs","count":0}]}"#,
    r#"{"schema":"strata-report/1","unknown":[{"strata":1}],"host":"h"}"#,
    "not json",
];

/// The seeds of the body of a request: the cases above.
pub(super) fn report_bodies() -> Vec<Vec<u8>> {
    REPORT_CASES.iter().map(|t| t.as_bytes().to_vec()).collect()
}

// ---- the log ---------------------------------------------------------------------------------

/// The most lines of the log that one entry of a report gives to a seed.
const MOST_COPIES: usize = 3;

/// The lines of the log of the entries of `report`, as strata writes a line. An entry gives as
/// many lines as it counts, and [`MOST_COPIES`] at most.
fn lines_of(report: &Report) -> String {
    let mut text = String::new();
    for entry in &report.unknown {
        let line = Line {
            schema: LINE_SCHEMA.to_string(),
            sighting: entry.sighting.clone(),
        };
        let line = serde_json::to_string(&line).expect("a line is JSON");
        for _ in 0..entry.count.clamp(1, MOST_COPIES) {
            text.push_str(&line);
            text.push('\n');
        }
    }
    text
}

/// The report of the output of the book that shows the entries of a report, one on each line.
fn book_report() -> Report {
    let output = book_output("unknown-value-report-json");
    let unknown: Vec<Entry> = utf8(&output)
        .expect("the output of the book is text")
        .lines()
        .map(|line| serde_json::from_str(line).expect("each line is an entry"))
        .collect();
    Report {
        schema: SCHEMA.to_string(),
        unknown,
    }
}

/// The seeds of the log: the lines of each report of the book and of the cases above. Also the logs
/// that a case needs and no report gives. They have blank lines, the line end of Windows, a line
/// with one more field, and a line of a later schema. They also have a line that is no JSON, a
/// valid line after a line that is not valid, and bytes that are not UTF-8.
pub(super) fn logs() -> Vec<Vec<u8>> {
    let mut reports = vec![book_report()];
    reports.extend(
        REPORT_CASES
            .iter()
            .filter_map(|t| serde_json::from_str(t).ok()),
    );
    let logs: Vec<String> = reports.iter().map(lines_of).collect();
    let line = logs[0]
        .lines()
        .next()
        .expect("the report of the book has an entry")
        .to_string();
    let mut wider: serde_json::Value = serde_json::from_str(&line).expect("a line is JSON");
    wider["host"] = "h".into();
    let mut later = wider.clone();
    later["schema"] = format!("{LINE_SCHEMA}0").into();
    let cases = [
        format!("\n  \n{line}\n\n"),
        format!("{line}\r\n{line}\r\n"),
        format!("{wider}\n"),
        format!("{later}\n"),
        format!("{line}\nnot json\n{line}\n"),
        format!("[]\n{line}\n"),
        String::new(),
    ];
    let mut seeds: Vec<Vec<u8>> = logs
        .into_iter()
        .chain(cases)
        .map(String::into_bytes)
        .collect();
    seeds.push(vec![0xff, 0xfe, b'\n']);
    seeds
}

// ---- the answer ------------------------------------------------------------------------------

/// The texts of the file of the answer that a case needs. They are a choice with blank space
/// around it, a word, two choices, a mark of the byte order, and bytes that are not UTF-8.
const ANSWER_CASES: [&[u8]; 9] = [
    b"1",
    b" 0 \r\n",
    b"\t1\t",
    b"yes",
    b"y\n",
    b"",
    b"1\n0\n",
    "\u{feff}1\n".as_bytes(),
    b"\xff1",
];

/// The seeds of the file of the answer: the file that `keep_answer` writes for each choice, and
/// the cases above.
pub(super) fn answers() -> Vec<Vec<u8>> {
    let mut seeds: Vec<Vec<u8>> = Choice::ALL
        .iter()
        .map(|&choice| {
            let file = scratch().join(ANSWER_FILE);
            keep_answer(&file, choice).expect("strata keeps the answer");
            fs::read(&file).expect("the file of the answer reads")
        })
        .collect();
    seeds.extend(ANSWER_CASES.iter().map(|case| case.to_vec()));
    seeds
}
