use super::*;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.005
}

#[test]
fn the_bound_with_no_wrong_result_is_the_table_of_the_issue() {
    for (n, want) in [
        (3, 0.37),
        (5, 0.55),
        (10, 0.74),
        (14, 0.81),
        (20, 0.86),
        (30, 0.90),
    ] {
        let b = lower_bound(n, n, 0.95).unwrap();
        assert!(close(b, want), "{n} of {n}: {b}");
    }
    assert_eq!(lower_bound(0, 0, 0.95), None);
    assert_eq!(lower_bound(0, 5, 0.95), Some(0.0));
}

#[test]
fn the_fewest_correct_runs_that_reach_a_bar_come_from_the_bar() {
    assert_eq!(runs_to_reach(crate::rule::DEFAULT_BAR), 14);
    assert_eq!(runs_to_reach(0.9), 29);
}

#[test]
fn the_bound_with_a_miss_uses_the_beta_quantile() {
    // 13 of 14: the one-sided 95% Clopper-Pearson lower bound is 0.703, under the bar of 0.8
    let b = lower_bound(13, 14, 0.95).unwrap();
    assert!(close(b, 0.703), "{b}");
    // 9 of 10 is 0.606
    let b = lower_bound(9, 10, 0.95).unwrap();
    assert!(close(b, 0.606), "{b}");
    // 19 of 20 is about 0.78, so one miss in 20 runs is under the bar
    let b = lower_bound(19, 20, 0.95).unwrap();
    assert!(close(b, 0.784), "{b}");
    // 1 of 2
    let b = lower_bound(1, 2, 0.95).unwrap();
    assert!(close(b, 0.0253), "{b}");
}

#[test]
fn the_regularized_beta_matches_known_values() {
    assert!(close(regularized_beta(0.5, 1.0, 1.0), 0.5));
    assert!(close(regularized_beta(0.5, 2.0, 2.0), 0.5));
    assert!(close(regularized_beta(0.2, 2.0, 3.0), 0.1808));
}

fn ledger(text: &str) -> Ledger {
    Ledger::parse(text, Path::new("runs.yaml")).unwrap()
}

const RUNS: &str = r#"
schema: strata-preflight-ledger/1
runs:
  - run: r1
    engine: gke
    release: "4.10"
    results:
      - {rule: gke-pool-multiple-3, result: caught}
      - {rule: gke-api-off, result: caught}
  - run: r2
    engine: gke
    release: "4.10.3"
    results:
      - {rule: gke-pool-multiple-3, result: passed}
      - {rule: gke-api-off, result: missed}
      - {rule: gke-no-gpu, result: caught}
      - {rule: gke-no-gpu, result: passed}
"#;

#[test]
fn the_counts_join_the_patches_of_one_release() {
    let l = ledger(RUNS);
    let c = l.counts_for("gke-pool-multiple-3", "gke", Some("4.10"));
    assert_eq!((c.caught, c.passed, c.missed), (1, 1, 0));
    assert_eq!(c.runs, vec!["r1", "r2"]);
    let c = l.counts_for("gke-pool-multiple-3", "gke", Some("4.11"));
    assert_eq!(c.caught + c.passed, 0, "another release has no run");
    assert_eq!(
        c.older,
        vec!["r1", "r2"],
        "the runs of 4.10 are older than 4.11"
    );
    let c = l.counts_for("gke-pool-multiple-3", "eks", None);
    assert_eq!(c.caught + c.passed, 0, "another engine has no run");
}

#[test]
fn a_release_is_its_first_two_numbers_as_version_reads_them() {
    for (a, b) in [
        ("4.10.3", "4.10"),
        ("4", "4.0"),
        ("4.01", "4.1"),
        ("v4.10", "4.10.0"),
        ("4.10.3-rc.1", "4.10"),
    ] {
        assert_eq!(standing(a, b), Standing::Same, "{a} is the release {b}");
        assert_eq!(standing(b, a), Standing::Same, "{b} is the release {a}");
    }
}

#[test]
fn the_order_of_two_releases_is_the_order_of_version_and_not_of_the_text() {
    for (run, want, standing_of_run) in [
        ("4.10", "4.1", Standing::Later),
        ("4.9", "4.10", Standing::Older),
        ("4.10.3", "4.11", Standing::Older),
        ("4.11.0-rc.1", "4.10", Standing::Later),
        ("4", "4.1", Standing::Older),
        ("5", "4.10", Standing::Later),
    ] {
        assert_eq!(standing(run, want), standing_of_run, "{run} against {want}");
    }
}

