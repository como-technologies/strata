//! The scripts of the live example under examples/live/.
//! No network: every host and every command outside this repository is a recording stand-in.

use crate::common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::*;
use strata::api::codes::{PROFILE_EXISTS_CODE, UPGRADE_NOT_SUPPORTED_CODE};

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
    let o = c.output_locked().unwrap();
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

/// A `curl` that answers from files: the status in `$FAKE_CURL_STATUS`, the body in
/// `$FAKE_CURL_BODY`.
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
        .output_locked()
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
    assert!(urls.contains(&cluster_route("c-123")), "{urls}");
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

/// A `curl` that answers by path. It gives the body of `<api path with / as _>` under
/// `$FAKE_ROUTES`, with status 200, or 404 when there is no such file.
///
/// A `<key>.status` file overrides the status. A `<key>.keyed.status` file overrides it again for
/// a request with an `ApiKey` header. A `<key>.as-<ApiKey>.status` file overrides it for a request
/// whose `ApiKey` header holds that value, and the file `<key>.as-<ApiKey>` is the body of that
/// answer. When an executable `$FAKE_ROUTES/<route key>.sh` exists, its output is the body of each
/// answer.
///
/// It logs each request as `METHOD path` to `$FAKE_LOG`. It keeps each request body as
/// `$FAKE_ROUTES/sent/<METHOD>_<route key>`. It adds the body to
/// `$FAKE_ROUTES/sent/<METHOD>_<route key>.all`, in the order of the requests. It keeps the query
/// and the headers as `$FAKE_ROUTES/sent/<METHOD>_<route key>.request`. The line of the
/// `ApiKey` header holds `(hidden)` in place of the key.
///
/// It logs the arguments of each call as a line of `$FAKE_ROUTES/sent/curl.args`, as `/proc/<pid>/cmdline`
/// shows them. A config on standard input (`--config -`) holds the header `ApiKey`, as `palette_curl` sends
/// it. The stand-in appends that config to `$FAKE_ROUTES/sent/curl.config`, and it reads the header from it.
const FAKE_API_CURL: &str = r#"#!/usr/bin/env bash
out=""; write=""; url=""; method=GET; sent=""; headers=""; config=""
mkdir -p "$FAKE_ROUTES/sent"
echo "$*" >> "$FAKE_ROUTES/sent/curl.args"
while [ $# -gt 0 ]; do
  case "$1" in
    -o) out=$2; shift 2 ;;
    -w) write=$2; shift 2 ;;
    -X) method=$2; shift 2 ;;
    --data-binary) sent=${2#@}; shift 2 ;;
    -H) headers="$headers$2"$'\n'; shift 2 ;;
    --config) [ "$2" = - ] || exit 9; config=$(cat); shift 2 ;;
    -d) shift 2 ;;
    -*) shift ;;
    *) url=$1; shift ;;
  esac
done
if [ -n "$config" ]; then
  printf '%s\n' "$config" >> "$FAKE_ROUTES/sent/curl.config"
  headers="$(printf '%s\n' "$config" | sed -n 's/^header = "\(.*\)"$/\1/p')"$'\n'"$headers"
fi
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
/// It fails when `$FAKE_HELM_FAIL` has a value. It logs its arguments and the Helm home that it
/// gets to `$FAKE_TOOL_LOG`.
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
/// It fails when `$FAKE_CRANE_FAIL` has a value, and logs its arguments to `$FAKE_TOOL_LOG`.
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

/// The sync route of `reg-1` for `FAKE_API_CURL`: Palette refuses the first `$SYNC_REFUSALS`
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
    let (uid, values) = (
        format!("pk-{name}-{version}"),
        format!("values of {name}\n"),
    );
    PackFixture::new(name, "addon", version, &uid, registry, "", &values).summary(&[])
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
        .output_locked()
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

    // a script that sets run_id names its own run, and a missing binary omits the version
    let o = lib_demo(dir.path(), "/no/such/strata", &["1", "custom-run"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert!(
        text(&o.stdout).contains("\nRun `custom-run`, cluster `c-1`, add-on `a-1`.\n"),
        "{}",
        text(&o.stdout)
    );
}

#[test]
fn lib_sh_stops_the_send_of_the_report_whatever_the_machine_says() {
    let dir = tempfile::tempdir().unwrap();
    let probe = dir.path().join("probe.sh");
    executable(
        &probe,
        &format!(
            "#!/usr/bin/env bash\nset -uo pipefail\nscript=probe\n. '{lib}'\nprintf '%s' \"$STRATA_REPORT\"\n",
            lib = live_script("lib.sh").display()
        ),
    );
    // the environment and a file of the home both start the send. The script sets the variable,
    // and the environment has priority over the file.
    std::fs::write(dir.path().join(".palette.env"), "STRATA_REPORT=1\n").unwrap();
    for machine in [Some("1"), Some("yes"), None] {
        let mut c = Command::new("bash");
        c.arg(&probe)
            .env("HOME", dir.path())
            .env("STRATA_BIN", "/no/such/strata")
            .env_remove("STRATA_REPORT");
        if let Some(setting) = machine {
            c.env("STRATA_REPORT", setting);
        }
        let o = c.output_locked().unwrap();
        assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
        assert_eq!(text(&o.stdout), "0", "{machine:?}");
    }
}

/// `isolate_git` of lib.sh gives git the author that the script names, and removes each git
/// setting of the environment. So a git user of the machine does not hide the config of the
/// repository.
#[test]
fn lib_sh_gives_git_the_author_of_the_script_and_no_setting_of_the_machine() {
    let dir = tempfile::tempdir().unwrap();
    let probe = dir.path().join("probe.sh");
    executable(
        &probe,
        &format!(
            "#!/usr/bin/env bash\nset -uo pipefail\nscript=probe\n. '{lib}'\nisolate_git probe\n\
             git init -q repo && git -C repo config user.name local\n\
             git -C repo config user.name\n\
             git -C repo var GIT_AUTHOR_IDENT | cut -d' ' -f1-2\n",
            lib = live_script("lib.sh").display()
        ),
    );
    for machine in MACHINE_GIT_USER {
        let work = tempfile::tempdir().unwrap();
        let o = Command::new("bash")
            .arg(&probe)
            .current_dir(work.path())
            .env("STRATA_BIN", "/no/such/strata")
            .envs(machine.iter().copied())
            .output_locked()
            .unwrap();
        assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
        assert_eq!(
            text(&o.stdout),
            "local\nprobe <probe@example.com>\n",
            "{machine:?}"
        );
    }
}

#[test]
fn common_sh_defines_die_and_usage_and_nothing_else_and_it_prints_nothing_when_a_script_sources_it()
{
    // lib.sh stops on a stale binary of strata. common.sh is for a script that runs no strata, so
    // it holds no check
    let dir = tempfile::tempdir().unwrap();
    let demo = dir.path().join("demo.sh");
    executable(
        &demo,
        &format!(
            "#!/usr/bin/env bash\n# The demo of common.sh.\n#\n#   demo.sh\nset -uo pipefail\nscript=demo\n. '{common}'\ndeclare -F\nusage\ndie 'it failed'\n",
            common = live_script("common.sh").display()
        ),
    );
    // no variable of the machine, so that no exported function of the machine shows in the list
    let o = Command::new("bash")
        .arg(&demo)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap())
        .output_locked()
        .unwrap();
    assert_eq!(o.status.code(), Some(2));
    assert_eq!(
        text(&o.stdout),
        "declare -f die\ndeclare -f usage\nThe demo of common.sh.\n\n  demo.sh\n"
    );
    assert_eq!(text(&o.stderr), "demo: it failed\n");
}

#[test]
fn die_and_usage_are_defined_in_common_sh_alone_and_the_scripts_that_have_them_source_it() {
    // the same two lines were in lib.sh, contract.sh, and contract-profiles.sh, and then in the
    // scripts that run no strata
    let common = repo_file("examples/live/common.sh");
    let die = r#"die() { echo "$script: $*" >&2; exit 2; }"#;
    assert!(common.lines().any(|l| l == die), "common.sh has die");
    assert!(
        common
            .lines()
            .any(|l| l.starts_with("usage() { sed -n '2,/^set -uo/p'")),
        "common.sh has usage"
    );
    // any head of the two functions is a copy, also one with another text or an indent
    for dir in ["examples/live", "deploy"] {
        for path in walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join(dir)) {
            if path.extension().is_none_or(|e| e != "sh")
                || path.ends_with("examples/live/common.sh")
            {
                continue;
            }
            let script = std::fs::read_to_string(&path).unwrap();
            assert!(
                !script.lines().any(|l| {
                    let l = l.trim_start();
                    l.starts_with("die()") || l.starts_with("usage()")
                }),
                "{} defines die or usage again: source common.sh",
                path.display()
            );
        }
    }
    for (file, source) in [
        ("examples/live/lib.sh", ". \"$live/common.sh\"\n"),
        ("examples/live/contract.sh", ". \"$live/common.sh\"\n"),
        (
            "examples/live/contract-profiles.sh",
            ". \"$live/common.sh\"\n",
        ),
        ("examples/live/api.sh", ". \"$live/common.sh\"\n"),
        ("examples/live/tenant.sh", ". \"$live/common.sh\"\n"),
        ("examples/live/tofu-install.sh", ". \"$live/common.sh\"\n"),
        (
            "deploy/contract/timer.sh",
            ". \"$repo/examples/live/common.sh\"\n",
        ),
        (
            "deploy/report/deploy.sh",
            ". \"$repo/examples/live/common.sh\"\n",
        ),
    ] {
        assert!(repo_file(file).contains(source), "{file} sources common.sh");
    }
}

/// The route that gives the release, as Palette answers it. It gives the release to a call with no
/// key, and HTTP 500 to a call with an `ApiKey` header.
fn release_route(routes: &Path) {
    std::fs::create_dir_all(routes).unwrap();
    std::fs::write(
        routes.join("v1_mgmt_app_version"),
        r#"{"version":{"installedVersion":"4.10.17-patch.1"}}"#,
    )
    .unwrap();
    std::fs::write(routes.join("v1_mgmt_app_version.keyed.status"), "500").unwrap();
}

/// The key of Palette is in no argument of `curl`. A process shows its arguments in
/// `/proc/<pid>/cmdline`, so a call that sends the key puts the header in a config on standard input.
#[test]
fn the_key_of_palette_is_in_no_argument_of_curl() {
    const KEY: &str = "k-SECRET-0123456789";
    let root = tempfile::tempdir().unwrap();
    let routes = root.path().join("routes");
    std::fs::create_dir_all(&routes).unwrap();
    for (route, body) in [
        ("v1_clusterprofiles_p-1", r#"{"metadata":{"uid":"p-1"}}"#),
        (
            "v1_spectroclusters_c-1_assets_kubeconfig",
            "apiVersion: v1\n",
        ),
        ("v1_dashboard_spectroclusters_meta", r#"{"items":[]}"#),
        (
            "v1_clusterprofiles_prof-2",
            r#"{"metadata":{"uid":"prof-2"},"spec":{"published":{"packs":[
                {"name":"kubernetes","tag":"1.36.2","type":"spectro","layer":"k8s","values":"a: 1\n"}]}}}"#,
        ),
        (
            "v1_spectroclusters_c-1_validate_repave",
            r#"{"isRepaveRequired":false,"reasons":[]}"#,
        ),
    ] {
        std::fs::write(routes.join(route), body).unwrap();
    }
    let calls: [(&str, &[&str]); 6] = [
        ("api.sh", &["get", "/v1/clusterprofiles/p-1"]),
        ("api.sh", &["get-status", "/v1/clusterprofiles/p-1"]),
        ("api.sh", &["get-kubeconfig", "c-1"]),
        ("api.sh", &["delete", "p-1"]),
        ("tenant.sh", &["listing"]),
        ("tenant.sh", &["repave", "c-1", "prof-1", "prof-2"]),
    ];
    for (script, args) in calls {
        let o = run_live(script, args, root.path(), &[("PALETTE_API_KEY", KEY)]);
        assert_eq!(
            o.status.code(),
            Some(0),
            "{script} {args:?}: {}",
            text(&o.stderr)
        );
    }
    let arguments = std::fs::read_to_string(routes.join("sent/curl.args")).unwrap();
    // Each call runs curl once. The repave reads a profile and asks Palette, so it runs curl twice.
    let curls = calls.len() + 1;
    assert_eq!(
        arguments.lines().count(),
        curls,
        "the calls of curl: {arguments}"
    );
    assert!(
        !arguments.contains(KEY),
        "the key is in an argument of curl: {arguments}"
    );
    assert!(
        !arguments.contains("ApiKey"),
        "the header is in an argument of curl: {arguments}"
    );
    for line in arguments.lines() {
        assert!(
            line.starts_with("--config - "),
            "a call reads its config from standard input: {line}"
        );
    }
    // The key reached each call through the config, so the stand-in saw the header.
    let config = std::fs::read_to_string(routes.join("sent/curl.config")).unwrap();
    let header = format!("header = \"ApiKey: {KEY}\"");
    assert_eq!(
        config.lines().filter(|l| *l == header).count(),
        curls,
        "{config}"
    );
    let request =
        std::fs::read_to_string(routes.join("sent/GET_v1_dashboard_spectroclusters_meta.request"))
            .unwrap();
    assert!(request.contains("ApiKey: (hidden)"), "{request}");
}

/// A key with a quote or a backslash reaches curl as the config reads it.
/// A key with a line break stops the script before any call, because it could add a line to the config.
#[test]
fn palette_curl_escapes_the_key_for_the_config_and_refuses_a_line_break() {
    let root = tempfile::tempdir().unwrap();
    let routes = root.path().join("routes");
    std::fs::create_dir_all(&routes).unwrap();
    std::fs::write(routes.join("v1_clusterprofiles_p-1"), "{}").unwrap();
    let o = run_live(
        "api.sh",
        &["get", "/v1/clusterprofiles/p-1"],
        root.path(),
        &[("PALETTE_API_KEY", "a\"b\\c")],
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let config = std::fs::read_to_string(routes.join("sent/curl.config")).unwrap();
    assert_eq!(config, "header = \"ApiKey: a\\\"b\\\\c\"\n");
    assert!(
        !std::fs::read_to_string(routes.join("sent/curl.args"))
            .unwrap()
            .contains("a\"b")
    );

    let again = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(again.path().join("routes")).unwrap();
    let o = run_live(
        "api.sh",
        &["get", "/v1/clusterprofiles/p-1"],
        again.path(),
        &[("PALETTE_API_KEY", "ab\nheader = \"X: y\"")],
    );
    assert_ne!(o.status.code(), Some(0));
    assert!(
        text(&o.stderr).contains("the key of Palette holds a line break"),
        "{}",
        text(&o.stderr)
    );
    assert!(
        !again.path().join("routes/sent/curl.config").exists(),
        "no call sent the key"
    );
}

/// Only `palette-curl.sh` names the header. A script that puts `ApiKey:` in a line of its own sends the key in
/// an argument again.
#[test]
fn only_palette_curl_sh_names_the_api_key_header_in_the_scripts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut named = Vec::new();
    for dir in ["examples", "scripts", "deploy", "crates"] {
        for path in walk(&root.join(dir)) {
            if path.extension().is_none_or(|e| e != "sh") {
                continue;
            }
            let code = std::fs::read_to_string(&path).unwrap();
            if code_lines(&code).any(|l| l.contains("ApiKey:")) {
                named.push(path.strip_prefix(root).unwrap().display().to_string());
            }
        }
    }
    assert_eq!(named, ["examples/live/palette-curl.sh"]);
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
fn api_sh_get_status_prints_the_status_and_the_body_of_each_answer_and_exits_0() {
    let root = tempfile::tempdir().unwrap();
    let routes = root.path().join("routes");
    std::fs::create_dir_all(&routes).unwrap();
    std::fs::write(routes.join("v1_x_found"), r#"{"items":[]}"#).unwrap();
    std::fs::write(
        routes.join("v1_x_refused"),
        r#"{"code":"ResourceAccessDenied","message":"access denied"}"#,
    )
    .unwrap();
    std::fs::write(routes.join("v1_x_refused.status"), "500").unwrap();

    let o = run_live("api.sh", &["get-status", "/v1/x/found"], root.path(), &[]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert_eq!(text(&o.stdout), "HTTP 200\n{\"items\":[]}");

    // a refusal is an answer to read: the status, then the body, and no error line
    let o = run_live("api.sh", &["get-status", "/v1/x/refused"], root.path(), &[]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert_eq!(
        text(&o.stdout),
        "HTTP 500\n{\"code\":\"ResourceAccessDenied\",\"message\":\"access denied\"}"
    );
    assert_eq!(text(&o.stderr), "");

    // `get` keeps its contract: exit 2 and the error line for the same refusal
    let o = run_live("api.sh", &["get", "/v1/x/refused"], root.path(), &[]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stdout));
    assert!(
        text(&o.stderr).contains("GET /v1/x/refused answered HTTP 500"),
        "{}",
        text(&o.stderr)
    );

    for args in [
        &["get-status"][..],
        &["get-status", "/v1/x/a", "/v1/x/b"][..],
    ] {
        let o = run_live("api.sh", args, root.path(), &[]);
        assert_eq!(o.status.code(), Some(2), "{args:?}");
        assert!(
            text(&o.stderr).contains("usage: api.sh get-status PATH"),
            "{}",
            text(&o.stderr)
        );
    }
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
    failed(&steps, &["release-with-key"]);

    // Palette refuses a call with no key: the run fails on the release
    let stage = Stage::new(baseline_outputs());
    release_route(&stage.path("routes"));
    std::fs::write(stage.path("routes/v1_mgmt_app_version.status"), "500").unwrap();
    let (o, steps) = stage.run("release.sh", &[]);
    assert_eq!(o.status.code(), Some(1), "{steps}");
    failed(&steps, &["release-no-key"]);

    let o = stage.run("release.sh", &["--help"]).0;
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stdout).contains("The release test (#146)"),
        "{}",
        text(&o.stdout)
    );
}

/// A `kubectl` that logs its arguments and the kubeconfig it was given, and answers only when
/// that kubeconfig is the one the fake Palette serves. With `$FAKE_KUBE_PUT`, it answers as a
/// cluster that runs the Deployment in the body of that PUT of a profile. The image field is the
/// first `image:` line, and the cluster has a pod. A body with no such line gives no Deployment.
/// With `$FAKE_KUBE_ORPHAN`, the Deployment stays after a body drops it, until `delete deployment`.
const FAKE_KUBECTL: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_KUBECTL_LOG"
grep -q 'fake-kubeconfig' "${KUBECONFIG:-/dev/null}" 2>/dev/null || { echo "no kubeconfig" >&2; exit 1; }
if [ -n "${FAKE_KUBE_PUT:-}" ]; then
  # The cluster runs the Deployment that the last PUT of the profile holds: the first image: line.
  image=$(jq -r '[.spec.template.packs[].manifests[].content] | join("\n") | capture("image: (?<i>[^\n]*)\n") | .i' "$FAKE_KUBE_PUT" 2>/dev/null)
  # With $FAKE_KUBE_ORPHAN, a Deployment that the content once named stays after the content drops it.
  # Palette leaves it so, until a delete. The marker files sit beside the path that the variable names.
  if [ -n "${FAKE_KUBE_ORPHAN:-}" ]; then
    [ -z "$image" ] || touch "$FAKE_KUBE_ORPHAN.seen"
    case "$*" in *"delete deployment"*) touch "$FAKE_KUBE_ORPHAN.deleted"; echo deleted; exit 0 ;; esac
    if [ -z "$image" ] && [ -f "$FAKE_KUBE_ORPHAN.seen" ] && [ ! -f "$FAKE_KUBE_ORPHAN.deleted" ]; then image=orphan; fi
  fi
  case "$*" in
    *"get pods"*) [ -z "$image" ] || echo "registry.k8s.io/pause@sha256:aaaa" ;;
    *"get deployment"*)
      [ -n "$image" ] || { echo "Error from server (NotFound): deployments.apps not found"; exit 1; }
      case "$*" in *jsonpath*) echo "$image" ;; esac ;;
  esac
  exit 0
fi
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
    c.output_locked().unwrap()
}

/// Every file under `dir`, as (path relative to dir, contents).
fn files_under(dir: &Path) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = walk(dir)
        .into_iter()
        .map(|p| {
            let rel = p.strip_prefix(dir).unwrap().display().to_string();
            (rel, std::fs::read_to_string(&p).unwrap())
        })
        .collect();
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

/// A `tofu` for baseline.sh. It gives outputs for cluster A `c-a` and four profile versions. Its
/// plan exits `$FAKE_PLAN_EXIT`. Its state manages r-1, r-2, g-1, and ad-1.
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
/// `$FAKE_COMPARE_EXIT`.
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
/// ids in `$FAKE_AWS/<operation>`, or nothing when that file is absent.
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
        .output_locked()
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
        .output_locked()
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
        .output_locked()
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
        .output_locked()
        .unwrap();
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("too many arguments"),
        "{}",
        text(&o.stderr)
    );
}

/// `record` of preflight-plan.sh keeps a result the ledger does not know out of the entry. The
/// ledger schema has caught, missed, passed, and false-fail. An entry with another word makes the
/// whole file unreadable.
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
        .output_locked()
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
/// sets, against an `api.sh` that answers every `get` with the file `answer`. Gives the output, and
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
        .output_locked()
        .unwrap();
    let asked = std::fs::read_to_string(dir.path().join("asked.txt")).unwrap_or_default();
    (o, asked.trim().to_string())
}

/// The fault of `instance-offering` in the plan of the lab (#143) comes from the instance type
/// route. It is the first type that has a zone in `nonSupportedZones`, for the region and the
/// account of the entity. The type comes from the route of the cloud of the engine. A type with
/// `null` has no such zone (#411).
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
            "get /v1/clouds/{account_kind}/regions/{region}/instancetypes?cloudAccountUid=acc-1&cpuGtEq=0&memoryGtEq=0"
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
            .output_locked()
            .unwrap();
        assert_eq!(
            text(&made.stdout).trim(),
            r#"{"spec":{"machinepoolconfig":[{"cloudConfig":{"instanceType":"t3.xlarge","azs":["us-east-2c"]}}]}}"#,
            "{engine}"
        );
    }
}

/// The check `instance-types-shape` of the plan tests the field `type`, as Palette names the type
/// of an entry (#404). It takes `nonSupportedZones` as an array or `null` (#411). The answer of
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
            .output_locked()
            .unwrap();
        assert_eq!(o.status.code(), status, "{answer}");
    }
}

/// The jq filter of the check `id` in the plan: the line after the line of `check id`, between the
/// single quotes.
fn plan_filter(id: &str) -> String {
    let script = std::fs::read_to_string(live_script("preflight-plan.sh")).unwrap();
    let line = script
        .lines()
        .skip_while(|l| !l.contains(&format!("check {id} ")))
        .nth(1)
        .unwrap_or_else(|| panic!("no check {id} in the plan"));
    line[line.find('\'').unwrap() + 1..line.rfind('\'').unwrap()].to_string()
}

/// The exit code of `jq -e filter` on `answer`.
fn jq_exit(filter: &str, answer: &str) -> Option<i32> {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("answer.json"), answer).unwrap();
    Command::new("jq")
        .args(["-e", filter])
        .arg(dir.path().join("answer.json"))
        .output_locked()
        .unwrap()
        .status
        .code()
}

/// The check `vpcs-route-keys` of the plan reads the exact keys of the VPC route (#393). The answer
/// of the route passes it. These answers fail it: a subnet without a zone, a subnet with a tag, a VPC
/// with another key, and an answer with no subnet.
#[test]
fn preflight_plan_reads_the_exact_keys_of_the_vpc_route() {
    let filter = plan_filter("vpcs-route-keys");
    let subnet = r#"{"subnetId":"subnet-1","az":"us-east-2a","isPrivate":true,"mapPublicIpOnLaunch":false,"name":"a"}"#;
    for (answer, status) in [
        (
            format!(
                r#"{{"vpcs":[{{"vpcId":"vpc-1","name":"a","cidrBlock":"10.0.0.0/16","subnets":[{subnet}]}}]}}"#
            ),
            Some(0),
        ),
        // a public subnet has no isPrivate, and the answer omits an empty name
        (
            r#"{"vpcs":[{"vpcId":"vpc-1","subnets":[{"subnetId":"subnet-1","az":"us-east-2a","mapPublicIpOnLaunch":true}]}]}"#.to_string(),
            Some(0),
        ),
        (
            r#"{"vpcs":[{"vpcId":"vpc-1","subnets":[{"subnetId":"subnet-1","isPrivate":true,"name":"a"}]}]}"#.to_string(),
            Some(1),
        ),
        (
            r#"{"vpcs":[{"vpcId":"vpc-1","subnets":[{"subnetId":"subnet-1","az":"us-east-2a","tags":[]}]}]}"#.to_string(),
            Some(1),
        ),
        (
            r#"{"vpcs":[{"vpcId":"vpc-1","subnets":[{"subnetId":"subnet-1","az":"us-east-2a","availableIpAddressCount":9}]}]}"#.to_string(),
            Some(1),
        ),
        (
            r#"{"vpcs":[{"subnets":[{"subnetId":"subnet-1","az":"us-east-2a"}]}]}"#.to_string(),
            Some(1),
        ),
        (
            r#"{"vpcs":[{"vpcId":"vpc-1","tags":{},"subnets":[{"subnetId":"subnet-1","az":"us-east-2a"}]}]}"#.to_string(),
            Some(1),
        ),
        (r#"{"vpcs":[{"vpcId":"vpc-1","subnets":[]}]}"#.to_string(), Some(1)),
        // the list is under `vpcs`, and not at any other depth
        (format!(r#"[{{"vpcId":"vpc-1","subnets":[{subnet}]}}]"#), Some(5)),
        ("{}".to_string(), Some(1)),
    ] {
        assert_eq!(jq_exit(&filter, &answer), status, "{answer}");
    }
}

/// The check `zones-route-keys` needs the name, the state, and the id of each zone, and no other
/// key. The route lists no zone type, and `eks-unsupported-zone` reads it.
#[test]
fn preflight_plan_reads_the_exact_keys_of_the_zone_route() {
    let filter = plan_filter("zones-route-keys");
    for (answer, status) in [
        (
            r#"{"zones":[{"name":"us-east-2a","state":"available","zoneId":"use2-az1"}]}"#,
            Some(0),
        ),
        (
            r#"{"zones":[{"name":"us-east-2a","state":"available","zoneId":"use2-az1","zoneType":"availability-zone"}]}"#,
            Some(1),
        ),
        (
            r#"{"zones":[{"name":"us-east-2a","zoneId":"use2-az1"}]}"#,
            Some(1),
        ),
        (
            r#"{"zones":[{"name":"us-east-2a","state":"available"}]}"#,
            Some(1),
        ),
        (r#"{"zones":[]}"#, Some(1)),
        // the list is under `zones`, and under no other name
        (
            r#"{"azs":[{"name":"us-east-2a","state":"available","zoneId":"use2-az1"}]}"#,
            Some(1),
        ),
    ] {
        assert_eq!(jq_exit(&filter, answer), status, "{answer}");
    }
}

/// The check `sgs-route-keys` needs the id and the name of each group. It allows `ownerId` and no
/// other key.
#[test]
fn preflight_plan_reads_the_exact_keys_of_the_security_group_route() {
    let filter = plan_filter("sgs-route-keys");
    for (answer, status) in [
        (
            r#"{"groups":[{"groupId":"sg-1","groupName":"default","ownerId":"123456789012"}]}"#,
            Some(0),
        ),
        (
            r#"{"groups":[{"groupId":"sg-1","groupName":"default"}]}"#,
            Some(0),
        ),
        (r#"{"groups":[{"groupName":"default"}]}"#, Some(1)),
        (r#"{"groups":[{"groupId":"sg-1"}]}"#, Some(1)),
        (
            r#"{"groups":[{"groupId":"sg-1","groupName":"default","tags":[]}]}"#,
            Some(1),
        ),
        (r#"{"groups":[]}"#, Some(1)),
    ] {
        assert_eq!(jq_exit(&filter, answer), status, "{answer}");
    }
}

/// The check `instancetypes-cpu` needs each pool type of the entity in the list, with `cpu` as a
/// number above 0. It also needs a type under 2 vCPUs or 4 GiB with `cpu` above 0. A type that the
/// list lacks, a type with no `cpu`, and a list with no small type fail it. The query of the rules
/// lifts the default filter of Palette, so the list holds the small types.
#[test]
fn preflight_plan_needs_the_cpu_of_each_pool_type_in_the_instance_type_route() {
    let filter = plan_filter("instancetypes-cpu");
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("clean.json"),
        r#"{"spec":{"machinepoolconfig":[{"cloudConfig":{"instanceType":"t3.xlarge"}},{"cloudConfig":{"instanceType":"m5.large"}}]}}"#,
    )
    .unwrap();
    for (answer, status) in [
        (
            r#"{"instanceTypes":[{"type":"t3.xlarge","cpu":4,"memory":16},{"type":"m5.large","cpu":2,"memory":8},{"type":"t3.small","cpu":2,"memory":2}]}"#,
            Some(0),
        ),
        // a type under 2 vCPUs is small too
        (
            r#"{"instanceTypes":[{"type":"t3.xlarge","cpu":4,"memory":16},{"type":"m5.large","cpu":2,"memory":8},{"type":"e2-micro","cpu":1,"memory":8}]}"#,
            Some(0),
        ),
        // the default filter of Palette: no type under 2 vCPUs or 4 GiB is in the list
        (
            r#"{"instanceTypes":[{"type":"t3.xlarge","cpu":4,"memory":16},{"type":"m5.large","cpu":2,"memory":8}]}"#,
            Some(1),
        ),
        // a pool type that the list lacks
        (
            r#"{"instanceTypes":[{"type":"t3.xlarge","cpu":4,"memory":16},{"type":"t3.small","cpu":2,"memory":2}]}"#,
            Some(1),
        ),
        // a small type with no cpu, or with 0
        (
            r#"{"instanceTypes":[{"type":"t3.xlarge","cpu":4,"memory":16},{"type":"m5.large","cpu":2,"memory":8},{"type":"t3.small","memory":2}]}"#,
            Some(1),
        ),
        (
            r#"{"instanceTypes":[{"type":"t3.xlarge","cpu":4,"memory":16},{"type":"m5.large","cpu":2,"memory":8},{"type":"t3.small","cpu":0,"memory":2}]}"#,
            Some(1),
        ),
        (
            r#"{"instanceTypes":[{"type":"t3.xlarge","cpu":4,"memory":16},{"type":"m5.large","memory":8},{"type":"t3.small","cpu":2,"memory":2}]}"#,
            Some(1),
        ),
        (
            r#"{"instanceTypes":[{"type":"t3.xlarge","cpu":4,"memory":16},{"type":"m5.large","cpu":"2","memory":8},{"type":"t3.small","cpu":2,"memory":2}]}"#,
            Some(1),
        ),
        (
            r#"{"instanceTypes":[{"type":"t3.xlarge","cpu":4,"memory":16},{"type":"m5.large","cpu":0,"memory":8},{"type":"t3.small","cpu":2,"memory":2}]}"#,
            Some(1),
        ),
    ] {
        std::fs::write(dir.path().join("instancetypes.json"), answer).unwrap();
        let out = Command::new("jq")
            .arg("-e")
            .arg("--slurpfile")
            .arg("e")
            .arg(dir.path().join("clean.json"))
            .arg(&filter)
            .arg(dir.path().join("instancetypes.json"))
            .output_locked()
            .unwrap();
        assert_eq!(out.status.code(), status, "{answer}");
    }
}

/// The check `instancetypes-default-filter` expects the answer of the route with no query to hold
/// no type under 2 vCPUs or 4 GiB. It expects the answer with the query `cpuGtEq=0&memoryGtEq=0`
/// to hold one, and to be longer. A route that ignores the filter, and a query that lifts nothing,
/// fail it.
#[test]
fn preflight_plan_shows_the_default_filter_of_the_instance_type_route() {
    let filter = plan_filter("instancetypes-default-filter");
    let list = |types: &[&str]| format!(r#"{{"instanceTypes":[{}]}}"#, types.join(","));
    let big = r#"{"type":"t3.xlarge","cpu":4,"memory":16}"#;
    let small_memory = r#"{"type":"t3.small","cpu":2,"memory":2}"#;
    let small_cpu = r#"{"type":"e2-micro","cpu":1,"memory":8}"#;
    let dir = tempfile::tempdir().unwrap();
    for (default, all, status) in [
        (list(&[big]), list(&[big, small_memory]), Some(0)),
        (list(&[big]), list(&[big, small_cpu]), Some(0)),
        // the route ignores the filter: the answer with no query holds a small type
        (
            list(&[big, small_memory]),
            list(&[big, small_memory]),
            Some(1),
        ),
        (list(&[big, small_cpu]), list(&[big, small_cpu]), Some(1)),
        // the query lifts nothing
        (list(&[big]), list(&[big]), Some(1)),
        // the answer with the query is not longer
        (list(&[big, big]), list(&[big, small_memory]), Some(1)),
        // an empty answer
        (list(&[]), list(&[big, small_memory]), Some(1)),
    ] {
        std::fs::write(dir.path().join("instancetypes-default.json"), &default).unwrap();
        std::fs::write(dir.path().join("instancetypes.json"), &all).unwrap();
        let out = Command::new("jq")
            .arg("-e")
            .arg("--slurpfile")
            .arg("all")
            .arg(dir.path().join("instancetypes.json"))
            .arg(&filter)
            .arg(dir.path().join("instancetypes-default.json"))
            .output_locked()
            .unwrap();
        assert_eq!(out.status.code(), status, "{default} against {all}");
    }
}

/// The five checks of the routes expect an exit code of 0. A key beside the known keys is a fault
/// of the run, and not a note.
#[test]
fn preflight_plan_judges_each_check_of_the_routes() {
    let script = std::fs::read_to_string(live_script("preflight-plan.sh")).unwrap();
    for id in [
        "instancetypes-cpu",
        "instancetypes-default-filter",
        "vpcs-route-keys",
        "zones-route-keys",
        "sgs-route-keys",
    ] {
        let line = script
            .lines()
            .find(|l| l.contains(&format!("check {id} ")))
            .unwrap_or_else(|| panic!("no check {id}"));
        assert!(line.contains(&format!("check {id} 0 ")), "{line}");
    }
}

/// The route checks of AWS run for the engines on AWS only, and the plan for GCP never calls them.
/// The instance type check runs for every engine.
#[test]
fn preflight_plan_asks_the_list_routes_of_aws_for_aws_and_eks_only() {
    let script = std::fs::read_to_string(live_script("preflight-plan.sh")).unwrap();
    let start = script
        .find("  aws|eks)\n    \"$live/api.sh\" get \"/v1/clouds/aws/regions/$region/keypairs")
        .expect("the case of aws and eks");
    let end = script[start..].find("\n  gcp)").expect("the case of gcp") + start;
    let aws = &script[start..end];
    for route in [
        "/vpcs?cloudAccountUid=",
        "/availabilityzones?cloudAccountUid=",
        "/securitygroups?cloudAccountUid=",
    ] {
        assert!(aws.contains(route), "{route}");
        assert!(!script[end..].contains(route), "{route} is asked for GCP");
    }
    assert!(
        !script[start..].contains("vpcs-route-shape") && !script.contains("zones-route-shape"),
        "the checks of the shape at any depth are gone"
    );
    assert!(
        script[..start].contains("check instancetypes-cpu "),
        "every engine runs the instance type check"
    );
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
            .output_locked()
            .unwrap();
        assert!(o.status.success(), "{engine}: {}", text(&o.stderr));
        assert_eq!(text(&o.stdout), format!("{kind} {account}\n"), "{engine}");
        let body = read_json(&dir.path().join("body.json"));
        assert_eq!(body["spec"]["cloudAccountUid"], account, "{engine}");
    }
}

/// The plan puts the fault of the key pair in the check `iaas-pod-identity` on aws, and in
/// `ssh-key-pair` on eks alone (#430). On aws, the property route refuses a key pair that the
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
    // the check passes on a report that names the code of Palette in a failed rule. An error is no
    // catch: the rule reads the HTTP 500 through `refusal_codes` (#546)
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
            Some(1),
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
            .output_locked()
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

/// A `tofu` for a stage: `output -json` prints `$FAKE_OUTPUTS`, a plan changes nothing, and each
/// other command passes. It logs each command to `$FAKE_TOFU_LOG`.
const FAKE_TOFU_STAGE: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_TOFU_LOG"
case "$*" in
  *"output -json"*) cat "$FAKE_OUTPUTS" ;;
  *"show -json"*) echo '{}' ;;
  *plan*) echo "No changes." ;;
  *apply*) echo "Apply complete! Resources: 0 added, 1 changed, 0 destroyed." ;;
esac
"#;

/// A `strata` for a stage. It logs its arguments to `$FAKE_STRATA/calls` and `XDG_STATE_HOME` to
/// `$FAKE_STRATA/state`. It answers from `$FAKE_STRATA/rules`. In that file, the first line
/// `GLOB<TAB>EXIT<TAB>FILE` whose GLOB matches the arguments prints FILE, or writes it to the file
/// of `-o`, and exits EXIT. A fourth field `err` prints FILE to standard error and writes no file.
/// With no match it exits 0.
const FAKE_STRATA_STAGE: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_STRATA/calls"
echo "${XDG_STATE_HOME-}" >> "$FAKE_STRATA/state"
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

/// One run of a stage of a live script against the stand-ins. A test gives the OpenTofu outputs,
/// the answers of strata, and the routes of Palette.
///
/// The stage puts two stand-ins on the `PATH`: a `sleep` that returns at once, and
/// [`FAKE_PORT_FORWARD`]. So a wait that counts its looks ends in less than a second. A wait for a
/// process that runs in the background limits its time, as `pf_start` of `lib.sh` does.
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
                ("fake-port-forward", FAKE_PORT_FORWARD),
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

    /// Palette answers `path` with `status` and `body` to a request whose `ApiKey` is `key`.
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
        let key = Self::route_key(&profile_route(uid));
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
        self.route(&manifests_route(uid), manifests);
        self.route(&format!("/v1/clusterprofiles/{uid}/publish"), "{}");
    }

    /// `body` is the stand-in for the command `name`, in place of the one that the stage has.
    fn command(&self, name: &str, body: &str) {
        executable(&self.path("bin").join(name), body);
    }

    /// `wrapper` is the stand-in for strata. A call that it does not answer goes to the answers
    /// that `strata` gave.
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
        // the harness shows the standard output of a test that fails, so its log holds the output
        // of each step that failed
        print!("{}", failed_outputs(&root.join("out"), &steps));
        (o, steps)
    }

    /// Each body of `METHOD path` that the script sent, in order. The list is empty when the
    /// script sent none to that route. This holds only after the script sent a request to some
    /// route. The stand-in makes the directory `routes/sent` at its first request.
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

