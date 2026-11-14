//! The mask of a secret in the values of a pack or of a cluster group, and in an answer.
//!
//! Palette writes `********` over each pack value whose schema format is `password` when it
//! serves the published packs of a profile. It serves the draft packs with no mask. Strata writes
//! the same mask over the draft, so that a record and a saved answer hold no clear secret. Strata
//! also writes the mask over each password in the values of a cluster group that a record or a
//! saved answer keeps. The body of an error of an answer holds no copy of the API key: see
//! [`mask_secret`].

use std::borrow::Cow;
use std::collections::BTreeMap;

use serde_json::Value;
use serde_yaml::Value as Yaml;

use crate::closed_set::ClosedSet;
use crate::content::canon::{Canon, canon_yaml, yaml_documents};
use crate::error::{Error, Result};
use crate::json::{arr, opt_text, str_at};
use crate::model::pack_type::PackType;

/// The text that Palette writes over a secret.
pub(crate) const MASKED: &str = "********";

/// `text` with [`MASKED`] in place of each whole copy of `secret`, such as the API key in the body
/// of an answer.
///
/// A copy is whole when no letter or digit touches it on either side. So a short key such as `k`
/// masks `'k'`, and not the `k` of `Pack`. Each whole copy gets the mask, also when `text` holds
/// more than one. An empty `secret` changes nothing.
pub(crate) fn mask_secret(text: &str, secret: &str) -> String {
    let Some(first) = secret.chars().next() else {
        return text.to_string();
    };
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    // the last character of `text` before `rest`
    let mut before: Option<char> = None;
    while let Some(at) = rest.find(secret) {
        let (head, copy) = rest.split_at(at);
        let prev = head.chars().next_back().or(before);
        let next = copy[secret.len()..].chars().next();
        out.push_str(head);
        if prev.is_some_and(char::is_alphanumeric) || next.is_some_and(char::is_alphanumeric) {
            // a part of a longer word: keep its first character, and look again after it
            out.push(first);
            before = Some(first);
            rest = &copy[first.len_utf8()..];
        } else {
            out.push_str(MASKED);
            before = secret.chars().next_back();
            rest = &copy[secret.len()..];
        }
    }
    out.push_str(rest);
    out
}

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
pub(crate) trait ScalarText {
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

/// True when the mask of a cluster group covers `value` under `key`. The key names a secret when
/// it ends with `password`, in capitals or not: `password` or `adminPassword`. The value is a
/// scalar that [`holds_secret`] covers.
fn is_group_secret(key: &str, value: &impl ScalarText) -> bool {
    key.to_ascii_lowercase().ends_with("password") && holds_secret(value)
}

/// A document that the mask of a cluster group walks. The two types are `serde_json::Value` for
/// the canonical values of a record, and `serde_yaml::Value` for the parsed values of a saved
/// document.
pub(crate) trait Tree: ScalarText + Sized {
    /// Call `visit` with the key and the value of each entry of a map, and with no key and each
    /// item of a list. A key that is not text gives no key.
    fn each_child(&mut self, visit: &mut impl FnMut(Option<&str>, &mut Self));

    /// The value that the mask writes over a secret.
    fn masked() -> Self;
}

impl Tree for Value {
    fn each_child(&mut self, visit: &mut impl FnMut(Option<&str>, &mut Self)) {
        match self {
            Value::Object(map) => map.iter_mut().for_each(|(k, v)| visit(Some(k), v)),
            Value::Array(items) => items.iter_mut().for_each(|v| visit(None, v)),
            _ => {}
        }
    }

    fn masked() -> Self {
        Value::String(MASKED.to_string())
    }
}

impl Tree for Yaml {
    fn each_child(&mut self, visit: &mut impl FnMut(Option<&str>, &mut Self)) {
        match self {
            Yaml::Mapping(map) => map.iter_mut().for_each(|(k, v)| visit(k.as_str(), v)),
            Yaml::Sequence(items) => items.iter_mut().for_each(|v| visit(None, v)),
            _ => {}
        }
    }

