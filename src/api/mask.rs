//! The mask of a secret in the values of a pack.
//!
//! Palette writes `********` over each pack value whose schema format is `password` when it
//! serves the published packs of a profile. It serves the draft packs with no mask. Strata writes
//! the same mask over the draft, so that a record and a saved answer hold no clear secret.

use serde_json::Value;
use serde_yaml::Value as Yaml;

/// The text that Palette writes over a secret.
pub const MASKED: &str = "********";

/// True when a format of the pack schema marks a secret: one of its `${...}` parts starts with
/// `password`, as Palette finds it. `${password}` and `${password:/.{8,}/}` are
/// secrets; `${string}` is not.
pub fn is_password_format(format: &str) -> bool {
    format.split("${").skip(1).any(|part| {
        let end = part.find([':', '}']).unwrap_or(part.len());
        &part[..end] == "password"
    })
}

/// The names of the secret fields of the pack `uid`, from the answer of `GET /v1/packs/{uid}`:
/// each `packValues[].schema[]` of that pack whose format is a password.
pub fn password_fields(pack: &Value, uid: &str) -> Vec<String> {
    pack.get("packValues")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|v| v.get("packUid").and_then(Value::as_str) == Some(uid))
        .filter_map(|v| v.get("schema").and_then(Value::as_array))
        .flatten()
        .filter(|s| {
            s.get("format")
                .and_then(Value::as_str)
                .is_some_and(is_password_format)
        })
        .filter_map(|s| s.get("name").and_then(Value::as_str).map(str::to_string))
        .collect()
}

/// `values` with `********` over each field of `fields`, as Palette masks a published pack. A
/// field is a path of keys with dots, such as `charts.app.adminPassword`. Palette does not mask a
/// field that the values do not have, an empty value, a map or a list, or a value that holds a
/// variable, `{{.spectro.var.NAME}}`. Strata does the same. Values that are not YAML stay as they
/// are, as in Palette.
///
/// Strata changes only the line of each field, so the rest of the text stays. When it cannot
/// change one line alone, for example for a block scalar, it writes the values again from the
/// parsed YAML, with no comments.
pub fn mask_values(values: &str, fields: &[String]) -> String {
    let Ok(mut doc) = serde_yaml::from_str::<Yaml>(values) else {
        return values.to_string();
    };
    let mut text = Some(values.to_string());
    let mut changed = false;
    for field in fields {
        let path: Vec<String> = field
            .replace('\\', "")
            .split('.')
            .map(str::to_string)
            .collect();
        let Some(slot) = lookup_mut(&mut doc, &path) else {
            continue;
        };
        if !holds_secret(slot) {
            continue;
        }
        *slot = Yaml::String(MASKED.to_string());
        changed = true;
        text = text
            .and_then(|t| edit_line(&t, &path))
            .filter(|t| serde_yaml::from_str::<Yaml>(t).is_ok_and(|v| v == doc));
    }
    match (changed, text) {
        (false, _) => values.to_string(),
        (true, Some(t)) => t,
        (true, None) => serde_yaml::to_string(&doc).unwrap_or_default(),
    }
}

/// The value at `path` in `doc`, through maps only.
fn lookup_mut<'a>(doc: &'a mut Yaml, path: &[String]) -> Option<&'a mut Yaml> {
    path.iter().try_fold(doc, |node, key| {
        node.as_mapping_mut()?.get_mut(Yaml::String(key.clone()))
    })
}

/// True when Palette masks this value: a scalar with text, and no variable in it.
fn holds_secret(v: &Yaml) -> bool {
    let text = match v {
        Yaml::String(s) => s.clone(),
        Yaml::Number(n) => n.to_string(),
        Yaml::Bool(b) => b.to_string(),
        _ => return false,
    };
    !text.is_empty() && !holds_variable(&text)
}

/// True when `text` holds a variable of a profile, `{{ .spectro.var.NAME }}`.
fn holds_variable(text: &str) -> bool {
    text.split("{{").skip(1).any(|part| {
        part.split_once("}}").is_some_and(|(inner, _)| {
            inner
                .trim_start()
                .strip_prefix(".spectro")
                .is_some_and(|rest| rest.len() > 4 && rest[1..].starts_with("var"))
        })
    })
}

/// `text` with the value of the key at `path` changed to `'********'` on its line, or `None` when
/// that key is not on a line of its own in block style.
fn edit_line(text: &str, path: &[String]) -> Option<String> {
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
fn split_key(body: &str) -> Option<(String, &str)> {
    if body.starts_with("- ") || body == "-" {
        return None;
    }
    let (key, rest) = match body.chars().next()? {
        q @ ('"' | '\'') => {
            let close = body[1..].find(q)? + 1;
            let rest = body[close + 1..].strip_prefix(':')?;
            (body[1..close].to_string(), rest)
        }
        _ => {
            let colon = body
                .match_indices(':')
                .map(|(i, _)| i)
                .find(|&i| body[i + 1..].is_empty() || body[i + 1..].starts_with([' ', '\r']))?;
            (body[..colon].trim_end().to_string(), &body[colon + 1..])
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
        let got = mask_values(values, &fields(&["charts.app.adminPassword"]));
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
        let got = mask_values(values, &fields(&["charts.password"]));
        assert_eq!(
            got,
            "other:\n  password: plain\ncharts:\n  password: '********'\n"
        );
    }

    #[test]
    fn palette_masks_no_empty_value_no_variable_and_no_map() {
        let values = "a: ''\nb: '{{ .spectro.var.db_password }}'\nc:\n  d: 1\ne: null\n";
        let names = fields(&["a", "b", "c", "e", "missing.key"]);
        assert_eq!(mask_values(values, &names), values);
    }

    #[test]
    fn a_number_or_a_quoted_key_is_masked() {
        let got = mask_values("\"pin\": 1234\nflag: true\n", &fields(&["pin", "flag"]));
        assert_eq!(got, "\"pin\": '********'\nflag: '********'\n");
    }

    #[test]
    fn a_block_scalar_is_masked_through_the_parsed_yaml() {
        let values = "# comment\nkey: |\n  line one\n  line two\nother: x\n";
        let got = mask_values(values, &fields(&["key"]));
        assert!(!got.contains("line one"), "{got}");
        let doc: Yaml = serde_yaml::from_str(&got).unwrap();
        assert_eq!(doc["key"], Yaml::String(MASKED.into()));
        assert_eq!(doc["other"], Yaml::String("x".into()));
    }

    #[test]
    fn values_that_are_not_yaml_stay() {
        let values = "a: [unclosed\npassword: s3cret";
        assert_eq!(mask_values(values, &fields(&["password"])), values);
    }
}
