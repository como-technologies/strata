//! Unified diffs of raw values and changed manifests, for `--full`.

use crate::diff::unified::{manifest_text, sides, values_text};
use crate::diff::{Diff, Op, Status};
use crate::snapshot::Snapshot;
use crate::term::color::Paint;
use crate::text::{code_block, terminal_text};

/// `text`, a unified diff, for the terminal: each line in the color of its mark.
///
/// A line quotes a value or a manifest of Palette, so it is [`terminal_text`]. Each control
/// character in a line shows as a visible escape, and the line breaks of the diff stay.
fn colorize(text: &str, p: &Paint) -> String {
    text.split_inclusive('\n')
        .map(|line| {
            let l = terminal_text(line.strip_suffix('\n').unwrap_or(line));
            let painted = if l.starts_with("+++") || l.starts_with("---") {
                p.bold(&l)
            } else if l.starts_with('+') {
                p.green(&l)
            } else if l.starts_with('-') {
                p.red(&l)
            } else if l.starts_with("@@") {
                p.cyan(&l)
            } else {
                l.into_owned()
            };
            painted + "\n"
        })
        .collect()
}

/// Unified diffs for every changed pack: raw values when they changed, and each changed manifest.
/// In markdown mode each diff is in a `diff` code block, with a fence that no line of the diff
/// ends. Otherwise `p` colors it.
pub(crate) fn full_diffs(
    sa: &Snapshot,
    sb: &Snapshot,
    d: &Diff,
    p: &Paint,
    markdown: bool,
) -> String {
    let mut o = String::new();
    let mut emit = |text: String| {
        if markdown {
            o.push_str(&code_block("diff", &text));
            o.push('\n');
        } else {
            o.push('\n');
            o.push_str(&colorize(&text, p));
        }
    };
    for pd in d.packs.iter().filter(|p| p.status == Status::Changed) {
        let Some((x, y)) = sides(sa, sb, pd) else {
            continue;
        };
        if pd.values.changed {
            emit(values_text(&pd.key, x, y));
        }
        for m in pd.manifests.iter().filter(|m| m.op == Op::Changed) {
            emit(manifest_text(&pd.key, &m.name, x, y));
        }
    }
    o
}
