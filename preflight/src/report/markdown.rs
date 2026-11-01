//! The markdown form of a report: a pull request comment.

use strata::text::markdown_cell;

use super::scorecard::{bound_text, not_run_text, scorecard_legend, written_text};
use super::{CheckResult, Report, pct};
use crate::confidence::NO_RUNS;
use crate::probe::Verdict;

/// The markdown form: a pull request comment.
pub fn markdown(r: &Report) -> String {
    let mut out = String::new();
    let s = &r.summary;
    out.push_str(&format!(
        "## strata-preflight: {} cluster `{}`\n\n",
        r.subject.cloud,
        r.subject.name_or_dash()
    ));
    out.push_str(&format!(
        "**{}** — source `{}`, Palette {}, tiers {}. {} ran: {} passed, {} failed, {} probe(s) failed; {} did not run, {} not checked; readiness **{}** ({} of {} checks passed).\n\n",
        s.verdict,
        r.subject.source,
        r.subject.release_text(),
        r.options.tiers_text(),
        s.ran, s.passed, s.failed, s.probe_failed, s.not_run, s.not_checked, pct(s.readiness), s.passed, s.ran + s.not_run
    ));
    out.push_str(
        "| result | rule | tier | what | time | confidence |\n|---|---|---|---|---|---|\n",
    );
    for c in r.checks.iter().filter(|c| c.is_listed()) {
        let what = if c.verdict == Verdict::Passed {
            c.title.clone()
        } else {
            let mut t = c.title.clone();
            if !c.messages.is_empty() {
                t.push_str("<br>");
                t.push_str(
                    &c.messages
                        .iter()
                        .map(|m| markdown_cell(m))
                        .collect::<Vec<_>>()
                        .join("<br>"),
                );
            }
            t
        };
        let conf = c
            .confidence
            .as_ref()
            .map(|k| format!("catch {}, clean {}", k.catch.text(), k.clean.text()))
            .unwrap_or_else(|| NO_RUNS.to_string());
        out.push_str(&format!(
            "| {} | `{}` | {} | {} | {} ms | {} |\n",
            c.label(),
            c.id,
            c.tier.letter(),
            what,
            c.elapsed_ms,
            conf
        ));
    }
    let repro: Vec<&CheckResult> = r
        .checks
        .iter()
        .filter(|c| {
            matches!(
                c.verdict,
                Verdict::Failed | Verdict::NotChecked | Verdict::Error
            ) && !c.reproduce.is_empty()
        })
        .collect();
    if !repro.is_empty() {
        out.push_str("\n<details><summary>Reproduce by hand</summary>\n\n");
        for c in repro {
            out.push_str(&format!(
                "**`{}`** — {}\n\n```bash\n{}\n```\n\n",
                c.id,
                c.title,
                c.reproduce.join("\n")
            ));
            if let Some(fix) = &c.fix
                && !fix.text.is_empty()
            {
                out.push_str(&format!("Fix ({}): {}\n\n", fix.owner, fix.text));
            }
        }
        out.push_str("</details>\n");
    }
    if s.not_run > 0 {
        out.push_str(&format!("\n{}\n", not_run_text(&s.not_run_reasons)));
    }
    out.push_str(&format!(
        "\n{}\n",
        written_text(r.options.out_dir.as_deref())
    ));
    out.push_str("\n### Section scorecard\n\n| section | rows | seen | with a check | preflight | with runs | weakest bound | ran | passed | failed | did not run | not checked | readiness |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|\n");
    for sc in &r.sections {
        out.push_str(&format!(
            "| {} | {} | {} ({}) | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            sc.title,
            sc.rows,
            sc.seen,
            pct((sc.rows > 0).then(|| f64::from(sc.seen) / f64::from(sc.rows))),
            sc.with_check,
            sc.preflight,
            sc.with_runs,
            bound_text(sc.weakest_bound),
            sc.ran,
            sc.passed,
            sc.failed,
            sc.not_run,
            sc.not_checked,
            pct(sc.readiness)
        ));
    }
    out.push_str(&format!("\n{}\n", scorecard_legend(&r.sections)));
    out
}
