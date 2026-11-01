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

/// Each file of the tests but the two that hold the lock and its test.
fn files_that_start_processes() -> Vec<std::path::PathBuf> {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let own = [tests.join("common/mod.rs"), tests.join("spawn.rs")];
    walk(&tests)
        .into_iter()
        .filter(|f| f.extension().is_some_and(|e| e == "rs") && !own.contains(f))
        .collect()
}

#[test]
fn no_test_starts_a_process_with_a_method_of_command_that_takes_no_lock() {
    // the method names are in two parts, so that this file does not hold them
    let methods = ["output", "spawn", "status"].map(|m| format!(".{m}()"));
    let mut found = Vec::new();
    for file in files_that_start_processes() {
        let text = std::fs::read_to_string(&file).unwrap();
        for (n, line) in text.lines().enumerate() {
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
