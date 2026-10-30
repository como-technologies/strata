//! Each chapter builds the tenant that one page needs and runs the commands that page shows.
//!
//! A chapter reads as the page does: the tenant, then the steps, with a change to the tenant
//! where the page describes one. `serve_here` marks a state the playground can hand over.

use crate::book::Runner;
use crate::tenant::{Edit, PackRefusal, rbacs_route};
use serde_json::json;

const DEV: &str = "64f20000000000000000a0d1";
const PROD: &str = "64f20000000000000000a0f1";

// the profile uids of the examples, in dev
const A100: &str = "64e0000000000000000000a1";
const A110: &str = "64e0000000000000000000b2";
const A111: &str = "64e0000000000000000000c3";
// other profile and cluster uids
const A111_PROD: &str = "64e1000000000000000000c3";
const A110_PROD: &str = "64e1000000000000000000b2";
const A100_PROD: &str = "64e1000000000000000000a1";
const A112: &str = "64e0000000000000000000d4";
const A113: &str = "64e0000000000000000000d5";
const A120: &str = "64e0000000000000000000e5";
// aws-stack@1.1.4: the content of 1.1.1, whose CSI values read a project macro, in dev and prod
const A114: &str = "64e0000000000000000000f7";
const A114_PROD: &str = "64e1000000000000000000f7";
// aws-stack@1.1.5: the content of 1.1.1, with no digest for the CSI pack
const A115: &str = "64e0000000000000000000f8";
// aws-stack@1.1.6: the content of 1.1.1, with one more kubeadm command in the Kubernetes pack
const A116: &str = "64e0000000000000000000f9";
// aws-stack@1.1.7: the content of 1.1.1, with Kubernetes 1.36.3 in place of 1.36.2
const A117: &str = "64e0000000000000000000fa";
// aws-stack@1.1.8: the content of 1.1.1, with an OS pack that names its machine image
const A118: &str = "64e0000000000000000000fb";
// aws-stack@1.1.9: the content of 1.1.1, whose cert-manager tag the registry deletes
const A119: &str = "64e0000000000000000000fc";
// aws-stack@1.1.10: the content of 1.1.1, with a secret in the cert-manager values, in dev and
// prod
const A1110: &str = "64e0000000000000000000fd";
const A1110_PROD: &str = "64e1000000000000000000fd";
// aws-stack@1.1.11: the content of 1.1.1, with the CSI pack at 1.63.1, the one pin that is behind
const A1111: &str = "64e0000000000000000000ff";
// aws-stack@1.3.0: the content of 1.1.1, made with no publish
const A130: &str = "64e0000000000000000000fe";
// aws-stack@1.1.1 of the tenant: the content of 1.1.1, published at tenant scope
const A111_TENANT: &str = "64e4000000000000000000c3";
const ADDONS_DEV: &str = "64e2000000000000000000a1";
const ADDONS_PROD: &str = "64e3000000000000000000a1";
const ADDONS_PROD2: &str = "64e3000000000000000000a2";
// charts-probe@1.0.0: one chart of a private Helm registry, and the pack that Palette made for it
const CHARTS_PROBE: &str = "64ed000000000000000000c1";
const CHART_PACK: &str = "64ed000000000000000000b1";
const A112_PROD: &str = "64e1000000000000000000d4";
const PAYMENTS: &str = "64f1000000000000000000e1";
// the cloud account platform-aws of the preflight chapter and the loop chapter
const PLATFORM_AWS: &str = "64f3000000000000000000a1";
// gcp-stack@1.0.0: a profile of the cloud type gcp, which the preflight chapter puts in the body of an
// AWS create
const GCP_STACK: &str = "64f3000000000000000000b1";
const SEARCH: &str = "64f1000000000000000000e2";
const LEDGER: &str = "64f1000000000000000000e3";
const LEGACY: &str = "64f1000000000000000000e7";
const PAYMENTS_PROD: &str = "64f1000000000000000000f1";
// a cluster on vSphere, for the shape tier of another cloud
const LAB: &str = "64f1000000000000000000e6";
// the virtual clusters of a cluster group
const TENANT_A: &str = "64f1000000000000000000e4";
const TENANT_B: &str = "64f1000000000000000000e5";
const TEAM_A_APPS: &str = "64e2000000000000000000b1";
const AWS_STANDARD: &str = "64f8000000000000000000a1";
const DEV_GROUP: &str = "64f5000000000000000000a1";
// the saved tag filter of the loop
const PRODUCTION: &str = "64f7000000000000000000a2";
// edge hosts, and an app profile
const STORE_1: &str = "64f4000000000000000000e1";
const STORE_2: &str = "64f4000000000000000000e2";
const SHOP: &str = "64f6000000000000000000a1";
const SHOP_DEV: &str = "64f7000000000000000000a1";
// the manifest pack of platform-addons
const ADDONS_PACK: &str = "64e2000000000000000000d1";

const RETAIN: &[Edit] = &[Edit::Reclaim("Retain")];
const TAG_2404: &[Edit] = &[Edit::Tag {
    pack: "ubuntu-aws",
    tag: "24.04",
}];
const TAG_LTS2404: &[Edit] = &[Edit::Tag {
    pack: "ubuntu-aws",
    tag: "LTS__24.04",
}];
const OCI_CSI: &[Edit] = &[Edit::OciCsi];
const IMAGE_1: &[Edit] = &[Edit::ImageId {
    pack: "ubuntu-aws",
    id: "ami-0a1b2c3d4e5f60001",
}];
const IMAGE_2: &[Edit] = &[Edit::ImageId {
    pack: "ubuntu-aws",
    id: "ami-0a1b2c3d4e5f60002",
}];
const CERT_MANAGER_TAG_DELETED: &[Edit] = &[Edit::Invalid {
    pack: "cert-manager",
    reason: "Cert Manager 1.21.1 tag is deleted",
}];
const SECRET_VALUE: &[Edit] = &[Edit::Masked {
    pack: "cert-manager",
    key: "registryPassword",
}];
const MACRO_REGION: &[Edit] = &[Edit::Macro {
    pack: "csi-aws-ebs",
    name: "region",
}];

/// The body of an AWS create named `name`, for the profile `profile`: the cloud account
/// platform-aws, and two pools of t3.xlarge in one zone, with one worker.
fn preflight_entity(name: &str, profile: &str) -> String {
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
const AWS_VALIDATE: &str = "v1/spectroclusters/aws/validate";

/// The answer of the validate route when a pool has too few cores for the packs: the one worker of
/// `preflight_entity` has 4 cores, and the packs need 4.1.
fn cpu_floor_answer() -> serde_json::Value {
    json!({"machinePools": {"results": [{"name": "worker-pool", "displayName": "worker-pool",
        "errors": [{"code": "MachinePoolResourcesInsufficient",
            "message": "A minimum allocation of '4.1' CPU cores is required across all worker nodes"}]}]},
        "profiles": []})
}

/// The answer of the validate route when it finds no fault.
fn accepted_answer() -> serde_json::Value {
    json!({"machinePools": {"results": []}, "profiles": []})
}

/// One entry of the answer of the instance type route, as Palette gives it: the name of the type is
/// `type`, and an entry has no `name`. `not_offered` is `None` for a type that each zone offers:
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

/// The answer of the instance type route for `us-east-2`: each zone offers `t3.large`, and
/// `t3.xlarge` is not offered in the zone `us-east-2c`, which no pool of `preflight_entity` uses.
fn instance_types_answer() -> serde_json::Value {
    json!({"instanceTypes": [
        instance_type("t3.large", 2, 8, 0.0832, None),
        instance_type("t3.xlarge", 4, 16, 0.1664, Some(&["us-east-2c"])),
    ]})
}

/// Write the answers that the live tier of `strata preflight` reads for `preflight_entity` in
/// `project`: the cloud account, the property route, the regions, the instance types of the region,
/// the key pairs of the account, and the validate route, which refuses the worker pool for its CPU.
fn preflight_routes(r: &mut Runner, project: &str) {
    r.answer(
        project,
        &format!("v1/cloudaccounts/aws/{PLATFORM_AWS}"),
        &json!({"kind": "aws", "metadata": {"name": "platform-aws", "uid": PLATFORM_AWS},
            "spec": {"credentialType": "secret", "partition": "aws", "policyARNs": []}, "status": {"state": "Active"}}),
    );
    // the property route of AWS answers 204 with no body; the stand-in serves 200 with an empty
    // object, which a rule reads the same way (`ok`); the regions of the account, with the query
    // in the file name as the stand-in maps it
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

/// The git settings that the merge-gate chapters need, so a commit is the same on every machine.
const GIT: &[(&str, &str)] = &[
    ("PALETTE_PROJECT", "dev"),
    ("GIT_CONFIG_GLOBAL", "/dev/null"),
    ("GIT_CONFIG_SYSTEM", "/dev/null"),
    ("GIT_AUTHOR_NAME", "dev"),
    ("GIT_AUTHOR_EMAIL", "dev@example.com"),
    ("GIT_COMMITTER_NAME", "dev"),
    ("GIT_COMMITTER_EMAIL", "dev@example.com"),
    ("GIT_AUTHOR_DATE", "2026-09-10T10:00:00+00:00"),
    ("GIT_COMMITTER_DATE", "2026-09-10T10:00:00+00:00"),
];

const DEV_ENV: &[(&str, &str)] = &[("PALETTE_PROJECT", "dev")];

/// Run every chapter against `r`.
pub fn run(r: &mut Runner) {
    getting_started(r);
    the_loop(r);
    commands(r);
    verify_objects(r);
    use_cases(r);
    automation(r);
}

/// What `verify` says when the object of a record changed its state, went, or took another name.
fn verify_objects(r: &mut Runner) {
    r.chapter("verify-objects", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111], &[]));
    r.quiet("strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json");
    r.quiet("strata snapshot cluster:payments -o runs/payments.json");
    // the cluster goes: it runs the same copy, and the verdict says the state
    r.tenant(|t| t.cluster_field(DEV, PAYMENTS, "/status/state", json!("Deleting")));
    r.step(
        "verify-state-full",
        0,
        false,
        "strata verify runs/payments.json",
    );
    r.excerpt("verify-state", "verify-state-full", r"^OK:");
    // Palette deletes the cluster, and keeps its document
    r.tenant(|t| t.delete_cluster(DEV, PAYMENTS, "2026-09-17T11:05:00Z"));
    r.step(
        "verify-deleted-full",
        1,
        false,
        "strata verify runs/payments.json",
    );
    r.excerpt("verify-deleted", "verify-deleted-full", r"^DRIFT:");
    // a rename keeps the uid and the version of the profile
    r.tenant(|t| t.edit(DEV, A111, &[Edit::Name("aws-stack-legacy")]));
    r.step(
        "verify-renamed-full",
        1,
        false,
        "strata verify releases/1.1.1/dev.json",
    );
    r.excerpt("verify-renamed", "verify-renamed-full", r"^DRIFT:");
    r.step(
        "verify-objects-bulk",
        1,
        false,
        "strata verify releases/1.1.1/dev.json runs/payments.json",
    );
}

fn getting_started(r: &mut Runner) {
    r.chapter("help", &[]);
    r.step("help", 0, false, "strata --help");
    for c in [
        "list",
        "packs",
        "snapshot",
        "candidate",
        "compare",
        "same-release",
        "verify",
        "consumers",
        "behind",
        "schema",
        "preflight",
    ] {
        r.step(
            &format!("help-{c}"),
            0,
            false,
            &format!("strata {c} --help"),
        );
    }

    r.chapter("quickstart", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A110], &[]));
    r.step("quickstart-offline", 1, false, "strata compare examples/profiles/aws-stack@1.1.0.json examples/profiles/aws-stack@1.1.1.json");
    r.step("quickstart-list", 0, false, "strata list");
    r.step(
        "quickstart-snapshot",
        0,
        true,
        "strata snapshot aws-stack@1.1.0 -o aws-stack-1.1.0.json",
    );
    r.step(
        "quickstart-verify",
        0,
        false,
        "strata verify aws-stack-1.1.0.json",
    );
    r.step(
        "quickstart-cluster",
        0,
        false,
        "strata compare aws-stack-1.1.0.json cluster:payments",
    );
    r.step(
        "quickstart-versions",
        1,
        false,
        "strata compare aws-stack@1.1.0 aws-stack@1.1.1",
    );
}

/// The labels that put a cluster of the loop in its environment, and in the fleet of its filter.
fn label(r: &mut Runner, project: &str, uid: &str, env: &str) {
    r.tenant(|t| {
        t.cluster_field(
            project,
            uid,
            "/metadata/labels",
            json!({"env": env, "managed-by": "opentofu"}),
        )
    });
}

