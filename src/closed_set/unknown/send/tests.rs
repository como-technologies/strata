//! The tests of the decision, the answer, and the post of the send.

use std::io::{Read, Write};
use std::net::TcpListener;

use super::{ANSWER_FILE, Choice, Facts, Step, keep_answer, post, question, read_answer, step};
use crate::closed_set::unknown::log::Line;
use crate::closed_set::unknown::report::Report;
use crate::error::Error;

/// The facts of a run in a terminal that wrote a line, has an address, and has no setting and no
/// answer. The first run of a user.
fn first_run() -> Facts {
    Facts {
        setting: None,
        answer: None,
        endpoint: Some("http://127.0.0.1:9/".to_string()),
        wrote: true,
        in_ci: false,
        terminal: true,
    }
}

fn setting(text: &str) -> Facts {
    Facts {
        setting: Some(text.to_string()),
        ..first_run()
    }
}

#[test]
fn a_first_run_in_a_terminal_asks_and_nothing_else_asks_or_sends() {
    assert_eq!(step(&first_run()), Step::Ask);
    let nothing = [
        Facts {
            wrote: false,
            ..first_run()
        },
        Facts {
            endpoint: None,
            ..first_run()
        },
        Facts {
            in_ci: true,
            ..first_run()
        },
        Facts {
            terminal: false,
            ..first_run()
        },
    ];
    for facts in nothing {
        assert_eq!(step(&facts), Step::Nothing, "{facts:?}");
    }
}

#[test]
fn the_setting_one_sends_and_the_setting_zero_does_not_whatever_the_answer_or_the_run_is() {
    for answer in [None, Some(Choice::Yes), Some(Choice::No)] {
        for (in_ci, terminal) in [(false, false), (false, true), (true, false), (true, true)] {
            let base = Facts {
                answer,
                in_ci,
                terminal,
                ..first_run()
            };
            let on = Facts {
                setting: Some("1".into()),
                ..base.clone()
            };
            let off = Facts {
                setting: Some("0".into()),
                ..base
            };
            assert_eq!(step(&on), Step::Send, "{on:?}");
            assert_eq!(step(&off), Step::Nothing, "{off:?}");
        }
    }
}

#[test]
fn a_setting_that_is_neither_one_nor_zero_sends_nothing_and_asks_nothing() {
    for text in ["yes", "true", "on", "2", "1 ", "report"] {
        let facts = setting(text);
        assert_eq!(
            facts.complaint().unwrap(),
            "STRATA_REPORT must be 1 or 0, so strata sends nothing",
            "{text}"
        );
        assert_eq!(step(&facts), Step::Nothing, "{text}");
    }
    assert_eq!(setting("1").complaint(), None);
    assert_eq!(setting("0").complaint(), None);
    assert_eq!(first_run().complaint(), None);
    // a run that wrote no line has nothing to send, so it says nothing
    let quiet = Facts {
        wrote: false,
        ..setting("yes")
    };
    assert_eq!(quiet.complaint(), None);
}

#[test]
fn the_setting_one_with_no_endpoint_sends_nothing_and_names_the_setting_that_gives_one() {
    let facts = Facts {
        endpoint: None,
        ..setting("1")
    };
    assert_eq!(step(&facts), Step::Nothing);
    assert_eq!(
        facts.complaint().unwrap(),
        "the send of the report is on, and this release names no endpoint: set STRATA_REPORT_URL"
    );
    // the answer of yes is the same choice as the setting 1
    let saved = Facts {
        endpoint: None,
        answer: Some(Choice::Yes),
        ..first_run()
    };
    assert_eq!(step(&saved), Step::Nothing);
    assert_eq!(saved.complaint(), facts.complaint());
    // the answer of no, and the setting 0 over a saved yes, have nothing to complain about
    assert_eq!(
        Facts {
            answer: Some(Choice::No),
            ..saved.clone()
        }
        .complaint(),
        None
    );
    assert_eq!(
        Facts {
            setting: Some("0".into()),
            ..saved
        }
        .complaint(),
        None
    );
    // with no setting there is nothing to complain about: strata asks nothing and sends nothing
    assert_eq!(
        Facts {
            endpoint: None,
            ..first_run()
        }
        .complaint(),
        None
    );
}

#[test]
fn the_answer_of_the_user_decides_without_a_setting_and_never_asks_again() {
    let yes = Facts {
        answer: Some(Choice::Yes),
        ..first_run()
    };
    let no = Facts {
        answer: Some(Choice::No),
        ..first_run()
    };
    assert_eq!(step(&yes), Step::Send);
    assert_eq!(step(&no), Step::Nothing);
    // the answer holds also in CI and without a terminal: the user chose it
    assert_eq!(
        step(&Facts {
            in_ci: true,
            terminal: false,
            ..yes
        }),
        Step::Send
    );
}

