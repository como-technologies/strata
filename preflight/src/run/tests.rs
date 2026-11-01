use super::*;
use crate::probe::FixtureBackend;
use crate::report::Tool;
use serde_json::json;
use std::path::Path;
use std::time::Duration;
use strata::api::{Palette, cluster_events_path, cluster_summary_path, subject_path};
use strata::snapshot::SubjectKind;

const SECTION: &str = r#"
section: compute
title: Compute and pools
rules:
  - id: gke-pool-multiple-3
    title: A GKE worker pool size is a multiple of 3
    engines: [gke]
    tier: static
    status: seen
    what: w
    check:
      kind: static
      expr: '[.entity.spec.machinepoolconfig[] | select(.poolConfig.isControlPlane | not) | select(.poolConfig.size % 3 != 0) | "pool \(.poolConfig.name) has size \(.poolConfig.size)"]'
  - id: eks-one-zone
    title: EKS needs two zones
    engines: [eks]
    tier: static
    status: source
    what: w
    check: {kind: static, expr: "true"}
  - id: gke-api-off
    title: The Kubernetes Engine API is off
    engines: [gke]
    tier: cloud
    status: seen
    what: w
    reproduce: ["gcloud services list --enabled --project {{ .entity.spec.cloudConfig.project }}"]
  - id: old-rule
    title: A rule of an old release
    engines: [all]
    tier: static
    status: source
    releases: "<4.0"
    what: w
    check: {kind: static, expr: "false"}
  - id: provider-thing
    title: A rule of the provider
    engines: [all]
    tier: tool
    status: seen
    what: w
  - id: on-change-only
    title: A rule on a change
    engines: [all]
    tier: static
    status: seen
    what: w
    when: ".before != null"
    check: {kind: static, expr: "false"}
"#;

fn catalog() -> Catalog {
    let mut c = Catalog::default();
    c.push(
        Catalog::parse_section(SECTION, Path::new("c.yaml")).unwrap(),
        Path::new("c.yaml"),
    )
    .unwrap();
    c
}

fn world(size: u64) -> World {
    World {
        cloud: "gke".into(),
        release: Some("4.10".into()),
        source: "c.json".into(),
        entity: json!({"metadata": {"name": "fred"}, "spec": {"cloudConfig": {"project": "p1"},
                "machinepoolconfig": [{"poolConfig": {"name": "worker-pool", "size": size, "isControlPlane": false}}]}}),
        ..World::default()
    }
}

#[test]
fn a_run_keeps_the_rules_of_the_engine_and_says_why_the_others_do_not_run() {
    let c = catalog();
    let (backend, _) = FixtureBackend::split(json!({}));
    let report = run(RunInputs {
        catalog: &c,
        world: &world(4),
        backend: &backend,
        ledger: &Ledger::default(),
        selection: &Selection::default(),
        now: "2026-09-30T00:00:00Z",
        out_dir: None,
        progress: &Progress::silent(),
        verbose: false,
        paint: Paint::off(),
    })
    .unwrap();
    let by_id = |id: &str| report.checks.iter().find(|c| c.id == id).unwrap();
    assert!(
        report.checks.iter().all(|c| c.id != "eks-one-zone"),
        "another engine is not in the report"
    );
    assert_eq!(by_id("gke-pool-multiple-3").verdict, Verdict::Failed);
    assert_eq!(
        by_id("gke-pool-multiple-3").messages,
        vec!["pool worker-pool has size 4"]
    );
    assert_eq!(by_id("gke-api-off").verdict, Verdict::NotChecked);
    assert_eq!(
        by_id("gke-api-off").reproduce,
        vec!["gcloud services list --enabled --project p1"]
    );
    assert_eq!(by_id("old-rule").verdict, Verdict::Skipped);
    assert_eq!(by_id("provider-thing").verdict, Verdict::Skipped);
    assert_eq!(
        by_id("on-change-only").verdict,
        Verdict::Skipped,
        "no change in this world"
    );
    assert!(
        by_id("on-change-only").messages[0].contains("does not apply here"),
        "{:?}",
        by_id("on-change-only").messages
    );
    assert_eq!(
        report.summary.exit_code, 0,
        "a failed probe does not change the exit code"
    );
    assert_eq!(report.summary.probe_failed, 1);
    assert_eq!(report.subject.name, "fred");
}

