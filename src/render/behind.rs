//! The lines of `behind`: the heading, the state of each pin, and its bump.

use super::{count_of, registry_sync_suffix};
use crate::text::and_list;

/// `4 pins from 2 records: 1 behind`, with the floating pins, the pins not checked, and the
/// versions that arrived when there are any.
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
    line
}

/// What a pin is: `behind: 1.65.0, 1.64.0`, `current`, or `not checked`, with its floating tag and
/// the version the tag names now, the state of the pinned version when the registry gives one,
/// and the last sync of the registry when that sync did not complete. Each newer version names
/// its state and what a move to it drops: `1.64.0 (deprecated, drops arm64)`.
pub(crate) fn pin_status(pin: &crate::ops::Pin) -> String {
    let mut text = if pin.error.is_some() {
        "not checked".to_string()
    } else if pin.behind {
        let newer: Vec<String> = pin.newer.iter().map(newer_version).collect();
        format!("behind: {}", newer.join(", "))
    } else {
        "current".to_string()
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

/// One newer version of a pin, with its notes in parentheses: `1.65.0`, `1.64.0 (deprecated)`,
/// `1.66.0 (drops arm64)`, or `1.64.0 (deprecated, drops arm64)`.
fn newer_version(n: &crate::ops::Newer) -> String {
    let drops = (!n.drops.is_empty()).then(|| format!("drops {}", and_list(&n.drops)));
    let notes: Vec<String> = n.state.iter().cloned().chain(drops).collect();
    if notes.is_empty() {
        n.version.clone()
    } else {
        format!("{} ({})", n.version, notes.join(", "))
    }
}

/// `bump to 1.65.0: in-place IDENTITY UPSTREAM AUTHORED`, for a pin with a bump. When Palette
/// calls a different newer version the latest, the line ends with `; Palette calls 1.30.0 the
/// latest`.
pub(crate) fn bump_line(pin: &crate::ops::Pin) -> Option<String> {
    let b = pin.bump.as_ref()?;
    let mut line = format!("bump to {}: {}", b.to, b.impact.as_str());
    for f in &b.flags {
        line.push_str(&format!(" {}", f.as_str()));
    }
    if let Some(n) = pin
        .newer
        .iter()
        .find(|n| n.palette_latest && n.version != b.to)
    {
        line.push_str(&format!("; Palette calls {} the latest", n.version));
    }
    Some(line)
}
