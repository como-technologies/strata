//! The texts that a message builds:
//!
//! - A phrase from a list of names, and the name of an object.
//! - A long text cut to a length.
//! - A mark for an empty text, a text of HTML, a text in a cell of a markdown table, a text in a
//!   code span, and a text in a fenced code block.
//! - A command as a person types it in a shell.
//! - A JSON Schema as text, with its descriptions in plain words.
//!
//! `strata-preflight` calls the public ones, so both tools write each of these in one way.

use schemars::Schema;
use serde_json::Value;

/// The name of an object, or its uid when Palette gives no name.
pub fn name_or_uid<'a>(name: &'a str, uid: &'a str) -> &'a str {
    if name.is_empty() { uid } else { name }
}

/// `one` when `n` is 1, else `many`: the noun that agrees with a count, such as `cluster` or
/// `clusters`.
///
/// It also picks a verb, an ending, or a pair of words, such as `"was"` and `"were"`. Each message
/// of strata, and each report of `strata-preflight`, that agrees a word with a count calls it.
pub fn plural<T>(n: usize, one: T, many: T) -> T {
    if n == 1 { one } else { many }
}

/// `items` as one phrase: `a`, `a and b`, or `a, b, and c`. Empty text for no item.
///
/// Each message of strata that names a list of things builds it here, so each one puts the comma
/// before `and` in the same way.
pub(crate) fn and_list<S: AsRef<str>>(items: &[S]) -> String {
    match items {
        [] => String::new(),
        [one] => one.as_ref().to_string(),
        [a, b] => format!("{} and {}", a.as_ref(), b.as_ref()),
        [rest @ .., last] => {
            let rest: Vec<&str> = rest.iter().map(AsRef::as_ref).collect();
            format!("{}, and {}", rest.join(", "), last.as_ref())
        }
    }
}

/// `s`, cut to at most `n` characters: its first `n - 1` characters and `…` when it is longer.
///
/// Each message and each column of a report that shows the start of a long text cuts it here.
/// So each one marks the cut in the same way. An `n` of zero gives empty text. A text that must stay
/// in ASCII, such as a value of the log, calls `cut_with` with another mark.
pub fn cut(s: &str, n: usize) -> String {
    cut_with(s, n, "…")
}

/// `s`, cut to at most `n` characters, with `mark` as the last characters when `s` is longer.
///
/// [`cut`] uses `…`. A text that must stay in ASCII, such as a value of the log, passes `...`. An
/// `n` that is smaller than the mark gives empty text.
pub(crate) fn cut_with(s: &str, n: usize, mark: &str) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let Some(kept) = n.checked_sub(mark.chars().count()) else {
        return String::new();
    };
    let mut part: String = s.chars().take(kept).collect();
    part.push_str(mark);
    part
}

/// `text` with each rustdoc link written as a plain code span: ``[`Snapshot`]`` is `` `Snapshot` ``.
///
/// A doc comment links a type for the reader of rustdoc. A reader of a schema sees the markup of
/// the link and cannot follow it.
fn plain_doc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("[`") {
        // `span` starts at the opening backtick of the code span
        let span = &rest[start + 1..];
        let Some(end) = span[1..].find("`]") else {
            break;
        };
        out.push_str(&rest[..start]);
        out.push_str(&span[..end + 2]);
        rest = &span[end + 3..];
    }
    out.push_str(rest);
    out
}

/// Write each `description` of the schema `value` as [`plain_doc`] writes a doc comment.
fn plain_descriptions(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                match item {
                    Value::String(text) if key == "description" => *text = plain_doc(text),
                    _ => plain_descriptions(item),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(plain_descriptions),
        _ => {}
    }
}

/// The JSON Schema `schema` as indented JSON, with no rustdoc link in a description.
///
/// A description of a schema is the doc comment of a type or a field, and a person who writes a
/// document reads it. `strata schema` and `strata-preflight --schema` print the schema here, so
/// that both show a link as a plain code span.
///
/// # Errors
///
/// A [`serde_json::Error`] when the schema cannot be written as JSON.
pub fn schema_json(schema: &Schema) -> serde_json::Result<String> {
    let mut value = schema.as_value().clone();
    plain_descriptions(&mut value);
    serde_json::to_string_pretty(&value)
}

/// `s`, or `-` when `s` is empty.
///
/// Each report that shows a field with no value shows this one mark.
pub fn dash(s: &str) -> &str {
    if s.is_empty() { "-" } else { s }
}

