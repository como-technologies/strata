//! The note and the log for a value of a closed set that Palette gives and this release does not
//! know (`strata-unknown/1`). The unit tests of the watch and of the line are in
//! `src/closed_set/unknown/tests.rs`. These tests run the reads of the commands, and the binary.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};
use strata::api::MemoryApi;
use strata::closed_set::unknown::Watch;
use strata::closed_set::unknown::hint;
use strata::closed_set::unknown::log::{FIELDS, FILE, Line, SCHEMA};
use strata::ops::{ListKind, ListOptions};
use strata::term::progress::Progress;

use crate::common::{
    PROFILE_LISTING, Run, UID_A, UID_B, isolate_palette, listing, profile_document, profile_route,
    strata_bin, template,
};

/// The profile type that a later Palette release adds.
const LATER_TYPE: &str = "later-type";

/// The note for that type, as a person reads it on standard error.
const NOTE: &str = "strata: Palette gave the ProfileType value 'later-type', which this release of strata does not know";

/// The listing entry of a profile version, as `GET /v1/dashboard/clusterprofiles/metadata` gives
/// it.
fn listed(name: &str, uid: &str) -> Value {
    json!({"metadata": {"name": name, "uid": uid}, "spec": {"cloudType": "aws", "version": "1.0.0"}})
}

/// The packs of the template: one pack, so that the template is the published one.
fn packs() -> Value {
    json!([{"name": "cert-manager", "layer": "addon", "type": "spectro",
        "tag": "1.14.0", "version": "1.14.0"}])
}

/// The documents of two profile versions whose template has the type `LATER_TYPE`, by route.
fn tenant() -> BTreeMap<String, Value> {
    let mut routes = BTreeMap::new();
    routes.insert(
        PROFILE_LISTING.to_string(),
        json!({"items": [listed("payments-prod", UID_A), listed("payments-test", UID_B)]}),
    );
    for (uid, name) in [(UID_A, "payments-prod"), (UID_B, "payments-test")] {
        routes.insert(
            profile_route(uid),
            profile_document(
                uid,
                name,
                "1.0.0",
                template("aws", LATER_TYPE, packs()),
                None,
            ),
        );
    }
    routes
}

/// Serve `routes` on this machine, one answer for each request. A route that nobody lists
/// answers 404. The address is the one for `PALETTE_API`.
fn serve(routes: BTreeMap<String, Value>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = Vec::new();
            let mut chunk = [0u8; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => request.extend_from_slice(&chunk[..n]),
                }
            }
            let text = String::from_utf8_lossy(&request).into_owned();
            let path = text
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            let (status, body) = match routes.get(path) {
                Some(doc) => ("200 OK", doc.to_string()),
                None => ("404 Not Found", "{}".to_string()),
            };
            let answer = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(answer.as_bytes());
        }
    });
    format!("http://{address}")
}

/// Run `strata list profiles --packs` with the arguments `more` against `api`. The run has a home
/// of its own, `home`, and no state folder from the environment of the test.
fn list(api: &str, home: &Path, more: &[&str]) -> Run {
    let mut command = Command::new(strata_bin());
    isolate_palette(&mut command, home);
    command
        .env_remove("STRATA_QUIET")
        .env_remove("XDG_STATE_HOME")
        .env("PALETTE_API", api)
        .args(["list", "profiles", "--packs"])
        .args(more);
    Run::of(&mut command)
}

/// The log of the run that used `home` as its home.
fn log_in(home: &Path) -> PathBuf {
    home.join(".local/state/strata").join(FILE)
}

