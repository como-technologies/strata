//! The terminal report: plain when piped, colored on a terminal.

use super::changes::{OpLine, artifacts_summary, moved_word, tier_op_lines, variable_lines};
use super::consumers::{
    NOT_LISTED, consumer_count, consumer_counts, consumer_runs, consumers_heading,
    consumers_sentence, edge_hosts_line, second_reading_line,
};
use super::list::{
    applied_ref, cluster_count, list_count, list_scope, no_version_sentence, pack_registry,
    pack_version, profile_marks, project_label, registry_heading, side_projects, version_note,
    virtual_line, virtual_note,
};
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
use super::{Renderer, count_of, entry_error, key_line, paint_state, palette_line, short};
use crate::closed_set::ClosedSet;
use crate::closed_set::unknown::report::Report;
use crate::diff::Diff;
use crate::error::Result;
use crate::ops::{
    Chain, ConsumerKind, ConsumersReport, ListedCluster, ListedProfile, ListedProject, Listing,
    PackListing, PairStatus, RecordStatus, SavedFilter, VerifySummary,
};
use crate::snapshot::Snapshot;
use crate::term::color::Paint;
use crate::text::dash;
use lines::{op_line, packdetail, packline, variable_line};
use table::{column_widths, matched, painted_text, table_line};

mod behind;
mod lines;
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
/// consumers follow when the diff has them.
fn head_lines(d: &Diff, p: &Paint) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    out.push(format!(
        "{} (uid {})  ->  {} (uid {})",
        p.bold(&d.a.title()),
        p.dim(dash(d.a.shown_uid())),
        p.bold(&d.b.title()),
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
            DiffNote::Palette(lines) => {
                out.extend(lines.iter().map(|line| palette_line(line)));
            }
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
        let parts = verdict_parts(v, d.same, &d.a);
        let word = paint_status(p, RecordStatus::from(v.status), parts.word());
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
            let word = paint_status(p, r.status, &format!("{:<5}", record_word(r.status)));
            out.push_str(&format!(
                "{word}  {}  {}  {}\n",
                r.path,
                p.bold(dash(&r.subject)),
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
        let width = r
            .consumers
            .iter()
            .map(|c| dash(&c.name).len())
            .max()
            .unwrap_or(0);
        for c in &r.consumers {
            let state = paint_state(p, &c.state);
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
        out.push_str(&format!(
            "  {}\n",
            p.dim(&second_reading_line(&r.clusters_read))
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
            out.push_str(&format!("{}\n", registry_heading(l, r, |t| p.bold(t))));
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
        for line in super::list::unread_lines(l) {
            out.push_str(&format!("\n{}\n", p.red(&line)));
        }
        Ok(out)
    }
}

/// The table of the profile versions of a listing, with its count and the packs of each one.
fn list_profiles(out: &mut String, p: &Paint, l: &Listing, profiles: &[ListedProfile]) {
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
                row.push(super::list::profile_type_cell(pr).to_string());
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
            .map(|pk| {
                let mut row = vec![pk.layer.clone(), pk.name.clone(), pack_version(pk)];
                // a pack with no registry ends at its version, with no space after it
                row.extend(pack_registry(pk).map(str::to_string));
                row
            })
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

/// The table of the clusters of a listing, with its count, the lines of its query, and the
/// profiles of each cluster.
fn list_clusters(out: &mut String, p: &Paint, l: &Listing, clusters: &[ListedCluster]) {
    out.push_str(&format!(
        "\n{}\n",
        cluster_count(clusters.len(), l.cluster.as_deref())
    ));
    for line in super::list::query_line(l)
        .into_iter()
        .chain(virtual_line(l))
    {
        out.push_str(&format!("{}\n", p.dim(&line)));
    }
    let rows: Vec<Vec<String>> = clusters.iter().map(|c| cluster_row(l, c)).collect();
    let widths = column_widths(&rows);
    for (c, row) in clusters.iter().zip(&rows) {
        out.push_str(&table_line("  ", row, &widths, |i, cell| match i {
            0 => p.bold(cell),
            1 => matched(p, cell, l.cluster.as_deref()),
            2 => painted_text(cell, |state| paint_state(p, state)),
            _ => cell.to_string(),
        }));
        let applied = applied_rows(c);
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

/// The cells of the row of a cluster.
fn cluster_row(l: &Listing, c: &ListedCluster) -> Vec<String> {
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
        row.push(super::list::labels_text(&c.labels));
    }
    row.extend(virtual_note(c));
    if let Some(msg) = entry_error(c.error.as_ref()) {
        row.push(format!("({msg})"));
    }
    if let Some(note) = super::list::imported_note(c) {
        row.push(note.to_string());
    }
    row
}

/// The rows of the profiles that a cluster runs, under the row of the cluster.
fn applied_rows(c: &ListedCluster) -> Vec<Vec<String>> {
    c.profiles
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
        .collect()
}

/// The table of the projects of a listing, with its count.
fn list_projects(out: &mut String, p: &Paint, projects: &[ListedProject]) {
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

/// The table of the saved tag filters of a listing, with its count.
fn list_filters(out: &mut String, p: &Paint, filters: &[SavedFilter]) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::color::{ColorMode, Stream};

    #[test]
    fn the_part_of_a_pack_name_that_matched_is_marked() {
        let on = Paint::resolve(ColorMode::Always, Stream::Stdout);
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

    #[test]
    fn the_table_of_clusters_paints_the_state_of_each_row_and_not_its_padding() {
        let on = Paint::resolve(ColorMode::Always, Stream::Stdout);
        let listing = Listing {
            schema: crate::ops::LIST_SCHEMA.to_string(),
            tool: crate::snapshot::Tool::current(),
            generated_at: "2026-11-02T00:00:00Z".to_string(),
            project_uid: String::new(),
            project_name: String::new(),
            pack: None,
            cluster: None,
            profiles: None,
            clusters: None,
            includes_virtual: None,
            projects: None,
            query: None,
            resolved: None,
            query_filter: None,
            filters: None,
            errors: std::collections::BTreeMap::new(),
        };
        let clusters: Vec<ListedCluster> = ["Running", "Provisioning", "Error"]
            .into_iter()
            .map(|state| {
                serde_json::from_value(serde_json::json!({
                    "ref": format!("cluster:{state}"),
                    "name": state,
                    "uid": state,
                    "state": state,
                    "cloudType": "aws",
                    "projectUid": "",
                }))
                .unwrap()
            })
            .collect();
        let mut out = String::new();
        list_clusters(&mut out, &on, &listing, &clusters);
        for (state, painted) in [
            ("Running", on.green("Running")),
            ("Provisioning", on.yellow("Provisioning")),
            ("Error", on.red("Error")),
        ] {
            // the renderer paints the state whatever the width of its column, and not its padding
            let padding = " ".repeat("Provisioning".len() - state.len());
            let cell = format!("{painted}{padding}  aws");
            assert!(out.contains(&cell), "{state}: {out:?}");
        }
        let mut plain = String::new();
        list_clusters(&mut plain, &Paint::off(), &listing, &clusters);
        assert!(!plain.contains('\x1b'), "{plain:?}");
        assert!(plain.contains("Running       aws"), "{plain:?}");
    }

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
