//! The lines that a person runs by hand to see what a check saw.

use strata::api::{API_KEY_HEADER, Method, PROJECT_UID_HEADER, cluster_events_path};
use strata::config::{DEFAULT_BASE_URL, KEY_VARS, PROJECT_VARS, URL_VARS};

use super::Subject;
use crate::jq;
use crate::probe::{Invocation, Outcome, Ran};
use crate::rule::{Check, ExecCheck, PaletteCheck, Rule};

/// The jq object that a program reads as `.response`, built from the `curl` of a request. The
/// status code is in `$code` and the body is in `$body`.
const RESPONSE_OF_CURL: &str =
    "{ok: (($code | tonumber) < 300), status: ($code | tonumber), body: ($body[0] // null)}";

/// The line that a person runs to see the same thing that the check saw. It is the `jq` over the
/// world file, the `curl` to Palette, or the command.
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
            lines.push(format!("jq '{}' {dir}/world.json", tool_program("", expr)));
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
                        tool_program("", program)
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
                "jq --arg code \"$code\" --slurpfile body body.json '{}' {dir}/world.json",
                tool_program(&format!(".response = {RESPONSE_OF_CURL} | "), expr)
            ));
        }
        Some(Check::Exec(ExecCheck { expr, .. })) => exec_lines(expr, outcome, dir, &mut lines),
        Some(Check::Watch { .. }) => {
            if let Some(uid) = &subject.cluster_uid {
                lines.push(curl_get(&cluster_events_path(uid)));
            }
        }
        None => {}
    }
    lines
}

/// The lines of a command check: the `curl` of its read of Palette when it has one, then the
/// command. The second command follows when it has one. The `jq` over the world with the answers
/// is the last line. The program reads `.exec`, `.also` after a second command, and `.response`
/// after a read.
fn exec_lines(expr: &str, outcome: &Outcome, dir: &str, lines: &mut Vec<String>) {
    let Ran::Exec {
        first,
        also,
        request,
    } = &outcome.ran
    else {
        return;
    };
    let (mut flags, mut set) = match request {
        Some(call) => {
            lines.push(format!(
                "code=$({})",
                curl_line(" -o body.json -w '%{http_code}'", "", &call.path)
            ));
            (
                "--arg code \"$code\" --slurpfile body body.json ".to_string(),
                format!(".response = {RESPONSE_OF_CURL} | "),
            )
        }
        None => (String::new(), String::new()),
    };
    lines.push(first.line.clone());
    flags.push_str("--argjson out \"$(<the output>)\" ");
    set.push_str(&output_in_world("exec", "out", first));
    if let Some(also) = also {
        lines.push(also.line.clone());
        flags.push_str("--argjson also \"$(<the output of the second command>)\" ");
        set.push_str(&output_in_world("also", "also", also));
    }
    lines.push(format!(
        "jq {flags}'{}' {dir}/world.json",
        tool_program(&set, expr)
    ));
}

/// The step of the `jq` program that puts the output of `command` in the world as `.key`. The
/// exit code is the one that the command gave. A command that did not start has none, so the step
/// has a placeholder for the exit code.
fn output_in_world(key: &str, output: &str, command: &Invocation) -> String {
    let exit = command
        .exit
        .map_or_else(|| "<the exit code>".to_string(), |exit| exit.to_string());
    format!(".{key} = {{exit: {exit}, stdout: ${output}, stderr: \"\"}} | ")
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

/// The program of a `jq` line, for single quotes: `prefix` and `program` on one line. A comment
/// goes, because the join puts the rest of the program in it. The `jq` tool lacks the filters
/// that the engine adds, so a definition of each filter that `program` names goes in front.
fn tool_program(prefix: &str, program: &str) -> String {
    one_line(&format!(
        "{}{prefix}{}",
        jq::tool_definitions(program),
        jq::without_comments(program)
    ))
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

    #[test]
    fn a_program_that_names_semver_numbers_starts_with_its_definition_and_another_does_not() {
        let line = tool_program(".x = 1 | ", "semver_numbers");
        assert!(line.starts_with("def semver_numbers: "), "{line}");
        assert!(line.ends_with("; .x = 1 | semver_numbers"), "{line}");
        assert_eq!(tool_program("", ".a | length"), ".a | length");
    }

    #[test]
    fn a_program_with_a_comment_gives_one_line_with_no_comment() {
        assert_eq!(tool_program("", "1 # a note\n  | 2\n"), "1 | 2");
        assert_eq!(tool_program("", "'a' # it's"), "'\\''a'\\''");
    }
}
