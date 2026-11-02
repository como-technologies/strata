//! The tests of the runner.

use super::*;
use std::os::unix::fs::PermissionsExt;

/// A runner whose stand-in is a script that writes a port and waits for the end of its input.
/// `test` names the directory of the test, so two tests can start a runner at once.
fn runner(mode: Mode, test: &str) -> Runner {
    let repo = std::env::temp_dir().join(format!("strata-book-{test}-{}", std::process::id()));
    std::fs::create_dir_all(&repo).expect("a directory");
    let script = repo.join("stand-in");
    std::fs::write(&script, "#!/bin/sh\necho 4242 > \"$3\"\ncat > /dev/null\n").expect("a script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("a mode");
    Runner::start(&repo, &repo.join("strata"), &script, mode).expect("a runner")
}

#[test]
fn a_served_run_writes_the_answer_of_the_chapter_it_builds_and_no_other() {
    let mode = Mode::Serve {
        chapter: "served".to_string(),
        seed: 1,
    };
    let mut r = runner(mode, "served");
    let answer = serde_json::json!({"ok": true});
    r.chapter("other", &[]);
    r.answer("p", "v1/other", &answer);
    assert!(!r.tenant.doc("p", "v1/other").exists());
    r.chapter("served", &[]);
    r.answer("p", "v1/served", &answer);
    assert!(r.tenant.doc("p", "v1/served").is_file());
    let _ = std::fs::remove_dir_all(r.repo.clone());
}

#[test]
fn a_run_keeps_the_routes_of_the_tenant_of_each_chapter_when_the_next_chapter_clears_it() {
    let mut r = runner(Mode::Write, "routes");
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
    let _ = std::fs::remove_dir_all(r.repo.clone());
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