/// `s` as text of HTML: each `&`, `<`, and `>` is an entity. So a reader of HTML shows each tag
/// and each entity of `s` as text.
///
/// The HTML report of `strata-preflight` and [`markdown_cell`] write a text here. The markdown of
/// GitHub reads HTML in a text too.
pub fn html_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// `s` as the text of one cell of a markdown table, or of one line of markdown.
///
/// A `|` ends a cell, and a line break ends the row. So the text escapes each `|`, and writes a
/// line break as a space. It is also [`html_text`]. So a text that strata quotes from an answer,
/// such as a message of Palette, shows each tag of HTML as text. A code span shows an entity as it
/// is, so [`code_cell`] writes a value in a code span.
pub fn markdown_cell(s: &str) -> String {
    table_text(&html_text(s))
}

/// `lines` as the text of one cell of a markdown table: each line as [`markdown_cell`] writes it,
/// with `<br>` between two lines. [`dash`] when the lines give no text.
///
/// Each line gets its escape before the join, so that the `<br>` of strata stays a tag.
pub(crate) fn lines_cell<S: AsRef<str>>(lines: impl IntoIterator<Item = S>) -> String {
    let cells: Vec<String> = lines
        .into_iter()
        .map(|line| markdown_cell(line.as_ref()))
        .collect();
    dash(&cells.join("<br>")).to_string()
}

