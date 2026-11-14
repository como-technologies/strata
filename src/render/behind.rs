//! The lines of `behind`: the heading, the state of each pin, its bump, and its carriers.

use super::{count_of, entry_error, registry_sync_suffix};
use crate::closed_set::impl_texts;
use crate::model::validation::answer_lines;
use crate::ops::{BehindReport, Pin, PinnedBy, ProfilePin};
use crate::reference::{Ref, name_version};
use crate::text::{and_list, name_or_uid, shell_words};

/// The state of a pin or of a profile version in the report of `behind`, as its status names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    /// The registry or Palette has a newer version.
    Behind,
    /// No newer version exists.
    Current,
    /// Palette no longer holds the profile version.
    Gone,
    /// Strata could not check it.
    NotChecked,
}

impl_texts!(pub RowState {
    Behind => "behind",
    Current => "current",
    Gone => "gone",
    NotChecked => "not checked",
});

impl RowState {
    /// The state of `pin`. A pin whose bump strata could not compare is not checked, also when it
    /// is behind.
    pub fn of_pin(pin: &Pin) -> RowState {
        if pin.error.is_some() {
            RowState::NotChecked
        } else if pin.behind {
            RowState::Behind
        } else {
            RowState::Current
        }
    }

    /// The state of the profile version `p`.
    pub fn of_profile(p: &ProfilePin) -> RowState {
        if p.error.is_some() {
            RowState::NotChecked
        } else if p.gone {
            RowState::Gone
        } else if p.behind {
            RowState::Behind
        } else {
            RowState::Current
        }
    }
}

/// `9 pins (2 with a floating tag) from 2 records: 4 behind, 5 current`. The counts after the
/// colon add up to the pins: see [`state_counts`]. Then the versions that arrived, when the report
/// has the time of `--since`. When the records hold a profile version, the line ends with
/// `; 2 profile versions: 1 behind, 1 current`, in the same form.
pub(crate) fn behind_heading(r: &BehindReport) -> String {
    let s = &r.summary;
    let floating = if s.floating > 0 {
        format!(" ({} with a floating tag)", s.floating)
    } else {
        String::new()
    };
    let mut line = format!(
        "{}{floating} from {}: {}",
        count_of(s.pins, "pin"),
        count_of(r.records.len(), "record"),
        state_counts(r.pins.iter().map(RowState::of_pin))
    );
    if let Some(since) = &r.since {
        line.push_str(&format!(
            "; {} arrived since {since}",
            count_of(s.arrived, "version")
        ));
    }
    if s.profiles > 0 {
        line.push_str(&format!(
            "; {}: {}",
            count_of(s.profiles, "profile version"),
            state_counts(r.profiles.iter().map(RowState::of_profile))
        ));
    }
    line
}

/// `4 behind, 5 current, 1 not checked`: the count of each state of `states`, in the order of
/// [`RowState`]. `behind` is always there, and each other state only when a row has it. So the
/// counts add up to the rows, and no row counts two times.
fn state_counts(states: impl Iterator<Item = RowState>) -> String {
    let states: Vec<RowState> = states.collect();
    let counts: Vec<String> = RowState::ALL
        .iter()
        .filter_map(|&state| {
            let n = states.iter().filter(|&&s| s == state).count();
            (n > 0 || state == RowState::Behind).then(|| format!("{n} {}", state.as_str()))
        })
        .collect();
    counts.join(", ")
}

/// The pins of `r` that the report shows: each pin, or with `behind_only`, each pin that is not
/// current.
pub(crate) fn shown_pins(r: &BehindReport, behind_only: bool) -> Vec<&Pin> {
    r.pins
        .iter()
        .filter(|p| !behind_only || RowState::of_pin(p) != RowState::Current)
        .collect()
}

/// The profile versions of `r` that the report shows, as [`shown_pins`] gives the pins.
pub(crate) fn shown_profiles(r: &BehindReport, behind_only: bool) -> Vec<&ProfilePin> {
    r.profiles
        .iter()
        .filter(|p| !behind_only || RowState::of_profile(p) != RowState::Current)
        .collect()
}

