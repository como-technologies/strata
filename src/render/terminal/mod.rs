//! The terminal report: plain when piped, colored on a terminal.

use super::changes::{
    OpLine, artifacts_summary, moved_word, packs_heading, tier_op_lines, variable_lines,
};
use super::consumers::{
    NOT_LISTED, consumer_counts, consumer_project, consumer_ref, consumer_runs, consumers_heading,
    consumers_sentence, not_checked_line, scope_line, searched_rows,
};
use super::list::{list_scope, no_version_sentence, registry_heading, side_projects, version_note};
use super::notes::{
    DiffNote, TierLine, diff_notes, invalid_pack_sentences, pack_order_note, tier_line,
    variables_comparable,
};
use super::report::report_heading;
use super::same_release::{
    chain_verdict, diff_word, pair_detail, pair_notes, pair_projects, pair_word, record_word,
    renumber_lines,
};
use super::verify::{counts_line, record_detail, verdict_parts, who_entry, who_heading};
use super::{
    Kind, Reading, Renderer, count_of, entry_error, key_line, paint_state, palette_line, short,
};
use crate::closed_set::ClosedSet;
use crate::closed_set::unknown::report::Report;
use crate::diff::Diff;
use crate::error::Result;
use crate::ops::{
    Chain, ConsumersReport, Listing, PackListing, PairStatus, RecordStatus, VerifySummary,
};
use crate::reference::Ref;
use crate::snapshot::{Snapshot, Source};
use crate::term::color::{LineClass, Paint};
use crate::text::dash;
use lines::{op_line, packdetail, packline, variable_line};
use list::{list_clusters, list_filters, list_profiles, list_projects, list_registries};
use table::{column_widths, painted_text, table_line};

mod behind;
mod lines;
mod list;
mod table;

/// `text` in the color of a record status. Green is a record that agrees with Palette, and red is
/// drift. Yellow is a record that is unverified, or that strata could not check.
fn paint_status(p: &Paint, status: RecordStatus, text: &str) -> String {
    match status {
        RecordStatus::Unchanged => p.bgreen(text),
        RecordStatus::Drifted => p.bred(text),
        RecordStatus::Unverified | RecordStatus::Error => p.byellow(text),
    }
}

/// Renders the text report.
pub struct TerminalRenderer {
    /// Color policy, already resolved.
    pub paint: Paint,
}

/// The lines above the variables: the two records, the root, and the notes. The images and the
/// consumers follow when the diff has them. The answer of Palette is not here: it is the last line
/// of the report, see [`answer_lines`].
fn head_lines(d: &Diff, p: &Paint) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    out.push(format!(
        "{} (uid {})  ->  {} (uid {})",
        side_title(&d.a, p),
        p.dim(dash(d.a.shown_uid())),
        side_title(&d.b, p),
        p.dim(dash(d.b.shown_uid()))
    ));
    let word = diff_word(d);
    out.push(key_line(
        "root",
        format!(
            "{} -> {}   {}",
            p.dim(short(&d.root_a)),
            p.dim(short(&d.root_b)),
            if d.same || d.same_content {
                p.bgreen(word)
            } else {
                p.bred(word)
            }
        ),
    ));
    if let Some((a, b)) = side_projects(&d.a.project_uid, &d.b.project_uid) {
        out.push(key_line("project", format!("{a} -> {b}")));
    }
    for note in diff_notes(d) {
        match note {
            DiffNote::Warn(text) => out.push(key_line("note", p.byellow(&text))),
            DiffNote::Good(text) => out.push(key_line("note", p.bgreen(&text))),
            DiffNote::Palette(_) => {}
        }
    }
    if let Some(line) = artifacts_summary(d) {
        out.push(key_line("images", line));
    }
    if let Some((a, b)) = consumer_counts(d) {
        // the noun agrees with the count next to it: `1 -> 2 clusters`, `2 -> 1 cluster`
        let noun = crate::text::plural(b, "cluster", "clusters");
        out.push(key_line("consumers", format!("{a} -> {b} {noun}")));
    } else if let Some(sentence) = consumers_sentence(d) {
        out.push(key_line("consumers", sentence));
    }
    out
}

