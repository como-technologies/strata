//! The mask of a secret in the values of a pack or of a cluster group.
//!
//! Palette writes `********` over each pack value whose schema format is `password` when it
//! serves the published packs of a profile. It serves the draft packs with no mask. Strata writes
//! the same mask over the draft, so that a record and a saved answer hold no clear secret. Strata
//! also writes the mask over each password in the values of a cluster group that a record or a
//! saved answer keeps.

use std::borrow::Cow;
use std::collections::BTreeMap;

use serde_json::Value;
use serde_yaml::Value as Yaml;

use crate::canon::{Canon, canon_yaml};
use crate::error::{Error, Result};
use crate::json::{arr, opt_text, str_at};
use crate::pack_type::PackType;

/// The text that Palette writes over a secret.
pub(crate) const MASKED: &str = "********";

/// True when a format of the pack schema marks a secret: one of its `${...}` parts starts with
/// `password`, as Palette finds it. `${password}` and `${password:/.{8,}/}` are
/// secrets; `${string}` is not.
pub(super) fn is_password_format(format: &str) -> bool {
    format.split("${").skip(1).any(|part| {
        let end = part.find([':', '}']).unwrap_or(part.len());
        &part[..end] == "password"
    })
}

/// The names of the secret fields of the pack `uid`, from the answer of `GET /v1/packs/{uid}`:
/// each `packValues[].schema[]` of that pack whose format is a password.
pub(super) fn password_fields(pack: &Value, uid: &str) -> Vec<String> {
    arr(pack, "/packValues")
        .iter()
        .filter(|v| str_at(v, "/packUid") == uid)
        .flat_map(|v| arr(v, "/schema"))
        .filter(|s| is_password_format(str_at(s, "/format")))
        .filter_map(|s| opt_text(s, "/name"))
        .collect()
}

/// The uids of the draft packs of `profile` that can have a schema, sorted, each one time. A
/// manifest pack has no schema, so this list does not have it.
pub(super) fn draft_pack_uids(profile: &Value) -> Vec<String> {
    let mut uids: Vec<String> = arr(profile, "/spec/draft/packs")
        .iter()
        .filter(|pk| PackType::parse(str_at(pk, "/type")) != Some(PackType::Manifest))
        .filter_map(|pk| opt_text(pk, "/packUid"))
        .collect();
    uids.sort();
    uids.dedup();
    uids
}

/// Write `********` over each secret of the draft packs of a profile, as Palette does for the
/// published packs. `secrets` gives the secret fields of each pack, by pack uid. The values of a
/// draft pack are in `spec.draft.packs` of `profile`, and in `spec.packs` of `manifests`, the
/// answer of `packs/manifests`.
pub(super) fn mask_draft_packs(
    profile: &mut Value,
    manifests: &mut Value,
    secrets: &BTreeMap<String, Vec<String>>,
) -> Result<()> {
    if secrets.is_empty() {
        return Ok(());
    }
    let mask_at = |v: &mut Value, fields: &[String]| -> Result<()> {
        if let Some(text) = v.as_str().filter(|t| !t.is_empty()) {
            *v = Value::String(mask_values(text, fields)?);
        }
        Ok(())
    };
    let draft = profile
        .pointer_mut("/spec/draft/packs")
        .and_then(Value::as_array_mut);
    for pk in draft.into_iter().flatten() {
        if let Some(fields) = opt_text(pk, "/packUid").and_then(|u| secrets.get(&u))
            && let Some(v) = pk.get_mut("values")
        {
            mask_at(v, fields)?;
        }
    }
    let served = manifests
        .pointer_mut("/spec/packs")
        .and_then(Value::as_array_mut);
    for pk in served.into_iter().flatten() {
        if let Some(fields) = opt_text(pk, "/metadata/uid").and_then(|u| secrets.get(&u))
            && let Some(v) = pk.pointer_mut("/spec/values")
        {
            mask_at(v, fields)?;
        }
    }
    Ok(())
}

