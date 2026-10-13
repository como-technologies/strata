//! End-to-end tests over raw-API-shaped fixtures (no network).

use crate::common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::{
    PROFILE_LISTING, PROJECT_LISTING, Run, against_closed_port, read_json, snapshot_from_raw,
    strata_bin, write_record,
};

const ESC: char = '\x1b';

fn run_in(args: &[&str], envs: &[(&str, &str)], clear: &[&str]) -> Run {
    let mut c = Command::new(strata_bin());
    c.args(args).env("STRATA_QUIET", "1");
    for k in clear {
        c.env_remove(k);
    }
    for (k, v) in envs {
        c.env(k, v);
    }
    Run::of(&mut c)
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
    let snap = |name: &str| record_of(&raw, dir.path(), name);
    let (a, a2, b) = (snap("a"), snap("a2"), snap("b"));
    Fx {
        _dir: dir,
        raw,
        a,
        a2,
        b,
    }
}

/// The path of the record of the raw profile fixture `name` under `raw`, written to
/// `dir/<name>.json`.
fn record_of(raw: &Path, dir: &Path, name: &str) -> String {
    let out = dir.join(format!("{name}.json"));
    snapshot_from_raw(raw, name, &out);
    out.to_str().unwrap().to_string()
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
    let a = read_json(Path::new(&fx.a));
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
    assert_eq!(
        read_json(Path::new(&fx.a))["root"],
        read_json(Path::new(&fx.a2))["root"]
    );
}

