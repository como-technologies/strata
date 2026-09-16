//! Where Palette is and how to authenticate. Read from the environment, with `~/.palette.env` as
//! the fallback, or built directly by a library user.

use std::fs;
use std::path::Path;
use std::time::Duration;

use crate::error::{Error, Result};

/// The default Palette API base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.spectrocloud.com";

const KEY_VARS: [&str; 2] = ["PALETTE_API_KEY", "SPECTROCLOUD_APIKEY"];
const URL_VARS: [&str; 2] = ["PALETTE_API", "SPECTROCLOUD_HOST"];
const PROJECT_VARS: [&str; 1] = ["PALETTE_PROJECT_UID"];
const PROJECT_NAME_VAR: &str = "PALETTE_PROJECT";
const RETRIES_VAR: &str = "STRATA_RETRIES";
const DEFAULT_RETRIES: u32 = 3;
const MAX_RETRY_AFTER_VAR: &str = "STRATA_MAX_RETRY_AFTER";
const DEFAULT_MAX_RETRY_AFTER: Duration = Duration::from_secs(300);

/// Connection settings for one Palette tenant or project.
#[derive(Debug, Clone)]
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
    /// Per-request timeout.
    pub timeout: Duration,
    /// Retries after a failed attempt on a transient error; `STRATA_RETRIES`, default 3.
    pub retries: u32,
    /// The longest `Retry-After` strata waits for; a longer one fails the request at once.
    /// `STRATA_MAX_RETRY_AFTER` in seconds, default five minutes.
    pub max_retry_after: Duration,
}

impl Config {
    /// Build from a base URL and key; the project is unset and the timeout is two minutes.
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Config {
            api_key: api_key.into(),
            base_url: normalize_url(base_url.into()),
            project_uid: None,
            project_name: None,
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

    /// Read `PALETTE_API_KEY` (or `SPECTROCLOUD_APIKEY`), `PALETTE_API` (or `SPECTROCLOUD_HOST`),
    /// `PALETTE_PROJECT_UID` (or the name `PALETTE_PROJECT`), `STRATA_RETRIES`, and
    /// `STRATA_MAX_RETRY_AFTER` from the environment, then from `~/.palette.env`.
    pub fn from_env() -> Result<Self> {
        Self::from_lookup(|key| env_nonempty(key).or_else(|| dotenv_lookup(key)))
    }

    /// Like [`Config::from_env`] but with a caller-supplied lookup; this is what tests use.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let first = |names: &[&str]| names.iter().find_map(|n| lookup(n));
        let api_key = first(&KEY_VARS).ok_or(Error::MissingCredential {
            primary: KEY_VARS[0],
            alternate: KEY_VARS[1],
        })?;
        let base_url = first(&URL_VARS).unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let mut cfg = Config::new(base_url, api_key);
        cfg.project_uid = first(&PROJECT_VARS);
        if cfg.project_uid.is_none() {
            cfg.project_name = lookup(PROJECT_NAME_VAR);
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

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

/// `KEY=VALUE` lines from `~/.palette.env`, with optional `export ` and quotes.
fn dotenv_lookup(key: &str) -> Option<String> {
    let home = std::env::var_os("HOME")?;
    let text = fs::read_to_string(Path::new(&home).join(".palette.env")).ok()?;
    parse_dotenv(&text, key)
}

fn parse_dotenv(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == key {
                let v = v.trim().trim_matches('"').trim_matches('\'');
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let m: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| m.get(k).cloned()
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
}