/// A side of the heading of a diff: the ref of a pack, such as
/// `'pack:Public Repo/kubernetes@1.36.2'`, or the title of each other record, such as
/// `aws-stack@1.1.1 on cluster payments`.
fn side_title(s: &Source, p: &Paint) -> String {
    match &s.pack {
        Some(pk) => {
            let r = Ref::pack_in(&pk.registry_name, &pk.registry_uid, &pk.name, &pk.version);
            Kind::Pack.paint(p, &r.shell_word())
        }
        None => p.bold(&s.title()),
    }
}

/// The answer of Palette to `--repave` and to `--validate`, one bold line each, such as
/// `palette    accepts`. It is the answer of a check, so it is the last part of the report.
fn answer_lines(d: &Diff, p: &Paint) -> Vec<String> {
    diff_notes(d)
        .into_iter()
        .filter_map(|note| match note {
            DiffNote::Palette(lines) => Some(lines),
            DiffNote::Warn(_) | DiffNote::Good(_) => None,
        })
        .flatten()
        .map(|line| p.line(LineClass::Answer, &palette_line(&line)))
        .collect()
}

/// The lines of the variables, the values of the cluster, and the tiers of the cluster.
fn change_lines(d: &Diff, p: &Paint) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    // the blank line belongs to the variables block, so two packs, which have no variables,
    // leave neither the line nor the gap where it was
    if variables_comparable(d) {
        out.push(String::new());
        out.push(format!(
            "  variables {}",
            moved_word(d.variables.changed, p)
        ));
    }
    for v in variable_lines(&d.variables.by_name) {
        out.push(format!("        {}", variable_line(&v, p)));
    }
    if let Some(cv) = d.cluster_variables.as_ref().filter(|c| c.changed) {
        out.push(format!("  cluster values {}", moved_word(true, p)));
        for c in &cv.values {
            out.push(format!("        {}", op_line(&OpLine::cluster_value(c), p)));
        }
    }
    match tier_line(d) {
        TierLine::NotCompared => {}
        TierLine::Unchanged => out.push(format!("  cluster tiers {}", moved_word(false, p))),
        TierLine::Differ(states) => {
            let words: Vec<String> = states
                .iter()
                .map(|(t, same)| format!("{t} {}", moved_word(!*same, p)))
                .collect();
            out.push(format!("  cluster tiers {}", words.join(", ")));
            for line in tier_op_lines(d) {
                out.push(format!("        {}", op_line(&line, p)));
            }
        }
    }
    out
}

impl Renderer for TerminalRenderer {
    fn render(&self, d: &Diff, a: &Snapshot, b: &Snapshot, full: bool) -> Result<String> {
        let p = &self.paint;
        let mut out = head_lines(d, p);
        out.extend(change_lines(d, p));
        out.push(String::new());
        out.push(format!("  {}", packs_heading(d)));
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
        let answer = answer_lines(d, p);
        if !answer.is_empty() {
            s.push_str(&format!("\n{}\n", answer.join("\n")));
        }
        Ok(s)
    }

    fn verdict(&self, d: &Diff) -> Option<String> {
        let v = d.verify.as_ref()?;
        let p = &self.paint;
        let parts = verdict_parts(v, d.same, &d.a);
        let mut lines = Vec::new();
        if let Some(heading) = who_heading(v) {
            lines.push(heading);
            for e in v.who.iter().flatten() {
                lines.push(format!("  {}", who_entry(e, &v.uid)));
            }
        }
        for sentence in invalid_pack_sentences(&v.invalid_packs) {
            lines.push(format!("{}: {sentence}", p.bred("INVALID")));
        }
        // the verdict is the answer of the check, so it is the last line
        let word = paint_status(p, RecordStatus::from(v.status), parts.word());
        let verdict = format!("{word}: {}", parts.line(&d.a));
        lines.push(p.line(LineClass::Answer, &verdict));
        Some(lines.join("\n"))
    }

