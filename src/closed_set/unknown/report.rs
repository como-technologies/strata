//! The report of the unknown values: the document `strata-report/1`.
//!
//! A report is the lines of the log, each different line once, with its count. An entry holds the
//! fields of [`FIELDS`](super::log::FIELDS) and no other, except `count`. The report holds no
//! `schema` for each entry, no `tool` block, and no time, because the log has none.
//!
//! A report states that this release of strata is out of date: Palette gave a value that the
//! release does not know. It is never a report of a fault of Palette, or of how a team uses
//! Palette.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::log::{Line, route_fault, value_fault};
use crate::reference::version;

/// The schema id of a report.
pub const SCHEMA: &str = "strata-report/1";

/// The most entries that the endpoint takes in one report.
pub const MOST_ENTRIES: usize = 100;

/// The most different values that strata puts in one report. The endpoint opens this many new
/// issues in an hour, so a report of this size fits an hour that no other sender used.
pub const MOST_VALUES: usize = 20;

/// The most lines that one entry counts. The check refuses a larger count, so a sender cannot set
/// any count in an issue.
pub const MOST_COUNT: usize = 1_000_000;

/// The most characters of the version of strata, of the Palette release, of the name of a set, and
/// of an error code.
const LONGEST_FIELD: usize = 64;

/// The most characters of the template of a route.
const LONGEST_ROUTE: usize = 200;

/// The document `strata-report/1`: the values that Palette gave and that this release of strata
/// does not know.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Report {
    /// The schema id of this document
    #[schemars(extend("const" = SCHEMA))]
    pub schema: String,
    /// One entry for each different line of the log, in the order of the set, the value, and the
    /// route.
    pub unknown: Vec<Entry>,
}

/// One line of the log, and how many times the log holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    /// The version of strata that read the value.
    pub strata: String,
    /// The Palette release, when strata knew it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette: Option<String>,
    /// The name of the closed set, as `strata schema` names its definition.
    pub set: String,
    /// The text of the value.
    pub value: String,
    /// The template of the route that gave the value, such as `/v1/clusterprofiles/{uid}`.
    pub route: String,
    /// The status of a refusal of Palette. A value of a document has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    /// The error code of a refusal of Palette. A value of a document has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// How many lines of the log are this entry.
    pub count: usize,
}

impl Report {
    /// The report of `lines`: each different line once, with its count. The entries are in the
    /// order of the set, the value, and the route.
    pub fn of<'a>(lines: impl IntoIterator<Item = &'a Line>) -> Report {
        let mut sorted: Vec<&Line> = lines.into_iter().collect();
        sorted.sort_by(|a, b| order(a, b));
        let mut unknown: Vec<Entry> = Vec::new();
        for line in sorted {
            match unknown.last_mut() {
                Some(last) if last.is(line) => last.count += 1,
                _ => unknown.push(Entry::of(line)),
            }
        }
        Report {
            schema: SCHEMA.to_string(),
            unknown,
        }
    }

    /// What is wrong with a report that arrives from outside, or `None`.
    ///
    /// The type refuses a field that the entry does not have. This check refuses the rest: another
    /// schema, no entry or more than [`MOST_ENTRIES`], and an entry with a field that is not a
    /// short word or with a count above [`MOST_COUNT`]. A report that strata makes from its own
    /// log passes.
    pub fn fault(&self) -> Option<String> {
        if self.schema != SCHEMA {
            return Some(format!("the schema is not {SCHEMA}"));
        }
        if self.unknown.is_empty() || self.unknown.len() > MOST_ENTRIES {
            return Some(format!(
                "the report has {} entries, and a report has 1 to {MOST_ENTRIES}",
                self.unknown.len()
            ));
        }
        self.unknown.iter().enumerate().find_map(|(at, entry)| {
            entry
                .fault()
                .map(|fault| format!("entry {}: {fault}", at + 1))
        })
    }

    /// The number of different values: a value of a set counts once, also when it has more than
    /// one entry.
    pub(crate) fn distinct_values(&self) -> usize {
        self.unknown
            .iter()
            .map(|e| (&e.set, &e.value))
            .collect::<BTreeSet<_>>()
            .len()
    }
}

