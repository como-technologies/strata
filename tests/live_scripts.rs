//! The scripts of the live example under examples/live/.
//! No network: every host and every command outside this repository is a recording stand-in.

use crate::common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::*;
use strata::api::codes::UPGRADE_NOT_SUPPORTED_CODE;

// ---- examples/live helpers -------------------------------------------------------------------

/// A `tofu` that reports the host and whether a key arrived, never the key itself.
const FAKE_TOFU: &str = r#"#!/usr/bin/env bash
echo "host=${SPECTROCLOUD_HOST-<unset>} key=${SPECTROCLOUD_APIKEY:+set} args=$*"
"#;

fn tf_sh(envs: &[(&str, &str)], palette_env: Option<&str>) -> Output {
    let dir = tempfile::tempdir().unwrap();
    let bin = fake_bin(dir.path(), &[("tofu", FAKE_TOFU)]);
    let home = dir.path().join("home");
    std::fs::create_dir(&home).unwrap();
    if let Some(text) = palette_env {
        std::fs::write(home.join(".palette.env"), text).unwrap();
    }
    let mut c = Command::new("bash");
    c.arg(live_script("tf.sh"))
        .arg("output")
        .env("PATH", path_with(&bin))
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
    tenant_run(args, status, body, false)
}

/// Run tenant.sh with one canned answer. When `old_jq` is true, `jq` acts as jq 1.6.
fn tenant_run(args: &[&str], status: &str, body: &str, old_jq: bool) -> (Output, String) {
    let dir = tempfile::tempdir().unwrap();
    let mut commands = vec![("curl", FAKE_CURL)];
    if old_jq {
        commands.push(("jq", JQ_1_6));
    }
    let bin = fake_bin(dir.path(), &commands);
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
        .env("PATH", path_with(&bin))
        .env("HOME", dir.path())
        .env("PALETTE_API_KEY", "k")
        .env("PALETTE_PROJECT_UID", "p-1")
        .env("FAKE_CURL_STATUS", &status_file)
        .env("FAKE_CURL_BODY", &body_file)
        .env("FAKE_CURL_LOG", &log)
        .output()
        .unwrap();
    let urls = std::fs::read_to_string(&log).unwrap();
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
/// status 200, or 404 when there is no such file.
///
/// A `<key>.status` file overrides the status. A `<key>.keyed.status` file overrides it again for
/// a request with an ApiKey header. A `<key>.as-<ApiKey>.status` file overrides it for a request
/// whose ApiKey header holds that value, and the file `<key>.as-<ApiKey>` is the body of that
/// answer. When an executable $FAKE_ROUTES/<route key>.sh
/// exists, its output is the body of each answer. It logs each request as `METHOD path` to
/// $FAKE_LOG. It keeps each request body as $FAKE_ROUTES/sent/<METHOD>_<route key>. It adds the
/// body to $FAKE_ROUTES/sent/<METHOD>_<route key>.all, in the order of the requests. It keeps
/// the query and the headers as $FAKE_ROUTES/sent/<METHOD>_<route key>.request. The line of the
/// ApiKey header holds `(hidden)` in place of the key.
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
printf 'query=%s\n%s' "$query" "$headers" | sed 's/^ApiKey:.*/ApiKey: (hidden)/' > "$FAKE_ROUTES/sent/${method}_$key.request"
body="$FAKE_ROUTES/$key"; status=404
[ -x "$body.sh" ] && "$body.sh" > "$body"
[ -f "$body" ] && status=200 || body=/dev/null
[ -f "$FAKE_ROUTES/$key.status" ] && status=$(cat "$FAKE_ROUTES/$key.status")
case "$headers" in *ApiKey:*) [ -f "$FAKE_ROUTES/$key.keyed.status" ] && status=$(cat "$FAKE_ROUTES/$key.keyed.status") ;; esac
apikey=$(printf '%s' "$headers" | sed -n 's/^ApiKey: //p')
if [ -n "$apikey" ] && [ -f "$FAKE_ROUTES/$key.as-$apikey.status" ]; then
  status=$(cat "$FAKE_ROUTES/$key.as-$apikey.status"); body="$FAKE_ROUTES/$key.as-$apikey"
  [ -f "$body" ] || body=/dev/null
fi
if [ -n "$out" ]; then cp "$body" "$out"; else cat "$body"; fi
[ -n "$write" ] && printf '%s' "$status"
exit 0
"#;

/// A `helm` whose `pull ... -d DIR` writes DIR/<name>-<version>.tgz with "chart archive\n".
///
/// It fails when $FAKE_HELM_FAIL has a value. It logs its arguments and the Helm home that it
/// gets to $FAKE_TOOL_LOG.
const FAKE_HELM: &str = r#"#!/usr/bin/env bash
echo "$* HELM_CONFIG_HOME=$HELM_CONFIG_HOME" >> "$FAKE_TOOL_LOG"
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

/// A `crane` whose `digest REF` prints a fixed digest.
///
/// It fails when $FAKE_CRANE_FAIL has a value, and logs its arguments to $FAKE_TOOL_LOG.
const FAKE_CRANE: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_TOOL_LOG"
[ -z "${FAKE_CRANE_FAIL:-}" ] || { echo "UNAUTHORIZED: authentication required" >&2; exit 1; }
echo "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
"#;

/// sha256 of "chart archive\n", the archive the fake helm writes.
const FAKE_CHART_SHA256: &str = "c7b3bbf2a53bd8a37070529402e843305d2cd0a95abd86cb31f9f3d2629382ed";

struct Digest {
    out: Output,
    /// What helm and crane logged.
    log: String,
    /// The TMPDIR tenant.sh ran with, and the entries it left there.
    tmp: String,
    tmp_left: usize,
}

/// Run `tenant.sh digest` with `args` against the `routes` of Palette and the stand-ins of helm
/// and crane, with `envs` in its environment.
fn tenant_digest(args: &[&str], routes: &[(&str, &str)], envs: &[(&str, &str)]) -> Digest {
    let dir = tempfile::tempdir().unwrap();
    fake_bin(dir.path(), &[("helm", FAKE_HELM), ("crane", FAKE_CRANE)]);
    let route_dir = dir.path().join("routes");
    std::fs::create_dir(&route_dir).unwrap();
    for (key, body) in routes {
        std::fs::write(route_dir.join(key), body).unwrap();
    }
    let log = dir.path().join("tools.log");
    // helm and crane write the log only when the script runs them. An empty log means no call, and
    // the `unwrap` below stops the test when the log is gone.
    std::fs::write(&log, "").unwrap();
    let log_path = log.display().to_string();
    let mut env = vec![("FAKE_TOOL_LOG", log_path.as_str())];
    env.extend_from_slice(envs);
    let args: Vec<&str> = std::iter::once("digest")
        .chain(args.iter().copied())
        .collect();
    let out = run_live("tenant.sh", &args, dir.path(), &env);
    let tmp = dir.path().join("tmp");
    Digest {
        out,
        log: std::fs::read_to_string(&log).unwrap(),
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

/// The sync route of `reg-1` for FAKE_API_CURL: Palette refuses the first $SYNC_REFUSALS
/// requests with HTTP 500 and `OciSyncInProgress`, and takes each request after them.
const FAKE_SYNC_ROUTE: &str = r#"#!/usr/bin/env bash
key=v1_registries_oci_reg-1_ecr_sync
n=$(cat "$FAKE_ROUTES/$key.calls" 2>/dev/null || echo 0); n=$((n + 1)); echo "$n" > "$FAKE_ROUTES/$key.calls"
if [ "$n" -le "${SYNC_REFUSALS:-0}" ]; then
  echo 500 > "$FAKE_ROUTES/$key.status"
  echo '{"code":"OciSyncInProgress","message":"another sync process is already in progress"}'
else
  rm -f "$FAKE_ROUTES/$key.status"
  echo '{}'
fi
"#;

#[test]
fn api_sh_sync_waits_while_a_sync_of_the_registry_runs_and_asks_again() {
    let root = tempfile::tempdir().unwrap();
    let routes = root.path().join("routes");
    std::fs::create_dir(&routes).unwrap();
    executable(
        &routes.join("v1_registries_oci_reg-1_ecr_sync.sh"),
        FAKE_SYNC_ROUTE,
    );
    // each read of the sync status gives a new run time, so the sync ends at once
    executable(
        &routes.join("v1_registries_oci_reg-1_ecr_sync_status.sh"),
        "#!/usr/bin/env bash\necho \"{\\\"status\\\":\\\"Completed\\\",\\\"lastRunTime\\\":\\\"$(date +%s%N)\\\"}\"\n",
    );
    let log = root.path().join("curl.log");
    let sync = |refusals: &str| {
        std::fs::write(&log, "").unwrap();
        let _ = std::fs::remove_file(routes.join("v1_registries_oci_reg-1_ecr_sync.calls"));
        run_live(
            "api.sh",
            &["sync-oci", "reg-1"],
            root.path(),
            &[("SYNC_WAIT", "0"), ("SYNC_REFUSALS", refusals)],
        )
    };
    let posts = || {
        lines_of(&log)
            .iter()
            .filter(|l| l.as_str() == "POST /v1/registries/oci/reg-1/ecr/sync")
            .count()
    };

    // two refusals: the third request is taken, and the sync ends
    let o = sync("2");
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert_eq!(posts(), 3, "{}", std::fs::read_to_string(&log).unwrap());
    assert!(
        text(&o.stdout).contains(r#""status":"Completed""#),
        "{}",
        text(&o.stdout)
    );

    // Palette refuses each request for five minutes: exit 2, and the reason
    let o = sync("61");
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stderr));
    assert_eq!(posts(), 60);
    assert!(
        text(&o.stderr).contains("a sync of registry reg-1 is in progress"),
        "{}",
        text(&o.stderr)
    );

    // each other refusal is an error at once
    std::fs::remove_file(routes.join("v1_registries_oci_reg-1_ecr_sync.sh")).unwrap();
    std::fs::write(
        routes.join("v1_registries_oci_reg-1_ecr_sync"),
        r#"{"code":"OciRegistryNotFound"}"#,
    )
    .unwrap();
    std::fs::write(
        routes.join("v1_registries_oci_reg-1_ecr_sync.status"),
        "500",
    )
    .unwrap();
    let o = sync("0");
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stderr));
    assert_eq!(posts(), 1);
    assert!(
        text(&o.stderr).contains("answered HTTP 500"),
        "{}",
        text(&o.stderr)
    );
}

/// The routes of the add-on profile `prof-1` for the stand-in curl.
///
/// The profile has one pack `config` with one manifest `m-1`, whose content says `edited: "no"`.
/// The publish route answers.
fn addon_profile_routes(routes: &Path) {
    std::fs::create_dir_all(routes).unwrap();
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
}

