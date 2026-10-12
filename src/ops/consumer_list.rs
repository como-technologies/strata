//! The consumer list of a profile document: the clusters that use the version, and the cluster
//! templates that link it. `list --packs` counts them, and `consumers` reads each one.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cluster_template::ClusterTemplateRef;
use crate::json::{arr, strings, text};

/// The consumers that a profile document names, by kind.
///
/// Palette refuses the delete of the version while one of them uses it. `list --packs` gives
/// [`ConsumerCounts::count`] as `consumers`, and the kinds as `consumersByKind`.
/// `consumers --list-only` lists each one, and the edge hosts that it searches: the profile
/// document names no edge host.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConsumerCounts {
    /// The clusters of the consumer list: `status.inUseClusterUids` and `status.inUseClusters`.
    pub clusters: usize,
    /// The cluster templates that link the version: `status.inUseClusterTemplates`.
    pub templates: usize,
}

impl ConsumerCounts {
    /// Count the consumers that the profile document `doc` names, each uid one time.
    pub(crate) fn of(doc: &Value) -> Self {
        ConsumerCounts {
            clusters: consumer_uids(doc).len(),
            templates: ClusterTemplateRef::linked(doc).len(),
        }
    }

    /// The consumers of each kind together: what a row of `list --packs` shows.
    pub fn count(&self) -> usize {
        self.clusters + self.templates
    }
}

/// The uids of the clusters a profile document says use it: `status.inUseClusterUids`, then any
/// uid that only `status.inUseClusters` names, in order and without repeats.
pub(super) fn consumer_uids(doc: &Value) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut uids = Vec::new();
    let by_uid = strings(doc, "/status/inUseClusterUids");
    let named = arr(doc, "/status/inUseClusters")
        .iter()
        .map(|n| text(n, "/uid"));
    for u in by_uid.into_iter().chain(named) {
        if !u.is_empty() && seen.insert(u.clone()) {
            uids.push(u);
        }
    }
    uids
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_consumer_uids_come_by_uid_first_and_then_by_name_with_no_repeat() {
        let doc = json!({"status": {
            "inUseClusterUids": ["c2", "c1", "c2"],
            "inUseClusters": [{"uid": "c1"}, {"uid": "c3"}, {"name": "no uid"}, {"uid": ""}],
        }});
        assert_eq!(consumer_uids(&doc), ["c2", "c1", "c3"]);
        assert!(consumer_uids(&json!({"status": {}})).is_empty());
    }

    #[test]
    fn the_counts_name_each_cluster_and_each_template_one_time_and_the_row_shows_their_sum() {
        let doc = json!({"status": {
            "inUseClusterUids": ["c1"],
            "inUseClusters": [{"uid": "c1"}, {"uid": "c2"}],
            "inUseClusterTemplates": [{"uid": "t1"}, {"uid": "t1"}, {"uid": "t2"}],
        }});
        let counts = ConsumerCounts::of(&doc);
        assert_eq!(
            counts,
            ConsumerCounts {
                clusters: 2,
                templates: 2
            }
        );
        assert_eq!(counts.count(), 4);
        assert_eq!(
            serde_json::to_value(counts).unwrap(),
            json!({"clusters": 2, "templates": 2})
        );

        let linked = ConsumerCounts::of(&json!({"status": {
            "inUseClusters": [], "inUseClusterTemplates": [{"uid": "t1"}],
        }}));
        assert_eq!(
            (linked.clusters, linked.templates, linked.count()),
            (0, 1, 1)
        );
        assert_eq!(ConsumerCounts::of(&json!({})), ConsumerCounts::default());
    }
}
