//! A check of Palette: one request, or one request for each item of `each`, then the program of
//! the rule.

use std::time::Instant;

use serde::Serialize;
use serde_json::Value;
use strata::api::Method;
use strata::config::KEY_VARS;
use strata::json::value_or_null;

use super::backend::{Backend, Response};
use super::judge;
use super::outcome::{Call, Outcome, Ran, Verdict};
use crate::error::Error;
use crate::jq::Jq;
use crate::rule::{PaletteCheck, Rule};

/// The answer of Palette to the request for one item of a check with `each`. It holds the item,
/// and the response in the shape of `.response` of a check with no `each`.
#[derive(Serialize)]
struct ItemResponse<'a> {
    item: &'a Value,
    response: Response,
}

/// A request that went out, and the answer to it.
struct Asked {
    call: Call,
    response: Response,
}

/// A request that no check can read: the templates did not fill, or Palette did not answer.
struct Failed {
    /// The request as it went out, when the templates filled.
    call: Option<Call>,
    error: Error,
}

/// Fill the templates of the request for `item`, and ask Palette. A check with no `each` has no
/// item.
fn ask(
    rule: &Rule,
    check: &PaletteCheck,
    item: Option<&Value>,
    world: &Value,
    backend: &dyn Backend,
    jq: &Jq,
) -> std::result::Result<Asked, Box<Failed>> {
    let unfilled = |error| Box::new(Failed { call: None, error });
    let path = jq
        .render(&rule.id, &check.path, world, item)
        .map_err(unfilled)?;
    let body = match (check.method, check.body.as_deref()) {
        (Method::Post, Some(program)) => {
            Some(jq.first(&rule.id, program, world, item).map_err(unfilled)?)
        }
        (Method::Post, None) => Some(value_or_null(world, "/entity")),
        (Method::Get, _) => None,
    };
    let method = check.method;
    match backend.palette(method, &path, body.as_ref()) {
        Ok(response) => {
            let answer = Some(format!("HTTP {}", response.status));
            Ok(Asked {
                call: Call {
                    method,
                    path,
                    answer,
                },
                response,
            })
        }
        Err(error) => Err(Box::new(Failed {
            call: Some(Call {
                method,
                path,
                answer: None,
            }),
            error,
        })),
    }
}

/// Run a check of Palette: one request, or one request for each item of `each`, then the program
/// of the rule.
pub(super) fn run_palette(
    rule: &Rule,
    check: &PaletteCheck,
    world: &Value,
    backend: &dyn Backend,
    jq: &Jq,
    started: Instant,
) -> Outcome {
    if !backend.has_palette() {
        return Outcome::not_run(
            Verdict::NotRun,
            format!("no key: set {} to run the Palette checks", KEY_VARS[0]),
        );
    }
    let Some(each) = check.each.as_deref() else {
        return match ask(rule, check, None, world, backend, jq) {
            Ok(Asked { call, response }) => {
                let mut with = world.clone();
                with["response"] = serde_json::to_value(&response).unwrap_or(Value::Null);
                let mut out = judge(rule, &check.expr, &with, jq, started);
                out.ran = Ran::Request(call);
                out
            }
            Err(failed) => {
                let mut out = Outcome::new(Verdict::Error, vec![failed.error.to_string()], started);
                if let Some(call) = failed.call {
                    out.ran = Ran::Request(call);
                }
                out
            }
        };
    };
    let items = match jq.first(&rule.id, each, world, None) {
        Ok(Value::Array(items)) => items,
        Ok(other) => {
            return Outcome::new(
                Verdict::Error,
                vec![format!("`each` gave {other}; give a list")],
                started,
            );
        }
        Err(e) => return Outcome::new(Verdict::Error, vec![e.to_string()], started),
    };
    let mut calls = Vec::with_capacity(items.len());
    let mut responses = Vec::with_capacity(items.len());
    for item in &items {
        match ask(rule, check, Some(item), world, backend, jq) {
            Ok(asked) => {
                calls.push(asked.call);
                responses.push(ItemResponse {
                    item,
                    response: asked.response,
                });
            }
            Err(failed) => {
                calls.extend(failed.call);
                let mut out = Outcome::new(Verdict::Error, vec![failed.error.to_string()], started);
                out.ran = Ran::Requests(calls);
                return out;
            }
        }
    }
    let mut with = world.clone();
    // a derived `Serialize` of plain data does not fail
    with["responses"] = serde_json::to_value(&responses).unwrap_or(Value::Null);
    let mut out = judge(rule, &check.expr, &with, jq, started);
    out.ran = Ran::Requests(calls);
    out
}
