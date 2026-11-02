//! The opt-in send of the report to the endpoint. The unit tests of the decision, the post, and
//! the question are in `src/closed_set/unknown/tests.rs` and `src/term/ask.rs`. These tests run the
//! binary against a stand-in Palette and a stand-in endpoint.

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use strata::closed_set::unknown::log::{FILE, SENT_FILE};
use strata::closed_set::unknown::send::ANSWER_FILE;

use crate::common::{
    Answer, Endpoint, Run, fake_bin, isolate_palette, later_type_tenant, list_command, log_in,
    path_with, serve_routes, strata_bin,
};

/// The report that one run of `list` sends: one value, seen once.
fn one_run_report() -> Value {
    json!({
        "schema": "strata-report/1",
        "unknown": [{
            "strata": strata::TOOL_VERSION,
            "set": "ProfileType",
            "value": "later-type",
            "route": "/v1/clusterprofiles/{uid}",
            "count": 1,
        }],
    })
}

/// A run of `strata list profiles --packs` that has the settings `env`, and gets its home from
/// `home`.
fn list_with(api: &str, home: &Path, env: &[(&str, &str)], more: &[&str]) -> Run {
    let mut command = list_command(api, home, more);
    command.envs(env.iter().copied());
    Run::of(&mut command)
}

/// The state folder of the run that used `home` as its home.
fn state_in(home: &Path) -> std::path::PathBuf {
    log_in(home).parent().unwrap().to_path_buf()
}

#[test]
fn with_no_setting_strata_sends_nothing_and_asks_nothing() {
    let api = serve_routes(later_type_tenant());
    let endpoint = Endpoint::answering(Answer::Status(200));
    let home = tempfile::tempdir().unwrap();
    let run = list_with(
        &api,
        home.path(),
        &[("STRATA_REPORT_URL", &endpoint.url)],
        &[],
    );
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(
        run.err.contains("Palette gave the ProfileType"),
        "{}",
        run.err
    );
    assert!(!run.err.contains("[y/N]"), "{}", run.err);
    assert!(endpoint.requests().is_empty(), "{:?}", endpoint.requests());
    assert!(
        !state_in(home.path()).join(ANSWER_FILE).exists(),
        "a run that asked nothing keeps no answer"
    );
}

#[test]
fn a_run_in_ci_with_no_setting_sends_nothing_and_asks_nothing() {
    let api = serve_routes(later_type_tenant());
    let endpoint = Endpoint::answering(Answer::Status(200));
    let home = tempfile::tempdir().unwrap();
    let run = list_with(
        &api,
        home.path(),
        &[("CI", "true"), ("STRATA_REPORT_URL", &endpoint.url)],
        &[],
    );
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(!run.err.contains("[y/N]"), "{}", run.err);
    assert!(endpoint.requests().is_empty());
}

