//! The tests of the masks of a secret: the values of a pack, of a cluster group, and of an answer.

use super::*;
use serde_json::json;

fn fields(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| n.to_string()).collect()
}

#[test]
fn each_value_whose_key_names_a_password_gets_the_mask_at_each_depth() {
    let mut values = json!({
        "password": "plain",
        "sync": {"adminPassword": "s3cr3t", "user": "barney", "pin": 1234},
        "users": [{"name": "fred", "PASSWORD": "pebbles"}],
        "passwordSecretName": "vc-admin",
        "rootPassword": "",
        "dbPassword": "{{ .spectro.var.db_password }}",
        "tlsPassword": {"from": "secret"},
        "keyPassword": ["a"],
    });
    mask_password_keys(&mut values);
    assert_eq!(
        values,
        json!({
            "password": MASKED,
            "sync": {"adminPassword": MASKED, "user": "barney", "pin": 1234},
            "users": [{"name": "fred", "PASSWORD": MASKED}],
            "passwordSecretName": "vc-admin",
            "rootPassword": "",
            "dbPassword": "{{ .spectro.var.db_password }}",
            "tlsPassword": {"from": "secret"},
            "keyPassword": ["a"],
        })
    );
    // a second mask changes nothing
    let once = values.clone();
    mask_password_keys(&mut values);
    assert_eq!(values, once);
}

#[test]
fn the_values_of_a_group_get_the_mask_of_the_record_and_keep_each_other_value() {
    let values = "# the admin\nsync:\n  adminPassword: s3cr3t\n  replicas: 1\n\
                  users:\n- name: fred\n  password: pebbles\n";
    let masked = mask_group_values(values).unwrap();
    assert!(
        !masked.contains("s3cr3t") && !masked.contains("pebbles"),
        "{masked}"
    );
    let Canon::Parsed(got) = canon_yaml(&masked) else {
        panic!("the masked values are YAML: {masked}");
    };
    let Canon::Parsed(mut want) = canon_yaml(values) else {
        panic!("the values are YAML");
    };
    mask_password_keys(&mut want);
    assert_eq!(got, want);
    // a second mask changes nothing
    assert_eq!(mask_group_values(&masked).unwrap(), masked);
    // values with no password, and values that are not YAML, keep their text
    let plain = "# a comment\nsync:\n  replicas: 1\n";
    assert_eq!(mask_group_values(plain).unwrap(), plain);
    let broken = "password: [s3cr3t\n";
    assert_eq!(mask_group_values(broken).unwrap(), broken);
    assert_eq!(mask_group_values("").unwrap(), "");
}

/// The parsed YAML of `text`: each document, and each key as the parser reads it.
fn documents(text: &str) -> Vec<Yaml> {
    yaml_documents(text)
        .collect::<std::result::Result<_, _>>()
        .unwrap_or_else(|e| panic!("{e}: {text}"))
}

#[test]
fn a_template_with_no_quotes_does_not_keep_a_password_in_clear() {
    // the template is a map in a key, so the values have no canonical form
    let values = "# the host\npassword: s3cr3t # keep it\nhost: {{ .spectro.var.host }}\n";
    assert!(matches!(canon_yaml(values), Canon::Unparseable(_)));
    let masked = mask_group_values(values).unwrap();
    // only the line of the password changes
    assert_eq!(
        masked,
        "# the host\npassword: '********'\nhost: {{ .spectro.var.host }}\n"
    );
    let mut want = documents(values);
    want.iter_mut().for_each(mask_password_keys);
    assert_eq!(documents(&masked), want);
    // the key stays a map, so a second mask changes nothing
    assert!(matches!(canon_yaml(&masked), Canon::Unparseable(_)));
    assert_eq!(mask_group_values(&masked).unwrap(), masked);
}

#[test]
fn a_password_that_strata_cannot_mask_in_values_with_no_canonical_form_is_an_error() {
    // a block scalar has no line to change, and a parser cannot write a key that is a map
    let values = "adminPassword: |\n  s3cr3t\nhost: {{ .spectro.var.host }}\n";
    assert!(matches!(
        mask_group_values(values),
        Err(Error::Serialize(_))
    ));
    // a key that is a list is written again
    let values = "adminPassword: |\n  s3cr3t\n? [a]\n: b\n";
    let masked = mask_group_values(values).unwrap();
    assert!(!masked.contains("s3cr3t"), "{masked}");
    assert_eq!(
        documents(&masked)[0]["adminPassword"],
        Yaml::String(MASKED.into())
    );
}

#[test]
fn a_key_that_is_not_text_does_not_hide_a_password() {
    let masked = mask_group_values("? [a]\n: {adminPassword: pw}\nPASSWORD: pw\n").unwrap();
    assert!(!masked.contains("pw"), "{masked}");
    let want = "? - a\n: adminPassword: '********'\nPASSWORD: '********'\n";
    assert_eq!(masked, want);
    // the tag of a key or of a value does not hide it, as in the canonical form
    let tagged = "? [a]\n: b\n!!str password: pw\ntoken: x\nsecretPassword: !!str pw\n";
    let masked = mask_group_values(tagged).unwrap();
    assert!(!masked.contains("pw"), "{masked}");
    assert!(masked.contains("token: x"), "{masked}");
}

