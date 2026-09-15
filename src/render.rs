//! Renderers for the diff document: the terminal report (plain or colored) and markdown for pull
//! request comments. The JSON document itself is serialized straight from `diff::Diff`.

use serde_json::Value;

use crate::color::Paint;
use crate::diff::{Diff, ImpactLevel, Leaf, Op, PackDiff, Status};
use crate::snapshot::Snapshot;

fn short(s: &str) -> &str {
    if s.is_empty() {
        "-"
    } else {
        let end = s.char_indices().nth(12).map(|(i, _)| i).unwrap_or(s.len());
        &s[..end]
    }
}

fn dash(s: &str) -> &str {
    if s.is_empty() {
        "-"
    } else {
        s
    }
}

/// JSON rendering of a leaf value, truncated for the text reports; the JSON document carries all of it.
fn show(v: &Value) -> String {
    let t = serde_json::to_string(v).unwrap_or_default();
    let n = t.chars().count();
    if n > 120 {
        format!(
            "{}... ({n} chars, see --full)",
            t.chars().take(100).collect::<String>()
        )
    } else {
        t
    }
}

fn opmark(op: Op, p: &Paint) -> String {
    match op {
        Op::Changed => p.yellow("~"),
        Op::Added => p.green("+"),
        Op::Removed => p.red("-"),
    }
}

fn vline(l: &Leaf, p: &Paint) -> String {
    let old = l.old.as_ref().map(show).unwrap_or_else(|| "null".into());
    let new = l.new.as_ref().map(show).unwrap_or_else(|| "null".into());
    match l.op {
        Op::Changed => format!(
            "{} {}: {} -> {}",
            opmark(l.op, p),
            l.path,
            p.red(&old),
            p.green(&new)
        ),
        Op::Added => format!("{} {}: {}", opmark(l.op, p), l.path, p.green(&new)),
        Op::Removed => format!("{} {}: {}", opmark(l.op, p), l.path, p.red(&old)),
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

fn plain_flags(pd: &PackDiff) -> String {
    flags(pd, &Paint::off())
}

fn version_display(pd: &PackDiff) -> String {
    let av = pd.a.as_ref().map(|s| s.version.as_str()).unwrap_or("");
    let bv = pd.b.as_ref().map(|s| s.version.as_str()).unwrap_or("");
    match pd.status {
        Status::Added => bv.to_string(),
        Status::Removed => av.to_string(),
        _ if pd.identity && av != bv => format!("{av} -> {bv}"),
        _ => av.to_string(),
    }
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

pub fn terminal(d: &Diff, p: &Paint) -> String {
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
    for l in &d.variables.diff {
        out.push(format!("        {}", vline(l, p)));
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
    s
}

fn md_escape(s: &str) -> String {
    s.replace('|', "\\|")
}

pub fn markdown(d: &Diff) -> String {
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
    o.push_str(&format!(
        "**Result:** {} · max impact: `{}` · packs: {} changed, {} added, {} removed, {} unchanged · variables: {}\n\n",
        if d.same { "IDENTICAL" } else { "CHANGED" },
        serde_json::to_value(s.max_impact).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default(),
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
            md_escape(&pd.name),
            md_escape(&version_display(pd).replace("->", "→")),
            change,
            md_escape(&pd.impact.text)
        ));
    }
    o.push('\n');
    if d.variables.changed {
        o.push_str(&format!(
            "<details><summary>variables: {} changes</summary>\n\n```diff\n",
            d.variables.diff.len()
        ));
        for l in &d.variables.diff {
            o.push_str(&md_leaf(l));
        }
        o.push_str("```\n</details>\n\n");
    }
    for pd in d.packs.iter().filter(|p| p.status == Status::Changed) {
        let mut body = String::new();
        if pd.upstream {
            if let (Some(a), Some(b)) = (&pd.a, &pd.b) {
                body.push_str(&format!(
                    "upstream digest: `{}` → `{}` ({})\n\n",
                    dash(&a.upstream),
                    dash(&b.upstream),
                    b.upstream_source
                ));
            }
        }
        if pd.values.changed && pd.values.parseable && !pd.values.diff.is_empty() {
            body.push_str("```diff\n");
            for l in &pd.values.diff {
                body.push_str(&md_leaf(l));
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
            md_escape(&pd.name),
            what,
            body
        ));
    }
    if d.order_changed {
        o.push_str("> note: pack order changed between the two profiles\n\n");
    }
    o
}

fn md_leaf(l: &Leaf) -> String {
    let old = l.old.as_ref().map(show).unwrap_or_else(|| "null".into());
    let new = l.new.as_ref().map(show).unwrap_or_else(|| "null".into());
    match l.op {
        Op::Changed => format!("- {}: {}\n+ {}: {}\n", l.path, old, l.path, new),
        Op::Added => format!("+ {}: {}\n", l.path, new),
        Op::Removed => format!("- {}: {}\n", l.path, old),
    }
}

fn colorize_unified(text: &str, p: &Paint) -> String {
    if !p.is_on() {
        return text.to_string();
    }
    text.lines()
        .map(|l| {
            if l.starts_with("+++") || l.starts_with("---") {
                p.bold(l)
            } else if l.starts_with('+') {
                p.green(l)
            } else if l.starts_with('-') {
                p.red(l)
            } else if l.starts_with("@@") {
                p.cyan(l)
            } else {
                l.to_string()
            }
        })
        .map(|l| l + "\n")
        .collect()
}

/// Unified diffs of raw values and changed manifests for every changed pack.
pub fn full_diffs(sa: &Snapshot, sb: &Snapshot, d: &Diff, p: &Paint, markdown: bool) -> String {
    let mut o = String::new();
    let mut emit = |title_a: String, title_b: String, old: &str, new: &str| {
        let diff = similar::TextDiff::from_lines(old, new);
        let text = diff
            .unified_diff()
            .context_radius(3)
            .header(&title_a, &title_b)
            .to_string();
        if markdown {
            o.push_str(&format!("```diff\n{text}```\n\n"));
        } else {
            o.push('\n');
            o.push_str(&colorize_unified(&text, p));
        }
    };
    for pd in d.packs.iter().filter(|p| p.status == Status::Changed) {
        let (Some(x), Some(y)) = (
            sa.packs.iter().find(|q| q.key == pd.key),
            sb.packs.iter().find(|q| q.key == pd.key),
        ) else {
            continue;
        };
        if pd.values.changed {
            emit(
                format!("a: {} values", pd.key),
                format!("b: {} values", pd.key),
                &x.values,
                &y.values,
            );
        }
        for m in pd.manifests.iter().filter(|m| m.op == Op::Changed) {
            let (Some(ma), Some(mb)) = (
                x.manifests.iter().find(|q| q.name == m.name),
                y.manifests.iter().find(|q| q.name == m.name),
            ) else {
                continue;
            };
            emit(
                format!("a: {} manifest {}", pd.key, m.name),
                format!("b: {} manifest {}", pd.key, m.name),
                &ma.content,
                &mb.content,
            );
        }
    }
    o
}
