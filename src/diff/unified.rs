//! The unified text of the values and the manifests of a comparison: the field `unified` of the
//! document, and the text that `--full` prints.

use super::{Diff, PackDiff, Status};
use crate::snapshot::{Pack, Snapshot};

/// The unified diff of `old` and `new`, with three lines of context. `title_a` and `title_b` are
/// the two header lines.
fn unified_text(title_a: &str, title_b: &str, old: &str, new: &str) -> String {
    similar::TextDiff::from_lines(old, new)
        .unified_diff()
        .context_radius(3)
        .header(title_a, title_b)
        .to_string()
}

/// The unified diff of the values of the pack `key`: side A, then side B.
pub(crate) fn values_text(key: &str, x: &Pack, y: &Pack) -> String {
    unified_text(
        &format!("a: {key} values"),
        &format!("b: {key} values"),
        &x.values,
        &y.values,
    )
}

/// The unified diff of the manifest `name` of the pack `key`. A manifest that one side lacks has
/// the empty text there.
pub(crate) fn manifest_text(key: &str, name: &str, x: &Pack, y: &Pack) -> String {
    unified_text(
        &format!("a: {key} manifest {name}"),
        &format!("b: {key} manifest {name}"),
        content_of(x, name),
        content_of(y, name),
    )
}

/// The content of the manifest `name` of `pack`, or the empty text when the pack has none.
fn content_of<'a>(pack: &'a Pack, name: &str) -> &'a str {
    pack.manifests
        .iter()
        .find(|m| m.name == name)
        .map_or("", |m| m.content.as_str())
}

/// The pack of `pd` on each side, when both records hold it.
pub(crate) fn sides<'a>(
    sa: &'a Snapshot,
    sb: &'a Snapshot,
    pd: &PackDiff,
) -> Option<(&'a Pack, &'a Pack)> {
    Some((
        sa.packs.iter().find(|q| q.key == pd.key)?,
        sb.packs.iter().find(|q| q.key == pd.key)?,
    ))
}

impl Diff {
    /// Give the values and each manifest change of every changed pack the field `unified`: the text
    /// of `--full`, with no color. `sa` and `sb` are the records that the diff compares. A manifest
    /// that one side added or removed has its text against an empty side. The text is the text of
    /// the records, so it holds the mask that Palette writes over a secret and no value of a
    /// cluster variable.
    pub(crate) fn fill_unified(&mut self, sa: &Snapshot, sb: &Snapshot) {
        for pd in self
            .packs
            .iter_mut()
            .filter(|p| p.status == Status::Changed)
        {
            let Some((x, y)) = sides(sa, sb, pd) else {
                continue;
            };
            if pd.values.changed {
                pd.values.unified = Some(values_text(&pd.key, x, y));
            }
            for m in &mut pd.manifests {
                m.unified = Some(manifest_text(&pd.key, &m.name, x, y));
            }
        }
    }
}