#[test]
fn a_text_that_is_no_version_is_the_release_of_the_same_text_only() {
    assert_eq!(standing("dev", "dev"), Standing::Same);
    for (run, want) in [
        ("beta", "dev"),
        ("dev", "4.10"),
        ("4.10", "dev"),
        ("4.10.3.1", "4.10"),
    ] {
        assert_eq!(standing(run, want), Standing::Older, "{run} against {want}");
    }
}

#[test]
fn a_run_with_no_release_is_a_run_of_an_older_release() {
    let l = ledger(
        r#"
schema: strata-preflight-ledger/1
runs:
  - {run: r1, engine: gke, release: "", results: [{rule: gke-api-off, result: caught}]}
  - {run: r2, engine: gke, release: "4.9", results: [{rule: gke-api-off, result: caught}]}
  - {run: r3, engine: gke, release: "4.10", results: [{rule: gke-api-off, result: passed}, {rule: gke-api-off, result: passed}]}
  - {run: r4, engine: gke, release: "4.11", results: [{rule: gke-api-off, result: caught}]}
"#,
    );
    let c = l.counts_for("gke-api-off", "gke", Some("4.10"));
    assert_eq!((c.caught, c.passed), (0, 2), "only the runs of 4.10 count");
    assert_eq!(c.runs, vec!["r3"]);
    assert_eq!(
        c.older,
        vec!["r1", "r2"],
        "the run with no release and the run of 4.9 are older; the run of 4.11 is not"
    );
    let c = l.counts_for("gke-api-off", "gke", None);
    assert_eq!(
        (c.caught, c.passed),
        (3, 2),
        "with no release, each run counts"
    );
    assert!(c.older.is_empty(), "with no release, no run is older");
}

#[test]
fn a_run_of_another_release_text_does_not_count() {
    let l = ledger(
        r#"
schema: strata-preflight-ledger/1
runs:
  - {run: r1, engine: gke, release: beta, results: [{rule: gke-api-off, result: caught}]}
  - {run: r2, engine: gke, release: "4", results: [{rule: gke-api-off, result: caught}]}
"#,
    );
    let c = l.counts_for("gke-api-off", "gke", Some("dev"));
    assert_eq!(c.caught, 0, "a run at beta is no run at dev");
    let c = l.counts_for("gke-api-off", "gke", Some("4.0"));
    assert_eq!(c.runs, vec!["r2"], "the release 4 is the release 4.0");
}

#[test]
fn confidence_reads_the_bar() {
    let c = Counts {
        caught: 14,
        missed: 0,
        passed: 14,
        false_fail: 0,
        runs: vec![],
        older: vec![],
    };
    let conf = Confidence::of("gke", "4.10", &c, 0.8);
    assert!(conf.meets_bar);
    assert_eq!(conf.catch.text(), "14/14 (81%)");
    let c = Counts {
        caught: 14,
        missed: 0,
        passed: 13,
        false_fail: 1,
        runs: vec![],
        older: vec![],
    };
    assert!(!Confidence::of("gke", "4.10", &c, 0.8).meets_bar);
    let none = Confidence::of("gke", "4.10", &Counts::default(), 0.8);
    assert_eq!(none.catch.text(), NO_RUNS);
    assert_eq!(none.weakest_bound(), None);
}

#[test]
fn a_ledger_of_another_schema_is_refused() {
    let err = Ledger::parse("schema: other/1\nruns: []\n", Path::new("l.yaml")).unwrap_err();
    assert!(err.to_string().contains("schema is 'other/1'"), "{err}");
}

