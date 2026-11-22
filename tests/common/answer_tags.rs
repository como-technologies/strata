//! A text of an answer with tags of HTML, one with the marks of markdown, and one with mentions
//! of GitHub. Also a text with references to issues of GitHub, one with bare addresses, one with
//! an address at the end of a line, one with control characters, and one with a mark of
//! direction. The checks that a markdown output shows each such tag, mark, mention, and reference
//! as text, and each such address as it is. The checks that a terminal output and a markdown
//! output show each such control character as a visible escape.
//!
//! `tests/common/mod.rs` re-exports this file for the tests. The unit tests of `src/text.rs`, and
//! of the report and the run of `strata-preflight`, include it with `#[path]`. So one text of each
//! kind and one list of the tags of strata exist.

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

/// The body of an answer with three references to issues of GitHub. One names an issue of the
/// same repository, one an issue of another repository, and one has the form `GH-`.
pub const ANSWER_ISSUES: &str = "see #12, octo-org/octo-repo#34 and GH-56";

/// [`ANSWER_ISSUES`] as GitHub must get it to link to no issue. The entity of a zero width space
/// comes after the `#` and after the `GH-` of each reference.
const ANSWER_ISSUES_AS_TEXT: &str = "see #&#8203;12, octo-org/octo-repo#&#8203;34 and GH-&#8203;56";

/// Make sure that `markdown` shows [`ANSWER_ISSUES`] as text, and holds no reference to an issue
/// of GitHub.
///
/// GitHub reads a reference at a `#` with a digit of ASCII after it. It also reads one at a `GH-`
/// in each case, with a digit after it and no letter or digit of ASCII before it. The `#` of an
/// entity, such as `&#8203;`, starts no reference.
pub fn assert_answer_issues_as_text(markdown: &str) {
    assert!(
        markdown.contains(ANSWER_ISSUES_AS_TEXT),
        "the references of the answer as text in\n{markdown}"
    );
    let lower = markdown.to_ascii_lowercase();
    let digit_after = |i: usize| lower.as_bytes().get(i + 1).is_some_and(u8::is_ascii_digit);
    let reference = lower.char_indices().find(|&(i, c)| match c {
        '#' => digit_after(i) && !lower[..i].ends_with('&'),
        '-' => {
            digit_after(i)
                && lower[..i].strip_suffix("gh").is_some_and(|before| {
                    !before
                        .chars()
                        .next_back()
                        .is_some_and(|c| c.is_ascii_alphanumeric())
                })
        }
        _ => false,
    });
    assert_eq!(reference, None, "a reference to an issue in\n{markdown}");
}

/// The body of an answer with three bare addresses that GitHub links. One has a query of two
/// values, one has `~`, `*`, and `_`, and one has `/@name`. A `,`, a `)`, and a `.` end them, as
/// in a sentence.
pub const ANSWER_ADDRESSES: &str = "see https://example.com/?a=1&b=2, \
     https://example.com/~a/*b*/c_d and (https://example.com/@octocat).";

/// Make sure that `markdown` holds [`ANSWER_ADDRESSES`] as it is. So the link that GitHub makes
/// of each address holds no escape, and each address keeps its target.
pub fn assert_answer_addresses_as_they_are(markdown: &str) {
    assert!(
        markdown.contains(ANSWER_ADDRESSES),
        "the addresses of the answer as they are in\n{markdown}"
    );
}

/// The message of an answer with a bare address at the end of its first line. GitHub ends the
/// link of a bare address only at a space or a `<`.
pub const ANSWER_ADDRESS_LINES: &str = "see https://example.com/a\nnext";

/// Make sure that `markdown` holds [`ANSWER_ADDRESS_LINES`] in a line or in a cell of a table. The
/// address stays as it is, and [`strata::text::ADDRESS_END`] comes before the escape of the line
/// break. So the link that GitHub makes of the address holds only the address.
pub fn assert_answer_address_lines_as_markdown(markdown: &str) {
    let lines = ANSWER_ADDRESS_LINES.replace('\n', &format!(r"{}\\n", strata::text::ADDRESS_END));
    assert!(
        markdown.contains(&lines),
        "the address of the answer and the escape after it in\n{markdown}"
    );
}

/// The message of an answer with an escape sequence of the terminal, a carriage return, and a line
/// break. A decoded message of Palette can hold them. On a terminal, the escape sequence makes the
/// next text red, and the carriage return writes over the start of the line. The line break starts
/// a new line.
pub const ANSWER_CONTROLS: &str = "\u{1b}[31mred\u{1b}[0m over\rback\nnext";

/// [`ANSWER_CONTROLS`] as a terminal must get it: each control character as a visible escape. A
/// code span and a block of HTML of markdown hold the same text.
const ANSWER_CONTROLS_AS_TEXT: &str = r"\u{1b}[31mred\u{1b}[0m over\rback\nnext";

/// [`ANSWER_CONTROLS_AS_TEXT`] as a line of markdown must get it: a `\` before each `\` and each
/// `[`. GitHub shows the text of the terminal.
const ANSWER_CONTROLS_AS_MARKDOWN: &str = r"\\u{1b}\[31mred\\u{1b}\[0m over\\rback\\nnext";

/// Make sure that `terminal`, a text with no color, shows [`ANSWER_CONTROLS`] as text. Its only
/// control characters are the line breaks between its lines.
pub fn assert_answer_controls_as_text(terminal: &str) {
    assert!(
        terminal.contains(ANSWER_CONTROLS_AS_TEXT),
        "the control characters of the answer as text in\n{terminal:?}"
    );
    assert_only_line_breaks(terminal);
}

/// Make sure that `markdown` shows [`ANSWER_CONTROLS`] as text in a line or in a cell of a table.
/// Its only control characters are the line breaks between its lines.
pub fn assert_answer_controls_as_markdown(markdown: &str) {
    assert!(
        markdown.contains(ANSWER_CONTROLS_AS_MARKDOWN),
        "the control characters of the answer as text in\n{markdown:?}"
    );
    assert_only_line_breaks(markdown);
}

/// The name of an object with a mark of the direction of a text, U+202E. A terminal that reads the
/// mark shows the characters after it from right to left, so `nimda` reads as `admin`. U+202C
/// ends the mark.
pub const ANSWER_DIRECTION: &str = "user\u{202e}nimda\u{202c} ok";

/// [`ANSWER_DIRECTION`] as a terminal must get it: each mark as a visible escape.
pub const ANSWER_DIRECTION_AS_TEXT: &str = r"user\u{202e}nimda\u{202c} ok";

/// Make sure that the only control characters of `text` are its line breaks, and that it holds no
/// mark of the direction of a text.
fn assert_only_line_breaks(text: &str) {
    let control = text.chars().find(|c| c.is_control() && *c != '\n');
    assert_eq!(control, None, "a control character in\n{text:?}");
    let mark = text
        .chars()
        .find(|c| strata::text::DIRECTION_MARKS.contains(c));
    assert_eq!(mark, None, "a mark of direction in\n{text:?}");
}

/// The tags of HTML that the markdown of strata writes itself.
const STRATA_TAGS: [&str; 8] = [
    "<br>",
    "<details>",
    "</details>",
    "<summary>",
    "</summary>",
    "<code>",
    "</code>",
    strata::text::ADDRESS_END,
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
