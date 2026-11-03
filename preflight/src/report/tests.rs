use super::*;
use crate::probe::{Invocation, Ran};
use crate::rule::EvidenceKind;
use std::time::Duration;
use strata::api::Method;

fn check(
    id: &str,
    verdict: Verdict,
    level: Level,
    severity: Severity,
    counts: bool,
) -> CheckResult {
    CheckResult {
        id: id.into(),
        title: format!("title of {id}"),
        section: "compute".into(),
        tier: Tier::Static,
        level,
        severity,
        status: Status::Seen,
        verdict,
        messages: if verdict == Verdict::Failed {
            vec!["why".into()]
        } else {
            vec![]
        },
        elapsed_ms: 1,
        ran: None,
        answer: None,
        requests: vec![],
        reproduce: vec![],
        fix: None,
        evidence: vec![],
        confidence: None,
        counts_as_failure: counts,
    }
}

fn options() -> Options {
    Options {
        tiers: vec![Tier::Static],
        strict: false,
        ready: None,
        out_dir: None,
    }
}

/// One section that holds every check, for the tests of the summary.
fn sections(checks: &[CheckResult]) -> Vec<SectionScore> {
    vec![SectionScore {
        section: "compute".into(),
        title: "Compute".into(),
        rows: checks.len() as u32,
        seen: 0,
        with_check: 0,
        preflight: 0,
        with_runs: 0,
        weakest_bound: None,
        ran: checks
            .iter()
            .filter(|c| matches!(c.verdict, Verdict::Passed | Verdict::Failed))
            .count() as u32,
        passed: checks
            .iter()
            .filter(|c| c.verdict == Verdict::Passed)
            .count() as u32,
        failed: checks
            .iter()
            .filter(|c| c.verdict == Verdict::Failed)
            .count() as u32,
        not_run: checks
            .iter()
            .filter(|c| c.verdict == Verdict::NotRun)
            .count() as u32,
        not_checked: 0,
        readiness: None,
    }]
}

#[test]
fn an_evidence_entry_with_an_empty_reference_has_no_ref_in_the_json() {
    let mut entry = check("a", Verdict::Passed, Level::Probe, Severity::Fail, false);
    entry.evidence = vec![
        Evidence {
            kind: EvidenceKind::Source,
            reference: String::new(),
            note: "Palette refuses the create.".into(),
        },
        Evidence {
            kind: EvidenceKind::Run,
            reference: "20260929T042910Z".into(),
            note: String::new(),
        },
    ];
    let json = serde_json::to_value(&entry).unwrap();
    assert_eq!(
        json["evidence"],
        serde_json::json!([
            {"kind": "source", "note": "Palette refuses the create."},
            {"kind": "run", "ref": "20260929T042910Z"},
        ])
    );
}

#[test]
fn a_check_that_did_not_run_lowers_the_readiness_and_a_rule_that_does_not_apply_does_not() {
    let mut not_run = check("b", Verdict::NotRun, Level::Probe, Severity::Fail, false);
    not_run.messages = vec!["tier palette was not asked for".into()];
    let checks = vec![
        check("a", Verdict::Passed, Level::Probe, Severity::Fail, false),
        not_run,
        check("c", Verdict::Skipped, Level::Probe, Severity::Fail, false),
        check(
            "d",
            Verdict::NotChecked,
            Level::Probe,
            Severity::Fail,
            false,
        ),
    ];
    let s = summarize(&checks, &sections(&checks), &options());
    assert_eq!((s.ran, s.not_run, s.skipped, s.not_checked), (1, 1, 1, 1));
    assert_eq!(
        s.readiness,
        Some(0.5),
        "one of two checks that apply passed"
    );
    assert_eq!(
        s.not_run_reasons.get("tier palette was not asked for"),
        Some(&1)
    );
    let none = vec![check(
        "c",
        Verdict::Skipped,
        Level::Probe,
        Severity::Fail,
        false,
    )];
    assert_eq!(
        summarize(&none, &sections(&none), &options()).readiness,
        None
    );
}

#[test]
fn a_note_that_did_not_run_is_outside_the_readiness_as_a_note_that_ran_is() {
    let mut note = check("n", Verdict::NotRun, Level::Probe, Severity::Note, false);
    note.messages = vec!["tier palette was not asked for".into()];
    let checks = vec![
        check("a", Verdict::Passed, Level::Probe, Severity::Fail, false),
        note,
    ];
    let s = summarize(&checks, &sections(&checks), &options());
    assert_eq!((s.ran, s.not_run), (1, 0));
    assert_eq!(s.readiness, Some(1.0), "the note does not count against it");
    assert!(s.not_run_reasons.is_empty());
}

