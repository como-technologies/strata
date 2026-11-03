//! The fields of the `policy` tier of a cluster record and of a cluster group record.
//!
//! The tier of a cluster holds the settings of the cluster in `spec.clusterConfig`, its role
//! bindings and its namespaces, and how it takes an update. The tier of a cluster group holds the
//! config that the group gives each virtual cluster.
//!
//! The tables [`CLUSTER_KEYS`], [`GROUP_KEYS`], and [`OLDER_KEYS`] hold each key of the tier. The
//! functions that write the fields and the schema of the tier both read them, so a key is in
//! both or in neither.

use std::borrow::Cow;
use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use super::cluster::unread;
use crate::api::RawCluster;
use crate::api::mask::{mask_group_values, mask_password_keys};
use crate::content::canon::{canon_yaml_hashed, canonical_json, sha256_hex};
use crate::error::Result;
use crate::json::{arr, flag, items, set_at, str_at, value_at, value_or_null};

/// The type that the schema gives to a key of the policy fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FieldType {
    /// Text.
    Text,
    /// True or false.
    Flag,
    /// A value of any type, as Palette gives it.
    Any,
    /// A list, or null when strata did not read it.
    ListOrNull,
    /// The host clusters of a cluster group: a list of objects with a `uid` and an
    /// `endpointConfig`.
    HostClusters,
}

