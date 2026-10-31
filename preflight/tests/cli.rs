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
fn verbose_names_what_each_check_ran_and_wins_over_the_quiet_variable() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("cluster.json"), ENTITY).unwrap();
    let o = Command::new(bin())
        .args(["cluster.json", "--cloud", "aws", "--offline", "--verbose"])
        .current_dir(dir.path())
        .env("STRATA_QUIET", "1")
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(
        err.contains("static: name-prefix: jq over the world -> ")
            && err.contains(" ms, PROBE PASS"),
        "{err}"
    );
    // --color always paints a pipe; auto does not
    let painted = Command::new(bin())
        .args([
            "cluster.json",
            "--cloud",
            "aws",
            "--offline",
            "--verbose",
            "--color",
            "always",
        ])
        .current_dir(dir.path())
        .env("STRATA_QUIET", "1")
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&painted.stderr).contains("\x1b[32mPROBE PASS\x1b[0m"),
        "{}",
        String::from_utf8_lossy(&painted.stderr)
    );
    assert!(!err.contains("\x1b["), "auto colors no pipe: {err}");
    // --quiet wins over --verbose
    let (_, _, quiet) = run(
        &[
            "cluster.json",
            "--cloud",
            "aws",
            "--offline",
            "--verbose",
            "--quiet",
        ],
        dir.path(),
    );
    assert!(quiet.is_empty(), "{quiet}");
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
fn a_rule_id_that_is_not_in_the_catalog_is_a_usage_error_that_tells_how_to_list_the_ids() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("cluster.json"), ENTITY).unwrap();
    // the two rules that left the catalog in 0.24.0
    for id in ["gke-pool-multiple-3", "profile-cloud-mismatch"] {
        let (code, out, err) = run(
            &["cluster.json", "--cloud", "eks", "--offline", "--rule", id],
            dir.path(),
        );
        assert_eq!(code, 2, "{id}: {err}");
        assert!(out.is_empty(), "no report: {out}");
        assert!(
            err.contains(&format!("no rule has the id {id}"))
                && err.contains("strata-preflight --rules"),
            "{id}: {err}"
        );
    }
    // a known id beside an unknown one: the message names the unknown one only
    let (code, _, err) = run(
        &[
            "cluster.json",
            "--cloud",
            "eks",
            "--offline",
            "--rule",
            "eks-one-zone",
            "--rule",
            "profile-cloud-mismatch",
            "--json",
        ],
        dir.path(),
    );
    assert_eq!(code, 2, "{err}");
    assert!(
        err.contains("\"kind\":\"usage\"")
            && err.contains("no rule has the id profile-cloud-mismatch."),
        "{err}"
    );
    // an id of a rule that exists but does not apply to the cloud is not an error
    let (code, _, err) = run(
        &[
            "cluster.json",
            "--cloud",
            "eks",
            "--offline",
            "--rule",
            "gke-api-off",
        ],
        dir.path(),
    );
    assert_eq!(code, 0, "{err}");
}

#[test]
fn a_rule_id_of_a_probes_file_is_known() {
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
            "--rule",
            "example-region",
            "--json",
        ],
        dir.path(),
    );
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("example-region"), "{out}");
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

#[test]
fn a_cluster_ref_names_the_whole_cluster_and_needs_palette() {
    let dir = tempfile::tempdir().unwrap();
    for source in ["cluster:abc/profile", "cluster:abc@pending"] {
        let (code, _, err) = run(&[source, "--offline"], dir.path());
        assert_eq!(code, 2, "{source}");
        assert!(
            err.contains("with no profile and no @pending"),
            "{source}: {err}"
        );
    }
    let (code, _, err) = run(&["cluster:abc", "--offline"], dir.path());
    assert_eq!(code, 2);
    assert!(err.contains("cluster:UID needs Palette"), "{err}");
}

#[test]
fn a_cluster_ref_with_a_profile_is_a_usage_error_with_no_key_and_before_any_request() {
    use std::io::Write;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // a server on this machine that counts the requests and refuses each one
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&requests);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            counter.fetch_add(1, Ordering::SeqCst);
            let mut stream = stream;
            let _ = stream
                .write_all(b"HTTP/1.1 500 Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        }
    });
    let dir = tempfile::tempdir().unwrap();
    for source in ["cluster:barney/addons", "cluster:barney@pending"] {
        // with no key, and with a key and a project name, which makes a project lookup first
        for key in [None, Some("k")] {
            let mut command = Command::new(bin());
            command
                .args([source, "--json"])
                .current_dir(dir.path())
                .env("HOME", dir.path())
                .env("STRATA_QUIET", "1")
                .env("STRATA_RETRIES", "0")
                .env("NO_COLOR", "1")
                .env("PALETTE_API", &host)
                .env("PALETTE_PROJECT", "payments")
                .env_remove("PALETTE_PROJECT_UID")
                .env_remove("SPECTROCLOUD_APIKEY")
                .env_remove("SPECTROCLOUD_HOST");
            match key {
                Some(key) => command.env("PALETTE_API_KEY", key),
                None => command.env_remove("PALETTE_API_KEY"),
            };
            let output = command.output().unwrap();
            let err = String::from_utf8_lossy(&output.stderr);
            assert_eq!(output.status.code(), Some(2), "{source} {key:?}: {err}");
            assert!(
                err.contains("\"kind\":\"usage\"") && err.contains("no profile and no @pending"),
                "{source} {key:?}: {err}"
            );
        }
    }
    assert_eq!(requests.load(Ordering::SeqCst), 0);
}

