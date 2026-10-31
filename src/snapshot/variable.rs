//! The profile variables of a record and of a candidate.

use schemars::JsonSchema;
use serde::de;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::content::canon::{canonical_json, sha256_hex};
use crate::json::{arr, flag, text, text_or};

/// The format that a variable has when Palette gives none.
const DEFAULT_FORMAT: &str = "string";

/// The input type that a variable has when Palette gives none.
const DEFAULT_INPUT_TYPE: &str = "text";

fn default_format() -> String {
    DEFAULT_FORMAT.to_string()
}

fn default_input_type() -> String {
    DEFAULT_INPUT_TYPE.to_string()
}

/// The text of a field that holds text. A null is the default of the field: empty text. A number
/// or a flag keeps the text that the file writes, so `defaultValue: 8080` is `8080`.
fn text_or_null<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(d)?.unwrap_or_default())
}

/// The flag of a field that holds a flag. A null is the default of the field: `false`.
fn flag_or_null<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(Option::<bool>::deserialize(d)?.unwrap_or_default())
}

/// The options of a variable. A null is the default of the field: no option.
fn options_or_null<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<VariableOption>, D::Error> {
    Ok(Option::<Vec<VariableOption>>::deserialize(d)?.unwrap_or_default())
}

/// The name of a variable. A null is an error: a name has no default.
fn name_not_null<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Option::<String>::deserialize(d)?
        .ok_or_else(|| de::Error::invalid_type(de::Unexpected::Unit, &"a string"))
}

/// Reads a field whose values are a closed set of Palette: its text, or `None` for a null, a
/// number, or a flag. A list or a map is an error.
struct SetText;

impl de::Visitor<'_> for SetText {
    type Value = Option<String>;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("text")
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(Some(v.to_string()))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
        Ok(None)
    }
}

/// The text of a field whose values are a closed set of Palette, or `default` when the value is
/// not text. [`Variable::from_palette`] reads such a field in the same way.
fn set_text_or<'de, D: Deserializer<'de>>(d: D, default: &str) -> Result<String, D::Error> {
    Ok(d.deserialize_any(SetText)?
        .unwrap_or_else(|| default.to_string()))
}

/// The `format` of a variable: a value of the closed set `v1VariableFormat`.
fn format_or_default<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    set_text_or(d, DEFAULT_FORMAT)
}

/// The `inputType` of a variable: a value of the closed set `v1VariableInputType`.
fn input_type_or_default<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    set_text_or(d, DEFAULT_INPUT_TYPE)
}

/// One profile variable: the fields that strata keeps of a variable of Palette.
///
/// Only `name` is required. A field that is not there, or is null, has its default, so a
/// candidate with a `name` and a `defaultValue` loads. A record always holds all the fields.
///
/// The fields are in the order of their names in the JSON, as a record of an older release holds
/// them. So a record that strata writes again keeps its text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct Variable {
    /// The value that the variable has when a cluster gives none.
    #[serde(default, deserialize_with = "text_or_null")]
    pub default_value: String,
    /// What the variable is for.
    #[serde(default, deserialize_with = "text_or_null")]
    pub description: String,
    /// The name of the variable in the form of the UI.
    #[serde(default, deserialize_with = "text_or_null")]
    pub display_name: String,
    /// The format of the value, as Palette gives it in `v1VariableFormat`. Strata writes
    /// `string` when Palette gives none.
    #[serde(default = "default_format", deserialize_with = "format_or_default")]
    pub format: String,
    /// True when the UI does not show the variable.
    #[serde(default, deserialize_with = "flag_or_null")]
    pub hidden: bool,
    /// True when the value cannot change after the cluster exists.
    #[serde(default, deserialize_with = "flag_or_null")]
    pub immutable: bool,
    /// How the UI asks for the value, as Palette gives it in `v1VariableInputType`. Strata writes
    /// `text` when Palette gives none.
    #[serde(
        default = "default_input_type",
        deserialize_with = "input_type_or_default"
    )]
    pub input_type: String,
    /// True when the value is a secret.
    #[serde(default, deserialize_with = "flag_or_null")]
    pub is_sensitive: bool,
    /// The name of the variable, as the macro `{{.spectro.var.NAME}}` names it.
    #[serde(deserialize_with = "name_not_null")]
    pub name: String,
    /// The values that the UI offers, as Palette gives them in `v1VariableOption`.
    #[serde(default, deserialize_with = "options_or_null")]
    pub options: Vec<VariableOption>,
    /// The pattern that the value must match.
    #[serde(default, deserialize_with = "text_or_null")]
    pub regex: String,
    /// True when a cluster must give a value.
    #[serde(default, deserialize_with = "flag_or_null")]
    pub required: bool,
}

/// One value that the UI offers for a variable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VariableOption {
    /// What the UI shows for the value.
    #[serde(default, deserialize_with = "text_or_null")]
    pub description: String,
    /// The text of the option.
    #[serde(default, deserialize_with = "text_or_null")]
    pub label: String,
    /// The value that the option sets.
    #[serde(default, deserialize_with = "text_or_null")]
    pub value: String,
}