fn lines_of(file: &Path) -> Vec<Line> {
    std::fs::read_to_string(file)
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn a_read_of_a_document_with_an_unknown_value_writes_the_note_one_time() {
    let api = serve(tenant());
    let home = tempfile::tempdir().unwrap();
    let run = list(&api, home.path(), &[]);
    assert_eq!(run.code, 0, "{}", run.err);
    // two profiles give the type, and the run notes it one time
    assert_eq!(run.err.matches(NOTE).count(), 1, "{}", run.err);
    assert!(
        run.out.contains(LATER_TYPE),
        "the list shows the type: {}",
        run.out
    );
}

#[test]
fn quiet_hides_the_note_and_the_environment_hides_it_too() {
    let api = serve(tenant());
    let home = tempfile::tempdir().unwrap();
    let noisy = list(&api, home.path(), &[]);
    let quiet = list(&api, home.path(), &["--quiet"]);
    assert_eq!(quiet.code, 0, "{}", quiet.err);
    assert!(!quiet.err.contains("Palette gave"), "{}", quiet.err);
    assert_eq!(quiet.out, noisy.out, "the note is not part of the output");

    let mut command = Command::new(strata_bin());
    isolate_palette(&mut command, home.path());
    command
        .env_remove("XDG_STATE_HOME")
        .env("STRATA_QUIET", "1")
        .env("PALETTE_API", &api)
        .args(["list", "profiles", "--packs"]);
    let silent = Run::of(&mut command);
    assert!(!silent.err.contains("Palette gave"), "{}", silent.err);
}

#[test]
fn the_json_document_is_the_same_with_and_without_the_note() {
    let api = serve(tenant());
    let home = tempfile::tempdir().unwrap();
    let noisy = list(&api, home.path(), &["--json"]);
    let quiet = list(&api, home.path(), &["--json", "--quiet"]);
    // the time of the document is the one field that two runs do not share
    let read = |run: &Run| -> Value {
        let mut doc: Value = serde_json::from_str(&run.out).unwrap();
        doc.as_object_mut().unwrap().remove("generatedAt");
        doc
    };
    assert_eq!(read(&noisy), read(&quiet));
    assert_eq!(read(&noisy)["profiles"][0]["type"], LATER_TYPE);
    assert!(!noisy.out.contains("Palette gave"));
}

#[test]
fn each_run_adds_one_line_to_the_log_in_the_state_folder_also_with_quiet() {
    let api = serve(tenant());
    let home = tempfile::tempdir().unwrap();
    list(&api, home.path(), &[]);
    list(&api, home.path(), &["--quiet"]);
    let lines = lines_of(&log_in(home.path()));
    assert_eq!(lines.len(), 2, "one line for each run: {lines:?}");
    let line = &lines[0];
    assert_eq!(line, &lines[1]);
    assert_eq!(line.schema, SCHEMA);
    assert_eq!(line.strata, strata::TOOL_VERSION);
    assert_eq!(line.set, "ProfileType");
    assert_eq!(line.value, LATER_TYPE);
    assert_eq!(line.route, "/v1/clusterprofiles/{uid}");
}

#[test]
fn the_state_home_of_the_environment_decides_where_the_log_is() {
    let api = serve(tenant());
    let home = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let mut command = Command::new(strata_bin());
    isolate_palette(&mut command, home.path());
    command
        .env_remove("STRATA_QUIET")
        .env("XDG_STATE_HOME", state.path())
        .env("PALETTE_API", &api)
        .args(["list", "profiles", "--packs"]);
    let run = Run::of(&mut command);
    assert_eq!(run.code, 0, "{}", run.err);
    assert_eq!(lines_of(&state.path().join("strata").join(FILE)).len(), 1);
    assert!(!log_in(home.path()).exists());
}

#[test]
fn a_document_with_a_uid_and_a_name_writes_neither_to_the_log() {
    let api = serve(tenant());
    let home = tempfile::tempdir().unwrap();
    list(&api, home.path(), &[]);
    let text = std::fs::read_to_string(log_in(home.path())).unwrap();
    for private in [
        UID_A,
        UID_B,
        "payments-prod",
        "payments-test",
        "cert-manager",
        "127.0.0.1",
    ] {
        assert!(!text.contains(private), "the log holds `{private}`: {text}");
    }
    let line: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    let mut names: Vec<&str> = line
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    names.sort_unstable();
    let mut allowed = FIELDS.to_vec();
    allowed.retain(|f| names.contains(f));
    allowed.sort_unstable();
    assert_eq!(names, allowed, "a field that is not in the list: {line}");
}

#[test]
fn a_watch_on_a_read_of_the_library_logs_the_route_template_and_no_uid() {
    let api = MemoryApi::new()
        .with(
            PROFILE_LISTING,
            json!({"items": [listed("payments-prod", UID_A)]}),
        )
        .with(
            profile_route(UID_A),
            profile_document(
                UID_A,
                "payments-prod",
                "1.0.0",
                template("aws", LATER_TYPE, packs()),
                None,
            ),
        );
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(FILE);
    let watching = Watch::new(Progress::silent())
        .logging_to(file.clone())
        .start();
    let options = ListOptions {
        kinds: vec![ListKind::Profiles],
        packs: true,
        ..ListOptions::default()
    };
    listing(&api, &options).unwrap();
    drop(watching);
    let lines = lines_of(&file);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0].route, "/v1/clusterprofiles/{uid}");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        !text.contains(UID_A) && !text.contains("payments-prod"),
        "{text}"
    );
}

#[test]
fn the_hint_does_not_show_when_standard_error_is_not_a_terminal_and_uses_up_no_day() {
    let api = serve(tenant());
    let home = tempfile::tempdir().unwrap();
    // the run reads its standard error through a pipe, as a script or a test does
    let run = list(&api, home.path(), &[]);
    assert_eq!(run.err.matches(NOTE).count(), 1, "{}", run.err);
    assert!(!run.err.contains("strata report"), "{}", run.err);
    let state = home.path().join(".local/state/strata");
    assert!(
        !state.join(hint::FILE).exists(),
        "a run in a pipe wrote the time of a hint"
    );
    // the log has the line, and a report of the next command shows it
    let mut command = Command::new(strata_bin());
    isolate_palette(&mut command, home.path());
    command
        .env_remove("STRATA_QUIET")
        .env_remove("XDG_STATE_HOME")
        .args(["report", "--json"]);
    let report = Run::of(&mut command);
    assert_eq!(report.code, 0, "{}", report.err);
    let document: Value = serde_json::from_str(&report.out).unwrap();
    assert_eq!(document["unknown"][0]["set"], "ProfileType");
    assert_eq!(document["unknown"][0]["value"], LATER_TYPE);
    assert_eq!(document["unknown"][0]["route"], "/v1/clusterprofiles/{uid}");
}
