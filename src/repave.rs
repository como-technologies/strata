//! The repaves of Palette: `compare --repave` records the answer of Palette to a repave question
//! in the diff, and a cluster record holds the [`PendingRepave`] that waits for approval. Each
//! holds the [`PaletteRepaveReason`] reasons that Palette gives.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The state of a repave that waits for approval, in `status.repave.state` of a cluster. Palette
/// also gives `Approved` and `Reverted`.
const PENDING_STATE: &str = "Pending";

/// True when the cluster document `cluster` says that a repave waits for approval.
pub(crate) fn is_pending(cluster: &Value) -> bool {
    cluster
        .pointer("/status/repave/state")
        .and_then(Value::as_str)
        == Some(PENDING_STATE)
}

/// One reason Palette gives for a repave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaletteRepaveReason {
    /// For example `PackValuesUpdated`.
    pub code: String,
    /// Palette's sentence.
    pub message: String,
    /// The pack the reason is about.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pack: String,
    /// The config keys that differ, such as `kubeadmconfig.postKubeadmCommands`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<String>,
}

impl PaletteRepaveReason {
    /// Each reason of the list `reasons`, in the order of Palette: `reasons` of an answer of
    /// `validate/repave`, or `spec.reasons` of the repave of a cluster. No list gives no reason.
    pub(crate) fn read_all(reasons: Option<&Value>) -> Vec<PaletteRepaveReason> {
        reasons
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(PaletteRepaveReason::read)
            .collect()
    }

    /// One reason. A pack that the change removes has no `target`, and a pack that it adds has no
    /// `current`, so the name comes from the side that is there.
    fn read(r: &Value) -> PaletteRepaveReason {
        let text = |ptr: &str| {
            r.pointer(ptr)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let pack = r
            .pointer("/pack/current/name")
            .or_else(|| r.pointer("/pack/target/name"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        PaletteRepaveReason {
            code: text("/code"),
            message: text("/message"),
            pack,
            keys: r
                .pointer("/pack/diffConfigKeys")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|k| k.as_str().map(str::to_string))
                .collect(),
        }
    }
}

/// A repave of a cluster that waits for approval. Palette replaces the nodes only after a person
/// approves it. While it waits, Palette refuses an update of the profiles of the cluster, and
/// holds a delete of the cluster in `Deleting`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PendingRepave {
    /// What started the repave, as Palette names it: `hubble` when Palette found the change, or
    /// `user`.
    pub source: String,
    /// When the repave started to wait, in RFC 3339.
    pub since: String,
    /// Each reason Palette gives, in its order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<PaletteRepaveReason>,
}

impl PendingRepave {
    /// The repave in `doc`, the answer of `GET /v1/spectroclusters/{uid}/repave/status`.
    pub(crate) fn read(doc: &Value) -> PendingRepave {
        let text = |ptr: &str| {
            doc.pointer(ptr)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        PendingRepave {
            source: text("/spec/source"),
            // Palette sets the time at each change of the state, so it is the time of `Pending`
            since: text("/status/repaveTransitionTime"),
            reasons: PaletteRepaveReason::read_all(doc.pointer("/spec/reasons")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_repave_reads_its_source_its_time_and_the_pack_of_each_reason() {
        let doc = json!({
            "spec": {"source": "hubble", "reasons": [
                {"code": "PackValuesUpdated", "message": "Pack 'kubernetes' values are updated",
                 "pack": {"current": {"name": "kubernetes", "values": "echo one"},
                          "target": {"name": "kubernetes", "values": "echo two"},
                          "diffConfigKeys": ["kubeadmconfig.postKubeadmCommands"]}},
                {"code": "PackAdded", "message": "Pack 'cni' is added",
                 "pack": {"target": {"name": "cni"}}},
            ]},
            "status": {"state": "Pending", "repaveTransitionTime": "2026-09-25T03:20:00Z"},
        });
        let r = PendingRepave::read(&doc);
        assert_eq!(r.source, "hubble");
        assert_eq!(r.since, "2026-09-25T03:20:00Z");
        assert_eq!(r.reasons.len(), 2);
        assert_eq!(r.reasons[0].pack, "kubernetes");
        assert_eq!(r.reasons[0].keys, ["kubeadmconfig.postKubeadmCommands"]);
        assert_eq!(r.reasons[1].pack, "cni");
        assert!(r.reasons[1].keys.is_empty());
        assert!(is_pending(
            &json!({"status": {"repave": {"state": "Pending"}}})
        ));
        assert!(!is_pending(
            &json!({"status": {"repave": {"state": "Approved"}}})
        ));
        assert!(!is_pending(&json!({"status": {}})));
    }
}
