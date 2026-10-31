//! The lines that a person runs by hand to see what a check saw.

use strata::api::{Method, cluster_events_path};
use strata::config::DEFAULT_BASE_URL;

use super::Subject;
use crate::probe::{Outcome, Ran};
use crate::rule::{Check, Rule};

/// The line a person runs to see the same thing the check saw: the `jq` over the world file, the
/// `curl` to Palette, or the command.
pub(super) fn auto_reproduce(
    rule: &Rule,
    outcome: &Outcome,
    subject: &Subject,
    out_dir: Option<&str>,
) -> Vec<String> {
    // with no --out, the two files are not on disk: $OUT names where --out would put them
    let dir = out_dir.unwrap_or("$OUT");
    let mut lines = Vec::new();
    match &rule.check {
        Some(Check::Static { expr }) => {
            lines.push(format!("jq '{}' {dir}/world.json", one_line(expr)));
        }
        // a request for each item: the lines show each request. A POST body depends on the item,
        // so a POST rule gets its lines from its own `reproduce`. A rule with its own lines for
        // the engine shows those, so that no request has two lines.
        Some(Check::Palette {
            method,
            each: Some(_),
            ..
        }) => {
            if *method == Method::Get && rule.reproduce.for_engine(&subject.cloud).is_empty() {
                for call in outcome.ran.requests() {
                    lines.push(curl_get(&call.path));
                }
            }
        }
        Some(Check::Palette {
            method, expr, body, ..
        }) => {
            let path = match &outcome.ran {
                Ran::Request(call) => call.path.as_str(),
                // the templates did not fill, so the check made no request
                _ => "",
            };
            let data = match (method, body) {
                // a rule with its own body program sends what the program gives, not the entity
                (Method::Post, Some(program)) => {
                    lines.push(format!(
                        "jq '{}' {dir}/world.json > request.json",
                        one_line(program)
                    ));
                    " -X POST -H 'Content-Type: application/json' --data @request.json".to_string()
                }
                (Method::Post, None) => format!(
                    " -X POST -H 'Content-Type: application/json' --data @{dir}/entity.json"
                ),
                (Method::Get, _) => String::new(),
            };
            lines.push(format!(
                "code=$(curl -s -o body.json -w '%{{http_code}}' -H \"ApiKey: $PALETTE_API_KEY\" -H \"ProjectUid: $PALETTE_PROJECT_UID\"{data} \"${{PALETTE_API:-{base}}}{path}\")",
                base = DEFAULT_BASE_URL
            ));
            lines.push(format!(
                "jq --arg code \"$code\" --slurpfile body body.json '.response = {{ok: (($code | tonumber) < 300), status: ($code | tonumber), body: ($body[0] // null)}} | {}' {dir}/world.json",
                one_line(expr)
            ));
        }
        Some(Check::Exec { expr, .. }) => {
            if let Ran::Exec { line, .. } = &outcome.ran {
                lines.push(line.clone());
                lines.push(format!(
                    "jq --argjson out \"$(<the output>)\" '.exec = {{exit: 0, stdout: $out, stderr: \"\"}} | {}' {dir}/world.json",
                    one_line(expr)
                ));
            }
        }
        Some(Check::Watch { .. }) => {
            if let Some(uid) = &subject.cluster_uid {
                lines.push(curl_get(&cluster_events_path(uid)));
            }
        }
        None => {}
    }
    lines
}

/// The `curl` that reads `path` of Palette.
fn curl_get(path: &str) -> String {
    format!(
        "curl -s -H \"ApiKey: $PALETTE_API_KEY\" -H \"ProjectUid: $PALETTE_PROJECT_UID\" \"${{PALETTE_API:-{DEFAULT_BASE_URL}}}{path}\""
    )
}

fn one_line(expr: &str) -> String {
    expr.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('\'', "'\\''")
}