/// The output and the errors of each check that failed in `steps`, a `steps.log`. They are the
/// files `<id>.out` and `<id>.err` that `check` of `lib.sh` writes in `out`.
fn failed_outputs(out: &Path, steps: &str) -> String {
    let mut shown = String::new();
    for id in ids_with(steps, &["FAIL"]) {
        for file in [format!("{id}.out"), format!("{id}.err")] {
            let body = std::fs::read_to_string(out.join(&file)).unwrap_or_default();
            shown.push_str(&format!("---- the failed check {id}: {file}\n{body}"));
            if !body.is_empty() && !body.ends_with('\n') {
                shown.push('\n');
            }
        }
    }
    shown
}

#[test]
fn the_log_of_a_test_shows_the_output_and_the_errors_of_each_check_that_failed() {
    let out = tempfile::tempdir().unwrap();
    for (file, body) in [
        ("alert-route.out", "the port-forward ended"),
        ("alert-route.err", "error: lost connection to pod\n"),
        ("alert-edit.out", "the output of a check that passed\n"),
    ] {
        std::fs::write(out.path().join(file), body).unwrap();
    }
    let steps = "== stage 8: the alerts\n\
        FAIL  alert-route                      expected 0, got 1\n\
        PASS  alert-edit                       expected 0, got 0\n\
        FAIL  alert-open                       expected 0, got 1\n";
    assert_eq!(
        failed_outputs(out.path(), steps),
        "---- the failed check alert-route: alert-route.out\n\
         the port-forward ended\n\
         ---- the failed check alert-route: alert-route.err\n\
         error: lost connection to pod\n\
         ---- the failed check alert-open: alert-open.out\n\
         ---- the failed check alert-open: alert-open.err\n"
    );
    assert_eq!(
        failed_outputs(out.path(), "PASS  alert-edit  expected 0, got 0\n"),
        ""
    );
}

/// The words that `check` of `lib.sh` writes at the start of the line of a check in `steps.log`.
const VERDICTS: [&str; 3] = ["PASS", "FAIL", "SEEN"];

/// The verdict and the id of `line` of a `steps.log`, or `None` when it is not the line of a check.
fn verdict_and_id(line: &str) -> Option<(&str, &str)> {
    let mut words = line.split_whitespace();
    let verdict = words.next().filter(|w| VERDICTS.contains(w))?;
    Some((verdict, words.next()?))
}

/// True when `steps` has a line of the check `id` with `verdict`, one of [`VERDICTS`].
fn has_verdict(steps: &str, verdict: &str, id: &str) -> bool {
    ids_with(steps, &[verdict]).any(|i| i == id)
}

/// The id of each check in `steps` with one of `verdicts`, in the order of the lines.
fn ids_with<'a>(steps: &'a str, verdicts: &'a [&str]) -> impl Iterator<Item = &'a str> {
    steps
        .lines()
        .filter_map(verdict_and_id)
        .filter(move |(verdict, _)| verdicts.contains(verdict))
        .map(|(_, id)| id)
}

/// The ids of the checks in `steps`, in the order that they ran.
fn check_ids(steps: &str) -> Vec<&str> {
    ids_with(steps, &VERDICTS).collect()
}

/// The line of `steps` for the check `id`.
fn step_line<'a>(steps: &'a str, id: &str) -> &'a str {
    steps
        .lines()
        .find(|l| verdict_and_id(l).is_some_and(|(_, i)| i == id))
        .unwrap_or_else(|| panic!("no line for {id}:\n{steps}"))
}

/// Each check of `ids` passed.
fn passed(steps: &str, ids: &[&str]) {
    for id in ids {
        assert!(has_verdict(steps, "PASS", id), "{id} passed:\n{steps}");
    }
}

/// Each check of `ids` failed.
fn failed(steps: &str, ids: &[&str]) {
    for id in ids {
        assert!(has_verdict(steps, "FAIL", id), "{id} failed:\n{steps}");
    }
}

#[test]
fn a_verdict_line_matches_its_verdict_and_the_whole_id_of_the_check() {
    let steps = "PASS  a-b      expected 0, got 0\nFAIL  a        expected 0, got 1\n";
    assert!(has_verdict(steps, "PASS", "a-b"));
    assert!(has_verdict(steps, "FAIL", "a"));
    assert!(!has_verdict(steps, "PASS", "a"));
    assert!(!has_verdict(steps, "FAIL", "a-b"));
    assert!(!has_verdict(steps, "FAIL", "a-"));
    // the heading of a stage is not the line of a check, also when its second word is an id
    let steps = "== stage 1: the checks\n\
        PASS  a-b      expected 0, got 0\n\
        SEEN  stage    expected ?, got 2\n";
    assert_eq!(check_ids(steps), ["a-b", "stage"]);
    assert_eq!(
        step_line(steps, "stage"),
        "SEEN  stage    expected ?, got 2"
    );
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
        &cluster_route("c-a"),
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
        &profile_route("r-1"),
        &format!(
            r#"{{"spec":{{"published":{{"packs":[{{"name":"ubuntu-aws","layer":"os",
                "packUid":"os-1","annotations":{annotations}}}]}}}}}}"#
        ),
    );
    stage.route(
        &pack_route("os-1"),
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
    failed(&steps, &["os-annotations"]);
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
        !steps.contains("== stage 7"),
        "stage 7 writes a pool, so it runs only when asked: {steps}"
    );
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
    passed(&steps, &["rbac-before", "rbac-after"]);
    // no route answers the PUT here
    failed(&steps, &["rbac-add"]);
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
    for seen in ["limited-code", "limited-record", "limited-record-errors"] {
        assert!(has_verdict(&steps, "SEEN", seen), "{seen}: {steps}");
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
    failed(
        &steps,
        &["limited-status", "limited-code-says", "limited-message"],
    );
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

/// A `strata` for tiers.sh stage 7. strata-preflight writes the create body of cluster A into the
/// directory of `--out` and exits 1, as it does when a rule fails. Any other call goes to the
/// answers of the stage.
const FAKE_STRATA_POOL: &str = r#"#!/usr/bin/env bash
if [ "$1" = preflight ]; then
  echo "$*" >> "$FAKE_STRATA/calls"
  dir=""; prev=""
  for a in "$@"; do [ "$prev" = --out ] && dir=$a; prev=$a; done
  mkdir -p "$dir" && cp "$FAKE_STRATA/entity.json" "$dir/entity.json"
  exit 1
fi
exec "$(dirname "$0")/fake-strata-rules" "$@"
"#;

/// The text that tiers.sh stage 7 sets as both overrides of the worker pool.
const POOL_OVERRIDE: &str = "# strata tiers.sh stage 7: this text sets no value\n";

/// The worker pool of cluster A as the create body holds it, and as strata-preflight writes it.
fn pool_entity() -> serde_json::Value {
    serde_json::json!({
        "cloudConfig": {"instanceType": "t3.xlarge", "azs": ["us-east-2a"]},
        "poolConfig": {"name": "worker-pool", "size": 2, "minSize": 2, "maxSize": 2,
            "isControlPlane": false, "labels": ["worker"]}})
}

/// A stage of tiers.sh stage 7. Cluster A has the cloud config `cc-1`, whose worker pool takes
/// the `poolConfig` of each PUT when `takes` is true, and ignores the PUT otherwise.
fn pool_stage(takes: bool) -> Stage {
    let mut stage = baseline_stage();
    stage.wrap_strata(FAKE_STRATA_POOL);
    stage.route(
        &cluster_route("c-a"),
        r#"{"metadata":{"name":"fred"},"spec":{"cloudType":"aws","cloudConfigRef":{"kind":"aws","uid":"cc-1"}},
            "status":{"state":"Running"}}"#,
    );
    stage.route("/v1/cloudconfigs/aws/cc-1/machinePools/worker-pool", "{}");
    let routes = stage.path("routes");
    std::fs::write(
        routes.join("base"),
        r#"{"spec":{"clusterConfig":{"region":"us-east-2"},"machinePoolConfig":[
            {"name":"cp-pool","isControlPlane":true,"size":1},
            {"name":"worker-pool","isControlPlane":false,"size":2,"instanceType":"t3.xlarge"}]}}"#,
    )
    .unwrap();
    let applied = if takes {
        r#"jq --slurpfile e "$sent" '(.spec.machinePoolConfig[] | select(.name == "worker-pool")) |= (. + $e[0].poolConfig)' "$FAKE_ROUTES/base""#
    } else {
        r#"cat "$FAKE_ROUTES/base""#
    };
    executable(
        &routes.join("v1_cloudconfigs_aws_cc-1.sh"),
        &format!(
            r#"#!/usr/bin/env bash
sent="$FAKE_ROUTES/sent/PUT_v1_cloudconfigs_aws_cc-1_machinePools_worker-pool"
if [ -f "$sent" ]; then {applied}; else cat "$FAKE_ROUTES/base"; fi
"#
        ),
    );
    std::fs::write(
        stage.path("strata/entity.json"),
        serde_json::json!({"spec": {"machinepoolconfig": [
            {"cloudConfig": {"instanceType": "t3.xlarge"}, "poolConfig": {"name": "cp-pool", "size": 1}},
            pool_entity()]}})
        .to_string(),
    )
    .unwrap();
    let record = |worker: serde_json::Value| {
        serde_json::json!({"source": {"cluster": {"tiers": {"shape": {"fields": {
            "cloudType": "aws", "cloud": {"region": "us-east-2"},
            "pools": [{"name": "cp-pool", "size": 1}, worker]}}}}}})
        .to_string()
    };
    stage.strata(
        "snapshot cluster:c-a --shape -o pool-before.json",
        0,
        &record(serde_json::json!({"name": "worker-pool", "size": 2})),
    );
    stage.strata(
        "snapshot cluster:c-a --shape -o pool-after.json",
        0,
        &record(serde_json::json!({"name": "worker-pool", "size": 2,
            "overrideClusterAPIConfig": POOL_OVERRIDE,
            "overrideHealthCheckConfiguration": POOL_OVERRIDE})),
    );
    let added = |field: &str| serde_json::json!({"op": "added", "path": format!("pools[name=worker-pool].{field}")});
    stage.strata(
        "compare pool-before.json pool-after.json --json",
        0,
        &serde_json::json!({"clusterTiers": [{"tier": "shape", "same": false, "diff": [
            added("overrideClusterAPIConfig"), added("overrideHealthCheckConfiguration")]}]})
        .to_string(),
    );
    stage.strata(
        "compare pool-before.json pool-after.json",
        0,
        "  cluster tiers applied unchanged, shape CHANGED\n        + shape.pools[name=worker-pool].overrideHealthCheckConfiguration: \"#\"\n",
    );
    stage.strata(
        "verify pool-before.json",
        0,
        "  cluster tiers applied unchanged, shape CHANGED\nOK: cluster fred\n",
    );
    stage.strata(
        "compare pool-before.json cluster:c-a",
        0,
        "  cluster tiers unchanged\nOK: IDENTICAL\n",
    );
    stage
}

#[test]
fn tiers_sh_stage_7_sets_both_overrides_on_the_worker_pool_records_them_and_puts_the_pool_back() {
    let stage = pool_stage(true);
    let (o, steps) = stage.run("tiers.sh", &["7"]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    assert!(!steps.contains("FAIL"), "{steps}");
    passed(
        &steps,
        &[
            "pool-before-none",
            "pool-entity",
            "pool-body",
            "pool-set",
            "pool-in-cloud-config",
            "pool-in-record",
            "pool-compare",
            "pool-compare-shape",
            "pool-compare-says",
            "pool-verify",
            "pool-verify-says",
            "pool-undo",
            "pool-undone",
            "pool-restored",
            "pool-restored-says",
        ],
    );
    assert!(
        has_verdict(&steps, "SEEN", "pool-world")
            && has_verdict(&steps, "SEEN", "pool-cluster-level"),
        "the two checks that record an answer judge none: {steps}"
    );
    assert!(!steps.contains("== stage 6"), "stage 7 runs alone: {steps}");

    let mut set = pool_entity();
    set["poolConfig"]["overrideClusterAPIConfig"] = POOL_OVERRIDE.into();
    set["poolConfig"]["overrideHealthCheckConfiguration"] = POOL_OVERRIDE.into();
    assert_eq!(
        stage.sent("PUT", "/v1/cloudconfigs/aws/cc-1/machinePools/worker-pool"),
        vec![set, pool_entity()],
        "the PUTs set both overrides in poolConfig and then send the pool as it was"
    );
    let calls = std::fs::read_to_string(stage.path("strata/calls")).unwrap();
    assert!(
        calls.contains("preflight cluster:c-a --tier static"),
        "the entity comes from the static tier, with no probe: {calls}"
    );
    let pool_cloud_config = std::fs::read_to_string(stage.path("out/pool-read.out")).unwrap();
    assert!(
        pool_cloud_config.contains("overrideHealthCheckConfiguration"),
        "{pool_cloud_config}"
    );
}

#[test]
fn tiers_sh_stage_7_fails_the_checks_of_the_override_when_palette_drops_it() {
    let stage = pool_stage(false);
    let (o, steps) = stage.run("tiers.sh", &["7"]);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    failed(&steps, &["pool-in-cloud-config"]);
    // the record is a canned answer here, so only the check of the cloud config sees the drop
    passed(&steps, &["pool-set", "pool-undone"]);
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
    stage.route(&app_profile_route("ap-1"), "{}");
    stage.route(&app_profile_route("ap-2"), "{}");
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
        &cluster_route("c-a"),
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

    // a list that omits the virtual cluster, as before #116, fails the check
    let mut stage = cluster_group_stage_1();
    stage.strata(
        "list clusters --json",
        0,
        r#"{"includesVirtual":false,"clusters":[{"name":"fred","uid":"c-a"}]}"#,
    );
    let (_, steps) = stage.run("cluster-group.sh", &["1"]);
    passed(&steps, &["member-listed"]);
    // the check sees a list with no virtual cluster
    failed(&steps, &["member-listed-group"]);
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
    // the stand-in still gives a changed tier after the undo, and the check sees it
    failed(&steps, &["limit-undone"]);
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

/// An `oras` that logs its arguments to `$FAKE_AWS/oras.log`, and prints a digest for a push.
const FAKE_ORAS: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_AWS/oras.log"
[ "$1" = push ] && echo "Digest: sha256:$(printf '%s' "$*" | sha256sum | cut -c1-64)"
exit 0
"#;

/// A stage of `registry.sh`: the outputs of the example's own registry and pack, and `more`. Each
/// sync ends at once. The wait for the clear of the flag ends at once. The profile that pins the
/// pack under a floating tag moves at once.
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
        &profile_route("own-1"),
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
    for seen in ["del-flag", "del-doc-fields", "del-verify", "del-cleared"] {
        assert!(has_verdict(&steps, "SEEN", seen), "{seen}: {steps}");
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
    let pack = PackFixture::new(
        "strata-live-pack",
        "addon",
        "1.0.1",
        "pk-101",
        "reg-1",
        "",
        "",
    );
    stage.route(
        "/v1/packs",
        &serde_json::json!({"items": [pack.summary(&[])]}).to_string(),
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
    failed(&steps, &["packs-paged-listing"]);
    failed(&steps, &["packs-paged-count"]);
}

/// A run of packs.sh whose `strata packs <name> --registry ... --json` prints `kubernetes` for
/// kubernetes and `cert_manager` for cert-manager. Gives steps.log.
fn packs_newest(kubernetes: &str, cert_manager: &str) -> String {
    let mut stage = Stage::new(serde_json::json!({
        "csi_pack_name": "csi-aws-ebs", "csi_previous_version": "1.63.1",
        "csi_version": "1.65.0", "cloud": "aws",
    }));
    stage.strata("packs kubernetes --registry * --json", 0, kubernetes);
    stage.strata("packs cert-manager --registry * --json", 0, cert_manager);
    let (_, steps) = stage.run("packs.sh", &[]);
    steps
}

#[test]
fn packs_sh_checks_that_the_newest_version_of_a_pack_is_the_version_that_palette_marks() {
    let marked = r#"{"registries":[{"versions":[
        {"version":"1.19.3","latest":true,"paletteLatest":true},
        {"version":"1.19.3-rev1","latest":false},
        {"version":"1.19.2","latest":false}]}]}"#;
    let steps = packs_newest(marked, marked);
    passed(&steps, &["packs-palette-latest"]);

    // the rebuild is the newest for strata, and Palette marks its base: the order of strata
    // differs from the order of Palette
    let rebuild_first = r#"{"registries":[{"versions":[
        {"version":"1.19.3-rev1","latest":true},
        {"version":"1.19.3","latest":false,"paletteLatest":true}]}]}"#;
    let steps = packs_newest(marked, rebuild_first);
    failed(&steps, &["packs-palette-latest"]);
    let steps = packs_newest(rebuild_first, marked);
    failed(&steps, &["packs-palette-latest"]);

    // Palette marks no version
    let unmarked = r#"{"registries":[{"versions":[{"version":"1.19.3","latest":true}]}]}"#;
    let steps = packs_newest(marked, unmarked);
    failed(&steps, &["packs-palette-latest"]);
}

/// A run of packs.sh whose Helm summary listing gives the Bitnami registry the sync `sync`. Its
/// `strata behind` on the record of the newest nginx of Bitnami prints `line` and `json`.
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
    failed(&steps, &["stale-note"]);
}

