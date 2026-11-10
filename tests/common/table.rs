//! The read of a markdown table, of a page of the book or of an output. The tests of the root
//! package and of `src/error/` use it, and so does the check of the facts in `xtask`.
//!
//! The tests of `src/error/` and `xtask` include this file with `#[path]`. So each test and the
//! check read a table in one way.

/// The cells of the table row `row`, from the first column, each with no blank at its ends. A `\|`
/// is a mark in the text of a cell, as in `--host github\|gitlab`, and not the end of the cell.
pub fn cells(row: &str) -> Vec<&str> {
    let row = row.trim();
    let row = row.strip_prefix('|').unwrap_or(row);
    let mut cells = Vec::new();
    let mut start = 0;
    for (end, _) in row.match_indices('|') {
        if !row[..end].ends_with('\\') {
            cells.push(row[start..end].trim());
            start = end + 1;
        }
    }
    // the text after the last `|` is the last cell of a row that does not end with `|`, and a row
    // has one cell at least
    let rest = row[start..].trim();
    if !rest.is_empty() || cells.is_empty() {
        cells.push(rest);
    }
    cells
}

/// The cells of each row of the first table of `text` whose head starts with `head`, such as
/// `| Kind |`. A `head` of `|` gives the first table of `text`. The rows come in the order of the
/// page, without the head and the line under it. The table ends at the first line that does not
/// start with `|`.
///
/// Panics when `text` has no such table, so that a test does not pass on a table that it did not
/// find.
pub fn table_rows(text: &str, head: &str) -> Vec<Vec<String>> {
    let mut lines = text.lines().skip_while(|line| !line.starts_with(head));
    assert!(
        lines.next().is_some(),
        "the text has no table whose head starts with {head:?}"
    );
    lines
        .skip(1) // the line under the head
        .take_while(|line| line.starts_with('|'))
        .map(|row| cells(row).into_iter().map(str::to_string).collect())
        .collect()
}

/// The first cell of each row of the table of [`table_rows`], without its backticks. A text that
/// the table holds two times is two entries.
pub fn first_cells(text: &str, head: &str) -> Vec<String> {
    table_rows(text, head)
        .into_iter()
        .map(|row| row[0].trim_matches('`').to_string())
        .collect()
}
