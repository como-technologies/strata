//! The binary, end to end: a custom probes file, the static tier offline, the exit codes, and the
//! JSON document. No network.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};

// The lock of the tests of strata: a process that a test starts takes it. These tests write no
// script, so they do not call `executable`.
#[path = "../../tests/common/process.rs"]
#[allow(dead_code)]
mod process;
use process::CommandExt;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_strata-preflight"))
}

fn run(args: &[&str], dir: &std::path::Path) -> (i32, String, String) {
    let o = Command::new(bin())
        .args(args)
        .current_dir(dir)
        .env("STRATA_QUIET", "1")
        .env("NO_COLOR", "1")
        .output_locked()
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
fn a_probes_file_with_a_program_that_cannot_run_is_refused_before_the_first_request() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("cluster.json"), ENTITY).unwrap();
    // the program over the responses runs with no item, so `$item` is not a variable of it
    let probes = r#"
section: landing-zone
title: The landing zone of Example Corp
rules:
  - id: example-items
    title: Each profile is known
    engines: [aws]
    tier: palette
    status: proposed
    what: Palette knows each profile of the entity.
    check:
      kind: palette
      path: /v1/clusterprofiles/{{ $item.uid }}
      each: .entity.spec.profiles
      expr: '[.responses[] | select(.response.ok | not) | "profile \($item.uid) is unknown"]'
"#;
    std::fs::write(dir.path().join("items.yaml"), probes).unwrap();
    let (code, out, err) = run(
        &[
            "cluster.json",
            "--cloud",
            "aws",
            "--offline",
            "--probes",
            "items.yaml",
        ],
        dir.path(),
    );
    assert_eq!(code, 2, "{out}");
    assert!(
        err.contains(
            "rule example-items: `expr`: the program does not compile: undefined variable $item"
        ),
        "{err}"
    );
}

#[test]
fn a_probes_file_with_a_program_that_nests_in_keywords_or_comments_is_refused_with_exit_2() {
    let dir = tempfile::tempdir().unwrap();
    // each program is far past the depth that overflows the stack of the thread of the binary
    let programs = [
        ("try", format!("{}[]", "try ".repeat(8000))),
        ("and", format!("{}true", "true and ".repeat(20000))),
        (
            "if",
            format!(
                "{}1{}",
                "if . then ".repeat(5000),
                " else 2 end".repeat(5000)
            ),
        ),
        (
            "def",
            format!("{}.{}", "def f: ".repeat(5000), "; f".repeat(5000)),
        ),
        (
            "comment",
            format!("{}1{}", "(# )\n".repeat(8000), ")".repeat(8000)),
        ),
    ];
    for (shape, program) in programs {
        let probes = format!(
            "section: deep\ntitle: Deep\nrules:\n  - id: deep-{shape}\n    title: A deep program\n    engines: [aws]\n    tier: static\n    status: proposed\n    what: The program nests too deep.\n    check:\n      kind: static\n      expr: {}\n",
            serde_json::to_string(&program).unwrap()
        );
        std::fs::write(dir.path().join("deep.yaml"), probes).unwrap();
        let (code, out, err) = run(&["--rules", "--probes", "deep.yaml"], dir.path());
        assert_eq!(code, 2, "{shape}: {err}");
        assert!(out.is_empty(), "{shape}: {out}");
        assert!(
            err.contains(&format!(
                "rule deep-{shape}: `expr`: the program nests more than 64 levels"
            )),
            "{shape}: {err}"
        );
    }
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
        .output_locked()
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
        .output_locked()
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

/// Run the offline report of `ENTITY` in a directory, with `--verbose` and the given variables,
/// and give the standard output and the standard error. `NO_COLOR`, `CLICOLOR_FORCE`, and `TERM`
/// are set only by `vars`.
fn colors(vars: &[(&str, &str)], args: &[&str]) -> (String, String) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("cluster.json"), ENTITY).unwrap();
    let mut command = Command::new(bin());
    command
        .args(["cluster.json", "--cloud", "aws", "--offline", "--verbose"])
        .args(args)
        .current_dir(dir.path())
        .env("STRATA_QUIET", "1")
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("TERM");
    for (name, value) in vars {
        command.env(name, value);
    }
    let o = command.output_locked().unwrap();
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