/// How the function of the tier gets the value of a key of the policy fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Read {
    /// The JSON pointer in the config: `spec.clusterConfig` of a cluster, or `spec.clustersConfig`
    /// of a cluster group.
    Config(&'static str),
    /// The JSON pointer in the document of the cluster.
    Cluster(&'static str),
    /// A function of this module calculates the value: a binding, a namespace, an annotation, or
    /// the values of a cluster group.
    Computed,
    /// No release writes the key now: a record of an older release holds it.
    Gone,
}

/// One key of the policy fields: its text, what the schema says about it, and how the tier reads
/// its value.
#[derive(Debug, Clone, Copy)]
pub(super) struct PolicyKey {
    /// The key in the fields of the tier.
    pub(super) key: &'static str,
    /// The type that the schema gives to the key.
    pub(super) field_type: FieldType,
    /// How the function of the tier gets the value.
    pub(super) read: Read,
    /// The description in the schema.
    pub(super) about: &'static str,
    /// True when a record from before strata read the bindings holds the key. `compare` reads
    /// such a record on these keys alone. Do not set this flag on a new key.
    legacy: bool,
}

impl PolicyKey {
    const fn new(
        key: &'static str,
        field_type: FieldType,
        read: Read,
        about: &'static str,
    ) -> PolicyKey {
        PolicyKey {
            key,
            field_type,
            read,
            about,
            legacy: false,
        }
    }

    /// The same key, as a key that a record from before strata read the bindings holds.
    const fn legacy(mut self) -> PolicyKey {
        self.legacy = true;
        self
    }
}

/// The key of the policy fields that holds the role bindings.
const RBACS: &str = "rbacs";

/// The key of the policy fields that holds the namespaces.
const NAMESPACES: &str = "namespaces";

/// The annotation of a cluster that pauses the upgrades of the agent of Palette.
///
/// It is also the key of the policy field that holds its value as Palette gives it.
const AGENT_UPGRADES_FORBIDDEN: &str = "spectroComponentsUpgradeForbidden";

/// The key of the policy fields of a cluster group that holds its values.
const VALUES: &str = "values";

/// The key of the policy fields of a cluster group that holds the hash of its values.
const VALUES_HASH: &str = "valuesHash";

/// The key of the policy fields of a cluster group that holds why its values did not parse.
const VALUES_PARSE_ERROR: &str = "valuesParseError";

/// The key of the policy fields of a cluster group that holds its host clusters.
const HOST_CLUSTERS: &str = "hostClusters";

/// The key of the policy fields of a cluster record from before strata read the bindings: the
/// references to the bindings and the namespaces. The policy tier of a cluster group does not
/// have it either.
const LEGACY_RESOURCES: &str = "resources";

/// The key of the policy fields of a cluster record from strata 0.18: `allowed` or `paused`, which
/// strata found from the annotation. Do not use it in a new record: `compare` and `verify` find an
/// older record by it.
const LEGACY_AGENT_UPGRADES: &str = "agentUpgrades";

/// The keys of the policy fields of a cluster, in the order of the schema.
///
/// [`fields`] writes each key, but the annotation of the upgrades of the agent, which the tier
/// holds only when the cluster has it.
pub(super) const CLUSTER_KEYS: &[PolicyKey] = &[
    PolicyKey::new(
        "osPatch",
        FieldType::Any,
        Read::Config("/machineManagementConfig/osPatchConfig"),
        "The OS patch policy of the cluster, as Palette gives it, or null.",
    )
    .legacy(),
    PolicyKey::new(
        "lifecycle",
        FieldType::Any,
        Read::Config("/lifecycleConfig"),
        "Whether a person paused the cluster, as Palette gives it, or null.",
    )
    .legacy(),
    PolicyKey::new(
        "machineHealth",
        FieldType::Any,
        Read::Config("/machineHealthConfig"),
        "The machine health checks of the cluster, as Palette gives them, or null.",
    )
    .legacy(),
    PolicyKey::new(
        "timezone",
        FieldType::Text,
        Read::Config("/timezone"),
        "The timezone of the scheduled tasks of the cluster.",
    )
    .legacy(),
    PolicyKey::new(
        "fips",
        FieldType::Text,
        Read::Cluster("/status/fips/mode"),
        "The FIPS mode of the cluster.",
    )
    .legacy(),
    PolicyKey::new(
        RBACS,
        FieldType::ListOrNull,
        Read::Computed,
        "Each role binding of the cluster. Null when strata did not read them.",
    ),
    PolicyKey::new(
        NAMESPACES,
        FieldType::ListOrNull,
        Read::Computed,
        "Each namespace of the cluster. Null when strata did not read them.",
    ),
    PolicyKey::new(
        "updateWorkerPoolsInParallel",
        FieldType::Flag,
        // Palette omits the field when it is false
        Read::Config("/updateWorkerPoolsInParallel"),
        "True when Palette updates all the worker pools at one time.",
    ),
    PolicyKey::new(
        "controlPlaneHealthCheckTimeout",
        FieldType::Text,
        Read::Config("/controlPlaneHealthCheckTimeout"),
        "How long Palette waits for a new control plane node to become ready.",
    ),
    PolicyKey::new(
        "clusterMetaAttribute",
        FieldType::Text,
        Read::Config("/clusterMetaAttribute"),
        "The text that a person adds to the cluster as more data.",
    ),
    PolicyKey::new(
        "applySetting",
        FieldType::Text,
        Read::Cluster("/status/spcApply/actionType"),
        "When the cluster applies an update, as Palette gives it.",
    ),
    PolicyKey::new(
        AGENT_UPGRADES_FORBIDDEN,
        FieldType::Any,
        Read::Computed,
        "The value of the annotation, as Palette gives it. A cluster that has no such annotation has no such field.",
    ),
];

/// The keys of the policy fields of a cluster group, in the order of the schema: the config that
/// the group gives each virtual cluster. [`group_fields`] writes each key.
pub(super) const GROUP_KEYS: &[PolicyKey] = &[
    PolicyKey::new(
        "endpointType",
        FieldType::Text,
        Read::Config("/endpointType"),
        "The endpoint type of the virtual clusters of a cluster group.",
    ),
    PolicyKey::new(
        "kubernetesDistroType",
        FieldType::Text,
        Read::Config("/kubernetesDistroType"),
        "The Kubernetes distribution of the virtual clusters of a cluster group.",
    ),
    PolicyKey::new(
        "limitConfig",
        FieldType::Any,
        Read::Config("/limitConfig"),
        "The resource limits of the virtual clusters of a cluster group, as Palette gives them, or null.",
    ),
    PolicyKey::new(
        VALUES,
        FieldType::Any,
        Read::Computed,
        "The values that a cluster group applies to each virtual cluster, as JSON with sorted keys, or null when they do not parse.",
    ),
    PolicyKey::new(
        VALUES_HASH,
        FieldType::Text,
        Read::Computed,
        "The hash of the values of a cluster group, after the mask of the passwords.",
    ),
    PolicyKey::new(
        VALUES_PARSE_ERROR,
        FieldType::Text,
        Read::Computed,
        "Why the values of a cluster group did not parse.",
    ),
    PolicyKey::new(
        HOST_CLUSTERS,
        FieldType::HostClusters,
        Read::Computed,
        "The host clusters of a cluster group, sorted by uid.",
    ),
];

/// The keys that only a record of an older release holds. No release writes them now, but the
/// schema must accept a record that holds one.
pub(super) const OLDER_KEYS: &[PolicyKey] = &[
    PolicyKey::new(
        LEGACY_RESOURCES,
        FieldType::Any,
        Read::Gone,
        "A record of an older release: the references to the bindings and the namespaces.",
    ),
    PolicyKey::new(
        "rbac",
        FieldType::Any,
        Read::Gone,
        "A record of an older release: the role bindings.",
    ),
    PolicyKey::new(
        LEGACY_AGENT_UPGRADES,
        FieldType::Text,
        Read::Gone,
        "A record of strata 0.18: `allowed` or `paused`, from the annotation that pauses the upgrades.",
    ),
];

/// Each key of the policy tier: those of a cluster, those of a cluster group, and those of a
/// record of an older release.
pub(super) fn all_keys() -> impl Iterator<Item = &'static PolicyKey> {
    CLUSTER_KEYS.iter().chain(GROUP_KEYS).chain(OLDER_KEYS)
}