#[test]
fn strict_counts_a_failed_probe_and_a_clean_entity_passes() {
    let c = catalog();
    let (backend, _) = FixtureBackend::split(json!({}));
    let strict = Selection {
        strict: true,
        ..Selection::default()
    };
    let inputs = |w: &World| {
        run(RunInputs {
            catalog: &c,
            world: w,
            backend: &backend,
            ledger: &Ledger::default(),
            selection: &strict,
            now: "t",
            out_dir: None,
            progress: &Progress::silent(),
            verbose: false,
            paint: Paint::off(),
        })
        .unwrap()
    };
    assert_eq!(inputs(&world(4)).summary.exit_code, 1);
    let clean = inputs(&world(3));
    assert_eq!(clean.summary.exit_code, 0);
    assert_eq!(clean.summary.passed, 1);
}

#[test]
fn a_report_counts_only_the_runs_of_its_release_and_names_the_older_runs() {
    let c = catalog();
    let (backend, _) = FixtureBackend::split(json!({}));
    let ledger = Ledger::parse(
        r#"
schema: strata-preflight-ledger/1
runs:
  - {run: r1, engine: gke, release: "", results: [{rule: gke-pool-multiple-3, result: caught}]}
  - {run: r2, engine: gke, release: "4.10", results: [{rule: gke-pool-multiple-3, result: passed}]}
"#,
        Path::new("runs.yaml"),
    )
    .unwrap();
    let confidence_on = |release: Option<&str>| {
        let mut w = world(3);
        w.release = release.map(str::to_string);
        let report = run(RunInputs {
            catalog: &c,
            world: &w,
            backend: &backend,
            ledger: &ledger,
            selection: &Selection::default(),
            now: "t",
            out_dir: None,
            progress: &Progress::silent(),
            verbose: false,
            paint: Paint::off(),
        })
        .unwrap();
        let by_id = |id: &str| report.checks.iter().find(|c| c.id == id).unwrap().clone();
        (by_id("gke-pool-multiple-3"), by_id("gke-api-off"))
    };

    let (pool, api) = confidence_on(Some("4.10"));
    let conf = pool
        .confidence
        .as_ref()
        .expect("the rule has a run of 4.10");
    assert_eq!((conf.catch.total, conf.clean.total), (0, 1));
    assert_eq!(conf.older_runs, 1, "the run with no release");
    assert_eq!(
        pool.confidence_text().as_deref(),
        Some("catch no runs, clean 1/1 (5%), older runs 1")
    );
    assert!(
        api.confidence.is_none(),
        "no run of this rule is in the ledger"
    );

    let (pool, _) = confidence_on(Some("4.11"));
    assert_eq!(
        pool.confidence_text().as_deref(),
        Some("no runs, older runs 2"),
        "no run of 4.11: the two runs are older"
    );
    assert!(!pool.confidence.unwrap().has_runs());

    let (pool, _) = confidence_on(None);
    assert_eq!(
        pool.confidence_text().as_deref(),
        Some("catch 1/1 (5%), clean 1/1 (5%)"),
        "with no release, each run counts"
    );
}
#[test]
fn a_tier_not_asked_for_is_not_run_and_ids_select() {
    let c = catalog();
    let (backend, _) = FixtureBackend::split(json!({}));
    let sel = Selection {
        tiers: vec![Tier::Palette],
        ..Selection::default()
    };
    let report = run(RunInputs {
        catalog: &c,
        world: &world(4),
        backend: &backend,
        ledger: &Ledger::default(),
        selection: &sel,
        now: "t",
        out_dir: None,
        progress: &Progress::silent(),
        verbose: false,
        paint: Paint::off(),
    })
    .unwrap();
    let pool = report
        .checks
        .iter()
        .find(|c| c.id == "gke-pool-multiple-3")
        .unwrap();
    assert_eq!(pool.verdict, Verdict::NotRun);
    assert!(pool.messages[0].contains("tier static was not asked for"));
    let sel = Selection {
        ids: vec!["gke-api-off".into()],
        ..Selection::default()
    };
    let report = run(RunInputs {
        catalog: &c,
        world: &world(4),
        backend: &backend,
        ledger: &Ledger::default(),
        selection: &sel,
        now: "t",
        out_dir: None,
        progress: &Progress::silent(),
        verbose: false,
        paint: Paint::off(),
    })
    .unwrap();
    assert_eq!(report.checks.len(), 1);
}

