//! The scripts of the live example under examples/live/.
//! No network: every host and every command outside this repository is a recording stand-in.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::*;

// ---- examples/live helpers -------------------------------------------------------------------

fn live_script(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/live")
        .join(name)
}

fn executable(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// A `tofu` that reports the host and whether a key arrived, never the key itself.
const FAKE_TOFU: &str = r#"#!/usr/bin/env bash
echo "host=${SPECTROCLOUD_HOST-<unset>} key=${SPECTROCLOUD_APIKEY:+set} args=$*"
"#;

fn tf_sh(envs: &[(&str, &str)], palette_env: Option<&str>) -> Output {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    executable(&bin.join("tofu"), FAKE_TOFU);
    let home = dir.path().join("home");
    std::fs::create_dir(&home).unwrap();
    if let Some(text) = palette_env {
        std::fs::write(home.join(".palette.env"), text).unwrap();
    }
    let mut c = Command::new("bash");
    c.arg(live_script("tf.sh"))
        .arg("output")
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .env("HOME", &home)
        .env_remove("SPECTROCLOUD_APIKEY")
        .env_remove("SPECTROCLOUD_HOST")
        .env_remove("PALETTE_API_KEY")
        .env_remove("PALETTE_API");
    for (k, v) in envs {
        c.env(k, v);
    }
    let o = c.output().unwrap();
    drop(dir);
    o
}

#[test]
fn tf_sh_passes_the_key_and_a_self_hosted_address_to_tofu() {
    let o = tf_sh(&[("PALETTE_API_KEY", "k")], None);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert_eq!(
        text(&o.stdout).trim(),
        "host=<unset> key=set args=-chdir=".to_string()
            + live_script("terraform").to_str().unwrap()
            + " output",
        "SaaS: no host, so the provider uses its default"
    );

    let o = tf_sh(
        &[
            ("PALETTE_API_KEY", "k"),
            ("PALETTE_API", "https://palette.example.com/"),
        ],
        None,
    );
    assert!(
        text(&o.stdout).starts_with("host=palette.example.com key=set"),
        "{}",
        text(&o.stdout)
    );

    // both read from ~/.palette.env, as strata reads them
    let o = tf_sh(
        &[],
        Some("PALETTE_API_KEY=k\nexport PALETTE_API=\"http://palette.internal:8080\"\n"),
    );
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        text(&o.stdout).starts_with("host=palette.internal:8080 key=set"),
        "{}",
        text(&o.stdout)
    );

    // an explicit SPECTROCLOUD_HOST wins
    let o = tf_sh(
        &[
            ("PALETTE_API_KEY", "k"),
            ("PALETTE_API", "https://a.example.com"),
            ("SPECTROCLOUD_HOST", "b.example.com"),
        ],
        None,
    );
    assert!(
        text(&o.stdout).starts_with("host=b.example.com"),
        "{}",
        text(&o.stdout)
    );

    let o = tf_sh(&[], None);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("PALETTE_API_KEY"),
        "{}",
        text(&o.stderr)
    );
}

/// A `curl` that answers from files: the status in $FAKE_CURL_STATUS, the body in $FAKE_CURL_BODY.
/// It honors `-o <file>` and `-w '%{http_code}'`, the two options tenant.sh relies on, and logs
/// each URL.
const FAKE_CURL: &str = r#"#!/usr/bin/env bash
out=""; write=""
while [ $# -gt 0 ]; do
  case "$1" in
    -o) out=$2; shift 2 ;;
    -w) write=$2; shift 2 ;;
    -H|-X|-d) shift 2 ;;
    -*) shift ;;
    *) echo "$1" >> "$FAKE_CURL_LOG"; shift ;;
  esac
done
if [ -n "$out" ]; then cp "$FAKE_CURL_BODY" "$out"; else cat "$FAKE_CURL_BODY"; fi
[ -n "$write" ] && printf '%s' "$(cat "$FAKE_CURL_STATUS")"
exit 0
"#;

fn tenant_cluster(status: &str, body: &str) -> (Output, String) {
    tenant_sh(&["cluster", "c-123"], status, body)
}

/// Run tenant.sh with one canned answer, and report what it asked for.
fn tenant_sh(args: &[&str], status: &str, body: &str) -> (Output, String) {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    executable(&bin.join("curl"), FAKE_CURL);
    let (status_file, body_file, log) = (
        dir.path().join("status"),
        dir.path().join("body"),
        dir.path().join("curl.log"),
    );
    std::fs::write(&status_file, status).unwrap();
    std::fs::write(&body_file, body).unwrap();
    let o = Command::new("bash")
        .arg(live_script("tenant.sh"))
        .args(args)
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .env("HOME", dir.path())
        .env("PALETTE_API_KEY", "k")
        .env("PALETTE_PROJECT_UID", "p-1")
        .env("FAKE_CURL_STATUS", &status_file)
        .env("FAKE_CURL_BODY", &body_file)
        .env("FAKE_CURL_LOG", &log)
        .output()
        .unwrap();
    let urls = std::fs::read_to_string(&log).unwrap_or_default();
    (o, urls)
}

#[test]
fn tenant_sh_listing_prints_the_cluster_listing_palette_answers() {
    let body =
        r#"{"items":[{"metadata":{"name":"fred","uid":"c-123"},"status":{"state":"Running"}}]}"#;
    let (o, urls) = tenant_sh(&["listing"], "200", body);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert!(
        urls.contains("/v1/dashboard/spectroclusters/meta"),
        "the listing strata lists clusters from: {urls}"
    );
    let printed: serde_json::Value = serde_json::from_str(&text(&o.stdout)).unwrap();
    assert_eq!(
        printed["items"][0]["metadata"]["uid"], "c-123",
        "the answer reaches the caller unchanged, so a run records what Palette sent"
    );

    let (o, _) = tenant_sh(&["listing"], "403", "{}");
    assert_eq!(
        o.status.code(),
        Some(2),
        "a refusal is an error, not an empty listing"
    );
    assert!(text(&o.stderr).contains("HTTP 403"), "{}", text(&o.stderr));
}

#[test]
fn tenant_sh_reports_cluster_state_and_never_reads_a_gateway_404_as_deleted() {
    let (o, urls) = tenant_cluster(
        "200",
        r#"{"metadata":{"name":"fred"},"status":{"state":"Running"}}"#,
    );
    assert_eq!(o.status.code(), Some(1), "still there: {}", text(&o.stdout));
    assert_eq!(text(&o.stdout).trim(), "fred c-123: Running");
    assert!(urls.contains("/v1/spectroclusters/c-123"), "{urls}");
    assert!(
        urls.starts_with("https://api.spectrocloud.com/"),
        "SaaS by default: {urls}"
    );

    let (o, _) = tenant_cluster(
        "200",
        r#"{"metadata":{"name":"fred"},"status":{"state":"Deleted"}}"#,
    );
    assert_eq!(
        o.status.code(),
        Some(0),
        "a deleted cluster answers 200 with state Deleted"
    );
    assert_eq!(text(&o.stdout).trim(), "fred c-123: Deleted");

    let (o, _) = tenant_cluster(
        "200",
        r#"{"metadata":{"name":"barney"},"status":{"state":"Provisioning","conditions":[
            {"type":"KubeConfigReady","status":"True","message":"Kubeconfig file is available."},
            {"type":"WorkerNodeAdditionDone","status":"False","message":"provision worker nodes"}]}}"#,
    );
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(
        text(&o.stdout),
        "barney c-123: Provisioning\n  not done: WorkerNodeAdditionDone: provision worker nodes\n",
        "a cluster that is not Running says which conditions hold it"
    );

    let (o, _) = tenant_cluster(
        "404",
        r#"{"code":"ResourceNotFound","message":"not found"}"#,
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert!(text(&o.stdout).contains("not found"), "{}", text(&o.stdout));

    let (o, _) = tenant_cluster("404", "404 page not found");
    assert_eq!(
        o.status.code(),
        Some(2),
        "a plain-text 404 is a gateway problem, not a deletion"
    );
    assert!(
        text(&o.stderr).contains("not proof of deletion"),
        "{}",
        text(&o.stderr)
    );

    let (o, _) = tenant_cluster("500", "boom");
    assert_eq!(o.status.code(), Some(2));
}

/// A `curl` that answers by path: the body of `<api path with / as _>` under $FAKE_ROUTES, with
/// status 200, or 404 when there is no such file. A `<key>.status` file overrides the status.
const FAKE_ROUTED_CURL: &str = r#"#!/usr/bin/env bash
out=""; write=""; url=""
while [ $# -gt 0 ]; do
  case "$1" in
    -o) out=$2; shift 2 ;;
    -w) write=$2; shift 2 ;;
    -H|-X|-d) shift 2 ;;
    -*) shift ;;
    *) url=$1; shift ;;
  esac
done
path=${url#*://*/}; key=${path//\//_}
body="$FAKE_ROUTES/$key"; status=404
[ -f "$body" ] && status=200 || body=/dev/null
[ -f "$FAKE_ROUTES/$key.status" ] && status=$(cat "$FAKE_ROUTES/$key.status")
if [ -n "$out" ]; then cp "$body" "$out"; else cat "$body"; fi
[ -n "$write" ] && printf '%s' "$status"
exit 0
"#;

/// A `helm` whose `pull ... -d DIR` writes DIR/<name>-<version>.tgz holding "chart archive\n",
/// or fails when $FAKE_HELM_FAIL is set. It logs its arguments and the Helm home it was given.
const FAKE_HELM: &str = r#"#!/usr/bin/env bash
echo "$* HELM_CONFIG_HOME=$HELM_CONFIG_HOME" >> "$FAKE_LOG"
[ -z "${FAKE_HELM_FAIL:-}" ] || { echo "Error: chart not found" >&2; exit 1; }
name=""; version=""; dest=""
shift
while [ $# -gt 0 ]; do
  case "$1" in
    --repo) shift 2 ;;
    --version) version=$2; shift 2 ;;
    -d) dest=$2; shift 2 ;;
    *) name=$1; shift ;;
  esac
done
printf 'chart archive\n' > "$dest/$name-$version.tgz"
"#;

/// A `crane` whose `digest REF` prints a fixed digest, or fails when $FAKE_CRANE_FAIL is set.
const FAKE_CRANE: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_LOG"
[ -z "${FAKE_CRANE_FAIL:-}" ] || { echo "UNAUTHORIZED: authentication required" >&2; exit 1; }
echo "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
"#;

/// sha256 of "chart archive\n", the archive the fake helm writes.
const FAKE_CHART_SHA256: &str = "c7b3bbf2a53bd8a37070529402e843305d2cd0a95abd86cb31f9f3d2629382ed";

struct Digest {
    out: Output,
    log: String,
    /// The TMPDIR tenant.sh ran with, and the entries it left there.
    tmp: String,
    tmp_left: usize,
}

fn tenant_digest(args: &[&str], routes: &[(&str, &str)], envs: &[(&str, &str)]) -> Digest {
    let dir = tempfile::tempdir().unwrap();
    let (bin, route_dir, tmp) = (
        dir.path().join("bin"),
        dir.path().join("routes"),
        dir.path().join("tmp"),
    );
    for d in [&bin, &route_dir, &tmp] {
        std::fs::create_dir(d).unwrap();
    }
    executable(&bin.join("curl"), FAKE_ROUTED_CURL);
    executable(&bin.join("helm"), FAKE_HELM);
    executable(&bin.join("crane"), FAKE_CRANE);
    for (key, body) in routes {
        std::fs::write(route_dir.join(key), body).unwrap();
    }
    let log = dir.path().join("tools.log");
    let mut cmd = Command::new("bash");
    cmd.arg(live_script("tenant.sh"))
        .arg("digest")
        .args(args)
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .env("HOME", dir.path())
        .env("TMPDIR", &tmp)
        .env("PALETTE_API_KEY", "k")
        .env("PALETTE_PROJECT_UID", "p-1")
        .env("FAKE_ROUTES", &route_dir)
        .env("FAKE_LOG", &log);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().unwrap();
    Digest {
        out,
        log: std::fs::read_to_string(&log).unwrap_or_default(),
        tmp: tmp.display().to_string(),
        tmp_left: std::fs::read_dir(&tmp).unwrap().count(),
    }
}

const HELM_REGISTRY: (&str, &str) = (
    "v1_registries_helm_r-helm",
    r#"{"spec":{"endpoint":"https://charts.example.com/repo","auth":{"type":"noAuth"}}}"#,
);
const OCI_SUMMARY: (&str, &str) = (
    "v1_registries_oci_summary",
    r#"{"items":[
        {"metadata":{"uid":"r-oci","name":"packs"},
         "spec":{"endpoint":"https://registry.example.com/","registryType":"basic","providerType":"pack"}},
        {"metadata":{"uid":"r-charts","name":"charts"},
         "spec":{"endpoint":"registry.example.com","registryType":"basic","providerType":"helm"}}]}"#,
);

