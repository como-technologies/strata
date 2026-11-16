//! The text of `behind` for the terminal: the heading, the lines of each pin, and one line for
//! each profile version. Each text of Palette in a line is [`terminal_text`].

use super::table::painted_text;
use crate::closed_set::ClosedSet;
use crate::ops::{BehindReport, Pin, PinnedBy, ProfilePin};
use crate::reference::name_version;
use crate::render::behind::pin_ref;
use crate::render::behind::{
    arrived_line, behind_heading, bump_line, carrier_lines, created_since, listed_since,
    next_patch_line, palette_lines, pin_status, profile_bump_line, profile_status, record_title,
    shown_pins, shown_profiles, update_check_lines,
};
use crate::render::{Kind, entry_error};
use crate::term::color::{LineClass, Paint};
use crate::text::terminal_text;

/// The report of `behind` for the terminal: the heading, the pins, and then the profile versions.
/// With `behind_only`, the report shows no pin and no profile version that is current.
pub(super) fn terminal_behind(r: &BehindReport, behind_only: bool, p: &Paint) -> String {
    let mut out = format!("{}\n", terminal_text(&behind_heading(r)));
    let pins = shown_pins(r, behind_only);
    // each row names its pack by its ref, as a person types it in a shell
    let refs: Vec<String> = pins
        .iter()
        .map(|pin| terminal_text(&pin_ref(pin, &pin.version).shell_word()).into_owned())
        .collect();
    let ref_w = refs.iter().map(|r| r.chars().count()).max().unwrap_or(0);
    for (pin, reference) in pins.into_iter().zip(&refs) {
        let status = terminal_text(&pin_status(pin)).into_owned();
        let status = if pin.error.is_some() {
            p.byellow(&status)
        } else if pin.behind {
            p.yellow(&status)
        } else {
            p.dim(&status)
        };
        out.push_str(&format!(
            "{} [{:<5}] {}  {status}\n",
            if pin.behind { "~" } else { " " },
            terminal_text(&pin.layer),
            painted_text(&format!("{reference:<ref_w$}"), |t| Kind::Pack.paint(p, t)),
        ));
        out.push_str(&pin_lines(pin, p));
    }
    out.push_str(&profile_lines(&shown_profiles(r, behind_only), p));
    out
}

/// The lines under the line of `pin`. They give when the newest version arrived, the bump, the
/// next patch, the arrivals, the error, the carriers, and the records. The answer of Palette about
/// the bump is the answer of a check, so it is the last line, in bold.
fn pin_lines(pin: &Pin, p: &Paint) -> String {
    let mut out = String::new();
    if let Some(listed) = listed_since(pin) {
        out.push_str(&format!("      {}\n", p.dim(&terminal_text(&listed))));
    }
    for line in bump_line(pin)
        .into_iter()
        .chain(update_check_lines(pin.bump.as_ref()))
        .chain(next_patch_line(pin))
    {
        out.push_str(&format!("      {}\n", terminal_text(&line)));
    }
    if let Some(arrived) = arrived_line(pin) {
        out.push_str(&format!("      {}\n", p.cyan(&terminal_text(&arrived))));
    }
    if let Some(msg) = entry_error(pin.error.as_ref()) {
        out.push_str(&format!(
            "      {}\n",
            p.dim(&format!("({})", terminal_text(msg)))
        ));
    }
    for line in carrier_lines(pin) {
        out.push_str(&format!("      {}\n", terminal_text(&line)));
    }
    out.push_str(&pinned_by_lines(&pin.pinned_by, p));
    for line in palette_lines(pin) {
        out.push_str(&format!(
            "      {}\n",
            p.line(LineClass::Answer, &terminal_text(&line))
        ));
    }
    out
}

/// `pinned by <path> (<title>)`, dim, under a pin or a profile version: one line for each
/// record. The title names the profile that holds the entry, when the record names it: see
/// [`record_title`].
fn pinned_by_lines(by: &[PinnedBy], p: &Paint) -> String {
    by.iter()
        .map(|b| {
            format!(
                "      {}\n",
                p.dim(&terminal_text(&format!(
                    "pinned by {} ({})",
                    b.path,
                    record_title(b)
                )))
            )
        })
        .collect()
}

/// The lines of the profile versions of `behind`, after the pins: one line for each version, in
/// the form of the line of a pin. The ref of the version, `<name>@<version>`, takes the place of
/// the ref of the pack, and its scope follows it.
fn profile_lines(profiles: &[&ProfilePin], p: &Paint) -> String {
    let scope = |x: &ProfilePin| x.scope.map_or("-", |s| s.as_str());
    let refs: Vec<String> = profiles
        .iter()
        .map(|x| terminal_text(&name_version(&x.name, &x.version)).into_owned())
        .collect();
    let ref_w = refs.iter().map(|r| r.chars().count()).max().unwrap_or(0);
    let scope_w = profiles.iter().map(|x| scope(x).len()).max().unwrap_or(0);
    let mut out = String::new();
    for (x, reference) in profiles.iter().zip(&refs) {
        let status = terminal_text(&profile_status(x)).into_owned();
        let status = if x.error.is_some() {
            p.byellow(&status)
        } else if x.behind {
            p.yellow(&status)
        } else {
            p.dim(&status)
        };
        out.push_str(&format!(
            "{} [profile] {}  {}  {status}\n",
            if x.behind { "~" } else { " " },
            painted_text(&format!("{reference:<ref_w$}"), |t| Kind::Profile
                .paint(p, t)),
            format_args!("{:<scope_w$}", scope(x)),
        ));
        if let Some(created) = created_since(x) {
            out.push_str(&format!("      {}\n", p.dim(&terminal_text(&created))));
        }
        for line in profile_bump_line(x)
            .into_iter()
            .chain(update_check_lines(x.bump.as_ref()))
        {
            out.push_str(&format!("      {}\n", terminal_text(&line)));
        }
        if let Some(msg) = entry_error(x.error.as_ref()) {
            out.push_str(&format!(
                "      {}\n",
                p.dim(&format!("({})", terminal_text(msg)))
            ));
        }
        out.push_str(&pinned_by_lines(&x.pinned_by, p));
    }
    out
}
