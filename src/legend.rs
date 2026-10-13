//! The legend of a document: what each value of a closed set means, by the name of the value in
//! the documents.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

/// A closed set of values whose meaning a document carries in its legend.
pub(crate) trait Described: Copy + Serialize + 'static {
    /// Each value of the set.
    const ALL: &'static [Self];

    /// What the value means, in plain words, for a reader without the book.
    fn describe(self) -> &'static str;
}

/// The legend of the set `T`: the meaning of each value, by its name in the documents. Each value
/// of [`Described::ALL`] has a line, so a new value cannot miss its line.
pub(crate) fn legend<T: Described>() -> BTreeMap<String, String> {
    T::ALL
        .iter()
        .map(|v| (document_name(v), v.describe().to_string()))
        .collect()
}

/// The name of `value` in the documents: the text that serde writes for it, so the name is the
/// one of the JSON and no second table names it.
pub(crate) fn document_name<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(Value::String(name)) => name,
        // a value of a closed set is a unit variant of an enum, and serde writes it as text with
        // no error, so no other answer comes
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{Op, Status};
    use crate::impact::ImpactLevel;
    use crate::ops::{PairStatus, Policy, RecordStatus};
    use schemars::JsonSchema;

    /// The names of the set `T`, as its JSON schema lists them, sorted.
    fn schema_names<T: JsonSchema>() -> Vec<String> {
        let schema = schemars::schema_for!(T);
        let mut names: Vec<String> = schema.as_value()["enum"]
            .as_array()
            .expect("a closed set has an enum list in its schema")
            .iter()
            .map(|v| v.as_str().unwrap_or_default().to_string())
            .collect();
        names.sort();
        names
    }

    /// The legend of `T` has one line for each name of its schema, and nothing else.
    fn legend_names_each_value<T: Described + JsonSchema>() {
        let legend = legend::<T>();
        let names: Vec<String> = legend.keys().cloned().collect();
        assert_eq!(names, schema_names::<T>(), "{}", std::any::type_name::<T>());
        assert_eq!(names.len(), T::ALL.len(), "{}", std::any::type_name::<T>());
        for (name, text) in &legend {
            assert!(!text.is_empty(), "{name}");
        }
    }

    #[test]
    fn each_legend_names_each_value_of_its_set_and_no_other() {
        legend_names_each_value::<ImpactLevel>();
        legend_names_each_value::<Status>();
        legend_names_each_value::<Op>();
        legend_names_each_value::<RecordStatus>();
        legend_names_each_value::<PairStatus>();
        legend_names_each_value::<Policy>();
    }

    #[test]
    fn the_document_name_is_the_text_of_the_json() {
        assert_eq!(document_name(&ImpactLevel::MayRepave), "may-repave");
        assert_eq!(document_name(&PairStatus::SameContent), "same-content");
        assert_eq!(document_name(&RecordStatus::Unverified), "unverified");
    }
}
