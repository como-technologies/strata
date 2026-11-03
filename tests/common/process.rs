//! The processes that the tests start, and the scripts that the tests write for them to run.
//!
//! The tests run on many threads. A thread that writes a script and runs it gets "Text file busy"
//! when another thread starts a process during the write. These items share one lock, so that a
//! start never happens during a write.
//!
//! `tests/common/mod.rs` re-exports this file for the tests of strata. The tests of
//! strata-preflight and of xtask include the file with `#[path]`, so that one lock covers the
//! processes of each package. `tests/spawn.rs` makes sure that no test of the three starts a
//! process without it.

use std::io;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::RwLock;

/// A test takes this lock for writing while it writes a file that it runs next, and for reading
/// while it starts a process.
///
/// A new process keeps a copy of each file that is open in its parent until it runs its program.
/// When a test runs a file in that time, the system answers "Text file busy". The tests run on
/// many threads, so a test failed with this answer in some runs.
static OPEN_FOR_WRITE: RwLock<()> = RwLock::new(());

/// Write a file and make it executable, for a stand-in command on a test's PATH.
pub fn executable(path: &Path, body: &str) {
    let made = {
        // no code panics while it holds the lock, so a panic never poisons the lock
        let _no_start = OPEN_FOR_WRITE.write().unwrap();
        std::fs::write(path, body).and_then(|()| make_executable(path))
    };
    made.unwrap();
}

#[cfg(unix)]
fn make_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn make_executable(_: &Path) -> io::Result<()> {
    Ok(())
}

/// The two ways to start a process in a test. A test never calls `spawn`, `output`, or `status` of
/// [`Command`]. While another test writes a script, they can start a process that keeps the script
/// open. See [`OPEN_FOR_WRITE`]. To wait for the end of a process, call `wait` on the result of
/// `spawn_locked`. The tests of `tests/spawn.rs` make sure of this.
pub trait CommandExt {
    /// As [`Command::spawn`].
    fn spawn_locked(&mut self) -> io::Result<Child>;

    /// As [`Command::output`]: no standard input, and the two outputs on pipes.
    fn output_locked(&mut self) -> io::Result<Output>;
}

impl CommandExt for Command {
    fn spawn_locked(&mut self) -> io::Result<Child> {
        // `spawn` gives its result after the new process runs its program, so it holds no copy
        // of an open file when the lock ends
        let _no_write = OPEN_FOR_WRITE.read().unwrap();
        self.spawn()
    }

    fn output_locked(&mut self) -> io::Result<Output> {
        self.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        self.spawn_locked()?.wait_with_output()
    }
}
