//! The cluster templates of Palette. A cluster template links profile versions and a maintenance
//! policy, and applies them to each cluster that it governs. A profile record names each
//! [`ClusterTemplateRef`] that links the version, and `consumers` counts each one.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A cluster template that links a profile version, as the profile document names it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterTemplateRef {
    /// Template uid.
    pub uid: String,
    /// Template name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The project of the template; empty when Palette does not name it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub project_uid: String,
}

impl ClusterTemplateRef {
    /// Each template that links the version of the profile document `profile`, from
    /// `status.inUseClusterTemplates`, once for each uid, in the order of Palette.
    pub(crate) fn linked(profile: &Value) -> Vec<Self> {
        let mut seen = BTreeSet::new();
        profile
            .pointer("/status/inUseClusterTemplates")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|t| ClusterTemplateRef {
                uid: text(t, "/uid"),
                name: text(t, "/name"),
                project_uid: text(t, "/projectUid"),
            })
            .filter(|t| !t.uid.is_empty() && seen.insert(t.uid.clone()))
            .collect()
    }
}

/// The string at `ptr` in `v`, or empty.
fn text(v: &Value, ptr: &str) -> String {
    v.pointer(ptr)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_linked_templates_of_a_profile_are_listed_once_for_each_uid() {
        let profile = json!({"status": {"inUseClusterTemplates": [
            {"uid": "t2", "name": "b", "projectUid": "p1", "kind": "clustertemplate"},
            {"uid": "t1", "name": "a", "projectUid": "p1", "kind": "clustertemplate"},
            {"uid": "t2", "name": "b", "projectUid": "p1", "kind": "clustertemplate"},
            {"uid": "", "name": "no uid"}
        ]}});
        let uids: Vec<String> = ClusterTemplateRef::linked(&profile)
            .into_iter()
            .map(|t| t.uid)
            .collect();
        assert_eq!(uids, ["t2", "t1"]);
        assert!(ClusterTemplateRef::linked(&json!({"status": {}})).is_empty());
    }
}