#[test]
fn the_rules_schema_takes_get_and_post_and_reads_when_a_check_names_no_method() {
    let dir = tempfile::tempdir().unwrap();
    let (code, out, err) = run(&["--schema", "rules"], dir.path());
    assert_eq!(code, 0, "{err}");
    let schema: serde_json::Value = serde_json::from_str(&out).unwrap();
    let values: Vec<&str> = schema["$defs"]["Method"]["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v["const"].as_str())
        .collect();
    assert_eq!(values, ["GET", "POST"]);
    let method = schema["$defs"]["Check"]["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["properties"]["kind"]["const"] == "palette")
        .map(|c| c["properties"]["method"].clone())
        .unwrap();
    assert_eq!(method["$ref"], "#/$defs/Method");
    assert_eq!(method["default"], "GET");
}

#[test]
fn the_report_schema_does_not_require_the_ref_of_an_evidence_entry() {
    let dir = tempfile::tempdir().unwrap();
    let (code, out, err) = run(&["--schema", "report"], dir.path());
    assert_eq!(code, 0, "{err}");
    let schema: serde_json::Value = serde_json::from_str(&out).unwrap();
    let evidence = &schema["$defs"]["Evidence"];
    assert!(evidence["properties"]["ref"].is_object(), "{evidence}");
    assert_eq!(evidence["required"], serde_json::json!(["kind"]));
}

#[test]
fn the_rules_listing_shows_each_on_the_rules_that_read_every_item() {
    let dir = tempfile::tempdir().unwrap();
    let (code, out, err) = run(&["--rules", "--cloud", "edge", "--json"], dir.path());
    assert_eq!(code, 0, "{err}");
    let listing: serde_json::Value = serde_json::from_str(&out).unwrap();
    let each = |id: &str| -> Option<String> {
        let rule = listing["rules"]
            .as_array()?
            .iter()
            .find(|r| r["id"] == id)?;
        rule["check"]["each"].as_str().map(str::to_string)
    };
    for id in [
        "template-outside",
        "edge-host-in-use",
        "edge-host-health",
        "edge-host-arch",
        "edge-host-resources",
        "edge-host-nic",
        "edge-agent-version",
    ] {
        assert!(
            each(id).is_some_and(|e| !e.trim().is_empty()),
            "{id} has no each"
        );
    }
    // the rules that Palette answers in one request keep one request
    assert_eq!(each("variables-in-use"), None);
}

#[test]
fn the_rules_schema_has_each_as_an_optional_program_of_a_check_of_palette() {
    let dir = tempfile::tempdir().unwrap();
    let (code, out, err) = run(&["--schema", "rules"], dir.path());
    assert_eq!(code, 0, "{err}");
    let schema: serde_json::Value = serde_json::from_str(&out).unwrap();
    let palette = schema["$defs"]["Check"]["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["properties"]["kind"]["const"] == "palette")
        .unwrap();
    assert_eq!(
        palette["properties"]["each"]["type"],
        serde_json::json!(["string", "null"])
    );
    let required: Vec<&str> = palette["required"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r.as_str())
        .collect();
    assert!(!required.contains(&"each"), "{required:?}");
}

#[test]
fn the_rules_listing_states_that_palette_checks_the_list_of_kms_keys_only() {
    let dir = tempfile::tempdir().unwrap();
    let (code, out, err) = run(&["--rules", "--cloud", "eks", "--json"], dir.path());
    assert_eq!(code, 0, "{err}");
    let listing: serde_json::Value = serde_json::from_str(&out).unwrap();
    let rule = listing["rules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "kms")
        .expect("the listing holds the kms rule");
    let what = rule["what"].as_str().unwrap();
    for text in [
        "the ARN is in the `kms:ListKeys` list",
        "reads no key state and no `kms:CreateGrant`",
        "answers `500` when the role has no `kms:ListKeys`",
    ] {
        assert!(what.contains(text), "{text}: {what}");
    }
    assert!(!what.contains("covers the Palette side"), "{what}");
    let fix = rule["fix"]["text"].as_str().unwrap();
    assert!(fix.contains("kms:CreateGrant"), "{fix}");
}

#[test]
fn the_report_schema_gives_a_request_a_method_and_a_path() {
    let dir = tempfile::tempdir().unwrap();
    let (code, out, err) = run(&["--schema", "report"], dir.path());
    assert_eq!(code, 0, "{err}");
    let schema: serde_json::Value = serde_json::from_str(&out).unwrap();
    let call = &schema["$defs"]["Call"];
    assert_eq!(call["properties"]["method"]["$ref"], "#/$defs/Method");
    assert_eq!(call["properties"]["path"]["type"], "string");
    assert_eq!(call["required"], serde_json::json!(["method", "path"]));
    assert!(call["properties"].get("ran").is_none());
}
