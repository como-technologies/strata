//! The tests of the runner.

use super::*;
use crate::process::executable;

/// A runner whose stand-in is a script that writes a port and waits for the end of its input. The
/// script is in `repo`, the directory of the test, so two tests can start a runner at once.
fn runner(mode: Mode, repo: &Path) -> Runner {
    let script = repo.join("stand-in");
    executable(&script, "#!/bin/sh\necho 4242 > \"$3\"\ncat > /dev/null\n");
    Runner::start(repo, &repo.join("strata"), &script, mode).expect("a runner")
}

#[test]
fn a_served_run_writes_the_answer_of_the_chapter_it_builds_and_no_other() {
    let mode = Mode::Serve {
        chapter: "served".to_string(),
        seed: 1,
    };
    let repo = tempfile::tempdir().unwrap();
    let mut r = runner(mode, repo.path());
    let answer = serde_json::json!({"ok": true});
    r.chapter("other", &[]);
    r.answer("p", "v1/other", &answer);
    assert!(!r.tenant.doc("p", "v1/other").exists());
    r.chapter("served", &[]);
    r.answer("p", "v1/served", &answer);
    assert!(r.tenant.doc("p", "v1/served").is_file());
}

#[test]
fn a_run_keeps_the_routes_of_the_tenant_of_each_chapter_when_the_next_chapter_clears_it() {
    let repo = tempfile::tempdir().unwrap();
    let mut r = runner(Mode::Write, repo.path());
    let answer = serde_json::json!({"ok": true});
    r.chapter("first", &[]);
    r.answer("p", "v1/first/u1", &answer);
    r.chapter("second", &[]);
    assert!(!r.tenant.doc("p", "v1/first/u1").exists());
    assert_eq!(r.routes, BTreeSet::from(["v1/first/u1".to_string()]));
    r.answer("p", "v1/second", &answer);
    r.routes.extend(r.tenant.routes());
    let routes: Vec<&str> = r.routes.iter().map(String::as_str).collect();
    assert_eq!(routes, ["v1/first/u1", "v1/second"]);
}

#[test]
fn a_playground_name_splits_into_its_chapter_and_its_state() {
    assert_eq!(playground("loop"), ("loop", 1));
    assert_eq!(playground("loop-3"), ("loop", 3));
    assert_eq!(playground("os-image"), ("os-image", 1));
    assert_eq!(playground("os-image-2"), ("os-image", 2));
    assert_eq!(
        playground("merge-gate-rollback"),
        ("merge-gate-rollback", 1)
    );
    assert_eq!(playground("loop-12"), ("loop-12", 1));
}

#[test]
fn each_duration_that_preflight_prints_becomes_one_fixed_value() {
    let clock = Clock::new();
    let cases = [
        (
            "strata: static: bastion-cost: jq over the world -> 12 ms, NOTE: no bastion",
            "strata: static: bastion-cost: jq over the world -> 1 ms, NOTE: no bastion",
        ),
        (
            "strata: static: floating-tag: jq over the world -> [\"a\"], 3 ms, WARN",
            "strata: static: floating-tag: jq over the world -> [\"a\"], 1 ms, WARN",
        ),
        (
            "  WARN  floating-tag  No profile pins a tag    104 ms",
            "  WARN  floating-tag  No profile pins a tag      1 ms",
        ),
        (
            // a title of 60 characters leaves one space before a count of five digits
            concat!(
                "  WARN  floating-tag  ",
                "No profile of the cluster pins a tag that moves to new image",
                " 12345 ms"
            ),
            concat!(
                "  WARN  floating-tag  ",
                "No profile of the cluster pins a tag that moves to new image",
                "     1 ms"
            ),
        ),
        (
            "| WARN | `floating-tag` | 7 ms | |",
            "| WARN | `floating-tag` | 1 ms | |",
        ),
        ("\"elapsedMs\": 250,", "\"elapsedMs\": 1,"),
        (
            "strata: watch: Running after 3s, reading 1",
            "strata: watch: Running after 0s, reading 1",
        ),
        (
            "made at 2026-10-03T11:02:59Z",
            "made at 2026-09-17T09:30:00Z",
        ),
    ];
    for (from, to) in cases {
        assert_eq!(clock.fixed(from), to, "{from}");
    }
}

#[test]
fn a_harvest_keeps_the_documents_of_each_chapter_in_its_own_directory_and_runs_no_command() {
    let repo = tempfile::tempdir().unwrap();
    let harvest = tempfile::tempdir().unwrap();
    let into = harvest.path().to_path_buf();
    let mut r = runner(Mode::Harvest { into: into.clone() }, repo.path());
    let answer = serde_json::json!({"ok": true});
    r.chapter("first", &[]);
    r.answer("p", "v1/first/u1", &answer);
    // a harvest runs no command: this step exits with 7, and a run of it stops the run
    r.step("never-run", 0, false, "exit 7");
    r.chapter("second", &[]);
    r.answer("p", "v1/second", &answer);
    assert!(into.join("first/p/v1/first/u1.json").is_file());
    assert!(!into.join("second").exists(), "the chapter has not ended");
    let _ = r.finish();
    assert!(into.join("second/p/v1/second.json").is_file());
    assert!(!into.join("first/p/v1/second.json").exists());
    assert!(!into.join("second/p/v1/first/u1.json").exists());
}

#[test]
fn the_examples_are_built_when_the_first_example_profile_is_there() {
    let repo = tempfile::tempdir().unwrap();
    assert_eq!(
        built_examples(repo.path()),
        Err("the examples are not built. Run: just examples".to_string())
    );
    let profile = repo.path().join(FIRST_EXAMPLE);
    std::fs::create_dir_all(profile.parent().unwrap()).unwrap();
    std::fs::write(&profile, "{}").unwrap();
    assert_eq!(built_examples(repo.path()), Ok(()));
}

#[test]
fn a_step_of_the_book_gets_the_stand_in_for_the_endpoint_and_no_setting_of_the_machine() {
    let repo = tempfile::tempdir().unwrap();
    let mut r = runner(Mode::Write, repo.path());
    r.chapter("send", &[]);
    let (code, address, _) = r.run("printf '%s' \"$STRATA_REPORT_URL\"", false);
    assert_eq!(code, 0);
    // the stand-in runs on this machine, so a step that sends posts to nothing outside it
    assert_eq!(address, "http://127.0.0.1:4242/strata-report");
    assert_ne!(
        Some(address.as_str()),
        strata::closed_set::unknown::send::ENDPOINT
    );
    // the step starts from an empty environment: a setting of the machine does not reach it
    let (code, setting, _) = r.run("printf '%s' \"${STRATA_REPORT-unset}\"", false);
    assert_eq!((code, setting.as_str()), (0, "unset"));
}
