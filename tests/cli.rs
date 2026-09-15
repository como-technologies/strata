//! End-to-end tests over the bash reference fixtures (no network).

use std::path::{Path, PathBuf};
use std::process::Command;

const ESC: char = '\x1b';

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_palette-profile-diff"))
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

struct Run {
    code: i32,
    out: String,
    err: String,
}

fn run_in(args: &[&str], envs: &[(&str, &str)], clear: &[&str]) -> Run {
    let mut c = Command::new(bin());
    c.args(args).env("PPD_QUIET", "1");
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
    let gen = root().join("bash/test/make-fixtures.sh");
    let st = Command::new("bash")
        .arg(&gen)
        .arg(&raw)
        .output()
        .expect("fixture generator runs");
    assert!(
        st.status.success(),
        "make-fixtures failed: {}",
        String::from_utf8_lossy(&st.stderr)
    );
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
    assert_eq!(a["schema"], "palette-profile-snapshot/1");
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
    assert!(has_line(t, "^lab-profile@1.2.0 (64f0000000000000000000a1)  ->  lab-profile@1.3.0 (64f0000000000000000000b2)"));
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
    assert!(t.contains("+ db_password.name: \"db_password\""));
    assert!(!t.contains(ESC));
}

#[test]
fn json_document_is_self_describing() {
    let fx = fixtures();
    let r = run(&["compare", "--json", &fx.a, &fx.b]);
    assert_eq!(r.code, 1);
    assert!(!r.out.contains(ESC));
    let d: serde_json::Value = serde_json::from_str(&r.out).unwrap();
    assert_eq!(d["schema"], "palette-profile-diff/1");
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
    assert!(r.err.contains("not a palette-profile-diff snapshot"));
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
fn schema_subcommand_prints_json_schema() {
    for kind in ["snapshot", "diff"] {
        let r = run(&["schema", kind]);
        assert_eq!(r.code, 0, "{}", r.err);
        let v: serde_json::Value = serde_json::from_str(&r.out).unwrap();
        assert!(v["$schema"].is_string());
        assert!(v["properties"]["schema"].is_object());
    }
}

/// The bash reference implementation must produce the same roots and node hashes.
#[test]
fn parity_with_bash_reference() {
    let bash_tool = root().join("bash/palette-profile-diff");
    let deps_ok = Command::new("python3")
        .args(["-c", "import yaml"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
        && Command::new("jq")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
    if !deps_ok || !bash_tool.exists() {
        eprintln!("skipping parity test: bash reference dependencies missing");
        return;
    }
    let fx = fixtures();
    for name in ["a", "b"] {
        let out = Path::new(&fx.a)
            .parent()
            .unwrap()
            .join(format!("bash-{name}.json"));
        let st = Command::new("bash")
            .arg(&bash_tool)
            .args([
                "snapshot",
                "--from-raw",
                fx.raw.join(name).to_str().unwrap(),
                "-o",
                out.to_str().unwrap(),
            ])
            .env("PPD_QUIET", "1")
            .output()
            .unwrap();
        assert!(
            st.status.success(),
            "bash snapshot failed: {}",
            String::from_utf8_lossy(&st.stderr)
        );
        let rust = json(if name == "a" { &fx.a } else { &fx.b });
        let bash = json(out.to_str().unwrap());
        assert_eq!(
            rust["root"], bash["root"],
            "root differs for fixture {name}"
        );
        assert_eq!(rust["variablesHash"], bash["variablesHash"]);
        for (rp, bp) in rust["packs"]
            .as_array()
            .unwrap()
            .iter()
            .zip(bash["packs"].as_array().unwrap())
        {
            assert_eq!(rp["key"], bp["key"]);
            assert_eq!(rp["hash"], bp["hash"], "pack {} hashes differ", rp["key"]);
        }
    }
}
