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
use serde::Serialize;

use super::log::Line;
use crate::reference::version;

/// The schema id of a report.
pub const SCHEMA: &str = "strata-report/1";

/// The document `strata-report/1`: the values that Palette gave and that this release of strata
/// does not know.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// The schema id of this document
    #[schemars(extend("const" = SCHEMA))]
    pub schema: String,
    /// One entry for each different line of the log, in the order of the set, the value, and the
    /// route.
    pub unknown: Vec<Entry>,
}

/// One line of the log, and how many times the log holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// The version of strata that read the value.
    pub strata: String,
    /// The Palette release, when strata knew it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub palette: Option<String>,
    /// The name of the closed set, as `strata schema` names its definition.
    pub set: String,
    /// The text of the value.
    pub value: String,
    /// The template of the route that gave the value, such as `/v1/clusterprofiles/{uid}`.
    pub route: String,
    /// The status of a refusal of Palette. A value of a document has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    /// The error code of a refusal of Palette. A value of a document has none.
    #[serde(skip_serializing_if = "Option::is_none")]
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
