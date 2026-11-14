//! The pull request comment.

mod behind;

use super::changes::{
    OpLine, VarLine, artifact_lines, artifacts_summary, manifest_change, moved, plain_flags,
    status_mark, status_word, tier_op_lines, traits_suffix, upstream_line, values_changed,
    values_summary, values_unparsed_sentence, variable_lines, version_display,
};
use super::consumers::{
    NOT_LISTED, consumer_count, consumer_counts, consumer_runs, consumers_heading,
    consumers_sentence, edge_hosts_line, second_reading_line,
};
use super::list::{
    applied_ref, cluster_count, list_count, list_scope, no_version_sentence, pack_registry,
    pack_version, profile_marks, project_label, registry_heading, side_projects, version_note,
    virtual_group, virtual_line,
};
use super::notes::{
    DiffNote, TierLine, diff_notes, invalid_mark, invalid_pack_sentences, pack_order_note,
    pending_profile_names, tier_line, variables_comparable,
};
use super::report::markdown_report;
use super::same_release::{
    chain_verdict, diff_word, pair_detail, pair_notes, pair_projects, pair_word, record_word,
    renumber_lines,
};
use super::verify::{counts_line, record_detail, verdict_parts, who_entry, who_heading};
use super::{Renderer, capitalized, count_of, entry_error, short};
use crate::closed_set::ClosedSet;
use crate::closed_set::unknown::report::Report;
use crate::diff::{Diff, Leaf, Op, PackDiff, Status};
use crate::error::Result;
use crate::model::profile_type::ProfileType;
use crate::ops::{
    Chain, ConsumerKind, ConsumersReport, ListedCluster, ListedProfile, ListedProject, Listing,
    PackListing, RecordStatus, SavedFilter, VerifySummary,
};
use crate::snapshot::Snapshot;
use crate::term::color::Paint;
use crate::text::{code_block, code_cell, code_span, dash, html_text, markdown_cell};

/// Renders markdown for a pull request comment.
pub struct MarkdownRenderer;

fn leaf(l: &Leaf) -> String {
    op_line(&OpLine::leaf(&l.path, l))
}

/// The lines of a `diff` block: `- path: old` and `+ path: new`, one of them for a side that has
/// no value.
fn op_line(l: &OpLine) -> String {
    let OpLine { path, old, new, .. } = l;
    match l.op {
        Op::Changed => format!("- {path}: {old}\n+ {path}: {new}\n"),
        Op::Added => format!("+ {path}: {new}\n"),
        Op::Removed => format!("- {path}: {old}\n"),
    }
}

pub(super) fn variable_line(v: &VarLine<'_>) -> String {
    match v {
        VarLine::Added(s) => format!("+ variable {}{}\n", s.name, traits_suffix(s)),
        VarLine::Removed(s) => format!("- variable {}{}\n", s.name, traits_suffix(s)),
        VarLine::Field(name, l) => op_line(&OpLine::leaf(&format!("{name}.{}", l.path), l)),
    }
}

/// The heading and the table of the two records: their uids, projects when they differ, roots,
/// consumers, and fetch times.
fn heading_table(d: &Diff) -> String {
    let mut o = String::new();
    o.push_str(&format!(
        "### {} → {}\n\n",
        markdown_cell(&d.a.title()),
        markdown_cell(&d.b.title())
    ));
    o.push_str("| | A | B |\n|---|---|---|\n");
    o.push_str(&format!(
        "| uid | {} | {} |\n",
        code_cell(dash(d.a.shown_uid())),
        code_cell(dash(d.b.shown_uid()))
    ));
    if let Some((a, b)) = side_projects(&d.a.project_uid, &d.b.project_uid) {
        o.push_str(&format!(
            "| project | {} | {} |\n",
            markdown_cell(a),
            markdown_cell(b)
        ));
    }
    o.push_str(&format!(
        "| root | {} | {} |\n",
        code_cell(short(&d.root_a)),
        code_cell(short(&d.root_b))
    ));
    if let Some((a, b)) = consumer_counts(d) {
        o.push_str(&format!(
            "| consumers | {} | {} |\n",
            count_of(a, "cluster"),
            count_of(b, "cluster")
        ));
    }
    o.push_str(&format!(
        "| fetched | {} | {} |\n\n",
        dash(&d.a.fetched_at),
        dash(&d.b.fetched_at)
    ));
    o
}