#[test]
fn tenant_sh_digest_reads_the_chart_archive_and_the_pack_manifest_from_the_registry() {
    let d = tenant_digest(
        &["helm", "r-helm", "nginx", "25.1.5"],
        &[HELM_REGISTRY],
        &[],
    );
    assert_eq!(d.out.status.code(), Some(0), "{}", text(&d.out.stderr));
    assert_eq!(
        text(&d.out.stdout).trim(),
        FAKE_CHART_SHA256,
        "the sha256 of the archive, bare hex as a Helm index records it"
    );
    assert!(
        d.log
            .contains("pull --repo https://charts.example.com/repo nginx --version 25.1.5 -d "),
        "{}",
        d.log
    );
    let home = d.log.split("HELM_CONFIG_HOME=").nth(1).unwrap_or("");
    assert!(
        home.starts_with(&format!("{}/", d.tmp)),
        "helm runs with a private home under TMPDIR, not the user's: {}",
        d.log
    );
    assert_eq!(d.tmp_left, 0, "the pulled chart is removed");

    let d = tenant_digest(
        &["oci", "r-oci", "argocd", "10.4.0.1"],
        &[
            OCI_SUMMARY,
            (
                "v1_registries_oci_r-oci_basic",
                r#"{"spec":{"baseContentPath":"/team/"}}"#,
            ),
        ],
        &[],
    );
    assert_eq!(d.out.status.code(), Some(0), "{}", text(&d.out.stderr));
    assert_eq!(
        text(&d.out.stdout).trim(),
        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    );
    assert_eq!(
        d.log.trim(),
        "digest registry.example.com/team/spectro-packs/archive/argocd:10.4.0.1"
    );
    assert_eq!(d.tmp_left, 0);

    let d = tenant_digest(
        &["oci", "r-oci", "argocd", "10.4.0.1"],
        &[
            OCI_SUMMARY,
            ("v1_registries_oci_r-oci_basic", r#"{"spec":{}}"#),
        ],
        &[],
    );
    assert_eq!(d.out.status.code(), Some(0), "{}", text(&d.out.stderr));
    assert_eq!(
        d.log.trim(),
        "digest registry.example.com/spectro-packs/archive/argocd:10.4.0.1",
        "no base path"
    );
}

#[test]
fn tenant_sh_digest_exits_2_when_the_registry_cannot_be_read() {
    let expect_2 = |d: &Digest, needle: &str| {
        assert_eq!(d.out.status.code(), Some(2), "{}", text(&d.out.stdout));
        assert!(
            text(&d.out.stderr).contains(needle),
            "wanted {needle:?}: {}",
            text(&d.out.stderr)
        );
        assert!(text(&d.out.stdout).trim().is_empty(), "no digest printed");
        assert_eq!(d.tmp_left, 0, "cleaned up after {needle:?}");
    };

    let d = tenant_digest(
        &["helm", "r-helm", "nginx", "25.1.5"],
        &[(
            "v1_registries_helm_r-helm",
            r#"{"spec":{"endpoint":"https://charts.example.com","auth":{"type":"basic"}}}"#,
        )],
        &[],
    );
    expect_2(&d, "needs credentials");
    assert!(d.log.is_empty(), "helm is not run: {}", d.log);

    let d = tenant_digest(
        &["helm", "r-helm", "nginx", "25.1.5"],
        &[HELM_REGISTRY],
        &[("FAKE_HELM_FAIL", "1")],
    );
    expect_2(
        &d,
        "helm pull nginx 25.1.5 from https://charts.example.com/repo failed",
    );

    let d = tenant_digest(
        &["helm", "r-helm", "nginx", "25.1.5"],
        &[HELM_REGISTRY, ("v1_registries_helm_r-helm.status", "500")],
        &[],
    );
    expect_2(&d, "HTTP 500");

    let d = tenant_digest(
        &["oci", "r-charts", "podinfo", "6.0.0"],
        &[OCI_SUMMARY],
        &[],
    );
    expect_2(&d, "holds helm, not packs");
    assert!(d.log.is_empty(), "crane is not run: {}", d.log);

    let d = tenant_digest(
        &["oci", "r-missing", "argocd", "1.0.0"],
        &[OCI_SUMMARY],
        &[],
    );
    expect_2(&d, "no OCI registry r-missing");

    let d = tenant_digest(
        &["oci", "r-oci", "argocd", "10.4.0.1"],
        &[
            OCI_SUMMARY,
            ("v1_registries_oci_r-oci_basic", r#"{"spec":{}}"#),
        ],
        &[("FAKE_CRANE_FAIL", "1")],
    );
    expect_2(&d, "log in to the registry first");

    expect_2(&tenant_digest(&["helm", "r-helm"], &[], &[]), "usage");
    expect_2(&tenant_digest(&["git", "r", "n", "1"], &[], &[]), "usage");
}

/// A `curl` for api.sh: FAKE_ROUTED_CURL that also logs each request as `METHOD path` to
/// $FAKE_LOG, keeps each request body as $FAKE_ROUTES/sent/<METHOD>_<route key>, and its query and
/// headers, without the key, as $FAKE_ROUTES/sent/<METHOD>_<route key>.request. It also adds each
/// body to $FAKE_ROUTES/sent/<METHOD>_<route key>.all, in the order of the requests. When an
/// executable $FAKE_ROUTES/<route key>.sh exists, its output is the body of each answer.
const FAKE_API_CURL: &str = r#"#!/usr/bin/env bash
out=""; write=""; url=""; method=GET; sent=""; headers=""
while [ $# -gt 0 ]; do
  case "$1" in
    -o) out=$2; shift 2 ;;
    -w) write=$2; shift 2 ;;
    -X) method=$2; shift 2 ;;
    --data-binary) sent=${2#@}; shift 2 ;;
    -H) headers="$headers$2"$'\n'; shift 2 ;;
    -d) shift 2 ;;
    -*) shift ;;
    *) url=$1; shift ;;
  esac
done
path=${url#*://*/}; query=""; [ "$path" != "${path#*\?}" ] && query=${path#*\?}; path=${path%%\?*}; key=${path//\//_}
echo "$method /$path" >> "$FAKE_LOG"
mkdir -p "$FAKE_ROUTES/sent"
[ -n "$sent" ] && cp "$sent" "$FAKE_ROUTES/sent/${method}_$key" && cat "$sent" >> "$FAKE_ROUTES/sent/${method}_$key.all"
printf 'query=%s\n%s' "$query" "$headers" | grep -v '^ApiKey:' > "$FAKE_ROUTES/sent/${method}_$key.request"
body="$FAKE_ROUTES/$key"; status=404
[ -x "$body.sh" ] && "$body.sh" > "$body"
[ -f "$body" ] && status=200 || body=/dev/null
[ -f "$FAKE_ROUTES/$key.status" ] && status=$(cat "$FAKE_ROUTES/$key.status")
if [ -n "$out" ]; then cp "$body" "$out"; else cat "$body"; fi
[ -n "$write" ] && printf '%s' "$status"
exit 0
"#;

#[test]
fn api_sh_edit_keeps_each_manifest_uid_so_clusters_keep_their_copies() {
    let dir = tempfile::tempdir().unwrap();
    let (bin, routes) = (dir.path().join("bin"), dir.path().join("routes"));
    for d in [&bin, &routes] {
        std::fs::create_dir(d).unwrap();
    }
    executable(&bin.join("curl"), FAKE_API_CURL);
    std::fs::write(
        routes.join("v1_clusterprofiles_prof-1"),
        r#"{"metadata":{"name":"addon","uid":"prof-1"},"spec":{"version":"1.0.0","published":{
            "type":"add-on","cloudType":"all","packs":[{"name":"config","tag":"1.0.0",
            "type":"manifest","layer":"addon","packUid":"pk-1","values":""}]}}}"#,
    )
    .unwrap();
    std::fs::write(
        routes.join("v1_clusterprofiles_prof-1_packs_manifests"),
        r#"{"metadata":{},"spec":{"packs":[{"metadata":{},"spec":{"name":"config","manifests":[
            {"name":"config","uid":"m-1","digest":"d","content":"edited: \"no\"\n"}]}}]}}"#,
    )
    .unwrap();
    std::fs::write(routes.join("v1_clusterprofiles_prof-1_publish"), "{}").unwrap();
    let log = dir.path().join("curl.log");
    let o = Command::new("bash")
        .arg(live_script("api.sh"))
        .args(["edit", "prof-1", r#"edited: "no""#, r#"edited: "yes""#])
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .env("HOME", dir.path())
        .env("PALETTE_API_KEY", "k")
        .env("PALETTE_PROJECT_UID", "p-1")
        .env("FAKE_ROUTES", &routes)
        .env("FAKE_LOG", &log)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let calls = std::fs::read_to_string(&log).unwrap();
    assert!(
        calls.contains("PATCH /v1/clusterprofiles/prof-1/publish"),
        "{calls}"
    );
    let put: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(routes.join("sent/PUT_v1_clusterprofiles_prof-1")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        put["spec"]["template"]["packs"][0]["manifests"],
        serde_json::json!([{"name": "config", "uid": "m-1", "content": "edited: \"yes\"\n"}]),
        "a manifest sent without its uid is a new manifest, and a cluster's copy of the old one is dropped when the cluster's update is accepted"
    );
}

#[test]
fn api_sh_draft_puts_the_change_with_no_publish_and_publish_publishes_it() {
    let dir = tempfile::tempdir().unwrap();
    let (bin, routes) = (dir.path().join("bin"), dir.path().join("routes"));
    for d in [&bin, &routes] {
        std::fs::create_dir(d).unwrap();
    }
    executable(&bin.join("curl"), FAKE_API_CURL);
    std::fs::write(
        routes.join("v1_clusterprofiles_prof-1"),
        r#"{"metadata":{"name":"addon","uid":"prof-1"},"spec":{"version":"1.0.0","published":{
            "type":"add-on","cloudType":"all","packs":[{"name":"config","tag":"1.0.0",
            "type":"manifest","layer":"addon","packUid":"pk-1","values":""}]}}}"#,
    )
    .unwrap();
    std::fs::write(
        routes.join("v1_clusterprofiles_prof-1_packs_manifests"),
        r#"{"metadata":{},"spec":{"packs":[{"metadata":{},"spec":{"name":"config","manifests":[
            {"name":"config","uid":"m-1","digest":"d","content":"edited: \"no\"\n"}]}}]}}"#,
    )
    .unwrap();
    std::fs::write(routes.join("v1_clusterprofiles_prof-1_publish"), "{}").unwrap();
    let log = dir.path().join("curl.log");
    let api = |args: &[&str]| {
        Command::new("bash")
            .arg(live_script("api.sh"))
            .args(args)
            .env(
                "PATH",
                format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
            )
            .env("HOME", dir.path())
            .env("PALETTE_API_KEY", "k")
            .env("PALETTE_PROJECT_UID", "p-1")
            .env("FAKE_ROUTES", &routes)
            .env("FAKE_LOG", &log)
            .output()
            .unwrap()
    };
    let o = api(&["draft", "prof-1", r#"edited: "no""#, r#"edited: "yes""#]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let calls = std::fs::read_to_string(&log).unwrap();
    assert!(calls.contains("PUT /v1/clusterprofiles/prof-1"), "{calls}");
    assert!(
        !calls.contains("publish"),
        "a draft is not published: {calls}"
    );
    let put: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(routes.join("sent/PUT_v1_clusterprofiles_prof-1")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        put["spec"]["template"]["packs"][0]["manifests"],
        serde_json::json!([{"name": "config", "uid": "m-1", "content": "edited: \"yes\"\n"}]),
    );

    let o = api(&["publish", "prof-1"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let calls = std::fs::read_to_string(&log).unwrap();
    assert!(
        calls.ends_with("PATCH /v1/clusterprofiles/prof-1/publish\n"),
        "{calls}"
    );
}

#[test]
fn api_sh_delete_named_deletes_every_version_of_exactly_that_name() {
    let dir = tempfile::tempdir().unwrap();
    let (bin, routes) = (dir.path().join("bin"), dir.path().join("routes"));
    for d in [&bin, &routes] {
        std::fs::create_dir(d).unwrap();
    }
    executable(&bin.join("curl"), FAKE_API_CURL);
    std::fs::write(
        routes.join("v1_dashboard_clusterprofiles_metadata"),
        r#"{"items":[
            {"metadata":{"name":"strata-live-addon","uid":"u1"},"spec":{"version":"1.0.0"}},
            {"metadata":{"name":"strata-live-addon","uid":"u2"},"spec":{"version":"1.0.1"}},
            {"metadata":{"name":"strata-live-addon-2","uid":"u3"},"spec":{"version":"1.0.0"}},
            {"metadata":{"name":"strata-live-profile","uid":"u4"},"spec":{"version":"1.0.0"}}
        ]}"#,
    )
    .unwrap();
    for uid in ["u1", "u2", "u3", "u4"] {
        std::fs::write(routes.join(format!("v1_clusterprofiles_{uid}")), "{}").unwrap();
    }
    let log = dir.path().join("curl.log");
    let o = Command::new("bash")
        .arg(live_script("api.sh"))
        .args(["delete-named", "strata-live-addon", "strata-live-move"])
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .env("HOME", dir.path())
        .env("PALETTE_API_KEY", "k")
        .env("PALETTE_PROJECT_UID", "p-1")
        .env("FAKE_ROUTES", &routes)
        .env("FAKE_LOG", &log)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let deletes: Vec<String> = std::fs::read_to_string(&log)
        .unwrap()
        .lines()
        .filter(|l| l.starts_with("DELETE "))
        .map(str::to_string)
        .collect();
    assert_eq!(
        deletes,
        [
            "DELETE /v1/clusterprofiles/u1",
            "DELETE /v1/clusterprofiles/u2"
        ],
        "only the exact name, never a name that starts with it"
    );
    assert!(
        text(&o.stdout).contains("strata-live-move: no versions"),
        "{}",
        text(&o.stdout)
    );
}

/// A `kubectl` that logs its arguments and the kubeconfig it was given, and answers only when
/// that kubeconfig is the one the fake Palette serves.
const FAKE_KUBECTL: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_KUBECTL_LOG"
grep -q 'fake-kubeconfig' "${KUBECONFIG:-/dev/null}" 2>/dev/null || { echo "no kubeconfig" >&2; exit 1; }
echo "kubectl says: $*"
"#;

/// The fake Palette routes for one stuck cluster, `c-1` named barney.
fn stuck_cluster_routes(routes: &Path, kubeconfig: bool) {
    std::fs::create_dir_all(routes).unwrap();
    std::fs::write(
        routes.join("v1_spectroclusters_c-1"),
        r#"{"metadata":{"name":"barney"},"status":{"state":"Provisioning","conditions":[
            {"type":"WorkerNodeAdditionDone","status":"False","message":"provision worker nodes"}]}}"#,
    )
    .unwrap();
    std::fs::write(
        routes.join("v1_events_components_spectrocluster_c-1"),
        r#"{"items":[{"reason":"CSIInstallFailed","message":"CSI install failed"}]}"#,
    )
    .unwrap();
    if kubeconfig {
        std::fs::write(
            routes.join("v1_spectroclusters_c-1_assets_kubeconfig"),
            "apiVersion: v1 # fake-kubeconfig\n",
        )
        .unwrap();
    }
}