#[test]
fn validate_packs_sh_stage_4_makes_a_profile_whose_pack_has_another_layer_than_its_registry() {
    let mut stage = Stage::new(serde_json::json!({}));
    stage.route(
        "/v1/clusterprofiles/validate/packs",
        r#"{"packs":{"results":[]}}"#,
    );
    let pack = PackFixture::new("cert-manager", "addon", "1.21.1", "pk-1", "reg-pub", "", "");
    stage.route(
        "/v1/packs",
        &serde_json::json!({"items": [pack.summary(&[])]}).to_string(),
    );
    stage.route(PROFILE_LISTING, r#"{"items":[]}"#);
    stage.route("/v1/clusterprofiles", r#"{"uid":"lp-1"}"#);
    stage.route("/v1/clusterprofiles/lp-1/publish", "{}");
    stage.route(
        &profile_route("lp-1"),
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
        &cluster_route("c-a"),
        r#"{"metadata":{"name":"fred"},"status":{"state":"Running"}}"#,
    );
    stage.route(
        &profile_route("n-1"),
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
    for seen in ["var-strata", "var-strata-answer", "var-none", "var-default"] {
        assert!(has_verdict(&steps, "SEEN", seen), "{seen}: {steps}");
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

/// The forward of the console address of the VM Orchestrator pack in stage 9 of `repave.sh`. The
/// record answers 502 after a question whose body holds the mark: Palette saved the mark, and it
/// has no host. It answers 200 before such a question, and after a question with the values of the
/// cluster. The forward keeps the status that it read for `FORWARD_KEEPS` more reads, as the
/// forward of Palette keeps the address that it read.
const FAKE_CONSOLE_FORWARD: &str = r#"#!/usr/bin/env bash
sent="$FAKE_ROUTES/sent/POST_v1_spectroclusters_v-1_validate_repave"
status=200
[ -f "$sent" ] && grep -q 'consoleBaseAddress: https://strata-live-check.invalid' "$sent" && status=502
kept="$FAKE_ROUTES/forward.kept"
if [ -s "$kept" ] && read -r was left < "$kept" && [ "$left" -gt 0 ]; then
  status=$was left=$((left - 1))
else
  left=${FORWARD_KEEPS:-0}
fi
echo "$status $left" > "$kept"
echo "$status" > "$FAKE_ROUTES/v1_tenantApps_f-1_.status"
echo console
"#;

/// A cluster for stage 9 of `repave.sh`, with three profiles. One has no pack of virtual machines.
/// One has `cert-manager` and `virtual-machine-orchestrator`, whose values hold
/// `consoleBaseAddress`. One has `vm-migration-assistant`, whose console address Palette saves too.
const VMO_CLUSTER: &str = r#"{"spec":{"clusterProfileTemplates":[
    {"uid":"p-k8s","packs":[{"name":"kubernetes","tag":"1.36.2","type":"spectro","values":"a: 1\n"}]},
    {"uid":"p-vmo","packs":[
        {"name":"virtual-machine-orchestrator","tag":"4.6.0","type":"spectro",
         "values":"charts:\n  virtual-machine-orchestrator:\n    appConfig:\n      clusterInfo:\n        consoleBaseAddress: \"\"\n"},
        {"name":"cert-manager","tag":"1.17.0","type":"oci","values":"b: 2\n"}]},
    {"uid":"p-vma","packs":[{"name":"vm-migration-assistant","tag":"1.2.0","type":"spectro","values":"c: 3\n"}]}]}}"#;

/// A stage for stage 9 of `repave.sh` on the cluster `v-1`, with the document `cluster`. The
/// summary gives the console address, and `FAKE_CONSOLE_FORWARD` answers at it.
fn vmo_stage(cluster: &str) -> Stage {
    let mut stage = Stage::new(baseline_outputs());
    stage.env("VMO_CLUSTER", "v-1");
    stage.env("VMO_WAIT", "0");
    stage.route(&cluster_route("v-1"), cluster);
    stage.route(
        "/v1/dashboard/spectroclusters/v-1",
        r#"{"status":{"addOnServices":[{"name":"vm-dashboard","endpoint":"https://console.example/v1/tenantApps/f-1"}]}}"#,
    );
    stage.route(
        "/v1/spectroclusters/v-1/validate/repave",
        r#"{"isRepaveRequired":false,"reasons":[]}"#,
    );
    executable(
        &stage.path("routes/v1_tenantApps_f-1_.sh"),
        FAKE_CONSOLE_FORWARD,
    );
    stage
}

/// Each read of the console address that the stage sent, as the arguments of `curl`.
fn console_reads(stage: &Stage) -> Vec<String> {
    std::fs::read_to_string(stage.path("routes/sent/curl.args"))
        .unwrap()
        .lines()
        .filter(|l| l.contains("tenantApps"))
        .map(str::to_string)
        .collect()
}

#[test]
fn repave_sh_stage_9_reads_the_console_address_before_and_after_a_question_with_a_mark() {
    let stage = Stage::new(baseline_outputs());
    let (o, steps) = stage.run("repave.sh", &["9"]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    assert!(
        steps.contains("== stage 9 skipped: give VMO_CLUSTER="),
        "no cluster, no question: {steps}"
    );

    let stage = vmo_stage(VMO_CLUSTER);
    let (o, steps) = stage.run("repave.sh", &["9"]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    passed(
        &steps,
        &[
            "vmo-cluster",
            "vmo-runs",
            "vmo-summary",
            "vmo-mark-body",
            "vmo-before",
            "vmo-ask",
            "vmo-ask-ok",
            "vmo-saved",
            "vmo-own",
            "vmo-own-ok",
            "vmo-back",
        ],
    );
    let sent = stage.sent("POST", "/v1/spectroclusters/v-1/validate/repave");
    assert_eq!(
        sent[0],
        serde_json::json!({"profiles": [
            {"uid": "p-k8s"},
            {"uid": "p-vmo", "packValues": [
                {"name": "virtual-machine-orchestrator", "tag": "4.6.0", "type": "spectro",
                 "values": "charts:\n  virtual-machine-orchestrator:\n    appConfig:\n      clusterInfo:\n        consoleBaseAddress: https://strata-live-check.invalid\n"},
                {"name": "cert-manager", "tag": "1.17.0", "type": "oci", "values": "b: 2\n"},
            ]},
            {"uid": "p-vma", "packValues": [
                {"name": "vm-migration-assistant", "tag": "1.2.0", "type": "spectro", "values": "c: 3\n"},
            ]},
        ]}),
        "each profile with a pack of virtual machines names each of its packs, so the target keeps \
         them, and the console of vm-migration-assistant does not move"
    );
    assert_eq!(
        sent[1],
        serde_json::json!({"profiles": [
            {"uid": "p-k8s"},
            {"uid": "p-vmo", "packValues": [
                {"name": "virtual-machine-orchestrator", "tag": "4.6.0", "type": "spectro",
                 "values": "charts:\n  virtual-machine-orchestrator:\n    appConfig:\n      clusterInfo:\n        consoleBaseAddress: \"\"\n"},
                {"name": "cert-manager", "tag": "1.17.0", "type": "oci", "values": "b: 2\n"},
            ]},
            {"uid": "p-vma", "packValues": [
                {"name": "vm-migration-assistant", "tag": "1.2.0", "type": "spectro", "values": "c: 3\n"},
            ]},
        ]}),
        "the second question sends the values of the cluster, and not the profile by its uid alone"
    );
    assert_eq!(sent.len(), 2);
    let forwards = console_reads(&stage);
    assert_eq!(
        forwards.len(),
        3,
        "a read before and after each question: {forwards:?}"
    );
    assert!(
        forwards.iter().all(|l| !l.contains("--config")),
        "a read of the console sends no key: {forwards:?}"
    );
}

/// The forward of Palette keeps the address that it read, usually for at most 15 minutes, and in
/// some cases until Palette starts it again. So stage 9 reads the console again after each
/// question, for at most `VMO_WAIT` seconds. When no read passes, it names the forward as a cause.
#[test]
fn repave_sh_stage_9_reads_the_console_again_while_the_forward_keeps_the_address_that_it_read() {
    let mut stage = vmo_stage(VMO_CLUSTER);
    stage.env("VMO_WAIT", "10");
    stage.env("FORWARD_KEEPS", "2");
    let (o, steps) = stage.run("repave.sh", &["9"]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    passed(&steps, &["vmo-before", "vmo-saved", "vmo-back"]);
    let saved = std::fs::read_to_string(stage.path("out/vmo-saved.out")).unwrap();
    assert_eq!(
        saved, "200\n200\n502\n",
        "the forward keeps the host for two reads"
    );
    let back = std::fs::read_to_string(stage.path("out/vmo-back.out")).unwrap();
    assert_eq!(
        back, "502\n502\n200\n",
        "the forward keeps the mark for two reads"
    );
    assert_eq!(console_reads(&stage).len(), 7);
    assert!(!steps.contains("The forward of Palette"), "{steps}");

    let mut stage = vmo_stage(VMO_CLUSTER);
    stage.env("VMO_WAIT", "10");
    stage.env("FORWARD_KEEPS", "99");
    let (o, steps) = stage.run("repave.sh", &["9"]);
    assert_ne!(o.status.code(), Some(0), "{steps}");
    failed(&steps, &["vmo-saved"]);
    assert!(
        steps.contains(
            "== vmo-saved: no read in 10 seconds reached the mark. The forward of Palette can keep \
             the address that it read before the question. Then the console forwards there until \
             Palette starts the forward service again"
        ),
        "{steps}"
    );
    let saved = std::fs::read_to_string(stage.path("out/vmo-saved.out")).unwrap();
    assert_eq!(saved, "200\n200\n200\n", "three reads in 10 seconds");
}

/// Stage 9 of `repave.sh` asks Palette nothing when a check before the first question fails.
/// Palette saves no record for a cluster that a cluster template governs. A body for a cluster
/// with no `virtual-machine-orchestrator` holds no mark, so the question shows nothing. A console
/// that reaches no host before the question shows nothing after it.
#[test]
fn repave_sh_stage_9_stops_before_the_first_question_when_a_check_before_it_fails() {
    let template = VMO_CLUSTER.replacen(
        r#"{"spec":{"#,
        r#"{"spec":{"clusterTemplate":{"uid":"t-1"},"#,
        1,
    );
    let dashboard = VMO_CLUSTER.replace(
        r#""name":"virtual-machine-orchestrator""#,
        r#""name":"spectro-vm-dashboard""#,
    );
    for (cluster, forward, fails, reads) in [
        (template.as_str(), FAKE_CONSOLE_FORWARD, "vmo-runs", 0),
        (dashboard.as_str(), FAKE_CONSOLE_FORWARD, "vmo-runs", 0),
        (
            VMO_CLUSTER,
            "#!/usr/bin/env bash\necho 404 > \"$FAKE_ROUTES/v1_tenantApps_f-1_.status\"\necho '{\"message\":\"foreq not found for hash\"}'\n",
            "vmo-before",
            1,
        ),
    ] {
        let stage = vmo_stage(cluster);
        executable(&stage.path("routes/v1_tenantApps_f-1_.sh"), forward);
        let (o, steps) = stage.run("repave.sh", &["9"]);
        assert_ne!(o.status.code(), Some(0), "{steps}");
        failed(&steps, &[fails]);
        assert!(
            steps.contains(
                "== stage 9 stopped: a check failed before the first question, so the stage asks \
                 Palette nothing"
            ),
            "{steps}"
        );
        assert!(
            stage
                .sent("POST", "/v1/spectroclusters/v-1/validate/repave")
                .is_empty(),
            "{fails}: no question"
        );
        assert_eq!(console_reads(&stage).len(), reads, "{fails}: {steps}");
        assert!(!steps.contains("vmo-ask"), "{steps}");
    }
}

/// A `strata` for pending.sh. An update waits for cluster A while the log of the stand-in curl
/// holds an odd number of PUTs of a profile. An edit in place opens an update. The edit that
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
        .sent("PUT", &profile_route("r-1"))
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

/// A `tofu` for template.sh. `output -json` prints `$FAKE_OUTPUTS`. An apply with `cluster=false`
/// copies `$FAKE_OUTPUTS.none` over it, and each other apply copies `$FAKE_OUTPUTS.cluster` over
/// it.
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
        &cluster_route("c-t"),
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
        .sent("PUT", &profile_route("n-1"))
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

/// The text of the declaration of the variable `name` in `variables.tf`, after its opening brace.
fn variable_declaration(name: &str) -> String {
    terraform_file("variables.tf")
        .split(&format!("variable \"{name}\" {{"))
        .nth(1)
        .unwrap_or_else(|| panic!("variables.tf declares {name}"))
        .split("\n}\n")
        .next()
        .unwrap()
        .to_string()
}

/// The default of the variable `name` in `variables.tf`, without the quotes of a text. `None` when
/// the variable has no default.
fn variable_default(name: &str) -> Option<String> {
    variable_declaration(name)
        .lines()
        .find_map(|l| l.trim().strip_prefix("default"))
        .and_then(|l| l.trim_start().strip_prefix('='))
        .map(|l| l.trim().trim_matches('"').to_string())
}

/// The scripts name the registry of the lab with `public_repo` in `lib.sh`, and OpenTofu makes the
/// registry with the default of the variable `registry_name`. A lab that names its registry in
/// another way changes both, and this test fails when only one changes.
#[test]
fn the_registry_of_the_lab_has_one_name_in_lib_sh_and_in_the_variables_of_the_configuration() {
    let lib = std::fs::read_to_string(live_script("lib.sh")).unwrap();
    let in_lib = lib
        .lines()
        .find_map(|l| l.strip_prefix("public_repo="))
        .map(|name| name.trim_matches('"'))
        .expect("lib.sh names the registry of the lab");
    assert!(!in_lib.is_empty());
    assert_eq!(
        variable_default("registry_name").as_deref(),
        Some(in_lib),
        "lib.sh and variables.tf name the registry of the lab in two ways"
    );
}

#[test]
fn a_cluster_in_the_cluster_template_gets_a_time_zone_because_palette_refuses_it_without_one() {
    let default = variable_default("cluster_timezone").expect("cluster_timezone has a default");
    let default = default.as_str();
    assert!(
        !default.is_empty() && !default.contains(' '),
        "the default is a name of the IANA time zone database: {default:?}"
    );

    // The clouds whose cluster resource takes a cluster template: variables.tf says aws and gcp.
    let templated: Vec<String> = terraform_files()
        .into_iter()
        .filter(|(name, text)| {
            name.starts_with("cloud-") && text.contains("dynamic \"cluster_template\"")
        })
        .map(|(name, _)| name)
        .collect();
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

// ---- the standing profiles of the contract run (#531) ----------------------------------------

/// One `resource` or `data` block of the OpenTofu module of the live example.
struct TfBlock {
    file: String,
    is_data: bool,
    ty: String,
    name: String,
    body: String,
}

impl TfBlock {
    /// The address of the block, such as `aws_ecr_repository.pack` or `data.aws_ami.ubuntu`.
    fn address(&self) -> String {
        let address = format!("{}.{}", self.ty, self.name);
        if self.is_data {
            format!("data.{address}")
        } else {
            address
        }
    }

    /// The expression of the `count` or of the `for_each` of the block, if the block has one.
    fn selector(&self) -> Option<&str> {
        self.body.lines().find_map(|line| {
            let line = line.strip_prefix("  ").filter(|l| !l.starts_with(' '))?;
            let (key, expression) = line.split_once('=')?;
            matches!(key.trim(), "count" | "for_each").then(|| expression.trim())
        })
    }

    /// Whether the body of the block names the object at `address`, and not another object whose
    /// address starts with the same text.
    fn names(&self, address: &str) -> bool {
        self.body.match_indices(address).any(|(at, _)| {
            let before = self.body[..at].chars().next_back();
            let after = self.body[at + address.len()..].chars().next();
            !before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.')
                && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
        })
    }
}

/// The `resource` and `data` blocks of one file. A block starts at a line such as
/// `resource "type" "name" {`. It ends at the next line that is a `}` alone, as `tofu fmt` writes
/// it.
fn tf_blocks(file: &str, text: &str) -> Vec<TfBlock> {
    let mut blocks = Vec::new();
    let mut open: Option<TfBlock> = None;
    for line in text.lines() {
        if let Some(block) = open.as_mut() {
            if line == "}" {
                blocks.extend(open.take());
            } else {
                block.body.push_str(line);
                block.body.push('\n');
            }
        } else if let [kind @ ("resource" | "data"), ty, name, "{"] =
            line.split_whitespace().collect::<Vec<_>>().as_slice()
        {
            open = Some(TfBlock {
                file: file.to_string(),
                is_data: *kind == "data",
                ty: ty.trim_matches('"').to_string(),
                name: name.trim_matches('"').to_string(),
                body: String::new(),
            });
        }
    }
    blocks
}

/// The name and the text of each file of the module, in the order of the names.
fn terraform_files() -> Vec<(String, String)> {
    let mut names: Vec<String> = std::fs::read_dir(live_script("terraform"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.ends_with(".tf"))
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let text = terraform_file(&name);
            (name, text)
        })
        .collect()
}

/// The expression that a line `  name = expression` gives to the local `name`.
fn local_expression<'a>(files: &'a [(String, String)], name: &str) -> Option<&'a str> {
    files
        .iter()
        .flat_map(|(_, text)| text.lines())
        .find_map(|line| {
            let (key, expression) = line.strip_prefix("  ")?.split_once('=')?;
            (key.trim() == name).then(|| expression.trim())
        })
}

/// Whether `expression` reads the variable `own_registries`, itself or through a local.
fn reads_the_switch(files: &[(String, String)], expression: &str, seen: &mut Vec<String>) -> bool {
    if expression.contains("var.own_registries") {
        return true;
    }
    for (at, _) in expression.match_indices("local.") {
        let local: String = expression[at + "local.".len()..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if seen.contains(&local) {
            continue;
        }
        seen.push(local.clone());
        if local_expression(files, &local).is_some_and(|e| reads_the_switch(files, e, seen)) {
            return true;
        }
    }
    false
}

/// The blocks that make an AWS object or a Palette registry. The cloud account of an EKS cluster
/// is the one exception: `cloud-eks.tf` counts it by `var.cloud`, and the standing set sets the
/// cloud `aws`.
fn registry_blocks(files: &[(String, String)]) -> Vec<TfBlock> {
    files
        .iter()
        .flat_map(|(file, text)| tf_blocks(file, text))
        .filter(|b| {
            let makes = b.ty.starts_with("aws_")
                || (!b.is_data && b.ty.starts_with("spectrocloud_registry_"));
            makes && !(b.file == "cloud-eks.tf" && b.selector().is_some())
        })
        .collect()
}

/// The addresses of the blocks that make an AWS object or a Palette registry, and the faults. A
/// fault is a block that does not count by the switch `own_registries`.
fn registry_blocks_by_the_switch(files: &[(String, String)]) -> (Vec<String>, Vec<String>) {
    let blocks = registry_blocks(files);
    let faults = blocks
        .iter()
        .filter(|b| {
            !b.selector()
                .is_some_and(|s| reads_the_switch(files, s, &mut Vec::new()))
        })
        .map(|b| format!("{} ({})", b.address(), b.file))
        .collect();
    (blocks.iter().map(TfBlock::address).collect(), faults)
}

/// The addresses of the other blocks that name an object of `registry_blocks`, and the faults: a
/// block that does not count by the switch.
fn blocks_that_need_a_registry(files: &[(String, String)]) -> (Vec<String>, Vec<String>) {
    let own: Vec<String> = registry_blocks(files)
        .iter()
        .map(TfBlock::address)
        .collect();
    let mut needing = Vec::new();
    let mut faults = Vec::new();
    for block in files.iter().flat_map(|(file, text)| tf_blocks(file, text)) {
        if own.contains(&block.address()) || !own.iter().any(|a| block.names(a)) {
            continue;
        }
        needing.push(block.address());
        let by_the_switch = block
            .selector()
            .is_some_and(|s| reads_the_switch(files, s, &mut Vec::new()));
        if !by_the_switch {
            faults.push(format!("{} ({})", block.address(), block.file));
        }
    }
    (needing, faults)
}

/// The files of the module with the first `from` of `file` changed to `to`.
fn mutated(file: &str, from: &str, to: &str) -> Vec<(String, String)> {
    terraform_files()
        .into_iter()
        .map(|(name, text)| {
            let text = if name == file {
                assert!(text.contains(from), "{file} holds {from:?}");
                text.replacen(from, to, 1)
            } else {
                text
            };
            (name, text)
        })
        .collect()
}

#[test]
fn each_aws_object_and_each_palette_registry_of_the_live_module_counts_by_own_registries() {
    let (made, faults) = registry_blocks_by_the_switch(&terraform_files());
    assert!(faults.is_empty(), "no count by the switch: {faults:?}");
    for address in [
        "data.aws_caller_identity.this",
        "aws_ecr_repository.pack",
        "aws_ecr_repository.dep",
        "aws_ecr_repository.chart",
        "aws_iam_user.registry",
        "aws_iam_access_key.registry",
        "aws_iam_user_policy.registry_read",
        "data.aws_iam_policy_document.registry_read",
        "spectrocloud_registry_oci.pack",
        "spectrocloud_registry_oci.helm",
        "aws_instance.kinds",
        "spectrocloud_registry_helm.kinds_charts",
        "spectrocloud_registry_oci.kinds",
    ] {
        assert!(made.iter().any(|a| a == address), "{address} in {made:?}");
    }
}

#[test]
fn a_resource_with_no_count_by_the_switch_fails_the_check_of_the_live_module() {
    let files = mutated("registry.tf", "  count                = local.own\n", "");
    let (_, faults) = registry_blocks_by_the_switch(&files);
    assert_eq!(faults, ["aws_ecr_repository.pack (registry.tf)"]);

    // the registry kinds count by `local.kinds`, which must read the switch too
    let files = mutated(
        "registry-kinds.tf",
        "var.registry_kinds && var.own_registries",
        "var.registry_kinds",
    );
    let (_, faults) = registry_blocks_by_the_switch(&files);
    assert!(
        faults.contains(&"aws_vpc.kinds (registry-kinds.tf)".to_string())
            && faults.contains(&"spectrocloud_registry_oci.kinds (registry-kinds.tf)".to_string()),
        "{faults:?}"
    );
}

#[test]
fn each_profile_that_needs_an_own_registry_counts_by_own_registries() {
    let (needing, faults) = blocks_that_need_a_registry(&terraform_files());
    assert!(faults.is_empty(), "no count by the switch: {faults:?}");
    for address in [
        "spectrocloud_cluster_profile.owned",
        "spectrocloud_cluster_profile.dep",
        "spectrocloud_cluster_profile.owned_helm",
        "data.spectrocloud_pack.owned",
        "data.spectrocloud_pack.requiring",
        "data.spectrocloud_pack.dep",
        "data.spectrocloud_pack.owned_chart",
    ] {
        assert!(
            needing.iter().any(|a| a == address),
            "{address} in {needing:?}"
        );
    }

    let files = mutated(
        "main.tf",
        "count        = var.owned_chart_profile ? local.own : 0",
        "count        = var.owned_chart_profile ? 1 : 0",
    );
    let (_, faults) = blocks_that_need_a_registry(&files);
    assert_eq!(faults, ["data.spectrocloud_pack.owned_chart (main.tf)"]);
}

#[test]
fn the_switch_own_registries_defaults_to_true_and_no_recipe_of_the_live_example_sets_it() {
    let declaration = variable_declaration("own_registries");
    assert!(declaration.contains("type        = bool"), "{declaration}");
    assert_eq!(
        variable_default("own_registries").as_deref(),
        Some("true"),
        "the default makes the registries, as the module did before the switch"
    );

    // A recipe sets a variable with -var, in the justfile or in a script. Only the script of the
    // standing set sets the switch to `false`.
    assert!(
        !repo_file("justfile").contains("own_registries"),
        "the justfile sets no switch: the script does"
    );
    let mut setters = Vec::new();
    for entry in std::fs::read_dir(live_script("")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "sh")
            && std::fs::read_to_string(&path)
                .unwrap()
                .contains("own_registries")
        {
            setters.push(path.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    assert_eq!(setters, ["contract-profiles.sh"]);
    assert!(!terraform_file("terraform.tfvars.example").contains("own_registries"));
}

/// A tree with `contract-profiles.sh`, the `common.sh` that it sources, and `tf.sh`, and a `tofu`
/// that logs its arguments to `FAKE_LOG`. The state file of the set is where the script puts it in
/// this tree.
struct StandingSet {
    dir: tempfile::TempDir,
}

impl StandingSet {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let live = dir.path().join("tree/examples/live");
        std::fs::create_dir_all(live.join("terraform")).unwrap();
        copy_scripts(
            &dir.path().join("tree"),
            &[
                "examples/live/contract-profiles.sh",
                "examples/live/common.sh",
                "examples/live/tf.sh",
            ],
        );
        fake_bin(dir.path(), &[("tofu", LOGGING_TOOL)]);
        std::fs::create_dir(dir.path().join("home")).unwrap();
        StandingSet { dir }
    }

    fn terraform(&self) -> PathBuf {
        self.dir.path().join("tree/examples/live/terraform")
    }

    fn state(&self) -> PathBuf {
        self.terraform().join("contract.tfstate")
    }

    fn run(&self, argument: &str, envs: &[(&str, &str)]) -> Output {
        Command::new("bash")
            .arg(
                self.dir
                    .path()
                    .join("tree/examples/live/contract-profiles.sh"),
            )
            .arg(argument)
            .env("PATH", path_with(&self.dir.path().join("bin")))
            .env("HOME", self.dir.path().join("home"))
            .env("PALETTE_API_KEY", "a-key-that-no-line-shows")
            .env_remove("SPECTROCLOUD_APIKEY")
            .env_remove("SPECTROCLOUD_HOST")
            .env_remove("PALETTE_API")
            .env("FAKE_LOG", self.dir.path().join("calls.log"))
            .envs(envs.iter().copied())
            .output_locked()
            .unwrap()
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("calls.log"))
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }
}

/// The arguments that make the standing set: the prefix, the cloud, no cluster, and no own
/// registry.
const STANDING_VARIABLES: &str =
    "-var name_prefix=strata-contract -var cloud=aws -var cluster=false -var own_registries=false";

#[test]
fn contract_profiles_up_applies_the_module_with_the_prefix_no_cluster_no_own_registry_and_a_state_of_its_own()
 {
    let set = StandingSet::new();
    let o = set.run("up", &[]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let chdir = format!("-chdir={}", set.terraform().display());
    assert_eq!(
        set.calls(),
        [
            format!("tofu {chdir} init -input=false"),
            format!(
                "tofu {chdir} apply -parallelism=1 -state={} {STANDING_VARIABLES}",
                set.state().display()
            ),
        ]
    );
    for output in [&o.stdout, &o.stderr] {
        assert!(!text(output).contains("a-key-that-no-line-shows"));
    }

    // The state is not terraform.tfstate, the file that tofu reads with no -state. live-up and
    // live-down read that file, so they never read the set.
    assert_ne!(set.state().file_name().unwrap(), "terraform.tfstate");
    for name in ["live-up", "live-down", "live-profiles"] {
        for line in recipe(name) {
            assert!(
                !line.contains("-state") && !line.contains("contract"),
                "{name} reads the default state: {line}"
            );
        }
    }
    assert_eq!(
        recipe("contract-profiles action"),
        ["examples/live/contract-profiles.sh {{action}}"]
    );
}

#[test]
fn contract_profiles_down_destroys_what_up_made_with_the_same_variables_and_the_same_state() {
    let set = StandingSet::new();
    let o = set.run("down", &[]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert_eq!(
        set.calls(),
        [format!(
            "tofu -chdir={} destroy -parallelism=1 -state={} {STANDING_VARIABLES}",
            set.terraform().display(),
            set.state().display()
        )]
    );
}

#[test]
fn contract_profiles_status_lists_the_state_and_exits_1_while_the_set_holds_no_resource() {
    let set = StandingSet::new();
    let o = set.run("status", &[]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    assert!(text(&o.stdout).contains("holds no resource"));
    assert!(set.calls().is_empty(), "no state file: no call of tofu");

    std::fs::write(set.state(), "{}").unwrap();
    let o = set.run("status", &[("FAKE_OUT", "")]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    assert!(text(&o.stdout).contains("holds no resource"));

    let o = set.run(
        "status",
        &[(
            "FAKE_OUT",
            "spectrocloud_cluster_profile.release\nspectrocloud_cluster_profile.addon\n",
        )],
    );
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert_eq!(
        text(&o.stdout),
        "spectrocloud_cluster_profile.release\nspectrocloud_cluster_profile.addon\n"
    );
    assert_eq!(
        set.calls().last().unwrap(),
        &format!(
            "tofu -chdir={} state list -state={}",
            set.terraform().display(),
            set.state().display()
        )
    );
}

#[test]
fn contract_profiles_sh_refuses_an_argument_that_is_not_up_down_or_status() {
    let set = StandingSet::new();
    let o = set.run("destroy", &[]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("up, down, or status"),
        "{}",
        text(&o.stderr)
    );
    assert!(set.calls().is_empty());
}

/// The statements of the live scripts that list the profiles of the project. They are a call of
/// `strata list` for profiles and a request of the listing route of Palette. A statement ends at
/// the first line that does not end with a backslash.
fn listings_of_profiles() -> Vec<(String, String)> {
    let mut listings = Vec::new();
    for entry in std::fs::read_dir(live_script("")).unwrap() {
        let path = entry.unwrap().path();
        if !path.extension().is_some_and(|e| e == "sh") {
            continue;
        }
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        let source = std::fs::read_to_string(&path).unwrap();
        let mut lines = code_lines(&source);
        while let Some(line) = lines.next() {
            let strata_list =
                line.contains("$strata\" list ") || line.contains("$strata\\\" list ");
            let others = ["list clusters", "list filters", "list projects"];
            if !(strata_list && !others.iter().any(|o| line.contains(o))
                || line.contains("/v1/dashboard/clusterprofiles"))
            {
                continue;
            }
            let mut statement = line.to_string();
            let mut last = line;
            while last.ends_with('\\') {
                let Some(next) = lines.next() else { break };
                statement.push('\n');
                statement.push_str(next);
                last = next;
            }
            listings.push((file.clone(), statement));
        }
    }
    listings
}

#[test]
fn each_live_script_that_lists_the_profiles_of_the_project_matches_them_by_name_or_uid() {
    // The script saves the listing, or checks it with an exit code. The next command reads it by
    // uid or by name. In baseline.sh, it selects by each name of the profile_names output. In
    // loop.sh, it selects by the uid of the release. In template.sh, it selects by the uid of the
    // linked version.
    let matched_later = [
        ("baseline.sh", "list profiles --json >"),
        ("loop.sh", "list --pack \"$pack\" --json"),
        ("template.sh", "list profiles --packs --json"),
    ];
    let matches = [".name ==", ".uid ==", "index(", "grep -qE '^  $"];
    let listings = listings_of_profiles();
    let mut unmatched = Vec::new();
    for (file, statement) in &listings {
        let later = matched_later
            .iter()
            .any(|(f, needle)| f == file && statement.contains(needle));
        if !later && !matches.iter().any(|m| statement.contains(m)) {
            unmatched.push(format!("{file}: {statement}"));
        }
    }
    assert!(
        unmatched.is_empty(),
        "no match by name or uid:\n{}",
        unmatched.join("\n")
    );
    for (file, needle) in matched_later {
        assert!(
            listings
                .iter()
                .any(|(f, s)| f == file && s.contains(needle)),
            "{file} lists the profiles with {needle:?}"
        );
    }
    // a script that lists each profile of the project and counts them has no place in this list
    assert!(listings.len() >= 20, "{} listings", listings.len());
}

#[test]
fn registry_move_nodes_sh_moves_the_os_pack_of_the_release_and_cluster_a_takes_it() {
    let mut stage = Stage::new(baseline_outputs());
    stage.env("WATCH_MINUTES", "0");
    stage.route(
        &cluster_route("c-a"),
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
        &profile_route("r-1"),
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
        "move-compare",
        "move-impact",
        "move-palette",
        "move-reasons",
        "replaced",
        "waits",
    ] {
        assert!(has_verdict(&steps, "SEEN", seen), "{seen}: {steps}");
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

/// The answer of `packs/manifests` for the add-on `ad-1` while its draft waits: the metadata of
/// the profile, and one pack summary with the name `summary` and the manifest `m-1`.
fn draft_manifests(summary: &str) -> String {
    serde_json::json!({"metadata": {"name": "strata-live-addon", "uid": "ad-1"},
        "spec": {"packs": [{"metadata": {"name": summary, "uid": "pk-1"},
            "spec": {"name": summary, "manifests": [
                {"name": "config", "uid": "m-1", "digest": "d", "content": "edited: \"no\"\n"}]}}]}})
    .to_string()
}

/// A stage of draft.sh where Palette has the add-on `ad-1`, and the create of the copy with no
/// publish answers the uid `un-1`. strata says `published` for `un-1` in the JSON of the list.
/// The add-on has a draft with the pin `config`, and `packs/manifests` gives its two names.
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
    let pins = r#"[{"name":"config","tag":"1.0.0","type":"manifest","layer":"addon",
        "packUid":"pk-1","values":""}]"#;
    stage.route(
        &profile_route("ad-1"),
        &format!(
            r#"{{"metadata":{{"name":"strata-live-addon","uid":"ad-1"}},"spec":{{"version":"1.0.0",
                "published":{{"type":"add-on","cloudType":"all","packs":{pins}}},
                "draft":{{"type":"add-on","cloudType":"all","packs":{pins}}}}}}}"#
        ),
    );
    stage.route(
        "/v1/clusterprofiles/ad-1/packs/manifests",
        &draft_manifests("config"),
    );
    stage.route("/v1/clusterprofiles", r#"{"uid":"un-1"}"#);
    stage.route(
        PROFILE_LISTING,
        r#"{"items":[{"metadata":{"name":"strata-live-unpublished","uid":"un-1"}}]}"#,
    );
    stage.route(&profile_route("un-1"), "{}");
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
    failed(&steps, &["unpublished-listed"]);
    assert_ne!(o.status.code(), Some(0));
}

#[test]
fn draft_sh_reads_the_two_names_of_packs_manifests_while_the_draft_waits() {
    let stage = unpublished_stage(false);
    let (_, steps) = stage.run("draft.sh", &[]);
    passed(&steps, &["route-list", "route-names"]);
    // the checks only read while the draft waits: each request between the PUT of the draft and
    // the publish is a GET
    let calls = std::fs::read_to_string(stage.path("curl.log")).unwrap();
    let waits: Vec<&str> = calls
        .lines()
        .skip_while(|l| *l != "PUT /v1/clusterprofiles/ad-1")
        .skip(1)
        .take_while(|l| *l != "PATCH /v1/clusterprofiles/ad-1/publish")
        .collect();
    assert!(
        waits.contains(&"GET /v1/clusterprofiles/ad-1/packs/manifests"),
        "{calls}"
    );
    assert!(waits.iter().all(|l| l.starts_with("GET ")), "{calls}");

    // a pack summary with a name that is not the name of its pin
    let stage = unpublished_stage(false);
    stage.route(
        "/v1/clusterprofiles/ad-1/packs/manifests",
        &draft_manifests("config-pack"),
    );
    let (o, steps) = stage.run("draft.sh", &[]);
    failed(&steps, &["route-names"]);
    passed(&steps, &["route-list"]);
    assert_ne!(o.status.code(), Some(0));
}

/// A stage of `scope.sh`: the release profile `r-1` of the project, and its copy `t-1` that
/// Palette creates at tenant scope. The document of `t-1` gives the scope `tenant_scope`.
fn scope_stage(tenant_scope: &str) -> Stage {
    let mut stage = Stage::new(baseline_outputs());
    stage.route(
        &profile_route("r-1"),
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
        &profile_route("t-1"),
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
    failed(&steps, &["annotation-tenant"]);
    assert_ne!(o.status.code(), Some(0));
    let calls = std::fs::read_to_string(stage.path("curl.log")).unwrap();
    assert!(
        calls.ends_with("DELETE /v1/clusterprofiles/t-1\n"),
        "the tenant profile goes also after a failed check: {calls}"
    );
}

/// Stage 7 of `packs.sh` against a listing of `loki` whose 2.10.2 names `amd64,arm64` and whose
/// 3.6.7 names `amd64`. The `behind` of the record of 2.10.2 answers `line` and `json`.
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
    failed(&steps, &["arch-behind-says"]);
    failed(&steps, &["arch-behind-json"]);
}

/// A run of `registry-kinds.sh` against the stand-ins. The tools of the registries read their
/// standard input and do nothing else.
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
    // The script pipes the password to each tool, under `pipefail`. A tool that ends before the
    // password arrives makes the writer fail with SIGPIPE, and so the pipeline fails. A real tool
    // reads its password. This one keeps what it reads in `TOOL.stdin`.
    let reads_password = "#!/usr/bin/env bash\ncat >> \"$0.stdin\"\n";
    fake_bin(
        stage.root.path(),
        &[
            ("tofu", &tofu),
            ("oras", reads_password),
            ("helm", reads_password),
            ("zarf", reads_password),
            ("spectro", reads_password),
        ],
    );
    stage.route("/v1/registries/pack", r#"{"uid":"reg-packs"}"#);
    stage.strata("packs strata-live-zarf *", 0, packs);
    let (_, steps) = stage.run("registry-kinds.sh", &[]);
    (stage, steps)
}

#[test]
fn registry_kinds_sh_gives_the_password_to_each_login_on_the_standard_input_and_each_tool_reads_it()
{
    let (stage, steps) = registry_kinds("");
    assert!(steps.contains("== 0. the registry host"), "{steps}");
    for tool in ["oras", "helm", "zarf", "spectro"] {
        let read = std::fs::read_to_string(stage.path(&format!("bin/{tool}.stdin")));
        assert_eq!(read.unwrap_or_default(), "pw\n", "{tool} read the password");
    }
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
    let (_, steps) = registry_kinds(&format!(
        "strata-live-zarf in strata-live-oci (1 version)\n{row}"
    ));
    failed(&steps, &["packs-zarf-says"]);
    let (_, steps) = registry_kinds(
        "strata-live-zarf in strata-live-oci (1 version, no digests)\n  1.0.0  pk-z  oci  all  1.0.0\n",
    );
    failed(&steps, &["packs-zarf-says"]);
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
        &format!(
            "palette    refuses: {PROFILE_EXISTS_CODE}: Cluster Profile with name \
             'strata-live-profile' and version '1.0.0' already exists\n"
        ),
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

/// The report of `strata behind --json` for a record that pins kubernetes 1.35.3. The newest
/// version is `newest`. The answer of Palette for the bump is `validation`.
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

/// The uid of the cluster `fred`.
const FRED_UID: &str = "c-fred";

/// The document of the cluster `fred`: the profile `p-infra` holds kubernetes and a CNI pack, and
/// the profile `p-addon` holds one pack.
fn fred_document() -> serde_json::Value {
    serde_json::json!({"spec": {"clusterProfileTemplates": [
        {"uid": "p-infra", "packs": [
            {"name": "kubernetes", "tag": "1.35.3", "type": "spectro", "values": "k: 1",
             "packUid": "pk-1"},
            {"name": "cni", "tag": "1.0.0", "type": "spectro", "values": "c: 1"}]},
        {"uid": "p-addon", "packs": [
            {"name": "csi", "tag": "2.0.0", "type": "spectro", "values": "s: 1"}]}]}})
}

/// A stage 6 run of validate-packs.sh on the profile `strata-live-release@1.0.0` and the cluster
/// `fred`. `strata behind --json` gives `report` for the record of the profile, and
/// `cluster_report` for the record of the cluster. Gives steps.log.
fn bump_stage(report: &str, cluster_report: &str) -> (Stage, String) {
    bump_stage_with(report, cluster_report, |_| {})
}

/// [`bump_stage`], where `arrange` changes the stage before the run, such as the answer of the
/// route of the cluster.
fn bump_stage_with(
    report: &str,
    cluster_report: &str,
    arrange: impl FnOnce(&Stage),
) -> (Stage, String) {
    let mut stage = Stage::new(serde_json::json!({}));
    stage.env("PROFILE", "strata-live-release@1.0.0");
    stage.env("CLUSTER", "fred");
    stage.strata(
        "snapshot strata-live-release@1.0.0 -o bump-profile.json",
        0,
        "{}",
    );
    stage.strata(
        "snapshot cluster:fred -o bump-cluster.json",
        0,
        &serde_json::json!({"source": {"cluster": {"uid": FRED_UID}}}).to_string(),
    );
    stage.route(&cluster_route(FRED_UID), &fred_document().to_string());
    stage.route(
        &format!("/v1/spectroclusters/{FRED_UID}/validate/packs"),
        r#"{"profiles":[]}"#,
    );
    stage.strata("behind bump-profile.json --json", 1, report);
    stage.strata("behind bump-cluster.json --json", 1, cluster_report);
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
    arrange(&stage);
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
    let report = kubernetes_bump_report("1.37.0", &upgrade_refused());
    let (_, steps) = bump_stage(&report, &report);
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
            "bump-cluster-same",
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
fn validate_packs_sh_stage_6_sends_the_route_of_the_cluster_a_tag_that_no_registry_has() {
    let report = kubernetes_bump_report("1.37.0", &upgrade_refused());
    let (stage, steps) = bump_stage(&report, &report);
    passed(
        &steps,
        &[
            "cluster-route-doc",
            "cluster-route-tag",
            "cluster-route-tag-says",
        ],
    );
    // the profile with kubernetes has its packs as the cluster runs them, with the tag 9.99.9.
    // The other profile of the cluster has its uid alone
    let sent = stage.sent(
        "POST",
        &format!("/v1/spectroclusters/{FRED_UID}/validate/packs"),
    );
    assert_eq!(
        sent,
        [serde_json::json!({"profiles": [
            {"uid": "p-infra", "packValues": [
                {"name": "kubernetes", "tag": "9.99.9", "type": "spectro", "values": "k: 1"},
                {"name": "cni", "tag": "1.0.0", "type": "spectro", "values": "c: 1"}]},
            {"uid": "p-addon"}]})]
    );
}

#[test]
fn validate_packs_sh_stage_6_sends_the_route_of_the_cluster_a_body_longer_than_one_argument() {
    // One argument of Linux holds 128 KiB at most. The values of the packs of a real cluster
    // are longer. A body passed as an argument fails the check with exit 126 (#466)
    const LIMIT: usize = 128 * 1024;
    let report = kubernetes_bump_report("1.37.0", &upgrade_refused());
    let values = "v: 1\n".repeat(LIMIT / 4);
    let mut document = fred_document();
    document["spec"]["clusterProfileTemplates"][0]["packs"][1]["values"] = values.clone().into();
    let (stage, steps) = bump_stage_with(&report, &report, |stage| {
        stage.route(&cluster_route(FRED_UID), &document.to_string());
    });
    passed(
        &steps,
        &[
            "cluster-route-doc",
            "cluster-route-tag",
            "cluster-route-tag-says",
        ],
    );
    let sent = stage.sent(
        "POST",
        &format!("/v1/spectroclusters/{FRED_UID}/validate/packs"),
    );
    assert_eq!(sent.len(), 1, "{steps}");
    assert!(sent[0].to_string().len() > LIMIT);
    assert_eq!(
        sent[0]["profiles"][0]["packValues"][1]["values"].as_str(),
        Some(values.as_str())
    );
}

#[test]
fn validate_packs_sh_stage_6_fails_when_the_route_of_the_cluster_checks_the_sent_tag() {
    let report = kubernetes_bump_report("1.37.0", &upgrade_refused());
    // Palette gives an error for the tag: the check of the tag fails, and the request passed
    let refusing = r#"{"profiles":[{"uid":"p-infra","packs":{"results":[{"name":"kubernetes",
        "errors":[{"code":"PackNotFound","message":"Pack 'kubernetes' 9.99.9 is not found"}]}]}}]}"#;
    let route = format!("/v1/spectroclusters/{FRED_UID}/validate/packs");
    let (_, steps) = bump_stage_with(&report, &report, |stage| stage.route(&route, refusing));
    passed(&steps, &["cluster-route-tag"]);
    failed(&steps, &["cluster-route-tag-says"]);

    // Palette refuses the whole request with a status: the status is not 200
    let (_, steps) = bump_stage_with(&report, &report, |stage| {
        stage.route(&route, r#"{"code":"InvalidParam"}"#);
        std::fs::write(
            stage
                .path("routes")
                .join(format!("{}.status", Stage::route_key(&route))),
            "400",
        )
        .unwrap();
    });
    passed(&steps, &["cluster-route-tag"]);
    failed(&steps, &["cluster-route-tag-says"]);
}

#[test]
fn validate_packs_sh_stage_6_fails_when_palette_accepts_a_move_of_two_minor_versions() {
    // Palette accepts, and the newest version is one minor version above the pin
    let report = kubernetes_bump_report("1.36.2", &serde_json::json!({"accepts": true}));
    let (_, steps) = bump_stage(&report, &report);
    passed(
        &steps,
        &["bump-asked", "bump-cluster-same", "bump-cluster-asked"],
    );
    failed(
        &steps,
        &[
            "bump-pair",
            "bump-refused",
            "bump-clusters",
            "bump-cluster-refused",
        ],
    );
}

#[test]
fn validate_packs_sh_stage_6_fails_when_the_report_of_the_cluster_holds_no_answer_of_palette() {
    // the record of the cluster has a bump, and Palette gave no answer for it
    let profile = kubernetes_bump_report("1.37.0", &upgrade_refused());
    let mut cluster: serde_json::Value = serde_json::from_str(&profile).unwrap();
    cluster["pins"][1]["bump"]
        .as_object_mut()
        .unwrap()
        .remove("paletteValidation");
    let (_, steps) = bump_stage(&profile, &cluster.to_string());
    passed(&steps, &["bump-cluster-same"]);
    failed(&steps, &["bump-cluster-asked", "bump-cluster-refused"]);
}

#[test]
fn validate_packs_sh_stage_6_fails_when_the_report_of_the_cluster_names_another_bump_or_none() {
    let profile = kubernetes_bump_report("1.37.0", &upgrade_refused());
    // the record of the cluster names another version to move to
    let other = kubernetes_bump_report("1.36.2", &upgrade_refused());
    let (_, steps) = bump_stage(&profile, &other);
    failed(&steps, &["bump-cluster-same"]);
    // the record of the cluster has no bump: the pin is behind, and the bump is absent
    let mut none: serde_json::Value = serde_json::from_str(&profile).unwrap();
    none["pins"][1].as_object_mut().unwrap().remove("bump");
    let (_, steps) = bump_stage(&profile, &none.to_string());
    failed(
        &steps,
        &[
            "bump-cluster-same",
            "bump-cluster-asked",
            "bump-cluster-refused",
        ],
    );
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
    (
        "eks-ami-type-al2",
        "UnsupportedAmiType: AMI type 'AL2_x86_64' is not supported",
    ),
    (
        "eks-ami-type-unknown",
        "IsValidValue: Invalid amiType 'BOTTLEROCKET_x86_64' specified",
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

/// A stage for validate-route.sh. It has the clean bodies of vSphere and EKS in `bodies/`, and the
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
    // the route takes an AL2 type: its case fails
    let stage = validate_route_stage(&[("eks-ami-type-al2", "passed", "")]);
    let bodies = stage.path("bodies");
    let (o, steps) = stage.run("validate-route.sh", &[bodies.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(1), "{steps}");
    // the report of a body that passed names no fault
    failed(&steps, &["eks-ami-type-al2", "eks-ami-type-al2-says"]);
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
    failed(&steps, &["vsphere-instance-floor-cpu-says"]);
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
    // each case goes to the route of AWS. The glob names the cloud, so a case on another engine
    // finds no answer. Such a case passes as a body that the route takes
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
    failed(&steps, &[taken, &says]);
}

// ---- route-bodies.sh -------------------------------------------------------------------------------

/// The pack listing of the lab. It holds the packs that route-bodies.sh pins. It also holds the
/// packs that its search for `pack-cloud-support` skips. These are one of another registry,
/// one that lists aws, one that lists `all`, one with no cloud type, and one of another layer.
/// `gcp-dns`, which lists gcp alone, is in the listing when `without_aws` is true.
fn route_packs(without_aws: bool) -> String {
    let pack = |name: &str, version: &str, registry: &str, layer: &str, clouds: &[&str]| {
        let uid = format!("pk-{name}");
        PackFixture::new(name, layer, version, &uid, registry, "", "").summary(clouds)
    };
    let mut items = vec![
        pack("gcp-elsewhere", "1.0.0", "reg-other", "addon", &["gcp"]),
        pack("aws-and-gcp", "3.0.0", "reg-pub", "addon", &["aws", "gcp"]),
        pack("all-clouds", "4.0.0", "reg-pub", "addon", &["all"]),
        pack("no-clouds", "5.0.0", "reg-pub", "addon", &[]),
        pack("gcp-csi", "6.0.0", "reg-pub", "csi", &["gcp"]),
        pack("gloo", "1.2.18", "reg-pub", "addon", &["all"]),
        pack("generic-vm-libvirt", "1.0.0", "reg-pub", "infra", &["all"]),
        pack("cert-manager", "1.21.1", "reg-pub", "addon", &["all"]),
    ];
    if without_aws {
        items.push(pack("gcp-dns", "2.0.0", "reg-pub", "addon", &["gcp"]));
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

/// A stage of route-bodies.sh in a lab on AWS. The lab has the account `lab-aws`, the release
/// profile `r-1` on AWS, the registry `Public Repo`, and the packs `packs`. Palette makes a
/// profile for each `POST /v1/clusterprofiles` with the uid `uid-<name>`, and publishes and
/// deletes it. The validate route refuses the fault of each case, except `pack-cloud-support` and
/// the cases in `takes`, which it takes.
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
        &profile_route("r-1"),
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
    // a route that takes a fault fails the run
    failed(&steps, &["validate-route"]);
    let route_steps = std::fs::read_to_string(stage.path("out/validate-route/steps.log")).unwrap();
    failed(&route_steps, &["profile-cloud-mismatch"]);
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
        &profile_route("r-1"),
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
/// the team. The repository is an empty one on `main`, where `loop.sh` resumes.
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
    git(&repo, &["init", "-q", "-b", "main"]);
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
        serde_json::json!({"schema": strata::ops::BEHIND_SCHEMA, "generatedAt": "2026-09-26T06:00:00Z",
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
fn loop_sh_stage_7_asks_the_nightly_job_to_put_the_writer_of_an_edit_in_the_issue() {
    let mut stage = lab_stage();
    stage.strata(
        "verify --summary --quiet --format json *",
        1,
        &format!(
            r#"{{"schema":"{}","records":[{{"path":"releases/addon.json",
            "status":"drifted","name":"strata-live-addon","version":"1.0.0","uid":"ad-1",
            "recordedAt":"2026-09-26T06:00:00Z"}}]}}"#,
            strata::ops::VERIFY_SUMMARY_SCHEMA
        ),
    );
    // the check of the record, with and without --who: the report is the same for this test
    stage.strata(
        "verify releases/addon.json --quiet --format markdown*",
        1,
        "**DRIFT**: published version strata-live-addon@1.0.0 differs\n\nwho: 1 write in the audit log since the record\n\n- 2026-09-26T06:30:00.123Z  John Doe (user)  update\n",
    );
    // the stage edits the add-on through the API, and this test does not serve that route, so the
    // first step fails. The checks of the nightly job do not need that step
    let (_, steps) = stage.run("loop.sh", &["7"]);
    passed(&steps, &["nightly-who", "nightly-who-says"]);
    let calls = std::fs::read_to_string(stage.path("strata/calls")).unwrap();
    assert!(
        calls
            .lines()
            .any(|l| l == "verify releases/addon.json --quiet --format markdown --who"),
        "{calls}"
    );
}

#[test]
fn loop_sh_stage_9_reads_the_log_of_the_unknown_values_from_a_state_folder_in_the_run() {
    let mut stage = lab_stage();
    stage.strata(
        "report",
        0,
        "Palette gave no values that this release of strata does not know\n",
    );
    // the last check of the stage runs the baseline script, which this test does not serve
    let (_, steps) = stage.run("loop.sh", &["9"]);
    passed(&steps, &["retire", "report-log"]);
    let calls = std::fs::read_to_string(stage.path("strata/calls")).unwrap();
    assert!(calls.lines().any(|l| l == "report"), "{calls}");
    // every call of strata in the run sees the one folder, an absolute path in the run directory
    let state = std::fs::read_to_string(stage.path("strata/state")).unwrap();
    let want = stage.path("out").join("state");
    assert!(
        state.lines().all(|l| Path::new(l) == want),
        "want {} in every call: {state}",
        want.display()
    );
}

/// Cluster A runs the release and the add-on. A strata that holds each profile on the cluster side
/// fails the checks of #615. A strata that holds the release alone passes them.
#[test]
fn loop_sh_stage_3_checks_that_a_cluster_ref_with_no_profile_holds_the_release_alone() {
    for whole in [false, true] {
        let mut stage = lab_stage();
        let addon = stage.path("out/repo/releases/addon.json");
        std::fs::create_dir_all(addon.parent().unwrap()).unwrap();
        std::fs::write(&addon, r#"{"packs":[{"name":"strata-addon"}]}"#).unwrap();
        let (exit, report) = if whole {
            (
                1,
                r#"{"a":{"ref":"cluster:c-a"},"packs":[{"name":"kubernetes"},{"name":"strata-addon"}]}"#,
            )
        } else {
            (
                0,
                r#"{"a":{"ref":"cluster:c-a/strata-live-profile"},"packs":[{"name":"kubernetes"}]}"#,
            )
        };
        stage.strata("compare repo/releases/1.0.0.json cluster:c-a", exit, "");
        stage.strata("compare cluster:c-a n-1 --repave --json", 1, report);
        let (_, steps) = stage.run("loop.sh", &["3"]);
        passed(&steps, &["ask-repave-plain"]);
        let checks = ["cluster-plain", "ask-repave-plain-says"];
        if whole {
            failed(&steps, &checks);
        } else {
            passed(&steps, &checks);
        }
    }
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
    // the comment of the pull request snapshots each ref once, and compares the files with the
    // question for Palette. Strata first names the ref that a comparison reads for the cluster
    stage.strata(
        "compare */ref1.json cluster:c-a/r-1 --format json",
        1,
        r#"{"b":{"ref":"cluster:c-a/r-1"}}"#,
    );
    stage.strata(
        "snapshot cluster:c-a/r-1 -o *",
        0,
        r#"{"source":{"uid":"r-1","cluster":{"uid":"c-a"}}}"#,
    );
    stage.strata("snapshot n-1 -o *", 0, r#"{"source":{"uid":"n-1"}}"#);
    stage.strata(
        "compare */ref0.json */ref1.json --format json --repave",
        1,
        r#"{"summary":{"maxImpact":"repave"},"paletteRepave":{"required":false,"agrees":false}}"#,
    );
    stage.strata(
        "compare */ref0.json */ref1.json --format markdown --repave",
        1,
        "**Result:** CHANGED\n\n**Palette:** no repave\n",
    );
    stage.strata(
        "preflight build.json --cloud aws --tier static --offline --quiet",
        0,
        "OK: each counted check passed\n",
    );
    stage.strata(
        "preflight build.json --cloud aws --tier static --offline --quiet --format markdown",
        0,
        "## strata-preflight: aws cluster `strata-loop-build`\n\n**OK: each counted check passed** — source `build.json`\n",
    );
    // the job of a pull request runs preflight.sh: offline, and then with the key of the lab
    stage.strata(
        "preflight build.json --cloud aws --tier static --quiet --offline --out preflight-job --format markdown",
        0,
        "## strata-preflight: aws cluster `strata-loop-build`\n\n**OK: each counted check passed** — source `build.json`, Palette release not given, tiers static, offline. 4 ran\n",
    );
    stage.strata(
        "preflight build.json --cloud aws --tier static,palette --quiet --out preflight-job-key --format markdown",
        0,
        "## strata-preflight: aws cluster `strata-loop-build`\n\n**OK: each counted check passed** — source `build.json`, Palette release not given, tiers static, palette. 6 ran\n",
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
        "preflight build.json --cloud aws --tier palette --rule iaas-pod-identity --json --quiet",
        0,
        r#"{"checks":[{"id":"iaas-pod-identity","verdict":"passed"}]}"#,
    );
    stage.strata(
        "preflight build-fault.json --cloud aws --tier static,palette --no-exec --strict --quiet",
        1,
        "FAIL: 1 check failed: account-missing\n",
    );

    let (_, steps) = stage.run("loop.sh", &["3"]);
    passed(
        &steps,
        &[
            "ask-repave",
            "ask-repave-answer",
            "ask-validate",
            "ask-validate-answer",
            "ask-repave-comment",
            "ask-repave-comment-says",
            "build-body",
            "preflight-static",
            "preflight-comment",
            "preflight-comment-says",
            "preflight-job",
            "preflight-job-says",
            "preflight-job-key",
            "preflight-job-key-says",
            "preflight-live",
            "preflight-each",
            "preflight-offering",
            "preflight-key-pair",
            "preflight-fault",
        ],
    );
    // the comment holds the answer of Palette, and the note says that Palette decided the label
    let comment = std::fs::read_to_string(stage.path("out/repo/repave-comment.md")).unwrap();
    assert!(comment.contains("**Palette:** no repave"), "{comment}");
    let note = std::fs::read_to_string(stage.path("out/ask-repave-comment.err")).unwrap();
    assert!(
        note.contains("max impact: repave; label repave: Palette says no repave; would remove label repave if present"),
        "{note}"
    );
    // the comment records the cluster side by the ref that strata names for it
    let calls = std::fs::read_to_string(stage.path("strata/calls")).unwrap();
    let at = |start: &str, end: &str| {
        calls
            .lines()
            .position(|l| l.starts_with(start) && l.ends_with(end))
    };
    let side = at("compare ", " cluster:c-a/r-1 --format json");
    let record = at("snapshot cluster:c-a/r-1 -o ", "ref0.json");
    assert!(side.zip(record).is_some_and(|(s, r)| s < r), "{calls}");
    // the pull request gets the report under the marker of its own
    let report = std::fs::read_to_string(stage.path("out/preflight-comment.md")).unwrap();
    assert!(
        report.starts_with("<!-- strata-preflight -->\n## strata-preflight: aws cluster"),
        "{report}"
    );
    // the body is the create of a cluster on the release in the account of the lab
    let body = read_json(&stage.path("out/build.json"));
    assert_eq!(body["metadata"]["name"], "strata-loop-build");
    assert_eq!(body["spec"]["cloudAccountUid"], "acc-1");
    assert_eq!(body["spec"]["profiles"][0]["uid"], "r-1");
    assert_eq!(body["spec"]["cloudConfig"]["region"], "us-east-2");
    let fault = read_json(&stage.path("out/build-fault.json"));
    assert_eq!(fault["spec"]["cloudAccountUid"], "000000000000000000000000");
    // the questions and the preflight write nothing to Palette: the script sent only reads
    let curl = std::fs::read_to_string(stage.path("curl.log")).unwrap();
    assert!(
        curl.lines().all(|l| l.starts_with("GET ")),
        "a request that is not a read:\n{curl}"
    );
}

// ---- report-send.sh ----------------------------------------------------------------------------

/// An `xtask` that makes the chapter `unknown-value` and "serves" it. It writes a port and waits
/// for the end of its standard input, as the real server does.
const FAKE_XTASK_SERVE: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_SEND/xtask.log"
case "$1" in
  harvest) mkdir -p "$2/unknown-value" ;;
  serve) echo 4321 > "$3"; cat > /dev/null ;;
esac
"#;

/// A stand-in `strata` for the send. A run appends a line to the log. It "posts" to the endpoint
/// when it changes the issue file, which a `gh` lists. It records the settings that it got.
/// The schema id of its log line is `FAKE_SCHEMA`, which the test sets from the `const` of the crate.
const FAKE_STRATA_SEND: &str = r#"#!/usr/bin/env bash
[ "$1" != "--version" ] || { echo "strata 0.26.0"; exit 0; }
echo "$STRATA_REPORT $STRATA_REPORT_URL $PALETTE_API $PALETTE_PROJECT $XDG_STATE_HOME $HOME $*" >> "$FAKE_SEND/strata.log"
log="$XDG_STATE_HOME/strata/strata-unknown.jsonl"
mkdir -p "$(dirname "$log")"
echo "{\"schema\":\"$FAKE_SCHEMA\"}" >> "$log"
if [ -z "$FAKE_NO_POST" ]; then
  lines=$(sed -n 's/^| Lines | \([0-9]*\) |.*/\1/p' "$FAKE_SEND/body" 2>/dev/null)
  lines=$(( ${lines:-0} + 1 ))
  printf '%s\n| Lines | %s |\n' '<!-- strata-report: values ProfileType later-type -->' "$lines" > "$FAKE_SEND/body"
  jq -n --rawfile body "$FAKE_SEND/body" '[{number: 7, state: "OPEN", body: $body}]' > "$FAKE_SEND/issues.json"
  wc -l < "$log" | tr -d ' ' > "$XDG_STATE_HOME/strata/strata-unknown.sent"
  echo "strata: sent the report of the unknown values" >&2
fi
echo '{"profiles":[{"name":"aws-stack"}]}'
"#;

/// A stand-in `curl` for the endpoint. The first post changes the issue file and gets 202.
/// A second post gets 200 and `duplicate`. It records the arguments it got.
const FAKE_CURL_SEND: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_SEND/curl.log"
out=""
while [ $# -gt 0 ]; do case "$1" in -o) out=$2; shift 2 ;; *) shift ;; esac; done
if [ -n "$FAKE_NO_POST" ]; then echo '{}' > "$out"; printf 500; exit 0; fi
if [ "$(wc -l < "$FAKE_SEND/curl.log" | tr -d ' ')" = 1 ]; then
  lines=$(sed -n 's/^| Lines | \([0-9]*\) |.*/\1/p' "$FAKE_SEND/body" 2>/dev/null)
  lines=$(( ${lines:-0} + 1 ))
  printf '%s\n| Lines | %s |\n' '<!-- strata-report: values ProfileType later-type -->' "$lines" > "$FAKE_SEND/body"
  jq -n --rawfile body "$FAKE_SEND/body" '[{number: 7, state: "OPEN", body: $body}]' > "$FAKE_SEND/issues.json"
  echo '{"ok":true,"opens":0,"updates":1}' > "$out"
  printf 202
else
  echo '{"ok":true,"duplicate":true}' > "$out"
  printf 200
fi
"#;

/// A `gh` that lists the issues of `$FAKE_SEND/issues.json`. It finds its login as the real `gh`
/// does. The login is the variable `GH_TOKEN`, or the file `hosts.yml` in the folder
/// `GH_CONFIG_DIR`, or else `gh` in `XDG_CONFIG_HOME`, or else `.config/gh` in the home. With no
/// login it fails with exit 4, as the real `gh` does. It records the folder that it looked in.
const FAKE_GH_SEND: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_SEND/gh.log"
folder=${GH_CONFIG_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/gh}
echo "$folder" >> "$FAKE_SEND/gh.folder"
if [ -z "$GH_TOKEN" ] && [ ! -f "$folder/hosts.yml" ]; then
  echo "To get started with GitHub CLI, please run:  gh auth login" >&2
  exit 4
fi
case "$*" in
  "issue list"*) cat "$FAKE_SEND/issues.json" 2>/dev/null || echo '[]' ;;
esac
"#;

/// How the machine of a test of `report-send.sh` keeps the login of `gh`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GhLogin {
    /// The file `hosts.yml` in `.config/gh` of the home of the machine, where `gh` looks first.
    HomeFolder,
    /// The file `hosts.yml` in the folder that `GH_CONFIG_DIR` names.
    ConfigDir,
    /// The file `hosts.yml` in `gh` under the folder that `XDG_CONFIG_HOME` names.
    XdgConfigHome,
    /// The variable `GH_TOKEN`, and no file.
    Token,
    /// No login.
    Nothing,
}

/// Run `report-send.sh` with `args` in a root of its own, on a machine with the login `login` of
/// `gh`. Gives the output and the result directory.
fn report_send(args: &[&str], no_post: bool, root: &Path, login: GhLogin) -> (Output, PathBuf) {
    let bin = fake_bin(
        root,
        &[
            ("gh", FAKE_GH_SEND),
            ("xtask", FAKE_XTASK_SERVE),
            ("curl", FAKE_CURL_SEND),
        ],
    );
    executable(&root.join("bin/strata"), FAKE_STRATA_SEND);
    let out = root.join("out");
    let mut c = Command::new("bash");
    c.arg(live_script("report-send.sh"))
        .args(args)
        .env("PATH", path_with(&bin))
        .env("FAKE_SEND", root)
        .env("FAKE_SCHEMA", strata::closed_set::unknown::log::SCHEMA)
        .env("XTASK_BIN", root.join("bin/xtask"))
        .env("STRATA_BIN", root.join("bin/strata"))
        .env("OUT", &out)
        .env("TMPDIR", root)
        .env("WAIT_TRIES", "1")
        .env("WAIT_SECONDS", "0");
    if no_post {
        c.env("FAKE_NO_POST", "1");
    }
    // the machine of the person: its home holds the login of gh, and the script changes HOME
    let home = root.join("machine");
    let hosts = |folder: PathBuf| {
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("hosts.yml"), "github.com:\n  user: lead\n").unwrap();
        folder
    };
    c.env("HOME", &home)
        .env_remove("GH_CONFIG_DIR")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("GH_TOKEN");
    match login {
        GhLogin::HomeFolder => {
            hosts(home.join(".config/gh"));
        }
        GhLogin::ConfigDir => {
            c.env("GH_CONFIG_DIR", hosts(root.join("elsewhere")));
        }
        GhLogin::XdgConfigHome => {
            hosts(root.join("xdg/gh"));
            c.env("XDG_CONFIG_HOME", root.join("xdg"));
        }
        GhLogin::Token => {
            c.env("GH_TOKEN", "token-of-the-environment");
        }
        GhLogin::Nothing => std::fs::create_dir_all(&home).unwrap(),
    }
    (c.output_locked().unwrap(), out)
}

