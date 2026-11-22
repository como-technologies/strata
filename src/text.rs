//! The texts that a message builds:
//!
//! - A phrase from a list of names, and the name of an object.
//! - A long text cut to a length.
//! - A mark for an empty text, a text of HTML, a text on a terminal, a line of markdown, a text in
//!   a cell of a markdown table, a text in a code span, and a text in a fenced code block. Each
//!   text of markdown writes a control character as a text on a terminal does.
//! - A command as a person types it in a shell.
//! - A JSON Schema as text, with its descriptions in plain words.
//!
//! `strata-preflight` calls the public ones, so both tools write each of these in one way.

use schemars::Schema;
use serde_json::Value;

mod terminal;

pub(crate) use terminal::terminal_lines;
pub use terminal::{DIRECTION_MARKS, terminal_text};

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

/// `s` as the text of one cell of a markdown table: [`markdown_line`], with each `|` escaped.
///
/// A `|` ends a cell, and the table reads a `\|` as a `|` before it reads the text of the cell.
/// So the cell escapes each `|`, also in a bare address, and the link of the address keeps its
/// target. A code span shows an entity and a `\` as they are, so [`code_cell`] writes a value in a
/// code span.
pub fn markdown_cell(s: &str) -> String {
    table_text(&markdown_line(s))
}

/// `s` as one line of markdown, such as a heading, a note, or an item of a list.
///
/// Each control character is the visible escape of [`terminal_text`], so a line end starts no new
/// line. A bare address that GitHub surely links stays as it is, so its link goes to its target.
/// Each other word is [`html_text`], with a `\` before each mark of markdown. It also gets a zero
/// width space in each mention and each reference to an issue. So a text that strata quotes from
/// an answer, such as a message of Palette, shows as text. A `@name` in it notifies nobody, and a
/// `#12` in it links to no issue.
///
/// GitHub links an address with a scheme only when no letter comes before the scheme. So a
/// caller puts no letter right before the line. A `|` outside a table needs no escape, and a `\`
/// before it stays in the link of an address. So the line keeps each `|` as it is.
pub fn markdown_line(s: &str) -> String {
    // each other white space of ASCII is a control character, so the escape leaves only the space
    terminal_text(s)
        .split_inclusive(' ')
        .enumerate()
        .map(|(i, piece)| {
            let word = piece.trim_end_matches(' ');
            markdown_word(word, i == 0) + &piece[word.len()..]
        })
        .collect()
}

/// The characters that end a line: a line break and a carriage return.
///
/// Markdown reads a carriage return with no line break after it as a line end too, so it ends a
/// row of a table. A text on one line, such as the body of an answer in an error, writes each one
/// as a space.
pub const LINE_ENDS: [char; 2] = ['\n', '\r'];

/// `word`, a text with no space of ASCII, as markdown. `first` is true for the first word of the
/// text, which can follow the text of the caller with no space.
///
/// A bare address stays as it is. GitHub cuts the marks at the end of the address from the link,
/// and a `\` before such a mark stays in the link. So a cut `*` is `&ast;` and a cut `_` is
/// `&lowbar;`, and GitHub shows each one as text. HTML names no entity for a `~`. A cut `~` stays
/// as it is, and GitHub shows it as text, as its markdown route shows.
fn markdown_word(word: &str, first: bool) -> String {
    let Some(link) = bare_address(word, first) else {
        return quoted_text(word);
    };
    let cut = word[link.end..]
        .replace('*', "&ast;")
        .replace('_', "&lowbar;");
    format!("{}{}{cut}", quoted_text(&word[..link.start]), &word[link])
}

/// `s` with the escapes of a quoted text that is no bare address. It is [`quoted_html`], with a
/// `\` before each mark of markdown.
fn quoted_text(s: &str) -> String {
    quoted_html(&markdown_text(s))
}

/// `s` as a text of HTML that strata quotes in the markdown of GitHub, such as the words of the
/// `<summary>` of a fold. It is [`html_text`], with a zero width space in each mention and each
/// reference to an issue.
///
/// GitHub reads no markdown in a block of HTML, so a `\` before a mark shows there. But it reads
/// a mention and a reference to an issue there too. So a `@name` in the words notifies nobody,
/// and a `#12` in them links to no issue. A blank line ends the block, so the text writes each
/// control character as [`terminal_text`] does.
pub(crate) fn quoted_html(s: &str) -> String {
    reference_text(&html_text(&terminal_text(s)))
}

