//! The text of `behind` for the terminal: the heading, the lines of each pin, and one line for
//! each profile version.

use crate::closed_set::ClosedSet;
use crate::ops::{BehindReport, PinnedBy, ProfilePin};
use crate::render::behind::{
    arrived_line, behind_heading, bump_line, carrier_lines, created_since, listed_since,
    palette_lines, pin_status, profile_bump_line, profile_status,
};
use crate::render::entry_error;
use crate::term::color::Paint;
use crate::text::dash;

/// The report of `behind` for the terminal: the heading, the pins, and then the profile versions.
pub(super) fn terminal_behind(r: &BehindReport, p: &Paint) -> String {
    let mut out = format!("{}\n", behind_heading(r));
    let name_w = r.pins.iter().map(|x| x.name.len()).max().unwrap_or(0);
    let ver_w = r.pins.iter().map(|x| x.version.len()).max().unwrap_or(0);
    let reg_w = r
        .pins
        .iter()
        .map(|x| dash(&x.registry).len())
        .max()
        .unwrap_or(0);
    for pin in &r.pins {
        let status = pin_status(pin);
        let status = if pin.error.is_some() {
            p.byellow(&status)
        } else if pin.behind {
            p.yellow(&status)
        } else {
            p.dim(&status)
        };
        out.push_str(&format!(
            "{} [{:<5}] {}  {}  {}  {status}\n",
            if pin.behind { "~" } else { " " },
            pin.layer,
            p.bold(&format!("{:<name_w$}", pin.name)),
            format_args!("{:<ver_w$}", pin.version),
            format_args!("{:<reg_w$}", dash(&pin.registry)),
        ));
        if let Some(listed) = listed_since(pin) {
            out.push_str(&format!("      {}\n", p.dim(&listed)));
        }
        if let Some(line) = bump_line(pin) {
            out.push_str(&format!("      {line}\n"));
        }
        for line in palette_lines(pin) {
            out.push_str(&format!("      {line}\n"));
        }
        if let Some(arrived) = arrived_line(pin) {
            out.push_str(&format!("      {}\n", p.cyan(&arrived)));
        }
        if let Some(msg) = entry_error(pin.error.as_ref()) {
            out.push_str(&format!("      {}\n", p.dim(&format!("({msg})"))));
        }
        for line in carrier_lines(pin) {
            out.push_str(&format!("      {line}\n"));
        }
        out.push_str(&pinned_by_lines(&pin.pinned_by, p));
    }
    out.push_str(&profile_lines(&r.profiles, p));
    out
}

/// `pinned by <path> (<subject>)`, dim, under a pin or a profile version: one line for each
/// record.
fn pinned_by_lines(by: &[PinnedBy], p: &Paint) -> String {
    by.iter()
        .map(|b| {
            format!(
                "      {}\n",
                p.dim(&format!("pinned by {} ({})", b.path, b.subject))
            )
        })
        .collect()
}

/// The lines of the profile versions of `behind`, after the pins: one line for each version, in
/// the form of the line of a pin. The scope takes the place of the registry.
fn profile_lines(profiles: &[ProfilePin], p: &Paint) -> String {
    let scope = |x: &ProfilePin| x.scope.map_or("-", |s| s.as_str());
    let name_w = profiles.iter().map(|x| x.name.len()).max().unwrap_or(0);
    let ver_w = profiles.iter().map(|x| x.version.len()).max().unwrap_or(0);
    let scope_w = profiles.iter().map(|x| scope(x).len()).max().unwrap_or(0);
    let mut out = String::new();
    for x in profiles {
        let status = profile_status(x);
        let status = if x.error.is_some() {
            p.byellow(&status)
        } else if x.behind {
            p.yellow(&status)
        } else {
            p.dim(&status)
        };
        out.push_str(&format!(
            "{} [profile] {}  {}  {}  {status}\n",
            if x.behind { "~" } else { " " },
            p.bold(&format!("{:<name_w$}", x.name)),
            format_args!("{:<ver_w$}", x.version),
            format_args!("{:<scope_w$}", scope(x)),
        ));
        if let Some(created) = created_since(x) {
            out.push_str(&format!("      {}\n", p.dim(&created)));
        }
        if let Some(line) = profile_bump_line(x) {
            out.push_str(&format!("      {line}\n"));
        }
        if let Some(msg) = entry_error(x.error.as_ref()) {
            out.push_str(&format!("      {}\n", p.dim(&format!("({msg})"))));
        }
        out.push_str(&pinned_by_lines(&x.pinned_by, p));
    }
    out
}
