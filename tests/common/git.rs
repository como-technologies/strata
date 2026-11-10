//! The git settings of each git command that a test or a chapter of the book starts.
//!
//! `tests/common/mod.rs` re-exports this file for the tests of strata. The runner of the book in
//! xtask includes it with `#[path]`. So the tests and the chapters keep one copy of the settings.
//! The live scripts are bash: `isolate_git` of `examples/live/lib.sh` gives them the same settings.

use std::ffi::OsString;
use std::process::Command;

/// The settings that give git no file of the machine and one author. So a signing setting of the
/// machine cannot reach a commit.
const SETTINGS: [(&str, &str); 6] = [
    ("GIT_CONFIG_GLOBAL", "/dev/null"),
    ("GIT_CONFIG_SYSTEM", "/dev/null"),
    ("GIT_AUTHOR_NAME", "t"),
    ("GIT_AUTHOR_EMAIL", "t@example.com"),
    ("GIT_COMMITTER_NAME", "t"),
    ("GIT_COMMITTER_EMAIL", "t@example.com"),
];

/// The start of the name of each variable that gives git a setting or a file of settings, such as
/// `GIT_CONFIG_COUNT`, `GIT_CONFIG_PARAMETERS`, and each `GIT_CONFIG_KEY_<n>`.
///
/// Git reads `GIT_CONFIG_COUNT` and `GIT_CONFIG_PARAMETERS` after the config of the repository, so
/// their settings win over the config that a test or a step writes. A container can set the user of
/// git with them.
const FROM_THE_ENVIRONMENT: &str = "GIT_CONFIG";

/// Give `command` no git setting of the machine and one author, so that the machine cannot change
/// what git does in a test or in a chapter of the book.
///
/// The command reads no global or system file, and no setting from the environment. The function
/// removes each variable whose name starts with [`FROM_THE_ENVIRONMENT`], also when the caller gave
/// it to `command`. A caller that gives one on purpose gives it after this call.
pub fn isolate_git(command: &mut Command) -> &mut Command {
    let names: Vec<OsString> = std::env::vars_os()
        .map(|(name, _)| name)
        .chain(command.get_envs().map(|(name, _)| name.to_os_string()))
        .filter(|name| {
            name.as_encoded_bytes()
                .starts_with(FROM_THE_ENVIRONMENT.as_bytes())
        })
        .collect();
    for name in &names {
        command.env_remove(name);
    }
    command.envs(SETTINGS)
}