#[test]
fn the_color_follows_the_order_of_strata_for_each_stream() {
    let painted = |text: &str| text.contains("\x1b[");
    // a pipe has no color
    let (out, err) = colors(&[], &[]);
    assert!(!painted(&out) && !painted(&err), "{out}{err}");
    // CLICOLOR_FORCE colors both streams, also with TERM=dumb
    let (out, err) = colors(&[("CLICOLOR_FORCE", "1"), ("TERM", "dumb")], &[]);
    assert!(painted(&out) && painted(&err), "{out}{err}");
    // NO_COLOR with a value wins over CLICOLOR_FORCE
    let (out, err) = colors(&[("CLICOLOR_FORCE", "1"), ("NO_COLOR", "1")], &[]);
    assert!(!painted(&out) && !painted(&err), "{out}{err}");
    // NO_COLOR with no value is not set, as `strata` reads it
    let (out, err) = colors(&[("CLICOLOR_FORCE", "1"), ("NO_COLOR", "")], &[]);
    assert!(painted(&out) && painted(&err), "{out}{err}");
    // the flags win over the environment
    let (out, err) = colors(&[("CLICOLOR_FORCE", "1")], &["--no-color"]);
    assert!(!painted(&out) && !painted(&err), "{out}{err}");
    let (out, err) = colors(&[("NO_COLOR", "1")], &["--color", "always"]);
    assert!(painted(&out) && painted(&err), "{out}{err}");
}

#[test]
fn out_names_the_path_in_the_error_when_it_cannot_write_and_makes_the_directory_when_it_can() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("cluster.json"), ENTITY).unwrap();
    // a file in the place of the directory
    std::fs::write(dir.path().join("taken"), "x").unwrap();
    let (code, _, err) = run(
        &[
            "cluster.json",
            "--cloud",
            "aws",
            "--offline",
            "--out",
            "taken/run",
        ],
        dir.path(),
    );
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("taken/run"), "the path is in the error: {err}");
    // a directory that is not there is made, with its parents
    let (code, _, err) = run(
        &[
            "cluster.json",
            "--cloud",
            "aws",
            "--offline",
            "--out",
            "a/b/run",
        ],
        dir.path(),
    );
    assert_eq!(code, 0, "{err}");
    for file in [
        "report.md",
        "report.json",
        "report.html",
        "world.json",
        "entity.json",
    ] {
        assert!(dir.path().join("a/b/run").join(file).is_file(), "{file}");
    }
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

/// A server on this machine that refuses each request. Gives its address and the first line of each
/// request that it got, such as `GET /v1/projects HTTP/1.1`.
fn refusing_server() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&requests);
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut line = String::new();
            let _ = BufReader::new(&stream).read_line(&mut line);
            seen.lock().unwrap().push(line.trim_end().to_string());
            let _ = stream
                .write_all(b"HTTP/1.1 500 Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        }
    });
    (host, requests)
}

/// The binary, set to run against the server `host` with the project `payments` and, when given, a
/// key. A request goes to the server, and a run with no key stops before it.
fn command_against(host: &str, key: Option<&str>, dir: &std::path::Path) -> Command {
    let mut command = Command::new(bin());
    command
        .current_dir(dir)
        .env("HOME", dir)
        .env("STRATA_QUIET", "1")
        .env("STRATA_RETRIES", "0")
        .env("NO_COLOR", "1")
        .env("PALETTE_API", host)
        .env("PALETTE_PROJECT", "payments")
        .env_remove("PALETTE_PROJECT_UID")
        .env_remove("SPECTROCLOUD_APIKEY")
        .env_remove("SPECTROCLOUD_HOST");
    match key {
        Some(key) => command.env("PALETTE_API_KEY", key),
        None => command.env_remove("PALETTE_API_KEY"),
    };
    command
}

