//! The state of a pack version in a registry: `deprecated`, `disabled`, `deleted`, or `absent`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::closed_set::impl_closed_set;

/// The state of a pack version, as the listing of its registry gives it and as `behind` names it.
///
/// A version in use has no state. A state of `disabled` or `deleted` is a version that Palette no
/// longer serves: see [`PackState::is_retired`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PackState {
    /// Spectro Cloud deprecated the version. Palette still serves it.
    Deprecated,
    /// Spectro Cloud disabled the version, or marked it `disabled`. Palette serves no such
    /// version, and refuses a profile that pins one.
    Disabled,
    /// Spectro Cloud deleted the version. Palette serves no such version, and refuses a profile
    /// that pins one.
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
    /// True for a state of a version that Palette no longer serves: `disabled` or `deleted`.
    pub(crate) fn is_retired(self) -> bool {
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
}
