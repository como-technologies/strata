//! The world of `cluster:UID`, end to end over the memory API: the variable values of the cluster
//! are in the entity, and the validate route of Palette gets them.

use serde_json::json;
use strata::api::{MemoryApi, Palette};
use strata::term::progress::Progress;
use strata_preflight::catalog;
use strata_preflight::confidence::Ledger;
use strata_preflight::probe::{LiveBackend, Verdict};
use strata_preflight::report::CheckResult;
use strata_preflight::rule::Tier;
use strata_preflight::run::{self, RunInputs, Selection};
use strata_preflight::world::World;

/// A cluster of one profile that declares the variable `pod_cidr`, and a value for it.
fn api() -> MemoryApi {
    MemoryApi::new()
        .with(
            "/v1/spectroclusters/c1",
            json!({"metadata": {"uid": "c1", "name": "barney"},
                "spec": {"cloudConfigRef": {"kind": "aws", "uid": "cc1"},
                    "clusterProfileTemplates": [{"uid": "p1", "packs": []}]}}),
        )
        .with("/v1/cloudconfigs/aws/cc1", json!({"spec": {}}))
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

/// Run `palette-validate` over the cluster `c1` and give its check.
fn palette_validate(api: &MemoryApi) -> CheckResult {
    let palette = Palette::new(api);
    let world = World::from_cluster(&palette, "c1", &Progress::silent()).unwrap();
    let catalog = catalog::builtin().unwrap();
    let backend = LiveBackend {
        palette: Some(api),
        no_exec: true,
    };
    let mut report = run::run(RunInputs {
        catalog: &catalog,
        world: &world,
        backend: &backend,
        ledger: &Ledger::default(),
        selection: &Selection {
            tiers: vec![Tier::Palette],
            ids: vec!["palette-validate".to_string()],
            ..Selection::default()
        },
        now: "t",
        out_dir: None,
        progress: &Progress::silent(),
        verbose: false,
        color: false,
    })
    .unwrap();
    assert_eq!(report.checks.len(), 1, "{:?}", report.checks);
    report.checks.remove(0)
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
