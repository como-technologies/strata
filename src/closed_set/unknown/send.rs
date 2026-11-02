//! The opt-in send of the report to the endpoint of the maintainers.
//!
//! Strata sends nothing by default. A user chooses the send, and nothing else does. At the end of
//! a run that wrote a line to the log, strata decides with [`step`]:
//!
//! - The setting `STRATA_REPORT` decides first. `1` sends, and `0` sends nothing. The environment
//!   and `~/.palette.env` hold the setting, as they hold the other settings of strata.
//! - Without the setting, the answer that the user gave before decides. A user gives it one time,
//!   when strata asks: see [`Step::Ask`]. [`keep_answer`] keeps it in the state folder.
//! - Without either, a run in a terminal asks one time. A run in CI, or a run without a terminal,
//!   sends nothing and asks nothing.
//!
//! A send posts the report of the lines that the user did not mark as sent: see [`post`]. It has
//! one attempt and a short timeout. The caller marks the lines as sent after an answer of 2xx, and
//! it never fails a run for the error of a send.

use std::path::PathBuf;
use std::time::Duration;

use super::Report;
use crate::BOOK_URL;
use crate::closed_set::impl_texts;
use crate::config::{in_ci, setting, state_dir};
use crate::error::{Error, Result};
use crate::file::{read_file, write_file};
use crate::text::cut;

#[cfg(test)]
mod tests;

/// The setting that chooses the send (`1`) or refuses it (`0`).
pub const SETTING: &str = "STRATA_REPORT";

/// The setting that names the address of the endpoint, in place of [`ENDPOINT`].
pub const ADDRESS_SETTING: &str = "STRATA_REPORT_URL";

/// The address of the endpoint of the maintainers, or `None` when this release names none. With no
/// address, strata asks nothing and sends nothing, unless [`ADDRESS_SETTING`] names one.
pub const ENDPOINT: Option<&str> = None;

/// The name of the file that keeps the answer of the user, in the state folder.
pub const ANSWER_FILE: &str = "strata-unknown.answer";

/// The question that strata asks one time. It names the page of the book that says what the send
/// holds.
pub fn question() -> String {
    format!(
        "Keep strata informed of what it does not know? \
         It sends only the fields of `strata report` (see {BOOK_URL}reference/report.html). [y/N] "
    )
}

/// The longest time of a send. The send comes at the end of a run, so the run waits this long at
/// most.
const TIMEOUT: Duration = Duration::from_secs(3);

/// The most characters of an answer of the endpoint that an error shows.
const LONGEST_ANSWER: usize = 200;

/// The choice of the user: the text of the setting, and the text in the file of the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// Send the report.
    Yes,
    /// Send nothing.
    No,
}

impl_texts!(pub Choice {
    Yes => "1",
    No => "0",
});

/// What the end of a run does about the send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Do nothing.
    Nothing,
    /// Post the report.
    Send,
    /// Ask the user the question, keep the answer, and post the report when the answer is yes.
    Ask,
}

/// The facts of a run that decide the [`Step`].
#[derive(Debug, Clone)]
pub struct Facts {
    /// The text of the setting [`SETTING`], when there is one.
    pub setting: Option<String>,
    /// The answer that the user gave before: see [`read_answer`].
    pub answer: Option<Choice>,
    /// The address of the endpoint, when strata has one: see [`ENDPOINT`].
    pub endpoint: Option<String>,
    /// True when the run wrote a line to the log. A run that saw no unknown value sends nothing.
    pub wrote: bool,
    /// True when the variable `CI` has a value.
    pub in_ci: bool,
    /// True when a person can answer: standard input and standard error are terminals, and the run
    /// is not quiet.
    pub terminal: bool,
}

impl Facts {
    /// The facts of this run. The settings and the file of the answer are read now. `wrote` is true
    /// when the run wrote a line to the log, and `terminal` is true when a person can answer.
    pub fn of_run(wrote: bool, terminal: bool) -> Facts {
        Facts {
            setting: setting(SETTING).map(|text| text.trim().to_string()),
            answer: answer_path().and_then(|file| read_answer(&file)),
            endpoint: setting(ADDRESS_SETTING).or_else(|| ENDPOINT.map(str::to_string)),
            wrote,
            in_ci: in_ci(),
            terminal,
        }
    }

