//! The terminal report: plain when piped, colored on a terminal.

use super::{
    DiffNote, OpLine, Renderer, VarLine, applied_ref, artifact_lines, artifacts_summary,
    chain_verdict, cluster_count, consumer_count, consumer_counts, consumers_heading,
    consumers_sentence, count_of, counts_line, dash, diff_notes, diff_word, edge_hosts_line,
    entry_error, invalid_mark, invalid_pack_sentences, list_count, list_scope, no_version_sentence,
    pack_order_note, pack_version, pair_detail, pair_notes, pair_word, profile_marks,
    project_label, record_detail, record_label, record_word, registry_note, second_reading_line,
    short, tag_rename, tier_path, tier_states, traits_suffix, values_summary,
    values_unparsed_sentence, variable_lines, variables_comparable, verdict_parts, version_display,
    version_note, virtual_line, virtual_note, who_entry, who_heading,
};
use crate::api::mask::MASKED;
use crate::color::Paint;
use crate::diff::{ChangeFlag, Diff, ImpactLevel, Leaf, Op, PackDiff, Status, VerifyStatus};
use crate::error::Result;
use crate::ops::{
    Chain, ConsumerKind, ConsumersReport, Listing, PackListing, PairStatus, RUNNING_STATE,
    RecordStatus, VerifySummary,
};
use crate::snapshot::Snapshot;

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

/// The pack name with the part that `--pack` matched in bold yellow, the way a search tool marks
/// a match. The cell is already padded, so the escape codes do not move the column. Text that is
/// not ASCII is left alone: lowercasing can change its length, and the match would slice wrongly.
fn matched(p: &Paint, cell: &str, text: Option<&str>) -> String {
    let Some(text) = text.filter(|t| !t.is_empty()) else {
        return cell.to_string();
    };
    if !cell.is_ascii() || !text.is_ascii() {
        return cell.to_string();
    }
    match cell.to_lowercase().find(&text.to_lowercase()) {
        Some(at) => format!(
            "{}{}{}",
            &cell[..at],
            p.byellow(&cell[at..at + text.len()]),
            &cell[at + text.len()..]
        ),
        None => cell.to_string(),
    }
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
    op_line(&OpLine::leaf(&l.path, l), p)
}