/// The policy fields that a record from before strata read the bindings holds, less the
/// references to the bindings and the namespaces. `compare` and `verify` read an older record
/// through them.
fn legacy_keys() -> impl Iterator<Item = &'static str> {
    CLUSTER_KEYS.iter().filter(|k| k.legacy).map(|k| k.key)
}

/// The value of each key of `keys` that reads a pointer: in `config` for [`Read::Config`], and in
/// `cluster` for [`Read::Cluster`]. A text is `""` and a flag is `false` when Palette omits the
/// field, and each other value is null.
fn pointer_fields(keys: &[PolicyKey], cluster: &Value, config: &Value) -> Map<String, Value> {
    let read = |doc: &Value, ptr: &str, field_type: FieldType| match field_type {
        FieldType::Text => Value::from(str_at(doc, ptr)),
        FieldType::Flag => Value::from(flag(doc, ptr)),
        _ => value_or_null(doc, ptr),
    };
    keys.iter()
        .filter_map(|k| {
            let value = match k.read {
                Read::Config(ptr) => read(config, ptr, k.field_type),
                Read::Cluster(ptr) => read(cluster, ptr, k.field_type),
                Read::Computed | Read::Gone => return None,
            };
            Some((k.key.to_string(), value))
        })
        .collect()
}

/// The fields of the policy tier of the cluster in `raw`, whose `spec.clusterConfig` is `config`.
/// The bindings and the namespaces come from their own routes, without the uids and the times.
/// Each one is null when strata did not read its route, and [`errors`] names each one that
/// Palette refused. The tier holds the annotation `spectroComponentsUpgradeForbidden` only when
/// the cluster has it.
pub(crate) fn fields(raw: &RawCluster, config: &Value) -> Value {
    let cluster = &raw.cluster;
    let mut fields = pointer_fields(CLUSTER_KEYS, cluster, config);
    fields.insert(
        RBACS.to_string(),
        raw.rbacs.as_ref().map_or(Value::Null, bindings),
    );
    fields.insert(
        NAMESPACES.to_string(),
        raw.namespaces.as_ref().map_or(Value::Null, namespaces),
    );
    if let Some(v) = agent_upgrades_forbidden(cluster) {
        fields.insert(AGENT_UPGRADES_FORBIDDEN.to_string(), v.clone());
    }
    Value::Object(fields)
}

