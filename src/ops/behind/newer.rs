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

/// The newest of the versions `newer` of the minor version of `pinned`, when the newest version is
/// of another minor version. This is [`Pin::next_patch`].
///
/// A pin of `1.36.2` that is behind `1.37.0` and `1.36.4` gives `1.36.4`. A team can take that
/// patch before the move to the newest version. `newer` is newest first, as [`newer_versions`]
/// gives it. None when `newer` is empty, when the newest version is of the minor version of
/// `pinned`, and when `pinned` does not parse. None also when `newer` has no version of that minor
/// version. [`version::line`] gives the minor version.
pub(super) fn next_patch(pinned: &str, newer: &[Newer]) -> Option<String> {
    let minor = |v: &str| version::line(v, 2);
    let own = minor(pinned)?;
    let newest = newer.first()?;
    if minor(&newest.version).as_ref() == Some(&own) {
        return None;
    }
    newer
        .iter()
        .find(|n| minor(&n.version).as_ref() == Some(&own))
        .map(|n| n.version.clone())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The newer versions `versions`, newest first, as the registry lists them.
    fn newer(versions: &[&str]) -> Vec<Newer> {
        versions
            .iter()
            .map(|v| Newer {
                version: v.to_string(),
                state: None,
                palette_latest: false,
                listed: String::new(),
                drops: Vec::new(),
            })
            .collect()
    }

    #[test]
    fn the_next_patch_is_the_newest_patch_of_the_minor_version_of_the_pin_under_a_newer_minor() {
        let cases = [
            (
                "1.36.2",
                &["1.37.0", "1.36.4", "1.36.3"][..],
                Some("1.36.4"),
            ),
            ("1.21.1", &["1.22.0", "1.21.3"], Some("1.21.3")),
            // the newest version is of the minor version of the pin: the bump names it
            ("1.36.3", &["1.36.4"], None),
            // no newer version of the minor version of the pin
            ("1.63.1", &["1.65.0", "1.64.0"], None),
            // a version that does not parse has no minor version
            ("LTS__20.4.x", &["24.04"], None),
            // a pin that is current
            ("1.36.4", &[], None),
        ];
        for (pinned, versions, patch) in cases {
            assert_eq!(
                next_patch(pinned, &newer(versions)).as_deref(),
                patch,
                "{pinned} behind {versions:?}"
            );
        }
    }
}
