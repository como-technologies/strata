use super::*;
use crate::curl::Saves;
use crate::probe::{Invocation, Ran};
use crate::rule::EvidenceKind;
use answer_tags::{ANSWER_TAGS, assert_only_strata_tags};
use std::time::Duration;
use strata::api::Method;

// The text of an answer with tags of HTML, and the check of the tags, in the file that the tests
// of strata use. A test here uses only those two, so the other checks are dead code here.
#[allow(dead_code)]
#[path = "../../../../tests/common/answer_tags.rs"]
mod answer_tags;

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
        failed_request: None,
    }
}

fn options() -> Options {
    Options {
        tiers: vec![Tier::Static],
        strict: false,
        offline: false,
        ready: None,
        out_dir: None,
        out_world: false,
    }
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
    let s = summarize(&checks, "aws", &options(), false);
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
    assert_eq!(summarize(&none, "aws", &options(), false).readiness, None);
}

#[test]
fn a_note_that_did_not_run_is_outside_the_readiness_as_a_note_that_ran_is() {
    let mut note = check("n", Verdict::NotRun, Level::Probe, Severity::Note, false);
    note.messages = vec!["tier palette was not asked for".into()];
    let checks = vec![
        check("a", Verdict::Passed, Level::Probe, Severity::Fail, false),
        note,
    ];
    let s = summarize(&checks, "aws", &options(), false);
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
    // the body holds the values of the variables, so it goes beside world.json, and not into the
    // current directory; so does the answer, which strata does not read for a value
    assert_eq!(
        lines[0],
        "jq '.account.spec' out/world.json > out/request.json"
    );
    assert!(
        lines[1].contains("--data @out/request.json"),
        "{}",
        lines[1]
    );
    assert!(lines[1].contains(" -o out/body.json "), "{}", lines[1]);
    assert!(
        lines[2].contains("--slurpfile body out/body.json "),
        "{}",
        lines[2]
    );
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

/// Palette can save the address of the console of a pack of virtual machines at `validate/repave`.
/// So the line of the request of a check that names it in `saves` says it, and the line of
/// another request has no comment (#717).
#[test]
fn the_line_of_a_request_at_which_palette_saves_something_says_what_it_saves() {
    let section = r#"
section: kubernetes
title: K
rules:
  - id: engine-repave-rules
    title: t
    engines: [eks]
    tier: palette
    status: seen
    what: w
    check: {kind: palette, method: POST, path: "/v1/spectroclusters/{{ .clusterUid }}/validate/repave", body: "{profiles: .entity.spec.profiles}", saves: console-address, expr: "true"}
  - id: palette-validate
    title: t
    engines: [eks]
    tier: palette
    status: seen
    what: w
    check: {kind: palette, method: POST, path: "/v1/spectroclusters/eks/validate", expr: "true"}
"#;
    let mut c = crate::rule::Catalog::default();
    c.push(
        crate::rule::Catalog::parse_section(section, std::path::Path::new("k.yaml")).unwrap(),
        std::path::Path::new("k.yaml"),
    )
    .unwrap();
    let subject = Subject {
        cloud: "eks".into(),
        release: None,
        source: "c.json".into(),
        name: "fred".into(),
        cluster_uid: Some("u-1".into()),
    };
    let outcome = |path: &str| Outcome {
        verdict: Verdict::Passed,
        messages: vec![],
        elapsed: Duration::from_millis(1),
        ran: Ran::Request(Call {
            method: Method::Post,
            path: path.to_string(),
            answer: None,
        }),
    };
    let repave = c.rule("engine-repave-rules").unwrap();
    let lines = auto_reproduce(
        repave,
        &outcome("/v1/spectroclusters/u-1/validate/repave"),
        &subject,
        Some("out"),
    );
    let comment = format!("\"){}", Saves::ConsoleAddress.comment());
    assert!(lines[1].ends_with(&comment), "{}", lines[1]);
    let validate = c.rule("palette-validate").unwrap();
    let lines = auto_reproduce(
        validate,
        &outcome("/v1/spectroclusters/eks/validate"),
        &subject,
        Some("out"),
    );
    assert!(
        lines[0].ends_with("/v1/spectroclusters/eks/validate\")"),
        "{}",
        lines[0]
    );
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
        lines[0].starts_with(&format!(
            "code=$({} -o out/body.json",
            crate::curl::palette_curl()
        )),
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
        lines[2].contains("--slurpfile body out/body.json"),
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
            cluster_gone: false,
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
    let s = summarize(&probe_fail, "aws", &options(), false);
    assert_eq!((s.exit_code, s.probe_failed, s.failed), (0, 1, 0));
    assert!(s.verdict.starts_with("OK"), "{}", s.verdict);

    let counted = vec![check(
        "b",
        Verdict::Failed,
        Level::Preflight,
        Severity::Fail,
        true,
    )];
    let s = summarize(&counted, "aws", &options(), false);
    assert_eq!(s.exit_code, 1);
    assert!(s.verdict.contains("b"), "{}", s.verdict);

    let error = vec![check(
        "c",
        Verdict::Error,
        Level::Probe,
        Severity::Fail,
        false,
    )];
    assert_eq!(summarize(&error, "aws", &options(), false).exit_code, 2);

    let mut ready = options();
    ready.ready = Some(0.9);
    let half = vec![
        check("a", Verdict::Passed, Level::Probe, Severity::Fail, false),
        check("b", Verdict::Failed, Level::Probe, Severity::Fail, false),
    ];
    let s = summarize(&half, "aws", &ready, false);
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
        let s = summarize(&checks, "aws", &options(), false);
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
        let s = summarize(&checks, "aws", &options(), false);
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
        summarize(&checks, "aws", &options(), false)
    };
    assert_eq!(
        probes(1).verdict,
        "NOT COUNTED: no check counted on aws; 1 probe failed, not counted"
    );
    assert_eq!(
        probes(2).verdict,
        "NOT COUNTED: no check counted on aws; 2 probes failed, not counted"
    );
}

