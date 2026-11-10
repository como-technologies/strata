//! Where Palette is and how to authenticate. Read from the environment, with `~/.palette.env` as
//! the fallback, or built directly by a library user.

use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use crate::TOOL_NAME;
use crate::error::{Error, Result};

/// The default Palette API base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.spectrocloud.com";

/// The file in the home directory that holds the settings below the environment.
pub(crate) const ENV_FILE: &str = ".palette.env";

/// The variables that hold the API key, the first one preferred.
pub const KEY_VARS: [&str; 2] = ["PALETTE_API_KEY", "SPECTROCLOUD_APIKEY"];
/// The variables that hold the address of Palette, the first one preferred.
pub const URL_VARS: [&str; 2] = ["PALETTE_API", "SPECTROCLOUD_HOST"];
/// The variables that hold the uid of the project.
pub const PROJECT_VARS: [&str; 1] = ["PALETTE_PROJECT_UID"];
/// The variable that holds the name of the project, below the uid.
pub const PROJECT_NAME_VAR: &str = "PALETTE_PROJECT";
/// The variable that stops the progress messages.
const QUIET_VAR: &str = "STRATA_QUIET";
/// The variable that pipelines set. A run with it never asks a question.
const CI_VAR: &str = "CI";
const RETRIES_VAR: &str = "STRATA_RETRIES";
const DEFAULT_RETRIES: u32 = 3;
/// The variable that sets the longest `Retry-After` wait that strata takes.
pub(crate) const MAX_RETRY_AFTER_VAR: &str = "STRATA_MAX_RETRY_AFTER";
const DEFAULT_MAX_RETRY_AFTER: Duration = Duration::from_secs(300);

/// Connection settings for one Palette tenant or project.
#[derive(Clone)]
pub struct Config {
    /// The tenant or project API key, sent as the `ApiKey` header.
    pub api_key: String,
    /// Base URL without a trailing slash.
    pub base_url: String,
    /// Sent as the `ProjectUid` header when set; scopes requests to one project.
    pub project_uid: Option<String>,
    /// A project by name, from `PALETTE_PROJECT` when `PALETTE_PROJECT_UID` is unset. A client
    /// resolves it to a uid with [`crate::api::Palette::project_uid_named`] at tenant scope.
    pub project_name: Option<String>,
    /// A name from `PALETTE_PROJECT` that a uid in `PALETTE_PROJECT_UID` overrode. Both were set,
    /// so the name decided nothing. The caller asked for one project by two means, and the two
    /// can name two projects: a caller that acts on the answer reads the wrong project and
    /// nothing says so. A command resolves this name and warns when the uids differ.
    pub shadowed_project_name: Option<String>,
    /// Per-request timeout.
    pub timeout: Duration,
    /// Retries after a failed attempt on a transient error; `STRATA_RETRIES`, default 3.
    pub retries: u32,
    /// The longest `Retry-After` strata waits for; a longer one fails the request at once.
    /// `STRATA_MAX_RETRY_AFTER` in seconds, default five minutes.
    pub max_retry_after: Duration,
}

// Written by hand, and not derived: a derived Debug prints `api_key`. One `{:?}` on a Config, in
// this crate or in a program that uses it as a library, would put the key in a log or on a
// terminal. The key is the one secret strata holds, so it never leaves this type.
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("api_key", &"<redacted>")
            .field("base_url", &self.base_url)
            .field("project_uid", &self.project_uid)
            .field("project_name", &self.project_name)
            .field("shadowed_project_name", &self.shadowed_project_name)
            .field("timeout", &self.timeout)
            .field("retries", &self.retries)
            .field("max_retry_after", &self.max_retry_after)
            .finish()
    }
}

