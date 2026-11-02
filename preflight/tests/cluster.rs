//! The world of `cluster:UID`, end to end over the memory API: the variable values of the cluster
//! are in the entity, and the validate route of Palette gets them.

use serde_json::json;
use strata::api::{MemoryApi, Palette, cloud_config_path, subject_path};
use strata::snapshot::SubjectKind;
use strata::term::color::Paint;
use strata::term::progress::Progress;
use strata_preflight::catalog;
use strata_preflight::confidence::Ledger;
use strata_preflight::probe::{Backend, LiveBackend, Verdict};
use strata_preflight::report::CheckResult;
use strata_preflight::rule::Tier;
use strata_preflight::run::{self, RunInputs, Selection};
use strata_preflight::world::World;

/// A cluster of one profile that declares the variable `pod_cidr`, and a value for it.
fn api() -> MemoryApi {
    MemoryApi::new()
        .with(
            subject_path(SubjectKind::Cluster, "c1"),
            json!({"metadata": {"uid": "c1", "name": "barney"},
                "spec": {"cloudConfigRef": {"kind": "aws", "uid": "cc1"},
                    "clusterProfileTemplates": [{"uid": "p1", "packs": []}]}}),
        )
        .with(cloud_config_path("aws", "cc1"), json!({"spec": {}}))
        .with(
            "/v1/spectroclusters/c1/variables",
            json!([{"profileUid": "p1", "variables": [{"name": "pod_cidr", "value": "10.244.0.0/16"}]}]),
        )
        .with(
            "POST /v1/spectroclusters/aws/validate",
            json!({"profiles": [], "machinePools": {"results": []}}),
        )
}

#[test]
fn the_validate_route_gets_the_variable_values_of_the_cluster() {
    let api = api();
    let palette = Palette::new(&api);
    let world = World::from_cluster(&palette, "c1", &Progress::silent()).unwrap();
    assert_eq!(
        world.entity["spec"]["profiles"][0]["variables"][0],
        json!({"name": "pod_cidr", "value": "10.244.0.0/16"}),
        "the entity that --out writes to entity.json"
    );
    palette_validate(&api);
    let posted: Vec<_> = api
        .posted()
        .into_iter()
        .filter(|(path, _)| path == "/v1/spectroclusters/aws/validate")
        .collect();
    assert_eq!(posted.len(), 1, "one request to the validate route");
    assert_eq!(
        posted[0].1["spec"]["profiles"][0]["variables"],
        json!([{"name": "pod_cidr", "value": "10.244.0.0/16"}]),
        "the body of palette-validate holds the variable of the profile"
    );
}

/// What Palette answers for a body with a profile of another cloud type (HTTP 500, as a live run
/// shows), word for word.
const CLOUD_MISMATCH: &str = r#"{"code":"ClusterProfilesNotSupported","message":"Cluster profiles 'strata-route-cloud-mismatch' are not supported with 'aws' environment"}"#;

/// Run the rules `ids` over the cluster `c1`, with the world and the checks reading through one
/// backend, as the command does. Give their checks.
fn run_rules(api: &MemoryApi, ids: &[&str]) -> Vec<CheckResult> {
    let backend = LiveBackend::new(Some(api), true);
    let palette = Palette::new(&backend);
    let world = World::from_cluster(&palette, "c1", &Progress::silent()).unwrap();
    let catalog = catalog::builtin().unwrap();
    let report = run::run(RunInputs {
        catalog: &catalog,
        world: &world,
        backend: &backend,
        ledger: &Ledger::default(),
        selection: &Selection {
            tiers: vec![Tier::Palette],
            ids: ids.iter().map(ToString::to_string).collect(),
            ..Selection::default()
        },
        now: "t",
        out_dir: None,
        progress: &Progress::silent(),
        verbose: false,
        paint: Paint::off(),
    })
    .unwrap();
    report.checks
}

/// Run `palette-validate` over the cluster `c1` and give its check.
fn palette_validate(api: &MemoryApi) -> CheckResult {
    let mut checks = run_rules(api, &["palette-validate"]);
    assert_eq!(checks.len(), 1, "{checks:?}");
    checks.remove(0)
}

#[test]
fn the_rules_that_ask_the_validate_route_send_it_one_request_with_the_name_of_the_cluster_and_a_suffix()
 {
    let api = api();
    let checks = run_rules(
        &api,
        &["palette-validate", "worker-cpu-floor", "pack-dependency"],
    );
    assert_eq!(checks.len(), 3, "{checks:?}");
    assert!(
        checks.iter().all(|c| c.verdict == Verdict::Passed),
        "{checks:?}"
    );
    let posted: Vec<_> = api
        .posted()
        .into_iter()
        .filter(|(path, _)| path == "/v1/spectroclusters/aws/validate")
        .collect();
    assert_eq!(posted.len(), 1, "three rules, one request");
    assert_eq!(
        posted[0].1["metadata"]["name"], "barney-preflight",
        "the route refuses the name of a cluster that Palette holds"
    );
}