/// Run [`command_against`] with `args` and give its output.
fn run_against(
    host: &str,
    key: Option<&str>,
    args: &[&str],
    dir: &std::path::Path,
) -> std::process::Output {
    command_against(host, key, dir)
        .args(args)
        .output_locked()
        .unwrap()
}

#[test]
fn a_cluster_ref_with_a_profile_is_a_usage_error_with_no_key_and_before_any_request() {
    let (host, requests) = refusing_server();
    let dir = tempfile::tempdir().unwrap();
    for source in ["cluster:barney/addons", "cluster:barney@pending"] {
        // with no key, and with a key and a project name, which makes a project lookup first
        for key in [None, Some("k")] {
            let output = run_against(&host, key, &[source, "--json"], dir.path());
            let err = String::from_utf8_lossy(&output.stderr);
            assert_eq!(output.status.code(), Some(2), "{source} {key:?}: {err}");
            assert!(
                err.contains("\"kind\":\"usage\"") && err.contains("no profile and no @pending"),
                "{source} {key:?}: {err}"
            );
        }
    }
    assert!(requests.lock().unwrap().is_empty());
}

#[test]
fn a_release_that_is_not_a_version_is_a_usage_error_with_no_key_and_before_any_request() {
    let (host, requests) = refusing_server();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("cluster.json"), ENTITY).unwrap();
    let runs: [&[&str]; 3] = [
        &["cluster.json", "--cloud", "aws", "--release", "4.11.x"],
        &["cluster:barney", "--release", "4.11.x"],
        &["--rules", "--release", "4.11.x"],
    ];
    for args in runs {
        for key in [None, Some("k")] {
            let mut args = args.to_vec();
            args.push("--json");
            let output = run_against(&host, key, &args, dir.path());
            let err = String::from_utf8_lossy(&output.stderr);
            assert_eq!(output.status.code(), Some(2), "{args:?} {key:?}: {err}");
            assert!(
                err.contains("\"kind\":\"usage\"")
                    && err.contains("--release 4.11.x: give a release such as 4.10 or 4.10.12"),
                "{args:?} {key:?}: {err}"
            );
        }
    }
    assert!(requests.lock().unwrap().is_empty());
    // a release with one to three numbers lists the rules
    for release in ["4", "4.11", "4.11.0", "4.11.0-rc.1"] {
        let (code, _, err) = run(&["--rules", "--release", release], dir.path());
        assert_eq!(code, 0, "{release}: {err}");
    }
}

#[test]
fn a_records_pattern_that_matches_no_file_is_a_pattern_error_as_in_strata() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("cluster.json"), ENTITY).unwrap();
    let pattern = "/nonexistent-barney/*.json";
    let (code, out, err) = run(
        &[
            "cluster.json",
            "--cloud",
            "aws",
            "--offline",
            "--records",
            pattern,
            "--json",
        ],
        dir.path(),
    );
    assert_eq!(code, 2, "{out}");
    let error: serde_json::Value = serde_json::from_str(err.trim()).unwrap();
    assert_eq!(
        error,
        serde_json::json!({"error": {
            "kind": "pattern",
            "message": format!("'{pattern}' matched no files"),
            "pattern": pattern,
        }})
    );
    // a pattern that is not valid has the kind too
    let (code, _, err) = run(
        &[
            "cluster.json",
            "--cloud",
            "aws",
            "--offline",
            "--records",
            "[",
            "--json",
        ],
        dir.path(),
    );
    assert_eq!(code, 2);
    assert!(err.contains("\"kind\":\"pattern\""), "{err}");
}

