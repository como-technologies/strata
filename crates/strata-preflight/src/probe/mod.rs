//! How each kind of check runs, and what it can answer. A check reads the world, adds its own
//! answer to a copy of it, runs the program of the rule, and gives an [`Outcome`].
//!
//! The answers of Palette and of a command come through a [`Backend`], so a test runs the same
//! code on a fixture. [`FixtureBackend`] reads the answers from the world document itself, and
//! [`LiveBackend`] asks Palette and runs the command.

use std::time::Instant;

use serde_json::Value;
use strata::json::{flag, set_at, text_or};

use crate::jq::Jq;
use crate::rule::{Check, Rule};

mod backend;
mod exec;
pub(crate) mod outcome;
mod palette;

use exec::run_exec;
use palette::run_palette;

pub use backend::{Backend, ExecOutput, FixtureBackend, LiveBackend, Reader, Response, is_answer};
pub use exec::{shell_words, tool_on_path};
pub use outcome::{Call, Invocation, Outcome, Ran, Verdict};

/// Run the check of `rule` on `world`, through `backend`. A rule with no check is `NOT CHECKED`.
///
/// The message of a request to Palette that failed shows the start of the body of the answer. It
/// holds a mark in place of each value of a variable of the entity of `world`, such as
/// `[value of pod_cidr]`.
pub fn run_check(rule: &Rule, world: &Value, backend: &dyn Backend, jq: &Jq) -> Outcome {
    let started = Instant::now();
    let Some(check) = &rule.check else {
        return Outcome::unchecked();
    };
    match check {
        Check::Static { expr } => judge(rule, expr, world, jq, started),
        Check::Palette(check) => run_palette(rule, check, world, backend, jq, started),
        Check::Exec(check) => run_exec(rule, check, world, backend, jq, started),
        // the watch adds the cluster, the summary, and the events; the caller of a watch does,
        // and then judges with `judge_watch`
        Check::Watch { .. } => Outcome::outside_watch(),
    }
}

/// Judge one reading of a watch: `world` holds `cluster`, `summary`, `events`, and `elapsed`.
pub fn judge_watch(rule: &Rule, world: &Value, jq: &Jq) -> Outcome {
    let started = Instant::now();
    match &rule.check {
        Some(Check::Watch { expr }) => judge(rule, expr, world, jq, started),
        _ => Outcome::not_run(Verdict::Skipped, "not a watch rule"),
    }
}

/// Put `data` in the field `key` of `world`, the world of a check, for the program to read. A
/// derived `Serialize` of plain data does not fail, so a program never reads the `null` here.
fn put_data(world: &mut Value, key: &str, data: &impl serde::Serialize) {
    set_at(
        world,
        &[key],
        serde_json::to_value(data).unwrap_or(Value::Null),
    );
}

/// Run the program and read its output: `true`/`false`, a list of messages, or `{pass,
/// message}`. Several outputs must all pass.
fn judge(rule: &Rule, expr: &str, world: &Value, jq: &Jq, started: Instant) -> Outcome {
    let outputs = match jq.run(&rule.id, expr, world, None) {
        Ok(o) => o,
        Err(e) => return Outcome::new(Verdict::Error, vec![e.to_string()], started),
    };
    if outputs.is_empty() {
        return Outcome::new(
            Verdict::Error,
            vec!["the program gave no output; give true, false, a list of messages, or {pass, message}".into()],
            started,
        );
    }
    let mut failures = Vec::new();
    for out in outputs {
        match &out {
            Value::Bool(true) => {}
            Value::Bool(false) => failures.push("the check is false".to_string()),
            Value::Array(items) => {
                for item in items {
                    failures.push(one_line(match item {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    }));
                }
            }
            Value::Object(_) => {
                if !flag(&out, "/pass") {
                    failures.push(one_line(
                        text_or(&out, "/message", "the check is false").to_string(),
                    ));
                }
            }
            // null is a path that is not in the world, not a pass: a wrong program says so
            other => {
                return Outcome::new(
                    Verdict::Error,
                    vec![format!(
                        "the program gave {other}; give true, false, a list of messages, or {{pass, message}}"
                    )],
                    started,
                );
            }
        }
    }
    if failures.is_empty() {
        Outcome::new(Verdict::Passed, vec![], started)
    } else {
        Outcome::new(Verdict::Failed, failures, started)
    }
}

/// A message on one line. The report prints each message under its rule, and a line break inside
/// one, such as the output of a tool, leaves the column.
pub(crate) fn one_line(s: String) -> String {
    if s.contains(['\n', '\r']) {
        s.split(['\n', '\r'])
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        s
    }
}

#[cfg(test)]
mod tests;
