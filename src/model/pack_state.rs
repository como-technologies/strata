//! The state of a pack version in a registry: `deprecated`, `disabled`, `deleted`, or `absent`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::closed_set::impl_closed_set;

/// The state of a pack version, as the listing of its registry gives it and as `behind` names it.
///
/// A version in use has no state. Palette refuses a profile that pins a version in the state
/// `disabled` or `deleted`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PackState {
    /// The `system_state` of the version is `deprecated`.
    Deprecated,
    /// The version is disabled: its `system_state` says so, or the listing marks it. Palette
    /// refuses a profile that pins it.
    Disabled,
    /// The `system_state` of the version is `deleted`. Palette serves no such version, and
    /// refuses a profile that pins one.
    Deleted,
    /// The registry no longer lists the version, or does not serve it in this tenant. `behind`
    /// gives this state to a pinned version. A listing never gives it.
    Absent,
}

impl_closed_set!(PackState {
    Deprecated => "deprecated",
    Disabled => "disabled",
    Deleted => "deleted",
    Absent => "absent",
});

impl PackState {
    /// True for a state that a listing of Palette gives: each state but `absent`, which only
    /// strata gives.
    pub(crate) fn listed(self) -> bool {
        match self {
            Self::Deprecated | Self::Disabled | Self::Deleted => true,
            Self::Absent => false,
        }
    }

    /// True for `disabled` and `deleted`: a profile cannot pin a version in such a state.
    pub fn is_retired(self) -> bool {
        matches!(self, Self::Disabled | Self::Deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::closed_set::ClosedSet;

    #[test]
    fn only_a_disabled_or_a_deleted_version_is_retired() {
        let retired: Vec<PackState> = PackState::ALL
            .iter()
            .copied()
            .filter(|s| s.is_retired())
            .collect();
        assert_eq!(retired, [PackState::Disabled, PackState::Deleted]);
    }

    #[test]
    fn each_state_but_absent_is_listed_by_palette() {
        let listed: Vec<PackState> = PackState::ALL
            .iter()
            .copied()
            .filter(|s| s.listed())
            .collect();
        assert_eq!(
            listed,
            [
                PackState::Deprecated,
                PackState::Disabled,
                PackState::Deleted
            ]
        );
    }
}
