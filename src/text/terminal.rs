//! A text on a terminal, with each control character as a visible escape.

use std::borrow::Cow;

/// `s` with each control character as a visible escape, such as `\u{1b}` for ESC and `\n` for a
/// line break. So a text of Palette does not change what a terminal shows.
///
/// A control character can start an escape sequence of the terminal, move the cursor back, or
/// start a new line in a one-line text. Each line that strata writes on a terminal puts a text of
/// Palette here before it paints the text. The JSON documents and the records keep the text as
/// Palette gave it.
pub fn terminal_text(s: &str) -> Cow<'_, str> {
    if !s.chars().any(char::is_control) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_control() {
            out.extend(c.escape_default());
        } else {
            out.push(c);
        }
    }
    Cow::Owned(out)
}
