//! The tests of the texts that a message builds. They test a phrase, a cut, a schema, a code span,
//! a code block, and a command of a shell. They also test the marks of HTML, of a terminal, and of
//! markdown.

use super::answer_tags::{
    ANSWER_ADDRESS_LINES, ANSWER_ADDRESSES, ANSWER_CONTROLS, ANSWER_DIRECTION,
    ANSWER_DIRECTION_AS_TEXT, ANSWER_ISSUES, ANSWER_MARKS, ANSWER_MENTIONS, ANSWER_TAGS,
    assert_answer_address_lines_as_markdown, assert_answer_addresses_as_they_are,
    assert_answer_controls_as_markdown, assert_answer_controls_as_text,
    assert_answer_issues_as_text, assert_answer_marks_as_text, assert_answer_mentions_as_text,
};
use super::*;
use crate::TOOL_NAME;
use crate::reference::PACK_PREFIX;
use std::borrow::Cow;

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
    assert_eq!(markdown_cell("first\nsecond"), r"first\\nsecond");
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
fn a_text_on_a_terminal_shows_each_control_character_as_a_visible_escape() {
    assert_answer_controls_as_text(&terminal_text(ANSWER_CONTROLS));
    // a tab, a delete, and a control character of C1, such as the one that starts a sequence
    assert_eq!(terminal_text("a\tb\u{7f}c\u{9b}d"), r"a\tb\u{7f}c\u{9b}d");
    // a text with no control character is the same text, with no copy
    assert!(matches!(
        terminal_text("plain é \\n"),
        Cow::Borrowed("plain é \\n")
    ));
}

#[test]
fn a_text_on_a_terminal_shows_each_mark_of_the_direction_of_a_text_as_a_visible_escape() {
    assert_eq!(terminal_text(ANSWER_DIRECTION), ANSWER_DIRECTION_AS_TEXT);
    let marks: String = DIRECTION_MARKS.iter().collect();
    assert_eq!(
        terminal_text(&marks),
        r"\u{61c}\u{200e}\u{200f}\u{202a}\u{202b}\u{202c}\u{202d}\u{202e}\u{2066}\u{2067}\u{2068}\u{2069}"
    );
    // a letter of a script that reads from right to left is text, and it stays as it is
    assert!(matches!(terminal_text("שלום"), Cow::Borrowed("שלום")));
}

#[test]
fn a_text_with_each_mark_of_direction_as_an_escape_keeps_each_control_character() {
    assert_eq!(direction_text(ANSWER_DIRECTION), ANSWER_DIRECTION_AS_TEXT);
    assert!(matches!(
        direction_text(ANSWER_CONTROLS),
        Cow::Borrowed(ANSWER_CONTROLS)
    ));
}

#[test]
fn a_text_on_a_terminal_keeps_each_backslash_so_a_backslash_and_n_show_as_a_line_break() {
    // `--json` keeps the text as Palette gave it, and so tells the two texts apart
    assert_eq!(terminal_text("a\\nb"), terminal_text("a\nb"));
    assert!(matches!(terminal_text(r"^\d+$"), Cow::Borrowed(r"^\d+$")));
}

#[test]
fn a_block_of_lines_on_a_terminal_keeps_each_line_break_and_escapes_each_other_character() {
    assert_eq!(
        terminal_lines(ANSWER_CONTROLS),
        "\\u{1b}[31mred\\u{1b}[0m over\\rback\nnext"
    );
    assert_eq!(terminal_lines("a\r\nb\u{202e}\n"), "a\\r\nb\\u{202e}\n");
    assert!(matches!(terminal_lines("a\nb\n"), Cow::Borrowed("a\nb\n")));
}

