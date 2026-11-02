//! The issue tracker that the endpoint writes to.
//!
//! The merge needs three things from a tracker: list the issues of the label, open an issue, and
//! change the body of an issue. [`Tracker`] names them. [`GitHub`] does them on GitHub, and the
//! tests of the merge use a stand-in.

use std::fmt;
use std::time::Duration;

/// An issue of the tracker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// The number of the issue.
    pub number: u64,
    /// The body of the issue.
    pub body: String,
}

/// The tracker did not do what the endpoint asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackerError {
    /// What went wrong. It never holds the token.
    pub message: String,
}

impl fmt::Display for TrackerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TrackerError {}

/// What the merge needs from an issue tracker.
pub trait Tracker {
    /// The issues that carry the label of the endpoint, open and closed. A pull request is no issue.
    ///
    /// # Errors
    ///
    /// [`TrackerError`] when the tracker did not answer.
    fn issues(&mut self) -> Result<Vec<Issue>, TrackerError>;

    /// Open an issue with the title `title` and the body `body`. It gives the number.
    ///
    /// # Errors
    ///
    /// [`TrackerError`] when the tracker did not open the issue.
    fn open(&mut self, title: &str, body: &str) -> Result<u64, TrackerError>;

    /// Replace the body of the issue `number`. The call neither opens a closed issue nor writes a
    /// comment, so it sends no notification.
    ///
    /// # Errors
    ///
    /// [`TrackerError`] when the tracker did not change the issue.
    fn update(&mut self, number: u64, body: &str) -> Result<(), TrackerError>;
}

/// The time that one call to GitHub takes at most.
const TIMEOUT: Duration = Duration::from_secs(15);

/// The issues that one call lists: GitHub gives 100 at most.
const PAGE: usize = 100;

/// The most pages that [`GitHub::issues`] reads, so a repository with very many issues of the label
/// does not make one request read for long.
const MOST_PAGES: usize = 10;

/// The tracker of a repository on GitHub, with a token that can write issues.
pub struct GitHub {
    api: String,
    repository: String,
    token: String,
    label: String,
    milestone: Option<String>,
    milestone_number: Option<Option<u64>>,
    agent: ureq::Agent,
}

// Written by hand: a derived `Debug` would print the token.
impl fmt::Debug for GitHub {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GitHub")
            .field("api", &self.api)
            .field("repository", &self.repository)
            .field("token", &"<redacted>")
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

impl GitHub {
    /// The tracker of `repository`, as `owner/name`, at the API address `api`. Each issue it
    /// opens has the label `label` and, when `milestone` names an open milestone, that milestone.
    pub fn new(
        api: &str,
        repository: &str,
        token: &str,
        label: &str,
        milestone: Option<&str>,
    ) -> GitHub {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(TIMEOUT))
            .build()
            .into();
        GitHub {
            api: api.trim_end_matches('/').to_string(),
            repository: repository.to_string(),
            token: token.to_string(),
            label: label.to_string(),
            milestone: milestone.map(str::to_string),
            milestone_number: None,
            agent,
        }
    }

    /// The address of `path` under the repository.
    fn url(&self, path: &str) -> String {
        format!("{}/repos/{}{path}", self.api, self.repository)
    }

    /// The headers of each call: the token, and the version of the API.
    fn headers<B>(&self, request: ureq::RequestBuilder<B>) -> ureq::RequestBuilder<B> {
        request
            .header("Authorization", &format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "strata-report-endpoint")
    }

    /// The JSON of the answer, or the error that the status stands for.
    fn read(
        what: &str,
        answer: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    ) -> Result<serde_json::Value, TrackerError> {
        let fail = |message: String| TrackerError {
            message: format!("{what}: {message}"),
        };
        let mut answer = answer.map_err(|e| fail(e.to_string()))?;
        let status = answer.status().as_u16();
        let text = answer
            .body_mut()
            .read_to_string()
            .map_err(|e| fail(e.to_string()))?;
        if !(200..300).contains(&status) {
            let start: String = text.chars().take(200).collect();
            return Err(fail(format!("GitHub answered HTTP {status}: {start}")));
        }
        serde_json::from_str(&text).map_err(|e| fail(e.to_string()))
    }

    /// The number of the milestone `title` among the open milestones, when there is one. The
    /// answer is kept, so a service asks one time.
    fn milestone_number(&mut self) -> Result<Option<u64>, TrackerError> {
        if let Some(known) = self.milestone_number {
            return Ok(known);
        }
        let Some(title) = self.milestone.clone() else {
            return Ok(None);
        };
        let url = self.url("/milestones?state=open&per_page=100");
        let list = Self::read(
            "list the milestones",
            self.headers(self.agent.get(&url)).call(),
        )?;
        let number = list.as_array().into_iter().flatten().find_map(|item| {
            (item["title"] == title.as_str())
                .then(|| item["number"].as_u64())
                .flatten()
        });
        self.milestone_number = Some(number);
        Ok(number)
    }
}

impl Tracker for GitHub {
    fn issues(&mut self) -> Result<Vec<Issue>, TrackerError> {
        let mut found = Vec::new();
        for page in 1..=MOST_PAGES {
            let url = self.url(&format!(
                "/issues?labels={}&state=all&per_page={PAGE}&page={page}",
                strata::text::query_value(&self.label)
            ));
            let list = Self::read("list the issues", self.headers(self.agent.get(&url)).call())?;
            let items = list.as_array().map_or(&[][..], Vec::as_slice);
            for item in items {
                // a pull request is an issue to the API, and it is not ours
                if item.get("pull_request").is_some() {
                    continue;
                }
                if let Some(number) = item["number"].as_u64() {
                    found.push(Issue {
                        number,
                        body: item["body"].as_str().unwrap_or_default().to_string(),
                    });
                }
            }
            if items.len() < PAGE {
                return Ok(found);
            }
        }
        // the last page was full: an issue of a value can be beyond it, and a merge that does not
        // find it would open a second issue
        Err(TrackerError {
            message: format!(
                "list the issues: more than {} issues have the label {}",
                MOST_PAGES * PAGE,
                self.label
            ),
        })
    }

    fn open(&mut self, title: &str, body: &str) -> Result<u64, TrackerError> {
        let mut issue = serde_json::json!({"title": title, "body": body, "labels": [self.label]});
        if let Some(number) = self.milestone_number()? {
            issue["milestone"] = number.into();
        }
        let url = self.url("/issues");
        let made = Self::read(
            "open an issue",
            self.headers(self.agent.post(&url)).send_json(&issue),
        )?;
        made["number"].as_u64().ok_or_else(|| TrackerError {
            message: "open an issue: the answer has no number".to_string(),
        })
    }

    fn update(&mut self, number: u64, body: &str) -> Result<(), TrackerError> {
        let url = self.url(&format!("/issues/{number}"));
        Self::read(
            "change an issue",
            self.headers(self.agent.patch(&url))
                .send_json(serde_json::json!({"body": body})),
        )
        .map(|_| ())
    }
}