#[test]
fn a_promotion_counts_the_clouds_a_rule_names_and_not_the_lab() {
    let section = r#"
section: tools
title: T
rules:
  - id: cloud-cli-auth
    title: t
    engines: [aws, lab]
    tier: cloud
    status: seen
    what: w
    check: {kind: exec, argv: [aws], expr: "true"}
"#;
    let mut catalog = Catalog::default();
    catalog
        .push(
            Catalog::parse_section(section, Path::new("t.yaml")).unwrap(),
            Path::new("t.yaml"),
        )
        .unwrap();
    let mut runs = String::from("schema: strata-preflight-ledger/1\nruns:\n");
    for i in 0..14 {
        runs.push_str(&format!(
            "  - run: r{i}\n    engine: aws\n    release: \"\"\n    results:\n      - {{rule: cloud-cli-auth, result: caught, note: f}}\n      - {{rule: cloud-cli-auth, result: passed, note: c}}\n"
        ));
    }
    let report = promotion(&catalog, &ledger(&runs), None);
    let rule = report
        .rules
        .iter()
        .find(|r| r.id == "cloud-cli-auth")
        .unwrap();
    assert_eq!(
        rule.earned,
        Level::Preflight,
        "14 correct runs of each kind on aws, and no run of the lab is needed"
    );
    assert_eq!(rule.advice, "promote");
}

#[test]
fn promotion_compares_the_level_with_the_ledger() {
    let section = r#"
section: compute
title: Compute
rules:
  - id: gke-pool-multiple-3
    title: t
    engines: [gke]
    tier: static
    status: seen
    what: w
    check: {kind: static, expr: "true"}
  - id: gke-api-off
    title: t
    engines: [gke]
    tier: cloud
    status: seen
    what: w
    check: {kind: exec, argv: [gcloud], expr: "true"}
  - id: gke-no-gpu
    title: t
    engines: [gke]
    tier: static
    status: source
    what: w
    check: {kind: static, expr: "true"}
"#;
    let mut catalog = Catalog::default();
    catalog
        .push(
            Catalog::parse_section(section, Path::new("c.yaml")).unwrap(),
            Path::new("c.yaml"),
        )
        .unwrap();
    let report = promotion(&catalog, &ledger(RUNS), None);
    let pool = report
        .rules
        .iter()
        .find(|r| r.id == "gke-pool-multiple-3")
        .unwrap();
    assert_eq!(
        pool.earned,
        Level::Preflight,
        "a static rule with one fault and one clean run is proven"
    );
    assert_eq!(pool.advice, "promote");
    let api = report.rules.iter().find(|r| r.id == "gke-api-off").unwrap();
    assert_eq!(api.earned, Level::Probe, "a miss, and no clean run");
    assert_eq!(api.advice, "");
    let gpu = report.rules.iter().find(|r| r.id == "gke-no-gpu").unwrap();
    assert_eq!(
        gpu.earned,
        Level::Probe,
        "the runs count, and no live run shows the fault: status source"
    );
    assert_eq!(report.disagreements, 1);
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn confidence_says_how_many_runs_of_older_releases_it_leaves_out() {
    let none = Counts {
        older: vec!["r1".into(), "r2".into(), "r3".into()],
        ..Counts::default()
    };
    let conf = Confidence::of("gke", "4.11", &none, 0.8);
    assert_eq!(conf.older_runs, 3);
    assert!(
        !conf.has_runs(),
        "the older runs are no run of this release"
    );
    assert_eq!(conf.text(), "no runs, older runs 3");
    let some = Counts {
        caught: 1,
        passed: 1,
        runs: vec!["r4".into()],
        ..none
    };
    assert_eq!(
        Confidence::of("gke", "4.11", &some, 0.8).text(),
        "catch 1/1 (5%), clean 1/1 (5%), older runs 3"
    );
    let all = Counts {
        caught: 1,
        ..Counts::default()
    };
    assert_eq!(
        Confidence::of("gke", "", &all, 0.8).text(),
        "catch 1/1 (5%), clean no runs",
        "with no older run, the text is the text of the rates"
    );
}

const LEVEL: &str = r#"
section: kubernetes
title: Kubernetes
rules:
  - id: gke-no-release-channel
    title: t
    engines: [gke]
    tier: static
    level: preflight
    promotion: {bar: 0.8, runs: [r1]}
    status: seen
    what: w
    check: {kind: static, expr: "true"}
  - id: gke-old-fault
    title: t
    engines: [gke]
    tier: static
    status: seen
    releases: "<4.11"
    what: w
    check: {kind: static, expr: "true"}
"#;

fn level_catalog() -> Catalog {
    let mut catalog = Catalog::default();
    catalog
        .push(
            Catalog::parse_section(LEVEL, Path::new("k.yaml")).unwrap(),
            Path::new("k.yaml"),
        )
        .unwrap();
    catalog
}

const RUNS_OF_4_10: &str = r#"
schema: strata-preflight-ledger/1
runs:
  - {run: r1, engine: gke, release: "4.10", results: [{rule: gke-no-release-channel, result: caught}]}
  - {run: r2, engine: gke, release: "4.10.3", results: [{rule: gke-no-release-channel, result: passed}]}
"#;

const RUNS_OF_4_11: &str = r#"
  - {run: r3, engine: gke, release: "4.11", results: [{rule: gke-no-release-channel, result: caught}]}
  - {run: r4, engine: gke, release: "4.11.1", results: [{rule: gke-no-release-channel, result: passed}]}
"#;

fn entry<'a>(report: &'a PromotionReport, id: &str) -> &'a PromotionEntry {
    report.rules.iter().find(|r| r.id == id).unwrap()
}

