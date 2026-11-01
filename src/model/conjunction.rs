//! How the conditions of a group join in the search of Palette: `and` or `or`.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::closed_set::{ClosedSet, impl_closed_set};

/// How the conditions of a group join, as the search of Palette names it: `and` or `or`. A saved
/// tag filter holds one, and each group of a search request gives one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Conjunction {
    /// Each condition must hold. Palette joins with `and` when a group names no conjunction.
    #[default]
    And,
    /// One condition or more must hold.
    Or,
}

impl_closed_set!(Conjunction {
    And => "and",
    Or => "or",
});

impl fmt::Display for Conjunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conjunction_prints_as_palette_names_it_and_reads_no_other_text() {
        for conjunction in Conjunction::ALL {
            assert_eq!(conjunction.to_string(), conjunction.as_str());
        }
        assert_eq!(Conjunction::parse("AND"), None);
        assert_eq!(Conjunction::parse(""), None);
        assert_eq!(Conjunction::default(), Conjunction::And);
    }
}