#[test]
fn the_setting_zero_sends_nothing_also_when_the_user_said_yes_before() {
    let api = serve_routes(later_type_tenant());
    let endpoint = Endpoint::answering(Answer::Status(200));
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(state_in(home.path())).unwrap();
    std::fs::write(state_in(home.path()).join(ANSWER_FILE), "1\n").unwrap();
    let env = [
        ("STRATA_REPORT", "0"),
        ("STRATA_REPORT_URL", endpoint.url.as_str()),
    ];
    let run = list_with(&api, home.path(), &env, &[]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(endpoint.requests().is_empty());
}

#[test]
fn a_setting_that_is_not_one_or_zero_sends_nothing_and_says_so() {
    let api = serve_routes(later_type_tenant());
    let endpoint = Endpoint::answering(Answer::Status(200));
    let home = tempfile::tempdir().unwrap();
    let env = [
        ("STRATA_REPORT", "yes"),
        ("STRATA_REPORT_URL", endpoint.url.as_str()),
    ];
    let run = list_with(&api, home.path(), &env, &[]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(
        run.err
            .contains("strata: warning: STRATA_REPORT must be 1 or 0, so strata sends nothing"),
        "{}",
        run.err
    );
    assert!(endpoint.requests().is_empty());
}

#[test]
fn offline_preflight_sends_nothing_though_the_setting_is_on_and_the_log_holds_a_line() {
    let endpoint = Endpoint::answering(Answer::Status(200));
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(state_in(home.path())).unwrap();
    std::fs::write(
        log_in(home.path()),
        format!(
            "{}\n",
            json!({"schema": "strata-unknown/1", "strata": strata::TOOL_VERSION,
                "set": "ProfileType", "value": "later-type", "route": "/v1/packs"})
        ),
    )
    .unwrap();
    // preflight is the other binary, and offline it reads nothing from Palette
    let bin = fake_bin(home.path(), &[("strata-preflight", "#!/bin/sh\nexit 0\n")]);
    let mut command = Command::new(strata_bin());
    isolate_palette(&mut command, home.path());
    command
        .env_remove("XDG_STATE_HOME")
        .env("PATH", path_with(&bin))
        .env("STRATA_REPORT", "1")
        .env("STRATA_REPORT_URL", &endpoint.url)
        .args(["preflight", "cluster.json", "--tier", "static", "--offline"]);
    let run = Run::of(&mut command);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(endpoint.requests().is_empty());
}

#[test]
fn with_the_setting_on_the_run_sends_the_report_one_time_at_its_end_and_marks_the_lines() {
    let api = serve_routes(later_type_tenant());
    let endpoint = Endpoint::answering(Answer::Status(200));
    let home = tempfile::tempdir().unwrap();
    let env = [
        ("STRATA_REPORT", "1"),
        ("STRATA_REPORT_URL", endpoint.url.as_str()),
    ];
    let run = list_with(&api, home.path(), &env, &[]);
    assert_eq!(run.code, 0, "{}", run.err);
    let requests = endpoint.requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert_eq!(
        (requests[0].method.as_str(), requests[0].path.as_str()),
        ("POST", "/")
    );
    assert_eq!(requests[0].json(), one_run_report());
    assert!(
        run.err.contains("sent the report of the unknown values to"),
        "{}",
        run.err
    );
    // the endpoint took the report, so the line is sent: a report after the run is empty
    let sent = std::fs::read_to_string(state_in(home.path()).join(SENT_FILE)).unwrap();
    assert_eq!(sent.trim(), "1");
    // the next run sends only the line that it wrote
    let again = list_with(&api, home.path(), &env, &[]);
    assert_eq!(again.code, 0, "{}", again.err);
    let requests = endpoint.requests();
    assert_eq!(requests.len(), 2, "{requests:?}");
    assert_eq!(requests[1].json(), one_run_report());
}

#[test]
fn the_setting_in_the_palette_env_file_turns_the_send_on() {
    let api = serve_routes(later_type_tenant());
    let endpoint = Endpoint::answering(Answer::Status(204));
    let home = tempfile::tempdir().unwrap();
    std::fs::write(
        home.path().join(".palette.env"),
        format!("STRATA_REPORT=1\nSTRATA_REPORT_URL={}\n", endpoint.url),
    )
    .unwrap();
    let run = list_with(&api, home.path(), &[], &[]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert_eq!(endpoint.requests().len(), 1);
}

#[test]
fn a_run_that_saw_no_unknown_value_sends_nothing() {
    let endpoint = Endpoint::answering(Answer::Status(200));
    let home = tempfile::tempdir().unwrap();
    let mut command = Command::new(strata_bin());
    isolate_palette(&mut command, home.path());
    command
        .env_remove("XDG_STATE_HOME")
        .env("STRATA_REPORT", "1")
        .env("STRATA_REPORT_URL", &endpoint.url)
        .args(["schema", "report"]);
    let run = Run::of(&mut command);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(endpoint.requests().is_empty());
}

#[test]
fn an_error_of_the_send_does_not_change_the_exit_code_or_the_output_and_marks_nothing() {
    let api = serve_routes(later_type_tenant());
    let quiet_home = tempfile::tempdir().unwrap();
    let plain = list_with(&api, quiet_home.path(), &[], &[]);
    assert_eq!(plain.code, 0, "{}", plain.err);
    let refusing = Endpoint::answering(Answer::Status(503));
    // an address that refuses the connection, and one that answers 503
    for url in ["http://127.0.0.1:9/", refusing.url.as_str()] {
        let home = tempfile::tempdir().unwrap();
        let env = [("STRATA_REPORT", "1"), ("STRATA_REPORT_URL", url)];
        let run = list_with(&api, home.path(), &env, &[]);
        assert_eq!(run.code, plain.code, "{}", run.err);
        assert_eq!(run.out, plain.out);
        assert!(
            run.err
                .contains("strata: warning: could not send the report to"),
            "{}",
            run.err
        );
        assert!(
            !state_in(home.path()).join(SENT_FILE).exists(),
            "a send that failed marks no line"
        );
        // --quiet hides the warning, as it hides each note
        let quiet = list_with(&api, home.path(), &env, &["--quiet"]);
        assert_eq!(quiet.code, plain.code);
        assert_eq!(quiet.err, "");
    }
    assert_eq!(refusing.requests().len(), 2);
}

#[test]
fn the_next_send_holds_the_lines_that_a_failed_send_left() {
    let api = serve_routes(later_type_tenant());
    let refusing = Endpoint::answering(Answer::Status(500));
    let taking = Endpoint::answering(Answer::Status(200));
    let home = tempfile::tempdir().unwrap();
    let failed = [
        ("STRATA_REPORT", "1"),
        ("STRATA_REPORT_URL", refusing.url.as_str()),
    ];
    assert_eq!(list_with(&api, home.path(), &failed, &[]).code, 0);
    let taken = [
        ("STRATA_REPORT", "1"),
        ("STRATA_REPORT_URL", taking.url.as_str()),
    ];
    assert_eq!(list_with(&api, home.path(), &taken, &[]).code, 0);
    let requests = taking.requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    // the line of the failed run and the line of this run are one entry with a count of 2
    assert_eq!(requests[0].json()["unknown"][0]["count"], 2);
}

#[test]
fn an_endpoint_that_does_not_answer_delays_the_run_by_a_few_seconds_at_most() {
    let api = serve_routes(later_type_tenant());
    let silent = Endpoint::answering(Answer::Silence);
    let home = tempfile::tempdir().unwrap();
    let env = [
        ("STRATA_REPORT", "1"),
        ("STRATA_REPORT_URL", silent.url.as_str()),
    ];
    let start = Instant::now();
    let run = list_with(&api, home.path(), &env, &[]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "{:?}",
        start.elapsed()
    );
    assert!(run.err.contains("could not send the report"), "{}", run.err);
}

#[test]
fn the_body_of_the_send_holds_the_fields_of_the_report_and_no_name_uid_or_host() {
    let api = serve_routes(later_type_tenant());
    let endpoint = Endpoint::answering(Answer::Status(200));
    let home = tempfile::tempdir().unwrap();
    let env = [
        ("STRATA_REPORT", "1"),
        ("STRATA_REPORT_URL", endpoint.url.as_str()),
    ];
    assert_eq!(list_with(&api, home.path(), &env, &[]).code, 0);
    let body = &endpoint.requests()[0].body;
    for secret in [
        "payments-prod",
        "payments-test",
        "127.0.0.1",
        "PALETTE",
        "ApiKey",
    ] {
        assert!(!body.contains(secret), "the body holds {secret}: {body}");
    }
    let report: Value = serde_json::from_str(body).unwrap();
    let entry = report["unknown"][0].as_object().unwrap();
    let mut fields: Vec<&str> = entry.keys().map(String::as_str).collect();
    fields.sort_unstable();
    assert_eq!(fields, ["count", "route", "set", "strata", "value"]);
    assert!(log_in(home.path()).exists());
    assert!(log_in(home.path()).file_name().unwrap() == FILE);
}