#[test]
fn each_document_of_values_that_have_no_canonical_form_gets_the_mask() {
    let values = "password: a\n---\n? [b]\n: c\nadminPassword: d\n---\nname: e\n";
    let masked = mask_group_values(values).unwrap();
    assert!(
        !masked.contains(": a") && !masked.contains(": d"),
        "{masked}"
    );
    let docs = documents(&masked);
    assert_eq!(docs.len(), 3);
    assert_eq!(docs[0]["password"], Yaml::String(MASKED.into()));
    assert_eq!(docs[2]["name"], Yaml::String("e".into()));
}

#[test]
fn values_that_have_no_canonical_form_and_no_password_keep_their_text() {
    // a variable, an empty value, a map, and a list are no secret
    let values = "# a comment\nhost: {{ .spectro.var.host }}\nadminPassword: \"{{ .spectro.var.p }}\"\n\
                  password: ''\ntlsPassword:\n  from: secret\n";
    assert_eq!(mask_group_values(values).unwrap(), values);
    let no_key = "host: {{ .spectro.var.host }}\nreplicas: 1\n";
    assert_eq!(mask_group_values(no_key).unwrap(), no_key);
    // text that is no YAML stays, with its password
    let broken = "password: [s3cr3t\nhost: {{ .spectro.var.host }}\n";
    assert_eq!(mask_group_values(broken).unwrap(), broken);
}

#[test]
fn a_password_under_each_kind_of_key_gets_the_mask_in_a_json_and_in_a_yaml_document() {
    let yaml: Yaml =
        serde_yaml::from_str("a:\n  - password: x\n  - n: {adminPassword: y}\nb: z\n").unwrap();
    let json = json!({"a": [{"password": "x"}, {"n": {"adminPassword": "y"}}], "b": "z"});
    let (mut yaml, mut json) = (yaml, json);
    mask_password_keys(&mut yaml);
    mask_password_keys(&mut json);
    assert_eq!(serde_yaml::to_value(&json).unwrap(), yaml);
    assert_eq!(json["a"][1]["n"]["adminPassword"], MASKED);
}

#[test]
fn a_password_format_is_a_secret() {
    assert!(is_password_format("${password}"));
    assert!(is_password_format("${password:/^.{8,}$/}"));
    assert!(is_password_format("${string} ${password}"));
    assert!(!is_password_format("${string}"));
    assert!(!is_password_format("${passwords}"));
    assert!(!is_password_format("password"));
    assert!(!is_password_format(""));
}

#[test]
fn the_fields_come_from_the_schema_of_the_pack() {
    let pack = json!({"packValues": [
        {"packUid": "p1", "schema": [
            {"name": "charts.app.adminPassword", "format": "${password}"},
            {"name": "charts.app.user", "format": "${string}"}]},
        {"packUid": "p2", "schema": [{"name": "other", "format": "${password}"}]}]});
    assert_eq!(
        password_fields(&pack, "p1"),
        fields(&["charts.app.adminPassword"])
    );
    assert!(password_fields(&pack, "p3").is_empty());
    assert!(password_fields(&json!({}), "p1").is_empty());
}

#[test]
fn only_the_line_of_the_secret_changes() {
    let values = "# the app\ncharts:\n  app:\n    user: admin # the login\n    \
                  adminPassword: s3cret # keep it safe\n    replicas: 2\n";
    let got = mask_values(values, &fields(&["charts.app.adminPassword"])).unwrap();
    assert_eq!(
        got,
        "# the app\ncharts:\n  app:\n    user: admin # the login\n    \
         adminPassword: '********'\n    replicas: 2\n"
    );
    assert!(!got.contains("s3cret"));
}

#[test]
fn a_key_with_the_same_name_in_another_map_stays() {
    let values = "other:\n  password: plain\ncharts:\n  password: s3cret\n";
    let got = mask_values(values, &fields(&["charts.password"])).unwrap();
    assert_eq!(
        got,
        "other:\n  password: plain\ncharts:\n  password: '********'\n"
    );
}

#[test]
fn palette_masks_no_empty_value_no_variable_and_no_map() {
    let values = "a: ''\nb: '{{ .spectro.var.db_password }}'\nc:\n  d: 1\ne: null\n";
    let names = fields(&["a", "b", "c", "e", "missing.key"]);
    assert_eq!(mask_values(values, &names).unwrap(), values);
}

#[test]
fn a_number_or_a_quoted_key_is_masked() {
    let got = mask_values("\"pin\": 1234\nflag: true\n", &fields(&["pin", "flag"])).unwrap();
    assert_eq!(got, "\"pin\": '********'\nflag: '********'\n");
}

