//! The run of each command, and the parts that the commands share.
//!
//! The shared parts are the connection, the progress notes, the time, and the exit code.

use std::io::IsTerminal;
use std::process::ExitCode;

use strata::api::{self, HttpApi, Palette, Retry};
use strata::closed_set::unknown::hint::Hint;
use strata::closed_set::unknown::{self, Watch, Watching, send};
use strata::config::{self, Config};
use strata::error::Result;
use strata::ops;
use strata::term::progress::Progress;
use strata::timestamp::now_utc;

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

/// At the end of a run, send the report of the log to the endpoint when the user chose the send,
/// or ask the user one time. The user chooses the send with `STRATA_REPORT=1` or by an answer of
/// yes. A run in CI, a run without a terminal, and a run that saw no unknown value send nothing and
/// ask nothing.
///
/// This never changes the exit code of the run. An error of the send is a warning, and
/// `--quiet` hides it.
pub(crate) fn send_unknown(watching: &Watching, progress: Progress) {
    let terminal =
        !progress.is_quiet() && std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    let facts = send::Facts::of_run(watching.lines_written() > 0, terminal);
    if let Some(complaint) = facts.complaint() {
        progress.warn(&complaint);
    }
    let go = match send::step(&facts) {
        send::Step::Send => true,
        send::Step::Ask => ask_to_send(progress),
        // `Nothing`, and a step that a later release adds and this one does not know
        _ => false,
    };
    if let (true, Some(endpoint), Some(log)) = (go, &facts.endpoint, unknown::log::path()) {
        match ops::send_unknown_report(&log, endpoint) {
            Ok(()) => progress.note("sent the report of the unknown values"),
            Err(e) => progress.warn(&e.to_string()),
        }
    }
}

/// Ask the user the question of the send, and keep the answer in the state folder so that strata
/// asks one time. True when the answer is yes.
fn ask_to_send(progress: Progress) -> bool {
    let yes = strata::term::ask::yes_or_no(
        &send::question(),
        &mut std::io::stdin().lock(),
        &mut std::io::stderr(),
    );
    let choice = if yes {
        send::Choice::Yes
    } else {
        send::Choice::No
    };
    if let Some(file) = send::answer_path()
        && let Err(e) = send::keep_answer(&file, choice)
    {
        progress.warn(&format!("could not keep the answer: {e}"));
    }
    yes
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