#[test]
fn the_reproduce_line_of_a_post_sends_the_body_the_check_sent() {
    let section = r#"
section: identity
title: I
rules:
  - id: sts-account-not-assumable
    title: t
    engines: [aws]
    tier: palette
    status: seen
    what: w
    check: {kind: palette, method: POST, path: "/v1/clouds/aws/account/validate", body: ".account.spec", expr: "true"}
  - id: palette-validate
    title: t
    engines: [aws]
    tier: palette
    status: seen
    what: w
    check: {kind: palette, method: POST, path: "/v1/spectroclusters/aws/validate", expr: "true"}
"#;
    let mut c = crate::rule::Catalog::default();
    c.push(
        crate::rule::Catalog::parse_section(section, std::path::Path::new("i.yaml")).unwrap(),
        std::path::Path::new("i.yaml"),
    )
    .unwrap();
    let subject = Subject {
        cloud: "aws".into(),
        release: None,
        source: "c.json".into(),
        name: "fred".into(),
        cluster_uid: None,
    };
    let outcome = |path: &str| Outcome {
        verdict: Verdict::Failed,
        messages: vec![],
        elapsed: Duration::from_millis(1),
        ran: Ran::Request(Call {
            method: Method::Post,
            path: path.to_string(),
            answer: None,
        }),
    };
    let with_body = c.rule("sts-account-not-assumable").unwrap();
    let lines = auto_reproduce(
        with_body,
        &outcome("/v1/clouds/aws/account/validate"),
        &subject,
        Some("out"),
    );
    assert_eq!(lines[0], "jq '.account.spec' out/world.json > request.json");
    assert!(lines[1].contains("--data @request.json"), "{}", lines[1]);
    assert!(
        lines[1].contains("/v1/clouds/aws/account/validate"),
        "{}",
        lines[1]
    );
    let entity = c.rule("palette-validate").unwrap();
    let lines = auto_reproduce(
        entity,
        &outcome("/v1/spectroclusters/aws/validate"),
        &subject,
        Some("out"),
    );
    assert!(lines[0].contains("--data @out/entity.json"), "{}", lines[0]);
}

#[test]
fn the_reproduce_lines_of_a_command_with_a_read_of_palette_send_the_request_then_judge_both_answers()
 {
    let section = r#"
section: quotas
title: Q
rules:
  - id: aws-quotas
    title: t
    engines: [aws]
    tier: cloud
    status: seen
    what: w
    check:
      kind: exec
      needs: [aws]
      palette: {path: "/v1/clouds/aws/regions/us-east-2/instancetypes?cloudAccountUid=acc"}
      argv: [aws, service-quotas, get-service-quota]
      expr: ".response.ok and .exec.exit == 0"
"#;
    let mut c = crate::rule::Catalog::default();
    c.push(
        crate::rule::Catalog::parse_section(section, std::path::Path::new("q.yaml")).unwrap(),
        std::path::Path::new("q.yaml"),
    )
    .unwrap();
    let subject = Subject {
        cloud: "aws".into(),
        release: None,
        source: "c.json".into(),
        name: "fred".into(),
        cluster_uid: None,
    };
    let rule = c.rule("aws-quotas").unwrap();
    let outcome = |request: Option<Call>| Outcome {
        verdict: Verdict::Failed,
        messages: vec![],
        elapsed: Duration::from_millis(1),
        ran: Ran::Exec {
            first: Invocation::ended("aws service-quotas get-service-quota", 0),
            also: None,
            request,
        },
    };
    let call = Call::get(
        "/v1/clouds/aws/regions/us-east-2/instancetypes?cloudAccountUid=acc",
        Some("HTTP 200"),
    );
    let lines = auto_reproduce(rule, &outcome(Some(call)), &subject, Some("out"));
    assert_eq!(lines.len(), 3, "{lines:#?}");
    assert!(
        lines[0].starts_with("code=$(curl -s -o body.json"),
        "{}",
        lines[0]
    );
    assert!(
        lines[0].contains("/instancetypes?cloudAccountUid=acc"),
        "{}",
        lines[0]
    );
    assert_eq!(lines[1], "aws service-quotas get-service-quota");
    assert!(
        lines[2].contains("--slurpfile body body.json"),
        "{}",
        lines[2]
    );
    assert!(lines[2].contains("'.response = {ok:"), "{}", lines[2]);
    assert!(lines[2].contains(".exec = {exit: 0"), "{}", lines[2]);
    // a command that sent no request shows the command and the program over `.exec` alone
    let lines = auto_reproduce(rule, &outcome(None), &subject, Some("out"));
    assert_eq!(lines.len(), 2, "{lines:#?}");
    assert!(!lines[1].contains(".response ="), "{}", lines[1]);
}

