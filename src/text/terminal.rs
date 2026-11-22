//! A text on a terminal, with each control character and each mark of the direction of a text as a
//! visible escape.

use std::borrow::Cow;

/// The marks of the direction of a text: the characters of the property `Bidi_Control` of Unicode.
///
/// `char::is_control` is false for each one. A terminal that reads such a mark, such as U+202E, can
/// show the characters after it in another order. So the text shows another text than the one
/// that it holds. [`terminal_text`] writes each one as a visible escape.
pub const DIRECTION_MARKS: [char; 12] = [
    '\u{61c}', '\u{200e}', '\u{200f}', '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}', '\u{202e}',
    '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
];

/// `s` with each control character and each mark of direction as a visible escape, such as
/// `\u{1b}` for ESC and `\n` for a line break.
///
/// A control character can start an escape sequence of the terminal, move the cursor back, or
/// start a new line in a one-line text. A mark of direction can change the order of the text. Each
/// line that strata writes on a terminal puts a text of Palette here before it paints the text.
/// The markdown of strata writes each text in the same way, because a person can read it on a
/// terminal too. The JSON documents and the records keep the text as Palette gave it.
///
/// A `\` stays as it is. So a text with the two characters `\n` and a text with a line break show
/// the same. A `\` of a value or of a command keeps its form, such as the `\d` of a pattern.
pub fn terminal_text(s: &str) -> Cow<'_, str> {
    escaped(s, shows_no_text)
}

/// `s` as [`terminal_text`] writes it, with each line break kept: a block of lines keeps its lines.
///
/// A unified diff of `--full` and a fenced code block of markdown write their text here.
pub(crate) fn terminal_lines(s: &str) -> Cow<'_, str> {
    escaped(s, |c| shows_no_text(c) && c != '\n')
}

/// `s` with each of the [`DIRECTION_MARKS`] as the visible escape of [`terminal_text`], and each
/// other character as it is.
///
/// The HTML report of `strata-preflight` writes each text here before [`html_text`]. A browser
/// reads a mark of direction as a terminal reads it, so a name can show as another name.
///
/// [`html_text`]: super::html_text
pub fn direction_text(s: &str) -> Cow<'_, str> {
    escaped(s, |c| DIRECTION_MARKS.contains(&c))
}

/// `s` with each character that `escape` finds as its visible escape.
fn escaped(s: &str, escape: impl Fn(char) -> bool) -> Cow<'_, str> {
    if !s.chars().any(&escape) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if escape(c) {
            out.extend(c.escape_default());
        } else {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

/// True for a character that a terminal reads as an instruction and not as text: a control
/// character, or one of the [`DIRECTION_MARKS`].
pub(super) fn shows_no_text(c: char) -> bool {
    c.is_control() || DIRECTION_MARKS.contains(&c)
}
