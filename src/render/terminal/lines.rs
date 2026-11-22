//! The lines of a change, painted for the terminal: a leaf, a variable, and a pack with its
//! flags and its detail. Each text of Palette in a line is [`terminal_text`].

use crate::api::mask::MASKED;
use crate::closed_set::ClosedSet;
use crate::diff::{ChangeFlag, ImpactLevel, Leaf, Op, PackDiff, Status};
use crate::render::changes::{
    OpLine, VarLine, artifact_lines, op_mark, status_mark, status_word, traits_suffix,
    upstream_line, values_summary, values_unparsed_sentence, version_display,
};
use crate::render::notes::{ARCHIVE_CHANGE_HIDDEN, invalid_mark, tag_rename};
use crate::term::color::Paint;
use crate::text::terminal_text;

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
    let OpLine { path, old, new, .. } = l;
    match l.op {
        Op::Changed => format!("{mark} {path}: {} -> {}", p.red(old), p.green(new)),
        Op::Added => format!("{mark} {path}: {}", p.green(new)),
        Op::Removed => format!("{mark} {path}: {}", p.red(old)),
    }
}

/// One line per added or removed variable, one per changed field of a changed variable.
pub(super) fn variable_line(v: &VarLine<'_>, p: &Paint) -> String {
    match v {
        VarLine::Added(s) => format!(
            "{} variable {}{}",
            opmark(Op::Added, p),
            p.bold(&terminal_text(&s.name)),
            traits_suffix(s)
        ),
        VarLine::Removed(s) => format!(
            "{} variable {}{}",
            opmark(Op::Removed, p),
            p.bold(&terminal_text(&s.name)),
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
            ChangeFlag::Identity => p.blue(&terminal_text(rename.as_deref().unwrap_or(f.as_str()))),
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
    let ver = terminal_text(&version_display(pd)).into_owned();
    let (layer, name) = (terminal_text(&pd.layer), terminal_text(&pd.name));
    let invalid =
        invalid_mark(pd).map_or_else(String::new, |m| format!("  {}", p.bred(&terminal_text(&m))));
    let (mark, word) = (status_mark(pd.status), status_word(pd.status));
    if pd.status == Status::Unchanged {
        let line = p.dim(&format!("{mark} [{layer:<5}] {name:<26} {ver:<18} {word}"));
        return format!("{line}{invalid}");
    }
    let (mark, st) = match pd.status {
        Status::Added => (p.green(mark), p.bgreen(word)),
        Status::Removed => (p.red(mark), p.bred(word)),
        _ => (p.yellow(mark), flags(pd, p)),
    };
    format!(
        "{mark} [{layer:<5}] {} {ver:<18} {st}{invalid}",
        p.bold(&format!("{name:<26}"))
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
                "masked: {}: the record holds {MASKED} in place of the secret",
                terminal_text(&pd.masked.join(", "))
            ))
        ));
    }
    if let Some(line) = upstream_line(pd, p, |s| terminal_text(s).into_owned()) {
        out.push(format!("      {line}"));
    }
    for line in artifact_lines(pd) {
        out.push(format!("      {}", terminal_text(&line)));
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
        out.push(format!(
            "      {} manifest {}",
            opmark(m.op, p),
            terminal_text(&m.name)
        ));
    }
    if pd.impact.level != ImpactLevel::None || !pd.impact.text.is_empty() {
        out.push(format!(
            "      impact: {}",
            paint_impact(pd.impact.level, &terminal_text(&pd.impact.text), p)
        ));
    }
}
