//! The fields of the `policy` tier of a cluster record and of a cluster group record.
//!
//! The tier of a cluster holds the settings of the cluster in `spec.clusterConfig`, its role
//! bindings and its namespaces, and how it takes an update. The tier of a cluster group holds the
//! config that the group gives each virtual cluster.

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

/// The annotation of a cluster that pauses the upgrades of the agent of Palette, and the key of the
/// policy field that holds its value as Palette gives it.
const AGENT_UPGRADES_FORBIDDEN: &str = "spectroComponentsUpgradeForbidden";

/// The key of the policy fields of a cluster record from strata 0.18: `allowed` or `paused`, which
/// strata found from the annotation. Do not use it in a new record: `compare` and `verify` find an
/// older record by it.
const LEGACY_AGENT_UPGRADES: &str = "agentUpgrades";

/// The fields of the policy tier of the cluster in `raw`, whose `spec.clusterConfig` is `config`.
/// The bindings and the namespaces come from their own routes, without the uids and the times.
/// Each one is null when strata did not read its route. The tier holds the annotation
/// `spectroComponentsUpgradeForbidden` only when the cluster has it.
pub(crate) fn fields(raw: &RawCluster, config: &Value) -> Value {
    let cluster = &raw.cluster;
    let field = |k: &str| config.get(k).cloned().unwrap_or(Value::Null);
    let mut fields = json!({
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
    });
    if let Some(v) = agent_upgrades_forbidden(cluster) {
        fields[AGENT_UPGRADES_FORBIDDEN] = v.clone();
    }
    fields
}

/// The value of the annotation `spectroComponentsUpgradeForbidden` of the cluster document
/// `cluster`, as Palette gives it.
fn agent_upgrades_forbidden(cluster: &Value) -> Option<&Value> {
    cluster
        .pointer("/metadata/annotations")?
        .get(AGENT_UPGRADES_FORBIDDEN)
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
/// from before strata read the bindings, each keeps only the fields that the two forms share. When
/// only one of them is from strata 0.18, each keeps all the fields less the two forms of the
/// upgrades of the agent.
pub(crate) fn comparable(a: &Value, b: &Value) -> (Value, Value) {
    let legacy = |v: &Value| v.get(LEGACY_RESOURCES).is_some();
    let legacy_agent = |v: &Value| v.get(LEGACY_AGENT_UPGRADES).is_some();
    if legacy(a) || legacy(b) {
        let shared = |v: &Value| Value::Object(pick(v, LEGACY_FIELDS));
        (shared(a), shared(b))
    } else if legacy_agent(a) != legacy_agent(b) {
        (without_agent_upgrades(a), without_agent_upgrades(b))
    } else {
        (a.clone(), b.clone())
    }
}

/// The policy fields `v` less the two forms of the upgrades of the agent: `agentUpgrades` of strata
/// 0.18, and the annotation `spectroComponentsUpgradeForbidden`.
fn without_agent_upgrades(v: &Value) -> Value {
    let mut v = v.clone();
    if let Some(m) = v.as_object_mut() {
        m.remove(LEGACY_AGENT_UPGRADES);
        m.remove(AGENT_UPGRADES_FORBIDDEN);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_annotation_of_the_upgrades_of_the_agent_is_the_text_of_palette() {
        let with = |v: &str| json!({"metadata": {"annotations": {AGENT_UPGRADES_FORBIDDEN: v}}});
        for v in ["true", "false", "True", ""] {
            let doc = with(v);
            assert_eq!(agent_upgrades_forbidden(&doc), Some(&json!(v)));
        }
        let none = json!({"metadata": {"annotations": {"projectUid": "p1"}}});
        assert_eq!(agent_upgrades_forbidden(&none), None);
        assert_eq!(agent_upgrades_forbidden(&json!({})), None);
    }

    #[test]
    fn a_policy_tier_of_strata_0_18_compares_with_a_newer_one_on_each_field_less_the_agent_upgrades()
     {
        let older = json!({"timezone": "Etc/UTC", RBACS: [], LEGACY_AGENT_UPGRADES: "paused"});
        let newer = json!({"timezone": "Etc/UTC", RBACS: [], AGENT_UPGRADES_FORBIDDEN: "true"});
        let (a, b) = comparable(&older, &newer);
        assert_eq!(a, b);
        let (a, b) = comparable(&older, &json!({"timezone": "Etc/UTC", RBACS: []}));
        assert_eq!(a, b, "no annotation on the cluster now");
        let (a, b) = comparable(&older, &json!({"timezone": "UTC", RBACS: []}));
        assert_ne!(a, b, "each other field still compares");
        // two records of strata 0.18 compare on each field
        let allowed = json!({"timezone": "Etc/UTC", RBACS: [], LEGACY_AGENT_UPGRADES: "allowed"});
        let (a, b) = comparable(&older, &allowed);
        assert_ne!(a, b);
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