#[test]
fn a_uid_and_a_name_of_a_project_start_the_lookup_of_the_name_as_in_strata() {
    let (host, requests) = refusing_server();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("cluster.json"), ENTITY).unwrap();
    let args = ["cluster.json", "--cloud", "aws"];
    let mut command = command_against(&host, Some("k"), dir.path());
    // a person at the terminal: the lookup and the warning are on
    command
        .args(args)
        .env_remove("STRATA_QUIET")
        .env("PALETTE_PROJECT_UID", "p1")
        .env("PALETTE_PROJECT", "lab");
    let output = command.output_locked().unwrap();
    let err = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{err}");
    assert!(
        err.contains("looking up project lab"),
        "the name is resolved to compare it with the uid: {err}"
    );
    // the server refuses the lookup, so the name is a warning and the uid decides
    assert!(
        err.contains("warning: PALETTE_PROJECT is 'lab', which no project matches")
            && err.contains("PALETTE_PROJECT_UID is p1, and a uid has priority"),
        "{err}"
    );
    assert!(!requests.lock().unwrap().is_empty());
    // a quiet run writes no warning, so it makes no lookup for one
    let mut command = command_against(&host, Some("k"), dir.path());
    command
        .args(args)
        .env("PALETTE_PROJECT_UID", "p1")
        .env("PALETTE_PROJECT", "lab");
    let output = command.output_locked().unwrap();
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(
        !err.contains("looking up project") && !err.contains("warning:"),
        "{err}"
    );
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
fn no_description_of_a_schema_holds_the_markup_of_a_rustdoc_link() {
    let dir = tempfile::tempdir().unwrap();
    for kind in ["report", "rules", "ledger", "promotion"] {
        let (code, out, err) = run(&["--schema", kind], dir.path());
        assert_eq!(code, 0, "{kind}: {err}");
        assert!(!out.contains("[`"), "{kind}: a link in a description");
    }
    let (_, out, _) = run(&["--schema", "rules"], dir.path());
    let schema: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        schema["$defs"]["Method"]["oneOf"][0]["description"],
        "A read of a document: `PaletteApi::get_json`."
    );
}

/// The split of long sentences in the doc comments gave seven descriptions new words. The section
/// of release 0.28.0 in `compatibility.md` names them, and a test of the root package reads that
/// page.
#[test]
fn the_schemas_hold_the_new_words_of_the_descriptions_that_the_release_notes_name() {
    let dir = tempfile::tempdir().unwrap();
    for (kind, pointer, new_words) in [
        (
            "report",
            "/$defs/Verdict/oneOf/3/description",
            "the check did not run. The tier was not asked",
        ),
        (
            "report",
            "/$defs/Verdict/oneOf/4/description",
            "The cause is another release or a `when` that is not true.",
        ),
        (
            "rules",
            "/$defs/Check/oneOf/1/description",
            "runs one time for each item of a list. The program runs one time",
        ),
        (
            "rules",
            "/$defs/Check/oneOf/2/description",
            "is on the PATH. When one is not, the result is `NOT CHECKED`",
        ),
        (
            "rules",
            "/$defs/Check/oneOf/3/description",
            "During a build, every `every` seconds: read the cluster",
        ),
        (
            "rules",
            "/$defs/Rule/properties/check/description",
            "A rule with none is a row of the catalog. The report prints it",
        ),
        (
            "rules",
            "/$defs/Rule/properties/when/description",
            "Use `.before != null` for a rule on a change.",
        ),
    ] {
        let (code, out, err) = run(&["--schema", kind], dir.path());
        assert_eq!(code, 0, "{kind}: {err}");
        let schema: serde_json::Value = serde_json::from_str(&out).unwrap();
        let description = schema
            .pointer(pointer)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("{kind} has no description at {pointer}"));
        let one_line = description.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(one_line.contains(new_words), "{kind} {pointer}: {one_line}");
    }
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

/// The ledger of a company. The runs of the lab have no release, so a run on 4.10 proves the one
/// preflight check of the catalog on 4.10.
const RUNS_OF_4_10: &str = r#"
schema: strata-preflight-ledger/1
runs:
  - run: 20261101T120000Z
    engine: gke
    release: "4.10.2"
    results:
      - {rule: gke-no-release-channel, result: caught}
      - {rule: gke-no-release-channel, result: passed}
"#;