#[test]
fn api_sh_edit_keeps_each_manifest_uid_so_clusters_keep_their_copies() {
    let root = tempfile::tempdir().unwrap();
    let routes = root.path().join("routes");
    addon_profile_routes(&routes);
    let o = run_live(
        "api.sh",
        &["edit", "prof-1", r#"edited: "no""#, r#"edited: "yes""#],
        root.path(),
        &[],
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let calls = std::fs::read_to_string(root.path().join("curl.log")).unwrap();
    assert!(
        calls.contains("PATCH /v1/clusterprofiles/prof-1/publish"),
        "{calls}"
    );
    let put = read_json(&routes.join("sent/PUT_v1_clusterprofiles_prof-1"));
    assert_eq!(
        put["spec"]["template"]["packs"][0]["manifests"],
        serde_json::json!([{"name": "config", "uid": "m-1", "content": "edited: \"yes\"\n"}]),
        "a manifest sent without its uid is a new manifest, and a cluster's copy of the old one is dropped when the cluster's update is accepted"
    );
}

#[test]
fn api_sh_draft_puts_the_change_with_no_publish_and_publish_publishes_it() {
    let root = tempfile::tempdir().unwrap();
    let routes = root.path().join("routes");
    addon_profile_routes(&routes);
    let log = root.path().join("curl.log");
    let api = |args: &[&str]| run_live("api.sh", args, root.path(), &[]);
    let o = api(&["draft", "prof-1", r#"edited: "no""#, r#"edited: "yes""#]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let calls = std::fs::read_to_string(&log).unwrap();
    assert!(calls.contains("PUT /v1/clusterprofiles/prof-1"), "{calls}");
    assert!(
        !calls.contains("publish"),
        "a draft is not published: {calls}"
    );
    let put = read_json(&routes.join("sent/PUT_v1_clusterprofiles_prof-1"));
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
    let root = tempfile::tempdir().unwrap();
    let routes = root.path().join("routes");
    std::fs::create_dir(&routes).unwrap();
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
    let o = run_live(
        "api.sh",
        &["delete-named", "strata-live-addon", "strata-live-move"],
        root.path(),
        &[],
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let deletes: Vec<String> = lines_of(&root.path().join("curl.log"))
        .into_iter()
        .filter(|l| l.starts_with("DELETE "))
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

/// The routes of the pack listing for `api.sh packs-where`, with `count` packs in all. The page at
/// offset 0 holds `first`, and the page at offset 100 holds `second`. The listing logs each query
/// in `queries`.
fn packs_pages(routes: &Path, count: u32, first: serde_json::Value, second: serde_json::Value) {
    std::fs::create_dir_all(routes).unwrap();
    std::fs::write(routes.join("page-0.json"), first.to_string()).unwrap();
    std::fs::write(routes.join("page-100.json"), second.to_string()).unwrap();
    std::fs::write(
        routes.join("count"),
        serde_json::json!({"listmeta": {"count": count}}).to_string(),
    )
    .unwrap();
    executable(
        &routes.join("v1_packs.sh"),
        r#"#!/usr/bin/env bash
request="$FAKE_ROUTES/sent/GET_v1_packs.request"
sed -n 's/^query=//p' "$request" >> "$FAKE_ROUTES/queries"
offset=$(sed -n 's/^query=.*offset=\([0-9]*\).*/\1/p' "$request")
jq -s '.[0] * {listmeta: .[1].listmeta}' "$FAKE_ROUTES/page-${offset}.json" "$FAKE_ROUTES/count"
"#,
    );
}

/// A pack of the listing: its name, version, and registry, and the uid and the values of the pack.
fn listed_pack(name: &str, version: &str, registry: &str) -> serde_json::Value {
    serde_json::json!({"metadata": {"uid": format!("pk-{name}-{version}")},
        "spec": {"name": name, "version": version, "registryUid": registry, "type": "spectro",
            "layer": "addon", "values": format!("values of {name}\n")}})
}

#[test]
fn api_sh_packs_where_reads_a_page_at_a_time_and_stops_at_the_first_pack_that_matches() {
    let root = tempfile::tempdir().unwrap();
    let routes = root.path().join("routes");
    packs_pages(
        &routes,
        250,
        serde_json::json!({"items": [listed_pack("gloo", "1.0.0", "reg-other")]}),
        serde_json::json!({"items": [
            listed_pack("gloo", "1.2.18", "reg-pub"), listed_pack("gloo", "1.2.19", "reg-pub")]}),
    );
    let filter = ".spec.registryUid == $r";
    let o = run_live(
        "api.sh",
        &["packs-where", "--arg", "r", "reg-pub", filter],
        root.path(),
        &[],
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let found: serde_json::Value = serde_json::from_str(text(&o.stdout).trim()).unwrap();
    assert_eq!(
        found,
        listed_pack("gloo", "1.2.18", "reg-pub"),
        "one pack, the first that matches: {}",
        text(&o.stdout)
    );
    assert_eq!(
        text(&std::fs::read(routes.join("queries")).unwrap()),
        "limit=100&offset=0\nlimit=100&offset=100\n",
        "the page at offset 200 is not read: the match is on the page at offset 100"
    );
}

#[test]
fn api_sh_packs_where_limits_the_listing_to_a_name_and_exits_1_when_no_pack_matches() {
    let root = tempfile::tempdir().unwrap();
    let routes = root.path().join("routes");
    packs_pages(
        &routes,
        101,
        serde_json::json!({"items": [listed_pack("gloo", "1.0.0", "reg-other")]}),
        serde_json::json!({"items": [listed_pack("gloo", "1.0.1", "reg-other")]}),
    );
    let o = run_live(
        "api.sh",
        &[
            "packs-where",
            "--name",
            "gloo",
            ".spec.registryUid == \"reg-pub\"",
        ],
        root.path(),
        &[],
    );
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    assert_eq!(text(&o.stdout), "", "no pack, no line");
    assert_eq!(
        text(&std::fs::read(routes.join("queries")).unwrap()),
        "filters=spec.name=gloo&limit=100&offset=0\nfilters=spec.name=gloo&limit=100&offset=100\n",
        "the name goes to Palette, and each page is read"
    );
}

#[test]
fn api_sh_packs_where_exits_2_when_palette_refuses_a_page() {
    let root = tempfile::tempdir().unwrap();
    let routes = root.path().join("routes");
    packs_pages(
        &routes,
        1,
        serde_json::json!({"items": []}),
        serde_json::json!({}),
    );
    std::fs::write(routes.join("v1_packs.status"), "500").unwrap();
    let o = run_live("api.sh", &["packs-where", "true"], root.path(), &[]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stdout));
    assert!(
        text(&o.stderr).contains("GET /v1/packs?limit=100&offset=0 answered HTTP 500"),
        "{}",
        text(&o.stderr)
    );
}

/// The body of an add-on profile that `api.sh` sent to Palette has this shape, whichever command
/// built it. `pack` is its one pack.
fn assert_addon_body(sent: &serde_json::Value, name: &str, cloud: &str, pack: serde_json::Value) {
    assert_eq!(
        *sent,
        serde_json::json!({"metadata": {"name": name},
            "spec": {"version": "1.0.0", "template": {"type": "add-on", "cloudType": cloud,
                "packs": [pack]}}})
    );
}

#[test]
fn api_sh_pin_profile_and_probe_profile_send_the_same_shape_of_add_on_profile() {
    let root = tempfile::tempdir().unwrap();
    let routes = root.path().join("routes");
    packs_pages(
        &routes,
        1,
        serde_json::json!({"items": [listed_pack("gloo", "1.2.18", "reg-pub")]}),
        serde_json::json!({}),
    );
    std::fs::write(routes.join("v1_clusterprofiles"), r#"{"uid":"prof-9"}"#).unwrap();
    std::fs::write(routes.join("v1_clusterprofiles_prof-9_publish"), "{}").unwrap();
    std::fs::write(routes.join("v1_clusterprofiles_prof-9"), "{}").unwrap();
    let values = root.path().join("values.yaml");
    std::fs::write(&values, "a: 1\n").unwrap();
    let sent = || read_json(&routes.join("sent/POST_v1_clusterprofiles"));

    // pin-profile: the pack that the registry lists gives its uid, and the profile has the cloud
    let o = run_live(
        "api.sh",
        &[
            "pin-profile",
            "pinned",
            "reg-pub",
            "spectro",
            "gloo",
            "1.2.18",
            values.to_str().unwrap(),
            "addon",
            "1.2.x",
            "aws",
        ],
        root.path(),
        &[],
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert_eq!(text(&o.stdout), "prof-9\n");
    assert_addon_body(
        &sent(),
        "pinned",
        "aws",
        serde_json::json!({"name": "gloo", "tag": "1.2.x", "registryUid": "reg-pub",
            "type": "spectro", "layer": "addon", "values": "a: 1\n", "uid": "pk-gloo-1.2.18"}),
    );

    // the registry lists no such version: the pack has no uid
    let o = run_live(
        "api.sh",
        &["pin-profile", "pinned", "reg-pub", "oci", "gloo", "9.9.9"],
        root.path(),
        &[],
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert_addon_body(
        &sent(),
        "pinned",
        "all",
        serde_json::json!({"name": "gloo", "tag": "9.9.9", "registryUid": "reg-pub",
            "type": "oci", "layer": "addon", "values": ""}),
    );

    // probe-profile: the listing gives the whole pack, and Palette takes the profile
    let o = run_live(
        "api.sh",
        &["probe-profile", "gloo", "reg-pub", "1.2.18"],
        root.path(),
        &[],
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert_eq!(text(&o.stdout), "accepted\n");
    assert_addon_body(
        &sent(),
        "strata-probe-gloo",
        "all",
        serde_json::json!({"name": "gloo", "tag": "1.2.18", "uid": "pk-gloo-1.2.18",
            "registryUid": "reg-pub", "type": "spectro", "layer": "addon",
            "values": "values of gloo\n"}),
    );
    let calls = std::fs::read_to_string(root.path().join("curl.log")).unwrap();
    assert!(
        calls.ends_with("DELETE /v1/clusterprofiles/prof-9\n"),
        "{calls}"
    );

    // a registry that lists no such version: exit 2, and no profile
    let before = calls.matches("POST /v1/clusterprofiles").count();
    let o = run_live(
        "api.sh",
        &["probe-profile", "gloo", "reg-pub", "9.9.9"],
        root.path(),
        &[],
    );
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("registry reg-pub lists no gloo@9.9.9"),
        "{}",
        text(&o.stderr)
    );
    let calls = std::fs::read_to_string(root.path().join("curl.log")).unwrap();
    assert_eq!(calls.matches("POST /v1/clusterprofiles").count(), before);
}

/// A script that uses the helpers of lib.sh the way a script of the live example does. It prints
/// its usage, and then the heading of stage 1 and of stage 2, and writes summary.md.
fn lib_demo(dir: &Path, strata: &str, args: &[&str]) -> Output {
    let script = dir.join("demo.sh");
    executable(
        &script,
        &format!(
            r#"#!/usr/bin/env bash
# The demo of lib.sh.
#
#   demo.sh [FROM]
#
# It prints the usage.
set -uo pipefail
script=demo
from=${{1:-1}}
. '{lib}'
run=$PWD/runs/20260101T000000Z/demo
mkdir -p "$run" && cd "$run" || exit 2
usage
stage 1 "first" || echo "no heading for stage 1"
stage 2 "second" || echo "no heading for stage 2"
[ -z "${{2:-}}" ] || run_id=$2
check demo-ok 0 'it passes' -- true
write_summary "the demo" ", cluster \`c-1\`," "add-on \`a-1\`."
cat summary.md
echo "$addon_from -> $addon_to"
"#,
            lib = live_script("lib.sh").display()
        ),
    );
    Command::new("bash")
        .arg(&script)
        .args(args)
        .current_dir(dir)
        .env("STRATA_BIN", strata)
        .output()
        .unwrap()
}

#[test]
fn lib_sh_prints_the_usage_and_the_heading_of_a_stage_and_writes_the_summary_of_a_run() {
    let dir = tempfile::tempdir().unwrap();
    let strata = dir.path().join("strata");
    executable(&strata, "#!/usr/bin/env bash\necho 'strata 9.9.9'\n");
    let o = lib_demo(dir.path(), strata.to_str().unwrap(), &["2"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert_eq!(
        text(&o.stdout),
        format!(
            "The demo of lib.sh.\n\
         \n\
         \x20 demo.sh [FROM]\n\
         \n\
         It prints the usage.\n\
         no heading for stage 1\n\
         == stage 2: second\n\
         PASS  {:<32} expected 0, got 0\n\
         # the demo\n\
         \n\
         Run `20260101T000000Z` with `strata 9.9.9`, cluster `c-1`, add-on `a-1`.\n\
         \n\
         | result | check | expected exit | got | what it shows |\n\
         |---|---|---|---|---|\n\
         | PASS | `demo-ok` | 0 | 0 | it passes |\n\
         edited: \"no\" -> edited: \"yes\"\n",
            "demo-ok"
        ),
        "{}",
        text(&o.stderr)
    );
    let steps =
        std::fs::read_to_string(dir.path().join("runs/20260101T000000Z/demo/steps.log")).unwrap();
    assert!(steps.contains("== stage 2: second\n"), "{steps}");

    // a script that sets run_id names its own run, and a missing binary leaves out the version
    let o = lib_demo(dir.path(), "/no/such/strata", &["1", "custom-run"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert!(
        text(&o.stdout).contains("\nRun `custom-run`, cluster `c-1`, add-on `a-1`.\n"),
        "{}",
        text(&o.stdout)
    );
}

/// The route that gives the release, as Palette answers it: the release to a call with no key,
/// and HTTP 500 to a call with an ApiKey header.
fn release_route(routes: &Path) {
    std::fs::create_dir_all(routes).unwrap();
    std::fs::write(
        routes.join("v1_mgmt_app_version"),
        r#"{"version":{"installedVersion":"4.10.17-patch.1"}}"#,
    )
    .unwrap();
    std::fs::write(routes.join("v1_mgmt_app_version.keyed.status"), "500").unwrap();
}

#[test]
fn api_sh_app_version_sends_no_key_and_no_project_header_with_no_key() {
    let root = tempfile::tempdir().unwrap();
    let routes = root.path().join("routes");
    release_route(&routes);
    let request =
        || std::fs::read_to_string(routes.join("sent/GET_v1_mgmt_app_version.request")).unwrap();

    let o = run_live("api.sh", &["app-version", "--no-key"], root.path(), &[]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert!(
        text(&o.stdout).contains(r#""installedVersion":"4.10.17-patch.1""#),
        "{}",
        text(&o.stdout)
    );
    let sent = request();
    assert!(!sent.contains("ApiKey"), "no key: {sent}");
    assert!(!sent.contains("ProjectUid"), "no project: {sent}");

    let o = run_live("api.sh", &["app-version"], root.path(), &[]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stdout));
    assert!(
        text(&o.stderr).contains("GET /v1/mgmt/app/version answered HTTP 500"),
        "{}",
        text(&o.stderr)
    );
    let sent = request();
    assert!(sent.contains("ApiKey: (hidden)"), "the key: {sent}");
    assert!(!sent.contains("ProjectUid"), "no project: {sent}");

    let o = run_live("api.sh", &["app-version", "--key"], root.path(), &[]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("usage: api.sh app-version [--no-key]"),
        "{}",
        text(&o.stderr)
    );
}

#[test]
fn release_sh_expects_the_release_with_no_key_and_a_refusal_with_the_key() {
    let ids = [
        "release-no-key",
        "release-no-key-says",
        "release-with-key",
        "release-with-key-says",
    ];
    let stage = Stage::new(baseline_outputs());
    release_route(&stage.path("routes"));
    let (o, steps) = stage.run("release.sh", &[]);
    assert_eq!(o.status.code(), Some(0), "{steps}");
    passed(&steps, &ids);

    // Palette answers a call with the key too: the run fails on the refusal
    let stage = Stage::new(baseline_outputs());
    release_route(&stage.path("routes"));
    std::fs::remove_file(stage.path("routes/v1_mgmt_app_version.keyed.status")).unwrap();
    let (o, steps) = stage.run("release.sh", &[]);
    assert_eq!(o.status.code(), Some(1), "{steps}");
    assert!(
        steps
            .lines()
            .any(|l| l.starts_with("FAIL") && l.contains("release-with-key ")),
        "{steps}"
    );

    // Palette refuses a call with no key: the run fails on the release
    let stage = Stage::new(baseline_outputs());
    release_route(&stage.path("routes"));
    std::fs::write(stage.path("routes/v1_mgmt_app_version.status"), "500").unwrap();
    let (o, steps) = stage.run("release.sh", &[]);
    assert_eq!(o.status.code(), Some(1), "{steps}");
    assert!(
        steps
            .lines()
            .any(|l| l.starts_with("FAIL") && l.contains("release-no-key ")),
        "{steps}"
    );

    let o = stage.run("release.sh", &["--help"]).0;
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stdout).contains("The release test (#146)"),
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
    let bin = fake_bin(root, &[("curl", FAKE_API_CURL), ("kubectl", FAKE_KUBECTL)]);
    let tmp = root.join("tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    let mut c = Command::new("bash");
    c.arg(live_script(script))
        .args(args)
        .env("PATH", path_with(&bin))
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
                out.push((rel, std::fs::read_to_string(&p).unwrap()));
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
    let sent =
        |key: &str| std::fs::read_to_string(root.path().join("routes/sent").join(key)).unwrap();
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
    fake_bin(root.path(), &[("tofu", FAKE_TOFU_STATE)]);
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
    let bin = fake_bin(
        root.path(),
        &[
            ("tofu", FAKE_TOFU_BASELINE),
            ("fake-strata", FAKE_STRATA_BASELINE),
        ],
    );
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
    let bin = fake_bin(root.path(), &[("aws", FAKE_AWS)]);
    let fake = root.path().join("aws");
    std::fs::create_dir_all(&fake).unwrap();
    for (op, ids) in answers {
        std::fs::write(fake.join(op), ids).unwrap();
    }
    let o = Command::new("bash")
        .arg(live_script("aws-inventory.sh"))
        .args(["fred", "us-east-2"])
        .env("PATH", path_with(&bin))
        .env("FAKE_AWS", &fake)
        .output()
        .unwrap();
    let calls = std::fs::read_to_string(fake.join("calls.log")).unwrap();
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
    let root = tempfile::tempdir().unwrap();
    let routes = root.path().join("routes");
    std::fs::create_dir(&routes).unwrap();
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
    let run = |args: &[&str]| run_live("api.sh", args, root.path(), &[]);
    let o = run(&["retag", "prof-1", "ubuntu-aws", "LTS__24.04", "24.04"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let put = read_json(&routes.join("sent/PUT_v1_clusterprofiles_prof-1"));
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
    let calls = std::fs::read_to_string(root.path().join("curl.log")).unwrap();
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
    let root = tempfile::tempdir().unwrap();
    let routes = root.path().join("routes");
    std::fs::create_dir(&routes).unwrap();
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
    let run = |args: &[&str]| run_live("api.sh", args, root.path(), &[]);
    let o = run(&["app-profile-order", "ap-1", "2"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let put = read_json(&routes.join("sent/PUT_v1_appProfiles_ap-1_tiers_t-1"));
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
    let sent = || read_json(&routes.join("sent/POST_v1_spectroclusters_c-1_validate_repave"));

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

#[test]
fn tenant_sh_takes_an_empty_answer_of_palette_as_no_answer_with_each_jq() {
    for old_jq in [false, true] {
        let case = format!("old jq: {old_jq}");

        // a 404 with no body is a gateway problem, not a deleted cluster
        let (o, _) = tenant_run(&["cluster", "c-123"], "404", "", old_jq);
        assert_eq!(o.status.code(), Some(2), "{case}: {}", text(&o.stdout));
        assert!(
            text(&o.stderr).contains("not proof of deletion"),
            "{case}: {}",
            text(&o.stderr)
        );

        // a 200 with no body to the check of the repave has no isRepaveRequired
        let root = tempfile::tempdir().unwrap();
        let routes = root.path().join("routes");
        std::fs::create_dir_all(&routes).unwrap();
        let commands: &[(&str, &str)] = if old_jq { &[("jq", JQ_1_6)] } else { &[] };
        fake_bin(root.path(), commands);
        std::fs::write(
            routes.join("v1_clusterprofiles_prof-2"),
            r#"{"metadata":{"uid":"prof-2"},"spec":{"published":{"packs":[]}}}"#,
        )
        .unwrap();
        std::fs::write(routes.join("v1_spectroclusters_c-1_validate_repave"), "").unwrap();
        let o = run_live(
            "tenant.sh",
            &["repave", "c-1", "prof-1", "prof-2"],
            root.path(),
            &[],
        );
        assert_eq!(o.status.code(), Some(2), "{case}: {}", text(&o.stdout));
        assert!(
            text(&o.stderr).contains("Palette's answer has no isRepaveRequired"),
            "{case}: {}",
            text(&o.stderr)
        );
    }
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
    let dir = live_script("preflight");
    for engine in ["aws", "eks", "gcp", "gke"] {
        let doc = read_json(&dir.join(format!("{engine}.json")));
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

/// Run aws-cleanup.sh with `args` against an `aws` that answers every query with nothing. Gives
/// what it printed, and each call to `aws`.
fn aws_cleanup(args: &[&str]) -> (Output, String) {
    let dir = tempfile::tempdir().unwrap();
    let bin = fake_bin(dir.path(), &[("aws", FAKE_AWS_EMPTY)]);
    let log = dir.path().join("aws.log");
    let o = Command::new("bash")
        .arg(live_script("aws-cleanup.sh"))
        .args(args)
        .env("PATH", path_with(&bin))
        .env("FAKE_AWS_LOG", &log)
        .output()
        .unwrap();
    let calls = std::fs::read_to_string(&log).unwrap();
    (o, calls)
}

#[test]
fn aws_cleanup_takes_yes_in_any_position_and_keeps_the_name_and_the_region() {
    let (o, calls) = aws_cleanup(&["--yes", "barney"]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
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

/// Run `offering_edit` of preflight-plan.sh for `engine`, with the `account_kind` that `create_body`
/// sets, against a `api.sh` that answers every `get` with the file `answer`. Gives the output, and
/// the path that the function asked for.
fn offering_edit(engine: &str, account_kind: &str, region: &str, answer: &str) -> (Output, String) {
    let script = std::fs::read_to_string(live_script("preflight-plan.sh")).unwrap();
    let start = script.find("\noffering_edit() {").unwrap() + 1;
    let end = start + script[start..].find("\n}\n").unwrap() + 3;
    let function = &script[start..end];
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("answer.json"), answer).unwrap();
    executable(
        &dir.path().join("api.sh"),
        "#!/usr/bin/env bash\necho \"$*\" > asked.txt\ncat answer.json\n",
    );
    let o = Command::new("bash")
        .arg("-c")
        .arg(format!(
            "live={live}; engine={engine}; account_kind={account_kind}; region={region}; A=acc-1\n{function}\noffering_edit",
            live = dir.path().display()
        ))
        .current_dir(dir.path())
        .output()
        .unwrap();
    let asked = std::fs::read_to_string(dir.path().join("asked.txt")).unwrap_or_default();
    (o, asked.trim().to_string())
}

/// The fault of `instance-offering` in the plan of the lab (#143) comes from the instance type
/// route: the first type that has a zone in `nonSupportedZones`, for the region and the account of
/// the entity, on the route of the cloud of the engine. A type with `null` has no such zone (#411).
#[test]
fn preflight_plan_makes_the_fault_of_instance_offering_from_the_instance_type_route() {
    let answer = r#"{"instanceTypes":[
        {"type":"t3.large","nonSupportedZones":[]},
        {"type":"c5.large","nonSupportedZones":null},
        {"type":"t3.xlarge","nonSupportedZones":["us-east-2c","us-east-2d"]},
        {"type":"m5.large","nonSupportedZones":["us-east-2a"]}]}"#;
    for (engine, account_kind, region) in [
        ("aws", "aws", "us-east-2"),
        ("eks", "aws", "us-east-2"),
        ("gcp", "gcp", "us-east1"),
        ("gke", "gcp", "us-east1"),
    ] {
        let path = format!(
            "get /v1/clouds/{account_kind}/regions/{region}/instancetypes?cloudAccountUid=acc-1"
        );
        let (o, asked) = offering_edit(engine, account_kind, region, answer);
        assert!(o.status.success(), "{engine}: {}", text(&o.stderr));
        assert_eq!(asked, path, "{engine}");
        // the edit puts the first pool of the clean entity in the first zone that the type lacks
        let edit = text(&o.stdout);
        let pool = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            pool.path(),
            r#"{"spec":{"machinepoolconfig":[{"cloudConfig":{"instanceType":"t3.small","azs":["us-east-2a"]}}]}}"#,
        )
        .unwrap();
        let made = Command::new("jq")
            .args(["-c", edit.trim()])
            .arg(pool.path())
            .output()
            .unwrap();
        assert_eq!(
            text(&made.stdout).trim(),
            r#"{"spec":{"machinepoolconfig":[{"cloudConfig":{"instanceType":"t3.xlarge","azs":["us-east-2c"]}}]}}"#,
            "{engine}"
        );
    }
}

/// The check `instance-types-shape` of the plan tests the field `type`, as Palette names the type
/// of an entry (#404), and takes `nonSupportedZones` as an array or `null` (#411). The answer of
/// the live run passes it. An entry with `name` and no `type` fails it, and so does a field of
/// another type.
#[test]
fn preflight_plan_tests_the_shape_of_the_instance_type_route_by_type() {
    let script = std::fs::read_to_string(live_script("preflight-plan.sh")).unwrap();
    let line = script
        .lines()
        .skip_while(|l| !l.contains("check instance-types-shape"))
        .nth(1)
        .unwrap();
    let filter = &line[line.find('\'').unwrap() + 1..line.rfind('\'').unwrap()];
    let dir = tempfile::tempdir().unwrap();
    for (answer, status) in [
        (
            r#"{"instanceTypes":[{"category":"GPU instance","cpu":32,"memory":488,"price":7.2,"type":"p2.8xlarge","cost":{"price":[{"onDemand":7.2,"os":"linux"}]},"nonSupportedZones":["us-east-2a","us-east-2b","us-east-2c"],"supportedArchitectures":["x86_64"]}]}"#,
            Some(0),
        ),
        // most types of Palette give null: each zone offers the type
        (
            r#"{"instanceTypes":[{"category":"General purpose","cpu":2,"memory":8,"price":0.0832,"type":"t3.large","cost":{"price":[{"onDemand":0.0832,"os":"linux"}]},"nonSupportedZones":null,"supportedArchitectures":["x86_64"]}]}"#,
            Some(0),
        ),
        (
            r#"{"instanceTypes":[{"type":"t3.large","nonSupportedZones":null},{"type":"p2.8xlarge","nonSupportedZones":["us-east-2a"]},{"type":"t3.xlarge","nonSupportedZones":[]}]}"#,
            Some(0),
        ),
        (
            r#"{"instanceTypes":[{"name":"p2.8xlarge","nonSupportedZones":[]}]}"#,
            Some(1),
        ),
        (
            r#"{"instanceTypes":[{"name":"t3.large","nonSupportedZones":null}]}"#,
            Some(1),
        ),
        (
            r#"{"instanceTypes":[{"type":"t3.large","nonSupportedZones":"us-east-2a"}]}"#,
            Some(1),
        ),
        (r#"{"instanceTypes":[]}"#, Some(1)),
    ] {
        std::fs::write(dir.path().join("instancetypes.json"), answer).unwrap();
        let o = Command::new("jq")
            .args(["-e", filter])
            .arg(dir.path().join("instancetypes.json"))
            .output()
            .unwrap();
        assert_eq!(o.status.code(), status, "{answer}");
    }
}

#[test]
fn preflight_plan_makes_no_fault_of_instance_offering_when_the_route_lists_no_zone_that_a_type_lacks()
 {
    for answer in [
        r#"{"instanceTypes":[{"type":"t3.large","nonSupportedZones":[]}]}"#,
        "{}",
    ] {
        let (o, _) = offering_edit("aws", "aws", "us-east-2", answer);
        assert_eq!(o.status.code(), Some(1), "{answer}");
        assert!(
            text(&o.stderr).contains("the route lists no type with a zone in nonSupportedZones"),
            "{}",
            text(&o.stderr)
        );
        assert!(text(&o.stdout).is_empty(), "no edit: {}", text(&o.stdout));
    }
}

/// `create_body` of lib.sh is the one place of the scripts that maps an engine to the kind of its
/// cloud account. It sets `$account_kind`, and `offering_edit` of the plan reads it (#430). The
/// kind picks the account in the summary, so an EKS cluster takes the account of kind `aws`.
#[test]
fn create_body_sets_the_kind_of_the_cloud_account_of_each_engine() {
    let dir = tempfile::tempdir().unwrap();
    let strata = dir.path().join("strata");
    executable(&strata, "#!/usr/bin/env bash\necho 'strata 9.9.9'\n");
    let live = dir.path().join("live");
    std::fs::create_dir(&live).unwrap();
    std::os::unix::fs::symlink(live_script("preflight"), live.join("preflight")).unwrap();
    executable(
        &live.join("api.sh"),
        r#"#!/usr/bin/env bash
echo '{"items":[
  {"metadata":{"name":"lab-aws","uid":"acc-aws"},"kind":"aws"},
  {"metadata":{"name":"strata-live-eks","uid":"acc-eks"},"kind":"aws"},
  {"metadata":{"name":"lab-gcp","uid":"acc-gcp"},"kind":"gcp"}]}'
"#,
    );
    let tfvars = dir.path().join("terraform.tfvars");
    std::fs::write(
        &tfvars,
        "cloud_account_name = \"lab-aws\"\ngcp_cloud_account_name = \"lab-gcp\"\n",
    )
    .unwrap();
    for (engine, kind, account) in [
        ("aws", "aws", "acc-aws"),
        ("eks", "aws", "acc-eks"),
        ("gcp", "gcp", "acc-gcp"),
        ("gke", "gcp", "acc-gcp"),
    ] {
        let o = Command::new("bash")
            .arg("-c")
            .arg(format!(
                ". '{lib}'\nlive={live}\ncreate_body {engine} p-1 fred body.json || exit 1\necho \"$account_kind $A\"",
                lib = live_script("lib.sh").display(),
                live = live.display()
            ))
            .current_dir(dir.path())
            .env("STRATA_BIN", &strata)
            .env("TFVARS", &tfvars)
            .output()
            .unwrap();
        assert!(o.status.success(), "{engine}: {}", text(&o.stderr));
        assert_eq!(text(&o.stdout), format!("{kind} {account}\n"), "{engine}");
        let body = read_json(&dir.path().join("body.json"));
        assert_eq!(body["spec"]["cloudAccountUid"], account, "{engine}");
    }
}

/// The plan puts the fault of the key pair in the check `iaas-pod-identity` on aws, and in
/// `ssh-key-pair` on eks alone (#430). On aws, the properties route refuses a key pair that the
/// region lacks, and `iaas-pod-identity` sends it the key pair.
#[test]
fn preflight_plan_sends_the_missing_key_pair_to_iaas_pod_identity_on_aws_and_to_ssh_key_pair_on_eks()
 {
    let script = std::fs::read_to_string(live_script("preflight-plan.sh")).unwrap();
    let lines: Vec<&str> = script.lines().map(str::trim).collect();
    let edit = r#"'.spec.cloudConfig.sshKeyName = "strata-no-such-key"'"#;
    let faults: Vec<&&str> = lines
        .iter()
        .filter(|l| l.contains("sshKeyName = \"strata-no-such-key\""))
        .collect();
    assert_eq!(
        faults,
        [
            &format!("fault iaas-pod-identity {edit}").as_str(),
            &format!("eks) fault ssh-key-pair {edit} ;;").as_str(),
        ]
    );
    // the edit changes the key pair of the entity of aws, and nothing else
    for engine in ["aws", "eks"] {
        let entity = read_json(&live_script("preflight").join(format!("{engine}.json")));
        assert_eq!(
            entity["spec"]["cloudConfig"]["sshKeyName"], "__SSH_KEY__",
            "{engine}"
        );
    }
    // the check passes on a report that names the code of Palette, as a failed rule or as an error
    let line = lines
        .iter()
        .skip_while(|l| !l.contains("check properties-key-pair"))
        .nth(1)
        .unwrap();
    let filter = &line[line.find('\'').unwrap() + 1..line.rfind('\'').unwrap()];
    let dir = tempfile::tempdir().unwrap();
    for (report, status) in [
        (
            r#"{"checks":[{"id":"iaas-pod-identity","verdict":"failed","messages":["Palette refuses the properties of the AWS cluster: AwsKeyPairNotFound: Aws KeyPair 'strata-no-such-key' not found for region us-east-2"]}]}"#,
            Some(0),
        ),
        (
            r#"{"checks":[{"id":"iaas-pod-identity","verdict":"error","messages":["POST /v1/clouds/aws/properties/validate failed with HTTP 500: {\"code\":\"AwsKeyPairNotFound\"}"]}]}"#,
            Some(0),
        ),
        (
            r#"{"checks":[{"id":"iaas-pod-identity","verdict":"failed","messages":["Palette refuses the properties of the AWS cluster: FeatureNotSupported"]}]}"#,
            Some(1),
        ),
        (
            r#"{"checks":[{"id":"iaas-pod-identity","verdict":"passed"}]}"#,
            Some(1),
        ),
        (
            r#"{"checks":[{"id":"ssh-key-pair","verdict":"failed","messages":["AwsKeyPairNotFound"]}]}"#,
            Some(1),
        ),
    ] {
        std::fs::write(dir.path().join("report.json"), report).unwrap();
        let o = Command::new("jq")
            .args(["-e", filter])
            .arg(dir.path().join("report.json"))
            .output()
            .unwrap();
        assert_eq!(o.status.code(), status, "{report}");
    }
}

#[test]
fn aws_cleanup_deletes_nothing_without_yes_and_names_what_it_would() {
    let (o, calls) = aws_cleanup(&["fred", "us-east-2"]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    let out = text(&o.stdout);
    assert!(out.contains("nothing deleted; add --yes"), "{out}");
    assert!(out.contains("eks cluster"), "{out}");
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

/// The text of `tofu output -json` for the outputs `outputs`, a JSON object of values.
fn tofu_output_json(outputs: &serde_json::Value) -> String {
    let outputs: serde_json::Map<String, serde_json::Value> = outputs
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::json!({ "value": v })))
        .collect();
    serde_json::Value::Object(outputs).to_string()
}

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
        fake_bin(
            root.path(),
            &[
                ("tofu", FAKE_TOFU_STAGE),
                ("fake-strata", FAKE_STRATA_STAGE),
                ("sleep", "#!/usr/bin/env bash\nexit 0\n"),
            ],
        );
        for d in ["routes", "strata", "aws"] {
            std::fs::create_dir_all(root.path().join(d)).unwrap();
        }
        std::fs::write(root.path().join("outputs.json"), tofu_output_json(&outputs)).unwrap();
        std::fs::write(root.path().join("strata/rules"), "").unwrap();
        Stage {
            root,
            answers: 0,
            env: Vec::new(),
        }
    }

    /// The name of the route file of `path`: the path without its first `/`, with `_` for each other.
    fn route_key(path: &str) -> String {
        path.trim_start_matches('/').replace('/', "_")
    }

    /// Palette answers `path`, such as `/v1/spectroclusters/c-a`, with `body` and status 200.
    fn route(&self, path: &str, body: &str) {
        std::fs::write(
            self.root.path().join("routes").join(Self::route_key(path)),
            body,
        )
        .unwrap();
    }

    /// Palette answers `path` with `status` and `body` to a request whose ApiKey is `key`.
    fn route_as(&self, path: &str, key: &str, status: u16, body: &str) {
        let route = self.root.path().join("routes");
        let name = format!("{}.as-{key}", Self::route_key(path));
        std::fs::write(route.join(&name), body).unwrap();
        std::fs::write(route.join(format!("{name}.status")), status.to_string()).unwrap();
    }

    /// Palette holds the published version `uid`, with the packs `packs` (a JSON list) and the
    /// manifests `manifests`, and takes its publish. After a PUT of the version, Palette gives the
    /// packs of that PUT.
    fn published(&self, uid: &str, packs: &str, manifests: &str) {
        let route = self.root.path().join("routes");
        let key = Self::route_key(&format!("/v1/clusterprofiles/{uid}"));
        std::fs::write(
            route.join(format!("{key}.first")),
            format!(
                r#"{{"metadata":{{"name":"p","uid":"{uid}"}},"spec":{{"version":"1.0.0","published":{{
                    "type":"cluster","cloudType":"all","packs":{packs}}}}}}}"#
            ),
        )
        .unwrap();
        executable(
            &route.join(format!("{key}.sh")),
            &format!(
                r#"#!/usr/bin/env bash
sent="$FAKE_ROUTES/sent/PUT_{key}"
if [ -f "$sent" ]; then
  jq '{{metadata: {{name: "p", uid: "{uid}"}}, spec: {{version: "1.0.0", published: {{type: "cluster",
    cloudType: "all", packs: [.spec.template.packs[] | . + {{packUid: .uid}}]}}}}}}' "$sent"
else
  cat "$FAKE_ROUTES/{key}.first"
fi
"#
            ),
        );
        self.route(
            &format!("/v1/clusterprofiles/{uid}/packs/manifests"),
            manifests,
        );
        self.route(&format!("/v1/clusterprofiles/{uid}/publish"), "{}");
    }

    /// `body` stands in for the command `name`, in place of the one that the stage has.
    fn command(&self, name: &str, body: &str) {
        executable(&self.path("bin").join(name), body);
    }

    /// `wrapper` stands in for strata. A call that it does not answer goes to the answers that
    /// `strata` gave.
    fn wrap_strata(&self, wrapper: &str) {
        let bin = self.path("bin");
        std::fs::rename(bin.join("fake-strata"), bin.join("fake-strata-rules")).unwrap();
        self.command("fake-strata", wrapper);
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
        // a script that runs no stage writes no steps.log. An empty log means no step, and the
        // `unwrap` below stops the test when the log is gone.
        std::fs::create_dir_all(root.join("out")).unwrap();
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("out/steps.log"))
            .unwrap();
        let o = run_live(script, args, root, &env);
        let steps = std::fs::read_to_string(root.join("out/steps.log")).unwrap();
        (o, steps)
    }

    /// Each body of `METHOD path` that the script sent, in order. The list is empty when the
    /// script sent none to that route, but only after the script sent a request to some route:
    /// the stand-in makes the directory `routes/sent` at its first request.
    fn sent(&self, method: &str, path: &str) -> Vec<serde_json::Value> {
        let key = Self::route_key(path);
        let dir = self.path("routes/sent");
        assert!(dir.is_dir(), "the script sent no request to any route");
        let all = dir.join(format!("{method}_{key}.all"));
        let sent = if all.exists() {
            std::fs::read_to_string(all).unwrap()
        } else {
            String::new()
        };
        serde_json::Deserializer::from_str(&sent)
            .into_iter::<serde_json::Value>()
            .map(Result::unwrap)
            .collect()
    }

    /// Each command that OpenTofu ran, one line each.
    fn tofu_calls(&self) -> String {
        std::fs::read_to_string(self.path("tofu.log")).unwrap()
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
    assert!(
        !steps.contains("== stage 6"),
        "stage 6 needs a second key, so it runs only when asked: {steps}"
    );

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

/// A `strata` for tiers.sh stage 6. It writes a record that says which key it read with: the
/// record names the key and does not hold it.
const FAKE_STRATA_LIMITED: &str = r#"#!/usr/bin/env bash
if [ "$*" = "snapshot cluster:c-a -o limited-record.json" ]; then
  with=first; [ "${PALETTE_API_KEY:-}" = limited-k ] && with=second
  echo "{\"readWith\":\"$with\",\"source\":{\"cluster\":{\"tiers\":{\"policy\":{\"errors\":{\"rbacs\":\"HTTP 500\"}}}}}}" > limited-record.json
fi
exit 0
"#;

#[test]
fn tiers_sh_stage_6_reads_the_role_bindings_with_a_second_key_and_records_the_refusal() {
    let mut stage = baseline_stage();
    stage.command("fake-strata", FAKE_STRATA_LIMITED);
    let rbacs = "/v1/spectroclusters/c-a/config/rbacs";
    stage.route(rbacs, r#"{"items":[]}"#);
    // an active user with no role in the project: Palette answers HTTP 500, and not 403
    stage.route_as(
        rbacs,
        "limited-k",
        500,
        r#"{"code":"ResourceAccessDenied","message":"Project 'p-1' access denied"}"#,
    );
    stage.env("STRATA_LIVE_LIMITED_KEY", "limited-k");

    let (o, steps) = stage.run("tiers.sh", &["6"]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    assert!(!steps.contains("FAIL"), "{steps}");
    passed(
        &steps,
        &[
            "limited-control",
            "limited-read",
            "limited-status",
            "limited-code-says",
            "limited-message",
        ],
    );
    for seen in ["limited-code ", "limited-record ", "limited-record-errors "] {
        assert!(steps.contains(&format!("SEEN  {seen}")), "{seen}: {steps}");
    }
    assert_eq!(
        std::fs::read_to_string(stage.path("out/limited-code.out")).unwrap(),
        "ResourceAccessDenied\n",
        "the check records the error code of the body"
    );
    assert!(
        std::fs::read_to_string(stage.path("out/limited-record.json"))
            .unwrap()
            .contains(r#""readWith":"second""#),
        "strata reads the record with the second key"
    );

    let calls = std::fs::read_to_string(stage.path("curl.log")).unwrap();
    assert_eq!(
        calls
            .lines()
            .filter(|l| l.ends_with("config/rbacs"))
            .count(),
        2,
        "one read with each key: {calls}"
    );
    assert!(
        !calls.contains("PUT") && !calls.contains("POST") && !calls.contains("PATCH"),
        "the stage writes nothing: {calls}"
    );
    let request = std::fs::read_to_string(
        stage.path("routes/sent/GET_v1_spectroclusters_c-a_config_rbacs.request"),
    )
    .unwrap();
    assert!(request.contains("ApiKey: (hidden)"), "{request}");
    let printed = format!("{}{}", text(&o.stdout), text(&o.stderr));
    assert!(
        !printed.contains("limited-k"),
        "the key is not printed: {printed}"
    );
    for (name, content) in files_under(&stage.path("out")) {
        assert!(!content.contains("limited-k"), "{name} holds the key");
    }
}

#[test]
fn tiers_sh_stage_6_fails_the_status_check_when_palette_answers_403_in_place_of_500() {
    let mut stage = baseline_stage();
    let rbacs = "/v1/spectroclusters/c-a/config/rbacs";
    stage.route(rbacs, r#"{"items":[]}"#);
    stage.route_as(
        rbacs,
        "limited-k",
        403,
        r#"{"code":"AccessDenied","message":"cluster.get operation is not permitted"}"#,
    );
    stage.env("STRATA_LIVE_LIMITED_KEY", "limited-k");
    let (o, steps) = stage.run("tiers.sh", &["6"]);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    for failed in ["limited-status", "limited-code-says", "limited-message"] {
        assert!(
            steps
                .lines()
                .any(|l| l.starts_with("FAIL") && l.split_whitespace().nth(1) == Some(failed)),
            "{failed} fails: {steps}"
        );
    }
    passed(&steps, &["limited-read"]);
}

#[test]
fn tiers_sh_stage_6_says_could_not_check_and_exits_2_without_the_second_key() {
    let stage = baseline_stage();
    let (o, steps) = stage.run("tiers.sh", &["6"]);
    assert_eq!(o.status.code(), Some(2), "{steps}{}", text(&o.stderr));
    assert!(
        text(&o.stderr).contains("could not check")
            && text(&o.stderr).contains("STRATA_LIVE_LIMITED_KEY"),
        "{}",
        text(&o.stderr)
    );
    assert!(!steps.contains("limited-"), "no check ran: {steps}");
    let calls = std::fs::read_to_string(stage.path("curl.log")).unwrap_or_default();
    assert!(!calls.contains("config/rbacs"), "no request: {calls}");
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

/// A stage 1 of `cluster-group.sh`: the group `g-1` with the virtual cluster `m-1`, named
/// `fred-member`, whose record strata gives.
fn cluster_group_stage_1() -> Stage {
    let mut outputs = baseline_outputs();
    outputs["cluster_group_uid"] = "g-1".into();
    outputs["cluster_group_name"] = "fred-group".into();
    outputs["member_cluster_uid"] = "m-1".into();
    let mut stage = Stage::new(outputs);
    stage.route(
        "/v1/spectroclusters/c-a",
        r#"{"metadata":{"name":"fred"},"status":{"state":"Running"}}"#,
    );
    stage.strata(
        "snapshot cluster:m-1 -o member.json",
        0,
        r#"{"source":{"cluster":{"uid":"m-1","name":"fred-member"}}}"#,
    );
    stage
}

#[test]
fn cluster_group_sh_stage_1_lists_the_virtual_cluster_and_resolves_its_name() {
    let mut stage = cluster_group_stage_1();
    stage.strata(
        "list clusters --json",
        0,
        r#"{"includesVirtual":true,"clusters":[{"name":"fred","uid":"c-a"},
            {"name":"fred-member","uid":"m-1","state":"Running","cloudType":"nested",
             "virtual":{"clusterGroup":{"uid":"g-1","name":"fred-group"}}}]}"#,
    );
    stage.strata(
        "snapshot cluster:fred-member",
        0,
        r#"{"source":{"cluster":{"uid":"m-1","name":"fred-member"}}}"#,
    );
    let (_, steps) = stage.run("cluster-group.sh", &["1"]);
    passed(
        &steps,
        &["member-listed", "member-listed-group", "member-by-name"],
    );
    let calls = lines_of(&stage.path("strata/calls"));
    assert!(
        calls.iter().any(|l| l == "snapshot cluster:fred-member"),
        "the name of the member, from its record: {calls:?}"
    );

    // a list that leaves out the virtual cluster, as before #116, fails the check
    let mut stage = cluster_group_stage_1();
    stage.strata(
        "list clusters --json",
        0,
        r#"{"includesVirtual":false,"clusters":[{"name":"fred","uid":"c-a"}]}"#,
    );
    let (_, steps) = stage.run("cluster-group.sh", &["1"]);
    passed(&steps, &["member-listed"]);
    assert!(
        steps.contains("FAIL  member-listed-group"),
        "the check sees a list with no virtual cluster: {steps}"
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
    fake_bin(stage.root.path(), &[("aws", FAKE_AWS), ("oras", FAKE_ORAS)]);
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

    let aws = lines_of(&stage.path("aws/calls.log"));
    let deletes: Vec<&String> = aws
        .iter()
        .filter(|l| l.contains("batch-delete-image"))
        .collect();
    assert_eq!(
        deletes.len(),
        2,
        "the delete, then the delete at the end: {aws:?}"
    );
    assert!(
        deletes
            .iter()
            .all(|l| l
                .contains("--repository-name strata/strata-live-pack --image-ids imageTag=1.0.9")),
        "{aws:?}"
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
    let sent = read_json(&stage.path("routes/sent/POST_v1_clusterprofiles"));
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
    let calls = lines_of(&stage.path("strata/calls"));
    let kept = calls.iter().position(|l| l == "verify floating-mod-1.json");
    let moved = calls.iter().position(|l| l == "verify profile-3.json");
    assert!(
        moved.is_some() && kept > moved,
        "the check of the profile with changed values comes after the wait for the move: {calls:?}"
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
    let calls = lines_of(&stage.path("strata/calls"));
    assert!(
        calls.iter().any(
            |l| l == "packs strata-live-dep --registry strata-live-registry --cloud aws --json"
        ),
        "{calls:?}"
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
    let calls = lines_of(&stage.path("strata/calls"));
    assert!(calls.iter().any(|l| l == "packs kubernetes"), "{calls:?}");
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
    stage.route(PROFILE_LISTING, r#"{"items":[]}"#);
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
    let log = lines_of(&stage.path("curl.log"));
    assert!(
        log.iter().any(|l| l == "DELETE /v1/clusterprofiles/lp-1"),
        "{log:?}"
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

/// A `strata` for pending.sh. An update waits for cluster A while the log of the stand-in curl
/// holds an odd number of PUTs of a profile: an edit in place opens an update, and the edit that
/// undoes it closes the update. A call it does not answer goes to the answers of the stage.
const FAKE_STRATA_PENDING: &str = r#"#!/usr/bin/env bash
if [ "$*" = "snapshot cluster:c-a -o settle.json" ]; then
  puts=$(grep -c '^PUT /v1/clusterprofiles/' "$FAKE_LOG" 2> /dev/null)
  waiting=false; [ $(( ${puts:-0} % 2 )) = 1 ] && waiting=true
  echo "{\"source\":{\"cluster\":{\"updatesPending\":$waiting}}}" > settle.json
  exit 0
fi
exec "$(dirname "$0")/fake-strata-rules" "$@"
"#;

/// The one pack of the Kubernetes layer that a profile holds in these tests, with the kubeadm
/// command that `lib.sh` edits.
const KUBERNETES_PACK: &str = r#"[{"name":"kubernetes","tag":"1.36.2","type":"spectro","layer":"k8s",
    "packUid":"k-1","values":"kubeadmconfig:\n  postKubeadmCommands:\n    - 'chmod 600 /var/lib/kubelet/config.yaml'\n"}]"#;

#[test]
fn pending_sh_stage_2_asks_palette_about_the_update_of_an_add_on_manifest_and_it_says_no_repave() {
    let mut stage = baseline_stage();
    stage.wrap_strata(FAKE_STRATA_PENDING);
    stage.published(
        "ad-1",
        r#"[{"name":"config","tag":"1.0.0","type":"manifest","layer":"addon","packUid":"pk-1","values":""}]"#,
        r#"{"spec":{"packs":[{"spec":{"name":"config","manifests":[
            {"name":"config","uid":"m-1","content":"edited: \"no\"\n"}]}}]}}"#,
    );
    stage.strata(
        "compare cluster:c-a cluster:c-a@pending --repave",
        1,
        "root       aaa -> bbb   CHANGED\npalette    no repave\n",
    );
    stage.strata(
        "compare cluster:c-a cluster:c-a@pending --repave --json",
        1,
        r#"{"paletteRepave":{"clusterUid":"c-a","required":false,"agrees":true}}"#,
    );

    let (_, steps) = stage.run("pending.sh", &["2"]);
    passed(
        &steps,
        &[
            "addon-edit",
            "addon-pending",
            "addon-repave",
            "addon-repave-says",
            "addon-repave-json",
        ],
    );
    let calls = std::fs::read_to_string(stage.path("strata/calls")).unwrap();
    assert!(
        calls.contains("compare cluster:c-a cluster:c-a@pending --repave\n"),
        "the question names the update that waits: {calls}"
    );
}

#[test]
fn pending_sh_stage_6_asks_palette_about_a_kubeadm_command_that_waits_and_closes_the_update() {
    let mut stage = baseline_stage();
    stage.wrap_strata(FAKE_STRATA_PENDING);
    stage.published("r-1", KUBERNETES_PACK, "{}");
    stage.strata(
        "compare cluster:c-a cluster:c-a@pending --repave",
        1,
        "root       aaa -> bbb   CHANGED\npalette    repave: PackValuesUpdated kubernetes (kubeadmconfig.postKubeadmCommands)\n",
    );
    stage.strata(
        "compare cluster:c-a cluster:c-a@pending --repave --json",
        1,
        r#"{"paletteRepave":{"clusterUid":"c-a","required":true,"agrees":true,"reasons":[
            {"code":"PackValuesUpdated","pack":"kubernetes","keys":["kubeadmconfig.postKubeadmCommands"]}]}}"#,
    );

    let (o, steps) = stage.run("pending.sh", &["6"]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    assert!(!steps.contains("FAIL"), "{steps}");
    passed(
        &steps,
        &[
            "k8s-edit",
            "k8s-pending",
            "k8s-repave",
            "k8s-repave-says",
            "k8s-repave-json",
            "k8s-undo",
            "k8s-undo-none",
            "k8s-baseline-none",
        ],
    );
    let values: Vec<String> = stage
        .sent("PUT", "/v1/clusterprofiles/r-1")
        .iter()
        .map(|b| {
            b["spec"]["template"]["packs"][0]["values"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(values.len(), 2, "the edit and its undo: {values:?}");
    assert!(
        values[0].contains("    - 'echo strata-repave-test'\n") && !values[1].contains("echo"),
        "one more kubeadm command, and then the command as before: {values:?}"
    );
    assert!(
        stage.tofu_calls().contains("apply"),
        "the apply of the baseline writes the text again: {}",
        stage.tofu_calls()
    );
}

/// A `tofu` for template.sh. `output -json` prints $FAKE_OUTPUTS. An apply with `cluster=false`
/// copies $FAKE_OUTPUTS.none over it, and each other apply copies $FAKE_OUTPUTS.cluster over it.
/// The apply of a cluster outside the template fails, as Palette refuses it.
const FAKE_TOFU_TEMPLATE: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_TOFU_LOG"
case "$*" in
  *"output -json"*) cat "$FAKE_OUTPUTS" ;;
  *apply*"template=true -var second_addon=true"*) echo "ClusterCreateWithTemplateProfileForbidden" >&2; exit 1 ;;
  *apply*"cluster=false"*) cp "$FAKE_OUTPUTS.none" "$FAKE_OUTPUTS"; echo "Apply complete! Resources: 0 added." ;;
  *apply*) cp "$FAKE_OUTPUTS.cluster" "$FAKE_OUTPUTS"; echo "Apply complete! Resources: 1 added." ;;
esac
"#;

#[test]
fn template_sh_stage_4_builds_a_cluster_in_the_template_and_asks_the_route_of_the_cluster_and_of_the_template()
 {
    let outputs = |cluster: &str| {
        serde_json::json!({
            "project_uid": "p-1", "cluster_uid": cluster, "template_uid": "t-1",
            "release_profile_uid": "r-1", "addon_profile_uid": "ad-1", "next_profile_uid": "n-1",
            "profile_name": "strata-live-profile", "profile_names": ["strata-live-profile"],
            "csi_reclaim_delete": "reclaimPolicy: Delete",
        })
    };
    let mut stage = Stage::new(outputs(""));
    stage.command("tofu", FAKE_TOFU_TEMPLATE);
    std::fs::write(
        stage.path("outputs.json.none"),
        tofu_output_json(&outputs("")),
    )
    .unwrap();
    std::fs::write(
        stage.path("outputs.json.cluster"),
        tofu_output_json(&outputs("c-t")),
    )
    .unwrap();
    stage.route(
        "/v1/spectroclusters/c-t",
        r#"{"metadata":{"name":"fred"},"status":{"state":"Running"},
            "spec":{"clusterTemplate":{"uid":"t-1","projectUid":"p-1"}}}"#,
    );
    stage.published("n-1", KUBERNETES_PACK, "{}");
    stage.route(
        "/v1/spectroclusters/c-t/validate/repave",
        r#"{"isRepaveRequired":false,"reasons":[]}"#,
    );
    stage.strata(
        "compare cluster:c-t/strata-live-profile n-1 --repave",
        1,
        "root       aaa -> bbb   CHANGED\n\
         palette    the cluster template t-1 governs the cluster: Palette answers for the template, by profile uid\n\
         palette    repave: PackValuesUpdated kubernetes (kubeadmconfig.postKubeadmCommands)\n",
    );
    stage.strata(
        "compare cluster:c-t/strata-live-profile n-1 --repave --json",
        1,
        r#"{"paletteRepave":{"clusterUid":"c-t","required":true,"agrees":true,
            "reasons":[{"code":"PackValuesUpdated","pack":"kubernetes","keys":["kubeadmconfig.postKubeadmCommands"]}],
            "clusterTemplate":{"uid":"t-1","projectUid":"p-1"}}}"#,
    );

    let (_, steps) = stage.run("template.sh", &[]);
    passed(
        &steps,
        &[
            "in-template-names",
            "in-template-edit",
            "cluster-route",
            "cluster-route-says",
            "template-route",
            "template-route-names",
            "template-route-says",
            "template-route-json",
            "in-template-undo",
        ],
    );

    let applies: Vec<String> = stage
        .tofu_calls()
        .lines()
        .filter(|l| l.contains("apply"))
        .map(str::to_string)
        .collect();
    let built = applies
        .iter()
        .position(|l| l.contains("cluster_in_template=true"))
        .unwrap_or_else(|| panic!("an apply builds the cluster in the template: {applies:?}"));
    assert!(
        applies[built].contains("template=true"),
        "the template must exist: {}",
        applies[built]
    );
    assert!(
        applies[built + 1].contains("cluster=false")
            && applies[built + 1].contains("template=true"),
        "the cluster goes before the next stage, and the template stays: {applies:?}"
    );

    let route = "/v1/spectroclusters/c-t/validate/repave";
    let asked = stage.sent("POST", route);
    assert_eq!(asked.len(), 1, "the route of the cluster, once: {asked:?}");
    assert_eq!(
        (
            asked[0]["profiles"][0]["uid"].as_str(),
            asked[0]["profiles"][0]["replaceWithProfile"].as_str()
        ),
        (Some("r-1"), Some("n-1")),
        "the move from the release version to the next one"
    );
    let edits: Vec<String> = stage
        .sent("PUT", "/v1/clusterprofiles/n-1")
        .iter()
        .map(|b| {
            b["spec"]["template"]["packs"][0]["values"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(edits.len(), 2, "the edit and its undo: {edits:?}");
    assert!(
        edits[0].contains("echo strata-repave-test") && !edits[1].contains("echo"),
        "one more kubeadm command, then as before: {edits:?}"
    );
    let calls = std::fs::read_to_string(stage.path("strata/calls")).unwrap();
    assert!(
        calls.contains("compare cluster:c-t/strata-live-profile n-1 --repave\n"),
        "strata asks about the move from the cluster: {calls}"
    );
}

/// The text of a file of the OpenTofu configuration of the live example.
fn terraform_file(name: &str) -> String {
    std::fs::read_to_string(live_script("terraform").join(name)).unwrap()
}

#[test]
fn a_cluster_in_the_cluster_template_gets_a_time_zone_because_palette_refuses_it_without_one() {
    let variables = terraform_file("variables.tf");
    let declaration = variables
        .split("variable \"cluster_timezone\" {")
        .nth(1)
        .expect("variables.tf declares cluster_timezone")
        .split("\n}\n")
        .next()
        .unwrap();
    let default = declaration
        .lines()
        .find_map(|l| l.trim().strip_prefix("default"))
        .and_then(|l| l.trim_start().strip_prefix('='))
        .map(|l| l.trim().trim_matches('"'))
        .expect("cluster_timezone has a default");
    assert!(
        !default.is_empty() && !default.contains(' '),
        "the default is a name of the IANA time zone database: {default:?}"
    );

    // The clouds whose cluster resource takes a cluster template: variables.tf says aws and gcp.
    let mut templated: Vec<String> = std::fs::read_dir(live_script("terraform"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.starts_with("cloud-") && name.ends_with(".tf"))
        .filter(|name| terraform_file(name).contains("dynamic \"cluster_template\""))
        .collect();
    templated.sort();
    assert_eq!(templated, ["cloud-aws.tf", "cloud-gcp.tf"]);
    for name in templated {
        assert!(
            terraform_file(&name).contains(
                "cluster_timezone = var.cluster_in_template ? var.cluster_timezone : null"
            ),
            "{name}: the cluster in the template has a time zone, and the other clusters keep theirs"
        );
    }
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
        PROFILE_LISTING,
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

/// A stage of `scope.sh`: the release profile `r-1` of the project, and its copy `t-1` that
/// Palette creates at tenant scope. The document of `t-1` gives the scope `tenant_scope`.
fn scope_stage(tenant_scope: &str) -> Stage {
    let mut stage = Stage::new(baseline_outputs());
    stage.route(
        "/v1/clusterprofiles/r-1",
        r#"{"metadata":{"name":"strata-live-profile","uid":"r-1","annotations":{"scope":"project"}},
            "spec":{"version":"1.0.0","published":{"type":"cluster","cloudType":"aws","packs":[
            {"name":"kubernetes","tag":"1.33.0","type":"spectro","layer":"k8s","packUid":"pk-1",
             "registryUid":"reg-1","values":""}]}}}"#,
    );
    stage.route(
        "/v1/clusterprofiles/r-1/packs/manifests",
        r#"{"metadata":{},"spec":{"packs":[]}}"#,
    );
    stage.route("/v1/clusterprofiles", r#"{"uid":"t-1"}"#);
    stage.route("/v1/clusterprofiles/t-1/publish", "{}");
    stage.route(
        "/v1/clusterprofiles/t-1",
        &format!(
            r#"{{"metadata":{{"name":"strata-live-profile","uid":"t-1","annotations":{{"scope":"{tenant_scope}"}}}},
                "spec":{{"version":"1.0.0"}}}}"#
        ),
    );
    stage.route(
        PROFILE_LISTING,
        r#"{"items":[
            {"metadata":{"name":"strata-live-profile","uid":"r-1"},"spec":{"version":"1.0.0","cloudType":"aws"}},
            {"metadata":{"name":"strata-live-profile","uid":"t-1"},"spec":{"version":"1.0.0","cloudType":"aws"}}]}"#,
    );
    stage.route(
        "/v1/dashboard/clusterprofiles",
        r#"{"items":[{"metadata":{"name":"strata-live-profile","annotations":{"scope":"tenant"}},
            "specSummary":{"versions":[{"version":"1.0.0","uid":"t-1"}]}}],
            "listmeta":{"continue":"","count":1,"limit":50,"offset":0}}"#,
    );
    stage.strata(
        "list profiles --json",
        0,
        r#"{"profiles":[{"uid":"r-1","scope":"project"},{"uid":"t-1","scope":"tenant"}]}"#,
    );
    stage.strata(
        "list profiles --no-color",
        0,
        "  strata-live-profile@1.0.0  r-1  aws\n  strata-live-profile@1.0.0  t-1  aws  tenant\n",
    );
    stage.strata(
        "list profiles --packs --no-color",
        0,
        "  strata-live-profile@1.0.0  r-1  aws  cluster  no consumers\n  \
         strata-live-profile@1.0.0  t-1  aws  cluster  no consumers  tenant\n",
    );
    stage.strata_err(
        "snapshot strata-live-profile@1.0.0 -o ambiguous.json",
        2,
        "strata: several profiles in this scope are named 'strata-live-profile' with version \
         '1.0.0': r-1 (project), t-1 (tenant); give the uid in place of the name\n",
    );
    stage.strata("snapshot t-1 -o tenant.json", 0, "{}");
    stage.strata(
        "list profiles --project  --json",
        0,
        r#"{"profiles":[{"uid":"t-1","scope":"tenant"}]}"#,
    );
    stage.strata(
        "list profiles --project  --no-color",
        0,
        "  strata-live-profile@1.0.0  t-1  aws\n",
    );
    stage
}

