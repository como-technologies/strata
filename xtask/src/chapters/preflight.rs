//! The chapter `preflight`: can Palette build a cluster? The helpers build the body of an AWS
//! create, and the answers of the routes that the live tier reads.

use super::DEV_ENV;
use super::ids::{A110, A111, AWS_STANDARD, DEV, GCP_STACK, PAYMENTS, PLATFORM_AWS, PROD};
use crate::book::Runner;
use crate::tenant::Edit;
use serde_json::json;

/// The body of an AWS create named `name`, for the profile `profile`. It names the cloud account
/// platform-aws, and two pools of t3.xlarge in one zone, with one worker.
pub(super) fn preflight_entity(name: &str, profile: &str) -> String {
    let pool = |name: &str, size: u32, control_plane: bool| {
        json!({
            "cloudConfig": {"instanceType": "t3.xlarge", "azs": ["us-east-2a"], "rootDeviceSize": 60},
            "poolConfig": {"name": name, "size": size, "isControlPlane": control_plane,
                           "useControlPlaneAsWorker": false},
        })
    };
    let entity = json!({
        "metadata": {"name": name, "labels": {"env": "prod"}},
        "spec": {
            "cloudAccountUid": PLATFORM_AWS,
            "cloudConfig": {"region": "us-east-2", "sshKeyName": "platform", "vpcId": ""},
            "machinepoolconfig": [pool("cp-pool", 1, true), pool("worker-pool", 1, false)],
            "profiles": [{"uid": profile, "packValues": [], "variables": []}],
        },
    });
    serde_json::to_string_pretty(&entity).expect("a JSON document")
}

/// The route of Palette that checks the body of an AWS create, with nothing created.
pub(super) const AWS_VALIDATE: &str = "v1/spectroclusters/aws/validate";

/// The answer of the validate route when a pool has too few cores for the packs. The one worker of
/// `preflight_entity` has 4 cores, and the packs need 4.1.
fn cpu_floor_answer() -> serde_json::Value {
    json!({"machinePools": {"results": [{"name": "worker-pool", "displayName": "worker-pool",
        "errors": [{"code": "MachinePoolWithMinCpuRequired",
            "message": "A minimum allocation of '4.1' CPU cores is required across all worker nodes"}]}]},
        "profiles": []})
}

/// The answer of the validate route when it finds no fault.
pub(super) fn accepted_answer() -> serde_json::Value {
    json!({"machinePools": {"results": []}, "profiles": []})
}

/// One entry of the answer of the instance type route, as Palette gives it. The name of the type
/// is `type`, and an entry has no `name`. `not_offered` is `None` for a type that each zone offers:
/// Palette gives `null` for it, as it does for most types.
fn instance_type(
    name: &str,
    cpu: u32,
    memory: u32,
    price: f64,
    not_offered: Option<&[&str]>,
) -> serde_json::Value {
    json!({"category": "General purpose", "cpu": cpu, "memory": memory, "price": price, "type": name,
        "cost": {"price": [{"onDemand": price, "os": "linux"}]},
        "nonSupportedZones": not_offered, "supportedArchitectures": ["x86_64"]})
}

/// The answer of the instance type route for `us-east-2`. Each zone offers `t3.large`, and
/// `t3.xlarge` is not offered in the zone `us-east-2c`, which no pool of `preflight_entity` uses.
fn instance_types_answer() -> serde_json::Value {
    json!({"instanceTypes": [
        instance_type("t3.large", 2, 8, 0.0832, None),
        instance_type("t3.xlarge", 4, 16, 0.1664, Some(&["us-east-2c"])),
    ]})
}