#[test]
fn an_output_path_makes_the_directory_it_names() {
    let fx = fixtures();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("runs/2026/payments.json");
    let r = run(&[
        "snapshot",
        "--from-raw",
        fx.raw.join("b").to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(r.code, 0, "{}{}", r.out, r.err);
    assert!(
        out.is_file(),
        "a record goes where it is asked for, with no mkdir first"
    );
    assert!(r.err.contains("wrote") || r.err.is_empty(), "{}", r.err);
}

#[test]
fn a_command_name_that_is_not_first_says_so_instead_of_failing_as_a_compare() {
    let fx = fixtures();
    let r = run(&["strata", "consumers", "aws-stack@1.2.0"]);
    assert_eq!(r.code, 2, "{}{}", r.out, r.err);
    assert!(
        r.err
            .contains("'consumers' is a command, so it comes first: strata consumers ..."),
        "{}",
        r.err
    );
    assert!(
        !r.err.contains("unexpected argument"),
        "the old message named compare, which the person did not type: {}",
        r.err
    );

    // a compare of two refs keeps working, and a ref is matched as a whole word only
    let r = run(&[&fx.a, &fx.a2]);
    assert_eq!(r.code, 0, "{}{}", r.out, r.err);
    let r = run(&["compare", &fx.a, &fx.a2]);
    assert_eq!(r.code, 0, "{}{}", r.out, r.err);
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
fn two_records_whose_os_pack_names_two_images_exit_one_with_identical_roots() {
    let fx = fixtures();
    let with_image = |image: &str| -> String {
        let mut doc = read_json(Path::new(&fx.a));
        for pack in doc["packs"].as_array_mut().unwrap() {
            if pack["layer"] == "os" {
                pack["annotations"] = serde_json::json!({"imageId": image});
            }
        }
        let path = format!("{}.{image}.json", fx.a);
        write_record(Path::new(&path), &doc);
        path
    };
    let (x, y) = (with_image("ami-0aaa"), with_image("ami-0bbb"));
    let r = run(&["compare", &x, &y]);
    assert_eq!(r.code, 1, "{}{}", r.out, r.err);
    assert!(has_line(&r.out, "IDENTICAL"), "{}", r.out);
    assert!(
        has_line(
            &r.out,
            "names image ami-0aaa on one side and ami-0bbb on the other"
        ),
        "{}",
        r.out
    );
    let r = run(&["compare", &x, &x]);
    assert_eq!(r.code, 0, "{}{}", r.out, r.err);
}

#[test]
fn changed_profiles_report_every_kind_of_change() {
    let fx = fixtures();
    let r = run(&[&fx.a, &fx.b]); // default subcommand
    assert_eq!(r.code, 1, "{}", r.err);
    let t = &r.out;
    assert!(has_line(
        t,
        "^lab-profile@1.2.0 (uid 64f0000000000000000000a1)  ->  lab-profile@1.3.0 (uid 64f0000000000000000000b2)"
    ));
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
    assert!(t.contains("upstream digest: sha256:cni-aaaa -> sha256:cni-bbbb (packRef.digest)"));
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
    assert!(
        r.out
            .starts_with("### lab-profile@1.2.0 → lab-profile@1.3.0")
    );
    assert!(
        r.out
            .contains("| | layer | pack | version | change | impact |")
    );
    assert!(r.out.contains("| `~` | k8s | `kubernetes` | 1.29.4 → 1.30.2 | IDENTITY UPSTREAM AUTHORED | node repave (Kubernetes version change) |"));
    assert!(r.out.contains(
        "<details><summary><code>kubernetes</code> values: 3 changed, 1 added, 0 removed</summary>"
    ));
    assert!(
        r.out
            .contains("+ kubeadmconfig.apiServer.extraArgs.profiling: \"false\"")
    );
    assert!(r.out.contains("max impact: `repave`"));
    assert!(
        r.out
            .contains("<details><summary>variables: 1 added, 0 removed, 1 changed</summary>")
    );
    assert!(
        r.out
            .contains("+ variable db_password (required, sensitive)\n")
    );
    assert!(r.out.contains(
        "- cluster_env.defaultValue: \"dev\"\n+ cluster_env.defaultValue: \"staging\"\n"
    ));
    assert!(!r.out.contains("db_password.name"));
    let full = run(&["compare", "--format", "markdown", "--full", &fx.a, &fx.b]);
    assert!(
        full.out
            .contains("```diff\n--- a: addon/prometheus-operator manifest extra-rules")
    );
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
    assert!(
        !run(&["compare", "--color=never", &fx.a, &fx.b])
            .out
            .contains(ESC)
    );
    assert!(
        !run(&["compare", "--no-color", &fx.a, &fx.b])
            .out
            .contains(ESC)
    );
    assert!(
        run_in(
            &["compare", &fx.a, &fx.b],
            &[("CLICOLOR_FORCE", "1")],
            &["NO_COLOR"]
        )
        .out
        .contains(ESC)
    );
    assert!(
        !run_in(
            &["compare", &fx.a, &fx.b],
            &[("CLICOLOR_FORCE", "1"), ("NO_COLOR", "1")],
            &[]
        )
        .out
        .contains(ESC)
    );
    assert!(
        !run(&["compare", "--color=always", "--json", &fx.a, &fx.b])
            .out
            .contains(ESC)
    );
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
    let snap = |name: &str| record_of(&fx.raw, dir, name);
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
        r.out.contains("NOT THE SAME RELEASE: 1 of 1 pair failed"),
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
fn same_release_names_a_macro_with_another_value_on_each_side_and_keeps_the_status() {
    let fx = fixtures();
    // A record whose content reads the macro region, as a promotion copies it to each project.
    // `digest` is the digest of the value that the project gives, or none for a record without
    // --macros.
    let record = |name: &str, digest: Option<&str>| -> String {
        let mut doc = read_json(Path::new(&fx.a));
        let pack = &mut doc["packs"][0];
        let values = pack["values"].as_str().unwrap_or_default();
        pack["values"] = format!("{values}\nregion: \"{{{{.spectro.macro.region}}}}\"\n").into();
        if let Some(d) = digest {
            doc["source"]["macroValues"] = serde_json::json!({ "region": d });
        }
        let path = format!("{}.{name}.json", fx.a);
        write_record(Path::new(&path), &doc);
        path
    };
    let dev = record("dev", Some("sha256:1111"));
    let prod = record("prod", Some("sha256:2222"));
    let dev_again = record("dev-again", Some("sha256:1111"));
    let (dev_plain, prod_plain) = (record("dev-plain", None), record("prod-plain", None));
    let other_value = "macro region has another value on each side";
    let not_compared = "the content uses macro region: a record holds no digest of its value";

    // two values: the line under the pair, and the status and the exit stay
    let r = run(&["same-release", &dev, &prod]);
    assert_eq!(r.code, 0, "{}{}", r.out, r.err);
    assert!(r.out.starts_with("IDENTICAL"), "{}", r.out);
    assert!(has_line(&r.out, other_value), "{}", r.out);
    assert!(!r.out.contains(not_compared), "{}", r.out);
    let r = run(&["same-release", &dev, &prod, "--json"]);
    let d: serde_json::Value = serde_json::from_str(&r.out).unwrap();
    assert_eq!(d["pairs"][0]["status"], "identical");
    assert_eq!(
        d["pairs"][0]["macrosChanged"],
        serde_json::json!(["region"])
    );
    assert!(d["pairs"][0].get("macrosNotCompared").is_none(), "{d}");
    let r = run(&["same-release", &dev, &prod, "--format", "markdown"]);
    assert!(r.out.contains(other_value), "{}", r.out);

    // one value: no line, and no field
    let r = run(&["same-release", &dev, &dev_again, "--json"]);
    let d: serde_json::Value = serde_json::from_str(&r.out).unwrap();
    assert!(d["pairs"][0].get("macrosChanged").is_none(), "{d}");
    assert!(d["pairs"][0].get("macrosNotCompared").is_none(), "{d}");

    // no digests on one side or on both: the report names the macro that it cannot compare
    for (a, b) in [(&dev_plain, &prod_plain), (&dev, &prod_plain)] {
        let r = run(&["same-release", a, b]);
        assert_eq!(r.code, 0, "{}{}", r.out, r.err);
        assert!(has_line(&r.out, not_compared), "{}", r.out);
        assert!(!r.out.contains(other_value), "{}", r.out);
        let r = run(&["same-release", a, b, "--json"]);
        let d: serde_json::Value = serde_json::from_str(&r.out).unwrap();
        assert_eq!(
            d["pairs"][0]["macrosNotCompared"],
            serde_json::json!(["region"])
        );
        assert!(d["pairs"][0].get("macrosChanged").is_none(), "{d}");
        let r = run(&["same-release", a, b, "--format", "markdown"]);
        assert!(r.out.contains(not_compared), "{}", r.out);
    }
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
            &format!(
                "^lab-profile@1.3.0 (uid 64f0000000000000000000b2)  ->  lab-profile@1.3.0 on cluster lab-cluster (uid {uid})"
            )
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

    let snap = read_json(Path::new(&on_b));
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

/// The values of a closed set of a record, as `strata schema` names them under `$defs`.
///
/// The schema gives a `oneOf`
/// of one constant for each value, in the order of the enum.
fn schema_values(schema: &serde_json::Value, def: &str) -> Vec<String> {
    schema["$defs"][def]["oneOf"]
        .as_array()
        .unwrap_or_else(|| panic!("{def} has no oneOf: {}", schema["$defs"][def]))
        .iter()
        .map(|c| c["const"].as_str().unwrap().to_string())
        .collect()
}

/// The definition of the closed set that `field` names, for a field of a document that strata
/// reads again. Such a field takes each value of the set, or any other text of a later release.
fn open_set_ref(field: &serde_json::Value) -> &serde_json::Value {
    assert_eq!(field["anyOf"][1]["type"], "string", "{field}");
    &field["anyOf"][0]["$ref"]
}

#[test]
fn the_schema_names_each_value_of_a_closed_set_of_a_record() {
    let r = run(&["schema", "snapshot"]);
    assert_eq!(r.code, 0, "{}", r.err);
    let v: serde_json::Value = serde_json::from_str(&r.out).unwrap();
    assert_eq!(
        schema_values(&v, "Template"),
        ["published", "draft", "applied", "candidate", "registry"]
    );
    assert_eq!(
        schema_values(&v, "UpstreamSource"),
        ["packRef.digest", "packSummary.digest", "none"]
    );
    let defs = &v["$defs"];
    assert_eq!(
        open_set_ref(&defs["Source"]["properties"]["template"]),
        "#/$defs/Template"
    );
    assert_eq!(
        open_set_ref(&defs["PackHash"]["properties"]["upstreamSource"]),
        "#/$defs/UpstreamSource"
    );
    assert_eq!(
        schema_values(&v, "SubjectKind"),
        ["cluster", "edgeHost", "clusterGroup"]
    );
    assert_eq!(
        open_set_ref(&defs["ClusterSource"]["properties"]["kind"]),
        "#/$defs/SubjectKind"
    );
    // the tiers of a cluster record stay a map from the name of the tier to the tier, and a name
    // can be any text
    let tiers = &defs["ClusterSource"]["properties"]["tiers"];
    assert_eq!(tiers["type"], "object");
    assert_eq!(tiers["additionalProperties"]["$ref"], "#/$defs/ClusterTier");

    let r = run(&["schema", "diff"]);
    assert_eq!(r.code, 0, "{}", r.err);
    let v: serde_json::Value = serde_json::from_str(&r.out).unwrap();
    assert_eq!(
        schema_values(&v, "Tier"),
        ["applied", "labels", "policy", "running", "shape"]
    );
    assert_eq!(
        open_set_ref(&v["$defs"]["TierDiff"]["properties"]["tier"]),
        "#/$defs/Tier"
    );
    assert_eq!(
        open_set_ref(&v["$defs"]["PackSide"]["properties"]["upstreamSource"]),
        "#/$defs/UpstreamSource"
    );
}

#[test]
fn the_schema_names_each_value_of_each_closed_set_of_the_other_documents() {
    let schema = |kind: &str| -> serde_json::Value {
        let r = run(&["schema", kind]);
        assert_eq!(r.code, 0, "{}", r.err);
        serde_json::from_str(&r.out).unwrap()
    };
    let v = schema("snapshot");
    assert_eq!(schema_values(&v, "Implementation"), ["rust", "bash"]);
    assert_eq!(
        open_set_ref(&v["$defs"]["Tool"]["properties"]["impl"]["anyOf"][0]),
        "#/$defs/Implementation"
    );
    let v = schema("diff");
    assert_eq!(schema_values(&v, "Side"), ["a", "b"]);
    assert_eq!(
        v["$defs"]["RunningBehind"]["properties"]["side"]["$ref"],
        "#/$defs/Side"
    );
    let v = schema("candidate");
    assert_eq!(schema_values(&v, "LayerKind"), ["manifest"]);
    assert_eq!(
        v["$defs"]["CandidateLayer"]["properties"]["type"]["anyOf"][0]["$ref"],
        "#/$defs/LayerKind"
    );
    let v = schema("list");
    assert_eq!(schema_values(&v, "Conjunction"), ["and", "or"]);
    assert_eq!(
        v["$defs"]["SavedFilter"]["properties"]["conjunction"]["$ref"],
        "#/$defs/Conjunction"
    );
    let v = schema("behind");
    assert_eq!(
        schema_values(&v, "ChangeFlag"),
        ["IDENTITY", "UPSTREAM", "AUTHORED", "IMAGES", "CHARTS"]
    );
    assert_eq!(
        open_set_ref(&v["$defs"]["Bump"]["properties"]["flags"]["items"]),
        "#/$defs/ChangeFlag"
    );
    assert_eq!(
        open_set_ref(&v["$defs"]["Bump"]["properties"]["impact"]),
        "#/$defs/ImpactLevel"
    );
    let v = schema("consumers");
    assert_eq!(
        schema_values(&v, "ProfileType"),
        ["cluster", "infra", "add-on", "system", "app", "pack"]
    );
    assert_eq!(
        v["$defs"]["ConsumedProfile"]["properties"]["type"]["anyOf"][0]["$ref"],
        "#/$defs/ProfileType"
    );
}

#[test]
fn schema_subcommand_prints_json_schema() {
    for (kind, id) in [
        ("snapshot", "strata-snapshot/1"),
        ("diff", "strata-diff/1"),
        ("verify-summary", "strata-verify-summary/1"),
        ("chain", "strata-chain/1"),
        ("consumers", "strata-consumers/1"),
        ("list", "strata-list/1"),
        ("packs", "strata-packs/1"),
        ("set", "strata-set/1"),
        ("candidate", ""),
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
        // The id is a `const` in the JSON Schema, so a validator rejects a document of another
        // version, and the id it holds is the one the writer puts in the document.
        assert_eq!(v["properties"]["schema"]["const"], id, "{kind}");
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
fn a_record_that_is_not_there_names_the_file_and_asks_palette_nothing() {
    let fx = fixtures();
    let missing = fx._dir.path().join("releases/dev.json");
    // Palette is a port that refuses connections, so a request is a transport error that names
    // its route. The temporary path can hold any number, so the test looks for no status number.
    let r = closed_port(&["compare", missing.to_str().unwrap(), &fx.b], &[]);
    assert_eq!(r.code, 2, "stderr: {}", r.err);
    assert!(
        !r.err.contains("GET /v1/") && !r.err.contains("failed with HTTP"),
        "nothing was asked of Palette: {}",
        r.err
    );
    assert!(
        r.err.contains("releases/dev.json") && r.err.contains("No such file"),
        "and the error names the file that is not there: {}",
        r.err
    );

    // the same in JSON mode, as an io error with the path, not a transport error
    let (code, e) = first_request(
        &["compare", "--json", missing.to_str().unwrap(), &fx.b],
        &[],
    );
    assert_eq!((code, e["kind"].as_str()), (2, Some("io")), "{e}");
    assert!(e["path"].as_str().unwrap().ends_with("dev.json"));
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
    assert!(
        e["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not a strata snapshot")
    );
    assert!(
        e["error"]["path"]
            .as_str()
            .unwrap()
            .ends_with("profile.json")
    );

    // a ref that is not a file is a uid or a name, which needs credentials. A ref shaped like a
    // path is a file, and a missing one is the io error above, before any credential is read.
    let home = tempfile::tempdir().unwrap();
    let r = run_in(
        &["compare", "--json", &fx.a, "aws-stack@1.1.1"],
        &[("HOME", home.path().to_str().unwrap())],
        &["PALETTE_API_KEY", "SPECTROCLOUD_APIKEY"],
    );
    assert_eq!(r.code, 2, "{}", r.err);
    let e: serde_json::Value = serde_json::from_str(r.err.trim()).unwrap();
    assert_eq!(e["error"]["kind"], "missing-credential");
    assert_eq!(e["error"]["variable"], "PALETTE_API_KEY");
    assert!(
        e["error"]["message"]
            .as_str()
            .unwrap()
            .contains("PALETTE_API_KEY is required")
    );

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

/// [`against_closed_port`] with `STRATA_QUIET` set, as [`run`] sets it.
fn closed_port(args: &[&str], envs: &[(&str, &str)]) -> Run {
    against_closed_port(Command::new(strata_bin()), args, envs, true)
}

/// [`closed_port`] in JSON mode: the exit status and the error object.
fn first_request(args: &[&str], envs: &[(&str, &str)]) -> (i32, serde_json::Value) {
    let r = closed_port(args, envs);
    let e: serde_json::Value = serde_json::from_str(r.err.trim())
        .unwrap_or_else(|_| serde_json::json!({"error": {"raw": r.err}}));
    (r.code, e["error"].clone())
}

#[test]
fn a_project_name_is_looked_up_in_the_project_listing_before_anything_else() {
    let fx = fixtures();
    let projects = PROJECT_LISTING;

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
    assert_eq!(e["path"], PROFILE_LISTING, "a uid wins over a name: {e}");

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
    let recorded = read_json(Path::new(&fx.b));
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

#[test]
fn candidate_names_the_whole_root_of_a_record_whose_root_has_fewer_than_twelve_characters() {
    let fx = fixtures();
    let out = tempfile::tempdir().unwrap();
    let mut doc = read_json(Path::new(&fx.b));
    doc["root"] = "abc".into();
    let record = out.path().join("short-root.json");
    write_record(&record, &doc);
    let file = out.path().join("profile.yaml");
    let r = Run::of(
        Command::new(strata_bin())
            .args(["candidate", record.to_str().unwrap(), "-o"])
            .arg(&file)
            .env_remove("STRATA_QUIET"),
    );
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(r.err.contains("(root abc)"), "{}", r.err);
    assert!(file.exists());
}

/// [`closed_port`] without `STRATA_QUIET`, so the warnings a person sees are on.
fn first_request_loud(args: &[&str], envs: &[(&str, &str)]) -> Run {
    against_closed_port(Command::new(strata_bin()), args, envs, false)
}

/// A uid and a name both reach strata and the uid decides, so the name decided nothing. Strata
/// resolves the name to find out whether the two name one project, and that listing is the first
/// request it makes. Before this check a person read an answer about the uid's project while
/// they had asked for the name's, and nothing said so.
#[test]
fn a_scope_conflict_is_looked_up_before_the_work() {
    let r = first_request_loud(
        &["consumers", "aws-stack@1.0.0"],
        &[("PALETTE_PROJECT_UID", "p1"), ("PALETTE_PROJECT", "lab")],
    );
    assert!(
        r.err.contains("looking up project lab"),
        "the name is resolved to compare it with the uid: {}",
        r.err
    );
}

/// The name that a uid overrode is read from the place the uid came from, and no lower. A name
/// in `~/.palette.env` under a uid in the environment is the fallback doing its job, so strata
/// makes no listing and says nothing.
#[test]
fn a_name_in_the_file_under_a_uid_in_the_environment_starts_no_lookup() {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join(".palette.env"), "PALETTE_PROJECT=lab\n").unwrap();
    let r = first_request_loud(
        &["consumers", "aws-stack@1.0.0"],
        &[
            ("HOME", home.path().to_str().unwrap()),
            ("PALETTE_PROJECT_UID", "p1"),
        ],
    );
    assert!(
        !r.err.contains("looking up project") && !r.err.contains("warning:"),
        "the fallback is not a conflict: {}",
        r.err
    );
}

/// A quiet run writes no warning, so it also skips the listing that would find one. With `--json`
/// a quiet run writes one JSON object to standard error and nothing else, which a caller reads.
#[test]
fn a_quiet_run_skips_the_scope_conflict_lookup() {
    let (_, e) = first_request(
        &["list", "profiles", "--json"],
        &[("PALETTE_PROJECT", "lab"), ("PALETTE_PROJECT_UID", "p1")],
    );
    assert_eq!(
        e["path"], PROFILE_LISTING,
        "no project listing on a quiet run: {e}"
    );
}

#[test]
fn snapshot_validate_of_a_draft_is_a_usage_error_that_names_the_draft_and_the_candidate() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path().to_str().unwrap();
    // no key: a usage error comes before strata needs one
    let r = run_in(
        &["snapshot", "aws-stack@1.1.1@draft", "--validate"],
        &[("HOME", home)],
        &["PALETTE_API_KEY", "SPECTROCLOUD_APIKEY"],
    );
    assert_eq!(r.code, 2, "{}", r.err);
    assert!(r.err.starts_with("strata: --validate "), "{}", r.err);
    assert!(r.err.contains("(aws-stack@1.1.1@draft)"), "{}", r.err);
    assert!(
        r.err.contains("`strata candidate aws-stack@1.1.1@draft`"),
        "{}",
        r.err
    );
}

#[test]
fn snapshot_of_a_set_with_json_prints_an_error_of_the_query_as_one_json_line() {
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join("fleet");
    let dir = dir.to_str().unwrap();
    for args in [
        &[
            "snapshot",
            "filter:production",
            "--dir",
            dir,
            "--json",
            "-q",
        ][..],
        &[
            "snapshot",
            "tag:env=prod",
            "--dir",
            dir,
            "--format",
            "json",
            "-q",
        ][..],
    ] {
        let r = run_in(
            args,
            &[("HOME", home.path().to_str().unwrap())],
            &["PALETTE_API_KEY", "SPECTROCLOUD_APIKEY"],
        );
        assert_eq!(r.code, 2, "{args:?}: {}", r.err);
        assert_eq!(r.out, "", "{args:?}: no document when the query fails");
        let e: serde_json::Value = serde_json::from_str(r.err.trim()).unwrap();
        assert_eq!(e["error"]["kind"], "missing-credential", "{args:?}");
    }

    // the format is the report of a set, so it needs --dir: a record is always JSON
    let r = run(&["snapshot", "aws-stack@1.1.1", "--json"]);
    assert_eq!(r.code, 2);
    assert!(r.err.contains("--dir"), "{}", r.err);
}

#[test]
fn snapshot_refuses_the_format_flags_with_each_argument_that_refuses_dir() {
    let dir = tempfile::tempdir().unwrap();
    let path = |name: &str| dir.path().join(name).to_str().unwrap().to_string();
    let (candidate, raw, out) = (path("nothere.yaml"), path("raw"), path("out.json"));
    for format in [&["--json"][..], &["--format", "json"][..]] {
        for args in [
            vec!["--from-candidate", candidate.as_str()],
            vec!["--from-raw", raw.as_str()],
            vec!["aws-stack@1.1.1", "-o", out.as_str()],
            vec!["aws-stack@1.1.1", "--raw-dir", raw.as_str()],
        ] {
            let args = [&["snapshot"][..], &args, format].concat();
            let r = run(&args);
            assert_eq!(r.code, 2, "{args:?}: {}", r.err);
            assert!(r.err.contains("cannot be used with"), "{args:?}: {}", r.err);
            assert_eq!(r.out, "", "{args:?}");
        }
    }
}

#[test]
fn snapshot_macros_names_the_refs_it_takes_and_refuses_another_before_any_request() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path().to_str().unwrap();
    for reference in [
        "edgehost:store-1",
        "appprofile:shop@1.0.0",
        "pack:kubernetes@1.36.2",
        "aws-stack@1.1.1@draft",
    ] {
        // no key: a usage error comes before strata needs one
        let r = run_in(
            &["snapshot", reference, "--macros"],
            &[("HOME", home)],
            &["PALETTE_API_KEY", "SPECTROCLOUD_APIKEY"],
        );
        assert_eq!(r.code, 2, "{reference}: {}", r.err);
        assert_eq!(
            r.err.trim_end(),
            "strata: --macros takes <name>@<version>, a profile uid, cluster:<cluster>, or \
             clustergroup:<group>",
            "{reference}"
        );
    }
    let help = run(&["snapshot", "--help"]);
    assert!(
        help.out.contains(
            "It takes <name>@<version>, a profile uid, cluster:<cluster>, or clustergroup:<group>"
        ),
        "{}",
        help.out
    );
}

#[test]
fn snapshot_of_a_set_with_no_dir_says_what_records_each_cluster_before_any_request() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path().to_str().unwrap();
    for reference in ["tag:env=dev", "filter:prod"] {
        // no key: a usage error comes before strata needs one
        let r = run_in(
            &["snapshot", reference],
            &[("HOME", home)],
            &["PALETTE_API_KEY", "SPECTROCLOUD_APIKEY"],
        );
        assert_eq!(r.code, 2, "{reference}: {}", r.err);
        assert_eq!(
            r.err.trim_end(),
            format!(
                "strata: {reference} names a set of clusters, and this command reads one: \
                 `strata snapshot {reference} --dir DIR` records each cluster, and \
                 `strata list clusters` lists them"
            ),
            "{reference}"
        );
    }
}

#[test]
fn snapshot_validate_with_no_profile_to_ask_about_is_a_usage_error_before_any_request() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path().to_str().unwrap();
    for args in [
        &["snapshot", "--from-raw", "raw/aws-stack", "--validate"][..],
        &["snapshot", "cluster:c1", "--validate"][..],
        &["snapshot", "pack:kubernetes@1.36.2", "--validate"][..],
        &["snapshot", "tag:env=dev", "--dir", "records", "--validate"][..],
    ] {
        // no key: a usage error comes before strata needs one
        let r = run_in(
            args,
            &[("HOME", home)],
            &["PALETTE_API_KEY", "SPECTROCLOUD_APIKEY"],
        );
        assert_eq!(r.code, 2, "{args:?}: {}", r.err);
        assert!(
            r.err.starts_with("strata: --validate "),
            "{args:?}: {}",
            r.err
        );
    }
}

#[test]
fn snapshot_of_a_version_whose_pack_palette_marks_invalid_warns_on_standard_error() {
    let fx = fixtures();
    let raw = fx.raw.join("invalid");
    std::fs::create_dir_all(&raw).unwrap();
    for name in ["manifests.json", "variables.json"] {
        std::fs::copy(fx.raw.join("a").join(name), raw.join(name)).unwrap();
    }
    let mut profile = read_json(&fx.raw.join("a").join("profile.json"));
    for pack in profile["spec"]["published"]["packs"]
        .as_array_mut()
        .unwrap()
    {
        if pack["layer"] == "os" {
            pack["isInvalid"] = serde_json::Value::Bool(true);
            pack["inValidReason"] = "ubuntu-aws 22.04 tag is deleted".into();
        }
    }
    std::fs::write(
        raw.join("profile.json"),
        serde_json::to_string(&profile).unwrap(),
    )
    .unwrap();
    let out = fx.raw.join("invalid.json");
    let args = [
        "snapshot",
        "--from-raw",
        raw.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ];
    let r = run_in(&args, &[], &["NO_COLOR", "CLICOLOR_FORCE", "STRATA_QUIET"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(
        r.err.contains(
            "strata: warning: Palette marks pack os/ubuntu-aws invalid (ubuntu-aws 22.04 tag is \
             deleted). The record keeps the flag, outside the root"
        ),
        "{}",
        r.err
    );
    let record = read_json(&out);
    assert_eq!(record["root"], read_json(Path::new(&fx.a))["root"]);
    // a quiet run writes no warning, as for each other warning
    let r = run(&args);
    assert_eq!((r.code, r.err.as_str()), (0, ""));
    // the record of the valid version warns of nothing
    let r = run_in(
        &["snapshot", "--from-raw", fx.raw.join("a").to_str().unwrap()],
        &[],
        &["NO_COLOR", "CLICOLOR_FORCE", "STRATA_QUIET"],
    );
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(!r.err.contains("invalid"), "{}", r.err);
}
