//! The opt-in send of the report to the endpoint. The unit tests of the decision, the post, and
//! the question are in `src/closed_set/unknown/tests.rs` and `src/term/ask.rs`. These tests run the
//! binary against a stand-in Palette and a stand-in endpoint. The release names the real endpoint,
//! so the tests also show the guard that keeps each test from posting to it. The page
//! `docs/src/development/build-test.md` states the rules in "The tests send no report".

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use strata::closed_set::unknown::log::{FILE, SENT_FILE};
use strata::closed_set::unknown::report::MOST_VALUES;
use strata::closed_set::unknown::send::{ADDRESS_SETTING, ANSWER_FILE, ENDPOINT, SETTING};

use crate::common::{
    Answer, Endpoint, REFUSED, Run, fake_bin, give_strata, isolate_palette, later_type_tenant,
    list_command, log_in, path_with, serve_routes, strata_command, through_proxy, walk,
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
/// `home`. The guard of the tests stops the send and names an address that refuses a
/// connection. This run has no setting `STRATA_REPORT` but the one in `env`. Each test that uses
/// it gives the address of a stand-in in `env`.
fn list_with(api: &str, home: &Path, env: &[(&str, &str)], more: &[&str]) -> Run {
    let mut command = list_command(api, home, more);
    command.env_remove(SETTING).envs(env.iter().copied());
    Run::of(&mut command)
}

/// A stand-in proxy. A run that sends to any address connects to it, and it refuses the connection.
fn trap() -> Endpoint {
    Endpoint::answering(Answer::Status(403))
}

/// A run of `strata list profiles --packs` that the guard of the tests does not cover: the run
/// has neither `STRATA_REPORT` nor `STRATA_REPORT_URL` from the environment. It reads the file
/// `.palette.env` of `home`, and else it has the address that the release names. The proxy `trap`
/// takes each request that leaves the machine, so no request reaches an address.
fn list_unguarded(api: &str, home: &Path, trap: &Endpoint, env: &[(&str, &str)]) -> Run {
    let mut command = list_command(api, home, &[]);
    through_proxy(&mut command, trap)
        .env_remove(SETTING)
        .env_remove(ADDRESS_SETTING)
        .envs(env.iter().copied());
    Run::of(&mut command)
}

/// The state folder of the run that used `home` as its home.
fn state_in(home: &Path) -> std::path::PathBuf {
    log_in(home).parent().unwrap().to_path_buf()
}

/// With the address that the release names, a run that wrote a line to the log sends nothing and
/// asks nothing. The proxy sees no request, whatever the address is, and the run keeps no answer.
fn assert_no_send_and_no_question(env: &[(&str, &str)]) {
    assert!(ENDPOINT.is_some(), "the release names the endpoint");
    let api = serve_routes(later_type_tenant());
    let trap = trap();
    let home = tempfile::tempdir().unwrap();
    let run = list_unguarded(&api, home.path(), &trap, env);
    assert_eq!(run.code, 0, "{}", run.err);
    // the run wrote a line, so a run with the setting on would have sent it
    assert!(
        run.err.contains("Palette gave the ProfileType"),
        "{}",
        run.err
    );
    assert!(log_in(home.path()).exists());
    assert!(!run.err.contains("[y/N]"), "{}", run.err);
    assert!(!run.err.contains("sent the report"), "{}", run.err);
    assert!(!run.err.contains("could not send"), "{}", run.err);
    assert!(trap.requests().is_empty(), "{:?}", trap.requests());
    assert!(
        !state_in(home.path()).join(ANSWER_FILE).exists(),
        "a run that asked nothing keeps no answer"
    );
}

#[test]
fn with_no_setting_and_the_address_of_the_release_strata_sends_nothing_and_asks_nothing() {
    // a run of the tests has no terminal, so strata cannot ask
    assert_no_send_and_no_question(&[]);
}

#[test]
fn a_run_in_ci_with_no_setting_and_the_address_of_the_release_sends_nothing_and_asks_nothing() {
    assert_no_send_and_no_question(&[("CI", "true")]);
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
    // no address in the environment: the run has the address that the release names
    assert!(ENDPOINT.is_some(), "the release names the endpoint");
    let trap = trap();
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
    // preflight is the other binary, and offline it reads nothing from Palette. The run uses the
    // binary beside strata when a build made it, and else this stand-in: the result is the same
    let bin = fake_bin(home.path(), &[("strata-preflight", "#!/bin/sh\nexit 0\n")]);
    let mut command = strata_command();
    isolate_palette(&mut command, home.path());
    through_proxy(&mut command, &trap)
        .env_remove("XDG_STATE_HOME")
        .env_remove(ADDRESS_SETTING)
        .env("PATH", path_with(&bin))
        .env(SETTING, "1")
        .args(["preflight", "cluster.json", "--tier", "static", "--offline"]);
    let run = Run::of(&mut command);
    assert!(
        !run.err.contains("sent the report") && !run.err.contains("could not send"),
        "an offline run sends nothing: {}",
        run.err
    );
    assert!(trap.requests().is_empty(), "{:?}", trap.requests());
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
        run.err
            .contains("strata: sent the report of the unknown values\n"),
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
    // the file is the only setting here, so the guard of the tests is off. The proxy takes any
    // request that would go to another address than the one in the file.
    let trap = trap();
    let run = list_unguarded(&api, home.path(), &trap, &[]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert_eq!(endpoint.requests().len(), 1);
    assert!(trap.requests().is_empty(), "{:?}", trap.requests());
}

#[test]
fn a_run_that_saw_no_unknown_value_sends_nothing() {
    let endpoint = Endpoint::answering(Answer::Status(200));
    let home = tempfile::tempdir().unwrap();
    let mut command = strata_command();
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
    for url in [REFUSED, refusing.url.as_str()] {
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

#[test]
fn a_test_that_turns_the_send_on_and_gives_no_address_posts_to_the_refused_address() {
    let api = serve_routes(later_type_tenant());
    let trap = trap();
    let home = tempfile::tempdir().unwrap();
    // the run has the setting 1 and the address of the guard: the second guard of the tests
    let mut command = list_command(&api, home.path(), &[]);
    through_proxy(&mut command, &trap).env(SETTING, "1");
    let run = Run::of(&mut command);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(
        run.err.contains(&format!(
            "strata: warning: could not send the report to {REFUSED}"
        )),
        "{}",
        run.err
    );
    assert!(!run.err.contains(ENDPOINT.unwrap()), "{}", run.err);
    assert!(trap.requests().is_empty(), "{:?}", trap.requests());
}

#[test]
fn a_run_of_the_tests_sends_nothing_though_the_home_and_a_saved_answer_turn_the_send_on() {
    let api = serve_routes(later_type_tenant());
    let trap = trap();
    let home = tempfile::tempdir().unwrap();
    // the home sends: its file turns the send on and names an address, and the user said yes
    std::fs::write(
        home.path().join(".palette.env"),
        "STRATA_REPORT=1\nSTRATA_REPORT_URL=https://report.example/\n",
    )
    .unwrap();
    std::fs::create_dir_all(state_in(home.path())).unwrap();
    std::fs::write(state_in(home.path()).join(ANSWER_FILE), "1\n").unwrap();
    // without the guard the run posts: the proxy takes a request for the address of the file
    let run = list_unguarded(&api, home.path(), &trap, &[]);
    assert_eq!(run.code, 0, "{}", run.err);
    let seen = trap.requests();
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert_eq!(
        (seen[0].method.as_str(), seen[0].path.as_str()),
        ("CONNECT", "report.example:443")
    );
    // the run as the tests start it has the guard, so the same home sends nothing
    let mut command = list_command(&api, home.path(), &[]);
    through_proxy(&mut command, &trap);
    let run = Run::of(&mut command);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(!run.err.contains("sent the report"), "{}", run.err);
    assert!(!run.err.contains("could not send"), "{}", run.err);
    assert_eq!(trap.requests().len(), 1, "{:?}", trap.requests());
}

#[test]
fn each_command_that_starts_strata_for_a_test_has_the_guard_of_the_send() {
    let home = tempfile::tempdir().unwrap();
    let mut script = Command::new("bash");
    give_strata(&mut script, "STRATA_BIN");
    let commands = [
        strata_command(),
        list_command("http://127.0.0.1:1", home.path(), &[]),
        script,
    ];
    for command in commands {
        let setting = |name: &str| {
            command
                .get_envs()
                .find(|(key, _)| *key == name)
                .and_then(|(_, value)| value)
                .map(|value| value.to_str().unwrap().to_string())
        };
        assert_eq!(setting(SETTING).as_deref(), Some("0"), "{command:?}");
        assert_eq!(setting(ADDRESS_SETTING).as_deref(), Some(REFUSED));
    }
}

#[test]
fn no_file_of_the_tests_but_the_helpers_names_the_binary_of_strata_to_start_it() {
    // the name is in two parts, so that this file does not hold it
    let name = format!("CARGO_BIN_EXE_{}\"", "strata");
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let helpers = tests.join("common/mod.rs");
    for file in walk(&tests) {
        if file.extension().is_some_and(|e| e == "rs") && file != helpers {
            let text = std::fs::read_to_string(&file).unwrap();
            assert!(
                !text.contains(&name),
                "{} starts strata with no guard: use `strata_command` or `give_strata`",
                file.display()
            );
        }
    }
}

/// Write a log of `count` lines, each a value of its own, and give its path.
fn long_log(home: &Path, count: usize) -> std::path::PathBuf {
    let file = log_in(home);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let lines: String = (0..count)
        .map(|n| {
            format!(
                "{}\n",
                json!({"schema": "strata-unknown/1", "strata": strata::TOOL_VERSION,
                    "set": "ProfileType", "value": format!("later-{n:03}"), "route": "/v1/packs"})
            )
        })
        .collect();
    std::fs::write(&file, lines).unwrap();
    file
}

/// How many entries each request of `endpoint` holds.
fn entries_of(endpoint: &Endpoint) -> Vec<usize> {
    endpoint
        .requests()
        .iter()
        .map(|r| r.json()["unknown"].as_array().unwrap().len())
        .collect()
}

#[test]
fn a_log_of_more_than_20_values_goes_in_reports_of_20_values_and_each_report_marks_its_lines() {
    let endpoint = Endpoint::answering(Answer::Status(200));
    let home = tempfile::tempdir().unwrap();
    let log = long_log(home.path(), 250);
    strata::ops::send_unknown_report(&log, &endpoint.url).unwrap();
    let mut expected = vec![MOST_VALUES; 12];
    expected.push(10);
    assert_eq!(entries_of(&endpoint), expected);
    let sent = std::fs::read_to_string(state_in(home.path()).join(SENT_FILE)).unwrap();
    assert_eq!(sent.trim(), "250");
    // nothing is left, so a second send posts nothing
    strata::ops::send_unknown_report(&log, &endpoint.url).unwrap();
    assert_eq!(endpoint.requests().len(), 13);
}

#[test]
fn a_report_has_20_values_or_fewer_also_when_the_endpoint_would_take_more_entries() {
    // 3 values with 50 routes each are 150 entries. The limit of 100 entries cuts the report, and the
    // limit of 20 values does not.
    let endpoint = Endpoint::answering(Answer::Status(200));
    let home = tempfile::tempdir().unwrap();
    let file = log_in(home.path());
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let lines: String = (0..3)
        .flat_map(|value| (0..50).map(move |route| (value, route)))
        .map(|(value, route)| {
            format!(
                "{}\n",
                json!({"schema": "strata-unknown/1", "strata": strata::TOOL_VERSION,
                    "set": "ProfileType", "value": format!("later-{value}"),
                    "route": format!("/v1/routes/{route}")})
            )
        })
        .collect();
    std::fs::write(&file, lines).unwrap();
    strata::ops::send_unknown_report(&file, &endpoint.url).unwrap();
    assert_eq!(entries_of(&endpoint), [100, 50]);
}

#[test]
fn a_report_that_fails_in_the_middle_leaves_the_mark_after_the_reports_that_were_taken() {
    let failing = Endpoint::answering(Answer::Statuses(&[200, 500]));
    let home = tempfile::tempdir().unwrap();
    let log = long_log(home.path(), 250);
    let error = strata::ops::send_unknown_report(&log, &failing.url).unwrap_err();
    assert_eq!(error.kind(), "report-send");
    assert_eq!(
        entries_of(&failing),
        [MOST_VALUES, MOST_VALUES],
        "the second report failed"
    );
    let sent = std::fs::read_to_string(state_in(home.path()).join(SENT_FILE)).unwrap();
    assert_eq!(
        sent.trim(),
        MOST_VALUES.to_string(),
        "only the first report was taken"
    );
    // the next send holds the 230 lines that are left
    let taking = Endpoint::answering(Answer::Status(200));
    strata::ops::send_unknown_report(&log, &taking.url).unwrap();
    let mut expected = vec![MOST_VALUES; 11];
    expected.push(10);
    assert_eq!(entries_of(&taking), expected);
}
