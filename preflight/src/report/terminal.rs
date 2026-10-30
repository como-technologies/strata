//! The terminal form of a report.

use strata::text::cut;

use super::scorecard::{not_run_reasons, not_run_text, scorecard_legend, written_text};
use super::{CheckResult, Report, label_color_for, paint, pct, percent};
use crate::probe::Verdict;
use crate::rule::Severity;

fn label_color(c: &CheckResult) -> &'static str {
    label_color_for(c.verdict, c.severity, c.level)
}

/// The terminal form.
pub fn terminal(r: &Report, color: bool) -> String {
    let mut out = String::new();
    let release = r
        .subject
        .release
        .as_deref()
        .map_or("release not given".to_string(), |x| format!("release {x}"));
    let tiers: Vec<&str> = r.options.tiers.iter().map(|t| t.name()).collect();
    out.push_str(&format!(
        "{}: {} cluster {} ({}), Palette {release}, tiers {}{}\n",
        crate::TOOL_NAME,
        r.subject.cloud,
        r.subject.name_or_dash(),
        r.subject.source,
        tiers.join(", "),
        if r.options.strict { ", strict" } else { "" }
    ));
    for sc in &r.sections {
        let rows: Vec<&CheckResult> = r
            .checks
            .iter()
            .filter(|c| c.section == sc.section)
            .collect();
        let visible: Vec<&CheckResult> = rows
            .iter()
            .copied()
            .filter(|c| !matches!(c.verdict, Verdict::Skipped | Verdict::NotRun))
            .collect();
        if visible.is_empty() && sc.not_run == 0 {
            continue;
        }
        out.push_str(&format!("\n{}\n", paint(color, "1", &sc.title)));
        if sc.not_run > 0 {
            // one line for the checks of the section that did not run, not one line each
            out.push_str(&format!(
                "  {}  {}\n",
                paint(color, "2", &format!("{:<12}", "NOT RUN")),
                not_run_text(&not_run_reasons(
                    rows.iter()
                        .copied()
                        .filter(|c| c.severity != Severity::Note)
                ))
            ));
        }
        for c in visible {
            let label = format!("{:<12}", c.label());
            let conf = c
                .confidence
                .as_ref()
                .map(|k| format!("  catch {} clean {}", k.catch.text(), k.clean.text()))
                .unwrap_or_default();
            out.push_str(&format!(
                "  {}  {:<28} {:<60} {:>5} ms{conf}\n",
                paint(color, label_color(c), &label),
                c.id,
                cut(&c.title, 60),
                c.elapsed_ms
            ));
            if c.verdict != Verdict::Passed {
                for m in &c.messages {
                    out.push_str(&format!("{:<16}{m}\n", ""));
                }
                let one = c.ran.iter().map(|ran| (ran, &c.answer));
                let each = c.requests.iter().map(|r| (&r.ran, &r.answer));
                for (ran, answer) in one.chain(each) {
                    out.push_str(&format!(
                        "{:<16}{} {ran}{}\n",
                        "",
                        paint(color, "2", "ran:"),
                        answer
                            .as_ref()
                            .map(|a| format!(" -> {a}"))
                            .unwrap_or_default()
                    ));
                }
            }
            if matches!(
                c.verdict,
                Verdict::Failed | Verdict::NotChecked | Verdict::Error
            ) {
                for line in &c.reproduce {
                    out.push_str(&format!(
                        "{:<16}{} {line}\n",
                        "",
                        paint(color, "2", "reproduce:")
                    ));
                }
                if c.verdict == Verdict::Failed
                    && let Some(fix) = &c.fix
                    && !fix.text.is_empty()
                {
                    out.push_str(&format!(
                        "{:<16}{} {} ({})\n",
                        "",
                        paint(color, "2", "fix:"),
                        fix.text,
                        fix.owner
                    ));
                }
            }
        }
    }
    out.push_str(&format!("\n{}\n", paint(color, "1", "Section scorecard")));
    out.push_str(&format!(
        "  {:<28} {:>4} {:>4} {:>5} {:>9} {:>4} {:>7} {:>4} {:>4} {:>4} {:>7} {:>11} {:>5}\n",
        "section",
        "rows",
        "seen",
        "check",
        "preflight",
        "runs",
        "weakest",
        "ran",
        "pass",
        "fail",
        "not run",
        "not checked",
        "ready"
    ));
    for s in &r.sections {
        out.push_str(&format!(
            "  {:<28} {:>4} {:>4} {:>5} {:>9} {:>4} {:>7} {:>4} {:>4} {:>4} {:>7} {:>11} {:>5}\n",
            cut(&s.title, 28),
            s.rows,
            s.seen,
            s.with_check,
            s.preflight,
            s.with_runs,
            s.weakest_bound
                .map_or_else(|| "no runs".to_string(), percent),
            s.ran,
            s.passed,
            s.failed,
            s.not_run,
            s.not_checked,
            pct(s.readiness)
        ));
    }
    out.push_str(&format!(
        "{}\n",
        paint(color, "2", &scorecard_legend(&r.sections))
    ));
    let s = &r.summary;
    out.push_str(&format!(
        "\n{} rules apply; {} ran: {} passed, {} failed, {} probe(s) failed, {} warning(s), {} note(s); {} did not run, {} not checked, {} do not apply, {} error(s); readiness {}: {} of {} checks passed\n",
        s.rules, s.ran, s.passed, s.failed, s.probe_failed, s.warned, s.noted, s.not_run, s.not_checked, s.skipped, s.errors, pct(s.readiness), s.passed, s.ran + s.not_run
    ));
    if s.not_run > 0 {
        out.push_str(&format!(
            "{}\n",
            paint(color, "2", &not_run_text(&s.not_run_reasons))
        ));
    }
    out.push_str(&format!(
        "{}\n",
        paint(color, "2", &written_text(r.options.out_dir.as_deref()))
    ));
    let code = if s.exit_code == 0 { "32" } else { "31" };
    out.push_str(&format!("{}\n", paint(color, code, &s.verdict)));
    out
}