#[test]
fn the_verdict_of_a_run_that_passed_says_that_the_cluster_is_gone_when_a_watch_ended_so() {
    let verdict = |checks: &[CheckResult], gone: bool| {
        let s = summarize(checks, "aws", &options(), gone);
        (s.exit_code, s.verdict)
    };
    let passed = vec![check(
        "a",
        Verdict::Passed,
        Level::Preflight,
        Severity::Fail,
        false,
    )];
    assert_eq!(
        verdict(&passed, false),
        (0, "OK: each counted check passed".to_string())
    );
    assert_eq!(
        verdict(&passed, true),
        (
            0,
            "OK: the cluster is gone; no counted check failed".to_string()
        )
    );
    let probe = vec![
        check(
            "a",
            Verdict::Passed,
            Level::Preflight,
            Severity::Fail,
            false,
        ),
        check("b", Verdict::Failed, Level::Probe, Severity::Fail, false),
    ];
    assert_eq!(
        verdict(&probe, true),
        (
            0,
            "OK: the cluster is gone; no counted check failed; 1 probe failed, not counted"
                .to_string()
        )
    );
    assert_eq!(
        verdict(&[], false),
        (0, "NOT COUNTED: no check ran on aws".to_string())
    );
    assert_eq!(
        verdict(&[], true),
        (
            0,
            "NOT COUNTED: the cluster is gone; no check ran on aws".to_string()
        )
    );
    let probe_passed = vec![check(
        "a",
        Verdict::Passed,
        Level::Probe,
        Severity::Fail,
        false,
    )];
    assert_eq!(
        verdict(&probe_passed, true),
        (
            0,
            "NOT COUNTED: the cluster is gone; no check counted on aws".to_string()
        )
    );
    // a failure, an error, and a readiness under the bar keep their words
    let counted = vec![check(
        "b",
        Verdict::Failed,
        Level::Preflight,
        Severity::Fail,
        true,
    )];
    assert_eq!(verdict(&counted, true), verdict(&counted, false));
    assert_eq!(verdict(&counted, true).1, "FAIL: 1 check failed: b");
    let error = vec![check(
        "c",
        Verdict::Error,
        Level::Probe,
        Severity::Fail,
        false,
    )];
    assert_eq!(verdict(&error, true), verdict(&error, false));
    assert_eq!(verdict(&error, true).0, 2);
}

