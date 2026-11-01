//! The lines of a change, painted for the terminal: a leaf, a variable, and a pack with its
//! flags and its detail.

use crate::api::mask::MASKED;
use crate::closed_set::ClosedSet;
use crate::diff::{ChangeFlag, ImpactLevel, Leaf, Op, PackDiff, Status};
use crate::render::changes::{
    OpLine, VarLine, artifact_lines, op_mark, status_mark, status_word, traits_suffix,
    upstream_line, values_summary, values_unparsed_sentence, version_display,
};
use crate::render::notes::{ARCHIVE_CHANGE_HIDDEN, invalid_mark, tag_rename};
use crate::term::color::Paint;

/// The mark of `op` in its color: yellow for a change, green for an addition, red for a removal.
fn opmark(op: Op, p: &Paint) -> String {
    let mark = op_mark(op);
    match op {
        Op::Changed => p.yellow(mark),
        Op::Added => p.green(mark),
        Op::Removed => p.red(mark),
    }
}

fn vline(l: &Leaf, p: &Paint) -> String {
    op_line(&OpLine::leaf(&l.path, l), p)
}

/// `~ path: old -> new`, `+ path: new`, or `- path: old`, with the mark and each side colored.
pub(super) fn op_line(l: &OpLine, p: &Paint) -> String {
    let mark = opmark(l.op, p);
    match l.op {
        Op::Changed => format!(
            "{mark} {}: {} -> {}",
            l.path,
            p.red(&l.old),
            p.green(&l.new)
        ),
        Op::Added => format!("{mark} {}: {}", l.path, p.green(&l.new)),
        Op::Removed => format!("{mark} {}: {}", l.path, p.red(&l.old)),
    }
}

/// One line per added or removed variable, one per changed field of a changed variable.
pub(super) fn variable_line(v: &VarLine<'_>, p: &Paint) -> String {
    match v {
        VarLine::Added(s) => format!(
            "{} variable {}{}",
            opmark(Op::Added, p),
            p.bold(&s.name),
            traits_suffix(s)
        ),
        VarLine::Removed(s) => format!(
            "{} variable {}{}",
            opmark(Op::Removed, p),
            p.bold(&s.name),
            traits_suffix(s)
        ),
        VarLine::Field(name, l) => op_line(&OpLine::leaf(&format!("{name}.{}", l.path), l), p),
    }
}

fn flags(pd: &PackDiff, p: &Paint) -> String {
    let rename = tag_rename(pd);
    pd.flags()
        .into_iter()
        .map(|f| match f {
            ChangeFlag::Identity => p.blue(rename.as_deref().unwrap_or(f.as_str())),
            ChangeFlag::Upstream => p.magenta(f.as_str()),
            ChangeFlag::Authored => p.yellow(f.as_str()),
            // details of AUTHORED: its color, dimmed, so AUTHORED reads as the flag
            ChangeFlag::Images | ChangeFlag::Charts => p.dyellow(f.as_str()),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn paint_impact(level: ImpactLevel, text: &str, p: &Paint) -> String {
    match level {
        ImpactLevel::Repave | ImpactLevel::MayRepave => p.bred(text),
        ImpactLevel::ClusterWide => p.yellow(text),
        ImpactLevel::InPlace => p.cyan(text),
        ImpactLevel::Sync | ImpactLevel::None => p.dim(text),
    }
}

pub(super) fn packline(pd: &PackDiff, p: &Paint) -> String {
    let ver = version_display(pd);
    let invalid = invalid_mark(pd).map_or_else(String::new, |m| format!("  {}", p.bred(&m)));
    let (mark, word) = (status_mark(pd.status), status_word(pd.status));
    if pd.status == Status::Unchanged {
        let line = p.dim(&format!(
            "{mark} [{:<5}] {:<26} {:<18} {word}",
            pd.layer, pd.name, ver
        ));
        return format!("{line}{invalid}");
    }
    let (mark, st) = match pd.status {
        Status::Added => (p.green(mark), p.bgreen(word)),
        Status::Removed => (p.red(mark), p.bred(word)),
        _ => (p.yellow(mark), flags(pd, p)),
    };
    format!(
        "{mark} [{:<5}] {} {:<18} {st}{invalid}",
        pd.layer,
        p.bold(&format!("{:<26}", pd.name)),
        ver
    )
}

pub(super) fn packdetail(pd: &PackDiff, p: &Paint, out: &mut Vec<String>) {
    if pd.upstream_unknown {
        out.push(format!(
            "      {}",
            p.yellow(&format!(
                "upstream unknown: no digest, {ARCHIVE_CHANGE_HIDDEN}"
            ))
        ));
    }
    if !pd.masked.is_empty() {
        out.push(format!(
            "      {}",
            p.yellow(&format!(
                "masked: {}: Palette serves {MASKED} in place of the secret",
                pd.masked.join(", ")
            ))
        ));
    }
    if let Some(line) = upstream_line(pd, p, str::to_string) {
        out.push(format!("      {line}"));
    }
    for line in artifact_lines(pd) {
        out.push(format!("      {line}"));
    }
    if pd.values.changed && pd.values.parseable && !pd.values.diff.is_empty() {
        out.push(format!("      {}", values_summary(&pd.values)));
        for l in &pd.values.diff {
            out.push(format!("        {}", vline(l, p)));
        }
    }
    if pd.values.changed && !pd.values.parseable {
        out.push(p.yellow(&format!(
            "      {}",
            values_unparsed_sentence(str::to_string)
        )));
    }
    for m in &pd.manifests {
        out.push(format!("      {} manifest {}", opmark(m.op, p), m.name));
    }
    if pd.impact.level != ImpactLevel::None || !pd.impact.text.is_empty() {
        out.push(format!(
            "      impact: {}",
            paint_impact(pd.impact.level, &pd.impact.text, p)
        ));
    }
}