#[test]
fn the_reproduce_lines_of_a_command_with_a_second_command_run_both_and_judge_both_outputs() {
    let section = r#"
section: identity
title: I
rules:
  - id: iam-role-policy-quota
    title: t
    engines: [aws]
    tier: cloud
    status: seen
    what: w
    check:
      kind: exec
      needs: [aws]
      argv: [aws, iam, list-attached-role-policies]
      also: [aws, service-quotas, get-service-quota]
      expr: ".exec.exit == 0 and .also.exit == 0"
"#;
    let mut c = crate::rule::Catalog::default();
    c.push(
        crate::rule::Catalog::parse_section(section, std::path::Path::new("i.yaml")).unwrap(),
        std::path::Path::new("i.yaml"),
    )
    .unwrap();
    let subject = Subject {
        cloud: "aws".into(),
        release: None,
        source: "c.json".into(),
        name: "fred".into(),
        cluster_uid: None,
    };
    let outcome = Outcome {
        verdict: Verdict::Failed,
        messages: vec![],
        elapsed: Duration::from_millis(1),
        ran: Ran::Exec {
            first: Invocation::ended("aws iam list-attached-role-policies", 0),
            also: Some(Invocation::ended(
                "aws service-quotas get-service-quota",
                254,
            )),
            request: None,
        },
    };
    let lines = auto_reproduce(
        c.rule("iam-role-policy-quota").unwrap(),
        &outcome,
        &subject,
        Some("out"),
    );
    assert_eq!(lines.len(), 3, "{lines:#?}");
    assert_eq!(lines[0], "aws iam list-attached-role-policies");
    assert_eq!(lines[1], "aws service-quotas get-service-quota");
    assert!(lines[2].contains("--argjson out"), "{}", lines[2]);
    assert!(lines[2].contains("--argjson also"), "{}", lines[2]);
    assert!(lines[2].contains(".exec = {exit: 0"), "{}", lines[2]);
    assert!(lines[2].contains(".also = {exit: 254"), "{}", lines[2]);
}

#[test]
fn the_report_line_of_a_check_with_a_second_command_shows_the_exit_code_of_each_command() {
    let section = r#"
section: identity
title: I
rules:
  - id: iam-role-policy-quota
    title: t
    engines: [aws]
    tier: cloud
    status: seen
    what: w
    check:
      kind: exec
      needs: [aws]
      argv: [aws, iam, list-attached-role-policies]
      also: [aws, service-quotas, get-service-quota]
      expr: ".exec.exit == 0"
"#;
    let mut catalog = Catalog::default();
    catalog
        .push(
            Catalog::parse_section(section, std::path::Path::new("i.yaml")).unwrap(),
            std::path::Path::new("i.yaml"),
        )
        .unwrap();
    let outcome = Outcome {
        verdict: Verdict::Failed,
        messages: vec![],
        elapsed: Duration::from_millis(1),
        ran: Ran::Exec {
            first: Invocation::ended("aws iam list-attached-role-policies", 0),
            also: Some(Invocation::ended(
                "aws service-quotas get-service-quota",
                254,
            )),
            request: None,
        },
    };
    let world = serde_json::json!({"cloud": "aws"});
    let report = assemble(
        Assembly {
            catalog: &catalog,
            outcomes: vec![(catalog.rule("iam-role-policy-quota").unwrap(), outcome)],
            confidence: BTreeMap::new(),
            world: &world,
            subject: Subject {
                cloud: "aws".into(),
                release: None,
                source: "c.json".into(),
                name: "fred".into(),
                cluster_uid: None,
            },
            options: options(),
            generated_at: "2026-09-30T00:00:00Z".into(),
        },
        &Jq::new(),
    );
    let check = &report.checks[0];
    assert_eq!(check.answer.as_deref(), Some("exit 0; exit 254"));
    assert_eq!(
        check.ran_lines(),
        [
            "aws iam list-attached-role-policies; aws service-quotas get-service-quota -> exit 0; exit 254"
        ]
    );
    assert!(check.requests.is_empty());
}

