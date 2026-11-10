//! The processes that the tests start, and the scripts that the tests write for them to run.
//!
//! The tests run on many threads. A thread that writes a script and runs it gets "Text file busy"
//! when another thread starts a process during the write. Many tests write a script and run it.
//! So one run of the tests in about twenty failed, and the test that failed changed from run to
//! run.

use std::path::Path;
use std::process::Command;

use crate::common::{CommandExt, executable, walk};

/// The threads that write and run scripts.
const THREADS: usize = 16;

/// The scripts that each thread writes and runs.
const SCRIPTS: usize = 100;

#[test]
fn a_script_that_a_test_writes_runs_while_other_threads_write_scripts_and_start_processes() {
    let dir = tempfile::tempdir().unwrap();
    std::thread::scope(|scope| {
        for thread in 0..THREADS {
            let dir = dir.path();
            scope.spawn(move || {
                for n in 0..SCRIPTS {
                    let script = dir.join(format!("tool-{thread}-{n}"));
                    executable(&script, "#!/bin/sh\nexit 0\n");
                    let o = Command::new(&script).output_locked();
                    let o = o.unwrap_or_else(|e| panic!("{}: {e}", script.display()));
                    assert!(o.status.success(), "{}", script.display());
                }
            });
        }
    });
}

/// The trees of Rust files that start a process in a test, from the root of the repository. They
/// are the tests of strata, the tests of strata-preflight, and the source of xtask. The unit tests
/// and the runner of the book in xtask start processes. The three include
/// `tests/common/process.rs`. The other packages start no process.
const TREES: [&str; 3] = ["tests", "crates/strata-preflight/tests", "xtask/src"];

/// Each Rust file of [`TREES`] but the file that holds the lock and the two methods that take it.
fn files_that_start_processes() -> Vec<std::path::PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let own = root.join("tests/common/process.rs");
    TREES
        .iter()
        .flat_map(|tree| walk(&root.join(tree)))
        .filter(|f| f.extension().is_some_and(|e| e == "rs") && *f != own)
        .collect()
}

/// A line without the status of an HTTP answer, which `status()` of a response also names.
fn without_http_status(line: &str) -> String {
    line.replace(".status().as_u16()", "")
}

#[test]
fn no_test_starts_a_process_with_a_method_of_command_that_takes_no_lock() {
    // the method names are in two parts, so that this file does not hold them
    let methods = ["output", "spawn", "status"].map(|m| format!(".{m}()"));
    let mut found = Vec::new();
    for file in files_that_start_processes() {
        let text = std::fs::read_to_string(&file).unwrap();
        for (n, line) in text.lines().enumerate() {
            let line = without_http_status(line);
            if methods.iter().any(|m| line.contains(m.as_str())) {
                found.push(format!("{}:{}: {}", file.display(), n + 1, line.trim()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "use `output_locked` or `spawn_locked` of `CommandExt`:\n{}",
        found.join("\n")
    );
}

#[test]
fn the_trees_that_the_scan_reads_are_the_trees_that_include_the_lock() {
    // a package that starts a process in a test includes the file with `#[path]`. The path goes up
    // to the root with one `../` for each level of the file, so its depth is not fixed.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let includes = |text: &str| {
        text.lines().any(|line| {
            line.strip_prefix("#[path = \"")
                .and_then(|path| path.strip_suffix("tests/common/process.rs\"]"))
                .is_some_and(|up| !up.is_empty() && up.split_terminator('/').all(|p| p == ".."))
        })
    };
    for file in ["crates/strata-preflight/tests/cli.rs", "xtask/src/main.rs"] {
        let text = std::fs::read_to_string(root.join(file)).unwrap();
        assert!(includes(&text), "{file} includes process.rs");
    }
    for tree in TREES {
        assert!(root.join(tree).is_dir(), "{tree}");
    }
}

/// The two files that give git its settings: the helper of the tests and of the runner of the
/// book, and the function of the live scripts, which are bash.
const GIT_HELPERS: [&str; 2] = ["tests/common/git.rs", "examples/live/lib.sh"];

#[test]
fn no_test_chapter_or_live_script_keeps_a_copy_of_the_git_settings() {
    // the names are in two parts, so that this file does not hold them
    let settings = [
        "CONFIG_GLOBAL",
        "CONFIG_SYSTEM",
        "AUTHOR_NAME",
        "COMMITTER_NAME",
    ]
    .map(|name| format!("GIT_{name}"));
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scripts = walk(&root.join("examples/live"))
        .into_iter()
        .filter(|f| f.extension().is_some_and(|e| e == "sh"));
    let mut found = Vec::new();
    for file in files_that_start_processes().into_iter().chain(scripts) {
        let name = file
            .strip_prefix(root)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        if GIT_HELPERS.contains(&name.as_str()) {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap();
        for setting in settings.iter().filter(|s| text.contains(s.as_str())) {
            found.push(format!("{name}: {setting}"));
        }
    }
    assert!(
        found.is_empty(),
        "give git its settings with `isolate_git` of {GIT_HELPERS:?}:\n{}",
        found.join("\n")
    );
}
