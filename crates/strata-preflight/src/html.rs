//! The HTML form of the report: one page, with no script and no external file.
//!
//! The page shows the readiness of each section as a bar. It also shows each result with its
//! messages and its reproduce lines, and the confidence of each rule. A person opens it from the
//! run directory, or a job attaches it.

use strata::closed_set::ClosedSet;
use strata::text::html_text;

use crate::probe::Verdict;
use crate::report::scorecard::{bound_text, not_run_text, scorecard_legend, written_text};
use crate::report::{
    CheckResult, DEFAULT_GATE, FAILED_REQUESTS, Report, SectionScore, pct, percent,
};

/// `s` as [`html_text`], with each `"` as an entity too, so that it can also be the value of an
/// attribute in quotes.
fn esc(s: &str) -> String {
    html_text(s).replace('"', "&quot;")
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
.verdict.none { background: color-mix(in srgb, var(--warn) 15%, transparent); color: var(--warn); }
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
    h.push_str(&tiles(r));
    h.push_str(&scorecard_section(r));
    h.push_str(&failed_request_section(r));
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
    let class = if s.exit_code != 0 {
        "bad"
    } else if s.not_counted() {
        "none"
    } else {
        "ok"
    };
    h.push_str(&format!(
        "<div class=\"verdict {class}\">{}</div>",
        esc(&s.verdict)
    ));
    h
}

/// The tiles of the totals. The counts of the failures are those of the summary and of the
/// scorecard, and the tile of the readiness names the tiers that it counts.
fn tiles(r: &Report) -> String {
    let s = &r.summary;
    let mut h = String::from("<div class=\"tiles\">");
    for (n, what) in [
        (
            pct(s.readiness),
            format!("readiness (tiers {})", r.options.tiers_text()),
        ),
        (s.passed.to_string(), "passed".to_string()),
        (s.failed.to_string(), "failed".to_string()),
        (s.probe_failed.to_string(), "probes failed".to_string()),
        (s.warned.to_string(), "warnings".to_string()),
        (s.not_run.to_string(), "did not run".to_string()),
        (s.not_checked.to_string(), "not checked".to_string()),
        (s.errors.to_string(), "errors".to_string()),
        (s.rules.to_string(), "rules apply".to_string()),
    ] {
        h.push_str(&format!(
            "<div class=\"tile\"><b>{n}</b><span>{}</span></div>",
            esc(&what)
        ));
    }
    h.push_str("</div>");
    h
}

/// Each request that failed, once: the request, the error, the checks that it stopped, and the
/// lines to run by hand. Empty when no request failed.
fn failed_request_section(r: &Report) -> String {
    let failed = r.failed_requests();
    if failed.is_empty() {
        return String::new();
    }
    let mut h = format!("<h2>{FAILED_REQUESTS}</h2><table><tbody>");
    for f in &failed {
        h.push_str(&format!(
            "<tr class=\"error\"><td style=\"white-space:nowrap\"><span class=\"label\">{}</span></td><td><code>{}</code>",
            Verdict::Error.name(),
            esc(&f.request)
        ));
        for m in f.messages {
            h.push_str(&format!("<div class=\"msg\">{}</div>", esc(m)));
        }
        h.push_str(&format!(
            "<div class=\"fix\">{}</div>",
            esc(&f.stopped_text())
        ));
        let reproduce = f.reproduce();
        if !reproduce.is_empty() {
            h.push_str(&format!(
                "<details><summary>reproduce by hand</summary><pre>{}</pre></details>",
                esc(&reproduce.join("\n"))
            ));
        }
        h.push_str("</td></tr>");
    }
    h.push_str("</tbody></table>");
    h
}

