//! The markdown of `behind`: the heading, one table of the pins, and one table of the profile
//! versions.

use crate::closed_set::ClosedSet;
use crate::ops::{BehindReport, Pin, PinnedBy, ProfilePin};
use crate::render::behind::{
    arrived_line, behind_heading, bump_line, created_since, listed_since, palette_lines,
    pin_status, profile_bump_line, profile_status,
};
use crate::render::entry_error;
use crate::text::{dash, markdown_cell};

/// The report of `behind` as markdown. A table that has no row is not there.
pub(super) fn markdown_behind(r: &BehindReport) -> String {
    let mut o = format!("### behind: {}\n\n", markdown_cell(&behind_heading(r)));
    if !r.pins.is_empty() {
        o.push_str(
            "| pack | pinned | registry | status | bump | pinned by |\n|---|---|---|---|---|---|\n",
        );
        for pin in &r.pins {
            o.push_str(&pin_row(pin));
        }
    }
    if !r.profiles.is_empty() {
        if !r.pins.is_empty() {
            o.push('\n');
        }
        o.push_str(
            "| profile | version | scope | status | bump | pinned by |\n|---|---|---|---|---|---|\n",
        );
        for p in &r.profiles {
            o.push_str(&profile_row(p));
        }
    }
    o
}

/// The row of one pin.
fn pin_row(pin: &Pin) -> String {
    let mut status = pin_status(pin);
    for line in [listed_since(pin), arrived_line(pin)].into_iter().flatten() {
        status.push_str(&format!("; {line}"));
    }
    if let Some(msg) = entry_error(pin.error.as_ref()) {
        status.push_str(&format!(" ({msg})"));
    }
    let bump: Vec<String> = bump_line(pin)
        .into_iter()
        .chain(palette_lines(pin))
        .map(|l| markdown_cell(&l))
        .collect();
    format!(
        "| `{}` | `{}` | {} | {} | {} | {} |\n",
        markdown_cell(&pin.name),
        markdown_cell(&pin.version),
        markdown_cell(dash(&pin.registry)),
        markdown_cell(&status),
        if bump.is_empty() {
            "-".to_string()
        } else {
            bump.join("<br>")
        },
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
    let bump = profile_bump_line(p).unwrap_or_default();
    format!(
        "| `{}` | `{}` | {} | {} | {} | {} |\n",
        markdown_cell(&p.name),
        markdown_cell(&p.version),
        markdown_cell(dash(scope)),
        markdown_cell(&status),
        markdown_cell(dash(&bump)),
        pinned_by(&p.pinned_by)
    )
}

/// The cell of the records that pin a pack or hold a profile version: `` `path` (subject) ``, one
/// for each record.
fn pinned_by(by: &[PinnedBy]) -> String {
    let cells: Vec<String> = by
        .iter()
        .map(|b| {
            format!(
                "`{}` ({})",
                markdown_cell(&b.path),
                markdown_cell(&b.subject)
            )
        })
        .collect();
    cells.join("<br>")
}