#[test]
fn each_text_of_markdown_shows_each_control_character_and_each_mark_of_direction_as_an_escape() {
    assert_answer_controls_as_markdown(&markdown_line(ANSWER_CONTROLS));
    assert_answer_controls_as_markdown(&markdown_cell(ANSWER_CONTROLS));
    assert_answer_controls_as_markdown(&lines_cell([ANSWER_CONTROLS, "next"]));
    // a code span and a block of HTML show the escapes of the terminal, with no `\` before them
    assert_answer_controls_as_text(&code_span(ANSWER_CONTROLS));
    assert_answer_controls_as_text(&code_cell(ANSWER_CONTROLS));
    assert_answer_controls_as_text(&quoted_html(ANSWER_CONTROLS));
    // a block keeps its line breaks
    assert_eq!(
        code_block("", ANSWER_CONTROLS),
        "```\n\\u{1b}[31mred\\u{1b}[0m over\\rback\nnext\n```\n"
    );
    assert_eq!(
        markdown_line(ANSWER_DIRECTION),
        r"user\\u{202e}nimda\\u{202c} ok"
    );
    for text in [
        code_span(ANSWER_DIRECTION),
        code_block("", ANSWER_DIRECTION),
        quoted_html(ANSWER_DIRECTION),
    ] {
        assert!(text.contains(ANSWER_DIRECTION_AS_TEXT), "{text:?}");
    }
}

#[test]
fn a_cell_of_a_markdown_table_holds_no_tag_of_html_from_a_quoted_text() {
    let cell = markdown_cell(&format!("{ANSWER_TAGS}\n| next"));
    assert!(!cell.contains('<'), "{cell}");
    assert_eq!(cell, format!(r"{}\\n\| next", html_text(ANSWER_TAGS)));
}

#[test]
fn a_cell_of_a_markdown_table_shows_each_mark_of_markdown_of_a_quoted_text_as_text() {
    assert_answer_marks_as_text(&markdown_cell(ANSWER_MARKS));
    // two backticks open no code span, so a code span cannot show an entity as it is
    assert_eq!(markdown_cell("`a < b`"), "\\`a &lt; b\\`");
    assert_eq!(markdown_cell("``a``"), "\\`\\`a\\`\\`");
    assert_eq!(
        markdown_cell("~~gone~~ _it_ ![i](x)"),
        "\\~\\~gone\\~\\~ \\_it\\_ !\\[i\\](x)"
    );
    // a `\` of the text stays a `\`, also before a mark and before a `|`
    assert_eq!(markdown_cell("a\\*b\\|c"), "a\\\\\\*b\\\\\\|c");
}

#[test]
fn a_cell_of_a_markdown_table_holds_no_mention_of_github_from_a_quoted_text() {
    let z = ZERO_WIDTH_SPACE;
    assert_answer_mentions_as_text(&markdown_cell(ANSWER_MENTIONS));
    // a `@` with no letter or digit of ASCII after it, or with one before it, starts no mention
    assert_eq!(
        markdown_cell("ops@example.com k@1.0 a@-b @ @-"),
        "ops@example.com k@1.0 a@-b @ @-"
    );
    // the `n` of the escape of a line break comes before `@f`, so that `@` starts no mention
    assert_eq!(
        markdown_cell("(@a) @@b é@c _@d *@e\n@f|@g"),
        format!(r"(@{z}a) @@{z}b é@{z}c \_@{z}d \*@{z}e\\n@f\|@{z}g")
    );
    // the space is an entity of strata, so its `&` gets no escape, and an `&` of the text does
    assert_eq!(markdown_cell("&@a <@b"), format!("&amp;@{z}a &lt;@{z}b"));
    assert_eq!(code_cell("@a"), "`@a`");
}

#[test]
fn a_cell_of_a_markdown_table_links_to_no_issue_of_github_from_a_quoted_text() {
    let z = ZERO_WIDTH_SPACE;
    assert_answer_issues_as_text(&markdown_cell(ANSWER_ISSUES));
    // each `#` before a digit gets the space, and so does a `GH-` in each case of its letters
    assert_eq!(
        markdown_cell("a#1 é#2 (#3) gh-4 Gh-5 éGH-6 €gH-7 _GH-8"),
        format!("a#{z}1 é#{z}2 (#{z}3) gh-{z}4 Gh-{z}5 éGH-{z}6 €gH-{z}7 \\_GH-{z}8")
    );
    // a `#` or a `GH-` with no digit after it, or a `GH-` after a letter or a digit, is no reference
    assert_eq!(
        markdown_cell("#a # GH- GH-x G-1 H-1 high-1 1GH-2"),
        "#a # GH- GH-x G-1 H-1 high-1 1GH-2"
    );
    // the space is an entity of strata, so its `#` gets no space, and a `#` of the text does
    assert_eq!(markdown_cell("@a &#12;"), format!("@{z}a &amp;#{z}12;"));
    assert_eq!(
        markdown_cell("https://example.com/#12 #12"),
        format!("https://example.com/#12 #{z}12")
    );
    assert_eq!(code_cell("#12 GH-3"), "`#12 GH-3`");
}

