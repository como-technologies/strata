//! The `strata-diff/1` document, serialized as is.

use serde::Serialize;

use super::Renderer;
use crate::diff::Diff;
use crate::error::Result;
use crate::ops::VerifySummary;
use crate::snapshot::Snapshot;

/// Serializes the diff document. Never colored, never truncated.
pub struct JsonRenderer {
    /// One line instead of pretty-printed.
    pub compact: bool,
}

impl JsonRenderer {
    fn document<T: Serialize>(&self, doc: &T) -> Result<String> {
        let mut s = if self.compact {
            serde_json::to_string(doc)?
        } else {
            serde_json::to_string_pretty(doc)?
        };
        s.push('\n');
        Ok(s)
    }
}

impl Renderer for JsonRenderer {
    fn render(&self, d: &Diff, _a: &Snapshot, _b: &Snapshot, _full: bool) -> Result<String> {
        self.document(d)
    }

    fn render_verify_summary(&self, s: &VerifySummary) -> Result<String> {
        self.document(s)
    }
}
