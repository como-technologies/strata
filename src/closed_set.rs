//! A value of a closed set in a document that strata reads again, also a value that a later
//! release adds.
//!
//! Strata reads two documents again: a record, and the report of `behind --since`. A later
//! release can add a value to a closed set of these documents and keep the schema id. So each
//! such field is an [`Open`] value. Strata reads a value that it does not know with no error.
//! Each document that strata writes from it holds the same text.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{DeserializeOwned, IntoDeserializer};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A closed set of values that a document names by text: an enum of unit variants.
pub trait ClosedSet: Copy + Serialize + DeserializeOwned + JsonSchema {
    /// The value as the documents name it: the text that serde writes for it.
    fn as_str(self) -> &'static str;
}

/// A value of the closed set `T` in a document that strata reads again.
///
/// A later release can add a value to `T` and keep the schema id of the document. This release
/// reads such a value as [`Open::Unknown`], with its text, and writes the same text. Two values
/// are equal, and sort, by their text. So a map with these keys keeps the order of a map with
/// text keys.
#[derive(Debug, Clone)]
pub enum Open<T> {
    /// A value that this release knows.
    Known(T),
    /// The text of a value that this release does not know: a later release wrote it.
    Unknown(String),
}

impl<T: ClosedSet> Open<T> {
    /// The value, when this release knows it.
    pub fn known(&self) -> Option<T> {
        match self {
            Open::Known(v) => Some(*v),
            Open::Unknown(_) => None,
        }
    }

    /// The value as the documents name it.
    pub fn as_str(&self) -> &str {
        match self {
            Open::Known(v) => v.as_str(),
            Open::Unknown(text) => text,
        }
    }
}

impl<T> From<T> for Open<T> {
    fn from(v: T) -> Open<T> {
        Open::Known(v)
    }
}

impl<T: ClosedSet + Default> Default for Open<T> {
    fn default() -> Open<T> {
        Open::Known(T::default())
    }
}

impl<T: ClosedSet> PartialEq for Open<T> {
    fn eq(&self, other: &Open<T>) -> bool {
        self.as_str() == other.as_str()
    }
}

impl<T: ClosedSet> Eq for Open<T> {}

impl<T: ClosedSet> PartialEq<T> for Open<T> {
    fn eq(&self, other: &T) -> bool {
        self.as_str() == other.as_str()
    }
}

impl<T: ClosedSet> PartialOrd for Open<T> {
    fn partial_cmp(&self, other: &Open<T>) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: ClosedSet> Ord for Open<T> {
    fn cmp(&self, other: &Open<T>) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl<T: ClosedSet> fmt::Display for Open<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<T: ClosedSet> Serialize for Open<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de, T: ClosedSet> Deserialize<'de> for Open<T> {
    /// Read the text, and then the value of `T` that the text names, as `T` reads it.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        let value: Result<T, serde::de::value::Error> =
            T::deserialize(text.as_str().into_deserializer());
        // a text that names no value of `T` is a value of a later release, and not an error
        Ok(value.map_or(Open::Unknown(text), Open::Known))
    }
}

/// What the schema of an [`Open`] value says about a text that names no value of the set.
const LATER_VALUE: &str =
    "A value that a later release of strata adds. This release reads it, and keeps its text.";

impl<T: ClosedSet> JsonSchema for Open<T> {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        format!("Open_{}", T::schema_name()).into()
    }

    /// Each value of `T`, as its own schema names it, or any other text.
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "anyOf": [
                generator.subschema_for::<T>(),
                {"type": "string", "description": LATER_VALUE},
            ]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    /// A closed set of two values, as the enums of the documents are.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "camelCase")]
    enum Color {
        /// Red.
        Red,
        /// Dark blue.
        DarkBlue,
    }

    impl ClosedSet for Color {
        fn as_str(self) -> &'static str {
            match self {
                Color::Red => "red",
                Color::DarkBlue => "darkBlue",
            }
        }
    }

    #[test]
    fn a_text_that_names_a_value_reads_as_that_value_and_writes_the_same_text() {
        let v: Open<Color> = serde_json::from_value(json!("darkBlue")).unwrap();
        assert_eq!(v.known(), Some(Color::DarkBlue));
        assert_eq!(v, Color::DarkBlue);
        assert_eq!(serde_json::to_value(&v).unwrap(), json!("darkBlue"));
    }

    #[test]
    fn a_text_of_a_later_release_reads_with_no_error_and_writes_the_same_text() {
        let v: Open<Color> = serde_json::from_value(json!("green")).unwrap();
        assert_eq!(v.known(), None);
        assert_eq!(v.as_str(), "green");
        assert_eq!(v.to_string(), "green");
        assert_ne!(v, Color::Red);
        assert_eq!(serde_json::to_value(&v).unwrap(), json!("green"));
        // a value that is not text is an error, as for the set itself
        assert!(serde_json::from_value::<Open<Color>>(json!(1)).is_err());
    }

    #[test]
    fn a_map_keyed_by_open_values_keeps_the_order_of_their_text() {
        let text = r#"{"red":1,"green":2,"darkBlue":3}"#;
        let map: std::collections::BTreeMap<Open<Color>, u8> = serde_json::from_str(text).unwrap();
        assert_eq!(
            serde_json::to_string(&map).unwrap(),
            r#"{"darkBlue":3,"green":2,"red":1}"#
        );
        assert_eq!(map.get(&Open::Known(Color::Red)), Some(&1));
    }

    #[test]
    fn the_schema_names_each_value_of_the_set_and_takes_any_other_text() {
        let schema = schemars::schema_for!(Open<Color>);
        let any_of = schema.as_value()["anyOf"].as_array().unwrap().clone();
        assert_eq!(any_of[0]["$ref"], "#/$defs/Color");
        assert_eq!(
            any_of[1],
            json!({"type": "string", "description": LATER_VALUE})
        );
        let names: Vec<Value> = schema.as_value()["$defs"]["Color"]["oneOf"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["const"].clone())
            .collect();
        assert_eq!(names, [json!("red"), json!("darkBlue")]);
    }
}
