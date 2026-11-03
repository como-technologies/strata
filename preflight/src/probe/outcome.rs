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

/// The commands of a check as a person types them: the second command follows the first, with `; `
/// between them.
pub(crate) fn by_hand(line: &str, also: Option<&str>) -> String {
    match also {
        Some(also) => format!("{line}; {also}"),
        None => line.to_string(),
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

/// One command that an exec check ran: its line as a person types it, and its exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    /// The command line.
    pub line: String,
    /// The exit code. None when the command did not run or did not start.
    pub exit: Option<i32>,
}

impl Invocation {
    /// A command that has no exit code: it did not run, or it did not start.
    pub fn not_run(line: impl Into<String>) -> Invocation {
        Invocation {
            line: line.into(),
            exit: None,
        }
    }

    /// A command that ran and ended with the exit code `exit`.
    pub fn ended(line: impl Into<String>, exit: i32) -> Invocation {
        Invocation {
            line: line.into(),
            exit: Some(exit),
        }
    }

    /// The exit code as the report writes it: `exit 0`.
    fn answer(&self) -> Option<String> {
        self.exit.map(|exit| format!("exit {exit}"))
    }
}

/// What a check ran, with the answer to it.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Ran {
    /// A static check, a check that did not run, or a check with `each` over an empty list.
    #[default]
    Nothing,
    /// The commands of a check that runs a program, as a person runs them by hand.
    Exec {
        /// The command.
        first: Invocation,
        /// The second command, when the check has one.
        also: Option<Invocation>,
        /// The read of Palette that the check made before the command, when it has one.
        request: Option<Call>,
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
            Ran::Exec { first, also, .. } => Some(by_hand(
                &first.line,
                also.as_ref().map(|also| also.line.as_str()),
            )),
            Ran::Request(call) => Some(call.ran()),
            Ran::Nothing | Ran::Requests(_) => None,
        }
    }

    /// The answer to that one request or command: `HTTP 200`, or `exit 0`. For two commands it is
    /// the exit code of each, in the order of [`Ran::text`], with `; ` between them:
    /// `exit 0; exit 254`. None when a command has no exit code.
    pub fn answer(&self) -> Option<String> {
        match self {
            Ran::Exec { first, also, .. } => std::iter::once(first)
                .chain(also)
                .map(Invocation::answer)
                .collect::<Option<Vec<_>>>()
                .map(|answers| answers.join("; ")),
            Ran::Request(call) => call.answer.clone(),
            Ran::Nothing | Ran::Requests(_) => None,
        }
    }

    /// The requests of a check with `each`, one for each item, in the order of the list. For a
    /// command with a read of Palette, the one request of the read.
    pub fn requests(&self) -> &[Call] {
        match self {
            Ran::Requests(calls) => calls,
            Ran::Exec {
                request: Some(call),
                ..
            } => std::slice::from_ref(call),
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

    /// A row with no check in strata-preflight: `NOT CHECKED`. The reproduce lines of the report
    /// say what to run by hand.
    pub(crate) fn unchecked() -> Outcome {
        Outcome::not_run(
            Verdict::NotChecked,
            "no check in strata-preflight; the reproduce lines say what to run by hand",
        )
    }

    /// A watch rule in a run that is no watch: `SKIP`. The rule is of the build, so it does not
    /// apply here.
    pub(crate) fn outside_watch() -> Outcome {
        Outcome::not_run(
            Verdict::Skipped,
            "a watch rule runs with --watch cluster:UID, during the build",
        )
    }
}
