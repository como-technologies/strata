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