/// `s` with each `|` escaped and each line break as a space, so that it cannot end a cell or a
/// row of a markdown table.
fn table_text(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

/// `text` as a code span of markdown. The fence is longer than each run of backticks in the text,
/// so a value cannot end the span.
///
/// The span writes the text as it is. A title and a sentence need no escape for a `|`. A span in a
/// table cell needs one: see [`code_cell`]. Each issue and each report that shows a value of an
/// unknown set writes the value here, so the value has one form in both.
pub fn code_span(text: &str) -> String {
    let fence = "`".repeat(longest_backticks(text) + 1);
    let pad = if text.starts_with('`') || text.ends_with('`') {
        " "
    } else {
        ""
    };
    format!("{fence}{pad}{text}{pad}{fence}")
}

/// `text` as a fenced code block of markdown, with the info string `info`, such as `diff`.
///
/// The fence has three backticks, or more when the text holds a run of three or more. So a line of
/// the text cannot end the block. The last line of the text gets a line break when it has none.
/// Strata and `strata-preflight` write each block of a markdown output here. So a text of an
/// answer, such as the values of a pack, stays in its block.
pub fn code_block(info: &str, text: &str) -> String {
    let fence = "`".repeat((longest_backticks(text) + 1).max(3));
    let end = if text.is_empty() || text.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    format!("{fence}{info}\n{text}{end}{fence}\n")
}

/// The length of the longest run of backticks in `text`, or 0 when it holds no backtick.
fn longest_backticks(text: &str) -> usize {
    text.split(|c| c != '`').map(str::len).max().unwrap_or(0)
}

/// `text` as a code span in one cell of a markdown table: [`code_span`], with each `|` escaped and
/// each line break as a space. So a value cannot end a cell or a row of the table.
///
/// A code span shows an entity as it is, so the span gets no [`html_text`]. It shows a tag as text.
pub fn code_cell(text: &str) -> String {
    table_text(&code_span(text))
}

/// The command `argv` as a person types it in a shell. An argument that is empty, or that holds a
/// space, a quote, or another character that the shell reads, goes in single quotes.
///
/// Strata and `strata-preflight` print each command that a person can copy here, so a copy runs
/// with the arguments that the tool names.
pub fn shell_words<S: AsRef<str>>(argv: &[S]) -> String {
    argv.iter()
        .map(|a| {
            let a = a.as_ref();
            if a.is_empty()
                || a.chars()
                    .any(|c| c.is_whitespace() || "\"'$`\\|&;<>()*?[]{}".contains(c))
            {
                format!("'{}'", a.replace('\'', "'\\''"))
            } else {
                a.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `text` as a value in a query string: each byte that is not a letter, a digit, or one of
/// `-._~:` becomes `%XX`. So a time with `+00:00` keeps its `+`.
///
/// A route of Palette and the link to a new issue of strata build their queries here. The endpoint
/// of the report writes the marker of an issue with it, so that a value cannot end the comment.
pub fn query_value(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~:".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

// The text of an answer with tags of HTML, in the file that the tests of strata use. A unit test
// here uses the text only, so the checks of a markdown output are dead code here.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/common/answer_tags.rs"]
mod answer_tags;

// The reads of a code block and of a code span, in the files of the tests of strata. A unit test
// here uses one read of each file, so the other reads are dead code here.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/common/code_block.rs"]
mod block_read;
#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/common/code_span.rs"]
mod span_read;

#[cfg(test)]
mod tests {
    use super::answer_tags::ANSWER_TAGS;
    use super::*;

    #[test]
    fn a_noun_agrees_with_its_count_and_only_one_takes_the_singular() {
        assert_eq!(plural(0, "cluster", "clusters"), "clusters");
        assert_eq!(plural(1, "cluster", "clusters"), "cluster");
        assert_eq!(plural(2, "cluster", "clusters"), "clusters");
        assert_eq!(plural(1, "was", "were"), "was");
        assert_eq!(plural(3, String::from("a"), String::from("b")), "b");
    }

    #[test]
    fn an_object_shows_its_name_and_its_uid_only_when_it_has_no_name() {
        assert_eq!(name_or_uid("lab", "u1"), "lab");
        assert_eq!(name_or_uid("", "u1"), "u1");
        assert_eq!(name_or_uid("", ""), "");
    }

    #[test]
    fn a_cut_text_has_at_most_n_characters_and_ends_with_a_mark() {
        assert_eq!(cut("abc", 3), "abc");
        assert_eq!(cut("abcd", 3), "ab…");
        assert_eq!(cut("abcd", 3).chars().count(), 3);
        // a character of two bytes is one character, and no cut stops in it
        assert_eq!(cut("ééééé", 4), "ééé…");
        assert_eq!(cut("abcd", 1), "…");
        assert_eq!(cut("abcd", 0), "");
        assert_eq!(cut("", 0), "");
    }

    #[test]
    fn a_cut_text_ends_with_the_mark_that_the_caller_gives_and_has_at_most_n_characters() {
        assert_eq!(cut_with("abcdef", 5, "..."), "ab...");
        assert_eq!(cut_with("abcde", 5, "..."), "abcde");
        assert_eq!(cut_with("ééééé", 4, "..."), "é...");
        // an n that is smaller than the mark leaves no room for the text
        assert_eq!(cut_with("abcdef", 2, "..."), "");
        assert_eq!(cut_with("abcdef", 3, "..."), "...");
        assert_eq!(cut_with("abcdef", 4, "…"), cut("abcdef", 4));
    }

    #[test]
    fn a_doc_comment_shows_each_link_as_a_plain_code_span_and_keeps_the_other_text() {
        assert_eq!(
            plain_doc("The digest, as [`Snapshot::content_digest`] gives it: see [`Template`]."),
            "The digest, as `Snapshot::content_digest` gives it: see `Template`."
        );
        assert_eq!(
            plain_doc("an [array] and `code` and [`open"),
            "an [array] and `code` and [`open"
        );
        assert_eq!(plain_doc("[`a`][`b`]"), "`a``b`");
        assert_eq!(plain_doc(""), "");
    }

    #[test]
    fn a_schema_prints_each_description_without_link_markup() {
        let schema = Schema::try_from(serde_json::json!({
            "description": "A [`Type`].",
            "properties": {
                "description": {"description": "see [`Field`]", "type": "string"},
                "list": {"items": [{"description": "[`First`]"}]}
            },
            "const": "[`kept`]"
        }))
        .unwrap();
        let printed: Value = serde_json::from_str(&schema_json(&schema).unwrap()).unwrap();
        assert_eq!(printed["description"], "A `Type`.");
        assert_eq!(
            printed["properties"]["description"]["description"],
            "see `Field`"
        );
        assert_eq!(
            printed["properties"]["list"]["items"][0]["description"],
            "`First`"
        );
        assert_eq!(printed["const"], "[`kept`]", "only a description changes");
    }

    #[test]
    fn an_empty_text_shows_a_dash_and_another_text_shows_as_it_is() {
        assert_eq!(dash(""), "-");
        assert_eq!(dash("lab"), "lab");
    }

    #[test]
    fn a_cell_of_a_markdown_table_holds_no_bar_and_no_line_break() {
        assert_eq!(markdown_cell("a|b"), "a\\|b");
        assert_eq!(markdown_cell("first\nsecond"), "first second");
        assert_eq!(markdown_cell("plain"), "plain");
    }

    #[test]
    fn a_text_of_html_shows_each_tag_and_each_entity_as_text() {
        assert_eq!(
            html_text(ANSWER_TAGS),
            "&lt;a href=\"https://example.com\"&gt;x&lt;/a&gt; &lt;Code&gt;AccessDenied&lt;/Code&gt;"
        );
        // an entity of the answer shows as it is, and not as the character that it names
        assert_eq!(html_text("a &lt; b & c"), "a &amp;lt; b &amp; c");
        assert_eq!(html_text("plain"), "plain");
    }

    #[test]
    fn a_cell_of_a_markdown_table_holds_no_tag_of_html_from_a_quoted_text() {
        let cell = markdown_cell(&format!("{ANSWER_TAGS}\n| next"));
        assert!(!cell.contains('<'), "{cell}");
        assert_eq!(cell, format!("{} \\| next", html_text(ANSWER_TAGS)));
    }

    #[test]
    fn a_code_span_in_a_table_cell_shows_a_tag_as_text_with_no_entity() {
        assert_eq!(code_cell("<Code>"), "`<Code>`");
        assert_eq!(code_cell("a & b|c"), "`a & b\\|c`");
    }

    #[test]
    fn a_code_span_has_a_fence_longer_than_each_run_of_backticks_in_the_text() {
        assert_eq!(code_span("later"), "`later`");
        assert_eq!(code_span("late`r"), "``late`r``");
        assert_eq!(code_span("late``r"), "```late``r```");
        // a backtick at an end needs a space, or it joins the fence
        assert_eq!(code_span("`later"), "`` `later ``");
        assert_eq!(code_span("later`"), "`` later` ``");
        assert_eq!(code_span("`"), "`` ` ``");
        assert_eq!(code_span(""), "``");
    }

    #[test]
    fn a_code_span_writes_the_text_as_it_is_for_a_title_and_a_sentence() {
        assert_eq!(code_span("a|b"), "`a|b`");
        assert_eq!(code_span("first\nsecond"), "`first\nsecond`");
    }

    #[test]
    fn a_code_block_has_a_fence_of_three_backticks_or_one_longer_than_each_run_in_the_text() {
        assert_eq!(code_block("diff", "+ a\n- b\n"), "```diff\n+ a\n- b\n```\n");
        assert_eq!(code_block("diff", "+ a``b\n"), "```diff\n+ a``b\n```\n");
        // a line of three backticks, after the space of a line of context, does not end the block
        assert_eq!(
            code_block("diff", " ```\n <Code>\n"),
            "````diff\n ```\n <Code>\n````\n"
        );
        assert_eq!(code_block("", "`````"), "``````\n`````\n``````\n");
        // the last line gets its line break, and an empty text gives an empty block
        assert_eq!(
            code_block("bash", "strata diff"),
            "```bash\nstrata diff\n```\n"
        );
        assert_eq!(code_block("diff", ""), "```diff\n```\n");
    }

    /// The tests of an output read each code span and each code block with the reads of
    /// `tests/common/`. Each read gives the text that this module wrote. So a test cannot see
    /// a part of a span or of a block as text outside it.
    #[test]
    fn the_reads_of_the_tests_give_the_text_that_each_code_span_and_each_code_block_holds() {
        for text in [
            "later", "late`r", "late``r", "`later", "later`", "`", "a ``` b",
        ] {
            assert_eq!(span_read::code_spans(&code_span(text)), [text]);
        }
        for text in ["+ a\n- b", " ```\n <Code>", "`````", "   ```"] {
            assert_eq!(
                block_read::code_blocks(&code_block("diff", text), "diff"),
                [text]
            );
        }
    }

    #[test]
    fn a_code_span_in_a_table_cell_holds_no_bar_and_no_line_break() {
        assert_eq!(code_cell("a|b"), "`a\\|b`");
        assert_eq!(code_cell("first\nsecond"), "`first second`");
        assert_eq!(code_cell("late`r|x"), "``late`r\\|x``");
        assert_eq!(code_cell("plain"), code_span("plain"));
    }

    #[test]
    fn a_list_of_names_has_a_comma_before_and_only_from_three_names() {
        assert_eq!(and_list::<&str>(&[]), "");
        assert_eq!(and_list(&["os"]), "os");
        assert_eq!(and_list(&["os", "k8s"]), "os and k8s");
        assert_eq!(and_list(&["os", "k8s", "cni"]), "os, k8s, and cni");
        assert_eq!(
            and_list(&[
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
                "d".to_string()
            ]),
            "a, b, c, and d"
        );
    }

    #[test]
    fn a_shell_word_is_quoted_only_when_the_shell_reads_a_character_of_it() {
        assert_eq!(
            shell_words(&["strata", "compare", "pack:k@1.0", "pack:Public Repo/k@1.1"]),
            "strata compare pack:k@1.0 'pack:Public Repo/k@1.1'"
        );
        assert_eq!(shell_words(&["", "a$b", "it's"]), "'' 'a$b' 'it'\\''s'");
    }
}
