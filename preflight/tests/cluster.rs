//! The world of `cluster:UID`, end to end over the memory API: the variable values of the cluster
//! are in the entity, and the validate route of Palette gets them.

use serde_json::json;
use strata::api::{MemoryApi, Palette};
use strata::term::progress::Progress;
use strata_preflight::catalog;
use strata_preflight::confidence::Ledger;
use strata_preflight::probe::LiveBackend;
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
    let catalog = catalog::builtin().unwrap();
    let backend = LiveBackend {
        palette: Some(&api),
        no_exec: true,
    };
    let report = run::run(RunInputs {
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
