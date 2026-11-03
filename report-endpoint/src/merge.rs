//! The merge of a report into the issues of the tracker.
//!
//! A set and a value have one issue. Its first line is a marker,
//! `<!-- strata-report: values <set> <value> -->`. The second line holds the state of the issue.
//! That is the count of the lines, the count of the reports, and the releases and routes that the
//! reports named. The state is the hex of a JSON document, so no text of a report can end the
//! comment. The visible table of the issue shows the same state.
//!
//! A report that names a value again changes the body of its issue. It opens no issue and writes no
//! comment. An issue that a maintainer closed gets the new count too, and stays closed: users of an
//! older release send the value until they update.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use strata::BOOK_URL;
use strata::closed_set::unknown::report::Report;
use strata::reference::version;
use strata::text::{code_cell, code_span, query_value};

use crate::tracker::{Issue, Tracker, TrackerError};

/// The most releases and routes that one issue lists. More are dropped, so that a sender
/// cannot make the body of an issue grow.
pub const MOST_LISTED: usize = 20;

/// The start of the first line of an issue of the endpoint.
const MARKER: &str = "<!-- strata-report: values ";

/// The start of the second line of an issue of the endpoint.
const STATE: &str = "<!-- strata-report-state: ";

/// What a merge did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Outcome {
    /// The issues that the merge opened.
    pub opened: usize,
    /// The issues that the merge changed.
    pub updated: usize,
}

/// What one report says about one set and one value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Group {
    set: String,
    value: String,
    state: State,
    /// The numbers of the entries of the report that this group sums, from 1.
    entries: Vec<usize>,
}

/// The state of an issue: what the reports said, summed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct State {
    lines: u64,
    reports: u64,
    strata: BTreeSet<String>,
    routes: BTreeSet<String>,
}

impl State {
    /// Add `other` to the state. A list holds [`MOST_LISTED`] items at most: the list of releases
    /// keeps the newest, and the list of routes keeps the first.
    fn add(&mut self, other: &State) {
        self.lines = self.lines.saturating_add(other.lines);
        self.reports = self.reports.saturating_add(other.reports);
        for item in &other.routes {
            if self.routes.len() < MOST_LISTED {
                self.routes.insert(item.clone());
            }
        }
        for item in &other.strata {
            add_release(&mut self.strata, item);
        }
    }
}

/// Add the release `item` to `releases`. When the list is full, the oldest release leaves if
/// `item` is newer, so the list keeps the releases that matter most for the maintainers.
fn add_release(releases: &mut BTreeSet<String>, item: &str) {
    if releases.contains(item) {
        return;
    }
    if releases.len() >= MOST_LISTED {
        let oldest = releases
            .iter()
            .min_by(|a, b| version::compare(a, b))
            .cloned();
        match oldest {
            Some(oldest) if version::compare(item, &oldest) == Ordering::Greater => {
                releases.remove(&oldest);
            }
            _ => return,
        }
    }
    releases.insert(item.to_string());
}

/// One group for each different set and value of `report`, in the order of the set and the value.
fn groups(report: &Report) -> Vec<Group> {
    let mut found: BTreeMap<(&str, &str), Group> = BTreeMap::new();
    for (at, entry) in report.unknown.iter().enumerate() {
        let group = found
            .entry((&entry.sighting.set, &entry.sighting.value))
            .or_insert_with(|| Group {
                set: entry.sighting.set.clone(),
                value: entry.sighting.value.clone(),
                state: State {
                    reports: 1,
                    ..State::default()
                },
                entries: Vec::new(),
            });
        group.entries.push(at + 1);
        let mut one = State {
            lines: entry.count as u64,
            ..State::default()
        };
        one.strata.insert(entry.sighting.strata.clone());
        one.routes.insert(entry.sighting.route.clone());
        group.state.add(&one);
    }
    found.into_values().collect()
}

/// The marker of the issue for the set `set` and the value `value`.
fn marker(set: &str, value: &str) -> String {
    format!("{MARKER}{} {} -->", query_value(set), query_value(value))
}