#[test]
fn the_rules_listing_names_each_rule_of_the_engine() {
    let c = catalog();
    let rules = list_rules(&c, Some("eks"), None, &[]);
    assert_eq!(
        rules.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        vec![
            "eks-one-zone",
            "old-rule",
            "provider-thing",
            "on-change-only"
        ]
    );
    let text = rules_text(&c, &rules);
    assert!(text.contains("eks-one-zone"), "{text}");
    assert!(text.contains("4 rules: 3 with a check"), "{text}");
    assert_eq!(list_rules(&c, None, Some("4.10"), &[Tier::Static]).len(), 3);
}

#[test]
fn a_watch_stops_at_the_first_counted_failure_with_the_evidence() {
    // the rule is a probe: it counts, and ends the watch, with --strict
    let section = r#"
section: platform
title: The Palette platform
rules:
  - id: status-not-cloud
    title: The status shows no error while the cloud refuses the build
    engines: [gke]
    tier: watch
    status: seen
    what: w
    check:
      kind: watch
      expr: '[.events[] | select(.reason == "ReconcileError" or (.message | test("InvalidArgument"))) | "event \(.reason): \(.message)"]'
"#;
    let mut c = Catalog::default();
    c.push(
        Catalog::parse_section(section, Path::new("p.yaml")).unwrap(),
        Path::new("p.yaml"),
    )
    .unwrap();
    let api = strata::api::MemoryApi::new()
            .with(subject_path(SubjectKind::Cluster, "c1"), json!({"metadata": {"uid": "c1"}, "spec": {"cloudType": "gke"}, "status": {"state": "Provisioning"}}))
            .with(cluster_events_path("c1"), json!({"items": [{"reason": "GCPManagedControlPlaneReconcile", "message": "InvalidArgument: no release channel"}]}));
    let mut w = world(3);
    w.cluster_uid = Some("c1".into());
    let slept = std::cell::Cell::new(0);
    let (report, _) = watch(WatchInputs {
        catalog: &c,
        world: &w,
        palette: &Palette::new(&api),
        ledger: &Ledger::default(),
        selection: &Selection {
            strict: true,
            ..Selection::default()
        },
        options: &WatchOptions::default(),
        now: "t",
        progress: &Progress::silent(),
        verbose: false,
        paint: Paint::off(),
        sleep: &|_| slept.set(slept.get() + 1),
        out_dir: None,
    })
    .unwrap();
    assert_eq!(report.checks[0].verdict, Verdict::Failed);
    assert!(
        report.checks[0].messages[0].contains("InvalidArgument"),
        "{:?}",
        report.checks[0].messages
    );
    assert_eq!(
        slept.get(),
        0,
        "the first reading failed, so the watch did not wait"
    );
    assert!(
        report.subject.source.contains("Provisioning after"),
        "{}",
        report.subject.source
    );
    assert_eq!(report.options.tiers, vec![Tier::Watch]);
}

#[test]
fn the_rules_document_names_this_tool_as_the_report_does_and_has_its_own_schema_id() {
    let catalog = crate::catalog::builtin().unwrap();
    let rules = list_rules(&catalog, Some("eks"), None, &[]);
    let doc = rules_document(&catalog, Some("eks"), None, &rules);
    assert_eq!(doc["schema"], RULES_SCHEMA);
    assert_eq!(doc["tool"], serde_json::to_value(Tool::current()).unwrap());
    assert_eq!(doc["cloud"], "eks");
    assert_eq!(doc["release"], Value::Null);
    assert_eq!(doc["rules"].as_array().unwrap().len(), rules.len());
    assert_eq!(doc["sections"][0]["section"], catalog.sections[0].section);
}

#[test]
fn a_reading_has_no_summary_for_a_404_and_stops_at_a_401_or_a_429() {
    let cluster = json!({"metadata": {"uid": "c1"}, "status": {"state": "Provisioning"}});
    let summary = cluster_summary_path("c1");
    let events = cluster_events_path("c1");
    let gone = strata::api::MemoryApi::new()
        .with(subject_path(SubjectKind::Cluster, "c1"), cluster.clone())
        .failing(events, 403, "{}");
    let read = reading(&Palette::new(&gone), "c1", Duration::from_secs(1)).unwrap();
    assert_eq!(read["summary"], Value::Null);
    assert_eq!(read["events"], json!([]));
    for status in [401, 429] {
        let refused = strata::api::MemoryApi::new()
            .with(subject_path(SubjectKind::Cluster, "c1"), cluster.clone())
            .failing(&summary, status, "{}");
        let err = reading(&Palette::new(&refused), "c1", Duration::from_secs(1)).unwrap_err();
        assert!(err.to_string().contains(&format!("HTTP {status}")), "{err}");
    }
}