/// The everyday loop: one change of one team, from the pull request to the retire of the
/// version that it replaces. Each step shows the few lines that answer its question; the use
/// case of the step shows the rest.
fn the_loop(r: &mut Runner) {
    r.chapter("loop", GIT);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.tenant(|t| t.registries(DEV));
    r.tenant(|t| t.registries(PROD));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    r.profile(PROD, A100_PROD, "1.0.0", "aws-stack@1.0.0", &[]);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A100], &[]));
    r.tenant(|t| t.cluster(PROD, PAYMENTS_PROD, "payments", &[A100_PROD], &[]));
    label(r, DEV, PAYMENTS, "dev");
    label(r, PROD, PAYMENTS_PROD, "prod");
    r.tenant(|t| t.tag_filter("production", PRODUCTION, "env", "prod"));
    // Palette takes the name aws-stack@1.1.0 and the packs of the candidate, which it does not hold
    r.tenant(|t| t.profile_create_checks(DEV, ("aws-stack", "1.1.0"), None, None));
    r.tenant(|t| t.profile_packs_answer(DEV, None, None));
    // the repository of the team: main holds the record of what each project runs
    r.quiet("git init -q -b main . && git remote add origin .");
    r.quiet("strata snapshot aws-stack@1.0.0 -o releases/1.0.0/dev.json &&\n  PALETTE_PROJECT=prod strata snapshot aws-stack@1.0.0 -o releases/1.0.0/prod.json &&\n  git add releases && git commit -qm \"dev and prod run aws-stack@1.0.0\"");
    r.serve_here(); // state 1: dev and prod run 1.0.0, and main records it

    // 1. a developer changes the profile in a pull request
    r.quiet("git checkout -q -b aws-stack-1.1.0");
    r.write_file("aws-stack.yaml", CANDIDATE_YAML);
    r.step(
        "loop-candidate",
        0,
        true,
        "strata snapshot --from-candidate aws-stack.yaml --validate -o candidate.json",
    );
    r.quiet("git add aws-stack.yaml && git commit -qm \"aws-stack 1.1.0: Kubernetes 1.36, Ubuntu 24.04\"");
    r.step(
        "loop-review",
        0,
        true,
        "scripts/ci/pr-comment.sh releases/1.0.0/dev.json candidate.json --dry-run > comment.md",
    );
    r.step("loop-review", 0, false, "grep '^\\*\\*Result' comment.md");
    // 2. the merge gate: the records after the merge button
    r.step(
        "loop-merge",
        0,
        true,
        r#"scripts/ci/merge-gate.sh "releases/*/*.json" --base main"#,
    );
    r.quiet("git checkout -q main && git merge -q --no-ff --no-edit aws-stack-1.1.0");

    // 3. the pipeline of the team publishes the version, and records it
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    // Palette answers for the move of payments to the version: it repaves, and it takes the packs
    r.tenant(|t| t.repave_answer(DEV, PAYMENTS, true));
    r.tenant(|t| t.packs_answer(DEV, PAYMENTS, None));
    r.serve_here(); // state 2: the pipeline published 1.1.0 in dev
    r.step(
        "loop-publish",
        0,
        true,
        "strata snapshot aws-stack@1.1.0 -o releases/1.1.0/dev.json",
    );
    r.step(
        "loop-publish",
        0,
        false,
        "strata same-release candidate.json releases/1.1.0/dev.json",
    );
    r.quiet("git add releases && git commit -qm \"record aws-stack@1.1.0 in dev\"");
    // before dev moves payments, the pipeline asks Palette about the move
    r.step(
        "loop-ask-repave",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.0 --repave | grep -E '^(aws-stack|root |palette)'",
    );
    r.step(
        "loop-ask-validate",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.0 --validate | grep -E '^(aws-stack|root |palette)'",
    );
    // dev moves payments to the new version, and a record says what the cluster runs
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A110], &[]));
    label(r, DEV, PAYMENTS, "dev");
    r.step(
        "loop-cluster",
        0,
        false,
        "strata compare releases/1.1.0/dev.json cluster:payments | head -2",
    );

    // 4. the promotion to prod, on demand
    r.profile(PROD, A110_PROD, "1.1.0", "aws-stack@1.1.0", &[]);
    r.quiet("PALETTE_PROJECT=prod strata snapshot aws-stack@1.1.0 -o releases/1.1.0/prod.json");
    r.step(
        "loop-promote",
        0,
        false,
        "strata same-release releases/1.1.0/dev.json releases/1.1.0/prod.json",
    );
    r.tenant(|t| t.cluster(PROD, PAYMENTS_PROD, "payments", &[A110_PROD], &[]));
    label(r, PROD, PAYMENTS_PROD, "prod");
    r.quiet("git add releases && git commit -qm \"promote aws-stack@1.1.0 to prod\"");

    // 5. before a build: the team writes the body of the create of a cluster that prod builds on
    // the release, and asks Palette about it, with nothing created
    r.write_file("cluster.json", &preflight_entity("ledger", A110_PROD));
    preflight_routes(r, PROD);
    r.serve_here(); // state 3: both projects run 1.1.0, main records it, and Palette refuses a worker pool
    r.step(
        "loop-body",
        0,
        false,
        "jq -c '{name: .metadata.name, profile: .spec.profiles[0].uid, pools: [.spec.machinepoolconfig[].poolConfig | {name, size}]}' cluster.json",
    );
    r.step(
        "loop-preflight-static",
        0,
        false,
        "PALETTE_PROJECT=prod strata preflight cluster.json --cloud aws --tier static --offline --quiet | grep -E '^(OK|FAIL)'",
    );
    r.step(
        "loop-preflight-live",
        1,
        false,
        r#"PALETTE_PROJECT=prod strata preflight cluster.json --cloud aws --no-exec --strict --json --quiet | jq -r '.checks[] | select(.verdict == "failed" and .severity == "fail") | "\(.id): \((.messages // []) | join("; "))"'"#,
    );
    // the fix is two workers; Palette then finds no fault
    r.step(
        "loop-preflight-fix",
        0,
        false,
        r#"jq '(.spec.machinepoolconfig[] | select(.poolConfig.name == "worker-pool") | .poolConfig.size) = 2' cluster.json > fixed.json && mv fixed.json cluster.json && jq -c '.spec.machinepoolconfig[].poolConfig | {name, size}' cluster.json"#,
    );
    r.answer(PROD, AWS_VALIDATE, &accepted_answer());
    r.serve_here(); // state 4: Palette finds no fault in the body with two workers
    r.step(
        "loop-preflight-clean",
        0,
        false,
        "PALETTE_PROJECT=prod strata preflight cluster.json --cloud aws --no-exec --strict --quiet | grep -E '^(OK|FAIL)'",
    );

    // 6. the nightly jobs. The fleet first: the records of the clusters that Palette names prod.
    r.step(
        "loop-fleet",
        0,
        true,
        "strata snapshot filter:production --dir fleet/prod --prune",
    );
    r.quiet("git add fleet && git commit -qm \"the fleet of prod\"");
    // a person edits the published version in prod through the API, with no review
    r.tenant(|t| t.edit(PROD, A110_PROD, RETAIN));
    r.serve_here(); // state 5: somebody edited aws-stack@1.1.0 in prod in place
    r.step(
        "loop-nightly",
        1,
        true,
        "scripts/ci/verify-report.sh 'releases/**/*.json' 'fleet/**/*.json' --dry-run > issues.md",
    );
    r.step(
        "loop-who-full",
        1,
        false,
        "strata verify releases/1.1.0/prod.json --who",
    );
    r.excerpt(
        "loop-who",
        "loop-who-full",
        r"^(aws-stack|DRIFT|who:|  \d{4}-)",
    );
    r.step(
        "loop-upstream",
        1,
        true,
        "scripts/ci/upstream-report.sh 'releases/**/*.json' --dry-run --no-impact > upstream.md",
    );

    // 7. an incident: a pack has a security problem, and a person asks who carries it
    for project in ["dev", "prod"] {
        r.step(
            "loop-incident-full",
            0,
            false,
            &format!("PALETTE_PROJECT={project} strata list --pack csi-aws-ebs"),
        );
    }
    r.excerpt(
        "loop-incident",
        "loop-incident-full",
        r"^(\$|project|\d+ profiles?|  aws-stack|    csi)",
    );
    for project in ["dev", "prod"] {
        r.step(
            "loop-consumers",
            1,
            false,
            &format!("PALETTE_PROJECT={project} strata consumers aws-stack@1.1.0 --list-only"),
        );
    }

    // 8. the retire of the version that nothing runs now
    r.step("loop-retire", 0, false, "strata consumers aws-stack@1.0.0");
}

