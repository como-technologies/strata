//! The state of a cluster, as Palette gives it in `status.state`.
//!
//! Each module that reads a state of a cluster uses [`ClusterState`]: the consumers, the reports
//! of `verify`, and `strata-preflight`. No module keeps the text of a state of its own.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::closed_set::{ClosedSet, impl_closed_set};
use crate::json::{flag, str_at};

/// A state of a cluster that strata acts on.
///
/// Palette can give another state. [`ClosedSet::parse`] gives
/// `None` for the text of such a state, and the caller decides what to do with it. The text is
/// the one that Palette writes, with a capital letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ClusterState {
    /// The cluster runs.
    Running,
    /// Palette builds the cluster.
    Provisioning,
    /// The build of the cluster waits.
    Pending,
    /// Palette imports the cluster.
    Importing,
    /// Palette deletes the cluster.
    Deleting,
    /// Palette deleted the cluster. Palette keeps the document of such a cluster.
    Deleted,
    /// The cluster failed.
    Error,
}

impl_closed_set!(ClusterState {
    Running => "Running",
    Provisioning => "Provisioning",
    Pending => "Pending",
    Importing => "Importing",
    Deleting => "Deleting",
    Deleted => "Deleted",
    Error => "Error",
});

impl ClusterState {
    /// True when `text`, such as the `status.state` of a document, names this state.
    pub fn is(self, text: &str) -> bool {
        self.as_str() == text
    }

    /// True when `doc`, the document of a cluster or of another object of a record, is the document
    /// of an object that Palette deleted. The state of the object is `Deleted`, or
    /// `metadata.isDeleted` is true. Palette keeps the document of such an object, with its spec.
    pub fn is_gone(doc: &Value) -> bool {
        ClusterState::Deleted.is(str_at(doc, "/status/state")) || flag(doc, "/metadata/isDeleted")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_reads_from_the_text_that_palette_writes_and_a_later_state_is_none() {
        assert_eq!(ClusterState::parse("Running"), Some(ClusterState::Running));
        assert_eq!(ClusterState::parse("running"), None);
        assert_eq!(ClusterState::parse("Upgrading"), None);
    }

    #[test]
    fn a_document_is_gone_when_its_state_is_deleted_or_palette_marks_it_deleted() {
        use serde_json::json;
        assert!(ClusterState::is_gone(
            &json!({"status": {"state": "Deleted"}})
        ));
        assert!(ClusterState::is_gone(&json!({
            "status": {"state": "Deleting"},
            "metadata": {"isDeleted": true},
        })));
        assert!(!ClusterState::is_gone(&json!({
            "status": {"state": "Running"},
            "metadata": {"isDeleted": false},
        })));
        assert!(!ClusterState::is_gone(&json!({})));
        assert!(!ClusterState::is_gone(&Value::Null));
    }
}
