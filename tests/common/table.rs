//! The read of a table of a page of the book, for the tests of the book and of `src/error/`.
//!
//! The tests of `src/error/` include this file with `#[path]`. So the test of the codes and the
//! test of the kinds in `reference/errors.md` read a table in one way.

/// The first cell of each row of the first table of `text` whose head starts with `head`, such as
/// `| Kind |`. The cells come in the order of the page, without their backticks. A text that the
/// table holds two times is two entries.
pub fn first_cells(text: &str, head: &str) -> Vec<String> {
    text.lines()
        .skip_while(|line| !line.starts_with(head))
        .skip(2) // the head and the line under it
        .take_while(|line| line.starts_with('|'))
        .map(|row| {
            let cell = row.split('|').nth(1).expect("a row has a first cell");
            cell.trim().trim_matches('`').to_string()
        })
        .collect()
}
