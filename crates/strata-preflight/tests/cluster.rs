//! The world of `cluster:UID`, end to end over the memory API: the variable values of the cluster
//! are in the entity, and the validate route of Palette gets them. The reports hide each value that
//! Palette quotes in a refusal.

use serde_json::json;
use strata::api::{MemoryApi, Palette};
use strata::term::color::Paint;
use strata::term::progress::Progress;
use strata_preflight::catalog;
use strata_preflight::confidence::Ledger;
use strata_preflight::probe::{Backend, LiveBackend, Verdict};
use strata_preflight::report::{self, CheckResult, Format, Report};
use strata_preflight::rule::Tier;
use strata_preflight::run::{self, RunInputs, Selection};
use strata_preflight::world::World;

// The routes that the tests of strata serve, in the file that they use. This test uses some of
// them, so the others are dead code here.
#[allow(dead_code)]
#[path = "../../../tests/common/palette.rs"]
mod routes;

use routes::{
    cloud_config_route, cluster_route, cluster_variables_route, edge_host_route, manifests_route,
    profile_route, variables_route,
};

/// A cluster of one profile that declares the variable `pod_cidr`, and a value for it.
fn api() -> MemoryApi {
    MemoryApi::new()
        .with(
            cluster_route("c1"),
            json!({"metadata": {"uid": "c1", "name": "barney"},
                "spec": {"cloudConfigRef": {"kind": "aws", "uid": "cc1"},
                    "clusterProfileTemplates": [{"uid": "p1", "packs": []}]}}),
        )
        .with(cloud_config_route("aws", "cc1"), json!({"spec": {}}))
        .with(
            cluster_variables_route("c1"),
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
/// backend, as the command does. Give the report.
fn run_report(api: &MemoryApi, ids: &[&str]) -> Report {
    let backend = LiveBackend::new(Some(api), true);
    let palette = Palette::new(&backend);
    let world = World::from_cluster(&palette, "c1", &Progress::silent()).unwrap();
    let catalog = catalog::builtin().unwrap();
    run::run(RunInputs {
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
        out_world: false,
        progress: &Progress::silent(),
        verbose: false,
        paint: Paint::off(),
    })
    .unwrap()
}

/// Run the rules `ids` over the cluster `c1`, as [`run_report`] does. Give their checks.
fn run_rules(api: &MemoryApi, ids: &[&str]) -> Vec<CheckResult> {
    run_report(api, ids).checks
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
            profile_route("p1"),
            json!({"metadata": {"uid": "p1", "name": "base"},
                "spec": {"version": "1.0.0", "published": {"cloudType": "aws", "type": "add-on", "packs": []}},
                "status": {"inUseClusterTemplates": [{"uid": "t1", "name": "standard"}]}}),
        )
        .with(
            manifests_route("p1"),
            json!({"spec": {"packs": []}}),
        )
        .with(
            variables_route("p1"),
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
        out_world: false,
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
        api.calls(&profile_route("p1")),
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
    let path = cluster_route("c1");
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

/// The variables of the cluster `c1` for the tests of the mask, as pairs of a name and a value: a
/// pod CIDR, a service CIDR that is not a CIDR, and a label that is not an option of its dropdown.
const QUOTED: [(&str, &str); 3] = [
    ("pod_cidr", "10.244.0.0/16"),
    ("service_cidr", "10.96.0.0/33"),
    ("tier_label", "platinum"),
];

/// Three refusals of Palette that quote the value of a variable: the code, the variable, and the
/// message as the message template of Palette writes it. Palette puts the value of a variable in
/// place of a `{{.spectro.var.NAME}}` reference before it checks.
const REFUSALS: [(&str, &str, &str); 3] = [
    (
        "K8SCidrOverlap",
        "pod_cidr",
        "Kubernetes podCIDR '10.244.0.0/16' is overlapping with vpc 'vpc-0a1b' cidr block '10.244.0.0/14'",
    ),
    (
        "CidrParseError",
        "service_cidr",
        "CIDR '10.96.0.0/33 parse error; caused by: invalid CIDR address: 10.96.0.0/33",
    ),
    (
        "InvalidDropDownVariable",
        "tier_label",
        "Dropdown variable 'tier_label' in spectrocluster is invalid: label 'platinum' is not a valid option",
    ),
];

/// What `palette-validate` says of each refusal of `REFUSALS`, in the same order: the code and the
/// message of Palette, with a mark in place of each value.
const SAID: [&str; 3] = [
    "Palette refused the entity: K8SCidrOverlap: Kubernetes podCIDR '[value of pod_cidr]' is overlapping with vpc 'vpc-0a1b' cidr block '10.244.0.0/14'",
    "Palette refused the entity: CidrParseError: CIDR '[value of service_cidr] parse error; caused by: invalid CIDR address: [value of service_cidr]",
    "Palette refused the entity: InvalidDropDownVariable: Dropdown variable 'tier_label' in spectrocluster is invalid: label '[value of tier_label]' is not a valid option",
];

/// The refusal of a required variable with no value, as the message template of Palette writes
/// it. It quotes no value.
const REQUIRED_VARIABLE: &str = "spectrocluster variable is invalid due to default value can't be empty for required variable 'region_name'";

/// The cluster `c1` with `variables`, as pairs of a name and a value. Its validate route answers
/// HTTP `status` with the refusal `code` and `message`.
fn refusing(variables: &[(&str, &str)], status: u16, code: &str, message: &str) -> MemoryApi {
    let variables: Vec<_> = variables
        .iter()
        .map(|(name, value)| json!({"name": name, "value": value}))
        .collect();
    api()
        .with(
            cluster_variables_route("c1"),
            json!([{"profileUid": "p1", "variables": variables}]),
        )
        .failing(
            "POST /v1/spectroclusters/aws/validate",
            status,
            json!({"code": code, "message": message}).to_string(),
        )
}

/// The three forms of `report` that `--out` writes, by the name of the file. The time of each check
/// is 0, so that two runs give the same text.
fn reports(mut report: Report) -> [(&'static str, String); 3] {
    for check in &mut report.checks {
        check.elapsed_ms = 0;
    }
    [
        ("report.md", Format::Markdown),
        ("report.json", Format::Json),
        ("report.html", Format::Html),
    ]
    .map(|(file, format)| (file, report::render(&report, format, false, &Paint::off())))
}

/// The route refuses a variable or a CIDR of the body with HTTP 500. The code of Palette shows it;
/// no live run shows it yet. `palette-validate` reads it as the answer of the route, as it reads
/// HTTP 400. The verdict is `failed`, and the message is the message of Palette with a mark in
/// place of each value. Strata sends the body one time (#816).
#[test]
fn a_refusal_of_a_variable_or_a_cidr_fails_palette_validate_with_the_message_of_palette_at_http_500_and_400()
 {
    let required = format!("Palette refused the entity: InvalidVariable: {REQUIRED_VARIABLE}");
    let cases = REFUSALS
        .iter()
        .zip(SAID)
        .map(|((code, _, message), said)| (*code, *message, said))
        .chain([("InvalidVariable", REQUIRED_VARIABLE, required.as_str())]);
    for (code, message, said) in cases {
        for status in [500, 400] {
            let api = refusing(&QUOTED, status, code, message);
            let check = palette_validate(&api);
            assert_eq!(
                check.verdict,
                Verdict::Failed,
                "{code} {status}: {:?}",
                check.messages
            );
            assert_eq!(check.messages, [said], "{code} {status}");
            assert_eq!(
                api.calls("POST /v1/spectroclusters/aws/validate"),
                1,
                "{code} {status}"
            );
        }
    }
}

/// The refusal of a machine pool with no subnet, for AWS with a VPC, as Palette writes it.
const NO_SUBNET: &str = "Parameter 'Subnets' is required";

/// The route refuses a body for AWS with a VPC and a pool with no subnet with HTTP 500 and
/// `RequiredParam`, as a live run shows.
///
/// `palette-validate` reads it as the answer of the route, as it reads HTTP 400. The verdict is
/// `failed`, with the message of Palette. Strata sends the body one time (#824).
#[test]
fn a_pool_with_no_subnet_fails_palette_validate_with_the_message_of_palette_at_http_500_and_400() {
    let said = format!("Palette refused the entity: RequiredParam: {NO_SUBNET}");
    for status in [500, 400] {
        let api = refusing(&[], status, "RequiredParam", NO_SUBNET);
        let check = palette_validate(&api);
        assert_eq!(
            check.verdict,
            Verdict::Failed,
            "{status}: {:?}",
            check.messages
        );
        assert_eq!(check.messages, [said.as_str()], "{status}");
        assert_eq!(
            api.calls("POST /v1/spectroclusters/aws/validate"),
            1,
            "{status}"
        );
    }
}

/// The check reads a 4xx, and a 500 with these codes, as a refusal, so the message holds the
/// message of Palette. The reports hold each other word of the message, such as the CIDR of the
/// VPC (#614).
#[test]
fn the_three_reports_write_a_mark_in_place_of_each_value_that_a_refusal_of_palette_quotes() {
    for (code, name, message) in REFUSALS {
        for status in [400, 500] {
            let api = refusing(&QUOTED, status, code, message);
            let report = run_report(&api, &["palette-validate"]);
            let messages = &report.checks[0].messages;
            assert_eq!(
                report.checks[0].verdict,
                Verdict::Failed,
                "{code} {status}: {messages:?}"
            );
            assert!(message.contains(QUOTED.iter().find(|(n, _)| *n == name).unwrap().1));
            for (file, text) in reports(report) {
                let at = format!("{code}, HTTP {status}, {file}");
                assert!(text.contains(code), "{at}");
                assert!(text.contains(&format!("[value of {name}]")), "{at}");
                for (_, value) in QUOTED {
                    assert!(!text.contains(value), "{at}: {value}");
                }
                if code == "K8SCidrOverlap" {
                    assert!(text.contains("10.244.0.0/14"), "{at}: the CIDR of the VPC");
                }
            }
        }
    }
}

/// Each short value here is a part of a longer word in the messages of the three refusals: of a
/// CIDR, of the route, or of the address `/33`. `true` is a value of the JSON of `report.json`.
/// So the variables change no text of the three reports (#614).
#[test]
fn a_short_or_common_value_of_a_variable_masks_no_other_text_of_the_three_reports() {
    let short = [
        ("prefix", "10"),
        ("zero", "0"),
        ("bits", "16"),
        ("cloud", "aws"),
        ("enabled", "true"),
        ("replicas", "3"),
    ];
    let with_short: Vec<(&str, &str)> = QUOTED.iter().chain(&short).copied().collect();
    for (code, _, message) in REFUSALS {
        for status in [400, 500] {
            let run = |variables: &[(&str, &str)]| {
                reports(run_report(
                    &refusing(variables, status, code, message),
                    &["palette-validate"],
                ))
            };
            for ((file, plain), (_, more)) in run(&QUOTED).into_iter().zip(run(&with_short)) {
                assert_eq!(plain, more, "{code}, HTTP {status}, {file}");
            }
        }
    }
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
            edge_host_route(uid),
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
        out_world: false,
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
            api.calls(&edge_host_route(uid)),
            1,
            "{uid}: six rules read the host, and Palette gets one request"
        );
    }
}