/// F-028: only a counted check can fail a run. A run in which none ran says so, and not `OK`.
#[test]
fn a_run_with_no_counted_check_is_not_ok_and_says_that_no_check_counted_on_the_engine() {
    let run = |checks: &[CheckResult], strict: bool| {
        let options = Options {
            strict,
            ..options()
        };
        summarize(checks, "aws", &options, false)
    };
    let probe = check("p", Verdict::Passed, Level::Probe, Severity::Fail, false);
    let warn = check(
        "w",
        Verdict::Passed,
        Level::Preflight,
        Severity::Warn,
        false,
    );
    let preflight = check(
        "c",
        Verdict::Passed,
        Level::Preflight,
        Severity::Fail,
        false,
    );
    // a probe and a warning never count without --strict, so no check counted
    let s = run(&[probe.clone(), warn.clone()], false);
    assert_eq!(
        (s.exit_code, s.counted, s.verdict.as_str()),
        (0, 0, "NOT COUNTED: no check counted on aws")
    );
    assert!(s.not_counted());
    // with --strict, the probe counts; a warning counts in no case
    let s = run(&[probe.clone(), warn.clone()], true);
    assert_eq!(
        (s.counted, s.verdict.as_str()),
        (1, "OK: each counted check passed")
    );
    assert!(!s.not_counted());
    assert_eq!(run(&[warn], true).counted, 0);
    // a preflight check of severity fail counts with no flag
    let s = run(&[probe, preflight], false);
    assert_eq!(
        (s.counted, s.verdict.as_str()),
        (1, "OK: each counted check passed")
    );
    // a check that did not run is no counted check
    let mut not_run = check(
        "n",
        Verdict::NotRun,
        Level::Preflight,
        Severity::Fail,
        false,
    );
    not_run.messages = vec!["tier palette was not asked for".into()];
    assert_eq!(run(&[not_run], false).counted, 0);
}

/// F-029: a warning is no failure. The scorecard and the summary count each result in one class, so
/// the counts of the sections add to the counts of the summary.
#[test]
fn the_summary_and_the_scorecard_give_one_count_of_failures_and_a_warning_is_no_failure() {
    let section = r#"
section: profiles
title: Profiles and packs
rules:
  - id: a-probe
    title: t
    engines: [all]
    tier: static
    status: seen
    what: w
    check: {kind: static, expr: "false"}
  - id: a-warning
    title: t
    engines: [all]
    tier: static
    severity: warn
    status: seen
    what: w
    check: {kind: static, expr: "false"}
  - id: a-pass
    title: t
    engines: [all]
    tier: static
    status: seen
    what: w
    check: {kind: static, expr: "true"}
"#;
    let mut catalog = Catalog::default();
    catalog
        .push(
            Catalog::parse_section(section, std::path::Path::new("p.yaml")).unwrap(),
            std::path::Path::new("p.yaml"),
        )
        .unwrap();
    let rules: Vec<&Rule> = catalog.rules().collect();
    let outcome = |verdict: Verdict| Outcome {
        verdict,
        messages: vec!["why".into()],
        elapsed: Duration::from_millis(1),
        ran: Ran::Nothing,
    };
    let world = serde_json::json!({"cloud": "aws"});
    let report = assemble(
        Assembly {
            catalog: &catalog,
            outcomes: vec![
                (rules[0], outcome(Verdict::Failed)),
                (rules[1], outcome(Verdict::Failed)),
                (rules[2], outcome(Verdict::Passed)),
            ],
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
            cluster_gone: false,
            generated_at: "t".into(),
        },
        &Jq::new(),
    );
    let s = &report.summary;
    let sc = &report.sections[0];
    assert_eq!(
        (s.failed, s.probe_failed, s.warned, s.passed, s.ran),
        (0, 1, 1, 1, 3)
    );
    assert_eq!(
        (sc.failed, sc.probe_failed, sc.warned, sc.passed, sc.ran),
        (s.failed, s.probe_failed, s.warned, s.passed, s.ran),
        "the scorecard counts as the summary does"
    );
    // each form shows the same counts, and the scorecard has a column for each
    let text = terminal(&report, &Paint::off());
    assert!(text.contains("fail probe fail warn"), "{text}");
    let row = text
        .lines()
        .find(|l| l.starts_with("  Profiles and packs"))
        .unwrap();
    let cells: Vec<&str> = row.split_whitespace().skip(3).collect();
    // rows seen check preflight runs weakest(no runs) ran pass fail probe-fail warn not-run ...
    assert_eq!(
        &cells[..12],
        &[
            "3", "3", "3", "0", "0", "no", "runs", "3", "1", "0", "1", "1"
        ],
        "{row}"
    );
    assert!(
        text.contains("3 ran: 1 passed, 0 failed, 1 probe failed, 1 warning"),
        "{text}"
    );
    let md = markdown(&report);
    assert!(md.contains("| failed | probes failed | warnings |"), "{md}");
    assert!(
        md.contains("3 ran: 1 passed, 0 failed, 1 probe failed, 1 warning;"),
        "{md}"
    );
    let html = crate::html::page(&report);
    for tile in [
        "<b>0</b><span>failed</span>",
        "<b>1</b><span>probes failed</span>",
        "<b>1</b><span>warnings</span>",
    ] {
        assert!(html.contains(tile), "{tile}");
    }
}