#[test]
fn template_outside_reads_the_profile_that_the_world_read_and_the_run_sends_no_second_request() {
    let api = api()
        .with(
            "/v1/clusterprofiles/p1",
            json!({"metadata": {"uid": "p1", "name": "base"},
                "spec": {"version": "1.0.0", "published": {"cloudType": "aws", "type": "add-on", "packs": []}},
                "status": {"inUseClusterTemplates": [{"uid": "t1", "name": "standard"}]}}),
        )
        .with(
            "/v1/clusterprofiles/p1/packs/manifests",
            json!({"spec": {"packs": []}}),
        )
        .with(
            "/v1/clusterprofiles/p1/variables",
            json!({"variables": []}),
        );
    let backend = LiveBackend::new(Some(&api), true);
    let palette = Palette::new(&backend);
    let world = World::from_cluster(&palette, "c1", &Progress::silent())
        .unwrap()
        .with_palette(&palette, "t", &Progress::silent())
        .unwrap();
    let report = run::run(RunInputs {
        catalog: &catalog::builtin().unwrap(),
        world: &world,
        backend: &backend,
        ledger: &Ledger::default(),
        selection: &Selection {
            tiers: vec![Tier::Palette],
            ids: vec!["template-outside".to_string()],
            ..Selection::default()
        },
        now: "t",
        out_dir: None,
        progress: &Progress::silent(),
        verbose: false,
        paint: Paint::off(),
    })
    .unwrap();
    assert_eq!(
        report.checks[0].verdict,
        Verdict::Failed,
        "{:?}",
        report.checks
    );
    assert_eq!(
        api.calls("/v1/clusterprofiles/p1"),
        1,
        "the world and the rule read the profile with one request"
    );
}

#[test]
fn the_world_reads_a_cluster_one_time_and_a_rule_that_reads_it_again_costs_no_request() {
    let api = api();
    let backend = LiveBackend::new(Some(&api), true);
    let palette = Palette::new(&backend);
    let world = World::from_cluster(&palette, "c1", &Progress::silent()).unwrap();
    assert_eq!(world.cluster_uid.as_deref(), Some("c1"));
    let path = subject_path(SubjectKind::Cluster, "c1");
    let again = backend
        .palette(strata::api::Method::Get, &path, None, &[])
        .unwrap();
    assert!(again.ok);
    assert_eq!(api.calls(&path), 1);
}

#[test]
fn a_body_with_a_profile_of_another_cloud_type_fails_palette_validate_with_the_message_of_palette()
{
    let api = api().failing("POST /v1/spectroclusters/aws/validate", 500, CLOUD_MISMATCH);
    let check = palette_validate(&api);
    assert_eq!(check.verdict, Verdict::Failed, "{:?}", check.messages);
    assert_eq!(
        check.messages,
        [
            "Palette refused the entity: ClusterProfilesNotSupported: Cluster profiles 'strata-route-cloud-mismatch' are not supported with 'aws' environment"
        ]
    );
    assert_eq!(api.calls("POST /v1/spectroclusters/aws/validate"), 1);
}

#[test]
fn a_500_that_the_cluster_validate_route_does_not_refuse_is_an_error_of_palette_validate() {
    let api = api().failing(
        "POST /v1/spectroclusters/aws/validate",
        500,
        r#"{"code":"InternalError","message":"boom"}"#,
    );
    let check = palette_validate(&api);
    assert_eq!(check.verdict, Verdict::Error, "{:?}", check.messages);
}

/// The six rules that read each edge host of the entity.
const EDGE_HOST_RULES: [&str; 6] = [
    "edge-host-nic",
    "edge-host-in-use",
    "edge-host-health",
    "edge-host-arch",
    "edge-host-resources",
    "edge-agent-version",
];

#[test]
fn six_rules_read_each_edge_host_with_one_request_for_the_host() {
    let hosts = ["h1", "h2", "h3"];
    let mut api = MemoryApi::new();
    for uid in hosts {
        api = api.with(
            format!("/v1/edgehosts/{uid}"),
            json!({"metadata": {"uid": uid, "name": uid}, "status": {"state": "ready"}, "spec": {}}),
        );
    }
    let world = World {
        cloud: "edge".to_string(),
        release: Some("4.10".to_string()),
        entity: json!({"metadata": {"name": "edge"}, "spec": {"machinepoolconfig": [
            {"poolConfig": {"name": "cp", "isControlPlane": true},
             "cloudConfig": {"edgeHosts": [{"hostUid": "h1"}]}},
            {"poolConfig": {"name": "worker"},
             "cloudConfig": {"edgeHosts": [{"hostUid": "h2"}, {"hostUid": "h3"}]}},
        ]}}),
        ..World::default()
    };
    let backend = LiveBackend::new(Some(&api), true);
    let report = run::run(RunInputs {
        catalog: &catalog::builtin().unwrap(),
        world: &world,
        backend: &backend,
        ledger: &Ledger::default(),
        selection: &Selection {
            tiers: vec![Tier::Palette],
            ids: EDGE_HOST_RULES.iter().map(ToString::to_string).collect(),
            ..Selection::default()
        },
        now: "t",
        out_dir: None,
        progress: &Progress::silent(),
        verbose: false,
        paint: Paint::off(),
    })
    .unwrap();
    assert_eq!(report.checks.len(), 6, "{:?}", report.checks);
    for check in &report.checks {
        assert_eq!(
            check.requests.len(),
            3,
            "{}: one request for each host",
            check.id
        );
    }
    for uid in hosts {
        assert_eq!(
            api.calls(&format!("/v1/edgehosts/{uid}")),
            1,
            "{uid}: six rules read the host, and Palette gets one request"
        );
    }
}