#[test]
fn a_block_scalar_is_masked_through_the_parsed_yaml() {
    let values = "# comment\nkey: |\n  line one\n  line two\nother: x\n";
    let got = mask_values(values, &fields(&["key"])).unwrap();
    assert!(!got.contains("line one"), "{got}");
    let doc: Yaml = serde_yaml::from_str(&got).unwrap();
    assert_eq!(doc["key"], Yaml::String(MASKED.into()));
    assert_eq!(doc["other"], Yaml::String("x".into()));
}

#[test]
fn the_mask_covers_the_same_scalars_of_a_yaml_and_of_a_json_document() {
    let yaml: Yaml = serde_yaml::from_str(
        "s: text\nn: 1234\nb: true\ne: ''\nv: '{{ .spectro.var.x }}'\nm:\n  k: v\nl: [a]\nz: null\n",
    )
    .unwrap();
    let json = json!({
        "s": "text", "n": 1234, "b": true, "e": "", "v": "{{ .spectro.var.x }}",
        "m": {"k": "v"}, "l": ["a"], "z": null
    });
    for (key, secret) in [
        ("s", true),
        ("n", true),
        ("b", true),
        ("e", false),
        ("v", false),
        ("m", false),
        ("l", false),
        ("z", false),
    ] {
        assert_eq!(holds_secret(&yaml[key]), secret, "yaml {key}");
        assert_eq!(holds_secret(&json[key]), secret, "json {key}");
    }
}

#[test]
fn only_a_variable_of_the_profile_stops_the_mask() {
    assert!(holds_variable("{{ .spectro.var.db_password }}"));
    assert!(holds_variable("x-{{.spectro.var.x}}-y"));
    assert!(!holds_variable("{{ .spectroXvar.x }}"));
    assert!(!holds_variable("{{ .spectro.varX }}"));
    assert!(!holds_variable("{{ .spectro.var }}"));
    assert!(!holds_variable("{{ .spectro.var.x"));
    assert!(!holds_variable(".spectro.var.x"));
}

#[test]
fn a_character_of_more_than_one_byte_after_spectro_does_not_panic() {
    assert!(!holds_variable("{{ .spectro\u{e9}var.x }}"));
    assert!(!holds_variable("{{ .spectro\u{1f600} }}"));
    let values = "password: '{{ .spectro\u{e9}var.x }}'\n";
    assert_eq!(
        mask_values(values, &fields(&["password"])).unwrap(),
        "password: '********'\n"
    );
}

#[test]
fn the_draft_packs_and_the_served_packs_are_masked_by_uid() {
    let values = "password: s3cret\nuser: fred\n";
    let mut profile = json!({"spec": {"draft": {"packs": [
        {"packUid": "barney+1", "type": "spectro", "values": values},
        {"packUid": "fred", "type": "spectro", "values": values},
        {"packUid": "", "type": "spectro", "values": values},
        {"packUid": "m1", "type": "manifest", "values": values}]}}});
    let mut manifests = json!({"spec": {"packs": [
        {"metadata": {"uid": "barney+1"}, "spec": {"values": values}},
        {"metadata": {"uid": "fred"}, "spec": {"values": values}}]}});
    assert_eq!(draft_pack_uids(&profile), fields(&["barney+1", "fred"]));
    let secrets = BTreeMap::from([("barney+1".to_string(), fields(&["password"]))]);
    mask_draft_packs(&mut profile, &mut manifests, &secrets).unwrap();
    let masked = "password: '********'\nuser: fred\n";
    let draft = &profile["spec"]["draft"]["packs"];
    assert_eq!(draft[0]["values"], masked);
    assert_eq!(draft[1]["values"], values);
    assert_eq!(draft[2]["values"], values);
    assert_eq!(manifests["spec"]["packs"][0]["spec"]["values"], masked);
    assert_eq!(manifests["spec"]["packs"][1]["spec"]["values"], values);
}

#[test]
fn values_that_are_not_yaml_stay() {
    let values = "a: [unclosed\npassword: s3cret";
    assert_eq!(mask_values(values, &fields(&["password"])).unwrap(), values);
}

#[test]
fn the_mask_of_a_secret_takes_each_whole_copy_and_leaves_a_part_of_a_word() {
    let key = format!("strata-{}", "0123456789".repeat(40));
    let body = format!(
        r#"{{"code":"ResourceNotFound","message":"API Key '{key}' is not found","details":"{{\"identifier\":\"{key}\"}} {key}"}}"#
    );
    let masked = mask_secret(&body, &key);
    assert!(!masked.contains(&key), "{masked}");
    assert_eq!(masked.matches(MASKED).count(), 3, "{masked}");
    assert!(
        masked.contains("API Key '********' is not found"),
        "{masked}"
    );
    // a short key masks its whole copies only
    assert_eq!(
        mask_secret(r#"{"code":"PackNotFound","message":"key 'k'"}"#, "k"),
        r#"{"code":"PackNotFound","message":"key '********'"}"#
    );
    // a copy next to a letter or a digit is a part of another word
    assert_eq!(
        mask_secret("ab abc 1ab ab", "ab"),
        "******** abc 1ab ********"
    );
    assert_eq!(mask_secret("no key here", ""), "no key here");
}