fn run_live(script: &str, args: &[&str], root: &Path, extra: &[(&str, &str)]) -> Output {
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    executable(&bin.join("curl"), FAKE_API_CURL);
    executable(&bin.join("kubectl"), FAKE_KUBECTL);
    let tmp = root.join("tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    let mut c = Command::new("bash");
    c.arg(live_script(script))
        .args(args)
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .env("HOME", root)
        .env("TMPDIR", &tmp)
        .env("PALETTE_API_KEY", "k")
        .env("PALETTE_PROJECT_UID", "p-1")
        .env("FAKE_ROUTES", root.join("routes"))
        .env("FAKE_LOG", root.join("curl.log"))
        .env("FAKE_KUBECTL_LOG", root.join("kubectl.log"))
        .env_remove("AWS_PROFILE");
    for (k, v) in extra {
        c.env(k, v);
    }
    c.output().unwrap()
}

/// Every file under `dir`, as (path relative to dir, contents).
fn files_under(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p.strip_prefix(dir).unwrap().display().to_string();
                out.push((rel, std::fs::read_to_string(&p).unwrap_or_default()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn tenant_sh_diagnose_saves_what_holds_a_cluster_without_keeping_its_kubeconfig() {
    let root = tempfile::tempdir().unwrap();
    stuck_cluster_routes(&root.path().join("routes"), true);
    let out = root.path().join("diag");
    let o = run_live(
        "tenant.sh",
        &["diagnose", "c-1", out.to_str().unwrap()],
        root.path(),
        &[],
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let files = files_under(&out);
    let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    for want in [
        "cluster.json",
        "events.json",
        "state.txt",
        "nodes.txt",
        "machines.txt",
        "pods.txt",
        "kube-events.txt",
        "logs/palette-controller-manager.txt",
        "logs/cluster-management-agent.txt",
    ] {
        assert!(names.contains(&want), "missing {want}: {names:?}");
    }
    let state = &files.iter().find(|(n, _)| n == "state.txt").unwrap().1;
    assert!(
        state.contains("not done: WorkerNodeAdditionDone: provision worker nodes"),
        "{state}"
    );
    assert!(
        files
            .iter()
            .all(|(_, body)| !body.contains("fake-kubeconfig")),
        "the kubeconfig carries credentials and is never saved: {names:?}"
    );
    assert!(!names.contains(&"errors.txt"), "nothing failed: {names:?}");
    let sent = |key: &str| {
        std::fs::read_to_string(root.path().join("routes/sent").join(key)).unwrap_or_default()
    };
    let kubeconfig_request = sent("GET_v1_spectroclusters_c-1_assets_kubeconfig.request");
    assert!(
        !kubeconfig_request.contains("application/json"),
        "asked for JSON, Palette answers {{\"Reader\":{{}}}} instead of the kubeconfig: {kubeconfig_request}"
    );
    let events_request = sent("GET_v1_events_components_spectrocluster_c-1.request");
    assert!(
        events_request.contains("query=limit=100"),
        "Palette answers 500 for more than 100 events a page: {events_request}"
    );
    let kubectl = std::fs::read_to_string(root.path().join("kubectl.log")).unwrap();
    assert!(
        kubectl.contains("-n cluster-c-1 logs deploy/palette-controller-manager --all-containers"),
        "{kubectl}"
    );
    assert_eq!(
        std::fs::read_dir(root.path().join("tmp")).unwrap().count(),
        0,
        "the temporary kubeconfig is removed"
    );
}

#[test]
fn tenant_sh_diagnose_keeps_what_palette_gives_when_the_cluster_cannot_be_read_from_inside() {
    let root = tempfile::tempdir().unwrap();
    stuck_cluster_routes(&root.path().join("routes"), false);
    let out = root.path().join("diag");
    let o = run_live(
        "tenant.sh",
        &["diagnose", "c-1", out.to_str().unwrap()],
        root.path(),
        &[],
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let files = files_under(&out);
    let errors = &files
        .iter()
        .find(|(n, _)| n == "errors.txt")
        .expect("errors.txt names what could not be read")
        .1;
    assert!(errors.contains("kubeconfig: HTTP 404"), "{errors}");
    assert!(files.iter().any(|(n, _)| n == "cluster.json"));
    assert!(
        !root.path().join("kubectl.log").exists(),
        "kubectl is not run without a kubeconfig"
    );

    let o = run_live(
        "tenant.sh",
        &["diagnose", "c-missing", out.to_str().unwrap()],
        root.path(),
        &[],
    );
    assert_eq!(
        o.status.code(),
        Some(2),
        "a cluster Palette cannot read is an error"
    );
}

/// A `tofu` whose `show -json` holds clusters A and B.
const FAKE_TOFU_STATE: &str = r#"#!/usr/bin/env bash
case "$*" in
  *"show -json"*) cat <<'JSON'
{"values":{"outputs":{"project_uid":{"value":"p-1"}},"root_module":{"resources":[
  {"type":"spectrocloud_cluster_profile","values":{"id":"prof-1","name":"strata-live-profile"}},
  {"type":"spectrocloud_cluster_aws","values":{"id":"c-1","name":"barney","cloud_config":[{"region":"us-east-2"}]}},
  {"type":"spectrocloud_cluster_aws","values":{"id":"c-2","name":"fred","cloud_config":[{"region":"us-east-2"}]}}
]}}}
JSON
  ;;
  *) exit 1 ;;
esac
"#;

#[test]
fn diagnose_sh_saves_diagnostics_for_every_cluster_in_the_tofu_state() {
    let root = tempfile::tempdir().unwrap();
    stuck_cluster_routes(&root.path().join("routes"), true);
    std::fs::write(
        root.path().join("routes/v1_spectroclusters_c-2"),
        r#"{"metadata":{"name":"fred"},"status":{"state":"Running"}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(root.path().join("bin")).unwrap();
    executable(&root.path().join("bin/tofu"), FAKE_TOFU_STATE);
    let out = root.path().join("diag");
    let o = run_live("diagnose.sh", &[out.to_str().unwrap()], root.path(), &[]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert!(out.join("barney/cluster.json").exists());
    assert!(out.join("fred/cluster.json").exists());
    let barney = std::fs::read_to_string(out.join("barney/state.txt")).unwrap();
    assert!(barney.contains("barney c-1: Provisioning"), "{barney}");
}

/// A `tofu` for baseline.sh: outputs for cluster A `c-a` and four profile versions, a plan
/// that exits $FAKE_PLAN_EXIT, and a state that manages r-1, r-2, g-1, and ad-1.
const FAKE_TOFU_BASELINE: &str = r#"#!/usr/bin/env bash
case "$*" in
  *"output -json"*) cat <<'JSON'
{"project_uid":{"value":"p-1"},"cluster_uid":{"value":"c-a"},"cluster_b_uid":{"value":""},
 "profile_name":{"value":"strata-live-profile"},"release_profile_uid":{"value":"r-1"},
 "addon_profile_uid":{"value":"ad-1"},"addon_profile_name":{"value":"strata-live-addon"},
 "profile_names":{"value":["strata-live-profile","strata-live-addon","strata-live-move"]},
 "cluster_names":{"value":{"a":"fred","b":"barney"}}}
JSON
  ;;
  *"plan"*)
    code=${FAKE_PLAN_EXIT:-0}
    [ "$code" = 2 ] && echo "  # spectrocloud_cluster_profile.addon will be updated in-place"
    [ "$code" = 0 ] && echo "No changes."
    exit "$code" ;;
  *"show -json"*) cat <<'JSON'
{"values":{"root_module":{"resources":[
  {"type":"spectrocloud_cluster_profile","values":{"id":"r-1","name":"strata-live-profile"}},
  {"type":"spectrocloud_cluster_profile","values":{"id":"r-2","name":"strata-live-profile"}},
  {"type":"spectrocloud_cluster_profile","values":{"id":"ad-1","name":"strata-live-addon"}},
  {"type":"spectrocloud_cluster_aws","values":{"id":"c-a","name":"fred"}}
]}}}
JSON
  ;;
  *) exit 1 ;;
esac
"#;

/// A `strata` for baseline.sh: listings from the test's environment, and a compare that exits
/// $FAKE_COMPARE_EXIT.
const FAKE_STRATA_BASELINE: &str = r#"#!/usr/bin/env bash
case "$*" in
  "list clusters --json") echo "{\"clusters\":[{\"name\":\"fred\",\"uid\":\"c-a\"}${FAKE_CLUSTER_B:+,{\"name\":\"barney\",\"uid\":\"c-b\"\}}]}" ;;
  "list profiles --json") echo "{\"profiles\":[{\"name\":\"strata-live-profile\",\"uid\":\"r-1\"},{\"name\":\"strata-live-profile\",\"uid\":\"r-2\"},{\"name\":\"strata-live-addon\",\"uid\":\"ad-1\"}${FAKE_EXTRA_ADDON:+,{\"name\":\"strata-live-addon\",\"uid\":\"ad-2\"\}}]}" ;;
  compare*) echo "x -> y"; echo "root       aaa -> bbb   CHANGED"; exit "${FAKE_COMPARE_EXIT:-0}" ;;
  *) exit 1 ;;
esac
"#;

fn run_baseline(extra: &[(&str, &str)]) -> Output {
    let root = tempfile::tempdir().unwrap();
    let routes = root.path().join("routes");
    std::fs::create_dir_all(&routes).unwrap();
    std::fs::write(
        routes.join("v1_spectroclusters_c-a"),
        r#"{"metadata":{"name":"fred"},"status":{"state":"Running"}}"#,
    )
    .unwrap();
    let bin = root.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    executable(&bin.join("tofu"), FAKE_TOFU_BASELINE);
    executable(&bin.join("fake-strata"), FAKE_STRATA_BASELINE);
    let strata = bin.join("fake-strata").display().to_string();
    let mut env = vec![("STRATA_BIN", strata.as_str())];
    env.extend_from_slice(extra);
    run_live("baseline.sh", &[], root.path(), &env)
}

#[test]
fn baseline_sh_passes_only_when_every_condition_holds_and_names_each_that_does_not() {
    let o = run_baseline(&[]);
    let out = text(&o.stdout);
    assert_eq!(o.status.code(), Some(0), "{out}{}", text(&o.stderr));
    for name in [
        "plan",
        "cluster-a",
        "no-cluster-b",
        "versions",
        "runs-release",
        "runs-addon",
    ] {
        assert!(
            out.lines().any(|l| l.starts_with("ok") && l.contains(name)),
            "{name} holds: {out}"
        );
    }
    assert!(!out.contains("differs"), "{out}");

    let o = run_baseline(&[
        ("FAKE_PLAN_EXIT", "2"),
        ("FAKE_EXTRA_ADDON", "1"),
        ("FAKE_CLUSTER_B", "1"),
        ("FAKE_COMPARE_EXIT", "1"),
    ]);
    let out = text(&o.stdout);
    assert_eq!(o.status.code(), Some(1), "{out}{}", text(&o.stderr));
    for want in [
        "spectrocloud_cluster_profile.addon will be updated in-place",
        "barney exists: c-b",
        "strata-live-addon: Palette has ad-1 ad-2, OpenTofu manages ad-1",
        "root       aaa -> bbb   CHANGED",
    ] {
        assert!(
            out.lines()
                .any(|l| l.starts_with("differs") && l.contains(want)),
            "names {want:?}: {out}"
        );
    }
    assert!(
        out.contains("ok       versions      strata-live-profile: r-1 r-2"),
        "a name that matches still passes: {out}"
    );

    let o = run_baseline(&[("FAKE_PLAN_EXIT", "1")]);
    assert_eq!(
        o.status.code(),
        Some(2),
        "a plan that fails means it cannot tell"
    );
}

/// An `aws` for aws-inventory.sh: it logs each call and answers `<service> <operation>` with the
/// ids in $FAKE_AWS/<operation>, or nothing when that file is absent.
const FAKE_AWS: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_AWS/calls.log"
[ -f "$FAKE_AWS/$2" ] && cat "$FAKE_AWS/$2"
exit 0
"#;

fn aws_inventory(answers: &[(&str, &str)]) -> (Output, String) {
    let root = tempfile::tempdir().unwrap();
    let (bin, fake) = (root.path().join("bin"), root.path().join("aws"));
    for d in [&bin, &fake] {
        std::fs::create_dir_all(d).unwrap();
    }
    executable(&bin.join("aws"), FAKE_AWS);
    for (op, ids) in answers {
        std::fs::write(fake.join(op), ids).unwrap();
    }
    let o = Command::new("bash")
        .arg(live_script("aws-inventory.sh"))
        .args(["fred", "us-east-2"])
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .env("FAKE_AWS", &fake)
        .output()
        .unwrap();
    let calls = std::fs::read_to_string(fake.join("calls.log")).unwrap_or_default();
    (o, calls)
}

#[test]
fn aws_inventory_counts_what_a_cluster_deletion_removes_last() {
    let (o, _) = aws_inventory(&[]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    for label in [
        "instances",
        "load balancers",
        "security groups",
        "subnets",
        "route tables",
        "internet gateways",
        "network interfaces",
        "vpcs",
    ] {
        assert!(
            text(&o.stdout)
                .lines()
                .any(|l| l.starts_with(label) && l.trim_end().ends_with(" 0")),
            "{label}: {}",
            text(&o.stdout)
        );
    }

    // after the instances, NAT gateway, and load balancer are gone, the network is still there
    let (o, calls) = aws_inventory(&[
        ("describe-vpcs", "vpc-1\n"),
        ("describe-security-groups", "sg-1\tsg-2\n"),
        ("describe-subnets", "subnet-1\n"),
        ("describe-route-tables", "rtb-1\n"),
        ("describe-internet-gateways", "igw-1\n"),
        ("describe-network-interfaces", "eni-1\n"),
    ]);
    assert_eq!(
        o.status.code(),
        Some(1),
        "anything left is not gone: {}",
        text(&o.stdout)
    );
    let out = text(&o.stdout);
    assert!(
        out.lines()
            .any(|l| l.starts_with("security groups") && l.trim_end().ends_with(" 2")),
        "{out}"
    );
    assert!(
        calls.contains("describe-network-interfaces --filters Name=vpc-id,Values=vpc-1"),
        "interfaces are found in the cluster's VPC, since they are not all tagged: {calls}"
    );
    assert!(
        calls.contains("describe-security-groups --filters Name=tag-key,Values=sigs.k8s.io/cluster-api-provider-aws/cluster/fred"),
        "{calls}"
    );
}

#[test]
fn tenant_sh_cluster_shows_the_latest_events_of_a_cluster_that_is_not_settled() {
    let root = tempfile::tempdir().unwrap();
    let routes = root.path().join("routes");
    std::fs::create_dir_all(&routes).unwrap();
    std::fs::write(
        routes.join("v1_spectroclusters_c-1"),
        r#"{"metadata":{"name":"barney"},"status":{"state":"Deleting","conditions":[
            {"type":"DeProvisionDone","status":"False","message":"ApplyingClusterSpec"}]}}"#,
    )
    .unwrap();
    std::fs::write(
        routes.join("v1_events_components_spectrocluster_c-1"),
        r#"{"items":[
            {"metadata":{"creationTimestamp":"2026-09-17T03:44:39.000Z"},"reason":"Deleted","message":"Cluster barney has been deleted"},
            {"metadata":{"creationTimestamp":"2026-09-17T03:44:36.000Z"},"reason":"SuccessfulDeleteSecurityGroup","message":"Deleted cluster managed SecurityGroup \"sg-1\""},
            {"metadata":{"creationTimestamp":"2026-09-17T03:33:20.000Z"},"reason":"Deleting","message":"Cluster barney is Deleting"},
            {"metadata":{"creationTimestamp":"2026-09-17T03:44:39.500Z"},"reason":"SuccessfulDeleteVPC","message":"Deleted managed VPC \"vpc-1\""}
        ]}"#,
    )
    .unwrap();
    let o = run_live("tenant.sh", &["cluster", "c-1"], root.path(), &[]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    assert_eq!(
        text(&o.stdout),
        "barney c-1: Deleting\n  not done: DeProvisionDone: ApplyingClusterSpec\n  event 03:44:36 SuccessfulDeleteSecurityGroup: Deleted cluster managed SecurityGroup \"sg-1\"\n  event 03:44:39 Deleted: Cluster barney has been deleted\n  event 03:44:39 SuccessfulDeleteVPC: Deleted managed VPC \"vpc-1\"\n",
        "the three latest events, oldest first, show a deletion that is moving"
    );
}