fn commands(r: &mut Runner) {
    r.chapter("list", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.profile(PROD, A111_PROD, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A110], &[]));
    r.tenant(|t| t.cluster(DEV, SEARCH, "search", &[A111], &[]));
    r.tenant(|t| t.cluster(PROD, PAYMENTS_PROD, "payments", &[A111_PROD], &[]));
    r.step("list", 0, false, "strata list");
    r.step("list-projects", 0, false, "strata list projects");
    r.step("list-prod", 0, false, "strata list --project-name prod");
    r.step("list-tenant", 0, false, "strata list clusters --project ''");
    r.step("list-applied", 0, false, "strata list clusters --applied");
    r.step("list-cluster", 0, false, "strata list --cluster payments");
    r.step(
        "list-cluster-tenant",
        0,
        false,
        "strata list --cluster payments --project ''",
    );
    r.step("list-pack", 0, false, "strata list --pack cilium");
    r.step(
        "list-pack-all",
        0,
        false,
        r#"for p in dev prod; do strata list --pack cilium --project-name "$p"; done"#,
    );
    r.step(
        "list-pack-tenant",
        0,
        false,
        "strata list --pack cilium --project ''",
    );
    r.step("list-packs", 0, false, "strata list --packs");
    r.step(
        "list-pack-consumers",
        1,
        false,
        "for v in 1.0.0 1.1.0 1.1.1; do strata consumers aws-stack@$v; done",
    );
    // the empty uid is the point of the step: a reader who exported one needs it, and the book
    // must print the command that works for them, not the one that works only here
    r.step(
        "list-pack-consumers",
        1,
        false,
        "PALETTE_PROJECT_UID= PALETTE_PROJECT=prod strata consumers aws-stack@1.1.1",
    );
    r.step("list-pack-json", 0, false, r#"strata list --pack cilium --json | jq -r '.profiles[] | .ref as $ref | .packs[] | select(.name | contains("cilium")) | "\($ref) \(.name) \(.version)"'"#);
    // a version that nobody published: its packs are the packs of its draft
    r.unpublished(DEV, A130, "1.3.0", "aws-stack@1.1.1", &[]);
    r.step("list-draft", 0, false, "strata list profiles --packs");
    r.step(
        "list-draft-json",
        0,
        false,
        r#"strata list profiles --packs --json | jq -r '.profiles[] | "\(.ref) published=\(.published)"'"#,
    );
    // an imported cluster in read-only mode: Palette lists it with an import type and the cloud
    // generic, and applies no add-on profile to it
    r.tenant(|t| t.cluster(DEV, LEGACY, "legacy", &[], &[]));
    r.tenant(|t| t.cluster_field(DEV, LEGACY, "/spec/clusterType", json!("AlloyMonitor")));
    r.tenant(|t| t.cluster_field(DEV, LEGACY, "/spec/cloudType", json!("generic")));
    r.step("list-imported", 0, false, "strata list clusters --applied");
    r.step(
        "list-imported-json",
        0,
        false,
        r#"strata list clusters --json | jq -r '.clusters[] | "\(.name) \(.clusterType)"'"#,
    );
    // a cluster template links a version that no cluster runs: the version has a consumer
    r.tenant(|t| t.cluster_template(DEV, AWS_STANDARD, "aws-standard", &[A100]));
    r.step(
        "list-consumers-full",
        0,
        false,
        "strata list profiles --packs",
    );
    r.excerpt("list-consumers", "list-consumers-full", r"^  aws-stack@");
    r.step(
        "list-consumers-json",
        0,
        false,
        r#"strata list profiles --packs --json | jq -c '.profiles[] | {ref, consumers, consumersByKind}'"#,
    );
    r.step(
        "list-consumers-same",
        1,
        false,
        "strata consumers aws-stack@1.0.0 --list-only",
    );

    // a profile of the tenant with the name and the version of a profile of the project
    r.chapter("scope", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant_profile(A111_TENANT, "1.1.1", "aws-stack@1.1.1", &[]);
    r.step("scope-list", 0, false, "strata list profiles");
    r.step(
        "scope-list-json",
        0,
        false,
        r#"strata list profiles --json | jq -r '.profiles[] | "\(.ref) \(.uid) \(.scope)"'"#,
    );
    r.step(
        "scope-ambiguous",
        2,
        true,
        "strata snapshot aws-stack@1.1.1 -o aws-stack.json",
    );
    r.step(
        "scope-by-uid",
        0,
        true,
        &format!("strata snapshot {A111_TENANT} -o tenant-aws-stack.json"),
    );
    r.step("scope-packs", 0, false, "strata list profiles --packs");
    r.step(
        "scope-tenant",
        0,
        false,
        "strata list profiles --project ''",
    );

    r.chapter("snapshot", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111], &[]));
    r.step(
        "snapshot-profile",
        0,
        true,
        "strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json",
    );
    r.step("snapshot-record", 0, false, r#"jq '{root, source: (.source | {ref, uid, projectUid, name, version, template, fetchedAt}), packs: [.packs[] | {key, version, node: .hash.node}]}' releases/1.1.1/dev.json"#);
    // What a ref that looks like a file gets when no such file is there; the refs page shows it.
    r.step(
        "refs-missing-file",
        2,
        true,
        "strata verify releases/1.1.1/missing.json",
    );
    r.step(
        "snapshot-latest",
        0,
        true,
        "strata snapshot aws-stack@latest -o latest.json",
    );
    r.step(
        "snapshot-latest-version",
        0,
        false,
        r#"jq -r '.source | "\(.name)@\(.version)"' latest.json"#,
    );
    r.step(
        "snapshot-cluster",
        0,
        true,
        "strata snapshot cluster:payments -o runs/payments.json",
    );
    r.step(
        "snapshot-cluster-source",
        0,
        false,
        "jq .source.cluster runs/payments.json",
    );
    r.step(
        "snapshot-raw",
        0,
        true,
        "strata snapshot aws-stack@1.1.1 --raw-dir raw/aws-stack-1.1.1 -o online.json",
    );
    r.step("snapshot-raw", 0, false, "ls raw/aws-stack-1.1.1");
    r.step(
        "snapshot-raw",
        0,
        true,
        "strata snapshot --from-raw raw/aws-stack-1.1.1 -o offline.json",
    );
    r.step(
        "snapshot-raw",
        0,
        false,
        "jq -r .root online.json offline.json",
    );

    r.chapter("candidate", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.registries(DEV));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.step(
        "candidate-write",
        0,
        true,
        "strata candidate aws-stack@1.1.1 -o aws-stack.yaml --files-dir aws-stack",
    );
    r.step("candidate-yaml", 0, false, "cat aws-stack.yaml");
    r.step("candidate-files", 0, false, "find aws-stack -type f | sort");
    r.step(
        "candidate-snapshot",
        0,
        true,
        "strata snapshot --from-candidate aws-stack.yaml -o candidate.json",
    );
    r.step(
        "candidate-published",
        0,
        true,
        "strata snapshot aws-stack@1.1.1 -o published.json",
    );
    r.step(
        "candidate-same",
        0,
        false,
        "strata same-release candidate.json published.json",
    );
    // the export of Palette is a candidate too: the export route gives the file, and its record
    // has the root of the version (#156)
    r.tenant(|t| t.export(DEV, A111));
    r.quiet(&format!(
        "curl -sS -H 'Accept: application/octet-stream' -H 'ProjectUid: {DEV}' \
         \"$PALETTE_API/v1/clusterprofiles/{A111}/export?format=json\" -o aws-stack-export.json"
    ));
    r.step(
        "candidate-export",
        0,
        false,
        "jq -c '.spec.template.packs[] | {name, type, layer, version, tag, registry: .registry.metadata.name}' aws-stack-export.json",
    );
    r.step(
        "candidate-export-record",
        0,
        true,
        "strata snapshot --from-candidate aws-stack-export.json -o from-export.json",
    );
    r.step(
        "candidate-export-same",
        0,
        false,
        "strata same-release from-export.json published.json",
    );
    // Palette's own checks before a create (#273). The candidate comes from aws-stack@1.1.1, which
    // the project holds, so the name route and the import validate route refuse it
    r.tenant(|t| t.profile_packs_answer(DEV, None, None));
    let exists = (
        409,
        "ClusterProfileAlreadyExists",
        "Cluster Profile with name 'aws-stack' and version '1.1.1' already exists",
    );
    r.tenant(|t| t.profile_create_checks(DEV, ("aws-stack", "1.1.1"), Some(exists), Some(exists)));
    r.step(
        "candidate-validate-exists",
        2,
        true,
        "strata snapshot --from-candidate aws-stack.yaml --validate -o validated.json",
    );
    // a version that Palette does not parse: both routes refuse it, and strata prints it once
    r.quiet("sed 's/^version: 1.1.1$/version: 1.1.1.1/' aws-stack.yaml > bad-version.yaml");
    let unparsed = (
        500,
        "VersionParseFailure",
        "Unable to parse the version '1.1.1.1'",
    );
    r.tenant(|t| {
        t.profile_create_checks(
            DEV,
            ("aws-stack", "1.1.1.1"),
            Some(unparsed),
            Some(unparsed),
        )
    });
    r.step(
        "candidate-validate-version",
        2,
        true,
        "strata snapshot --from-candidate bad-version.yaml --validate -o bad-version.json",
    );
    // a new version: Palette takes the name, and the check of the packs refuses, then accepts
    r.quiet(
        "sed 's/^version: 1.1.1$/version: 1.2.0/' aws-stack.yaml > next.yaml && mv next.yaml aws-stack.yaml",
    );
    r.tenant(|t| t.profile_create_checks(DEV, ("aws-stack", "1.2.0"), None, None));
    r.tenant(|t| {
        t.profile_packs_answer(DEV, None, Some(("cni-cilium-oss", "kubernetes", "1.37.0")))
    });
    r.step(
        "candidate-validate-refuses",
        2,
        true,
        "strata snapshot --from-candidate aws-stack.yaml --validate -o validated.json",
    );
    r.step(
        "candidate-validate-refuses",
        0,
        false,
        "test -e validated.json || echo no record",
    );
    r.tenant(|t| t.profile_packs_answer(DEV, None, None));
    r.step(
        "candidate-validate-accepts",
        0,
        true,
        "strata snapshot --from-candidate aws-stack.yaml --validate -o validated.json",
    );
    r.step(
        "candidate-validate-accepts",
        0,
        false,
        "jq -c .source.paletteValidation validated.json",
    );
    // Spectro Cloud marks a version that a profile must not hold. Palette serves it, and the import
    // validate route refuses a candidate that holds it (#405)
    r.tenant(|t| t.mark_pack_disabled(DEV, "cert-manager", "1.21.1"));
    let disabled = (
        500,
        "DisabledPacksForbidden",
        "Disabled packs 'cert-manager' are forbidden",
    );
    r.tenant(|t| t.profile_create_checks(DEV, ("aws-stack", "1.2.0"), None, Some(disabled)));
    r.step(
        "candidate-validate-disabled",
        2,
        true,
        "strata snapshot --from-candidate aws-stack.yaml --validate -o validated.json",
    );
    r.tenant(|t| t.registries(DEV));
    r.tenant(|t| t.profile_create_checks(DEV, ("aws-stack", "1.2.0"), None, None));
    // a version that Palette holds, as an update of itself
    r.tenant(|t| t.profile_packs_answer(DEV, Some(A111), None));
    r.step(
        "snapshot-validate-version",
        0,
        true,
        "strata snapshot aws-stack@1.1.1 --validate -o checked.json",
    );
    r.step(
        "snapshot-validate-usage",
        2,
        true,
        "strata snapshot --from-raw raw/aws-stack-1.1.1 --validate",
    );
    // a template on cert-manager 1.21.1 with one input parameter and one option, as the listing
    // gives it: the option has no presets
    r.tenant(|t| {
        t.pack_template(
            DEV,
            "cert-manager",
            "1.21.1",
            &json!({"parameters": {"inputParameters": [{
                "name": "access", "displayName": "Access", "type": "string",
                "targetKey": "pack.access",
                "options": {"public": {"label": "Direct", "dependencies": []}},
            }]}}),
        )
    });
    r.write_file("certs.yaml", PRESET_CANDIDATE_YAML);
    r.step("candidate-preset-yaml", 0, false, "cat certs.yaml");
    r.step(
        "candidate-preset-warning",
        0,
        true,
        "strata snapshot --from-candidate certs.yaml -o certs.json",
    );
    r.step(
        "candidate-preset-options",
        0,
        false,
        "jq -c '.packs[0].presetOptions' certs.json",
    );
    // Palette refuses an os layer in an add-on profile only at the create, so strata refuses it
    // before any request
    r.quiet("sed 's/^type: cluster$/type: add-on/' aws-stack.yaml > add-on.yaml");
    r.step(
        "candidate-refused",
        2,
        true,
        "strata snapshot --from-candidate add-on.yaml -o add-on.json",
    );
    // Palette takes the layer of a pack from its registry, so strata refuses another layer
    r.quiet("sed 's/^  layer: addon$/  layer: cni/' aws-stack.yaml > wrong-layer.yaml");
    r.step(
        "candidate-layer-refused",
        2,
        true,
        "strata snapshot --from-candidate wrong-layer.yaml -o wrong-layer.json",
    );

    r.chapter("compare", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.profile(DEV, A112, "1.1.2", "aws-stack@1.1.1", RETAIN);
    r.step(
        "compare-identical",
        0,
        false,
        "strata compare aws-stack@1.1.1 aws-stack@1.1.1",
    );
    r.step(
        "compare-repave",
        1,
        false,
        "strata compare aws-stack@1.0.0 aws-stack@1.1.0",
    );
    r.step(
        "compare-images-count",
        1,
        false,
        "strata compare aws-stack@1.0.0 aws-stack@1.1.0 --json | jq -c '(.summary | {imagesChanged, chartsChanged}), [.packs[] | select(.images or .charts) | .name]'",
    );
    r.step(
        "compare-values",
        1,
        false,
        "strata compare aws-stack@1.1.1 aws-stack@1.1.2",
    );
    r.step(
        "compare-full",
        1,
        false,
        "strata compare aws-stack@1.1.1 aws-stack@1.1.2 --full",
    );
    r.step(
        "compare-markdown",
        1,
        false,
        "strata compare aws-stack@1.0.0 aws-stack@1.1.0 --format markdown",
    );
    r.step(
        "compare-json",
        1,
        false,
        "strata compare aws-stack@1.0.0 aws-stack@1.1.0 --json | jq .summary",
    );

    r.chapter("schema", &[]);
    r.step(
        "schema-diff",
        0,
        false,
        r#"strata schema diff | jq -r '.required | join(", ")'"#,
    );
}

const CANDIDATE_YAML: &str = "\
name: aws-stack
version: 1.1.0
cloudType: aws
registry: Public Repo
layers:
  - name: ubuntu-aws
    version: \"24.04\"
  - name: kubernetes
    version: 1.36.2
  - name: cni-cilium-oss
    version: 1.20.1
  - name: csi-aws-ebs
    version: 1.65.0
  - name: cert-manager
    version: 1.21.1
";

/// An add-on candidate whose values select the option `public` of the template of cert-manager.
const PRESET_CANDIDATE_YAML: &str = "\
name: certs
version: 1.0.0
type: add-on
registry: Public Repo
layers:
  - name: cert-manager
    version: 1.21.1
    values: |
      pack:
        namespace: cert-manager
        access: public
";

fn use_cases(r: &mut Runner) {
    r.chapter("review", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.registries(DEV));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    // the release under review runs somewhere: the repave has a target
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A100], &[]));
    r.step(
        "review-record",
        0,
        true,
        "strata snapshot aws-stack@1.0.0 -o releases/1.0.0/dev.json",
    );
    r.write_file("aws-stack.yaml", CANDIDATE_YAML);
    r.step("review-candidate", 0, false, "cat aws-stack.yaml");
    r.step(
        "review-snapshot",
        0,
        true,
        "strata snapshot --from-candidate aws-stack.yaml -o candidate.json",
    );
    r.step(
        "review-markdown",
        1,
        false,
        "strata compare releases/1.0.0/dev.json candidate.json --format markdown",
    );
    r.step("review-repave", 1, false, r#"strata compare releases/1.0.0/dev.json candidate.json --json | jq -r '.packs[] | select(.impact.level == "repave") | "\(.name): \(.impact.text)"'"#);
    r.step(
        "review-pr-comment",
        0,
        true,
        "scripts/ci/pr-comment.sh releases/1.0.0/dev.json candidate.json --dry-run > comment.md",
    );
    r.step("review-comment-head", 0, false, "head -3 comment.md");
    r.serve_here(); // the page describes what follows: the publication of the reviewed version
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.step(
        "review-published",
        0,
        true,
        "strata snapshot aws-stack@1.1.0 -o releases/1.1.0/dev.json",
    );
    r.step(
        "review-same",
        0,
        false,
        "strata same-release candidate.json releases/1.1.0/dev.json",
    );

    r.chapter("merge-gate", GIT);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.registries(DEV));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    // main pins aws-stack@1.0.0, and a branch changes the record: the profile was edited in place
    r.quiet("git init -q -b main . && git remote add origin .");
    r.quiet("strata snapshot aws-stack@1.0.0 -o releases/dev.json &&\n  git add releases && git commit -qm \"dev pins aws-stack@1.0.0\"");
    r.quiet("git checkout -q -b keep-the-volumes");
    r.tenant(|t| t.edit(DEV, A100, RETAIN));
    r.quiet("strata snapshot aws-stack@1.0.0 -o releases/dev.json &&\n  git commit -qam \"keep the volumes on delete\"");
    // while that branch waited, another merge request published 1.1.0 and pinned it on main
    r.quiet("git checkout -q main");
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.quiet("strata snapshot aws-stack@1.1.0 -o releases/dev.json &&\n  git commit -qam \"dev pins aws-stack@1.1.0\"");
    // the branch settled the conflict by keeping its own record, which is what a hand merge does
    r.quiet("git checkout -q keep-the-volumes && git fetch -q origin main &&\n  { git merge -q --no-edit --no-commit origin/main || true; } &&\n  git checkout HEAD -- releases/dev.json &&\n  git commit -qm \"merge main, keep our record\"");
    r.step(
        "merge-gate-behind",
        1,
        true,
        r#"scripts/ci/merge-gate.sh "releases/*.json" --base main"#,
    );
    // the fix: pin the version that main has, with the change on top
    r.quiet("strata snapshot aws-stack@1.1.0 -o releases/dev.json && git commit -qam \"pin aws-stack@1.1.0\"");
    r.step(
        "merge-gate-clear",
        0,
        true,
        r#"scripts/ci/merge-gate.sh "releases/*.json" --base main"#,
    );
    // and the check that git cannot make: the pinned profile is edited in Palette after the record
    r.tenant(|t| t.edit(DEV, A110, RETAIN));
    r.step(
        "merge-gate-drifted",
        1,
        true,
        r#"scripts/ci/merge-gate.sh "releases/*.json" --base main"#,
    );

    r.chapter("merge-gate-rollback", GIT);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.registries(DEV));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    // dev ran 1.0.0, and then 1.1.0
    r.quiet("git init -q -b main . && git remote add origin .");
    r.quiet("strata snapshot aws-stack@1.0.0 -o releases/dev.json &&\n  git add releases && git commit -qm \"dev pins aws-stack@1.0.0\"");
    r.quiet("strata snapshot aws-stack@1.1.0 -o releases/dev.json &&\n  git commit -qam \"dev pins aws-stack@1.1.0\"");
    // 1.1.0 repaved the nodes badly, so a branch off main puts back what dev ran before
    r.quiet("git checkout -q -b roll-dev-back &&\n  git show main~1:releases/dev.json > releases/dev.json &&\n  git commit -qam \"pin what dev ran before 1.1.0\"");
    r.step(
        "merge-gate-rollback-red",
        1,
        true,
        r#"scripts/ci/merge-gate.sh "releases/*.json" --base main"#,
    );
    // the same change, with the rollback declared in the commit that makes it
    r.quiet("git commit -q --amend -m \"roll dev back to aws-stack@1.0.0\n\nRollback: releases/dev.json to aws-stack@1.0.0\n1.1.0 repaved the nodes with a CSI driver that lost the volumes\"");
    r.step(
        "merge-gate-rollback",
        0,
        true,
        r#"scripts/ci/merge-gate.sh "releases/*.json" --base main"#,
    );

    r.chapter("promote", &[]);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    // the release under review runs somewhere, so the check discriminates
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111], &[]));
    // 1.1.0 reached prod by a promotion from the dev source, so its content is dev's content.
    r.profile(PROD, A110_PROD, "1.1.0", "aws-stack@1.1.0", &[]);
    // 1.1.1 was built by hand in prod, and one value differs.
    r.profile_own_copy(
        PROD,
        A111_PROD,
        "1.1.1",
        "aws-stack@1.1.1",
        RETAIN,
        "the promote chapter gives prod a changed value",
    );
    r.step(
        "promote-record",
        0,
        true,
        "PALETTE_PROJECT=dev strata snapshot aws-stack@1.1.0 -o releases/1.1.0/dev.json",
    );
    r.step(
        "promote-record",
        0,
        true,
        "PALETTE_PROJECT=prod strata snapshot aws-stack@1.1.0 -o releases/1.1.0/prod.json",
    );
    r.step(
        "promote-record",
        0,
        true,
        "PALETTE_PROJECT=dev strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json",
    );
    r.step(
        "promote-record",
        0,
        true,
        "PALETTE_PROJECT=prod strata snapshot aws-stack@1.1.1 -o releases/1.1.1/prod.json",
    );
    r.step(
        "promote-same",
        0,
        false,
        "strata same-release releases/1.1.0/dev.json releases/1.1.0/prod.json",
    );
    r.step(
        "promote-different",
        1,
        false,
        "strata same-release releases/1.1.1/dev.json releases/1.1.1/prod.json",
    );
    r.step(
        "promote-compare",
        1,
        false,
        "strata compare releases/1.1.1/dev.json releases/1.1.1/prod.json",
    );
    r.step(
        "promote-consumers",
        0,
        true,
        "PALETTE_PROJECT=prod strata consumers aws-stack@1.1.1",
    );
    // 1.1.4 reached prod by a promotion, and its CSI values read the macro region, which each
    // project gives another value: one release, two configurations (#164)
    r.profile(DEV, A114, "1.1.4", "aws-stack@1.1.1", MACRO_REGION);
    r.profile(PROD, A114_PROD, "1.1.4", "aws-stack@1.1.1", MACRO_REGION);
    r.tenant(|t| t.set_macro(DEV, "region", "us-east-2"));
    r.tenant(|t| t.set_macro(PROD, "region", "eu-west-1"));
    r.step(
        "promote-macro-record",
        0,
        true,
        "PALETTE_PROJECT=dev strata snapshot aws-stack@1.1.4 -o releases/1.1.4/dev.json",
    );
    r.step(
        "promote-macro-record",
        0,
        true,
        "PALETTE_PROJECT=prod strata snapshot aws-stack@1.1.4 -o releases/1.1.4/prod.json",
    );
    r.step(
        "promote-macro-not-compared",
        0,
        false,
        "strata same-release releases/1.1.4/dev.json releases/1.1.4/prod.json",
    );
    r.step(
        "promote-macro-record-digests",
        0,
        true,
        "PALETTE_PROJECT=dev strata snapshot aws-stack@1.1.4 --macros -o releases/1.1.4/dev-macros.json",
    );
    r.step(
        "promote-macro-record-digests",
        0,
        true,
        "PALETTE_PROJECT=prod strata snapshot aws-stack@1.1.4 --macros -o releases/1.1.4/prod-macros.json",
    );
    r.step(
        "promote-macro-changed",
        0,
        false,
        "strata same-release releases/1.1.4/dev-macros.json releases/1.1.4/prod-macros.json",
    );
    r.step(
        "promote-macro-changed-json",
        0,
        false,
        "strata same-release releases/1.1.4/dev-macros.json releases/1.1.4/prod-macros.json --json | jq -c '.pairs[] | {status, macrosChanged}'",
    );

    r.chapter("registry-move", &[]);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.profile_own_copy(
        PROD,
        A111_PROD,
        "1.1.1",
        "aws-stack@1.1.1",
        OCI_CSI,
        "the registry-move chapter gives prod the OCI pack",
    );
    // prod also holds 1.1.2, which moved the pack and changed a value on top of the move. Both
    // exist before the playground hands the tenant over, so every step of the page runs against it.
    r.profile_own_copy(
        PROD,
        A112_PROD,
        "1.1.2",
        "aws-stack@1.1.1",
        &[Edit::OciCsi, Edit::Reclaim("Retain")],
        "the move, and a changed value",
    );
    r.step(
        "move-record",
        0,
        true,
        "PALETTE_PROJECT=dev strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json",
    );
    r.step(
        "move-record",
        0,
        true,
        "PALETTE_PROJECT=prod strata snapshot aws-stack@1.1.1 -o releases/1.1.1/prod.json",
    );
    r.step(
        "move-exact",
        1,
        false,
        "strata same-release releases/1.1.1/dev.json releases/1.1.1/prod.json",
    );
    r.step(
        "move-compare",
        1,
        false,
        "strata compare releases/1.1.1/dev.json releases/1.1.1/prod.json",
    );
    // the page teaches the flags on the CSI line and its impact, and not the other packs
    r.excerpt(
        "move-compare-csi",
        "move-compare",
        r"^(aws-stack|root |note |~ \[csi)|  +(impact:|upstream digest)",
    );
    r.step("move-fields", 1, false, r#"strata compare releases/1.1.1/dev.json releases/1.1.1/prod.json --json | jq '.packs[] | select(.identity) | {name, a: .a | {packUid, registryUid, upstream}, b: .b | {packUid, registryUid, upstream}}'"#);
    r.step("move-allow", 0, false, "strata same-release --allow registry-move releases/1.1.1/dev.json releases/1.1.1/prod.json");
    r.step(
        "move-rerecord",
        0,
        true,
        "PALETTE_PROJECT=prod strata snapshot aws-stack@1.1.2 -o releases/1.1.2/prod.json",
    );
    r.step("move-allow-edited", 1, false, "strata same-release --allow registry-move releases/1.1.1/dev.json releases/1.1.2/prod.json");

    r.chapter("tag-name", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.profile(DEV, A113, "1.1.3", "aws-stack@1.1.1", TAG_2404);
    r.step(
        "tag-compare",
        1,
        false,
        "strata compare aws-stack@1.1.1 aws-stack@1.1.3",
    );
    // the page teaches the verdict, the note and the OS line; it never reads the other packs
    r.excerpt(
        "tag-compare-os",
        "tag-compare",
        r"^(aws-stack|root |note |~ \[os)|  +impact:",
    );
    r.step(
        "tag-same",
        0,
        false,
        "strata same-release aws-stack@1.1.1 aws-stack@1.1.3",
    );
    r.step("tag-json", 1, false, r#"strata compare aws-stack@1.1.1 aws-stack@1.1.3 --json | jq '{same, sameContent, maxImpact: .summary.maxImpact, renamed: [.packs[] | select(.tagRenamed) | .name]}'"#);
    r.step(
        "tag-record",
        0,
        true,
        "strata snapshot aws-stack@1.1.3 -o releases/1.1.3/dev.json",
    );
    r.serve_here(); // the page describes what follows: the tag rename that it reports as drift
    r.tenant(|t| t.edit(DEV, A113, TAG_LTS2404));
    r.step(
        "tag-verify",
        1,
        false,
        "strata verify releases/1.1.3/dev.json",
    );
    // the page teaches the two notes and the verdict. formats.md keeps the whole report.
    r.excerpt(
        "tag-verify-notes",
        "tag-verify",
        r"^(aws-stack|root |note |~ \[os|DRIFT)|  +impact:",
    );

    r.chapter("manifest-pack", &[]);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.tenant(|t| t.addons(DEV, ADDONS_DEV, ADDONS_PACK, "restricted", "1.0.0"));
    r.tenant(|t| {
        t.addons(
            PROD,
            ADDONS_PROD,
            "64e3000000000000000000d1",
            "restricted",
            "1.0.0",
        )
    });
    // prod also holds 1.0.1, whose manifest sets another Pod Security level. Both versions exist
    // before the playground hands the tenant over, so every step of the page runs against it.
    r.tenant(|t| {
        t.addons(
            PROD,
            ADDONS_PROD2,
            "64e3000000000000000000d2",
            "baseline",
            "1.0.1",
        )
    });
    r.step(
        "manifest-record",
        0,
        true,
        "PALETTE_PROJECT=dev strata snapshot platform-addons@1.0.0 -o addons/1.0.0/dev.json",
    );
    r.step(
        "manifest-record",
        0,
        true,
        "PALETTE_PROJECT=prod strata snapshot platform-addons@1.0.0 -o addons/1.0.0/prod.json",
    );
    r.step(
        "manifest-compare",
        1,
        false,
        "strata compare addons/1.0.0/dev.json addons/1.0.0/prod.json",
    );
    // the page teaches the root line, the note, and the pack line with its impact
    r.excerpt(
        "manifest-compare-note",
        "manifest-compare",
        r"^(platform-addons|root |note |~ )|  +impact:",
    );
    r.step(
        "manifest-same",
        0,
        false,
        "strata same-release addons/1.0.0/dev.json addons/1.0.0/prod.json",
    );
    r.step(
        "manifest-rerecord",
        0,
        true,
        "PALETTE_PROJECT=prod strata snapshot platform-addons@1.0.1 -o addons/1.0.1/prod.json",
    );
    r.step(
        "manifest-different",
        1,
        false,
        "strata same-release addons/1.0.0/dev.json addons/1.0.1/prod.json",
    );
    r.step(
        "manifest-full",
        1,
        false,
        "strata compare addons/1.0.0/dev.json addons/1.0.1/prod.json --full",
    );
    // the page teaches the one line that differs, which only --full prints
    r.excerpt(
        "manifest-full-diff",
        "manifest-full",
        r"^([$]|--- |[+][+][+] |[-+] +pod-security)",
    );
    // a manifest pack in a candidate file: a layer of type manifest, with no registry, whose
    // record differs from the published version by the minted uid alone (#150)
    r.step(
        "manifest-candidate",
        0,
        true,
        "PALETTE_PROJECT=dev strata candidate platform-addons@1.0.0 -o addons/candidate.yaml",
    );
    r.step("manifest-candidate", 0, false, "cat addons/candidate.yaml");
    r.step(
        "manifest-candidate-record",
        0,
        true,
        "PALETTE_PROJECT=dev strata snapshot --from-candidate addons/candidate.yaml -o addons/candidate.json",
    );
    r.step(
        "manifest-candidate-same",
        0,
        false,
        "strata same-release addons/candidate.json addons/1.0.0/dev.json",
    );
    r.step(
        "manifest-candidate-same",
        0,
        false,
        "strata same-release addons/candidate.json addons/1.0.0/dev.json --json | jq -c '.pairs[] | {status, mintedUidOnly}'",
    );

    r.chapter("edited-in-place", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111], &[]));
    r.step(
        "edited-record",
        0,
        true,
        "strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json",
    );
    r.step(
        "edited-record",
        0,
        true,
        "strata snapshot cluster:payments -o runs/payments.json",
    );
    r.step(
        "edited-verify-ok",
        0,
        false,
        "strata verify releases/1.1.1/dev.json",
    );
    r.serve_here(); // state 1: the records are taken, and nobody has edited the version
    r.tenant(|t| t.edit(DEV, A111, RETAIN));
    r.step(
        "edited-verify",
        1,
        false,
        "strata verify releases/1.1.1/dev.json",
    );
    // the verdict alone: it ends with the time of the last change of the document (#140)
    r.excerpt("edited-verify-verdict", "edited-verify", r"^DRIFT");
    // the same drift, and who made it: the page teaches the lines that --who adds
    r.step(
        "edited-verify-who-full",
        1,
        false,
        "strata verify releases/1.1.1/dev.json --who",
    );
    r.excerpt(
        "edited-verify-who",
        "edited-verify-who-full",
        r"^(aws-stack|DRIFT|who:|  \d{4}-)",
    );
    // the edit waits for payments, which runs the version: the listing says so
    r.step(
        "edited-list-behind",
        0,
        false,
        "strata list clusters --applied",
    );
    r.step(
        "edited-verify-who-json",
        1,
        false,
        "strata verify releases/1.1.1/dev.json --who --json | jq '.verify.who'",
    );
    r.step(
        "edited-compare",
        1,
        false,
        "strata compare releases/1.1.1/dev.json aws-stack@1.1.1",
    );
    r.step(
        "edited-cluster-kept",
        0,
        false,
        "strata verify runs/payments.json",
    );
    // what the update that waits would change on payments, before anybody accepts it (#78)
    r.step(
        "edited-pending-record",
        0,
        true,
        "strata snapshot cluster:payments@pending -o review/payments-pending.json",
    );
    r.step(
        "edited-pending",
        1,
        false,
        "strata compare cluster:payments cluster:payments@pending",
    );
    // the update waits: does accepting it repave? A change of the CSI values repaves nothing.
    r.tenant(|t| t.repave_answer(DEV, PAYMENTS, false));
    r.step(
        "edited-repave",
        1,
        false,
        "strata compare cluster:payments cluster:payments@pending --repave",
    );
    r.excerpt(
        "edited-repave-lines",
        "edited-repave",
        r"^(aws-stack|root |note |palette|      impact)",
    );
    r.tenant(|t| t.cluster(DEV, SEARCH, "search", &[A111], &[]));
    r.serve_here(); // state 2: the edit is in Palette, and search started after it
    r.step(
        "edited-two-clusters",
        1,
        false,
        "strata compare cluster:payments cluster:search",
    );
    // Palette answers for one cluster: two clusters ask nothing
    r.step(
        "edited-repave-usage",
        2,
        true,
        "strata compare cluster:payments cluster:search --repave",
    );
    r.step(
        "edited-bulk",
        1,
        false,
        "strata verify 'releases/**/*.json' 'runs/*.json'",
    );
    r.step(
        "edited-bulk-json",
        1,
        false,
        "strata verify 'releases/**/*.json' 'runs/*.json' --json | jq '.summary'",
    );
    r.step(
        "edited-report",
        1,
        true,
        "scripts/ci/verify-report.sh 'releases/**/*.json' 'runs/*.json' --dry-run",
    );
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111], &[]));
    r.step(
        "edited-cluster-updated",
        1,
        false,
        "strata verify runs/payments.json",
    );

    r.chapter("draft", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.registries(DEV));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.step(
        "draft-release",
        0,
        true,
        "strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json",
    );
    r.step(
        "draft-none",
        2,
        true,
        "strata snapshot aws-stack@1.1.1@draft",
    );
    r.serve_here(); // state 1: aws-stack@1.1.1 has no draft, and the release record is there
    // a person edits the version in the console, and does not publish
    r.tenant(|t| t.draft(DEV, A111, RETAIN));
    r.serve_here(); // state 2: a draft of aws-stack@1.1.1 waits
    r.step(
        "draft-compare",
        1,
        false,
        "strata compare aws-stack@1.1.1 aws-stack@1.1.1@draft",
    );
    r.step(
        "draft-version-holds",
        0,
        false,
        "strata verify releases/1.1.1/dev.json",
    );
    r.step(
        "draft-record",
        0,
        true,
        "strata snapshot aws-stack@1.1.1@draft -o review/aws-stack-1.1.1-draft.json",
    );
    r.step(
        "draft-template",
        0,
        false,
        "jq -r '.source | \"\\(.ref)  \\(.template)  uid \\(.uid)\"' review/aws-stack-1.1.1-draft.json",
    );
    r.step(
        "draft-candidate",
        0,
        true,
        "strata candidate aws-stack@1.1.1@draft -o aws-stack.yaml",
    );
    // the reviewer approves, and the person publishes the draft
    r.tenant(|t| t.publish(DEV, A111));
    r.serve_here(); // state 3: the draft is published
    r.step(
        "draft-published",
        0,
        false,
        "strata verify review/aws-stack-1.1.1-draft.json",
    );
    r.step(
        "draft-release-drift",
        1,
        false,
        "strata verify releases/1.1.1/dev.json",
    );
    r.step(
        "draft-gone",
        2,
        true,
        "strata snapshot aws-stack@1.1.1@draft",
    );

    r.chapter("overwritten", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111], &[]));
    r.step(
        "overwritten-records",
        0,
        true,
        "strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json",
    );
    r.step(
        "overwritten-records",
        0,
        true,
        "strata snapshot cluster:payments -o runs/payments.json",
    );
    r.serve_here(); // state 1: the records are taken, and nobody has bumped the version
    r.tenant(|t| {
        t.edit(
            DEV,
            A111,
            &[Edit::Version("1.2.0"), Edit::Reclaim("Retain")],
        )
    });
    r.serve_here(); // state 2: the bump took the uid of 1.1.1, with the flag off
    r.step("overwritten-list", 0, false, "strata list profiles");
    r.step(
        "overwritten-verify",
        1,
        false,
        "strata verify releases/1.1.1/dev.json",
    );
    r.step(
        "overwritten-cluster",
        1,
        false,
        "strata verify runs/payments.json",
    );
    // the bump is a write to the profile, and not to the cluster: --who reads both logs (#140)
    r.step(
        "overwritten-cluster-who-full",
        1,
        false,
        "strata verify runs/payments.json --who",
    );
    r.excerpt(
        "overwritten-cluster-who",
        "overwritten-cluster-who-full",
        r"^(aws-stack|DRIFT|who:|  \d{4}-)",
    );
    r.step(
        "overwritten-consumers",
        1,
        false,
        "strata consumers aws-stack@1.2.0",
    );
    r.step(
        "overwritten-compare",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.2.0",
    );
    r.step("overwritten-candidate", 0, true, "strata candidate releases/1.1.1/dev.json -o aws-stack-1.1.1.yaml --files-dir aws-stack-1.1.1");
    r.tenant(|t| {
        t.edit(
            DEV,
            A111,
            &[Edit::Version("1.1.1"), Edit::Reclaim("Delete")],
        )
    });
    r.profile(DEV, A120, "1.2.0", "aws-stack@1.1.1", RETAIN);
    // state 3, which serve_last reaches at the next chapter: the same bump with the flag on
    r.step("overwritten-flag-list", 0, false, "strata list profiles");
    r.step(
        "overwritten-flag-verify",
        0,
        false,
        "strata verify releases/1.1.1/dev.json runs/payments.json",
    );

    r.chapter("cluster-release", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.addons(DEV, ADDONS_DEV, ADDONS_PACK, "restricted", "1.0.0"));
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A110, ADDONS_DEV], &[]));
    r.tenant(|t| t.cluster(DEV, SEARCH, "search", &[A111], &[]));
    r.tenant(|t| t.cluster(DEV, LEDGER, "ledger", &[A110], &[]));
    r.tenant(|t| t.on_cluster_reclaim(DEV, LEDGER, "Retain"));
    r.step(
        "cluster-release-record",
        0,
        true,
        "strata snapshot aws-stack@1.1.0 -o releases/1.1.0/dev.json",
    );
    r.step(
        "cluster-applied",
        0,
        false,
        "strata list clusters --applied",
    );
    r.step(
        "cluster-identical",
        0,
        false,
        "strata compare releases/1.1.0/dev.json cluster:payments/aws-stack",
    );
    r.step(
        "cluster-all-profiles",
        1,
        false,
        "strata compare releases/1.1.0/dev.json cluster:payments",
    );
    r.step(
        "cluster-moved",
        1,
        false,
        "strata compare releases/1.1.0/dev.json cluster:search",
    );
    r.step(
        "cluster-edited",
        1,
        false,
        "strata compare releases/1.1.0/dev.json cluster:ledger",
    );
    // the page tells the reader to run this next, so the page must show it
    r.step(
        "cluster-edited-verify",
        0,
        false,
        "strata verify releases/1.1.0/dev.json",
    );
    r.step(
        "cluster-run-record",
        0,
        true,
        "strata snapshot cluster:payments/aws-stack -o runs/payments.json",
    );
    r.step(
        "cluster-run-record",
        0,
        false,
        "strata verify runs/payments.json",
    );
    // the page teaches that verify fetches the cluster again with the same selection: the record,
    // the two roots and the verdict. The packs of the profile are not its subject.
    r.excerpt(
        "cluster-run-verify",
        "cluster-run-record",
        r"^([$] |strata: wrote|root |OK:)",
    );

    r.chapter("cluster-values", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| {
        t.cluster(
            DEV,
            PAYMENTS,
            "payments",
            &[A111],
            &[("platform_owner", "team-payments")],
        )
    });
    r.tenant(|t| {
        t.cluster(
            DEV,
            SEARCH,
            "search",
            &[A111],
            &[("platform_owner", "team-search")],
        )
    });
    r.step(
        "values-compare",
        0,
        false,
        "strata compare cluster:payments cluster:search",
    );
    r.step(
        "cluster-values-record",
        0,
        true,
        "strata snapshot cluster:payments -o runs/payments.json",
    );
    r.step(
        "values-record",
        0,
        false,
        "jq .source.cluster.variables runs/payments.json",
    );
    r.serve_here(); // state 1: each cluster keeps the value it started with
    r.tenant(|t| t.set_value(DEV, PAYMENTS, "platform_owner", "team-platform"));
    // state 2, which serve_last reaches at the next chapter: a person changed the value
    r.step(
        "values-verify",
        0,
        false,
        "strata verify runs/payments.json",
    );
    // the page tells a job to read records[].clusterVariables, so the page must show that shape
    r.step(
        "values-json",
        0,
        true,
        "strata snapshot cluster:search -o runs/search.json",
    );
    r.step(
        "values-json",
        0,
        false,
        r#"strata verify 'runs/*.json' --format json | jq '.records[] | {path, clusterVariables}'"#,
    );

    // a project macro that the values of a pack read: the root covers its name, and a record
    // with --macros holds the digest of its value. The add-on profile that the cluster group pins
    // reads a second macro in its manifest.
    r.chapter("macros", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A114, "1.1.4", "aws-stack@1.1.1", MACRO_REGION);
    r.tenant(|t| {
        t.addons(
            DEV,
            ADDONS_DEV,
            ADDONS_PACK,
            "\"{{.spectro.macro.podSecurity}}\"",
            "1.0.0",
        )
    });
    r.tenant(|t| t.set_macro(DEV, "region", "us-east-2"));
    r.tenant(|t| t.set_macro(DEV, "podSecurity", "baseline"));
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A114], &[]));
    r.tenant(|t| t.cluster_group(DEV, DEV_GROUP, "dev-group", &[ADDONS_DEV], &[]));
    r.step(
        "macros-record",
        0,
        true,
        "strata snapshot aws-stack@1.1.4 -o releases/1.1.4/dev.json",
    );
    r.step(
        "macros-record",
        0,
        true,
        "strata snapshot cluster:payments --macros -o runs/payments.json",
    );
    r.step(
        "macros-digests",
        0,
        false,
        "jq .source.cluster.macroValues runs/payments.json",
    );
    r.step(
        "macros-record-version",
        0,
        true,
        "strata snapshot aws-stack@1.1.4 --macros -o releases/1.1.4/dev-macros.json",
    );
    r.step(
        "macros-digests-version",
        0,
        false,
        "jq .source.macroValues releases/1.1.4/dev-macros.json",
    );
    r.step(
        "macros-record-group",
        0,
        true,
        "strata snapshot clustergroup:dev-group --macros -o runs/dev-group.json",
    );
    r.step(
        "macros-digests-group",
        0,
        false,
        "jq .source.cluster.macroValues runs/dev-group.json",
    );
    r.serve_here(); // state 1: the records are taken, and each macro has its first value
    r.tenant(|t| t.set_macro(DEV, "region", "eu-west-1"));
    r.tenant(|t| t.set_macro(DEV, "podSecurity", "restricted"));
    r.step(
        "macros-verify-release-full",
        0,
        false,
        "strata verify releases/1.1.4/dev.json",
    );
    r.excerpt(
        "macros-verify-release",
        "macros-verify-release-full",
        r"^(aws-stack|root |note |OK)",
    );
    r.step(
        "macros-verify-cluster-full",
        1,
        false,
        "strata verify runs/payments.json",
    );
    r.excerpt(
        "macros-verify-cluster",
        "macros-verify-cluster-full",
        r"^(aws-stack|root |note |DRIFT)",
    );
    r.step(
        "macros-verify-version-full",
        1,
        false,
        "strata verify releases/1.1.4/dev-macros.json",
    );
    r.excerpt(
        "macros-verify-version",
        "macros-verify-version-full",
        r"^(aws-stack|root |note |DRIFT)",
    );
    r.step(
        "macros-verify-group-full",
        1,
        false,
        "strata verify runs/dev-group.json",
    );
    r.excerpt(
        "macros-verify-group",
        "macros-verify-group-full",
        r"^(platform-addons|root |note |DRIFT)",
    );

    // an OS pack that names its machine image in the annotation imageId: the root does not
    // cover the annotations, and compare, verify, and same-release name a new image (#189)
    r.chapter("os-image", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A118, "1.1.8", "aws-stack@1.1.1", IMAGE_1);
    r.step(
        "os-image-record",
        0,
        true,
        "strata snapshot aws-stack@1.1.8 -o releases/1.1.8/dev.json",
    );
    r.step(
        "os-image-annotations",
        0,
        false,
        r#"jq '.packs[] | select(.layer == "os") | .annotations' releases/1.1.8/dev.json"#,
    );
    r.serve_here(); // state 1: the record is taken, and the OS pack names its first image
    r.tenant(|t| t.edit(DEV, A118, IMAGE_2));
    r.step(
        "os-image-verify-full",
        1,
        false,
        "strata verify releases/1.1.8/dev.json",
    );
    r.excerpt(
        "os-image-verify",
        "os-image-verify-full",
        r"^(aws-stack|root |note |DRIFT)",
    );
    r.step(
        "os-image-compare-json",
        1,
        false,
        "strata compare releases/1.1.8/dev.json aws-stack@1.1.8 --json | jq -c '{same, imageIdsChanged}'",
    );
    // same-release reads the imageId of each pack too: one root, two images (#229)
    r.step(
        "os-image-same-release",
        1,
        false,
        "strata same-release releases/1.1.8/dev.json aws-stack@1.1.8",
    );

    // a registry deletes the tag of a pack that a published version uses. Palette marks the
    // pack ref invalid, the root stays, and snapshot, verify, and compare say so (#108).
    r.chapter("invalid-pack", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A119, "1.1.9", "aws-stack@1.1.1", &[]);
    r.step(
        "invalid-pack-record",
        0,
        true,
        "strata snapshot aws-stack@1.1.9 -o releases/1.1.9/dev.json",
    );
    r.serve_here(); // state 1: the record is taken, and each pack is valid
    r.tenant(|t| t.edit(DEV, A119, CERT_MANAGER_TAG_DELETED));
    r.step(
        "invalid-pack-verify-full",
        0,
        false,
        "strata verify releases/1.1.9/dev.json",
    );
    r.excerpt(
        "invalid-pack-verify",
        "invalid-pack-verify-full",
        r"^(aws-stack|root |  \[addon|OK|INVALID)",
    );
    r.step(
        "invalid-pack-snapshot",
        0,
        true,
        "strata snapshot aws-stack@1.1.9 -o releases/1.1.9/dev-now.json",
    );
    r.step(
        "invalid-pack-fields",
        0,
        false,
        r#"jq -c '.packs[] | select(.invalid) | {key, invalid, invalidReason}' releases/1.1.9/dev-now.json"#,
    );
    r.step(
        "invalid-pack-compare-json",
        0,
        false,
        "strata compare releases/1.1.9/dev.json releases/1.1.9/dev-now.json --json | jq -c '{same, packsInvalid: .summary.packsInvalid}'",
    );

    // a pack with no upstream digest, as a registry that Palette does not sync gives it
    r.chapter("no-digest", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.profile(
        DEV,
        A115,
        "1.1.5",
        "aws-stack@1.1.1",
        &[Edit::NoDigest("csi-aws-ebs")],
    );
    r.step(
        "nodigest-record",
        0,
        true,
        "strata snapshot aws-stack@1.1.5 -o releases/1.1.5/dev.json",
    );
    r.step(
        "nodigest-source",
        0,
        false,
        r#"jq -r '.packs[] | "\(.name) \(.hash.upstreamSource)"' releases/1.1.5/dev.json"#,
    );
    r.step(
        "nodigest-verify-full",
        1,
        false,
        "strata verify releases/1.1.5/dev.json",
    );
    r.excerpt(
        "nodigest-verify",
        "nodigest-verify-full",
        r"^(aws-stack|root |note |  \[csi|      upstream unknown|UNVERIFIED)",
    );
    r.step(
        "nodigest-allow-full",
        0,
        false,
        "strata verify releases/1.1.5/dev.json --allow-no-digest",
    );
    r.excerpt("nodigest-allow", "nodigest-allow-full", r"^(note |OK)");
    r.step(
        "nodigest-same-release",
        1,
        false,
        "strata same-release aws-stack@1.1.1 aws-stack@1.1.5",
    );
    // a candidate layer from a private Helm registry: Palette lists no chart of it, and makes
    // the pack when it creates the profile (#154)
    r.tenant(|t| t.registries(DEV));
    r.tenant(|t| t.private_charts(DEV));
    r.write_file(
        "charts-probe.yaml",
        "name: charts-probe\nversion: 1.0.0\ntype: add-on\ncloudType: all\nlayers:\n  - name: podinfo\n    version: 6.7.1\n    registry: charts-auth\n    values: |\n      replicaCount: 2\n",
    );
    r.step("nodigest-chart-file", 0, false, "cat charts-probe.yaml");
    // no registry has the chart in the listing: the error names the registry that Palette does
    // not sync (#123)
    r.step("nodigest-chart-unsynced", 2, true, "strata packs podinfo");
    r.step(
        "nodigest-chart-record",
        0,
        true,
        "strata snapshot --from-candidate charts-probe.yaml -o charts/candidate.json",
    );
    r.step(
        "nodigest-chart-behind",
        2,
        false,
        "strata behind charts/candidate.json",
    );
    r.step(
        "nodigest-chart-pack",
        0,
        false,
        "jq -c '.packs[] | {name, type, layer, version, packUid, upstreamSource: .hash.upstreamSource}' charts/candidate.json",
    );
    r.tenant(|t| t.chart_profile(DEV, CHARTS_PROBE, CHART_PACK, "6.7.1"));
    r.step(
        "nodigest-chart-published",
        0,
        true,
        "strata snapshot charts-probe@1.0.0 -o charts/published.json",
    );
    r.step(
        "nodigest-chart-pack",
        0,
        false,
        "jq -c '.packs[] | {name, type, layer, version, packUid, upstreamSource: .hash.upstreamSource}' charts/published.json",
    );
    r.step(
        "nodigest-chart-packs",
        0,
        false,
        "strata packs podinfo --registry charts-auth",
    );
    r.step(
        "nodigest-chart-same",
        1,
        false,
        "strata same-release charts/candidate.json charts/published.json",
    );
    r.step(
        "nodigest-chart-allow",
        0,
        false,
        "strata same-release charts/candidate.json charts/published.json --allow no-digest",
    );
    r.step(
        "nodigest-chart-allow",
        0,
        false,
        "strata same-release charts/candidate.json charts/published.json --allow no-digest --json | jq -c '.pairs[] | {status, mintedUidOnly}'",
    );

    // a pack value whose schema format is password: Palette serves the mask in place of the
    // secret, so an equal root does not show an equal secret (#163)
    r.chapter("masked-value", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.profile(DEV, A1110, "1.1.10", "aws-stack@1.1.1", SECRET_VALUE);
    r.profile(PROD, A1110_PROD, "1.1.10", "aws-stack@1.1.1", SECRET_VALUE);
    r.step(
        "masked-record",
        0,
        true,
        "strata snapshot aws-stack@1.1.10 -o releases/1.1.10/dev.json",
    );
    r.step(
        "masked-paths",
        0,
        false,
        r#"jq -c '.packs[] | select(.masked) | {key, masked}' releases/1.1.10/dev.json"#,
    );
    r.quiet("PALETTE_PROJECT=prod strata snapshot aws-stack@1.1.10 -o releases/1.1.10/prod.json");
    r.serve_here(); // state 1: the two records are taken, and each one holds the mask
    r.step(
        "masked-same-release",
        0,
        false,
        "strata same-release releases/1.1.10/dev.json releases/1.1.10/prod.json",
    );
    r.step(
        "masked-compare-full",
        0,
        false,
        "strata compare releases/1.1.10/dev.json releases/1.1.10/prod.json",
    );
    r.excerpt(
        "masked-compare",
        "masked-compare-full",
        r"^(aws-stack|root |note )",
    );
    r.step(
        "masked-verify-full",
        0,
        false,
        "strata verify releases/1.1.10/dev.json",
    );
    r.excerpt(
        "masked-verify",
        "masked-verify-full",
        r"^(aws-stack|root |note |  \[addon\] cert|      masked|OK)",
    );
    r.step(
        "masked-same-release-json",
        0,
        false,
        "strata same-release releases/1.1.10/dev.json releases/1.1.10/prod.json --json | jq -c '.pairs[] | {status, masked}'",
    );

    // Palette's own answer beside the impact table: a change that repaves, and one that does not
    r.chapter("repave", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.profile(
        DEV,
        A116,
        "1.1.6",
        "aws-stack@1.1.1",
        &[Edit::KubeadmCommand("echo strata-repave-test")],
    );
    r.profile(DEV, A112, "1.1.2", "aws-stack@1.1.1", RETAIN);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111], &[]));
    r.tenant(|t| t.repave_answer(DEV, PAYMENTS, true));
    r.step(
        "repave-kubeadm",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.6 --repave",
    );
    r.excerpt(
        "repave-kubeadm-lines",
        "repave-kubeadm",
        r"^(aws-stack|root |note |palette|[ ~] \[k8s|      impact)",
    );
    r.step(
        "repave-kubeadm-json",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.6 --repave --json | jq .paletteRepave",
    );
    r.tenant(|t| t.repave_answer(DEV, PAYMENTS, false));
    r.step(
        "repave-csi",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.2 --repave",
    );
    r.excerpt(
        "repave-csi-lines",
        "repave-csi",
        r"^(aws-stack|root |note |palette|[ ~] \[csi|      impact)",
    );
    r.profile(
        DEV,
        A117,
        "1.1.7",
        "aws-stack@1.1.1",
        &[Edit::Upgrade {
            pack: "kubernetes",
            uid: "64ea0000000000000000a117",
            version: "1.36.3",
        }],
    );
    r.step(
        "repave-disagree",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.7 --repave",
    );
    r.excerpt(
        "repave-disagree-lines",
        "repave-disagree",
        r"^(aws-stack|root |note |palette|[ ~] \[k8s|      impact)",
    );
    r.step(
        "repave-usage",
        2,
        true,
        "strata compare aws-stack@1.1.1 aws-stack@1.1.6 --repave",
    );

    // Palette's own check of the packs of the change on the cluster: a refusal, and an accept
    r.tenant(|t| {
        t.packs_answer(
            DEV,
            PAYMENTS,
            Some(PackRefusal {
                profile: A117,
                pack: "kubernetes",
                required: "cni-cilium-oss",
                version: "1.21.0",
            }),
        )
    });
    r.step(
        "validate-refuses",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.7 --validate",
    );
    r.excerpt(
        "validate-refuses-lines",
        "validate-refuses",
        r"^(aws-stack|root |palette|[ ~] \[k8s|  \[cni)",
    );
    r.step(
        "validate-refuses-json",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.7 --validate --json | jq .paletteValidation",
    );
    r.tenant(|t| t.packs_answer(DEV, PAYMENTS, None));
    r.step(
        "validate-accepts",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.2 --validate",
    );
    r.excerpt(
        "validate-accepts-lines",
        "validate-accepts",
        r"^(aws-stack|root |palette|[ ~] \[csi)",
    );
    r.step(
        "validate-usage",
        2,
        true,
        "strata compare aws-stack@1.1.1 aws-stack@1.1.7 --validate",
    );

    // a repave that waits for approval: the cluster record holds it outside the root, and verify
    // names it (#107)
    r.step(
        "repave-waits-record",
        0,
        true,
        "strata snapshot cluster:payments -o runs/payments.json",
    );
    r.tenant(|t| t.repave_waits(DEV, PAYMENTS, "2026-09-10T10:15:00Z"));
    r.step(
        "repave-waits-verify",
        0,
        false,
        "strata verify runs/payments.json",
    );
    r.excerpt(
        "repave-waits-verify-lines",
        "repave-waits-verify",
        r"^(aws-stack|root |note |OK)",
    );
    r.step(
        "repave-waits-json",
        0,
        true,
        "strata snapshot cluster:payments -o runs/payments-now.json",
    );
    r.step(
        "repave-waits-json",
        0,
        false,
        "jq '.source.cluster | {repavePending, pendingRepave}' runs/payments-now.json",
    );

    // a cluster that a cluster template governs: on the route of the cluster, Palette compares
    // nothing and answers no repave, so --repave asks the route of the template (#130)
    r.tenant(|t| t.cluster_in_template(DEV, PAYMENTS, AWS_STANDARD));
    r.tenant(|t| t.template_repave_answer(DEV, AWS_STANDARD, true));
    r.step(
        "repave-template",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.6 --repave",
    );
    r.excerpt(
        "repave-template-lines",
        "repave-template",
        r"^(aws-stack|root |palette|[ ~] \[k8s|      impact)",
    );
    r.step(
        "repave-template-json",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.6 --repave --json | jq .paletteRepave",
    );
    r.step(
        "repave-template-usage",
        2,
        true,
        "strata compare cluster:payments aws-stack@1.1.1 --repave",
    );

    // More of a cluster than its release: the tiers that Palette reports of the cluster
    r.chapter("cluster-tiers", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111], &[]));
    r.tenant(|t| t.cluster(DEV, SEARCH, "search", &[A111], &[]));
    r.serve_here(); // state 1: two clusters on one release, nothing changed
    r.step(
        "tiers-record-payments",
        0,
        false,
        "strata snapshot cluster:payments --shape -o runs/payments.json",
    );
    r.step(
        "tiers-record-search",
        0,
        false,
        "strata snapshot cluster:search --shape -o runs/search.json",
    );
    r.step(
        "tiers-show",
        0,
        false,
        "jq '.source.cluster | {labels, tiers: (.tiers | map_values(.hash[0:12]))}' runs/payments.json",
    );
    r.step(
        "tiers-policy",
        0,
        false,
        "jq '.source.cluster.tiers.policy.fields' runs/payments.json",
    );
    r.step(
        "tiers-same",
        0,
        false,
        "strata compare runs/payments.json runs/search.json",
    );
    r.excerpt(
        "tiers-same-lines",
        "tiers-same",
        r"^(runs/|root |  variables|  cluster tiers)",
    );
    // search takes a larger worker pool with a taint, and another timezone and label
    r.tenant(|t| {
        t.cluster_field(
            DEV,
            SEARCH,
            "/cloudconfig/spec/machinePoolConfig/1/size",
            json!(3),
        )
    });
    r.tenant(|t| {
        t.cluster_field(
            DEV,
            SEARCH,
            "/cloudconfig/spec/machinePoolConfig/1/taints",
            json!([{"key": "dedicated", "value": "search", "effect": "NoSchedule"}]),
        )
    });
    r.tenant(|t| {
        t.cluster_field(
            DEV,
            SEARCH,
            "/spec/clusterConfig/timezone",
            json!("America/Chicago"),
        )
    });
    r.tenant(|t| {
        t.cluster_field(
            DEV,
            SEARCH,
            "/metadata/labels",
            json!({"managed-by": "opentofu", "team": "search"}),
        )
    });
    // a person adds a user to the role binding of search: the binding keeps its uid
    r.tenant(|t| {
        t.cluster_field(
            DEV,
            SEARCH,
            "/rbacs/items/0/spec/bindings/0/subjects",
            json!([
                {"type": "Group", "name": "platform", "namespace": ""},
                {"type": "User", "name": "fred@example.com", "namespace": ""}
            ]),
        )
    });
    // payments reconciled an older Kubernetes version than its profile declares
    r.tenant(|t| t.cluster_field(DEV, PAYMENTS, "/status/packs/1/version", json!("1.36.1")));
    r.serve_here(); // state 2: search has another shape and policy, and payments is behind
    r.step(
        "tiers-verify-search",
        0,
        false,
        "strata verify runs/search.json",
    );
    r.excerpt(
        "tiers-verify-search-lines",
        "tiers-verify-search",
        r"^(root |  cluster tiers|        ~ policy|        \+ policy|OK)",
    );
    r.step(
        "tiers-record-search-again",
        0,
        false,
        "strata snapshot cluster:search --shape -o runs/search.json",
    );
    r.step(
        "tiers-differ",
        0,
        false,
        "strata compare runs/payments.json runs/search.json",
    );
    r.excerpt(
        "tiers-differ-lines",
        "tiers-differ",
        r"^(runs/|root |  variables|  cluster tiers|        [~+-] )",
    );
    r.step(
        "tiers-differ-json",
        0,
        false,
        "strata compare runs/payments.json runs/search.json --json | jq -c '.clusterTiers[] | {tier, same}'",
    );
    r.step(
        "tiers-behind",
        0,
        false,
        "strata compare cluster:payments aws-stack@1.1.1",
    );
    r.excerpt(
        "tiers-behind-lines",
        "tiers-behind",
        r"^(aws-stack|root |note )",
    );
    r.step("tiers-verify", 0, false, "strata verify runs/payments.json");
    r.step(
        "tiers-behind-json",
        0,
        false,
        "strata compare cluster:payments aws-stack@1.1.1 --json | jq -c '.runningBehind'",
    );
    // the key loses the permission to read the role bindings of payments: the record holds the
    // error in the policy tier and no value for the field, and verify does not compare the field
    // (#251, #274)
    r.tenant(|t| t.forbid(DEV, &rbacs_route(PAYMENTS)));
    r.serve_here(); // state 3: Palette refuses the role bindings of payments
    r.step(
        "tiers-forbidden-record",
        0,
        true,
        "strata snapshot cluster:payments -o runs/payments-now.json",
    );
    r.step(
        "tiers-forbidden-errors",
        0,
        false,
        "jq '.source.cluster.tiers.policy | {rbacsInFields: (.fields | has(\"rbacs\")), errors}' \
         runs/payments-now.json",
    );
    r.step(
        "tiers-forbidden-verify",
        0,
        false,
        "strata verify runs/payments.json",
    );

    // The shape tier reads the fields of the cloud type of the cluster. lab runs on vSphere: its
    // cloud config has the model of the API reference of Palette.
    r.chapter("cluster-shape", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.cluster(DEV, LAB, "lab", &[A111], &[]));
    r.tenant(|t| t.cloud(DEV, LAB, "vsphere", vsphere_spec()));
    r.serve_here();
    r.step(
        "shape-vsphere-record",
        0,
        false,
        "strata snapshot cluster:lab --shape -o runs/lab.json",
    );
    r.step(
        "shape-vsphere",
        0,
        false,
        "jq '.source.cluster.tiers.shape.fields | {cloudType, cloud, pool: .pools[0]}' runs/lab.json",
    );

    // Edge hosts: an add-on profile bound to two edge hosts and applied to one cluster
    r.chapter("edge-hosts", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.addons(DEV, ADDONS_DEV, ADDONS_PACK, "baseline", "1.0.0"));
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111, ADDONS_DEV], &[]));
    r.tenant(|t| t.edge_host(DEV, STORE_1, "store-1", &[ADDONS_DEV]));
    r.tenant(|t| t.edge_host(DEV, STORE_2, "store-2", &[ADDONS_DEV]));
    r.serve_here(); // state 1: one cluster and two edge hosts on platform-addons@1.0.0
    r.step(
        "edge-consumers",
        1,
        false,
        "strata consumers platform-addons@1.0.0",
    );
    r.step(
        "edge-record",
        0,
        true,
        "strata snapshot edgehost:store-1 -o runs/store-1.json",
    );
    r.step(
        "edge-record-cluster",
        0,
        true,
        "strata snapshot cluster:payments/platform-addons -o runs/payments.json",
    );
    r.step(
        "edge-compare",
        0,
        false,
        "strata compare platform-addons@1.0.0 runs/store-1.json",
    );
    // an edit in place of the manifest: the edge host and the cluster each keep their own copy
    r.tenant(|t| t.addons(DEV, ADDONS_DEV, ADDONS_PACK, "restricted", "1.0.0"));
    r.serve_here(); // state 2: the manifest of platform-addons@1.0.0 is edited in place
    r.step("edge-verify", 0, false, "strata verify 'runs/*.json'");
    r.step(
        "edge-verify-profile",
        1,
        false,
        "strata compare runs/store-1.json platform-addons@1.0.0",
    );

    // A cluster group pins an add-on profile for its virtual clusters
    r.chapter("cluster-groups", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.addons(DEV, ADDONS_DEV, ADDONS_PACK, "baseline", "1.0.0"));
    r.tenant(|t| {
        t.addons_named(
            DEV,
            TEAM_A_APPS,
            "team-a-apps",
            "64e2000000000000000000d3",
            "restricted",
            "1.0.0",
        )
    });
    r.tenant(|t| t.cluster(DEV, TENANT_A, "tenant-a", &[ADDONS_DEV, TEAM_A_APPS], &[]));
    r.tenant(|t| t.cluster(DEV, TENANT_B, "tenant-b", &[ADDONS_DEV], &[]));
    r.tenant(|t| {
        t.cluster_group(
            DEV,
            DEV_GROUP,
            "dev-group",
            &[ADDONS_DEV],
            &[TENANT_A, TENANT_B],
        )
    });
    r.serve_here(); // state 1: the group pins platform-addons@1.0.0, and two members run it
    r.step(
        "group-record",
        0,
        true,
        "strata snapshot clustergroup:dev-group -o runs/dev-group.json",
    );
    r.step(
        "group-show",
        0,
        false,
        "jq -c '.source.cluster | {kind, profiles: [.profiles[].name], members: [.members[].name]}' runs/dev-group.json",
    );
    r.step(
        "group-compare",
        0,
        false,
        "strata compare runs/dev-group.json cluster:tenant-a",
    );
    // tenant-b is a virtual cluster, and the consumer list of the profile does not name it
    r.tenant(|t| t.cluster_field(DEV, TENANT_B, "/spec/cloudType", json!("nested")));
    r.tenant(|t| t.drop_consumer(DEV, ADDONS_DEV, TENANT_B));
    r.step(
        "group-consumers",
        1,
        false,
        "strata consumers platform-addons@1.0.0",
    );
    // the cluster listing leaves out the virtual cluster, so list asks the search for it (#116)
    r.step("group-list", 0, false, "strata list clusters");
    r.step(
        "group-list-json",
        0,
        false,
        r#"strata list clusters --json | jq -c '.includesVirtual, (.clusters[] | select(.virtual) | {name, virtual})'"#,
    );
    r.tenant(|t| {
        t.cluster_field(
            DEV,
            TENANT_B,
            "/metadata/labels",
            json!({"managed-by": "opentofu", "team": "b"}),
        )
    });
    r.step(
        "group-list-tag",
        0,
        false,
        "strata list clusters --tag team=b",
    );
    r.step(
        "group-member-by-name",
        0,
        false,
        "strata compare clustergroup:dev-group cluster:tenant-b",
    );
    r.step(
        "group-policy",
        0,
        false,
        "jq -c '.source.cluster.tiers.policy.fields | {endpointType, kubernetesDistroType, limitConfig, hostClusters: [.hostClusters[].uid]}' runs/dev-group.json",
    );
    r.step(
        "group-policy-values",
        0,
        false,
        "jq -c '.source.cluster.tiers.policy.fields.values' runs/dev-group.json",
    );
    // the group gives each virtual cluster 4 cores in place of 6
    r.tenant(|t| {
        t.cluster_group_field(
            DEV,
            DEV_GROUP,
            "/spec/clustersConfig/limitConfig",
            json!({"cpu": 4, "cpuMilliCore": 4000, "memory": 12288, "memoryMiB": 12288,
            "overSubscription": 120, "storageGiB": 10}),
        )
    });
    r.serve_here(); // state 2: the group gives each virtual cluster less CPU
    r.step(
        "group-policy-verify",
        0,
        false,
        "strata verify runs/dev-group.json",
    );
    r.step(
        "group-policy-compare",
        0,
        false,
        "strata compare runs/dev-group.json clustergroup:dev-group",
    );

    // An app profile, and a plain update of it
    r.chapter("app-profiles", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.app_profile(DEV, SHOP, "1.0.0", "2", 0));
    r.serve_here(); // state 1: shop@1.0.0
    r.step(
        "app-record",
        0,
        true,
        "strata snapshot appprofile:shop@1.0.0 -o runs/shop.json",
    );
    r.step(
        "app-show",
        0,
        false,
        "jq -c '.packs[] | {key, type, tag, params, upstream: .hash.upstreamSource}' runs/shop.json",
    );
    // a tier has no digest, so verify cannot vouch for the chart of the helm tier
    r.step(
        "app-verify-unverified",
        1,
        false,
        "strata verify runs/shop.json",
    );
    r.step(
        "app-verify-ok",
        0,
        false,
        "strata verify runs/shop.json --allow-no-digest",
    );
    // no app deployment uses the app profile yet
    r.step(
        "app-consumers-free",
        0,
        false,
        "strata consumers appprofile:shop@1.0.0",
    );
    // a plain update: the replicas of the web tier, with the same uid and version
    r.tenant(|t| t.app_profile(DEV, SHOP, "1.0.0", "3", 0));
    r.serve_here(); // state 2: the web tier of shop@1.0.0 has another replica count
    r.step(
        "app-verify",
        1,
        false,
        "strata verify runs/shop.json --allow-no-digest",
    );
    // a plain update of the install order: web installs after config
    r.tenant(|t| t.app_profile(DEV, SHOP, "1.0.0", "3", 1));
    r.serve_here(); // state 3: the web tier of shop@1.0.0 has the install order 1
    r.step(
        "app-order-compare",
        1,
        false,
        "strata compare runs/shop.json appprofile:shop@1.0.0",
    );
    // a team deploys the app profile, so Palette refuses its delete
    r.tenant(|t| t.app_deployment(DEV, SHOP_DEV, "shop-dev", SHOP));
    r.serve_here(); // state 4: the app deployment shop-dev uses shop@1.0.0
    r.step(
        "app-consumers",
        1,
        false,
        "strata consumers appprofile:shop@1.0.0",
    );
    r.step(
        "app-consumers-json",
        1,
        false,
        "strata consumers runs/shop.json --json | jq -c '.consumers[]'",
    );

    // The pins that are behind the registry, and a version that arrives between two runs
    r.chapter("behind", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.registries(DEV));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.hide_pack_version(DEV, "csi-aws-ebs", "1.65.0"));
    r.serve_here(); // state 1: the registry has not published csi-aws-ebs 1.65.0 yet
    r.quiet("strata snapshot aws-stack@1.0.0 -o releases/1.0.0/dev.json");
    r.step(
        "behind-last-week",
        0,
        false,
        "strata behind releases/1.0.0/dev.json --no-impact --json > behind.json; jq -c '.pins[] | select(.name == \"csi-aws-ebs\") | {version, newer: [.newer[].version]}' behind.json",
    );
    r.tenant(|t| t.registries(DEV));
    // the newest kubernetes supports amd64 alone, as loki 3.6.7 does in the Public Repo
    r.tenant(|t| t.pack_architectures(DEV, "kubernetes", "1.36.2", "amd64"));
    // Palette accepts each move of the two versions: no pack declares a limit
    r.tenant(|t| {
        t.profile_upgrade_answer(DEV, A100, None);
        t.profile_upgrade_answer(DEV, A111, None);
    });
    r.serve_here(); // state 2: the registry publishes csi-aws-ebs 1.65.0
    r.quiet("strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json");
    r.step("behind", 1, false, "strata behind 'releases/*/dev.json'");
    // a pin that is current against the copy of a registry whose sync stopped before its end
    r.quiet("strata snapshot 'pack:Palette Registry/csi-aws-ebs@1.65.0' -o csi-oci.json");
    r.step("behind-stale", 0, false, "strata behind csi-oci.json");
    r.step(
        "behind-since",
        1,
        false,
        "strata behind 'releases/*/dev.json' --since behind.json --no-impact",
    );
    r.excerpt(
        "behind-since-lines",
        "behind-since",
        r"^(\$|\d+ pins|~ \[csi|      arrived)",
    );
    r.step(
        "behind-json",
        1,
        false,
        "strata behind releases/1.0.0/dev.json --json | jq -c '.pins[] | select(.behind) | {name, version, newest: .newer[0].version, bump: .bump.impact}'",
    );
    r.step(
        "behind-report",
        1,
        false,
        "scripts/ci/upstream-report.sh 'releases/*/dev.json' --dry-run --no-impact",
    );

    // A move that Palette refuses on a cluster that runs the profile. The only pin that is behind
    // is csi-aws-ebs 1.63.1, two minor versions below 1.65.0. The stand-in refuses each request of
    // the route, so the profile has no other pin that is behind
    r.profile(
        DEV,
        A1111,
        "1.1.11",
        "aws-stack@1.1.1",
        &[Edit::Upgrade {
            pack: "csi-aws-ebs",
            uid: "6a75f441d410c3b5762fed2d",
            version: "1.63.1",
        }],
    );
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A1111], &[]));
    r.tenant(|t| t.profile_upgrade_answer(DEV, A1111, Some(("csi-aws-ebs", &["payments"]))));
    r.serve_here(); // state 3: payments runs aws-stack@1.1.11, and Palette refuses the move of its CSI pack
    r.quiet("strata snapshot aws-stack@1.1.11 -o refused/dev.json");
    r.step("behind-refused", 1, false, "strata behind refused/dev.json");
    r.step(
        "behind-refused-json",
        1,
        false,
        "strata behind refused/dev.json --json | jq -c '.pins[] | select(.behind) | .bump.paletteValidation'",
    );
    r.step(
        "behind-refused-no-impact",
        1,
        false,
        "strata behind refused/dev.json --no-impact",
    );

    // The fleet from Palette: the clusters of a label, in two projects, and a record of each
    r.chapter("fleet", &[]);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.profile(PROD, A111_PROD, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111], &[]));
    r.tenant(|t| t.cluster(DEV, SEARCH, "search", &[A111], &[]));
    r.tenant(|t| t.cluster(PROD, PAYMENTS_PROD, "payments", &[A111_PROD], &[]));
    r.tenant(|t| t.cluster(PROD, LEDGER, "ledger", &[A111_PROD], &[]));
    for (project, uid, env) in [
        (DEV, PAYMENTS, "dev"),
        (DEV, SEARCH, "dev"),
        (PROD, PAYMENTS_PROD, "prod"),
        (PROD, LEDGER, "prod"),
    ] {
        r.tenant(|t| {
            t.cluster_field(
                project,
                uid,
                "/metadata/labels",
                json!({"env": env, "managed-by": "opentofu"}),
            )
        });
    }
    r.tenant(|t| t.tag_filter("production", PRODUCTION, "env", "prod"));
    r.serve_here(); // state 1: two clusters of each project, and the production ones labeled so
    r.step("fleet-tag", 0, false, "strata list clusters --tag env=prod");
    r.step(
        "fleet-filters",
        0,
        false,
        "strata list filters --project ''",
    );
    r.step(
        "fleet-snapshot",
        0,
        true,
        "strata snapshot filter:production --dir fleet/prod --prune",
    );
    r.step(
        "fleet-set-json",
        0,
        false,
        "strata snapshot filter:production --dir fleet/prod --json --quiet | jq -c '.schema, .summary, (.records[] | {name: .cluster.name, path, root, error})'",
    );
    r.step(
        "fleet-json",
        0,
        false,
        "strata list clusters --filter production --json | jq -c '.resolved, (.clusters[] | {name, projectName, labels})'",
    );
    // ledger leaves production: the automation that labels it drops the label
    r.tenant(|t| {
        t.cluster_field(
            PROD,
            LEDGER,
            "/metadata/labels",
            json!({"env": "retired", "managed-by": "opentofu"}),
        )
    });
    r.serve_here(); // state 2: ledger is no longer labeled env=prod
    r.step(
        "fleet-snapshot-again",
        0,
        true,
        "strata snapshot filter:production --dir fleet/prod --prune",
    );
    r.step(
        "fleet-verify",
        0,
        false,
        "strata verify 'fleet/prod/*.json' --summary",
    );

    r.chapter("same-names", &[]);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.profile_own_copy(
        PROD,
        A111_PROD,
        "1.1.1",
        "aws-stack@1.1.1",
        RETAIN,
        "the same-names chapter gives prod a changed value",
    );
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111], &[]));
    r.tenant(|t| t.cluster(PROD, PAYMENTS_PROD, "payments", &[A111_PROD], &[]));
    r.step(
        "names-tenant",
        0,
        false,
        "strata list clusters --project ''",
    );
    r.step(
        "names-ambiguous",
        2,
        true,
        "strata snapshot cluster:payments -o payments.json",
    );
    r.step(
        "names-cluster",
        0,
        false,
        "strata list --cluster payments --project ''",
    );
    r.step(
        "names-profile",
        2,
        true,
        "strata snapshot aws-stack@1.1.1 -o aws-stack.json",
    );
    r.step(
        "names-by-project",
        0,
        true,
        "PALETTE_PROJECT=dev strata snapshot cluster:payments -o dev-payments.json",
    );
    r.step(
        "names-by-project",
        0,
        true,
        "PALETTE_PROJECT=prod strata snapshot cluster:payments -o prod-payments.json",
    );
    r.step(
        "names-verify",
        0,
        false,
        "strata verify dev-payments.json prod-payments.json",
    );
    r.step(
        "names-compare",
        1,
        false,
        "strata compare dev-payments.json prod-payments.json",
    );

    r.chapter("packs", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.tenant(|t| t.registries(DEV));
    r.step(
        "pack-compare",
        1,
        false,
        "strata compare 'pack:Public Repo/kubernetes@1.35.3' 'pack:Public Repo/kubernetes@1.36.2'",
    );
    r.excerpt(
        "pack-compare-images",
        "pack-compare",
        r"^(pack |root |images |~ \[|      [~+-] (image|chart) )",
    );
    r.step(
        "pack-os",
        1,
        false,
        "strata compare 'pack:Public Repo/ubuntu-aws@20.04' 'pack:Public Repo/ubuntu-aws@24.04'",
    );
    r.step(
        "pack-ambiguous",
        2,
        true,
        "strata compare pack:csi-aws-ebs@1.65.0 pack:csi-aws-ebs@1.65.0",
    );
    r.step("pack-registries", 1, false, "strata compare 'pack:Public Repo/csi-aws-ebs@1.65.0' 'pack:Palette Registry/csi-aws-ebs@1.65.0'");
    r.step(
        "pack-record",
        0,
        true,
        "strata snapshot 'pack:Public Repo/kubernetes@latest' -o kubernetes.json",
    );
    r.step("pack-verify", 0, false, "strata verify kubernetes.json");
    r.step(
        "pack-verify-json",
        0,
        false,
        "strata verify kubernetes.json --json | jq '.verify'",
    );
    r.step(
        "packs-list",
        0,
        false,
        "strata packs csi-aws-ebs --registry 'Public Repo'",
    );
    r.step(
        "packs-latest",
        0,
        false,
        "strata packs csi-aws-ebs --latest",
    );
    r.step(
        "pack-behind",
        1,
        false,
        "strata compare 'pack:Public Repo/kubernetes@1.35.3' 'pack:Public Repo/kubernetes@latest'",
    );
    // the claim is that @latest resolves, and that the report names the version it resolved to
    r.excerpt("pack-behind-head", "pack-behind", r"^(pack |root )");
    r.step(
        "packs-cloud",
        0,
        false,
        "strata packs kubernetes --registry 'Public Repo' --cloud aws --latest",
    );
    r.step("packs-json", 0, false, r#"strata packs csi-aws-ebs --registry 'Public Repo' --latest --json | jq '.registries[0].versions[0]'"#);

    // A rebuild of the newest version: strata orders it above its base, and Palette below
    r.chapter("palette-latest", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.registries(DEV));
    r.tenant(|t| t.rebuild_pack_version(DEV, "cert-manager", "1.21.1"));
    r.step(
        "packs-palette-latest",
        0,
        false,
        "strata packs cert-manager --latest",
    );
    r.step(
        "packs-palette-latest-json",
        0,
        false,
        "strata packs cert-manager --latest --json | jq -c '.registries[0].versions[] | {version, latest, paletteLatest}'",
    );

    r.chapter("retire", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A110], &[]));
    r.tenant(|t| t.cluster(DEV, SEARCH, "search", &[A110], &[]));
    r.step(
        "retire-in-use",
        1,
        false,
        "strata consumers aws-stack@1.1.0",
    );
    r.step(
        "consumers-list-only",
        1,
        false,
        "strata consumers aws-stack@1.1.0 --list-only",
    );
    r.step("retire-free", 0, false, "strata consumers aws-stack@1.0.0");
    r.step(
        "retire-gate",
        0,
        false,
        r#"strata consumers aws-stack@1.0.0 && echo "no cluster uses aws-stack@1.0.0""#,
    );
    r.step("retire-json", 1, false, r#"strata consumers aws-stack@1.1.0 --json | jq -r '.consumers[] | "\(.name) \(.state) \(.version)"'"#);
    // no cluster runs 1.1.1, and a cluster template links it: Palette refuses the delete
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.cluster_template(DEV, AWS_STANDARD, "aws-standard", &[A111]));
    r.step(
        "retire-template",
        1,
        false,
        "strata consumers aws-stack@1.1.1",
    );
}

