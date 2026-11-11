//! The lines of `behind`: the heading, the state of each pin, and its bump.

use super::{count_of, registry_sync_suffix};
use crate::model::validation::answer_lines;
use crate::text::and_list;

/// The status of a pin or of a profile version that strata could not check.
const NOT_CHECKED: &str = "not checked";
/// The status of a pin or of a profile version with no newer version.
const CURRENT: &str = "current";

/// `4 pins from 2 records: 1 behind`, with the floating pins, the pins not checked, and the
/// versions that arrived when there are any. When the records hold a profile version, the line
/// ends with `; 2 profile versions: 1 behind`, and the profile versions not checked.
pub(crate) fn behind_heading(r: &crate::ops::BehindReport) -> String {
    let s = &r.summary;
    let mut line = format!(
        "{} from {}: {} behind",
        count_of(s.pins, "pin"),
        count_of(r.records.len(), "record"),
        s.behind
    );
    if s.floating > 0 {
        line.push_str(&format!(", {} floating", s.floating));
    }
    if s.errors > 0 {
        line.push_str(&format!(", {} not checked", s.errors));
    }
    if let Some(since) = &r.since {
        line.push_str(&format!(
            "; {} arrived since {since}",
            count_of(s.arrived, "version")
        ));
    }
    if s.profiles > 0 {
        line.push_str(&format!(
            "; {}: {} behind",
            count_of(s.profiles, "profile version"),
            s.profiles_behind
        ));
    }
    if s.profile_errors > 0 {
        line.push_str(&format!(", {} not checked", s.profile_errors));
    }
    line
}

/// What a profile version is: `behind: 1.4.0, 1.3.2`, `current`, `gone`, or `not checked`. The
/// newer versions are newest first, as for a pin.
pub(crate) fn profile_status(p: &crate::ops::ProfilePin) -> String {
    if p.error.is_some() {
        NOT_CHECKED.to_string()
    } else if p.gone {
        "gone".to_string()
    } else if p.behind {
        let newer: Vec<&str> = p.newer.iter().map(|n| n.version.as_str()).collect();
        format!("behind: {}", newer.join(", "))
    } else {
        CURRENT.to_string()
    }
}

/// `1.4.0 created 2026-09-17T09:30:00Z`: when Palette created the newest version of a profile
/// version that is behind. None when it is current, or Palette gives no time.
pub(crate) fn created_since(p: &crate::ops::ProfilePin) -> Option<String> {
    let newest = p.newer.first().filter(|n| !n.created.is_empty())?;
    Some(format!("{} created {}", newest.version, newest.created))
}

/// What a pin is: `behind: 1.65.0, 1.64.0`, `current`, or `not checked`, with its floating tag and
/// the version the tag names now, the state of the pinned version when the registry gives one,
/// and the last sync of the registry when that sync did not complete. Each newer version names
/// its state and what a move to it drops: `1.64.0 (deprecated, drops arm64)`.
pub(crate) fn pin_status(pin: &crate::ops::Pin) -> String {
    let mut text = if pin.error.is_some() {
        NOT_CHECKED.to_string()
    } else if pin.behind {
        let newer: Vec<String> = pin.newer.iter().map(newer_version).collect();
        format!("behind: {}", newer.join(", "))
    } else {
        CURRENT.to_string()
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
pub(crate) fn listed_since(pin: &crate::ops::Pin) -> Option<String> {
    let newest = pin
        .newer
        .first()
        .filter(|n| pin.behind && !n.listed.is_empty())?;
    Some(format!("{} listed since {}", newest.version, newest.listed))
}

/// `arrived: 1.65.0, 1.66.0`: the newer versions of a pin that arrived since the time of
/// `--since`. None when none arrived, or the report has no such time.
pub(crate) fn arrived_line(pin: &crate::ops::Pin) -> Option<String> {
    let arrived = pin.arrived.as_ref().filter(|a| !a.is_empty())?;
    Some(format!("arrived: {}", arrived.join(", ")))
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
pub(crate) fn palette_lines(pin: &crate::ops::Pin) -> Vec<String> {
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
pub(crate) fn bump_line(pin: &crate::ops::Pin) -> Option<String> {
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
pub(crate) fn profile_bump_line(p: &crate::ops::ProfilePin) -> Option<String> {
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
