//! The legend of a document: what each value of a closed set means, by the name of the value in
//! the documents.

use std::collections::BTreeMap;

use super::ClosedSet;

/// A closed set of values whose meaning a document carries in its legend.
pub(crate) trait Described: ClosedSet {
    /// What the value means, in plain words, for a reader without the book.
    fn describe(self) -> &'static str;
}

/// The legend of the set `T`: the meaning of each value, by its name in the documents. Each value
/// of [`ClosedSet::ALL`] has a line, so a new value cannot miss its line.
pub(crate) fn legend<T: Described>() -> BTreeMap<String, String> {
    T::ALL
        .iter()
        .map(|v| (v.as_str().to_string(), v.describe().to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{ImpactLevel, Op, Status};
    use crate::ops::{PairStatus, Policy, RecordStatus};

    /// The legend of `T` has one line for each value, and each line says something.
    fn legend_describes_each_value<T: Described>() {
        let legend = legend::<T>();
        assert_eq!(legend.len(), T::ALL.len(), "{}", std::any::type_name::<T>());
        for (name, text) in &legend {
            assert!(!text.is_empty(), "{name}");
        }
    }

    #[test]
    fn each_legend_describes_each_value_of_its_set_and_no_other() {
        legend_describes_each_value::<ImpactLevel>();
        legend_describes_each_value::<Status>();
        legend_describes_each_value::<Op>();
        legend_describes_each_value::<RecordStatus>();
        legend_describes_each_value::<PairStatus>();
        legend_describes_each_value::<Policy>();
    }

    #[test]
    fn the_legend_names_a_value_by_the_text_of_the_json() {
        assert!(legend::<PairStatus>().contains_key("same-content"));
        assert!(legend::<ImpactLevel>().contains_key("may-repave"));
        assert!(legend::<RecordStatus>().contains_key("unverified"));
    }
}
