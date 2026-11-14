//! The read of the code spans of a markdown text, as the markdown of GitHub reads them. The tests
//! of the root package use it, and so does the check of the facts in `xtask`.
//!
//! `xtask`, the unit tests of `src/text.rs`, and the report tests of `strata-preflight` include
//! this file with `#[path]`. So each test and the check read a code span in one way, and a change
//! of the rule changes this file only.

/// The parts of `text`, split at the fences of its code spans. A part with an odd index is a code
/// span, and a part with an even index is outside the code spans.
///
/// A run of backticks opens a span, and the next run of the same length closes it. A run that no
/// run closes is text. So a span that holds a backtick has a longer fence, as `text::code_span`
/// writes it.
fn parts(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut from, mut at) = (0, 0);
    while let Some(i) = text[at..].find('`') {
        let open = at + i;
        let run = backtick_run(&text[open..]);
        let inside = open + run;
        match closing_run(&text[inside..], run) {
            Some(end) => {
                parts.push(&text[from..open]);
                parts.push(&text[inside..inside + end]);
                from = inside + end + run;
                at = from;
            }
            None => at = inside,
        }
    }
    parts.push(&text[from..]);
    parts
}

/// The number of backticks at the start of `text`.
fn backtick_run(text: &str) -> usize {
    text.len() - text.trim_start_matches('`').len()
}

/// The position in `text` of its first run of exactly `n` backticks.
fn closing_run(text: &str, n: usize) -> Option<usize> {
    let mut at = 0;
    while let Some(i) = text[at..].find('`') {
        let run = backtick_run(&text[at + i..]);
        if run == n {
            return Some(at + i);
        }
        at += i + run;
    }
    None
}

/// The code spans of `text`, in their order, without their fences, as GitHub shows them.
pub fn code_spans(text: &str) -> Vec<&str> {
    parts(text)
        .into_iter()
        .skip(1)
        .step_by(2)
        .map(without_pad)
        .collect()
}

/// `span` without one space at each end, when it has a space at each end and another character.
/// GitHub removes these two spaces, and `text::code_span` writes them next to a backtick.
fn without_pad(span: &str) -> &str {
    match span.strip_prefix(' ').and_then(|s| s.strip_suffix(' ')) {
        Some(inner) if !span.trim_matches(' ').is_empty() => inner,
        _ => span,
    }
}

/// `text` without its code spans: the parts outside them, joined with a space.
pub fn without_code(text: &str) -> String {
    parts(text)
        .into_iter()
        .step_by(2)
        .collect::<Vec<_>>()
        .join(" ")
}