#[test]
fn the_reproduce_line_of_a_watch_reads_the_events_route_that_the_watch_reads() {
    let section = r"
section: lifecycle
title: L
rules:
  - id: webhook-call-failed
    title: t
    engines: [all]
    tier: watch
    status: seen
    what: w
    check: {kind: watch, expr: '[]'}
";
    let mut c = crate::rule::Catalog::default();
    c.push(
        crate::rule::Catalog::parse_section(section, std::path::Path::new("l.yaml")).unwrap(),
        std::path::Path::new("l.yaml"),
    )
    .unwrap();
    let mut subject = Subject {
        cloud: "aws".into(),
        release: None,
        source: "cluster:c1".into(),
        name: String::new(),
        cluster_uid: Some("c 1".into()),
    };
    let outcome = Outcome::not_run(Verdict::Failed, "m");
    let rule = c.rule("webhook-call-failed").unwrap();
    let lines = auto_reproduce(rule, &outcome, &subject, None);
    assert_eq!(lines.len(), 1);
    let route = "/v1/events/components/spectrocluster/c%201?limit=50";
    assert!(lines[0].ends_with(&format!("}}{route}\"")), "{}", lines[0]);
    subject.cluster_uid = None;
    assert!(auto_reproduce(rule, &outcome, &subject, None).is_empty());
}

#[test]
fn a_subject_with_no_name_shows_a_dash() {
    let mut subject = Subject {
        cloud: "aws".into(),
        release: None,
        source: "c.json".into(),
        name: String::new(),
        cluster_uid: None,
    };
    assert_eq!(subject.name_or_dash(), "-");
    subject.name = "fred".into();
    assert_eq!(subject.name_or_dash(), "fred");
}

#[test]
fn labels_say_the_level_and_the_severity() {
    assert_eq!(
        check("a", Verdict::Passed, Level::Probe, Severity::Fail, false).label(),
        "PROBE PASS"
    );
    assert_eq!(
        check("a", Verdict::Failed, Level::Probe, Severity::Fail, false).label(),
        "PROBE FAIL"
    );
    assert_eq!(
        check(
            "a",
            Verdict::Passed,
            Level::Preflight,
            Severity::Fail,
            false
        )
        .label(),
        "PASS"
    );
    assert_eq!(
        check("a", Verdict::Failed, Level::Preflight, Severity::Fail, true).label(),
        "FAIL"
    );
    assert_eq!(
        check(
            "a",
            Verdict::Failed,
            Level::Preflight,
            Severity::Warn,
            false
        )
        .label(),
        "WARN"
    );
    assert_eq!(
        check("a", Verdict::Failed, Level::Probe, Severity::Note, false).label(),
        "NOTE"
    );
    assert_eq!(
        check(
            "a",
            Verdict::NotChecked,
            Level::Probe,
            Severity::Fail,
            false
        )
        .label(),
        "NOT CHECKED"
    );
}

#[test]
fn the_exit_code_follows_the_counted_failures_then_the_errors() {
    let probe_fail = vec![
        check(
            "a",
            Verdict::Passed,
            Level::Preflight,
            Severity::Fail,
            false,
        ),
        check("b", Verdict::Failed, Level::Probe, Severity::Fail, false),
    ];
    let s = summarize(&probe_fail, &sections(&probe_fail), &options());
    assert_eq!((s.exit_code, s.probe_failed, s.failed), (0, 1, 0));
    assert!(s.verdict.starts_with("OK"), "{}", s.verdict);

    let counted = vec![check(
        "b",
        Verdict::Failed,
        Level::Preflight,
        Severity::Fail,
        true,
    )];
    let s = summarize(&counted, &sections(&counted), &options());
    assert_eq!(s.exit_code, 1);
    assert!(s.verdict.contains("b"), "{}", s.verdict);

    let error = vec![check(
        "c",
        Verdict::Error,
        Level::Probe,
        Severity::Fail,
        false,
    )];
    assert_eq!(
        summarize(&error, &sections(&error), &options()).exit_code,
        2
    );

    let mut ready = options();
    ready.ready = Some(0.9);
    let half = vec![
        check("a", Verdict::Passed, Level::Probe, Severity::Fail, false),
        check("b", Verdict::Failed, Level::Probe, Severity::Fail, false),
    ];
    let s = summarize(&half, &sections(&half), &ready);
    assert_eq!(s.exit_code, 1);
    assert!(
        s.verdict.starts_with("NOT READY: readiness 50%"),
        "{}",
        s.verdict
    );
}