/// The title of a record that pins a pack or holds a profile version: the title with only the
/// profile that holds the entry, when the record names it, else the title of the record.
pub(crate) fn record_title(by: &PinnedBy) -> &str {
    by.owner.as_deref().unwrap_or(&by.subject)
}

/// What a profile version is: `behind: 1.4.0, 1.3.2`, `current`, `gone`, or `not checked`. The
/// newer versions are newest first, as for a pin.
pub(crate) fn profile_status(p: &ProfilePin) -> String {
    match RowState::of_profile(p) {
        RowState::Behind => {
            let newer: Vec<&str> = p.newer.iter().map(|n| n.version.as_str()).collect();
            format!("behind: {}", newer.join(", "))
        }
        state => state.as_str().to_string(),
    }
}

/// `1.4.0 created 2026-09-17T09:30:00Z`: when Palette created the newest version of a profile
/// version that is behind. None when it is current, or Palette gives no time.
pub(crate) fn created_since(p: &ProfilePin) -> Option<String> {
    let newest = p.newer.first().filter(|n| !n.created.is_empty())?;
    Some(format!("{} created {}", newest.version, newest.created))
}

/// What a pin is: `behind: 1.65.0, 1.64.0`, `current`, or `not checked`, with its floating tag and
/// the version the tag names now, the state of the pinned version when the registry gives one,
/// and the last sync of the registry when that sync did not complete. Each newer version names
/// its state and what a move to it drops: `1.64.0 (deprecated, drops arm64)`.
pub(crate) fn pin_status(pin: &Pin) -> String {
    let mut text = match RowState::of_pin(pin) {
        RowState::Behind => {
            let newer: Vec<String> = pin.newer.iter().map(newer_version).collect();
            format!("behind: {}", newer.join(", "))
        }
        state => state.as_str().to_string(),
    };
    if pin.floating {
        let now = pin
            .resolved
            .as_deref()
            .map(|v| format!(", now {v}"))
            .unwrap_or_default();
        text.push_str(&format!(" (floating tag {}{now})", pin.tag));
    }
    if let Some(state) = &pin.pinned_state {
        text.push_str(&format!("; the pinned version is {state}"));
    }
    text.push_str(&registry_sync_suffix(pin.registry_sync.as_ref()));
    text
}

/// `1.65.0 listed since 2026-09-17T09:30:00Z`: when Palette first listed the newest version of a
/// pin that is behind. None when the pin is current, or the listing gives no time.
pub(crate) fn listed_since(pin: &Pin) -> Option<String> {
    let newest = pin
        .newer
        .first()
        .filter(|n| pin.behind && !n.listed.is_empty())?;
    Some(format!("{} listed since {}", newest.version, newest.listed))
}

/// The line `next patch` of a pin: the newest patch of the minor version of the pin, and the
/// command that gives what a move to it does.
///
/// An example is `next patch 1.36.4: strata compare <pin> <patch>`, with the pack ref of each
/// side, such as `'pack:Public Repo/kubernetes@1.36.2'`. Strata sends no request for the
/// line. None when the pin has no such patch: see [`Pin::next_patch`].
pub(crate) fn next_patch_line(pin: &Pin) -> Option<String> {
    let patch = pin.next_patch()?;
    let registry = name_or_uid(&pin.registry, &pin.registry_uid);
    let pack = |version: &str| {
        Ref::Pack {
            registry: Some(registry.to_string()),
            name: pin.name.clone(),
            version: version.to_string(),
        }
        .to_string()
    };
    let command = shell_words(&[
        "strata",
        "compare",
        &pack(&pin.version),
        &pack(&patch.version),
    ]);
    Some(format!("next patch {}: {command}", patch.version))
}