/// `values` with `********` over each field of `fields`, as Palette masks a published pack. A
/// field is a path of keys with dots, such as `charts.app.adminPassword`. Palette does not mask a
/// field that the values do not have, an empty value, a map or a list, or a value that holds a
/// variable, `{{.spectro.var.NAME}}`. Strata does the same. Values that are not YAML stay as they
/// are, as in Palette.
///
/// Strata changes only the line of each field, so the rest of the text stays. When it cannot
/// change one line alone, for example for a block scalar, it writes the values again from the
/// parsed YAML, with no comments. When that write fails, the result is an error, and never values
/// with no text.
pub(super) fn mask_values(values: &str, fields: &[String]) -> Result<String> {
    let Ok(mut doc) = serde_yaml::from_str::<Yaml>(values) else {
        return Ok(values.to_string());
    };
    let mut text = Some(values.to_string());
    let mut changed = false;
    for field in fields {
        let field = field.replace('\\', "");
        let path: Vec<&str> = field.split('.').collect();
        let Some(slot) = lookup_mut(&mut doc, &path) else {
            continue;
        };
        if !holds_secret(&*slot) {
            continue;
        }
        *slot = Yaml::String(MASKED.to_string());
        changed = true;
        text = text
            .and_then(|t| edit_line(&t, &path))
            .filter(|t| serde_yaml::from_str::<Yaml>(t).is_ok_and(|v| v == doc));
    }
    match (changed, text) {
        (false, _) => Ok(values.to_string()),
        (true, Some(t)) => Ok(t),
        (true, None) => to_yaml(&doc, "the masked values of a pack"),
    }
}

/// `value` as YAML text. `what` names the value in the error.
fn to_yaml(value: &impl serde::Serialize, what: &str) -> Result<String> {
    serde_yaml::to_string(value).map_err(|e| {
        Error::Serialize(<serde_json::Error as serde::ser::Error>::custom(format!(
            "{what}: {e}"
        )))
    })
}

/// The value at `path` in `doc`, through maps only.
fn lookup_mut<'a>(doc: &'a mut Yaml, path: &[&str]) -> Option<&'a mut Yaml> {
    path.iter()
        .try_fold(doc, |node, key| node.as_mapping_mut()?.get_mut(*key))
}

/// The text of a scalar of a document: a string, a number, or a boolean. A map, a list, and a
/// null have none, so a mask never covers them.
trait ScalarText {
    fn scalar_text(&self) -> Option<Cow<'_, str>>;
}

impl ScalarText for Yaml {
    fn scalar_text(&self) -> Option<Cow<'_, str>> {
        match self {
            Yaml::String(s) => Some(Cow::Borrowed(s)),
            Yaml::Number(n) => Some(Cow::Owned(n.to_string())),
            Yaml::Bool(b) => Some(Cow::Owned(b.to_string())),
            _ => None,
        }
    }
}

impl ScalarText for Value {
    fn scalar_text(&self) -> Option<Cow<'_, str>> {
        match self {
            Value::String(s) => Some(Cow::Borrowed(s)),
            Value::Number(n) => Some(Cow::Owned(n.to_string())),
            Value::Bool(b) => Some(Cow::Owned(b.to_string())),
            _ => None,
        }
    }
}

/// True when the mask covers this value: a scalar with text, and no variable in it. Palette
/// masks such a value of a pack, and strata such a value of a cluster group.
fn holds_secret(v: &impl ScalarText) -> bool {
    v.scalar_text()
        .is_some_and(|text| !text.is_empty() && !holds_variable(&text))
}

/// True when the key of a value of a cluster group names a secret. Such a key ends with
/// `password`, in capitals or not: `password` or `adminPassword`.
fn is_password_key(key: &str) -> bool {
    key.to_ascii_lowercase().ends_with("password")
}

