//! The lines of `report`: the heading, and the facts of an entry.

use super::count_of;
use super::markdown::code;
use crate::closed_set::unknown::Report;
use crate::closed_set::unknown::report::{Entry, SCHEMA};
use crate::text::{dash, markdown_cell};

/// `Palette gave 2 values that this release of strata does not know`, or `Palette gave no values
/// that this release of strata does not know` for an empty report.
pub(crate) fn report_heading(r: &Report) -> String {
    format!(
        "Palette gave {} that this release of strata does not know",
        count_of(r.distinct_values(), "value")
    )
}

/// `strata 0.26.0, Palette 4.8.12, status 404, error code SomeCode`: where an entry comes from.
/// The Palette release, the status, and the error code are there only when the entry has them.
pub(crate) fn entry_origin(e: &Entry) -> String {
    let mut parts = vec![format!("strata {}", e.strata)];
    if let Some(release) = &e.palette {
        parts.push(format!("Palette {release}"));
    }
    if let Some(status) = e.status {
        parts.push(format!("status {status}"));
    }
    if let Some(code) = &e.error_code {
        parts.push(format!("error code {code}"));
    }
    parts.join(", ")
}

/// The report as markdown: the heading as a sentence, and a table with a row for each entry. It
/// serves as the body of an issue, so it holds no heading of its own.
pub(super) fn markdown_report(r: &Report) -> String {
    let mut o = format!("{}.\n", report_heading(r));
    if r.unknown.is_empty() {
        return o;
    }
    o.push_str(&format!(
        "\nThe table is the output of `strata report`, the document `{SCHEMA}`.\n\n"
    ));
    o.push_str("| set | value | route | count | strata | Palette | status | error code |\n");
    o.push_str("| --- | --- | --- | --- | --- | --- | --- | --- |\n");
    for e in &r.unknown {
        let status = e.status.map(|s| s.to_string()).unwrap_or_default();
        o.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
            code(&markdown_cell(&e.set)),
            code(&markdown_cell(&e.value)),
            code(&markdown_cell(&e.route)),
            e.count,
            markdown_cell(&e.strata),
            markdown_cell(dash(e.palette.as_deref().unwrap_or_default())),
            dash(&status),
            markdown_cell(dash(e.error_code.as_deref().unwrap_or_default())),
        ));
    }
    o
}