#[test]
fn scope_sh_makes_a_tenant_profile_with_no_project_and_checks_each_answer_about_it() {
    let stage = scope_stage("tenant");
    let (o, steps) = stage.run("scope.sh", &[]);
    passed(
        &steps,
        &[
            "tenant-create",
            "listing-both",
            "search-tenant",
            "annotation-tenant",
            "annotation-project",
            "list-scope",
            "list-mark",
            "list-packs-mark",
            "ambiguous",
            "ambiguous-says",
            "by-uid",
            "tenant-list",
            "tenant-list-no-mark",
            "tenant-delete",
        ],
    );
    assert_eq!(o.status.code(), Some(0), "{steps}");

    // the create and the publish go to tenant scope: no ProjectUid header
    for request in [
        "routes/sent/POST_v1_clusterprofiles.request",
        "routes/sent/PATCH_v1_clusterprofiles_t-1_publish.request",
    ] {
        let sent = std::fs::read_to_string(stage.path(request)).unwrap();
        assert!(!sent.contains("ProjectUid"), "{request}: {sent}");
    }
    let posts = stage.sent("POST", "/v1/clusterprofiles");
    assert_eq!(posts.len(), 1, "one create: {posts:?}");
    assert_eq!(
        posts[0]["metadata"],
        serde_json::json!({"name": "strata-live-profile"})
    );
    assert_eq!(posts[0]["spec"]["version"], "1.0.0");
    // the search asks for the tenant versions, in the project
    let searches = stage.sent("POST", "/v1/dashboard/clusterprofiles");
    assert_eq!(searches.len(), 1, "{searches:?}");
    assert_eq!(searches[0]["filter"]["scope"], "tenant");
    let request = std::fs::read_to_string(
        stage.path("routes/sent/POST_v1_dashboard_clusterprofiles.request"),
    )
    .unwrap();
    assert!(request.contains("ProjectUid: p-1"), "{request}");
    let calls = std::fs::read_to_string(stage.path("curl.log")).unwrap();
    assert!(
        calls.ends_with("DELETE /v1/clusterprofiles/t-1\n"),
        "the last request deletes the tenant profile: {calls}"
    );
}