impl Entry {
    /// The entry for one `line`.
    fn of(line: &Line) -> Entry {
        Entry {
            strata: line.strata.clone(),
            palette: line.palette.clone(),
            set: line.set.clone(),
            value: line.value.clone(),
            route: line.route.clone(),
            status: line.status,
            error_code: line.error_code.clone(),
            count: 1,
        }
    }

    /// What is wrong with the entry, or `None`. Each field is a short word, and the route is the
    /// template of a route.
    fn fault(&self) -> Option<String> {
        word_fault("strata", &self.strata, is_version_char)
            .or_else(|| {
                let palette = self.palette.as_deref()?;
                word_fault("palette", palette, is_version_char)
            })
            .or_else(|| word_fault("set", &self.set, |c| c.is_ascii_alphanumeric()))
            .or_else(|| value_fault(&self.value))
            .or_else(|| route_fault(&self.route))
            .or_else(|| {
                (self.route.chars().count() > LONGEST_ROUTE)
                    .then(|| format!("the route has more than {LONGEST_ROUTE} characters"))
            })
            .or_else(|| self.refusal_fault())
            .or_else(|| (self.count == 0).then(|| "the count is 0".to_string()))
            .or_else(|| {
                (self.count > MOST_COUNT).then(|| format!("the count is more than {MOST_COUNT}"))
            })
    }

    /// What is wrong with the status and the error code, or `None`.
    fn refusal_fault(&self) -> Option<String> {
        if self
            .status
            .is_some_and(|status| !(100..600).contains(&status))
        {
            return Some("the status is not an HTTP status".to_string());
        }
        let code = self.error_code.as_deref()?;
        (code.chars().count() > LONGEST_FIELD || code.chars().any(char::is_control)).then(|| {
            format!("the error code is not a short word of {LONGEST_FIELD} characters or fewer")
        })
    }

    /// True when `line` has the fields of this entry.
    fn is(&self, line: &Line) -> bool {
        self.strata == line.strata
            && self.palette == line.palette
            && self.set == line.set
            && self.value == line.value
            && self.route == line.route
            && self.status == line.status
            && self.error_code == line.error_code
    }
}

/// What is wrong with the text `text` of the field `name`, or `None`. A field of this kind is not
/// empty, has [`LONGEST_FIELD`] characters or fewer, and holds only characters that `allowed` names.
fn word_fault(name: &str, text: &str, allowed: fn(char) -> bool) -> Option<String> {
    (text.is_empty() || text.chars().count() > LONGEST_FIELD || !text.chars().all(allowed))
        .then(|| format!("the {name} is not a short word of {LONGEST_FIELD} characters or fewer"))
}

/// True for a character of a version: a letter, a digit, or one of `.`, `-`, `+`, and `_`.
fn is_version_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+' | '_')
}

/// The order of two lines: the set, the value, the route, the refusal, the Palette release, and
/// the version of strata. Two lines that are equal are next to each other.
fn order(a: &Line, b: &Line) -> Ordering {
    (&a.set, &a.value, &a.route, a.status, &a.error_code)
        .cmp(&(&b.set, &b.value, &b.route, b.status, &b.error_code))
        .then_with(|| match (&a.palette, &b.palette) {
            (Some(x), Some(y)) => version::compare(x, y),
            (x, y) => x.is_some().cmp(&y.is_some()),
        })
        .then_with(|| version::compare(&a.strata, &b.strata))
        // two texts can have one version order, such as `1.0.1+2` and `1.0.1`: the text decides
        // that tie only, so that equal lines stay next to each other
        .then_with(|| (&a.palette, &a.strata).cmp(&(&b.palette, &b.strata)))
}
