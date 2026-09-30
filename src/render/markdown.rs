//! The pull request comment.

use super::{
    Renderer, VarLine, applied_ref, artifact_lines, artifacts_summary, capitalized, chain_verdict,
    cluster_count, cluster_value, consumer_count, consumer_counts, consumers_heading,
    consumers_sentence, count_of, counts_line, cross_kind_note, dash, edge_hosts_line,
    edited_in_place_note, entry_error, gone_sentence, list_count, list_scope, macros_changed_note,
    macros_changed_sentence, macros_note, overwritten_in_place_notes, pack_version, pair_detail,
    pair_word, palette_repave_lines, palette_repave_note, plain_flags,
    profiles_overwritten_sentence, project_label, record_detail, record_label, registry_note,
    registry_sync_suffix, running_behind_notes, same_content_note, short, show, state_suffix,
    subject, tier_path, tier_states, traits_suffix, unverified_sentence, updates_pending_notes,
    upstream_unknown_note, values_changed, values_changed_suffix, variable_lines,
    variables_comparable, variables_unavailable_note, version_display, who_entry, who_heading,
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
        let s = &d.summary;
        let max_impact = serde_json::to_value(s.max_impact)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        // two packs have no profile variables, and no request asked for any
        let variables = if variables_comparable(d) {
            format!(
                " · variables: {}",
                if s.variables_changed {
                    "changed"
                } else {
                    "unchanged"
                }
            )
        } else {
            String::new()
        };
        o.push_str(&format!(
            "**Result:** {} · max impact: `{}` · packs: {} changed, {} added, {} removed, {} unchanged{}\n\n",
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
            variables
        ));
        if let Some(sentence) = consumers_sentence(d) {
            o.push_str(&format!("**Consumers:** {sentence}\n\n"));
        }
        if let Some(note) = edited_in_place_note(d) {
            o.push_str(&format!("**Note:** {note}\n\n"));
        }
        for note in overwritten_in_place_notes(d) {
            o.push_str(&format!("**Note:** {note}\n\n"));
        }
        for note in updates_pending_notes(d) {
            o.push_str(&format!("**Note:** {note}\n\n"));
        }
        for note in running_behind_notes(d) {
            o.push_str(&format!("**Note:** {note}\n\n"));
        }
        if let Some(note) = upstream_unknown_note(d) {
            o.push_str(&format!("**Note:** {note}\n\n"));
        }
        if let Some(note) = macros_note(d) {
            o.push_str(&format!("**Note:** {note}\n\n"));
        }
        if let Some(note) = macros_changed_note(d) {
            o.push_str(&format!("**Note:** {note}\n\n"));
        }
        let palette = palette_repave_lines(d);
        if !palette.is_empty() {
            o.push_str(&format!("**Palette:** {}\n\n", escape(&palette.join("; "))));
        }
        if let Some(note) = palette_repave_note(d) {
            o.push_str(&format!("**Note:** {note}\n\n"));
        }
        if let Some(note) = same_content_note(d) {
            o.push_str(&format!("**Note:** {note}\n\n"));
        }
        if let Some(note) = variables_unavailable_note(d) {
            o.push_str(&format!("**Note:** {note}\n\n"));
        }
        if let Some(note) = cross_kind_note(d) {
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
        if let Some(line) = artifacts_summary(d) {
            o.push_str(&format!("**Images:** {line}\n\n"));
        }
        let states = tier_states(d);
        if !states.is_empty() && states.iter().all(|(_, same)| *same) {
            o.push_str("**Cluster tiers:** unchanged\n\n");
        } else if !states.is_empty() {
            let words: Vec<String> = states
                .iter()
                .map(|(t, same)| format!("{t} {}", if *same { "unchanged" } else { "changed" }))
                .collect();
            o.push_str(&format!("**Cluster tiers:** {}\n\n", words.join(", ")));
            let changed: Vec<_> = d.cluster_tiers.iter().filter(|t| !t.same).collect();
            if !changed.is_empty() {
                o.push_str("<details><summary>cluster tiers that differ</summary>\n\n```diff\n");
                for t in changed {
                    for l in &t.diff {
                        o.push_str(&leaf_at(&tier_path(&t.tier, &l.path), l));
                    }
                }
                o.push_str("```\n</details>\n\n");
            }
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
            if pd.upstream
                && let (Some(x), Some(y)) = (&pd.a, &pd.b)
            {
                body.push_str(&format!(
                    "upstream digest: `{}` → `{}` ({})\n\n",
                    dash(&x.upstream),
                    dash(&y.upstream),
                    y.upstream_source
                ));
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
            let artifacts = artifact_lines(pd);
            if !artifacts.is_empty() {
                body.push_str("```diff\n");
                for line in &artifacts {
                    body.push_str(&format!("{line}\n"));
                }
                body.push_str("```\n");
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
            VerifyStatus::Unverified => format!(
                "\n**UNVERIFIED**: {what} has the content recorded {at}, but {}{}",
                unverified_sentence(&v.upstream_unknown),
                values_changed_suffix(v.cluster_values_changed)
            ),
            VerifyStatus::Drifted => match &v.overwritten_by {
                _ if !v.profiles_overwritten.is_empty() => format!(
                    "\n**DRIFT**: {}{}",
                    profiles_overwritten_sentence(v, recorded).unwrap_or_default(),
                    values_changed_suffix(v.cluster_values_changed)
                ),
                _ if v.gone => format!(
                    "\n**DRIFT**: {}",
                    gone_sentence(v, recorded, |uid| format!("`{uid}`"))
                ),
                _ if v.renamed_to.is_some() => format!(
                    "\n**DRIFT**: {what} was renamed to {}",
                    v.renamed_to.as_deref().unwrap_or_default()
                ),
                Some(now) => format!(
                    "\n**DRIFT**: {what} now names {now}: the version recorded {at} was overwritten in place{}",
                    values_changed_suffix(v.cluster_values_changed)
                ),
                None if !v.macros_changed.is_empty() => format!(
                    "\n**DRIFT**: {what} has the content recorded {at}, but {}{}",
                    macros_changed_sentence(&v.macros_changed),
                    values_changed_suffix(v.cluster_values_changed)
                ),
                None => format!(
                    "\n**DRIFT**: {what} differs from the copy recorded {at}{}",
                    values_changed_suffix(v.cluster_values_changed)
                ),
            },
        })
        .map(|mut line| {
            line.push_str(&registry_sync_suffix(v.registry_sync.as_ref()));
            line.push_str(&state_suffix(v, recorded));
            if let Some(heading) = who_heading(v) {
                line.push_str("\n\n");
                line.push_str(&heading);
                line.push('\n');
                for e in v.who.iter().flatten() {
                    line.push_str(&format!("\n- {}", escape(&who_entry(e))));
                }
            }
            line
        })
    }

    fn render_verify_summary(&self, s: &VerifySummary) -> Result<String> {
        let mut o = format!("### verify: {}\n\n", counts_line(s));
        o.push_str("| status | record | profile | detail |\n|---|---|---|---|\n");
        for r in &s.records {
            let word = match r.status {
                RecordStatus::Unchanged => "OK",
                RecordStatus::Drifted => "DRIFT",
                RecordStatus::Unverified => "UNVERIFIED",
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

    fn render_behind(&self, r: &crate::ops::BehindReport) -> Result<String> {
        let mut o = format!("### behind: {}\n\n", escape(&super::behind_heading(r)));
        if r.pins.is_empty() {
            return Ok(o);
        }
        o.push_str(
            "| pack | pinned | registry | status | bump | pinned by |\n|---|---|---|---|---|---|\n",
        );
        for pin in &r.pins {
            let mut status = super::pin_status(pin);
            if let Some(arrived) = pin.arrived.as_ref().filter(|a| !a.is_empty()) {
                status.push_str(&format!("; arrived: {}", arrived.join(", ")));
            }
            if let Some(msg) = entry_error(pin.error.as_ref()) {
                status.push_str(&format!(" ({msg})"));
            }
            let by: Vec<String> = pin
                .pinned_by
                .iter()
                .map(|b| format!("`{}` ({})", escape(&b.path), escape(&b.subject)))
                .collect();
            o.push_str(&format!(
                "| `{}` | `{}` | {} | {} | {} | {} |\n",
                escape(&pin.name),
                escape(&pin.version),
                escape(dash(&pin.registry)),
                escape(&status),
                super::bump_line(pin)
                    .map(|l| escape(&l))
                    .unwrap_or_else(|| "-".to_string()),
                by.join("<br>")
            ));
        }
        Ok(o)
    }

    fn render_consumers(&self, r: &ConsumersReport) -> Result<String> {
        let mut o = format!("### consumers: {}\n\n", consumers_heading(r));
        // The consumer list of a profile is empty on an error too, so the second reading says
        // how far it got.
        let read = &r.clusters_read;
        let second = match entry_error(read.error.as_ref()) {
            _ if read.skipped && !read.reason.is_empty() => {
                format!("Second reading: not done ({}).", escape(&read.reason))
            }
            _ if read.skipped => "Second reading: not done (`--list-only`).".to_string(),
            Some(msg) => format!("Second reading: not done ({}).", escape(msg)),
            None => format!(
                "Second reading: {} of {} clusters read.",
                read.read,
                read.read + read.unread
            ),
        };
        let edge = format!("{}.", capitalized(&edge_hosts_line(r)));
        if r.consumers.is_empty() {
            o.push_str(&second);
            o.push_str("\n\n");
            o.push_str(&edge);
            o.push('\n');
            return Ok(o);
        }
        let subject = if r.profile.kind == crate::snapshot::APP_KIND {
            "app deployment"
        } else {
            "cluster"
        };
        o.push_str(&format!(
            "| {subject} | uid | project | state | runs |\n|---|---|---|---|---|\n"
        ));
        for c in &r.consumers {
            // an empty version means two things: the cluster named none, or strata could not read
            // the cluster at all. `-` states the first for both, so the error decides the cell.
            // a cluster template keeps no copy of the version, so its cell names the kind alone
            let mut runs = if c.kind == crate::ops::TEMPLATE_KIND {
                "(cluster template)".to_string()
            } else if let Some(msg) = entry_error(c.error.as_ref()) {
                format!("not read: {}", escape(msg))
            } else if c.version.is_empty() {
                "-".to_string()
            } else if c.version != r.profile.version {
                format!("`{}`, which the uid no longer names", escape(&c.version))
            } else {
                format!("`{}`", escape(&c.version))
            };
            if c.unlisted {
                runs.push_str(" (not in the consumer list of the profile)");
            }
            if c.kind == crate::ops::EDGE_HOST_KIND {
                runs.push_str(" (edge host)");
            } else if c.kind == crate::ops::APP_DEPLOYMENT_KIND {
                runs.push_str(" (app deployment)");
            }
            o.push_str(&format!(
                "| `{}` | `{}` | `{}` | {} | {runs} |\n",
                escape(dash(&c.name)),
                escape(&c.uid),
                escape(&project_label(&c.project_uid, &c.project_name)),
                escape(dash(&c.state))
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
        for r in &l.registries {
            out.push_str(&format!(
                "### {} in {}{}\n\n",
                l.name,
                r.registry,
                registry_note(r)
                    .map(|n| format!(" ({n})"))
                    .unwrap_or_default()
            ));
            out.push_str("| version | tags | type | clouds | pack uid |\n");
            out.push_str("| --- | --- | --- | --- | --- |\n");
            for v in &r.versions {
                out.push_str(&format!(
                    "| `{}`{} | {} | {} | {} | `{}` |\n",
                    v.version,
                    v.state
                        .as_ref()
                        .map(|s| format!(" {s}"))
                        .unwrap_or_default(),
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
                escape(&cluster_count(clusters.len(), l.cluster.as_deref()))
            ));
            if let Some(line) = super::query_line(l) {
                o.push_str(&format!("{}.\n\n", escape(&super::capitalized(&line))));
            }
            let labels = if l.query.is_some() { " labels |" } else { "" };
            let elsewhere = clusters.iter().any(|c| c.project_uid != l.project_uid);
            let with_applied = clusters
                .iter()
                .any(|c| c.profiles.is_some() || c.error.is_some());
            let project = if elsewhere { " project |" } else { "" };
            let applied = if with_applied {
                " profiles | behind |"
            } else {
                ""
            };
            if !clusters.is_empty() {
                let head = format!("| ref | name | state | cloud |{project}{labels}{applied}");
                let rule: String = head.matches('|').skip(1).map(|_| "---|").collect();
                o.push_str(&format!("{head}\n|{rule}\n"));
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
                    o.push_str(&format!(
                        " {} |",
                        escape(&project_label(&c.project_uid, &c.project_name))
                    ));
                }
                if l.query.is_some() {
                    o.push_str(&format!(
                        " {} |",
                        escape(dash(&super::labels_text(&c.labels)))
                    ));
                }
                if with_applied {
                    let profiles = match entry_error(c.error.as_ref()) {
                        Some(msg) => format!("error: {msg}"),
                        None => c
                            .profiles
                            .iter()
                            .flatten()
                            .map(|a| format!("{} ({})", applied_ref(a), dash(&a.kind)))
                            .collect::<Vec<_>>()
                            .join("<br>"),
                    };
                    o.push_str(&format!(" {} |", escape(dash(&profiles))));
                    let behind = match c.updates_pending {
                        Some(true) => {
                            let mut names: Vec<&str> = c
                                .pending_updates
                                .iter()
                                .map(|u| u.profile.as_str())
                                .collect();
                            names.dedup();
                            format!("yes: {}", names.join(", "))
                        }
                        Some(false) => "no".to_string(),
                        None => "-".to_string(),
                    };
                    o.push_str(&format!(" {} |", escape(&behind)));
                }
                o.push('\n');
            }
            o.push('\n');
        }
        if let Some(projects) = &l.projects {
            o.push_str(&format!("**{}**\n\n", count_of(projects.len(), "project")));
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
        if let Some(filters) = &l.filters {
            o.push_str(&format!(
                "**{}**\n\n",
                count_of(filters.len(), "saved tag filter")
            ));
            if !filters.is_empty() {
                o.push_str("| ref | uid | selects |\n|---|---|---|\n");
            }
            for f in filters {
                o.push_str(&format!(
                    "| `filter:{}` | `{}` | {} |\n",
                    escape(&f.name),
                    escape(&f.uid),
                    escape(&f.expression())
                ));
            }
            o.push('\n');
        }
        Ok(o)
    }
}
