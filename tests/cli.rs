//! End-to-end tests over raw-API-shaped fixtures (no network).

use crate::common;

use std::path::{Path, PathBuf};

use strata::diff::DIFF_SCHEMA;
use strata::ops::{CHAIN_SCHEMA, VERIFY_SUMMARY_SCHEMA};
use strata::snapshot::{SNAPSHOT_SCHEMA, Tier};

use common::{
    CLUSTER_LISTING, PROFILE_LISTING, PROJECT_LISTING, Run, Stand, UID_A, UID_B,
    against_closed_port, isolate_palette, listed, project_listing, read_json, schema,
    schema_values, snapshot_from_raw, strata_command, write_record,
};

const ESC: char = '\x1b';

fn run_in(args: &[&str], envs: &[(&str, &str)], clear: &[&str]) -> Run {
    let mut c = strata_command();
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
    assert_eq!(a["schema"], SNAPSHOT_SCHEMA);
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

/// `strata report` is a command, so a file named `report` is the command when it comes first.
/// `compare` reads it as a file (compatibility.md, release 0.27.0).
#[test]
fn a_file_named_report_names_the_command_after_another_argument_and_compare_reads_it_as_a_file() {
    let fx = fixtures();
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(&fx.a, dir.path().join("report")).unwrap();
    let mut c = strata_command();
    c.args(["compare", "report", &fx.a2])
        .current_dir(dir.path())
        .env("STRATA_QUIET", "1");
    let r = Run::of(&mut c);
    assert_eq!(r.code, 0, "{}{}", r.out, r.err);
    assert!(r.out.contains("IDENTICAL"), "{}", r.out);

    let r = run(&[&fx.a, "report"]);
    assert_eq!(r.code, 2, "{}{}", r.out, r.err);
    assert!(
        r.err
            .contains("'report' is a command, so it comes first: strata report ..."),
        "{}",
        r.err
    );
}

#[test]
fn each_command_that_reads_a_record_with_a_null_variable_name_names_the_field_and_exits_two() {
    let fx = fixtures();
    let mut record = read_json(Path::new(&fx.a));
    record["variables"][0]["name"] = serde_json::Value::Null;
    let bad = Path::new(&fx.a).with_file_name("null-name.json");
    write_record(&bad, &record);
    let bad = bad.to_str().unwrap();
    for args in [
        vec!["compare", bad, fx.a.as_str()],
        vec!["verify", bad],
        vec!["same-release", bad, fx.a.as_str()],
        vec!["behind", bad],
    ] {
        let r = run(&args);
        assert_eq!(r.code, 2, "{args:?}: {}{}", r.out, r.err);
        assert!(
            r.err.contains(
                "variables[0].name: invalid type: null, expected a string. Take the record again."
            ),
            "{args:?}: {}",
            r.err
        );
    }
}

/// A record of a cluster, written to a file next to the profile records of `fx`.
fn cluster_record(fx: &Fx) -> (PathBuf, serde_json::Value) {
    let path = Path::new(&fx.a).with_file_name("cluster-a.json");
    snapshot_from_raw(&fx.raw, "cluster-a", &path);
    let record = read_json(&path);
    (path, record)
}

#[test]
fn a_field_inside_the_cluster_of_a_record_gives_its_own_path_in_snapshot_schema() {
    let fx = fixtures();
    let (_, record) = cluster_record(&fx);
    let cases: [(&[&str], serde_json::Value, &str); 4] = [
        (
            &["profiles", "0", "version"],
            serde_json::json!(7),
            "source.cluster.profiles[0].version: invalid type: integer `7`, expected a string",
        ),
        (
            &["name"],
            serde_json::json!(5),
            "source.cluster.name: invalid type: integer `5`, expected a string",
        ),
        (
            &["labels"],
            serde_json::json!("env=dev"),
            "source.cluster.labels: invalid type: string \"env=dev\", expected a map",
        ),
        // a read error of strata 0.24 and before moves into `errors`, but the error names the
        // key that the file holds
        (
            &["pendingRepaveError"],
            serde_json::json!(7),
            "source.cluster.pendingRepaveError: invalid type: integer `7`, expected a string",
        ),
    ];
    for (n, (keys, wrong, message)) in cases.into_iter().enumerate() {
        let mut bad = record.clone();
        let mut at = &mut bad["source"]["cluster"];
        for key in keys {
            at = match key.parse::<usize>() {
                Ok(index) => &mut at[index],
                Err(_) => &mut at[*key],
            };
        }
        *at = wrong;
        let file = Path::new(&fx.a).with_file_name(format!("bad-cluster-{n}.json"));
        write_record(&file, &bad);
        let file = file.to_str().unwrap();
        for args in [
            vec!["compare", file, fx.a.as_str()],
            vec!["verify", file],
            vec!["behind", file],
        ] {
            let r = run(&args);
            assert_eq!(r.code, 2, "{args:?}: {}{}", r.out, r.err);
            assert!(
                r.err.contains(message) && r.err.contains("Take the record again."),
                "{args:?}: {}",
                r.err
            );
        }
    }
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
        &format!("^lab-profile@1.2.0 (uid {UID_A})  ->  lab-profile@1.3.0 (uid {UID_B})")
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
    assert_eq!(d["schema"], DIFF_SCHEMA);
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

/// The changes of the pack `name` in the JSON document of a comparison.
fn pack_of<'a>(doc: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    doc["packs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .unwrap_or_else(|| panic!("no pack {name}"))
}

/// The unified text of the manifest `name` of a pack, in the JSON document of a comparison.
fn manifest_text<'a>(pack: &'a serde_json::Value, name: &str) -> (&'a str, &'a str) {
    let m = pack["manifests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == name)
        .unwrap_or_else(|| panic!("no manifest {name}"));
    (m["op"].as_str().unwrap(), m["unified"].as_str().unwrap())
}

#[test]
fn compare_json_with_full_carries_the_unified_text_and_without_it_carries_none() {
    let fx = fixtures();
    let plain = run(&["compare", "--json", &fx.a, &fx.b]);
    assert!(!plain.out.contains("\"unified\""), "{}", plain.out);
    let full: serde_json::Value =
        serde_json::from_str(&run(&["compare", "--json", "--full", &fx.a, &fx.b]).out).unwrap();
    assert_eq!(
        full["schema"], DIFF_SCHEMA,
        "a new field keeps the id of the document"
    );

    let values = pack_of(&full, "kubernetes")["values"]["unified"]
        .as_str()
        .unwrap();
    assert!(
        values.starts_with("--- a: k8s/kubernetes values\n+++ b: k8s/kubernetes values\n@@"),
        "{values}"
    );
    assert!(values.contains("+      profiling: \"false\"\n"), "{values}");
    let unchanged = pack_of(&full, "ubuntu-aws");
    assert!(unchanged["values"].get("unified").is_none(), "{unchanged}");
}

#[test]
fn compare_json_with_full_gives_a_manifest_that_one_side_added_or_removed_its_text() {
    let fx = fixtures();
    let forward: serde_json::Value =
        serde_json::from_str(&run(&["compare", "--json", "--full", &fx.a, &fx.b]).out).unwrap();
    let back: serde_json::Value =
        serde_json::from_str(&run(&["compare", "--json", "--full", &fx.b, &fx.a]).out).unwrap();
    let (forward, back) = (
        pack_of(&forward, "prometheus-operator"),
        pack_of(&back, "prometheus-operator"),
    );

    let (op, text) = manifest_text(forward, "extra-rules");
    assert_eq!(op, "changed");
    assert!(text.starts_with("--- a: addon/prometheus-operator manifest extra-rules\n+++ b: addon/prometheus-operator manifest extra-rules\n"), "{text}");
    assert!(
        text.contains("-          for: 5m\n\\ No newline at end of file\n+          for: 10m\n"),
        "{text}"
    );

    let (op, added) = manifest_text(forward, "dashboards");
    assert_eq!(op, "added");
    assert!(
        added.starts_with("--- a: addon/prometheus-operator manifest dashboards\n+++ b: "),
        "{added}"
    );
    assert!(added.contains("\n+kind: ConfigMap\n"), "{added}");
    assert!(
        !added.contains("\n-"),
        "an added manifest has no removed line: {added}"
    );

    let (op, removed) = manifest_text(back, "dashboards");
    assert_eq!(op, "removed");
    assert!(removed.contains("\n-kind: ConfigMap\n"), "{removed}");
    assert!(
        !removed.contains("\n+kind"),
        "a removed manifest has no added line: {removed}"
    );
}

#[test]
fn behind_refuses_a_pin_with_no_impact_and_a_pin_that_no_record_holds() {
    let fx = fixtures();
    let r = run(&["behind", &fx.a, "--pin", "k8s/kubernetes", "--no-impact"]);
    assert_eq!(r.code, 2);
    assert!(r.err.contains("cannot be used with"), "{}", r.err);
    assert_eq!(r.out, "");

    // the records give the answer, so the command needs no connection
    let r = run(&["behind", &fx.a, "--pin", "addon/nothing"]);
    assert_eq!(r.code, 2);
    assert!(r.err.contains("--pin addon/nothing"), "{}", r.err);
    assert_eq!(r.out, "");
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
fn compare_refuses_a_pair_that_a_question_to_palette_cannot_take_before_it_needs_a_key() {
    let fx = fixtures();
    let home = tempfile::tempdir().unwrap();
    let no_key = [("HOME", home.path().to_str().unwrap())];
    let clear = ["PALETTE_API_KEY", "SPECTROCLOUD_APIKEY"];
    let ask = |args: &[&str]| {
        let mut all = vec!["compare", "--json"];
        all.extend_from_slice(args);
        let r = run_in(&all, &no_key, &clear);
        let e: serde_json::Value = serde_json::from_str(r.err.trim()).unwrap();
        (r.code, e["error"].clone())
    };
    // the refs show the fault, so the run needs no key and sends no request
    for (args, said) in [
        (
            &["aws-stack@1.1.1", "aws-stack@1.1.6", "--repave"][..],
            "--repave needs one side that is a cluster and one that is a profile version",
        ),
        (
            &["cluster:payments", "cluster:search", "--repave"][..],
            "--repave needs one side that is a cluster and one that is a profile version, or a \
             cluster and the update that waits for it",
        ),
        (
            &["aws-stack@1.1.1", "aws-stack@1.1.7", "--validate"][..],
            "--validate needs one side that is a cluster and one that is a profile version",
        ),
        (
            &["edgehost:e1", "aws-stack@1.1.1", "--repave"][..],
            "--repave needs a cluster: Palette answers it for a cluster, and not for an edge host",
        ),
        (
            &["cluster:payments", "pack:kubernetes@1.30.0", "--validate"][..],
            "--validate needs the profile side to be a profile version that Palette holds",
        ),
        (
            &["tag:env=dev", "cluster:payments", "--repave"][..],
            "tag:env=dev names a set of clusters",
        ),
        // a record file shows its side after the read of the file, which sends no request
        (
            &[fx.a.as_str(), fx.b.as_str(), "--repave"][..],
            "--repave needs one side that is a cluster and one that is a profile version",
        ),
        (
            &[fx.a.as_str(), "aws-stack@1.1.1", "--validate"][..],
            "--validate needs one side that is a cluster and one that is a profile version",
        ),
    ] {
        let (code, error) = ask(args);
        assert_eq!(
            (code, error["kind"].as_str()),
            (2, Some("usage")),
            "{args:?}: {error}"
        );
        assert!(
            error["message"].as_str().unwrap().contains(said),
            "{args:?}: {error}"
        );
    }
    // a pair that the questions take needs the key: only then strata connects
    for args in [
        &["cluster:payments", "aws-stack@1.1.1", "--repave"][..],
        &["cluster:payments", "cluster:payments@pending", "--repave"][..],
        &[
            "aws-stack@1.1.1",
            "cluster:payments",
            "--validate",
            "--repave",
        ][..],
    ] {
        let (code, error) = ask(args);
        assert_eq!(
            (code, error["kind"].as_str()),
            (2, Some("missing-credential")),
            "{args:?}: {error}"
        );
    }
    // with no question, the refs are not checked: two profile versions compare
    let r = run_in(&["compare", "--json", &fx.a, &fx.b], &no_key, &clear);
    assert_eq!((r.code, r.err.as_str()), (1, ""), "{}", r.err);
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
    assert_eq!(d["schema"], VERIFY_SUMMARY_SCHEMA);
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
    assert_eq!(d["schema"], VERIFY_SUMMARY_SCHEMA);
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
    assert_eq!(d["schema"], CHAIN_SCHEMA);
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
                "^lab-profile@1.3.0 (uid {UID_B})  ->  lab-profile@1.3.0 on cluster lab-cluster (uid {uid})"
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

/// A field test typed `strata list project`, and strata refused it before it read Palette. Each
/// kind also takes its singular, which sends the requests and prints the output of the plural.
#[test]
fn list_takes_the_singular_of_each_kind_and_does_what_the_plural_does() {
    let routes = std::collections::BTreeMap::from([
        (
            PROJECT_LISTING.to_string(),
            project_listing(&[("dev", "p-dev"), ("prod", "p-prod")]),
        ),
        (
            PROFILE_LISTING.to_string(),
            serde_json::json!({"items": [listed("aws-stack", UID_A)]}),
        ),
        (
            CLUSTER_LISTING.to_string(),
            serde_json::json!({"items": []}),
        ),
    ]);
    let stand = Stand::new(routes, "p-dev");
    // the exit code, the document with no time stamp, the error, and each request
    let list = |kind: &str| {
        let r = stand.run(&["list", kind, "--json"]);
        (r.code, r.document_without_time(), r.err, stand.requests())
    };
    for (singular, plural) in [
        ("profile", "profiles"),
        ("cluster", "clusters"),
        ("project", "projects"),
        ("filter", "filters"),
    ] {
        let one = list(singular);
        let all = list(plural);
        assert!(!all.3.is_empty(), "list {plural} reads Palette");
        assert_eq!(one, all, "list {singular} does what list {plural} does");
    }
    let (code, document, err, _) = list("project");
    assert_eq!(code, 0, "{err}");
    let names: Vec<&str> = document["projects"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| p["name"].as_str())
        .collect();
    assert_eq!(names, ["dev", "prod"], "{document}");
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

/// The definition of the closed set that `field` names, for a field of a document that strata
/// writes and reads. Such a field takes each value of the set, or any other text of a later
/// release.
fn open_set_ref(field: &serde_json::Value) -> &serde_json::Value {
    assert_eq!(field["anyOf"][1]["type"], "string", "{field}");
    &field["anyOf"][0]["$ref"]
}

#[test]
fn no_description_of_a_schema_holds_the_markup_of_a_rustdoc_link() {
    for kind in [
        "snapshot",
        "diff",
        "verify-summary",
        "chain",
        "consumers",
        "behind",
        "set",
        "list",
        "packs",
        "candidate",
    ] {
        assert!(
            !schema(kind).to_string().contains("[`"),
            "{kind}: a link in a description"
        );
    }
}

#[test]
fn the_schema_of_a_record_points_each_closed_set_field_at_the_definition_of_its_set() {
    let v = schema("snapshot");
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
        open_set_ref(&defs["ClusterSource"]["properties"]["kind"]),
        "#/$defs/SubjectKind"
    );
    assert_eq!(
        open_set_ref(&defs["Pack"]["properties"]["registryProviderType"]["anyOf"][0]),
        "#/$defs/OciProvider"
    );

    let v = schema("diff");
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
fn the_schema_of_each_other_document_points_each_closed_set_field_at_the_definition_of_its_set() {
    let v = schema("snapshot");
    assert_eq!(
        open_set_ref(&v["$defs"]["Tool"]["properties"]["impl"]["anyOf"][0]),
        "#/$defs/Implementation"
    );
    let v = schema("diff");
    assert_eq!(schema_values(&v["$defs"]["Side"]), ["a", "b"]);
    assert_eq!(
        v["$defs"]["RunningBehind"]["properties"]["side"]["$ref"],
        "#/$defs/Side"
    );
    let v = schema("candidate");
    assert_eq!(schema_values(&v["$defs"]["LayerKind"]), ["manifest"]);
    assert_eq!(
        v["$defs"]["CandidateLayer"]["properties"]["type"]["anyOf"][0]["$ref"],
        "#/$defs/LayerKind"
    );
    let v = schema("list");
    assert_eq!(
        v["$defs"]["SavedFilter"]["properties"]["conjunction"]["$ref"],
        "#/$defs/Conjunction"
    );
    assert_eq!(
        open_set_ref(&v["$defs"]["ListedProfile"]["properties"]["type"]["anyOf"][0]),
        "#/$defs/ProfileType"
    );
    let v = schema("behind");
    assert_eq!(
        open_set_ref(&v["$defs"]["Bump"]["properties"]["flags"]["items"]),
        "#/$defs/ChangeFlag"
    );
    assert_eq!(
        open_set_ref(&v["$defs"]["Bump"]["properties"]["impact"]),
        "#/$defs/ImpactLevel"
    );
    assert_eq!(
        open_set_ref(&v["$defs"]["BumpPack"]["properties"]["status"]),
        "#/$defs/Status"
    );
    assert_eq!(
        open_set_ref(&v["$defs"]["BumpPack"]["properties"]["flags"]["items"]),
        "#/$defs/ChangeFlag"
    );
    assert_eq!(
        open_set_ref(&v["$defs"]["Pin"]["properties"]["pinnedState"]["anyOf"][0]),
        "#/$defs/PackState"
    );
    assert_eq!(
        open_set_ref(&v["$defs"]["Newer"]["properties"]["state"]["anyOf"][0]),
        "#/$defs/PackState"
    );
    let v = schema("consumers");
    assert_eq!(
        v["$defs"]["ConsumedProfile"]["properties"]["type"]["anyOf"][0]["$ref"],
        "#/$defs/ProfileType"
    );
}

/// The `description` of `field` in the definition `def` of the schema of a document.
fn schema_description(schema: &serde_json::Value, def: &str, field: &str) -> String {
    schema["$defs"][def]["properties"][field]["description"]
        .as_str()
        .unwrap_or_else(|| panic!("{def}.{field} has no description: {}", schema["$defs"][def]))
        .to_string()
}

#[test]
fn the_schema_names_the_values_that_strata_writes_in_a_field_that_palette_gives() {
    let snapshot = schema("snapshot");
    for (def, field) in [("Source", "type"), ("Pack", "layer")] {
        let text = schema_description(&snapshot, def, field);
        assert!(text.contains("`app`"), "{def}.{field}: {text}");
    }
    let text = schema_description(&snapshot, "Source", "type");
    assert!(
        text.contains("`pack`"),
        "a pack record has the type pack: {text}"
    );
    let consumers = schema("consumers");
    let text = schema_description(&consumers, "Consumer", "state");
    assert!(text.contains("`unknown`"), "strata writes unknown: {text}");
}

#[test]
fn the_schema_names_the_model_of_palette_and_copies_no_value_of_a_field_that_palette_owns() {
    let (snapshot, candidate, diff) = (schema("snapshot"), schema("candidate"), schema("diff"));
    let (list, consumers) = (schema("list"), schema("consumers"));
    let fields = [
        (&snapshot, "Pack", "layer", "v1PackLayer"),
        (&snapshot, "Pack", "type", "v1PackType"),
        (&snapshot, "Pack", "type", "v1AppTierType"),
        (&candidate, "CandidateLayer", "layer", "v1PackLayer"),
        (&diff, "AuditEntry", "actorType", "v1AuditActor"),
        (&diff, "AuditEntry", "action", "v1AuditActionType"),
        (&list, "ListedCluster", "state", "v1ClusterState"),
        (&consumers, "Consumer", "state", "v1ClusterState"),
    ];
    // a value that Palette has today in one of these fields. A later release of Palette can
    // change the set, so the description must not copy it
    let copied = [
        "k8s",
        "cni",
        "csi",
        "kernel",
        "spectro",
        "helm",
        "oci",
        "operator-instance",
        "container",
        "service",
        "activity",
        "publish",
        "deploy",
        "Running",
        "Deleting",
        "Provisioning",
    ];
    for (document, def, field, model) in fields {
        let text = schema_description(document, def, field);
        assert!(
            text.contains("as Palette gives it"),
            "{def}.{field}: {text}"
        );
        assert!(text.contains(model), "{def}.{field} names {model}: {text}");
        for value in copied {
            assert!(
                !text.contains(value),
                "{def}.{field} copies {value}: {text}"
            );
        }
    }
}

/// The twelve fields of a profile variable, by name.
const VARIABLE_FIELDS: [&str; 12] = [
    "defaultValue",
    "description",
    "displayName",
    "format",
    "hidden",
    "immutable",
    "inputType",
    "isSensitive",
    "name",
    "options",
    "regex",
    "required",
];

/// The names of the properties of the schema `object`, in the order of the document.
fn property_names(object: &serde_json::Value) -> Vec<&str> {
    object["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("no properties: {object}"))
        .keys()
        .map(String::as_str)
        .collect()
}

#[test]
fn the_schema_of_a_record_and_of_a_candidate_name_the_twelve_fields_of_a_variable() {
    for kind in ["snapshot", "candidate"] {
        let v = schema(kind);
        let variable = &v["properties"]["variables"]["items"];
        assert_eq!(property_names(variable), VARIABLE_FIELDS, "{kind}");
        assert_eq!(variable["required"], serde_json::json!(["name"]), "{kind}");
        let option = &v["$defs"]["VariableOption"];
        assert_eq!(
            property_names(option),
            ["description", "label", "value"],
            "{kind}: {option}"
        );
        assert_eq!(
            variable["properties"]["options"]["items"]["$ref"], "#/$defs/VariableOption",
            "{kind}"
        );
    }
}

#[test]
fn the_schema_of_a_variable_names_the_model_of_palette_and_copies_no_value_of_it() {
    let snapshot = schema("snapshot");
    let variable = &snapshot["properties"]["variables"]["items"]["properties"];
    for (field, model) in [
        ("format", "v1VariableFormat"),
        ("inputType", "v1VariableInputType"),
    ] {
        let text = variable[field]["description"].as_str().unwrap();
        assert!(text.contains("as Palette gives it"), "{field}: {text}");
        assert!(text.contains(model), "{field} names {model}: {text}");
        assert!(
            variable[field].get("enum").is_none(),
            "{field}: a set that Palette owns"
        );
        assert_eq!(variable[field]["type"], "string", "{field}");
        for value in ["ipv4", "ipv6", "base64", "dropdown", "multiline"] {
            assert!(!text.contains(value), "{field} copies {value}: {text}");
        }
    }
}

#[test]
fn the_schema_of_a_leaf_gives_a_segment_a_key_an_index_or_a_named_item() {
    for kind in ["diff", "verify-summary"] {
        let v = schema(kind);
        let any_of = &v["$defs"]["Leaf"]["properties"]["segments"]["items"]["anyOf"];
        assert_eq!(any_of.as_array().unwrap().len(), 3, "{kind}");
        let types: Vec<&serde_json::Value> = any_of
            .as_array()
            .unwrap()
            .iter()
            .map(|s| &s["type"])
            .collect();
        assert_eq!(types, ["string", "integer", "object"], "{kind}");
        assert_eq!(any_of[2]["required"], serde_json::json!(["name"]), "{kind}");
        assert_eq!(any_of[2]["properties"]["name"]["type"], "string", "{kind}");
    }
}

#[test]
fn the_schema_of_a_candidate_layer_types_the_params_as_the_schema_of_a_record_does() {
    // the record of a candidate keeps the params of its layers, so the two schemas must agree
    let shape = |v: &serde_json::Value, def: &str| {
        let params = &v["$defs"][def]["properties"]["params"];
        (
            params["type"].clone(),
            params["properties"]["installOrder"]["type"].clone(),
            params["additionalProperties"].clone(),
        )
    };
    let (candidate, record) = (
        shape(&schema("candidate"), "CandidateLayer"),
        shape(&schema("snapshot"), "Pack"),
    );
    assert_eq!(candidate, record);
    assert_eq!(candidate.2["type"], "string");
}

#[test]
fn the_schema_of_a_record_types_the_params_and_the_tiers() {
    let v = schema("snapshot");
    let params = &v["$defs"]["Pack"]["properties"]["params"];
    assert_eq!(params["type"], "object");
    assert_eq!(params["additionalProperties"]["type"], "string");
    assert_eq!(
        params["properties"]["installOrder"]["type"], "integer",
        "the install order of a tier is the one param that is a number"
    );
    let tiers = &v["$defs"]["ClusterSource"]["properties"]["tiers"];
    assert_eq!(
        property_names(tiers),
        ["applied", "policy", "running", "shape"],
        "a record carries no tier labels: only compare gives it"
    );
    let names = &tiers["propertyNames"]["anyOf"];
    assert_eq!(names[0]["$ref"], "#/$defs/Tier");
    assert_eq!(
        names[1]["type"], "string",
        "the name of a tier of a later release"
    );
    assert_eq!(tiers["additionalProperties"]["$ref"], "#/$defs/ClusterTier");
    let fields = |tier: Tier| common::tier_fields(&v, tier);
    assert_eq!(
        property_names(fields(Tier::Applied)),
        ["spcHash", "spcInfraHash"]
    );
    assert_eq!(
        property_names(&fields(Tier::Running)["items"]),
        ["name", "profileUid", "version"]
    );
    for key in [
        "osPatch",
        "lifecycle",
        "machineHealth",
        "timezone",
        "rbacs",
        "namespaces",
        "fips",
    ] {
        assert!(
            fields(Tier::Policy)["properties"].get(key).is_some(),
            "{key}"
        );
    }
    assert_eq!(
        property_names(fields(Tier::Shape)),
        ["cloud", "cloudType", "pools"]
    );
    assert!(fields(Tier::Shape)["properties"]["pools"]["items"]["properties"]["name"].is_boolean());
}

#[test]
fn the_schema_of_a_record_names_the_parts_of_a_cluster_that_strata_could_not_read() {
    let v = schema("snapshot");
    let errors = &v["$defs"]["ClusterSource"]["properties"]["errors"];
    assert_eq!(errors["type"], "object");
    assert_eq!(errors["additionalProperties"]["type"], "string");
    let names = &errors["propertyNames"]["anyOf"];
    assert_eq!(names[0]["$ref"], "#/$defs/ClusterRead");
    assert_eq!(names[1]["type"], "string", "the part of a later release");
}

#[test]
fn the_schema_of_each_error_is_an_object_with_a_kind_and_a_message() {
    for (kind, def, field) in [
        ("list", "ListedProfile", "error"),
        ("list", "ListedCluster", "error"),
        ("consumers", "Consumer", "error"),
        ("consumers", "ClustersRead", "error"),
        ("consumers", "ClustersRead", "groupsError"),
        ("consumers", "EdgeHostsRead", "error"),
        ("set", "ListedCluster", "error"),
        ("verify-summary", "VerifiedRecord", "error"),
        ("set", "SetRecord", "error"),
        ("behind", "Pin", "error"),
    ] {
        let v = schema(kind);
        let property = &v["$defs"][def]["properties"][field];
        assert_eq!(
            property["anyOf"][0]["$ref"], "#/$defs/ErrorDocument",
            "{kind} {def}.{field}: {property}"
        );
        let error = &v["$defs"]["ErrorDocument"];
        assert_eq!(error["type"], "object", "{kind}");
        assert_eq!(error["required"], serde_json::json!(["kind", "message"]));
        assert_eq!(error["properties"]["kind"]["type"], "string");
        assert_eq!(error["properties"]["message"]["type"], "string");
    }
}

/// True when the schema `property` of a time field is a time in RFC 3339. A field that Palette
/// gives, or that a report holds only sometimes, is `anyOf` the time and empty text, or null.
fn date_time(property: &serde_json::Value) -> bool {
    let is_time = |s: &serde_json::Value| s["format"] == "date-time";
    is_time(property)
        || property["anyOf"]
            .as_array()
            .is_some_and(|options| options.iter().any(is_time))
}

/// True when the name of a property is the name of a time field. Such a name is one of `names`,
/// or ends in `At`, `Time`, or `Modified`. The ending finds a new time field with no change here.
fn is_time_name(name: &str, names: &[&str]) -> bool {
    names.contains(&name)
        || ["At", "Time", "Modified"]
            .iter()
            .any(|end| name.ends_with(end))
}

/// Each property of the schema `v` whose name is the name of a time field, with the path to it.
fn time_properties<'a>(
    v: &'a serde_json::Value,
    names: &[&str],
    at: &str,
    found: &mut Vec<(String, &'a serde_json::Value)>,
) {
    if let Some(properties) = v["properties"].as_object() {
        for (name, property) in properties {
            if is_time_name(name, names) {
                found.push((format!("{at}.{name}"), property));
            }
        }
    }
    if let Some(defs) = v["$defs"].as_object() {
        for (def, schema) in defs {
            time_properties(schema, names, def, found);
        }
    }
}

#[test]
fn the_schema_of_each_time_field_gives_it_the_format_date_time() {
    let names = [
        "fetchedAt",
        "generatedAt",
        "since",
        "time",
        "recordedAt",
        "checkedAt",
        "listed",
        "deletedAt",
        "lastRunTime",
        "lastSyncedTime",
        "lastModified",
    ];
    let mut seen = Vec::new();
    for kind in [
        "snapshot",
        "diff",
        "verify-summary",
        "chain",
        "consumers",
        "behind",
        "set",
        "list",
        "packs",
    ] {
        let v = schema(kind);
        let mut found = Vec::new();
        time_properties(&v, &names, kind, &mut found);
        for (at, property) in found {
            assert!(date_time(property), "{at}: {property}");
            seen.push(at);
        }
    }
    for name in names {
        assert!(
            seen.iter().any(|at| at.ends_with(&format!(".{name}"))),
            "no schema has the time field {name}: {seen:?}"
        );
    }
}

#[test]
fn the_summary_row_of_a_file_that_strata_cannot_read_matches_the_schema_of_the_row() {
    let dir = tempfile::tempdir().unwrap();
    let not_a_record = dir.path().join("notarecord.json");
    std::fs::write(&not_a_record, "{}").unwrap();
    let missing = dir.path().join("missing.json");
    let r = run(&[
        "verify",
        "--summary",
        "--json",
        not_a_record.to_str().unwrap(),
        missing.to_str().unwrap(),
    ]);
    assert_eq!(r.code, 2, "{}", r.err);
    let summary: serde_json::Value = serde_json::from_str(&r.out).unwrap();
    let rows = summary["records"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let recorded_at =
        &schema("verify-summary")["$defs"]["VerifiedRecord"]["properties"]["recordedAt"];
    for row in rows {
        assert_eq!(row["status"], "error", "{row}");
        let time = row["recordedAt"].as_str().unwrap();
        assert_eq!(time, "", "strata read no time from the file");
        assert!(
            common::text_matches_schema(recorded_at, time),
            "{recorded_at}"
        );
    }
    // the schema still refuses a text that is not a time
    assert!(!common::text_matches_schema(recorded_at, "yesterday"));
    assert!(common::text_matches_schema(
        recorded_at,
        "2026-09-17T09:30:00Z"
    ));
}

/// The root and the `variablesHash` that strata 0.24.0 gives the profile `a` of the fixtures with
/// the variables of [`the_variables_of_an_older_release_keep_their_root_and_their_hash`]. They
/// hold a field of another name, a format that is not text, and a bare name.
const OLDER_VARIABLES_ROOT: &str =
    "8c8b5780bf3b563be53d0d04a314bc817874b33eea20e139f7d4410bbf18d13d";
const OLDER_VARIABLES_HASH: &str =
    "ce5f9cb050c99ef9e9d75a6fa929826cbf1b477d718b7f12b874c6de7c8e00f9";

#[test]
fn the_variables_of_an_older_release_keep_their_root_and_their_hash() {
    let dir = tempfile::tempdir().unwrap();
    let raw = dir.path().join("fx");
    common::make_fixtures(&raw);
    std::fs::write(
        raw.join("a/variables.json"),
        serde_json::json!({"variables": [
            {"name": "tier", "displayName": "Tier", "description": "The tier.", "format": "string",
             "defaultValue": "gold", "regex": "^[a-z]+$", "required": true, "immutable": true,
             "hidden": true, "isSensitive": true, "inputType": "dropdown",
             "options": [{"label": "Gold", "value": "gold", "description": "Best"},
                         {"label": "Tin", "value": "tin"}],
             "extra": 7},
            {"name": "bare"},
            {"name": "numbers", "format": 3, "defaultValue": 5, "required": "yes",
             "options": [{"label": 1, "value": null}]},
        ]})
        .to_string(),
    )
    .unwrap();
    let record = read_json(Path::new(&record_of(&raw, dir.path(), "a")));
    assert_eq!(record["variablesHash"], OLDER_VARIABLES_HASH);
    assert_eq!(record["root"], OLDER_VARIABLES_ROOT);
    // the file holds the twelve fields of each variable and no other, in the order of their names
    let tier = record["variables"][2].as_object().unwrap();
    assert_eq!(tier.len(), 12);
    assert!(tier.get("extra").is_none());
    assert_eq!(record["variables"][0]["name"], "bare");
    assert_eq!(record["variables"][1]["format"], "string");
}

#[test]
fn schema_subcommand_prints_json_schema() {
    // This table pins the ids on purpose. `strata schema` names each kind with the id that a
    // consumer reads, so a change of an id must change this table too
    for (kind, id) in [
        ("snapshot", "strata-snapshot/1"),
        ("diff", "strata-diff/1"),
        ("verify-summary", "strata-verify-summary/1"),
        ("chain", "strata-chain/1"),
        ("consumers", "strata-consumers/1"),
        ("list", "strata-list/1"),
        ("packs", "strata-packs/1"),
        ("set", "strata-set/1"),
        ("report", "strata-report/1"),
        ("candidate", ""),
    ] {
        let v = schema(kind);
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
        // version. The id it holds is the one the writer puts in the document.
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
    against_closed_port(strata_command(), args, envs, true)
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
        strata_command()
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
    against_closed_port(strata_command(), args, envs, false)
}

/// A uid and a name both reach strata and the uid decides, so the name decided nothing. Strata
/// resolves the name to learn whether the two name one project, and that listing is the first
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

/// Strata reads `~/.palette.env` only from a `HOME` that is an absolute path. An empty or a
/// relative `HOME` names a folder from the working folder, and a checkout can put a file there.
/// Such a file gives no setting, so it cannot send the key of the environment to its address.
#[test]
fn an_empty_or_a_relative_home_reads_no_palette_env_of_the_working_folder() {
    let work = tempfile::tempdir().unwrap();
    std::fs::create_dir(work.path().join("home")).unwrap();
    // strata refuses this setting before the first request, so the kind shows a read of the file
    for file in [".palette.env", "home/.palette.env"] {
        std::fs::write(work.path().join(file), "STRATA_MAX_RETRY_AFTER=soon\n").unwrap();
    }
    let absolute = work.path().join("home");
    for (home, kind) in [
        (Path::new(""), "transport"),
        (Path::new("home"), "transport"),
        (Path::new("./home"), "transport"),
        (absolute.as_path(), "invalid-setting"),
    ] {
        let mut c = strata_command();
        isolate_palette(&mut c, home)
            .current_dir(work.path())
            .env("STRATA_QUIET", "1")
            .args(["list", "--json", "projects"]);
        let r = Run::of(&mut c);
        let e: serde_json::Value = serde_json::from_str(r.err.trim())
            .unwrap_or_else(|_| panic!("HOME={home:?}: {}", r.err));
        assert_eq!(
            (r.code, e["error"]["kind"].as_str()),
            (2, Some(kind)),
            "HOME={home:?}: {e}"
        );
    }
}

/// A quiet run writes no warning, so it also skips the listing that finds one. With `--json`
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

/// Palette can save the address of the console of a pack of virtual machines at the request of
/// `--repave`. So the help says it before a person sends the request (#717).
#[test]
fn the_help_of_repave_says_what_palette_saves_at_the_request() {
    let help = run(&["compare", "--help"]);
    assert!(
        help.out.contains(
            "With the pack virtual-machine-orchestrator, spectro-vm-dashboard, or \
             vm-migration-assistant on either side, Palette can save the address that the \
             console of that pack forwards to, and the question can move it"
        ),
        "{}",
        help.out
    );
}

/// Two clusters on one release can print `CHANGED` for their values and exit 0. So the help
/// states each exit code, and what does not change it (#624).
#[test]
fn the_help_of_compare_states_the_exit_codes_and_what_does_not_change_them() {
    let help = run(&["compare", "--help"]);
    assert_eq!(help.code, 0, "{}", help.err);
    let exits = help
        .out
        .split_once("\nExit codes:\n")
        .map(|(_, tail)| tail)
        .unwrap_or_else(|| panic!("no exit codes: {}", help.out));
    let codes: Vec<&str> = exits
        .lines()
        .map_while(|l| l.strip_prefix("  "))
        .filter_map(|l| l.split_once("  ").map(|(code, _)| code))
        .collect();
    assert_eq!(codes, ["0", "1", "2"], "{exits}");
    assert!(exits.contains("SAME CONTENT also exits 1"), "{exits}");
    assert!(
        exits.contains(
            "The cluster values, the cluster tiers, the labels, an INVALID pack, and the answers \
             of --repave and --validate do not change the exit code."
        ),
        "{exits}"
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

/// The documents of a tenant with two versions of `barney`, by route: `1.0.0`, and the
/// pre-release `1.0.1-mr2.770685` above it. Each version has a draft, so `@draft` reads one too.
fn barney_tenant() -> std::collections::BTreeMap<String, serde_json::Value> {
    let versions = [(UID_A, "1.0.0"), (UID_B, "1.0.1-mr2.770685")];
    let mut routes = std::collections::BTreeMap::new();
    let listed: Vec<serde_json::Value> = versions
        .iter()
        .map(|(uid, version)| {
            serde_json::json!({"metadata": {"name": "barney", "uid": uid},
                "spec": {"cloudType": "aws", "version": version}})
        })
        .collect();
    routes.insert(
        PROFILE_LISTING.to_string(),
        serde_json::json!({"items": listed}),
    );
    for (uid, version) in versions {
        let published = common::template("aws", "add-on", common::packs());
        let draft = Some(published.clone());
        routes.insert(
            common::profile_route(uid),
            common::profile_document(uid, "barney", version, published, draft),
        );
        routes.insert(common::manifests_route(uid), common::no_manifests());
    }
    routes
}

/// The documents of a tenant with two versions of the app profile `shop`, by route. They are
/// `1.0.0` at `ap1`, and the pre-release `1.0.1-rc.1` above it at `ap2`. Each version has one
/// helm tier.
fn shop_tenant() -> std::collections::BTreeMap<String, serde_json::Value> {
    let versions = [("ap1", "1.0.0"), ("ap2", "1.0.1-rc.1")];
    let mut routes = std::collections::BTreeMap::new();
    let listed: Vec<serde_json::Value> = versions
        .iter()
        .map(|(uid, version)| {
            serde_json::json!({"metadata": {"name": "shop", "uid": uid}, "spec": {"version": version}})
        })
        .collect();
    routes.insert(
        common::APP_PROFILE_LISTING.to_string(),
        serde_json::json!({"appProfiles": listed}),
    );
    for (uid, version) in versions {
        routes.insert(
            common::app_profile_route(uid),
            serde_json::json!({"metadata": {"name": "shop", "uid": uid},
                "spec": {"version": version, "template": {"appTiers": [
                    {"name": "web", "uid": "t1", "type": "helm", "version": "2.1.0"}]}}}),
        );
        routes.insert(
            common::app_tiers_route(uid),
            serde_json::json!({"spec": {"appTiers": [{"metadata": {"name": "web", "uid": "t1"},
                "spec": {"type": "helm", "version": "2.1.0", "registryUid": "r1",
                    "sourceAppTierUid": "s1", "values": "replicaCount: 2\n"}}]}}),
        );
    }
    routes
}

/// Issue #625. A ref `<name>@latest` names the version that it takes, and the highest pre-release
/// above that version, which it skips. The draft of the version and `consumers` print the same
/// note. A quiet run writes no note, as for each other progress note.
#[test]
fn a_ref_to_latest_names_the_version_it_takes_and_the_pre_release_it_skips() {
    let stand = Stand::new(barney_tenant(), "p-dev");
    let strata = |args: &[&str]| stand.run_loud(args);
    let out = "latest.json";
    let note =
        "strata: @latest is barney@1.0.0; it skips 1.0.1-mr2.770685, a pre-release by SemVer\n";
    for reference in ["barney@latest", "barney@latest@draft"] {
        let r = strata(&["snapshot", reference, "-o", out]);
        assert_eq!(r.code, 0, "{reference}: {}", r.err);
        assert_eq!(r.err.matches(note).count(), 1, "{reference}: {}", r.err);
        assert!(
            r.err
                .contains(&format!("strata: fetching profile {UID_A}\n")),
            "{reference}: {}",
            r.err
        );
        let record = read_json(&stand.home.path().join(out));
        assert_eq!(record["source"]["version"], "1.0.0");
        let r = strata(&["snapshot", reference, "-o", out, "-q"]);
        assert_eq!((r.code, r.err.as_str()), (0, ""), "{reference}");
    }
    let r = strata(&["consumers", "barney@latest", "--list-only"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert_eq!(r.err.matches(note).count(), 1, "{}", r.err);
    assert!(
        r.err.contains(&format!("consumers of profile {UID_A}")),
        "{}",
        r.err
    );
    // an exact version names its version already, so it writes no note
    let r = strata(&["snapshot", "barney@1.0.0", "-o", out]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(!r.err.contains("@latest"), "{}", r.err);
}

/// Issue #625. `appprofile:<name>@latest` writes the note of the profile form, and names the
/// version as its ref. So does `consumers`.
#[test]
fn an_app_profile_at_latest_names_its_ref_and_the_pre_release_it_skips() {
    let stand = Stand::new(shop_tenant(), "p-dev");
    let strata = |args: &[&str]| stand.run_loud(args);
    let out = "shop.json";
    let note = "strata: @latest is appprofile:shop@1.0.0; it skips 1.0.1-rc.1, a pre-release by \
                SemVer\n";
    let r = strata(&["snapshot", "appprofile:shop@latest", "-o", out]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert_eq!(r.err.matches(note).count(), 1, "{}", r.err);
    assert!(
        r.err.contains("strata: fetching app profile ap1\n"),
        "{}",
        r.err
    );
    let record = read_json(&stand.home.path().join(out));
    assert_eq!(record["source"]["version"], "1.0.0");
    let r = strata(&["consumers", "appprofile:shop@latest"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert_eq!(r.err.matches(note).count(), 1, "{}", r.err);
    // an exact version names its version already, so it writes no note
    let r = strata(&["snapshot", "appprofile:shop@1.0.0", "-o", out]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(!r.err.contains("@latest"), "{}", r.err);
}