/// Write `********` over each secret in `values`, the canonical values of a cluster group, at each
/// depth. Palette serves the values of a group with no mask, and the values have no schema. So a
/// secret is a value whose key names a password. As for a pack, strata does not mask an empty
/// value, a map, a list, or a value that holds a variable.
pub(crate) fn mask_password_keys(values: &mut Value) {
    match values {
        Value::Object(map) => {
            for (key, v) in map.iter_mut() {
                if is_password_key(key) && holds_secret(&*v) {
                    *v = Value::String(MASKED.to_string());
                } else {
                    mask_password_keys(v);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(mask_password_keys),
        _ => {}
    }
}

/// `values`, the YAML text of the values of a cluster group, with `********` over each password,
/// as a record keeps them.
///
/// The rule of a password is the rule of [`mask_password_keys`]. When strata masks a value, it
/// writes the values again from their canonical form, with no comments. So a record of the saved
/// document is the record of the fetch. Values with no password stay as they are. Values that are
/// not YAML also stay as they are, as for a pack: strata cannot find a key in them.
pub(super) fn mask_group_values(values: &str) -> Result<String> {
    let Canon::Parsed(parsed) = canon_yaml(values) else {
        return Ok(values.to_string());
    };
    let mut masked = parsed.clone();
    mask_password_keys(&mut masked);
    if masked == parsed {
        return Ok(values.to_string());
    }
    to_yaml(&masked, "the masked values of a cluster group")
}

/// True when `text` holds a variable of a profile, `{{ .spectro.var.NAME }}`.
fn holds_variable(text: &str) -> bool {
    text.split("{{").skip(1).any(|part| {
        part.split_once("}}")
            .is_some_and(|(inner, _)| inner.trim_start().starts_with(".spectro.var."))
    })
}

/// `text` with the value of the key at `path` changed to `'********'` on its line, or `None` when
/// that key is not on a line of its own in block style.
fn edit_line(text: &str, path: &[&str]) -> Option<String> {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let mut level = 0;
    // the indent of the key that holds the current level, and of the keys of that level
    let mut parent: Option<usize> = None;
    let mut child: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        let body = line.trim_start_matches(' ');
        if body.trim().is_empty() || body.starts_with('#') || body.starts_with("---") {
            continue;
        }
        let indent = line.len() - body.len();
        if parent.is_some_and(|p| indent <= p) {
            return None;
        }
        if indent != *child.get_or_insert(indent) {
            continue;
        }
        let Some((key, rest)) = split_key(body) else {
            continue;
        };
        if key != path[level] {
            continue;
        }
        if level + 1 < path.len() {
            level += 1;
            parent = Some(indent);
            child = None;
            continue;
        }
        if rest.trim().is_empty() {
            return None;
        }
        let end = if line.ends_with('\r') { "\r" } else { "" };
        let raw_key = &body[..body.len() - rest.len() - 1];
        lines[i] = format!("{}{raw_key}: '{MASKED}'{end}", &line[..indent]);
        return Some(lines.join("\n"));
    }
    None
}

/// The key of a `key: value` line, with no quotes, and the text after its colon.
fn split_key(body: &str) -> Option<(&str, &str)> {
    if body.starts_with("- ") || body == "-" {
        return None;
    }
    let (key, rest) = match body.chars().next()? {
        q @ ('"' | '\'') => {
            let close = body[1..].find(q)? + 1;
            let rest = body[close + 1..].strip_prefix(':')?;
            (&body[1..close], rest)
        }
        _ => {
            let colon = body
                .match_indices(':')
                .map(|(i, _)| i)
                .find(|&i| body[i + 1..].is_empty() || body[i + 1..].starts_with([' ', '\r']))?;
            (body[..colon].trim_end(), &body[colon + 1..])
        }
    };
    (rest.is_empty() || rest.starts_with([' ', '\r'])).then_some((key, rest))
}

#[cfg(test)]
mod tests {
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
}