    fn render_verify_summary(&self, s: &VerifySummary) -> Result<String> {
        let p = &self.paint;
        let mut out = String::new();
        for r in &s.records {
            // the words of a column: `OK` padded to the width of `DRIFT`
            let word = paint_status(p, r.status, &format!("{:<5}", record_word(r.status)));
            out.push_str(&format!(
                "{word}  {}  {}  {}\n",
                r.path,
                p.bold(dash(&r.subject)),
                record_detail(r)
            ));
        }
        out.push_str(&p.line(LineClass::Answer, &counts_line(s)));
        out.push('\n');
        Ok(out)
    }

    fn render_chain(&self, c: &Chain) -> Result<String> {
        let p = &self.paint;
        let mut out = String::new();
        for (index, pair) in c.pairs.iter().enumerate() {
            let word = format!("{:<13}", pair_word(pair.status));
            let word = match pair.status {
                PairStatus::Identical | PairStatus::SameContent => p.bgreen(&word),
                PairStatus::RegistryMove | PairStatus::Renumber => p.byellow(&word),
                PairStatus::Different | PairStatus::UnknownUpstream => p.bred(&word),
            };
            out.push_str(&format!(
                "{word}  {} -> {}   {}\n",
                pair.a,
                pair.b,
                p.dim(&pair_detail(pair))
            ));
            for line in pair_projects(c, index)
                .into_iter()
                .chain(renumber_lines(pair))
            {
                out.push_str(&format!("               {line}\n"));
            }
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
        // a row starts with the ref of the consumer, which names its kind, or with its uid for a
        // kind that no ref reads
        let refs: Vec<Option<Ref>> = r.consumers.iter().map(consumer_ref).collect();
        let ids: Vec<String> = r
            .consumers
            .iter()
            .zip(&refs)
            .map(|(c, reference)| match reference {
                Some(reference) => reference.shell_word(),
                None => dash(&c.uid).to_string(),
            })
            .collect();
        let id_w = ids.iter().map(|i| i.chars().count()).max().unwrap_or(0);
        let width = r
            .consumers
            .iter()
            .map(|c| dash(&c.name).chars().count())
            .max()
            .unwrap_or(0);
        for ((c, reference), id) in r.consumers.iter().zip(&refs).zip(&ids) {
            let state = paint_state(p, &c.state);
            let id = format!("{id:<id_w$}");
            out.push_str(&format!(
                "  {}  {}  project {}",
                match reference {
                    Some(_) => painted_text(&id, |t| Kind::Cluster.paint(p, t)),
                    None => p.dim(&id),
                },
                format_args!("{:<width$}", dash(&c.name)),
                consumer_project(c),
            ));
            // a cluster template has no state
            if !state.is_empty() {
                out.push_str(&format!("  {state}"));
            }
            // a ref names the kind, so a row with none names its kind in words
            if reference.is_none() {
                out.push_str(&format!("  {}", p.dim(c.kind.noun())));
            }
            if let Some(runs) = consumer_runs(c, r, str::to_string) {
                out.push_str(&format!("  {}", p.byellow(&runs)));
            }
            if let Some(msg) = entry_error(c.error.as_ref()) {
                out.push_str(&format!("  {}", p.dim(&format!("({msg})"))));
            }
            if c.unlisted {
                out.push_str(&format!("  {}", p.byellow(NOT_LISTED)));
            }
            out.push('\n');
        }
        out.push_str(&format!("  {}\n", p.dim("searched:")));
        // one width for each report, so the rows of two reports line up
        let width = Reading::ALL.iter().map(|w| w.as_str().len()).max();
        let width = width.unwrap_or(0);
        for (what, found) in searched_rows(r) {
            out.push_str(&format!(
                "    {}\n",
                p.dim(&format!("{:<width$}  {found}", what.as_str()))
            ));
        }
        if let Some(scope) = scope_line(r) {
            out.push_str(&format!("  {}\n", p.dim(&scope)));
        }
        if let Some(unread) = not_checked_line(r) {
            out.push_str(&format!("  {}\n", p.byellow(&unread)));
        }
        Ok(out)
    }

    fn render_set(&self, r: &crate::ops::SetReport) -> Result<String> {
        let mut out = String::new();
        for rec in &r.records {
            // each line names the cluster by its ref, and then by its name
            let cluster = Ref::cluster(&rec.cluster.uid).shell_word();
            out.push_str(&match &rec.root {
                Some(root) => format!(
                    "wrote {}  {cluster}  {}  root {}\n",
                    rec.path.display(),
                    dash(&rec.cluster.name),
                    short(root)
                ),
                None => format!(
                    "could not record {cluster} ({}): {}\n",
                    dash(&rec.cluster.name),
                    super::list::set_error(rec)
                ),
            });
        }
        for path in &r.removed {
            out.push_str(&format!("removed {}\n", path.display()));
        }
        Ok(out)
    }

    fn render_report(&self, r: &Report) -> Result<String> {
        let p = &self.paint;
        let mut out = format!("{}\n", p.bold(&report_heading(r)));
        let rows: Vec<Vec<String>> = r
            .unknown
            .iter()
            .map(|e| {
                vec![
                    e.sighting.set.clone(),
                    e.sighting.value.clone(),
                    e.sighting.route.clone(),
                    count_of(e.count, "time"),
                    format!("strata {}", e.sighting.strata),
                ]
            })
            .collect();
        if !rows.is_empty() {
            out.push('\n');
        }
        let widths = column_widths(&rows);
        for row in &rows {
            out.push_str(&table_line("  ", row, &widths, |i, cell| match i {
                1 => p.bold(cell),
                4 => p.dim(cell),
                _ => cell.to_string(),
            }));
        }
        Ok(out)
    }

    fn render_behind(&self, r: &crate::ops::BehindReport, behind_only: bool) -> Result<String> {
        Ok(behind::terminal_behind(r, behind_only, &self.paint))
    }

    fn render_packs(&self, l: &PackListing) -> Result<String> {
        let p = &self.paint;
        let mut out = String::new();
        if let Some(sentence) = no_version_sentence(l) {
            return Ok(format!("{sentence}\n"));
        }
        for r in &l.registries {
            out.push_str(&format!(
                "{}\n",
                registry_heading(l, r, |kind, name| kind.paint(p, name))
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
                        v.kind.as_str().to_string(),
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
            list_profiles(&mut out, p, l, profiles);
        }
        if let Some(clusters) = &l.clusters {
            list_clusters(&mut out, p, l, clusters);
        }
        if let Some(projects) = &l.projects {
            list_projects(&mut out, p, projects);
        }
        if let Some(filters) = &l.filters {
            list_filters(&mut out, p, filters);
        }
        if let Some(registries) = &l.registries {
            list_registries(&mut out, p, registries);
        }
        for line in super::list::unread_lines(l) {
            out.push_str(&format!("\n{}\n", p.red(&line)));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::color::{ColorMode, Stream};

    #[test]
    fn a_record_status_has_one_color_in_the_verdict_and_in_the_row_of_the_summary() {
        let on = Paint::resolve(ColorMode::Always, Stream::Stdout);
        assert_eq!(
            paint_status(&on, RecordStatus::Unchanged, "OK"),
            on.bgreen("OK")
        );
        assert_eq!(
            paint_status(&on, RecordStatus::Drifted, "DRIFT"),
            on.bred("DRIFT")
        );
        // an error shows the color of an unverified record
        for status in [RecordStatus::Unverified, RecordStatus::Error] {
            assert_eq!(paint_status(&on, status, "x"), on.byellow("x"));
        }
        assert_eq!(
            paint_status(&Paint::off(), RecordStatus::Drifted, "DRIFT"),
            "DRIFT"
        );
    }
}