/// The value of the annotation `spectroComponentsUpgradeForbidden` of the cluster document
/// `cluster`, as Palette gives it.
fn agent_upgrades_forbidden(cluster: &Value) -> Option<&Value> {
    cluster
        .pointer("/metadata/annotations")?
        .get(AGENT_UPGRADES_FORBIDDEN)
}

/// Each policy field of the cluster in `raw` that strata could not read, with why.
///
/// These are the role bindings and the namespaces, when Palette answered their route with an
/// error other than 404.
pub(crate) fn errors(raw: &RawCluster) -> BTreeMap<String, String> {
    unread([
        (RBACS.to_string(), &raw.errors.rbacs),
        (NAMESPACES.to_string(), &raw.errors.namespaces),
    ])
}

/// The fields of the policy tier of a cluster group, from its `spec.clustersConfig` in `config`:
/// the config that the group gives each virtual cluster.
///
/// The group applies `values` to each virtual cluster as YAML, as a pack applies its values. So
/// the tier holds the values in canonical form with their hash, and the host clusters sorted by
/// uid. The values hold `********` over each password, and the hash covers the masked values.
///
/// Some values have no canonical form, and the tier holds `values` as `null`. The hash then covers
/// the text, so it covers the text with the mask. A new password changes no hash. The record of the
/// saved directory has the hash of the record of the fetch.
///
/// # Errors
///
/// [`Error::Serialize`](crate::Error::Serialize) when the values have no canonical form and
/// strata cannot mask a password in them: see [`mask_group_values`].
pub(crate) fn group_fields(config: &Value) -> Result<Value> {
    let text = str_at(config, "/values");
    let (values, mut values_hash, values_parse_error) = canon_yaml_hashed(text);
    if values_parse_error.is_some() {
        values_hash = sha256_hex(mask_group_values(text)?.trim());
    }
    let mut hosts: Vec<Value> = arr(config, "/hostClustersConfig")
        .iter()
        .map(|h| {
            json!({
                "uid": str_at(h, "/clusterUid"),
                "endpointConfig": value_or_null(h, "/endpointConfig"),
            })
        })
        .collect();
    hosts.sort_by(|a, b| str_at(a, "/uid").cmp(str_at(b, "/uid")));
    let mut fields = pointer_fields(GROUP_KEYS, &Value::Null, config);
    fields.insert(VALUES.to_string(), json!(values));
    fields.insert(VALUES_HASH.to_string(), json!(values_hash));
    fields.insert(HOST_CLUSTERS.to_string(), Value::Array(hosts));
    if let Some(e) = values_parse_error {
        fields.insert(VALUES_PARSE_ERROR.to_string(), Value::String(e));
    }
    let mut fields = Value::Object(fields);
    mask_group_fields(&mut fields);
    Ok(fields)
}

/// Write `********` over each password in `values` of the policy fields `fields` of a cluster
/// group, and make `valuesHash` the hash of the masked values. Fields with no values, or with
/// values that did not parse, do not change.
fn mask_group_fields(fields: &mut Value) {
    let Some(values) = fields.get_mut(VALUES).filter(|v| !v.is_null()) else {
        return;
    };
    mask_password_keys(values);
    let hash = sha256_hex(&canonical_json(values));
    set_at(fields, &[VALUES_HASH], Value::String(hash));
}

