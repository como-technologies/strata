//! `behind`: what a move to a newer version does. A pin and a profile version share the type of a
//! bump, from `compare` of the two versions.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::closed_set::{ClosedSet, Open};
use crate::diff::{ChangeFlag, Diff, ImpactLevel, PackDiff, Status};
use crate::model::validation::ProfileValidation;

/// What a move to the newest version does: the move of a pin to the newest version of its pack, or
/// the move of a profile version to the newest version of its profile.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Bump {
    /// The version compared: the newest one.
    pub to: String,
    /// The impact level of the change. For a profile version, the most severe level of its packs.
    pub impact: Open<ImpactLevel>,
    /// The change flags of the pack, in the order of the legend. For a profile version, each flag
    /// of a pack that the move changes.
    pub flags: Vec<Open<ChangeFlag>>,
    /// For a profile version: each pack that the move changes, adds, or removes, in the order of
    /// `compare`. Empty for a pin, which is one pack.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packs: Vec<BumpPack>,
    /// Whether Palette accepts the move, with the errors that it gives and the clusters that they
    /// name. Palette checks the move against the packs of the clusters that run the profile of
    /// each record. Strata asks the records that pin the pack in the project of the pin. Absent
    /// when strata can ask no record of the pin. Also absent when a request failed or strata did
    /// not ask a record, and no answer holds an error. Strata says that Palette accepts only when
    /// it asked each record and each request answered. Absent for a profile version: strata does
    /// not ask Palette about the move of a profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette_validation: Option<ProfileValidation>,
    /// Why a request to Palette failed, when one did. Also the records of another project that
    /// strata did not ask, such as `1 record of project p2 was not asked`. The pin stays behind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette_validation_error: Option<String>,
}

impl Bump {
    /// What a move to the version `to` does to the pack `name`, from the comparison `d`. Palette
    /// is not asked: the fields about its answer are empty.
    ///
    /// `d` compares the pinned version with `to`. None when `d` holds no pack of that name.
    pub fn of(to: &str, d: &Diff, name: &str) -> Option<Bump> {
        let pd = d.packs.iter().find(|p| p.name == name)?;
        Some(Bump {
            to: to.to_string(),
            impact: d.summary.max_impact.into(),
            flags: pd.flags().into_iter().map(Open::from).collect(),
            packs: Vec::new(),
            palette_validation: None,
            palette_validation_error: None,
        })
    }

    /// What a move of a profile version to the version `to` of its profile does, from the
    /// comparison `d` of the two versions. The impact is the most severe level that `d` gives.
    pub fn of_profile(to: &str, d: &Diff) -> Bump {
        let flags = ChangeFlag::ALL
            .iter()
            .filter(|flag| d.packs.iter().any(|p| p.flags().contains(flag)))
            .map(|&flag| Open::from(flag))
            .collect();
        Bump {
            to: to.to_string(),
            impact: d.summary.max_impact.into(),
            flags,
            packs: d.packs.iter().filter_map(BumpPack::of).collect(),
            palette_validation: None,
            palette_validation_error: None,
        }
    }
}

/// One pack that the move of a profile version changes, adds, or removes: see [`Bump::packs`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BumpPack {
    /// `<layer>/<name>`.
    pub key: String,
    /// `changed`, `added`, or `removed`, as `compare` gives it.
    pub status: Open<Status>,
    /// The version of the pack in the version that the records hold. Absent for a pack that the
    /// move adds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// The version of the pack in the newest version. Absent for a pack that the move removes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// The change flags of the pack, in the order of the legend. Empty for a pack that the move
    /// adds or removes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<Open<ChangeFlag>>,
    /// True when the values of the pack change.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub values_changed: bool,
}

impl BumpPack {
    /// The pack of `pd` in a move, or None for a pack that the move does not change.
    fn of(pd: &PackDiff) -> Option<BumpPack> {
        (pd.status != Status::Unchanged).then(|| BumpPack {
            key: pd.key.clone(),
            status: pd.status.into(),
            from: pd.a.as_ref().map(|a| a.version.clone()),
            to: pd.b.as_ref().map(|b| b.version.clone()),
            flags: pd.flags().into_iter().map(Open::from).collect(),
            values_changed: pd.values.changed,
        })
    }
}
