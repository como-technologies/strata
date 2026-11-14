//! The tests of the fields of the policy tier: the tables of the keys, the masks of a cluster group,
//! the comparison of an older tier, and the role bindings.

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

/// The raw documents of the cluster `cluster`, with the role bindings `rbacs` and no namespace.
fn raw_cluster(cluster: Value, rbacs: Value) -> RawCluster {
    RawCluster {
        cluster,
        manifests: json!([]),
        variables: json!([]),
        variables_unavailable: false,
        project_uid: None,
        fetched_at: None,
        notifications: None,
        repave: None,
        resolved_values: None,
        cloud_config: None,
        rbacs: Some(rbacs),
        namespaces: Some(json!({"items": []})),
        errors: ReadErrors::default(),
    }
}

#[test]
fn the_fields_of_a_cluster_have_each_key_of_the_table_and_no_other() {
    let raw = raw_cluster(
        json!({"metadata": {"annotations": {AGENT_UPGRADES_FORBIDDEN: "true"}}}),
        json!({"items": []}),
    );
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
fn the_fields_of_a_group_have_each_key_of_the_table_and_the_parse_error_only_for_broken_values() {
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
fn a_policy_tier_of_strata_0_18_compares_with_a_newer_one_on_each_field_less_the_agent_upgrades() {
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
    let from_directory = group_fields(&json!({"endpointType": "LoadBalancer", "values": saved}));
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
    let fields = group_fields(&json!({"endpointType": "LoadBalancer", "values": text})).unwrap();
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
    let config = json!({"values": "adminPassword: |\n  s3cr3t\nhost: {{ .spectro.var.host }}\n"});
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
    let a = bindings(
        &doc(json!([{"name": "fred", "type": "User"},
        {"name": "barney", "type": "User"}])),
        false,
    );
    let b = bindings(
        &doc(json!([{"name": "barney", "type": "User"},
        {"name": "fred", "type": "User"}])),
        false,
    );
    assert_eq!(a, b);
    assert_eq!(a[0]["subjects"][0]["name"], "barney");
    assert!(a[0].get("uid").is_none());
}

#[test]
fn the_role_bindings_of_the_system_leave_the_fields_and_the_hash_of_the_tier() {
    let item = |system: bool, role: &str, user: &str| {
        json!({"spec": {"isSystem": system, "bindings": [{"type": "ClusterRoleBinding",
            "namespace": "", "role": {"kind": "ClusterRole", "name": role},
            "subjects": [{"type": "User", "name": user, "namespace": ""}]}]}})
    };
    // Palette answers the binding of the system with the user of each key as its subject
    let raw = |user: &str| {
        let answer = json!({"items": [item(true, "view", user),
            item(false, "edit", "barney@example.com")]});
        raw_cluster(json!({}), answer)
    };
    let (fred, wilma) = (raw("fred@example.com"), raw("wilma@example.com"));
    assert_eq!(fields(&fred, &json!({})), fields(&wilma, &json!({})));
    let rbacs = &fields(&fred, &json!({}))[RBACS];
    assert_eq!(rbacs.as_array().unwrap().len(), 1);
    assert_eq!(rbacs[0]["role"]["name"], "edit");

    let system = system_rbacs(&fred);
    assert_eq!(system.len(), 1);
    assert_eq!(system[0]["role"]["name"], "view");
    assert_eq!(system[0]["subjects"][0]["name"], "fred@example.com");
    assert_ne!(system, system_rbacs(&wilma));

    // strata did not read the bindings
    let mut unread = fred.clone();
    unread.rbacs = None;
    assert!(system_rbacs(&unread).is_empty());
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
