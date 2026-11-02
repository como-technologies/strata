//! `strata report`: the report of the unknown values that the log holds, and the link that opens a
//! new issue with it. The unit tests of the log, the report, and the hint are in
//! `src/closed_set/unknown/tests.rs`. These tests run the binary and the operations.

use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use serde_json::{Value, json};
use strata::TOOL_VERSION;
use strata::closed_set::unknown::log::{FILE, SCHEMA as LOG_SCHEMA, SENT_FILE};
use strata::closed_set::unknown::report::SCHEMA as REPORT_SCHEMA;
use strata::ops::{ReportOptions, mark_reported, report_link, unknown_report};

use crate::common::{LATER_TYPE, Run, log_in, log_line, report_command, write_lines, write_log};

/// The lines of a log with a value that two runs gave, and a value that one run gave.
fn two_values() -> Vec<String> {
    let type_line = log_line("ProfileType", LATER_TYPE, "/v1/clusterprofiles/{uid}");
    vec![
        type_line.clone(),
        log_line("PackState", "retired", "/v1/packs"),
        type_line,
    ]
}

/// Run `strata report` with the arguments `more` and a home of its own, `home`.
fn report(home: &Path, more: &[&str]) -> Run {
    Run::of(&mut report_command(home, more))
}

/// The text that a link holds, with each `%XX` read as a byte.
fn decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            out.push(u8::from_str_radix(&text[at + 1..at + 3], 16).unwrap());
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

/// The title and the body of the link `link`, decoded.
fn title_and_body(link: &str) -> (String, String) {
    let query = link.trim().split_once('?').unwrap().1;
    let (title, body) = query.split_once("&body=").unwrap();
    (
        decoded(title.strip_prefix("title=").unwrap()),
        decoded(body),
    )
}

