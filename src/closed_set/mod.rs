//! A value of a closed set in a document that strata writes and reads, also when a later release
//! adds the value.
//!
//! Strata writes two documents that it also reads: a record, and the report of `behind --since`.
//! Some fields of these documents hold one value of a closed set. For example, the name of a tier
//! is `applied`, `labels`, `policy`, `running`, or `shape`.
//!
//! A later release can add a value to such a set, such as a tier `network`, and keep the schema
//! id. If the field is a plain enum, this release refuses the full document with the serde error
//! `unknown variant`. So each such field is an [`Open`] value. Strata reads a value that it does
//! not know with no error, and writes its text with no change. The page "Versions and
//! compatibility" of the book lists these fields, and what each command does with such a value.
//!
//! # The two parts
//!
//! [`ClosedSet`] is the trait of the enum. The enum has only unit variants, and serde writes
//! each variant as text. [`ClosedSet::as_str`] gives the same text, and [`ClosedSet::ALL`] lists
//! the values. The macro `impl_closed_set` writes both from one table. [`Open`] uses `as_str` to
//! compare, sort, and show a value, with no call to serde.
//!
//! > **Make sure that `as_str` gives the text that serde writes.** If the two texts are different,
//! > strata writes a known value with one text, and the next read gives [`Open::Unknown`]. The
//! > compiler does not make this check. The tests in `tests/closed_set.rs` make it for each set.
//!
//! [`Open`] holds one of two values:
//!
//! - [`Open::Known`] holds a value that this release knows, such as `Tier::Running`.
//! - [`Open::Unknown`] holds the text of a value that a later release wrote, such as `"network"`.
//!
//! The name comes from the two kinds of set. A [`ClosedSet`] takes only its own values. An
//! [`Open`] value takes them, and also any other text. Serde has `#[serde(other)]` for a variant
//! that takes any other text, but that variant does not keep the text. So a document that strata
//! writes from it loses the value.
//!
//! # Read and write
//!
//! [`Open`] reads a value in three steps:
//!
//! 1. Read the value as text. A value that is not text, such as `1`, gives an error.
//! 2. Look the text up in [`ClosedSet::ALL`], by [`ClosedSet::as_str`].
//! 3. If the text names a value of `T`, the value is [`Open::Known`]. If not, the value is
//!    [`Open::Unknown`] with the text, and there is no error.
//!
//! [`Open`] always writes the text of [`Open::as_str`]. A known value writes the text of its
//! variant. An unknown value writes the text that strata read. So each value keeps its text when
//! strata reads a document and then writes it.
//!
//! # Equality and order
//!
//! Two [`Open`] values are equal, and sort, by their text. So a `BTreeMap` with [`Open`] keys
//! writes its keys in the order of a map with text keys. The keys of a document have the same
//! order when this release or a later release writes it. A derived `Ord` sorts by the order of
//! the variants, and has no position for an unknown value.
//!
//! The order of [`Open`] is not the order of `T`. To compare two values by the order of `T`,
//! such as two impact levels by their severity, compare the values of [`Open::known`].
//!
//! # Use a value
//!
//! [`Open::known`] gives `Some` for a known value, and `None` for an unknown value. Code that
//! acts on the value matches on it, and decides what to do with an unknown value.
//! [`Open::as_str`] and `Display` give the text of each value. `tier == Tier::Running` compares
//! with a value of `T`, and `Tier::Running.into()` makes an [`Open`] value.
//!
//! # The schema
//!
//! The JSON Schema of an [`Open`] field is `anyOf` the schema of `T` and any text. So a validator
//! accepts each value of the set, and also a value of a later release.
//!
//! # The legend
//!
//! The module `legend` holds the legend of a document: the meaning of each value of a set. It names
//! each value with [`ClosedSet::as_str`] and lists the values with [`ClosedSet::ALL`]. So no other
//! table names the values of a set.
//!
//! # Examples
//!
//! A later release adds the tier `network`, and writes this map of tiers. This release reads the
//! map, knows two of the tiers, and writes the same text in the same order:
//!
//! ```
//! use std::collections::BTreeMap;
//! use strata::closed_set::Open;
//! use strata::snapshot::Tier;
//!
//! let text = r#"{"applied":1,"network":2,"running":3}"#;
//! let tiers: BTreeMap<Open<Tier>, u8> = serde_json::from_str(text).unwrap();
//!
//! let known: Vec<Option<Tier>> = tiers.keys().map(Open::known).collect();
//! assert_eq!(known, [Some(Tier::Applied), None, Some(Tier::Running)]);
//! assert!(tiers.contains_key(&Open::Unknown("network".to_string())));
//!
//! assert_eq!(serde_json::to_string(&tiers).unwrap(), text);
//! ```

use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub(crate) mod legend;

/// A closed set of values that a document names by text: an enum of unit variants.
///
/// A set lists its values in [`ClosedSet::ALL`] and names each one with [`ClosedSet::as_str`].
/// Implement both with the macro `impl_closed_set`, from one table of the values and their texts,
/// so that no list of the values is written twice.
pub trait ClosedSet: Copy + Serialize + DeserializeOwned + JsonSchema + 'static {
    /// Each value of the set, in the order of the type.
    const ALL: &'static [Self];

    /// The value as the documents name it: the text that serde writes for it.
    fn as_str(self) -> &'static str;

    /// The value that the documents name `text`, or `None` for another text.
    fn parse(text: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|v| v.as_str() == text)
    }
}

/// Implement [`ClosedSet`] for an enum of unit variants, from the table of its values and their
/// texts.
///
/// The table gives [`ClosedSet::ALL`] and [`ClosedSet::as_str`]. The `match` of `as_str` names each
/// variant, so a variant that the table does not list is a compile error. `tests/closed_set.rs`
/// makes sure that each text is the one that serde writes.
macro_rules! impl_closed_set {
    ($set:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        impl $crate::closed_set::ClosedSet for $set {
            const ALL: &'static [Self] = &[$(Self::$variant),+];

            fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }
        }
    };
}
pub(crate) use impl_closed_set;

/// A value of the closed set `T` in a document that strata writes and reads.
///
/// This release reads a value that a later release adds to `T` as [`Open::Unknown`], with its
/// text, and writes the same text. The [module](crate::closed_set) shows how it reads, writes,
/// and sorts.
#[derive(Debug, Clone)]
pub enum Open<T> {
    /// A value that this release knows.
    Known(T),
    /// The text of a value that this release does not know: a later release wrote it.
    Unknown(String),
}

impl<T: ClosedSet> Open<T> {
    /// The value that `text` names, or [`Open::Unknown`] with the text when it names no value.
    pub fn from_text(text: &str) -> Open<T> {
        T::parse(text).map_or_else(|| Open::Unknown(text.to_string()), Open::Known)
    }

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
    /// Read the text, and then the value of `T` that the text names: see [`Open::from_text`].
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        // a text that names no value of `T` is a value of a later release, and not an error
        Ok(Open::from_text(&text))
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

    impl_closed_set!(Color {
        Red => "red",
        DarkBlue => "darkBlue",
    });

    #[test]
    fn a_set_lists_each_value_and_parses_the_text_of_each_one() {
        assert_eq!(Color::ALL, [Color::Red, Color::DarkBlue]);
        assert_eq!(Color::parse("darkBlue"), Some(Color::DarkBlue));
        for color in Color::ALL {
            assert_eq!(Color::parse(color.as_str()), Some(*color));
        }
        // the text is the one of the documents, not the name of the variant
        assert_eq!(Color::parse("DarkBlue"), None);
        assert_eq!(Color::parse(""), None);
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