/// The section scorecard: a bar and the counts for each section, then the legends.
fn scorecard_section(r: &Report) -> String {
    let s = &r.summary;
    let mut h = String::from(
        "<h2>Section scorecard</h2><table><thead><tr><th>section</th><th>readiness</th><th class=\"num\">ran</th><th class=\"num\">passed</th><th class=\"num\">failed</th><th class=\"num\">probes failed</th><th class=\"num\">warnings</th><th class=\"num\">did not run</th><th class=\"num\">not checked</th><th class=\"num hide-sm\">rows</th><th class=\"num hide-sm\">seen</th><th class=\"num hide-sm\">with a check</th><th class=\"num hide-sm\">preflight</th><th class=\"num hide-sm\">with runs</th><th class=\"hide-sm\">weakest bound</th></tr></thead><tbody>",
    );
    let gate = r.options.ready.unwrap_or(DEFAULT_GATE);
    for sc in &r.sections {
        let (width, class) = match sc.readiness {
            Some(x) => (x * 100.0, if x < gate { "low" } else { "" }),
            None => (0.0, "none"),
        };
        h.push_str(&format!(
            "<tr><td>{}</td><td><div class=\"bar\" title=\"{}\"><i class=\"{class}\" style=\"width:{width:.0}%\"></i><em style=\"left:{:.0}%\"></em></div> <span class=\"conf\">{}</span></td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num hide-sm\">{}</td><td class=\"num hide-sm\">{}</td><td class=\"num hide-sm\">{}</td><td class=\"num hide-sm\">{}</td><td class=\"num hide-sm\">{}</td><td class=\"hide-sm\">{}</td></tr>",
            esc(&sc.title),
            pct(sc.readiness),
            gate * 100.0,
            pct(sc.readiness),
            sc.ran, sc.passed, sc.failed, sc.probe_failed, sc.warned, sc.not_run, sc.not_checked, sc.rows, sc.seen, sc.with_check, sc.preflight, sc.with_runs,
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
        esc(&written_text(&r.options))
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
    // a check that a request stopped points to the request: the page shows the error once
    if let Some(stopped) = c.stopped_text() {
        h.push_str(&format!("<div class=\"msg\">{}</div>", esc(&stopped)));
    } else if c.verdict != Verdict::Passed {
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
    if let Some(fix) = c.shown_fix() {
        h.push_str(&format!(
            "<div class=\"fix\">fix: {}</div>",
            esc(&fix.line())
        ));
    }
    if !c.shown_reproduce().is_empty() {
        h.push_str(&format!(
            "<details><summary>reproduce by hand</summary><pre>{}</pre></details>",
            esc(&c.shown_reproduce().join("\n"))
        ));
    }
    h.push_str(&format!(
        "</td><td class=\"conf\">{}</td></tr>",
        esc(&c.confidence_cell())
    ));
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::confidence::{Confidence, Counts, NO_RUNS};
    use crate::probe::Call;
    use crate::report::*;
    use crate::rule::{Fix, Level, Owner, Severity, Status, Tier};
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
            failed_request: None,
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
                offline: false,
                ready: None,
                out_dir: None,
                out_world: false,
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
                failed: 0,
                probe_failed: 1,
                warned: 0,
                not_run: 0,
                not_checked: 0,
                readiness: Some(0.0),
            }],
            summary: Summary {
                rules: 1,
                ran: 1,
                counted: 0,
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
            html.contains("<b>-</b><span>readiness (tiers static)</span>"),
            "the tile of no readiness"
        );
        assert!(!html.contains('–'), "one mark for no percent");
        let _ = BTreeMap::<String, String>::new();
    }

    #[test]
    fn the_three_forms_write_offline_after_the_tiers_of_a_run_that_read_nothing_from_palette() {
        let mut report = report();
        for (offline, tiers) in [(true, "tiers static, offline"), (false, "tiers static")] {
            report.options.offline = offline;
            for (form, text) in [
                (
                    "terminal",
                    terminal(&report, &strata::term::color::Paint::off()),
                ),
                ("markdown", markdown(&report)),
                ("html", page(&report)),
            ] {
                assert!(text.contains(tiers), "{form}: {text}");
                assert_eq!(
                    text.contains("tiers static, offline"),
                    offline,
                    "{form}: {text}"
                );
            }
        }
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
    fn the_three_forms_show_the_fix_and_the_confidence_of_a_check_by_one_rule() {
        let counts = Counts {
            caught: 7,
            missed: 0,
            passed: 9,
            false_fail: 0,
            runs: vec![],
            older: vec![],
        };
        let mut report = report();
        let mut failed = report.checks[0].clone();
        failed.fix = Some(Fix {
            owner: Owner::Palette,
            text: "Check the zones.".into(),
        });
        failed.confidence = Some(Confidence::of("gke", "4.10", &counts, 0.7));
        // a failed check shows its fix with no line to run by hand too
        let mut bare = failed.clone();
        bare.id = "bare".into();
        bare.reproduce = vec![];
        bare.fix.as_mut().unwrap().text = "Name the zones.".into();
        // a check that found no fault shows no fix, and no confidence shows as no runs
        let mut others = Vec::new();
        for (id, verdict) in [
            ("not-checked", Verdict::NotChecked),
            ("error", Verdict::Error),
            ("passed", Verdict::Passed),
        ] {
            let mut c = failed.clone();
            c.id = id.into();
            c.verdict = verdict;
            c.confidence = None;
            c.fix.as_mut().unwrap().text = format!("The fix of {id}.");
            others.push(c);
        }
        report.checks = vec![failed, bare];
        report.checks.extend(others);
        let forms = [
            (
                "terminal",
                terminal(&report, &strata::term::color::Paint::off()),
                "fix:",
            ),
            ("markdown", markdown(&report), "Fix:"),
            ("html", page(&report), "fix:"),
        ];
        for (form, text, label) in &forms {
            for fix in ["Check the zones. (palette)", "Name the zones. (palette)"] {
                assert!(text.contains(&format!("{label} {fix}")), "{form}: {text}");
            }
            for id in ["not-checked", "error", "passed"] {
                assert!(
                    !text.contains(&format!("The fix of {id}.")),
                    "{form} shows the fix of a check that found no fault: {text}"
                );
            }
            assert!(
                text.contains("catch 7/7 (65%), clean 9/9 (72%)"),
                "{form}: {text}"
            );
            assert_eq!(text.matches("catch 7/7").count(), 2, "{form}: {text}");
        }
        // a table cell says that a check has no runs, and a line of the terminal says nothing
        assert!(forms[1].1.contains("| no runs |"), "{}", forms[1].1);
        assert!(
            forms[2].1.contains("<td class=\"conf\">no runs</td>"),
            "{}",
            forms[2].1
        );
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