impl Variable {
    /// The variable that strata keeps of `v`, a variable of Palette: the same fields, and the
    /// default of the field for each one that Palette does not give.
    ///
    /// The defaults are those that each record of an older release holds, so its `variablesHash`
    /// stays. One case differs: a name that is not there. A record of an older release holds
    /// `null` for it, and strata gives empty text. Palette requires a name, so only a raw
    /// directory or a profile export that a person made gives such a variable.
    pub(crate) fn from_palette(v: &Value) -> Variable {
        Variable {
            default_value: text(v, "/defaultValue"),
            description: text(v, "/description"),
            display_name: text(v, "/displayName"),
            format: text_or(v, "/format", DEFAULT_FORMAT).to_string(),
            hidden: flag(v, "/hidden"),
            immutable: flag(v, "/immutable"),
            input_type: text_or(v, "/inputType", DEFAULT_INPUT_TYPE).to_string(),
            is_sensitive: flag(v, "/isSensitive"),
            name: text(v, "/name"),
            options: arr(v, "/options")
                .iter()
                .map(|o| VariableOption {
                    description: text(o, "/description"),
                    label: text(o, "/label"),
                    value: text(o, "/value"),
                })
                .collect(),
            regex: text(v, "/regex"),
            required: flag(v, "/required"),
        }
    }

    /// The variable as a JSON document, for the comparison of two variables by leaf.
    pub(crate) fn to_value(&self) -> Value {
        // a variable holds text, flags, and a list of those, so it always serializes
        serde_json::to_value(self).expect("a variable serializes")
    }
}

/// The hash of `variables`: the sha256 of their canonical JSON, in the order given.
pub(crate) fn hash_of(variables: &[Variable]) -> String {
    let values: Vec<Value> = variables.iter().map(Variable::to_value).collect();
    sha256_hex(&canonical_json(&Value::Array(values)))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_variable_of_palette_gets_the_default_of_each_field_that_it_does_not_give() {
        let v = Variable::from_palette(&json!({"name": "env", "format": 3, "required": true}));
        assert_eq!(
            serde_json::to_value(&v).unwrap(),
            json!({
                "name": "env", "displayName": "", "description": "", "format": "string",
                "defaultValue": "", "regex": "", "required": true, "immutable": false,
                "hidden": false, "isSensitive": false, "inputType": "text", "options": [],
            }),
            "a format that is not text is no format"
        );
    }

    #[test]
    fn a_variable_keeps_the_twelve_fields_and_the_three_fields_of_an_option() {
        let given = json!({
            "name": "tier", "displayName": "Tier", "description": "The tier.", "format": "string",
            "defaultValue": "gold", "regex": "^[a-z]+$", "required": true, "immutable": true,
            "hidden": true, "isSensitive": true, "inputType": "dropdown",
            "options": [{"label": "Gold", "value": "gold", "description": "Best"}],
            "order": 3,
        });
        let v = Variable::from_palette(&given);
        let mut kept = given.clone();
        kept.as_object_mut().unwrap().remove("order");
        assert_eq!(serde_json::to_value(&v).unwrap(), kept);
        assert_eq!(Variable::deserialize(&kept).unwrap(), v);
    }

    #[test]
    fn a_variable_in_a_candidate_with_a_name_and_a_default_loads() {
        let v: Variable =
            serde_json::from_value(json!({"name": "a", "defaultValue": "1"})).unwrap();
        assert_eq!(v.format, "string");
        assert_eq!(v.input_type, "text");
        assert_eq!(v.default_value, "1");
        assert!(serde_json::from_value::<Variable>(json!({"defaultValue": "1"})).is_err());
    }

    #[test]
    fn a_null_field_of_a_variable_in_json_is_the_default_of_the_field() {
        let v: Variable = serde_json::from_value(json!({
            "name": "a", "description": null, "format": null, "inputType": 4, "hidden": null,
            "options": null, "defaultValue": null,
        }))
        .unwrap();
        assert_eq!(v, Variable::from_palette(&json!({"name": "a"})));
        let err = serde_json::from_value::<Variable>(json!({"name": null})).unwrap_err();
        assert!(err.to_string().contains("expected a string"), "{err}");
    }

    #[test]
    fn a_variable_serializes_its_keys_in_the_order_of_a_record_of_an_older_release() {
        let v = Variable::from_palette(&json!({"name": "a", "options": [{"label": "x"}]}));
        assert_eq!(
            serde_json::to_string(&v).unwrap(),
            serde_json::to_string(&v.to_value()).unwrap(),
            "the keys of a Value are in the order of their names"
        );
    }

    #[test]
    fn the_hash_of_the_variables_is_the_hash_of_the_canonical_json_of_the_older_form() {
        let older = json!([{
            "name": "a", "displayName": "", "description": "", "format": "string",
            "defaultValue": "x", "regex": "", "required": false, "immutable": false,
            "hidden": false, "isSensitive": false, "inputType": "text", "options": [],
        }]);
        let now = [Variable::from_palette(
            &json!({"name": "a", "defaultValue": "x"}),
        )];
        assert_eq!(hash_of(&now), sha256_hex(&canonical_json(&older)));
    }
}