impl Config {
    /// Build from a base URL and key; the project is unset and the timeout is two minutes.
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Config {
            api_key: api_key.into(),
            base_url: normalize_url(base_url.into()),
            project_uid: None,
            project_name: None,
            shadowed_project_name: None,
            timeout: Duration::from_secs(120),
            retries: DEFAULT_RETRIES,
            max_retry_after: DEFAULT_MAX_RETRY_AFTER,
        }
    }

    /// Scope requests to one project.
    pub fn with_project(mut self, project_uid: impl Into<String>) -> Self {
        self.project_uid = Some(project_uid.into());
        self
    }

    /// Scope requests to the tenant: they name no project. A route of the tenant refuses a request
    /// that names one.
    pub fn without_project(mut self) -> Self {
        self.project_uid = None;
        self
    }

    /// Read `PALETTE_API_KEY` (or `SPECTROCLOUD_APIKEY`), `PALETTE_API` (or `SPECTROCLOUD_HOST`),
    /// `PALETTE_PROJECT_UID` (or the name `PALETTE_PROJECT`), `STRATA_RETRIES`, and
    /// `STRATA_MAX_RETRY_AFTER` from the environment, then from `~/.palette.env`.
    ///
    /// Strata reads the file only when `HOME` is an absolute path. An empty or a relative `HOME`
    /// names a folder below the working folder, so strata then reads the environment alone.
    ///
    /// # Errors
    ///
    /// The errors of [`Config::from_lookup`], with the environment and `~/.palette.env` as the
    /// lookup.
    pub fn from_env() -> Result<Self> {
        Self::from_lookup(|key| {
            // An empty project variable is a choice, not an absence: it says tenant scope, as
            // `--project ''` does, and it must beat `~/.palette.env`. Without this, a reader who
            // keeps a project in that file cannot reach tenant scope from the environment. Both
            // `unset` and an empty value go to the file. For every other variable an empty value
            // stays the same as an unset one.
            if (PROJECT_VARS.contains(&key) || key == PROJECT_NAME_VAR)
                && let Ok(value) = std::env::var(key)
            {
                return Some(value);
            }
            setting(key)
        })
        .map(|mut cfg| {
            cfg.shadowed_project_name = shadowed_project_name(
                env_nonempty(PROJECT_VARS[0]),
                env_nonempty(PROJECT_NAME_VAR),
                dotenv_lookup(PROJECT_VARS[0]),
                dotenv_lookup(PROJECT_NAME_VAR),
            );
            cfg
        })
    }

    /// Like [`Config::from_env`] but with a caller-supplied lookup; this is what tests use.
    /// `lookup` gives the value of a variable by its name, or `None` when it has none.
    ///
    /// # Errors
    ///
    /// [`Error::MissingCredential`] when `lookup` gives no key. [`Error::InvalidSetting`] when the
    /// retries or the longest `Retry-After` is not a whole number.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let first = |names: &[&str]| names.iter().find_map(|n| lookup(n));
        let api_key = first(&KEY_VARS).ok_or(Error::MissingCredential {
            primary: KEY_VARS[0],
            alternate: KEY_VARS[1],
        })?;
        let base_url = first(&URL_VARS).unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let mut cfg = Config::new(base_url, api_key);
        // An empty uid means no uid, and it stops `~/.palette.env` from giving one. It does not
        // stop the name: `PALETTE_PROJECT_UID= PALETTE_PROJECT=dev` is a project, and the
        // playground prints both lines. Tenant scope is an empty name as well as an empty uid.
        match first(&PROJECT_VARS) {
            Some(uid) if !uid.is_empty() => cfg.project_uid = Some(uid),
            _ => cfg.project_name = lookup(PROJECT_NAME_VAR).filter(|n| !n.is_empty()),
        }
        if let Some(v) = lookup(RETRIES_VAR) {
            cfg.retries = v.trim().parse().map_err(|_| Error::InvalidSetting {
                variable: RETRIES_VAR,
                value: v.clone(),
                expected: "a whole number of retries",
            })?;
        }
        if let Some(v) = lookup(MAX_RETRY_AFTER_VAR) {
            let seconds: u64 = v.trim().parse().map_err(|_| Error::InvalidSetting {
                variable: MAX_RETRY_AFTER_VAR,
                value: v.clone(),
                expected: "a whole number of seconds",
            })?;
            cfg.max_retry_after = Duration::from_secs(seconds);
        }
        Ok(cfg)
    }
}

fn normalize_url(url: String) -> String {
    let with_scheme = if url.starts_with("http://") || url.starts_with("https://") {
        url
    } else {
        format!("https://{url}")
    };
    with_scheme.trim_end_matches('/').to_string()
}

/// The name that a uid overrode, and that is worth a warning.
///
/// The environment is above `~/.palette.env`, and a uid is above a name. A name loses without
/// complaint only to a uid from a higher place: a name in the file, under a uid in the
/// environment, is the fallback doing its job. A name the caller set at or above the uid's place
/// lost to something they did not mean to give twice, so say so. The worst of those is a name in
/// the environment under a uid in the file, where a command the caller typed loses to a file they
/// wrote months ago.
fn shadowed_project_name(
    env_uid: Option<String>,
    env_name: Option<String>,
    file_uid: Option<String>,
    file_name: Option<String>,
) -> Option<String> {
    let set = |v: Option<String>| v.filter(|s| !s.is_empty());
    let (env_uid, env_name) = (set(env_uid), set(env_name));
    if env_uid.is_some() {
        env_name
    } else if set(file_uid).is_some() {
        env_name.or_else(|| set(file_name))
    } else {
        None
    }
}