#[test]
fn report_send_sh_sends_twice_to_the_endpoint_and_checks_that_one_issue_holds_both() {
    let dir = tempfile::tempdir().unwrap();
    let (o, out) = report_send(
        &["https://report.example/", "--repo", "o/r"],
        false,
        dir.path(),
        GhLogin::HomeFolder,
    );
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}\n{}",
        text(&o.stdout),
        text(&o.stderr)
    );
    let steps = std::fs::read_to_string(out.join("steps.log")).unwrap();
    for id in [
        "send-1",
        "send-1-says",
        "send-1-marked",
        "issue",
        "issue-new",
        "send-2",
        "issue-2",
        "repeat-1",
        "repeat-2",
        "repeat-count",
    ] {
        assert!(has_verdict(&steps, "PASS", id), "{id}:\n{steps}");
    }
    assert!(steps.contains("the issue is o/r#7"), "{steps}");
    // the repeat posts one report twice with one key of 64 hex characters
    let curl = std::fs::read_to_string(dir.path().join("curl.log")).unwrap();
    let posts: Vec<&str> = curl.lines().collect();
    assert_eq!(posts.len(), 2, "{curl}");
    let key = |post: &str| {
        let after = post.split("Idempotency-Key: ").nth(1).unwrap();
        after.split(' ').next().unwrap().to_string()
    };
    assert_eq!(key(posts[0]), key(posts[1]), "{curl}");
    assert_eq!(key(posts[0]).len(), 64, "{curl}");
    assert!(
        posts[0].ends_with("https://report.example/"),
        "the post goes to the endpoint: {curl}"
    );
    assert!(
        steps.contains("waited 1 tries for lines_of_issue = 1"),
        "{steps}"
    );
    // the run served the chapter, and strata got the settings of the send and no other setting
    let xtask = std::fs::read_to_string(dir.path().join("xtask.log")).unwrap();
    assert!(xtask.contains("harvest tenants"), "{xtask}");
    assert!(
        xtask.contains("serve tenants/unknown-value port"),
        "{xtask}"
    );
    let strata = std::fs::read_to_string(dir.path().join("strata.log")).unwrap();
    let runs: Vec<&str> = strata.lines().collect();
    assert_eq!(runs.len(), 2, "{strata}");
    for run in runs {
        let words: Vec<&str> = run.split(' ').collect();
        assert_eq!(
            &words[..4],
            [
                "1",
                "https://report.example/",
                "http://127.0.0.1:4321",
                "dev"
            ],
            "{run}"
        );
        assert!(words[4].ends_with("/out/state"), "{run}");
        assert!(run.ends_with("list profiles --packs --json"), "{run}");
    }
    let gh = std::fs::read_to_string(dir.path().join("gh.log")).unwrap();
    assert!(
        gh.contains("issue list --repo o/r --label unknown-value --state all"),
        "{gh}"
    );
    // the script changed HOME, and gh still looked in the folder of the login
    assert_gh_looked_in(dir.path(), &dir.path().join("machine/.config/gh"));
    assert!(
        std::fs::read_to_string(out.join("summary.md"))
            .unwrap()
            .contains("report send test"),
    );
}

/// Make sure that each call of `gh` in the test root `root` looked in the folder `folder`.
fn assert_gh_looked_in(root: &Path, folder: &Path) {
    let looked = std::fs::read_to_string(root.join("gh.folder")).unwrap();
    assert!(!looked.is_empty());
    for line in looked.lines() {
        assert_eq!(Path::new(line), folder, "{looked}");
    }
}

#[test]
fn report_send_sh_keeps_the_login_of_gh_when_it_changes_the_home() {
    // each place where gh finds a login: the script sets GH_CONFIG_DIR to it before the change
    let cases = [
        (GhLogin::HomeFolder, "machine/.config/gh"),
        (GhLogin::ConfigDir, "elsewhere"),
        (GhLogin::XdgConfigHome, "xdg/gh"),
    ];
    for (login, folder) in cases {
        let dir = tempfile::tempdir().unwrap();
        let (o, _) = report_send(&["https://report.example/"], false, dir.path(), login);
        assert_eq!(o.status.code(), Some(0), "{folder}: {}", text(&o.stderr));
        assert_gh_looked_in(dir.path(), &dir.path().join(folder));
    }
    // a GH_TOKEN of the environment works with no file of a login
    let dir = tempfile::tempdir().unwrap();
    let (o, _) = report_send(
        &["https://report.example/"],
        false,
        dir.path(),
        GhLogin::Token,
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
}

#[test]
fn report_send_sh_stops_before_the_first_send_when_gh_has_no_login() {
    let dir = tempfile::tempdir().unwrap();
    let (o, out) = report_send(
        &["https://report.example/", "--repo", "o/r"],
        false,
        dir.path(),
        GhLogin::Nothing,
    );
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stderr));
    assert!(
        text(&o.stderr).contains("gh cannot read the issues of o/r"),
        "{}",
        text(&o.stderr)
    );
    // gh said why, and the script kept it
    let said = std::fs::read_to_string(out.join("gh.err")).unwrap();
    assert!(said.contains("gh auth login"), "{said}");
    // no run of strata and no post: a send with no login can only fail the checks
    assert!(!dir.path().join("strata.log").exists());
    assert!(!dir.path().join("curl.log").exists());
}

#[test]
fn report_send_sh_fails_the_checks_when_the_endpoint_took_no_report() {
    let dir = tempfile::tempdir().unwrap();
    let (o, out) = report_send(
        &["https://report.example/"],
        true,
        dir.path(),
        GhLogin::HomeFolder,
    );
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    let steps = std::fs::read_to_string(out.join("steps.log")).unwrap();
    failed(&steps, &["send-1-says", "issue"]);
}

#[test]
fn report_send_sh_needs_the_address_and_the_tools_and_stops_before_it_runs_anything() {
    let dir = tempfile::tempdir().unwrap();
    let (o, _) = report_send(&[], false, dir.path(), GhLogin::HomeFolder);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("give the address of the endpoint"),
        "{}",
        text(&o.stderr)
    );
    assert!(
        text(&o.stderr).contains("Usage") || text(&o.stderr).contains("just live-report-send URL")
    );
    let (o, _) = report_send(&["a", "b"], false, dir.path(), GhLogin::HomeFolder);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("one address only"),
        "{}",
        text(&o.stderr)
    );
    let (o, _) = report_send(&["--bogus"], false, dir.path(), GhLogin::HomeFolder);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        !dir.path().join("strata.log").exists(),
        "no run before the arguments are right"
    );
}

// ---- the image digest test (#630) ------------------------------------------------------------------

/// The packs of the add-on version `ad-1` at the baseline: one manifest pack with no values.
const ADDON_PACK: &str = r#"[{"name":"strata-live-config","tag":"1.0.0","type":"manifest",
    "layer":"addon","packUid":"pk-1","values":""}]"#;

/// The digest that the fake `crane` serves.
const IMAGE_DIGEST: &str =
    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// A stage of `image-digest.sh`. Cluster A takes the updates of the add-on version `ad-1`, and runs
/// the Deployment of the last PUT of the version. The answers of strata are the records and the
/// comparisons that the script reads. The record of stage 2 holds `list_images`, a JSON list.
fn image_digest_stage(list_images: &str) -> Stage {
    let mut stage = Stage::new(baseline_outputs());
    stage.route(
        &cluster_route("c-a"),
        r#"{"metadata":{"name":"fred"},"status":{"state":"Running"},
            "spec":{"clusterProfileTemplates":[{"uid":"ad-1"}]}}"#,
    );
    stage.route(
        "/v1/spectroclusters/c-a/assets/kubeconfig",
        "apiVersion: v1 # fake-kubeconfig\n",
    );
    stage.route(
        "/v1/spectroclusters/c-a/profiles/packs/manifests",
        r#"{"profiles":[]}"#,
    );
    stage.route("/v1/spectroclusters/c-a/profiles", "{}");
    let manifests = r#"{"spec":{"packs":[{"spec":{"name":"strata-live-config","manifests":[
        {"name":"strata-live-config","uid":"m-1","content":"edited: \"no\"\n"}]}}]}}"#;
    stage.published("ad-1", ADDON_PACK, manifests);
    // after a PUT, Palette gives the manifests of that PUT
    let key = Stage::route_key("/v1/clusterprofiles/ad-1/packs/manifests");
    let routes = stage.path("routes");
    std::fs::write(routes.join(format!("{key}.first")), manifests).unwrap();
    executable(
        &routes.join(format!("{key}.sh")),
        &format!(
            r#"#!/usr/bin/env bash
sent="$FAKE_ROUTES/sent/PUT_v1_clusterprofiles_ad-1"
if [ -f "$sent" ]; then
  jq '{{spec: {{packs: [.spec.template.packs[] | {{spec: {{name, manifests}}}}]}}}}' "$sent"
else
  cat "$FAKE_ROUTES/{key}.first"
fi
"#
        ),
    );
    stage.command(
        "crane",
        &format!("#!/usr/bin/env bash\necho {IMAGE_DIGEST}\n"),
    );
    let put = stage.path("routes/sent/PUT_v1_clusterprofiles_ad-1");
    stage.env("FAKE_KUBE_PUT", put.to_str().unwrap());
    let image = format!("registry.k8s.io/pause:3.10.2@{IMAGE_DIGEST}");
    let record = |images: &str| {
        format!(r#"{{"root":"r","packs":[{{"name":"strata-live-config","images":{images}}}]}}"#)
    };
    stage.strata("snapshot ad-1 -o addon-0.json", 0, &record("[]"));
    stage.strata("snapshot ad-1 -o addon-list.json", 0, &record(list_images));
    stage.strata(
        "snapshot ad-1 -o addon-digest.json",
        0,
        &record(&format!(r#"["{image}"]"#)),
    );
    stage.strata(
        "compare addon-0.json ad-1",
        1,
        "~ [addon] strata-live-config AUTHORED IMAGES\n",
    );
    stage.strata(
        "compare addon-list.json ad-1",
        1,
        "~ [addon] strata-live-config AUTHORED\n",
    );
    stage
}

#[test]
fn image_digest_sh_puts_the_digest_in_the_list_and_then_in_the_manifest_and_reads_the_cluster() {
    let image = format!("registry.k8s.io/pause:3.10.2@{IMAGE_DIGEST}");
    let stage = image_digest_stage(&format!(r#"["{image}"]"#));
    let (o, steps) = stage.run("image-digest.sh", &[]);
    assert_eq!(o.status.code(), Some(0), "{steps}\n{}", text(&o.stderr));
    passed(
        &steps,
        &[
            "base-record",
            "base-no-deployment",
            "list-values",
            "list-manifest",
            "list-record-digest",
            "list-compare",
            "list-compare-flag",
            "list-accept",
            "list-deployed",
            "digest-manifest",
            "digest-compare",
            "digest-compare-flag",
            "digest-accept",
            "digest-deployed",
            "undo-manifest",
            "undo-values",
            "undo-accept",
            "undo-delete",
            "undo-clean",
            "undo-baseline",
        ],
    );
    for id in [
        "list-spec",
        "list-spec-digest",
        "list-pod",
        "digest-spec",
        "digest-pod",
        "undo-gone",
    ] {
        assert!(
            has_verdict(&steps, "SEEN", id),
            "{id} is recorded:\n{steps}"
        );
    }
    let said = |id: &str| std::fs::read_to_string(stage.path(&format!("out/{id}.out"))).unwrap();
    assert_eq!(said("list-spec").trim(), "registry.k8s.io/pause:3.10.2");
    assert_eq!(
        said("digest-spec").trim(),
        format!("registry.k8s.io/pause:3.10.2@{IMAGE_DIGEST}")
    );

    // the first PUT gives the pack the list, and no PUT changes the tag, the uid, or the manifest uid
    let puts = stage.sent("PUT", "/v1/clusterprofiles/ad-1");
    assert_eq!(
        puts.len(),
        5,
        "values, manifest, digest, manifest back, values back"
    );
    let pack = |n: usize| &puts[n]["spec"]["template"]["packs"][0];
    assert_eq!(
        pack(0)["values"],
        format!(
            "pack:\n  content:\n    images:\n      - image: registry.k8s.io/pause:3.10.2@{IMAGE_DIGEST}"
        )
    );
    let manifest = |n: usize| {
        pack(n)["manifests"][0]["content"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert!(
        manifest(0).starts_with("edited: \"no\"\n"),
        "{}",
        manifest(0)
    );
    assert!(manifest(1).contains("image: registry.k8s.io/pause:3.10.2\n"));
    assert!(
        !manifest(1).contains("@sha256"),
        "the tag alone: {}",
        manifest(1)
    );
    assert!(manifest(2).contains(&format!(
        "image: registry.k8s.io/pause:3.10.2@{IMAGE_DIGEST}\n"
    )));
    assert_eq!(manifest(3), "edited: \"no\"\n", "the manifest is back");
    assert_eq!(pack(4)["values"], "", "the values are back");
    for n in 0..5 {
        assert_eq!(
            pack(n)["manifests"][0]["uid"],
            "m-1",
            "PUT {n} keeps the manifest uid"
        );
        assert_eq!(pack(n)["tag"], "1.0.0");
    }
    // the kubeconfig is a credential: kubectl reads a file, and no file is left
    let left = files_under(&stage.path("tmp"));
    assert!(
        left.iter()
            .all(|(_, body)| !body.contains("fake-kubeconfig")),
        "a kubeconfig is left: {left:?}"
    );
    let kubectl = std::fs::read_to_string(stage.path("kubectl.log")).unwrap();
    assert!(
        kubectl.contains("get deployment strata-live-image -n default"),
        "{kubectl}"
    );
}

/// Palette applies each object that the content names. It removes none that a later edit drops. The
/// Deployment stays after the undo, so `undo-gone` records its result and judges nothing. The undo
/// then deletes the Deployment with kubectl, and the run leaves the cluster clean.
#[test]
fn image_digest_sh_deletes_the_deployment_that_stays_after_the_content_drops_it() {
    let image = format!("registry.k8s.io/pause:3.10.2@{IMAGE_DIGEST}");
    let mut stage = image_digest_stage(&format!(r#"["{image}"]"#));
    let marker = stage.path("orphan");
    stage.env("FAKE_KUBE_ORPHAN", marker.to_str().unwrap());
    let (o, steps) = stage.run("image-digest.sh", &[]);
    assert_eq!(o.status.code(), Some(0), "{steps}\n{}", text(&o.stderr));
    assert!(
        has_verdict(&steps, "SEEN", "undo-gone"),
        "the check records its result:\n{steps}"
    );
    assert!(
        step_line(&steps, "undo-gone").ends_with("got 1"),
        "the Deployment stayed, so the wait ended with 1:\n{steps}"
    );
    passed(&steps, &["undo-delete", "undo-clean", "undo-baseline"]);
    let kubectl = std::fs::read_to_string(stage.path("kubectl.log")).unwrap();
    let delete = kubectl
        .lines()
        .position(|l| l.contains("delete deployment strata-live-image -n default"))
        .unwrap_or_else(|| panic!("no delete in:\n{kubectl}"));
    let lines: Vec<&str> = kubectl.lines().collect();
    let last_get = lines
        .iter()
        .rposition(|l| l.contains("get deployment strata-live-image -n default"))
        .unwrap();
    assert!(
        delete < last_get,
        "undo-clean reads after the delete:\n{kubectl}"
    );
}

#[test]
fn image_digest_sh_fails_the_check_of_the_record_when_the_list_has_no_digest() {
    let stage = image_digest_stage(r#"["registry.k8s.io/pause:3.10.2"]"#);
    let (o, steps) = stage.run("image-digest.sh", &[]);
    failed(&steps, &["list-record-digest"]);
    assert_ne!(o.status.code(), Some(0));
}

#[test]
fn image_digest_sh_stops_before_it_changes_anything_when_crane_cannot_read_the_digest() {
    let stage = image_digest_stage("[]");
    stage.command(
        "crane",
        "#!/usr/bin/env bash\necho 'UNAUTHORIZED' >&2\nexit 1\n",
    );
    let (o, steps) = stage.run("image-digest.sh", &[]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("crane cannot read the digest"),
        "{}",
        text(&o.stderr)
    );
    assert!(steps.is_empty(), "no stage ran: {steps}");
    assert!(!stage.path("routes/sent").exists(), "no request to Palette");
}

#[test]
fn api_sh_set_values_replaces_the_values_of_one_pack_and_keeps_the_rest() {
    let root = tempfile::tempdir().unwrap();
    let routes = root.path().join("routes");
    addon_profile_routes(&routes);
    let api = |args: &[&str]| run_live("api.sh", args, root.path(), &[]);
    let o = api(&[
        "set-values",
        "prof-1",
        "config",
        "pack:\n  content:\n    images: []\n",
    ]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let put = read_json(&routes.join("sent/PUT_v1_clusterprofiles_prof-1"));
    let pack = &put["spec"]["template"]["packs"][0];
    assert_eq!(pack["values"], "pack:\n  content:\n    images: []\n");
    assert_eq!(
        (pack["tag"].as_str(), pack["uid"].as_str()),
        (Some("1.0.0"), Some("pk-1"))
    );
    assert_eq!(
        pack["manifests"],
        serde_json::json!([{"name": "config", "uid": "m-1", "content": "edited: \"no\"\n"}]),
        "the manifest keeps its uid"
    );
    let calls = std::fs::read_to_string(root.path().join("curl.log")).unwrap();
    assert!(
        calls.contains("PATCH /v1/clusterprofiles/prof-1/publish"),
        "{calls}"
    );

    // an empty text gives the pack no values
    let o = api(&["set-values", "prof-1", "config", ""]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let put = read_json(&routes.join("sent/PUT_v1_clusterprofiles_prof-1"));
    assert_eq!(put["spec"]["template"]["packs"][0]["values"], "");

    let o = api(&["set-values", "prof-1", "other", "x"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("no pack other in prof-1"),
        "{}",
        text(&o.stderr)
    );
}

#[test]
fn api_sh_manifest_profile_sends_the_uid_that_palette_asks_of_a_manifest_pack() {
    let root = tempfile::tempdir().unwrap();
    let routes = root.path().join("routes");
    std::fs::create_dir_all(routes.join("sent")).unwrap();
    std::fs::write(routes.join("v1_clusterprofiles"), r#"{"uid":"prof-9"}"#).unwrap();
    std::fs::write(routes.join("v1_clusterprofiles_prof-9_publish"), "{}").unwrap();
    std::fs::write(routes.join("v1_clusterprofiles_prof-9"), "{}").unwrap();
    let file = root.path().join("observer.yaml");
    std::fs::write(&file, "kind: Namespace\n").unwrap();
    let o = run_live(
        "api.sh",
        &["manifest-profile", "barney", "fred", file.to_str().unwrap()],
        root.path(),
        &[],
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let sent = read_json(&routes.join("sent/POST_v1_clusterprofiles"));
    let pack = &sent["spec"]["template"]["packs"][0];
    assert_eq!(pack["uid"], "spectro-manifest-pack", "{pack}");
    assert_eq!(pack["type"], "manifest", "{pack}");
    assert_eq!(
        pack["manifests"][0]["content"], "kind: Namespace\n",
        "{pack}"
    );
}

// ---- observer-kind.sh --------------------------------------------------------------------------------

/// A `kubectl` for observer-kind.sh. It logs each call to `$FAKE_OBS/kubectl.log`.
///
/// It keeps the state of the cluster in files of `$FAKE_OBS`. `applied.yaml` holds what each apply
/// got. `meta.b64` and `count` come from the last apply of records. `key.seen` holds the key that
/// came through standard input. `get pod -o json` prints `pod.json`.
///
/// The reader can open a port-forward and nothing else. `FAKE_READER_SECRETS` lets the reader read
/// a Secret. `FAKE_REACHED` lets a pod of another namespace reach the page. A pod of the mirror prints
/// `mirror-id.json` or `mirror-string.json`, and `FAKE_NO_MIRROR_POD` takes it away. A port-forward
/// runs [`FAKE_PORT_FORWARD`].
const FAKE_KUBECTL_OBSERVER: &str = r#"#!/usr/bin/env bash
obs=$FAKE_OBS
echo "$*" >> "$obs/kubectl.log"
case "$*" in
  "kustomize ."*)
    cat "$FAKE_REPO/deploy/observer/deployment.yaml"
    [ -n "${FAKE_NO_ALERTS_FLAG-}" ] || echo "        - --alerts=/alerts/alerts.json" ;;
  "kustomize "*) cat "$FAKE_REPO/deploy/observer/deployment.yaml" ;;
  *"exec alert-receiver"*"/data/bodies.log"*)
    [ -f "$obs/edited" ] || [ -f "$obs/cleared" ] || exit 1
    profile=strata-live-addon@1.0.0
    open=$(printf 'strata · payments · Drift\n%s: The published version differs from its record.\nPacks: addon/strata-live-config\nFirst seen: 2026-11-04T10:00:00Z' "$profile")
    [ -z "${FAKE_VALUE_LEAKS-}" ] || open="$open"$'\n''edited: "yes"'
    clear=$(printf 'strata · payments · Drift cleared\n%s\nA check proved that this is gone, at 2026-11-04T10:30:00Z.' "$profile")
    [ -n "${FAKE_NO_OPEN-}" ] || jq -cn --arg t "$open" '{text: $t}'
    [ -z "${FAKE_TWO_OPENS-}" ] || jq -cn --arg t "$open" '{text: $t}'
    [ ! -f "$obs/cleared" ] || [ -n "${FAKE_NO_CLEAR-}" ] || jq -cn --arg t "$clear" '{text: $t}' ;;
  *"apply --server-side -f -"*)
    body=$(cat)
    printf '%s\n' "$body" >> "$obs/applied.yaml"
    if grep -q 'meta.json:' <<<"$body"; then
      grep -m1 'meta.json:' <<<"$body" | awk '{print $2}' > "$obs/meta.b64"
      grep -c 'json.gz:' <<<"$body" > "$obs/count"
    fi ;;
  *"create secret generic strata-observer-key"*) cat > "$obs/key.seen" ;;
  *"get secret -l"*) printf 'secret/strata-observer-records-%s\n' 0 1 2 3 ;;
  *"get secret strata-observer-records-0"*) cat "$obs/meta.b64" ;;
  *"get pod"*"jsonpath"*) echo 10.0.0.7 ;;
  *"get pod mirror-"*)
    pod=$(grep -o 'mirror-[a-z]*' <<<"$*" | head -n 1)
    [ -z "${FAKE_NO_MIRROR_POD-}" ] || exit 1
    cat "$obs/$pod.json" ;;
  *"get pod"*) cat "$obs/pod.json" ;;
  *"auth can-i"*)
    case "$*" in
      *portforward*) echo yes; exit 0 ;;
      *"get secrets"*) [ -z "${FAKE_READER_SECRETS-}" ] || { echo yes; exit 0; } ;;
    esac
    echo no; exit 1 ;;
  *"port-forward"*) exec fake-port-forward ;;
  *" run "*) if [ -n "${FAKE_REACHED-}" ]; then echo reached; else echo refused; fi ;;
esac
exit 0
"#;