#[test]
fn api_sh_retag_renames_one_pack_tag_in_place_and_keeps_everything_else() {
    let dir = tempfile::tempdir().unwrap();
    let (bin, routes) = (dir.path().join("bin"), dir.path().join("routes"));
    for d in [&bin, &routes] {
        std::fs::create_dir(d).unwrap();
    }
    executable(&bin.join("curl"), FAKE_API_CURL);
    std::fs::write(
        routes.join("v1_clusterprofiles_prof-1"),
        r#"{"metadata":{"name":"release","uid":"prof-1"},"spec":{"version":"1.0.2","published":{
            "type":"cluster","cloudType":"aws","packs":[
            {"name":"ubuntu-aws","tag":"LTS__24.04","type":"spectro","layer":"os","packUid":"pk-os","registryUid":"reg-1","values":"os: values\n"},
            {"name":"kubernetes","tag":"1.36.2","type":"spectro","layer":"k8s","packUid":"pk-k8s","registryUid":"reg-1","values":"k8s: values\n"}]}}}"#,
    )
    .unwrap();
    std::fs::write(
        routes.join("v1_clusterprofiles_prof-1_packs_manifests"),
        r#"{"spec":{"packs":[{"spec":{"name":"ubuntu-aws","manifests":[]}},{"spec":{"name":"kubernetes","manifests":[]}}]}}"#,
    )
    .unwrap();
    std::fs::write(routes.join("v1_clusterprofiles_prof-1_publish"), "{}").unwrap();
    let run = |args: &[&str]| {
        Command::new("bash")
            .arg(live_script("api.sh"))
            .args(args)
            .env(
                "PATH",
                format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
            )
            .env("HOME", dir.path())
            .env("PALETTE_API_KEY", "k")
            .env("PALETTE_PROJECT_UID", "p-1")
            .env("FAKE_ROUTES", &routes)
            .env("FAKE_LOG", dir.path().join("curl.log"))
            .output()
            .unwrap()
    };
    let o = run(&["retag", "prof-1", "ubuntu-aws", "LTS__24.04", "24.04"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let put: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(routes.join("sent/PUT_v1_clusterprofiles_prof-1")).unwrap(),
    )
    .unwrap();
    let packs = &put["spec"]["template"]["packs"];
    assert_eq!(
        (
            packs[0]["name"].as_str(),
            packs[0]["tag"].as_str(),
            packs[0]["uid"].as_str()
        ),
        (Some("ubuntu-aws"), Some("24.04"), Some("pk-os")),
        "the tag changes and the pack uid stays: {put}"
    );
    assert_eq!(packs[0]["values"], "os: values\n");
    assert_eq!(packs[1]["tag"], "1.36.2", "other packs keep their tags");
    let calls = std::fs::read_to_string(dir.path().join("curl.log")).unwrap();
    assert!(
        calls.contains("PATCH /v1/clusterprofiles/prof-1/publish"),
        "{calls}"
    );

    let o = run(&["retag", "prof-1", "ubuntu-aws", "9.9", "24.04"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("no pack ubuntu-aws with tag 9.9"),
        "{}",
        text(&o.stderr)
    );
}

#[test]
fn api_sh_app_profile_order_puts_the_first_tier_with_the_order_and_reads_the_order_again() {
    let dir = tempfile::tempdir().unwrap();
    let (bin, routes) = (dir.path().join("bin"), dir.path().join("routes"));
    for d in [&bin, &routes] {
        std::fs::create_dir(d).unwrap();
    }
    executable(&bin.join("curl"), FAKE_API_CURL);
    std::fs::write(
        routes.join("v1_appProfiles_ap-1_tiers"),
        r#"{"spec":{"appTiers":[
            {"metadata":{"name":"web","uid":"t-1"},"spec":{"type":"container","version":"1.0.0",
              "values":"replicas: 2\n","containerRegistryUid":"cr-1","installOrder":2,
              "properties":[{"name":"port","type":"number","value":"80"}]}},
            {"metadata":{"name":"config","uid":"t-2"},"spec":{"type":"manifest","version":"1.0.0"}}]}}"#,
    )
    .unwrap();
    std::fs::write(routes.join("v1_appProfiles_ap-1_tiers_t-1"), "{}").unwrap();
    let run = |args: &[&str]| {
        Command::new("bash")
            .arg(live_script("api.sh"))
            .args(args)
            .env(
                "PATH",
                format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
            )
            .env("HOME", dir.path())
            .env("PALETTE_API_KEY", "k")
            .env("FAKE_ROUTES", &routes)
            .env("FAKE_LOG", dir.path().join("curl.log"))
            .output()
            .unwrap()
    };
    let o = run(&["app-profile-order", "ap-1", "2"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let put: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(routes.join("sent/PUT_v1_appProfiles_ap-1_tiers_t-1")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        put,
        serde_json::json!({"name": "web", "version": "1.0.0", "values": "replicas: 2\n",
            "installOrder": 2, "manifests": [], "containerRegistryUid": "cr-1",
            "properties": [{"name": "port", "value": "80"}]}),
        "the order changes, and the values, the properties, and the registry stay"
    );

    // the tier route still gives the order 2 after the update to 3, as Palette does
    let o = run(&["app-profile-order", "ap-1", "3"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    assert!(
        text(&o.stderr).contains("tier t-1 of app profile ap-1 keeps the install order 2"),
        "{}",
        text(&o.stderr)
    );

    let o = run(&["app-profile-order", "ap-1", "first"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(text(&o.stderr).contains("usage"), "{}", text(&o.stderr));
}

#[test]
fn tenant_sh_repave_asks_palette_whether_a_move_to_another_profile_version_repaves() {
    let root = tempfile::tempdir().unwrap();
    let routes = root.path().join("routes");
    std::fs::create_dir_all(&routes).unwrap();
    std::fs::write(
        routes.join("v1_clusterprofiles_prof-2"),
        r#"{"metadata":{"uid":"prof-2"},"spec":{"version":"1.0.2","published":{"packs":[
            {"name":"ubuntu-aws","tag":"LTS__24.04","type":"spectro","layer":"os","values":"os: values\n"},
            {"name":"kubernetes","tag":"1.36.2","type":"spectro","layer":"k8s","values":"postKubeadmCommands:\n  - chmod\n"}]}}}"#,
    )
    .unwrap();
    let validate = routes.join("v1_spectroclusters_c-1_validate_repave");
    std::fs::write(&validate, r#"{"isRepaveRequired":false,"reasons":[]}"#).unwrap();
    let sent = || -> serde_json::Value {
        serde_json::from_str(
            &std::fs::read_to_string(
                routes.join("sent/POST_v1_spectroclusters_c-1_validate_repave"),
            )
            .unwrap(),
        )
        .unwrap()
    };

    let o = run_live(
        "tenant.sh",
        &["repave", "c-1", "prof-1", "prof-2"],
        root.path(),
        &[],
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert_eq!(
        text(&o.stdout),
        "cluster c-1, profile prof-1 -> prof-2: no repave\n"
    );
    assert_eq!(
        sent(),
        serde_json::json!({"profiles": [{"uid": "prof-1", "replaceWithProfile": "prof-2", "variables": [],
        "packValues": [
            {"name": "ubuntu-aws", "tag": "LTS__24.04", "type": "spectro", "values": "os: values\n", "manifests": []},
            {"name": "kubernetes", "tag": "1.36.2", "type": "spectro", "values": "postKubeadmCommands:\n  - chmod\n", "manifests": []}
        ]}]}),
        "the target version's packs replace the current version on the cluster"
    );

    // the control: a change to the values that Palette repaves for
    std::fs::write(
        &validate,
        r#"{"isRepaveRequired":true,"reasons":[{"code":"PackValuesUpdated","message":"Pack 'kubernetes' values are updated",
            "pack":{"diffConfigKeys":["kubeadmconfig.postKubeadmCommands"]}}]}"#,
    )
    .unwrap();
    let o = run_live(
        "tenant.sh",
        &[
            "repave",
            "c-1",
            "prof-1",
            "prof-2",
            "kubernetes",
            "postKubeadmCommands:\n",
            "postKubeadmCommands:\n  - echo strata\n",
        ],
        root.path(),
        &[],
    );
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    assert_eq!(
        text(&o.stdout),
        "cluster c-1, profile prof-1 -> prof-2 with kubernetes values edited: repave\n  PackValuesUpdated: Pack 'kubernetes' values are updated (kubeadmconfig.postKubeadmCommands)\n"
    );
    assert_eq!(
        sent()["profiles"][0]["packValues"][1]["values"],
        "postKubeadmCommands:\n  - echo strata\n  - chmod\n"
    );

    let o = run_live(
        "tenant.sh",
        &[
            "repave",
            "c-1",
            "prof-1",
            "prof-2",
            "kubernetes",
            "absent",
            "x",
        ],
        root.path(),
        &[],
    );
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr)
            .contains("pack kubernetes of prof-2 has no values containing the text to replace"),
        "{}",
        text(&o.stderr)
    );
}

// ---- preflight.sh and preflight-plan.sh ------------------------------------------------------------

fn preflight_script(name: &str, args: &[&str]) -> Output {
    // a strata binary that is newer than the sources, so lib.sh does not stop the script
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("strata");
    executable(&bin, "#!/usr/bin/env bash\necho strata 0.0.0\n");
    Command::new("bash")
        .arg(live_script(name))
        .args(args)
        .env("STRATA_BIN", &bin)
        .env("OUT", dir.path().join("out"))
        .output()
        .unwrap()
}

#[test]
fn preflight_scripts_print_their_usage_and_refuse_an_engine_they_have_no_plan_for() {
    let o = preflight_script("preflight.sh", &["--help"]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    assert!(
        text(&o.stdout).contains("The checks of the lab itself"),
        "{}",
        text(&o.stdout)
    );
    let o = preflight_script("preflight.sh", &["--cloud", "mars"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("no lab for the cloud 'mars'"),
        "{}",
        text(&o.stderr)
    );

    let o = preflight_script("preflight-plan.sh", &["--help"]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    assert!(
        text(&o.stdout).contains("The test plan of #91"),
        "{}",
        text(&o.stdout)
    );
    let o = preflight_script("preflight-plan.sh", &["mars"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("no plan for the engine 'mars'"),
        "{}",
        text(&o.stderr)
    );
    let o = preflight_script("preflight-plan.sh", &[]);
    assert_eq!(o.status.code(), Some(2), "no engine is usage, exit 2");
}

#[test]
fn the_entities_of_the_test_plan_are_the_shape_of_a_create_for_each_engine() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/live/preflight");
    for engine in ["aws", "eks", "gcp", "gke"] {
        let text = std::fs::read_to_string(dir.join(format!("{engine}.json"))).unwrap();
        let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            doc["spec"]["cloudAccountUid"], "__ACCOUNT_UID__",
            "{engine}"
        );
        assert_eq!(
            doc["spec"]["profiles"][0]["uid"], "__PROFILE_UID__",
            "{engine}"
        );
        let pools = doc["spec"]["machinepoolconfig"].as_array().unwrap();
        assert!(!pools.is_empty(), "{engine} has a pool");
        // preflight-plan.sh edits the first pool for the faults of size and type, and on gke and
        // eks that pool is the one worker pool
        if engine == "gke" || engine == "eks" {
            assert_eq!(pools[0]["poolConfig"]["isControlPlane"], false, "{engine}");
        }
    }
}

/// An `aws` that answers every query with nothing, and records each call.
const FAKE_AWS_EMPTY: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_AWS_LOG"
exit 0
"#;

#[test]
fn aws_cleanup_takes_yes_in_any_position_and_keeps_the_name_and_the_region() {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    executable(&bin.join("aws"), FAKE_AWS_EMPTY);
    let log = dir.path().join("aws.log");
    let o = Command::new("bash")
        .arg(live_script("aws-cleanup.sh"))
        .args(["--yes", "barney"])
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .env("FAKE_AWS_LOG", &log)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    let calls = std::fs::read_to_string(&log).unwrap();
    assert!(
        calls.contains("--region us-east-2"),
        "the region stays the default: {calls}"
    );
    assert!(
        calls.contains("cluster/barney"),
        "the name is the first word: {calls}"
    );
    let o = Command::new("bash")
        .arg(live_script("aws-cleanup.sh"))
        .args(["barney", "us-west-2", "--yes", "one-more"])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("too many arguments"),
        "{}",
        text(&o.stderr)
    );
}

/// `record` of preflight-plan.sh keeps a result the ledger does not know out of the entry: the
/// ledger schema has caught, missed, passed, and false-fail, and an entry with another word makes
/// the whole file unreadable.
#[test]
fn preflight_plan_keeps_a_result_the_ledger_does_not_know_out_of_the_entry() {
    let script = std::fs::read_to_string(live_script("preflight-plan.sh")).unwrap();
    let start = script.find("\nrecord() {").unwrap() + 1;
    let end = start + script[start..].find("\n}\n").unwrap() + 3;
    let record = &script[start..end];
    let dir = tempfile::tempdir().unwrap();
    let o = Command::new("bash")
        .arg("-c")
        .arg(format!(
            "{record}\nresults=()\nrecord a caught n\nrecord b not-made 'the edit failed'\nrecord c passed n\nprintf '%s\\n' \"${{results[@]}}\""
        ))
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    assert!(
        out.contains("not in the ledger: b not-made: the edit failed"),
        "{out}"
    );
    assert_eq!(
        out.lines().filter(|l| l.contains('|')).collect::<Vec<_>>(),
        vec!["a|caught|n", "c|passed|n"]
    );
    assert!(
        std::fs::read_to_string(dir.path().join("steps.log"))
            .unwrap()
            .contains("b not-made"),
        "steps.log names it"
    );
}

#[test]
fn aws_cleanup_deletes_nothing_without_yes_and_names_what_it_would() {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    executable(&bin.join("aws"), FAKE_AWS_EMPTY);
    let log = dir.path().join("aws.log");
    let o = Command::new("bash")
        .arg(live_script("aws-cleanup.sh"))
        .args(["fred", "us-east-2"])
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .env("FAKE_AWS_LOG", &log)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    let out = text(&o.stdout);
    assert!(out.contains("nothing deleted; add --yes"), "{out}");
    assert!(out.contains("eks cluster"), "{out}");
    let calls = std::fs::read_to_string(&log).unwrap();
    assert!(
        !calls.contains("delete-")
            && !calls.contains("release-address")
            && !calls.contains("detach-"),
        "no delete without --yes: {calls}"
    );
    assert!(calls.contains("describe-nat-gateways"), "{calls}");
}

// ---- the stages of the live checks -----------------------------------------------------------------

/// A `tofu` for a stage: `output -json` prints $FAKE_OUTPUTS, a plan changes nothing, and each
/// other command passes. It logs each command to $FAKE_TOFU_LOG.
const FAKE_TOFU_STAGE: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_TOFU_LOG"
case "$*" in
  *"output -json"*) cat "$FAKE_OUTPUTS" ;;
  *"show -json"*) echo '{}' ;;
  *plan*) echo "No changes." ;;
  *apply*) echo "Apply complete! Resources: 0 added, 1 changed, 0 destroyed." ;;