/// The result line, then the consumers, the notes, and the verify line, each when the diff has it.
fn result_section(d: &Diff) -> String {
    let mut o = String::new();
    let s = &d.summary;
    // two packs have no profile variables, and no request asked for any
    let variables = if variables_comparable(d) {
        format!(" · variables: {}", moved(s.variables_changed).as_str())
    } else {
        String::new()
    };
    o.push_str(&format!(
        "**Result:** {} · max impact: `{}` · packs: {} changed, {} added, {} removed, {} unchanged{}\n\n",
        diff_word(d),
        s.max_impact.as_str(),
        s.packs_changed,
        s.packs_added,
        s.packs_removed,
        s.packs_unchanged,
        variables
    ));
    if let Some(sentence) = consumers_sentence(d) {
        o.push_str(&format!("**Consumers:** {}\n\n", markdown_cell(&sentence)));
    }
    for note in diff_notes(d) {
        o.push_str(&match note {
            DiffNote::Warn(text) | DiffNote::Good(text) => {
                format!("**Note:** {}\n\n", markdown_cell(&text))
            }
            DiffNote::Palette(lines) => {
                format!("**Palette:** {}\n\n", markdown_cell(&lines.join("; ")))
            }
        });
    }
    if let Some(v) = &d.verify {
        o.push_str(&format!(
            "**Verify:** `{}` for uid {}, recorded {}, checked {}\n\n",
            record_word(RecordStatus::from(v.status)),
            code_span(&v.uid),
            v.recorded_at,
            v.checked_at
        ));
    }
    o
}

/// The table with one row for each pack of the diff.
fn pack_table(d: &Diff) -> String {
    let mut o =
        String::from("| | layer | pack | version | change | impact |\n|---|---|---|---|---|---|\n");
    for pd in &d.packs {
        let mark = status_mark(pd.status);
        // a tag rename holds the tags of the answer
        let change = match pd.status {
            Status::Changed => markdown_cell(&plain_flags(pd).replace("->", "→")),
            status => status_word(status).to_string(),
        };
        let change = match invalid_mark(pd) {
            Some(m) => format!("{change} **{}**", markdown_cell(&m)),
            None => change,
        };
        o.push_str(&format!(
            "| `{mark}` | {} | {} | {} | {} | {} |\n",
            markdown_cell(&pd.layer),
            code_cell(&pd.name),
            markdown_cell(&version_display(pd).replace("->", "→")),
            change,
            markdown_cell(&pd.impact.text)
        ));
    }
    o.push('\n');
    o
}

/// The fold with the changes of the variables. It is empty when no variable changed.
fn variables_section(d: &Diff) -> String {
    if !d.variables.changed {
        return String::new();
    }
    let g = &d.variables.by_name;
    let lines: String = variable_lines(g).iter().map(variable_line).collect();
    format!(
        "<details><summary>variables: {} added, {} removed, {} changed</summary>\n\n{}</details>\n\n",
        g.added.len(),
        g.removed.len(),
        g.changed.len(),
        code_block("diff", &lines)
    )
}

/// The line with the state of each tier of the cluster, and the fold with the tiers that differ.
/// It is empty when the diff does not compare the tiers.
fn tiers_section(d: &Diff) -> String {
    match tier_line(d) {
        TierLine::NotCompared => String::new(),
        TierLine::Unchanged => format!("**Cluster tiers:** {}\n\n", moved(false).as_str()),
        TierLine::Differ(states) => {
            let words: Vec<String> = states
                .iter()
                .map(|(t, same)| format!("{t} {}", moved(!*same).as_str()))
                .collect();
            let lines: String = tier_op_lines(d).iter().map(op_line).collect();
            format!(
                "**Cluster tiers:** {}\n\n<details><summary>cluster tiers that differ</summary>\n\n{}</details>\n\n",
                markdown_cell(&words.join(", ")),
                code_block("diff", &lines)
            )
        }
    }
}

/// The fold with the changes of the values of the cluster. It is empty when none changed.
fn cluster_values_section(d: &Diff) -> String {
    let Some(cv) = d.cluster_variables.as_ref().filter(|c| c.changed) else {
        return String::new();
    };
    let lines: String = cv
        .values
        .iter()
        .map(|c| op_line(&OpLine::cluster_value(c)))
        .collect();
    format!(
        "<details><summary>cluster values: {}</summary>\n\n{}</details>\n\n",
        values_changed(cv.values.len()),
        code_block("diff", &lines)
    )
}