#[test]
fn a_reading_of_a_cluster_with_no_event_has_an_empty_list_of_events() {
    let cluster = json!({"metadata": {"uid": "c1"}, "status": {"state": "Provisioning"}});
    // Palette gives `items: null` when the cluster has no event
    let api = strata::api::MemoryApi::new()
        .with(subject_path(SubjectKind::Cluster, "c1"), cluster)
        .with(cluster_events_path("c1"), json!({"items": null}));
    let read = reading(&Palette::new(&api), "c1", Duration::from_secs(1)).unwrap();
    assert_eq!(read["events"], json!([]));
}

#[test]
fn a_watch_ends_when_the_cluster_runs() {
    let section = r#"
section: platform
title: P
rules:
  - id: agent-egress
    title: t
    engines: [all]
    tier: watch
    status: source
    what: w
    check: {kind: watch, expr: "true"}
"#;
    let mut c = Catalog::default();
    c.push(
        Catalog::parse_section(section, Path::new("p.yaml")).unwrap(),
        Path::new("p.yaml"),
    )
    .unwrap();
    let api = strata::api::MemoryApi::new().with(
        subject_path(SubjectKind::Cluster, "c1"),
        json!({"metadata": {"uid": "c1"}, "status": {"state": "Running"}}),
    );
    let mut w = world(3);
    w.cluster_uid = Some("c1".into());
    let (report, _) = watch(WatchInputs {
        catalog: &c,
        world: &w,
        palette: &Palette::new(&api),
        ledger: &Ledger::default(),
        selection: &Selection::default(),
        options: &WatchOptions::default(),
        now: "t",
        progress: &Progress::silent(),
        verbose: false,
        paint: Paint::off(),
        sleep: &|_| {},
        out_dir: None,
    })
    .unwrap();
    assert_eq!(report.checks[0].verdict, Verdict::Passed);
    assert_eq!(report.summary.exit_code, 0);
}

#[test]
fn a_watch_that_reads_on_past_a_counted_failure_ends_when_the_cluster_runs() {
    let section = r#"
section: platform
title: P
rules:
  - id: agent-egress
    title: t
    engines: [all]
    tier: watch
    status: source
    what: w
    check: {kind: watch, expr: "false"}
"#;
    let mut c = Catalog::default();
    c.push(
        Catalog::parse_section(section, Path::new("p.yaml")).unwrap(),
        Path::new("p.yaml"),
    )
    .unwrap();
    let api = strata::api::MemoryApi::new().with(
        subject_path(SubjectKind::Cluster, "c1"),
        json!({"metadata": {"uid": "c1"}, "status": {"state": "Running"}}),
    );
    let mut w = world(3);
    w.cluster_uid = Some("c1".into());
    let readings = std::cell::Cell::new(0);
    let (report, _) = watch(WatchInputs {
        catalog: &c,
        world: &w,
        palette: &Palette::new(&api),
        ledger: &Ledger::default(),
        selection: &Selection {
            strict: true,
            ..Selection::default()
        },
        options: &WatchOptions {
            stop_at_first: false,
            ..WatchOptions::default()
        },
        now: "t",
        progress: &Progress::silent(),
        verbose: false,
        paint: Paint::off(),
        sleep: &|_| readings.set(readings.get() + 1),
        out_dir: None,
    })
    .unwrap();
    assert_eq!(report.checks[0].verdict, Verdict::Failed);
    assert!(report.checks[0].counts_as_failure);
    assert_eq!(report.summary.exit_code, 1);
    assert_eq!(
        readings.get(),
        0,
        "the first reading saw Running, and the watch ended there"
    );
}