/// Each role binding in `doc`, the answer of `GET /v1/spectroclusters/{uid}/config/rbacs`: its
/// `namespace`, `role`, `subjects`, and `type`. A binding is a set of subjects, and the cluster a
/// set of bindings, so both are in canonical order.
fn bindings(doc: &Value) -> Value {
    let mut out: Vec<Value> = items(doc)
        .flat_map(|item| arr(item, "/spec/bindings"))
        .map(|b| {
            let mut binding = pick(b, ["namespace", "role", "type"]);
            let mut subjects: Vec<Value> = arr(b, "/subjects")
                .iter()
                .map(|s| Value::Object(pick(s, ["name", "namespace", "type"])))
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
            let spec = value_at(n, "/spec");
            let mut namespace = pick(spec, ["isRegex", "resourceAllocation"]);
            if let Some(name) = n.pointer("/metadata/name") {
                namespace.insert("name".to_string(), name.clone());
            }
            Value::Object(namespace)
        })
        .collect();
    out.sort_by_cached_key(|n| canonical_json(value_at(n, "/name")));
    Value::Array(out)
}

/// The keys of `v` that `keys` names, without a null value.
fn pick<'k>(v: &Value, keys: impl IntoIterator<Item = &'k str>) -> Map<String, Value> {
    keys.into_iter()
        .filter_map(|k| v.get(k).map(|x| (k.to_string(), x.clone())))
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
/// upgrades of the agent. The values of a cluster group get the mask on each side, because a
/// record of strata 0.19 or before holds each password in clear. A tier with no values to mask
/// compares as it is, so the fields are not copied.
pub(crate) fn comparable<'a>(a: &'a Value, b: &'a Value) -> (Cow<'a, Value>, Cow<'a, Value>) {
    let legacy = |v: &Value| v.get(LEGACY_RESOURCES).is_some();
    let legacy_agent = |v: &Value| v.get(LEGACY_AGENT_UPGRADES).is_some();
    if legacy(a) || legacy(b) {
        let shared = |v: &Value| Cow::Owned(Value::Object(pick(v, legacy_keys())));
        (shared(a), shared(b))
    } else if legacy_agent(a) != legacy_agent(b) {
        (
            Cow::Owned(without_agent_upgrades(a)),
            Cow::Owned(without_agent_upgrades(b)),
        )
    } else {
        (masked(a), masked(b))
    }
}

