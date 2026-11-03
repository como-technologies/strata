//! The latest release of each repository that Palette builds from.
//!
//! A new release of the Palette SDK or of the Terraform provider can change what Palette does.
//! The run reads the latest release of each repository from GitHub, and compares its tag with the
//! tag that the last run saw. A state file keeps that tag between two runs.
//!
//! The run orders no version. The latest release is the one that GitHub names, and a tag such as
//! `v4.10.b` is no Semantic Version. A tag is new when it is not the tag of the last run.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use strata::json::opt_text;

use super::document::Release;

/// The repositories whose releases the run reads, as `owner/name`.
pub const REPOSITORIES: [&str; 2] = [
    "spectrocloud/palette-sdk-go",
    "spectrocloud/terraform-provider-spectrocloud",
];

/// The address of the GitHub API.
pub const GITHUB: &str = "https://api.github.com";

/// The schema id of the state file.
pub(super) const STATE_SCHEMA: &str = "strata-contract-state/1";

/// How long a request to GitHub can take.
const TIMEOUT: Duration = Duration::from_secs(30);

/// What the run keeps between two runs: the tag that it saw for each repository.
#[derive(Default, Serialize, Deserialize)]
struct State {
    schema: String,
    releases: BTreeMap<String, String>,
}

impl State {
    /// The state in `file`; none for a file that does not exist yet.
    fn load(file: &Path) -> Result<State, String> {
        let text = match std::fs::read_to_string(file) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(State::default()),
            Err(e) => return Err(format!("{}: {e}", file.display())),
        };
        let state: State =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))?;
        if state.schema == STATE_SCHEMA {
            Ok(state)
        } else {
            Err(format!(
                "{}: the schema is `{}`, and it must be `{STATE_SCHEMA}`",
                file.display(),
                state.schema
            ))
        }
    }

    fn save(&self, file: &Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(file, text + "\n").map_err(|e| format!("{}: {e}", file.display()))
    }
}

/// Read the latest release of each repository from the GitHub API at `base`, and compare each tag
/// with the state in `state_file`. Keep the tags that the function read in the state file.
///
/// # Errors
///
/// A message when the function cannot read or write the state file. A release that the function
/// cannot read is an entry with an `error`, and the state keeps its old tag.
pub fn read(base: &str, state_file: &Path) -> Result<Vec<Release>, String> {
    let mut state = State::load(state_file)?;
    let releases: Vec<Release> = REPOSITORIES
        .iter()
        .map(|repo| release(base, repo, state.releases.get(*repo).cloned()))
        .collect();
    state.schema = STATE_SCHEMA.to_string();
    for release in &releases {
        if let Some(tag) = &release.tag {
            state.releases.insert(release.repo.clone(), tag.clone());
        }
    }
    state.save(state_file)?;
    Ok(releases)
}

fn release(base: &str, repo: &str, seen: Option<String>) -> Release {
    let mut release = Release {
        repo: repo.to_string(),
        tag: None,
        published_at: None,
        seen,
        new: false,
        error: None,
    };
    match latest(base, repo) {
        Ok(document) => {
            release.tag = opt_text(&document, "/tag_name");
            release.published_at = opt_text(&document, "/published_at");
            match &release.tag {
                Some(tag) => release.new = release.seen.as_ref().is_some_and(|seen| seen != tag),
                None => release.error = Some(format!("GET {repo}: the release has no tag_name")),
            }
        }
        Err(e) => release.error = Some(e),
    }
    release
}

/// The document of the latest release of `repo`.
///
/// The request carries no token. The organization can require SAML single sign-on, and a token that
/// it has not granted gets HTTP 403 for a public repository. A request without a token gets the
/// release.
fn latest(base: &str, repo: &str) -> Result<Value, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(TIMEOUT))
        .build()
        .into();
    let url = format!("{base}/repos/{repo}/releases/latest");
    let mut response = agent
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "strata-contract")
        .call()
        .map_err(|e| format!("GET {url}: {e}"))?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(format!("GET {url} answered HTTP {status}"));
    }
    response
        .body_mut()
        .read_json()
        .map_err(|e| format!("GET {url}: {e}"))
}