/// The items of `items` as code spans, in version order, or `none` when there is no item.
fn cell(items: &BTreeSet<String>, by_version: bool) -> String {
    let mut sorted: Vec<&String> = items.iter().collect();
    if by_version {
        sorted.sort_by(|a, b| version::compare(a, b));
    }
    if sorted.is_empty() {
        return "none".to_string();
    }
    sorted
        .iter()
        .map(|item| code_cell(item))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The title of the issue for the set `set` and the value `value`.
fn title(set: &str, value: &str) -> String {
    format!(
        "A user of strata reports the {} value {} that strata does not know",
        code_span(set),
        code_span(value)
    )
}

/// The body of the issue for the set `set` and the value `value`, with the state `state`.
fn body(set: &str, value: &str, state: &State) -> String {
    // the state is plain data, so it always serializes; an empty text only drops the state
    let json = serde_json::to_vec(state).unwrap_or_default();
    let page = format!("{BOOK_URL}development/facts.html#what-to-do-with-a-new-value");
    format!(
        "{marker}\n{STATE}{state_hex} -->\n\
         Users of strata sent {reports} that Palette gave the {set} value {value}. \
         Their release of strata does not know the value.\n\
         \n\
         | | |\n\
         | --- | --- |\n\
         | Lines | {lines} |\n\
         | Reports | {reports_count} |\n\
         | Releases of strata | {strata} |\n\
         | Routes | {routes} |\n\
         \n\
         A report states that strata is out of date. It is never a report of a fault of Palette. \
         A report holds no name, no uid, and no host.\n\
         \n\
         Prove the value in the lab. Then add the value to the closed set of strata, or write why not. \
         [What to do with a new value]({page}) says how.\n\
         \n\
         The endpoint writes the table above again with each report. Write your notes in a comment.\n",
        marker = marker(set, value),
        state_hex = hex::encode(json),
        reports = if state.reports == 1 {
            "1 report".to_string()
        } else {
            format!("{} reports", state.reports)
        },
        set = code_span(set),
        value = code_span(value),
        lines = state.lines,
        reports_count = state.reports,
        strata = cell(&state.strata, true),
        routes = cell(&state.routes, false),
    )
}

/// The state that the body of an issue holds. A body with no state, or with a state that does not
/// read, has the empty state: the count then starts again.
fn read_state(body: &str) -> State {
    body.lines()
        .nth(1)
        .and_then(|line| line.strip_prefix(STATE))
        .and_then(|rest| rest.strip_suffix(" -->"))
        .and_then(|text| hex::decode(text).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// What a merge does, found with the reads alone. The caller can refuse the merge before it
/// writes anything.
#[derive(Debug)]
pub struct Plan {
    steps: Vec<(Group, Option<Issue>)>,
}

impl Plan {
    /// How many issues the merge opens.
    pub fn new_issues(&self) -> usize {
        self.steps
            .iter()
            .filter(|(_, issue)| issue.is_none())
            .count()
    }

    /// How many issues the merge will change.
    pub fn updates(&self) -> usize {
        self.steps.len() - self.new_issues()
    }

    /// True when the plan has no step.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Keep the steps that `fits` takes, in the order of the plan, and drop the others. `fits` gets
    /// true for a step that opens an issue, and false for a step that changes one.
    ///
    /// Gives the numbers of the entries of the dropped steps, from 1, in the order of the report.
    /// They name the values that wait.
    pub fn keep(&mut self, mut fits: impl FnMut(bool) -> bool) -> Vec<usize> {
        let mut waiting = Vec::new();
        self.steps.retain(|(group, issue)| {
            let keep = fits(issue.is_none());
            if !keep {
                waiting.extend(&group.entries);
            }
            keep
        });
        waiting.sort_unstable();
        waiting
    }
}

/// Find the issue of each set and value of `report`. Nothing is written.
///
/// # Errors
///
/// [`TrackerError`] when the tracker did not list the issues.
pub fn plan(tracker: &mut impl Tracker, report: &Report) -> Result<Plan, TrackerError> {
    let issues = tracker.issues()?;
    let steps = groups(report)
        .into_iter()
        .map(|group| {
            let wanted = marker(&group.set, &group.value);
            let issue = issues
                .iter()
                .filter(|issue| {
                    issue
                        .body
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .trim_end_matches('\r')
                        == wanted
                })
                .min_by_key(|issue| issue.number)
                .cloned();
            (group, issue)
        })
        .collect();
    Ok(Plan { steps })
}

/// Write the plan: open the issue of each new value, and add the report to each issue that exists.
///
/// # Errors
///
/// [`TrackerError`] when the tracker did not open or change an issue. The issues written before
/// the error stay written.
pub fn apply(tracker: &mut impl Tracker, plan: Plan) -> Result<Outcome, TrackerError> {
    let mut outcome = Outcome::default();
    for (group, issue) in plan.steps {
        match issue {
            Some(issue) => {
                let mut state = read_state(&issue.body);
                state.add(&group.state);
                tracker.update(issue.number, &body(&group.set, &group.value, &state))?;
                outcome.updated += 1;
            }
            None => {
                let text = body(&group.set, &group.value, &group.state);
                tracker.open(&title(&group.set, &group.value), &text)?;
                outcome.opened += 1;
            }
        }
    }
    Ok(outcome)
}

/// Merge `report` into the issues: [`plan`], then [`apply`].
///
/// # Errors
///
/// The errors of [`plan`] and [`apply`].
pub fn merge(tracker: &mut impl Tracker, report: &Report) -> Result<Outcome, TrackerError> {
    let plan = plan(tracker, report)?;
    apply(tracker, plan)
}

/// A tracker in memory, for the tests of the merge and of the service.
#[cfg(test)]
pub(crate) mod memory {
    use super::*;

    /// The issues in memory: the title, the body, and whether they are open.
    #[derive(Debug, Default)]
    pub struct Memory {
        pub issues: Vec<(String, String, bool)>,
    }

    impl Tracker for Memory {
        fn issues(&mut self) -> Result<Vec<Issue>, TrackerError> {
            Ok(self
                .issues
                .iter()
                .enumerate()
                .map(|(at, (_, body, _))| Issue {
                    number: at as u64 + 1,
                    body: body.clone(),
                })
                .collect())
        }

        fn open(&mut self, title: &str, body: &str) -> Result<u64, TrackerError> {
            self.issues
                .push((title.to_string(), body.to_string(), true));
            Ok(self.issues.len() as u64)
        }

        fn update(&mut self, number: u64, body: &str) -> Result<(), TrackerError> {
            self.issues[number as usize - 1].1 = body.to_string();
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
