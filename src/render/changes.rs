//! The lines of a change: a leaf, a pack, a variable, and the values.

use serde_json::Value;

use super::notes::tag_rename;
use super::{short_digest, show, shown};
use crate::closed_set::ClosedSet;
use crate::diff::{
    ChangeFlag, ClusterValueChange, Diff, Leaf, ManifestChange, Op, PackDiff, Status, ValuesDiff,
    VariableSummary, VariablesByName,
};
use crate::term::color::Paint;

/// The mark of a pack in a comparison, as the text reports print it.
///
/// `+` is a pack only in B, `-` a pack only in A, `~` a change, and a space no change.
pub(crate) fn status_mark(status: Status) -> &'static str {
    match status {
        Status::Unchanged => " ",
        Status::Changed => "~",
        Status::Added => "+",
        Status::Removed => "-",
    }
}

/// The word of a pack in a comparison, as the text reports print it.
///
/// The line of a changed pack names its change flags, and not this word.
pub(crate) fn status_word(status: Status) -> &'static str {
    match status {
        Status::Unchanged => "unchanged",
        Status::Changed => "CHANGED",
        Status::Added => "ADDED",
        Status::Removed => "REMOVED",
    }
}

/// The status of a part of a comparison that has no status of its own, such as the variables or a
/// tier of a cluster: it moved or it did not.
pub(crate) fn moved(changed: bool) -> Status {
    if changed {
        Status::Changed
    } else {
        Status::Unchanged
    }
}

/// The word of [`moved`] for the terminal: `CHANGED` in yellow, or `unchanged` dimmed.
pub(crate) fn moved_word(changed: bool, p: &Paint) -> String {
    let word = status_word(moved(changed));
    if changed {
        p.byellow(word)
    } else {
        p.dim(word)
    }
}

/// One line of a diff of values: a leaf, or a value that a cluster gives a variable.
///
/// It holds the path and the two sides as the reports show them.
pub(crate) struct OpLine {
    pub(crate) op: Op,
    pub(crate) path: String,
    /// The value on side A, or `null`.
    pub(crate) old: String,
    /// The value on side B, or `null`.
    pub(crate) new: String,
}

impl OpLine {
    /// The line of a leaf at `path`: the path of the leaf alone, or inside its variable or its
    /// tier.
    pub(crate) fn leaf(path: &str, l: &Leaf) -> Self {
        Self {
            op: l.op,
            path: path.to_string(),
            old: shown(l.old.as_ref()),
            new: shown(l.new.as_ref()),
        }
    }

    /// The line of a value that a cluster gives a variable, at `<profile>.<name>`.
    pub(crate) fn cluster_value(c: &ClusterValueChange) -> Self {
        Self {
            op: c.op,
            path: format!("{}.{}", c.profile, c.name),
            old: cluster_value(&c.old, c.sensitive),
            new: cluster_value(&c.new, c.sensitive),
        }
    }
}