/// True when `STRATA_QUIET` is set to any value, also an empty one. It stops the progress
/// messages, as `--quiet` does.
pub fn quiet_from_env() -> bool {
    std::env::var_os(QUIET_VAR).is_some()
}

/// The variable that names the base folder of the files that a program keeps for itself.
const STATE_HOME_VAR: &str = "XDG_STATE_HOME";
/// The variable that names the home folder.
const HOME_VAR: &str = "HOME";

/// The folder that `value`, the value of a variable, names. `None` when the value is not set, or
/// is not an absolute path.
///
/// An empty or a relative value names a folder below the working folder, such as a checkout of a
/// job, and not a folder of the person. So strata reads no `~/.palette.env` and keeps no state
/// below such a `HOME`. This one rule gives the home for the key file and for the state folder.
fn absolute_folder(value: Option<OsString>) -> Option<PathBuf> {
    value.map(PathBuf::from).filter(|p| p.is_absolute())
}

/// The folder where strata keeps the files of its own state: `$XDG_STATE_HOME/strata`, or
/// `~/.local/state/strata` when the variable is not set. `None` when strata knows no home.
///
/// A relative `XDG_STATE_HOME` is no place, so strata skips it, as the XDG Base Directory
/// Specification says. The folder can be missing: the code that writes a file creates it.
pub(crate) fn state_dir() -> Option<PathBuf> {
    state_dir_from(std::env::var_os(STATE_HOME_VAR), std::env::var_os(HOME_VAR))
}

/// [`state_dir`] for the given values of `XDG_STATE_HOME` and `HOME`.
fn state_dir_from(state_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let base = absolute_folder(state_home)
        .or_else(|| absolute_folder(home).map(|h| h.join(".local/state")))?;
    Some(base.join(TOOL_NAME))
}

/// The file `~/.palette.env` for `home`, the value of `HOME`. `None` when strata knows no home:
/// see [`absolute_folder`].
fn env_file_from(home: Option<OsString>) -> Option<PathBuf> {
    absolute_folder(home).map(|h| h.join(ENV_FILE))
}

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

/// The setting `key`: the environment first, then a line of `~/.palette.env`. An empty value is
/// no setting. [`Config::from_env`] reads its settings in the same order.
pub(crate) fn setting(key: &str) -> Option<String> {
    env_nonempty(key).or_else(|| dotenv_lookup(key))
}

/// True when the variable `CI` has a value: a job of a pipeline runs strata, and no person answers
/// a question.
pub(crate) fn in_ci() -> bool {
    env_nonempty(CI_VAR).is_some()
}

/// `KEY=VALUE` lines from `~/.palette.env`, with optional `export ` and quotes. With no absolute
/// `HOME`, strata reads no file.
fn dotenv_lookup(key: &str) -> Option<String> {
    let file = env_file_from(std::env::var_os(HOME_VAR))?;
    let text = fs::read_to_string(file).ok()?;
    parse_dotenv(&text, key)
}

