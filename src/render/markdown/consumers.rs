//! The markdown of `consumers`: the heading, one table of the consumers, one table of what strata
//! searched, and the scope.

use crate::model::profile_type::ProfileType;
use crate::ops::{ConsumerKind, ConsumersReport};
use crate::render::consumers::{
    NOT_LISTED, consumer_project, consumer_runs, consumers_heading, not_checked_line, scope_line,
    searched_rows,
};
use crate::render::{capitalized, entry_error};
use crate::text::{code_cell, dash, markdown_cell};

/// The report of `consumers` as markdown. The table of the consumers is not there when strata
/// found none.
pub(super) fn markdown_consumers(r: &ConsumersReport) -> String {
    let mut o = format!(
        "### consumers: {}\n\n",
        markdown_cell(&consumers_heading(r))
    );
    if !r.consumers.is_empty() {
        o.push_str(&consumers_table(r));
        o.push('\n');
    }
    o.push_str("| searched | found |\n|---|---|\n");
    for (what, found) in searched_rows(r) {
        o.push_str(&format!(
            "| {} | {} |\n",
            what.as_str(),
            markdown_cell(&found)
        ));
    }
    if let Some(scope) = scope_line(r) {
        o.push_str(&format!("\n{}.\n", capitalized(&markdown_cell(&scope))));
    }
    if let Some(unread) = not_checked_line(r) {
        o.push_str(&format!("\n**{}.**\n", markdown_cell(&unread)));
    }
    o
}

/// The table of the consumers: one row for each, with the version that its copy names.
fn consumers_table(r: &ConsumersReport) -> String {
    let subject = if r.profile.kind == Some(ProfileType::App) {
        ConsumerKind::App.noun()
    } else {
        ConsumerKind::Cluster.noun()
    };
    let mut o = format!("| {subject} | uid | project | state | runs |\n|---|---|---|---|---|\n");
    for c in &r.consumers {
        // a cluster template keeps no copy of the version, so its cell names the kind alone
        let version = if c.kind == ConsumerKind::Template {
            format!("({})", c.kind.noun())
        } else if let Some(runs) = consumer_runs(c, r, code_cell) {
            runs
        } else if c.version.is_empty() {
            String::new()
        } else {
            code_cell(&c.version)
        };
        // an empty version means two things: the cluster named none, or strata could not read
        // the cluster at all. `-` states the first, so the error takes the place of the dash
        let error =
            entry_error(c.error.as_ref()).map(|msg| format!("not read: {}", markdown_cell(msg)));
        let mut runs = match (version.is_empty(), error) {
            (true, None) => "-".to_string(),
            (true, Some(error)) => error,
            (false, None) => version,
            (false, Some(error)) => format!("{version} ({error})"),
        };
        if c.unlisted {
            runs.push_str(&format!(" ({NOT_LISTED})"));
        }
        if matches!(c.kind, ConsumerKind::EdgeHost | ConsumerKind::App) {
            runs.push_str(&format!(" ({})", c.kind.noun()));
        }
        o.push_str(&format!(
            "| {} | {} | {} | {} | {runs} |\n",
            code_cell(dash(&c.name)),
            code_cell(&c.uid),
            code_cell(consumer_project(c)),
            markdown_cell(dash(&c.state))
        ));
    }
    o
}
