//! A text of an answer with tags of HTML, and the checks that a markdown output shows each such
//! tag as text.
//!
//! `tests/common/mod.rs` re-exports this file for the tests. The unit tests of `src/text.rs` and
//! of the report of `strata-preflight` include it with `#[path]`. So one text of an answer and one
//! list of the tags of strata exist.

/// The body of an answer with a link and a tag of HTML, as a message of Palette can hold them.
pub const ANSWER_TAGS: &str = "<a href=\"https://example.com\">x</a> <Code>AccessDenied</Code>";

/// The tags of HTML that the markdown of strata writes itself.
const STRATA_TAGS: [&str; 7] = [
    "<br>",
    "<details>",
    "</details>",
    "<summary>",
    "</summary>",
    "<code>",
    "</code>",
];

/// Make sure that each tag of HTML in `markdown` is a tag that strata writes. So a text that
/// strata quotes from an answer shows its tags as text.
pub fn assert_only_strata_tags(markdown: &str) {
    let rest = STRATA_TAGS
        .iter()
        .fold(markdown.to_string(), |text, tag| text.replace(tag, ""));
    assert_eq!(rest.find('<'), None, "a tag of an answer in\n{markdown}");
}

/// Make sure that `markdown` shows [`ANSWER_TAGS`] as text, with no tag of HTML from it.
pub fn assert_answer_tags_as_text(markdown: &str) {
    assert!(
        markdown.contains(&strata::text::html_text(ANSWER_TAGS)),
        "the answer as text in\n{markdown}"
    );
    assert_only_strata_tags(markdown);
}

/// The lines of `markdown` outside its fenced code blocks, as the markdown of GitHub reads them.
///
/// A line of three or more backticks opens a block. The next line that holds only backticks, at
/// least as many, closes it. Up to three spaces can come before each fence. A block that no line
/// closes continues to the end.
pub fn outside_code_blocks(markdown: &str) -> String {
    let mut open: Option<usize> = None;
    let mut outside = String::new();
    for line in markdown.lines() {
        let rest = line.trim_start_matches(' ');
        let fence = rest.len() - rest.trim_start_matches('`').len();
        let is_fence = line.len() - rest.len() <= 3 && fence >= 3;
        match open {
            None if is_fence && !rest[fence..].contains('`') => open = Some(fence),
            None => {
                outside.push_str(line);
                outside.push('\n');
            }
            Some(n) if is_fence && fence >= n && rest[fence..].trim().is_empty() => open = None,
            Some(_) => {}
        }
    }
    outside
}