fn parse_dotenv(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        if let Some((k, v)) = line.split_once('=')
            && k.trim() == key
        {
            let v = v.trim().trim_matches('"').trim_matches('\'');
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let m: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| m.get(k).cloned()
    }

    #[test]
    fn a_config_without_a_project_names_no_project_and_keeps_the_rest() {
        let cfg = Config::new("https://api.example.com", "k")
            .with_project("p1")
            .without_project();
        assert_eq!(cfg.project_uid, None);
        assert_eq!(cfg.api_key, "k");
        assert_eq!(cfg.base_url, "https://api.example.com");
        // a config with no project stays so
        assert_eq!(cfg.clone().without_project().project_uid, None);
    }

    #[test]
    fn missing_key_is_an_error_naming_both_variables() {
        let err = Config::from_lookup(lookup(&[])).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("PALETTE_API_KEY is required"));
        assert!(msg.contains("SPECTROCLOUD_APIKEY"));
    }

    #[test]
    fn alternate_names_and_defaults() {
        let cfg = Config::from_lookup(lookup(&[
            ("SPECTROCLOUD_APIKEY", "k"),
            ("SPECTROCLOUD_HOST", "api.example.com/"),
        ]))
        .unwrap();
        assert_eq!(cfg.api_key, "k");
        assert_eq!(cfg.base_url, "https://api.example.com");
        assert!(cfg.project_uid.is_none());
        let cfg = Config::from_lookup(lookup(&[
            ("PALETTE_API_KEY", "k"),
            ("PALETTE_PROJECT_UID", "p1"),
        ]))
        .unwrap();
        assert_eq!(cfg.base_url, DEFAULT_BASE_URL);
        assert_eq!(cfg.project_uid.as_deref(), Some("p1"));
    }

    /// An empty project variable asks for tenant scope. It must stop the search, so that no later
    /// source — the name variable, or `~/.palette.env` — puts a project back. Without this a
    /// reader who keeps a project in that file cannot reach tenant scope from the environment.
    #[test]
    fn an_empty_project_variable_is_tenant_scope_and_stops_the_search() {
        // an empty uid is not tenant scope on its own: the playground prints an empty uid and a
        // project name together, and the pages then run `PALETTE_PROJECT=dev strata ...`
        let cfg = Config::from_lookup(lookup(&[
            ("PALETTE_API_KEY", "k"),
            ("PALETTE_PROJECT_UID", ""),
            ("PALETTE_PROJECT", "dev"),
        ]))
        .unwrap();
        assert!(cfg.project_uid.is_none(), "an empty uid gives no uid");
        assert_eq!(
            cfg.project_name.as_deref(),
            Some("dev"),
            "an empty uid must not suppress an explicit project name"
        );

        let cfg = Config::from_lookup(lookup(&[("PALETTE_API_KEY", "k"), ("PALETTE_PROJECT", "")]))
            .unwrap();
        assert!(cfg.project_uid.is_none());
        assert!(cfg.project_name.is_none(), "an empty name is tenant scope");
    }

    #[test]
    fn retries_default_to_three_and_read_strata_retries() {
        let cfg = Config::from_lookup(lookup(&[("PALETTE_API_KEY", "k")])).unwrap();
        assert_eq!(cfg.retries, 3);
        let cfg = Config::from_lookup(lookup(&[("PALETTE_API_KEY", "k"), ("STRATA_RETRIES", "0")]))
            .unwrap();
        assert_eq!(cfg.retries, 0);
        let err = Config::from_lookup(lookup(&[
            ("PALETTE_API_KEY", "k"),
            ("STRATA_RETRIES", "lots"),
        ]))
        .unwrap_err();
        assert!(
            matches!(
                err,
                Error::InvalidSetting {
                    variable: "STRATA_RETRIES",
                    ..
                }
            ),
            "{err}"
        );
        assert!(err.to_string().contains("'lots'"));
    }

    #[test]
    fn the_retry_after_ceiling_defaults_to_five_minutes_and_reads_strata_max_retry_after() {
        let cfg = Config::from_lookup(lookup(&[("PALETTE_API_KEY", "k")])).unwrap();
        assert_eq!(cfg.max_retry_after, Duration::from_secs(300));
        let cfg = Config::from_lookup(lookup(&[
            ("PALETTE_API_KEY", "k"),
            ("STRATA_MAX_RETRY_AFTER", "900"),
        ]))
        .unwrap();
        assert_eq!(cfg.max_retry_after, Duration::from_secs(900));
        let err = Config::from_lookup(lookup(&[
            ("PALETTE_API_KEY", "k"),
            ("STRATA_MAX_RETRY_AFTER", "soon"),
        ]))
        .unwrap_err();
        assert!(
            matches!(
                err,
                Error::InvalidSetting {
                    variable: "STRATA_MAX_RETRY_AFTER",
                    ..
                }
            ),
            "{err}"
        );
        assert!(err.to_string().contains("'soon'"));
    }

    #[test]
    fn palette_project_names_a_project_when_no_uid_is_set() {
        let cfg = Config::from_lookup(lookup(&[
            ("PALETTE_API_KEY", "k"),
            ("PALETTE_PROJECT", "strata-test"),
        ]))
        .unwrap();
        assert_eq!(cfg.project_name.as_deref(), Some("strata-test"));
        assert_eq!(cfg.project_uid, None);
        let cfg = Config::from_lookup(lookup(&[
            ("PALETTE_API_KEY", "k"),
            ("PALETTE_PROJECT", "strata-test"),
            ("PALETTE_PROJECT_UID", "p1"),
        ]))
        .unwrap();
        assert_eq!(cfg.project_uid.as_deref(), Some("p1"));
        assert_eq!(cfg.project_name, None, "the uid wins");
    }

    #[test]
    fn dotenv_parsing() {
        let text = "# comment\nexport PALETTE_API_KEY=\"abc\"\nPALETTE_PROJECT_UID='p'\nEMPTY=\n";
        assert_eq!(
            parse_dotenv(text, "PALETTE_API_KEY").as_deref(),
            Some("abc")
        );
        assert_eq!(
            parse_dotenv(text, "PALETTE_PROJECT_UID").as_deref(),
            Some("p")
        );
        assert_eq!(parse_dotenv(text, "EMPTY"), None);
        assert_eq!(parse_dotenv(text, "MISSING"), None);
    }

    #[test]
    fn the_debug_of_a_config_hides_the_api_key() {
        let cfg = Config::new("https://api.example.com", "the-key-value-itself");
        let shown = format!("{cfg:?}");
        assert!(
            !shown.contains("the-key-value-itself"),
            "the Debug of Config shows the key: {shown}"
        );
        assert!(shown.contains("<redacted>"), "{shown}");
        // Every other setting still prints, so the Debug keeps its use.
        assert!(shown.contains("api.example.com"), "{shown}");
    }
}