/// The policy fields `v` with the mask on the values of a cluster group. The fields of a cluster
/// have no values, and stay as they are.
fn masked(v: &Value) -> Cow<'_, Value> {
    if v.get(VALUES).is_none_or(Value::is_null) {
        return Cow::Borrowed(v);
    }
    let mut v = v.clone();
    mask_group_fields(&mut v);
    Cow::Owned(v)
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
    use std::collections::BTreeSet;

    use crate::api::ReadErrors;

    /// The keys of the object `fields`.
    fn keys_of(fields: &Value) -> BTreeSet<&str> {
        fields
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect()
    }

    /// The keys of `table`.
    fn keys_in(table: &[PolicyKey]) -> BTreeSet<&str> {
        table.iter().map(|k| k.key).collect()
    }

    #[test]
    fn the_fields_of_a_cluster_have_each_key_of_the_table_and_no_other() {
        let raw = RawCluster {
            cluster: json!({"metadata": {"annotations": {AGENT_UPGRADES_FORBIDDEN: "true"}}}),
            manifests: json!([]),
            variables: json!([]),
            variables_unavailable: false,
            project_uid: None,
            fetched_at: None,
            notifications: None,
            repave: None,
            resolved_values: None,
            cloud_config: None,
            rbacs: Some(json!({"items": []})),
            namespaces: Some(json!({"items": []})),
            errors: ReadErrors::default(),
        };
        let written = fields(&raw, &json!({}));
        assert_eq!(keys_of(&written), keys_in(CLUSTER_KEYS));

        // the annotation is the one key that a cluster can lack
        let mut plain = raw.clone();
        plain.cluster = json!({});
        let mut without = keys_in(CLUSTER_KEYS);
        without.remove(AGENT_UPGRADES_FORBIDDEN);
        assert_eq!(keys_of(&fields(&plain, &json!({}))), without);
    }

    #[test]
    fn the_fields_of_a_group_have_each_key_of_the_table_and_the_parse_error_only_for_broken_values()
    {
        let good = group_fields(&json!({"values": "a: 1\n"})).unwrap();
        let mut without = keys_in(GROUP_KEYS);
        without.remove(VALUES_PARSE_ERROR);
        assert_eq!(keys_of(&good), without);
        let broken = group_fields(&json!({"values": "a: [1"})).unwrap();
        assert_eq!(keys_of(&broken), keys_in(GROUP_KEYS));
    }

    #[test]
    fn a_key_is_in_one_table_only_and_the_five_older_fields_are_the_keys_that_compare_reads() {
        let all: Vec<&str> = all_keys().map(|k| k.key).collect();
        let distinct: BTreeSet<&str> = all.iter().copied().collect();
        assert_eq!(all.len(), distinct.len(), "{all:?}");
        assert_eq!(
            legacy_keys().collect::<Vec<_>>(),
            ["osPatch", "lifecycle", "machineHealth", "timezone", "fips"]
        );
        for key in OLDER_KEYS {
            assert_eq!(key.read, Read::Gone, "{}", key.key);
        }
    }

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
        let current = json!({"timezone": "Etc/UTC", RBACS: []});
        let (a, b) = comparable(&older, &current);
        assert_eq!(a, b, "no annotation on the cluster now");
        let other_zone = json!({"timezone": "UTC", RBACS: []});
        let (a, b) = comparable(&older, &other_zone);
        assert_ne!(a, b, "each other field still compares");
        // two records of strata 0.18 compare on each field
        let allowed = json!({"timezone": "Etc/UTC", RBACS: [], LEGACY_AGENT_UPGRADES: "allowed"});
        let (a, b) = comparable(&older, &allowed);
        assert_ne!(a, b);
    }

    #[test]
    fn the_policy_tier_of_a_group_masks_each_password_and_hashes_the_masked_values() {
        let config = |password: &str| {
            json!({"endpointType": "LoadBalancer",
                "values": format!("# the admin\nsync:\n  adminPassword: {password}\n  replicas: 1\n")})
        };
        let fields = group_fields(&config("s3cr3t")).unwrap();
        let masked = json!({"sync": {"adminPassword": "********", "replicas": 1}});
        assert_eq!(fields["values"], masked);
        assert_eq!(
            fields["valuesHash"],
            Value::String(sha256_hex(&canonical_json(&masked)))
        );
        assert!(!canonical_json(&fields).contains("s3cr3t"));
        // the hash does not show a new password: the record holds no trace of it
        assert_eq!(fields, group_fields(&config("an0ther")).unwrap());
        // values that do not parse keep no values
        let broken = group_fields(&json!({"values": "a: [1"})).unwrap();
        assert_eq!(broken["values"], Value::Null);
        assert!(broken.get("valuesParseError").is_some());
    }

    #[test]
    fn values_with_no_canonical_form_hash_the_text_with_the_mask_and_not_the_password() {
        // an unquoted template is a map in a key, so the values have no canonical form
        let config = |password: &str| {
            json!({"endpointType": "LoadBalancer",
                "values": format!("password: {password}\nhost: {{{{ .spectro.var.host }}}}\n")})
        };
        let fields = group_fields(&config("s3cr3t")).unwrap();
        assert_eq!(fields["values"], Value::Null);
        assert!(fields.get("valuesParseError").is_some());
        let masked = "password: '********'\nhost: {{ .spectro.var.host }}";
        assert_eq!(fields["valuesHash"], Value::String(sha256_hex(masked)));
        // a new password changes no hash, so it changes no tier
        assert_eq!(fields, group_fields(&config("an0ther")).unwrap());
        // the text that the saved directory holds has the same hash as the text of the fetch
        let saved = mask_group_values(str_at(&config("s3cr3t"), "/values")).unwrap();
        let from_directory =
            group_fields(&json!({"endpointType": "LoadBalancer", "values": saved}));
        assert_eq!(from_directory.unwrap(), fields);
        // each other change of the text still changes the hash
        let other = json!({"endpointType": "LoadBalancer",
            "values": "password: s3cr3t\nhost: {{ .spectro.var.other }}\n"});
        assert_ne!(group_fields(&other).unwrap(), fields);
    }

    /// Review of Wave 14, D3. Strata 0.27.0 took the hash of the trimmed text for values with no
    /// canonical form. Such values with no password keep that hash, so the book names a change for
    /// a group with a password only.
    #[test]
    fn values_with_no_canonical_form_and_no_password_keep_the_hash_of_the_trimmed_text() {
        let text = "host: {{ .spectro.var.host }}\nreplicas: 1\n";
        let fields =
            group_fields(&json!({"endpointType": "LoadBalancer", "values": text})).unwrap();
        assert_eq!(fields["values"], Value::Null);
        assert!(fields.get("valuesParseError").is_some());
        assert_eq!(fields["valuesHash"], Value::String(sha256_hex(text.trim())));
        // the hash that the parser gives for the text is the hash of 0.27.0
        assert_eq!(fields["valuesHash"], json!(canon_yaml_hashed(text).1));

        // a password in the same values moves the hash
        let with = format!("password: s3cr3t\n{text}");
        let masked = group_fields(&json!({"values": with})).unwrap();
        assert_ne!(masked["valuesHash"], json!(canon_yaml_hashed(&with).1));
    }

    #[test]
    fn values_with_a_password_that_strata_cannot_mask_give_no_hash_and_an_error() {
        // a block scalar has no line to change, and strata cannot write a key that is a map again
        let config =
            json!({"values": "adminPassword: |\n  s3cr3t\nhost: {{ .spectro.var.host }}\n"});
        assert!(matches!(
            group_fields(&config),
            Err(crate::Error::Serialize(_))
        ));
    }
    #[test]
    fn an_older_group_tier_with_a_password_in_clear_compares_with_a_masked_one() {
        let config = json!({"endpointType": "LoadBalancer",
            "values": "sync:\n  adminPassword: s3cr3t\n  replicas: 1\n"});
        let newer = group_fields(&config).unwrap();
        // a record of strata 0.19 or before kept the values in clear, with their hash
        let clear = json!({"sync": {"adminPassword": "s3cr3t", "replicas": 1}});
        let mut older = newer.clone();
        older["values"] = clear.clone();
        older["valuesHash"] = Value::String(sha256_hex(&canonical_json(&clear)));
        assert_ne!(older, newer);
        let (a, b) = comparable(&older, &newer);
        assert_eq!(a, b);
        assert!(!canonical_json(&a).contains("s3cr3t"));
        // each other value still compares
        let mut more = config.clone();
        more["values"] = json!("sync:\n  adminPassword: s3cr3t\n  replicas: 2\n");
        let changed = group_fields(&more).unwrap();
        let (a, b) = comparable(&older, &changed);
        assert_ne!(a, b);
    }

    #[test]
    fn the_policy_fields_of_a_cluster_compare_as_they_are_and_those_of_a_group_in_a_masked_copy() {
        let cluster = json!({"timezone": "Etc/UTC", RBACS: []});
        let (a, b) = comparable(&cluster, &cluster);
        assert!(matches!((a, b), (Cow::Borrowed(_), Cow::Borrowed(_))));
        let group = group_fields(&json!({"values": "sync:\n  adminPassword: s3cr3t\n"})).unwrap();
        let (a, b) = comparable(&group, &group);
        assert!(matches!((a, b), (Cow::Owned(_), Cow::Owned(_))));
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
        let bound = json!({"timezone": "Etc/UTC", RBACS: [{"type": "x"}]});
        let (a, b) = comparable(&newer, &bound);
        assert_ne!(a, b);
        // two policy tiers of a cluster group compare on each field
        let group = |cpu: u32| json!({"endpointType": "LoadBalancer", "limitConfig": {"cpu": cpu}});
        let (six, four) = (group(6), group(4));
        let (a, b) = comparable(&six, &four);
        assert_ne!(a, b);
    }
}
