//! The pull request comment.

use super::{
    counts_line, dash, plain_flags, record_detail, record_label, short, show, traits_suffix,
    variable_lines, version_display, Renderer, VarLine,
};
use crate::color::Paint;
use crate::diff::{Diff, Leaf, Op, Status, Verify, VerifyStatus};
use crate::error::Result;
use crate::ops::{RecordStatus, VerifySummary};
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
        o.push_str(&format!(
            "### {}@{} → {}@{}\n\n",
            d.a.name, d.a.version, d.b.name, d.b.version
        ));
        o.push_str("| | A | B |\n|---|---|---|\n");
        o.push_str(&format!(
            "| profile uid | `{}` | `{}` |\n",
            dash(&d.a.uid),
            dash(&d.b.uid)
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
            if d.same { "IDENTICAL" } else { "CHANGED" },
            max_impact,
            s.packs_changed,
            s.packs_added,
            s.packs_removed,
            s.packs_unchanged,
            if s.variables_changed { "changed" } else { "unchanged" }
        ));
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
                Status::Changed => ("~", plain_flags(pd)),
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
        let (name, ver, at) = (&recorded.name, &recorded.version, &recorded.fetched_at);
        Some(match v.status {
            VerifyStatus::Unchanged => {
                format!("\n**OK**: published version `{name}@{ver}` is unchanged since {at}")
            }
            VerifyStatus::Drifted => {
                format!("\n**DRIFT**: published version `{name}@{ver}` differs from the copy recorded {at}")
            }
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
}