/// F-026: the readiness counts the checks of the tiers that ran, so each form names the tiers
/// beside the number.
#[test]
fn the_readiness_states_the_tiers_beside_the_number_in_each_form() {
    let mut report = assemble(
        Assembly {
            catalog: &Catalog::default(),
            outcomes: vec![],
            confidence: BTreeMap::new(),
            world: &serde_json::json!({}),
            subject: Subject {
                cloud: "aws".into(),
                release: None,
                source: "c.json".into(),
                name: "fred".into(),
                cluster_uid: None,
            },
            options: Options {
                tiers: vec![Tier::Static, Tier::Palette],
                ready: Some(0.8),
                ..options()
            },
            cluster_gone: false,
            generated_at: "t".into(),
        },
        &Jq::new(),
    );
    report.summary.readiness = Some(0.38);
    let text = terminal(&report, &Paint::off());
    assert!(
        text.contains("readiness 38% (tiers static, palette): 0 of 0 checks passed"),
        "{text}"
    );
    assert!(
        markdown(&report)
            .contains("readiness **38%** (tiers static, palette): 0 of 0 checks passed"),
        "the markdown"
    );
    assert!(
        crate::html::page(&report)
            .contains("<b>38%</b><span>readiness (tiers static, palette)</span>"),
        "the page"
    );
    // the verdict of a readiness under the gate names the tiers too
    assert_eq!(
        report.summary.verdict,
        "NOT READY: readiness none (tiers static, palette) is under 80%"
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
    let s = summarize(&checks, "aws", &options(), false);
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
                out_world: true,
                ..options()
            },
            cluster_gone: false,
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
        text.contains("no errors; readiness 0% (tiers static): 0 of 1 check passed"),
        "{text}"
    );
    let md = markdown(&report);
    assert!(
        md.contains("| PROBE FAIL | `gke-pool-multiple-3` | S |"),
        "{md}"
    );
    assert!(md.contains("Reproduce by hand"), "{md}");
    assert!(md.contains("1 probe failed, no warnings; "), "{md}");
    assert!(
        md.contains("readiness **0%** (tiers static): 0 of 1 check passed"),
        "{md}"
    );
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
            cluster_gone: false,
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
        .filter(|l| l.starts_with(&crate::curl::palette_curl()))
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

/// The criterion of #832: a message from an answer shows each tag of HTML as text in the markdown,
/// in the row of a check that failed and in the line of the checks that did not run.
#[test]
fn the_markdown_shows_each_tag_of_a_message_from_an_answer_as_text() {
    let message = format!("HTTP 403: {ANSWER_TAGS}");
    let escaped = format!("HTTP 403: {}", strata::text::html_text(ANSWER_TAGS));
    let failed = Outcome {
        verdict: Verdict::Failed,
        messages: vec![message.clone()],
        elapsed: Duration::from_millis(3),
        ran: Ran::Request(Call::get("/v1/edgehosts/h2", Some("HTTP 403"))),
    };
    let not_run = Outcome::not_run(Verdict::NotRun, message);
    for (outcome, line) in [
        (failed, format!("<br>{escaped} |")),
        (not_run, format!("\n1 check did not run: {escaped} (1)\n")),
    ] {
        let md = markdown(&report_of_edge_host_in_use(
            None,
            "/v1/edgehosts/h2",
            outcome,
        ));
        assert!(md.contains(&line), "{line:?} in\n{md}");
        assert_only_strata_tags(&md);
    }
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
    assert!(markdown.contains(": 1 of 1 check passed."), "{markdown}");

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

/// `world.json` and `entity.json` hold the values of the variables, so `--out` alone writes the
/// reports and no world. The reproduce lines then name `$OUT`, and the line of the files says how
/// to write the world (#614).
#[test]
fn the_report_names_the_folder_of_the_world_only_when_world_wrote_it_there() {
    let reports = Options {
        out_dir: Some("out".into()),
        ..options()
    };
    assert_eq!(reports.world_dir(), None);
    let text = scorecard::written_text(&reports);
    assert!(
        text.starts_with("Written to out: report.html, report.md, report.json. "),
        "{text}"
    );
    assert!(
        text.contains("world.json and entity.json there ($OUT): run again with --world"),
        "{text}"
    );
    let world = Options {
        out_world: true,
        ..reports.clone()
    };
    assert_eq!(world.world_dir(), Some("out"));
    assert!(
        scorecard::written_text(&world)
            .contains("report.json, world.json, entity.json. The last two hold the values"),
        "{}",
        scorecard::written_text(&world)
    );
    // with no --out, nothing is on disk
    let nowhere = Options {
        out_world: true,
        ..options()
    };
    assert_eq!(nowhere.world_dir(), None);
    assert!(scorecard::written_text(&nowhere).starts_with("No file written; "));
}
