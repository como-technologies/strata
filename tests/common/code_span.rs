//! The read of the code spans of a markdown text: the text between a pair of backticks. The tests
//! of the root package use it, and so does the check of the facts in `xtask`.
//!
//! `xtask` includes this file with `#[path]`. So each test and the check read a code span in one
//! way, and a change of the rule changes this file only.

/// The parts of `text` between its backticks. A part with an odd index is a code span, and a part
/// with an even index is outside the code spans. A backtick with no pair starts a code span that
/// continues to the end of `text`.
fn parts(text: &str) -> std::str::Split<'_, char> {
    text.split('`')
}

/// The code spans of `text`, in their order, without their backticks.
pub fn code_spans(text: &str) -> Vec<&str> {
    parts(text).skip(1).step_by(2).collect()
}

/// `text` without its code spans: the parts outside them, joined with a space.
pub fn without_code(text: &str) -> String {
    parts(text).step_by(2).collect::<Vec<_>>().join(" ")
}
