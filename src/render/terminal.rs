//! The terminal report: plain when piped, colored on a terminal.

use super::{
    chain_verdict, cluster_value, consumer_count, consumers_heading, counts_line, dash,
    edited_in_place_note, entry_error, list_count, list_scope, overwritten_in_place_notes,
    pack_version, pair_detail, pair_word, record_detail, record_label, short, show, subject,
    traits_suffix, values_changed_suffix, variable_lines, version_display, Renderer, VarLine,
};
use crate::color::Paint;
use crate::diff::{
    ClusterValueChange, Diff, ImpactLevel, Leaf, Op, PackDiff, Status, Verify, VerifyStatus,
};
use crate::error::Result;
use crate::ops::{Chain, ConsumersReport, Listing, PairStatus, RecordStatus, VerifySummary};
use crate::snapshot::{Snapshot, Source};

/// The width of each column: its widest cell, leaving out the last cell of each row, which is
/// never padded.
fn column_widths(rows: &[Vec<String>]) -> Vec<usize> {
    let mut widths = Vec::new();
    for row in rows {
        for (i, cell) in row.iter().enumerate().take(row.len().saturating_sub(1)) {
            if widths.len() <= i {
                widths.resize(i + 1, 0);
            }
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    widths
}

/// One table line: cells padded to their column and two spaces apart, the last cell unpadded so
/// no line ends in spaces. `style` colors a padded cell by its column.
fn table_line(
    indent: &str,
    row: &[String],
    widths: &[usize],
    style: impl Fn(usize, &str) -> String,
) -> String {
    let last = row.len().saturating_sub(1);
    let cells: Vec<String> = row
        .iter()
        .enumerate()
        .map(|(i, cell)| {
            if i < last {
                let width = widths.get(i).copied().unwrap_or(0);
                style(i, &format!("{cell:<width$}"))
            } else {
                style(i, cell)
            }
        })
        .collect();
    format!("{indent}{}\n", cells.join("  "))
}

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

fn cluster_value_line(c: &ClusterValueChange, p: &Paint) -> String {
    let path = format!("{}.{}", c.profile, c.name);
    let (old, new) = (
        cluster_value(&c.old, c.sensitive),
        cluster_value(&c.new, c.sensitive),
    );
    match c.op {
        Op::Changed => format!(
            "{} {path}: {} -> {}",
            opmark(c.op, p),
            p.red(&old),
            p.green(&new)
        ),
        Op::Added => format!("{} {path}: {}", opmark(c.op, p), p.green(&new)),
        Op::Removed => format!("{} {path}: {}", opmark(c.op, p), p.red(&old)),
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
            "{} (uid {})  ->  {} (uid {})",
            p.bold(&d.a.title()),
            p.dim(dash(d.a.shown_uid())),
            p.bold(&d.b.title()),
            p.dim(dash(d.b.shown_uid()))
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
        if let Some(note) = edited_in_place_note(d) {
            out.push(format!("note       {}", p.byellow(&note)));
        }
        for note in overwritten_in_place_notes(d) {
            out.push(format!("note       {}", p.byellow(&note)));
        }
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
        if let Some(cv) = d.cluster_variables.as_ref().filter(|c| c.changed) {
            out.push(format!("  cluster values {}", p.byellow("CHANGED")));
            for c in &cv.values {
                out.push(format!("        {}", cluster_value_line(c, p)));
            }
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
        let (what, at) = (subject(recorded), &recorded.fetched_at);
        Some(match v.status {
            VerifyStatus::Unchanged => format!(
                "{}: {what} is unchanged since {at}{}",
                p.bgreen("OK"),
                values_changed_suffix(v.cluster_values_changed)
            ),
            VerifyStatus::Drifted => match &v.overwritten_by {
                Some(now) => format!(
                    "{}: {what} now names {now}: the version recorded {at} was overwritten in place{}",
                    p.bred("DRIFT"),
                    values_changed_suffix(v.cluster_values_changed)
                ),
                None => format!(
                    "{}: {what} differs from the copy recorded {at}{}",
                    p.bred("DRIFT"),
                    values_changed_suffix(v.cluster_values_changed)
                ),
            },
        })
    }

    fn render_verify_summary(&self, s: &VerifySummary) -> Result<String> {
        let p = &self.paint;
        let mut out = String::new();
        for r in &s.records {
            let word = match r.status {
                RecordStatus::Unchanged => p.bgreen("OK   "),
                RecordStatus::Drifted => p.bred("DRIFT"),
                RecordStatus::Error => p.byellow("ERROR"),
            };
            out.push_str(&format!(
                "{word}  {}  {}  {}\n",
                r.path,
                p.bold(&record_label(&r.name, &r.version)),
                record_detail(r)
            ));
        }
        out.push_str(&counts_line(s));
        out.push('\n');
        Ok(out)
    }

    fn render_chain(&self, c: &Chain) -> Result<String> {
        let p = &self.paint;
        let mut out = String::new();
        for pair in &c.pairs {
            let word = format!("{:<13}", pair_word(pair.status));
            let word = match pair.status {
                PairStatus::Identical => p.bgreen(&word),
                PairStatus::RegistryMove => p.byellow(&word),
                PairStatus::Different => p.bred(&word),
            };
            out.push_str(&format!(
                "{word}  {} -> {}   {}\n",
                pair.a,
                pair.b,
                p.dim(&pair_detail(pair))
            ));
            for r in &pair.reasons {
                out.push_str(&format!("               {r}\n"));
            }
        }
        let verdict = chain_verdict(c);
        out.push_str(&if c.same {
            p.bgreen(&verdict)
        } else {
            p.bred(&verdict)
        });
        out.push('\n');
        Ok(out)
    }

    fn render_consumers(&self, r: &ConsumersReport) -> Result<String> {
        let p = &self.paint;
        let mut out = format!("{}\n", consumers_heading(r));
        let width = r
            .consumers
            .iter()
            .map(|c| dash(&c.name).len())
            .max()
            .unwrap_or(0);
        for c in &r.consumers {
            let state = match c.state.as_str() {
                "Running" => p.green(&c.state),
                crate::ops::UNKNOWN_STATE => p.yellow(&c.state),
                _ => c.state.clone(),
            };
            out.push_str(&format!(
                "  {}  {}  project {}  {}",
                p.bold(&format!("{:<width$}", dash(&c.name))),
                p.dim(&c.uid),
                dash(&c.project_uid),
                state
            ));
            if let Some(msg) = c
                .error
                .as_ref()
                .and_then(|e| e.get("message"))
                .and_then(serde_json::Value::as_str)
            {
                out.push_str(&format!("  {}", p.dim(&format!("({msg})"))));
            }
            out.push('\n');
        }
        Ok(out)
    }

    fn render_list(&self, l: &Listing) -> Result<String> {
        let p = &self.paint;
        let mut out = format!("{}\n", list_scope(l));
        if let Some(profiles) = &l.profiles {
            out.push_str(&format!(
                "\n{}\n",
                list_count(profiles.len(), "profile", l.pack.as_deref())
            ));
            let rows: Vec<Vec<String>> = profiles
                .iter()
                .map(|pr| {
                    let mut row = vec![
                        pr.reference.clone(),
                        pr.uid.clone(),
                        dash(&pr.cloud_type).to_string(),
                    ];
                    if let Some(msg) = entry_error(pr.error.as_ref()) {
                        row.push(format!("({msg})"));
                    } else if pr.packs.is_some() {
                        row.push(dash(pr.profile_type.as_deref().unwrap_or("")).to_string());
                        row.push(consumer_count(pr.consumers.unwrap_or(0)));
                    }
                    row
                })
                .collect();
            let widths = column_widths(&rows);
            for (pr, row) in profiles.iter().zip(&rows) {
                let failed = pr.error.is_some();
                out.push_str(&table_line("  ", row, &widths, |i, cell| match i {
                    0 => p.bold(cell),
                    1 => p.dim(cell),
                    3 if failed => p.dim(cell),
                    _ => cell.to_string(),
                }));
                let packs: Vec<Vec<String>> = pr
                    .packs
                    .iter()
                    .flatten()
                    .map(|pk| vec![pk.layer.clone(), pk.name.clone(), pack_version(pk)])
                    .collect();
                let pack_widths = column_widths(&packs);
                for row in &packs {
                    out.push_str(&table_line("    ", row, &pack_widths, |i, cell| match i {
                        0 => p.dim(cell),
                        _ => cell.to_string(),
                    }));
                }
            }
        }
        if let Some(clusters) = &l.clusters {
            out.push_str(&format!(
                "\n{}\n",
                list_count(clusters.len(), "cluster", None)
            ));
            let rows: Vec<Vec<String>> = clusters
                .iter()
                .map(|c| {
                    let mut row = vec![
                        c.reference.clone(),
                        dash(&c.name).to_string(),
                        dash(&c.state).to_string(),
                        dash(&c.cloud_type).to_string(),
                    ];
                    if c.project_uid != l.project_uid {
                        row.push(format!("project {}", dash(&c.project_uid)));
                    }
                    row
                })
                .collect();
            let widths = column_widths(&rows);
            for (c, row) in clusters.iter().zip(&rows) {
                out.push_str(&table_line("  ", row, &widths, |i, cell| match i {
                    0 => p.bold(cell),
                    2 if c.state == "Running" => p.green(cell),
                    _ => cell.to_string(),
                }));
            }
        }
        if let Some(projects) = &l.projects {
            out.push_str(&format!(
                "\n{}\n",
                list_count(projects.len(), "project", None)
            ));
            let rows: Vec<Vec<String>> = projects
                .iter()
                .map(|pr| vec![dash(&pr.name).to_string(), pr.uid.clone()])
                .collect();
            let widths = column_widths(&rows);
            for row in &rows {
                out.push_str(&table_line("  ", row, &widths, |i, cell| match i {
                    0 => p.bold(cell),
                    _ => p.dim(cell),
                }));
            }
        }
        Ok(out)
    }
}
