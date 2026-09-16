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

    /// Write one note, prefixed with the tool name.
    pub fn note(&self, message: &str) {
        if !self.quiet {
            eprintln!("{TOOL_NAME}: {message}");
        }
    }
}