#[test]
fn the_answer_is_one_or_zero_in_a_file_and_any_other_text_is_no_answer() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("state").join(ANSWER_FILE);
    assert_eq!(read_answer(&file), None);
    keep_answer(&file, Choice::Yes).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "1\n");
    assert_eq!(read_answer(&file), Some(Choice::Yes));
    keep_answer(&file, Choice::No).unwrap();
    assert_eq!(read_answer(&file), Some(Choice::No));
    for damaged in ["", "yes\n", "2\n"] {
        std::fs::write(&file, damaged).unwrap();
        assert_eq!(read_answer(&file), None, "{damaged:?}");
    }
}

#[test]
fn the_question_names_the_fields_of_the_report_and_the_page_and_the_default_is_no() {
    let text = question();
    assert!(text.starts_with("Keep strata informed of what it does not know?"));
    assert!(text.contains("only the fields of `strata report`"));
    assert!(text.contains("reference/report.html"));
    assert!(text.ends_with("[y/N] "));
}

fn report_of(lines: &[Line]) -> Report {
    Report::of(lines)
}

/// A change to a line.
type Change = fn(&mut Line);

fn line(value: &str) -> Line {
    Line::new("ProfileType", value, "/v1/clusterprofiles/{uid}")
}

#[test]
fn a_report_that_strata_makes_from_its_own_log_passes_the_check_of_the_endpoint() {
    let report = report_of(&[line("a"), line("a"), line("b")]);
    assert_eq!(report.fault(), None);
}

#[test]
fn a_report_with_another_schema_no_entry_or_too_many_entries_is_refused() {
    let good = report_of(&[line("a")]);
    let other = Report {
        schema: "strata-report/2".into(),
        ..good.clone()
    };
    assert_eq!(other.fault().unwrap(), "the schema is not strata-report/1");
    let empty = Report {
        unknown: Vec::new(),
        ..good.clone()
    };
    assert!(empty.fault().unwrap().contains("0 entries"));
    let lines: Vec<Line> = (0..=super::super::report::MOST_ENTRIES)
        .map(|n| line(&format!("v{n}")))
        .collect();
    assert!(report_of(&lines).fault().unwrap().contains("101 entries"));
}

#[test]
fn an_entry_with_a_field_that_is_not_a_short_word_is_refused_and_the_message_names_the_field() {
    let cases: [(&str, Change); 9] = [
        ("the value", |l| l.sighting.value = "x".repeat(65)),
        ("the value", |l| l.sighting.value = "a\nb".into()),
        ("the value", |l| l.sighting.value = "pending\u{200b}".into()),
        ("the route", |l| {
            l.sighting.route = "/v1/clusterprofiles/64f0a1b2c3d4e5f6a7b8c9d0".into();
        }),
        ("the route has more than 200", |l| {
            l.sighting.route = format!("/v1/{}", "z/".repeat(100));
        }),
        ("the route is not a template", |l| {
            l.sighting.route = "/v1/packs\n\n@someone".into();
        }),
        ("the set", |l| l.sighting.set = "Profile Type".into()),
        ("the set", |l| l.sighting.set = String::new()),
        ("the strata", |l| l.sighting.strata = "0.26.0 <b>".into()),
    ];
    for (what, change) in cases {
        let mut broken = line("a");
        change(&mut broken);
        let fault = report_of(&[broken]).fault().unwrap();
        assert!(fault.starts_with("entry 1: "), "{fault}");
        assert!(fault.contains(what), "{what}: {fault}");
    }
    let mut zero = report_of(&[line("a")]);
    zero.unknown[0].count = 0;
    assert!(zero.fault().unwrap().contains("the count is 0"));
}

#[test]
fn an_entry_with_a_count_above_the_most_is_refused_and_the_most_itself_passes() {
    use super::super::report::MOST_COUNT;
    let mut report = report_of(&[line("a")]);
    report.unknown[0].count = MOST_COUNT;
    assert_eq!(report.fault(), None);
    report.unknown[0].count = MOST_COUNT + 1;
    let fault = report.fault().unwrap();
    assert!(
        fault.starts_with("entry 1: the count is more than"),
        "{fault}"
    );
    report.unknown[0].count = usize::MAX;
    assert!(report.fault().is_some());
}

#[test]
fn a_report_document_with_another_field_does_not_parse() {
    let text = r#"{"schema":"strata-report/1","unknown":[{"strata":"0.26.0","set":"S","value":"v","route":"/v1/packs","count":1,"uid":"x"}]}"#;
    let error = serde_json::from_str::<Report>(text)
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown field `uid`"), "{error}");
    let top = r#"{"schema":"strata-report/1","unknown":[],"host":"h"}"#;
    assert!(serde_json::from_str::<Report>(top).is_err());
    // a report that strata writes parses back to the same document
    let report = report_of(&[line("a")]);
    let text = serde_json::to_string(&report).unwrap();
    assert_eq!(serde_json::from_str::<Report>(&text).unwrap(), report);
}

