//! What a check answered: the [`Verdict`], the requests and the command it ran, and the
//! [`Outcome`] that holds them.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use strata::api::Method;
use strata::closed_set::impl_closed_set;
use strata::closed_set::legend::Described;

/// What one check answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    /// The check ran, and found no fault.
    Passed,
    /// The check ran, and found the fault.
    Failed,
    /// The rule has no check strata-preflight can run here: a cloud rule with no tool, a race, or
    /// a row with no check. The reproduce lines say what to run by hand.
    NotChecked,
    /// The rule has a check that applies here, and the check did not run: the tier was not asked
    /// for (`--tier`, `--offline`), there was no key, or `--no-exec`. It counts against the
    /// readiness, because nothing shows that it passes.
    NotRun,
    /// The rule does not apply to this run: another release, a `when` that is not true, a rule of
    /// the tool that makes the entity, or a watch rule outside a watch.
    Skipped,
    /// The check could not answer: Palette did not answer, the command did not start, or the
    /// program failed. The exit code is `2`.
    Error,
}

impl_closed_set!(Verdict {
    Passed => "passed",
    Failed => "failed",
    NotChecked => "not-checked",
    NotRun => "not-run",
    Skipped => "skipped",
    Error => "error",
});

impl Described for Verdict {
    fn describe(self) -> &'static str {
        match self {
            Verdict::Passed => "the check ran and found no fault",
            Verdict::Failed => "the check ran and found the fault; the messages say what",
            Verdict::NotChecked => {
                "strata-preflight has no check it can run here; the reproduce lines say what to run by hand"
            }
            Verdict::NotRun => {
                "the check applies here and did not run: a tier not asked for, no key, or --no-exec; it counts against the readiness"
            }
            Verdict::Skipped => "the rule does not apply to this run",
            Verdict::Error => "the check could not answer",
        }
    }
}

impl Verdict {
    /// The word of the report.
    pub fn name(&self) -> &'static str {
        match self {
            Verdict::Passed => "PASS",
            Verdict::Failed => "FAIL",
            Verdict::NotChecked => "NOT CHECKED",
            Verdict::NotRun => "NOT RUN",
            Verdict::Skipped => "SKIP",
            Verdict::Error => "ERROR",
        }
    }
}

/// One line for a request or a command, with its answer when it has one: `GET /v1/x -> HTTP 200`.
pub(crate) fn ran_line(ran: &str, answer: Option<&str>) -> String {
    match answer {
        Some(answer) => format!("{ran} -> {answer}"),
        None => ran.to_string(),
    }
}

/// One request that a check made, with the answer to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Call {
    /// The method of the request.
    pub method: Method,
    /// The path of the request, with its query.
    pub path: String,
    /// The answer: `HTTP 200`. None when Palette gave no answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
}

impl Call {
    /// The request as a person writes it: the method and the path, `GET /v1/x`.
    pub fn ran(&self) -> String {
        format!("{} {}", self.method, self.path)
    }

    /// The request and its answer on one line: `GET /v1/x -> HTTP 200`.
    pub fn line(&self) -> String {
        ran_line(&self.ran(), self.answer.as_deref())
    }
}

#[cfg(test)]
impl Call {
    /// A `GET` request with its answer, for the tests of this crate.
    pub(crate) fn get(path: &str, answer: Option<&str>) -> Call {
        Call {
            method: Method::Get,
            path: path.into(),
            answer: answer.map(Into::into),
        }
    }
}

/// What a check ran, with the answer to it.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Ran {
    /// A static check, a check that did not run, or a check with `each` over an empty list.
    #[default]
    Nothing,
    /// The command of a check that runs a program, as a person runs it by hand. The answer is
    /// `exit 0`, and none when the command did not start.
    Exec {
        /// The command line.
        line: String,
        /// The exit code, as `exit 0`.
        answer: Option<String>,
    },
    /// The one request of a check of Palette.
    Request(Call),
    /// The requests of a check with `each`, one for each item, in the order of the list.
    Requests(Vec<Call>),
}

impl Ran {
    /// The request or the command of a check that makes one: what a person runs by hand. None for
    /// a check that made none, or one for each item.
    pub fn text(&self) -> Option<String> {
        match self {
            Ran::Exec { line, .. } => Some(line.clone()),
            Ran::Request(call) => Some(call.ran()),
            Ran::Nothing | Ran::Requests(_) => None,
        }
    }

    /// The answer to that one request or command: `HTTP 200`, or `exit 0`.
    pub fn answer(&self) -> Option<&str> {
        match self {
            Ran::Exec { answer, .. } => answer.as_deref(),
            Ran::Request(call) => call.answer.as_deref(),
            Ran::Nothing | Ran::Requests(_) => None,
        }
    }

    /// The requests of a check with `each`, one for each item, in the order of the list.
    pub fn requests(&self) -> &[Call] {
        match self {
            Ran::Requests(calls) => calls,
            Ran::Nothing | Ran::Exec { .. } | Ran::Request(_) => &[],
        }
    }
}

/// The result of one check.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// What it answered.
    pub verdict: Verdict,
    /// One line for each failure, or the reason for `NOT CHECKED`, `SKIP`, or `ERROR`.
    pub messages: Vec<String>,
    /// How long the check took.
    pub elapsed: Duration,
    /// The request or the command the check ran, for the report: what a person runs by hand.
    pub ran: Ran,
}

impl Outcome {
    pub(super) fn new(verdict: Verdict, messages: Vec<String>, started: Instant) -> Outcome {
        Outcome {
            verdict,
            messages,
            elapsed: started.elapsed(),
            ran: Ran::Nothing,
        }
    }

    /// A check that did not run, with the reason.
    pub fn not_run(verdict: Verdict, reason: impl Into<String>) -> Outcome {
        Outcome {
            verdict,
            messages: vec![reason.into()],
            elapsed: Duration::ZERO,
            ran: Ran::Nothing,
        }
    }
}