/// `IDENTITY UPSTREAM AUTHORED`, only the flags that are set, uncolored; a renamed tag in place of
/// `IDENTITY`.
pub(crate) fn plain_flags(pd: &PackDiff) -> String {
    let rename = tag_rename(pd);
    pd.flags()
        .into_iter()
        .map(|f| match (f, &rename) {
            (ChangeFlag::Identity, Some(t)) => t.as_str(),
            _ => f.as_str(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `upstream digest: sha256:ff3df77ebc9c -> sha256:c4f7f20f40c7 (packRef.digest)`: the digest of
/// the archive of a pack on each side, and the field that gives it. None when the digest did not
/// move.
///
/// Each report cuts the hash of a digest to twelve characters, as it cuts the root, and keeps the
/// name of the algorithm: see [`short_digest`]. The JSON document holds the full digest. `p`
/// paints side A red and side B green, and `code` puts each digest in the form of the report.
pub(crate) fn upstream_line(pd: &PackDiff, p: &Paint, code: fn(&str) -> String) -> Option<String> {
    let (Some(a), Some(b)) = (&pd.a, &pd.b) else {
        return None;
    };
    pd.upstream.then(|| {
        format!(
            "upstream digest: {} -> {} {}",
            p.red(&code(&short_digest(&a.upstream))),
            p.green(&code(&short_digest(&b.upstream))),
            p.dim(&format!("({})", b.upstream_source))
        )
    })
}

/// One line for each image, chart, or param that moved: `~ image <repo>: <old> -> <new>`, `+
/// chart <name> <version>`, `~ param <name>: <old> -> <new>`.
pub(crate) fn artifact_lines(pd: &PackDiff) -> Vec<String> {
    let line = |kind: &str, c: &crate::diff::ArtifactChange| -> String {
        let old = c.old.as_deref().unwrap_or_default();
        let new = c.new.as_deref().unwrap_or_default();
        let at = |v: &str| {
            if kind == "image" {
                format!(":{v}")
            } else {
                format!(" {v}")
            }
        };
        match c.op {
            crate::diff::Op::Changed => format!("~ {kind} {}: {old} -> {new}", c.name),
            crate::diff::Op::Added => format!("+ {kind} {}{}", c.name, at(new)),
            crate::diff::Op::Removed => format!("- {kind} {}{}", c.name, at(old)),
        }
    };
    // a param value is JSON: a string reads as it is, and anything else as compact JSON
    let value = |v: &Option<Value>| match v {
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    };
    let param = |c: &crate::diff::ParamChange| -> String {
        match c.op {
            crate::diff::Op::Changed => {
                format!("~ param {}: {} -> {}", c.name, value(&c.old), value(&c.new))
            }
            crate::diff::Op::Added => format!("+ param {}: {}", c.name, value(&c.new)),
            crate::diff::Op::Removed => format!("- param {}: {}", c.name, value(&c.old)),
        }
    };
    pd.image_changes
        .iter()
        .map(|c| line("image", c))
        .chain(pd.chart_changes.iter().map(|c| line("chart", c)))
        .chain(pd.param_changes.iter().map(param))
        .collect()
}

/// The line of the images and the charts that changed across the packs: `4 images changed in 2
/// packs`. None when none changed.
pub(crate) fn artifacts_summary(d: &Diff) -> Option<String> {
    let s = &d.summary;
    if s.images_changed == 0 && s.charts_changed == 0 {
        return None;
    }
    let packs = d.packs.iter().filter(|p| p.images || p.charts).count();
    let n = |count: usize, one: &str, many: &str| {
        format!("{count} {}", crate::text::plural(count, one, many))
    };
    Some(format!(
        "{}, {} changed in {}",
        n(s.images_changed, "image", "images"),
        n(s.charts_changed, "chart", "charts"),
        n(packs, "pack", "packs")
    ))
}

/// The version column: `a -> b` when the identity moved and the versions differ.
pub(crate) fn version_display(pd: &PackDiff) -> String {
    let av = pd.a.as_ref().map(|s| s.version.as_str()).unwrap_or("");
    let bv = pd.b.as_ref().map(|s| s.version.as_str()).unwrap_or("");
    match pd.status {
        Status::Added => bv.to_string(),
        Status::Removed => av.to_string(),
        _ if pd.identity && av != bv => format!("{av} -> {bv}"),
        _ => av.to_string(),
    }
}

/// One printable variable change: a whole variable added or removed, or one changed field.
pub(crate) enum VarLine<'a> {
    /// The variable exists only in B.
    Added(&'a VariableSummary),
    /// The variable exists only in A.
    Removed(&'a VariableSummary),
    /// One field of a variable present on both sides; the leaf path is relative to the variable.
    Field(&'a str, &'a Leaf),
}

/// The variable changes in name order, one entry per line the text reports print.
pub(crate) fn variable_lines(g: &VariablesByName) -> Vec<VarLine<'_>> {
    let mut lines: Vec<(&str, VarLine<'_>)> = Vec::new();
    lines.extend(g.added.iter().map(|v| (v.name.as_str(), VarLine::Added(v))));
    lines.extend(
        g.removed
            .iter()
            .map(|v| (v.name.as_str(), VarLine::Removed(v))),
    );
    for c in &g.changed {
        lines.extend(
            c.fields
                .iter()
                .map(|l| (c.name.as_str(), VarLine::Field(&c.name, l))),
        );
    }
    lines.sort_by_key(|(name, _)| *name);
    lines.into_iter().map(|(_, l)| l).collect()
}

/// `values: 2 changed, 1 added, 0 removed`: the counts of the leaves of the values of a pack.
pub(crate) fn values_summary(v: &ValuesDiff) -> String {
    format!(
        "values: {} changed, {} added, {} removed",
        v.changed_count, v.added_count, v.removed_count
    )
}

/// What a report says when the values of a pack changed and one side does not parse as YAML.
pub(crate) const VALUES_UNPARSED: &str = "values changed (not parseable as YAML on one side)";

/// [`VALUES_UNPARSED`], with the flag that shows the change: `...; use --full for a unified
/// diff`.
///
/// `code` puts the flag in the form of the report.
pub(crate) fn values_unparsed_sentence(code: fn(&str) -> String) -> String {
    format!(
        "{VALUES_UNPARSED}; use {} for a unified diff",
        code("--full")
    )
}

/// `manifest MyRules changed`: a manifest of a pack that changed, with its name as the pack
/// names it.
///
/// `code` puts the name in the form of the report.
pub(crate) fn manifest_change(m: &ManifestChange, code: fn(&str) -> String) -> String {
    format!("manifest {} {}", code(&m.name), m.op.as_str())
}

/// `1 variable value changed`, `2 variable values changed`.
pub(crate) fn values_changed(n: usize) -> String {
    format!(
        "{n} variable {} changed",
        crate::text::plural(n, "value", "values")
    )
}

/// A cluster value as the reports print it: quoted, or `(sensitive)`.
fn cluster_value(value: &Option<String>, sensitive: bool) -> String {
    match value {
        Some(v) => show(&Value::from(v.as_str())),
        None if sensitive => "(sensitive)".to_string(),
        None => "null".to_string(),
    }
}

/// ` (required, sensitive)`, or nothing when no trait is set.
pub(crate) fn traits_suffix(v: &VariableSummary) -> String {
    let t = v.traits();
    if t.is_empty() {
        String::new()
    } else {
        format!(" ({})", t.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_part_that_moved_is_changed_and_one_that_did_not_is_unchanged() {
        assert_eq!(moved(true), Status::Changed);
        assert_eq!(moved(false), Status::Unchanged);
        let off = Paint::off();
        assert_eq!(moved_word(true, &off), "CHANGED");
        assert_eq!(moved_word(false, &off), "unchanged");
        for status in Status::ALL {
            assert_eq!(
                status_word(*status).to_lowercase(),
                status.as_str(),
                "the word of a status is its name in the documents"
            );
        }
    }
}
