//! The `strata-diff/1` document, serialized as is.

use super::Renderer;
use crate::diff::Diff;
use crate::error::Result;
use crate::snapshot::Snapshot;

/// Serializes the diff document. Never colored, never truncated.
pub struct JsonRenderer {
    /// One line instead of pretty-printed.
    pub compact: bool,
}

impl Renderer for JsonRenderer {
    fn render(&self, d: &Diff, _a: &Snapshot, _b: &Snapshot, _full: bool) -> Result<String> {
        let mut s = if self.compact {
            serde_json::to_string(d)?
        } else {
            serde_json::to_string_pretty(d)?
        };
        s.push('\n');
        Ok(s)
    }
}
