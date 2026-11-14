//! `behind`: which versions are newer than a pin or a profile version, and what a move to one
//! drops.

use std::collections::BTreeSet;

use super::{Newer, Offered, Pin};
use crate::model::cloud::supports;
use crate::ops::packs::PackBuild;
use crate::reference::version;

/// The versions above the version of `pin` that a move can go to, newest first. `pinned` is the
/// build of the pinned version, when the registry still lists it. `pre_release` also names a
/// pre-release above a pin that is not one: see [`is_newer`].
pub(super) fn newer_versions(
    pin: &Pin,
    offered: &Offered,
    pinned: Option<&PackBuild>,
    pre_release: bool,
) -> Vec<Newer> {
    let clouds: &[String] = pinned.map_or(&[], |b| b.cloud_types.as_slice());
    let architectures_of = |version: &str| -> &[String] {
        offered
            .architectures
            .get(version)
            .map_or(&[], Vec::as_slice)
    };
    let architectures = pinned.map_or(&[][..], |b| architectures_of(&b.version));
    // a newer version fits when it supports one cloud of the pinned version. A pinned version
    // with no cloud type supports each cloud, so each newer version fits it
    let fits =
        |b: &PackBuild| clouds.is_empty() || clouds.iter().any(|c| supports(&b.cloud_types, c));
    offered
        .builds
        .iter()
        .filter(|b| is_newer(&b.version, &pin.version, pre_release))
        .filter(|b| !b.retired())
        .filter(|b| fits(b))
        .map(|b| Newer {
            version: b.version.clone(),
            state: b.state.clone(),
            palette_latest: b.palette_latest,
            listed: b.listed.clone(),
            drops: dropped(architectures, architectures_of(&b.version)),
        })
        .collect()
}

impl Pin {
    /// The newest of the newer versions of the minor version of the pin, when the newest version
    /// is of another minor version. A pin of `1.36.2` that is behind `1.37.0` and `1.36.4` gives
    /// `1.36.4`: a team can take that patch before the move to the newest version.
    ///
    /// None when the pin is current, when the newest version is of the minor version of the pin,
    /// and when the version of the pin does not parse. [`version::line`] gives the minor version.
    pub(crate) fn next_patch(&self) -> Option<&Newer> {
        let minor = |v: &str| version::line(v, 2);
        let own = minor(&self.version)?;
        let newest = self.newer.first()?;
        if minor(&newest.version).as_ref() == Some(&own) {
            return None;
        }
        self.newer
            .iter()
            .find(|n| minor(&n.version).as_ref() == Some(&own))
    }
}

/// True when `candidate` is a newer version than `pin`: it is above `pin` by [`version::compare`].
///
/// A pre-release is below its version, so a pin of `1.0.1` has no newer version in `1.0.1-rc.1`.
/// A pre-release above a pin that is not one is still not a version to move to, unless
/// `pre_release` asks for it. A pin that is a pre-release follows the pre-releases: it has each
/// version above it, and its final version is the first of them.
pub(super) fn is_newer(candidate: &str, pin: &str, pre_release: bool) -> bool {
    version::compare(candidate, pin).is_gt()
        && (pre_release || version::is_pre_release(pin) || !version::is_pre_release(candidate))
}

/// The architectures of `pinned` that `newer` does not support: what a move drops. When the
/// listing names the architectures of only one of the two versions, strata cannot say what a
/// move drops, and the answer is empty.
fn dropped(pinned: &[String], newer: &[String]) -> Vec<String> {
    if pinned.is_empty() || newer.is_empty() {
        return Vec::new();
    }
    let newer: BTreeSet<&String> = newer.iter().collect();
    pinned
        .iter()
        .filter(|a| !newer.contains(a))
        .cloned()
        .collect()
}