/// The fold with the detail of a changed pack. It is empty when the pack has no detail to show.
fn pack_detail(pd: &PackDiff) -> String {
    let mut body = String::new();
    if let Some(line) = upstream_line(pd, &Paint::off(), code_span) {
        body.push_str(&format!("{line}\n\n"));
    }
    if pd.values.changed && pd.values.parseable && !pd.values.diff.is_empty() {
        let lines: String = pd.values.diff.iter().map(leaf).collect();
        body.push_str(&code_block("diff", &lines));
    }
    if pd.values.changed && !pd.values.parseable {
        body.push_str(&values_unparsed_sentence(code_span));
        body.push('\n');
    }
    let artifacts = artifact_lines(pd);
    if !artifacts.is_empty() {
        let lines: String = artifacts.iter().map(|line| format!("{line}\n")).collect();
        body.push_str(&code_block("diff", &lines));
    }
    for m in &pd.manifests {
        body.push_str(&format!("- {}\n", manifest_change(m, code_span)));
    }
    if body.is_empty() {
        return body;
    }
    let what = if pd.values.changed && pd.values.parseable {
        values_summary(&pd.values)
    } else {
        plain_flags(pd)
    };
    format!(
        "<details><summary><code>{}</code> {}</summary>\n\n{}\n</details>\n\n",
        html_text(&pd.name),
        html_text(&what),
        body
    )
}

impl Renderer for MarkdownRenderer {
    fn render(&self, d: &Diff, a: &Snapshot, b: &Snapshot, full: bool) -> Result<String> {
        let mut o = heading_table(d);
        o.push_str(&result_section(d));
        o.push_str(&pack_table(d));
        o.push_str(&variables_section(d));
        if let Some(line) = artifacts_summary(d) {
            o.push_str(&format!("**Images:** {line}\n\n"));
        }
        o.push_str(&tiers_section(d));
        o.push_str(&cluster_values_section(d));
        for pd in d.packs.iter().filter(|p| p.status == Status::Changed) {
            o.push_str(&pack_detail(pd));
        }
        if let Some(note) = pack_order_note(d) {
            o.push_str(&format!("> note: {note}\n\n"));
        }
        if full {
            o.push_str(&super::unified::full_diffs(a, b, d, &Paint::off(), true));
        }
        Ok(o)
    }

    fn verdict(&self, d: &Diff) -> Option<String> {
        let v = d.verify.as_ref()?;
        let parts = verdict_parts(v, d.same, &d.a, code_span);
        let mut line = format!(
            "\n**{}**: {}",
            parts.word(),
            markdown_cell(&parts.line(&d.a))
        );
        for sentence in invalid_pack_sentences(&v.invalid_packs) {
            line.push_str(&format!("\n\n**INVALID**: {}", markdown_cell(&sentence)));
        }
        if let Some(heading) = who_heading(v) {
            line.push_str("\n\n");
            line.push_str(&markdown_cell(&heading));
            line.push('\n');
            for e in v.who.iter().flatten() {
                line.push_str(&format!("\n- {}", markdown_cell(&who_entry(e, &v.uid))));
            }
        }
        Some(line)
    }

