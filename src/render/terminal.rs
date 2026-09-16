//! The terminal report: plain when piped, colored on a terminal.

use super::{dash, short, show, traits_suffix, variable_lines, version_display, Renderer, VarLine};
use crate::color::Paint;
use crate::diff::{Diff, ImpactLevel, Leaf, Op, PackDiff, Status, Verify, VerifyStatus};
use crate::error::Result;
use crate::snapshot::{Snapshot, Source};

/// Renders the text report.
pub struct TerminalRenderer {
    /// Color policy, already resolved.
    pub paint: Paint,
}

fn opmark(op: Op, p: &Paint) -> String {
    match op {
        Op::Changed => p.yellow("~"),
        Op::Added => p.green("+"),
        Op::Removed => p.red("-"),
    }
}

fn vline(l: &Leaf, p: &Paint) -> String {
    vline_at(&l.path, l, p)
}

fn vline_at(path: &str, l: &Leaf, p: &Paint) -> String {
    let old = l.old.as_ref().map(show).unwrap_or_else(|| "null".into());
    let new = l.new.as_ref().map(show).unwrap_or_else(|| "null".into());
    match l.op {
        Op::Changed => format!(
            "{} {}: {} -> {}",
            opmark(l.op, p),
            path,
            p.red(&old),
            p.green(&new)
        ),
        Op::Added => format!("{} {}: {}", opmark(l.op, p), path, p.green(&new)),
        Op::Removed => format!("{} {}: {}", opmark(l.op, p), path, p.red(&old)),
    }
}

/// One line per added or removed variable, one per changed field of a changed variable.
fn variable_line(v: &VarLine<'_>, p: &Paint) -> String {
    match v {
        VarLine::Added(s) => format!(
            "{} variable {}{}",
            p.green("+"),
            p.bold(&s.name),
            traits_suffix(s)
        ),
        VarLine::Removed(s) => format!(
            "{} variable {}{}",
            p.red("-"),
            p.bold(&s.name),
            traits_suffix(s)
        ),
        VarLine::Field(name, l) => vline_at(&format!("{name}.{}", l.path), l, p),
    }
}

fn flags(pd: &PackDiff, p: &Paint) -> String {
    let mut f = Vec::new();
    if pd.identity {
        f.push(p.blue("IDENTITY"));
    }
    if pd.upstream {
        f.push(p.magenta("UPSTREAM"));
    }
    if pd.authored {
        f.push(p.yellow("AUTHORED"));
    }
    f.join(" ")
}

fn paint_impact(level: ImpactLevel, text: &str, p: &Paint) -> String {
    match level {
        ImpactLevel::Repave | ImpactLevel::MayRepave => p.bred(text),
        ImpactLevel::ClusterWide => p.yellow(text),
        ImpactLevel::InPlace => p.cyan(text),
        ImpactLevel::Sync | ImpactLevel::None => p.dim(text),
    }
}

fn packline(pd: &PackDiff, p: &Paint) -> String {
    let ver = version_display(pd);
    if pd.status == Status::Unchanged {
        return p.dim(&format!(
            "  [{:<5}] {:<26} {:<18} unchanged",
            pd.layer, pd.name, ver
        ));
    }
    let (mark, st) = match pd.status {
        Status::Added => (p.green("+"), p.bgreen("ADDED")),
        Status::Removed => (p.red("-"), p.bred("REMOVED")),
        _ => (p.yellow("~"), flags(pd, p)),
    };
    format!(
        "{mark} [{:<5}] {} {:<18} {st}",
        pd.layer,
        p.bold(&format!("{:<26}", pd.name)),
        ver
    )
}

fn packdetail(pd: &PackDiff, p: &Paint, out: &mut Vec<String>) {
    if pd.upstream {
        if let (Some(a), Some(b)) = (&pd.a, &pd.b) {
            out.push(format!(
                "      upstream digest  {} -> {}   {}",
                p.red(short(&a.upstream)),
                p.green(short(&b.upstream)),
                p.dim(&format!("({})", b.upstream_source))
            ));
        }
    }
    if pd.values.changed && pd.values.parseable && !pd.values.diff.is_empty() {
        out.push(format!(
            "      values: {} changed, {} added, {} removed",
            pd.values.changed_count, pd.values.added_count, pd.values.removed_count
        ));
        for l in &pd.values.diff {
            out.push(format!("        {}", vline(l, p)));
        }
    }
    if pd.values.changed && !pd.values.parseable {
        out.push(p.yellow("      values changed (not parseable as YAML on one side; use --full for a unified diff)"));
    }
    for m in &pd.manifests {
        out.push(format!("      {} manifest {}", opmark(m.op, p), m.name));
    }
    if pd.impact.level != ImpactLevel::None {
        out.push(format!(
            "      impact: {}",
            paint_impact(pd.impact.level, &pd.impact.text, p)
        ));
    }
}

impl Renderer for TerminalRenderer {
    fn render(&self, d: &Diff, a: &Snapshot, b: &Snapshot, full: bool) -> Result<String> {
        let p = &self.paint;
        let mut out: Vec<String> = Vec::new();
        out.push(format!(
            "{}@{} ({})  ->  {}@{} ({})",
            p.bold(&d.a.name),
            p.bold(&d.a.version),
            p.dim(dash(&d.a.uid)),
            p.bold(&d.b.name),
            p.bold(&d.b.version),
            p.dim(dash(&d.b.uid))
        ));
        out.push(format!(
            "root       {} -> {}   {}",
            p.dim(short(&d.root_a)),
            p.dim(short(&d.root_b)),
            if d.same {
                p.bgreen("IDENTICAL")
            } else {
                p.bred("CHANGED")
            }
        ));
        out.push(format!(
            "consumers  {} -> {} clusters",
            d.a.consumers.count, d.b.consumers.count
        ));
        out.push(String::new());
        out.push(format!(
            "  variables {}",
            if d.variables.changed {
                p.byellow("CHANGED")
            } else {
                p.dim("unchanged")
            }
        ));
        for v in variable_lines(&d.variables.by_name) {
            out.push(format!("        {}", variable_line(&v, p)));
        }
        out.push(String::new());
        for pd in &d.packs {
            out.push(packline(pd, p));
            packdetail(pd, p, &mut out);
        }
        if d.order_changed {
            out.push(String::new());
            out.push(p.yellow("  note: pack order changed between the two profiles"));
        }
        let mut s = out.join("\n");
        s.push('\n');
        if full {
            s.push_str(&super::unified::full_diffs(a, b, d, p, false));
        }
        Ok(s)
    }

    fn verdict(&self, v: &Verify, recorded: &Source) -> Option<String> {
        let p = &self.paint;
        let (name, ver, at) = (&recorded.name, &recorded.version, &recorded.fetched_at);
        Some(match v.status {
            VerifyStatus::Unchanged => format!(
                "{}: published version {name}@{ver} is unchanged since {at}",
                p.bgreen("OK")
            ),
            VerifyStatus::Drifted => format!(
                "{}: published version {name}@{ver} differs from the copy recorded {at}",
                p.bred("DRIFT")
            ),
        })
    }
}