#[cfg(test)]
mod home_tests {
    use super::{env_file_from, state_dir_from};
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn os(text: &str) -> Option<OsString> {
        Some(OsString::from(text))
    }

    #[test]
    fn the_state_home_decides_when_it_is_an_absolute_path() {
        assert_eq!(
            state_dir_from(os("/var/state"), os("/home/a")),
            Some(PathBuf::from("/var/state/strata"))
        );
    }

    #[test]
    fn the_home_decides_when_the_state_home_is_not_set() {
        assert_eq!(
            state_dir_from(None, os("/home/a")),
            Some(PathBuf::from("/home/a/.local/state/strata"))
        );
    }

    #[test]
    fn a_state_home_that_is_empty_or_relative_is_no_place() {
        for bad in ["", "state", "./state"] {
            assert_eq!(
                state_dir_from(os(bad), os("/home/a")),
                Some(PathBuf::from("/home/a/.local/state/strata")),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_program_that_knows_no_absolute_home_has_no_state_folder() {
        assert_eq!(state_dir_from(None, None), None);
        assert_eq!(state_dir_from(None, os("")), None);
        assert_eq!(state_dir_from(os("state"), os("home")), None);
    }

    #[test]
    fn the_key_file_is_in_a_home_that_is_an_absolute_path() {
        assert_eq!(
            env_file_from(os("/home/a")),
            Some(PathBuf::from("/home/a/.palette.env"))
        );
    }

    /// An empty or a relative `HOME` names a folder below the working folder. A checkout can put
    /// a `.palette.env` there, with an address that then gets the key of the environment.
    #[test]
    fn an_empty_or_a_relative_home_has_no_key_file() {
        assert_eq!(env_file_from(None), None);
        for bad in ["", "home", "./home", "../home"] {
            assert_eq!(env_file_from(os(bad)), None, "{bad:?}");
        }
    }
}

#[cfg(test)]
mod scope_conflict_tests {
    use super::shadowed_project_name as shadowed;

    fn s(v: &str) -> Option<String> {
        Some(v.to_string())
    }

    /// The environment gave both, so the caller named one project twice and a uid won.
    #[test]
    fn a_name_beside_a_uid_in_the_environment_is_shadowed() {
        assert_eq!(shadowed(s("uid"), s("dev"), None, None), s("dev"));
    }

    /// `~/.palette.env` is the fallback. A uid in the environment is meant to override the file,
    /// so the name in the file lost to a higher place and nothing is wrong.
    #[test]
    fn a_name_in_the_file_under_a_uid_in_the_environment_is_not_shadowed() {
        assert_eq!(shadowed(s("uid"), None, None, s("dev")), None);
    }

    /// The worst case: the caller typed a name for this command, and a uid they wrote into the
    /// file months ago decided the scope instead.
    #[test]
    fn a_name_in_the_environment_under_a_uid_in_the_file_is_shadowed() {
        assert_eq!(shadowed(None, s("dev"), s("uid"), None), s("dev"));
    }

    /// One place gave both, so the file names one project twice.
    #[test]
    fn a_name_and_a_uid_in_the_file_alone_are_shadowed() {
        assert_eq!(shadowed(None, None, s("uid"), s("dev")), s("dev"));
    }

    /// No uid decides anything, so the name decides and shadows nothing.
    #[test]
    fn a_name_with_no_uid_anywhere_is_not_shadowed() {
        assert_eq!(shadowed(None, s("dev"), None, None), None);
        assert_eq!(shadowed(None, None, None, s("dev")), None);
    }

    /// An empty value is tenant scope asked for on purpose, and not a name to compare.
    #[test]
    fn an_empty_value_is_never_shadowed_and_never_shadows() {
        assert_eq!(shadowed(s("uid"), s(""), None, None), None);
        assert_eq!(shadowed(s(""), s("dev"), None, None), None);
        assert_eq!(shadowed(None, s("dev"), s(""), None), None);
    }
}
