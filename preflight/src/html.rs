//! The HTML form of the report: one page, no script and no external file, with the readiness of
//! each section as a bar, each result with its messages and its reproduce lines, and the
//! confidence of each rule. A person opens it from the run directory, or a job attaches it.

use strata::closed_set::ClosedSet;

use crate::confidence::NO_RUNS;
use crate::probe::Verdict;
use crate::report::scorecard::{bound_text, not_run_text, scorecard_legend, written_text};
use crate::report::{CheckResult, Report, SectionScore, Summary, pct, percent};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const STYLE: &str = r#"
:root { --bg: #fff; --fg: #1a1a1a; --muted: #666; --line: #e3e3e3; --card: #f7f7f7;
  --pass: #2e7d32; --fail: #c62828; --probe: #7b1fa2; --warn: #ef6c00; --note: #0277bd; --nc: #9e9e9e; --bar: #dcdcdc; }
@media (prefers-color-scheme: dark) { :root { --bg: #121212; --fg: #e8e8e8; --muted: #a0a0a0; --line: #2c2c2c; --card: #1c1c1c;
  --pass: #66bb6a; --fail: #ef5350; --probe: #ba68c8; --warn: #ffa726; --note: #4fc3f7; --nc: #8a8a8a; --bar: #333; } }
body { margin: 0; padding: 24px 16px; background: var(--bg); color: var(--fg); font: 15px/1.45 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; }
main { max-width: 1100px; margin: 0 auto; }
h1 { font-size: 22px; margin: 0 0 4px; } h2 { font-size: 17px; margin: 28px 0 8px; }
.sub { color: var(--muted); margin: 0 0 16px; }
.verdict { display: inline-block; padding: 6px 12px; border-radius: 6px; font-weight: 600; margin: 8px 0 16px; }
.verdict.ok { background: color-mix(in srgb, var(--pass) 15%, transparent); color: var(--pass); }
.verdict.bad { background: color-mix(in srgb, var(--fail) 15%, transparent); color: var(--fail); }
.tiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(120px, 1fr)); gap: 10px; margin: 0 0 8px; }
.tile { background: var(--card); border: 1px solid var(--line); border-radius: 8px; padding: 10px 12px; }
.tile b { display: block; font-size: 22px; font-variant-numeric: tabular-nums; } .tile span { color: var(--muted); font-size: 13px; }
table { width: 100%; border-collapse: collapse; font-size: 14px; } th, td { text-align: left; padding: 6px 8px; border-bottom: 1px solid var(--line); vertical-align: top; }
th { color: var(--muted); font-weight: 600; } td.num, th.num { text-align: right; font-variant-numeric: tabular-nums; }
.bar { position: relative; height: 10px; background: var(--bar); border-radius: 5px; min-width: 120px; }
.bar i { position: absolute; left: 0; top: 0; bottom: 0; border-radius: 5px; background: var(--pass); }
.bar i.low { background: var(--warn); } .bar i.none { background: var(--nc); }
.bar em { position: absolute; top: -1px; bottom: -1px; width: 2px; background: var(--fg); opacity: .5; }
.label { display: inline-block; min-width: 96px; padding: 1px 8px; border-radius: 4px; font: 600 12px/20px ui-monospace, SFMono-Regular, Menlo, monospace; text-align: center; color: #fff; }
.pass .label { background: var(--pass); } .fail .label, .error .label { background: var(--fail); } .probe-fail .label { background: var(--probe); }
.warn .label { background: var(--warn); } .note .label { background: var(--note); } .not-checked .label { background: var(--nc); } .skip .label { background: var(--nc); opacity: .6; }
code, pre { font: 13px ui-monospace, SFMono-Regular, Menlo, monospace; } pre { background: var(--card); border: 1px solid var(--line); border-radius: 6px; padding: 8px 10px; overflow-x: auto; margin: 6px 0 0; white-space: pre-wrap; word-break: break-all; }
.msg { margin: 4px 0 0; color: var(--fg); } .fix { color: var(--muted); margin: 4px 0 0; } .conf { color: var(--muted); font-size: 13px; white-space: nowrap; }
details summary { cursor: pointer; color: var(--muted); font-size: 13px; margin-top: 4px; }
.legend { color: var(--muted); font-size: 13px; margin-top: 24px; }
@media (max-width: 700px) { .hide-sm { display: none; } }
"#;
/// The page.
pub fn page(r: &Report) -> String {
    let mut h = head(r);
    h.push_str(&tiles(&r.summary));
    h.push_str(&scorecard_section(r));
    h.push_str("<h2>Results</h2>");
    for sc in &r.sections {
        h.push_str(&section_results(r, sc));
    }
    h.push_str(KEY);
    h.push_str("</main></body></html>\n");
    h
}

/// What the labels of the results mean.
const KEY: &str = "<p class=\"legend\">PASS and FAIL are preflight checks: the runs prove them, and a FAIL is exit 1. PROBE PASS and PROBE FAIL are probes: the fault is known, the runs do not yet prove the check, and a failure does not change the exit code without <code>--strict</code>. NOT CHECKED rows say what to run by hand. NOT RUN is a check that applies here and did not run: a tier not asked for, no key, or <code>--no-exec</code>. Confidence is a count of runs: <code>14/14 (81%)</code> is 14 correct runs of 14, and 81% is the 95% lower bound of the rate.</p>";

/// The start of the page: the head, the title, the line of the subject, and the verdict.
fn head(r: &Report) -> String {
    let s = &r.summary;
    let mut h = String::new();
    h.push_str("<!DOCTYPE html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    h.push_str(&format!(
        "<title>preflight: {} {}</title><style>{STYLE}</style></head><body><main>",
        esc(&r.subject.cloud),
        esc(if r.subject.name.is_empty() {
            "cluster"
        } else {
            &r.subject.name
        })
    ));
    h.push_str(&format!(
        "<h1>Can Palette build this cluster? <span style=\"color:var(--muted)\">{} · {}</span></h1>",
        esc(&r.subject.cloud),
        esc(r.subject.name_or_dash())
    ));
    h.push_str(&format!(
        "<p class=\"sub\">source <code>{}</code> · Palette {} · tiers {} · {} · {} {}</p>",
        esc(&r.subject.source),
        esc(&r.subject.release_text()),
        esc(&r.options.tiers_text()),
        esc(&r.generated_at),
        esc(&r.tool.name),
        esc(&r.tool.version)
    ));
    h.push_str(&format!(
        "<div class=\"verdict {}\">{}</div>",
        if s.exit_code == 0 { "ok" } else { "bad" },
        esc(&s.verdict)
    ));
    h
}

/// The tiles of the totals.
fn tiles(s: &Summary) -> String {
    let mut h = String::from("<div class=\"tiles\">");
    for (n, what) in [
        (pct(s.readiness), "readiness"),
        (s.passed.to_string(), "passed"),
        ((s.failed + s.probe_failed).to_string(), "failed"),
        (s.not_run.to_string(), "did not run"),
        (s.warned.to_string(), "warnings"),
        (s.not_checked.to_string(), "not checked"),
        (s.errors.to_string(), "errors"),
        (s.rules.to_string(), "rules apply"),
    ] {
        h.push_str(&format!(
            "<div class=\"tile\"><b>{n}</b><span>{what}</span></div>"
        ));
    }
    h.push_str("</div>");
    h
}

/// The section scorecard: a bar and the counts for each section, then the legends.
fn scorecard_section(r: &Report) -> String {
    let s = &r.summary;
    let mut h = String::from(
        "<h2>Section scorecard</h2><table><thead><tr><th>section</th><th>readiness</th><th class=\"num\">ran</th><th class=\"num\">passed</th><th class=\"num\">failed</th><th class=\"num\">did not run</th><th class=\"num\">not checked</th><th class=\"num hide-sm\">rows</th><th class=\"num hide-sm\">seen</th><th class=\"num hide-sm\">with a check</th><th class=\"num hide-sm\">preflight</th><th class=\"num hide-sm\">with runs</th><th class=\"hide-sm\">weakest bound</th></tr></thead><tbody>",
    );
    let gate = r.options.ready.unwrap_or(0.8);
    for sc in &r.sections {
        let (width, class) = match sc.readiness {
            Some(x) => (x * 100.0, if x < gate { "low" } else { "" }),
            None => (0.0, "none"),
        };
        h.push_str(&format!(
            "<tr><td>{}</td><td><div class=\"bar\" title=\"{}\"><i class=\"{class}\" style=\"width:{width:.0}%\"></i><em style=\"left:{:.0}%\"></em></div> <span class=\"conf\">{}</span></td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num hide-sm\">{}</td><td class=\"num hide-sm\">{}</td><td class=\"num hide-sm\">{}</td><td class=\"num hide-sm\">{}</td><td class=\"num hide-sm\">{}</td><td class=\"hide-sm\">{}</td></tr>",
            esc(&sc.title),
            pct(sc.readiness),
            gate * 100.0,
            pct(sc.readiness),
            sc.ran, sc.passed, sc.failed, sc.not_run, sc.not_checked, sc.rows, sc.seen, sc.with_check, sc.preflight, sc.with_runs,
            bound_text(sc.weakest_bound)
        ));
    }
    h.push_str("</tbody></table>");
    h.push_str(&format!("<p class=\"legend\">The mark on each bar is the gate, {}. Readiness is the share of the checks that apply here and passed: a check that did not run counts against it, and a row with no check in strata-preflight is outside it, so read the two together.</p>", percent(gate)));
    h.push_str(&format!(
        "<p class=\"legend\">{}</p>",
        esc(&scorecard_legend(&r.sections))
    ));
    if s.not_run > 0 {
        h.push_str(&format!(
            "<p class=\"legend\">{}</p>",
            esc(&not_run_text(&s.not_run_reasons))
        ));
    }
    h.push_str(&format!(
        "<p class=\"legend\">{}</p>",
        esc(&written_text(r.options.out_dir.as_deref()))
    ));
    h
}

/// The table of the results of one section. Empty when the section lists no result.
fn section_results(r: &Report, sc: &SectionScore) -> String {
    let Some(section) = r.section_rows(sc) else {
        return String::new();
    };
    let mut h = format!("<h3>{}</h3><table><tbody>", esc(&sc.title));
    if sc.not_run > 0 {
        h.push_str(&format!(
            "<tr class=\"skip\"><td style=\"white-space:nowrap\"><span class=\"label\">{}</span></td><td>{}</td><td class=\"conf\"></td></tr>",
            Verdict::NotRun.name(),
            esc(&section.not_run_text())
        ));
    }
    for c in section.listed {
        h.push_str(&result_row(c));
    }
    h.push_str("</tbody></table>");
    h
}

/// The row of one result: its label, its title and messages, its requests, its fix and reproduce
/// lines, and its confidence.
fn result_row(c: &CheckResult) -> String {
    let mut h = format!(
        "<tr class=\"{}\"><td style=\"white-space:nowrap\"><span class=\"label\">{}</span></td><td>",
        c.label_kind().class(),
        esc(&c.label())
    );
    h.push_str(&format!(
        "<code>{}</code> — {} <span class=\"conf\">· {} · {} ms{}</span>",
        esc(&c.id),
        esc(&c.title),
        c.tier.as_str(),
        c.elapsed_ms,
        c.answer
            .as_ref()
            .map(|a| format!(" · {}", esc(a)))
            .unwrap_or_default()
    ));
    if c.verdict != Verdict::Passed {
        for m in &c.messages {
            h.push_str(&format!("<div class=\"msg\">{}</div>", esc(m)));
        }
    }
    // a check with `each` has no answer of its own: each request has one
    if !c.requests.is_empty() {
        h.push_str(&format!(
            "<details><summary>requests and answers</summary><pre>{}</pre></details>",
            esc(&c.ran_lines().join("\n"))
        ));
    }
    if matches!(
        c.verdict,
        Verdict::Failed | Verdict::NotChecked | Verdict::Error
    ) {
        if let Some(fix) = &c.fix
            && !fix.text.is_empty()
        {
            h.push_str(&format!(
                "<div class=\"fix\">fix ({}): {}</div>",
                esc(&fix.owner),
                esc(&fix.text)
            ));
        }
        if !c.reproduce.is_empty() {
            h.push_str(&format!(
                "<details><summary>reproduce by hand</summary><pre>{}</pre></details>",
                esc(&c.reproduce.join("\n"))
            ));
        }
    }
    h.push_str("</td><td class=\"conf\">");
    match &c.confidence {
        Some(k) => h.push_str(&format!(
            "catch {}<br>clean {}",
            esc(&k.catch.text()),
            esc(&k.clean.text())
        )),
        None => h.push_str(NO_RUNS),
    }
    h.push_str("</td></tr>");
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::confidence::NO_RUNS;
    use crate::probe::Call;
    use crate::report::*;
    use crate::rule::{Level, Severity, Status, Tier};
    use std::collections::BTreeMap;

    /// The report of one probe that failed, with no request.
    fn report() -> Report {
        let check = CheckResult {
            id: "gke-pool-multiple-3".into(),
            title: "A GKE pool <size>".into(),
            section: "compute".into(),
            tier: Tier::Static,
            level: Level::Probe,
            severity: Severity::Fail,
            status: Status::Seen,
            verdict: Verdict::Failed,
            messages: vec!["pool worker has size 4".into()],
            elapsed_ms: 2,
            ran: None,
            answer: None,
            requests: vec![],
            reproduce: vec!["jq '.a' world.json".into()],
            fix: None,
            evidence: vec![],
            confidence: None,
            counts_as_failure: false,
        };
        Report {
            schema: REPORT_SCHEMA.into(),
            tool: Tool {
                name: "strata-preflight".into(),
                version: "0".into(),
            },
            generated_at: "t".into(),
            subject: Subject {
                cloud: "gke".into(),
                release: None,
                source: "c.json".into(),
                name: "fred".into(),
                cluster_uid: None,
            },
            options: Options {
                tiers: vec![Tier::Static],
                strict: false,
                ready: None,
                out_dir: None,
            },
            checks: vec![check],
            sections: vec![SectionScore {
                section: "compute".into(),
                title: "Compute".into(),
                rows: 1,
                seen: 1,
                with_check: 1,
                preflight: 0,
                with_runs: 0,
                weakest_bound: None,
                ran: 1,
                passed: 0,
                failed: 1,
                not_run: 0,
                not_checked: 0,
                readiness: Some(0.0),
            }],
            summary: Summary {
                rules: 1,
                ran: 1,
                passed: 0,
                failed: 0,
                probe_failed: 1,
                warned: 0,
                noted: 0,
                not_run: 0,
                not_run_reasons: Default::default(),
                not_checked: 0,
                skipped: 0,
                errors: 0,
                readiness: Some(0.0),
                exit_code: 0,
                verdict: "OK".into(),
            },
            legend: serde_json::from_value(
                serde_json::json!({"verdicts": {}, "levels": {}, "tiers": {}, "exit": {}}),
            )
            .unwrap(),
        }
    }

    #[test]
    fn the_page_is_one_file_with_each_result() {
        let mut report = report();
        let html = page(&report);
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(!html.contains("<script"), "no script");
        assert!(
            !html.contains("http://") && !html.contains("https://"),
            "no external file"
        );
        assert!(html.contains("A GKE pool &lt;size&gt;"), "escaped");
        assert!(html.contains("PROBE FAIL"));
        assert!(html.contains("pool worker has size 4"));
        assert!(html.contains("prefers-color-scheme: dark"));
        // a readiness that no check gives shows as the terminal and the markdown show it
        report.summary.readiness = None;
        report.sections[0].readiness = None;
        let html = page(&report);
        assert!(
            html.contains("<b>-</b><span>readiness</span>"),
            "the tile of no readiness"
        );
        assert!(!html.contains('–'), "one mark for no percent");
        let _ = BTreeMap::<String, String>::new();
    }

    #[test]
    fn the_three_forms_write_the_release_the_tiers_and_the_missing_name_in_one_form() {
        let mut report = report();
        report.subject.release = Some("4.10.3".into());
        report.subject.name = String::new();
        report.options.tiers = vec![Tier::Static, Tier::Palette];
        report.options.strict = true;
        let forms = [
            (
                "terminal",
                terminal(&report, &strata::term::color::Paint::off()),
            ),
            ("markdown", markdown(&report)),
            ("html", page(&report)),
        ];
        for (form, text) in &forms {
            assert!(text.contains("Palette release 4.10.3"), "{form}: {text}");
            assert!(
                text.contains("tiers static, palette, strict"),
                "{form}: {text}"
            );
        }
        // no release: each form says so in the same words
        report.subject.release = None;
        for (form, text) in [
            (
                "terminal",
                terminal(&report, &strata::term::color::Paint::off()),
            ),
            ("markdown", markdown(&report)),
            ("html", page(&report)),
        ] {
            assert!(text.contains("Palette release not given"), "{form}: {text}");
        }
        // no run of the ledger gives a bound: each form says so in the same words
        for (form, text) in &forms {
            assert!(text.contains(NO_RUNS), "{form}: {text}");
        }
        assert!(forms[0].1.contains("gke cluster - ("), "{}", forms[0].1);
        assert!(forms[1].1.contains("gke cluster `-`"), "{}", forms[1].1);
        assert!(forms[2].1.contains("gke · -"), "{}", forms[2].1);
    }

    #[test]
    fn the_page_lists_each_request_of_a_check_with_each_with_its_answer() {
        let mut report = report();
        report.checks[0].requests = vec![
            Call::get("/v1/edgehosts/h1", Some("HTTP 200")),
            Call::get("/v1/edgehosts/h2", Some("HTTP 404")),
        ];
        let html = page(&report);
        assert!(
            html.contains(
                "GET /v1/edgehosts/h1 -&gt; HTTP 200\nGET /v1/edgehosts/h2 -&gt; HTTP 404"
            ),
            "{html}"
        );
    }
}
