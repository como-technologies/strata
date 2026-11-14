//! A text of an answer with tags of HTML, one with the marks of markdown, and one with mentions of
//! GitHub. The checks that a markdown output shows each such tag, mark, and mention as text.
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

/// The body of an answer with a mention of a person and of a team, as GitHub reads them. Its
/// address of mail and its pack ref hold a `@` that starts no mention.
pub const ANSWER_MENTIONS: &str = "@octocat and @acme/ops, ops@example.com, k@1.0";

/// [`ANSWER_MENTIONS`] as GitHub must get it to notify nobody: the entity of a zero width space
/// after the `@` of each mention. The other two `@` stay as they are.
const ANSWER_MENTIONS_AS_TEXT: &str =
    "@&#8203;octocat and @&#8203;acme/ops, ops@example.com, k@1.0";

/// Make sure that `markdown` shows [`ANSWER_MENTIONS`] as text, and holds no mention of GitHub.
///
/// GitHub reads a mention at a `@` with a letter or a digit of ASCII after it. The `@` is the
/// first character, or the character before it is no letter, no digit of ASCII, and no `_`.
pub fn assert_answer_mentions_as_text(markdown: &str) {
    assert!(
        markdown.contains(ANSWER_MENTIONS_AS_TEXT),
        "the mentions of the answer as text in\n{markdown}"
    );
    let chars: Vec<char> = markdown.chars().collect();
    let word = |i: Option<usize>| {
        i.and_then(|i| chars.get(i))
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
    };
    let mention = (0..chars.len()).find(|&i| {
        chars[i] == '@'
            && chars.get(i + 1).is_some_and(char::is_ascii_alphanumeric)
            && !word(i.checked_sub(1))
    });
    assert_eq!(mention, None, "a mention of GitHub in\n{markdown}");
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
