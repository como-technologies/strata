//! Progress notes on stderr. Library users pass [`Progress::silent`] to keep the library quiet.

use crate::TOOL_NAME;

/// Writes short progress notes to stderr unless quiet.
#[derive(Debug, Clone, Copy)]
pub struct Progress {
    quiet: bool,
}

impl Progress {
    /// Notes go to stderr unless `quiet`.
    pub fn new(quiet: bool) -> Self {
        Progress { quiet }
    }

    /// Never writes.
    pub fn silent() -> Self {
        Progress { quiet: true }
    }

    /// True when nothing is written. A caller that would do work only to write a note or a
    /// warning skips the work.
    pub fn is_quiet(&self) -> bool {
        self.quiet
    }

    /// Write one note, prefixed with the tool name.
    pub fn note(&self, message: &str) {
        if !self.quiet {
            eprintln!("{TOOL_NAME}: {message}");
        }
    }

    /// Write one warning, prefixed with the tool name and `warning:`. A warning says that the
    /// answer may not be the answer the caller asked for, where a note says what strata is doing.
    ///
    /// `quiet` hides it, as it hides a note. With `--json` strata writes the error as one JSON
    /// object on stderr, and a caller that reads it passes `-q` to keep that stream clean; a
    /// warning that ignored `quiet` would break every such caller. So a warning is for the
    /// person at the terminal, who passes no `-q`.
    pub fn warn(&self, message: &str) {
        if !self.quiet {
            eprintln!("{TOOL_NAME}: warning: {message}");
        }
    }
}
