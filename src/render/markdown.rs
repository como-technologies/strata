//! The pull request comment.

use super::{
    chain_verdict, cluster_value, consumer_count, consumers_heading, counts_line, dash,
    edited_in_place_note, entry_error, list_count, list_scope, overwritten_in_place_notes,
    pack_version, pair_detail, pair_word, plain_flags, profiles_overwritten_sentence,
    record_detail, record_label, same_content_note, short, show, subject, traits_suffix,
    values_changed, values_changed_suffix, variable_lines, version_display, Renderer, VarLine,
};
use crate::color::Paint;
use crate::diff::{Diff, Leaf, Op, Status, Verify, VerifyStatus};
use crate::error::Result;
use crate::ops::{Chain, ConsumersReport, Listing, PackListing, RecordStatus, VerifySummary};
use crate::snapshot::{Snapshot, Source};

/// Renders markdown for a pull request comment.
pub struct MarkdownRenderer;

fn escape(s: &str) -> String {
    s.replace('|', "\\|")
}

fn leaf(l: &Leaf) -> String {
    leaf_at(&l.path, l)
}

fn leaf_at(path: &str, l: &Leaf) -> String {
    let old = l.old.as_ref().map(show).unwrap_or_else(|| "null".into());
    let new = l.new.as_ref().map(show).unwrap_or_else(|| "null".into());
    match l.op {
        Op::Changed => format!("- {path}: {old}\n+ {path}: {new}\n"),
        Op::Added => format!("+ {path}: {new}\n"),
        Op::Removed => format!("- {path}: {old}\n"),
    }
}

fn variable_line(v: &VarLine<'_>) -> String {
    match v {
        VarLine::Added(s) => format!("+ variable {}{}\n", s.name, traits_suffix(s)),
        VarLine::Removed(s) => format!("- variable {}{}\n", s.name, traits_suffix(s)),
        VarLine::Field(name, l) => leaf_at(&format!("{name}.{}", l.path), l),
    }
}

impl Renderer for MarkdownRenderer {
    fn render(&self, d: &Diff, a: &Snapshot, b: &Snapshot, full: bool) -> Result<String> {
        let mut o = String::new();
        o.push_str(&format!("### {} → {}\n\n", d.a.title(), d.b.title()));
        o.push_str("| | A | B |\n|---|---|---|\n");
        o.push_str(&format!(
            "| uid | `{}` | `{}` |\n",
            dash(d.a.shown_uid()),
            dash(d.b.shown_uid())
        ));
        o.push_str(&format!(
            "| root | `{}` | `{}` |\n",
            short(&d.root_a),
            short(&d.root_b)
        ));
        o.push_str(&format!(
            "| consumers | {} clusters | {} clusters |\n",
            d.a.consumers.count, d.b.consumers.count
        ));
        o.push_str(&format!(
            "| fetched | {} | {} |\n\n",
            dash(&d.a.fetched_at),
            dash(&d.b.fetched_at)
        ));
        let s = &d.summary;
        let max_impact = serde_json::to_value(s.max_impact)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        o.push_str(&format!(
            "**Result:** {} · max impact: `{}` · packs: {} changed, {} added, {} removed, {} unchanged · variables: {}\n\n",
            if d.same {
                "IDENTICAL"
            } else if d.same_content {
                "SAME CONTENT"
            } else {
                "CHANGED"
            },
            max_impact,
            s.packs_changed,
            s.packs_added,
            s.packs_removed,
            s.packs_unchanged,
            if s.variables_changed { "changed" } else { "unchanged" }
        ));
        if let Some(note) = edited_in_place_note(d) {
            o.push_str(&format!("**Note:** {note}\n\n"));
        }
        for note in overwritten_in_place_notes(d) {
            o.push_str(&format!("**Note:** {note}\n\n"));
        }
        if let Some(note) = same_content_note(d) {
            o.push_str(&format!("**Note:** {note}\n\n"));
        }
        if let Some(v) = &d.verify {
            o.push_str(&format!(
                "**Verify:** `{:?}` for uid `{}`, recorded {}, checked {}\n\n",
                v.status, v.uid, v.recorded_at, v.checked_at
            ));
        }
        o.push_str("| | layer | pack | version | change | impact |\n|---|---|---|---|---|---|\n");
        for pd in &d.packs {
            let (mark, change) = match pd.status {
                Status::Unchanged => (" ", "unchanged".to_string()),
                Status::Added => ("+", "ADDED".to_string()),
                Status::Removed => ("-", "REMOVED".to_string()),
                Status::Changed => ("~", plain_flags(pd).replace("->", "→")),
            };
            o.push_str(&format!(
                "| `{mark}` | {} | `{}` | {} | {} | {} |\n",
                pd.layer,
                escape(&pd.name),
                escape(&version_display(pd).replace("->", "→")),
                change,
                escape(&pd.impact.text)
            ));
        }
        o.push('\n');
        if d.variables.changed {
            let g = &d.variables.by_name;
            o.push_str(&format!(
                "<details><summary>variables: {} added, {} removed, {} changed</summary>\n\n```diff\n",
                g.added.len(),
                g.removed.len(),
                g.changed.len()
            ));
            for v in variable_lines(g) {
                o.push_str(&variable_line(&v));
            }
            o.push_str("```\n</details>\n\n");
        }
        if let Some(cv) = d.cluster_variables.as_ref().filter(|c| c.changed) {
            o.push_str(&format!(
                "<details><summary>cluster values: {}</summary>\n\n```diff\n",
                values_changed(cv.values.len())
            ));
            for c in &cv.values {
                let path = format!("{}.{}", c.profile, c.name);
                let (old, new) = (
                    cluster_value(&c.old, c.sensitive),
                    cluster_value(&c.new, c.sensitive),
                );
                o.push_str(&match c.op {
                    Op::Changed => format!("- {path}: {old}\n+ {path}: {new}\n"),
                    Op::Added => format!("+ {path}: {new}\n"),
                    Op::Removed => format!("- {path}: {old}\n"),
                });
            }
            o.push_str("```\n</details>\n\n");
        }
        for pd in d.packs.iter().filter(|p| p.status == Status::Changed) {
            let mut body = String::new();
            if pd.upstream {
                if let (Some(x), Some(y)) = (&pd.a, &pd.b) {
                    body.push_str(&format!(
                        "upstream digest: `{}` → `{}` ({})\n\n",
                        dash(&x.upstream),
                        dash(&y.upstream),
                        y.upstream_source
                    ));
                }
            }
            if pd.values.changed && pd.values.parseable && !pd.values.diff.is_empty() {
                body.push_str("```diff\n");
                for l in &pd.values.diff {
                    body.push_str(&leaf(l));
                }
                body.push_str("```\n");
            }
            if pd.values.changed && !pd.values.parseable {
                body.push_str("values changed but did not parse as YAML on one side; see `--full` for the text diff\n");
            }
            for m in &pd.manifests {
                let word = match m.op {
                    Op::Changed => "changed",
                    Op::Added => "added",
                    Op::Removed => "removed",
                };
                body.push_str(&format!("- manifest `{}` {word}\n", m.name));
            }
            if body.is_empty() {
                continue;
            }
            let what = if pd.values.changed && pd.values.parseable {
                format!(
                    "values: {} changed, {} added, {} removed",
                    pd.values.changed_count, pd.values.added_count, pd.values.removed_count
                )
            } else {
                plain_flags(pd)
            };
            o.push_str(&format!(
                "<details><summary><code>{}</code> {}</summary>\n\n{}\n</details>\n\n",
                escape(&pd.name),
                what,
                body
            ));
        }
        if d.order_changed {
            o.push_str("> note: pack order changed between the two profiles\n\n");
        }
        if full {
            o.push_str(&super::unified::full_diffs(a, b, d, &Paint::off(), true));
        }
        Ok(o)
    }

