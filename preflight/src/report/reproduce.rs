//! The lines that a person runs by hand to see what a check saw.

use strata::api::{API_KEY_HEADER, Method, PROJECT_UID_HEADER, cluster_events_path};
use strata::config::{DEFAULT_BASE_URL, KEY_VARS, PROJECT_VARS, URL_VARS};

use super::Subject;
use crate::probe::{Outcome, Ran};
use crate::rule::{Check, PaletteCheck, Rule};

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
        Some(Check::Palette(PaletteCheck {
            method,
            each: Some(_),
            ..
        })) => {
            if *method == Method::Get && rule.reproduce.for_engine(&subject.cloud).is_empty() {
                for call in outcome.ran.requests() {
                    lines.push(curl_get(&call.path));
                }
            }
        }
        Some(Check::Palette(PaletteCheck {
            method, expr, body, ..
        })) => {
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
                "code=$({})",
                curl_line(" -o body.json -w '%{http_code}'", &data, path)
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
    curl_line("", "", path)
}

/// A `curl` command that sends the key and the project to `path` of Palette. The names of the
/// headers and of the variables are the ones that strata reads. `options` go before the headers,
/// and `data` goes after them.
fn curl_line(options: &str, data: &str, path: &str) -> String {
    format!(
        "curl -s{options} -H \"{API_KEY_HEADER}: ${key}\" -H \"{PROJECT_UID_HEADER}: ${project}\"{data} \"${{{url}:-{DEFAULT_BASE_URL}}}{path}\"",
        key = KEY_VARS[0],
        project = PROJECT_VARS[0],
        url = URL_VARS[0]
    )
}

fn one_line(expr: &str) -> String {
    expr.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('\'', "'\\''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_curl_line_names_the_headers_and_the_variables_that_strata_reads() {
        let line = curl_get("/v1/x");
        assert!(line.starts_with("curl -s -H "), "{line}");
        for part in [
            format!("-H \"{API_KEY_HEADER}: ${}\"", KEY_VARS[0]),
            format!("-H \"{PROJECT_UID_HEADER}: ${}\"", PROJECT_VARS[0]),
            format!("\"${{{}:-{DEFAULT_BASE_URL}}}/v1/x\"", URL_VARS[0]),
        ] {
            assert!(line.contains(&part), "{part} is not in {line}");
        }
    }

    #[test]
    fn a_curl_line_with_options_and_data_puts_the_options_before_the_headers() {
        let line = curl_line(" -o body.json", " -X POST", "/v1/y");
        let headers = line.find(" -H ").expect("the headers");
        assert!(line.find(" -o body.json").unwrap() < headers, "{line}");
        assert!(line.find(" -X POST").unwrap() > headers, "{line}");
        assert!(line.ends_with("/v1/y\""), "{line}");
    }
}
