//! The options of a pack template that the values of a layer select, and the warning for them.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::content::canon::{Canon, canon_yaml};
use crate::json::{arr, object_at, opt_text};
use crate::snapshot::{PresetOption, Snapshot};
use crate::term::progress::Progress;

/// The options of the pack template that `values` select, sorted by parameter. `summary` is the
/// entry of the pack version in the listing, which gives the template in
/// `spec.template.parameters.inputParameters`. On a create and an update of a profile, Palette
/// reads the value at the `targetKey` of each input parameter as text, and when the text names
/// one of its `options`, writes the presets of that option into the values. The listing gives
/// each option with no presets, so strata records the selection and does not change the values.
pub(crate) fn preset_options(summary: &Value, values: &str) -> Vec<PresetOption> {
    let params = arr(summary, "/spec/template/parameters/inputParameters");
    if params.is_empty() {
        return Vec::new();
    }
    let Canon::Parsed(parsed) = canon_yaml(values) else {
        return Vec::new();
    };
    let mut flat = BTreeMap::new();
    if let Value::Object(top) = &parsed {
        flatten(top, "", &mut flat);
    }
    // by name, as Palette keys the parameters: a later parameter of one name replaces an earlier
    let mut selected: BTreeMap<String, PresetOption> = BTreeMap::new();
    for p in params {
        let (Some(name), Some(key), Some(options)) = (
            opt_text(p, "/name"),
            opt_text(p, "/targetKey"),
            object_at(p, "/options"),
        ) else {
            continue;
        };
        let at = flat
            .get(key.as_str())
            .copied()
            .map(as_text)
            .unwrap_or_default();
        if options.contains_key(&at) {
            selected.insert(
                name.clone(),
                PresetOption {
                    parameter: name,
                    target_key: key,
                    option: at,
                },
            );
        }
    }
    selected.into_values().collect()
}

/// Each key of a values map as Palette spells it, a dotted path, with its value. A list is a key
/// of its own, and each of its items is a key with the index. A map is not a key: its keys are.
fn flatten<'v>(map: &'v Map<String, Value>, prefix: &str, out: &mut BTreeMap<String, &'v Value>) {
    for (k, v) in map {
        let key = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}.{k}")
        };
        match v {
            Value::Object(inner) => flatten(inner, &key, out),
            Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    let at = format!("{key}.{i}");
                    match item {
                        Value::Object(inner) => flatten(inner, &at, out),
                        _ => {
                            out.insert(at, item);
                        }
                    }
                }
                out.insert(key, v);
            }
            _ => {
                out.insert(key, v);
            }
        }
    }
}

/// A value as Palette reads it for an option: a string as it is, a number or a boolean as its
/// text, and nothing for null, a list, or a map.
fn as_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => match (n.as_i64(), n.as_u64(), n.as_f64()) {
            (Some(i), _, _) => i.to_string(),
            (_, Some(u), _) => u.to_string(),
            (_, _, Some(f)) => f.to_string(),
            _ => n.to_string(),
        },
        _ => String::new(),
    }
}

/// Warn once for each option of a pack template that the values of a pack of `snap` select.
///
/// Palette can write the presets of that option into the values when it publishes the version,
/// and the root of `snap` does not have them.
pub(crate) fn warn_preset_options(snap: &Snapshot, progress: &Progress) {
    for p in &snap.packs {
        for o in &p.preset_options {
            progress.warn(&format!(
                "{}: the values select the option {} of {} ({}). Palette can write the presets of \
                 that option into the values, and no route of Palette shows them. So the root \
                 can differ from the version that Palette publishes",
                p.name, o.option, o.parameter, o.target_key
            ));
        }
    }
}