/// `arrived: 1.65.0, 1.66.0`: the newer versions of a pin that arrived since the time of
/// `--since`. None when none arrived, or the report has no such time.
pub(crate) fn arrived_line(pin: &Pin) -> Option<String> {
    let arrived = pin.arrived.as_ref().filter(|a| !a.is_empty())?;
    Some(format!("arrived: {}", arrived.join(", ")))
}

/// One line for each profile version that holds a pin that is behind, and has a newer version:
/// `barney-addons@1.4.0 carries fred-compositions@0.2.0`, or `no newer version of
/// barney-addons@1.3.1 carries a newer fred-compositions`. When strata could not read a version,
/// the line gives the error.
pub(crate) fn carrier_lines(pin: &Pin) -> Vec<String> {
    pin.carriers
        .iter()
        .map(|c| {
            let held = name_version(&c.profile, &c.version);
            match (&c.carrier, entry_error(c.error.as_ref())) {
                (Some(by), _) => format!(
                    "{} carries {}",
                    name_version(&c.profile, &by.version),
                    name_version(&pin.name, &by.pack_version)
                ),
                (None, Some(msg)) => format!(
                    "strata could not read which newer version of {held} carries a newer {} ({msg})",
                    pin.name
                ),
                (None, None) => format!("no newer version of {held} carries a newer {}", pin.name),
            }
        })
        .collect()
}

/// One newer version of a pin, with its notes in parentheses: `1.65.0`, `1.64.0 (deprecated)`,
/// `1.66.0 (drops arm64)`, or `1.64.0 (deprecated, drops arm64)`.
fn newer_version(n: &crate::ops::Newer) -> String {
    let drops = (!n.drops.is_empty()).then(|| format!("drops {}", and_list(&n.drops)));
    let notes: Vec<String> = n
        .state
        .iter()
        .map(ToString::to_string)
        .chain(drops)
        .collect();
    if notes.is_empty() {
        n.version.clone()
    } else {
        format!("{} ({})", n.version, notes.join(", "))
    }
}

/// The answer of Palette about the bump of a pin, one line each: `palette accepts`, or `palette
/// refuses: DependencyPackUpgradeNotSupported on payments` for each error. An error that names no
/// cluster gives its pack and its message. When a request failed, or strata did not ask a record of
/// another project, a line gives the message: `palette gave no answer: <message>`. Empty when
/// Palette was not asked.
pub(crate) fn palette_lines(pin: &Pin) -> Vec<String> {
    let Some(bump) = pin.bump.as_ref() else {
        return Vec::new();
    };
    let answer = bump
        .palette_validation
        .iter()
        .flat_map(|v| answer_lines(v.errors()))
        .map(|line| format!("palette {line}"));
    let failed = bump
        .palette_validation_error
        .iter()
        .map(|e| format!("palette gave no answer: {e}"));
    answer.chain(failed).collect()
}

/// `bump to 1.65.0: in-place IDENTITY UPSTREAM AUTHORED`, for a pin with a bump. When Palette
/// calls a different newer version the latest, the line ends with `; Palette calls 1.30.0 the
/// latest`.
pub(crate) fn bump_line(pin: &Pin) -> Option<String> {
    let b = pin.bump.as_ref()?;
    let mut line = bump_text(b);
    if let Some(n) = pin
        .newer
        .iter()
        .find(|n| n.palette_latest && n.version != b.to)
    {
        line.push_str(&format!("; Palette calls {} the latest", n.version));
    }
    Some(line)
}

/// `bump to 1.4.0: in-place IDENTITY AUTHORED`, for a profile version with a bump: the impact
/// level and the flags of the packs that the move changes.
pub(crate) fn profile_bump_line(p: &ProfilePin) -> Option<String> {
    p.bump.as_ref().map(bump_text)
}

/// `bump to <version>: <impact> <flags>`: the words that a pin and a profile version share.
fn bump_text(b: &crate::ops::Bump) -> String {
    let mut line = format!("bump to {}: {}", b.to, b.impact.as_str());
    for f in &b.flags {
        line.push_str(&format!(" {}", f.as_str()));
    }
    line
}