/// The schemes of a bare address that GitHub links after each character that is not a letter.
const SCHEMES: [&str; 2] = ["https://", "http://"];

/// The start of a bare address with no scheme. GitHub links it only after a space or a `(`.
const WWW: &str = "www.";

/// The characters that GitHub cuts from the end of a bare address, as the end of a sentence. The
/// link ends before them.
const LINK_CUTS: [char; 11] = ['?', '!', '.', ',', ':', '*', '_', '~', '\'', '"', ';'];

/// The range of `word` that GitHub surely links as a bare address, or `None`. `word` is a text
/// with no space of ASCII, and `first` is true when it starts the text.
///
/// The address starts with `https://` or `http://`, after an optional `(`, `"`, or `'`. Or it
/// starts with `www.` after a `(`, or after a space when `first` is false. Its host has two
/// labels or more of letters, digits, and `-` of ASCII, with a letter or a digit at each end of a
/// label. The word holds only printable characters of ASCII, and no `<`. The markdown route of
/// GitHub links each such address, with each mark and each `@` in it as text. The range ends where
/// the link ends: see [`link_end`].
fn bare_address(word: &str, first: bool) -> Option<std::ops::Range<usize>> {
    if !word.bytes().all(|b| b.is_ascii_graphic()) || word.contains('<') {
        return None;
    }
    let start = usize::from(word.starts_with(['(', '"', '\'']));
    let rest = &word[start..];
    let www = rest.starts_with(WWW)
        && if start == 0 {
            !first
        } else {
            word.starts_with('(')
        };
    let scheme = match SCHEMES.iter().find(|s| rest.starts_with(**s)) {
        Some(s) => s.len(),
        None if www => WWW.len(),
        None => return None,
    };
    let end = start + link_end(rest)?;
    let host = word
        .get(start + scheme..end)?
        .split(['/', '?', '#', ':'])
        .next()?;
    (host.contains('.') && host.split('.').all(is_label)).then_some(start..end)
}

/// True for a label of a host that GitHub links: letters, digits, and `-` of ASCII, with a letter
/// or a digit at each end.
fn is_label(label: &str) -> bool {
    let ends = |c: Option<&u8>| c.is_some_and(u8::is_ascii_alphanumeric);
    let bytes = label.as_bytes();
    ends(bytes.first())
        && ends(bytes.last())
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'-')
}

/// The length of `text`, a bare address from its scheme on, that GitHub keeps in its link. `None`
/// when GitHub cuts an entity from the end of the text.
///
/// GitHub cuts each character of [`LINK_CUTS`] from the end. It cuts a `)` from the end while
/// the text holds more `)` than `(`. It cuts an entity, such as `&amp;`, and shows the character
/// that the entity names. So an address that ends with an entity does not stay as it is.
fn link_end(text: &str) -> Option<usize> {
    let open = text.matches('(').count();
    let mut close = text.matches(')').count();
    let mut end = text.len();
    while let Some(c) = text[..end].chars().next_back() {
        if c == ')' && close > open {
            close -= 1;
        } else if c == ';' && is_entity_end(&text[..end]) {
            return None;
        } else if !LINK_CUTS.contains(&c) {
            break;
        }
        end -= c.len_utf8();
    }
    Some(end)
}

/// True when `text` ends with an entity of a name: a `&`, letters of ASCII, and a `;`.
fn is_entity_end(text: &str) -> bool {
    let name = text.strip_suffix(';').unwrap_or(text);
    let before = name.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    before.len() < name.len() && before.ends_with('&')
}

/// The entity of a zero width space. GitHub shows no character for it.
const ZERO_WIDTH_SPACE: &str = "&#8203;";