#[test]
fn report_prints_each_line_of_the_log_once_with_its_count() {
    let home = tempfile::tempdir().unwrap();
    write_log(home.path(), &two_values());
    let run = report(home.path(), &[]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert_eq!(run.err, "");
    let rows: Vec<&str> = run.out.lines().filter(|l| l.starts_with("  ")).collect();
    assert_eq!(rows.len(), 2, "{}", run.out);
    assert!(rows[0].starts_with("  PackState"), "{}", rows[0]);
    assert!(
        rows[0].contains("1 time ") && rows[0].contains(&format!("strata {TOOL_VERSION}")),
        "{}",
        rows[0]
    );
    assert!(rows[1].starts_with("  ProfileType"), "{}", rows[1]);
    assert!(rows[1].contains("2 times"), "{}", rows[1]);
    assert!(
        run.out
            .starts_with("Palette gave 2 values that this release of strata does not know\n"),
        "{}",
        run.out
    );
}

#[test]
fn report_json_is_the_document_with_the_fields_of_the_log_and_a_count() {
    let home = tempfile::tempdir().unwrap();
    write_log(home.path(), &two_values());
    let run = report(home.path(), &["--json"]);
    assert_eq!(run.code, 0, "{}", run.err);
    let document: Value = serde_json::from_str(&run.out).unwrap();
    assert_eq!(
        document,
        json!({
            "schema": REPORT_SCHEMA,
            "unknown": [
                {"strata": TOOL_VERSION, "set": "PackState", "value": "retired",
                    "route": "/v1/packs", "count": 1},
                {"strata": TOOL_VERSION, "set": "ProfileType", "value": LATER_TYPE,
                    "route": "/v1/clusterprofiles/{uid}", "count": 2},
            ],
        })
    );
    // the same document with --format json and --compact
    let compact = report(home.path(), &["--format", "json", "--compact"]);
    assert_eq!(
        serde_json::from_str::<Value>(&compact.out).unwrap(),
        document
    );
    assert_eq!(compact.out.lines().count(), 1);
}

#[test]
fn report_markdown_is_a_table_that_serves_as_the_body_of_an_issue() {
    let home = tempfile::tempdir().unwrap();
    write_log(home.path(), &two_values());
    let run = report(home.path(), &["--format", "markdown"]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(
        run.out
            .starts_with("Palette gave 2 values that this release of strata does not know.\n"),
        "{}",
        run.out
    );
    assert!(
        run.out
            .contains("| set | value | route | count | strata | Palette | status | error code |"),
        "{}",
        run.out
    );
    assert!(
        run.out.contains(&format!(
            "| `ProfileType` | `{LATER_TYPE}` | `/v1/clusterprofiles/{{uid}}` | 2 | {TOOL_VERSION} | - | - | - |"
        )),
        "{}",
        run.out
    );
}

#[test]
fn report_of_a_log_that_is_not_there_says_that_palette_gave_no_value_and_exits_0() {
    let home = tempfile::tempdir().unwrap();
    let run = report(home.path(), &[]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert_eq!(
        run.out,
        "Palette gave no values that this release of strata does not know\n"
    );
    let json = report(home.path(), &["--json"]);
    assert_eq!(
        serde_json::from_str::<Value>(&json.out).unwrap(),
        json!({"schema": REPORT_SCHEMA, "unknown": []})
    );
    assert!(
        !log_in(home.path()).exists(),
        "a report must not make the log"
    );
}

#[test]
fn a_log_with_a_uid_in_a_line_fails_the_read_and_nothing_is_printed_or_marked() {
    let home = tempfile::tempdir().unwrap();
    let mut with_uid: Value =
        serde_json::from_str(&log_line("PackState", "retired", "/v1/packs")).unwrap();
    with_uid["uid"] = json!("64f0000000000000000000a1");
    let mut lines = two_values();
    lines.push(with_uid.to_string());
    write_log(home.path(), &lines);
    for more in [
        vec![],
        vec!["--link"],
        vec!["--sent"],
        vec!["--format", "markdown"],
    ] {
        let run = report(home.path(), &more);
        assert_eq!(run.code, 2, "{more:?}: {}", run.err);
        assert_eq!(run.out, "", "{more:?}");
        assert!(
            run.err.contains(&format!(
                "line 4 is not a line of {LOG_SCHEMA}: unknown field `uid`"
            )),
            "{}",
            run.err
        );
    }
    assert!(
        !log_in(home.path()).with_file_name(SENT_FILE).exists(),
        "a log that fails the read must not be marked"
    );
    let json = report(home.path(), &["--json", "--quiet"]);
    assert_eq!(json.code, 2);
    let error: Value = serde_json::from_str(&json.err).unwrap();
    assert_eq!(error["error"]["kind"], "log-invalid");
    assert_eq!(error["error"]["line"], 4);
}

#[test]
fn link_prints_the_address_of_a_new_issue_that_holds_the_same_text_as_the_markdown_report() {
    let home = tempfile::tempdir().unwrap();
    write_log(home.path(), &two_values());
    let link = report(home.path(), &["--link"]);
    assert_eq!(link.code, 0, "{}", link.err);
    assert_eq!(link.err, "");
    assert_eq!(link.out.lines().count(), 1, "{}", link.out);
    assert!(
        link.out.starts_with(&format!(
            "{}/issues/new?title=",
            env!("CARGO_PKG_REPOSITORY")
        )),
        "{}",
        link.out
    );
    let (title, body) = title_and_body(&link.out);
    assert_eq!(title, "Values that Palette gave and strata does not know");
    let markdown = report(home.path(), &["--format", "markdown"]);
    assert_eq!(
        body, markdown.out,
        "the body of the issue is the markdown report"
    );
    // the address holds no newline, no space, and no pipe: a browser takes it whole
    assert!(!link.out.trim().contains([' ', '|', '`']), "{}", link.out);
}

#[test]
fn link_sends_no_request() {
    let home = tempfile::tempdir().unwrap();
    write_log(home.path(), &two_values());
    // a server that counts what reaches it, in the place where Palette is
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&count);
    let (stop, stopped) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        while stopped.recv_timeout(Duration::from_millis(5)).is_err() {
            if listener.accept().is_ok() {
                seen.fetch_add(1, Ordering::SeqCst);
            }
        }
    });
    for more in [
        vec!["--link"],
        vec![],
        vec!["--json"],
        vec!["--link", "--sent"],
    ] {
        let mut command = report_command(home.path(), &more);
        command
            .env("PALETTE_API", format!("http://{address}"))
            .env("HTTPS_PROXY", format!("http://{address}"))
            .env("HTTP_PROXY", format!("http://{address}"));
        let run = Run::of(&mut command);
        assert_eq!(run.code, 0, "{more:?}: {}", run.err);
    }
    stop.send(()).unwrap();
    assert_eq!(
        count.load(Ordering::SeqCst),
        0,
        "a request reached the server"
    );
}