#[test]
fn a_rule_promoted_on_one_release_is_a_probe_on_the_next_until_a_run_proves_it() {
    let catalog = level_catalog();
    let id = "gke-no-release-channel";

    // the runs are of 4.10 alone: the rule is a preflight check on 4.10
    let runs = ledger(RUNS_OF_4_10);
    let report = promotion(&catalog, &runs, Some("4.10"));
    let a = entry(&report, id);
    assert_eq!((a.earned, a.advice.as_str()), (Level::Preflight, ""));
    assert_eq!(a.confidence[0].older_runs, 0);
    assert_eq!(report.disagreements, 0);
    assert_eq!(report.exit_code(), 0);

    // the same ledger on 4.11: no run proves the rule, so it is a probe, and the level demotes
    let report = promotion(&catalog, &runs, Some("4.11"));
    let b = entry(&report, id);
    assert_eq!((b.earned, b.advice.as_str()), (Level::Probe, "demote"));
    assert_eq!(b.confidence.len(), 1);
    assert!(!b.confidence[0].has_runs());
    assert_eq!(b.confidence[0].older_runs, 2, "the two runs of 4.10");
    assert_eq!(b.confidence[0].release, "4.11");
    assert_eq!((report.disagreements, report.exit_code()), (1, 1));
    assert_eq!(report.release.as_deref(), Some("4.11"));

    // the runs on 4.11 make it a preflight check again, and the older runs stay in the count
    let runs = ledger(&format!("{RUNS_OF_4_10}{RUNS_OF_4_11}"));
    let report = promotion(&catalog, &runs, Some("4.11"));
    let c = entry(&report, id);
    assert_eq!((c.earned, c.advice.as_str()), (Level::Preflight, ""));
    assert_eq!(c.confidence[0].older_runs, 2);
    assert_eq!(c.confidence[0].runs, vec!["r3", "r4"]);
    assert_eq!(report.disagreements, 0);

    // with no release, each run counts, as before
    let report = promotion(&catalog, &runs, None);
    assert_eq!(report.release, None);
    assert_eq!(
        entry(&report, id).confidence.len(),
        4,
        "one for each text of a release"
    );
    assert_eq!(entry(&report, id).earned, Level::Preflight);
}

#[test]
fn a_run_with_no_release_proves_no_rule_on_a_release_that_a_report_names() {
    let runs = ledger(
        r#"
schema: strata-preflight-ledger/1
runs:
  - {run: r1, engine: gke, release: "", results: [{rule: gke-no-release-channel, result: caught}, {rule: gke-no-release-channel, result: passed}]}
"#,
    );
    let catalog = level_catalog();
    let id = "gke-no-release-channel";
    let report = promotion(&catalog, &runs, Some("4.10"));
    assert_eq!(entry(&report, id).earned, Level::Probe);
    assert_eq!(entry(&report, id).confidence[0].older_runs, 1);
    let report = promotion(&catalog, &runs, None);
    assert_eq!(entry(&report, id).earned, Level::Preflight);
}

#[test]
fn a_rule_that_does_not_apply_to_the_release_has_no_entry_in_the_promotion_report() {
    let catalog = level_catalog();
    let runs = ledger(RUNS_OF_4_10);
    let ids = |release| -> Vec<String> {
        promotion(&catalog, &runs, release)
            .rules
            .into_iter()
            .map(|r| r.id)
            .collect()
    };
    assert_eq!(
        ids(Some("4.10")),
        ["gke-no-release-channel", "gke-old-fault"]
    );
    assert_eq!(ids(Some("4.11")), ["gke-no-release-channel"]);
    assert_eq!(ids(None), ["gke-no-release-channel", "gke-old-fault"]);
}