#[test]
fn a_text_of_html_that_strata_quotes_holds_no_mention_no_link_to_an_issue_and_no_line_end() {
    let z = ZERO_WIDTH_SPACE;
    assert_answer_mentions_as_text(&quoted_html(ANSWER_MENTIONS));
    assert_answer_issues_as_text(&quoted_html(ANSWER_ISSUES));
    // GitHub reads no markdown in a block of HTML, so a mark of markdown gets no `\`
    assert_eq!(
        quoted_html("<b> *a* `c` @octocat #12 k@1.0"),
        format!("&lt;b&gt; *a* `c` @{z}octocat #{z}12 k@1.0")
    );
    // a blank line ends a block of HTML, and the rest of the text is markdown
    assert_eq!(quoted_html("a\n\n*b*\rc"), r"a\n\n*b*\rc");
}

#[test]
fn a_cell_of_a_markdown_table_keeps_each_bare_address_that_github_links_as_it_is() {
    let z = ZERO_WIDTH_SPACE;
    assert_answer_addresses_as_they_are(&markdown_cell(ANSWER_ADDRESSES));
    assert_eq!(markdown_cell(ANSWER_ADDRESSES), ANSWER_ADDRESSES);
    for address in [
        "https://example.com/?a=1&b=2",
        "http://A.b-c.example:8080/x?y=@z#`f`[g](h)",
        "https://xn--bcher-kva.example/a\\b>c",
    ] {
        assert_eq!(markdown_cell(address), address);
        assert_eq!(
            markdown_cell(&format!("*a* {address} @b")),
            format!("\\*a\\* {address} @{z}b")
        );
    }
    // GitHub links `www.` only after a space or a `(`, and a scheme also after a quote
    assert_eq!(
        markdown_cell("a www.example.com/@b (www.example.com/@b) 'https://example.com/@b'"),
        "a www.example.com/@b (www.example.com/@b) 'https://example.com/@b'"
    );
    assert_eq!(
        markdown_cell("www.example.com/@b"),
        format!("www.example.com/@{z}b")
    );
    assert_eq!(
        markdown_cell("a \"www.example.com/@b\""),
        format!("a \"www.example.com/@{z}b\"")
    );
    // GitHub cuts the marks at the end from the link, and no `\` comes before them
    assert_eq!(
        markdown_cell("https://example.com/a_(b)) https://example.com/x_*~.\""),
        "https://example.com/a_(b)) https://example.com/x&lowbar;&ast;~.\""
    );
    // a form that GitHub does not surely link keeps each escape, and each `@` gets its space
    for text in [
        "https://-a.example/@b",
        "https://a_b.example/@b",
        "https://localhost/@b",
        "xhttps://example.com/@b",
        "ftp://example.com/@b",
        "https://example.com/é@b",
    ] {
        assert_eq!(
            markdown_cell(text),
            text.replace('@', &format!("@{z}")),
            "{text}"
        );
    }
    for (text, escaped) in [
        ("https://example.com/<b>", "https://example.com/&lt;b&gt;"),
        (
            "https://example.com/a&amp;",
            "https://example.com/a&amp;amp;",
        ),
        ("https://", "https://"),
        ("www.", "www."),
    ] {
        assert_eq!(markdown_cell(text), escaped, "{text}");
    }
    assert_eq!(
        markdown_cell("https://example.com/a|b"),
        "https://example.com/a\\|b"
    );
}

