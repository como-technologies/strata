//! Unified diffs of raw values and manifests: the text of `--full`, and the field `unified` of
//! the document.

use crate::diff::{Diff, Op, PackDiff, Status};
use crate::snapshot::{Pack, Snapshot};
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

/// The unified diff of `old` and `new`, with three lines of context. `title_a` and `title_b` are
/// the two header lines.
fn unified_text(title_a: &str, title_b: &str, old: &str, new: &str) -> String {
    similar::TextDiff::from_lines(old, new)
        .unified_diff()
        .context_radius(3)
        .header(title_a, title_b)
        .to_string()
}

/// The unified diff of the values of the pack `key`, as a person reads it: side A, then side B.
fn values_text(key: &str, x: &Pack, y: &Pack) -> String {
    unified_text(
        &format!("a: {key} values"),
        &format!("b: {key} values"),
        &x.values,
        &y.values,
    )
}

/// The unified diff of the manifest `name` of the pack `key`. `old` and `new` are its text on the
/// two sides. A manifest that one side lacks has the empty text there.
fn manifest_text(key: &str, name: &str, old: &str, new: &str) -> String {
    unified_text(
        &format!("a: {key} manifest {name}"),
        &format!("b: {key} manifest {name}"),
        old,
        new,
    )
}

/// The pack of `pd` on each side, when both sides of the records hold it.
fn sides<'a>(sa: &'a Snapshot, sb: &'a Snapshot, pd: &PackDiff) -> Option<(&'a Pack, &'a Pack)> {
    Some((
        sa.packs.iter().find(|q| q.key == pd.key)?,
        sb.packs.iter().find(|q| q.key == pd.key)?,
    ))
}

/// The content of the manifest `name` of `pack`, or the empty text when the pack has none.
fn content_of<'a>(pack: &'a Pack, name: &str) -> &'a str {
    pack.manifests
        .iter()
        .find(|m| m.name == name)
        .map_or("", |m| m.content.as_str())
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
    let mut emit = |text: String| {
        if markdown {
            o.push_str(&format!("```diff\n{text}```\n\n"));
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
            emit(manifest_text(
                &pd.key,
                &m.name,
                content_of(x, &m.name),
                content_of(y, &m.name),
            ));
        }
    }
    o
}

/// Give the values and each manifest change of every changed pack of `d` the field `unified`: the
/// text of `--full`, with no color. A manifest that one side added or removed has its text against
/// an empty side. The text is the text of the records, so it holds the mask that Palette writes
/// over a secret and no value of a cluster variable.
pub(crate) fn fill_unified(d: &mut Diff, sa: &Snapshot, sb: &Snapshot) {
    for pd in d.packs.iter_mut().filter(|p| p.status == Status::Changed) {
        let Some((x, y)) = sides(sa, sb, pd) else {
            continue;
        };
        if pd.values.changed {
            pd.values.unified = Some(values_text(&pd.key, x, y));
        }
        for m in &mut pd.manifests {
            m.unified = Some(manifest_text(
                &pd.key,
                &m.name,
                content_of(x, &m.name),
                content_of(y, &m.name),
            ));
        }
    }
}