/// A `curl` for observer-kind.sh: the page, the health route, and the summary of the cluster, which
/// reads the records that the stand-in `kubectl` holds. `/v1/images` says `read`, or `not-read` with
/// `FAKE_PODS_UNREAD`. The row of an image holds a digest, or a pod that never started with
/// `FAKE_NO_DIGEST`, or is a 404 with `FAKE_NO_ROW`.
const FAKE_CURL_OBSERVER: &str = r#"#!/usr/bin/env bash
args=("$@")
url=${args[${#args[@]}-1]}
out=""
for ((i = 0; i < ${#args[@]}; i++)); do [ "${args[i]}" = -o ] && out=${args[i+1]}; done
case "$url" in
  */healthz) body=ok ;;
  */v1/images/*)
    [ -z "${FAKE_NO_ROW-}" ] || exit 22
    enc=${url##*/v1/images/}
    ref=$(printf '%b' "${enc//%/\\x}")
    digests='[{"digest":"sha256:aaaa","status":"first-digest","pods":1}]'
    [ -z "${FAKE_NO_DIGEST-}" ] || digests='[{"status":"not-started","pods":1}]'
    body=$(jq -cn --arg ref "$ref" --argjson d "$digests" '{schema: "strata-observed/1", pods: "read",
      image: {ref: $ref, status: "steady", match: "none", byDigest: false, packs: [], digests: $d}}') ;;
  */v1/images)
    pods=read; [ -z "${FAKE_PODS_UNREAD-}" ] || pods=not-read
    body=$(jq -cn --arg p "$pods" '{schema: "strata-observed/1", pods: $p, readAt: "2026-11-04T10:00:00Z",
      counts: {images: 2, matched: 0, changed: 0}, images: []}') ;;
  */v1/cluster)
    meta=$(base64 -d "$FAKE_OBS/meta.b64" 2>/dev/null || echo '{}')
    cycle=$(( $(cat "$FAKE_OBS/cycle" 2>/dev/null || echo 0) + 1 ))
    echo "$cycle" > "$FAKE_OBS/cycle"
    body=$(jq -cn --argjson meta "$meta" --arg uid "$FAKE_CLUSTER_UID" \
      --argjson n "$(cat "$FAKE_OBS/count" 2>/dev/null || echo 0)" --arg at "2026-11-04T10:$(printf %02d "$cycle"):00Z" \
      '{cluster: {uid: $uid, name: "payments"}, content: {checkedAt: $at}, records: {count: $n, baseline: $meta.baseline, revision: $meta.revision}}') ;;
  */v1/alerts)
    set=true; [ -z "${FAKE_ALERTS_UNSET-}" ] || set=false
    body=$(jq -cn --argjson set "$set" '{set: $set, channels: ["platform-oncall", "platform-planning"], digestEvery: "168h", waiting: 0, sample: "x",
      rules: [{level: "cluster", news: "drift", send: "now", to: "platform-oncall", state: "inactive"},
              {level: "cluster", news: "image", send: "now", to: "platform-oncall", state: "inactive"},
              {level: "cluster", news: "lost", send: "now", to: "platform-oncall", state: "inactive"},
              {level: "cluster", news: "newer", send: "digest", to: "platform-planning", state: "inactive"},
              {level: "profile", match: "barney-base", news: "newer", muteUntil: "2027-06-30", state: "muted"}]}') ;;
  */v1/findings)
    body='{"findings":[]}'
    [ ! -f "$FAKE_OBS/edited" ] || body='{"findings":[{"id":"f1","kind":"published-changed","profile":"strata-live-addon@1.0.0","packs":["addon/strata-live-config"]}]}' ;;
  */v1/packs/*/diff*)
    unified=""
    [ ! -f "$FAKE_OBS/edited" ] || [ -n "${FAKE_NO_DIFF_TEXT-}" ] || unified='-edited: "no"
+edited: "yes"'
    body=$(jq -cn --arg u "$unified" '{values: {unified: $u}, manifests: []}') ;;
  *) body='<!doctype html><html lang="en"></html>' ;;
esac
if [ -n "$out" ]; then printf '%s' "$body" > "$out"; printf 200; else printf '%s\n' "$body"; fi
"#;

/// A `kind` for observer-kind.sh. It logs each call to `$FAKE_OBS/kind.log`, except the list of the
/// nodes, which has one node. `create cluster` copies the file of `--config` to
/// `$FAKE_OBS/kind-config.yaml`. `get kubeconfig` prints a kubeconfig.
const FAKE_KIND_OBSERVER: &str = r#"#!/usr/bin/env bash
if [ "$1 $2" = "get nodes" ]; then echo strata-observer-test-control-plane; exit 0; fi
echo "$*" >> "$FAKE_OBS/kind.log"
if [ "$1 $2" = "create cluster" ]; then
  for arg in "$@"; do
    [ "$previous" != "--config" ] || cp "$arg" "$FAKE_OBS/kind-config.yaml"
    previous=$arg
  done
fi
[ "$1 $2" != "get kubeconfig" ] || echo "apiVersion: v1 # fake-kubeconfig"
"#;

/// A `docker` for observer-kind.sh. It logs each call to `$FAKE_OBS/docker.log`, and the input of
/// `exec -i` to `hosts.log`. `FAKE_NO_IMAGE` takes the image away. `FAKE_NO_CERTS_D` gives a node
/// whose containerd reads no folder certs.d. `FAKE_NO_PUSH` refuses the push. `FAKE_DIE_AT_NODES` ends
/// the script with SIGTERM when it asks the first node.
const FAKE_DOCKER_OBSERVER: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_OBS/docker.log"
case "$*" in
  "image inspect"*) [ -z "${FAKE_NO_IMAGE-}" ] ;;
  "exec "*"grep -q config_path"*)
    [ -z "${FAKE_DIE_AT_NODES-}" ] || kill -TERM "$PPID"
    [ -z "${FAKE_NO_CERTS_D-}" ] ;;
  "exec -i "*) { echo "-- $*"; cat; } >> "$FAKE_OBS/hosts.log" ;;
  "push "*) [ -z "${FAKE_NO_PUSH-}" ] ;;
esac
"#;

/// The pod that the Deployment of the repository makes: its spec, and the status of a pod that is
/// Ready. The cluster adds no volume for a token, because the pod asks for none.
fn observer_pod() -> serde_json::Value {
    let deployment: serde_yaml::Value =
        serde_yaml::from_str(&repo_file("deploy/observer/deployment.yaml")).unwrap();
    let spec = serde_json::to_value(&deployment["spec"]["template"]["spec"]).unwrap();
    serde_json::json!({"items": [{
        "spec": spec,
        "status": {"podIP": "10.0.0.7", "containerStatuses": [{"ready": true, "restartCount": 0}]},
    }]})
}

/// A profile record, as the stand-in `strata` writes it.
fn observer_record(name: &str) -> String {
    serde_json::json!({"schema": "strata-snapshot/1", "source": {"name": name, "version": "1.0.0"}})
        .to_string()
}

/// The `api.sh` of the tree of observer-kind.sh. It logs each call to `$FAKE_OBS/api.log`. The edit
/// of the add-on version makes the file `edited`, and the edit back makes the file `cleared`. The
/// stand-ins of `kubectl` and `curl` read the two files. With `FAKE_DIE_AFTER_EDIT`, the edit ends the
/// script with SIGTERM.
const FAKE_API_KIND: &str = r##"#!/usr/bin/env bash
obs=$FAKE_OBS
echo "$*" >> "$obs/api.log"
case "$1" in
  get-kubeconfig) echo "apiVersion: v1 # fake-kubeconfig" ;;
  edit)
    if [ "$3" = 'edited: "no"' ]; then
      touch "$obs/edited"; rm -f "$obs/cleared"
      [ -z "${FAKE_DIE_AFTER_EDIT-}" ] || kill -TERM "$PPID"
    else
      rm -f "$obs/edited"; touch "$obs/cleared"
    fi ;;
esac
exit 0
"##;

/// A stage for observer-kind.sh with the stand-ins of the tools of the script. The script runs in a
/// tree of its own, so that its `api.sh`, `tf.sh`, and `git` are stand-ins. The key of the tenant is
/// `key-that-no-log-shows`.
fn observer_kind_stage() -> Stage {
    let mut stage = Stage::new(serde_json::json!({
        "project_uid": "p-1", "cluster_uid": "c-a",
        "release_profile_uid": "r-1", "addon_profile_uid": "ad-1",
    }));
    let tree = stage.path("tree");
    let mut files = vec![
        "examples/live/observer-kind.sh".to_string(),
        "examples/live/lib.sh".to_string(),
        "examples/live/common.sh".to_string(),
        "examples/observer-alerts/alerts.json".to_string(),
        "scripts/ci/observer-records.sh".to_string(),
        "Cargo.toml".to_string(),
    ];
    for file in [
        "kustomization",
        "namespace",
        "serviceaccount",
        "networkpolicy",
        "deployment",
        "reader-role",
        "alerts/kustomization",
    ] {
        files.push(format!("deploy/observer/{file}.yaml"));
    }
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    copy_scripts(&tree, &files);
    let live = tree.join("examples/live");
    executable(&live.join("api.sh"), FAKE_API_KIND);
    executable(
        &live.join("tf.sh"),
        "#!/usr/bin/env bash\ncat \"$FAKE_OUTPUTS\"\n",
    );
    let obs = stage.path("obs");
    std::fs::create_dir_all(&obs).unwrap();
    std::fs::write(
        obs.join("pod.json"),
        serde_json::to_string(&observer_pod()).unwrap(),
    )
    .unwrap();
    for (pod, image) in [
        ("mirror-id", "vendor.example/product:1.0"),
        ("mirror-string", "strata-observer-registry:5000/product:1.0"),
    ] {
        let repository = image.rsplit_once(':').unwrap().0;
        std::fs::write(
            obs.join(format!("{pod}.json")),
            serde_json::json!({
                "spec": {"containers": [{"image": image}]},
                "status": {"containerStatuses": [{"image": image, "imageID": format!("{repository}@sha256:aaaa")}]},
            })
            .to_string(),
        )
        .unwrap();
    }
    let bin = stage.path("obs-bin");
    std::fs::create_dir_all(&bin).unwrap();
    for (name, body) in [
        ("kubectl", FAKE_KUBECTL_OBSERVER),
        ("curl", FAKE_CURL_OBSERVER),
        ("kind", FAKE_KIND_OBSERVER),
        ("docker", FAKE_DOCKER_OBSERVER),
        (
            "git",
            "#!/usr/bin/env bash\necho 0123456789abcdef0123456789abcdef01234567\n",
        ),
    ] {
        executable(&bin.join(name), body);
    }
    stage.strata(
        "snapshot r-1 -o release.json -q",
        0,
        &observer_record("strata-live-profile"),
    );
    stage.strata(
        "snapshot ad-1 -o addon.json -q",
        0,
        &observer_record("strata-live-addon"),
    );
    stage.strata(
        "snapshot cluster:c-a -o * -q",
        0,
        r#"{"schema":"strata-snapshot/1","source":{"cluster":{"uid":"c-a","profiles":[
            {"name":"strata-live-profile","version":"1.0.0"},
            {"name":"strata-live-addon","version":"1.0.0"}]}}}"#,
    );
    stage.strata(
        "snapshot strata-live-profile@1.0.0 -o * -q",
        0,
        &observer_record("strata-live-profile"),
    );
    stage.strata(
        "snapshot strata-live-addon@1.0.0 -o * -q",
        0,
        &observer_record("strata-live-addon"),
    );
    let path = format!("{}:{}", bin.display(), path_with(&stage.path("bin")));
    stage.env("PATH", &path);
    stage.env("PALETTE_API_KEY", "key-that-no-log-shows");
    stage.env("FAKE_OBS", obs.to_str().unwrap());
    stage.env("FAKE_REPO", env!("CARGO_MANIFEST_DIR"));
    stage.env("FAKE_CLUSTER_UID", "c-a");
    stage
}

/// Run observer-kind.sh of the tree of `stage`.
fn run_kind(stage: &Stage) -> (Output, String) {
    let script = stage.path("tree/examples/live/observer-kind.sh");
    stage.run(script.to_str().unwrap(), &[])
}

/// Each check of observer-kind.sh, in the order that it runs.
const OBSERVER_KIND_CHECKS: [&str; 46] = [
    "kind-create",
    "image-load",
    "namespace",
    "config",
    "key",
    "record-release",
    "record-addon",
    "records-apply",
    "records-four",
    "records-meta",
    "manifests-apply",
    "rollout",
    "pod-ready",
    "pod-non-root",
    "pod-read-only-root",
    "pod-no-token",
    "pod-ready-status",
    "reader-role",
    "reader-can-forward",
    "reader-no-secret",
    "reader-no-exec",
    "reader-forward",
    "reader-health",
    "reader-page",
    "reader-cluster",
    "other-pod-refused",
    "first-seen-apply",
    "first-seen-meta",
    "mirror-registry",
    "mirror-nodes",
    "mirror-pods",
    "component-apply",
    "component-read",
    "mirror-clean",
    "alert-receiver",
    "alert-secret",
    "alert-apply",
    "alert-route",
    "alert-edit",
    "alert-open",
    "alert-no-value",
    "alert-undo",
    "alert-clear",
    "alert-one-each",
    "alert-clean",
    "kind-delete",
];

/// The checks of observer-kind.sh that record a result and judge none.
const OBSERVER_KIND_SEEN: [&str; 2] = ["mirror-image-id", "mirror-image-string"];

fn observer_log(stage: &Stage, file: &str) -> String {
    std::fs::read_to_string(stage.path("obs").join(file)).unwrap_or_default()
}

#[test]
fn observer_kind_sh_installs_the_observer_on_kind_and_checks_the_pod_the_page_and_the_refusal() {
    let stage = observer_kind_stage();
    let (o, steps) = run_kind(&stage);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    assert!(!steps.contains("FAIL"), "{steps}");
    passed(&steps, &OBSERVER_KIND_CHECKS);
    for id in OBSERVER_KIND_SEEN {
        assert!(
            has_verdict(&steps, "SEEN", id),
            "{id} records a result and judges none:\n{steps}"
        );
    }

    let kubectl = observer_log(&stage, "kubectl.log");
    for call in [
        "create configmap strata-observer --from-literal=cluster-uid=c-a",
        "--from-literal=project-uid=p-1",
        "create secret generic strata-observer-key --from-file=api-key=/dev/stdin",
        "--as=system:serviceaccount:strata-observer:reader -n strata-observer port-forward deploy/strata-observer 18080:8080",
        "auth can-i create pods --subresource=portforward --as=system:serviceaccount:strata-observer:reader",
        "auth can-i get secrets --as=system:serviceaccount:strata-observer:reader",
        "auth can-i create pods --subresource=exec --as=system:serviceaccount:strata-observer:reader",
        "create rolebinding reader --role=strata-observer-reader --serviceaccount=strata-observer:reader",
    ] {
        assert!(kubectl.contains(call), "{call}:\n{kubectl}");
    }
    // Each apply of a namespace, a role, or a record is a server-side apply. A client-side apply copies
    // each Secret to an annotation, and it fails above 256 KiB. The ConfigMap and the Secret of the key
    // are small: `create_apply` sends each one with `apply -f -`.
    let applies: Vec<&str> = kubectl.lines().filter(|l| l.contains(" apply ")).collect();
    let (server, client): (Vec<&str>, Vec<&str>) =
        applies.iter().partition(|l| l.contains("--server-side"));
    assert_eq!(server.len(), 10, "{applies:?}");
    assert_eq!(client.len(), 2, "{applies:?}");
    assert!(
        client.iter().all(|l| l.ends_with(" apply -f -")),
        "{client:?}"
    );

    let applied = observer_log(&stage, "applied.yaml");
    assert!(
        applied.contains("image: strata-observer:dev") && !applied.contains(":VERSION"),
        "the manifests carry the image of this run"
    );
    assert_eq!(
        applied.matches("kind: Secret").count(),
        8,
        "two applies of four Secrets"
    );

    // the key reaches the Secret through standard input, and no file or call shows it
    assert_eq!(observer_log(&stage, "key.seen"), "key-that-no-log-shows");
    let mut shown = Vec::new();
    for dir in ["out", "obs"] {
        for (file, content) in files_under(&stage.path(dir)) {
            if file != "key.seen" && content.contains("key-that-no-log-shows") {
                shown.push(format!("{dir}/{file}"));
            }
        }
    }
    assert!(shown.is_empty(), "the key is in {shown:?}");

    let kind = observer_log(&stage, "kind.log");
    assert_eq!(
        kind.lines().collect::<Vec<_>>(),
        [
            "create cluster --name strata-observer-test --wait 120s --config kind-config.yaml",
            "load docker-image strata-observer:dev --name strata-observer-test",
            "get kubeconfig --name strata-observer-test",
            "delete cluster --name strata-observer-test",
        ]
    );
    // containerd reads the folder certs.d only when the config of the cluster sets config_path, and
    // the files of the mirror go to that folder
    let config: serde_yaml::Value =
        serde_yaml::from_str(&observer_log(&stage, "kind-config.yaml")).unwrap();
    assert_eq!(config["kind"], "Cluster");
    assert_eq!(config["apiVersion"], "kind.x-k8s.io/v1alpha4");
    let patches = config["containerdConfigPatches"].as_sequence().unwrap();
    assert_eq!(patches.len(), 1, "{patches:?}");
    let patch = patches[0].as_str().unwrap();
    for line in [
        r#"[plugins."io.containerd.grpc.v1.cri".registry]"#,
        r#"config_path = "/etc/containerd/certs.d""#,
    ] {
        assert!(patch.lines().any(|l| l.trim() == line), "{line}:\n{patch}");
    }
    // the kubeconfig of the port-forward is a credential: no file of it stays
    assert_eq!(
        std::fs::read_dir(stage.path("tmp")).unwrap().count(),
        0,
        "a temporary file stays"
    );
    // stage 7: the component applies as a team lists it, and the two pods wait to be Ready
    let root = stage.path("tree").display().to_string();
    for call in [
        format!("kustomize {root}/examples/observer-pods"),
        "-n strata-observer rollout status deploy/strata-observer --timeout=180s".to_string(),
        "-n default wait --for=condition=Ready pod/mirror-id --timeout=120s".to_string(),
        "-n default wait --for=condition=Ready pod/mirror-string --timeout=120s".to_string(),
        "-n default get pod mirror-id -o json".to_string(),
        "-n default get pod mirror-string -o json".to_string(),
        "-n default delete pod mirror-id --ignore-not-found --wait=false".to_string(),
        "-n default delete pod mirror-string --ignore-not-found --wait=false".to_string(),
    ] {
        assert!(kubectl.contains(&call), "{call}:\n{kubectl}");
    }
    let pods = observer_log(&stage, "applied.yaml");
    for image in [
        "image: vendor.example/product:1.0",
        "image: strata-observer-registry:5000/product:1.0",
    ] {
        assert!(pods.contains(image), "{image}:\n{pods}");
    }
    // the registry holds the image under the name that the node mirror serves, and goes at the end
    let docker = observer_log(&stage, "docker.log");
    let calls: Vec<&str> = docker
        .lines()
        .filter(|l| !l.starts_with("image inspect"))
        .collect();
    assert_eq!(
        calls,
        [
            "rm -f strata-observer-registry",
            "run -d --name strata-observer-registry --network kind -p 127.0.0.1:5001:5000 registry:2.8.3",
            "pull registry.k8s.io/pause:3.10",
            "tag registry.k8s.io/pause:3.10 localhost:5001/product:1.0",
            "push localhost:5001/product:1.0",
            "exec strata-observer-test-control-plane grep -q config_path /etc/containerd/config.toml",
            "exec -i strata-observer-test-control-plane sh -c mkdir -p /etc/containerd/certs.d/vendor.example && cat > /etc/containerd/certs.d/vendor.example/hosts.toml",
            "exec -i strata-observer-test-control-plane sh -c mkdir -p /etc/containerd/certs.d/strata-observer-registry:5000 && cat > /etc/containerd/certs.d/strata-observer-registry:5000/hosts.toml",
            "exec strata-observer-test-control-plane rm -rf /etc/containerd/certs.d/vendor.example /etc/containerd/certs.d/strata-observer-registry:5000",
            "rm -f strata-observer-registry",
        ]
    );
    let hosts = observer_log(&stage, "hosts.log");
    assert!(
        hosts.contains("server = \"https://vendor.example\"\n\n[host.\"http://strata-observer-registry:5000\"]\n  capabilities = [\"pull\", \"resolve\"]"),
        "vendor.example goes to the registry first:\n{hosts}"
    );
    assert!(
        hosts.contains("server = \"http://strata-observer-registry:5000\""),
        "the registry has no TLS:\n{hosts}"
    );
    // each report names the string of the pod, the id that the pod reports, and the row of the observer
    for (check, image, id) in [
        (
            "mirror-image-id",
            "vendor.example/product:1.0",
            "vendor.example/product@sha256:aaaa",
        ),
        (
            "mirror-image-string",
            "strata-observer-registry:5000/product:1.0",
            "strata-observer-registry:5000/product@sha256:aaaa",
        ),
    ] {
        let report = std::fs::read_to_string(stage.path(&format!("out/{check}.out"))).unwrap();
        for line in [
            format!("spec.image:   {image}"),
            format!("imageID:      {id}"),
            format!("\"ref\": \"{image}\""),
        ] {
            assert!(report.contains(&line), "{check}: {line}\n{report}");
        }
    }
    // stage 8: the receiver, the Secret, the overlay with the component, the edit and its undo, and the end
    let address = "http://alert-receiver.default.svc.cluster.local:443/cgi-bin/hook";
    for call in [
        "-n default create configmap alert-receiver --from-file=hook=hook.cgi".to_string(),
        "-n default wait --for=condition=Ready pod/alert-receiver --timeout=120s".to_string(),
        format!(
            "-n strata-observer create secret generic strata-observer-alert-channels --from-literal=platform-oncall={address} --from-literal=platform-planning={address}"
        ),
        "kustomize .".to_string(),
        "apply --server-side -f observer.yaml".to_string(),
        "-n default exec alert-receiver -- cat /data/bodies.log".to_string(),
        "-n default delete pod,service,configmap alert-receiver --ignore-not-found".to_string(),
    ] {
        assert!(kubectl.contains(&call), "{call}:\n{kubectl}");
    }
    let rendered = std::fs::read_to_string(stage.path("out/observer.yaml")).unwrap();
    assert!(
        rendered.contains("- --alerts=/alerts/alerts.json"),
        "{rendered}"
    );
    let overlay: serde_yaml::Value = serde_yaml::from_str(
        &std::fs::read_to_string(stage.path("out/kustomization.yaml")).unwrap(),
    )
    .unwrap();
    assert_eq!(overlay["components"][0], "alerts");
    assert_eq!(overlay["resources"][0], "base");
    assert_eq!(overlay["namespace"], "strata-observer");
    assert_eq!(overlay["images"][0]["newName"], "strata-observer");
    assert_eq!(overlay["images"][0]["newTag"], "dev");
    let component: serde_yaml::Value = serde_yaml::from_str(
        &std::fs::read_to_string(stage.path("out/alerts/kustomization.yaml")).unwrap(),
    )
    .unwrap();
    assert_eq!(component["kind"], "Component");
    let patch = overlay["patches"][0]["patch"].as_str().unwrap();
    for flag in ["--content-every=1m", "--alerts-allow-http"] {
        assert!(patch.contains(flag), "{flag}:\n{patch}");
    }
    assert_eq!(
        std::fs::read_to_string(stage.path("out/alerts.json")).unwrap(),
        repo_file("examples/observer-alerts/alerts.json"),
        "the overlay carries the file of the example"
    );
    assert_eq!(
        observer_log(&stage, "api.log").lines().collect::<Vec<_>>(),
        [
            "edit ad-1 edited: \"no\" edited: \"yes\"",
            "edit ad-1 edited: \"yes\" edited: \"no\""
        ],
        "the edit goes in once and comes back once"
    );
    let out =
        |check: &str| std::fs::read_to_string(stage.path(&format!("out/{check}.out"))).unwrap();
    assert!(
        out("alert-open").starts_with("strata · payments · Drift\nstrata-live-addon@1.0.0: "),
        "{}",
        out("alert-open")
    );
    assert!(out("alert-clear").contains("Drift cleared\nstrata-live-addon@1.0.0\n"));
    assert!(
        out("alert-no-value")
            .contains("the detail route holds the text of the edit for addon/strata-live-config")
    );
    assert!(
        out("alert-one-each")
            .contains("strata-live-addon@1.0.0: 1 message of the open, 1 message of the clear")
    );
    assert!(
        !out("alert-route").contains("alert-receiver"),
        "the route shows the address"
    );
    // the last records say first-seen, with no revision
    let meta = observer_log(&stage, "meta.b64");

    let o = Command::new("bash")
        .arg("-c")
        .arg(format!("echo {} | base64 -d", meta.trim()))
        .output_locked()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&text(&o.stdout)).unwrap(),
        serde_json::json!({"baseline": "first-seen", "firstSeenFrom": "c-a"})
    );
}

#[test]
fn observer_kind_sh_fails_a_check_when_the_reader_can_read_a_secret_and_still_deletes_the_cluster()
{
    let mut stage = observer_kind_stage();
    stage.env("FAKE_READER_SECRETS", "1");
    let (o, steps) = run_kind(&stage);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    failed(&steps, &["reader-no-secret"]);
    passed(&steps, &["reader-no-exec", "reader-page", "kind-delete"]);
    assert!(observer_log(&stage, "kind.log").contains("delete cluster"));
}

#[test]
fn observer_kind_sh_fails_the_checks_of_a_pod_that_runs_as_root_with_a_token_and_a_writable_root() {
    let stage = observer_kind_stage();
    let mut pod = observer_pod();
    let spec = &mut pod["items"][0]["spec"];
    spec["securityContext"]["runAsNonRoot"] = false.into();
    spec["containers"][0]["securityContext"]["readOnlyRootFilesystem"] = false.into();
    spec["automountServiceAccountToken"] = true.into();
    spec["volumes"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"name": "kube-api-access-x7k2p", "projected": {}}));
    std::fs::write(stage.path("obs/pod.json"), pod.to_string()).unwrap();
    let (o, steps) = run_kind(&stage);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    failed(
        &steps,
        &["pod-non-root", "pod-read-only-root", "pod-no-token"],
    );
    passed(&steps, &["pod-ready", "pod-ready-status", "reader-page"]);
}

#[test]
fn observer_kind_sh_fails_the_refusal_when_a_pod_of_another_namespace_reaches_the_page() {
    let mut stage = observer_kind_stage();
    stage.env("FAKE_REACHED", "1");
    let (o, steps) = run_kind(&stage);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    failed(&steps, &["other-pod-refused"]);
}

/// The checks of observer-kind.sh that open a port-forward with `pf_start` of `lib.sh`.
const OBSERVER_KIND_FORWARDS: [&str; 3] = ["reader-forward", "component-read", "alert-route"];

#[test]
fn observer_kind_sh_waits_for_a_port_forward_that_prints_its_line_late() {
    // each port-forward prints its line half a second late. A wait of 30 looks ends before the line,
    // because the stand-in sleep returns at once
    let mut stage = observer_kind_stage();
    stage.env("FAKE_FORWARD_LATE", "0.5");
    let (o, steps) = run_kind(&stage);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    passed(&steps, &OBSERVER_KIND_CHECKS);
}

#[test]
fn observer_kind_sh_fails_each_port_forward_that_ends_with_no_line_and_says_why() {
    let mut stage = observer_kind_stage();
    stage.env("FAKE_FORWARD_FAILS", "1");
    let (o, steps) = run_kind(&stage);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    failed(&steps, &OBSERVER_KIND_FORWARDS);
    passed(&steps, &["reader-health", "alert-edit", "kind-delete"]);
    for check in OBSERVER_KIND_FORWARDS {
        let said = std::fs::read_to_string(stage.path(&format!("out/{check}.err"))).unwrap();
        for line in [
            "the port-forward ended with no line \"Forwarding from\". Its log:",
            "error: lost connection to pod",
        ] {
            assert!(said.contains(line), "{check}: {line}\n{said}");
        }
    }
}

#[test]
fn observer_kind_sh_keeps_the_cluster_when_keep_cluster_is_1_and_stops_without_the_image() {
    let mut stage = observer_kind_stage();
    stage.env("KEEP_CLUSTER", "1");
    let (o, steps) = run_kind(&stage);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    assert!(!observer_log(&stage, "kind.log").contains("delete"));
    assert!(steps.contains("KEEP_CLUSTER=1: the kind cluster strata-observer-test stays"));

    let mut stage = observer_kind_stage();
    stage.env("FAKE_NO_IMAGE", "1");
    let (o, _) = run_kind(&stage);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("no image strata-observer:dev; run just observer-image"),
        "{}",
        text(&o.stderr)
    );
    assert_eq!(observer_log(&stage, "kind.log"), "", "no cluster was made");
}

#[test]
fn observer_kind_sh_records_what_the_mirror_checks_find_and_judges_none_of_it() {
    // the row holds no digest: the check records 1 and the run still ends with 0
    let mut stage = observer_kind_stage();
    stage.env("FAKE_NO_DIGEST", "1");
    let (o, steps) = run_kind(&stage);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    for id in OBSERVER_KIND_SEEN {
        let line = step_line(&steps, id);
        assert!(
            line.starts_with("SEEN") && line.ends_with("expected ?, got 1"),
            "{line}"
        );
    }
    // the observer shows no row, or the pod is gone: the check records 2
    let mut stage = observer_kind_stage();
    stage.env("FAKE_NO_ROW", "1");
    let (o, steps) = run_kind(&stage);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    assert!(step_line(&steps, "mirror-image-id").ends_with("got 2"));
    let said = std::fs::read_to_string(stage.path("out/mirror-image-id.out")).unwrap();
    assert!(
        said.contains("the observer shows no row for vendor.example/product:1.0"),
        "{said}"
    );
}

#[test]
fn observer_kind_sh_fails_the_read_when_the_observer_says_not_read_and_still_cleans_up() {
    let mut stage = observer_kind_stage();
    stage.env("FAKE_PODS_UNREAD", "1");
    stage.env("CYCLE_WAIT", "2");
    let (o, steps) = run_kind(&stage);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    failed(&steps, &["component-read"]);
    passed(&steps, &["component-apply", "mirror-clean", "kind-delete"]);
}

#[test]
fn observer_kind_sh_fails_the_nodes_check_for_a_containerd_without_certs_d_and_still_removes_all() {
    let mut stage = observer_kind_stage();
    stage.env("FAKE_NO_CERTS_D", "1");
    let (o, steps) = run_kind(&stage);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    failed(&steps, &["mirror-nodes"]);
    passed(&steps, &["mirror-registry", "mirror-clean", "kind-delete"]);
    let said = std::fs::read_to_string(stage.path("out/mirror-nodes.out")).unwrap();
    assert!(
        said.contains("strata-observer-test-control-plane: containerd reads no folder certs.d"),
        "{said}"
    );
    assert!(
        !observer_log(&stage, "hosts.log").contains("hosts.toml"),
        "no file is written to such a node"
    );
}

#[test]
fn observer_kind_sh_removes_the_registry_and_the_cluster_when_the_script_ends_in_stage_7() {
    let mut stage = observer_kind_stage();
    stage.env("FAKE_DIE_AT_NODES", "1");
    let (o, steps) = run_kind(&stage);
    assert_ne!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    passed(&steps, &["mirror-registry"]);
    assert!(
        !steps.contains("mirror-clean"),
        "the script ended first:\n{steps}"
    );
    let docker = observer_log(&stage, "docker.log");
    assert_eq!(
        docker.lines().last(),
        Some("rm -f strata-observer-registry"),
        "the registry goes:\n{docker}"
    );
    assert!(
        observer_log(&stage, "kind.log").contains("delete cluster"),
        "the cluster goes"
    );
}

/// The edits that the stand-in `api.sh` of observer-kind.sh got, one line each.
fn kind_edits(stage: &Stage) -> Vec<String> {
    observer_log(stage, "api.log")
        .lines()
        .filter(|l| l.starts_with("edit "))
        .map(String::from)
        .collect()
}

const KIND_EDIT: &str = "edit ad-1 edited: \"no\" edited: \"yes\"";
const KIND_UNDO: &str = "edit ad-1 edited: \"yes\" edited: \"no\"";

#[test]
fn observer_kind_sh_fails_alert_no_value_when_a_message_holds_the_text_of_the_edit_and_puts_the_edit_back()
 {
    let mut stage = observer_kind_stage();
    stage.env("FAKE_VALUE_LEAKS", "1");
    let (o, steps) = run_kind(&stage);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    failed(&steps, &["alert-no-value"]);
    passed(
        &steps,
        &["alert-open", "alert-clear", "alert-clean", "kind-delete"],
    );
    let said = std::fs::read_to_string(stage.path("out/alert-no-value.out")).unwrap();
    assert!(said.contains("a message holds: edited: \"yes\""), "{said}");
    assert_eq!(kind_edits(&stage), [KIND_EDIT, KIND_UNDO]);
}

#[test]
fn observer_kind_sh_fails_alert_no_value_when_the_detail_route_holds_no_text_because_the_search_proves_nothing()
 {
    let mut stage = observer_kind_stage();
    stage.env("FAKE_NO_DIFF_TEXT", "1");
    let (o, steps) = run_kind(&stage);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    failed(&steps, &["alert-no-value"]);
    let said = std::fs::read_to_string(stage.path("out/alert-no-value.out")).unwrap();
    assert!(said.contains("so the search proves nothing"), "{said}");
}

#[test]
fn observer_kind_sh_fails_alert_open_and_alert_one_each_for_two_messages_of_one_finding() {
    let mut stage = observer_kind_stage();
    stage.env("FAKE_TWO_OPENS", "1");
    let (o, steps) = run_kind(&stage);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    failed(&steps, &["alert-open", "alert-one-each"]);
    passed(&steps, &["alert-clear"]);
}

#[test]
fn observer_kind_sh_fails_alert_open_and_alert_one_each_when_the_receiver_gets_no_message() {
    let mut stage = observer_kind_stage();
    stage.env("FAKE_NO_OPEN", "1");
    let (o, steps) = run_kind(&stage);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    failed(&steps, &["alert-open", "alert-one-each"]);
    assert_eq!(kind_edits(&stage), [KIND_EDIT, KIND_UNDO]);
}

#[test]
fn observer_kind_sh_fails_alert_clear_when_the_receiver_gets_no_message_of_the_clear() {
    let mut stage = observer_kind_stage();
    stage.env("FAKE_NO_CLEAR", "1");
    let (o, steps) = run_kind(&stage);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    failed(&steps, &["alert-clear", "alert-one-each"]);
    passed(&steps, &["alert-open", "alert-no-value", "alert-undo"]);
}

#[test]
fn observer_kind_sh_fails_alert_apply_for_a_render_without_the_flag_and_alert_route_for_a_page_that_is_not_set()
 {
    let mut stage = observer_kind_stage();
    stage.env("FAKE_NO_ALERTS_FLAG", "1");
    stage.env("FAKE_ALERTS_UNSET", "1");
    let (o, steps) = run_kind(&stage);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    failed(&steps, &["alert-apply", "alert-route"]);
}

#[test]
fn observer_kind_sh_puts_the_edit_back_when_the_script_ends_with_the_edit_open() {
    let mut stage = observer_kind_stage();
    stage.env("FAKE_DIE_AFTER_EDIT", "1");
    let (o, steps) = run_kind(&stage);
    assert_ne!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    assert!(
        !steps.contains("alert-open"),
        "the script ended first:\n{steps}"
    );
    assert_eq!(
        kind_edits(&stage),
        [KIND_EDIT, KIND_UNDO],
        "the edit goes back"
    );
    assert!(
        observer_log(&stage, "kind.log").contains("delete cluster"),
        "the cluster goes"
    );
}

#[test]
fn observer_kind_sh_runs_from_stage_8_and_names_the_nine_stages() {
    let script = repo_file("examples/live/observer-kind.sh");
    for stage in 1..=9 {
        assert!(
            script.contains(&format!("\n#   {stage}  "))
                && script.contains(&format!("if stage {stage} \"")),
            "stage {stage}"
        );
    }
    assert!(script.contains("case \"$from\" in 1|2|3|4|5|6|7|8|9) ;;"));
}

// ---- observer-images.sh ------------------------------------------------------------------------------

/// The `api.sh` of the tree of observer-images.sh. It logs each call to `$FAKE_OBS/api.log`. The
/// values of the add-on pack are in `$FAKE_OBS/values.txt`. With `FAKE_DIE_AFTER_EDIT`, the first
/// edit that sets values ends the script with SIGTERM.
const FAKE_API_IMAGES: &str = r##"#!/usr/bin/env bash
obs=$FAKE_OBS
echo "$*" >> "$obs/api.log"
case "$1" in
  get-kubeconfig) echo "apiVersion: v1 # fake-kubeconfig" ;;
  set-values)
    printf '%s' "$4" > "$obs/values.txt"
    echo "set the values of $3 in $2 in place"
    [ -z "${FAKE_DIE_AFTER_EDIT-}" ] || [ -z "$4" ] || kill -TERM "$PPID" ;;
esac
exit 0
"##;

/// The `strata` of observer-images.sh. A record of the add-on profile lists the images of
/// `values.txt` in its pack. Each other call goes to the answers that the stage gave.
const FAKE_STRATA_IMAGES: &str = r##"#!/usr/bin/env bash
out=""; prev=""
for a in "$@"; do [ "$prev" = -o ] && out=$a; prev=$a; done
case "$*" in
  "snapshot ad-1 -o "*)
    echo "$*" >> "$FAKE_STRATA/calls"
    images=$(sed -n 's/^ *- image: //p' "$FAKE_OBS/values.txt" 2> /dev/null | jq -R . | jq -cs .)
    jq -n --argjson i "$images" '{schema: "strata-snapshot/1",
      source: {uid: "ad-1", name: "strata-live-addon", version: "1.0.0", type: "add-on"},
      packs: [{key: "addon/strata-live-config", layer: "addon", name: "strata-live-config", images: $i}]}' > "$out" ;;
  *) exec "$(dirname "$0")/fake-strata-rules" "$@" ;;
esac
"##;

/// The `crane` of observer-images.sh. A registry is a folder `$FAKE_OBS/registry`, with a digest for
/// each name. The digest of a copy is the sha256 of the name of its source, so two sources give two
/// digests. With `FAKE_TAG_STAYS`, a copy to a name that holds an image changes nothing.
const FAKE_CRANE_IMAGES: &str = r##"#!/usr/bin/env bash
obs=$FAKE_OBS
echo "$*" >> "$obs/crane.log"
file="$obs/registry/$(printf %s "${3:-$2}" | tr '/:' '__')"
case "$1" in
  copy)
    mkdir -p "$obs/registry"
    echo "copy $2 $3" >> "$obs/events.log"
    [ -z "${FAKE_TAG_STAYS-}" ] || [ ! -f "$file" ] || exit 0
    echo "sha256:$(printf %s "$2" | sha256sum | cut -d' ' -f1)" > "$file" ;;
  digest)
    [ -f "$file" ] || { echo "MANIFEST_UNKNOWN: manifest unknown" >&2; exit 1; }
    cat "$file" ;;
esac
"##;

/// The `kubectl` of observer-images.sh. It logs each call to `$FAKE_OBS/kubectl.log`. The state of the
/// cluster is in files of `$FAKE_OBS`. A pod made with `apply -f -` gets a file in `pods/`. Its image
/// id is the digest that the registry holds at that time, as a pull with Always gives. The pod
/// `absent` waits for an image it cannot pull, and the pod `crash` has restarted.
///
/// The role of the observer gets and lists pods. `FAKE_ROLE_WIDE` adds a verb to the role.
/// `FAKE_NEVER_HAS_ID` gives the pod that cannot pull an image id. `FAKE_POLICY_STAYS` keeps
/// `IfNotPresent`. A port-forward runs [`FAKE_PORT_FORWARD`].
const FAKE_KUBECTL_IMAGES: &str = r##"#!/usr/bin/env bash
obs=$FAKE_OBS
echo "$*" >> "$obs/kubectl.log"
notfound() { echo "Error from server (NotFound): $1 \"$2\" not found"; exit 1; }
if [ "$1 $2" = "auth can-i" ]; then
  verb=$3; resource=$4
  if [ "$resource" = pods ] && { [ "$verb" = get ] || [ "$verb" = list ] || [ -n "${FAKE_ROLE_WIDE-}" ]; }; then
    echo yes; exit 0
  fi
  echo no; exit 1
fi
case "$*" in
  "kustomize .")
    [ -f kustomization.yaml ] && [ -d base ] && [ -d pods ] && grep -q '^components:' kustomization.yaml \
      || { echo "no overlay here" >&2; exit 1; }
    for f in namespace serviceaccount networkpolicy deployment; do cat "base/$f.yaml"; echo "---"; done
    for f in clusterrole clusterrolebinding; do cat "pods/$f.yaml"; echo "---"; done
    cat pods/kustomization.yaml ;;
  *"apply -f "*"namespace.yaml"*) touch "$obs/ns.strata-observer" ;;
  *"create namespace strata-live-images"*) touch "$obs/ns.strata-live-images" ;;
  *"create configmap strata-observer"*) touch "$obs/config" ;;
  *"create secret generic strata-observer-key"*) cat > "$obs/key.seen" ;;
  *"create secret generic ghcr-pull"*) cat > "$obs/pull.seen" ;;
  *"apply --server-side -f -"*)
    cat > "$obs/records.yaml"
    cp "$obs/values.txt" "$obs/records.values" 2> /dev/null || : > "$obs/records.values"
    touch "$obs/records.applied" ;;
  *"apply -f observer.yaml"*)
    [ -f observer.yaml ] || exit 1
    touch "$obs/observer.applied" "$obs/role.exists" "$obs/ns.strata-observer" ;;
  *"apply -f -"*)
    body=$(cat)
    name=$(sed -n 's/^  name: //p' <<<"$body" | head -n 1)
    image=$(sed -n 's/^      image: //p' <<<"$body")
    asked=$(sed -n 's/^      imagePullPolicy: //p' <<<"$body")
    digest=$(cat "$obs/registry/$(printf %s "$image" | tr '/:' '__')" 2> /dev/null)
    [ -n "$digest" ] || digest="sha256:$(printf %s "$image" | sha256sum | cut -d' ' -f1)"
    id="${image%:*}@$digest"; restarts=0; waiting=""
    if [ "$name" = absent ]; then
      id=""; waiting=ImagePullBackOff
      [ -z "${FAKE_NEVER_HAS_ID-}" ] || id="${image%:*}@sha256:ffff"
    fi
    [ "$name" != crash ] || restarts=3
    policy=Always; [ -z "${FAKE_POLICY_STAYS-}" ] || policy=$asked
    mkdir -p "$obs/pods"
    echo "pod $name $digest" >> "$obs/events.log"
    jq -n --arg n "$name" --arg i "$image" --arg id "$id" --arg p "$policy" --argjson r "$restarts" --arg w "$waiting" \
      '{name: $n, namespace: "strata-live-images", image: $i, imageID: $id, policy: $p, restarts: $r, waiting: $w}' \
      > "$obs/pods/$name.json" ;;
  *"get pod "*" -o json"*)
    name=$(sed -E 's/.* get pod ([a-z0-9-]+) .*/\1/' <<<"$*")
    [ -f "$obs/pods/$name.json" ] || notfound pods "$name"
    jq '{spec: {containers: [{name: "c", image: .image, imagePullPolicy: .policy}]},
         status: {containerStatuses: [{name: "c", image: .image, imageID: .imageID, restartCount: .restarts,
           state: (if .waiting != "" then {waiting: {reason: .waiting}} else {running: {}} end)}]}}' "$obs/pods/$name.json" ;;
  *"get pods --all-namespaces -o json"*)
    { cat "$obs/agents.json"; cat "$obs"/pods/*.json 2> /dev/null; } \
      | jq -s '(.[0] + .[1:]) | {items: map({metadata: {name, namespace}, spec: {containers: [{name: "c", image}]}})}' ;;
  *"delete pod "*)
    echo "delete $(sed -E 's/.*delete pod ([a-z0-9-]+).*/\1/' <<<"$*")" >> "$obs/events.log"
    rm -f "$obs/pods/$(sed -E 's/.*delete pod ([a-z0-9-]+).*/\1/' <<<"$*").json" ;;
  *"delete namespace strata-live-images"*) rm -rf "$obs/ns.strata-live-images" "$obs/pods" ;;
  *"delete -f observer.yaml"*|*"delete namespace strata-observer"*)
    rm -f "$obs/observer.applied" "$obs/role.exists" "$obs/ns.strata-observer" ;;
  *"delete clusterrole"*) rm -f "$obs/role.exists" ;;
  *"get namespace "*)
    n=${*##*get namespace }; n=${n%% *}
    [ -f "$obs/ns.$n" ] || notfound namespaces "$n"
    echo "$n Active" ;;
  *"get clusterrole"*) [ -f "$obs/role.exists" ] || notfound clusterroles strata-observer-pods; echo found ;;
  *port-forward*) exec fake-port-forward ;;
esac
exit 0
"##;