/// An endpoint that answers one request with `status` and `body`, and gives the text it got.
fn answering(status: u16, body: &'static str) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/path", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut got = Vec::new();
        let mut chunk = [0u8; 4096];
        while !String::from_utf8_lossy(&got).contains("\"unknown\"") {
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => got.extend_from_slice(&chunk[..n]),
            }
        }
        let _ = write!(
            stream,
            "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        String::from_utf8_lossy(&got).into_owned()
    });
    (url, handle)
}

#[test]
fn the_post_sends_the_report_as_json_with_the_user_agent_of_strata() {
    let (url, got) = answering(200, "{}");
    post(&url, &report_of(&[line("a")]), 0).unwrap();
    let request = got.join().unwrap().to_ascii_lowercase();
    assert!(request.starts_with("post /path http/1.1"), "{request}");
    assert!(
        request.contains("content-type: application/json"),
        "{request}"
    );
    assert!(
        request.contains(&format!("user-agent: strata/{}", crate::TOOL_VERSION)),
        "{request}"
    );
    assert!(
        request.contains(r#""schema":"strata-report/1""#),
        "{request}"
    );
}

/// The value of the header `Idempotency-Key` in `request`, a lower-case text of a request.
fn key_of(request: &str) -> String {
    request
        .lines()
        .find_map(|l| l.strip_prefix("idempotency-key: "))
        .unwrap_or_default()
        .to_string()
}

#[test]
fn the_key_of_a_post_is_64_hex_characters_and_the_same_lines_at_the_same_place_give_the_same_key() {
    let report = report_of(&[line("a"), line("b")]);
    let mut keys = Vec::new();
    for position in [0, 0, 7] {
        let (url, got) = answering(200, "{}");
        post(&url, &report, position).unwrap();
        keys.push(key_of(&got.join().unwrap().to_ascii_lowercase()));
    }
    assert_eq!(keys[0].len(), super::KEY_LENGTH, "{keys:?}");
    assert!(keys[0].bytes().all(|b| b.is_ascii_hexdigit()), "{keys:?}");
    assert_eq!(keys[0], keys[1], "a second try has the key of the first");
    assert_ne!(
        keys[0], keys[2],
        "the same lines at another place are another send"
    );
    let (url, got) = answering(200, "{}");
    post(&url, &report_of(&[line("a")]), 0).unwrap();
    assert_ne!(keys[0], key_of(&got.join().unwrap().to_ascii_lowercase()));
}

#[test]
fn the_key_holds_no_text_of_the_report() {
    let (url, got) = answering(200, "{}");
    post(&url, &report_of(&[line("hunter2")]), 0).unwrap();
    let request = got.join().unwrap().to_ascii_lowercase();
    assert!(!key_of(&request).contains("hunter2"));
}

#[test]
fn the_post_gives_the_status_and_the_start_of_the_answer_when_the_endpoint_refuses() {
    let (url, _) = answering(
        422,
        r#"{"error":"the entry has a field that is not in the schema"}"#,
    );
    let error = post(&url, &report_of(&[line("a")]), 0).unwrap_err();
    assert!(matches!(error, Error::ReportSend { .. }), "{error}");
    let text = error.to_string();
    assert!(
        text.starts_with(&format!("could not send the report to {url}: ")),
        "{text}"
    );
    assert!(text.contains("HTTP 422"), "{text}");
    assert!(text.contains("not in the schema"), "{text}");
}

#[test]
fn the_post_follows_no_redirect() {
    let (url, _) = answering(307, "");
    let error = post(&url, &report_of(&[line("a")]), 0)
        .unwrap_err()
        .to_string();
    assert!(error.contains("HTTP 307"), "{error}");
}

#[test]
fn the_post_refuses_a_report_that_is_not_valid_before_it_calls_the_endpoint() {
    let mut report = report_of(&[line("a")]);
    report.unknown[0].sighting.value = "x".repeat(65);
    // nothing listens here: the error must come from the check, not from the connection
    let error = post("http://127.0.0.1:9/", &report, 0)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("the report is not valid: entry 1: the value"),
        "{error}"
    );
}

#[test]
fn the_post_to_an_address_that_refuses_the_connection_is_an_error_of_the_send() {
    let error = post("http://127.0.0.1:9/", &report_of(&[line("a")]), 0).unwrap_err();
    assert!(matches!(error, Error::ReportSend { .. }), "{error}");
    assert_eq!(error.kind(), "report-send");
}