#[test]
fn the_promotion_counts_only_the_runs_of_the_release_it_is_given() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("runs.yaml"), RUNS_OF_4_10).unwrap();
    let lab_runs = lab_runs_of_the_rule();
    let level = |args: &[&str]| -> (i32, serde_json::Value) {
        let mut all = vec!["--promotion", "--json"];
        all.extend_from_slice(args);
        let (code, out, err) = run(&all, dir.path());
        assert!(code <= 1, "{err}");
        (code, serde_json::from_str(&out).unwrap())
    };
    let rule = |doc: &serde_json::Value| -> Option<serde_json::Value> {
        doc["rules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == "gke-no-release-channel")
            .cloned()
    };

    // no release: each run counts, and the level agrees with the lab ledger
    let (code, doc) = level(&[]);
    assert_eq!((code, doc["disagreements"].as_u64()), (0, Some(0)));
    assert!(doc.get("release").is_none(), "{doc}");

    // 4.10: the lab runs have no release, so the rule is a probe, and the level demotes
    let (code, doc) = level(&["--release", "4.10"]);
    assert_eq!(code, 1, "a level that the runs do not give is exit 1");
    assert_eq!(doc["release"], "4.10");
    let r = rule(&doc).expect("the rule applies to 4.10");
    assert_eq!(
        (r["earned"].as_str(), r["advice"].as_str()),
        (Some("probe"), Some("demote"))
    );
    assert_eq!(
        r["confidence"][0]["olderRuns"], lab_runs,
        "the runs of the lab"
    );

    // a run on 4.10 with the ledger of a company proves it again, on a patch of 4.10 too
    let (code, doc) = level(&["--release", "4.10.7", "--ledger", "runs.yaml"]);
    assert_eq!(code, 0, "{doc}");
    let r = rule(&doc).unwrap();
    assert_eq!(
        (r["earned"].as_str(), r["advice"].as_str()),
        (Some("preflight"), Some(""))
    );
    assert_eq!(r["confidence"][0]["olderRuns"], lab_runs);
    assert_eq!(
        r["confidence"][0]["runs"],
        serde_json::json!(["20261101T120000Z"])
    );

    // 4.11: the rule ends at 4.11, so the report has no entry for it
    let (code, doc) = level(&["--release", "4.11", "--ledger", "runs.yaml"]);
    assert_eq!(code, 0);
    assert!(
        rule(&doc).is_none(),
        "a rule outside the release has no entry"
    );
}

#[test]
fn the_promotion_text_names_the_runs_of_older_releases_for_each_rule() {
    let dir = tempfile::tempdir().unwrap();
    let (code, out, err) = run(&["--promotion", "--release", "4.10"], dir.path());
    assert_eq!(code, 1, "{err}");
    let line = out
        .lines()
        .find(|l| l.starts_with("gke-no-release-channel"))
        .expect("the rule is in the list");
    assert!(line.contains("demote"), "{line}");
    let lab_runs = lab_runs_of_the_rule();
    assert!(
        line.contains(&format!("gke 4.10: no runs, older runs {lab_runs}")),
        "{line}"
    );
    let (code, _, err) = run(&["--promotion", "--release", "4.11.x"], dir.path());
    assert_eq!(
        code, 2,
        "a release that is not a version is a usage error: {err}"
    );
}

/// How many runs of the ledger in the binary answer `gke-no-release-channel`: they have no release.
fn lab_runs_of_the_rule() -> usize {
    let ledger = strata_preflight::catalog::builtin_ledger().unwrap();
    ledger
        .counts_for("gke-no-release-channel", "gke", None)
        .runs
        .len()
}

#[test]
fn the_text_of_the_promotion_agrees_the_noun_and_the_verb_with_the_count_of_levels() {
    let dir = tempfile::tempdir().unwrap();
    // no release: each run counts, and the level agrees with the lab ledger
    let (code, out, err) = run(&["--promotion"], dir.path());
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains(" rules with a check; no levels disagree with the ledger\n"),
        "{out}"
    );
    // 4.10: the runs of the lab have no release, so one level disagrees with the ledger
    let (code, out, err) = run(&["--promotion", "--release", "4.10"], dir.path());
    assert_eq!(code, 1, "{err}");
    assert!(
        out.contains(" rules with a check; 1 level disagrees with the ledger\n"),
        "{out}"
    );
}