    fn render_verify_summary(&self, s: &VerifySummary) -> Result<String> {
        let mut o = format!("### verify: {}\n\n", counts_line(s));
        o.push_str("| status | record | profile | detail |\n|---|---|---|---|\n");
        for r in &s.records {
            o.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                record_word(r.status),
                code_cell(&r.path),
                code_cell(dash(&r.subject)),
                markdown_cell(&record_detail(r))
            ));
        }
        o.push('\n');
        Ok(o)
    }

    fn render_chain(&self, c: &Chain) -> Result<String> {
        let mut o = format!("### same release: {}\n\n", chain_verdict(c));
        o.push_str("| status | from | to | detail |\n|---|---|---|---|\n");
        for (index, p) in c.pairs.iter().enumerate() {
            let mut detail = markdown_cell(&pair_detail(p));
            for line in pair_projects(c, index).into_iter().chain(renumber_lines(p)) {
                detail.push_str("<br>");
                detail.push_str(&markdown_cell(&line));
            }
            for r in &p.reasons {
                detail.push_str("<br>");
                detail.push_str(&markdown_cell(r));
            }
            for note in pair_notes(p) {
                detail.push_str("<br>");
                detail.push_str(&markdown_cell(&note));
            }
            o.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                pair_word(p.status),
                code_cell(&p.a),
                code_cell(&p.b),
                detail
            ));
        }
        o.push('\n');
        Ok(o)
    }

    fn render_set(&self, r: &crate::ops::SetReport) -> Result<String> {
        let mut o = format!(
            "### snapshot: {}\n\n",
            markdown_cell(&super::set_counts_line(r))
        );
        if !r.records.is_empty() {
            o.push_str("| cluster | uid | file | root |\n|---|---|---|---|\n");
        }
        for rec in &r.records {
            let root = match &rec.root {
                Some(root) => code_cell(short(root)),
                None => format!("error: {}", markdown_cell(super::list::set_error(rec))),
            };
            o.push_str(&format!(
                "| {} | {} | {} | {root} |\n",
                markdown_cell(dash(&rec.cluster.name)),
                code_cell(&rec.cluster.uid),
                code_cell(&rec.path.display().to_string())
            ));
        }
        if !r.removed.is_empty() {
            o.push_str("\nRemoved:\n\n");
            for path in &r.removed {
                o.push_str(&format!("- {}\n", code_span(&path.display().to_string())));
            }
        }
        o.push('\n');
        Ok(o)
    }

    fn render_report(&self, r: &Report) -> Result<String> {
        Ok(markdown_report(r))
    }

    fn render_behind(&self, r: &crate::ops::BehindReport, behind_only: bool) -> Result<String> {
        Ok(behind::markdown_behind(r, behind_only))
    }

    fn render_consumers(&self, r: &ConsumersReport) -> Result<String> {
        let mut o = format!(
            "### consumers: {}\n\n",
            markdown_cell(&consumers_heading(r))
        );
        let second = format!(
            "{}.",
            capitalized(&markdown_cell(&second_reading_line(
                &r.clusters_read,
                code_span
            )))
        );
        let edge = format!("{}.", capitalized(&markdown_cell(&edge_hosts_line(r))));
        if r.consumers.is_empty() {
            o.push_str(&second);
            o.push_str("\n\n");
            o.push_str(&edge);
            o.push('\n');
            return Ok(o);
        }
        let subject = if r.profile.kind == Some(ProfileType::App) {
            ConsumerKind::App.noun()
        } else {
            ConsumerKind::Cluster.noun()
        };
        o.push_str(&format!(
            "| {subject} | uid | project | state | runs |\n|---|---|---|---|---|\n"
        ));
        for c in &r.consumers {
            // a cluster template keeps no copy of the version, so its cell names the kind alone
            let version = if c.kind == ConsumerKind::Template {
                format!("({})", c.kind.noun())
            } else if let Some(runs) = consumer_runs(c, r, code_cell) {
                runs
            } else if c.version.is_empty() {
                String::new()
            } else {
                code_cell(&c.version)
            };
            // an empty version means two things: the cluster named none, or strata could not read
            // the cluster at all. `-` states the first, so the error takes the place of the dash
            let error = entry_error(c.error.as_ref())
                .map(|msg| format!("not read: {}", markdown_cell(msg)));
            let mut runs = match (version.is_empty(), error) {
                (true, None) => "-".to_string(),
                (true, Some(error)) => error,
                (false, None) => version,
                (false, Some(error)) => format!("{version} ({error})"),
            };
            if c.unlisted {
                runs.push_str(&format!(" ({NOT_LISTED})"));
            }
            if matches!(c.kind, ConsumerKind::EdgeHost | ConsumerKind::App) {
                runs.push_str(&format!(" ({})", c.kind.noun()));
            }
            o.push_str(&format!(
                "| {} | {} | {} | {} | {runs} |\n",
                code_cell(dash(&c.name)),
                code_cell(&c.uid),
                code_cell(&project_label(&c.project_uid, &c.project_name)),
                markdown_cell(dash(&c.state))
            ));
        }
        o.push('\n');
        o.push_str(&second);
        o.push_str("\n\n");
        o.push_str(&edge);
        o.push('\n');
        Ok(o)
    }

    fn render_packs(&self, l: &PackListing) -> Result<String> {
        let mut out = String::new();
        if let Some(sentence) = no_version_sentence(l) {
            return Ok(format!("{}.\n", capitalized(&markdown_cell(&sentence))));
        }
        for r in &l.registries {
            out.push_str(&format!(
                "### {}\n\n",
                markdown_cell(&registry_heading(l, r, str::to_string))
            ));
            out.push_str("| version | tags | type | clouds | pack uid |\n");
            out.push_str("| --- | --- | --- | --- | --- |\n");
            for v in &r.versions {
                out.push_str(&format!(
                    "| {}{} | {} | {} | {} | {} |\n",
                    code_cell(&v.version),
                    version_note(v)
                        .map(|n| format!(" {}", markdown_cell(&n)))
                        .unwrap_or_default(),
                    if v.tags.is_empty() {
                        "-".to_string()
                    } else {
                        v.tags
                            .iter()
                            .map(|t| code_cell(t))
                            .collect::<Vec<_>>()
                            .join(" ")
                    },
                    v.kind.as_str(),
                    if v.cloud_types.is_empty() {
                        "-".to_string()
                    } else {
                        markdown_cell(&v.cloud_types.join(", "))
                    },
                    code_cell(&v.pack_uid)
                ));
            }
            out.push('\n');
        }
        Ok(out)
    }

    fn render_list(&self, l: &Listing) -> Result<String> {
        let mut o = format!("### list: {}\n\n", markdown_cell(&list_scope(l)));
        if let Some(profiles) = &l.profiles {
            list_profiles(&mut o, l, profiles);
        }
        if let Some(clusters) = &l.clusters {
            list_clusters(&mut o, l, clusters);
        }
        if let Some(projects) = &l.projects {
            list_projects(&mut o, projects);
        }
        if let Some(filters) = &l.filters {
            list_filters(&mut o, filters);
        }
        Ok(o)
    }
}