    fn verdict(&self, v: &Verify, recorded: &Source) -> Option<String> {
        let (what, at) = (subject(recorded), &recorded.fetched_at);
        Some(match v.status {
            VerifyStatus::Unchanged => format!(
                "\n**OK**: {what} is unchanged since {at}{}",
                values_changed_suffix(v.cluster_values_changed)
            ),
            VerifyStatus::Drifted => match &v.overwritten_by {
                _ if !v.profiles_overwritten.is_empty() => format!(
                    "\n**DRIFT**: {}{}",
                    profiles_overwritten_sentence(v, recorded).unwrap_or_default(),
                    values_changed_suffix(v.cluster_values_changed)
                ),
                Some(now) => format!(
                    "\n**DRIFT**: {what} now names {now}: the version recorded {at} was overwritten in place{}",
                    values_changed_suffix(v.cluster_values_changed)
                ),
                None => format!(
                    "\n**DRIFT**: {what} differs from the copy recorded {at}{}",
                    values_changed_suffix(v.cluster_values_changed)
                ),
            },
        })
    }

    fn render_verify_summary(&self, s: &VerifySummary) -> Result<String> {
        let mut o = format!("### verify: {}\n\n", counts_line(s));
        o.push_str("| status | record | profile | detail |\n|---|---|---|---|\n");
        for r in &s.records {
            let word = match r.status {
                RecordStatus::Unchanged => "OK",
                RecordStatus::Drifted => "DRIFT",
                RecordStatus::Error => "ERROR",
            };
            o.push_str(&format!(
                "| {word} | `{}` | `{}` | {} |\n",
                escape(&r.path),
                record_label(&r.name, &r.version),
                escape(&record_detail(r))
            ));
        }
        o.push('\n');
        Ok(o)
    }

    fn render_chain(&self, c: &Chain) -> Result<String> {
        let mut o = format!("### same release: {}\n\n", chain_verdict(c));
        o.push_str("| status | from | to | detail |\n|---|---|---|---|\n");
        for p in &c.pairs {
            let mut detail = escape(&pair_detail(p));
            for r in &p.reasons {
                detail.push_str("<br>");
                detail.push_str(&escape(r));
            }
            o.push_str(&format!(
                "| {} | `{}` | `{}` | {} |\n",
                pair_word(p.status),
                escape(&p.a),
                escape(&p.b),
                detail
            ));
        }
        o.push('\n');
        Ok(o)
    }

