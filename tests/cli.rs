//! End-to-end tests over raw-API-shaped fixtures (no network).

mod common;

use std::path::PathBuf;
use std::process::Command;

const ESC: char = '\x1b';

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_strata"))
}

struct Run {
    code: i32,
    out: String,
    err: String,
}

fn run_in(args: &[&str], envs: &[(&str, &str)], clear: &[&str]) -> Run {
    let mut c = Command::new(bin());
    c.args(args).env("STRATA_QUIET", "1");
    for k in clear {
        c.env_remove(k);
    }
    for (k, v) in envs {
        c.env(k, v);
    }
    let o = c.output().expect("binary runs");
    Run {
        code: o.status.code().unwrap_or(-1),
        out: String::from_utf8_lossy(&o.stdout).into_owned(),
        err: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

fn run(args: &[&str]) -> Run {
    run_in(args, &[], &["NO_COLOR", "CLICOLOR_FORCE"])
}

struct Fx {
    _dir: tempfile::TempDir,
    raw: PathBuf,
    a: String,
    a2: String,
    b: String,
}

fn fixtures() -> Fx {
    let dir = tempfile::tempdir().unwrap();
    let raw = dir.path().join("fx");
    common::make_fixtures(&raw);
    let snap = |name: &str| -> String {
        let out = dir.path().join(format!("{name}.json"));
        let r = run(&[
            "snapshot",
            "--from-raw",
            raw.join(name).to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ]);
        assert_eq!(r.code, 0, "snapshot {name}: {}", r.err);
        out.to_str().unwrap().to_string()
    };
    let (a, a2, b) = (snap("a"), snap("a2"), snap("b"));
    Fx {
        _dir: dir,
        raw,
        a,
        a2,
        b,
    }
}

fn json(path: &str) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn has_line(text: &str, re: &str) -> bool {
    // tiny matcher: "^" anchors a line start, the rest is a literal
    let lit = re.strip_prefix('^').unwrap_or(re);
    text.lines().any(|l| {
        if re.starts_with('^') {
            l.starts_with(lit)
        } else {
            l.contains(lit)
        }
    })
}

#[test]
fn snapshot_shape_and_upstream_sources() {
    let fx = fixtures();
    let a = json(&fx.a);
    assert_eq!(a["schema"], "strata-snapshot/1");
    assert_eq!(a["tool"]["impl"], "rust");
    assert_eq!(a["packs"].as_array().unwrap().len(), 7);
    let pack = |n: &str| {
        a["packs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == n)
            .cloned()
            .unwrap()
    };
    assert_eq!(
        pack("ubuntu-aws")["hash"]["upstreamSource"],
        "packSummary.digest"
    );
    assert_eq!(
        pack("kubernetes")["hash"]["upstreamSource"],
        "packRef.digest"
    );
    assert!(pack("metrics-server")["valuesParseError"].is_string());
    assert_eq!(a["source"]["consumers"]["count"], 0);
}

#[test]
fn reordered_yaml_gives_the_same_root() {
    let fx = fixtures();
    assert_eq!(json(&fx.a)["root"], json(&fx.a2)["root"]);
}

#[test]
fn identical_profiles_exit_zero() {
    let fx = fixtures();
    let r = run(&["compare", &fx.a, &fx.a2]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(r.out.contains("IDENTICAL"));
    assert!(!r.out.lines().any(|l| l.starts_with("~ [")));
}

#[test]
fn changed_profiles_report_every_kind_of_change() {
    let fx = fixtures();
    let r = run(&[&fx.a, &fx.b]); // default subcommand
    assert_eq!(r.code, 1, "{}", r.err);
    let t = &r.out;
    assert!(has_line(t, "^lab-profile@1.2.0 (uid 64f0000000000000000000a1)  ->  lab-profile@1.3.0 (uid 64f0000000000000000000b2)"));
    assert!(t.contains("CHANGED"));
    assert!(has_line(t, "^consumers  0 -> 2 clusters"));
    assert!(has_line(
        t,
        "^~ [k8s  ] kubernetes                 1.29.4 -> 1.30.2   IDENTITY UPSTREAM AUTHORED"
    ));
    assert!(t.contains("~ kubeadmconfig.apiServer.extraArgs.audit-log-maxage: \"30\" -> \"90\""));
    assert!(t.contains("+ kubeadmconfig.apiServer.extraArgs.profiling: \"false\""));
    assert!(t.contains("~ pack.k8sHardening: true -> false"));
    assert!(t.contains("impact: node repave (Kubernetes version change)"));
    assert!(has_line(
        t,
        "^~ [cni  ] cni-cilium-oss             1.15.3             UPSTREAM"
    ));
    assert!(t.contains("upstream digest  sha256:cni-a -> sha256:cni-b   (packRef.digest)"));
    assert!(t.contains("pack content changed under the same tag"));
    assert!(has_line(
        t,
        "^  [os   ] ubuntu-aws                 22.04              unchanged"
    ));
    assert!(has_line(
        t,
        "^~ [addon] prometheus-operator        55.8.3             AUTHORED"
    ));
    assert!(t.contains("~ manifest extra-rules"));
    assert!(t.contains("+ manifest dashboards"));
    assert!(has_line(
        t,
        "^+ [addon] cert-manager               1.14.0             ADDED"
    ));
    assert!(has_line(
        t,
        "^- [addon] external-dns               0.14.0             REMOVED"
    ));
    assert!(t.contains("values changed (not parseable as YAML"));
    assert!(t.contains("variables CHANGED"));
    assert!(t.contains("~ cluster_env.defaultValue: \"dev\" -> \"staging\""));
    assert!(
        t.contains("+ variable db_password (required, sensitive)"),
        "an added variable is one line, not one per field:\n{t}"
    );
    assert!(!t.contains("db_password.name"), "{t}");
    assert!(
        !t.contains("cluster_env.name"),
        "only the changed field of a changed variable shows"
    );
    assert!(!t.contains(ESC));
}

#[test]
fn json_document_is_self_describing() {
    let fx = fixtures();
    let r = run(&["compare", "--json", &fx.a, &fx.b]);
    assert_eq!(r.code, 1);
    assert!(!r.out.contains(ESC));
    let d: serde_json::Value = serde_json::from_str(&r.out).unwrap();
    assert_eq!(d["schema"], "strata-diff/1");
    assert_eq!(d["same"], false);
    assert!(d["legend"]["impact"]["repave"].is_string());
    assert!(d["legend"]["upstream"].is_string());
    let s = &d["summary"];
    assert_eq!(s["packsAdded"], 1);
    assert_eq!(s["packsRemoved"], 1);
    assert_eq!(s["packsChanged"], 4);
    assert_eq!(s["packsUnchanged"], 2);
    assert_eq!(s["variablesChanged"], true);
    assert_eq!(s["maxImpact"], "repave");
    let pack = |n: &str| {
        d["packs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == n)
            .cloned()
            .unwrap()
    };
    let cil = pack("cni-cilium-oss");
    assert_eq!(
        (
            cil["identity"].clone(),
            cil["upstream"].clone(),
            cil["authored"].clone()
        ),
        (false.into(), true.into(), false.into())
    );
    assert_eq!(cil["impact"]["level"], "sync");
    let k8s = pack("kubernetes");
    assert_eq!(k8s["values"]["diff"].as_array().unwrap().len(), 4);
    assert_eq!(k8s["values"]["parseable"], true);
    assert_eq!(k8s["impact"]["level"], "repave");
    let leaf = &k8s["values"]["diff"][0];
    assert!(leaf["segments"].is_array());
    assert!(leaf["op"].is_string());
    let ms = pack("metrics-server");
    assert_eq!(ms["values"]["parseable"], false);
    assert_eq!(pack("cert-manager")["status"], "added");
    assert_eq!(pack("external-dns")["status"], "removed");
    assert!(
        !d["variables"]["diff"].as_array().unwrap().is_empty(),
        "leaf form stays"
    );
    assert_eq!(
        d["variables"]["changed"], true,
        "the boolean keeps its meaning"
    );
    let vars = &d["variables"]["byName"];
    assert_eq!(vars["added"].as_array().unwrap().len(), 1);
    assert_eq!(vars["added"][0]["name"], "db_password");
    assert_eq!(vars["added"][0]["required"], true);
    assert_eq!(vars["added"][0]["sensitive"], true);
    assert_eq!(vars["added"][0]["hidden"], false);
    assert_eq!(vars["removed"].as_array().unwrap().len(), 0);
    assert_eq!(vars["changed"].as_array().unwrap().len(), 1);
    assert_eq!(vars["changed"][0]["name"], "cluster_env");
    let fields = vars["changed"][0]["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0]["path"], "defaultValue");
    assert_eq!(fields[0]["op"], "changed");
    assert_eq!(fields[0]["old"], "dev");
    assert_eq!(fields[0]["new"], "staging");
    let r2 = run(&["compare", "--json", "--compact", &fx.a, &fx.b]);
    assert_eq!(r2.out.trim().lines().count(), 1);
}

#[test]
fn markdown_report_has_table_and_details() {
    let fx = fixtures();
    let r = run(&["compare", "--format", "markdown", &fx.a, &fx.b]);
    assert_eq!(r.code, 1);
    assert!(r
        .out
        .starts_with("### lab-profile@1.2.0 → lab-profile@1.3.0"));
    assert!(r
        .out
        .contains("| | layer | pack | version | change | impact |"));
    assert!(r.out.contains("| `~` | k8s | `kubernetes` | 1.29.4 → 1.30.2 | IDENTITY UPSTREAM AUTHORED | node repave (Kubernetes version change) |"));
    assert!(r.out.contains(
        "<details><summary><code>kubernetes</code> values: 3 changed, 1 added, 0 removed</summary>"
    ));
    assert!(r
        .out
        .contains("+ kubeadmconfig.apiServer.extraArgs.profiling: \"false\""));
    assert!(r.out.contains("max impact: `repave`"));
    assert!(r
        .out
        .contains("<details><summary>variables: 1 added, 0 removed, 1 changed</summary>"));
    assert!(r
        .out
        .contains("+ variable db_password (required, sensitive)\n"));
    assert!(r.out.contains(
        "- cluster_env.defaultValue: \"dev\"\n+ cluster_env.defaultValue: \"staging\"\n"
    ));
    assert!(!r.out.contains("db_password.name"));
    let full = run(&["compare", "--format", "markdown", "--full", &fx.a, &fx.b]);
    assert!(full
        .out
        .contains("```diff\n--- a: addon/prometheus-operator manifest extra-rules"));
}

#[test]
fn color_policy() {
    let fx = fixtures();
    let plain = run(&["compare", &fx.a, &fx.b]).out;
    let always = run(&["compare", "--color=always", &fx.a, &fx.b]).out;
    assert!(always.contains(ESC));
    let stripped = strip_ansi(&always);
    assert_eq!(
        stripped, plain,
        "colored output must equal plain output once escapes are stripped"
    );
    assert!(!run(&["compare", "--color=never", &fx.a, &fx.b])
        .out
        .contains(ESC));
    assert!(!run(&["compare", "--no-color", &fx.a, &fx.b])
        .out
        .contains(ESC));
    assert!(run_in(
        &["compare", &fx.a, &fx.b],
        &[("CLICOLOR_FORCE", "1")],
        &["NO_COLOR"]
    )
    .out
    .contains(ESC));
    assert!(!run_in(
        &["compare", &fx.a, &fx.b],
        &[("CLICOLOR_FORCE", "1"), ("NO_COLOR", "1")],
        &[]
    )
    .out
    .contains(ESC));
    assert!(!run(&["compare", "--color=always", "--json", &fx.a, &fx.b])
        .out
        .contains(ESC));
    let bad = run(&["compare", "--color=bogus", &fx.a, &fx.b]);
    assert_eq!(bad.code, 2);
    let full = run(&["compare", "--color=always", "--full", &fx.a, &fx.b]).out;
    assert!(strip_ansi(&full).contains("+      - --metric-resolution=15s"));
    assert!(strip_ansi(&full).contains("--- a: addon/prometheus-operator manifest extra-rules"));
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ESC {
            for n in chars.by_ref() {
                if n == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[test]
fn bad_inputs_exit_two() {
    let fx = fixtures();
    let not_snapshot = fx.raw.join("a/profile.json");
    let r = run(&["compare", not_snapshot.to_str().unwrap(), &fx.b]);
    assert_eq!(r.code, 2);
    assert!(r.err.contains("not a strata snapshot"));
    let home = tempfile::tempdir().unwrap();
    let r = run_in(
        &["verify", &fx.a],
        &[("HOME", home.path().to_str().unwrap())],
        &["PALETTE_API_KEY", "SPECTROCLOUD_APIKEY"],
    );
    assert_eq!(r.code, 2, "{}", r.err);
    assert!(r.err.contains("PALETTE_API_KEY is required"));
    let r = run(&["verify", not_snapshot.to_str().unwrap()]);
    assert_eq!(r.code, 2);
}

#[test]
fn verify_over_many_records_prints_one_line_each_and_a_summary_document() {
    let fx = fixtures();
    let home = tempfile::tempdir().unwrap();
    let no_creds = [("HOME", home.path().to_str().unwrap())];
    let clear = ["PALETTE_API_KEY", "SPECTROCLOUD_APIKEY"];
    let r = run_in(&["verify", &fx.a, &fx.b], &no_creds, &clear);
    assert_eq!(r.code, 2, "{}", r.err);
    let lines: Vec<&str> = r.out.lines().collect();
    assert_eq!(
        lines.len(),
        3,
        "one line per record and one summary line:\n{}",
        r.out
    );
    assert!(
        lines[0].starts_with("ERROR") && lines[0].contains(&fx.a),
        "{}",
        lines[0]
    );
    assert!(
        lines[1].starts_with("ERROR") && lines[1].contains("lab-profile@1.3.0"),
        "{}",
        lines[1]
    );
    assert!(
        lines[2].contains("2 records") && lines[2].contains("2 errors"),
        "{}",
        lines[2]
    );
    assert!(
        r.err.is_empty(),
        "per-record errors live in the report, not on stderr: {}",
        r.err
    );

    let r = run_in(
        &["verify", "--format", "json", &fx.a, &fx.b],
        &no_creds,
        &clear,
    );
    assert_eq!(r.code, 2);
    let d: serde_json::Value = serde_json::from_str(&r.out).unwrap();
    assert_eq!(d["schema"], "strata-verify-summary/1");
    assert_eq!(d["summary"]["records"], 2);
    assert_eq!(d["summary"]["errors"], 2);
    assert_eq!(d["summary"]["status"], "error");
    assert_eq!(d["records"][0]["status"], "error");
    assert_eq!(d["records"][0]["error"]["kind"], "missing-credential");
    assert_eq!(d["records"][1]["name"], "lab-profile");
    assert!(d["legend"]["status"]["drifted"].is_string());

    // strata expands a quoted pattern itself, so any shell or scheduler can pass one
    let dir = std::path::Path::new(&fx.a).parent().unwrap();
    let pattern = format!("{}/*.json", dir.display());
    let r = run_in(&["verify", "--json", &pattern], &no_creds, &clear);
    let d: serde_json::Value = serde_json::from_str(&r.out).unwrap();
    assert_eq!(d["summary"]["records"], 3, "a, a2 and b");
    // a pattern that matches nothing is an error, never a silent success
    let none = format!("{}/nothing-*.json", dir.display());
    let r = run_in(&["verify", &none], &no_creds, &clear);
    assert_eq!(r.code, 2);
    assert!(r.err.contains("matched no files"), "{}", r.err);

    // --summary gives one file the summary too, so a script reads one shape whatever it passed
    let r = run_in(&["verify", "--summary", "--json", &fx.a], &no_creds, &clear);
    assert_eq!(r.code, 2, "{}", r.err);
    let d: serde_json::Value = serde_json::from_str(&r.out).unwrap();
    assert_eq!(d["schema"], "strata-verify-summary/1");
    assert_eq!(d["summary"]["records"], 1);
    assert_eq!(d["records"][0]["path"], fx.a.as_str());
    assert_eq!(d["records"][0]["error"]["kind"], "missing-credential");
    let r = run_in(&["verify", "--summary", &fx.a], &no_creds, &clear);
    assert_eq!(
        r.out.lines().count(),
        2,
        "one record line and the counts:\n{}",
        r.out
    );
}

#[test]
fn same_release_checks_consecutive_records_with_an_optional_registry_move_policy() {
    let fx = fixtures();
    let dir = std::path::Path::new(&fx.a).parent().unwrap();
    let snap = |name: &str| -> String {
        let out = dir.join(format!("{name}.json"));
        let r = run(&[
            "snapshot",
            "--from-raw",
            fx.raw.join(name).to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ]);
        assert_eq!(r.code, 0, "{}", r.err);
        out.to_str().unwrap().to_string()
    };
    let (a3, a4) = (snap("a3"), snap("a4"));

    let r = run(&["same-release", &fx.a, &fx.a2, &fx.a]);
    assert_eq!(r.code, 0, "{}{}", r.out, r.err);
    assert!(
        r.out.lines().filter(|l| l.starts_with("IDENTICAL")).count() == 2,
        "{}",
        r.out
    );
    assert!(
        r.out.trim_end().ends_with("SAME RELEASE: 2 pairs passed"),
        "{}",
        r.out
    );

    let r = run(&["same-release", &fx.a, &fx.b]);
    assert_eq!(r.code, 1);
    let first = r.out.lines().next().unwrap();
    assert!(
        first.starts_with("DIFFERENT") && first.contains(&fx.a) && first.contains(&fx.b),
        "{first}"
    );
    assert!(
        r.out.contains("NOT THE SAME RELEASE: 1 of 1 pairs failed"),
        "{}",
        r.out
    );

    let r = run(&["same-release", &fx.a, &a3]);
    assert_eq!(
        r.code, 1,
        "a registry move is a difference under the exact policy"
    );
    let r = run(&["same-release", "--allow", "registry-move", &fx.a, &a3]);
    assert_eq!(r.code, 0, "{}{}", r.out, r.err);
    assert!(r.out.starts_with("REGISTRY MOVE"), "{}", r.out);
    assert!(r.out.contains("7 packs re-pinned"), "{}", r.out);

    let r = run(&["same-release", "--allow", "registry-move", &fx.a, &a4]);
    assert_eq!(r.code, 1);
    assert!(
        r.out.contains(
            "k8s/kubernetes AUTHORED: kubeadmconfig.clusterConfiguration.networking.podSubnet"
        ),
        "{}",
        r.out
    );

    let r = run(&[
        "same-release",
        "--json",
        "--allow",
        "registry-move",
        &fx.a,
        &a3,
        &a4,
    ]);
    assert_eq!(r.code, 1);
    let d: serde_json::Value = serde_json::from_str(&r.out).unwrap();
    assert_eq!(d["schema"], "strata-chain/1");
    assert_eq!(d["policy"], "registry-move");
    assert_eq!(d["same"], false);
    assert_eq!(d["pairs"][0]["status"], "registry-move");
    assert_eq!(d["pairs"][1]["status"], "different");
    assert_eq!(d["summary"]["failed"], 1);
    assert!(d["legend"]["status"]["registry-move"].is_string());
    assert_eq!(d["records"].as_array().unwrap().len(), 3);

    let r = run(&["same-release", &fx.a]);
    assert_eq!(r.code, 2, "two records at least");
}

#[test]
fn a_cluster_snapshot_from_raw_compares_against_profile_records() {
    let fx = fixtures();
    let dir = std::path::Path::new(&fx.a).parent().unwrap();
    let cluster = |raw: &str, reference: &str| {
        let out = dir.join(format!("{raw}.json"));
        let r = run(&[
            "snapshot",
            "--from-raw",
            fx.raw.join(raw).to_str().unwrap(),
            reference,
            "-o",
            out.to_str().unwrap(),
        ]);
        (r, out.to_str().unwrap().to_string())
    };
    let uid = common::CLUSTER_UID;
    let (r, on_b) = cluster("cluster-b", &format!("cluster:{uid}"));
    assert_eq!(r.code, 0, "{}", r.err);

    let r = run(&["compare", &fx.b, &on_b]);
    assert_eq!(r.code, 0, "{}{}", r.out, r.err);
    assert!(r.out.contains("IDENTICAL"), "{}", r.out);
    assert!(
        has_line(
            &r.out,
            &format!("^lab-profile@1.3.0 (uid 64f0000000000000000000b2)  ->  lab-profile@1.3.0 on cluster lab-cluster (uid {uid})")
        ),
        "{}",
        r.out
    );
    let r = run(&["compare", &fx.a, &on_b]);
    assert_eq!(r.code, 1);
    assert!(has_line(&r.out, "^~ [k8s  ] kubernetes"), "{}", r.out);

    let r = run(&["compare", "--format", "markdown", &fx.b, &on_b]);
    assert!(
        r.out
            .starts_with("### lab-profile@1.3.0 → lab-profile@1.3.0 on cluster lab-cluster"),
        "{}",
        r.out
    );

    let snap = json(&on_b);
    assert_eq!(snap["source"]["cluster"]["uid"], uid);
    assert_eq!(snap["source"]["cluster"]["name"], "lab-cluster");
    assert_eq!(snap["source"]["cluster"]["profiles"][0]["version"], "1.3.0");
    assert_eq!(snap["source"]["template"], "applied");

    // selecting one of several applied profiles, and naming one that is not there
    let (r, selected) = cluster("cluster-merged", &format!("cluster:{uid}/lab-profile"));
    assert_eq!(r.code, 0, "{}", r.err);
    assert_eq!(run(&["compare", &fx.b, &selected]).code, 0);
    let (r, _) = cluster("cluster-merged", &format!("cluster:{uid}/nope"));
    assert_eq!(r.code, 2);
    assert!(
        r.err.contains("no applied profile named 'nope'")
            && r.err.contains("applied: lab-profile, lab-addons"),
        "{}",
        r.err
    );
}

#[test]
fn list_needs_credentials_and_takes_only_known_kinds() {
    let home = tempfile::tempdir().unwrap();
    let r = run_in(
        &["list", "--json", "projects"],
        &[("HOME", home.path().to_str().unwrap())],
        &["PALETTE_API_KEY", "SPECTROCLOUD_APIKEY"],
    );
    assert_eq!(r.code, 2, "{}", r.err);
    let e: serde_json::Value = serde_json::from_str(r.err.trim()).unwrap();
    assert_eq!(e["error"]["kind"], "missing-credential");

    let r = run(&["list", "packs"]);
    assert_eq!(r.code, 2, "{}", r.err);
    assert!(
        r.err.contains("invalid value 'packs'") && r.err.contains("profiles, clusters, projects"),
        "{}",
        r.err
    );
}

#[test]
fn consumers_refuses_a_cluster_ref_with_a_structured_error() {
    let r = run(&["consumers", "--json", "cluster:c1"]);
    assert_eq!(r.code, 2, "{}", r.err);
    let e: serde_json::Value = serde_json::from_str(r.err.trim()).unwrap();
    assert_eq!(e["error"]["kind"], "profile-ref-required");
    assert_eq!(e["error"]["reference"], "cluster:c1");
}

#[test]
fn snapshot_from_candidate_reports_local_problems_without_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("profile.yaml");
    std::fs::write(
        &file,
        "name: aws-stack\nversion: 1.2.0\nregistry: Public Repo\nlayers:\n  - name: kubernetes\n    version: 1.36.2\n    valuesFile: values/k8s.yaml\n",
    )
    .unwrap();
    let home = tempfile::tempdir().unwrap();
    let r = run_in(
        &["snapshot", "--from-candidate", file.to_str().unwrap()],
        &[("HOME", home.path().to_str().unwrap())],
        &["PALETTE_API_KEY", "SPECTROCLOUD_APIKEY"],
    );
    assert_eq!(r.code, 2);
    assert!(
        r.err.contains("values/k8s.yaml"),
        "names the missing file, not the credential: {}",
        r.err
    );

    let r = run(&[
        "snapshot",
        "--from-candidate",
        file.to_str().unwrap(),
        "--from-raw",
        "x",
    ]);
    assert_eq!(
        r.code, 2,
        "a candidate and raw responses are two different sources"
    );
}

#[test]
fn schema_subcommand_prints_json_schema() {
    for kind in [
        "snapshot",
        "diff",
        "verify-summary",
        "chain",
        "consumers",
        "list",
        "candidate",
    ] {
        let r = run(&["schema", kind]);
        assert_eq!(r.code, 0, "{}", r.err);
        let v: serde_json::Value = serde_json::from_str(&r.out).unwrap();
        assert!(v["$schema"].is_string());
        if kind == "candidate" {
            let layer = &v["$defs"]["CandidateLayer"]["properties"];
            assert!(
                layer["valuesFile"].is_object() && layer["tag"].is_object(),
                "{v}"
            );
            continue;
        }
        assert!(v["properties"]["schema"].is_object());
        if kind == "snapshot" {
            let cluster = &v["$defs"]["ClusterSource"]["properties"];
            assert!(cluster["uid"].is_object() && cluster["profiles"].is_object());
            let value = &v["$defs"]["ClusterVariable"]["properties"];
            assert!(
                value["value"].is_object() && value["sensitive"].is_object(),
                "{v}"
            );
        }
        if kind == "diff" {
            let vars = &v["$defs"]["VariablesDiff"]["properties"];
            assert!(vars["diff"].is_object() && vars["byName"].is_object());
            assert!(v["properties"]["clusterVariables"].is_object(), "{v}");
            let by_name = &v["$defs"]["VariablesByName"]["properties"];
            assert!(by_name["added"].is_object() && by_name["changed"].is_object());
            assert!(v["$defs"]["VariableSummary"]["properties"]["sensitive"].is_object());
        }
    }
}

#[test]
fn json_mode_writes_one_structured_error_line_on_stderr() {
    let fx = fixtures();
    let not_snapshot = fx.raw.join("a/profile.json");
    let r = run(&["compare", "--json", not_snapshot.to_str().unwrap(), &fx.b]);
    assert_eq!(r.code, 2, "exit code is unchanged");
    assert_eq!(r.err.lines().count(), 1, "stderr: {}", r.err);
    let e: serde_json::Value = serde_json::from_str(r.err.trim()).unwrap();
    assert_eq!(e["error"]["kind"], "not-snapshot");
    assert!(e["error"]["message"]
        .as_str()
        .unwrap()
        .contains("not a strata snapshot"));
    assert!(e["error"]["path"]
        .as_str()
        .unwrap()
        .ends_with("profile.json"));

    // the issue's example: a ref that is not a file is a uid, which needs credentials
    let home = tempfile::tempdir().unwrap();
    let r = run_in(
        &["compare", "--json", &fx.a, "missing.json"],
        &[("HOME", home.path().to_str().unwrap())],
        &["PALETTE_API_KEY", "SPECTROCLOUD_APIKEY"],
    );
    assert_eq!(r.code, 2, "{}", r.err);
    let e: serde_json::Value = serde_json::from_str(r.err.trim()).unwrap();
    assert_eq!(e["error"]["kind"], "missing-credential");
    assert_eq!(e["error"]["variable"], "PALETTE_API_KEY");
    assert!(e["error"]["message"]
        .as_str()
        .unwrap()
        .contains("PALETTE_API_KEY is required"));

    // --format json works the same as --json, on verify too
    let r = run_in(
        &["verify", "--format", "json", &fx.a],
        &[("HOME", home.path().to_str().unwrap())],
        &["PALETTE_API_KEY", "SPECTROCLOUD_APIKEY"],
    );
    assert_eq!(r.code, 2);
    let e: serde_json::Value = serde_json::from_str(r.err.trim()).unwrap();
    assert_eq!(e["error"]["kind"], "missing-credential");

    // without JSON selected the sentence stays
    let r = run(&["compare", not_snapshot.to_str().unwrap(), &fx.b]);
    assert!(r.err.starts_with("strata: "), "{}", r.err);
    assert!(!r.err.contains("\"kind\""));
}

/// Run against an address that refuses connections, with no retries and a clean environment, so
/// the path in the transport error is the first request strata made.
fn first_request(args: &[&str], envs: &[(&str, &str)]) -> (i32, serde_json::Value) {
    let home = tempfile::tempdir().unwrap();
    let mut all = vec![
        ("HOME", home.path().to_str().unwrap()),
        ("PALETTE_API_KEY", "k"),
        ("PALETTE_API", "http://127.0.0.1:9"),
        ("STRATA_RETRIES", "0"),
    ];
    all.extend_from_slice(envs);
    let r = run_in(
        args,
        &all,
        &[
            "PALETTE_PROJECT_UID",
            "PALETTE_PROJECT",
            "SPECTROCLOUD_APIKEY",
            "SPECTROCLOUD_HOST",
        ],
    );
    let e: serde_json::Value = serde_json::from_str(r.err.trim())
        .unwrap_or_else(|_| serde_json::json!({"error": {"raw": r.err}}));
    (r.code, e["error"].clone())
}

#[test]
fn a_project_name_is_looked_up_in_the_project_listing_before_anything_else() {
    let fx = fixtures();
    let projects = "/v1/dashboard/projects/metadata";

    let (code, e) = first_request(
        &["list", "profiles", "--json"],
        &[("PALETTE_PROJECT", "lab")],
    );
    assert_eq!((code, e["kind"].as_str()), (2, Some("transport")), "{e}");
    assert_eq!(e["path"], projects, "PALETTE_PROJECT is resolved first");

    let (_, e) = first_request(
        &["list", "profiles", "--json"],
        &[("PALETTE_PROJECT", "lab"), ("PALETTE_PROJECT_UID", "p1")],
    );
    assert_eq!(
        e["path"], "/v1/dashboard/clusterprofiles/metadata",
        "a uid wins over a name: {e}"
    );

    for args in [
        vec!["list", "profiles", "--json", "--project-name", "lab"],
        vec!["verify", "--json", fx.a.as_str(), "--project-name", "lab"],
    ] {
        let (code, e) = first_request(&args, &[]);
        assert_eq!(code, 2);
        assert_eq!(e["path"], projects, "{args:?}: {e}");
    }

    let (code, e) = first_request(
        &[
            "verify",
            fx.a.as_str(),
            "--project",
            "p1",
            "--project-name",
            "lab",
        ],
        &[],
    );
    assert_eq!(code, 2);
    assert!(
        e["raw"]
            .as_str()
            .unwrap_or("")
            .contains("cannot be used with"),
        "{e}"
    );
}

#[test]
fn candidate_writes_a_record_as_a_candidate_file_with_values_and_manifests_in_files() {
    let fx = fixtures();
    let out = tempfile::tempdir().unwrap();
    let file = out.path().join("profile.yaml");
    let files = out.path().join("files");
    let r = run(&[
        "candidate",
        &fx.b,
        "-o",
        file.to_str().unwrap(),
        "--files-dir",
        files.to_str().unwrap(),
    ]);
    assert_eq!(r.code, 0, "{}", r.err);
    let doc: serde_json::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(doc["name"], "lab-profile");
    let recorded = json(&fx.b);
    let k8s = doc["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["name"] == "kubernetes")
        .unwrap();
    let values =
        std::fs::read_to_string(out.path().join(k8s["valuesFile"].as_str().unwrap())).unwrap();
    let recorded_k8s = recorded["packs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "kubernetes")
        .unwrap();
    assert_eq!(values, recorded_k8s["values"].as_str().unwrap());

    let r = run(&["candidate", &fx.b]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(r.out.starts_with("name: lab-profile\n"), "{}", r.out);

    let r = run(&["candidate", &fx.b, "--files-dir", files.to_str().unwrap()]);
    assert_eq!(r.code, 2, "--files-dir needs -o");
}
