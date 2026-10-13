//! How the conditions of a group join in the search of Palette: `and` or `or`.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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

impl Conjunction {
    /// The conjunction as Palette names it: `and` or `or`.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::And => "and",
            Self::Or => "or",
        }
    }

    /// The conjunction that the text names, or `None` for another text.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "and" => Some(Self::And),
            "or" => Some(Self::Or),
            _ => None,
        }
    }
}

impl fmt::Display for Conjunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conjunction_parses_and_prints_as_palette_names_it() {
        for conjunction in [Conjunction::And, Conjunction::Or] {
            assert_eq!(Conjunction::parse(conjunction.as_str()), Some(conjunction));
            let json = serde_json::to_value(conjunction).unwrap();
            assert_eq!(json, conjunction.as_str());
            assert_eq!(
                serde_json::from_value::<Conjunction>(json).unwrap(),
                conjunction
            );
        }
        assert_eq!(Conjunction::parse("AND"), None);
        assert_eq!(Conjunction::parse(""), None);
        assert_eq!(Conjunction::default(), Conjunction::And);
    }
}
