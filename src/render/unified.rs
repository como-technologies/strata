//! Unified diffs of raw values and changed manifests, for `--full`.

use crate::diff::{Diff, Op, Status};
use crate::snapshot::Snapshot;
use crate::term::color::Paint;

fn colorize(text: &str, p: &Paint) -> String {
    if !p.is_on() {
        return text.to_string();
    }
    text.lines()
        .map(|l| {
            if l.starts_with("+++") || l.starts_with("---") {
                p.bold(l)
            } else if l.starts_with('+') {
                p.green(l)
            } else if l.starts_with('-') {
                p.red(l)
            } else if l.starts_with("@@") {
                p.cyan(l)
            } else {
                l.to_string()
            }
        })
        .map(|l| l + "\n")
        .collect()
}

/// Unified diffs for every changed pack: raw values when they changed, and each changed manifest.
/// In markdown mode each diff is wrapped in a `diff` code block; otherwise it is colored by `p`.
pub(crate) fn full_diffs(
    sa: &Snapshot,
    sb: &Snapshot,
    d: &Diff,
    p: &Paint,
    markdown: bool,
) -> String {
    let mut o = String::new();
    let mut emit = |title_a: String, title_b: String, old: &str, new: &str| {
        let diff = similar::TextDiff::from_lines(old, new);
        let text = diff
            .unified_diff()
            .context_radius(3)
            .header(&title_a, &title_b)
            .to_string();
        if markdown {
            o.push_str(&format!("```diff\n{text}```\n\n"));
        } else {
            o.push('\n');
            o.push_str(&colorize(&text, p));
        }
    };
    for pd in d.packs.iter().filter(|p| p.status == Status::Changed) {
        let (Some(x), Some(y)) = (
            sa.packs.iter().find(|q| q.key == pd.key),
            sb.packs.iter().find(|q| q.key == pd.key),
        ) else {
            continue;
        };
        if pd.values.changed {
            emit(
                format!("a: {} values", pd.key),
                format!("b: {} values", pd.key),
                &x.values,
                &y.values,
            );
        }
        for m in pd.manifests.iter().filter(|m| m.op == Op::Changed) {
            let (Some(ma), Some(mb)) = (
                x.manifests.iter().find(|q| q.name == m.name),
                y.manifests.iter().find(|q| q.name == m.name),
            ) else {
                continue;
            };
            emit(
                format!("a: {} manifest {}", pd.key, m.name),
                format!("b: {} manifest {}", pd.key, m.name),
                &ma.content,
                &mb.content,
            );
        }
    }
    o
}
