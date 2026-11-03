//! The lines of `report`: the heading, and the facts of an entry.

use super::count_of;
use crate::closed_set::unknown::report::{Report, SCHEMA};
use crate::text::{code_span, dash, markdown_cell};

/// For example, the heading is `Palette gave 2 values that this release of strata does not know`.
/// For an empty report, it is `Palette gave no values that this release of strata does not know`.
pub(crate) fn report_heading(r: &Report) -> String {
    format!(
        "Palette gave {} that this release of strata does not know",
        count_of(r.distinct_values(), "value")
    )
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
    o.push_str("| set | value | route | count | strata |\n");
    o.push_str("| --- | --- | --- | --- | --- |\n");
    for e in &r.unknown {
        let s = &e.sighting;
        o.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            code_span(&s.set),
            code_span(&s.value),
            code_span(&s.route),
            e.count,
            markdown_cell(dash(&s.strata)),
        ));
    }
    o
}