#[test]
fn a_failed_probe_does_not_end_the_watch_and_is_in_the_report() {
    let section = r#"
section: platform
title: P
rules:
  - id: webhook-call-failed
    title: t
    engines: [all]
    tier: watch
    severity: warn
    status: seen
    what: w
    check: {kind: watch, expr: '[.events[] | select(.message | test("failed calling webhook")) | .message]'}
"#;
    let mut c = Catalog::default();
    c.push(
        Catalog::parse_section(section, Path::new("p.yaml")).unwrap(),
        Path::new("p.yaml"),
    )
    .unwrap();
    let api = strata::api::MemoryApi::new()
            .with(
                subject_path(SubjectKind::Cluster, "c1"),
                json!({"metadata": {"uid": "c1"}, "status": {"state": "Running"}}),
            )
            .with(
                cluster_events_path("c1"),
                json!({"items": [{"reason": "Log", "message": "failed calling webhook \"vspectrocluster.kb.io\": connection refused", "metadata": {"creationTimestamp": "2026-09-29T17:59:22.272Z"}}]}),
            );
    let mut w = world(3);
    w.cluster_uid = Some("c1".into());
    let (report, reading) = watch(WatchInputs {
        catalog: &c,
        world: &w,
        palette: &Palette::new(&api),
        ledger: &Ledger::default(),
        selection: &Selection::default(),
        options: &WatchOptions::default(),
        now: "t",
        progress: &Progress::silent(),
        verbose: false,
        paint: Paint::off(),
        sleep: &|_| {},
        out_dir: None,
    })
    .unwrap();
    // the warning is in the report, the exit is 0, and the watch ended at Running
    assert_eq!(report.checks[0].verdict, Verdict::Failed);
    assert_eq!(report.summary.exit_code, 0);
    assert!(
        report.subject.source.contains("Running after"),
        "{}",
        report.subject.source
    );
    // the reading the rules saw is the world of --out
    assert_eq!(reading["cluster"]["status"]["state"], "Running");
    assert_eq!(reading["events"].as_array().map(Vec::len), Some(1));
}

#[test]
fn a_check_with_each_prints_a_line_for_each_request_and_the_count_of_requests_on_its_line() {
    let section = r#"
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
      each: '[.hosts[]]'
      path: /v1/edgehosts/{{ $item }}
      expr: '[.responses[] | select(.response.ok | not) | "edge host \(.item) is not there"]'
"#;
    let mut c = Catalog::default();
    c.push(
        Catalog::parse_section(section, Path::new("e.yaml")).unwrap(),
        Path::new("e.yaml"),
    )
    .unwrap();
    let (backend, world) = FixtureBackend::split(json!({"hosts": ["h1", "h2"],
            "_responses": {"GET /v1/edgehosts/h1": {"ok": true, "status": 200, "body": {}},
                "GET /v1/edgehosts/h2": {"ok": false, "status": 404, "body": {}}}}));
    let rule = c.rule("edge-host-in-use").unwrap();
    let outcome = probe::run_check(rule, &world, &backend, &Jq::new());
    let lines = verbose_lines(rule, &outcome, &Paint::off());
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert_eq!(
        lines[..2],
        [
            "palette: edge-host-in-use: GET /v1/edgehosts/h1 -> HTTP 200",
            "palette: edge-host-in-use: GET /v1/edgehosts/h2 -> HTTP 404",
        ]
    );
    assert!(
        lines[2].starts_with("palette: edge-host-in-use: 2 requests -> ")
            && lines[2].ends_with(" ms, PROBE FAIL: edge host h2 is not there"),
        "{}",
        lines[2]
    );
}

#[test]
fn a_run_of_an_existing_cluster_whose_metadata_is_not_an_object_is_an_input_error() {
    let c = catalog();
    let (backend, _) = FixtureBackend::split(json!({}));
    let mut existing = world(4);
    existing.cluster_uid = Some("c1".into());
    existing.entity["metadata"] = json!("fred");
    let err = run(RunInputs {
        catalog: &c,
        world: &existing,
        backend: &backend,
        ledger: &Ledger::default(),
        selection: &Selection::default(),
        now: "2026-09-30T00:00:00Z",
        out_dir: None,
        progress: &Progress::silent(),
        verbose: false,
        paint: Paint::off(),
    })
    .unwrap_err();
    assert_eq!(err.kind(), "input");
    assert!(
        err.to_string()
            .starts_with("entity.metadata is not an object"),
        "{err}"
    );
}