esac
"#;

/// A `strata` for a stage. It logs its arguments to $FAKE_STRATA/calls, and answers from
/// $FAKE_STRATA/rules: the first line `GLOB<TAB>EXIT<TAB>FILE` whose GLOB matches the arguments
/// prints FILE, or writes it to the file of `-o`, and exits EXIT. A fourth field `err` prints FILE
/// to standard error and writes no file. With no match it exits 0.
const FAKE_STRATA_STAGE: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_STRATA/calls"
out=""; prev=""
for a in "$@"; do [ "$prev" = -o ] && out=$a; prev=$a; done
while IFS=$'\t' read -r glob code file stream; do
  [[ "$*" == $glob ]] || continue
  if [ "$stream" = err ]; then cat "$FAKE_STRATA/$file" >&2
  elif [ -n "$out" ]; then cp "$FAKE_STRATA/$file" "$out"; else cat "$FAKE_STRATA/$file"; fi
  exit "$code"
done < "$FAKE_STRATA/rules"
exit 0
"#;

/// One run of a stage of a live script against the stand-ins: the OpenTofu outputs, the answers
/// of strata, and the routes of Palette that a test gives.
struct Stage {
    root: tempfile::TempDir,
    answers: usize,
    /// More variables of the environment of the script.
    env: Vec<(String, String)>,
}

impl Stage {
    fn new(outputs: serde_json::Value) -> Self {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        for d in [
            bin.clone(),
            root.path().join("routes"),
            root.path().join("strata"),
            root.path().join("aws"),
        ] {
            std::fs::create_dir_all(d).unwrap();
        }
        executable(&bin.join("tofu"), FAKE_TOFU_STAGE);
        executable(&bin.join("fake-strata"), FAKE_STRATA_STAGE);
        let outputs: serde_json::Map<String, serde_json::Value> = outputs
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::json!({ "value": v })))
            .collect();
        std::fs::write(
            root.path().join("outputs.json"),
            serde_json::Value::Object(outputs).to_string(),
        )
        .unwrap();
        std::fs::write(root.path().join("strata/rules"), "").unwrap();
        Stage {
            root,
            answers: 0,
            env: Vec::new(),
        }
    }

    /// Palette answers `path`, such as `/v1/spectroclusters/c-a`, with `body` and status 200.
    fn route(&self, path: &str, body: &str) {
        let key = path.trim_start_matches('/').replace('/', "_");
        std::fs::write(self.root.path().join("routes").join(key), body).unwrap();
    }

    /// strata answers the arguments that match `glob` with `stdout` and the exit `code`.
    fn strata(&mut self, glob: &str, code: i32, stdout: &str) {
        self.answers += 1;
        let file = format!("answer-{}", self.answers);
        let dir = self.root.path().join("strata");
        std::fs::write(dir.join(&file), stdout).unwrap();
        let mut rules = std::fs::read_to_string(dir.join("rules")).unwrap();
        rules.push_str(&format!("{glob}\t{code}\t{file}\n"));
        std::fs::write(dir.join("rules"), rules).unwrap();
    }

    /// strata answers the arguments that match `glob` with `stderr`, no record, and the exit
    /// `code`.
    fn strata_err(&mut self, glob: &str, code: i32, stderr: &str) {
        self.strata(glob, code, stderr);
        // the rule that `strata` added is the last line
        let rules = self.path("strata/rules");
        let text = std::fs::read_to_string(&rules).unwrap();
        let text = format!("{}\terr\n", text.trim_end_matches('\n'));
        std::fs::write(&rules, text).unwrap();
    }

    /// The script runs with the variable `key` set to `value`.
    fn env(&mut self, key: &str, value: &str) {
        self.env.push((key.to_string(), value.to_string()));
    }

    /// A file that the stage reads, under the root.
    fn path(&self, rel: &str) -> PathBuf {
        self.root.path().join(rel)
    }

    /// Run `script` with `args`, and the results in out/. Gives the output and steps.log.
    fn run(&self, script: &str, args: &[&str]) -> (Output, String) {
        let root = self.root.path();
        let env = [
            ("STRATA_BIN", root.join("bin/fake-strata")),
            ("OUT", root.join("out")),
            ("FAKE_OUTPUTS", root.join("outputs.json")),
            ("FAKE_TOFU_LOG", root.join("tofu.log")),
            ("FAKE_STRATA", root.join("strata")),
            ("FAKE_AWS", root.join("aws")),
        ]
        .map(|(k, v)| (k, v.display().to_string()));
        let env: Vec<(&str, &str)> = env
            .iter()
            .map(|(k, v)| (*k, v.as_str()))
            .chain(self.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .collect();
        let o = run_live(script, args, root, &env);
        let steps = std::fs::read_to_string(root.join("out/steps.log")).unwrap_or_default();
        (o, steps)
    }

    /// Each body of `METHOD path` that the script sent, in order.
    fn sent(&self, method: &str, path: &str) -> Vec<serde_json::Value> {
        let key = path.trim_start_matches('/').replace('/', "_");
        let all = self.path(&format!("routes/sent/{method}_{key}.all"));
        serde_json::Deserializer::from_str(&std::fs::read_to_string(all).unwrap_or_default())
            .into_iter::<serde_json::Value>()
            .map(Result::unwrap)
            .collect()
    }

    /// Each command that OpenTofu ran, one line each.
    fn tofu_calls(&self) -> String {
        std::fs::read_to_string(self.path("tofu.log")).unwrap_or_default()
    }
}

/// Each check of `ids` passed.
fn passed(steps: &str, ids: &[&str]) {
    for id in ids {
        assert!(
            steps
                .lines()
                .any(|l| l.starts_with("PASS") && l.split_whitespace().nth(1) == Some(*id)),
            "{id} passed:\n{steps}"
        );
    }
}

/// The outputs of the live example at its baseline, with cluster A `c-a` named fred.
fn baseline_outputs() -> serde_json::Value {
    serde_json::json!({
        "project_uid": "p-1", "cluster_uid": "c-a", "cluster_b_uid": "",
        "cluster_names": {"a": "fred", "b": "barney"},
        "profile_name": "strata-live-profile", "release_profile_uid": "r-1",
        "addon_profile_uid": "ad-1", "addon_profile_name": "strata-live-addon",
    })
}

/// A stage at the baseline: cluster A is Running, and the baseline checks pass.
fn baseline_stage() -> Stage {
    let stage = Stage::new(baseline_outputs());
    stage.route(
        "/v1/spectroclusters/c-a",
        r#"{"metadata":{"name":"fred"},"status":{"state":"Running"}}"#,
    );
    stage
}

/// A stage of `packs.sh` whose release profile has an OS pack ref with `annotations`, and whose
/// record of that profile is `record`.
fn packs_os_stage(annotations: &str, record: &str) -> Stage {
    let mut stage = Stage::new(serde_json::json!({
        "csi_pack_name": "csi-aws-ebs", "csi_previous_version": "1.63.0",
        "csi_version": "1.64.0", "release_profile_uid": "r-1", "os_pack_name": "ubuntu-aws",
    }));
    stage.route(
        "/v1/clusterprofiles/r-1",
        &format!(
            r#"{{"spec":{{"published":{{"packs":[{{"name":"ubuntu-aws","layer":"os",
                "packUid":"os-1","annotations":{annotations}}}]}}}}}}"#
        ),
    );
    stage.route(
        "/v1/packs/os-1",
        &format!(r#"{{"packValues":[{{"packUid":"os-1","annotations":{annotations}}}]}}"#),
    );
    stage.strata("snapshot r-1 -o release.json", 0, record);
    stage
}

#[test]
fn packs_sh_stage_5_finds_no_annotation_of_the_os_pack_in_a_record_and_each_one_on_the_pack_ref() {
    let annotations = r#"{"osName":"ubuntu","os_version":"24.04"}"#;
    let ids = [
        "os-record",
        "os-annotations",
        "os-profile",
        "os-annotations-ref",
        "os-pack-registry",
        "os-annotations-registry",
    ];
    let stage = packs_os_stage(
        annotations,
        r#"{"packs":[{"layer":"os","name":"ubuntu-aws","packUid":"os-1"}]}"#,
    );
    let (_, steps) = stage.run("packs.sh", &[]);
    passed(&steps, &ids);

    // a record that keeps each annotation, as strata 0.19 wrote it, fails the check
    let stage = packs_os_stage(
        annotations,
        &format!(
            r#"{{"packs":[{{"layer":"os","name":"ubuntu-aws","packUid":"os-1","annotations":{annotations}}}]}}"#
        ),
    );
    let (_, steps) = stage.run("packs.sh", &[]);
    assert!(
        steps
            .lines()
            .any(|l| l.starts_with("FAIL") && l.split_whitespace().nth(1) == Some("os-annotations")),
        "{steps}"
    );
}

#[test]
fn tiers_sh_stage_5_edits_the_subjects_of_a_binding_of_cluster_a_and_puts_the_bindings_back() {
    let mut stage = baseline_stage();
    let rbacs = "/v1/spectroclusters/c-a/config/rbacs";
    stage.route(
        rbacs,
        r#"{"items":[{"metadata":{"name":"ops"},"spec":{"bindings":[{"type":"RoleBinding","namespace":"ops",
            "role":{"kind":"Role","name":"edit"},"subjects":[{"type":"Group","name":"ops"}]}]}}]}"#,
    );
    stage.strata(
        "snapshot cluster:c-a -o rbac.json",
        0,
        r#"{"source":{"cluster":{"tiers":{"policy":{"fields":{"rbacs":[{"role":{"kind":"ClusterRole","name":"view"},
            "subjects":[{"name":"barney@example.com","namespace":"","type":"User"}],"type":"ClusterRoleBinding"}]}}}}}}"#,
    );
    stage.strata(
        "compare rbac.json cluster:c-a --json",
        0,
        r#"{"clusterTiers":[{"tier":"policy","same":false,"diff":[
            {"op":"removed","path":"rbacs[0].subjects[name=barney@example.com].name"},
            {"op":"added","path":"rbacs[0].subjects[name=fred@example.com].name"}]}]}"#,
    );
    stage.strata(
        "compare rbac.json cluster:c-a",
        0,
        "  cluster tiers applied unchanged, policy CHANGED\n        + policy.rbacs[0].subjects[name=fred@example.com].name: \"fred@example.com\"\n",
    );
    stage.strata(
        "verify rbac.json",
        0,
        "  cluster tiers applied unchanged, policy CHANGED\nOK: cluster fred\n",
    );

    let (o, steps) = stage.run("tiers.sh", &["5"]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    assert!(!steps.contains("FAIL"), "{steps}");
    passed(
        &steps,
        &[
            "rbac-add",
            "rbac-in-record",
            "rbac-edit",
            "rbac-compare",
            "rbac-compare-policy",
            "rbac-compare-says",
            "rbac-verify",
            "rbac-verify-says",
            "rbac-undo",
            "rbac-after",
        ],
    );
    assert!(!steps.contains("== stage 4"), "stage 5 runs alone: {steps}");

    let ops = serde_json::json!({"spec": {"bindings": [{"type": "RoleBinding", "namespace": "ops",
        "role": {"kind": "Role", "name": "edit"}, "subjects": [{"type": "Group", "name": "ops"}]}]}});
    let view = |s: &str| {
        serde_json::json!({"spec": {"bindings": [{"type": "ClusterRoleBinding",
        "role": {"kind": "ClusterRole", "name": "view"}, "subjects": [{"type": "User", "name": s}]}]}})
    };
    assert_eq!(
        stage.sent("PUT", rbacs),
        vec![
            serde_json::json!({"rbacs": [ops.clone(), view("barney@example.com")]}),
            serde_json::json!({"rbacs": [ops.clone(), view("fred@example.com")]}),
            serde_json::json!({"rbacs": [ops]}),
        ],
        "the PUTs keep the binding of the cluster, add one, edit its subject, and take it away"
    );
    assert!(
        stage.tofu_calls().contains("plan"),
        "stage 5 alone checks the baseline first: {}",
        stage.tofu_calls()
    );
}

#[test]
fn tiers_sh_stage_5_reads_a_404_of_the_bindings_as_a_cluster_with_none() {
    let mut stage = baseline_stage();
    stage.strata(
        "snapshot cluster:c-a -o rbac.json",
        0,
        r#"{"source":{"cluster":{"tiers":{"policy":{"fields":{"rbacs":[{"role":{"kind":"ClusterRole","name":"view"},
            "subjects":[{"name":"barney@example.com","type":"User"}],"type":"ClusterRoleBinding"}]}}}}}}"#,
    );
    let (_, steps) = stage.run("tiers.sh", &["5"]);
    assert!(
        steps.contains("PASS  rbac-before") && steps.contains("PASS  rbac-after "),
        "{steps}"
    );
    assert!(
        steps.contains("FAIL  rbac-add"),
        "no route answers the PUT here: {steps}"
    );
}