/// The table of the profile versions of a listing, with its count.
fn list_profiles(o: &mut String, l: &Listing, profiles: &[ListedProfile]) {
    let count = list_count(profiles.len(), "profile", l.pack.as_deref());
    o.push_str(&format!("**{}**\n\n", markdown_cell(&count)));
    let with_packs = profiles
        .iter()
        .any(|p| p.packs.is_some() || p.error.is_some());
    if with_packs && !profiles.is_empty() {
        o.push_str("| ref | uid | cloud | type | consumers | packs |\n|---|---|---|---|---|---|\n");
    } else if !profiles.is_empty() {
        o.push_str("| ref | uid | cloud |\n|---|---|---|\n");
    }
    for pr in profiles {
        o.push_str(&profile_row(l, pr, with_packs));
    }
    o.push('\n');
}

/// The row of a profile version. `with_packs` adds the columns of its type, consumers, and packs.
fn profile_row(l: &Listing, pr: &ListedProfile, with_packs: bool) -> String {
    let mark = profile_marks(pr, l)
        .map(|m| format!(" {m}"))
        .unwrap_or_default();
    let mut row = format!(
        "| {}{mark} | {} | {} |",
        code_cell(&pr.reference),
        code_cell(&pr.uid),
        markdown_cell(dash(&pr.cloud_type))
    );
    if with_packs {
        // each line gets its escape before the join, so that the `<br>` of strata stays a tag
        let packs = match entry_error(pr.error.as_ref()) {
            Some(msg) => markdown_cell(&format!("error: {msg}")),
            None => pr
                .packs
                .iter()
                .flatten()
                .map(|pk| {
                    let registry = pack_registry(pk)
                        .map(|r| format!(" in {r}"))
                        .unwrap_or_default();
                    markdown_cell(&format!(
                        "{}/{} {}{registry}",
                        pk.layer,
                        pk.name,
                        pack_version(pk)
                    ))
                })
                .collect::<Vec<_>>()
                .join("<br>"),
        };
        row.push_str(&format!(
            " {} | {} | {} |",
            markdown_cell(super::list::profile_type_cell(pr)),
            pr.consumers
                .map(consumer_count)
                .unwrap_or_else(|| "-".into()),
            dash(&packs)
        ));
    }
    row.push('\n');
    row
}

/// The columns of the table of clusters that a listing shows only when a cluster needs them.
struct ClusterColumns {
    project: bool,
    labels: bool,
    virtual_group: bool,
    applied: bool,
}

impl ClusterColumns {
    fn of(l: &Listing, clusters: &[ListedCluster]) -> Self {
        Self {
            project: clusters.iter().any(|c| c.project_uid != l.project_uid),
            // a query is a question about labels, so its rows show them
            labels: l.query.is_some(),
            virtual_group: clusters.iter().any(|c| c.virtual_cluster.is_some()),
            applied: clusters
                .iter()
                .any(|c| c.profiles.is_some() || c.error.is_some()),
        }
    }