    fn render_consumers(&self, r: &ConsumersReport) -> Result<String> {
        let mut o = format!("### consumers: {}\n\n", consumers_heading(r));
        if r.consumers.is_empty() {
            return Ok(o);
        }
        o.push_str("| cluster | uid | project | state | runs |\n|---|---|---|---|---|\n");
        for c in &r.consumers {
            let runs = if c.version.is_empty() {
                "-".to_string()
            } else if c.version != r.profile.version {
                format!("`{}`, which the uid no longer names", escape(&c.version))
            } else {
                format!("`{}`", escape(&c.version))
            };
            o.push_str(&format!(
                "| `{}` | `{}` | `{}` | {} | {runs} |\n",
                escape(dash(&c.name)),
                escape(&c.uid),
                escape(dash(&c.project_uid)),
                escape(&c.state)
            ));
        }
        o.push('\n');
        Ok(o)
    }

    fn render_packs(&self, l: &PackListing) -> Result<String> {
        let mut out = String::new();
        for r in &l.registries {
            out.push_str(&format!("### {} in {}\n\n", l.name, r.registry));
            out.push_str("| version | tags | type | clouds | pack uid |\n");
            out.push_str("| --- | --- | --- | --- | --- |\n");
            for v in &r.versions {
                out.push_str(&format!(
                    "| `{}` | {} | {} | {} | `{}` |\n",
                    v.version,
                    if v.tags.is_empty() {
                        "-".to_string()
                    } else {
                        v.tags
                            .iter()
                            .map(|t| format!("`{t}`"))
                            .collect::<Vec<_>>()
                            .join(" ")
                    },
                    v.kind,
                    if v.cloud_types.is_empty() {
                        "-".to_string()
                    } else {
                        v.cloud_types.join(", ")
                    },
                    v.pack_uid
                ));
            }
            out.push('\n');
        }
        Ok(out)
    }

    fn render_list(&self, l: &Listing) -> Result<String> {
        let mut o = format!("### list: {}\n\n", escape(&list_scope(l)));
        if let Some(profiles) = &l.profiles {
            let count = list_count(profiles.len(), "profile", l.pack.as_deref());
            o.push_str(&format!("**{}**\n\n", escape(&count)));
            let with_packs = profiles
                .iter()
                .any(|p| p.packs.is_some() || p.error.is_some());
            if with_packs && !profiles.is_empty() {
                o.push_str(
                    "| ref | uid | cloud | type | consumers | packs |\n|---|---|---|---|---|---|\n",
                );
            } else if !profiles.is_empty() {
                o.push_str("| ref | uid | cloud |\n|---|---|---|\n");
            }
            for pr in profiles {
                o.push_str(&format!(
                    "| `{}` | `{}` | {} |",
                    escape(&pr.reference),
                    escape(&pr.uid),
                    escape(dash(&pr.cloud_type))
                ));
                if with_packs {
                    let packs = match entry_error(pr.error.as_ref()) {
                        Some(msg) => format!("error: {msg}"),
                        None => pr
                            .packs
                            .iter()
                            .flatten()
                            .map(|pk| format!("{}/{} {}", pk.layer, pk.name, pack_version(pk)))
                            .collect::<Vec<_>>()
                            .join("<br>"),
                    };
                    o.push_str(&format!(
                        " {} | {} | {} |",
                        escape(dash(pr.profile_type.as_deref().unwrap_or(""))),
                        pr.consumers
                            .map(consumer_count)
                            .unwrap_or_else(|| "-".into()),
                        escape(dash(&packs))
                    ));
                }
                o.push('\n');
            }
            o.push('\n');
        }
        if let Some(clusters) = &l.clusters {
            o.push_str(&format!(
                "**{}**\n\n",
                list_count(clusters.len(), "cluster", None)
            ));
            let elsewhere = clusters.iter().any(|c| c.project_uid != l.project_uid);
            if elsewhere {
                o.push_str("| ref | name | state | cloud | project |\n|---|---|---|---|---|\n");
            } else if !clusters.is_empty() {
                o.push_str("| ref | name | state | cloud |\n|---|---|---|---|\n");
            }
            for c in clusters {
                o.push_str(&format!(
                    "| `{}` | {} | {} | {} |",
                    escape(&c.reference),
                    escape(dash(&c.name)),
                    escape(dash(&c.state)),
                    escape(dash(&c.cloud_type))
                ));
                if elsewhere {
                    o.push_str(&format!(" `{}` |", escape(dash(&c.project_uid))));
                }
                o.push('\n');
            }
            o.push('\n');
        }
        if let Some(projects) = &l.projects {
            o.push_str(&format!(
                "**{}**\n\n",
                list_count(projects.len(), "project", None)
            ));
            if !projects.is_empty() {
                o.push_str("| name | uid |\n|---|---|\n");
            }
            for pr in projects {
                o.push_str(&format!(
                    "| {} | `{}` |\n",
                    escape(dash(&pr.name)),
                    escape(&pr.uid)
                ));
            }
            o.push('\n');
        }
        Ok(o)
    }
}