fn automation(r: &mut Runner) {
    r.chapter("automation", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.step(
        "auto-max-impact",
        1,
        false,
        "strata compare aws-stack@1.0.0 aws-stack@1.1.0 --json | jq -r .summary.maxImpact",
    );
    r.step(
        "auto-gate",
        1,
        false,
        "strata compare aws-stack@1.1.0 aws-stack@1.1.1 --json > report.json",
    );
    r.step(
        "auto-gate",
        0,
        false,
        r#"jq -e '.summary.maxImpact != "repave"' report.json"#,
    );
    r.step(
        "auto-error",
        2,
        true,
        "strata compare aws-stack@1.1.1 aws-stack@9.9.9 --json --quiet",
    );
    r.step(
        "auto-error-text",
        2,
        true,
        "strata compare aws-stack@1.1.1 aws-stack@9.9.9 --quiet",
    );

    // preflight: can Palette build this cluster? The entity of an AWS create, with the profile
    // aws-stack@1.1.1; the stand-in answers the validate route with the CPU floor of the packs,
    // and holds the cloud account of the entity.
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
    // once Running: the cluster as Palette holds it, a change of its worker pool before the
    // change is made, and one reading of the watch (the stand-in's cluster is Running, so the
    // watch ends at once; a real build reads every 30 seconds until Running). The cluster gives
    // the variable of its profile a value, and the entity of the cluster carries it.
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
        "strata preflight cluster-addon.json --cloud aws --no-exec --tier palette --rule template-outside --strict --quiet | sed -n '4,12p'",
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
    // two rules that read the account of the entity (#143): the instance type route lists the
    // zones that do not offer a type, and the key pair route lists the key pairs of the region.
    // The pools move to the zone that the list names, and the key pair is not in the list.
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
    r.quiet(r#"jq '.spec.cloudConfig.sshKeyName = "retired"' cluster.json > cluster-key.json"#);
    r.step(
        "preflight-ssh-key",
        1,
        false,
        "strata preflight cluster-key.json --cloud aws --no-exec --tier palette --rule ssh-key-pair --strict --quiet | sed -n '4,6p'",
    );
    // a profile of another cloud type (#415): the validate route refuses the body with HTTP 500 and
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
}

/// The spec of the cloud config of a vSphere cluster, in the model of the API reference of
/// Palette. It holds a placement for the cluster and for each pool, with the uid that Palette gives
/// each one. It also holds the network of the control plane.
fn vsphere_spec() -> serde_json::Value {
    let network = json!({"networkName": "VM Network", "staticIp": true});
    let placement = |uid: &str| {
        json!({"uid": uid, "cluster": "compute-1", "datastore": "ds-fast",
            "resourcePool": "rp-lab", "folder": "spectro/lab", "network": network})
    };
    json!({
        "clusterConfig": {
            "placement": {"uid": "64f9000000000000000000a1", "datacenter": "dc-east",
                "folder": "spectro/lab", "network": network},
            "ntpServers": ["pool.ntp.org"], "staticIp": true,
            "sshKeys": ["ssh-ed25519 AAAAC3Nza lab"],
            "controlPlaneEndpoint": {"type": "IP", "host": "10.10.0.10"}},
        "machinePoolConfig": [
            {"name": "cp-pool", "isControlPlane": true, "size": 1,
                "instanceType": {"diskGiB": 60, "memoryMiB": 8192, "numCPUs": 4},
                "placements": [placement("64f9000000000000000000a2")],
                "machinePoolProperties": {"archType": "amd64"}, "labels": ["control-plane"]},
            {"name": "worker-pool", "isControlPlane": false, "size": 2,
                "instanceType": {"diskGiB": 80, "memoryMiB": 16384, "numCPUs": 8},
                "placements": [placement("64f9000000000000000000a3")],
                "machinePoolProperties": {"archType": "amd64"}, "labels": ["worker"]},
        ]
    })
}
