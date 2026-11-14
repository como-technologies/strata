//! A text of an answer with tags of HTML, a text of an answer with the marks of markdown, and the
//! checks that a markdown output shows each such tag and each such mark as text.
//!
//! `tests/common/mod.rs` re-exports this file for the tests. The unit tests of `src/text.rs` and
//! of the report of `strata-preflight` include it with `#[path]`. So one text of each kind and one
//! list of the tags of strata exist.

/// The body of an answer with a link and a tag of HTML, as a message of Palette can hold them.
pub const ANSWER_TAGS: &str = "<a href=\"https://example.com\">x</a> <Code>AccessDenied</Code>";

/// The body of an answer with the marks of markdown: a link, an emphasis, and two backticks
/// around a tag. GitHub reads each one as markdown, when no `\` comes before its marks.
pub const ANSWER_MARKS: &str = "[x](https://example.com) *a* `<Code>`";

/// [`ANSWER_MARKS`] as GitHub must get it to show the text of the answer: a `\` before each mark,
/// and each `<` and `>` as an entity. Two backticks with a `\` open no code span, so the entities
/// show as `<` and `>`.
const ANSWER_MARKS_AS_TEXT: &str = r"\[x\](https://example.com) \*a\* \`&lt;Code&gt;\`";

/// Make sure that `markdown` shows [`ANSWER_MARKS`] as text, with no link, no emphasis, and no
/// code span from it.
pub fn assert_answer_marks_as_text(markdown: &str) {
    assert!(
        markdown.contains(ANSWER_MARKS_AS_TEXT),
        "the marks of the answer as text in\n{markdown}"
    );
}

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
