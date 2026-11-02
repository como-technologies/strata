//! The document that the contract run writes: `strata-contract/1`.

use serde::Serialize;

/// The schema id of the document.
pub const SCHEMA: &str = "strata-contract/1";

/// What the run found for one fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    /// The answer of the tenant has the status, the types, and the values of the stand-in.
    Same,
    /// The answer of the tenant differs from the stand-in: the entry lists each difference.
    Differs,
    /// The run sent no request for the fact, or the tenant has no object to ask about. The entry
    /// gives the reason.
    Skipped,
    /// The run could not check the fact: a request failed, or the tenant gave no answer.
    Error,
}

/// What differs between an answer of the tenant and the stand-in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DifferenceKind {
    /// The tenant answers with a status that is not 2xx.
    Status,
    /// A field has a type that no document of the stand-in has there.
    Type,
    /// A field that each document of the stand-in has is missing from each answer.
    Missing,
    /// A field holds a value that neither the fact nor the stand-in names.
    Value,
}

/// One difference between an answer of the tenant and the stand-in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Difference {
    /// What differs.
    pub kind: DifferenceKind,
    /// The field, as the fact writes it, or `status` for a status.
    pub path: String,
    /// What the stand-in has: the types, or the values, or the status.
    pub expected: Vec<String>,
    /// What the tenant has, in the same form.
    pub found: Vec<String>,
}

/// The entry of one fact.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// The id of the fact.
    pub id: String,
    /// The request of the fact, as the list writes it.
    pub request: String,
    /// What the run found.
    pub outcome: Outcome,
    /// Why the run skipped the fact, or could not check it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Each difference, when the outcome is `differs`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub differences: Vec<Difference>,
    /// The live checks of the fact, which prove it again.
    pub live: Vec<String>,
    /// The pages of the book that state the fact.
    pub pages: Vec<String>,
}

/// The count of each outcome.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Summary {
    /// The facts with the outcome `same`.
    pub same: usize,
    /// The facts with the outcome `differs`.
    pub differs: usize,
    /// The facts with the outcome `skipped`.
    pub skipped: usize,
    /// The facts with the outcome `error`.
    pub error: usize,
}

impl Summary {
    /// The count of the entries `entries`.
    pub fn of(entries: &[Entry]) -> Summary {
        let count = |outcome| entries.iter().filter(|e| e.outcome == outcome).count();
        Summary {
            same: count(Outcome::Same),
            differs: count(Outcome::Differs),
            skipped: count(Outcome::Skipped),
            error: count(Outcome::Error),
        }
    }
}

/// The latest release of a repository that Palette is built from, and whether it is new.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    /// The repository, as `owner/name`.
    pub repo: String,
    /// The tag of the latest release; none when the run could not read it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// The time of the publish of the release, in RFC 3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    /// The tag that the last run saw; none for the first run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seen: Option<String>,
    /// True when the tag is not the tag that the last run saw, and a last run saw one.
    pub new: bool,
    /// Why the run could not read the release.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The document of one run.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    /// Always [`SCHEMA`].
    pub schema: &'static str,
    /// The time of the run, in RFC 3339.
    pub checked_at: String,
    /// The address of the tenant that the run asked.
    pub target: String,
    /// The count of each outcome.
    pub summary: Summary,
    /// One entry for each fact of the kind `request`, in the order of the list.
    pub facts: Vec<Entry>,
    /// The latest release of each repository that the run reads; empty when it keeps no state.
    pub releases: Vec<Release>,
}

impl Contract {
    /// The exit code of the run: 0 when each fact the run checked is the same, 1 when a fact
    /// differs, and 2 when the run could not check a fact or a release.
    pub fn exit_code(&self) -> u8 {
        if self.summary.error > 0 || self.releases.iter().any(|r| r.error.is_some()) {
            2
        } else {
            u8::from(self.summary.differs > 0)
        }
    }
}