#[test]
fn a_bare_address_ends_its_link_before_the_escape_of_a_character_in_a_line_and_a_cell_of_markdown()
{
    let (end, z) = (ADDRESS_END, ZERO_WIDTH_SPACE);
    assert_answer_address_lines_as_markdown(&markdown_line(ANSWER_ADDRESS_LINES));
    assert_answer_address_lines_as_markdown(&markdown_cell(ANSWER_ADDRESS_LINES));
    assert_answer_address_lines_as_markdown(&lines_cell([ANSWER_ADDRESS_LINES]));
    // the tag comes after the marks that GitHub cuts from the link, also before a mark of direction
    assert_eq!(
        markdown_line("https://example.com/a_*.\r(www.example.com/b)\u{202e} c"),
        format!(
            r"https://example.com/a&lowbar;&ast;.{end}\\r(www.example.com/b){end}\\u{{202e}} c"
        )
    );
    // GitHub links a scheme after the escape `\u{1b}`, and none after the `n` of the escape `\n`
    assert_eq!(
        markdown_line("\u{1b}https://example.com/*a* \nhttps://example.com/*b*"),
        r"\\u{1b}https://example.com/*a&ast; \\nhttps://example.com/\*b\*"
    );
    // GitHub links `www.` after no escape
    assert_eq!(
        markdown_line("a\u{1b}www.example.com/@b"),
        format!(r"a\\u{{1b}}www.example.com/@{z}b")
    );
    // a word that is no address gets no tag before its escape
    assert_eq!(
        markdown_line("https://localhost/a\nb"),
        r"https://localhost/a\\nb"
    );
}

#[test]
fn a_line_of_markdown_keeps_each_bar_and_a_cell_escapes_each_bar_also_in_an_address() {
    let z = ZERO_WIDTH_SPACE;
    // GitHub links `https://example.com/a\|b` to `a%5C%7Cb` outside a table
    assert_eq!(
        markdown_line("https://example.com/a|b a|b"),
        "https://example.com/a|b a|b"
    );
    assert_eq!(
        markdown_cell("https://example.com/a|b a|b"),
        "https://example.com/a\\|b a\\|b"
    );
    // a line holds no line end, and its other escapes are those of a cell
    assert_eq!(markdown_line("*a*\n@b\r#1"), format!(r"\*a\*\\n@b\\r#{z}1"));
    for text in [
        ANSWER_TAGS,
        ANSWER_MARKS,
        ANSWER_MENTIONS,
        ANSWER_ISSUES,
        ANSWER_ADDRESSES,
        "a\\*b\\|c\nd",
    ] {
        assert_eq!(markdown_cell(text), table_text(&markdown_line(text)));
    }
}

#[test]
fn a_cell_of_a_markdown_table_writes_a_lone_carriage_return_as_a_visible_escape() {
    let row = format!("| {} | {} |", markdown_cell("x\ry"), code_cell("x\ry"));
    assert_eq!(row, r"| x\\ry | `x\ry` |");
    // markdown ends a line at a `\r` too, so the row holds no line end
    assert!(!row.contains(LINE_ENDS), "{row:?}");
    assert_eq!(markdown_cell("x\r\ny"), r"x\\r\\ny");
}

#[test]
fn a_run_of_underscores_inside_a_word_has_no_escape_and_each_other_run_has_one() {
    // a run with a letter or a digit on each side starts no emphasis, so a name keeps its form
    assert_eq!(
        markdown_cell("aws_stack LTS__24.x é_ü"),
        "aws_stack LTS__24.x é_ü"
    );
    assert_eq!(
        markdown_cell("__init__ a_ _b a_-b"),
        "\\_\\_init\\_\\_ a\\_ \\_b a\\_-b"
    );
    assert_eq!(markdown_cell("_"), "\\_");
    assert_eq!(markdown_cell(""), "");
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
fn a_code_span_keeps_a_bar_for_a_title_and_a_sentence_and_escapes_a_line_break() {
    assert_eq!(code_span("a|b"), "`a|b`");
    assert_eq!(code_span("first\nsecond"), r"`first\nsecond`");
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
    assert_eq!(code_cell("first\nsecond"), r"`first\nsecond`");
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
    let (a, b) = (
        format!("{PACK_PREFIX}k@1.0"),
        format!("{PACK_PREFIX}Public Repo/k@1.1"),
    );
    assert_eq!(
        shell_words(&[TOOL_NAME, "compare", &a, &b]),
        format!("{TOOL_NAME} compare {a} '{b}'")
    );
    assert_eq!(shell_words(&["", "a$b", "it's"]), "'' 'a$b' 'it'\\''s'");
}