#[test]
fn app_profile_sh_stage_3_makes_two_app_profiles_whose_tiers_differ_in_the_install_order() {
    let mut stage = Stage::new(serde_json::json!({"project_uid": "p-1"}));
    // each create gives the next uid: ap-1, then ap-2
    executable(
        &stage.path("routes/v1_appProfiles.sh"),
        "#!/usr/bin/env bash\nn=$(( $(cat \"$FAKE_ROUTES/n\" 2> /dev/null || echo 0) + 1 ))\necho $n > \"$FAKE_ROUTES/n\"\necho \"{\\\"uid\\\":\\\"ap-$n\\\"}\"\n",
    );
    stage.route(
        "/v1/appProfiles/ap-1/tiers",
        r#"{"spec":{"appTiers":[
            {"metadata":{"name":"tier1","uid":"t-1"},"spec":{"type":"manifest","version":"1.0.0","installOrder":2}},
            {"metadata":{"name":"tier2","uid":"t-2"},"spec":{"type":"manifest","version":"1.0.0","installOrder":5}}]}}"#,
    );
    stage.route("/v1/appProfiles/ap-1/tiers/t-1", "{}");
    stage.route("/v1/appProfiles/ap-1", "{}");
    stage.route("/v1/appProfiles/ap-2", "{}");
    stage.strata(
        "snapshot appprofile:ap-1 -o order-a.json",
        0,
        r#"{"packs":[{"key":"app/tier1","params":{"installOrder":2}},{"key":"app/tier2","params":{"installOrder":5}}]}"#,
    );
    stage.strata(
        "snapshot appprofile:ap-2 -o order-b.json",
        0,
        r#"{"packs":[{"key":"app/tier2","params":{"installOrder":2}},{"key":"app/tier1","params":{"installOrder":5}}]}"#,
    );
    stage.strata(
        "compare order-a.json order-b.json",
        1,
        "~ [app  ] tier1                      1.0.0              AUTHORED\n      ~ param installOrder: 2 -> 5\n",
    );

    let (_, steps) = stage.run("app-profile.sh", &["3"]);
    passed(
        &steps,
        &[
            "order-a-create",
            "order-b-create",
            "order-a-tiers",
            "order-a-record",
            "order-a-params",
            "order-b-record",
            "order-b-params",
            "order-compare",
            "order-compare-says",
            "order-update",
            "order-update-verify",
            "order-a-delete",
            "order-b-delete",
        ],
    );
    let creates: Vec<_> = stage
        .sent("POST", "/v1/appProfiles")
        .iter()
        .map(|b| {
            b["spec"]["template"]["appTiers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| (t["name"].clone(), t["installOrder"].clone()))
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(
        creates,
        [
            [("tier1".into(), 2.into()), ("tier2".into(), 5.into())],
            [("tier1".into(), 5.into()), ("tier2".into(), 2.into())],
        ],
        "the same two tiers, with the orders in the other order"
    );
    let bodies = stage.sent("POST", "/v1/appProfiles");
    assert_eq!(
        bodies[0]["spec"]["template"]["appTiers"],
        bodies[1]["spec"]["template"]["appTiers"]
            .as_array()
            .unwrap()
            .iter()
            .zip([2, 5])
            .map(|(t, o)| {
                let mut t = t.clone();
                t["installOrder"] = o.into();
                t
            })
            .collect::<serde_json::Value>(),
        "the tiers differ in the install order alone"
    );
}

#[test]
fn cluster_group_sh_stage_3_changes_the_cpu_limit_of_the_group_and_puts_it_back() {
    let mut outputs = baseline_outputs();
    outputs["cluster_group_uid"] = "g-1".into();
    outputs["member_cluster_uid"] = "m-1".into();
    let mut stage = Stage::new(outputs);
    stage.strata(
        "snapshot clustergroup:g-1 -o limit.json",
        0,
        r#"{"source":{"cluster":{"tiers":{"policy":{"fields":{"limitConfig":{"cpu":6,"cpuMilliCore":6000}}}}}}}"#,
    );
    stage.strata(
        "verify limit.json",
        0,
        "  cluster tiers policy CHANGED, labels unchanged\n",
    );
    stage.strata(
        "compare limit.json clustergroup:g-1 --json",
        0,
        r#"{"clusterTiers":[{"tier":"policy","same":false,"diff":[
            {"op":"changed","path":"limitConfig.cpu"},{"op":"changed","path":"limitConfig.cpuMilliCore"}]}]}"#,
    );
    stage.strata(
        "compare limit.json clustergroup:g-1",
        0,
        "        ~ policy.limitConfig.cpuMilliCore: 6000 -> 4000\n",
    );

    let (_, steps) = stage.run("cluster-group.sh", &["3"]);
    passed(
        &steps,
        &[
            "limit-record",
            "limit-before",
            "limit-verify",
            "limit-verify-says",
            "limit-compare",
            "limit-compare-says",
            "limit-compare-tier",
        ],
    );
    assert!(
        steps.contains("FAIL  limit-undone"),
        "the stand-in still gives a changed tier after the undo, and the check sees it: {steps}"
    );
    let applies: Vec<String> = stage
        .tofu_calls()
        .lines()
        .filter(|l| l.contains("apply"))
        .map(String::from)
        .collect();
    assert!(
        applies[0].contains("-var cluster_group=true -var group_cpu_millicore=4000"),
        "{applies:?}"
    );
    assert!(
        applies[1].contains("-var cluster_group=true") && !applies[1].contains("group_cpu"),
        "the second apply gives the default limit again: {applies:?}"
    );
}

/// An `oras` that logs its arguments to $FAKE_AWS/oras.log, and prints a digest for a push.
const FAKE_ORAS: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_AWS/oras.log"
[ "$1" = push ] && echo "Digest: sha256:$(printf '%s' "$*" | sha256sum | cut -c1-64)"
exit 0
"#;

/// A stage of `registry.sh`: the outputs of the example's own registry and pack, and `more`. Each
/// sync ends at once, the wait for the clear of the flag ends at once, and the profile that pins
/// the pack under a floating tag moves at once.
fn registry_stage(more: serde_json::Value) -> Stage {
    let mut outputs = serde_json::json!({
        "project_uid": "p-1", "cluster_uid": "c-a", "region": "us-east-2",
        "aws_profile": "spectro-admin", "owned_registry_uid": "reg-1",
        "owned_registry_name": "strata-live-registry", "owned_registry_host": "host.example",
        "owned_repository": "host.example/strata/strata-live-pack",
        "owned_pack_name": "strata-live-pack", "owned_profile_uid": "own-1",
    });
    for (k, v) in more.as_object().unwrap() {
        outputs[k] = v.clone();
    }
    let mut stage = Stage::new(outputs);
    let bin = stage.path("bin");
    executable(&bin.join("aws"), FAKE_AWS);
    executable(&bin.join("oras"), FAKE_ORAS);
    std::fs::write(stage.path("aws/list-images"), "[]\n").unwrap();
    // the profile route of the stand-in keeps the flag, so the wait for the clear ends at once
    stage.env("INVALID_MINUTES", "0");
    // each read of the sync status gives a new run time, so each sync ends at once
    stage.route("/v1/registries/oci/reg-1/ecr/sync", "{}");
    executable(
        &stage.path("routes/v1_registries_oci_reg-1_ecr_sync_status.sh"),
        "#!/usr/bin/env bash\necho \"{\\\"status\\\":\\\"Completed\\\",\\\"lastRunTime\\\":\\\"$(date +%s%N)\\\"}\"\n",
    );
    stage.route(
        "/v1/clusterprofiles/own-1",
        r#"{"spec":{"published":{"packs":[{"name":"strata-live-pack","tag":"1.0.9",
            "isInvalid":true,"inValidReason":"strata-live-pack 1.0.9 tag is deleted"}]}}}"#,
    );
    stage.strata("verify profile-1.json*", 1, "DRIFT\n");
    stage
}

#[test]
fn registry_sh_step_3b_deletes_the_tag_that_the_profile_pins_and_pushes_it_again() {
    let mut stage = registry_stage(serde_json::json!({}));
    stage.strata(
        "snapshot own-1 -o deleted-before.json",
        0,
        r#"{"root":"aaa","packs":[{"key":"addon/strata-live-pack"}]}"#,
    );
    stage.strata(
        "snapshot own-1 -o deleted-now.json",
        0,
        r#"{"root":"aaa","packs":[{"key":"addon/strata-live-pack","invalid":true,
            "invalidReason":"strata-live-pack 1.0.9 tag is deleted"}]}"#,
    );
    stage.strata(
        "verify deleted-before.json",
        0,
        "INVALID: Palette marks pack addon/strata-live-pack invalid now (strata-live-pack 1.0.9 tag is deleted)\n",
    );

    let (o, steps) = stage.run("registry.sh", &[]);
    // the stand-ins give no OCI Helm registry, so the script stops at step 5
    assert_eq!(o.status.code(), Some(2), "{steps}{}", text(&o.stderr));
    assert!(steps.contains("== 3b."), "{steps}");
    passed(
        &steps,
        &[
            "push-del",
            "sync-del-1",
            "del-record",
            "del-tag",
            "sync-del-2",
            "del-doc",
            "del-record-now",
            "del-record-flag",
            "del-record-root",
            "del-verify-says",
            "push-del-again",
            "sync-del-3",
            "del-doc-after",
            "del-tag-end",
            "sync-del-4",
        ],
    );
    for seen in [
        "del-flag ",
        "del-doc-fields ",
        "del-verify ",
        "del-cleared ",
    ] {
        assert!(steps.contains(&format!("SEEN  {seen}")), "{seen}: {steps}");
    }
    let fields = std::fs::read_to_string(stage.path("out/del-doc-fields.out")).unwrap();
    assert!(
        fields.contains(r#""isInvalid":true"#) && fields.contains("tag is deleted"),
        "{fields}"
    );

    let aws = std::fs::read_to_string(stage.path("aws/calls.log")).unwrap();
    let deletes: Vec<&str> = aws
        .lines()
        .filter(|l| l.contains("batch-delete-image"))
        .collect();
    assert_eq!(
        deletes.len(),
        2,
        "the delete, then the delete at the end: {aws}"
    );
    assert!(
        deletes
            .iter()
            .all(|l| l
                .contains("--repository-name strata/strata-live-pack --image-ids imageTag=1.0.9")),
        "{aws}"
    );
    let oras = std::fs::read_to_string(stage.path("aws/oras.log")).unwrap();
    assert_eq!(
        oras.matches("push host.example/strata/strata-live-pack:1.0.9 ")
            .count(),
        2,
        "the push, then the push again: {oras}"
    );
    let tofu = stage.tofu_calls();
    let pins: Vec<&str> = tofu
        .lines()
        .filter(|l| l.contains("owned_pack_profile=true"))
        .collect();
    assert!(
        pins.iter().any(|l| l.contains(
            "-var owned_pack_profile=true -var owned_pack_version=1.0.9 -var owned_pack_tag=1.0.9"
        )),
        "{tofu}"
    );
    let back = pins
        .iter()
        .rposition(|l| !l.contains("owned_pack_version"))
        .unwrap();
    let pin = pins
        .iter()
        .position(|l| l.contains("owned_pack_version=1.0.9"))
        .unwrap();
    assert!(
        back > pin,
        "the profile goes back to 1.0.0 after the step: {tofu}"
    );
}

#[test]
fn registry_sh_step_4b_pins_a_profile_with_values_of_its_own_under_the_floating_tag_and_runs_behind()
 {
    let mut stage = registry_stage(serde_json::json!({}));
    stage.route("/v1/clusterprofiles", r#"{"uid":"fm-1"}"#);
    stage.route("/v1/clusterprofiles/fm-1/publish", "{}");
    stage.route(
        "/v1/packs",
        r#"{"items":[{"metadata":{"uid":"pk-101"},"spec":{"registryUid":"reg-1","version":"1.0.1"}}]}"#,
    );
    stage.strata(
        "packs strata-live-pack --json",
        0,
        r#"{"registries":[{"registry":"strata-live-registry","versions":[
            {"version":"1.0.1","tags":["1.0.1","1.0.x","1.x"]},{"version":"1.0.0","tags":["1.0.0"]}]}]}"#,
    );
    stage.strata("verify profile-3.json*", 1, "DRIFT\n");
    stage.strata(
        "behind floating-mod-1.json --no-impact --json",
        1,
        r#"{"pins":[{"version":"1.0.1","tag":"1.0.x","floating":true,"resolved":"1.0.2",
            "newer":[{"version":"1.0.2"}],"behind":true}]}"#,
    );

    let (_, steps) = stage.run("registry.sh", &[]);
    assert!(steps.contains("== 4b. the floating tag 1.0.x"), "{steps}");
    passed(
        &steps,
        &[
            "pin-floating-modified",
            "floating-mod",
            "floating-mod-keeps",
            "floating-mod-behind",
            "floating-mod-behind-names",
            "delete-floating-modified",
        ],
    );
    // the profile pins the build of 1.0.1 under the floating tag, with the changed values
    let sent: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(stage.path("routes/sent/POST_v1_clusterprofiles")).unwrap(),
    )
    .unwrap();
    let pack = &sent["spec"]["template"]["packs"][0];
    assert_eq!(pack["tag"], "1.0.x", "{sent}");
    assert_eq!(pack["uid"], "pk-101", "{sent}");
    assert_eq!(pack["layer"], "addon", "{sent}");
    assert!(
        pack["values"]
            .as_str()
            .unwrap()
            .contains("strataModified: true"),
        "{sent}"
    );
    let calls = std::fs::read_to_string(stage.path("strata/calls")).unwrap();
    let kept = calls
        .lines()
        .position(|l| l == "verify floating-mod-1.json");
    let moved = calls.lines().position(|l| l == "verify profile-3.json");
    assert!(
        moved.is_some() && kept > moved,
        "the check of the profile with changed values comes after the wait for the move: {calls}"
    );
}

/// A `helm` that passes each command, and prints a digest for a push.
const FAKE_HELM_PUSH: &str = r#"#!/usr/bin/env bash
[ "$1" = push ] && echo "Digest: sha256:$(printf '%s' "$*" | sha256sum | cut -c1-64)"
exit 0
"#;

#[test]
fn registry_sh_step_6_lists_the_required_pack_with_the_cloud_type_all_for_the_cloud_aws() {
    let mut stage = registry_stage(serde_json::json!({
        "owned_helm_registry_uid": "reg-helm", "owned_helm_registry_name": "strata-live-charts",
        "owned_chart_repository": "host.example/strata/charts/strata-live-chart",
        "owned_chart_name": "strata-live-chart", "owned_helm_profile_uid": "hp-1",
        "dep_pack_name": "strata-live-dep",
        "owned_dep_repository": "host.example/strata/strata-live-dep",
        "dep_profile_uid": "dp-1",
    }));
    executable(&stage.path("bin/helm"), FAKE_HELM_PUSH);
    stage.strata(
        "packs strata-live-dep --registry strata-live-registry --cloud aws --json",
        0,
        r#"{"cloud":"aws","registries":[{"registry":"strata-live-registry",
            "versions":[{"version":"1.0.0","cloudTypes":["all"]}]}]}"#,
    );

    let (_, steps) = stage.run("registry.sh", &[]);
    assert!(steps.contains("== 6."), "{steps}");
    passed(&steps, &["dep-packs-cloud", "dep-packs-cloud-version"]);
    let calls = std::fs::read_to_string(stage.path("strata/calls")).unwrap();
    assert!(
        calls.lines().any(
            |l| l == "packs strata-live-dep --registry strata-live-registry --cloud aws --json"
        ),
        "{calls}"
    );
}