/// The `curl` of observer-images.sh: the observer, from the pods of `$FAKE_OBS`. It answers as the
/// observer does, once the observer is applied. The first digest of an image is the first one that a
/// call saw. A row is split with two digests, moved with one digest that is not the first, steady with
/// the first, and not checked with none. An image matches when the records listed it. `FAKE_NOT_READ`
/// says that the observer did not read the pods, and `FAKE_NONE_FINDING` makes a finding name an image
/// of an agent.
const FAKE_CURL_IMAGES: &str = r##"#!/usr/bin/env bash
obs=$FAKE_OBS
url=${*: -1}
case "$url" in http://127.0.0.1:*) ;; *) echo "curl: no network" >&2; exit 7 ;; esac
[ -f "$obs/observer.applied" ] || exit 7
path=/${url#http://127.0.0.1:*/}
containers=$({ cat "$obs/agents.json"; cat "$obs"/pods/*.json 2> /dev/null; } | jq -s '.[0] + .[1:]')
values=$(sed -n 's/^ *- image: //p' "$obs/records.values" 2> /dev/null | jq -R . | jq -cs .)
first=$(jq -c --argjson c "$containers" 'reduce ($c | group_by(.image)[]) as $g (.;
  $g[0].image as $ref
  | if has($ref) then . else
      ([$g[].imageID | select(contains("@sha256:")) | split("@")[1]] | sort | first) as $d
      | if $d then . + {($ref): $d} else . end
    end)' "$obs/first.json" 2> /dev/null || echo '{}')
[ -n "$first" ] || first='{}'
echo "$first" > "$obs/first.json"
rows=$(jq -c --argjson f "$first" --argjson v "$values" '
  group_by(.image) | map(
    .[0].image as $ref
    | (map(.imageID) | group_by(.) | map({id: .[0], n: length})) as $ids
    | ([$ids[] | select(.id | contains("@sha256:")) | {digest: (.id | split("@")[1]), pods: .n}]) as $real
    | ($f[$ref] // null) as $want
    | ((($v | index($ref)) != null) | if . then "exact" else "none" end) as $match
    | {ref: $ref,
       status: (if ($real | length) >= 2 then "split"
                elif ($real | length) == 1 and $real[0].digest != $want then "moved"
                elif ($real | length) == 1 then "steady" else "not-checked" end),
       match: $match, byDigest: ($ref | contains("@sha256")),
       packs: (if $match == "exact" then [{profile: "strata-live-addon@1.0.0", key: "addon/strata-live-config"}] else [] end),
       digests: ([$real[] | {digest, pods, status: (if .digest == $want then "first-digest" else "another-digest" end)}]
                 + [$ids[] | select(.id == "") | {status: "not-started", pods: .n}])})' <<<"$containers")
case "$path" in
  /v1/images)
    if [ -n "${FAKE_NOT_READ-}" ]; then
      echo '{"schema":"strata-observed/1","pods":"not-read","readAt":null,"error":{"kind":"kube-api","status":403},"counts":{"images":0,"matched":0,"changed":0},"images":[]}'
    else
      jq -c '{schema: "strata-observed/1", pods: "read", readAt: "2026-11-04T10:00:00Z",
        counts: {images: length, matched: ([.[] | select(.match != "none")] | length), changed: 0}, images: .}' <<<"$rows"
    fi ;;
  /v1/images/*)
    enc=${path#/v1/images/}
    ref=$(printf '%b' "${enc//%/\\x}")
    row=$(jq -c --arg r "$ref" '.[] | select(.ref == $r)' <<<"$rows")
    [ -n "$row" ] || exit 22
    jq -c '{schema: "strata-observed/1", pods: "read", image: .}' <<<"$row" ;;
  /v1/findings)
    jq -c --arg none "${FAKE_NONE_FINDING:+registry.k8s.io/coredns/coredns:v1.11.3}" '{findings: ([.[] | select((.status == "moved" or .status == "split") and .match == "exact" and (.byDigest | not))]
      | group_by(.status)
      | map({kind: ("image-" + .[0].status), profile: "strata-live-addon@1.0.0", packs: ["addon/strata-live-config"], images: map(.ref)})
      | . + (if $none != "" then [{kind: "image-moved", profile: "strata-live-addon@1.0.0", packs: [], images: [$none]}] else [] end))}' <<<"$rows" ;;
  *) exit 22 ;;
esac
"##;

/// The agents of Palette in the namespace of a cluster, and one pod of the system. The observer reads
/// them with the pods of the run.
const AGENT_PODS: &str = r#"[
  {"name":"crony-0","namespace":"cluster-0123456789abcdef01234567","image":"gcr.io/spectro-images-public/release/crony:4.1.0","imageID":"gcr.io/spectro-images-public/release/crony@sha256:1111111111111111111111111111111111111111111111111111111111111111"},
  {"name":"capi-0","namespace":"capi-system","image":"registry.k8s.io/cluster-api/cluster-api-controller:v1.9.0","imageID":"registry.k8s.io/cluster-api/cluster-api-controller@sha256:2222222222222222222222222222222222222222222222222222222222222222"},
  {"name":"coredns-0","namespace":"kube-system","image":"registry.k8s.io/coredns/coredns:v1.11.3","imageID":"registry.k8s.io/coredns/coredns@sha256:3333333333333333333333333333333333333333333333333333333333333333"}
]"#;

/// A stage for observer-images.sh. The script runs in a tree of its own, so that its `api.sh`,
/// `tf.sh`, and `baseline.sh` are stand-ins. Every command outside the repository is a stand-in too.
/// Cluster A is `c-a`. It runs the release profile `r-1` and the add-on profile `ad-1`.
fn images_stage() -> Stage {
    let mut stage = Stage::new(baseline_outputs());
    let tree = stage.path("tree");
    let mut files = vec![
        "examples/live/observer-images.sh".to_string(),
        "examples/live/lib.sh".to_string(),
        "examples/live/common.sh".to_string(),
        "scripts/ci/observer-records.sh".to_string(),
        "Cargo.toml".to_string(),
    ];
    for file in [
        "kustomization",
        "namespace",
        "serviceaccount",
        "networkpolicy",
        "deployment",
        "reader-role",
        "pods/kustomization",
        "pods/clusterrole",
        "pods/clusterrolebinding",
    ] {
        files.push(format!("deploy/observer/{file}.yaml"));
    }
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    copy_scripts(&tree, &files);
    let live = tree.join("examples/live");
    executable(&live.join("api.sh"), FAKE_API_IMAGES);
    executable(
        &live.join("tf.sh"),
        "#!/usr/bin/env bash\ncat \"$FAKE_OUTPUTS\"\n",
    );
    executable(&live.join("baseline.sh"), "#!/usr/bin/env bash\nexit 0\n");
    executable(&live.join("diagnose.sh"), "#!/usr/bin/env bash\nexit 0\n");

    let obs = stage.path("obs");
    std::fs::create_dir_all(&obs).unwrap();
    std::fs::write(obs.join("agents.json"), AGENT_PODS).unwrap();
    let bin = stage.path("obs-bin");
    std::fs::create_dir_all(&bin).unwrap();
    for (name, body) in [
        ("kubectl", FAKE_KUBECTL_IMAGES),
        ("curl", FAKE_CURL_IMAGES),
        ("crane", FAKE_CRANE_IMAGES),
        // the script never calls docker: the log of this stand-in stays empty
        (
            "docker",
            "#!/usr/bin/env bash\necho \"$*\" >> \"$FAKE_OBS/docker.log\"\nexit 1\n",
        ),
        (
            "gh",
            "#!/usr/bin/env bash\ncase \"$*\" in\n  \"api user\"*) echo tester ;;\nesac\n",
        ),
        (
            "git",
            "#!/usr/bin/env bash\necho 0123456789abcdef0123456789abcdef01234567\n",
        ),
    ] {
        executable(&bin.join(name), body);
    }
    stage.wrap_strata(FAKE_STRATA_IMAGES);
    stage.strata(
        "snapshot r-1 -o *",
        0,
        &observer_profile_record("r-1", "strata-live-profile", "cluster"),
    );
    let path = format!("{}:{}", bin.display(), path_with(&stage.path("bin")));
    stage.env("PATH", &path);
    // the token file is outside the folder of the run, so that the look for it in the run is fair
    let token_file = stage.path("ghcr-pull-token");
    write_with_mode(&token_file, &format!("{OBSERVER_PULL_TOKEN}\n"), 0o600);
    stage.env("GHCR_PULL_TOKEN_FILE", token_file.to_str().unwrap());
    stage.env("PALETTE_API_KEY", OBSERVER_KEY);
    stage.env("FAKE_OBS", obs.to_str().unwrap());
    stage.env("CYCLE_TRIES", "3");
    stage
}

/// Run observer-images.sh of the tree of `stage`.
fn run_images(stage: &Stage) -> (Output, String) {
    let script = stage.path("tree/examples/live/observer-images.sh");
    stage.run(script.to_str().unwrap(), &[])
}

/// Each check of observer-images.sh, in the order that it runs on a good run.
const IMAGES_CHECKS: [&str; 21] = [
    "pull-secret",
    "tag-first",
    "values-edit",
    "namespaces",
    "settings",
    "records",
    "observer-apply",
    "observer-ready",
    "pods-role",
    "pods-read",
    "image-first",
    "image-split",
    "image-moved",
    "image-never-started",
    "image-crash-loop",
    "image-agents-no-finding",
    "pull-policy",
    "undo-values",
    "undo-namespaces",
    "undo-role",
    "undo-baseline",
];

/// The sha256 of `text` in hex: the digest that the stand-in `crane` gives a copy of the image `text`.
fn sha256_of(text: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The suffix of the names of the images of a run.
fn images_suffix(stage: &Stage) -> String {
    std::fs::read_to_string(stage.path("out/ids.env"))
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("suffix=").map(str::to_string))
        .expect("ids.env holds the suffix")
}

#[test]
fn observer_images_sh_runs_the_stages_in_order_and_each_check_passes() {
    let stage = images_stage();
    let (o, steps) = run_images(&stage);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{steps}{}{}",
        text(&o.stdout),
        text(&o.stderr)
    );
    assert!(!steps.contains("FAIL"), "{steps}");
    assert_eq!(check_ids(&steps), IMAGES_CHECKS, "{steps}");
    let line = step_line(&steps, "pull-policy");
    assert!(
        line.starts_with("SEEN") && line.ends_with("expected ?, got 0"),
        "{line}"
    );
}

/// The tag holds the first image when the first pod starts. It moves to the second image, and a new
/// pod pulls it. Then the first pod is replaced, and the new pod pulls the second image too. The
/// digests are those of the registry, and the page of the observer shows them.
#[test]
fn observer_images_sh_moves_the_tag_between_the_pods_and_shows_the_digests_of_the_registry() {
    let stage = images_stage();
    let (o, steps) = run_images(&stage);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    let suffix = images_suffix(&stage);
    let moving = format!("ttl.sh/strata-live-{suffix}:2h");
    let (d1, d2) = (
        format!("sha256:{}", sha256_of("registry.k8s.io/pause:3.9")),
        format!("sha256:{}", sha256_of("registry.k8s.io/pause:3.10")),
    );
    assert_ne!(d1, d2);
    let events = observer_log(&stage, "events.log");
    let moves: Vec<&str> = events
        .lines()
        .filter(|l| {
            l.starts_with("copy registry.k8s.io/pause:3")
                || l.starts_with("pod p1 ")
                || l.starts_with("pod p2 ")
                || *l == "delete p1"
        })
        .collect();
    assert_eq!(
        moves,
        [
            format!("copy registry.k8s.io/pause:3.9 {moving}"),
            format!("pod p1 {d1}"),
            format!("copy registry.k8s.io/pause:3.10 {moving}"),
            format!("pod p2 {d2}"),
            "delete p1".to_string(),
            format!("pod p1 {d2}"),
        ],
        "the tag moves between the pods"
    );

    let said =
        |check: &str| std::fs::read_to_string(stage.path(&format!("out/{check}.out"))).unwrap();
    let first = said("image-first");
    assert!(
        first.contains(&d1) && first.contains("first-digest") && first.contains("\"steady\""),
        "{first}"
    );
    let split = said("image-split");
    assert!(
        split.contains(&d1) && split.contains(&d2) && split.contains("\"split\""),
        "{split}"
    );
    assert!(
        split.contains("another-digest") && split.contains("first-digest"),
        "{split}"
    );
    let moved = said("image-moved");
    assert!(
        moved.contains(&d2) && moved.contains("\"moved\"") && !moved.contains(&d1),
        "{moved}"
    );
    let never = said("image-never-started");
    assert!(
        never.contains("not-started") && never.contains("not-checked"),
        "{never}"
    );
    let crash = said("image-crash-loop");
    assert!(
        crash.contains("\"restartCount\":3") && crash.contains("\"steady\""),
        "{crash}"
    );
    let agents = said("image-agents-no-finding");
    assert!(
        agents.contains("3 images match no pack, and no finding names one"),
        "{agents}"
    );
    assert!(
        said("pull-policy").contains("asked IfNotPresent, the cluster set Always"),
        "{}",
        said("pull-policy")
    );
}

#[test]
fn observer_images_sh_lists_the_three_images_in_the_values_of_the_add_on_and_puts_them_back() {
    let stage = images_stage();
    let (o, steps) = run_images(&stage);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    let suffix = images_suffix(&stage);
    let api = observer_log(&stage, "api.log");
    let edits: Vec<&str> = api.split("set-values ").skip(1).collect();
    assert_eq!(edits.len(), 2, "one edit and one undo:\n{api}");
    for image in [
        format!("ttl.sh/strata-live-{suffix}:2h"),
        format!("ttl.sh/strata-live-{suffix}-absent:2h"),
        format!("ttl.sh/strata-live-{suffix}-crash:2h"),
    ] {
        assert!(
            edits[0].contains(&format!("- image: {image}")),
            "{image}:\n{}",
            edits[0]
        );
    }
    assert!(
        edits[0].starts_with("ad-1 strata-live-config pack:"),
        "{}",
        edits[0]
    );
    assert_eq!(
        edits[1].lines().next().map(str::trim_end),
        Some("ad-1 strata-live-config"),
        "the undo gives no values"
    );
    assert_eq!(
        observer_log(&stage, "values.txt"),
        "",
        "the values are back"
    );
    // the record that the observer got lists the tag, and the records apply with --server-side
    assert!(
        observer_log(&stage, "records.values")
            .contains(&format!("- image: ttl.sh/strata-live-{suffix}:2h")),
        "the add-on record lists the tag"
    );
    let kubectl = observer_log(&stage, "kubectl.log");
    assert!(
        kubectl.lines().any(|l| l == "apply --server-side -f -"),
        "{kubectl}"
    );
}

#[test]
fn observer_images_sh_installs_the_observer_with_the_component_and_the_pull_secret() {
    let mut stage = images_stage();
    stage.env(
        "OBSERVER_IMAGE",
        "ghcr.io/como-technologies/strata-observer:test-abc1234",
    );
    let (o, steps) = run_images(&stage);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    let overlay = std::fs::read_to_string(stage.path("out/kustomization.yaml")).unwrap();
    for line in [
        "components:\n  - pods\n",
        "newName: ghcr.io/como-technologies/strata-observer\n",
        "newTag: test-abc1234\n",
        "- name: ghcr-pull\n",
        "value: --content-every=1m",
    ] {
        assert!(overlay.contains(line), "{line:?}:\n{overlay}");
    }
    assert!(
        !overlay.contains("ttl.sh"),
        "the observer does not come from ttl.sh:\n{overlay}"
    );
    // the folders of the overlay are copies of the base and of the component
    for file in [
        "base/deployment.yaml",
        "pods/kustomization.yaml",
        "pods/clusterrole.yaml",
        "pods/clusterrolebinding.yaml",
    ] {
        assert_eq!(
            std::fs::read_to_string(stage.path(&format!("out/{file}"))).unwrap(),
            repo_file(&format!("deploy/observer/{}", file.replace("base/", ""))),
            "{file}"
        );
    }
    // the pull Secret holds the token of the file, through standard input, and no file shows it
    let pull = observer_log(&stage, "pull.seen");
    let auths: serde_json::Value = serde_json::from_str(&pull).unwrap();
    assert_eq!(
        auths["auths"]["ghcr.io"]["password"], OBSERVER_PULL_TOKEN,
        "{pull}"
    );
    let kubectl = observer_log(&stage, "kubectl.log");
    assert!(
        kubectl.contains("create secret generic ghcr-pull --type=kubernetes.io/dockerconfigjson"),
        "{kubectl}"
    );
    // the key reaches the Secret through standard input, and no other file or call shows it
    assert_eq!(observer_log(&stage, "key.seen"), OBSERVER_KEY);
    let mut shown = Vec::new();
    for dir in ["out", "obs"] {
        for (file, content) in files_under(&stage.path(dir)) {
            if !["key.seen", "pull.seen"].contains(&file.as_str())
                && (content.contains(OBSERVER_KEY) || content.contains(OBSERVER_PULL_TOKEN))
            {
                shown.push(format!("{dir}/{file}"));
            }
        }
    }
    assert!(shown.is_empty(), "a secret is in {shown:?}");
    // the kubeconfig is a credential: no file of it stays
    assert_eq!(
        std::fs::read_dir(stage.path("tmp")).unwrap().count(),
        0,
        "a temporary file stays"
    );
}

/// The image of the observer is on a private package. The script never tags it, never pushes it, and
/// never copies it: docker is not called, and `crane` copies public images to `ttl.sh` only.
#[test]
fn observer_images_sh_puts_the_image_of_the_observer_on_no_registry() {
    let mut stage = images_stage();
    stage.env(
        "OBSERVER_IMAGE",
        "ghcr.io/como-technologies/strata-observer:test-abc1234",
    );
    let (o, steps) = run_images(&stage);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    assert_eq!(observer_log(&stage, "docker.log"), "", "docker runs");
    let suffix = images_suffix(&stage);
    let crane = observer_log(&stage, "crane.log");
    let copies: Vec<&str> = crane.lines().filter(|l| l.starts_with("copy ")).collect();
    assert_eq!(
        copies,
        [
            format!("copy registry.k8s.io/pause:3.9 ttl.sh/strata-live-{suffix}:2h"),
            format!("copy docker.io/library/busybox:1.37 ttl.sh/strata-live-{suffix}-crash:2h"),
            format!("copy registry.k8s.io/pause:3.10 ttl.sh/strata-live-{suffix}:2h"),
        ],
        "only public images go to ttl.sh"
    );
    for call in crane.lines() {
        assert!(
            !call.contains("strata-observer"),
            "crane touches the observer: {call}"
        );
    }
    // the code of the script has no command that tags, pushes, or copies an image of its own
    let script = repo_file("examples/live/observer-images.sh");
    let code: Vec<&str> = script
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect();
    for line in &code {
        assert!(!line.contains("docker "), "docker in the code: {line}");
        for verb in ["push", "append", "mutate", "pull", "export"] {
            assert!(
                !line.contains(&format!("crane {verb}")),
                "crane {verb} in the code: {line}"
            );
        }
        if line.contains("crane copy") {
            assert!(
                line.contains("crane copy \"$source_"),
                "crane copies a source that is not a public image: {line}"
            );
        }
    }
}

/// A token file that fails stops the run at the pull Secret, before any change. The run makes no copy to
/// `ttl.sh`, no edit of the add-on, and no call to the cluster.
#[test]
fn observer_images_sh_stops_at_the_pull_secret_before_it_changes_anything() {
    for (why, body, mode) in [
        ("absent", None, 0o600),
        ("empty", Some(""), 0o600),
        ("open to others", Some(OBSERVER_PULL_TOKEN), 0o644),
    ] {
        let stage = images_stage();
        let path = stage.path("ghcr-pull-token");
        match body {
            Some(body) => write_with_mode(&path, body, mode),
            None => std::fs::remove_file(&path).unwrap(),
        }
        let (o, steps) = run_images(&stage);
        assert_eq!(
            o.status.code(),
            Some(1),
            "{why}:\n{steps}{}",
            text(&o.stderr)
        );
        assert_eq!(check_ids(&steps), ["pull-secret"], "{why}:\n{steps}");
        failed(&steps, &["pull-secret"]);
        let said = std::fs::read_to_string(stage.path("out/pull-secret.err")).unwrap();
        assert!(
            said.contains(path.to_str().unwrap()) && said.contains("read:packages"),
            "{why}: the line names the file and the scope:\n{said}"
        );
        for log in [
            "kubectl.log",
            "api.log",
            "crane.log",
            "docker.log",
            "pull.seen",
        ] {
            assert_eq!(observer_log(&stage, log), "", "{why}: {log} is not empty");
        }
        assert!(
            !text(&o.stdout).contains(OBSERVER_PULL_TOKEN)
                && !all_text(&stage.path("out")).contains(OBSERVER_PULL_TOKEN),
            "{why}: the token is in an output"
        );
    }
}

/// The page names the images that match no pack. A cluster with no such image fails the check, and
/// so does a finding that names one.
#[test]
fn observer_images_sh_takes_the_images_of_the_agents_from_the_answer_of_the_page() {
    let stage = images_stage();
    std::fs::write(stage.path("obs/agents.json"), "[]").unwrap();
    let (o, steps) = run_images(&stage);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    failed(&steps, &["image-agents-no-finding"]);
    let said = std::fs::read_to_string(stage.path("out/image-agents-no-finding.out")).unwrap();
    assert!(said.contains("no image matches no pack"), "{said}");

    // the check reads the page: it names no namespace, and the script keeps no list of them
    let script = repo_file("examples/live/observer-images.sh");
    assert!(
        !script.contains("AGENT_NAMESPACES") && !script.contains("agent_namespaces"),
        "the script keeps a list of namespaces"
    );
    let stage = images_stage();
    let (o, steps) = run_images(&stage);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    let list = std::fs::read_to_string(stage.path("out/agent-images.txt")).unwrap();
    assert_eq!(
        list.lines().collect::<Vec<_>>(),
        [
            "gcr.io/spectro-images-public/release/crony:4.1.0",
            "registry.k8s.io/cluster-api/cluster-api-controller:v1.9.0",
            "registry.k8s.io/coredns/coredns:v1.11.3",
        ]
    );
}

#[test]
fn observer_images_sh_removes_what_it_made_and_checks_that_it_is_gone() {
    let stage = images_stage();
    let (o, steps) = run_images(&stage);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    let kubectl = observer_log(&stage, "kubectl.log");
    for call in [
        "delete namespace strata-live-images --ignore-not-found",
        "delete -f observer.yaml --ignore-not-found",
        "delete namespace strata-observer --ignore-not-found",
        "delete clusterrolebinding strata-observer-pods --ignore-not-found",
        "delete clusterrole strata-observer-pods --ignore-not-found",
    ] {
        assert!(kubectl.contains(call), "{call}:\n{kubectl}");
    }
    for state in [
        "ns.strata-observer",
        "ns.strata-live-images",
        "role.exists",
        "pods",
    ] {
        assert!(!stage.path("obs").join(state).exists(), "{state} stays");
    }
}

#[test]
fn observer_images_sh_undoes_the_run_when_the_script_ends_in_stage_1() {
    let mut stage = images_stage();
    stage.env("FAKE_DIE_AFTER_EDIT", "1");
    let (o, steps) = run_images(&stage);
    assert_ne!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    passed(&steps, &["tag-first"]);
    assert!(
        !steps.contains("namespaces"),
        "the script ended first:\n{steps}"
    );
    assert_eq!(
        observer_log(&stage, "values.txt"),
        "",
        "the values are back"
    );
    let kubectl = observer_log(&stage, "kubectl.log");
    assert!(
        kubectl.contains("delete namespace strata-live-images --ignore-not-found")
            && kubectl.contains("delete clusterrole strata-observer-pods --ignore-not-found"),
        "{kubectl}"
    );
}

#[test]
fn observer_images_sh_keeps_the_run_with_keep_1_and_refuses_an_image_with_no_tag() {
    let mut stage = images_stage();
    stage.env("KEEP", "1");
    let (o, steps) = run_images(&stage);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    assert!(
        steps.contains("KEEP=1: the observer, the pods, and the edit stay"),
        "{steps}"
    );
    assert!(!steps.contains("undo-values"), "{steps}");
    assert!(stage.path("obs/observer.applied").exists());
    assert_ne!(observer_log(&stage, "values.txt"), "", "the edit stays");

    let mut stage = images_stage();
    stage.env(
        "OBSERVER_IMAGE",
        "ghcr.io/como-technologies/strata-observer",
    );
    let (o, steps) = run_images(&stage);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("OBSERVER_IMAGE needs a tag or a digest"),
        "{}",
        text(&o.stderr)
    );
    assert!(steps.is_empty(), "no stage ran: {steps}");
    assert_eq!(
        observer_log(&stage, "kubectl.log"),
        "",
        "nothing changed in the cluster"
    );
}

/// Each fault below makes one check fail, and the run still removes what it made.
#[test]
fn observer_images_sh_fails_the_check_that_a_fault_breaks_and_still_undoes_the_run() {
    for (fault, check) in [
        ("FAKE_NOT_READ", "pods-read"),
        ("FAKE_ROLE_WIDE", "pods-role"),
        ("FAKE_TAG_STAYS", "image-split"),
        ("FAKE_NEVER_HAS_ID", "image-never-started"),
        ("FAKE_NONE_FINDING", "image-agents-no-finding"),
    ] {
        let mut stage = images_stage();
        stage.env(fault, "1");
        let (o, steps) = run_images(&stage);
        assert_eq!(
            o.status.code(),
            Some(1),
            "{fault}: {steps}{}",
            text(&o.stderr)
        );
        failed(&steps, &[check]);
        passed(
            &steps,
            &[
                "undo-values",
                "undo-namespaces",
                "undo-role",
                "undo-baseline",
            ],
        );
        assert_eq!(
            observer_log(&stage, "values.txt"),
            "",
            "{fault}: the values are back"
        );
    }
}

#[test]
fn observer_images_sh_records_the_pull_policy_that_the_cluster_leaves_and_judges_none_of_it() {
    let mut stage = images_stage();
    stage.env("FAKE_POLICY_STAYS", "1");
    let (o, steps) = run_images(&stage);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    let line = step_line(&steps, "pull-policy");
    assert!(
        line.starts_with("SEEN") && line.ends_with("expected ?, got 1"),
        "{line}"
    );
    let said = std::fs::read_to_string(stage.path("out/pull-policy.out")).unwrap();
    assert!(
        said.contains("asked IfNotPresent, the cluster set IfNotPresent"),
        "{said}"
    );
}

#[test]
fn the_scripts_of_the_digest_checks_are_valid_bash_and_refuse_a_stage_that_does_not_exist() {
    for script in ["observer-images.sh", "observer-kind.sh"] {
        let o = Command::new("bash")
            .arg("-n")
            .arg(live_script(script))
            .output_locked()
            .unwrap();
        assert!(o.status.success(), "{script}: {}", text(&o.stderr));
    }
    let stage = images_stage();
    let script = stage.path("tree/examples/live/observer-images.sh");
    let (o, steps) = stage.run(script.to_str().unwrap(), &["9"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stdout).contains("observer-images.sh [FROM_STAGE]"),
        "{}",
        text(&o.stdout)
    );
    assert!(steps.is_empty(), "no stage ran: {steps}");
}

/// The scripts of the observer share their helpers. Each helper has one definition, in `lib.sh`. A copy
/// in a script fails this test. `observer-kind.sh` defines `kc` and `cluster_kubeconfig` again for its
/// kind cluster, as the comments of `lib.sh` say. `observer-up.sh` sets `KUBE_FILE` and defines none of them.
#[test]
fn the_helpers_of_the_observer_scripts_have_one_definition_in_lib_sh() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/live");
    let mut scripts: Vec<(String, String)> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "sh"))
        .map(|path| {
            let name = path.file_name().unwrap().to_str().unwrap().to_string();
            (name, std::fs::read_to_string(&path).unwrap())
        })
        .collect();
    scripts.sort();
    for (helper, other) in [
        ("kc", Some("observer-kind.sh")),
        ("cluster_kubeconfig", Some("observer-kind.sh")),
        ("kj", None),
        ("can_i", None),
        ("not_found", None),
        ("poll", None),
        ("uri", None),
        ("page", None),
        ("pf_start", None),
        ("pf_stop", None),
        ("images_read", None),
        ("pods_read", None),
        ("kube", None),
        ("create_apply", None),
        ("observer_config", None),
        ("observer_key", None),
        ("observer_image", None),
        ("observer_overlay", None),
        ("observer_overlay_pods", None),
        ("observer_render", None),
        ("observer_ready", None),
        ("pull_token", None),
        ("pull_secret", None),
        ("pull_secret_stop", None),
    ] {
        let definition = format!("{helper}() ");
        let homes: Vec<&str> = scripts
            .iter()
            .filter(|(_, text)| text.lines().any(|l| l.starts_with(&definition)))
            .map(|(name, _)| name.as_str())
            .collect();
        let mut want = vec!["lib.sh"];
        want.extend(other);
        want.sort_unstable();
        assert_eq!(homes, want, "{helper} has a copy");
    }
}

/// The ids of the checks that a script runs: the word after `check` on each line that starts with it.
fn script_check_ids(script: &str) -> Vec<String> {
    repo_file(&format!("examples/live/{script}"))
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix("check "))
        .filter_map(|rest| rest.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

/// The ids that the rows of a section of `live-runs.md` name: each name in backticks in the first cell.
fn page_check_ids(script: &str) -> Vec<String> {
    page_section("docs/src/development/live-runs.md", &format!("## {script}"))
        .lines()
        .filter(|l| l.starts_with("| `"))
        .flat_map(|l| {
            let cell = l.trim_start_matches('|').split('|').next().unwrap_or("");
            cell.split('`')
                .skip(1)
                .step_by(2)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn the_page_of_live_runs_names_each_check_of_observer_images_sh_and_of_observer_kind_sh() {
    for script in ["observer-images.sh", "observer-kind.sh"] {
        let mut in_script = script_check_ids(script);
        let mut on_page = page_check_ids(script);
        in_script.sort();
        on_page.sort();
        assert_eq!(in_script, on_page, "{script}");
    }
    // the eight checks of the plan, by name
    let page = script_check_ids("observer-images.sh");
    for name in [
        "pods-read",
        "image-first",
        "image-split",
        "image-moved",
        "image-never-started",
        "image-crash-loop",
        "image-agents-no-finding",
        "pull-policy",
    ] {
        assert!(
            page.iter().any(|id| id == name),
            "observer-images.sh lacks {name}"
        );
    }
    let kind = script_check_ids("observer-kind.sh");
    for name in ["mirror-image-string", "mirror-image-id"] {
        assert!(
            kind.iter().any(|id| id == name),
            "observer-kind.sh lacks {name}"
        );
    }
    // a check with ? records a result and judges none: the plan marks these three
    for (script, name) in [
        ("observer-images.sh", "pull-policy"),
        ("observer-kind.sh", "mirror-image-string"),
        ("observer-kind.sh", "mirror-image-id"),
    ] {
        let source = repo_file(&format!("examples/live/{script}"));
        assert!(
            source.contains(&format!("check {name} ? ")),
            "{script}: {name} is a check with ?"
        );
    }
}

// ---- observer.sh -------------------------------------------------------------------------------------

/// The `api.sh` of the tree of observer.sh. It logs each call to `$FAKE_OBS/api.log`. The state of
/// Palette is in files of `$FAKE_OBS`. An edit of the add-on version sets or clears `drifted`.
/// `attach` and `detach` set or clear `attached` and `deploy.exists`. `FAKE_NO_DRIFT`,
/// `FAKE_REPAVE`, and `FAKE_DIE_AFTER_EDIT` give the faults: no drift, a replaced machine, and a
/// SIGTERM after the first edit.
const FAKE_API_FRED: &str = r##"#!/usr/bin/env bash
obs=$FAKE_OBS
echo "$*" >> "$obs/api.log"
case "$1" in
  get-kubeconfig) echo "apiVersion: v1 # fake-kubeconfig" ;;
  edit)
    case "$3" in
      'edited: "no"')
        [ -n "${FAKE_NO_DRIFT-}" ] || touch "$obs/drifted"
        echo "edited $2 in place"
        [ -z "${FAKE_DIE_AFTER_EDIT-}" ] || kill -TERM "$PPID" ;;
      *) rm -f "$obs/drifted"; echo "edited $2 in place" ;;
    esac ;;
  manifest-profile) cp "$4" "$obs/profile-manifest.yaml"; echo op-1 ;;
  attach)
    touch "$obs/attached" "$obs/deploy.exists"
    [ -z "${FAKE_REPAVE-}" ] || touch "$obs/repaved" ;;
  detach) rm -f "$obs/attached" "$obs/deploy.exists" ;;
  get)
    case "$2" in
      /v1/spectroclusters/c-a) echo '{"spec":{"cloudType":"aws","cloudConfigRef":{"uid":"cfg"}}}' ;;
      /v1/cloudconfigs/aws/cfg) echo '{"spec":{"machinePoolConfig":[{"name":"cp"}]}}' ;;
      /v1/cloudconfigs/aws/cfg/machinePools/cp/machines)
        if [ -f "$obs/repaved" ]; then name=m-2; else name=m-1; fi
        echo "{\"items\":[{\"metadata\":{\"name\":\"$name\"},\"status\":{\"instanceState\":\"Running\"}}]}" ;;
    esac ;;
esac
exit 0
"##;

/// The `tenant.sh` of that tree: cluster A is Running, and after an attach with `FAKE_NOT_RUNNING` it
/// stays in Provisioning.
const FAKE_TENANT_FRED: &str = r##"#!/usr/bin/env bash
if [ -n "${FAKE_NOT_RUNNING-}" ] && [ -f "$FAKE_OBS/attached" ]; then
  echo "fred c-a: Provisioning"
else
  echo "fred c-a: Running"
fi
"##;

/// The `kubectl` of observer.sh. It logs each call to `$FAKE_OBS/kubectl.log`. The state of the
/// cluster is in files of `$FAKE_OBS`: `ns.exists`, `deploy.exists`, `restarts`, and
/// `first-load-pending`. The key goes to `key.current`, and the pull Secret to `pull.seen`.
/// `FAKE_READER_SECRETS`, `FAKE_REACHED`, and `FAKE_LOG_LEAK` give the faults. A port-forward runs
/// [`FAKE_PORT_FORWARD`].
const FAKE_KUBECTL_FRED: &str = r##"#!/usr/bin/env bash
obs=$FAKE_OBS
echo "$*" >> "$obs/kubectl.log"
notfound() { echo "Error from server (NotFound): $1 \"$2\" not found" >&2; exit 1; }
case "$*" in
  "kustomize .")
    [ -f kustomization.yaml ] && [ -d base ] || { echo "no overlay here" >&2; exit 1; }
    cat "$FAKE_REPO/deploy/observer/deployment.yaml"
    [ -z "${FAKE_KEY_IN_MANIFEST-}" ] || echo "# $PALETTE_API_KEY" ;;
  *"create secret generic strata-observer-key"*"--dry-run=client"*) printf 'kind: Secret\napi-key: %s\n' "$(cat)" ;;
  *"create secret generic strata-observer-key"*) cat > "$obs/key.current" ;;
  *"create secret generic ghcr-pull"*) cat > "$obs/pull.seen" ;;
  *"apply"*"-f -"*)
    body=$(cat)
    printf '%s\n' "$body" >> "$obs/applied.yaml"
    if grep -q '^api-key: ' <<<"$body"; then sed -n 's/^api-key: //p' <<<"$body" | tr -d '\n' > "$obs/key.current"; fi ;;
  *"apply -f "*"namespace.yaml"*) touch "$obs/ns.exists" ;;
  *"apply -f observer.yaml"*) touch "$obs/ns.exists" "$obs/deploy.exists" ;;
  *"delete -f observer.yaml"*) rm -f "$obs/ns.exists" "$obs/deploy.exists" ;;
  *"delete namespace"*) rm -f "$obs/ns.exists" "$obs/deploy.exists" ;;
  *"get namespace strata-observer -o json"*)
    [ -f "$obs/ns.exists" ] || notfound namespaces strata-observer
    echo '{"metadata":{"labels":{"pod-security.kubernetes.io/enforce":"restricted"}}}' ;;
  *"get namespace strata-observer"*)
    [ -f "$obs/ns.exists" ] || notfound namespaces strata-observer
    echo "strata-observer Active" ;;
  *"get pod -l"*jsonpath*) echo 10.0.0.7 ;;
  *"get pod -l"*"-o json"*) cat "$obs/pod.json" ;;
  *"get replicaset"*) echo '{"items":[]}' ;;
  *"get deployment strata-observer -o json"*)
    [ -f "$obs/deploy.exists" ] || notfound deployments.apps strata-observer
    echo '{"status":{"availableReplicas":1}}' ;;
  *"get deployment strata-observer"*)
    [ -f "$obs/deploy.exists" ] || notfound deployments.apps strata-observer
    echo "strata-observer 1/1" ;;
  *"rollout restart"*)
    n=$(cat "$obs/restarts" 2>/dev/null || echo 0); echo $((n + 1)) > "$obs/restarts"
    touch "$obs/first-load-pending" ;;
  *"auth can-i"*)
    case "$*" in
      *strata-observer-reader*)
        case "$*" in
          *portforward*) echo yes; exit 0 ;;
          *"get secrets"*) [ -z "${FAKE_READER_SECRETS-}" ] || { echo yes; exit 0; } ;;
        esac ;;
    esac
    echo no; exit 1 ;;
  *" run "*) if [ -n "${FAKE_REACHED-}" ]; then echo reached; else echo refused; fi ;;
  *" logs "*)
    echo 'time=2026-11-04T10:00:00Z level=INFO msg=strata command=verify exit=0 duration=2s'
    [ -z "${FAKE_LOG_LEAK-}" ] || echo "value strata live example" ;;
  *port-forward*) exec fake-port-forward ;;
esac
exit 0
"##;

/// The `curl` of observer.sh: the routes of the observer, from the state of `$FAKE_OBS`. The page is
/// lost with the wrong key, and first-load once after a restart. `FAKE_NO_PALETTE` keeps it in
/// first-load with a `lastError`, and `FAKE_LEAK` puts the value of a variable in `/v1/events`.
/// `FAKE_LEAK_IMAGES` puts it in `/v1/images`, which answers `pods: "off"` otherwise.
/// With `FAKE_STATE_KEPT`, the first sight of a finding does not change at a restart.
///
/// The pod is the port 18080. Any other port is the observer that the script runs on its machine. That
/// observer answers when its stand-in is up.
///
/// A click is `POST .../ask` with the header `Sec-Fetch-Site: same-origin`. Without the header, curl
/// exits 22, as it does for a 403. A click on the observer of the machine adds POST lines to the log of
/// the counter. `$FAKE_ASK_POSTS` gives the number for each click (default "1 0"). A click on the pod
/// adds none, because the counter does not see the pod.
///
/// `FAKE_ASK_POD` and `FAKE_ASK_LOCAL` give the verdict of each answer: no-error (default), refuses,
/// refuses-accepts-message, refuses-no-code, no-answer, accepts-field, or unknown. `FAKE_ACCEPTS_IN`
/// names a route whose answer holds the word accepts. `FAKE_NO_MOVE` takes the newer version from the
/// page. `FAKE_STALE_MOVE` adds a first move that a click answers with a 404.
///
/// `FAKE_NO_DIFF_TEXT` empties the text of the detail route. `FAKE_TEXT_LEAK` puts the text of the
/// edit in `/v1/events`.
const FAKE_CURL_FRED: &str = r##"#!/usr/bin/env bash
obs=$FAKE_OBS
url=${*: -1}
case "$url" in http://127.0.0.1:*) ;; *) echo "curl: no network" >&2; exit 7 ;; esac
method=GET fetch_site="" prev=""
for a in "$@"; do
  case "$prev" in
    -X) method=$a ;;
    -H) case "$a" in Sec-Fetch-Site:*) fetch_site=${a#*: } ;; esac ;;
  esac
  prev=$a
done
port=${url#http://127.0.0.1:}; port=${port%%/*}
who=POD
if [ "$port" != 18080 ]; then
  who=LOCAL
  for _ in $(seq 100); do
    [ -f "$obs/observer.up" ] && break
    [ ! -f "$obs/observer.dead" ] || exit 7
    command -p sleep 0.05
  done
  [ -f "$obs/observer.up" ] || exit 7
fi
path=${url#http://127.0.0.1:*/}
path=/${path}
answer() {
  local base='"profile":"strata-live-profile@1.0.0","key":"addon/cilium","newest":"1.1.0"'
  case "$1" in
    no-error) echo "{$base,\"verdict\":\"no-error\",\"refusals\":[],\"status\":0}" ;;
    refuses) echo "{$base,\"verdict\":\"refuses\",\"refusals\":[{\"pack\":\"cilium\",\"code\":\"DependencyPackUpgradeNotSupported\",\"message\":\"Pack cilium cannot move\",\"clusters\":[]}],\"status\":0}" ;;
    refuses-accepts-message) echo "{$base,\"verdict\":\"refuses\",\"refusals\":[{\"pack\":\"cilium\",\"code\":\"InvalidValues\",\"message\":\"Palette accepts only a list\",\"clusters\":[]}],\"status\":0}" ;;
    refuses-no-code) echo "{$base,\"verdict\":\"refuses\",\"refusals\":[{\"pack\":\"cilium\",\"code\":\"\",\"message\":\"no code\",\"clusters\":[]}],\"status\":0}" ;;
    no-answer) echo "{$base,\"verdict\":\"no-answer\",\"refusals\":[],\"status\":403}" ;;
    accepts-field) echo "{$base,\"verdict\":\"no-error\",\"refusals\":[],\"status\":0,\"accepts\":true}" ;;
    *) echo "{$base,\"verdict\":\"maybe\",\"refusals\":[],\"status\":0}" ;;
  esac
}
verdict_var=FAKE_ASK_$who; verdict=${!verdict_var:-no-error}
clicks=$(cat "$obs/$who.clicks" 2>/dev/null || echo 0)
restarts=$(cat "$obs/restarts" 2>/dev/null || echo 0)
page=ok
if [ -f "$obs/first-load-pending" ]; then page=first-load; rm -f "$obs/first-load-pending"; fi
grep -q wrong "$obs/key.current" 2>/dev/null && page=lost
checked='"2026-11-04T10:01:00Z"'; last=null; failed=0
if [ -n "${FAKE_NO_PALETTE-}" ]; then page=first-load; checked=null; last='{"kind":"transport"}'; failed=3; fi
newer=',"newer":"1.1.0"'; [ -z "${FAKE_NO_MOVE-}" ] || newer=
stale=; [ -z "${FAKE_STALE_MOVE-}" ] || stale='{"key":"addon/stale","name":"stale","layer":"addon","version":"1.0.0","newer":"2.0.0"},'
profiles='{"profiles":[
  {"name":"strata-live-profile","version":"1.0.0","status":"ok","published":{"status":"unchanged"},"clusterCopy":{"status":"same"},"packs":['"$stale"'{"key":"addon/cilium","name":"cilium","layer":"addon","version":"1.0.0"'"$newer"'}]},
  {"name":"strata-live-addon","version":"1.0.0","status":"ok","published":{"status":"unchanged"},"clusterCopy":{"status":"same"},"packs":[]}]}'
