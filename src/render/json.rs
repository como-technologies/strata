//! The `strata-diff/1` document, serialized as is.

use serde::Serialize;

use super::Renderer;
use crate::closed_set::unknown::Report;
use crate::diff::Diff;
use crate::error::Result;
use crate::ops::{Chain, ConsumersReport, Listing, PackListing, VerifySummary};
use crate::snapshot::Snapshot;

/// Serializes the diff document. Never colored, never truncated.
pub struct JsonRenderer {
    /// One line instead of pretty-printed.
    pub compact: bool,
}

impl JsonRenderer {
    /// Serializes `doc`, pretty-printed unless `compact`, with a trailing newline. Each JSON
    /// document of strata, also a record, goes through here.
    pub(crate) fn document<T: Serialize>(&self, doc: &T) -> Result<String> {
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

    fn render_chain(&self, c: &Chain) -> Result<String> {
        self.document(c)
    }

    fn render_consumers(&self, r: &ConsumersReport) -> Result<String> {
        self.document(r)
    }

    fn render_list(&self, l: &Listing) -> Result<String> {
        self.document(l)
    }

    fn render_packs(&self, l: &PackListing) -> Result<String> {
        self.document(l)
    }

    fn render_behind(&self, r: &crate::ops::BehindReport) -> Result<String> {
        self.document(r)
    }

    fn render_set(&self, r: &crate::ops::SetReport) -> Result<String> {
        self.document(r)
    }

    fn render_report(&self, r: &Report) -> Result<String> {
        self.document(r)
    }
}