    /// The heading of the table, and the rule under it.
    fn head(&self) -> String {
        let optional: String = [
            (self.project, " project |"),
            (self.labels, " labels |"),
            (self.virtual_group, " virtual |"),
            (self.applied, " profiles | behind |"),
        ]
        .into_iter()
        .filter(|(shown, _)| *shown)
        .map(|(_, text)| text)
        .collect();
        let head = format!("| ref | name | state | cloud |{optional}");
        let rule: String = head.matches('|').skip(1).map(|_| "---|").collect();
        format!("{head}\n|{rule}\n")
    }
}

/// The table of the clusters of a listing, with its count and the lines of its query.
fn list_clusters(o: &mut String, l: &Listing, clusters: &[ListedCluster]) {
    o.push_str(&format!(
        "**{}**\n\n",
        markdown_cell(&cluster_count(clusters.len(), l.cluster.as_deref()))
    ));
    for line in super::list::query_line(l)
        .into_iter()
        .chain(virtual_line(l))
    {
        o.push_str(&format!(
            "{}.\n\n",
            markdown_cell(&super::capitalized(&line))
        ));
    }
    let columns = ClusterColumns::of(l, clusters);
    if !clusters.is_empty() {
        o.push_str(&columns.head());
    }
    for c in clusters {
        o.push_str(&cluster_row(c, &columns));
    }
    o.push('\n');
}

/// The row of a cluster, with the columns that `columns` shows.
fn cluster_row(c: &ListedCluster, columns: &ClusterColumns) -> String {
    let mut row = format!(
        "| {} | {} | {} | {} |",
        code_cell(&c.reference),
        markdown_cell(dash(&c.name)),
        markdown_cell(dash(&c.state)),
        markdown_cell(dash(&c.cloud_type))
    );
    if columns.project {
        row.push_str(&format!(
            " {} |",
            markdown_cell(&project_label(&c.project_uid, &c.project_name))
        ));
    }
    if columns.labels {
        row.push_str(&format!(
            " {} |",
            markdown_cell(dash(&super::list::labels_text(&c.labels)))
        ));
    }
    if columns.virtual_group {
        // a virtual cluster whose group Palette does not name is still virtual
        let cell = c
            .virtual_cluster
            .as_ref()
            .map(|v| virtual_group(v).unwrap_or_else(|| "yes".to_string()));
        row.push_str(&format!(
            " {} |",
            markdown_cell(dash(cell.as_deref().unwrap_or("")))
        ));
    }
    if columns.applied {
        row.push_str(&applied_cells(c));
    }
    row.push('\n');
    row
}

/// The cells of the profiles that a cluster runs, and of the updates that wait for it.
fn applied_cells(c: &ListedCluster) -> String {
    // each line gets its escape before the join, so that the `<br>` of strata stays a tag
    let profiles = if let Some(msg) = entry_error(c.error.as_ref()) {
        markdown_cell(&format!("error: {msg}"))
    } else if let Some(note) = super::list::imported_note(c) {
        markdown_cell(note)
    } else {
        c.profiles
            .iter()
            .flatten()
            .map(|a| markdown_cell(&format!("{} ({})", applied_ref(a), dash(&a.kind))))
            .collect::<Vec<_>>()
            .join("<br>")
    };
    let behind = match c.updates_pending {
        Some(true) => format!("yes: {}", pending_profile_names(&c.pending_updates)),
        Some(false) => "no".to_string(),
        None => "-".to_string(),
    };
    format!(" {} | {} |", dash(&profiles), markdown_cell(&behind))
}

/// The table of the projects of a listing, with its count.
fn list_projects(o: &mut String, projects: &[ListedProject]) {
    o.push_str(&format!("**{}**\n\n", count_of(projects.len(), "project")));
    if !projects.is_empty() {
        o.push_str("| name | uid |\n|---|---|\n");
    }
    for pr in projects {
        o.push_str(&format!(
            "| {} | {} |\n",
            markdown_cell(dash(&pr.name)),
            code_cell(&pr.uid)
        ));
    }
    o.push('\n');
}

/// The table of the saved tag filters of a listing, with its count.
fn list_filters(o: &mut String, filters: &[SavedFilter]) {
    o.push_str(&format!(
        "**{}**\n\n",
        count_of(filters.len(), "saved tag filter")
    ));
    if !filters.is_empty() {
        o.push_str("| ref | uid | selects |\n|---|---|---|\n");
    }
    for f in filters {
        o.push_str(&format!(
            "| {} | {} | {} |\n",
            code_cell(&f.reference()),
            code_cell(&f.uid),
            markdown_cell(&f.expression())
        ));
    }
    o.push('\n');
}