[ -f "$obs/drifted" ] && profiles=$(jq '(.profiles[] | select(.name == "strata-live-addon") | .published.status) = "drifted"' <<<"$profiles")
[ -f "$obs/attached" ] && profiles=$(jq '.profiles += [{"name":"strata-observer-live","version":"1.0.0","status":"no-record"}]' <<<"$profiles")
seen=$restarts; [ -z "${FAKE_STATE_KEPT-}" ] || seen=0
findings='{"findings":[]}'
[ -f "$obs/drifted" ] && findings=$(printf '{"findings":[{"kind":"published-changed","profile":"strata-live-addon@1.0.0","packs":["addon/strata-live-config"],"firstSeen":"2026-11-04T10:%02d:30Z","who":[{"actor":"ada@example.com","actorType":"user","action":"update"}]}]}' "$seen")
case "$path" in
  /healthz) echo ok ;;
  /v1/cluster)
    printf '{"cluster":{"uid":"c-a"},"page":"%s","records":{"count":2,"baseline":"approved","startedAt":"2026-11-04T10:%02d:00.5Z"},"content":{"checkedAt":%s,"every":"1m","failedRuns":%s,"lastError":%s},"versions":{"readAt":%s,"every":"5m","failedRuns":0,"lastError":null}}\n' \
      "$page" "$restarts" "$checked" "$failed" "$last" "$checked" ;;
  /v1/findings) echo "$findings" ;;
  /v1/profiles) echo "$profiles" ;;
  /v1/profiles/*) jq --arg n "${path#/v1/profiles/}" '.profiles[] | select(.name + "@" + .version == $n)' <<<"$profiles" ;;
  /v1/notchecked)
    if [ -f "$obs/attached" ]; then echo '{"notChecked":[{"cause":"no-record","profiles":["strata-observer-live@1.0.0"],"packs":[]}],"limits":{}}'
    else echo '{"notChecked":[],"limits":{}}'; fi ;;
  /v1/events)
    if [ -n "${FAKE_LEAK-}" ]; then echo '{"events":[{"message":"purpose is strata live example"}]}'
    elif [ -n "${FAKE_TEXT_LEAK-}" ]; then echo '{"events":[{"message":"+edited: \"yes\""}]}'
    else echo '{"events":[]}'; fi ;;
  /v1/newer) echo '{"newer":[]}' ;;
  /v1/asks)
    if [ "$clicks" -gt 0 ]; then echo "{\"asks\":[$(answer "$verdict")]}"; else echo '{"asks":[]}'; fi ;;
  /v1/images)
    if [ -n "${FAKE_LEAK_IMAGES-}" ]; then echo '{"schema":"strata-observed/1","pods":"off","images":[{"ref":"purpose is strata live example"}]}'
    else echo '{"schema":"strata-observed/1","pods":"off","readAt":null,"counts":{"images":0,"matched":0,"changed":0},"images":[]}'; fi ;;
  /v1/packs/*/addon/stale/ask) exit 22 ;;
  /v1/packs/*/ask)
    [ "$method" = POST ] && [ "$fetch_site" = same-origin ] || exit 22
    clicks=$((clicks + 1)); echo "$clicks" > "$obs/$who.clicks"
    if [ "$who" = LOCAL ]; then
      read -ra counts <<<"${FAKE_ASK_POSTS:-1 0}"
      for _ in $(seq "${counts[clicks - 1]:-0}"); do
        echo "POST /v1/clusterprofiles/0123456789abcdef01234567/validate/packs" >> "$(cat "$obs/counter.last")"
      done
    fi
    answer "$verdict" ;;
  /v1/packs/*/diff?side=published)
    unified=$'--- a: addon/strata-live-config manifest config\n+++ b: addon/strata-live-config manifest config\n@@ -1 +1 @@\n-edited: "no"\n+edited: "yes"\n'
    [ -z "${FAKE_NO_DIFF_TEXT-}" ] || unified=
    jq -n --arg u "$unified" '{values: {parseable: true, unified: ""}, manifests: [{op: "changed", name: "config", unified: $u}], textTooLarge: false}' ;;
  /v1/packs/*) echo '{"values":{}}' ;;
  /) echo '<!doctype html><html lang="en"></html>' ;;
  *) exit 22 ;;
esac
if [ "$path" = "${FAKE_ACCEPTS_IN-}" ]; then echo '{"note":"Palette accepts the move"}'; fi
"##;

/// The `go` of observer.sh: `go -C DIR build -o FILE .` writes a stand-in into FILE. The counter
/// writes `$FAKE_REQUESTS` lines (default 3) to its log, logs its target and the name of its log, and
/// runs the command. The observer logs its arguments, the key that it got, and the files of its
/// records. It marks itself up, and it waits for a signal.
///
/// `FAKE_ASK_POSTS_BEFORE` makes the counter write that many POST lines before it runs the command.
/// A cycle that sends a POST does the same. `FAKE_OBSERVER_DIES` makes the observer stop at once.
const FAKE_GO_FRED: &str = r##"#!/usr/bin/env bash
echo "$*" >> "$FAKE_OBS/go.log"
[ -z "${FAKE_NO_GO-}" ] || { echo "go: cannot build" >&2; exit 1; }
out=""; prev=""
for a in "$@"; do [ "$prev" = -o ] && out=$a; prev=$a; done
case "$out" in
*/strata-observer)
cat > "$out" <<'OBSERVER'
#!/usr/bin/env bash
echo "$*" >> "$FAKE_OBS/observer.args"
echo "PALETTE_API_KEY=${PALETTE_API_KEY-}" >> "$FAKE_OBS/observer.env"
if [ -n "${FAKE_OBSERVER_DIES-}" ]; then touch "$FAKE_OBS/observer.dead"; echo "strata-observer: the flags are wrong" >&2; exit 2; fi
records=""; prev=""
for a in "$@"; do [ "$prev" = --records ] && records=$a; prev=$a; done
cp "$records/meta.json" "$FAKE_OBS/observer.meta"
ls "$records" > "$FAKE_OBS/observer.records"
for f in "$records"/*.json.gz; do gzip -dc "$f" > "$FAKE_OBS/observer.record.$(basename "$f" .json.gz)"; done
touch "$FAKE_OBS/observer.up"
exec tail -f /dev/null
OBSERVER
;;
*)
cat > "$out" <<'COUNTER'
#!/usr/bin/env bash
target=""; log=""
while [ $# -gt 0 ]; do
  case "$1" in
    -target) target=$2; shift 2 ;;
    -log) log=$2; shift 2 ;;
    --) shift; break ;;
    *) shift ;;
  esac
done
echo "$target" >> "$FAKE_OBS/counter.targets"
echo "$PWD/$log" > "$FAKE_OBS/counter.last"
: > "$log"
for _ in $(seq "${FAKE_REQUESTS:-3}"); do echo "GET /v1/spectroclusters/0123456789abcdef01234567" >> "$log"; done
for _ in $(seq "${FAKE_ASK_POSTS_BEFORE:-0}"); do echo "POST /v1/clusterprofiles/0123456789abcdef01234567/validate/packs" >> "$log"; done
exec "$@"
COUNTER
;;
esac
chmod +x "$out"
"##;

/// The token of `gh auth token` in the tests of observer.sh. It has the scopes of a person, so it
/// never reaches the cluster.
const OBSERVER_GH_TOKEN: &str = "ghp_token_that_no_log_shows";
/// The token in the token file of observer.sh. It has only the scope read:packages.
const OBSERVER_PULL_TOKEN: &str = "ghp_pull_token_that_no_log_shows";

/// Write `body` to `path` with the mode `mode`.
fn write_with_mode(path: &Path, body: &str, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}
/// The key of the tenant in the tests of observer.sh.
const OBSERVER_KEY: &str = "key-that-no-log-shows";

/// A profile record, as the stand-in `strata` writes it, with a uid.
fn observer_profile_record(uid: &str, name: &str, kind: &str) -> String {
    serde_json::json!({
        "schema": "strata-snapshot/1",
        "source": {"uid": uid, "name": name, "version": "1.0.0", "type": kind},
    })
    .to_string()
}

/// A stage for observer.sh. The script runs in a tree of its own, so that its `api.sh`,
/// `tenant.sh`, `tf.sh`, and `baseline.sh` are stand-ins. Every command outside the repository is
/// a stand-in too. Cluster A is `c-a` named fred. It runs the release profile `r-1` and the
/// add-on profile `ad-1`. The tree is `stage.path("tree")`.
fn observer_stage() -> Stage {
    let mut stage = Stage::new(baseline_outputs());
    let tree = stage.path("tree");
    copy_scripts(
        &tree,
        &[
            "examples/live/observer.sh",
            "examples/live/lib.sh",
            "examples/live/common.sh",
            "scripts/ci/observer-records.sh",
            "Cargo.toml",
        ],
    );
    for file in [
        "kustomization",
        "namespace",
        "serviceaccount",
        "networkpolicy",
        "deployment",
        "reader-role",
    ] {
        let path = format!("deploy/observer/{file}.yaml");
        copy_scripts(&tree, &[path.as_str()]);
    }
    let live = tree.join("examples/live");
    std::fs::create_dir_all(live.join("count-requests")).unwrap();
    executable(&live.join("api.sh"), FAKE_API_FRED);
    executable(&live.join("tenant.sh"), FAKE_TENANT_FRED);
    executable(
        &live.join("tf.sh"),
        "#!/usr/bin/env bash\ncat \"$FAKE_OUTPUTS\"\n",
    );
    executable(&live.join("baseline.sh"), "#!/usr/bin/env bash\nexit 0\n");
    executable(&live.join("diagnose.sh"), "#!/usr/bin/env bash\nexit 0\n");

    let obs = stage.path("obs");
    std::fs::create_dir_all(&obs).unwrap();
    std::fs::write(
        obs.join("pod.json"),
        serde_json::to_string(&observer_pod()).unwrap(),
    )
    .unwrap();
    let bin = stage.path("obs-bin");
    std::fs::create_dir_all(&bin).unwrap();
    for (name, body) in [
        ("kubectl", FAKE_KUBECTL_FRED),
        ("curl", FAKE_CURL_FRED),
        ("go", FAKE_GO_FRED),
        (
            "git",
            "#!/usr/bin/env bash\necho 0123456789abcdef0123456789abcdef01234567\n",
        ),
        (
            "gh",
            &format!(
                "#!/usr/bin/env bash\ncase \"$*\" in\n  \"auth token\") echo {OBSERVER_GH_TOKEN} ;;\n  \"api user\"*) echo tester ;;\nesac\n"
            ),
        ),
    ] {
        executable(&bin.join(name), body);
    }

    let cluster = serde_json::json!({
        "schema": "strata-snapshot/1",
        "source": {"cluster": {"uid": "c-a", "name": "fred", "profiles": [
            {"uid": "r-1", "name": "strata-live-profile", "version": "1.0.0", "type": "cluster"},
            {"uid": "ad-1", "name": "strata-live-addon", "version": "1.0.0", "type": "add-on"}],
            "variables": [{"name": "purpose", "value": "strata live example"}]}},
    });
    stage.strata(
        "snapshot cluster:c-a *-o cluster.json*",
        0,
        &cluster.to_string(),
    );
    stage.strata(
        "snapshot r-1 -o *",
        0,
        &observer_profile_record("r-1", "strata-live-profile", "cluster"),
    );
    stage.strata(
        "snapshot ad-1 -o *",
        0,
        &observer_profile_record("ad-1", "strata-live-addon", "add-on"),
    );
    stage.strata(
        "snapshot op-1 -o observer-profile.json*",
        0,
        &observer_profile_record("op-1", "strata-observer-live", "add-on"),
    );
    stage.strata("snapshot cluster:c-a/* --from-raw*", 0, "{}");
    stage.strata(
        "verify record-* --summary*",
        0,
        r#"{"records":[{"status":"unchanged"}]}"#,
    );
    stage.strata("compare record-* side-*", 0, r#"{"same":true}"#);
    stage.strata("compare record-* cluster:c-a/*", 0, r#"{"same":true}"#);
    stage.strata(
        "compare cluster.json cluster:c-a --json*",
        0,
        r#"{"same":true,"a":{"cluster":{"variables":[{"name":"purpose","value":"strata live example"}]}}}"#,
    );
    stage.strata(
        "behind record-*",
        0,
        r#"{"pins":[{"behind":true,"error":null,"registryUid":"reg-1","name":"cilium","version":"1.0.0","newer":[{"version":"1.1.0"}]}]}"#,
    );
    stage.strata("compare pack:*", 0, "{}");
    stage.strata(
        "list profiles --json",
        0,
        r#"{"profiles":[{"name":"strata-live-profile","uid":"r-1"},{"name":"strata-live-addon","uid":"ad-1"}]}"#,
    );

    // the token file is outside the folder of the run, so that the look for it in the run is fair
    let token_file = stage.path("ghcr-pull-token");
    write_with_mode(&token_file, &format!("{OBSERVER_PULL_TOKEN}\n"), 0o600);

    let path = format!("{}:{}", bin.display(), path_with(&stage.path("bin")));
    stage.env("PATH", &path);
    stage.env("PALETTE_API_KEY", OBSERVER_KEY);
    stage.env("GHCR_PULL_TOKEN_FILE", token_file.to_str().unwrap());
    stage.env("FAKE_OBS", obs.to_str().unwrap());
    stage.env("FAKE_REPO", env!("CARGO_MANIFEST_DIR"));
    stage
}

/// Run observer.sh of the tree of `stage`.
fn run_observer(stage: &Stage, args: &[&str]) -> (Output, String) {
    let script = stage.path("tree/examples/live/observer.sh");
    stage.run(script.to_str().unwrap(), args)
}

/// Each check of observer.sh, in the order that it runs on a good run.
const OBSERVER_CHECKS: [&str; 61] = [
    "pull-secret",
    "cluster-record",
    "profile-records",
    "namespace",
    "config",
    "key",
    "records-apply",
    "manifests-render",
    "manifests-apply",
    "apply-admitted",
    "pod-ready",
    "pod-hardened",
    "reaches-palette",
    "no-finding-at-baseline",
    "summary-matches-strata",
    "reader-role",
    "reader-can-open",
    "reader-cannot-read",
    "nobody-cannot-open",
    "other-pod-refused",
    "page-admin",
    "requests-counted",
    "drift-edit",
    "drift-found",
    "drift-who",
    "cluster-copy-unchanged",
    "variable-in-raw",
    "no-value-leaves",
    "requests-counted-drift",
    "ask-one-post",
    "ask-words",
    "diff-text",
    "drift-undo",
    "drift-clears",
    "page-admin-again",
    "wrong-key",
    "lost-after-wrong-key",
    "key-back",
    "ok-after-key-back",
    "restart-keeps-working",
    "restart-first-load",
    "first-seen-starts-again",
    "manifests-delete",
    "removal-is-clean",
    "fred-unchanged",
    "profile-prepare",
    "nodes-before",
    "profile-create",
    "profile-attach",
    "profile-applied",
    "profile-does-not-disturb",
    "own-profile-not-checked",
    "profile-record",
    "no-key-in-profile",
    "no-repave",
    "profile-detach",
    "profile-removal",
    "undo-profile",
    "undo-namespace",
    "no-secret-in-run",
    "undo-baseline",
];

#[test]
fn observer_sh_runs_the_stages_in_order_and_each_check_passes() {
    let stage = observer_stage();
    let (o, steps) = run_observer(&stage, &[]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    assert!(!steps.contains("FAIL"), "{steps}");
    assert_eq!(check_ids(&steps), OBSERVER_CHECKS, "{steps}");
    for id in [
        "requests-counted",
        "requests-counted-drift",
        "restart-first-load",
        "no-repave",
    ] {
        assert!(has_verdict(&steps, "SEEN", id), "{id}:\n{steps}");
    }
}

#[test]
fn observer_sh_waits_for_a_port_forward_that_prints_its_line_late() {
    // each port-forward prints its line half a second late. A wait of 30 looks ends before the line,
    // because the stand-in sleep returns at once. ask-words opens one port-forward.
    let mut stage = observer_stage();
    stage.env("FAKE_FORWARD_LATE", "0.5");
    let (o, steps) = run_observer(&stage, &[]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    assert!(!steps.contains("FAIL"), "{steps}");
    assert_eq!(check_ids(&steps), OBSERVER_CHECKS, "{steps}");
}

/// What the files of `dir` hold, joined, for a search of a secret
fn all_text(dir: &Path) -> String {
    files_under(dir)
        .into_iter()
        .map(|(_, content)| content)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn observer_sh_installs_with_a_plain_apply_and_sets_the_image_the_pull_secret_and_the_periods() {
    let stage = observer_stage();
    let (o, steps) = run_observer(&stage, &[]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));

    let kustomization = std::fs::read_to_string(stage.path("out/kustomization.yaml")).unwrap();
    for part in [
        "name: ghcr.io/como-technologies/strata-observer",
        "newName: ghcr.io/como-technologies/strata-observer",
        &format!("newTag: v{}", version_of("Cargo.toml")),
        "- name: ghcr-pull",
        "value: --content-every=1m",
        "value: --versions-every=5m",
    ] {
        assert!(kustomization.contains(part), "{part}:\n{kustomization}");
    }
    assert!(
        stage.path("out/base/deployment.yaml").is_file(),
        "base/ holds the manifests of the run"
    );

    let kubectl = observer_log(&stage, "kubectl.log");
    // way 1 is the plain apply of the rendered file; only the records need --server-side
    assert!(
        kubectl.lines().any(|l| l == "apply -f observer.yaml"),
        "{kubectl}"
    );
    let server_side: Vec<&str> = kubectl
        .lines()
        .filter(|l| l.contains("--server-side"))
        .collect();
    assert_eq!(
        server_side.len(),
        2,
        "records, and records again: {server_side:?}"
    );
    for call in [
        "create configmap strata-observer --from-literal=cluster-uid=c-a",
        "--from-literal=project-uid=p-1",
        "create secret generic strata-observer-key --from-file=api-key=/dev/stdin",
        "create secret generic ghcr-pull --type=kubernetes.io/dockerconfigjson",
        "create rolebinding reader --role=strata-observer-reader --user=strata-observer-reader",
        "auth can-i -n strata-observer get secrets --as=strata-observer-reader",
        "auth can-i -n strata-observer create pods --subresource=exec --as=strata-observer-reader",
        "auth can-i -n strata-observer create pods --subresource=portforward --as=strata-observer-nobody",
        "--as=strata-observer-reader -n strata-observer port-forward deploy/strata-observer 18080:8080",
        "-n strata-observer exec deploy/strata-observer -- /usr/local/bin/strata-observer health",
        "-n strata-observer wait --for=condition=Available deploy/strata-observer --timeout=180s",
        "-n default run observer-probe --rm -i --restart=Never",
    ] {
        assert!(kubectl.contains(call), "{call}:\n{kubectl}");
    }

    // the pull secret holds the token of the file, and the key Secret holds the right key at the end
    let pull = observer_log(&stage, "pull.seen");
    let auths: serde_json::Value = serde_json::from_str(&pull).unwrap();
    assert_eq!(auths["auths"]["ghcr.io"]["password"], OBSERVER_PULL_TOKEN);
    assert_eq!(auths["auths"]["ghcr.io"]["username"], "tester");
    assert_eq!(observer_log(&stage, "key.current"), OBSERVER_KEY);
    assert_eq!(
        observer_log(&stage, "restarts").trim(),
        "4",
        "the wrong key, the key back, the restart, and the restart with a finding"
    );

    // way 2 sends the rendered file as the one manifest, and takes the profile off at the end
    let api = observer_log(&stage, "api.log");
    for call in [
        "manifest-profile strata-observer-live strata-observer observer.yaml",
        "attach c-a op-1",
        "detach c-a op-1",
        "delete op-1",
        "edit ad-1 edited: \"no\" edited: \"yes\"",
        "edit ad-1 edited: \"yes\" edited: \"no\"",
    ] {
        assert!(api.contains(call), "{call}:\n{api}");
    }
    assert_eq!(
        observer_log(&stage, "profile-manifest.yaml"),
        std::fs::read_to_string(stage.path("out/observer.yaml")).unwrap()
    );
    // nothing stays: not the namespace, not the Deployment, not the edit
    for file in ["ns.exists", "deploy.exists", "drifted", "attached"] {
        assert!(!stage.path("obs").join(file).exists(), "{file} stays");
    }

    // the requests are counted behind the counter, which gets the address of Palette
    let targets = observer_log(&stage, "counter.targets");
    assert_eq!(
        targets.lines().count(),
        7,
        "two counts of three commands, and the observer of the click: {targets}"
    );
    assert!(targets.lines().all(|l| l == "https://api.spectrocloud.com"));
    let counted = std::fs::read_to_string(stage.path("out/requests-counted.out")).unwrap();
    for line in [
        "content: 3 requests",
        "behind: 3 requests",
        "moves: 3 requests",
    ] {
        assert!(counted.contains(line), "{line}:\n{counted}");
    }
    assert!(
        counted.contains("GET /v1/spectroclusters/{uid}"),
        "{counted}"
    );
    let go = observer_log(&stage, "go.log");
    assert_eq!(
        go.lines().count(),
        2,
        "the counter and the observer are built once each: {go}"
    );
    assert!(go.lines().any(|l| l.contains("count-requests build -o")));
    assert!(go.lines().any(|l| l.contains("observer build -o")));

    // the script deletes no cluster
    assert!(!api.contains("delete-cluster"), "{api}");
    assert!(
        !kubectl.lines().any(|l| l.contains("delete cluster")),
        "{kubectl}"
    );
}

#[test]
fn observer_sh_puts_no_token_and_no_key_in_an_output_a_file_of_the_run_or_a_call() {
    let stage = observer_stage();
    let (o, steps) = run_observer(&stage, &[]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    let mut shown = Vec::new();
    for (name, haystack) in [
        ("stdout", text(&o.stdout)),
        ("stderr", text(&o.stderr)),
        ("the files of the run", all_text(&stage.path("out"))),
        ("the calls of kubectl", observer_log(&stage, "kubectl.log")),
        ("the calls of api.sh", observer_log(&stage, "api.log")),
        ("the calls of strata", all_text(&stage.path("strata"))),
    ] {
        for secret in [OBSERVER_KEY, OBSERVER_PULL_TOKEN, OBSERVER_GH_TOKEN] {
            if haystack.contains(secret) {
                shown.push(format!("{secret} in {name}"));
            }
        }
    }
    assert!(shown.is_empty(), "{shown:?}");
    // the two reach their Secrets through standard input
    assert!(observer_log(&stage, "pull.seen").contains(OBSERVER_PULL_TOKEN));
    assert_eq!(observer_log(&stage, "key.current"), OBSERVER_KEY);
    // the kubeconfig is a credential: no file of it stays in the run or in the temporary directory
    let kubeconfigs: Vec<_> = files_under(&stage.path("out"))
        .into_iter()
        .filter(|(_, c)| c.contains("fake-kubeconfig"))
        .collect();
    assert!(kubeconfigs.is_empty(), "{kubeconfigs:?}");
    let left: Vec<_> = files_under(&stage.path("tmp"))
        .into_iter()
        .filter(|(_, c)| c.contains("fake-kubeconfig"))
        .collect();
    assert!(left.is_empty(), "the kubeconfig stays: {left:?}");
}

/// The token file of the stage holds `body` with the mode `mode`. `None` removes the file.
fn observer_token_file(stage: &Stage, body: Option<&str>, mode: u32) {
    let path = stage.path("ghcr-pull-token");
    match body {
        Some(body) => write_with_mode(&path, body, mode),
        None => std::fs::remove_file(path).unwrap(),
    }
}

/// The run of `stage` stops at `pull-secret`: it is the one check, it fails, and its line names the
/// file and the scope. No call changes the cluster, and no call makes a Secret.
fn assert_observer_stops_at_the_pull_secret(stage: &Stage, why: &str) {
    let (o, steps) = run_observer(stage, &[]);
    assert_eq!(
        o.status.code(),
        Some(1),
        "{why}:\n{steps}{}",
        text(&o.stderr)
    );
    assert_eq!(check_ids(&steps), ["pull-secret"], "{why}:\n{steps}");
    failed(&steps, &["pull-secret"]);
    let said = std::fs::read_to_string(stage.path("out/pull-secret.err")).unwrap();
    assert_eq!(said.lines().count(), 1, "{why}: one line:\n{said}");
    assert!(
        said.contains(stage.path("ghcr-pull-token").to_str().unwrap())
            && said.contains("read:packages"),
        "{why}: the line names the file and the scope:\n{said}"
    );
    assert!(
        steps.contains(said.trim()),
        "{why}: the log holds it:\n{steps}"
    );
    assert_eq!(
        observer_log(stage, "kubectl.log"),
        "",
        "{why}: no call to the cluster"
    );
    assert_eq!(observer_log(stage, "pull.seen"), "", "{why}: no Secret");
    // the token of gh is no fallback, and no secret is in a place that the run shows
    for secret in [OBSERVER_PULL_TOKEN, OBSERVER_GH_TOKEN, OBSERVER_KEY] {
        assert!(!text(&o.stdout).contains(secret), "{why}: {secret}");
        assert!(
            !all_text(&stage.path("out")).contains(secret),
            "{why}: {secret}"
        );
    }
}

#[test]
fn observer_sh_stops_at_the_pull_secret_when_the_token_file_is_absent_or_empty() {
    for (why, body) in [
        ("absent", None),
        ("empty", Some("")),
        ("only white space", Some(" \n\t\n")),
    ] {
        let stage = observer_stage();
        observer_token_file(&stage, body, 0o600);
        assert_observer_stops_at_the_pull_secret(&stage, why);
    }
}

#[test]
fn observer_sh_stops_at_the_pull_secret_when_a_group_or_others_can_read_the_token_file() {
    for mode in [0o644, 0o640, 0o604, 0o660] {
        let stage = observer_stage();
        observer_token_file(&stage, Some(OBSERVER_PULL_TOKEN), mode);
        assert_observer_stops_at_the_pull_secret(&stage, &format!("mode {mode:o}"));
    }
}

#[test]
fn observer_sh_sends_the_token_of_gh_to_no_call_and_no_input_of_kubectl() {
    let stage = observer_stage();
    let (o, steps) = run_observer(&stage, &[]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    // the stand-in keeps the calls, the key, the pull Secret, and the input of each apply
    let kept = files_under(&stage.path("obs"));
    for file in ["kubectl.log", "key.current", "pull.seen", "applied.yaml"] {
        assert!(kept.iter().any(|(path, _)| path == file), "{file} is kept");
    }
    for (path, content) in kept {
        assert!(!content.contains(OBSERVER_GH_TOKEN), "in {path}");
    }
}

#[test]
fn observer_sh_finds_the_pull_token_or_the_token_of_gh_in_a_file_of_the_run() {
    for leak in [OBSERVER_PULL_TOKEN, OBSERVER_GH_TOKEN] {
        let mut stage = observer_stage();
        stage.env("FAKE_LEAK_SECRET", leak);
        stage.wrap_strata(
            r#"#!/usr/bin/env bash
out=""; prev=""
for a in "$@"; do [ "$prev" = -o ] && out=$a; prev=$a; done
case "$*" in
  "snapshot op-1 -o "*) echo "{\"manifest\": \"$FAKE_LEAK_SECRET\"}" > "$out"; exit 0 ;;
esac
exec "$(dirname "$0")/fake-strata-rules" "$@"
"#,
        );
        let (o, steps) = run_observer(&stage, &[]);
        assert_eq!(
            o.status.code(),
            Some(1),
            "{leak}:\n{steps}{}",
            text(&o.stderr)
        );
        failed(&steps, &["no-secret-in-run"]);
        passed(&steps, &["no-key-in-profile", "undo-namespace"]);
    }
}

#[test]
fn observer_sh_finds_the_key_in_a_manifest_or_in_the_record_of_the_profile() {
    let mut stage = observer_stage();
    stage.env("FAKE_KEY_IN_MANIFEST", "1");
    stage.wrap_strata(
        r#"#!/usr/bin/env bash
out=""; prev=""
for a in "$@"; do [ "$prev" = -o ] && out=$a; prev=$a; done
case "$*" in
  "snapshot op-1 -o "*) echo "{\"manifest\": \"$PALETTE_API_KEY\"}" > "$out"; exit 0 ;;
esac
exec "$(dirname "$0")/fake-strata-rules" "$@"
"#,
    );
    let (o, steps) = run_observer(&stage, &[]);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    failed(&steps, &["no-key-in-profile", "no-secret-in-run"]);
    passed(&steps, &["profile-attach", "undo-namespace"]);
}

#[test]
fn observer_sh_names_the_last_error_when_the_pod_does_not_reach_palette_and_runs_no_later_stage() {
    let mut stage = observer_stage();
    stage.env("FAKE_NO_PALETTE", "1");
    stage.env("CYCLE_TRIES", "2");
    let (o, steps) = run_observer(&stage, &[]);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    failed(&steps, &["reaches-palette"]);
    let said = std::fs::read_to_string(stage.path("out/reaches-palette.out")).unwrap();
    assert!(
        said.contains("lastError") && said.contains("transport"),
        "{said}"
    );
    // the checks that need the page wait, and stage 3 does not need Palette
    for id in [
        "no-finding-at-baseline",
        "summary-matches-strata",
        "drift-edit",
        "wrong-key",
        "profile-create",
    ] {
        assert!(!steps.contains(id), "{id} ran:\n{steps}");
    }
    assert!(
        steps.contains("reaches-palette failed: this stage needs the page"),
        "{steps}"
    );
    passed(
        &steps,
        &["reader-can-open", "other-pod-refused", "undo-namespace"],
    );
    assert!(observer_log(&stage, "kubectl.log").contains("delete namespace strata-observer"));
    assert!(!stage.path("obs/ns.exists").exists());
}

#[test]
fn observer_sh_undoes_the_edit_in_place_and_removes_the_namespace_when_a_signal_ends_the_run() {
    let mut stage = observer_stage();
    stage.env("FAKE_DIE_AFTER_EDIT", "1");
    let (o, steps) = run_observer(&stage, &[]);
    assert!(!o.status.success(), "the signal ends the run: {steps}");
    assert!(
        !steps.contains("drift-found"),
        "the run stopped at the edit:\n{steps}"
    );
    let api = observer_log(&stage, "api.log");
    let edits: Vec<&str> = api.lines().filter(|l| l.starts_with("edit ")).collect();
    assert_eq!(
        edits,
        [
            "edit ad-1 edited: \"no\" edited: \"yes\"",
            "edit ad-1 edited: \"yes\" edited: \"no\""
        ],
        "the trap puts the version back"
    );
    assert!(!stage.path("obs/drifted").exists());
    assert!(observer_log(&stage, "kubectl.log").contains("delete namespace strata-observer"));
    assert!(!stage.path("obs/ns.exists").exists());
    assert!(
        stage.path("out/summary.md").is_file(),
        "the summary is written"
    );
}

#[test]
fn observer_sh_takes_the_profile_off_and_removes_the_namespace_when_the_cluster_stays_busy() {
    let mut stage = observer_stage();
    stage.env("FAKE_NOT_RUNNING", "1");
    let (o, steps) = run_observer(&stage, &[]);
    assert_eq!(o.status.code(), Some(2), "{steps}{}", text(&o.stderr));
    // the message of the stop is in the errors of the check that stopped the run
    let said = std::fs::read_to_string(stage.path("out/profile-attach.err")).unwrap();
    assert!(said.contains("not Running after an hour"), "{said}");
    let api = observer_log(&stage, "api.log");
    let calls: Vec<&str> = api
        .lines()
        .filter(|l| {
            l.starts_with("attach ") || l.starts_with("detach ") || l.starts_with("delete ")
        })
        .collect();
    assert_eq!(
        calls,
        ["attach c-a op-1", "detach c-a op-1", "delete op-1"],
        "{api}"
    );
    assert!(observer_log(&stage, "kubectl.log").contains("delete namespace strata-observer"));
    assert!(!stage.path("obs/ns.exists").exists());
}

#[test]
fn observer_sh_fails_the_finding_checks_when_the_edit_changes_nothing_and_still_restores_the_version()
 {
    let mut stage = observer_stage();
    stage.env("FAKE_NO_DRIFT", "1");
    stage.env("CYCLE_TRIES", "2");
    let (o, steps) = run_observer(&stage, &[]);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    failed(&steps, &["drift-found", "drift-who"]);
    passed(
        &steps,
        &[
            "drift-edit",
            "cluster-copy-unchanged",
            "drift-undo",
            "drift-clears",
            "undo-namespace",
        ],
    );
}

#[test]
fn observer_sh_fails_when_a_route_or_the_log_shows_the_value_of_a_variable() {
    for (fault, shown) in [
        ("FAKE_LEAK", "the route /v1/events shows the value"),
        ("FAKE_LEAK_IMAGES", "the route /v1/images shows the value"),
        ("FAKE_LOG_LEAK", "the log shows the value"),
    ] {
        let mut stage = observer_stage();
        stage.env(fault, "1");
        let (o, steps) = run_observer(&stage, &[]);
        assert_eq!(
            o.status.code(),
            Some(1),
            "{fault}: {steps}{}",
            text(&o.stderr)
        );
        // the raw output holds the value, so a grep that finds nothing proves something
        passed(&steps, &["variable-in-raw"]);
        failed(&steps, &["no-value-leaves"]);
        let said = std::fs::read_to_string(stage.path("out/no-value-leaves.out")).unwrap();
        assert!(said.contains(shown), "{fault}: {said}");
    }
}

#[test]
fn observer_sh_fails_the_access_checks_when_the_reader_reads_a_secret_or_a_pod_reaches_the_page() {
    let mut stage = observer_stage();
    stage.env("FAKE_READER_SECRETS", "1");
    stage.env("FAKE_REACHED", "1");
    let (o, steps) = run_observer(&stage, &[]);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    failed(&steps, &["reader-cannot-read", "other-pod-refused"]);
    passed(&steps, &["reader-can-open", "nobody-cannot-open"]);
}

#[test]
fn observer_sh_records_what_palette_did_to_the_machines_without_judging_it() {
    let mut stage = observer_stage();
    stage.env("FAKE_REPAVE", "1");
    let (o, steps) = run_observer(&stage, &[]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    let line = step_line(&steps, "no-repave");
    assert!(
        line.starts_with("SEEN") && line.ends_with("got 1"),
        "{line}"
    );
    let said = std::fs::read_to_string(stage.path("out/no-repave.out")).unwrap();
    assert!(
        said.contains("before: m-1") && said.contains("after:  m-2"),
        "{said}"
    );
}

#[test]
fn observer_sh_records_a_counter_that_does_not_build_fails_the_count_of_the_click_and_goes_on() {
    let mut stage = observer_stage();
    stage.env("FAKE_NO_GO", "1");
    let (o, steps) = run_observer(&stage, &[]);
    // the count of a cycle only records a result, but the count of a click judges one
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    let line = step_line(&steps, "requests-counted");
    assert!(
        line.starts_with("SEEN") && line.ends_with("got 1"),
        "{line}"
    );
    failed(&steps, &["ask-one-post"]);
    passed(
        &steps,
        &["drift-found", "ask-words", "diff-text", "profile-removal"],
    );
    assert!(
        !stage.path("obs/observer.args").exists(),
        "no observer runs on this machine"
    );
}

// ---- the click and the text of a diff (#647) -------------------------------------------------------

/// The output of the check `id` in the run of `stage`.
fn check_out(stage: &Stage, id: &str) -> String {
    std::fs::read_to_string(stage.path(&format!("out/{id}.out"))).unwrap()
}

/// The POST lines in the log of the counter in front of the observer that the script runs.
fn ask_posts(stage: &Stage) -> Vec<String> {
    std::fs::read_to_string(stage.path("out/ask.log"))
        .unwrap()
        .lines()
        .filter(|l| l.starts_with("POST "))
        .map(str::to_string)
        .collect()
}

/// A run of observer.sh with the variables `vars` set. Gives the exit code, the log of the steps,
/// and the stage.
fn observer_with(vars: &[(&str, &str)]) -> (Option<i32>, String, Stage) {
    let mut stage = observer_stage();
    for (key, value) in vars {
        stage.env(key, value);
    }
    let (o, steps) = run_observer(&stage, &[]);
    (o.status.code(), steps, stage)
}

/// The POST line of a validate request to the add-on version, as the stand-in counter writes it.
const ASK_VALIDATE_POST: &str = "POST /v1/clusterprofiles/0123456789abcdef01234567/validate/packs";

#[test]
fn observer_sh_ask_one_post_passes_when_one_click_makes_one_post_to_the_validate_route_and_a_second_click_makes_none()
 {
    let (code, steps, stage) = observer_with(&[]);
    assert_eq!(code, Some(0), "{steps}");
    passed(&steps, &["ask-one-post"]);
    let said = check_out(&stage, "ask-one-post");
    for line in [
        "the move: addon/cilium in strata-live-profile@1.0.0",
        "the cycles, before the click: 3 requests, 0 POST",
        "the first click: 1 POST, 1 of them to the validate route",
        "the second click: 0 POST",
    ] {
        assert!(said.contains(line), "{line}:\n{said}");
    }
    // the log of the counter holds the cycles and the one POST
    assert_eq!(ask_posts(&stage), [ASK_VALIDATE_POST], "{said}");
    let log = std::fs::read_to_string(stage.path("out/ask.log")).unwrap();
    assert_eq!(log.lines().count(), 4, "{log}");
    // the observer of the machine got both clicks, and the pod got the one click of ask-words
    assert_eq!(observer_log(&stage, "LOCAL.clicks").trim(), "2");
    assert_eq!(observer_log(&stage, "POD.clicks").trim(), "1");
}

#[test]
fn observer_sh_ask_one_post_runs_the_observer_of_this_checkout_behind_the_counter_with_the_records_and_the_key()
 {
    let (code, steps, stage) = observer_with(&[]);
    assert_eq!(code, Some(0), "{steps}");
    // the observer is built from the directory observer/ of the checkout, once
    let go = observer_log(&stage, "go.log");
    let build: Vec<&str> = go
        .lines()
        .filter(|l| l.contains("observer build"))
        .collect();
    assert_eq!(build.len(), 1, "{go}");
    assert!(
        build[0].starts_with("-C ") && build[0].contains("/tree/observer build -o "),
        "{build:?}"
    );
    // the flags: the cluster, the records, the short periods, and the port of the script plus 1
    let args = observer_log(&stage, "observer.args");
    let out = stage.path("out");
    for part in [
        "serve --strata ",
        "--cluster-uid c-a",
        "--content-every=1m --versions-every=5m",
        "--listen 127.0.0.1:18081",
    ] {
        assert!(args.contains(part), "{part}:\n{args}");
    }
    assert_eq!(args.lines().count(), 1, "{args}");
    assert!(
        !args.contains(OBSERVER_KEY),
        "the key is no argument: {args}"
    );
    // the state and the records are outside the directory of the run, and they are gone
    assert!(!args.contains(out.to_str().unwrap()), "{args}");
    let gone: Vec<_> = files_under(&stage.path("tmp"))
        .into_iter()
        .filter(|(path, _)| path.contains("records") || path.contains("state"))
        .collect();
    assert!(gone.is_empty(), "the files of the observer stay: {gone:?}");
    // the key is in the environment of the observer, and in no file of the run
    assert_eq!(
        observer_log(&stage, "observer.env").trim(),
        format!("PALETTE_API_KEY={OBSERVER_KEY}")
    );
    assert!(!all_text(&out).contains(OBSERVER_KEY));
    // the records are the ones of the run, in the files that the observer reads
    let meta: serde_json::Value =
        serde_json::from_str(&observer_log(&stage, "observer.meta")).unwrap();
    assert_eq!(meta["baseline"], "approved");
    assert_eq!(meta["revision"], "0123456789abcdef0123456789abcdef01234567");
    let mut names: Vec<String> = observer_log(&stage, "observer.records")
        .lines()
        .map(str::to_string)
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "meta.json",
            "strata-live-addon_1.0.0.json.gz",
            "strata-live-profile_1.0.0.json.gz"
        ]
    );
    for (name, record) in [
        ("strata-live-addon_1.0.0", "record-2.json"),
        ("strata-live-profile_1.0.0", "record-1.json"),
    ] {
        assert_eq!(
            observer_log(&stage, &format!("observer.record.{name}")),
            std::fs::read_to_string(stage.path("out").join(record)).unwrap(),
            "{name}"
        );
    }
}

#[test]
fn observer_sh_ask_one_post_fails_when_the_clicks_make_another_count_of_posts_than_one_and_none() {
    for (posts, first, second) in [("2 0", 2, 0), ("0 0", 0, 0), ("1 1", 1, 1), ("1 3", 1, 3)] {
        let (code, steps, stage) = observer_with(&[("FAKE_ASK_POSTS", posts)]);
        assert_eq!(code, Some(1), "{posts}:\n{steps}");
        failed(&steps, &["ask-one-post"]);
        passed(&steps, &["drift-found", "diff-text", "profile-removal"]);
        let said = check_out(&stage, "ask-one-post");
        assert!(
            said.contains(&format!("the first click: {first} POST")),
            "{posts}:\n{said}"
        );
        // the check shows the POST lines that it counted
        if first + second > 0 {
            assert!(said.contains(ASK_VALIDATE_POST), "{posts}:\n{said}");
        }
        if first == 1 {
            assert!(
                said.contains(&format!("the second click: {second} POST")),
                "{posts}:\n{said}"
            );
        } else {
            assert!(
                !said.contains("the second click"),
                "{posts}: the run stops at the first click:\n{said}"
            );
        }
    }
}

#[test]
fn observer_sh_ask_one_post_fails_when_a_cycle_sends_a_post_before_the_click_and_makes_no_click() {
    let (code, steps, stage) = observer_with(&[("FAKE_ASK_POSTS_BEFORE", "1")]);
    assert_eq!(code, Some(1), "{steps}");
    failed(&steps, &["ask-one-post"]);
    let said = check_out(&stage, "ask-one-post");
    assert!(
        said.contains("the cycles, before the click: 4 requests, 1 POST"),
        "{said}"
    );
    assert!(said.contains(ASK_VALIDATE_POST), "{said}");
    assert_eq!(
        observer_log(&stage, "LOCAL.clicks"),
        "",
        "no click on the observer of the machine"
    );
}

#[test]
fn observer_sh_ask_one_post_fails_when_the_first_click_gets_no_answer_because_the_second_click_asks_again()
 {
    let (code, steps, stage) = observer_with(&[("FAKE_ASK_LOCAL", "no-answer")]);
    assert_eq!(code, Some(1), "{steps}");
    failed(&steps, &["ask-one-post"]);
    let said = check_out(&stage, "ask-one-post");
    assert!(
        said.contains("the first click: 1 POST") && said.contains("the first click got no answer"),
        "{said}"
    );
    assert!(said.contains("asks again"), "{said}");
    assert_eq!(observer_log(&stage, "LOCAL.clicks").trim(), "1");
    // ask-words records the answers and does not fail for a verdict
    passed(&steps, &["ask-words"]);
}

#[test]
fn observer_sh_ask_one_post_tries_the_next_move_when_the_click_on_the_first_gets_a_404() {
    let (code, steps, stage) = observer_with(&[("FAKE_STALE_MOVE", "1")]);
    assert_eq!(code, Some(0), "{steps}");
    passed(&steps, &["ask-one-post", "ask-words"]);
    let said = check_out(&stage, "ask-one-post");
    assert!(
        said.contains("the move: addon/cilium in strata-live-profile@1.0.0"),
        "{said}"
    );
    // the click on the stale move sent nothing to Palette: the log holds the one POST
    assert_eq!(ask_posts(&stage), [ASK_VALIDATE_POST], "{said}");
    assert_eq!(observer_log(&stage, "LOCAL.clicks").trim(), "2");
}

#[test]
fn observer_sh_ask_one_post_fails_when_the_observer_of_the_machine_stops_or_lists_no_move() {
    let (code, steps, stage) = observer_with(&[("FAKE_OBSERVER_DIES", "1")]);
    assert_eq!(code, Some(1), "{steps}");
    failed(&steps, &["ask-one-post"]);
    assert!(
        check_out(&stage, "ask-one-post").contains("the observer on this machine"),
        "{}",
        check_out(&stage, "ask-one-post")
    );
    assert_eq!(observer_log(&stage, "LOCAL.clicks"), "");

    let (code, steps, stage) = observer_with(&[("FAKE_NO_MOVE", "1")]);
    assert_eq!(code, Some(1), "{steps}");
    failed(&steps, &["ask-one-post", "ask-words"]);
    let said = check_out(&stage, "ask-one-post");
    assert!(
        said.contains("lists no move to a newer pack version"),
        "{said}"
    );
    // the observer of the machine stops at the end of the check, also after a failure
    assert!(
        !stage.path("obs/LOCAL.clicks").exists() && stage.path("obs/observer.up").exists(),
        "the observer ran and got no click"
    );
}

#[test]
fn observer_sh_ask_words_records_each_verdict_with_the_code_or_the_status_and_judges_none() {
    for (verdict, words) in [
        ("no-error", "no-error: Palette gave no error"),
        (
            "refuses",
            "refuses DependencyPackUpgradeNotSupported: Palette refuses: DependencyPackUpgradeNotSupported",
        ),
        ("no-answer", "no-answer, status 403: Palette gave no answer"),
    ] {
        let (code, steps, stage) = observer_with(&[("FAKE_ASK_POD", verdict)]);
        assert_eq!(code, Some(0), "{verdict}:\n{steps}");
        passed(&steps, &["ask-words"]);
        let said = check_out(&stage, "ask-words");
        assert!(
            said.contains(&format!("the pod: {words}")),
            "{verdict}:\n{said}"
        );
        // the observer of the machine answers with no error: both answers are in the record
        assert!(
            said.contains("the observer on this machine: no-error: Palette gave no error"),
            "{verdict}:\n{said}"
        );
        // the click on the pod uses the port-forward, and it carries the header of the page
        assert_eq!(observer_log(&stage, "POD.clicks").trim(), "1", "{verdict}");
        let pod: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(stage.path("out/ask-pod.json")).unwrap())
                .unwrap();
        assert_eq!(pod["verdict"], verdict);
        let asks: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(stage.path("out/asks-pod.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(asks["asks"], serde_json::json!([pod]), "/v1/asks lists it");
    }
}

#[test]
fn observer_sh_ask_words_keeps_a_refusal_whose_message_holds_the_word_accepts() {
    // the message is the text of Palette, and the word of the API is the word of the answer
    let (code, steps, stage) = observer_with(&[("FAKE_ASK_POD", "refuses-accepts-message")]);
    assert_eq!(code, Some(0), "{steps}");
    passed(&steps, &["ask-words"]);
    let said = check_out(&stage, "ask-words");
    assert!(
        said.contains("the pod: refuses InvalidValues: Palette refuses: InvalidValues"),
        "{said}"
    );
}

#[test]
fn observer_sh_ask_words_fails_on_the_word_accepts_in_an_answer_or_in_a_route() {
    for (name, vars, holder) in [
        (
            "the pod",
            vec![("FAKE_ASK_POD", "accepts-field")],
            "ask-pod.json",
        ),
        (
            "the observer of the machine",
            vec![("FAKE_ASK_LOCAL", "accepts-field")],
            "ask-first.json",
        ),
        (
            "a summary route",
            vec![("FAKE_ACCEPTS_IN", "/v1/newer")],
            "/v1/newer",
        ),
        (
            "the list of answers",
            vec![("FAKE_ASK_POD", "accepts-field")],
            "asks-pod.json",
        ),
    ] {
        let (code, steps, stage) = observer_with(&vars);
        assert_eq!(code, Some(1), "{name}:\n{steps}");
        failed(&steps, &["ask-words"]);
        let said = check_out(&stage, "ask-words");
        assert!(
            said.contains(&format!("{holder} holds the word accepts")),
            "{name}:\n{said}"
        );
        // the checks around it still run and pass
        passed(&steps, &["diff-text", "profile-removal"]);
    }
}

#[test]
fn observer_sh_ask_words_fails_on_an_answer_without_a_verdict_or_a_code() {
    for (verdict, said_part) in [
        ("unknown", "the pod: unknown verdict maybe"),
        (
            "refuses-no-code",
            "the pod: the answer has no verdict, no code, or no status",
        ),
    ] {
        let (code, steps, stage) = observer_with(&[("FAKE_ASK_POD", verdict)]);
        assert_eq!(code, Some(1), "{verdict}:\n{steps}");
        failed(&steps, &["ask-words"]);
        let said = check_out(&stage, "ask-words");
        assert!(said.contains(said_part), "{verdict}:\n{said}");
    }
}

#[test]
fn observer_sh_diff_text_passes_when_the_detail_route_holds_the_text_of_the_edit_and_no_summary_route_does()
 {
    let (code, steps, stage) = observer_with(&[]);
    assert_eq!(code, Some(0), "{steps}");
    passed(&steps, &["diff-text"]);
    let said = check_out(&stage, "diff-text");
    assert!(
        said.contains("the detail route holds the text of the edit for addon/strata-live-config"),
        "{said}"
    );
    assert!(
        said.contains("8 summary routes hold no text of the edit"),
        "{said}"
    );
    // the answer of the detail route is the one of the pack that the finding names
    let diff: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(stage.path("out/diff-1.json")).unwrap())
            .unwrap();
    assert!(
        diff["manifests"][0]["unified"]
            .as_str()
            .unwrap()
            .contains("+edited: \"yes\""),
        "{diff}"
    );
}

#[test]
fn observer_sh_diff_text_fails_when_a_summary_route_holds_the_text_of_the_edit() {
    let (code, steps, stage) = observer_with(&[("FAKE_TEXT_LEAK", "1")]);
    assert_eq!(code, Some(1), "{steps}");
    failed(&steps, &["diff-text"]);
    let said = check_out(&stage, "diff-text");
    assert!(
        said.contains("the route /v1/events holds the text of the edit"),
        "{said}"
    );
    // the detail route did hold it, so the search of the summary routes had a text to find
    assert!(
        said.contains("the detail route holds the text of the edit for addon/strata-live-config"),
        "{said}"
    );
    passed(&steps, &["no-value-leaves", "ask-words"]);
}

#[test]
fn observer_sh_diff_text_fails_when_the_detail_route_holds_no_text_of_the_edit() {
    let (code, steps, stage) = observer_with(&[("FAKE_NO_DIFF_TEXT", "1")]);
    assert_eq!(code, Some(1), "{steps}");
    failed(&steps, &["diff-text"]);
    let said = check_out(&stage, "diff-text");
    assert!(
        said.contains("the detail route holds no text of the edit for addon/strata-live-config"),
        "{said}"
    );
    assert!(!said.contains("summary routes hold no text"), "{said}");
}

#[test]
fn the_page_of_live_runs_names_each_new_check_of_observer_sh_and_of_observer_role_sh_with_its_result()
 {
    for (script, ids) in [
        (
            "observer.sh",
            vec!["ask-one-post", "ask-words", "diff-text"],
        ),
        (
            "observer-role.sh",
            vec![
                "validate-body",
                "control-validate-packs",
                "role-validate-packs",
            ],
        ),
    ] {
        let in_script = script_check_ids(script);
        let page = page_section("docs/src/development/live-runs.md", &format!("## {script}"));
        let on_page = page_check_ids(script);
        for id in ids {
            assert!(in_script.iter().any(|i| i == id), "{script} lacks {id}");
            assert!(on_page.iter().any(|i| i == id), "the page lacks {id}");
            let row = page
                .lines()
                .find(|l| l.starts_with("| `") && l.contains(&format!("`{id}`")))
                .unwrap_or_else(|| panic!("no row for {id}"));
            assert!(
                !row.to_lowercase().contains("no result yet"),
                "the row of {id} says that it has no result:\n{row}"
            );
        }
    }
    // the two checks that the lead named with `?` or 0
    let source = repo_file("examples/live/observer.sh");
    for id in ["ask-one-post", "ask-words", "diff-text"] {
        assert!(source.contains(&format!("check {id} 0 ")), "{id} expects 0");
    }
    assert!(
        repo_file("examples/live/observer-role.sh").contains("check role-validate-packs ? "),
        "the request of the role is a check with ?"
    );
}

#[test]
fn observer_sh_takes_the_image_by_its_tag_or_its_digest_and_refuses_a_name_with_neither() {
    let mut stage = observer_stage();
    stage.env(
        "OBSERVER_IMAGE",
        "ghcr.io/como-technologies/strata-observer@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    );
    let (o, steps) = run_observer(&stage, &[]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    let kustomization = std::fs::read_to_string(stage.path("out/kustomization.yaml")).unwrap();
    assert!(
        kustomization.contains("newName: ghcr.io/como-technologies/strata-observer\n")
            && kustomization.contains("digest: sha256:0123456789abcdef"),
        "{kustomization}"
    );
    assert!(!kustomization.contains("newTag"), "{kustomization}");

    let mut stage = observer_stage();
    stage.env(
        "OBSERVER_IMAGE",
        "registry.example.com:5000/strata-observer",
    );
    let (o, _) = run_observer(&stage, &[]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("OBSERVER_IMAGE needs a tag or a digest"),
        "{}",
        text(&o.stderr)
    );
    assert!(
        observer_log(&stage, "kubectl.log").is_empty(),
        "nothing ran in the cluster"
    );
}

#[test]
fn observer_sh_keeps_the_install_for_a_resume_when_keep_is_1_and_refuses_a_stage_that_does_not_exist()
 {
    let mut stage = observer_stage();
    stage.env("FAKE_NO_PALETTE", "1");
    stage.env("CYCLE_TRIES", "2");
    stage.env("KEEP", "1");
    let (o, steps) = run_observer(&stage, &[]);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    assert!(!observer_log(&stage, "kubectl.log").contains("delete namespace"));
    assert!(stage.path("obs/ns.exists").exists(), "the install stays");

    let stage = observer_stage();
    let (o, _) = run_observer(&stage, &["8"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stdout).contains("examples/live/observer.sh [FROM_STAGE]"),
        "{}",
        text(&o.stdout)
    );
}

#[test]
fn observer_sh_fails_the_first_sight_check_when_a_restart_keeps_the_state_and_restores_the_version()
{
    let mut stage = observer_stage();
    stage.env("FAKE_STATE_KEPT", "1");
    let (o, steps) = run_observer(&stage, &[]);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    failed(&steps, &["first-seen-starts-again"]);
    passed(
        &steps,
        &["restart-keeps-working", "drift-clears", "undo-namespace"],
    );
    let api = observer_log(&stage, "api.log");
    let edits = api.lines().filter(|l| l.starts_with("edit ")).count();
    assert_eq!(edits, 4, "two edits and two undos: {api}");
    assert!(!stage.path("obs/drifted").exists());
}

// ---- the least role of the key of the observer (#645) ----------------------------------------------

/// A `strata` that logs each call with the name of the key it read. The name is `role` for the
/// second key, and `lab` for the key of the example. A real key never appears in the log.
const FAKE_STRATA_ROLE: &str = r#"#!/usr/bin/env bash
who=lab; [ "${PALETTE_API_KEY:-}" = role-k ] && who=role
echo "$who $*" >> "$FAKE_STRATA/keys"
exec "$(dirname "$0")/fake-strata-rules" "$@"
"#;

/// The requests of the cycles that `observer-role.sh` sends, with the paths of the baseline stage.
const OBSERVER_ROLE_ROUTES: [(&str, &str); 16] = [
    ("cluster", "/v1/spectroclusters/c-a"),
    (
        "cluster-manifests",
        "/v1/spectroclusters/c-a/profiles/packs/manifests",
    ),
    ("cluster-variables", "/v1/spectroclusters/c-a/variables"),
    ("cluster-rbacs", "/v1/spectroclusters/c-a/config/rbacs"),
    (
        "cluster-namespaces",
        "/v1/spectroclusters/c-a/config/namespaces",
    ),
    ("notifications", "/v1/notifications/spectrocluster/c-a"),
    ("profile", "/v1/clusterprofiles/ad-1"),
    (
        "profile-manifests",
        "/v1/clusterprofiles/ad-1/packs/manifests",
    ),
    ("profile-variables", "/v1/clusterprofiles/ad-1/variables"),
    ("audits", "/v1/audits"),
    ("registries", "/v1/registries/metadata"),
    ("registry-pack-summary", "/v1/registries/pack/summary"),
    ("registry-helm-summary", "/v1/registries/helm/summary"),
    ("registry-oci-summary", "/v1/registries/oci/summary"),
    ("packs", "/v1/packs"),
    ("pack-tags", "/v1/packs/cert-manager/registries/reg-1"),
];

/// The path of the validate request that `observer-role.sh` sends, for the add-on version `ad-1`.
const OBSERVER_ROLE_VALIDATE: &str = "/v1/clusterprofiles/ad-1/validate/packs";

/// The record of the add-on version, as strata writes it: one pack with values and one manifest. The
/// record holds more than the validate body takes, such as the hashes.
fn observer_role_record() -> serde_json::Value {
    serde_json::json!({
        "schema": "strata-snapshot/1",
        "source": {"uid": "ad-1", "name": "strata-live-addon", "version": "1.0.0", "type": "add-on",
                   "cloudType": "all"},
        "packs": [{
            "key": "addon/cert-manager", "layer": "addon", "name": "cert-manager", "type": "spectro",
            "tag": "1.21.1", "version": "1.21.1", "packUid": "pk-1", "registryUid": "reg-1",
            "values": "installCRDs: true\n", "valuesHash": "h-1",
            "manifests": [{"name": "config", "uid": "m-1", "digest": "d-1",
                           "content": "edited: \"no\"\n", "contentHash": "c-1"}],
        }],
    })
}

/// The body that the validate request must carry for `observer_role_record`: the shape that the
/// click of the observer sends.
fn observer_role_validate_body() -> serde_json::Value {
    serde_json::json!({
        "cloudType": "all", "type": "add-on",
        "packs": [{
            "name": "cert-manager", "tag": "1.21.1", "uid": "pk-1", "registryUid": "reg-1",
            "type": "spectro", "layer": "addon", "values": "installCRDs: true\n",
            "manifests": [{"name": "config", "content": "edited: \"no\"\n"}],
        }],
    })
}

/// A stage of `observer-role.sh`: Palette answers each request to the key of the example. The
/// registry listing names `Public Repo`, and the validate route takes the add-on version.
fn observer_role_stage() -> Stage {
    let mut stage = baseline_stage();
    stage.wrap_strata(FAKE_STRATA_ROLE);
    for (_, path) in OBSERVER_ROLE_ROUTES {
        stage.route(path, "{}");
    }
    stage.route(OBSERVER_ROLE_VALIDATE, r#"{"packs":{"results":[]}}"#);
    stage.strata("snapshot ad-1 -o *", 0, &observer_role_record().to_string());
    stage.route(
        "/v1/registries/metadata",
        r#"{"items":[{"name":"Public Repo","uid":"reg-1"}]}"#,
    );
    stage.env("OBSERVER_ROLE", "Project Viewer");
    stage.env("OBSERVER_ROLE_KEY", "role-k");
    stage
}

#[test]
fn observer_role_sh_records_the_answer_of_each_request_to_the_key_of_the_role() {
    let stage = observer_role_stage();
    // the second key lacks the permission of the audit log: Palette refuses the whole route
    stage.route_as(
        "/v1/audits",
        "role-k",
        403,
        r#"{"code":"AccessDenied","message":"no audit.list"}"#,
    );
    let (o, steps) = stage.run("observer-role.sh", &[]);
    assert_eq!(o.status.code(), Some(0), "{steps}\n{}", text(&o.stderr));
    for (name, _) in OBSERVER_ROLE_ROUTES {
        assert!(
            has_verdict(&steps, "PASS", &format!("control-{name}")),
            "control-{name} passes:\n{steps}"
        );
        assert!(
            has_verdict(&steps, "SEEN", &format!("role-{name}")),
            "role-{name} is recorded:\n{steps}"
        );
    }
    let said = |id: &str| std::fs::read_to_string(stage.path(&format!("out/{id}.out"))).unwrap();
    assert_eq!(said("role-audits").trim(), "HTTP 403 AccessDenied");
    assert_eq!(said("role-cluster").trim(), "HTTP 200");
    let line = step_line(&steps, "role-audits");
    assert!(
        line.starts_with("SEEN") && line.ends_with("got 1"),
        "a refusal is recorded as a failed request:\n{steps}"
    );
    passed(
        &steps,
        &[
            "command-record",
            "command-control-verify",
            "command-control-cluster",
        ],
    );
    for id in [
        "command-role-verify",
        "command-role-cluster",
        "command-control-behind",
        "command-role-behind",
    ] {
        assert!(
            has_verdict(&steps, "SEEN", id),
            "{id} is recorded:\n{steps}"
        );
    }
}

#[test]
fn observer_role_sh_gives_the_second_key_to_strata_in_the_environment_and_saves_it_nowhere() {
    let stage = observer_role_stage();
    let (o, steps) = stage.run("observer-role.sh", &[]);
    assert_eq!(o.status.code(), Some(0), "{steps}\n{}", text(&o.stderr));
    let keys = std::fs::read_to_string(stage.path("strata/keys")).unwrap();
    for call in [
        "role verify addon.json --who",
        "role snapshot cluster:c-a -o cluster-role.json",
        "role behind addon.json --no-impact --json --compact",
        "lab verify addon.json --who",
    ] {
        assert!(keys.lines().any(|l| l == call), "{call} in:\n{keys}");
    }
    // the key is in no argument of a request, in no file of the run, and in no output
    let curl = std::fs::read_to_string(stage.path("curl.log")).unwrap();
    assert!(!curl.contains("role-k"), "{curl}");
    for (name, body) in files_under(&stage.path("out")) {
        assert!(!body.contains("role-k"), "{name} holds the key");
    }
    assert!(!text(&o.stdout).contains("role-k") && !text(&o.stderr).contains("role-k"));
}

#[test]
fn observer_role_sh_stops_before_any_request_when_it_has_no_key_or_no_role_name() {
    for missing in ["OBSERVER_ROLE_KEY", "OBSERVER_ROLE"] {
        let mut stage = observer_role_stage();
        stage.env.retain(|(k, _)| k != missing);
        let (o, steps) = stage.run("observer-role.sh", &[]);
        assert_eq!(o.status.code(), Some(2), "{missing}");
        assert!(
            text(&o.stderr).contains("could not check"),
            "{}",
            text(&o.stderr)
        );
        assert!(steps.is_empty(), "no stage ran: {steps}");
        assert!(!stage.path("routes/sent").exists(), "no request to Palette");
    }
}

#[test]
fn observer_role_sh_sends_the_validate_request_with_the_body_of_the_record_with_both_keys() {
    let stage = observer_role_stage();
    let (o, steps) = stage.run("observer-role.sh", &[]);
    assert_eq!(o.status.code(), Some(0), "{steps}\n{}", text(&o.stderr));
    passed(&steps, &["validate-body", "control-validate-packs"]);
    assert!(
        has_verdict(&steps, "SEEN", "role-validate-packs"),
        "{steps}"
    );
    // one POST for each key, and each carries the body that a record gives
    let curl = std::fs::read_to_string(stage.path("curl.log")).unwrap();
    let posts: Vec<&str> = curl.lines().filter(|l| l.starts_with("POST ")).collect();
    let want_post = format!("POST {OBSERVER_ROLE_VALIDATE}");
    assert_eq!(posts, [want_post.as_str(), want_post.as_str()], "{curl}");
    let sent = stage.sent("POST", OBSERVER_ROLE_VALIDATE);
    let want_body = observer_role_validate_body();
    assert_eq!(sent, [want_body.clone(), want_body], "{sent:?}");
    // the record is the one of the add-on version, taken with the key of the example
    let keys = std::fs::read_to_string(stage.path("strata/keys")).unwrap();
    assert!(
        keys.lines().any(|l| l == "lab snapshot ad-1 -o addon.json"),
        "{keys}"
    );
    let said = |id: &str| std::fs::read_to_string(stage.path(&format!("out/{id}.out"))).unwrap();
    assert_eq!(said("control-validate-packs").trim(), "HTTP 200");
    assert_eq!(said("role-validate-packs").trim(), "HTTP 200");
}

#[test]
fn observer_role_sh_records_the_refusal_of_the_validate_request_to_the_role_and_judges_none_of_it()
{
    for (status, code) in [(403, "AccessDenied"), (500, "InternalError"), (422, "")] {
        let stage = observer_role_stage();
        let body = if code.is_empty() {
            "{}".to_string()
        } else {
            format!(r#"{{"code":"{code}","message":"no validate"}}"#)
        };
        stage.route_as(OBSERVER_ROLE_VALIDATE, "role-k", status, &body);
        let (o, steps) = stage.run("observer-role.sh", &[]);
        assert_eq!(o.status.code(), Some(0), "{status}:\n{steps}");
        passed(&steps, &["control-validate-packs"]);
        let line = step_line(&steps, "role-validate-packs");
        assert!(
            line.starts_with("SEEN") && line.ends_with("got 1"),
            "a refusal is recorded as a failed request:\n{steps}"
        );
        let said = std::fs::read_to_string(stage.path("out/role-validate-packs.out")).unwrap();
        assert_eq!(said.trim(), format!("HTTP {status} {code}").trim());
    }
}

#[test]
fn observer_role_sh_fails_the_validate_request_of_the_example_key_when_palette_refuses_it() {
    let stage = observer_role_stage();
    stage.route_as(
        OBSERVER_ROLE_VALIDATE,
        "k",
        400,
        r#"{"code":"ShouldNotBeEmpty"}"#,
    );
    let (o, steps) = stage.run("observer-role.sh", &[]);
    assert_eq!(o.status.code(), Some(1), "{steps}\n{}", text(&o.stderr));
    failed(&steps, &["control-validate-packs"]);
    passed(&steps, &["validate-body", "control-cluster"]);
    let said = std::fs::read_to_string(stage.path("out/control-validate-packs.out")).unwrap();
    assert_eq!(said.trim(), "HTTP 400 ShouldNotBeEmpty");
}

#[test]
fn observer_role_sh_fails_the_body_check_and_sends_no_post_when_strata_gives_no_record() {
    let stage = observer_role_stage();
    // the only rule of the stage fails, and it writes no record for the add-on version
    std::fs::write(
        stage.path("strata/rules"),
        "snapshot ad-1 -o *\t3\tanswer-1\terr\n",
    )
    .unwrap();
    let (o, steps) = stage.run("observer-role.sh", &[]);
    assert_eq!(o.status.code(), Some(1), "{steps}\n{}", text(&o.stderr));
    failed(&steps, &["validate-body", "control-validate-packs"]);
    let curl = std::fs::read_to_string(stage.path("curl.log")).unwrap();
    assert!(!curl.contains("POST "), "no POST without a body:\n{curl}");
}

/// A `strata` for who.sh stage 3. It knows the key `noaudit-k`, which cannot read the audit log:
/// the verdict stays, and `--who` says that it could not read the log. Each call goes to the log
/// `calls` with the key that it got.
const FAKE_STRATA_NO_AUDIT: &str = r#"#!/usr/bin/env bash
echo "key=${PALETTE_API_KEY:-} $*" >> "$FAKE_STRATA/calls"
refused=; [ "${PALETTE_API_KEY:-}" = noaudit-k ] && refused=1
case "$*" in
  "snapshot prof-1 -o noaudit.json") echo '{"source":{"fetchedAt":"2026-11-04T09:00:00Z"}}' > noaudit.json ;;
  "verify noaudit.json --who")
    if [ -n "$refused" ]; then
      echo 'DRIFT: published version strata-live-addon@1.0.0 differs'
      echo 'who: not read: HTTP 403: {"code":"AccessDenied"}'
      exit 1
    fi ;;
  "verify noaudit.json --who --json")
    if [ -n "$refused" ]; then
      echo '{"verify":{"status":"drifted","whoError":"HTTP 403: {\"code\":\"AccessDenied\"}"}}'
      exit 1
    fi ;;
esac
exit 0
"#;

/// A stage of who.sh whose add-on version is `prof-1`. The manifest of the version says what the
/// last `PUT` sent, so that an edit and its undo both find their text.
fn no_audit_stage() -> Stage {
    let stage = Stage::new(serde_json::json!({}));
    stage.command("fake-strata", FAKE_STRATA_NO_AUDIT);
    let routes = stage.path("routes");
    addon_profile_routes(&routes);
    executable(
        &routes.join("v1_clusterprofiles_prof-1_packs_manifests.sh"),
        r#"#!/usr/bin/env bash
content='edited: "no"'
sent="$FAKE_ROUTES/sent/PUT_v1_clusterprofiles_prof-1"
[ -f "$sent" ] && content=$(jq -r '.spec.template.packs[0].manifests[0].content' "$sent")
jq -n --arg c "$content" '{metadata: {}, spec: {packs: [{metadata: {}, spec: {name: "config", manifests: [{name: "config", uid: "m-1", digest: "d", content: $c}]}}]}}'
"#,
    );
    // stage 1 saved the uid of the version in an earlier run
    std::fs::create_dir_all(stage.path("out")).unwrap();
    std::fs::write(stage.path("out/ids.env"), "AD=prof-1\n").unwrap();
    stage
}

#[test]
fn who_sh_stage_3_makes_sure_that_a_key_with_no_audit_permission_keeps_the_verdict() {
    let mut stage = no_audit_stage();
    stage.env("STRATA_LIVE_NO_AUDIT_KEY", "noaudit-k");

    let (o, steps) = stage.run("who.sh", &["3"]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    assert!(!steps.contains("FAIL"), "{steps}");
    passed(
        &steps,
        &[
            "noaudit-record",
            "noaudit-edit",
            "noaudit-who",
            "noaudit-who-verdict",
            "noaudit-who-says",
            "noaudit-who-json",
            "noaudit-undo",
            "noaudit-restored",
        ],
    );
    // the key is for the two reads of the verdict, and the edit and the record use the key of the lab
    let calls = std::fs::read_to_string(stage.path("strata/calls")).unwrap();
    assert_eq!(
        calls
            .lines()
            .filter(|l| !l.ends_with("--version"))
            .collect::<Vec<_>>(),
        [
            "key=k snapshot prof-1 -o noaudit.json",
            "key=noaudit-k verify noaudit.json --who",
            "key=noaudit-k verify noaudit.json --who --json",
            "key=k verify noaudit.json",
        ]
    );
    // the edit and its undo each sent one PUT
    let texts: Vec<_> = stage
        .sent("PUT", "/v1/clusterprofiles/prof-1")
        .iter()
        .map(|b| b["spec"]["template"]["packs"][0]["manifests"][0]["content"].clone())
        .collect();
    assert_eq!(texts.len(), 2, "{texts:?}");
    assert_ne!(texts[0], texts[1], "the undo puts the text back: {texts:?}");
    let printed = format!("{}{}", text(&o.stdout), text(&o.stderr));
    assert!(
        !printed.contains("noaudit-k"),
        "the key is not printed: {printed}"
    );
    for (name, content) in files_under(&stage.path("out")) {
        assert!(!content.contains("noaudit-k"), "{name} holds the key");
    }
}

#[test]
fn who_sh_stage_3_fails_the_check_of_the_verdict_when_the_key_still_reads_the_audit_log() {
    let mut stage = no_audit_stage();
    // a key that reads the log: the fake gives no refusal to it
    stage.env("STRATA_LIVE_NO_AUDIT_KEY", "reader-k");

    let (o, steps) = stage.run("who.sh", &["3"]);
    assert_eq!(o.status.code(), Some(1), "{steps}{}", text(&o.stderr));
    failed(
        &steps,
        &["noaudit-who", "noaudit-who-says", "noaudit-who-json"],
    );
}

#[test]
fn who_sh_stage_3_records_nothing_and_says_so_without_the_key() {
    let stage = no_audit_stage();

    let (o, steps) = stage.run("who.sh", &["3"]);
    assert_eq!(o.status.code(), Some(0), "{steps}{}", text(&o.stderr));
    assert!(steps.contains("STRATA_LIVE_NO_AUDIT_KEY"), "{steps}");
    assert!(!steps.contains("noaudit-"), "{steps}");
    // only the report of the run asks strata for its version
    let calls = std::fs::read_to_string(stage.path("strata/calls")).unwrap_or_default();
    assert!(
        calls.lines().all(|l| l.ends_with("--version")),
        "no call to strata without the key: {calls}"
    );
}

/// A recipe of the justfile runs a script of `examples/live/` by its path, so Git must record the
/// script with the mode 100755. With the mode 100644, the recipe stops with "Permission denied".
/// The tests run each script with `bash`, so only this test sees the fault.
#[test]
fn each_script_of_the_live_example_that_a_recipe_runs_has_the_mode_100755_in_git() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let listing = git(
        root,
        &[
            "-c",
            "safe.directory=*",
            "ls-files",
            "-s",
            "--",
            "examples/live",
        ],
    )
    .stdout;
    let listing = text(&listing);
    // a line of the listing: `<mode> <hash> <stage>\t<path>`
    let modes: std::collections::BTreeMap<&str, &str> = listing
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(info, path)| (path, info.split(' ').next().unwrap_or_default()))
        .collect();

    let justfile = repo_file("justfile");
    let run: std::collections::BTreeSet<&str> = code_lines(&justfile)
        .flat_map(str::split_whitespace)
        .filter(|word| word.starts_with("examples/live/") && word.ends_with(".sh"))
        .collect();
    assert!(
        run.contains("examples/live/observer-up.sh"),
        "the scan finds the scripts of the recipes: {run:?}"
    );

    let wrong: Vec<String> = run
        .iter()
        .filter(|path| modes.get(*path) != Some(&"100755"))
        .map(|path| format!("{path}: {}", modes.get(*path).unwrap_or(&"not in Git")))
        .collect();
    assert!(
        wrong.is_empty(),
        "a recipe runs these scripts, and Git does not record them as 100755 \
         (git update-index --chmod=+x <file>): {wrong:?}"
    );
}