/// A run of packs.sh whose listing of kubernetes holds `count` entries, and whose
/// `strata packs kubernetes` prints `headings`. Gives steps.log.
fn packs_paged(count: u32, headings: &str) -> String {
    let mut stage = Stage::new(serde_json::json!({
        "csi_pack_name": "csi-aws-ebs", "csi_previous_version": "1.63.1",
        "csi_version": "1.65.0", "cloud": "aws",
    }));
    stage.route(
        "/v1/packs",
        &format!(r#"{{"listmeta":{{"count":{count}}}}}"#),
    );
    stage.strata("packs kubernetes", 0, headings);
    let (_, steps) = stage.run("packs.sh", &[]);
    let calls = std::fs::read_to_string(stage.path("strata/calls")).unwrap();
    assert!(calls.lines().any(|l| l == "packs kubernetes"), "{calls}");
    steps
}

#[test]
fn packs_sh_counts_the_versions_of_all_registries_against_the_100_of_one_page() {
    let steps = packs_paged(
        266,
        "kubernetes in Palette Registry (169 versions)\n  1.36.2  1.36.x\n\
         kubernetes in Public Repo (97 versions)\n  1.36.2  1.36.x\n",
    );
    passed(
        &steps,
        &["packs-paged-listing", "packs-paged", "packs-paged-count"],
    );

    let steps = packs_paged(
        97,
        "kubernetes in Public Repo (97 versions)\n  1.36.2  1.36.x\n",
    );
    assert!(steps.contains("FAIL  packs-paged-listing"), "{steps}");
    assert!(steps.contains("FAIL  packs-paged-count"), "{steps}");
}

/// A run of packs.sh whose Helm summary listing gives the Bitnami registry the sync `sync`, and
/// whose `strata behind` on the record of the newest nginx of Bitnami prints `line` and `json`.
/// Gives steps.log.
fn packs_stale(sync: &str, line: &str, json: &str) -> String {
    let mut stage = Stage::new(serde_json::json!({
        "csi_pack_name": "csi-aws-ebs", "csi_previous_version": "1.63.1",
        "csi_version": "1.65.0", "cloud": "aws",
    }));
    stage.route(
        "/v1/registries/helm/summary",
        &format!(
            r#"{{"items":[{{"metadata":{{"name":"Bitnami","uid":"reg-bitnami"}},"status":{{"sync":{sync}}}}}]}}"#
        ),
    );
    stage.strata(
        "packs nginx --registry Bitnami --latest --json",
        0,
        r#"{"registries":[{"versions":[{"version":"25.1.5"}]}]}"#,
    );
    stage.strata("snapshot pack:Bitnami/nginx@25.1.5 -o nginx.json", 0, "{}");
    stage.strata("behind nginx.json --no-impact", 0, line);
    stage.strata("behind nginx.json --no-impact --json", 0, json);
    let (_, steps) = stage.run("packs.sh", &[]);
    steps
}

#[test]
fn packs_sh_stage_8_checks_the_sync_note_of_a_pin_that_is_current_against_an_older_copy() {
    // the sync of Bitnami stays in progress: the line names the sync, and the JSON carries it
    let steps = packs_stale(
        r#"{"status":"InProgress","lastSyncedTime":"2026-08-29T00:00:11.032Z","lastRunTime":"2026-10-05T18:04:27.776Z","isSyncSupported":true}"#,
        "1 pin from 1 record: 0 behind\n  [addon] nginx  25.1.5  Bitnami  current; a sync of the \
         registry is in progress since 2026-10-05T18:04:27.776Z, and the last complete sync of \
         the registry started 2026-08-29T00:00:11.032Z\n",
        r#"{"pins":[{"registrySync":{"status":"InProgress"}}]}"#,
    );
    passed(
        &steps,
        &[
            "bitnami-sync",
            "stale-record",
            "stale-current",
            "stale-note",
            "stale-json",
        ],
    );
    assert!(!steps.contains("stale-no-note"), "{steps}");

    // the last sync failed: the line names the last complete sync and the failure
    let steps = packs_stale(
        r#"{"status":"Failed","lastSyncedTime":"2025-10-16T00:00:11.742Z","message":"invalid helm registry http status 404","isSyncSupported":true}"#,
        "1 pin from 1 record: 0 behind\n  [addon] nginx  25.1.5  Bitnami  current; the last \
         complete sync of the registry started 2025-10-16T00:00:11.742Z; the last sync of the \
         registry failed: invalid helm registry http status 404\n",
        r#"{"pins":[{"registrySync":{"status":"Failed"}}]}"#,
    );
    passed(&steps, &["stale-note", "stale-json"]);

    // the last sync completed: the pin has no note
    let steps = packs_stale(
        r#"{"status":"Completed","lastSyncedTime":"2026-10-05T00:00:14.200Z","lastRunTime":"2026-10-05T00:00:14.200Z","isSyncSupported":true}"#,
        "1 pin from 1 record: 0 behind\n  [addon] nginx  25.1.5  Bitnami  current\n",
        r#"{"pins":[{}]}"#,
    );
    passed(&steps, &["stale-current", "stale-no-note"]);
    assert!(!steps.contains("stale-note"), "{steps}");

    // a line with no note while the last sync did not complete fails the check
    let steps = packs_stale(
        r#"{"status":"Failed","lastSyncedTime":"2025-10-16T00:00:11.742Z","isSyncSupported":true}"#,
        "1 pin from 1 record: 0 behind\n  [addon] nginx  25.1.5  Bitnami  current\n",
        r#"{"pins":[{}]}"#,
    );
    assert!(steps.contains("FAIL  stale-note"), "{steps}");
}

#[test]
fn validate_packs_sh_stage_4_makes_a_profile_whose_pack_has_another_layer_than_its_registry() {
    let mut stage = Stage::new(serde_json::json!({}));
    stage.route(
        "/v1/clusterprofiles/validate/packs",
        r#"{"packs":{"results":[]}}"#,
    );
    stage.route(
        "/v1/packs",
        r#"{"items":[{"metadata":{"uid":"pk-1"},"spec":{"registryUid":"reg-pub","version":"1.21.1"}}]}"#,
    );
    stage.route("/v1/dashboard/clusterprofiles/metadata", r#"{"items":[]}"#);
    stage.route("/v1/clusterprofiles", r#"{"uid":"lp-1"}"#);
    stage.route("/v1/clusterprofiles/lp-1/publish", "{}");
    stage.route(
        "/v1/clusterprofiles/lp-1",
        r#"{"spec":{"published":{"packs":[{"name":"cert-manager","layer":"addon"}]}}}"#,
    );
    let record = r#"{"packs":[{"key":"addon/cert-manager","registryUid":"reg-pub",
        "type":"spectro","packUid":"pk-1","tag":"1.21.1","values":"installCRDs: true"}]}"#;
    stage.strata(
        "snapshot --from-candidate strata-live-layer.yaml*",
        0,
        record,
    );
    stage.strata(
        "snapshot --from-candidate strata-validate-wilma.yaml*",
        2,
        "",
    );
    stage.strata("snapshot lp-1 -o layer.json", 0, record);

    let (_, steps) = stage.run("validate-packs.sh", &["4"]);
    passed(
        &steps,
        &[
            "layer-registry",
            "layer-refused",
            "layer-validate",
            "layer-validate-says",
            "layer-create",
            "layer-profile",
            "layer-published",
            "layer-record",
            "layer-record-key",
            "layer-root",
            "layer-delete",
        ],
    );
    let wilma = std::fs::read_to_string(stage.path("out/strata-validate-wilma.yaml")).unwrap();
    assert!(wilma.contains("    layer: csi\n"), "{wilma}");
    let validate = &stage.sent("POST", "/v1/clusterprofiles/validate/packs")[0];
    assert_eq!(validate["packs"][0]["layer"], "csi", "{validate}");
    let create = &stage.sent("POST", "/v1/clusterprofiles")[0]["spec"]["template"]["packs"][0];
    assert_eq!(
        (
            &create["name"],
            &create["layer"],
            &create["uid"],
            &create["values"]
        ),
        (
            &"cert-manager".into(),
            &"csi".into(),
            &"pk-1".into(),
            &"installCRDs: true".into()
        ),
        "the pack of the candidate, with the values of the candidate and the layer csi"
    );
    let log = std::fs::read_to_string(stage.path("curl.log")).unwrap();
    assert!(
        log.lines().any(|l| l == "DELETE /v1/clusterprofiles/lp-1"),
        "{log}"
    );
}

#[test]
fn validate_packs_sh_creates_no_profile_unless_stage_4_is_given() {
    let stage = Stage::new(serde_json::json!({}));
    stage.route("/v1/clusterprofiles", r#"{"uid":"lp-1"}"#);

    let (_, steps) = stage.run("validate-packs.sh", &[]);
    assert!(!steps.contains("layer"), "{steps}");
    assert!(stage.sent("POST", "/v1/clusterprofiles").is_empty());
}

#[test]
fn repave_sh_stage_8_asks_palette_about_a_variable_in_a_repave_key_with_and_without_its_value() {
    let mut outputs = baseline_outputs();
    outputs["next_profile_uid"] = "n-1".into();
    let mut stage = Stage::new(outputs);
    stage.route(
        "/v1/spectroclusters/c-a",
        r#"{"metadata":{"name":"fred"},"status":{"state":"Running"}}"#,
    );
    stage.route(
        "/v1/clusterprofiles/n-1",
        r#"{"metadata":{"uid":"n-1"},"spec":{"published":{"packs":[{"name":"kubernetes","tag":"1.36.2",
            "type":"spectro","layer":"k8s","values":"- 'chmod {{.spectro.var.strata_mode}} /var/lib/kubelet/config.yaml'\n"}]}}}"#,
    );
    stage.route(
        "/v1/spectroclusters/c-a/validate/repave",
        r#"{"isRepaveRequired":true,"reasons":[{"code":"PackValuesUpdated","message":"Pack 'kubernetes' values are updated",
            "pack":{"diffConfigKeys":["kubeadmconfig.postKubeadmCommands"]}}]}"#,
    );
    stage.strata(
        "snapshot n-1 -o next-before.json",
        0,
        r#"{"packs":[{"layer":"k8s","values":"- 'chmod 600 /var/lib/kubelet/config.yaml'\n"}]}"#,
    );
    stage.strata(
        "snapshot n-1 -o next-variable.json",
        0,
        r#"{"packs":[{"layer":"k8s","values":"- 'chmod {{.spectro.var.strata_mode}} /var/lib/kubelet/config.yaml'\n"}]}"#,
    );
    stage.strata(
        "compare cluster:c-a/strata-live-profile n-1 --repave --json",
        1,
        r#"{"paletteRepave":{"required":true}}"#,
    );

    let (o, steps) = stage.run("repave.sh", &["8"]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    assert!(!steps.contains("FAIL"), "{steps}");
    passed(
        &steps,
        &[
            "var-command",
            "var-record",
            "var-in-values",
            "var-control",
            "var-control-says",
            "var-undone",
        ],
    );
    for seen in [
        "var-strata ",
        "var-strata-answer ",
        "var-none ",
        "var-default ",
    ] {
        assert!(steps.contains(&format!("SEEN  {seen}")), "{seen}: {steps}");
    }
    let variables: Vec<_> = stage
        .sent("POST", "/v1/spectroclusters/c-a/validate/repave")
        .iter()
        .map(|b| b["profiles"][0]["variables"].clone())
        .collect();
    assert_eq!(
        variables,
        [
            serde_json::json!([]),
            serde_json::json!([{"name": "strata_mode", "value": "600"}]),
            serde_json::json!([{"name": "strata_mode", "value": "644"}]),
        ],
        "no variables, the default, and the control"
    );
    let applies: Vec<String> = stage
        .tofu_calls()
        .lines()
        .filter(|l| l.contains("apply"))
        .map(|l| {
            l.split(" -var ")
                .skip(1)
                .filter(|v| !v.starts_with("immutable_profiles"))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    assert_eq!(
        applies,
        [
            "next_variable=true",
            "next_variable=true next_variable_in_values=true",
            "next_variable=true",
            "",
        ],
        "the variable before its use, and its use away before the variable"
    );
}

#[test]
fn registry_move_nodes_sh_moves_the_os_pack_of_the_release_and_cluster_a_takes_it() {
    let mut stage = Stage::new(baseline_outputs());
    stage.env("WATCH_MINUTES", "0");
    stage.route(
        "/v1/spectroclusters/c-a",
        r#"{"metadata":{"name":"fred"},"status":{"state":"Running"},"spec":{"cloudType":"aws",
            "cloudConfigRef":{"uid":"cc-1"},"clusterProfileTemplates":[{"uid":"r-1"}]}}"#,
    );
    stage.route(
        "/v1/cloudconfigs/aws/cc-1",
        r#"{"spec":{"machinePoolConfig":[]}}"#,
    );
    stage.route("/v1/spectroclusters/c-a/profiles/packs/manifests", "{}");
    stage.route("/v1/spectroclusters/c-a/profiles", "{}");
    stage.route(
        "/v1/clusterprofiles/r-1",
        r#"{"metadata":{"uid":"r-1"},"spec":{"published":{"packs":[{"name":"ubuntu-aws","tag":"24.04","type":"oci"}]}}}"#,
    );
    stage.route("/v1/clusterprofiles/r-1/packs/manifests", "{}");
    std::fs::create_dir_all(stage.path("out")).unwrap();
    std::fs::write(
        stage.path("out/release-0.json"),
        r#"{"packs":[{"layer":"os","version":"24.04","values":"os: values\n","registryUid":"reg-a","type":"spectro"}]}"#,
    )
    .unwrap();
    stage.strata(
        "snapshot r-1 -o release-moved.json",
        0,
        r#"{"source":{"uid":"r-1"},"packs":[{"layer":"os","version":"24.04","values":"os: values\n","registryUid":"reg-b","type":"oci"}]}"#,
    );
    stage.strata(
        "compare cluster:c-a/strata-live-profile r-1 --repave --json",
        1,
        r#"{"packs":[{"key":"os/ubuntu-aws","impact":{"level":"repave"}}],"paletteRepave":{"required":false,"reasons":[]}}"#,
    );
    stage.strata(
        "snapshot cluster:c-a -o cluster-moved.json",
        0,
        r#"{"packs":[{"layer":"os","registryUid":"reg-b"}]}"#,
    );
    stage.strata(
        "snapshot cluster:c-a",
        0,
        r#"{"source":{"cluster":{"repavePending":false}}}"#,
    );

    let (o, steps) = stage.run("registry-move-nodes.sh", &["2"]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    assert!(!steps.contains("FAIL"), "{steps}");
    passed(
        &steps,
        &[
            "move-record",
            "move-same-uid",
            "move-identity",
            "take",
            "after-record",
            "after-runs",
            "undo-take",
            "undo-baseline",
        ],
    );
    for seen in [
        "move-compare ",
        "move-impact ",
        "move-palette ",
        "move-reasons ",
        "replaced ",
        "waits ",
    ] {
        assert!(steps.contains(&format!("SEEN  {seen}")), "{seen}: {steps}");
    }
    let applies: Vec<&str> = stage
        .tofu_calls()
        .lines()
        .filter(|l| l.contains("apply"))
        .map(|l| {
            if l.contains("move_os_registry=true") {
                "move"
            } else {
                "back"
            }
        })
        .collect();
    assert_eq!(applies, ["move", "back"], "{}", stage.tofu_calls());
    assert_eq!(
        stage.sent("PUT", "/v1/spectroclusters/c-a/profiles").len(),
        2,
        "cluster A takes the move, then the release back"
    );
}

#[test]
fn registry_move_nodes_sh_stops_before_the_undo_when_a_repave_waits_for_approval() {
    let mut stage = Stage::new(baseline_outputs());
    stage.strata(
        "snapshot cluster:c-a",
        0,
        r#"{"source":{"cluster":{"repavePending":true}}}"#,
    );
    let (_, steps) = stage.run("registry-move-nodes.sh", &["4"]);
    assert!(
        steps.contains("a repave of cluster A waits for approval"),
        "{steps}"
    );
    assert!(
        !stage.tofu_calls().contains("apply"),
        "no apply: {}",
        stage.tofu_calls()
    );
}

/// A stage of draft.sh where Palette has the add-on `ad-1`, and the create of the copy with no
/// publish answers the uid `un-1`. strata says `published` for `un-1` in the JSON of the list.
fn unpublished_stage(un_published: bool) -> Stage {
    let mut stage = baseline_stage();
    stage.strata(
        "list --json",
        0,
        r#"{"profiles":[{"name":"strata-live-addon","version":"1.0.0","uid":"ad-1"}]}"#,
    );
    stage.strata(
        "list --packs --json",
        0,
        &serde_json::json!({"profiles": [
            {"uid": "ad-1", "published": true, "packs": [{"name": "config"}]},
            {"uid": "un-1", "published": un_published, "packs": [{"name": "config"}]},
        ]})
        .to_string(),
    );
    stage.strata(
        "list --packs --no-color",
        0,
        "  strata-live-addon@1.0.0        ad-1  all  add-on  no consumers\n  \
         strata-live-unpublished@1.0.0  un-1  all  add-on  no consumers  draft\n",
    );
    stage.route(
        "/v1/clusterprofiles/ad-1",
        r#"{"metadata":{"name":"strata-live-addon","uid":"ad-1"},"spec":{"version":"1.0.0",
            "published":{"type":"add-on","cloudType":"all","packs":[{"name":"config",
            "tag":"1.0.0","type":"manifest","layer":"addon","packUid":"pk-1","values":""}]}}}"#,
    );
    stage.route(
        "/v1/clusterprofiles/ad-1/packs/manifests",
        r#"{"metadata":{},"spec":{"packs":[{"metadata":{},"spec":{"name":"config","manifests":[
            {"name":"config","uid":"m-1","digest":"d","content":"edited: \"no\"\n"}]}}]}}"#,
    );
    stage.route("/v1/clusterprofiles", r#"{"uid":"un-1"}"#);
    stage.route(
        "/v1/dashboard/clusterprofiles/metadata",
        r#"{"items":[{"metadata":{"name":"strata-live-unpublished","uid":"un-1"}}]}"#,
    );
    stage.route("/v1/clusterprofiles/un-1", "{}");
    stage
}

