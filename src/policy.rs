//! The fields of the `policy` tier of a cluster record and of a cluster group record.
//!
//! The tier of a cluster holds the settings of the cluster in `spec.clusterConfig`, its role
//! bindings and its namespaces, and how it takes an update. The tier of a cluster group holds the
//! config that the group gives each virtual cluster.

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::api::RawCluster;
use crate::canon::{canon_yaml_hashed, canonical_json};

/// The key of the policy fields that holds the role bindings.
const RBACS: &str = "rbacs";

/// The key of the policy fields of a cluster record from before strata read the bindings: the
/// references to the bindings and the namespaces. The policy tier of a cluster group does not
/// have it either.
const LEGACY_RESOURCES: &str = "resources";

/// The policy fields that a record from before strata read the bindings holds, less the
/// references to the bindings and the namespaces. Do not change this list: `compare` and `verify`
/// read an older record through it.
const LEGACY_FIELDS: &[&str] = &["osPatch", "lifecycle", "machineHealth", "timezone", "fips"];

/// The annotation of a cluster that stops the upgrades of the agent of Palette on the cluster.
/// Palette stops them when the value is `true`, and not for another value.
const AGENT_UPGRADES_FORBIDDEN: &str = "spectroComponentsUpgradeForbidden";

/// Whether Palette upgrades its agent on the cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum AgentUpgrades {
    /// Palette upgrades the agent: the cluster has no annotation that stops it.
    Allowed,
    /// A person paused the upgrades of the agent.
    Paused,
}

impl AgentUpgrades {
    /// The state of the upgrades of the agent of the cluster document `cluster`.
    fn of(cluster: &Value) -> AgentUpgrades {
        let forbidden = cluster
            .pointer("/metadata/annotations")
            .and_then(|a| a.get(AGENT_UPGRADES_FORBIDDEN))
            .and_then(Value::as_str);
        match forbidden {
            Some("true") => AgentUpgrades::Paused,
            _ => AgentUpgrades::Allowed,
        }
    }
}

/// The fields of the policy tier of the cluster in `raw`, whose `spec.clusterConfig` is `config`.
/// The bindings and the namespaces come from their own routes, without the uids and the times.
/// Each one is null when strata did not read its route.
pub(crate) fn fields(raw: &RawCluster, config: &Value) -> Value {
    let cluster = &raw.cluster;
    let field = |k: &str| config.get(k).cloned().unwrap_or(Value::Null);
    json!({
        "osPatch": config
            .pointer("/machineManagementConfig/osPatchConfig")
            .cloned()
            .unwrap_or(Value::Null),
        "lifecycle": field("lifecycleConfig"),
        "machineHealth": field("machineHealthConfig"),
        "timezone": text(config, "/timezone"),
        "fips": text(cluster, "/status/fips/mode"),
        RBACS: raw.rbacs.as_ref().map(bindings),
        "namespaces": raw.namespaces.as_ref().map(namespaces),
        // Palette omits the field when it is false
        "updateWorkerPoolsInParallel": config
            .get("updateWorkerPoolsInParallel")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "controlPlaneHealthCheckTimeout": text(config, "/controlPlaneHealthCheckTimeout"),
        "clusterMetaAttribute": text(config, "/clusterMetaAttribute"),
        "applySetting": text(cluster, "/status/spcApply/actionType"),
        "agentUpgrades": AgentUpgrades::of(cluster),
    })
}

/// The fields of the policy tier of a cluster group, from its `spec.clustersConfig` in `config`:
/// the config that the group gives each virtual cluster.
///
/// The group applies `values` to each virtual cluster as YAML, as a pack applies its values. So
/// the tier holds the values in canonical form with their hash, and the host clusters sorted by
/// uid.
pub(crate) fn group_fields(config: &Value) -> Value {
    let (values, values_hash, values_parse_error) = canon_yaml_hashed(text(config, "/values"));
    let mut hosts: Vec<Value> = config
        .get("hostClustersConfig")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|h| {
            json!({
                "uid": text(h, "/clusterUid"),
                "endpointConfig": h.get("endpointConfig").cloned().unwrap_or(Value::Null),
            })
        })
        .collect();
    hosts.sort_by(|a, b| text(a, "/uid").cmp(text(b, "/uid")));
    let mut fields = json!({
        "endpointType": text(config, "/endpointType"),
        "kubernetesDistroType": text(config, "/kubernetesDistroType"),
        "limitConfig": config.get("limitConfig").cloned().unwrap_or(Value::Null),
        "values": values,
        "valuesHash": values_hash,
        "hostClusters": hosts,
    });
    if let Some(e) = values_parse_error {
        fields["valuesParseError"] = Value::String(e);
    }
    fields
}

