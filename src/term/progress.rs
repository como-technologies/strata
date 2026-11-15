//! Progress notes on stderr. Library users pass [`Progress::silent`] to keep the library quiet.

use crate::TOOL_NAME;
use crate::term::color::{LineClass, Paint};
use std::io::Write;

/// Writes short progress notes to stderr unless quiet.
#[derive(Debug, Clone, Copy)]
pub struct Progress {
    quiet: bool,
    /// The color of standard error: a note is dim, and a warning is yellow.
    paint: Paint,
}

impl Progress {
    /// Notes go to stderr unless `quiet`. They have no color: see [`Progress::painted`].
    pub fn new(quiet: bool) -> Self {
        Progress {
            quiet,
            paint: Paint::off(),
        }
    }

    /// Never writes.
    pub fn silent() -> Self {
        Progress::new(true)
    }

    /// The same notes, in the look of their class when `paint` is on: see [`LineClass`]. The
    /// text does not change, so a log with no color keeps the class of each line.
    pub fn painted(self, paint: Paint) -> Self {
        Progress { paint, ..self }
    }

    /// True when nothing is written. A caller that would do work only to write a note or a
    /// warning skips the work.
    pub fn is_quiet(&self) -> bool {
        self.quiet
    }

    /// Write one note, prefixed with the tool name.
    pub fn note(&self, message: &str) {
        self.write(LineClass::Progress, &format!("{TOOL_NAME}: {message}"));
    }

    /// Write one warning, prefixed with the tool name and `warning:`. A warning says that the
    /// answer may not be the answer the caller asked for, where a note says what strata is doing.
    ///
    /// `quiet` hides it, as it hides a note. With `--json` strata writes the error as one JSON
    /// object on stderr, and a caller that reads it passes `-q` to keep that stream clean; a
    /// warning that ignored `quiet` would break every such caller. So a warning is for the
    /// person at the terminal, who passes no `-q`.
    pub fn warn(&self, message: &str) {
        self.write(
            LineClass::Warning,
            &format!("{TOOL_NAME}: warning: {message}"),
        );
    }

    fn write(&self, class: LineClass, line: &str) {
        if !self.quiet {
            // a reader that closes the pipe early (`| head`) is not an error of strata
            let _ = writeln!(std::io::stderr(), "{}", self.paint.line(class, line));
        }
    }
}

/// The line of an error that stops `tool`: `strata: error: <message>`, red when `paint` is on.
///
/// `quiet` does not hide it. `strata` and `strata-preflight` write each error that stops them
/// here, so a script can find the error with no color.
pub fn error_line(tool: &str, message: &str, paint: &Paint) -> String {
    paint.line(LineClass::Error, &format!("{tool}: error: {message}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::color::{ColorMode, Stream};

    #[test]
    fn an_error_line_names_the_tool_and_the_word_error_with_or_without_color() {
        assert_eq!(
            error_line("strata", "no such file", &Paint::off()),
            "strata: error: no such file"
        );
        let on = Paint::resolve(ColorMode::Always, Stream::Stderr);
        assert_eq!(
            error_line("strata-preflight", "x", &on),
            on.red("strata-preflight: error: x")
        );
    }
}
