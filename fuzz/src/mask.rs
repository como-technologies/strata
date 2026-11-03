//! The targets of the mask: the values of a draft pack, and the group values and the documents of
//! a raw directory of a cluster.
//!
//! The mask is not public. The draft target reaches it as a program does: through the read of a
//! profile that has a draft. The raw directory target reaches it through the save of a cluster.
//! Each target checks the answer against a second way to say the same rule, in a few lines. The
//! rule is that no secret stays in clear, and nothing else changes.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use serde::Deserialize;
use serde_json::{Value, json};
use serde_yaml::Value as Yaml;
use strata::api::{MemoryApi, Palette, RawCluster, ReadErrors};
use strata::term::progress::Progress;

use crate::routes::{
    PackFixture, manifests_answer, no_manifests, no_template, no_variables, pack_route,
    profile_document, serve_profile, template,
};
use crate::support::{AT, UID, answer, fingerprint, scratch, utf8};

/// The text that Palette writes over a secret.
///
/// This is a second text of the const of strata, on purpose. The check states the rule on its own,
/// and it fails when strata changes the text that a record and a raw directory hold. The library
/// does not make its const public for a target.
const MASKED: &str = "********";

/// The format that marks a field of a pack schema as a secret.
const PASSWORD_FORMAT: &str = "${password}";

/// The fields that no document holds, in the forms that a schema can write. The forms are a path
/// that is not there, an empty path, and a path with an escape.
const ODD_FIELDS: [&str; 4] = ["no.such.field", "", ".", "a\\.b"];

/// A Palette that serves a profile with one draft pack, which holds `values`. It also serves the
/// schema of that pack, which marks each of `fields` as a secret.
fn draft_api(values: &str, fields: &[String]) -> MemoryApi {
    let schema: Vec<Value> = fields
        .iter()
        .map(|name| json!({"name": name, "format": PASSWORD_FORMAT}))
        .collect();
    let pack = PackFixture::new("pack", "addon", "1.0.0", "p1", "r1", "", values);
    let draft = template("aws", "add-on", json!([pack.pack_ref(&[])]));
    serve_profile(
        MemoryApi::new(),
        UID,
        profile_document(UID, "demo", "1.0.0", no_template(), Some(draft)),
        manifests_answer(vec![pack.pack_manifests(Vec::new())]),
        no_variables(),
    )
    .with(
        pack_route("p1"),
        json!({"packValues": [{"packUid": "p1", "schema": schema}]}),
    )
}

/// The values of the draft pack of the profile that [`draft_api`] serves, after the read of the
/// profile masked `fields`. The profile document and the manifests answer hold the same values, so
/// they must be the same text.
pub(crate) fn mask_draft(values: &str, fields: &[String]) -> Option<String> {
    let api = draft_api(values, fields);
    let raw = answer(Palette::new(&api).fetch_profile(UID, &Progress::silent()))?;
    let in_profile = raw
        .profile
        .pointer("/spec/draft/packs/0/values")
        .and_then(Value::as_str)
        .expect("the draft pack keeps its values");
    let in_manifests = raw
        .manifests
        .pointer("/spec/packs/0/spec/values")
        .and_then(Value::as_str)
        .expect("the manifests answer keeps its values");
    assert_eq!(
        in_profile, in_manifests,
        "the draft and the manifests mask {values:?} two ways"
    );
    Some(in_profile.to_string())
}