    fn masked() -> Self {
        Yaml::String(MASKED.to_string())
    }
}

/// Write `********` over each secret in `values`, the values of a cluster group, at each depth.
/// Palette serves the values of a group with no mask, and the values have no schema. So a secret
/// is a value whose key names a password. As for a pack, strata does not mask an empty value, a
/// map, a list, or a value that holds a variable. A value under a key that is not text can hold a
/// secret too, so the walk goes into it.
pub(crate) fn mask_password_keys<T: Tree>(values: &mut T) {
    values.each_child(&mut |key, child| {
        if key.is_some_and(|k| is_group_secret(k, &*child)) {
            *child = T::masked();
        } else {
            mask_password_keys(child);
        }
    });
}

/// A copy of `values` with the mask over each secret, or `None` when no secret is there.
fn masked_copy<T: Tree + Clone + PartialEq>(values: &T) -> Option<T> {
    let mut masked = values.clone();
    mask_password_keys(&mut masked);
    (masked != *values).then_some(masked)
}

/// `values`, the YAML text of the values of a cluster group, with `********` over each password,
/// as a record keeps them.
///
/// The rule of a password is the rule of [`mask_password_keys`]. When strata masks a value, it
/// writes the values again with no comments, from their canonical form. It does this so that a
/// record of the saved document is the record of the fetch. Values with no password stay as they
/// are. Values that are not YAML also stay as they are, as for a pack: strata cannot find a key
/// in them.
///
/// Some YAML has no canonical form: it has a key that is a map or a list. Two examples are an
/// explicit `? key` and a template with no quotes, `{{ .spectro.var.x }}`. Such values keep their
/// keys as they are, and strata writes them again from the parsed YAML.
pub(crate) fn mask_group_values(values: &str) -> Result<String> {
    let masked = match canon_yaml(values) {
        Canon::Parsed(parsed) => masked_copy(&parsed)
            .map(|m| to_yaml(&m, "the masked values of a cluster group"))
            .transpose()?,
        Canon::Unparseable(_) => mask_unreadable_values(values)?,
    };
    Ok(masked.unwrap_or_else(|| values.to_string()))
}

/// `values` with the mask over each password, when the parser reads `values` as YAML and the
/// values have no canonical form. `None` when the parser cannot read them, or when no password is
/// there.
///
/// Strata changes only the line of each password, so that the key that is a map stays as it is.
/// When the edited text does not read as the masked documents, for example for a block scalar, it
/// writes the documents again. A parser cannot write a key that is a map: then the result is an
/// error, and never values with a password in clear.
fn mask_unreadable_values(values: &str) -> Result<Option<String>> {
    let Some(docs) = untagged_documents(values) else {
        return Ok(None);
    };
    // the documents are the items of a list, as the canonical form of several documents is a list
    let Some(Yaml::Sequence(masked)) = masked_copy(&Yaml::Sequence(docs)) else {
        return Ok(None);
    };
    let edited = mask_password_lines(values);
    if untagged_documents(&edited).is_some_and(|read| read == masked) {
        return Ok(Some(edited));
    }
    let texts = masked
        .iter()
        .map(|doc| {
            to_yaml(
                doc,
                "the masked values of a cluster group, which have a key that is not text",
            )
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(texts.join("---\n")))
}

/// The documents of `text` with no tag, or `None` when the parser cannot read the text.
fn untagged_documents(text: &str) -> Option<Vec<Yaml>> {
    yaml_documents(text)
        .map(|doc| doc.map(untagged))
        .collect::<std::result::Result<_, _>>()
        .ok()
}

/// `doc` with each tag removed: a tagged value is its value, as the canonical form reads it. So
/// `!!str adminPassword: pw` has a password.
fn untagged(doc: Yaml) -> Yaml {
    match doc {
        Yaml::Tagged(tagged) => untagged(tagged.value),
        Yaml::Sequence(items) => Yaml::Sequence(items.into_iter().map(untagged).collect()),
        Yaml::Mapping(map) => Yaml::Mapping(
            map.into_iter()
                .map(|(k, v)| (untagged(k), untagged(v)))
                .collect(),
        ),
        other => other,
    }
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

/// `text` with `'********'` in place of the value on each line `key: value`. The change applies
/// to a line whose key names a password and whose value is a scalar that the mask covers. A line
/// can be an item of a list, `- key: value`. The text can hold a line that is no key, such as a
/// line of a block scalar. The caller reads the result again to check it.
fn mask_password_lines(text: &str) -> String {
    text.split('\n')
        .map(|line| masked_password_line(line).unwrap_or_else(|| line.to_string()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `line` with the mask over its value, or `None` when the line is no secret.
fn masked_password_line(line: &str) -> Option<String> {
    let mut body = line.trim_start_matches(' ');
    while let Some(item) = body.strip_prefix("- ") {
        body = item.trim_start_matches(' ');
    }
    let (key, rest) = split_key(body)?;
    let value = serde_yaml::from_str::<Yaml>(rest.trim())
        .ok()
        .map(untagged)?;
    if !is_group_secret(key, &value) {
        return None;
    }
    let end = if line.ends_with('\r') { "\r" } else { "" };
    let raw_key = &body[..body.len() - rest.len() - 1];
    let indent = &line[..line.len() - body.len()];
    Some(format!("{indent}{raw_key}: '{MASKED}'{end}"))
}

#[cfg(test)]
mod tests;
