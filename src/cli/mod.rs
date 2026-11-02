//! The run of each command, and the parts that the commands share.
//!
//! The shared parts are the connection, the progress notes, the time, and the exit code.

use std::io::IsTerminal;
use std::process::ExitCode;

use strata::api::{self, HttpApi, Palette, Retry};
use strata::closed_set::unknown::{self, Hint, Watch, Watching};
use strata::config::{self, Config};
use strata::error::Result;
use strata::ops;
use strata::term::progress::Progress;

use args::ProjectArgs;

pub(crate) mod args;
pub(crate) mod behind;
pub(crate) mod candidate;
pub(crate) mod compare;
pub(crate) mod consumers;
pub(crate) mod list;
pub(crate) mod packs;
pub(crate) mod preflight;
pub(crate) mod report;
pub(crate) mod same_release;
pub(crate) mod schema;
pub(crate) mod snapshot;
pub(crate) mod verify;

fn now_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn progress(quiet: bool) -> Progress {
    Progress::new(quiet || config::quiet_from_env())
}

/// Watch the reads of the command for a value of a closed set that Palette gives and this release
/// does not know. Each such value gets one note through `progress`, and one line in the log in the
/// state folder. In a terminal, the first note of a day also gets a hint to run `strata report`.
/// The watch ends when the guard drops.
pub(crate) fn watch_unknown(progress: Progress) -> Watching {
    let watch = Watch::new(progress);
    let watch = match unknown::log::path() {
        Some(file) => watch.logging_to(file),
        None => watch,
    };
    match unknown::hint::path() {
        Some(file) => watch.hinting(Hint::new(
            file,
            chrono::Utc::now(),
            std::io::stderr().is_terminal(),
        )),
        None => watch,
    }
    .start()
}

/// A client from the environment; `project` replaces `PALETTE_PROJECT_UID` or `PALETTE_PROJECT`
/// when given, and an empty one is tenant scope.
fn connect(progress: Progress, project: Option<&str>) -> Result<Palette<Retry<HttpApi>>> {
    match project {
        Some(p) => {
            let mut config = Config::from_env()?;
            config.project_uid = ops::scope_of(p).map(str::to_string);
            Ok(api::connect_with(config, progress))
        }
        None => api::connect_from_env(progress),
    }
}

/// `--project`, or the uid of the project `--project-name` names.
fn project_arg(progress: Progress, project: &ProjectArgs) -> Result<Option<String>> {
    match &project.project_name {
        Some(n) => Ok(Some(api::project_uid_named(
            &Config::from_env()?,
            n,
            progress,
        )?)),
        None => Ok(project.project.clone()),
    }
}

fn exit_for(same: bool) -> ExitCode {
    if same {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
