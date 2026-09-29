//! The binary, end to end: a custom probes file, the static tier offline, the exit codes, and the
//! JSON document. No network.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_strata-preflight"))
}

fn run(args: &[&str], dir: &std::path::Path) -> (i32, String, String) {
    let o = Command::new(bin())
        .args(args)
        .current_dir(dir)
        .env("STRATA_QUIET", "1")
        .env("NO_COLOR", "1")
        .output()
        .expect("the binary runs");
    (
        o.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

const ENTITY: &str = r#"{"metadata": {"name": "payments"}, "spec": {"cloudAccountUid": "a1",
  "cloudConfig": {"region": "eu-west-3", "vpcId": "", "sshKeyName": "k"},
  "machinepoolconfig": [{"cloudConfig": {"instanceType": "t3.xlarge", "azs": ["eu-west-3a", "eu-west-3b"]},
    "poolConfig": {"name": "worker-pool", "size": 2, "isControlPlane": false}}],
  "profiles": [{"uid": "p1", "packValues": [], "variables": []}]}}"#;

const PROBES: &str = r#"
section: landing-zone
title: The landing zone of Example Corp
rules:
  - id: example-region
    title: The cluster is in an allowed region
    engines: [aws, eks]
    tier: static
    status: proposed
    what: The landing zone holds us-east-2 and eu-west-1. A cluster in another region has no VPC.
    check:
      kind: static
      expr: |
        .entity.spec.cloudConfig.region as $r
        | if (["us-east-2", "eu-west-1"] | index($r)) != null then []
          else ["region \($r) is not in the landing zone"] end
"#;

#[test]
fn a_custom_probes_file_runs_beside_the_catalog_and_strict_counts_it() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("cluster.json"), ENTITY).unwrap();
    std::fs::write(dir.path().join("landing-zone.yaml"), PROBES).unwrap();
    let (code, out, err) = run(
        &[
            "cluster.json",
            "--cloud",
            "eks",
            "--offline",
            "--probes",
            "landing-zone.yaml",
            "--json",
        ],
        dir.path(),
    );
    assert_eq!(
        code, 0,
        "a failed probe does not change the exit code: {err}"
    );
    let doc: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(doc["schema"], "strata-preflight/1");
    let probe = doc["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "example-region")
        .expect("the custom probe is in the report");
    assert_eq!(probe["verdict"], "failed");
    assert_eq!(probe["section"], "landing-zone");
    assert_eq!(
        probe["messages"][0],
        "region eu-west-3 is not in the landing zone"
    );
    assert!(
        doc["sections"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["section"] == "landing-zone"),
        "the section is in the scorecard"
    );
    // the same run with --strict is exit 1, and names the probe
    let (code, out, _) = run(
        &[
            "cluster.json",
            "--cloud",
            "eks",
            "--offline",
            "--probes",
            "landing-zone.yaml",
            "--strict",
        ],
        dir.path(),
    );
    assert_eq!(code, 1);
    assert!(
        out.contains("FAIL: ") && out.contains("example-region"),
        "{out}"
    );
    // the listing names it too, with its level
    let (code, out, _) = run(
        &["--rules", "--cloud", "eks", "--probes", "landing-zone.yaml"],
        dir.path(),
    );
    assert_eq!(code, 0);
    assert!(
        out.contains("example-region") && out.contains("probe"),
        "{out}"
    );
}

#[test]
fn a_probes_file_that_shadows_a_rule_of_the_catalog_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("dup.yaml"),
        PROBES.replace("id: example-region", "id: eks-one-zone"),
    )
    .unwrap();
    let (code, _, err) = run(&["--rules", "--probes", "dup.yaml"], dir.path());
    assert_eq!(code, 2);
    assert!(
        err.contains("eks-one-zone is in the catalog already"),
        "{err}"
    );
}

#[test]
fn an_entity_with_no_cloud_says_to_give_one_and_a_bad_cloud_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("cluster.json"), ENTITY).unwrap();
    let (code, _, err) = run(&["cluster.json", "--offline"], dir.path());
    assert_eq!(code, 2);
    assert!(err.contains("give --cloud"), "{err}");
    let (code, _, err) = run(
        &["cluster.json", "--offline", "--cloud", "mars", "--json"],
        dir.path(),
    );
    assert_eq!(code, 2);
    assert!(
        err.contains("\"kind\":\"input\""),
        "a JSON error with --json: {err}"
    );
}