#[test]
fn draft_sh_creates_a_version_with_no_publish_lists_it_as_a_draft_and_deletes_it() {
    let stage = unpublished_stage(false);
    let (_, steps) = stage.run("draft.sh", &[]);
    passed(
        &steps,
        &[
            "list-published",
            "unpublished-create",
            "unpublished-listed",
            "unpublished-mark",
            "unpublished-delete",
        ],
    );
    let posts = stage.sent("POST", "/v1/clusterprofiles");
    assert_eq!(posts.len(), 1, "one create: {posts:?}");
    let post = &posts[0];
    assert_eq!(
        post["metadata"],
        serde_json::json!({"name": "strata-live-unpublished"})
    );
    assert_eq!(post["spec"]["version"], "1.0.0");
    assert_eq!(
        post["spec"]["template"]["packs"][0]["manifests"],
        serde_json::json!([{"name": "config", "content": "edited: \"no\"\n"}]),
        "a manifest goes with no uid, so the copy gets its own manifest"
    );
    let calls = std::fs::read_to_string(stage.path("curl.log")).unwrap();
    assert!(
        !calls.contains("/v1/clusterprofiles/un-1/publish"),
        "nobody publishes the copy: {calls}"
    );
    assert!(
        calls.ends_with("DELETE /v1/clusterprofiles/un-1\n"),
        "the last request deletes the copy: {calls}"
    );
}

#[test]
fn draft_sh_fails_the_check_when_list_says_that_the_copy_is_published() {
    let stage = unpublished_stage(true);
    let (o, steps) = stage.run("draft.sh", &[]);
    assert!(steps.contains("FAIL  unpublished-listed"), "{steps}");
    assert_ne!(o.status.code(), Some(0));
}

/// Stage 7 of `packs.sh` against a listing of `loki` whose 2.10.2 names `amd64,arm64` and whose
/// 3.6.7 names `amd64`, with `behind` of the record of 2.10.2 answering `line` and `json`.
fn packs_arch_stage(line: &str, json: &str) -> String {
    let mut stage = Stage::new(serde_json::json!({
        "csi_pack_name": "csi-aws-ebs", "csi_previous_version": "1.63.1",
        "csi_version": "1.65.0", "cloud": "aws",
    }));
    stage.route(
        "/v1/packs",
        r#"{"items":[{"spec":{"version":"2.10.2","annotations":{"architecture":"amd64,arm64"}}},
            {"spec":{"version":"3.6.7","annotations":{"architecture":"amd64"}}}]}"#,
    );
    stage.strata(
        "snapshot pack:Public Repo/loki@2.10.2 -o loki.json",
        0,
        "{}",
    );
    stage.strata("behind loki.json --no-impact --json", 1, json);
    stage.strata("behind loki.json --no-impact", 1, line);
    let (_, steps) = stage.run("packs.sh", &[]);
    steps
}

#[test]
fn packs_sh_stage_7_says_that_the_move_to_the_newer_version_drops_arm64() {
    let steps = packs_arch_stage(
        "1 pin from 1 record: 1 behind\n~ [addon] loki  2.10.2  Public Repo  behind: 3.6.7 (drops arm64)\n",
        r#"{"pins":[{"version":"2.10.2","newer":[{"version":"3.6.7","drops":["arm64"]}]}]}"#,
    );
    passed(
        &steps,
        &[
            "arch-listing",
            "arch-record",
            "arch-behind",
            "arch-behind-says",
            "arch-behind-json",
        ],
    );

    // a behind that says nothing of the architectures fails the two checks that read it
    let steps = packs_arch_stage(
        "1 pin from 1 record: 1 behind\n~ [addon] loki  2.10.2  Public Repo  behind: 3.6.7\n",
        r#"{"pins":[{"version":"2.10.2","newer":[{"version":"3.6.7"}]}]}"#,
    );
    passed(&steps, &["arch-listing", "arch-record", "arch-behind"]);
    assert!(steps.contains("FAIL  arch-behind-says"), "{steps}");
    assert!(steps.contains("FAIL  arch-behind-json"), "{steps}");
}

/// A run of `registry-kinds.sh` against the stand-ins: the tools of the registries do nothing,
/// and strata answers `packs` of the Zarf pack with `packs`.
fn registry_kinds(packs: &str) -> (Stage, String) {
    let mut stage = Stage::new(serde_json::json!({
        "aws_profile": "p", "project_uid": "p-1", "cluster_uid": "",
        "kinds_host": "kinds.example", "kinds_user": "barney",
        "kinds_registries": {"oci-zarf": {"uid": "reg-zarf", "name": "strata-live-oci"}},
    }));
    let bin = stage.path("bin");
    executable(
        &bin.join("tofu"),
        &FAKE_TOFU_STAGE.replace(
            "case \"$*\" in\n",
            "case \"$*\" in\n  *\"output -raw kinds_password\"*) echo pw ;;\n",
        ),
    );
    for tool in ["oras", "helm", "zarf", "spectro"] {
        executable(&bin.join(tool), "#!/usr/bin/env bash\nexit 0\n");
    }
    stage.route("/v1/registries/pack", r#"{"uid":"reg-packs"}"#);
    stage.strata("packs strata-live-zarf *", 0, packs);
    let (_, steps) = stage.run("registry-kinds.sh", &[]);
    (stage, steps)
}

#[test]
fn registry_kinds_sh_checks_that_packs_says_no_digest_on_the_row_and_in_the_heading_of_the_zarf_pack()
 {
    let row = "  1.0.0  pk-z  oci  all  1.0.0  no digest\n";
    let (stage, steps) = registry_kinds(&format!(
        "strata-live-zarf in strata-live-oci (1 version, no digests)\n{row}"
    ));
    passed(&steps, &["packs-zarf", "packs-zarf-says"]);
    let calls = std::fs::read_to_string(stage.path("strata/calls")).unwrap();
    assert!(
        calls
            .lines()
            .any(|l| l == "packs strata-live-zarf --registry strata-live-oci"),
        "{calls}"
    );

    // a heading with no mark, or a row with no mark, fails the check
    let failed = |steps: &str| {
        steps.lines().any(|l| {
            l.starts_with("FAIL") && l.split_whitespace().nth(1) == Some("packs-zarf-says")
        })
    };
    let (_, steps) = registry_kinds(&format!(
        "strata-live-zarf in strata-live-oci (1 version)\n{row}"
    ));
    assert!(failed(&steps), "{steps}");
    let (_, steps) = registry_kinds(
        "strata-live-zarf in strata-live-oci (1 version, no digests)\n  1.0.0  pk-z  oci  all  1.0.0\n",
    );
    assert!(failed(&steps), "{steps}");
}

/// The hook of the fake name route: HTTP 500 and `VersionParseFailure` for the version
/// 2.2.2.develop, as Palette answers it, and an empty answer for each other version.
const NAME_ROUTE_HOOK: &str = r#"#!/usr/bin/env bash
route="$FAKE_ROUTES/v1_clusterprofiles_validate_name"
if grep -q 'version=2.2.2.develop' "$FAKE_ROUTES/sent/GET_v1_clusterprofiles_validate_name.request"; then
  echo 500 > "$route.status"
  printf '%s' '{"code":"VersionParseFailure","message":"Unable to parse the version '"'"'2.2.2.develop'"'"'"}'
else
  rm -f "$route.status"
fi
"#;

#[test]
fn validate_packs_sh_stage_4_asks_the_two_routes_for_each_rule_that_they_answer() {
    let mut stage = Stage::new(serde_json::json!({}));
    executable(
        &stage.path("routes/v1_clusterprofiles_validate_name.sh"),
        NAME_ROUTE_HOOK,
    );
    for (file, code, message) in [
        (
            "validate-bad-version",
            "VersionParseFailure",
            "Unable to parse the version '2.2.2.develop'",
        ),
        (
            "validate-pack-twice",
            "DuplicatePacksForbidden",
            "Duplicate packs 'cert-manager' are forbidden",
        ),
        (
            "validate-same-pack-twice",
            "610",
            "packs in body shouldn't contain duplicates",
        ),
        (
            "validate-manifest-name",
            "InvalidName",
            "Invalid name 'Wilma_1'",
        ),
        (
            "validate-manifest-twice",
            "DuplicatePackManifestsForbidden",
            "Pack 'cert-manager' contains duplicate manifests 'wilma'",
        ),
    ] {
        stage.strata_err(
            &format!("snapshot --from-candidate {file}.yaml --validate *"),
            2,
            &format!("palette    refuses: {code}: {message}\n"),
        );
    }

    let (o, steps) = stage.run("validate-packs.sh", &["4"]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    assert!(!steps.contains("FAIL"), "{steps}");
    let mut ids = vec![
        "validate-name-free",
        "validate-name-short",
        "validate-name-version",
        "validate-name-version-says",
    ];
    let files = [
        "validate-bad-version",
        "validate-pack-twice",
        "validate-same-pack-twice",
        "validate-manifest-name",
        "validate-manifest-twice",
    ];
    let says: Vec<String> = files.iter().map(|f| format!("{f}-says")).collect();
    let none: Vec<String> = files.iter().map(|f| format!("{f}-no-record")).collect();
    ids.extend(files);
    ids.extend(says.iter().map(String::as_str));
    ids.extend(none.iter().map(String::as_str));
    passed(&steps, &ids);
    assert!(
        !steps.contains("== a candidate whose"),
        "stage 4 runs alone: {steps}"
    );

    // each candidate has the fault that its check names
    let candidate = |f: &str| -> serde_json::Value {
        let text = std::fs::read_to_string(stage.path(&format!("out/{f}.yaml"))).unwrap();
        serde_yaml::from_str(&text).unwrap()
    };
    assert_eq!(
        candidate("validate-bad-version")["version"],
        "2.2.2.develop"
    );
    let twice = candidate("validate-pack-twice");
    assert_eq!(twice["layers"][0]["name"], twice["layers"][1]["name"]);
    assert_ne!(twice["layers"][0]["values"], twice["layers"][1]["values"]);
    let same = candidate("validate-same-pack-twice");
    assert_eq!(same["layers"][0], same["layers"][1]);
    assert_eq!(
        candidate("validate-manifest-name")["layers"][0]["manifests"][0]["name"],
        "Wilma_1"
    );
    let manifests = &candidate("validate-manifest-twice")["layers"][0]["manifests"];
    assert_eq!(manifests[0]["name"], manifests[1]["name"]);
}

#[test]
fn validate_packs_sh_stage_3_asks_for_a_candidate_with_the_name_and_the_version_of_profile() {
    let mut stage = Stage::new(serde_json::json!({}));
    stage.env("PROFILE", "strata-live-profile@1.0.0");
    stage.strata_err(
        "snapshot strata-live-profile@1.0.0 --validate *",
        0,
        "palette    accepts\n",
    );
    stage.strata_err(
        "snapshot --from-candidate exists.yaml --validate *",
        2,
        "palette    refuses: ClusterProfileAlreadyExists: Cluster Profile with name \
         'strata-live-profile' and version '1.0.0' already exists\n",
    );

    let (o, steps) = stage.run("validate-packs.sh", &["3"]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    passed(
        &steps,
        &[
            "validate-version",
            "validate-version-says",
            "validate-exists",
            "validate-exists-says",
        ],
    );
    let text = std::fs::read_to_string(stage.path("out/exists.yaml")).unwrap();
    let exists: serde_json::Value = serde_yaml::from_str(&text).unwrap();
    assert_eq!(exists["name"], "strata-live-profile");
    assert_eq!(exists["version"], "1.0.0");
}