#[test]
fn the_verdict_and_the_summary_agree_each_noun_with_its_count() {
    let failed = |n: usize| -> Vec<CheckResult> {
        (0..n)
            .map(|i| {
                check(
                    &format!("f{i}"),
                    Verdict::Failed,
                    Level::Preflight,
                    Severity::Fail,
                    true,
                )
            })
            .collect()
    };
    for (n, words) in [(1, "1 check failed: f0"), (2, "2 checks failed: f0, f1")] {
        let checks = failed(n);
        let s = summarize(&checks, &sections(&checks), &options());
        assert_eq!(s.verdict, format!("FAIL: {words}"));
    }
    let errors = |n: usize| -> Vec<CheckResult> {
        (0..n)
            .map(|i| {
                check(
                    &format!("e{i}"),
                    Verdict::Error,
                    Level::Probe,
                    Severity::Fail,
                    false,
                )
            })
            .collect()
    };
    for (n, words) in [
        (1, "1 check could not answer: e0"),
        (2, "2 checks could not answer: e0, e1"),
    ] {
        let checks = errors(n);
        let s = summarize(&checks, &sections(&checks), &options());
        assert_eq!(s.verdict, format!("ERROR: {words}"));
    }
    let probes = |n: usize| -> Summary {
        let checks: Vec<CheckResult> = (0..n)
            .map(|i| {
                check(
                    &format!("p{i}"),
                    Verdict::Failed,
                    Level::Probe,
                    Severity::Fail,
                    false,
                )
            })
            .collect();
        summarize(&checks, &sections(&checks), &options())
    };
    assert_eq!(
        probes(1).verdict,
        "OK: each counted check passed; 1 probe failed, not counted"
    );
    assert_eq!(
        probes(2).verdict,
        "OK: each counted check passed; 2 probes failed, not counted"
    );
}

#[test]
fn the_checks_that_apply_are_the_checks_that_ran_and_the_checks_that_did_not_run() {
    let checks = vec![
        check("a", Verdict::Passed, Level::Probe, Severity::Fail, false),
        check("b", Verdict::Failed, Level::Probe, Severity::Fail, false),
        check("c", Verdict::NotRun, Level::Probe, Severity::Fail, false),
        check("d", Verdict::Skipped, Level::Probe, Severity::Fail, false),
    ];
    let sections = sections(&checks);
    let s = summarize(&checks, &sections, &options());
    assert_eq!((s.ran, s.not_run, s.applies()), (2, 1, 3));
    assert_eq!(s.readiness, Some(1.0 / 3.0));
}

