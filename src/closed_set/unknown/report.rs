//! The report of the unknown values: the document `strata-report/1`.
//!
//! A report is the lines of the log, each different line once, with its count. An entry holds the
//! [`Sighting`] of its line and the field `count`, and no other field. The report holds no `schema`
//! for each entry, no `tool` block, and no time, because the log has none.
//!
//! A report states that this release of strata is out of date: Palette gave a value that the
//! release does not know. It is never a report of a fault of Palette, or of how a team uses
//! Palette.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::log::Line;
use super::sighting::Sighting;

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
    /// The value, and the version of strata and the route that read it.
    #[serde(flatten)]
    pub sighting: Sighting,
    /// How many lines of the log are this entry.
    pub count: usize,
}

impl Report {
    /// The report of `lines`: each different line once, with its count. The entries are in the
    /// order of the set, the value, the route, and the version of strata.
    pub fn of<'a>(lines: impl IntoIterator<Item = &'a Line>) -> Report {
        let mut sorted: Vec<&Sighting> = lines.into_iter().map(|line| &line.sighting).collect();
        sorted.sort_by(|a, b| a.order(b));
        let mut unknown: Vec<Entry> = Vec::new();
        for sighting in sorted {
            match unknown.last_mut() {
                Some(last) if last.sighting == *sighting => last.count += 1,
                _ => unknown.push(Entry {
                    sighting: sighting.clone(),
                    count: 1,
                }),
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
            .map(|e| (&e.sighting.set, &e.sighting.value))
            .collect::<BTreeSet<_>>()
            .len()
    }
}

impl Entry {
    /// What is wrong with the entry, or `None`. The sighting is valid, and the count is from 1 to
    /// [`MOST_COUNT`].
    fn fault(&self) -> Option<String> {
        self.sighting
            .fault()
            .or_else(|| (self.count == 0).then(|| "the count is 0".to_string()))
            .or_else(|| {
                (self.count > MOST_COUNT).then(|| format!("the count is more than {MOST_COUNT}"))
            })
    }
}