    /// What the user must hear when the setting cannot do what it says, or `None`. A run that
    /// wrote no line has nothing to send, so it says nothing. The caller prints the text as a
    /// warning.
    ///
    /// A setting that is neither `1` nor `0` sends nothing. The setting `1` sends nothing when
    /// the release names no endpoint and [`ADDRESS_SETTING`] is not set.
    pub fn complaint(&self) -> Option<String> {
        if !self.wrote {
            return None;
        }
        match self.setting.as_deref().map(Choice::from_text) {
            Some(None) => Some(format!("{SETTING} must be 1 or 0, so strata sends nothing")),
            Some(Some(Choice::Yes)) if self.endpoint.is_none() => Some(format!(
                "{SETTING} is 1, and this release names no endpoint: set {ADDRESS_SETTING}"
            )),
            _ => None,
        }
    }
}

/// What the end of a run does, from the facts of the run.
///
/// A setting that is neither `1` nor `0` sends nothing: the user did not choose the send.
pub fn step(facts: &Facts) -> Step {
    if !facts.wrote || facts.endpoint.is_none() {
        return Step::Nothing;
    }
    let choice = match &facts.setting {
        Some(text) => Choice::from_text(text),
        None => facts.answer,
    };
    match choice {
        Some(Choice::Yes) => Step::Send,
        Some(Choice::No) => Step::Nothing,
        None if facts.setting.is_some() || facts.in_ci || !facts.terminal => Step::Nothing,
        None => Step::Ask,
    }
}

/// The file that keeps the answer of the user: [`ANSWER_FILE`] in the state folder. `None` when
/// strata knows no state folder.
pub fn answer_path() -> Option<PathBuf> {
    Some(state_dir()?.join(ANSWER_FILE))
}

/// The answer in the file `file`. A file that is not there, or that holds another text, is no
/// answer, so strata asks again.
pub fn read_answer(file: &std::path::Path) -> Option<Choice> {
    // a file that cannot be read is no answer: the user is asked again, and never sent for
    read_file(file)
        .ok()
        .and_then(|text| Choice::from_text(text.trim()))
}

/// Keep the answer `choice` in the file `file`.
///
/// # Errors
///
/// `Io`, with the file: for example, a folder that the user cannot write.
pub fn keep_answer(file: &std::path::Path, choice: Choice) -> Result<()> {
    write_file(file, &format!("{}\n", choice.as_str()))
}

/// Post the report `report` to the endpoint `endpoint` as JSON, with one attempt.
///
/// The attempt ends after 3 seconds. The call follows no redirect, because the user named the
/// address and no other.
///
/// # Errors
///
/// `ReportSend` when strata cannot reach the endpoint, when the answer is not a status of 2xx, or
/// when the report is not valid: see [`Report::fault`]. A report that strata made from its own log
/// is valid.
pub fn post(endpoint: &str, report: &Report) -> Result<()> {
    let failed = |message: String| Error::ReportSend {
        endpoint: endpoint.to_string(),
        message,
    };
    if let Some(fault) = report.fault() {
        return Err(failed(format!("the report is not valid: {fault}")));
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .timeout_global(Some(TIMEOUT))
        .build()
        .into();
    let body = serde_json::to_string(report).map_err(|e| failed(e.to_string()))?;
    let mut answer = agent
        .post(endpoint)
        .header("User-Agent", concat!("strata/", env!("CARGO_PKG_VERSION")))
        .content_type("application/json")
        .send(body)
        .map_err(|e| failed(e.to_string()))?;
    let status = answer.status().as_u16();
    if (200..300).contains(&status) {
        return Ok(());
    }
    // the first part of the answer tells why the endpoint refused; a body that cannot be read
    // adds nothing to the status
    let body = answer.body_mut().read_to_string().unwrap_or_default();
    let body = cut(&body.replace(['\n', '\r'], " "), LONGEST_ANSWER);
    Err(failed(format!(
        "the endpoint answered HTTP {status}: {body}"
    )))
}