#[test]
fn the_terminal_and_the_markdown_carry_each_result() {
    let outcome = |v: Verdict| Outcome {
        verdict: v,
        messages: vec!["pool worker has size 4".into()],
        elapsed: Duration::from_millis(3),
        ran: Ran::Nothing,
    };
    let section = r#"
section: compute
title: Compute and pools
rules:
  - id: gke-pool-multiple-3
    title: A GKE worker pool size is a multiple of 3
    engines: [gke]
    tier: static
    status: seen
    what: w
    check: {kind: static, expr: "true"}
    reproduce: ["echo {{ .cloud }}"]
  - id: gke-no-gpu
    title: GPU types are not supported on GKE
    engines: [gke]
    tier: static
    status: source
    what: w
"#;
    let mut catalog = Catalog::default();
    catalog
        .push(
            Catalog::parse_section(section, std::path::Path::new("c.yaml")).unwrap(),
            std::path::Path::new("c.yaml"),
        )
        .unwrap();
    let rules: Vec<&Rule> = catalog.rules().collect();
    let world = serde_json::json!({"cloud": "gke"});
    let report = assemble(
        Assembly {
            catalog: &catalog,
            outcomes: vec![
                (rules[0], outcome(Verdict::Failed)),
                (rules[1], Outcome::not_run(Verdict::NotChecked, "no check")),
            ],
            confidence: BTreeMap::new(),
            world: &world,
            subject: Subject {
                cloud: "gke".into(),
                release: Some("4.10".into()),
                source: "c.json".into(),
                name: "fred".into(),
                cluster_uid: None,
            },
            options: Options {
                out_dir: Some("runs/x".into()),
                ..options()
            },
            generated_at: "2026-09-30T00:00:00Z".into(),
        },
        &Jq::new(),
    );
    assert_eq!(
        report.checks[0].reproduce[0], "echo gke",
        "the template is filled"
    );
    assert!(
        report.checks[0].reproduce[1].starts_with("jq 'true' runs/x/world.json"),
        "{:?}",
        report.checks[0].reproduce
    );
    assert_eq!(report.summary.exit_code, 0, "a probe");
    assert_eq!(report.sections[0].not_checked, 1);
    let text = terminal(&report, &Paint::off());
    assert!(text.contains("PROBE FAIL    gke-pool-multiple-3"), "{text}");
    assert!(text.contains("pool worker has size 4"), "{text}");
    assert!(text.contains("NOT CHECKED   gke-no-gpu"), "{text}");
    assert!(text.contains("Section scorecard"), "{text}");
    assert!(
        text.contains("1 probe failed, no warnings, no notes"),
        "{text}"
    );
    assert!(
        text.contains("no errors; readiness 0%: 0 of 1 check passed"),
        "{text}"
    );
    let md = markdown(&report);
    assert!(
        md.contains("| PROBE FAIL | `gke-pool-multiple-3` | S |"),
        "{md}"
    );
    assert!(md.contains("Reproduce by hand"), "{md}");
    assert!(md.contains("1 probe failed; "), "{md}");
    assert!(md.contains("(0 of 1 check passed)"), "{md}");
    let json: Value =
        serde_json::from_str(&render(&report, Format::Json, true, &Paint::off())).unwrap();
    assert_eq!(json["schema"], REPORT_SCHEMA);
    assert_eq!(json["checks"][0]["verdict"], "failed");
}

/// The report of a run with one rule, `edge-host-in-use`, that gave `outcome`.
fn report_of_edge_host_in_use(each: Option<&str>, path: &str, outcome: Outcome) -> Report {
    let each = each.map_or(String::new(), |each| format!("      each: '{each}'\n"));
    let section = format!(
        r#"
section: compute
title: Compute and pools
rules:
  - id: edge-host-in-use
    title: Each edge host is free
    engines: [edge]
    tier: palette
    status: source
    what: w
    check:
      kind: palette
{each}      path: {path}
      expr: 'true'
"#
    );
    let mut catalog = Catalog::default();
    catalog
        .push(
            Catalog::parse_section(&section, std::path::Path::new("c.yaml")).unwrap(),
            std::path::Path::new("c.yaml"),
        )
        .unwrap();
    let world = serde_json::json!({"cloud": "edge"});
    assemble(
        Assembly {
            catalog: &catalog,
            outcomes: vec![(catalog.rule("edge-host-in-use").unwrap(), outcome)],
            confidence: BTreeMap::new(),
            world: &world,
            subject: Subject {
                cloud: "edge".into(),
                release: None,
                source: "c.json".into(),
                name: "fred".into(),
                cluster_uid: None,
            },
            options: options(),
            generated_at: "2026-09-30T00:00:00Z".into(),
        },
        &Jq::new(),
    )
}

#[test]
fn a_check_with_each_keeps_each_request_and_prints_a_line_and_a_curl_for_each() {
    let outcome = Outcome {
        verdict: Verdict::Failed,
        messages: vec!["edge host h2 is in use".into()],
        elapsed: Duration::from_millis(3),
        ran: Ran::Requests(vec![
            Call::get("/v1/edgehosts/h1", Some("HTTP 200")),
            Call::get("/v1/edgehosts/h2", Some("HTTP 404")),
        ]),
    };
    let report =
        report_of_edge_host_in_use(Some("[.hosts[]]"), "/v1/edgehosts/{{ $item }}", outcome);
    let check = &report.checks[0];
    assert_eq!(check.ran, None);
    assert_eq!(check.requests.len(), 2);
    let curls: Vec<&String> = check
        .reproduce
        .iter()
        .filter(|l| l.starts_with("curl "))
        .collect();
    assert_eq!(curls.len(), 2, "{:?}", check.reproduce);
    assert!(curls[1].ends_with("/v1/edgehosts/h2\""), "{}", curls[1]);
    let text = render(&report, Format::Terminal, false, &Paint::off());
    assert!(
        text.contains("ran: GET /v1/edgehosts/h1 -> HTTP 200\n")
            && text.contains("ran: GET /v1/edgehosts/h2 -> HTTP 404\n"),
        "{text}"
    );
    let json: serde_json::Value =
        serde_json::from_str(&render(&report, Format::Json, true, &Paint::off())).unwrap();
    assert_eq!(
        json["checks"][0]["requests"][1],
        serde_json::json!({"method": "GET", "path": "/v1/edgehosts/h2", "answer": "HTTP 404"})
    );
    assert!(json["checks"][0].get("ran").is_none());
}

