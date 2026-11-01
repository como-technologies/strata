//! The client that a command line builds from the environment: the key, the host, the retry
//! policy, and the project, which is a uid or a name.
//!
//! `strata` and `strata-preflight` connect in this one way, so a setting of the environment means
//! the same in both.

use std::sync::OnceLock;

use super::{HttpApi, Palette, Retry, RetryPolicy};
use crate::config::{Config, PROJECT_NAME_VAR, PROJECT_VARS};
use crate::error::Result;
use crate::term::progress::Progress;

/// A client for `config`, with the retry policy that `config` gives.
pub fn connect_with(config: Config, progress: Progress) -> Palette<Retry<HttpApi>> {
    let policy = RetryPolicy::from_config(&config);
    Palette::new(Retry::new(HttpApi::new(config), policy, progress))
}

/// A client from the environment and from `~/.palette.env`.
///
/// When `PALETTE_PROJECT` names the project and no uid is set, it finds the uid first, at tenant
/// scope. When both are set, the uid decides, and a warning says so if the two name two projects.
///
/// # Errors
///
/// - `MissingCredential` when no key is set.
/// - `ProjectNotFound` when `PALETTE_PROJECT` names no project, and each error of the lookup.
pub fn connect_from_env(progress: Progress) -> Result<Palette<Retry<HttpApi>>> {
    let mut config = Config::from_env()?;
    if let Some(name) = config.project_name.clone() {
        config.project_uid = Some(project_uid_named(&config, &name, progress)?);
    }
    warn_on_scope_conflict(&config, progress);
    Ok(connect_with(config, progress))
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

/// The uid of the project named `name`, looked up at tenant scope once per run.
///
/// # Errors
///
/// [`Error::ProjectNotFound`](crate::Error::ProjectNotFound) when no project has the name, and
/// each error of the request.
pub fn project_uid_named(config: &Config, name: &str, progress: Progress) -> Result<String> {
    static RESOLVED: OnceLock<(String, String)> = OnceLock::new();
    if let Some((_, uid)) = RESOLVED.get().filter(|(n, _)| n == name) {
        return Ok(uid.clone());
    }
    let mut tenant = config.clone();
    tenant.project_uid = None;
    progress.note(&format!("looking up project {name}"));
    let uid = connect_with(tenant, progress).project_uid_named(name)?;
    // the cache holds one name: when it holds another, the next lookup of this name asks again
    let _ = RESOLVED.set((name.to_string(), uid.clone()));
    Ok(uid)
}
