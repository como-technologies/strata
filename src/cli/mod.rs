//! The run of each command, and the parts that the commands share.
//!
//! The shared parts are the connection, the progress notes, the time, the records of a pattern,
//! and the exit code.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::OnceLock;

use strata::api::{HttpApi, Palette, Retry, RetryPolicy};
use strata::config::{self, Config, PROJECT_NAME_VAR, PROJECT_VARS};
use strata::error::{Error, Result};
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

/// A client from the environment; `project` replaces `PALETTE_PROJECT_UID` or `PALETTE_PROJECT`
/// when given, and an empty one is tenant scope.
fn connect(progress: Progress, project: Option<&str>) -> Result<Palette<Retry<HttpApi>>> {
    let mut config = Config::from_env()?;
    match project {
        Some(p) => config.project_uid = ops::scope_of(p).map(str::to_string),
        None => {
            if let Some(name) = config.project_name.clone() {
                config.project_uid = Some(project_uid_named(&config, &name, progress)?);
            }
            warn_on_scope_conflict(&config, progress);
        }
    }
    Ok(client(config, progress))
}

/// `PALETTE_PROJECT_UID` and `PALETTE_PROJECT` were both set, so the name decided nothing. Say so
/// when the two name two projects, because every answer that follows is about the uid's project
/// and the caller asked for the name's.
///
/// This costs one listing, and only when both are set. It says nothing when the two agree, which
/// is the common setting: the playground prints both lines, and so do many `~/.palette.env`
/// files. A name that resolves to nothing is a warning too, and not an error: a stale variable
/// that decides nothing must not fail a command that asked about something else.
///
/// A quiet run skips the listing as well as the warning. The warning is the only reason to make
/// that request, and with `--json` a quiet run writes one JSON object to stderr and nothing else,
/// which a caller reads.
fn warn_on_scope_conflict(config: &Config, progress: Progress) {
    if progress.is_quiet() {
        return;
    }
    let (Some(uid), Some(name)) = (&config.project_uid, &config.shadowed_project_name) else {
        return;
    };
    let (uid_var, name_var) = (PROJECT_VARS[0], PROJECT_NAME_VAR);
    match project_uid_named(config, name, progress) {
        Ok(named) if &named == uid => {}
        Ok(named) => progress.warn(&format!(
            "{uid_var} is {uid} and {name_var} is '{name}', which is {named}. \
             A uid has priority, so this answers for {uid}. Clear one of the two: \
             `{uid_var}= {name_var}={name}` asks for the project you named."
        )),
        Err(e) => progress.warn(&format!(
            "{name_var} is '{name}', which no project matches ({e}). \
             {uid_var} is {uid}, and a uid has priority, so this answers for {uid}."
        )),
    }
}

fn client(config: Config, progress: Progress) -> Palette<Retry<HttpApi>> {
    let policy = RetryPolicy::from_config(&config);
    Palette::new(Retry::new(HttpApi::new(config), policy, progress))
}

/// The uid of the project named `name`, looked up at tenant scope once per run.
fn project_uid_named(config: &Config, name: &str, progress: Progress) -> Result<String> {
    static RESOLVED: OnceLock<(String, String)> = OnceLock::new();
    if let Some((_, uid)) = RESOLVED.get().filter(|(n, _)| n == name) {
        return Ok(uid.clone());
    }
    let mut tenant = config.clone();
    tenant.project_uid = None;
    progress.note(&format!("looking up project {name}"));
    let uid = client(tenant, progress).project_uid_named(name)?;
    let _ = RESOLVED.set((name.to_string(), uid.clone()));
    Ok(uid)
}

/// `--project`, or the uid of the project `--project-name` names.
fn project_arg(progress: Progress, project: &ProjectArgs) -> Result<Option<String>> {
    match &project.project_name {
        Some(n) => Ok(Some(project_uid_named(&Config::from_env()?, n, progress)?)),
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

fn is_pattern(s: &str) -> bool {
    s.contains(['*', '?', '['])
}

/// The record files named by the arguments, with patterns expanded by strata so a quoted glob
/// works from any shell or scheduler. A pattern that matches nothing is an error.
fn expand_records(args: &[String]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for arg in args {
        if !is_pattern(arg) {
            out.push(PathBuf::from(arg));
            continue;
        }
        let matches: Vec<PathBuf> = glob::glob(arg)
            .map_err(|e| Error::Pattern {
                pattern: arg.clone(),
                message: format!("is not a valid pattern: {}", e.msg),
            })?
            .filter_map(std::result::Result::ok)
            .filter(|p| p.is_file())
            .collect();
        if matches.is_empty() {
            return Err(Error::Pattern {
                pattern: arg.clone(),
                message: "matched no files".to_string(),
            });
        }
        out.extend(matches);
    }
    Ok(out)
}
