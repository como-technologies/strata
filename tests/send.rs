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
use strata::closed_set::unknown::report::{MOST_VALUES, SCHEMA as REPORT_SCHEMA};
use strata::closed_set::unknown::send::{ADDRESS_SETTING, ANSWER_FILE, ENDPOINT, SETTING};
use strata::ops::Sent;

use crate::common::{
    Answer, Endpoint, LATER_TYPE, PROFILE_TEMPLATE, REFUSED, Request, Run, fake_bin, give_strata,
    isolate_palette, later_type_tenant, list_command, log_in, log_line, path_with, report_command,
    serve_routes, state_in, strata_command, through_proxy, walk, write_log,
};

/// The report that one run of `list` sends: one value, seen once.
fn one_run_report() -> Value {
    json!({
        "schema": REPORT_SCHEMA,
        "unknown": [{
            "strata": strata::TOOL_VERSION,
            "set": "ProfileType",
            "value": LATER_TYPE,
            "route": PROFILE_TEMPLATE,
            "count": 1,
        }],
    })
}

/// A run of `strata list profiles --detail` that has the settings `env`, and gets its home from
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

/// A run of `strata list profiles --detail` that the guard of the tests does not cover. The run
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

/// With the address that the release names, a run that wrote a line to the log sends nothing and
/// asks nothing. The proxy sees no request, whatever the address is, and the run keeps no answer.
fn assert_no_send_and_no_question(env: &[(&str, &str)]) {
    assert!(ENDPOINT.is_some(), "the release names the endpoint");
    let api = serve_routes(later_type_tenant());
    let trap = trap();
    let home = tempfile::tempdir().unwrap();
    let run = list_unguarded(&api, home.path(), &trap, env);
    assert_eq!(run.code, 0, "{}", run.err);
    // the run wrote a line, so a run with the setting on has a line to send
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
    let env = [(SETTING, "0"), (ADDRESS_SETTING, endpoint.url.as_str())];
    let run = list_with(&api, home.path(), &env, &[]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(endpoint.requests().is_empty());
}

#[test]
fn a_setting_that_is_not_one_or_zero_sends_nothing_and_says_so() {
    let api = serve_routes(later_type_tenant());
    let endpoint = Endpoint::answering(Answer::Status(200));
    let home = tempfile::tempdir().unwrap();
    let env = [(SETTING, "yes"), (ADDRESS_SETTING, endpoint.url.as_str())];
    let run = list_with(&api, home.path(), &env, &[]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(
        run.err.contains(&format!(
            "strata: warning: {SETTING} must be 1 or 0, so strata sends nothing"
        )),
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
    write_log(
        home.path(),
        &[log_line("ProfileType", LATER_TYPE, "/v1/packs")],
    );
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
    let env = [(SETTING, "1"), (ADDRESS_SETTING, endpoint.url.as_str())];
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
    // the endpoint took the report, so the run sent the line: a report after the run is empty
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
        format!("{SETTING}=1\n{ADDRESS_SETTING}={}\n", endpoint.url),
    )
    .unwrap();
    // the file is the only setting here, so the guard of the tests is off. The proxy takes any
    // request for another address than the one in the file.
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
        .env(SETTING, "1")
        .env(ADDRESS_SETTING, &endpoint.url)
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
        let env = [(SETTING, "1"), (ADDRESS_SETTING, url)];
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
fn a_log_with_a_value_of_release_0_27_0_that_is_not_one_word_does_not_stop_the_send() {
    let api = serve_routes(later_type_tenant());
    let endpoint = Endpoint::answering(Answer::Status(200));
    let home = tempfile::tempdir().unwrap();
    // release 0.27.0 wrote a value with a space, and its endpoint took it
    write_log(
        home.path(),
        &[log_line("ProfileType", "two words", PROFILE_TEMPLATE)],
    );
    let env = [(SETTING, "1"), (ADDRESS_SETTING, endpoint.url.as_str())];
    let run = list_with(&api, home.path(), &env, &[]);
    assert_eq!(run.code, 0, "{}", run.err);
    // the send skips the line, and sends the value of this run
    assert_eq!(endpoint.requests().len(), 1, "{:?}", endpoint.requests());
    assert_eq!(endpoint.requests()[0].json(), one_run_report());
    assert!(
        run.err
            .contains("strata: sent the report of the unknown values\n"),
        "{}",
        run.err
    );
    assert!(
        run.err.contains(
            "strata: warning: skipped 1 line of the log that is not valid, the first at line 1: \
             run `strata report` to see the fault\n"
        ),
        "{}",
        run.err
    );
    // the mark counts each line that is not blank, valid or not: the log has two lines
    assert_eq!(mark_in(home.path()).as_deref(), Some("2"));
    // `strata report` still stops on the line
    let mut command = report_command(home.path(), &["--json"]);
    let report = Run::of(&mut command);
    assert_eq!(report.code, 2, "{}", report.err);
    assert!(
        report.err.contains("\"kind\":\"log-invalid\""),
        "{}",
        report.err
    );
}

#[test]
fn quiet_hides_the_line_about_a_skipped_line_as_it_hides_the_note_of_the_send() {
    let api = serve_routes(later_type_tenant());
    let endpoint = Endpoint::answering(Answer::Status(200));
    let home = tempfile::tempdir().unwrap();
    write_log(
        home.path(),
        &[log_line("ProfileType", "two words", "/v1/packs")],
    );
    let env = [(SETTING, "1"), (ADDRESS_SETTING, endpoint.url.as_str())];
    let quiet = list_with(&api, home.path(), &env, &["--quiet"]);
    assert_eq!(quiet.code, 0);
    assert_eq!(quiet.err, "", "--quiet hides the line");
    assert_eq!(endpoint.requests().len(), 1, "the send sent the other line");
}

#[test]
fn a_send_that_fails_says_the_error_and_not_the_skipped_lines() {
    let api = serve_routes(later_type_tenant());
    let refusing = Endpoint::answering(Answer::Status(500));
    let home = tempfile::tempdir().unwrap();
    write_log(
        home.path(),
        &[log_line("ProfileType", "two words", "/v1/packs")],
    );
    let env = [(SETTING, "1"), (ADDRESS_SETTING, refusing.url.as_str())];
    let run = list_with(&api, home.path(), &env, &[]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(run.err.contains("could not send the report"), "{}", run.err);
    assert!(!run.err.contains("skipped"), "{}", run.err);
    assert!(!state_in(home.path()).join(SENT_FILE).exists());
}

#[test]
fn the_next_send_holds_the_lines_that_a_failed_send_left() {
    let api = serve_routes(later_type_tenant());
    let refusing = Endpoint::answering(Answer::Status(500));
    let taking = Endpoint::answering(Answer::Status(200));
    let home = tempfile::tempdir().unwrap();
    let failed = [(SETTING, "1"), (ADDRESS_SETTING, refusing.url.as_str())];
    assert_eq!(list_with(&api, home.path(), &failed, &[]).code, 0);
    let taken = [(SETTING, "1"), (ADDRESS_SETTING, taking.url.as_str())];
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
    let env = [(SETTING, "1"), (ADDRESS_SETTING, silent.url.as_str())];
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
    let env = [(SETTING, "1"), (ADDRESS_SETTING, endpoint.url.as_str())];
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
        format!("{SETTING}=1\n{ADDRESS_SETTING}=https://report.example/\n"),
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
    let lines: Vec<String> = (0..count)
        .map(|n| log_line("ProfileType", &format!("later-{n:03}"), "/v1/packs"))
        .collect();
    write_log(home, &lines)
}

/// How many entries each request of `endpoint` holds.
fn entries_of(endpoint: &Endpoint) -> Vec<usize> {
    endpoint
        .requests()
        .iter()
        .map(|r| r.json()["unknown"].as_array().unwrap().len())
        .collect()
}

/// The values that the report in `request` holds, in the order of the report.
fn values_of(request: &Request) -> Vec<String> {
    request.json()["unknown"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["value"].as_str().unwrap().to_string())
        .collect()
}

/// Write a log that has a line for each of `values`, and mark its first `mark` lines as sent, as
/// release 0.27.0 did. A value with a space is a line that this release cannot read.
fn marked_log(home: &Path, values: &[&str], mark: usize) -> std::path::PathBuf {
    let lines: Vec<String> = values
        .iter()
        .map(|value| log_line("ProfileType", value, "/v1/packs"))
        .collect();
    let log = write_log(home, &lines);
    std::fs::write(state_in(home).join(SENT_FILE), format!("{mark}\n")).unwrap();
    log
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
    // nothing remains, so a second send posts nothing
    strata::ops::send_unknown_report(&log, &endpoint.url).unwrap();
    assert_eq!(endpoint.requests().len(), 13);
}

#[test]
fn a_report_has_20_values_or_fewer_also_when_the_endpoint_would_take_more_entries() {
    // 3 values with 50 routes each are 150 entries. The limit of 100 entries cuts the report, and the
    // limit of 20 values does not.
    let endpoint = Endpoint::answering(Answer::Status(200));
    let home = tempfile::tempdir().unwrap();
    let lines: Vec<String> = (0..3)
        .flat_map(|value| (0..50).map(move |route| (value, route)))
        .map(|(value, route)| {
            log_line(
                "ProfileType",
                &format!("later-{value}"),
                &format!("/v1/routes/{route}"),
            )
        })
        .collect();
    let file = write_log(home.path(), &lines);
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
    // the next send holds the 230 lines that remain
    let taking = Endpoint::answering(Answer::Status(200));
    strata::ops::send_unknown_report(&log, &taking.url).unwrap();
    let mut expected = vec![MOST_VALUES; 11];
    expected.push(10);
    assert_eq!(entries_of(&taking), expected);
}

/// Release 0.27.0 sent a line with a space, and its mark counted the line. The send of this release
/// skips such a line, and the mark must still count it. A mark of 0.27.0 and a mark of this release
/// mean the same: the number of lines that are not blank, valid or not.
#[test]
fn a_mark_of_release_0_27_0_covers_the_lines_it_counted_also_when_the_send_skips_one_of_them() {
    // case A: 0.27.0 sent the first 2 lines, and the third line stayed unsent
    let home = tempfile::tempdir().unwrap();
    let log = marked_log(
        home.path(),
        &["alpha", "two words", "gamma", "new-run-type"],
        2,
    );
    let endpoint = Endpoint::answering(Answer::Status(200));
    let sent = strata::ops::send_unknown_report(&log, &endpoint.url).unwrap();
    assert_eq!(endpoint.requests().len(), 1);
    assert_eq!(
        values_of(&endpoint.requests()[0]),
        ["gamma", "new-run-type"]
    );
    assert_eq!(sent.marked, 2);
    assert_eq!(sent.skipped, [2]);
    assert_eq!(mark_in(home.path()).as_deref(), Some("4"));

    // case B: 0.27.0 sent the 2 lines of the log, and the run wrote one line
    let home = tempfile::tempdir().unwrap();
    let log = marked_log(home.path(), &["alpha", "two words", "new-run-type"], 2);
    let endpoint = Endpoint::answering(Answer::Status(200));
    let sent = strata::ops::send_unknown_report(&log, &endpoint.url).unwrap();
    assert_eq!(endpoint.requests().len(), 1, "the line of the run is sent");
    assert_eq!(values_of(&endpoint.requests()[0]), ["new-run-type"]);
    assert_eq!(sent.marked, 1);
    assert_eq!(mark_in(home.path()).as_deref(), Some("3"));

    // case C: 0.27.0 sent the 3 lines of the log, and only one of them is valid here
    let home = tempfile::tempdir().unwrap();
    let values = ["alpha", "two words", "three more words", "new-run-type"];
    let log = marked_log(home.path(), &values, 3);
    let endpoint = Endpoint::answering(Answer::Status(200));
    strata::ops::send_unknown_report(&log, &endpoint.url).unwrap();
    assert_eq!(endpoint.requests().len(), 1);
    assert_eq!(
        values_of(&endpoint.requests()[0]),
        ["new-run-type"],
        "the mark is not read as the mark of a removed log"
    );
    assert_eq!(mark_in(home.path()).as_deref(), Some("4"));
}

#[test]
fn a_line_that_is_not_valid_between_two_reports_and_at_the_end_is_counted_by_the_mark() {
    let home = tempfile::tempdir().unwrap();
    let values: Vec<String> = (0..21).map(|n| format!("later-{n:03}")).collect();
    let mut lines: Vec<&str> = values.iter().map(String::as_str).collect();
    // 21 values go in two reports. The invalid lines come between the two and at the end.
    lines.insert(20, "two words");
    lines.push("three more words");
    let log = marked_log(home.path(), &lines, 0);
    let endpoint = Endpoint::answering(Answer::Status(200));
    let sent = strata::ops::send_unknown_report(&log, &endpoint.url).unwrap();
    assert_eq!(entries_of(&endpoint), [MOST_VALUES, 1]);
    assert_eq!(sent.marked, 21);
    assert_eq!(sent.skipped, [21, 23]);
    assert_eq!(mark_in(home.path()).as_deref(), Some("23"));
    // the next send has nothing to post
    strata::ops::send_unknown_report(&log, &endpoint.url).unwrap();
    assert_eq!(endpoint.requests().len(), 2);
}

/// The answer of an endpoint that takes the report and makes the entries `waiting` wait.
fn holding_back(waiting: &'static str) -> Answer {
    Answer::Body(202, waiting)
}

/// The number in the file of the mark in the state folder of `home`, or `None` when there is none.
fn mark_in(home: &Path) -> Option<String> {
    std::fs::read_to_string(state_in(home).join(SENT_FILE))
        .ok()
        .map(|text| text.trim().to_string())
}

#[test]
fn a_value_that_the_endpoint_holds_back_is_not_marked_as_sent() {
    // the hour has room for 3 of the 5 new issues: the endpoint takes the report and lists the rest
    let endpoint = Endpoint::answering(holding_back(
        r#"{"ok":true,"opens":3,"updates":0,"waiting":[4,5]}"#,
    ));
    let home = tempfile::tempdir().unwrap();
    let log = long_log(home.path(), 5);
    let sent = strata::ops::send_unknown_report(&log, &endpoint.url).unwrap();
    assert_eq!(
        sent,
        Sent {
            marked: 3,
            waiting: 2,
            skipped: Vec::new()
        }
    );
    assert_eq!(entries_of(&endpoint), [5], "the send stops at the report");
    assert_eq!(mark_in(home.path()).as_deref(), Some("3"));
}

#[test]
fn the_next_send_after_a_value_waited_holds_that_value_and_the_lines_after_it() {
    let holding = Endpoint::answering(holding_back(r#"{"ok":true,"waiting":[4,5]}"#));
    let home = tempfile::tempdir().unwrap();
    let log = long_log(home.path(), 5);
    strata::ops::send_unknown_report(&log, &holding.url).unwrap();
    let taking = Endpoint::answering(Answer::Status(202));
    let sent = strata::ops::send_unknown_report(&log, &taking.url).unwrap();
    assert_eq!(sent.marked, 2);
    assert_eq!(sent.waiting, 0);
    assert_eq!(values_of(&taking.requests()[0]), ["later-003", "later-004"]);
    assert_eq!(mark_in(home.path()).as_deref(), Some("5"));
}

#[test]
fn a_send_whose_first_value_waits_marks_nothing_and_posts_no_second_report() {
    // 250 values are 13 reports. The first value of the first report waits at the endpoint.
    let endpoint = Endpoint::answering(holding_back(r#"{"ok":true,"waiting":[1]}"#));
    let home = tempfile::tempdir().unwrap();
    let log = long_log(home.path(), 250);
    let sent = strata::ops::send_unknown_report(&log, &endpoint.url).unwrap();
    assert_eq!(
        sent,
        Sent {
            marked: 0,
            waiting: 250,
            skipped: Vec::new()
        }
    );
    assert_eq!(endpoint.requests().len(), 1);
    assert_eq!(
        mark_in(home.path()),
        None,
        "a send that marked nothing writes no mark"
    );
}

#[test]
fn a_value_that_waits_is_found_by_its_set_and_value_and_not_by_the_order_of_the_log() {
    // the report sorts its entries: later-000 is entry 1, later-001 entry 2, and later-004 entry 3
    let endpoint = Endpoint::answering(holding_back(r#"{"ok":true,"waiting":[3]}"#));
    let home = tempfile::tempdir().unwrap();
    let lines: Vec<String> = ["later-004", "later-000", "later-001"]
        .iter()
        .map(|value| log_line("ProfileType", value, "/v1/packs"))
        .collect();
    let log = write_log(home.path(), &lines);
    let sent = strata::ops::send_unknown_report(&log, &endpoint.url).unwrap();
    assert_eq!(
        (sent.marked, sent.waiting),
        (0, 3),
        "the first line of the log is the value that waits"
    );
    assert_eq!(mark_in(home.path()), None);
}

#[test]
fn a_number_of_waiting_that_names_no_entry_holds_back_nothing() {
    for waiting in [
        r#"{"waiting":[0]}"#,
        r#"{"waiting":[9]}"#,
        r#"{"waiting":[]}"#,
    ] {
        let endpoint = Endpoint::answering(holding_back(waiting));
        let home = tempfile::tempdir().unwrap();
        let log = long_log(home.path(), 2);
        let sent = strata::ops::send_unknown_report(&log, &endpoint.url).unwrap();
        assert_eq!((sent.marked, sent.waiting), (2, 0), "{waiting}");
        assert_eq!(mark_in(home.path()).as_deref(), Some("2"), "{waiting}");
    }
}

#[test]
fn a_run_says_that_the_endpoint_holds_back_lines_and_quiet_hides_the_line() {
    let api = serve_routes(later_type_tenant());
    let endpoint = Endpoint::answering(holding_back(r#"{"ok":true,"waiting":[1]}"#));
    let home = tempfile::tempdir().unwrap();
    let env = [(SETTING, "1"), (ADDRESS_SETTING, endpoint.url.as_str())];
    let run = list_with(&api, home.path(), &env, &[]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(
        run.err.contains(
            "strata: warning: the endpoint holds back 1 line for now, because its hour has no \
             room: the next send holds them again\n"
        ),
        "{}",
        run.err
    );
    assert!(
        !run.err.contains("sent the report"),
        "the endpoint took no line, so the run does not say that it sent the report: {}",
        run.err
    );
    assert_eq!(mark_in(home.path()), None);
    let quiet = list_with(&api, home.path(), &env, &["--quiet"]);
    assert_eq!(quiet.err, "", "--quiet hides the line");
}
#[test]
fn a_later_release_can_add_a_step_or_a_choice_so_a_match_outside_the_library_needs_a_wildcard() {
    use strata::closed_set::unknown::send::{Choice, Facts, Step, step};
    // without `#[non_exhaustive]` the wildcard arms are unreachable, and `just lint` refuses them
    let facts = Facts {
        setting: Some("1".to_string()),
        answer: None,
        endpoint: Some("http://127.0.0.1:9/".to_string()),
        wrote: true,
        in_ci: false,
        terminal: false,
    };
    let sends = match step(&facts) {
        Step::Send => true,
        Step::Nothing | Step::Ask => false,
        _ => false,
    };
    assert!(sends);
    let yes = match Choice::Yes {
        Choice::Yes => true,
        Choice::No => false,
        _ => false,
    };
    assert!(yes);
}