#[test]
fn the_code_of_report_makes_no_request_and_names_no_client() {
    for file in ["src/cli/report.rs", "src/ops/report.rs"] {
        let source =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(file)).unwrap();
        let code = source.split("#[cfg(test)]").next().unwrap_or(&source);
        for client in [
            "ureq",
            "HttpApi",
            "connect",
            "TcpStream",
            "crate::api",
            "strata::api",
        ] {
            assert!(!code.contains(client), "{file} names {client}");
        }
    }
}

#[test]
fn link_of_an_empty_report_prints_the_sentence_and_no_address() {
    let home = tempfile::tempdir().unwrap();
    let run = report(home.path(), &["--link"]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert_eq!(
        run.out,
        "Palette gave no values that this release of strata does not know\n"
    );
}

#[test]
fn link_of_a_report_that_is_too_long_for_an_address_says_to_paste_the_markdown() {
    let home = tempfile::tempdir().unwrap();
    let lines: Vec<String> = (0..60)
        .map(|n| {
            log_line(
                "ProfileType",
                &format!("value-{n}"),
                "/v1/clusterprofiles/{uid}",
            )
        })
        .collect();
    write_log(home.path(), &lines);
    let run = report(home.path(), &["--link"]);
    assert_eq!(run.code, 2, "{}", run.err);
    assert_eq!(run.out, "");
    assert!(
        run.err.contains("the report is too long for a link"),
        "{}",
        run.err
    );
    assert!(
        run.err.contains("strata report --format markdown"),
        "{}",
        run.err
    );
    assert!(
        run.err
            .contains(&format!("{}/issues/new", env!("CARGO_PKG_REPOSITORY"))),
        "{}",
        run.err
    );
    // the markdown report has no such limit
    assert_eq!(report(home.path(), &["--format", "markdown"]).code, 0);
}

#[test]
fn a_link_that_is_too_long_marks_nothing_with_sent_so_the_markdown_still_holds_the_lines() {
    let home = tempfile::tempdir().unwrap();
    let lines: Vec<String> = (0..60)
        .map(|n| {
            log_line(
                "ProfileType",
                &format!("value-{n}"),
                "/v1/clusterprofiles/{uid}",
            )
        })
        .collect();
    write_log(home.path(), &lines);
    let run = report(home.path(), &["--link", "--sent"]);
    assert_eq!(run.code, 2, "{}", run.err);
    assert!(run.err.contains("too long for a link"), "{}", run.err);
    assert!(
        !log_in(home.path()).with_file_name(SENT_FILE).exists(),
        "the user sent nothing, so strata marks nothing"
    );
    let markdown = report(home.path(), &["--format", "markdown"]);
    assert_eq!(markdown.code, 0, "{}", markdown.err);
    assert!(markdown.out.contains("value-59"), "{}", markdown.out);
    assert!(!markdown.out.contains("gave no values"), "{}", markdown.out);
}

#[test]
fn a_link_that_fits_is_printed_and_then_marked_with_sent() {
    let home = tempfile::tempdir().unwrap();
    write_log(home.path(), &two_values());
    let run = report(home.path(), &["--link", "--sent"]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(run.out.contains("/issues/new?title="), "{}", run.out);
    assert_eq!(
        std::fs::read_to_string(log_in(home.path()).with_file_name(SENT_FILE)).unwrap(),
        "3\n"
    );
}

#[test]
fn link_does_not_take_a_format() {
    let home = tempfile::tempdir().unwrap();
    for more in [["--link", "--json"], ["--link", "--format=markdown"]] {
        let run = report(home.path(), &more);
        assert_eq!(run.code, 2, "{more:?}");
        assert!(run.err.contains("cannot be used with"), "{}", run.err);
    }
}

#[test]
fn sent_marks_the_lines_so_that_the_next_report_holds_only_the_new_lines() {
    let home = tempfile::tempdir().unwrap();
    write_log(home.path(), &two_values());
    // a report alone marks nothing: the user decides when the lines went out
    assert_eq!(report(home.path(), &[]).code, 0);
    assert!(report(home.path(), &[]).out.contains(LATER_TYPE));
    assert!(!log_in(home.path()).with_file_name(SENT_FILE).exists());

    let first = report(home.path(), &["--sent"]);
    assert_eq!(first.code, 0, "{}", first.err);
    assert!(
        first.out.contains(LATER_TYPE),
        "the report still shows the lines"
    );
    assert_eq!(
        std::fs::read_to_string(log_in(home.path()).with_file_name(SENT_FILE)).unwrap(),
        "3\n"
    );

    let second = report(home.path(), &[]);
    assert_eq!(
        second.out,
        "Palette gave no values that this release of strata does not know\n"
    );
    // a new line is the only line of the next report; --all holds each line again
    let mut lines = two_values();
    lines.push(log_line(
        "OciProvider",
        "oras",
        "/v1/registries/oci/summary",
    ));
    write_log(home.path(), &lines);
    let third = report(home.path(), &[]);
    assert!(
        third.out.contains("oras") && !third.out.contains(LATER_TYPE),
        "{}",
        third.out
    );
    let all = report(home.path(), &["--all"]);
    assert!(
        all.out.contains("oras") && all.out.contains(LATER_TYPE),
        "{}",
        all.out
    );
    assert!(all.out.starts_with("Palette gave 3 values"), "{}", all.out);
    // the log is the user's file: no report changes it
    assert_eq!(
        std::fs::read_to_string(log_in(home.path()))
            .unwrap()
            .lines()
            .count(),
        4
    );
}

#[test]
fn a_mark_past_the_end_of_the_log_counts_for_nothing() {
    let home = tempfile::tempdir().unwrap();
    write_log(home.path(), &two_values());
    std::fs::write(log_in(home.path()).with_file_name(SENT_FILE), "10\n").unwrap();
    let run = report(home.path(), &[]);
    assert!(
        run.out.contains(LATER_TYPE),
        "a new log is reported whole: {}",
        run.out
    );
}

#[test]
fn the_operations_read_a_log_by_its_path_and_mark_only_when_asked() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(FILE);
    write_lines(&file, &two_values());
    let plain = unknown_report(&file, ReportOptions::default()).unwrap();
    assert_eq!(plain.report.unknown.len(), 2);
    assert_eq!(plain.lines, 3);
    assert!(
        !dir.path().join(SENT_FILE).exists(),
        "the read writes no mark"
    );
    mark_reported(&file, plain.lines).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join(SENT_FILE)).unwrap(),
        "3\n"
    );
    assert!(
        unknown_report(&file, ReportOptions::default())
            .unwrap()
            .report
            .unknown
            .is_empty()
    );
    let all = unknown_report(&file, ReportOptions { all: true }).unwrap();
    assert_eq!(all, plain);
}

#[test]
fn a_link_of_an_empty_report_is_none_and_a_long_one_is_a_usage_error() {
    let empty = strata::closed_set::unknown::Report::of(&[]);
    assert_eq!(report_link(&empty, "").unwrap(), None);
    let lines: Vec<strata::closed_set::unknown::Line> = (0..2)
        .map(|n| {
            serde_json::from_str(&log_line("PackState", &format!("value-{n}"), "/v1/packs"))
                .unwrap()
        })
        .collect();
    let report = strata::closed_set::unknown::Report::of(&lines);
    let link = report_link(&report, "short body").unwrap().unwrap();
    assert!(link.ends_with("&body=short%20body"), "{link}");
    let error = report_link(&report, &"x".repeat(7000)).unwrap_err();
    assert_eq!(error.kind(), "usage");
}