/// Write the answers that the live tier of `strata preflight` reads for `preflight_entity` in
/// `project`. They are the cloud account, the property route, the regions, the instance types of
/// the region, and the key pairs of the account. The validate route refuses the worker pool for
/// its CPU.
pub(super) fn preflight_routes(r: &mut Runner, project: &str) {
    r.answer(
        project,
        &format!("v1/cloudaccounts/aws/{PLATFORM_AWS}"),
        &json!({"kind": "aws",
            "metadata": {"name": "platform-aws", "uid": PLATFORM_AWS,
                "annotations": {"scope": "project"}},
            "spec": {"credentialType": "secret", "partition": "aws", "policyARNs": []}, "status": {"state": "Active"}}),
    );
    // the listing of the AWS accounts of the project: the contract run takes the account of the
    // regions route from it
    r.answer(
        project,
        "v1/cloudaccounts/aws",
        &json!({"items": [{"kind": "aws",
            "metadata": {"name": "platform-aws", "uid": PLATFORM_AWS}}]}),
    );
    // the property route of AWS answers 200 for a key pair that the region holds. The stand-in
    // serves an empty object, which a rule reads as `ok`. The regions of the account follow, with
    // the query in the file name as the stand-in maps it
    r.answer(project, "v1/clouds/aws/properties/validate", &json!({}));
    r.answer(
        project,
        &format!("v1/clouds/aws/regions@cloudAccountUid={PLATFORM_AWS}"),
        &json!({"regions": [{"name": "us-east-2", "zones": [{"name": "us-east-2a"}, {"name": "us-east-2b"}, {"name": "us-east-2c"}]}]}),
    );
    r.answer(
        project,
        &format!("v1/clouds/aws/regions/us-east-2/instancetypes@cloudAccountUid={PLATFORM_AWS}"),
        &instance_types_answer(),
    );
    r.answer(
        project,
        &format!("v1/clouds/aws/regions/us-east-2/keypairs@cloudAccountUid={PLATFORM_AWS}"),
        &json!({"keyNames": ["platform", "lab"]}),
    );
    r.answer(project, AWS_VALIDATE, &cpu_floor_answer());
}