/// `s` with a zero width space in each mention of GitHub and in each reference to an issue of
/// GitHub. So a quoted `@name` notifies nobody, and a quoted `#12` links to no issue.
///
/// The space goes after the character that [`starts_reference`] finds. GitHub reads no mention
/// and no reference in a code span, so [`code_cell`] writes no space.
fn reference_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, c) in s.char_indices() {
        out.push(c);
        if starts_reference(&s[..i], c, &s[i + c.len_utf8()..]) {
            out.push_str(ZERO_WIDTH_SPACE);
        }
    }
    out
}

/// True when GitHub can read a mention or a reference to an issue right after `c`. `before` and
/// `after` are the text on each side of `c`.
///
/// - A mention, such as `@name` or `@org/team`, starts at a `@` with a letter or a digit of ASCII
///   after it. A `@` after such a character starts no mention, as in an address of mail or in
///   `k@1.0`. So such a `@` stays as it is, and a ref keeps its form.
/// - A reference to an issue is a `#` with a digit of ASCII after it, as in `#12` or in
///   `owner/repo#12`. A link to an issue of another repository adds an event to that issue.
/// - GitHub also reads `GH-12`, in each case of the letters, as a reference to an issue. A
///   `GH-` after a letter or a digit of ASCII starts no reference, as in `high-12`.
fn starts_reference(before: &str, c: char, after: &str) -> bool {
    let word = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric());
    let digit = after.starts_with(|c: char| c.is_ascii_digit());
    match c {
        '@' => !word(before.chars().next_back()) && word(after.chars().next()),
        '#' => digit,
        '-' => {
            digit
                && before
                    .strip_suffix(['h', 'H'])
                    .and_then(|b| b.strip_suffix(['g', 'G']))
                    .is_some_and(|b| !word(b.chars().next_back()))
        }
        _ => false,
    }
}

/// The marks of markdown that get a `\` in each place. [`markdown_text`] gives a `_` its `\` only
/// where the `_` can start or end an emphasis.
const MARKS: [char; 6] = ['\\', '`', '*', '[', ']', '~'];

/// `s` with a `\` before each mark of markdown, so that GitHub shows each mark as text.
///
/// The marks are a `\`, a backtick, `*`, `[`, `]`, `~`, and `_`. A run of `_` with a letter or a
/// digit on each side, such as the `_` of `aws_stack`, starts no emphasis. So such a run stays as
/// it is, and a name or a version keeps its form.
fn markdown_text(s: &str) -> String {
    let word = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
    let mut out = String::with_capacity(s.len());
    let mut before = None;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '_' {
            let mut run = 1;
            while chars.next_if_eq(&'_').is_some() {
                run += 1;
            }
            let mark = if word(before) && word(chars.peek().copied()) {
                "_"
            } else {
                "\\_"
            };
            out.push_str(&mark.repeat(run));
        } else {
            if MARKS.contains(&c) {
                out.push('\\');
            }
            out.push(c);
        }
        before = Some(c);
    }
    out
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

/// `s` with each `|` escaped, so that it cannot end a cell of a markdown table. `s` holds no line
/// end, because [`markdown_line`] and [`code_span`] escape each one.
fn table_text(s: &str) -> String {
    s.replace('|', "\\|")
}

/// `text` as a code span of markdown. The fence is longer than each run of backticks in the text,
/// so a value cannot end the span.
///
/// The span writes each control character as [`terminal_text`] does, and each other character as
/// it is. A title and a sentence need no escape for a `|`. A span in a table cell needs one: see
/// [`code_cell`]. Each issue and each report that shows a value of an unknown set writes the value
/// here, so the value has one form in both.
pub fn code_span(text: &str) -> String {
    let text = terminal_text(text);
    let fence = "`".repeat(longest_backticks(&text) + 1);
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
/// answer, such as the values of a pack, stays in its block. The block keeps each line break, and
/// writes each other control character as [`terminal_text`] does.
pub fn code_block(info: &str, text: &str) -> String {
    let text = terminal_lines(text);
    let fence = "`".repeat((longest_backticks(&text) + 1).max(3));
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

/// `text` as a code span in one cell of a markdown table: [`code_span`], with each `|` escaped. So
/// a value cannot end a cell or a row of the table.
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

// The texts of an answer, in the file that the tests of strata use. A unit test
// here uses the texts and some of the checks, so the other checks are dead code here.
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
mod tests;