/// The path of each value of `doc` that a dotted field can reach, with the value. A dotted field
/// goes through mappings only, so a sequence is one value. A key that is not text gets a name that
/// no text key has.
fn leaves(doc: &Yaml) -> BTreeMap<Vec<String>, Yaml> {
    fn walk(doc: &Yaml, path: &mut Vec<String>, out: &mut BTreeMap<Vec<String>, Yaml>) {
        match doc {
            Yaml::Mapping(map) if !map.is_empty() => {
                for (key, value) in map {
                    path.push(match key {
                        Yaml::String(s) => s.clone(),
                        other => format!("\0{other:?}"),
                    });
                    walk(value, path, out);
                    path.pop();
                }
            }
            leaf => {
                out.insert(path.clone(), leaf.clone());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(doc, &mut Vec::new(), &mut out);
    out
}

/// The text of a scalar that a mask covers: a string, a number, or a boolean.
fn scalar_text(value: &Yaml) -> Option<String> {
    match value {
        Yaml::String(s) => Some(s.clone()),
        Yaml::Number(n) => Some(n.to_string()),
        Yaml::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Make sure that `masked` is `values` with the mask over each secret of `fields` and no other
/// change. A secret is a scalar with text at a field. A scalar with a `{{` can be a variable of
/// the profile, which Palette leaves in clear, so the check allows both.
#[track_caller]
fn check_draft(values: &str, fields: &[String], masked: &str) {
    let Ok(before) = serde_yaml::from_str::<Yaml>(values) else {
        assert_eq!(masked, values, "values that are not YAML stay as they are");
        return;
    };
    let after: Yaml = serde_yaml::from_str(masked)
        .unwrap_or_else(|e| panic!("the mask broke the YAML of {values:?}: {e}\n{masked}"));
    // A schema field with no name is no field: Palette omits it from the list of secrets.
    let secret: BTreeSet<Vec<String>> = fields
        .iter()
        .filter(|field| !field.is_empty())
        .map(|field| {
            field
                .replace('\\', "")
                .split('.')
                .map(String::from)
                .collect()
        })
        .collect();
    let (before, after) = (leaves(&before), leaves(&after));
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>(),
        "the mask changes the shape of {values:?}:\n{masked}"
    );
    let mask = Yaml::String(MASKED.to_string());
    for (path, was) in &before {
        let now = &after[path];
        let text = scalar_text(was).filter(|t| !t.is_empty());
        // a field is text, so it cannot name a value under a key that is not text
        let reachable = secret.contains(path) && path.iter().all(|key| !key.starts_with('\0'));
        match text {
            Some(t) if reachable && t.contains("{{") => assert!(
                now == was || *now == mask,
                "{path:?} of {values:?} became {now:?}"
            ),
            Some(_) if reachable => assert_eq!(
                *now, mask,
                "the secret at {path:?} of {values:?} stays in clear:\n{masked}"
            ),
            _ => assert_eq!(
                now, was,
                "the mask changes {path:?} of {values:?}, which is no secret"
            ),
        }
    }
}

/// The fields that `values` gives a schema. They are the path of about two values in three, each
/// value whose key names a password, and the fields that no document holds.
pub(crate) fn fields_of(values: &str) -> Vec<String> {
    let mut fields: Vec<String> = ODD_FIELDS.iter().map(|f| f.to_string()).collect();
    if let Ok(doc) = serde_yaml::from_str::<Yaml>(values) {
        for path in leaves(&doc).keys() {
            let field = path.join(".");
            let named = path
                .last()
                .is_some_and(|key| key.to_ascii_lowercase().contains("password"));
            // a fixed rule, so that a corpus file always chooses the same fields
            if named || !fingerprint(field.as_bytes()).is_multiple_of(3) {
                fields.push(field);
            }
        }
    }
    fields
}

/// Mask the values of a draft pack at `fields`, and check the answer. Return true when the mask
/// changed the values.
pub(crate) fn mask_and_check(values: &str, fields: &[String]) -> bool {
    let Some(masked) = mask_draft(values, fields) else {
        return false;
    };
    check_draft(values, fields, &masked);
    let twice = mask_draft(&masked, fields).expect("masked values mask again");
    assert_eq!(twice, masked, "the mask of {values:?} is not idempotent");
    masked != values
}

/// Mask the text of `bytes` as the values of a draft pack. Return true when the text is UTF-8 and
/// the mask changed it.
pub(crate) fn draft(bytes: &[u8]) -> bool {
    utf8(bytes).is_some_and(|values| mask_and_check(values, &fields_of(values)))
}

#[test]
fn a_draft_pack_keeps_no_secret_in_clear_and_nothing_else_changes() {
    bolero::check!().for_each(|bytes| {
        draft(bytes);
    });
}

/// The directory that `save_dir` writes for a cluster whose group values are `values`, and the
/// documents `notifications` and `repave`.
fn save_cluster(values: &str, other: Option<Value>) -> Option<std::path::PathBuf> {
    let out = scratch().join("raw");
    let raw = RawCluster {
        cluster: json!({
            "metadata": {"uid": UID, "name": "demo"},
            "spec": {"clustersConfig": {"values": values}},
        }),
        manifests: no_manifests(),
        variables: json!([]),
        variables_unavailable: false,
        project_uid: None,
        fetched_at: None,
        notifications: other.clone(),
        repave: other,
        resolved_values: None,
        cloud_config: None,
        rbacs: None,
        namespaces: None,
        errors: ReadErrors::default(),
    };
    answer(raw.save_dir(&out, AT))?;
    Some(out)
}

/// The group values that `save_cluster` kept in `dir`.
fn saved_values(dir: &std::path::Path) -> String {
    let text = fs::read_to_string(dir.join(RawCluster::MARKER)).expect("the cluster file is there");
    let cluster: Value = serde_json::from_str(&text).expect("the cluster file is JSON");
    cluster
        .pointer("/spec/clustersConfig/values")
        .and_then(Value::as_str)
        .expect("the cluster file keeps the group values")
        .to_string()
}

/// The path of each secret that stays in clear in `doc`. It is a key that ends with `password`,
/// in capitals or not, with a scalar that has text and no `{{`. The walk goes into a value under
/// a key that is not text too.
fn clear_passwords(doc: &Yaml, path: &str, out: &mut Vec<String>) {
    match doc {
        Yaml::Mapping(map) => {
            for (key, value) in map {
                let name = key.as_str();
                let at = format!("{path}/{}", name.unwrap_or("?"));
                let secret = name.is_some_and(|k| k.to_ascii_lowercase().ends_with("password"));
                match scalar_text(value) {
                    Some(t) if secret && !t.is_empty() && !t.contains("{{") && t != MASKED => {
                        out.push(at);
                    }
                    _ if !secret => clear_passwords(value, &at, out),
                    _ => {}
                }
            }
        }
        Yaml::Sequence(items) => {
            for (i, item) in items.iter().enumerate() {
                clear_passwords(item, &format!("{path}/{i}"), out);
            }
        }
        _ => {}
    }
}

/// Make sure that `saved` keeps no password of `values` in clear, when the parser reads `values`.
/// The check holds for each key, also for a key that is not text. Such a key is a map or a list,
/// such as a template with no quotes. Values that are not YAML stay as they are.
#[track_caller]
fn check_group(values: &str, saved: &str) {
    let docs: Option<Vec<Yaml>> = serde_yaml::Deserializer::from_str(saved)
        .map(Yaml::deserialize)
        .collect::<Result<_, _>>()
        .ok();
    let Some(docs) = docs else { return };
    let mut clear = Vec::new();
    for doc in &docs {
        clear_passwords(doc, "", &mut clear);
    }
    assert!(
        clear.is_empty(),
        "the save of {values:?} keeps {clear:?} in clear:\n{saved}"
    );
}

/// Save a cluster whose group values are `values`, and check the file. `other` is a JSON document
/// that the save also reads as the notifications and as the repave of the cluster. Return true
/// when the save wrote the directory.
fn save_and_check(values: &str, other: Option<Value>) -> bool {
    let Some(dir) = save_cluster(values, other) else {
        return false;
    };
    let saved = saved_values(&dir);
    check_group(values, &saved);
    // the directory reads back, and the second save is the first
    answer(RawCluster::load_dir(&dir)).expect("a directory that strata wrote loads");
    let second = save_cluster(&saved, None).expect("saved values save again");
    assert_eq!(
        saved_values(&second),
        saved,
        "the save of {values:?} is not idempotent"
    );
    true
}

/// Save a cluster whose group values are the text of `bytes`. When the text is JSON, the save reads
/// it as the other documents too. Return true when the save wrote the directory.
pub(crate) fn group(bytes: &[u8]) -> bool {
    utf8(bytes).is_some_and(|values| save_and_check(values, serde_json::from_str(values).ok()))
}

#[test]
fn a_saved_cluster_keeps_no_password_of_its_group_values_in_clear() {
    bolero::check!().for_each(|bytes| {
        group(bytes);
    });
}

/// The panic of Wave 3. The mask sliced the values of a user at a byte that was not a `char`
/// boundary. This happened for a value that holds the text `.spectroévar` where a variable of a
/// profile has `.spectro.var`. The file in `crashes/` of the target holds the same values.
#[test]
fn a_draft_value_with_a_non_ascii_variable_name_is_masked_and_does_not_panic() {
    let values = "adminPassword: \"{{ .spectroévar.x }}\"\nsync:\n  password: s3cr3t\n";
    let fields: Vec<String> = ["adminPassword", "sync.password"]
        .map(String::from)
        .to_vec();
    let masked = mask_draft(values, &fields).expect("the draft masks");
    // the text is no variable of the profile, so it is a secret as any other text
    assert_eq!(
        masked,
        "adminPassword: '********'\nsync:\n  password: '********'\n"
    );
    // a real variable stays in clear
    let variable = "adminPassword: \"{{ .spectro.var.admin }}\"\n";
    assert_eq!(mask_draft(variable, &fields).as_deref(), Some(variable));
}