/// The chapter `preflight`: can Palette build this cluster?
pub(super) fn run(r: &mut Runner) {
    // preflight: can Palette build this cluster? The entity is the body of an AWS create, with the
    // profile aws-stack@1.1.1. The stand-in answers the validate route with the CPU floor of the
    // packs, and holds the cloud account of the entity.
    r.chapter("preflight", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.write_file("cluster.json", &preflight_entity("payments", A111));
    preflight_routes(r, DEV);
    r.step(
        "preflight-static",
        0,
        false,
        "strata preflight cluster.json --cloud aws --tier static --offline --quiet",
    );
    r.step(
        "preflight-verbose",
        0,
        false,
        "strata preflight cluster.json --cloud aws --tier static --offline --verbose 2>&1 > /dev/null | sed -n '1,6p'",
    );
    r.step(
        "preflight-live",
        0,
        false,
        "strata preflight cluster.json --cloud aws --no-exec --quiet",
    );
    r.step(
        "preflight-strict",
        1,
        false,
        "strata preflight cluster.json --cloud aws --no-exec --strict --quiet | tail -n 3",
    );
    preflight_cluster(r);
    preflight_report(r);
    preflight_each(r);
    preflight_account(r);
    preflight_clouds(r);
}

/// The part of the chapter `preflight` for a cluster that Palette holds, and a change of its worker
/// pool.
fn preflight_cluster(r: &mut Runner) {
    // Once Running, the page shows the cluster as Palette holds it. It also shows a change of the
    // worker pool before the change is made, and one reading of the watch. The cluster of the
    // stand-in is Running, so the watch ends at once. A real build reads every 30 seconds until
    // Running. The cluster gives the variable of its profile a value, and the entity of the
    // cluster carries it.
    r.tenant(|t| {
        t.cluster(
            DEV,
            PAYMENTS,
            "payments",
            &[A111],
            &[("platform_owner", "team-payments")],
        )
    });
    r.write_file(
        "change.json",
        r#"{"spec": {"machinepoolconfig": [{"poolConfig": {"name": "worker-pool"}, "cloudConfig": {"instanceType": "m5.xlarge"}}]}}"#,
    );
    r.step(
        "preflight-cluster",
        0,
        false,
        &format!("strata preflight cluster:{PAYMENTS} --tier static,palette --quiet | tail -n 4"),
    );
    r.step(
        "preflight-cluster-variables",
        0,
        false,
        &format!(
            "strata preflight cluster:{PAYMENTS} --tier static --quiet --out cluster-world > /dev/null; \
             jq -c '.spec.profiles[] | {{uid, variables}}' cluster-world/entity.json"
        ),
    );
    r.step(
        "preflight-change",
        0,
        false,
        &format!(
            "strata preflight cluster:{PAYMENTS} change.json --tier static --quiet | grep -A 1 WARN"
        ),
    );
    r.step(
        "preflight-watch",
        0,
        false,
        &format!("strata preflight cluster:{PAYMENTS} --watch --limit 1 --verbose --color never 2>&1 > /dev/null | grep -v 'reads the cluster' | sed -n '1,12p'"),
    );
}

/// The part of the chapter `preflight` for the forms of the report and the catalog of rules.
fn preflight_report(r: &mut Runner) {
    r.step(
        "preflight-json",
        0,
        false,
        "strata preflight cluster.json --cloud aws --no-exec --json --quiet \
  | jq '.checks[] | select(.id == \"worker-cpu-floor\") | {id, tier, level, verdict, messages, reproduce}'",
    );
    r.step(
        "preflight-json-summary",
        0,
        false,
        "strata preflight cluster.json --cloud aws --no-exec --json --quiet | jq '{summary, scorecard: [.sections[] | {title, ran, passed, failed, notChecked, readiness}]}'",
    );
    r.step(
        "preflight-out",
        0,
        false,
        "strata preflight cluster.json --cloud aws --no-exec --quiet --out preflight > /dev/null; ls preflight",
    );
    r.step(
        "preflight-rules",
        0,
        false,
        "strata preflight --rules --cloud aws | head -n 12",
    );
    r.step(
        "preflight-rules-json",
        0,
        false,
        "strata preflight --rules --cloud aws --json | jq '.rules[] | select(.id == \"worker-cpu-floor\") | {id, tier, level, severity, status, engines, check: .check.kind}'",
    );
    // a rule that left the catalog is an id that no rule has: a usage error, before any request
    r.step(
        "preflight-rule-unknown",
        2,
        true,
        "strata preflight cluster.json --cloud aws --offline --rule gke-pool-multiple-3",
    );
    // the three rules of the CAPA stack of an AWS account (#365)
    r.step(
        "preflight-capa-rules",
        0,
        false,
        "strata preflight --rules --cloud aws | grep -E '^  capa-(bootstrap|stack)'",
    );
    r.step(
        "preflight-promotion",
        0,
        false,
        "strata preflight --promotion | tail -n 2",
    );
    // The runs of the lab have no release, so a named release counts none of them (#392).
    r.step(
        "preflight-promotion-release",
        1,
        false,
        "strata preflight --promotion --release 4.10 | grep -E '^(rule  |gke-no-release-channel)'",
    );
    preflight_promotion_upgrade(r);
}

/// The rule of a company that a run on 4.11 promoted.
const LANDING_ZONE: &str = r#"section: landing-zone
title: The landing zone
rules:
  - id: example-region
    title: The cluster is in an allowed region
    engines: [aws]
    tier: static
    level: preflight
    promotion:
      runs: ["20261001T090000Z"]
    status: seen
    what: The landing zone holds us-east-2 and eu-west-1.
    check:
      kind: static
      expr: |
        .entity.spec.cloudConfig.region as $r
        | if (["us-east-2", "eu-west-1"] | index($r)) != null then []
          else ["region \($r) is not in the landing zone"] end
"#;

/// A ledger of one run of the rule `example-region`: the run made the fault, and a clean run
/// passed. `run` is the run id, and `release` is the Palette release of the run.
fn landing_zone_run(run: &str, release: &str) -> String {
    format!(
        "schema: strata-preflight-ledger/1\nruns:\n  - run: {run}\n    engine: aws\n    release: \"{release}\"\n    results:\n      - {{rule: example-region, result: caught}}\n      - {{rule: example-region, result: passed}}\n"
    )
}

/// The part of the chapter `preflight` for the level of a rule after a Palette upgrade (#392).
fn preflight_promotion_upgrade(r: &mut Runner) {
    r.write_file("landing-zone.yaml", LANDING_ZONE);
    r.write_file(
        "runs-4-11.yaml",
        &landing_zone_run("20261001T090000Z", "4.11.0"),
    );
    r.write_file(
        "runs-4-12.yaml",
        &landing_zone_run("20261101T090000Z", "4.12.0"),
    );
    let promotion = |release: &str, ledgers: &str| {
        format!(
            "strata preflight --promotion --release {release} --probes landing-zone.yaml {ledgers} | grep -E '^(rule  |example-region)'"
        )
    };
    // the runs are of 4.11 alone: the rule is a preflight check on 4.11 and a probe on 4.12
    r.step(
        "preflight-promotion-upgrade",
        0,
        false,
        &promotion("4.11", "--ledger runs-4-11.yaml"),
    );
    r.step(
        "preflight-promotion-upgrade",
        1,
        false,
        &promotion("4.12", "--ledger runs-4-11.yaml"),
    );
    r.step(
        "preflight-promotion-run",
        0,
        false,
        &promotion("4.12", "--ledger runs-4-11.yaml --ledger runs-4-12.yaml"),
    );
}

/// The part of the chapter `preflight` for a rule over a list.
fn preflight_each(r: &mut Runner) {
    // a rule over a list asks once for each item: the entity names a second profile, and a
    // cluster template links it
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.tenant(|t| t.cluster_template(DEV, AWS_STANDARD, "aws-standard", &[A110]));
    r.quiet(&format!(
        r#"jq '.spec.profiles += [{{"uid": "{A110}", "packValues": [], "variables": []}}]' cluster.json > cluster-addon.json"#
    ));
    r.step(
        "preflight-each",
        1,
        false,
        "strata preflight cluster-addon.json --cloud aws --no-exec --tier palette --rule template-outside --strict --quiet | sed -n '4,9p'",
    );
    r.step(
        "preflight-each-verbose",
        1,
        false,
        "strata preflight cluster-addon.json --cloud aws --no-exec --tier palette --rule template-outside --strict --verbose --color never 2>&1 > /dev/null",
    );
    r.step(
        "preflight-rules-each",
        0,
        false,
        "strata preflight --rules --cloud edge --json | jq '.rules[] | select(.id == \"edge-host-in-use\") | {id, tier, check: {kind: .check.kind, method: .check.method, each: (.check.each | gsub(\"\\\\s+\"; \" \") | rtrimstr(\" \")), path: .check.path}}'",
    );
}

/// The part of the chapter `preflight` for the rules that read the account of the entity.
fn preflight_account(r: &mut Runner) {
    // Two rules read the account of the entity (#143). The instance type route lists the zones
    // that do not offer a type. The key pair route lists the key pairs of the region. The pools
    // move to the zone that the list names, and the key pair is not in the list. The key pair step
    // uses an EKS body with a dynamic VPC. The AWS property route checks the key pair of an AWS
    // body.
    r.quiet(
        r#"jq '.spec.machinepoolconfig[].cloudConfig.azs = ["us-east-2c"]' cluster.json > cluster-zone.json"#,
    );
    r.step(
        "preflight-offering",
        1,
        false,
        "strata preflight cluster-zone.json --cloud aws --no-exec --tier palette --rule instance-offering --strict --quiet | sed -n '4,8p'",
    );
    // the route gives null for a type that each zone offers: t3.large passes in the same zone
    r.quiet(
        r#"jq '.spec.machinepoolconfig[].cloudConfig.instanceType = "t3.large"' cluster-zone.json > cluster-null.json"#,
    );
    r.step(
        "preflight-offering-null",
        0,
        false,
        "strata preflight cluster-null.json --cloud aws --no-exec --tier palette --rule instance-offering --strict --quiet | sed -n '4p'",
    );
    r.quiet(
        r#"jq '.spec.cloudConfig.sshKeyName = "retired" | .spec.cloudConfig.vpcId = ""' cluster.json > cluster-key.json"#,
    );
    r.step(
        "preflight-ssh-key",
        1,
        false,
        "strata preflight cluster-key.json --cloud eks --no-exec --tier palette --rule ssh-key-pair --strict --quiet | sed -n '4,6p'",
    );
}

/// The part of the chapter `preflight` for a profile of another cloud type, and the GCP properties.
fn preflight_clouds(r: &mut Runner) {
    // A profile of another cloud type (#415). The validate route refuses the body with HTTP 500 and
    // the code ClusterProfilesNotSupported, and palette-validate reports the refusal as a failure
    r.profile(
        DEV,
        GCP_STACK,
        "1.0.0",
        "aws-stack@1.0.0",
        &[Edit::Name("gcp-stack"), Edit::CloudType("gcp")],
    );
    r.quiet(&format!(
        r#"jq '.spec.profiles += [{{"uid": "{GCP_STACK}", "packValues": [], "variables": []}}]' cluster.json > cluster-gcp.json"#
    ));
    r.step(
        "preflight-cloud-mismatch",
        1,
        false,
        "strata preflight cluster-gcp.json --cloud aws --no-exec --tier palette --rule palette-validate --strict --quiet | sed -n '4,8p'",
    );
    // the property route of GCP reads the project and the zones of a GCP body. The stand-in
    // gives the answer of a route that accepts them, as it does for the route of AWS
    r.answer(DEV, "v1/clouds/gcp/properties/validate", &json!({}));
    r.quiet(
        r#"jq '.spec.cloudConfig = {"project": "strata-lab", "region": "us-central1"} | .spec.machinepoolconfig[].cloudConfig = {"instanceType": "n1-standard-4", "azs": ["us-central1-a"]}' cluster.json > gcp-cluster.json"#,
    );
    r.step(
        "preflight-gcp-properties",
        0,
        false,
        "strata preflight gcp-cluster.json --cloud gcp --no-exec --tier palette --rule gcp-properties --strict --quiet | sed -n '4p'",
    );
    r.step(
        "preflight-gcp-properties",
        0,
        false,
        "strata preflight gcp-cluster.json --cloud gcp --no-exec --tier palette --rule gcp-properties --verbose --color never 2>&1 > /dev/null | grep 'POST'",
    );
}