/// `~ path: old -> new`, `+ path: new`, or `- path: old`, with the mark and each side colored.
fn op_line(l: &OpLine, p: &Paint) -> String {
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

fn packline(pd: &PackDiff, p: &Paint) -> String {
    let ver = version_display(pd);
    let invalid = invalid_mark(pd).map_or_else(String::new, |m| format!("  {}", p.bred(&m)));
    if pd.status == Status::Unchanged {
        let line = p.dim(&format!(
            "  [{:<5}] {:<26} {:<18} unchanged",
            pd.layer, pd.name, ver
        ));
        return format!("{line}{invalid}");
    }
    let (mark, st) = match pd.status {
        Status::Added => (p.green("+"), p.bgreen("ADDED")),
        Status::Removed => (p.red("-"), p.bred("REMOVED")),
        _ => (p.yellow("~"), flags(pd, p)),
    };
    format!(
        "{mark} [{:<5}] {} {:<18} {st}{invalid}",
        pd.layer,
        p.bold(&format!("{:<26}", pd.name)),
        ver
    )
}

fn packdetail(pd: &PackDiff, p: &Paint, out: &mut Vec<String>) {
    if pd.upstream_unknown {
        out.push(format!(
            "      {}",
            p.yellow("upstream unknown: no digest, so a change of the archive does not show")
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
    if pd.upstream
        && let (Some(a), Some(b)) = (&pd.a, &pd.b)
    {
        out.push(format!(
            "      upstream digest  {} -> {}   {}",
            p.red(short(&a.upstream)),
            p.green(short(&b.upstream)),
            p.dim(&format!("({})", b.upstream_source))
        ));
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
        let word = diff_word(d);
        out.push(format!(
            "root       {} -> {}   {}",
            p.dim(short(&d.root_a)),
            p.dim(short(&d.root_b)),
            if d.same || d.same_content {
                p.bgreen(word)
            } else {
                p.bred(word)
            }
        ));
        for note in diff_notes(d) {
            match note {
                DiffNote::Warn(text) => out.push(format!("note       {}", p.byellow(&text))),
                DiffNote::Good(text) => out.push(format!("note       {}", p.bgreen(&text))),
                DiffNote::Palette(lines) => {
                    out.extend(lines.iter().map(|line| format!("palette    {line}")));
                }
            }
        }
        if let Some(line) = artifacts_summary(d) {
            out.push(format!("images     {line}"));
        }
        if let Some((a, b)) = consumer_counts(d) {
            // the noun agrees with the count next to it: `1 -> 2 clusters`, `2 -> 1 cluster`
            let noun = if b == 1 { "cluster" } else { "clusters" };
            out.push(format!("consumers  {a} -> {b} {noun}"));
        } else if let Some(sentence) = consumers_sentence(d) {
            out.push(format!("consumers  {sentence}"));
        }
        // the blank line belongs to the variables block, so two packs, which have no variables,
        // leave neither the line nor the gap where it was
        if variables_comparable(d) {
            out.push(String::new());
            out.push(format!(
                "  variables {}",
                if d.variables.changed {
                    p.byellow("CHANGED")
                } else {
                    p.dim("unchanged")
                }
            ));
        }
        for v in variable_lines(&d.variables.by_name) {
            out.push(format!("        {}", variable_line(&v, p)));
        }
        if let Some(cv) = d.cluster_variables.as_ref().filter(|c| c.changed) {
            out.push(format!("  cluster values {}", p.byellow("CHANGED")));
            for c in &cv.values {
                out.push(format!("        {}", op_line(&OpLine::cluster_value(c), p)));
            }
        }
        let states = tier_states(d);
        if !states.is_empty() && states.iter().all(|(_, same)| *same) {
            out.push(format!("  cluster tiers {}", p.dim("unchanged")));
        } else if !states.is_empty() {
            let words: Vec<String> = states
                .iter()
                .map(|(t, same)| {
                    if *same {
                        format!("{t} {}", p.dim("unchanged"))
                    } else {
                        format!("{t} {}", p.byellow("CHANGED"))
                    }
                })
                .collect();
            out.push(format!("  cluster tiers {}", words.join(", ")));
            for t in d.cluster_tiers.iter().filter(|t| !t.same) {
                for l in &t.diff {
                    out.push(format!(
                        "        {}",
                        op_line(&OpLine::leaf(&tier_path(t.tier, &l.path), l), p)
                    ));
                }
            }
        }
        out.push(String::new());
        for pd in &d.packs {
            out.push(packline(pd, p));
            packdetail(pd, p, &mut out);
        }
        if let Some(note) = pack_order_note(d) {
            out.push(String::new());
            out.push(p.yellow(&format!("  note: {note}")));
        }
        let mut s = out.join("\n");
        s.push('\n');
        if full {
            s.push_str(&super::unified::full_diffs(a, b, d, p, false));
        }
        Ok(s)
    }

    fn verdict(&self, d: &Diff) -> Option<String> {
        let v = d.verify.as_ref()?;
        let p = &self.paint;
        let parts = verdict_parts(v, d.same, &d.a, str::to_string);
        let word = match v.status {
            VerifyStatus::Unchanged => p.bgreen(parts.word()),
            VerifyStatus::Unverified => p.byellow(parts.word()),
            VerifyStatus::Drifted => p.bred(parts.word()),
        };
        let mut line = format!("{word}: {}", parts.line(&d.a));
        for sentence in invalid_pack_sentences(&v.invalid_packs) {
            line.push_str(&format!("\n{}: {sentence}", p.bred("INVALID")));
        }
        if let Some(heading) = who_heading(v) {
            line.push('\n');
            line.push_str(&heading);
            for e in v.who.iter().flatten() {
                line.push_str("\n  ");
                line.push_str(&who_entry(e, &v.uid));
            }
        }
        Some(line)
    }

    fn render_verify_summary(&self, s: &VerifySummary) -> Result<String> {
        let p = &self.paint;
        let mut out = String::new();
        for r in &s.records {
            // the words of a column: `OK` padded to the width of `DRIFT`
            let word = format!("{:<5}", record_word(r.status));
            let word = match r.status {
                RecordStatus::Unchanged => p.bgreen(&word),
                RecordStatus::Drifted => p.bred(&word),
                RecordStatus::Unverified | RecordStatus::Error => p.byellow(&word),
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
                PairStatus::Identical | PairStatus::SameContent => p.bgreen(&word),
                PairStatus::RegistryMove => p.byellow(&word),
                PairStatus::Different | PairStatus::UnknownUpstream => p.bred(&word),
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
            for note in pair_notes(pair) {
                out.push_str(&format!("               {}\n", p.yellow(&note)));
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
                RUNNING_STATE => p.green(&c.state),
                crate::ops::UNKNOWN_STATE => p.yellow(&c.state),
                _ => c.state.clone(),
            };
            out.push_str(&format!(
                "  {}  {}  project {}",
                p.bold(&format!("{:<width$}", dash(&c.name))),
                p.dim(&c.uid),
                project_label(&c.project_uid, &c.project_name),
            ));
            // a cluster template has no state
            if !state.is_empty() {
                out.push_str(&format!("  {state}"));
            }
            // the heading names the clusters, so a row names each other kind
            if c.kind != ConsumerKind::Cluster {
                out.push_str(&format!("  {}", p.cyan(c.kind.noun())));
            }
            if !c.version.is_empty() && c.version != r.profile.version {
                out.push_str(&format!(
                    "  {}",
                    p.byellow(&format!(
                        "runs {}@{}, which uid {} no longer names",
                        r.profile.name, c.version, r.profile.uid
                    ))
                ));
            }
            if let Some(msg) = entry_error(c.error.as_ref()) {
                out.push_str(&format!("  {}", p.dim(&format!("({msg})"))));
            }
            if c.unlisted {
                out.push_str(&format!(
                    "  {}",
                    p.byellow("not in the consumer list of the profile")
                ));
            }
            out.push('\n');
        }
        out.push_str(&format!(
            "  {}\n",
            p.dim(&second_reading_line(&r.clusters_read, str::to_string))
        ));
        out.push_str(&format!("  {}\n", p.dim(&edge_hosts_line(r))));
        Ok(out)
    }

    fn render_set(&self, r: &crate::ops::SetReport) -> Result<String> {
        let mut out = String::new();
        for rec in &r.records {
            out.push_str(&match &rec.root {
                Some(root) => format!(
                    "wrote {}  cluster {} (uid {})  root {}\n",
                    rec.path.display(),
                    rec.cluster.name,
                    rec.cluster.uid,
                    short(root)
                ),
                None => format!(
                    "could not record cluster {} (uid {}): {}\n",
                    rec.cluster.name,
                    rec.cluster.uid,
                    super::set_error(rec)
                ),
            });
        }
        for path in &r.removed {
            out.push_str(&format!("removed {}\n", path.display()));
        }
        Ok(out)
    }

    fn render_behind(&self, r: &crate::ops::BehindReport) -> Result<String> {
        let p = &self.paint;
        let mut out = format!("{}\n", super::behind_heading(r));
        let name_w = r.pins.iter().map(|x| x.name.len()).max().unwrap_or(0);
        let ver_w = r.pins.iter().map(|x| x.version.len()).max().unwrap_or(0);
        let reg_w = r
            .pins
            .iter()
            .map(|x| dash(&x.registry).len())
            .max()
            .unwrap_or(0);
        for pin in &r.pins {
            let status = super::pin_status(pin);
            let status = if pin.error.is_some() {
                p.byellow(&status)
            } else if pin.behind {
                p.yellow(&status)
            } else {
                p.dim(&status)
            };
            out.push_str(&format!(
                "{} [{:<5}] {}  {}  {}  {status}\n",
                if pin.behind { "~" } else { " " },
                pin.layer,
                p.bold(&format!("{:<name_w$}", pin.name)),
                format_args!("{:<ver_w$}", pin.version),
                format_args!("{:<reg_w$}", dash(&pin.registry)),
            ));
            if let Some(newest) = pin
                .newer
                .first()
                .filter(|n| pin.behind && !n.listed.is_empty())
            {
                out.push_str(&format!(
                    "      {}\n",
                    p.dim(&format!(
                        "{} listed since {}",
                        newest.version, newest.listed
                    ))
                ));
            }
            if let Some(line) = super::bump_line(pin) {
                out.push_str(&format!("      {line}\n"));
            }
            if let Some(arrived) = pin.arrived.as_ref().filter(|a| !a.is_empty()) {
                out.push_str(&format!(
                    "      {}\n",
                    p.cyan(&format!("arrived: {}", arrived.join(", ")))
                ));
            }
            if let Some(msg) = entry_error(pin.error.as_ref()) {
                out.push_str(&format!("      {}\n", p.dim(&format!("({msg})"))));
            }
            for by in &pin.pinned_by {
                out.push_str(&format!(
                    "      {}\n",
                    p.dim(&format!("pinned by {} ({})", by.path, by.subject))
                ));
            }
        }
        Ok(out)
    }

    fn render_packs(&self, l: &PackListing) -> Result<String> {
        let p = &self.paint;
        let mut out = String::new();
        if let Some(sentence) = no_version_sentence(l) {
            return Ok(format!("{sentence}\n"));
        }
        for r in &l.registries {
            // the heading states what the registry holds. A flag that cuts the list says so, so
            // that the count is never read as the registry's inventory.
            let count = if r.versions.len() == r.versions_offered {
                count_of(r.versions_offered, "version")
            } else {
                format!(
                    "{} of {}",
                    r.versions.len(),
                    count_of(r.versions_offered, "version")
                )
            };
            out.push_str(&format!(
                "{} in {} ({count}{})\n",
                p.bold(&l.name),
                p.bold(&r.registry),
                registry_note(r)
                    .map(|n| format!(", {n}"))
                    .unwrap_or_default()
            ));
            // the note column only where a version has a note, so a registry in use reads as before
            let notes: Vec<Option<String>> = r.versions.iter().map(version_note).collect();
            let any_note = notes.iter().any(Option::is_some);
            let rows: Vec<Vec<String>> = r
                .versions
                .iter()
                .zip(&notes)
                .map(|(v, note)| {
                    let mut row = vec![
                        v.version.clone(),
                        v.pack_uid.clone(),
                        dash(&v.kind).to_string(),
                        if v.cloud_types.is_empty() {
                            "-".to_string()
                        } else {
                            v.cloud_types.join(",")
                        },
                        if v.tags.is_empty() {
                            "-".to_string()
                        } else {
                            v.tags.join(" ")
                        },
                    ];
                    if any_note {
                        row.push(note.clone().unwrap_or_default());
                    }
                    row
                })
                .collect();
            let widths = column_widths(&rows);
            for (v, row) in r.versions.iter().zip(&rows) {
                let line = table_line("  ", row, &widths, |i, cell| match i {
                    0 if v.latest => p.bold(cell),
                    1 => p.dim(cell),
                    _ => cell.to_string(),
                });
                // a version with no note leaves the note column empty: no spaces at the end
                out.push_str(line.trim_end());
                out.push('\n');
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
            // the marks of a version end its row, so their column moves with the columns before
            let (rows, marked): (Vec<Vec<String>>, Vec<Option<usize>>) = profiles
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
                    let marks = profile_marks(pr, l);
                    let marked = marks.as_ref().map(|_| row.len());
                    row.extend(marks);
                    (row, marked)
                })
                .unzip();
            let widths = column_widths(&rows);
            for ((pr, row), marked) in profiles.iter().zip(&rows).zip(&marked) {
                let failed = pr.error.is_some();
                out.push_str(&table_line("  ", row, &widths, |i, cell| match i {
                    0 => p.bold(cell),
                    1 => p.dim(cell),
                    3 if failed => p.dim(cell),
                    i if Some(i) == *marked => p.yellow(cell),
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
                        1 => matched(p, cell, l.pack.as_deref()),
                        _ => cell.to_string(),
                    }));
                }
            }
        }
        if let Some(clusters) = &l.clusters {
            out.push_str(&format!(
                "\n{}\n",
                cluster_count(clusters.len(), l.cluster.as_deref())
            ));
            for line in super::query_line(l).into_iter().chain(virtual_line(l)) {
                out.push_str(&format!("{}\n", p.dim(&line)));
            }
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
                        row.push(format!(
                            "project {}",
                            project_label(&c.project_uid, &c.project_name)
                        ));
                    }
                    // a query is a question about labels, so its rows show them
                    if l.query.is_some() {
                        row.push(super::labels_text(&c.labels));
                    }
                    row.extend(virtual_note(c));
                    if let Some(msg) = entry_error(c.error.as_ref()) {
                        row.push(format!("({msg})"));
                    }
                    if let Some(note) = super::imported_note(c) {
                        row.push(note.to_string());
                    }
                    row
                })
                .collect();
            let widths = column_widths(&rows);
            for (c, row) in clusters.iter().zip(&rows) {
                out.push_str(&table_line("  ", row, &widths, |i, cell| match i {
                    0 => p.bold(cell),
                    1 => matched(p, cell, l.cluster.as_deref()),
                    2 if c.state == RUNNING_STATE => p.green(cell),
                    _ => cell.to_string(),
                }));
                let applied: Vec<Vec<String>> = c
                    .profiles
                    .iter()
                    .flatten()
                    .map(|a| {
                        let mut row = vec![
                            applied_ref(a),
                            dash(&a.uid).to_string(),
                            dash(&a.kind).to_string(),
                        ];
                        if c.pending_updates.iter().any(|u| u.profile_uid == a.uid) {
                            row.push("behind: an update waits".to_string());
                        }
                        row
                    })
                    .collect();
                let applied_widths = column_widths(&applied);
                for row in &applied {
                    out.push_str(&table_line(
                        "    ",
                        row,
                        &applied_widths,
                        |i, cell| match i {
                            1 => p.dim(cell),
                            3 => p.yellow(cell),
                            _ => cell.to_string(),
                        },
                    ));
                }
            }
        }
        if let Some(projects) = &l.projects {
            out.push_str(&format!("\n{}\n", count_of(projects.len(), "project")));
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
        if let Some(filters) = &l.filters {
            out.push_str(&format!(
                "\n{}\n",
                count_of(filters.len(), "saved tag filter")
            ));
            let rows: Vec<Vec<String>> = filters
                .iter()
                .map(|f| vec![f.reference(), f.uid.clone(), f.expression()])
                .collect();
            let widths = column_widths(&rows);
            for row in &rows {
                out.push_str(&table_line("  ", row, &widths, |i, cell| match i {
                    0 => p.bold(cell),
                    1 => p.dim(cell),
                    _ => cell.to_string(),
                }));
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_part_of_a_pack_name_that_matched_is_marked() {
        let on = Paint::resolve(crate::color::ColorMode::Always);
        assert_eq!(
            matched(&on, "cni-cilium-oss", Some("cilium")),
            format!("cni-{}-oss", on.byellow("cilium")),
            "the match is marked, and the rest of the name is not"
        );
        // the search ignores case, and the name keeps its own
        assert_eq!(
            matched(&on, "cni-Cilium-oss", Some("cilium")),
            format!("cni-{}-oss", on.byellow("Cilium"))
        );
        // the padding of the column stays in the cell
        assert_eq!(
            matched(&on, "csi-aws-ebs   ", Some("aws")),
            format!("csi-{}-ebs   ", on.byellow("aws"))
        );

        // nothing to mark, nothing to do
        assert_eq!(matched(&on, "kubernetes", Some("cilium")), "kubernetes");
        assert_eq!(matched(&on, "kubernetes", None), "kubernetes");
        assert_eq!(matched(&on, "kubernetes", Some("")), "kubernetes");
        // no color: the plain name, so a pipe and a file read the same
        let off = Paint::off();
        assert_eq!(
            matched(&off, "cni-cilium-oss", Some("cilium")),
            "cni-cilium-oss"
        );
        // text that is not ASCII is left alone, because lowercasing can change its length
        assert_eq!(matched(&on, "pack-ß", Some("ß")), "pack-ß");
        assert_eq!(matched(&on, "cilium", Some("CILIUM")), on.byellow("cilium"));
    }
}