#[test]
fn a_check_with_each_and_its_own_reproduce_lines_has_no_second_line_for_a_request() {
    let catalog = crate::catalog::builtin().unwrap();
    let rule = catalog.rule("edge-host-in-use").unwrap();
    assert!(
        !rule.reproduce.for_engine("edge").is_empty(),
        "the rule writes its own loop"
    );
    let outcome = Outcome {
        verdict: Verdict::Failed,
        messages: vec![],
        elapsed: Duration::from_millis(3),
        ran: Ran::Requests(vec![
            Call::get("/v1/edgehosts/h1", Some("HTTP 200")),
            Call::get("/v1/edgehosts/h2", Some("HTTP 200")),
        ]),
    };
    let subject = Subject {
        cloud: "edge".into(),
        release: None,
        source: "c.json".into(),
        name: "fred".into(),
        cluster_uid: None,
    };
    assert_eq!(
        auto_reproduce(rule, &outcome, &subject, None),
        Vec::<String>::new()
    );
}

#[test]
fn a_check_with_one_request_keeps_its_ran_and_answer_and_has_no_requests() {
    let outcome = Outcome {
        verdict: Verdict::Failed,
        messages: vec!["edge host h2 is in use".into()],
        elapsed: Duration::from_millis(3),
        ran: Ran::Request(Call::get("/v1/edgehosts/h2", Some("HTTP 404"))),
    };
    let report = report_of_edge_host_in_use(None, "/v1/edgehosts/h2", outcome);
    let json: serde_json::Value =
        serde_json::from_str(&render(&report, Format::Json, true, &Paint::off())).unwrap();
    let check = &json["checks"][0];
    assert_eq!(check["ran"], "GET /v1/edgehosts/h2");
    assert_eq!(check["answer"], "HTTP 404");
    assert!(check.get("requests").is_none());
    let text = render(&report, Format::Terminal, false, &Paint::off());
    assert!(
        text.contains("ran: GET /v1/edgehosts/h2 -> HTTP 404\n"),
        "{text}"
    );
}

/// A noun of the summary agrees with its count, also when the count is 1 or 0 (#589): the rules,
/// the checks, and the verb of the rules.
#[test]
fn the_summary_of_a_run_of_one_rule_agrees_each_noun_with_its_count() {
    let outcome = Outcome {
        verdict: Verdict::Passed,
        messages: vec![],
        elapsed: Duration::from_millis(1),
        ran: Ran::Nothing,
    };
    let report = report_of_edge_host_in_use(None, "/v1/edgehosts/h2", outcome);
    let text = render(&report, Format::Terminal, false, &Paint::off());
    assert!(
        text.contains("\n1 rule applies; 1 ran: 1 passed, "),
        "{text}"
    );
    assert!(text.contains("1 of 1 check passed"), "{text}");
    let markdown = render(&report, Format::Markdown, false, &Paint::off());
    assert!(markdown.contains("(1 of 1 check passed)"), "{markdown}");

    let mut none = report;
    none.summary.rules = 0;
    none.summary.ran = 0;
    none.summary.passed = 0;
    let text = render(&none, Format::Terminal, false, &Paint::off());
    assert!(text.contains("\nno rules apply; 0 ran: "), "{text}");
    assert!(text.contains("0 of 0 checks passed"), "{text}");

    let mut many = none;
    many.summary.rules = 2;
    many.summary.ran = 2;
    many.summary.passed = 2;
    let text = render(&many, Format::Terminal, false, &Paint::off());
    assert!(text.contains("\n2 rules apply; 2 ran: "), "{text}");
    assert!(text.contains("2 of 2 checks passed"), "{text}");
}
