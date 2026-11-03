//! The settings of the service: environment variables, as Cloud Run sets them.
//!
//! The token comes from the secret of the project. Cloud Run puts it in `GITHUB_TOKEN` when the
//! service starts. The token is never in the repository, in GitHub, or in a log.

use std::fmt;

/// The variable that holds the token of GitHub. It is required.
pub const TOKEN_VAR: &str = "GITHUB_TOKEN";

/// The repository that gets the issues, when `GITHUB_REPOSITORY` is not set: the repository that the
/// manifest of this crate names, as `owner/name`. So the address of the repository has one text.
fn default_repository() -> String {
    let url = env!("CARGO_PKG_REPOSITORY");
    url.strip_prefix("https://github.com/")
        .unwrap_or(url)
        .to_string()
}

/// The address of the API of GitHub, when `GITHUB_API` is not set.
const DEFAULT_API: &str = "https://api.github.com";

/// The label of each issue, and the only label that the service lists.
///
/// The service reads the issues of this label, open and closed, up to a limit. No other writer uses
/// the label, so the issues of the weekly contract run, which has the label `palette-drift`, do not
/// spend that limit.
pub const DEFAULT_LABEL: &str = "unknown-value";

/// The milestone of a new issue, when the repository has it open. It is the milestone that the
/// weekly contract run uses, and a test makes sure that the two agree.
pub const DEFAULT_MILESTONE: &str = "Backlog";

/// The port that Cloud Run gives to the service.
const DEFAULT_PORT: u16 = 8080;

/// What the service needs to start.
#[derive(Clone)]
pub struct Settings {
    /// The port to listen on: `PORT`, or 8080.
    pub port: u16,
    /// The token that writes issues: `GITHUB_TOKEN`.
    pub token: String,
    /// The repository as `owner/name`: `GITHUB_REPOSITORY`.
    pub repository: String,
    /// The address of the API: `GITHUB_API`. A test points it at a stand-in.
    pub api: String,
    /// The label of each issue: `REPORT_LABEL`.
    pub label: String,
    /// The milestone of a new issue: `REPORT_MILESTONE`. An empty value is no milestone.
    pub milestone: Option<String>,
    /// True when the service reads the sender from `X-Forwarded-For`: `TRUST_FORWARDED_FOR=1`.
    /// Only a service behind the front end of Cloud Run sets it. This is because a client can send
    /// the header to a service that is not behind a proxy.
    pub trust_forwarded: bool,
}

// Written by hand: a derived `Debug` prints the token.
impl fmt::Debug for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Settings")
            .field("port", &self.port)
            .field("token", &"<redacted>")
            .field("repository", &self.repository)
            .field("api", &self.api)
            .field("label", &self.label)
            .field("milestone", &self.milestone)
            .field("trust_forwarded", &self.trust_forwarded)
            .finish()
    }
}

impl Settings {
    /// The settings from `lookup`, which gives the value of a variable by its name. An empty value
    /// is no value.
    ///
    /// # Errors
    ///
    /// A message when `GITHUB_TOKEN` is not set, or `PORT` is not a port.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Settings, String> {
        let get = |name: &str| lookup(name).filter(|value| !value.is_empty());
        let token = get(TOKEN_VAR).ok_or_else(|| format!("{TOKEN_VAR} is required"))?;
        let port = match get("PORT") {
            Some(text) => text
                .parse()
                .map_err(|_| format!("PORT: expected a port number, got '{text}'"))?,
            None => DEFAULT_PORT,
        };
        let milestone = match lookup("REPORT_MILESTONE") {
            Some(text) => Some(text).filter(|text| !text.is_empty()),
            None => Some(DEFAULT_MILESTONE.to_string()),
        };
        Ok(Settings {
            port,
            token,
            repository: get("GITHUB_REPOSITORY").unwrap_or_else(default_repository),
            api: get("GITHUB_API").unwrap_or_else(|| DEFAULT_API.into()),
            label: get("REPORT_LABEL").unwrap_or_else(|| DEFAULT_LABEL.into()),
            milestone,
            trust_forwarded: get("TRUST_FORWARDED_FOR").is_some_and(|text| text == "1"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        }
    }

    #[test]
    fn the_token_is_required_and_everything_else_has_a_default() {
        assert_eq!(
            Settings::from_lookup(lookup(&[])).unwrap_err(),
            "GITHUB_TOKEN is required"
        );
        assert!(Settings::from_lookup(lookup(&[("GITHUB_TOKEN", "")])).is_err());
        let settings = Settings::from_lookup(lookup(&[("GITHUB_TOKEN", "t")])).unwrap();
        assert_eq!(settings.port, 8080);
        assert_eq!(settings.repository, "como-technologies/strata");
        assert_eq!(settings.api, "https://api.github.com");
        assert_eq!(settings.label, DEFAULT_LABEL);
        assert_eq!(settings.milestone.as_deref(), Some(DEFAULT_MILESTONE));
        assert!(
            !settings.trust_forwarded,
            "the header is not trusted by default"
        );
    }

    #[test]
    fn a_variable_replaces_its_default_and_an_empty_milestone_is_no_milestone() {
        let settings = Settings::from_lookup(lookup(&[
            ("GITHUB_TOKEN", "t"),
            ("PORT", "9000"),
            ("GITHUB_REPOSITORY", "o/r"),
            ("GITHUB_API", "http://127.0.0.1:1"),
            ("REPORT_LABEL", "l"),
            ("REPORT_MILESTONE", ""),
            ("TRUST_FORWARDED_FOR", "1"),
        ]))
        .unwrap();
        assert_eq!(settings.port, 9000);
        assert_eq!(settings.repository, "o/r");
        assert_eq!(settings.api, "http://127.0.0.1:1");
        assert_eq!(settings.label, "l");
        assert_eq!(settings.milestone, None);
        assert!(settings.trust_forwarded);
        let error = Settings::from_lookup(lookup(&[("GITHUB_TOKEN", "t"), ("PORT", "http")]));
        assert_eq!(
            error.unwrap_err(),
            "PORT: expected a port number, got 'http'"
        );
    }

    #[test]
    fn the_debug_text_of_the_settings_holds_no_token() {
        let settings = Settings::from_lookup(lookup(&[("GITHUB_TOKEN", "ghp_secret")])).unwrap();
        assert!(!format!("{settings:?}").contains("ghp_secret"));
    }
}