/// Each role binding in `doc`, the answer of `GET /v1/spectroclusters/{uid}/config/rbacs`: its
/// `namespace`, `role`, `subjects`, and `type`. A binding is a set of subjects, and the cluster a
/// set of bindings, so both are in canonical order.
fn bindings(doc: &Value) -> Value {
    let mut out: Vec<Value> = items(doc)
        .filter_map(|item| item.pointer("/spec/bindings").and_then(Value::as_array))
        .flatten()
        .map(|b| {
            let mut binding = pick(b, &["namespace", "role", "type"]);
            let mut subjects: Vec<Value> = b
                .get("subjects")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|s| Value::Object(pick(s, &["name", "namespace", "type"])))
                .collect();
            sort_canonical(&mut subjects);
            binding.insert("subjects".to_string(), Value::Array(subjects));
            Value::Object(binding)
        })
        .collect();
    sort_canonical(&mut out);
    Value::Array(out)
}

/// Each namespace in `doc`, the answer of `GET /v1/spectroclusters/{uid}/config/namespaces`: its
/// `name`, `isRegex`, and `resourceAllocation`, in name order.
fn namespaces(doc: &Value) -> Value {
    let mut out: Vec<Value> = items(doc)
        .map(|n| {
            let spec = n.get("spec").unwrap_or(&Value::Null);
            let mut namespace = pick(spec, &["isRegex", "resourceAllocation"]);
            if let Some(name) = n.pointer("/metadata/name") {
                namespace.insert("name".to_string(), name.clone());
            }
            Value::Object(namespace)
        })
        .collect();
    out.sort_by_cached_key(|n| canonical_json(n.get("name").unwrap_or(&Value::Null)));
    Value::Array(out)
}

/// The entries of `items` of an answer of Palette.
fn items(doc: &Value) -> impl Iterator<Item = &Value> {
    doc.get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

/// The text at the pointer `ptr` of `v`, or empty when `v` has none.
fn text<'a>(v: &'a Value, ptr: &str) -> &'a str {
    v.pointer(ptr).and_then(Value::as_str).unwrap_or_default()
}

/// The keys of `v` that `keys` names, without a null value.
fn pick(v: &Value, keys: &[&str]) -> Map<String, Value> {
    keys.iter()
        .filter_map(|k| v.get(*k).map(|x| (k.to_string(), x.clone())))
        .filter(|(_, x)| !x.is_null())
        .collect()
}

/// Sort `values` by their canonical JSON, so the order of Palette does not change the hash.
fn sort_canonical(values: &mut [Value]) {
    values.sort_by_cached_key(canonical_json);
}

/// The policy fields `a` and `b` of two records, in a form that compares. When one of them is
/// from before strata read the bindings, each keeps only the fields that the two forms share.
pub(crate) fn comparable(a: &Value, b: &Value) -> (Value, Value) {
    let legacy = |v: &Value| v.get(LEGACY_RESOURCES).is_some();
    if legacy(a) || legacy(b) {
        let shared = |v: &Value| Value::Object(pick(v, LEGACY_FIELDS));
        (shared(a), shared(b))
    } else {
        (a.clone(), b.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_upgrades_of_the_agent_are_paused_only_when_the_annotation_is_true() {
        let with = |v: &str| json!({"metadata": {"annotations": {AGENT_UPGRADES_FORBIDDEN: v}}});
        assert_eq!(AgentUpgrades::of(&with("true")), AgentUpgrades::Paused);
        assert_eq!(AgentUpgrades::of(&with("false")), AgentUpgrades::Allowed);
        assert_eq!(AgentUpgrades::of(&json!({})), AgentUpgrades::Allowed);
    }

    #[test]
    fn the_bindings_drop_the_uids_and_have_one_order_whatever_order_palette_gives() {
        let doc = |subjects: Value| {
            json!({"items": [{"metadata": {"uid": "r1", "creationTimestamp": "t"},
                "spec": {"bindings": [{"namespace": "", "type": "ClusterRoleBinding",
                    "role": {"kind": "ClusterRole", "name": "view"}, "subjects": subjects}]}}]})
        };
        let a = bindings(&doc(json!([{"name": "fred", "type": "User"},
            {"name": "barney", "type": "User"}])));
        let b = bindings(&doc(json!([{"name": "barney", "type": "User"},
            {"name": "fred", "type": "User"}])));
        assert_eq!(a, b);
        assert_eq!(a[0]["subjects"][0]["name"], "barney");
        assert!(a[0].get("uid").is_none());
    }

    #[test]
    fn an_older_policy_tier_compares_on_the_shared_fields_and_a_group_tier_on_each_field() {
        let older = json!({"timezone": "Etc/UTC", LEGACY_RESOURCES: {"rbacs": [{"uid": "r1"}]}});
        let newer = json!({"timezone": "Etc/UTC", RBACS: [], "applySetting": "DownloadAndInstall"});
        let (a, b) = comparable(&older, &newer);
        assert_eq!(a, b);
        let (a, b) = comparable(
            &newer,
            &json!({"timezone": "Etc/UTC", RBACS: [{"type": "x"}]}),
        );
        assert_ne!(a, b);
        // two policy tiers of a cluster group compare on each field
        let group = |cpu: u32| json!({"endpointType": "LoadBalancer", "limitConfig": {"cpu": cpu}});
        let (a, b) = comparable(&group(6), &group(4));
        assert_ne!(a, b);
    }
}
