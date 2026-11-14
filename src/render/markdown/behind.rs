//! The markdown of `behind`: the heading, one table of the pins, and one table of the profile
//! versions.

use crate::closed_set::ClosedSet;
use crate::ops::{BehindReport, Pin, PinnedBy, ProfilePin};
use crate::render::behind::{
    arrived_line, behind_heading, bump_line, carrier_lines, created_since, listed_since,
    next_patch_line, palette_lines, pin_status, profile_bump_line, profile_status, record_title,
    shown_pins, shown_profiles, update_check_lines,
};
use crate::render::entry_error;
use crate::text::{code_cell, dash, lines_cell, markdown_cell};

/// The report of `behind` as markdown. A table that has no row is not there. With `behind_only`,
/// no table has a row that is current.
pub(super) fn markdown_behind(r: &BehindReport, behind_only: bool) -> String {
    let mut o = format!("### behind: {}\n\n", markdown_cell(&behind_heading(r)));
    let pins = shown_pins(r, behind_only);
    let profiles = shown_profiles(r, behind_only);
    if !pins.is_empty() {
        o.push_str(
            "| pack | pinned | registry | status | bump | pinned by |\n|---|---|---|---|---|---|\n",
        );
        for pin in &pins {
            o.push_str(&pin_row(pin));
        }
    }
    if !profiles.is_empty() {
        if !pins.is_empty() {
            o.push('\n');
        }
        o.push_str(
            "| profile | version | scope | status | bump | pinned by |\n|---|---|---|---|---|---|\n",
        );
        for p in &profiles {
            o.push_str(&profile_row(p));
        }
    }
    o
}

/// The row of one pin.
fn pin_row(pin: &Pin) -> String {
    let mut status = pin_status(pin);
    let lines = [listed_since(pin), next_patch_line(pin), arrived_line(pin)];
    for line in lines.into_iter().flatten() {
        status.push_str(&format!("; {line}"));
    }
    if let Some(msg) = entry_error(pin.error.as_ref()) {
        status.push_str(&format!(" ({msg})"));
    }
    for line in carrier_lines(pin) {
        status.push_str(&format!("; {line}"));
    }
    let bump = bump_line(pin)
        .into_iter()
        .chain(palette_lines(pin))
        .chain(update_check_lines(pin.bump.as_ref()));
    format!(
        "| {} | {} | {} | {} | {} | {} |\n",
        code_cell(&pin.name),
        code_cell(&pin.version),
        markdown_cell(dash(&pin.registry)),
        markdown_cell(&status),
        lines_cell(bump),
        pinned_by(&pin.pinned_by)
    )
}

/// The row of one profile version.
fn profile_row(p: &ProfilePin) -> String {
    let mut status = profile_status(p);
    if let Some(line) = created_since(p) {
        status.push_str(&format!("; {line}"));
    }
    if let Some(msg) = entry_error(p.error.as_ref()) {
        status.push_str(&format!(" ({msg})"));
    }
    let scope = p.scope.map(|s| s.as_str()).unwrap_or_default();
    let bump = profile_bump_line(p)
        .into_iter()
        .chain(update_check_lines(p.bump.as_ref()));
    format!(
        "| {} | {} | {} | {} | {} | {} |\n",
        code_cell(&p.name),
        code_cell(&p.version),
        markdown_cell(dash(scope)),
        markdown_cell(&status),
        lines_cell(bump),
        pinned_by(&p.pinned_by)
    )
}

/// The cell of the records that pin a pack or hold a profile version: `` `path` (title) ``, one
/// for each record. The title is that of [`record_title`].
fn pinned_by(by: &[PinnedBy]) -> String {
    let cells: Vec<String> = by
        .iter()
        .map(|b| {
            format!(
                "{} ({})",
                code_cell(&b.path),
                markdown_cell(record_title(b))
            )
        })
        .collect();
    cells.join("<br>")
}