#[test]
fn scope_sh_fails_the_annotation_check_when_the_document_of_the_copy_says_project() {
    let stage = scope_stage("project");
    let (o, steps) = stage.run("scope.sh", &[]);
    assert!(steps.contains("FAIL  annotation-tenant"), "{steps}");
    assert_ne!(o.status.code(), Some(0));
    let calls = std::fs::read_to_string(stage.path("curl.log")).unwrap();
    assert!(
        calls.ends_with("DELETE /v1/clusterprofiles/t-1\n"),
        "the tenant profile goes also after a failed check: {calls}"
    );
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

/// A run of `registry-kinds.sh` against the stand-ins. The tools of the registries do nothing.
/// The fake strata prints the text `packs` for `strata packs strata-live-zarf`.
fn registry_kinds(packs: &str) -> (Stage, String) {
    let mut stage = Stage::new(serde_json::json!({
        "aws_profile": "p", "project_uid": "p-1", "cluster_uid": "",
        "kinds_host": "kinds.example", "kinds_user": "barney",
        "kinds_registries": {"oci-zarf": {"uid": "reg-zarf", "name": "strata-live-oci"}},
    }));
    let tofu = FAKE_TOFU_STAGE.replace(
        "case \"$*\" in\n",
        "case \"$*\" in\n  *\"output -raw kinds_password\"*) echo pw ;;\n",
    );
    let nothing = "#!/usr/bin/env bash\nexit 0\n";
    fake_bin(
        stage.root.path(),
        &[
            ("tofu", &tofu),
            ("oras", nothing),
            ("helm", nothing),
            ("zarf", nothing),
            ("spectro", nothing),
        ],
    );
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
    let calls = lines_of(&stage.path("strata/calls"));
    assert!(
        calls
            .iter()
            .any(|l| l == "packs strata-live-zarf --registry strata-live-oci"),
        "{calls:?}"
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

/// The hook of the fake name route. It gives HTTP 500 and `VersionParseFailure` for the version
/// 2.2.2.develop, as Palette does, and an empty answer for each other version.
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
fn validate_packs_sh_stage_5_asks_the_two_routes_for_each_rule_that_they_answer() {
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

    let (o, steps) = stage.run("validate-packs.sh", &["5"]);
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
        !steps.contains("== a candidate whose") && !steps.contains("== a pack with another"),
        "stage 5 runs alone: {steps}"
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

/// The report of `strata behind --json` for a record that pins kubernetes 1.35.3, with the newest
/// version `newest`, and the answer `validation` of Palette for the bump.
fn kubernetes_bump_report(newest: &str, validation: &serde_json::Value) -> String {
    serde_json::json!({"pins": [
        {"name": "cni", "version": "1.0.0", "behind": false},
        {"name": "kubernetes", "version": "1.35.3", "behind": true,
         "newer": [{"version": newest}],
         "bump": {"to": newest, "paletteValidation": validation}}]})
    .to_string()
}

/// The answer of Palette that refuses the move of kubernetes on the cluster `fred`.
fn upgrade_refused() -> serde_json::Value {
    serde_json::json!({"accepts": false, "errors": [{
        "pack": "kubernetes", "code": UPGRADE_NOT_SUPPORTED_CODE,
        "message": "on the clusters: fred", "clusters": ["fred"]}]})
}

/// A stage 6 run of validate-packs.sh on the profile `strata-live-release@1.0.0` and the cluster
/// `fred`, where `strata behind --json` gives `report` for both records. Gives steps.log.
fn bump_stage(report: &str) -> (Stage, String) {
    let mut stage = Stage::new(serde_json::json!({}));
    stage.env("PROFILE", "strata-live-release@1.0.0");
    stage.env("CLUSTER", "fred");
    stage.strata(
        "snapshot strata-live-release@1.0.0 -o bump-profile.json",
        0,
        "{}",
    );
    stage.strata("snapshot cluster:fred -o bump-cluster.json", 0, "{}");
    stage.strata("behind bump-profile.json --json", 1, report);
    stage.strata("behind bump-cluster.json --json", 1, report);
    stage.strata(
        "behind bump-profile.json --no-impact --json",
        1,
        r#"{"pins":[{"name":"kubernetes","version":"1.35.3","behind":true}]}"#,
    );
    stage.strata(
        "behind bump-profile.json",
        1,
        &format!(
            "~ [k8s  ] kubernetes  1.35.3  Public Repo  behind: 1.37.0\n      bump to 1.37.0: repave IDENTITY\n      palette refuses: {UPGRADE_NOT_SUPPORTED_CODE} on fred\n"
        ),
    );
    let (o, steps) = stage.run("validate-packs.sh", &["6"]);
    assert!(
        steps.contains("== behind asks Palette"),
        "{steps}{}",
        text(&o.stderr)
    );
    (stage, steps)
}

#[test]
fn validate_packs_sh_stage_6_asks_behind_about_the_bump_of_kubernetes_for_the_profile_and_the_cluster()
 {
    let (_, steps) = bump_stage(&kubernetes_bump_report("1.37.0", &upgrade_refused()));
    assert!(!steps.contains("FAIL"), "{steps}");
    passed(
        &steps,
        &[
            "bump-profile-record",
            "bump-pair",
            "bump-asked",
            "bump-refused",
            "bump-clusters",
            "bump-text-says",
            "bump-no-impact-says",
            "bump-cluster-record",
            "bump-cluster-asked",
            "bump-cluster-refused",
        ],
    );
    assert!(
        !steps.contains("== a candidate whose") && !steps.contains("== the name route"),
        "stage 6 runs alone: {steps}"
    );
}

#[test]
fn validate_packs_sh_stage_6_fails_when_palette_accepts_a_move_of_two_minor_versions() {
    // Palette accepts, and the newest version is one minor version above the pin
    let (_, steps) = bump_stage(&kubernetes_bump_report(
        "1.36.2",
        &serde_json::json!({"accepts": true}),
    ));
    passed(&steps, &["bump-asked", "bump-cluster-asked"]);
    for id in [
        "bump-pair",
        "bump-refused",
        "bump-clusters",
        "bump-cluster-refused",
    ] {
        assert!(
            steps
                .lines()
                .any(|l| l.starts_with("FAIL") && l.split_whitespace().nth(1) == Some(id)),
            "{id} failed:\n{steps}"
        );
    }
}

#[test]
fn validate_packs_sh_stage_6_needs_a_profile_and_runs_only_when_its_number_is_given() {
    let stage = Stage::new(serde_json::json!({}));
    let (o, steps) = stage.run("validate-packs.sh", &["6"]);
    assert!(
        steps.contains("stage 6 skipped: give PROFILE"),
        "{steps}{}",
        text(&o.stderr)
    );
    assert!(!steps.contains("bump-"), "{steps}");

    let mut stage = Stage::new(serde_json::json!({}));
    stage.env("PROFILE", "strata-live-release@1.0.0");
    let (_, steps) = stage.run("validate-packs.sh", &["5"]);
    assert!(!steps.contains("bump-"), "{steps}");
}

// ---- validate-route.sh -----------------------------------------------------------------------------

/// The report of `strata preflight --rule palette-validate --json`, with the one check.
fn route_report(verdict: &str, messages: &[&str]) -> String {
    serde_json::json!({"checks": [{"id": "palette-validate", "verdict": verdict, "messages": messages}]})
        .to_string()
}

/// A case of validate-route.sh whose body is an AWS body, because its fault is in a profile.
struct AwsCase {
    id: &'static str,
    /// The name of the profile that route-bodies.sh makes for the case.
    profile: &'static str,
    /// What the route says when it refuses the body, or `None` when it takes the body.
    refusal: Option<&'static str>,
}

/// The four cases of validate-route.sh whose body is an AWS body, and what the route says of each.
/// The profile of `profile-cloud-mismatch` is of the cloud type gcp: Palette answers HTTP 500, and
/// `palette-validate` reads it as a refusal. The route takes a pack version that does not list the
/// cloud type, so no route answers the check of `pack-cloud-support` (#415).
const AWS_CASES: [AwsCase; 4] = [
    AwsCase {
        id: "pack-state-disabled",
        profile: "strata-route-pack-disabled",
        refusal: Some("DisabledPacksForbidden: Disabled packs 'gloo' are forbidden"),
    },
    AwsCase {
        id: "pack-state-deleted",
        profile: "strata-route-pack-deleted",
        refusal: Some("ClusterProfileInvalidPackState: the pack is deleted"),
    },
    AwsCase {
        id: "profile-cloud-mismatch",
        profile: "strata-route-cloud-mismatch",
        refusal: Some(
            "ClusterProfilesNotSupported: Cluster profiles 'strata-route-cloud-mismatch' are not supported with 'aws' environment",
        ),
    },
    AwsCase {
        id: "pack-cloud-support",
        profile: "strata-route-cloud-support",
        refusal: None,
    },
];

/// The case of `AWS_CASES` that the route takes.
fn aws_case_taken() -> &'static AwsCase {
    AWS_CASES
        .iter()
        .find(|case| case.refusal.is_none())
        .expect("one case of AWS_CASES has no refusal")
}

/// Answer the validate route for the clean AWS body and for each case of `AWS_CASES`. The route
/// refuses a case with its refusal. It takes a case with no refusal, and a case in `takes`.
fn answer_aws_cases(stage: &mut Stage, takes: &[&str]) {
    stage.strata(
        "preflight */aws-clean.json --cloud aws *",
        0,
        &route_report("passed", &[]),
    );
    for case in &AWS_CASES {
        let (code, report) = match case.refusal {
            Some(message) if !takes.contains(&case.id) => (1, route_report("failed", &[message])),
            _ => (0, route_report("passed", &[])),
        };
        stage.strata(
            &format!("preflight */{}.json --cloud aws *", case.id),
            code,
            &report,
        );
    }
}

/// The cases of validate-route.sh that the route refuses on the clean body of their engine, and
/// what the route says of each. `OWN_BODY_CASE` has a body of its own.
const ROUTE_REFUSALS: [(&str, &str); 12] = [
    (
        "vsphere-instance-floor-cpu",
        "InvalidParam: numCPUs should be equal or greater than 1",
    ),
    (
        "vsphere-instance-floor-memory",
        "InvalidParam: memoryMiB should be equal or greater than 1024 MiB",
    ),
    (
        "vsphere-instance-floor-disk",
        "InvalidParam: diskGiB should be equal or greater than 20 GiB",
    ),
    (
        "vsphere-instance-floor-no-type",
        "InvalidParam: instanceType should not be empty",
    ),
    ("eks-ami-type-al2", "UnsupportedAmiType: AL2_x86_64"),
    (
        "eks-ami-type-unknown",
        "InvalidParam: Invalid parameter 'AmiType'",
    ),
    (
        "ssh-key-format-vsphere",
        "SshKeyValidationFailure: ssh: no key found",
    ),
    (
        "vsphere-static-ip-pool-empty",
        "InvalidParam: ParentPoolUid should not be empty",
    ),
    (
        "vsphere-static-ip-pool-dhcp",
        "InvalidParam: staticIp is not allowed on a DHCP cluster",
    ),
    (
        "vsphere-controlplane-endpoint-no-host",
        "InvalidParam: Host endpoint should not be empty",
    ),
    (
        "ntp-servers-vsphere",
        "InvalidParam: ntpServers entry is not an address",
    ),
    (
        OWN_BODY_CASE,
        "DisabledPacksForbidden: Disabled packs 'nginx' are forbidden",
    ),
];

/// The case of `ROUTE_REFUSALS` whose fault is in a profile, so its body is a file of its own.
const OWN_BODY_CASE: &str = "pack-state-disabled";

/// A body of a create on vSphere that holds what the faults of validate-route.sh edit.
const VSPHERE_BODY: &str = r#"{"spec": {
  "cloudConfig": {
    "staticIp": true,
    "placement": {"network": {"networkName": "net", "parentPoolUid": "pool-1", "staticIp": true}},
    "sshKeys": ["ssh-ed25519 AAAA barney"],
    "ntpServers": ["pool.ntp.org"],
    "controlPlaneEndpoint": {"type": "VIP", "host": "10.0.0.5"}
  },
  "machinepoolconfig": [{
    "poolConfig": {"name": "worker-pool"},
    "cloudConfig": {
      "instanceType": {"numCPUs": 4, "memoryMiB": 8192, "diskGiB": 60},
      "placements": [{"network": {"networkName": "net", "parentPoolUid": "pool-1", "staticIp": true}}]
    }
  }]
}}"#;

/// A stage for validate-route.sh with the clean bodies of vSphere and EKS in `bodies/`, and the
/// answers of the route to the faults that it makes. `answers` replaces the answer for a case with
/// a verdict and a message: a fault that the route takes, or a refusal for another reason.
fn validate_route_stage(answers: &[(&str, &str, &str)]) -> Stage {
    let mut stage = Stage::new(serde_json::json!({}));
    std::fs::create_dir_all(stage.path("bodies")).unwrap();
    std::fs::write(stage.path("bodies/vsphere-clean.json"), VSPHERE_BODY).unwrap();
    std::fs::write(
        stage.path("bodies/eks-clean.json"),
        r#"{"spec": {"machinepoolconfig": [{"poolConfig": {"name": "worker-pool"}, "cloudConfig": {"amiType": "AL2023_x86_64_STANDARD"}}]}}"#,
    )
    .unwrap();
    // a body of its own: the fault is in a profile
    std::fs::write(stage.path(&format!("bodies/{OWN_BODY_CASE}.json")), "{}").unwrap();
    for (case, message) in ROUTE_REFUSALS {
        let (verdict, message) = answers
            .iter()
            .find(|a| a.0 == case)
            .map_or(("failed", message), |a| (a.1, a.2));
        let messages: Vec<&str> = if message.is_empty() {
            vec![]
        } else {
            vec![message]
        };
        let glob = if case == OWN_BODY_CASE {
            format!("preflight */{case}.json *")
        } else {
            format!("preflight {case}.json *")
        };
        stage.strata(&glob, 1, &route_report(verdict, &messages));
    }
    for engine in ["vsphere", "eks"] {
        stage.strata(
            &format!("preflight */{engine}-clean.json *"),
            0,
            &route_report("passed", &[]),
        );
    }
    stage
}

#[test]
fn validate_route_sh_sends_each_fault_to_the_route_and_expects_a_refusal_that_names_it() {
    let stage = validate_route_stage(&[]);
    let bodies = stage.path("bodies");
    let (o, steps) = stage.run("validate-route.sh", &[bodies.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    assert!(!steps.contains("FAIL"), "{steps}");

    let cases = ROUTE_REFUSALS.map(|(case, _)| case);
    let says: Vec<String> = cases.iter().map(|c| format!("{c}-says")).collect();
    let mut ids = vec!["vsphere-clean", "eks-clean"];
    ids.extend(cases);
    ids.extend(says.iter().map(String::as_str));
    passed(&steps, &ids);
    // the clean body is asked for once, and a case with no body of its engine is named and skipped
    assert_eq!(steps.matches("vsphere-clean ").count(), 1, "{steps}");
    for skipped in [
        "edge-two-node-pxk",
        "azure-ssh-key-empty",
        "ntp-servers-edge",
        "pack-cloud-support",
    ] {
        assert!(
            steps.contains(&format!("skipped {skipped}: no ")),
            "{steps}"
        );
    }
    assert!(
        !steps.contains(&format!("{OWN_BODY_CASE}: no")),
        "an own body is read: {steps}"
    );

    // each fault is one edit of the clean body of its engine, and nothing else changes
    let made =
        |case: &str| -> serde_json::Value { read_json(&stage.path(&format!("out/{case}.json"))) };
    let clean: serde_json::Value = serde_json::from_str(VSPHERE_BODY).unwrap();
    let pool = "/spec/machinepoolconfig/0/cloudConfig";
    assert_eq!(
        made("vsphere-instance-floor-cpu").pointer(&format!("{pool}/instanceType/numCPUs")),
        Some(&serde_json::json!(0))
    );
    assert_eq!(
        made("vsphere-instance-floor-memory").pointer(&format!("{pool}/instanceType/memoryMiB")),
        Some(&serde_json::json!(512))
    );
    assert_eq!(
        made("vsphere-instance-floor-disk").pointer(&format!("{pool}/instanceType/diskGiB")),
        Some(&serde_json::json!(10))
    );
    assert_eq!(
        made("vsphere-instance-floor-no-type").pointer(&format!("{pool}/instanceType")),
        None
    );
    assert_eq!(
        made("ssh-key-format-vsphere")["spec"]["cloudConfig"]["sshKeys"],
        serde_json::json!(["ssh-rsa"])
    );
    let empty = made("vsphere-static-ip-pool-empty");
    assert_eq!(
        empty.pointer("/spec/cloudConfig/placement/network/parentPoolUid"),
        Some(&serde_json::json!(""))
    );
    assert_eq!(
        empty.pointer(&format!("{pool}/placements/0/network/parentPoolUid")),
        Some(&serde_json::json!(""))
    );
    let dhcp = made("vsphere-static-ip-pool-dhcp");
    assert_eq!(
        dhcp.pointer("/spec/cloudConfig/staticIp"),
        Some(&serde_json::json!(false))
    );
    assert_eq!(
        dhcp.pointer(&format!("{pool}/placements/0/network/staticIp")),
        Some(&serde_json::json!(true))
    );
    assert_eq!(
        made("vsphere-controlplane-endpoint-no-host")
            .pointer("/spec/cloudConfig/controlPlaneEndpoint"),
        Some(&serde_json::json!({"type": "VIP", "host": ""}))
    );
    assert_eq!(
        made("ntp-servers-vsphere")["spec"]["cloudConfig"]["ntpServers"],
        serde_json::json!(["not a host name!"])
    );
    assert_eq!(
        made("eks-ami-type-al2").pointer("/spec/machinepoolconfig/0/cloudConfig/amiType"),
        Some(&serde_json::json!("AL2_x86_64"))
    );
    // an edit leaves the rest of the body as it was
    let mut same = made("ssh-key-format-vsphere");
    same["spec"]["cloudConfig"]["sshKeys"] = clean["spec"]["cloudConfig"]["sshKeys"].clone();
    assert_eq!(same, clean);
}

#[test]
fn validate_route_sh_fails_on_a_fault_that_the_route_takes_and_on_a_refusal_for_another_reason() {
    // the route takes an AL2 type: the rule `eks-ami-type` must stay
    let stage = validate_route_stage(&[("eks-ami-type-al2", "passed", "")]);
    let bodies = stage.path("bodies");
    let (o, steps) = stage.run("validate-route.sh", &[bodies.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(1), "{steps}");
    assert!(
        steps
            .lines()
            .any(|l| l.starts_with("FAIL") && l.contains("eks-ami-type-al2 ")),
        "{steps}"
    );
    assert!(
        steps
            .lines()
            .any(|l| l.starts_with("FAIL") && l.contains("eks-ami-type-al2-says ")),
        "the report of a body that passed names no fault: {steps}"
    );
    passed(
        &steps,
        &["eks-ami-type-unknown", "vsphere-instance-floor-cpu"],
    );

    // the route refuses the cpu fault, but not for the fault: the refusal does not name the cpu
    let stage = validate_route_stage(&[(
        "vsphere-instance-floor-cpu",
        "failed",
        "ResourceNotFound: cloud account 'x' does not exist",
    )]);
    let bodies = stage.path("bodies");
    let (o, steps) = stage.run("validate-route.sh", &[bodies.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(1), "{steps}");
    passed(&steps, &["vsphere-instance-floor-cpu"]);
    assert!(
        steps
            .lines()
            .any(|l| l.starts_with("FAIL") && l.contains("vsphere-instance-floor-cpu-says ")),
        "{steps}"
    );
}

#[test]
fn validate_route_sh_prints_its_usage_and_needs_a_directory_of_bodies() {
    let stage = Stage::new(serde_json::json!({}));
    let o = stage.run("validate-route.sh", &["--help"]).0;
    assert_eq!(o.status.code(), Some(0));
    assert!(
        text(&o.stdout).contains("The cluster validate route test (#243)"),
        "{}",
        text(&o.stdout)
    );
    let o = stage.run("validate-route.sh", &[]).0;
    assert_eq!(o.status.code(), Some(2), "no directory is usage, exit 2");
    let o = stage.run("validate-route.sh", &["/no/such/directory"]).0;
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("no directory /no/such/directory"),
        "{}",
        text(&o.stderr)
    );
}

#[test]
fn validate_route_sh_runs_each_case_whose_body_is_an_aws_body() {
    let mut stage = Stage::new(serde_json::json!({}));
    std::fs::create_dir_all(stage.path("bodies")).unwrap();
    for body in std::iter::once("aws-clean").chain(AWS_CASES.iter().map(|case| case.id)) {
        std::fs::write(stage.path(&format!("bodies/{body}.json")), "{}").unwrap();
    }
    // each case goes to the route of AWS: the glob names the cloud, so a case on another engine
    // finds no answer, and passes as a body that the route takes
    answer_aws_cases(&mut stage, &[]);
    let bodies = stage.path("bodies");
    let (o, steps) = stage.run("validate-route.sh", &[bodies.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    let mut ids = vec!["aws-clean".to_string()];
    for case in &AWS_CASES {
        ids.push(case.id.to_string());
        ids.push(format!("{}-says", case.id));
    }
    passed(&steps, &ids.iter().map(String::as_str).collect::<Vec<_>>());
    for case in &AWS_CASES {
        assert!(!steps.contains(&format!("skipped {}", case.id)), "{steps}");
    }
    assert_eq!(steps.matches("aws-clean ").count(), 1, "{steps}");
}

#[test]
fn validate_route_sh_expects_the_route_to_take_a_pack_version_that_does_not_list_the_cloud_type() {
    // the route takes the body, so no route answers the check, and the rule stays (#415)
    let taken = aws_case_taken().id;
    let says = format!("{taken}-says");
    let mut stage = Stage::new(serde_json::json!({}));
    std::fs::create_dir_all(stage.path("bodies")).unwrap();
    for body in ["aws-clean", taken] {
        std::fs::write(stage.path(&format!("bodies/{body}.json")), "{}").unwrap();
    }
    stage.strata(
        "preflight */aws-clean.json --cloud aws *",
        0,
        &route_report("passed", &[]),
    );
    let taken_glob = format!("preflight */{taken}.json --cloud aws *");
    stage.strata(&taken_glob, 0, &route_report("passed", &[]));
    let bodies = stage.path("bodies");
    let (o, steps) = stage.run("validate-route.sh", &[bodies.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    passed(&steps, &[taken, &says]);

    // a route that refuses the body answers the check: both checks fail, so the lead removes the
    // rule from the catalog
    let mut stage = Stage::new(serde_json::json!({}));
    std::fs::create_dir_all(stage.path("bodies")).unwrap();
    for body in ["aws-clean", taken] {
        std::fs::write(stage.path(&format!("bodies/{body}.json")), "{}").unwrap();
    }
    stage.strata(
        "preflight */aws-clean.json --cloud aws *",
        0,
        &route_report("passed", &[]),
    );
    stage.strata(
        &taken_glob,
        1,
        &route_report(
            "failed",
            &["PackNotSupported: the pack does not support aws"],
        ),
    );
    let bodies = stage.path("bodies");
    let (o, steps) = stage.run("validate-route.sh", &[bodies.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(1), "{steps}");
    for id in [taken, &says] {
        assert!(
            steps
                .lines()
                .any(|l| l.starts_with("FAIL") && l.contains(&format!("{id} "))),
            "{id}: {steps}"
        );
    }
}

// ---- route-bodies.sh -------------------------------------------------------------------------------

/// The pack listing of the lab. It holds the packs that route-bodies.sh pins, and the packs that
/// its search for `pack-cloud-support` passes over: one of another registry, one that lists aws,
/// one that lists `all`, one with no cloud type, and one of another layer. `gcp-dns`, which lists
/// gcp alone, is in the listing when `without_aws` is true.
fn route_packs(without_aws: bool) -> String {
    let pack =
        |name: &str, version: &str, registry: &str, layer: &str, clouds: serde_json::Value| {
            serde_json::json!({"metadata": {"uid": format!("pk-{name}")},
            "spec": {"name": name, "version": version, "registryUid": registry,
                "layer": layer, "cloudTypes": clouds}})
        };
    let mut items = vec![
        pack(
            "gcp-elsewhere",
            "1.0.0",
            "reg-other",
            "addon",
            ["gcp"].into(),
        ),
        pack(
            "aws-and-gcp",
            "3.0.0",
            "reg-pub",
            "addon",
            ["aws", "gcp"].into(),
        ),
        pack("all-clouds", "4.0.0", "reg-pub", "addon", ["all"].into()),
        pack(
            "no-clouds",
            "5.0.0",
            "reg-pub",
            "addon",
            serde_json::json!([]),
        ),
        pack("gcp-csi", "6.0.0", "reg-pub", "csi", ["gcp"].into()),
        pack("gloo", "1.2.18", "reg-pub", "addon", ["all"].into()),
        pack(
            "generic-vm-libvirt",
            "1.0.0",
            "reg-pub",
            "infra",
            ["all"].into(),
        ),
        pack("cert-manager", "1.21.1", "reg-pub", "addon", ["all"].into()),
    ];
    if without_aws {
        items.push(pack("gcp-dns", "2.0.0", "reg-pub", "addon", ["gcp"].into()));
    }
    serde_json::json!({"items": items, "listmeta": {"count": items.len()}}).to_string()
}

/// A `strata` for route-bodies.sh. `preflight BODY` keeps a copy of BODY in strata/seen/, and logs
/// `VALIDATE <file>` to the log of the stand-in curl. Palette answers 404 for a profile of BODY
/// that the log shows a DELETE for. Strata then exits 2 with no report. A call it does not answer
/// goes to the answers of the stage.
const FAKE_STRATA_ROUTE: &str = r#"#!/usr/bin/env bash
if [ "$1" = preflight ]; then
  mkdir -p "$FAKE_STRATA/seen"
  cp "$2" "$FAKE_STRATA/seen/$(basename "$2")"
  echo "VALIDATE $(basename "$2")" >> "$FAKE_LOG"
  for uid in $(jq -r '.spec.profiles[].uid' "$2"); do
    if grep -qx "DELETE /v1/clusterprofiles/$uid" "$FAKE_LOG"; then
      echo "GET /v1/clusterprofiles/$uid failed with HTTP 404: ResourceNotFound" >&2
      exit 2
    fi
  done
fi
exec "$(dirname "$0")/fake-strata-rules" "$@"
"#;

/// A stage of route-bodies.sh in a lab on AWS: the account `lab-aws`, the release profile `r-1`
/// on AWS, the registry `Public Repo`, and the packs `packs`. Palette makes a profile for each
/// `POST /v1/clusterprofiles` with the uid `uid-<name>`, and publishes and deletes it. The
/// validate route refuses the fault of each case, except `pack-cloud-support` and the cases in
/// `takes`, which it takes.
fn route_bodies_stage(packs: &str, takes: &[&str]) -> Stage {
    let mut stage = Stage::new(serde_json::json!({
        "project_uid": "p-1", "release_profile_uid": "r-1",
    }));
    let tfvars = stage.path("terraform.tfvars");
    std::fs::write(
        &tfvars,
        "cloud = \"aws\"\ncloud_account_name = \"lab-aws\"\nregion = \"us-east-2\"\nssh_key_name = \"lab\"\n",
    )
    .unwrap();
    stage.env("TFVARS", tfvars.to_str().unwrap());
    stage.route(
        "/v1/cloudaccounts/summary",
        r#"{"items":[{"kind":"aws","metadata":{"name":"lab-aws","uid":"acc-1"}}]}"#,
    );
    stage.route(
        "/v1/clusterprofiles/r-1",
        r#"{"spec":{"published":{"cloudType":"aws"}}}"#,
    );
    stage.route(
        "/v1/registries/metadata",
        r#"{"items":[{"name":"Public Repo","uid":"reg-pub"},{"name":"Other","uid":"reg-other"}]}"#,
    );
    stage.route(PROFILE_LISTING, r#"{"items":[]}"#);
    stage.route("/v1/packs", packs);
    executable(
        &stage.path("routes/v1_clusterprofiles.sh"),
        "#!/usr/bin/env bash\njq -c '{uid: (\"uid-\" + .metadata.name)}' \"$FAKE_ROUTES/sent/POST_v1_clusterprofiles\"\n",
    );
    for case in &AWS_CASES {
        let name = case.profile;
        stage.route(&format!("/v1/clusterprofiles/uid-{name}/publish"), "{}");
        stage.route(&format!("/v1/clusterprofiles/uid-{name}"), "{}");
    }
    stage.wrap_strata(FAKE_STRATA_ROUTE);
    answer_aws_cases(&mut stage, takes);
    stage
}

/// The profiles that the script deleted, in order, as the uids.
fn deleted_profiles(stage: &Stage) -> Vec<String> {
    lines_of(&stage.path("curl.log"))
        .iter()
        .filter_map(|l| l.strip_prefix("DELETE /v1/clusterprofiles/"))
        .map(str::to_string)
        .collect()
}

#[test]
fn route_bodies_sh_runs_validate_route_on_the_bodies_while_the_profiles_live_and_deletes_each_after()
 {
    let stage = route_bodies_stage(&route_packs(true), &[]);
    let bodies = stage.path("bodies");
    let (o, steps) = stage.run("route-bodies.sh", &[bodies.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    assert!(
        !steps.contains("FAIL") && !steps.contains("skipped"),
        "{steps}"
    );

    let mut ids = vec!["aws-clean-body".to_string(), "validate-route".to_string()];
    for AwsCase { id: case, .. } in &AWS_CASES {
        ids.push(format!("{case}-body"));
        ids.push(format!("{case}-deleted"));
    }
    passed(&steps, &ids.iter().map(String::as_str).collect::<Vec<_>>());

    // the clean body is the body of create_body: the account, the release profile, the region
    let clean = read_json(&bodies.join("aws-clean.json"));
    assert_eq!(clean["metadata"]["name"], "strata-route-aws");
    assert_eq!(clean["spec"]["cloudAccountUid"], "acc-1");
    assert_eq!(clean["spec"]["cloudConfig"]["region"], "us-east-2");
    assert_eq!(
        clean["spec"]["profiles"],
        serde_json::json!([{"uid": "r-1", "packValues": [], "variables": []}])
    );

    // a case body is the clean body and one more profile, the one that the script made. The
    // directory holds no case body at the end, so the test reads what validate-route.sh got.
    for AwsCase {
        id: case,
        profile: name,
        ..
    } in &AWS_CASES
    {
        let mut body = read_json(&stage.path(&format!("strata/seen/{case}.json")));
        assert_eq!(
            body["spec"]["profiles"],
            serde_json::json!([
                {"uid": "r-1", "packValues": [], "variables": []},
                {"uid": format!("uid-{name}"), "packValues": [], "variables": []},
            ]),
            "{case}"
        );
        body["spec"]["profiles"] = clean["spec"]["profiles"].clone();
        assert_eq!(body, clean, "{case}: nothing else changes");
    }

    // each profile is an add-on profile with the pack of its case, of the cloud type of its case
    let made: Vec<(String, String, String, String, String, String)> = stage
        .sent("POST", "/v1/clusterprofiles")
        .iter()
        .map(|p| {
            let template = &p["spec"]["template"];
            let pack = &template["packs"][0];
            let field = |v: &serde_json::Value| v.as_str().unwrap().to_string();
            (
                field(&p["metadata"]["name"]),
                field(&template["type"]),
                field(&template["cloudType"]),
                field(&pack["name"]),
                field(&pack["tag"]),
                field(&pack["uid"]),
            )
        })
        .collect();
    let want = [
        (
            "strata-route-pack-disabled",
            "all",
            "gloo",
            "1.2.18",
            "pk-gloo",
        ),
        (
            "strata-route-pack-deleted",
            "all",
            "generic-vm-libvirt",
            "1.0.0",
            "pk-generic-vm-libvirt",
        ),
        (
            "strata-route-cloud-mismatch",
            "gcp",
            "cert-manager",
            "1.21.1",
            "pk-cert-manager",
        ),
        (
            "strata-route-cloud-support",
            "all",
            "gcp-dns",
            "2.0.0",
            "pk-gcp-dns",
        ),
    ]
    .map(|(name, cloud, pack, tag, uid)| {
        let s = str::to_string;
        (s(name), s("add-on"), s(cloud), s(pack), s(tag), s(uid))
    });
    assert_eq!(made, want);

    // validate-route.sh sends each body after the last create and before the first delete, so
    // Palette still holds each profile. A case passes only when its profile is there.
    let log = lines_of(&stage.path("curl.log"));
    let last_create = log
        .iter()
        .rposition(|l| l == "POST /v1/clusterprofiles")
        .unwrap();
    let first_delete = log.iter().position(|l| l.starts_with("DELETE ")).unwrap();
    let sent: Vec<usize> = (0..log.len())
        .filter(|&i| log[i].starts_with("VALIDATE "))
        .collect();
    assert_eq!(sent.len(), 5, "aws-clean and the four cases: {log:?}");
    assert!(
        sent.iter().all(|&i| last_create < i && i < first_delete),
        "{log:?}"
    );
    let route_steps = std::fs::read_to_string(stage.path("out/validate-route/steps.log")).unwrap();
    let mut answered = vec!["aws-clean".to_string()];
    for AwsCase { id: case, .. } in &AWS_CASES {
        answered.push(case.to_string());
        answered.push(format!("{case}-says"));
    }
    passed(
        &route_steps,
        &answered.iter().map(String::as_str).collect::<Vec<_>>(),
    );

    // the script deletes each profile that it made, and the body of each case, and nothing else
    assert_eq!(
        deleted_profiles(&stage),
        AWS_CASES.map(|case| format!("uid-{}", case.profile))
    );
    for AwsCase { id: case, .. } in &AWS_CASES {
        assert!(!bodies.join(format!("{case}.json")).exists(), "{case}");
    }
    assert!(
        bodies.join("aws-clean.json").exists(),
        "it names the release profile, which stays"
    );
}

#[test]
fn route_bodies_sh_fails_and_deletes_each_profile_when_the_route_takes_a_fault() {
    let stage = route_bodies_stage(&route_packs(true), &["profile-cloud-mismatch"]);
    let bodies = stage.path("bodies");
    let (o, steps) = stage.run("route-bodies.sh", &[bodies.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    assert!(
        steps
            .lines()
            .any(|l| l.starts_with("FAIL") && l.contains("validate-route ")),
        "a route that takes a fault fails the run: {steps}"
    );
    let route_steps = std::fs::read_to_string(stage.path("out/validate-route/steps.log")).unwrap();
    assert!(
        route_steps
            .lines()
            .any(|l| l.starts_with("FAIL") && l.contains("profile-cloud-mismatch ")),
        "{route_steps}"
    );
    // the failure does not leave a profile or a body
    assert_eq!(
        deleted_profiles(&stage),
        AWS_CASES.map(|case| format!("uid-{}", case.profile))
    );
    passed(
        &steps,
        &AWS_CASES
            .map(|case| format!("{}-deleted", case.id))
            .each_ref()
            .map(String::as_str),
    );
    for AwsCase { id: case, .. } in &AWS_CASES {
        assert!(!bodies.join(format!("{case}.json")).exists(), "{case}");
    }
}

#[test]
fn route_bodies_sh_deletes_each_profile_that_it_made_when_a_step_fails() {
    let stage = route_bodies_stage(&route_packs(true), &[]);
    // the write of the third body fails, after Palette made its profile
    stage.command(
        "jq",
        r#"#!/usr/bin/env bash
PATH=${PATH#"$(dirname "$0")":}
case "$*" in *uid-strata-route-cloud-mismatch*) exit 5 ;; esac
exec jq "$@"
"#,
    );
    let bodies = stage.path("bodies");
    let (o, steps) = stage.run("route-bodies.sh", &[bodies.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(2), "{steps}{}", text(&o.stderr));
    assert!(
        text(&o.stderr).contains("cannot write"),
        "{}",
        text(&o.stderr)
    );
    assert_eq!(stage.sent("POST", "/v1/clusterprofiles").len(), 3);
    assert_eq!(
        deleted_profiles(&stage),
        [
            "uid-strata-route-pack-disabled",
            "uid-strata-route-pack-deleted",
            "uid-strata-route-cloud-mismatch"
        ]
    );
    passed(
        &steps,
        &[
            "pack-state-disabled-deleted",
            "pack-state-deleted-deleted",
            "profile-cloud-mismatch-deleted",
        ],
    );
    assert!(
        !bodies.join("profile-cloud-mismatch.json").exists(),
        "a body that failed stays out of the directory"
    );
    assert!(!bodies.join("pack-cloud-support.json").exists());
    let validations = lines_of(&stage.path("curl.log"))
        .iter()
        .filter(|l| l.starts_with("VALIDATE "))
        .count();
    assert_eq!(validations, 0, "the script stopped before the route");
    for AwsCase { id: case, .. } in &AWS_CASES {
        assert!(!bodies.join(format!("{case}.json")).exists(), "{case}");
    }
}

#[test]
fn route_bodies_sh_skips_a_case_whose_profile_palette_refuses_and_leaves_no_body_for_it() {
    let stage = route_bodies_stage(&route_packs(true), &[]);
    stage.route(
        "/v1/clusterprofiles",
        r#"{"code":"DisabledPacksForbidden","message":"Disabled packs 'gloo' are forbidden"}"#,
    );
    std::fs::remove_file(stage.path("routes/v1_clusterprofiles.sh")).unwrap();
    std::fs::write(stage.path("routes/v1_clusterprofiles.status"), "500").unwrap();
    // a body of an earlier run names a profile that is gone
    let bodies = stage.path("bodies");
    std::fs::create_dir_all(&bodies).unwrap();
    std::fs::write(bodies.join("pack-state-disabled.json"), "{}").unwrap();

    let (o, steps) = stage.run("route-bodies.sh", &[bodies.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    for AwsCase { id: case, .. } in &AWS_CASES {
        assert!(
            steps.contains(&format!(
                "skipped {case}: Palette refuses the profile with "
            )),
            "{steps}"
        );
        assert!(!bodies.join(format!("{case}.json")).exists(), "{case}");
    }
    assert!(
        steps.contains("skipped pack-state-disabled: Palette refuses the profile with gloo@1.2.18: api.sh: POST /v1/clusterprofiles answered HTTP 500: {\"code\":\"DisabledPacksForbidden\""),
        "{steps}"
    );
    passed(&steps, &["aws-clean-body", "validate-route"]);
    assert!(bodies.join("aws-clean.json").exists());
    assert!(
        deleted_profiles(&stage).is_empty(),
        "Palette made no profile"
    );
}

#[test]
fn route_bodies_sh_skips_the_two_pack_state_cases_as_the_create_of_palette_refuses_their_packs() {
    let stage = route_bodies_stage(&route_packs(true), &[]);
    // as live Palette: the create refuses the pack with the mark disabled, and a deleted pack
    executable(
        &stage.path("routes/v1_clusterprofiles.sh"),
        r#"#!/usr/bin/env bash
sent="$FAKE_ROUTES/sent/POST_v1_clusterprofiles"
status="$FAKE_ROUTES/v1_clusterprofiles.status"
refuse() { echo 500 > "$status"; jq -n -c --arg c "$1" --arg m "$2" '{code: $c, message: $m}'; }
case "$(jq -r '.spec.template.packs[0].name' "$sent")" in
  gloo) refuse DisabledPacksForbidden "Disabled packs 'gloo' are forbidden" ;;
  generic-vm-libvirt) refuse ClusterProfileInvalidPackState "Cluster Profile operation not supported as pack generic-vm-libvirt:1.0.0 is deleted" ;;
  *) rm -f "$status"; jq -c '{uid: ("uid-" + .metadata.name)}' "$sent" ;;
esac
"#,
    );
    let bodies = stage.path("bodies");
    let (o, steps) = stage.run("route-bodies.sh", &[bodies.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    assert!(
        steps.contains("skipped pack-state-disabled: Palette refuses the profile with gloo@1.2.18: api.sh: POST /v1/clusterprofiles answered HTTP 500: {\"code\":\"DisabledPacksForbidden\""),
        "{steps}"
    );
    assert!(
        steps.contains("skipped pack-state-deleted: Palette refuses the profile with generic-vm-libvirt@1.0.0: api.sh: POST /v1/clusterprofiles answered HTTP 500: {\"code\":\"ClusterProfileInvalidPackState\""),
        "{steps}"
    );

    // the other two cases run on their profiles, and the script deletes those profiles
    let seen = |case: &str| stage.path(&format!("strata/seen/{case}.json"));
    for case in ["profile-cloud-mismatch", "pack-cloud-support"] {
        assert!(seen(case).exists(), "the route got the body of {case}");
    }
    for case in ["pack-state-disabled", "pack-state-deleted"] {
        assert!(!seen(case).exists(), "the route got no body of {case}");
    }
    assert_eq!(
        deleted_profiles(&stage),
        [
            "uid-strata-route-cloud-mismatch",
            "uid-strata-route-cloud-support"
        ]
    );
}

#[test]
fn route_bodies_sh_skips_pack_cloud_support_when_the_registry_has_no_pack_without_aws() {
    let stage = route_bodies_stage(&route_packs(false), &[]);
    let bodies = stage.path("bodies");
    let (o, steps) = stage.run("route-bodies.sh", &[bodies.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    assert!(
        steps.contains("skipped pack-cloud-support: Public Repo lists no pack in the layer addon without aws; give PACK_CLOUD_SUPPORT=<pack>@<version>."),
        "{steps}"
    );
    assert!(!bodies.join("pack-cloud-support.json").exists());
    for AwsCase {
        id: case,
        profile: name,
        ..
    } in &AWS_CASES[..3]
    {
        passed(
            &steps,
            &[&format!("{case}-body"), &format!("{case}-deleted")],
        );
        assert!(
            stage.path(&format!("strata/seen/{case}.json")).exists(),
            "the route got the body of {name}"
        );
        assert!(!bodies.join(format!("{case}.json")).exists(), "{name}");
    }
    assert!(!stage.path("strata/seen/pack-cloud-support.json").exists());
    assert_eq!(stage.sent("POST", "/v1/clusterprofiles").len(), 3);

    // a pack that the lab names takes the place of the search
    let mut stage = route_bodies_stage(&route_packs(false), &[]);
    stage.env("PACK_CLOUD_SUPPORT", "all-clouds@4.0.0");
    let (o, steps) = stage.run("route-bodies.sh", &[bodies.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    passed(
        &steps,
        &["pack-cloud-support-body", "pack-cloud-support-deleted"],
    );
    let last = stage.sent("POST", "/v1/clusterprofiles").pop().unwrap();
    assert_eq!(last["metadata"]["name"], "strata-route-cloud-support");
    assert_eq!(last["spec"]["template"]["packs"][0]["name"], "all-clouds");
    assert_eq!(last["spec"]["template"]["packs"][0]["tag"], "4.0.0");
    let searches = lines_of(&stage.path("curl.log"))
        .iter()
        .filter(|l| *l == "GET /v1/packs")
        .count();
    assert_eq!(searches, 4, "the four pins, and no search of the listing");
}

#[test]
fn route_bodies_sh_stops_when_the_release_profile_is_not_on_aws() {
    let stage = route_bodies_stage(&route_packs(true), &[]);
    stage.route(
        "/v1/clusterprofiles/r-1",
        r#"{"spec":{"published":{"cloudType":"gcp"}}}"#,
    );
    let bodies = stage.path("bodies");
    let (o, _) = stage.run("route-bodies.sh", &[bodies.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("is of cloud type gcp, not aws"),
        "{}",
        text(&o.stderr)
    );
    assert!(stage.sent("POST", "/v1/clusterprofiles").is_empty());
}

#[test]
fn route_bodies_sh_prints_its_usage_and_needs_a_directory() {
    let stage = Stage::new(serde_json::json!({}));
    let o = stage.run("route-bodies.sh", &["--help"]).0;
    assert_eq!(o.status.code(), Some(0));
    assert!(
        text(&o.stdout).contains("The case bodies of validate-route.sh on AWS (#408)"),
        "{}",
        text(&o.stdout)
    );
    let o = stage.run("route-bodies.sh", &[]).0;
    assert_eq!(o.status.code(), Some(2), "no directory is usage, exit 2");
}

// ---- loop.sh ---------------------------------------------------------------------------------------

/// A stage in a lab on AWS with the cloud account `lab-aws`, and a run that holds a repository of
/// the team: an empty one on `main`, where `loop.sh` resumes.
fn lab_stage() -> Stage {
    let mut stage = Stage::new(serde_json::json!({
        "project_uid": "p-1", "cluster_uid": "c-a", "release_profile_uid": "r-1",
        "next_profile_uid": "n-1", "addon_profile_uid": "ad-1",
        "profile_name": "strata-live-profile",
    }));
    let tfvars = stage.path("terraform.tfvars");
    std::fs::write(
        &tfvars,
        "cloud = \"aws\"\ncloud_account_name = \"lab-aws\"\nregion = \"us-east-2\"\nssh_key_name = \"lab\"\n",
    )
    .unwrap();
    stage.env("TFVARS", tfvars.to_str().unwrap());
    stage.route(
        "/v1/cloudaccounts/summary",
        r#"{"items":[{"kind":"aws","metadata":{"name":"lab-aws","uid":"acc-1"}}]}"#,
    );
    let repo = stage.path("out/repo");
    std::fs::create_dir_all(&repo).unwrap();
    let init = Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(init.status.success(), "{}", text(&init.stderr));
    stage
}

#[test]
fn loop_sh_stage_2_asks_palette_under_a_name_the_project_lacks_and_writes_the_comment_of_the_pull_request()
 {
    let mut stage = lab_stage();
    stage.strata(
        "candidate n-1 -o change.yaml",
        0,
        "name: strata-live-profile\nversion: 1.0.1\nlayers:\n  - name: ubuntu-aws\n",
    );
    stage.strata(
        "snapshot --from-candidate ask.yaml --validate -o ask.json",
        0,
        r#"{"source":{"paletteValidation":{"accepts":true}}}"#,
    );
    // the comment compares the record of the release with the candidate, and both differ
    stage.strata(
        "snapshot --from-candidate change.yaml -o candidate.json",
        0,
        "{}",
    );
    stage.strata(
        "compare releases/1.0.0.json candidate.json --format json",
        1,
        r#"{"summary":{"maxImpact":"repave"}}"#,
    );
    stage.strata(
        "compare releases/1.0.0.json candidate.json --format markdown",
        1,
        "**Result:** CHANGED · max impact: `repave`\n",
    );
    let release = stage.path("out/repo/releases/1.0.0.json");
    std::fs::create_dir_all(release.parent().unwrap()).unwrap();
    std::fs::write(&release, "{}").unwrap();

    // the merge at the end of the stage needs a commit on main, which this repository lacks, so
    // the script stops there with exit 2
    let (_, steps) = stage.run("loop.sh", &["2"]);
    passed(
        &steps,
        &[
            "candidate",
            "candidate-record",
            "ask-candidate",
            "comment",
            "comment-says",
            "commit-branch",
        ],
    );
    let ask = std::fs::read_to_string(stage.path("out/repo/ask.yaml")).unwrap();
    assert!(ask.starts_with("name: strata-live-profile-ask\n"), "{ask}");
    assert!(ask.contains("version: 1.0.1"), "{ask}");
}

#[test]
fn loop_sh_stage_5_asks_palette_about_the_bump_of_each_pin_that_the_nightly_job_finds_behind() {
    let mut stage = lab_stage();
    // the report of `behind`, with the answer of Palette in the bump when there is one
    let report = |validation: Option<serde_json::Value>| {
        let mut bump =
            serde_json::json!({"to": "1.37.0", "impact": "repave", "flags": ["IDENTITY"]});
        if let Some(validation) = validation {
            bump["paletteValidation"] = validation;
        }
        serde_json::json!({"schema": "strata-behind/1", "generatedAt": "2026-09-26T06:00:00Z",
            "pins": [{"name": "kubernetes", "layer": "k8s", "registry": "Public Repo",
                "registryUid": "reg-public", "version": "1.35.3", "tag": "1.35.3",
                "behind": true, "newer": [{"version": "1.37.0"}],
                "pinnedBy": [{"path": "releases/1.0.0.json",
                    "subject": "strata-live-profile@1.0.0"}],
                "bump": bump}]})
        .to_string()
    };
    stage.strata(
        "behind --quiet --format json --no-impact *",
        1,
        &report(None),
    );
    stage.strata(
        "behind --quiet --format json releases/*.json",
        1,
        &report(Some(upgrade_refused())),
    );
    let (_, steps) = stage.run("loop.sh", &["5"]);
    passed(&steps, &["upstream", "upstream-asks", "upstream-asks-says"]);
    let issue = std::fs::read_to_string(stage.path("out/upstream-asks.out")).unwrap();
    assert!(
        issue.contains(&format!(
            "| 1.37.0 | 1.37.0: repave IDENTITY<br>palette refuses: {UPGRADE_NOT_SUPPORTED_CODE} on fred | - |"
        )),
        "{issue}"
    );
    // the check with --no-impact sends no question: its issue holds no answer
    let quick = std::fs::read_to_string(stage.path("out/upstream.out")).unwrap();
    assert!(!quick.contains("palette"), "{quick}");
}

#[test]
fn loop_sh_stages_3_and_4_ask_palette_about_the_move_and_run_preflight_on_the_body_of_a_create() {
    let mut stage = lab_stage();
    stage.strata(
        "compare cluster:c-a/r-1 n-1 --repave --json",
        1,
        r#"{"paletteRepave":{"required":true,"reasons":[],"agrees":true}}"#,
    );
    stage.strata(
        "compare cluster:c-a/r-1 n-1 --validate --json",
        1,
        r#"{"paletteValidation":{"accepts":true,"errors":[]}}"#,
    );
    stage.strata(
        "preflight build.json --cloud aws --tier static --offline --quiet",
        0,
        "OK: each counted check passed\n",
    );
    stage.strata(
        "preflight build.json --cloud aws --tier static,palette --no-exec --strict --quiet",
        0,
        "OK: each counted check passed\n",
    );
    stage.strata(
        "preflight build.json --cloud aws --tier palette --rule template-outside --json --quiet",
        0,
        r#"{"checks":[{"id":"template-outside","verdict":"passed","requests":[{"method":"GET","path":"/v1/clusterprofiles/r-1","answer":"HTTP 200"}]}]}"#,
    );
    stage.strata(
        "preflight build.json --cloud aws --tier palette --rule instance-offering --json --quiet",
        0,
        r#"{"checks":[{"id":"instance-offering","verdict":"passed","ran":"GET /v1/clouds/aws/regions/us-east-2/instancetypes?cloudAccountUid=acc-1","answer":"HTTP 200"}]}"#,
    );
    stage.strata(
        "preflight build-fault.json --cloud aws --tier static,palette --no-exec --strict --quiet",
        1,
        "FAIL: 1 check(s) failed: account-missing\n",
    );

    let (_, steps) = stage.run("loop.sh", &["3"]);
    passed(
        &steps,
        &[
            "ask-repave",
            "ask-repave-answer",
            "ask-validate",
            "ask-validate-answer",
            "build-body",
            "preflight-static",
            "preflight-live",
            "preflight-each",
            "preflight-offering",
            "preflight-fault",
        ],
    );
    // the body is the create of a cluster on the release in the account of the lab
    let body = read_json(&stage.path("out/build.json"));
    assert_eq!(body["metadata"]["name"], "strata-loop-build");
    assert_eq!(body["spec"]["cloudAccountUid"], "acc-1");
    assert_eq!(body["spec"]["profiles"][0]["uid"], "r-1");
    assert_eq!(body["spec"]["cloudConfig"]["region"], "us-east-2");
    let fault = read_json(&stage.path("out/build-fault.json"));
    assert_eq!(fault["spec"]["cloudAccountUid"], "000000000000000000000000");
    // the asks and the preflight write nothing to Palette: the script sent only reads
    let curl = std::fs::read_to_string(stage.path("curl.log")).unwrap();
    assert!(
        curl.lines().all(|l| l.starts_with("GET ")),
        "a request that is not a read:\n{curl}"
    );
}
