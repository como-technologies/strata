//! The shell scripts under scripts/, run through bash against temporary repositories and the
//! built binary. No network: the GitHub CLI is replaced by a recording stand-in where needed.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn script(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join(name)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Run git in `dir` with no user or global configuration, so signing settings cannot leak in.
fn git(dir: &Path, args: &[&str]) -> Output {
    let o = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .unwrap();
    assert!(o.status.success(), "git {args:?}: {}", text(&o.stderr));
    o
}

fn tag_release(dir: &Path) -> Output {
    Command::new("bash")
        .arg(script("tag-release.sh"))
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .unwrap()
}

#[test]
fn tag_release_tags_the_cargo_version_once_and_only_on_a_clean_tree() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    git(d, &["init", "-q", "-b", "main"]);
    std::fs::write(
        d.join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.9.9\"\n",
    )
    .unwrap();
    git(d, &["add", "Cargo.toml"]);
    git(d, &["commit", "-q", "-m", "init"]);

    let o = tag_release(d);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert_eq!(text(&o.stdout).trim(), "v0.9.9", "prints the tag it made");
    assert!(
        text(&o.stderr).contains("git push origin v0.9.9"),
        "tells the user how to push, and does not push itself: {}",
        text(&o.stderr)
    );
    let tags = git(d, &["tag", "--list"]);
    assert_eq!(text(&tags.stdout).trim(), "v0.9.9");
    let kind = git(d, &["cat-file", "-t", "v0.9.9"]);
    assert_eq!(text(&kind.stdout).trim(), "tag", "an annotated tag");

    let again = tag_release(d);
    assert_eq!(again.status.code(), Some(1));
    assert!(
        text(&again.stderr).contains("v0.9.9 already exists"),
        "{}",
        text(&again.stderr)
    );

    std::fs::write(
        d.join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.9.10\"\n",
    )
    .unwrap();
    let dirty = tag_release(d);
    assert_eq!(dirty.status.code(), Some(1));
    assert!(
        text(&dirty.stderr).contains("uncommitted changes"),
        "{}",
        text(&dirty.stderr)
    );
    let tags = git(d, &["tag", "--list"]);
    assert_eq!(
        text(&tags.stdout).trim(),
        "v0.9.9",
        "no tag on a dirty tree"
    );
}

// ---- pr-comment.sh ---------------------------------------------------------------------------

/// A `gh` that records every invocation, saves any comment body it is handed, and answers the
/// two listings the script reads from files the test controls.
const FAKE_GH: &str = r#"#!/usr/bin/env bash
set -euo pipefail
args="$*"
echo "$args" >> "$FAKE_GH_LOG"
prev=""
for a in "$@"; do
  case "$a" in body=@*) cp "${a#body=@}" "$FAKE_GH_SAVED_BODY" ;; esac
  [ "$prev" = "--body-file" ] && cp "$a" "$FAKE_GH_SAVED_BODY"
  prev="$a"
done
case "$args" in
  *"-X PATCH"*) ;;
  "label create"*) ;;
  "label list"*) if [ -n "${FAKE_GH_REPO_LABELS:-}" ]; then cat "$FAKE_GH_REPO_LABELS"; else echo "[]"; fi ;;
  *"/comments"*) if [ -n "${FAKE_GH_COMMENTS:-}" ]; then cat "$FAKE_GH_COMMENTS"; else echo "[]"; fi ;;
  "pr view"*"--json labels"*) if [ -n "${FAKE_GH_LABELS:-}" ]; then cat "$FAKE_GH_LABELS"; else echo '{"labels":[]}'; fi ;;
  "pr view"*"--json number"*) echo '{"number":7}' ;;
  "repo view"*) echo '{"nameWithOwner":"o/r"}' ;;
esac
"#;

/// A `strata` that logs every call and answers `snapshot <ref> -o <file>` with a canned snapshot,
/// so a live ref needs no Palette; everything else goes to the real binary.
const FAKE_STRATA: &str = r#"#!/usr/bin/env bash
set -euo pipefail
echo "$*" >> "$FAKE_STRATA_LOG"
if [ "$1" = snapshot ]; then
  cp "$FAKE_STRATA_SNAPSHOT" "$4"
  exit 0
fi
exec "$REAL_STRATA" "$@"
"#;

struct Host {
    _dir: tempfile::TempDir,
    bin: PathBuf,
    log: PathBuf,
    body: PathBuf,
    comments: PathBuf,
    labels: PathBuf,
    repo_labels: PathBuf,
    home: PathBuf,
    a: String,
    a2: String,
    b: String,
    not_a_record: String,
}

/// Snapshot files for fixtures a, a2 and b, plus the fake `gh` on its own bin directory.
fn host() -> Host {
    let dir = tempfile::tempdir().unwrap();
    let raw = dir.path().join("raw");
    common::make_fixtures(&raw);
    let snap = |name: &str| -> String {
        let out = dir.path().join(format!("{name}.json"));
        let o = Command::new(env!("CARGO_BIN_EXE_strata"))
            .args(["snapshot", "--from-raw"])
            .arg(raw.join(name))
            .arg("-o")
            .arg(&out)
            .env("STRATA_QUIET", "1")
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", text(&o.stderr));
        out.to_str().unwrap().to_string()
    };
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    for (name, body) in [("gh", FAKE_GH), ("fake-strata", FAKE_STRATA)] {
        let path = bin.join(name);
        std::fs::write(&path, body).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    Host {
        a: snap("a"),
        a2: snap("a2"),
        b: snap("b"),
        not_a_record: raw.join("a/profile.json").display().to_string(),
        bin,
        log: dir.path().join("gh.log"),
        body: dir.path().join("saved-body.md"),
        comments: dir.path().join("comments.json"),
        labels: dir.path().join("labels.json"),
        repo_labels: dir.path().join("repo-labels.json"),
        home: {
            let h = dir.path().join("home");
            std::fs::create_dir(&h).unwrap();
            h
        },
        _dir: dir,
    }
}

impl Host {
    fn run(&self, args: &[&str]) -> Output {
        self.run_with(args, &[])
    }

    fn run_with(&self, args: &[&str], envs: &[(&str, &str)]) -> Output {
        let _ = std::fs::remove_file(&self.log);
        let _ = std::fs::remove_file(&self.body);
        let path = format!(
            "{}:{}",
            self.bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut c = Command::new("bash");
        c.arg(script("pr-comment.sh"))
            .args(args)
            .env("PATH", path)
            .env("STRATA_BIN", env!("CARGO_BIN_EXE_strata"))
            .env("STRATA_QUIET", "1")
            .env("FAKE_GH_LOG", &self.log)
            .env("FAKE_GH_SAVED_BODY", &self.body)
            .env("HOME", &self.home)
            .env_remove("GITHUB_REPOSITORY")
            .env_remove("PALETTE_API_KEY")
            .env_remove("SPECTROCLOUD_APIKEY");
        if self.repo_labels.exists() {
            c.env("FAKE_GH_REPO_LABELS", &self.repo_labels);
        }
        for (k, v) in envs {
            c.env(k, v);
        }
        if self.comments.exists() {
            c.env("FAKE_GH_COMMENTS", &self.comments);
        }
        if self.labels.exists() {
            c.env("FAKE_GH_LABELS", &self.labels);
        }
        c.output().unwrap()
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn saved_body(&self) -> String {
        std::fs::read_to_string(&self.body).expect("gh was handed a comment body")
    }
}

#[test]
fn pr_comment_posts_one_marked_comment_and_adds_the_label_at_the_threshold() {
    let h = host();
    let o = h.run(&[&h.a, &h.b, "--pr", "7", "--repo", "o/r"]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    let calls = h.calls();
    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("pr comment 7 --repo o/r --body-file ")),
        "{calls:?}"
    );
    assert!(!calls.iter().any(|c| c.contains("PATCH")), "{calls:?}");
    let body = h.saved_body();
    assert!(
        body.starts_with("<!-- strata -->\n"),
        "marker first so the comment can be found again:\n{body}"
    );
    assert!(
        body.contains("### lab-profile@1.2.0 → lab-profile@1.3.0"),
        "{body}"
    );
    assert!(body.contains("| `~` | k8s | `kubernetes` |"), "{body}");
    let create = calls
        .iter()
        .position(|c| c.starts_with("label create repave --repo o/r"))
        .expect("a label the repository lacks is created, since gh cannot add an unknown label");
    let add = calls
        .iter()
        .position(|c| c == "pr edit 7 --repo o/r --add-label repave")
        .expect("repave reaches the default threshold");
    assert!(create < add, "{calls:?}");
    assert!(
        text(&o.stderr).contains("max impact: repave"),
        "{}",
        text(&o.stderr)
    );

    // a repository that already has the label gets no create
    std::fs::write(&h.repo_labels, r#"[{"name": "bug"}, {"name": "repave"}]"#).unwrap();
    let o = h.run(&[&h.a, &h.b, "--pr", "7", "--repo", "o/r"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        !h.calls().iter().any(|c| c.starts_with("label create")),
        "{:?}",
        h.calls()
    );
    assert!(h
        .calls()
        .contains(&"pr edit 7 --repo o/r --add-label repave".to_string()));
}

#[test]
fn pr_comment_updates_the_existing_marked_comment_instead_of_adding_one() {
    let h = host();
    std::fs::write(
        &h.comments,
        r#"[{"id": 9, "body": "unrelated"}, {"id": 123, "body": "<!-- strata -->\nold report"}]"#,
    )
    .unwrap();
    let o = h.run(&[&h.a, &h.b, "--pr", "7", "--repo", "o/r"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let calls = h.calls();
    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("api -X PATCH repos/o/r/issues/comments/123 -F body=@")),
        "{calls:?}"
    );
    assert!(
        !calls.iter().any(|c| c.starts_with("pr comment")),
        "{calls:?}"
    );
    assert!(h.saved_body().starts_with("<!-- strata -->\n"));
}

#[test]
fn pr_comment_removes_the_label_below_the_threshold_and_touches_nothing_when_already_right() {
    let h = host();
    std::fs::write(
        &h.labels,
        r#"{"labels": [{"name": "repave"}, {"name": "other"}]}"#,
    )
    .unwrap();
    let o = h.run(&[&h.a, &h.a2, "--pr", "7", "--repo", "o/r"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let calls = h.calls();
    assert!(
        calls.contains(&"pr edit 7 --repo o/r --remove-label repave".to_string()),
        "identical profiles are below the threshold: {calls:?}"
    );
    assert!(h.saved_body().contains("IDENTICAL"));

    std::fs::remove_file(&h.labels).unwrap();
    let o = h.run(&[&h.a, &h.a2, "--pr", "7", "--repo", "o/r"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        !h.calls().iter().any(|c| c.starts_with("pr edit")),
        "no label change when the label is already absent: {:?}",
        h.calls()
    );

    // a lower threshold and another label name
    let o = h.run(&[
        &h.a,
        &h.b,
        "--pr",
        "7",
        "--repo",
        "o/r",
        "--label",
        "needs-window",
        "--threshold",
        "in-place",
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(h
        .calls()
        .contains(&"pr edit 7 --repo o/r --add-label needs-window".to_string()));
}

#[test]
fn pr_comment_dry_run_prints_the_comment_and_calls_no_host() {
    let h = host();
    let o = h.run(&[&h.a, &h.b, "--dry-run"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        text(&o.stdout).starts_with("<!-- strata -->\n"),
        "{}",
        text(&o.stdout)
    );
    assert!(
        text(&o.stderr).contains("would add label repave"),
        "{}",
        text(&o.stderr)
    );
    assert!(h.calls().is_empty(), "{:?}", h.calls());
}

#[test]
fn pr_comment_fails_before_posting_when_strata_cannot_compare() {
    let h = host();
    let o = h.run(&[&h.not_a_record, &h.b, "--pr", "7", "--repo", "o/r"]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stderr));
    assert!(
        text(&o.stderr).contains("not a strata snapshot"),
        "{}",
        text(&o.stderr)
    );
    assert!(
        h.calls().is_empty(),
        "nothing is posted on a failed comparison: {:?}",
        h.calls()
    );

    // a live ref is snapshotted once, before any comparison; without credentials that step fails
    let o = h.run(&["lab-profile@1.3.0", &h.b, "--pr", "7", "--repo", "o/r"]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stderr));
    assert!(
        text(&o.stderr).contains("PALETTE_API_KEY is required"),
        "{}",
        text(&o.stderr)
    );
    assert!(h.calls().is_empty(), "{:?}", h.calls());
}

#[test]
fn pr_comment_snapshots_a_live_ref_once_and_compares_files_offline() {
    let h = host();
    let log = h.bin.parent().unwrap().join("strata.log");
    let o = h.run_with(
        &["lab-profile@1.3.0", &h.a, "--pr", "7", "--repo", "o/r"],
        &[
            ("STRATA_BIN", h.bin.join("fake-strata").to_str().unwrap()),
            ("REAL_STRATA", env!("CARGO_BIN_EXE_strata")),
            ("FAKE_STRATA_SNAPSHOT", &h.b),
            ("FAKE_STRATA_LOG", log.to_str().unwrap()),
        ],
    );
    assert!(o.status.success(), "{}", text(&o.stderr));
    let calls: Vec<String> = std::fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    let snapshots: Vec<&String> = calls
        .iter()
        .filter(|c| c.starts_with("snapshot "))
        .collect();
    assert_eq!(snapshots.len(), 1, "{calls:?}");
    assert!(
        snapshots[0].starts_with("snapshot lab-profile@1.3.0 -o "),
        "{calls:?}"
    );
    let compares: Vec<&String> = calls.iter().filter(|c| c.starts_with("compare ")).collect();
    assert_eq!(compares.len(), 2, "one per format: {calls:?}");
    assert!(
        compares.iter().all(|c| !c.contains("lab-profile@1.3.0")),
        "the comparisons read the snapshot file, not Palette: {calls:?}"
    );
    assert!(h
        .saved_body()
        .contains("### lab-profile@1.3.0 → lab-profile@1.2.0"));
}

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

/// A `terraform` that reports the host and whether a key arrived, never the key itself.
const FAKE_TERRAFORM: &str = r#"#!/usr/bin/env bash
echo "host=${SPECTROCLOUD_HOST-<unset>} key=${SPECTROCLOUD_APIKEY:+set} args=$*"
"#;

fn tf_sh(envs: &[(&str, &str)], palette_env: Option<&str>) -> Output {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    executable(&bin.join("terraform"), FAKE_TERRAFORM);
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
fn tf_sh_passes_the_key_and_a_self_hosted_address_to_terraform() {
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
        .args(["cluster", "c-123"])
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
/// headers, without the key, as $FAKE_ROUTES/sent/<METHOD>_<route key>.request.
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
[ -n "$sent" ] && cp "$sent" "$FAKE_ROUTES/sent/${method}_$key"
printf 'query=%s\n%s' "$query" "$headers" | grep -v '^ApiKey:' > "$FAKE_ROUTES/sent/${method}_$key.request"
body="$FAKE_ROUTES/$key"; status=404
[ -f "$body" ] && status=200 || body=/dev/null
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

/// A `terraform` whose `show -json` holds clusters A and B.
const FAKE_TERRAFORM_STATE: &str = r#"#!/usr/bin/env bash
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
fn diagnose_sh_saves_diagnostics_for_every_cluster_in_the_terraform_state() {
    let root = tempfile::tempdir().unwrap();
    stuck_cluster_routes(&root.path().join("routes"), true);
    std::fs::write(
        root.path().join("routes/v1_spectroclusters_c-2"),
        r#"{"metadata":{"name":"fred"},"status":{"state":"Running"}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(root.path().join("bin")).unwrap();
    executable(&root.path().join("bin/terraform"), FAKE_TERRAFORM_STATE);
    let out = root.path().join("diag");
    let o = run_live("diagnose.sh", &[out.to_str().unwrap()], root.path(), &[]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert!(out.join("barney/cluster.json").exists());
    assert!(out.join("fred/cluster.json").exists());
    let barney = std::fs::read_to_string(out.join("barney/state.txt")).unwrap();
    assert!(barney.contains("barney c-1: Provisioning"), "{barney}");
}

/// A `terraform` for baseline.sh: outputs for cluster A `c-a` and four profile versions, a plan
/// that exits $FAKE_PLAN_EXIT, and a state that manages r-1, r-2, g-1, and ad-1.
const FAKE_TERRAFORM_BASELINE: &str = r#"#!/usr/bin/env bash
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
    executable(&bin.join("terraform"), FAKE_TERRAFORM_BASELINE);
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
        "strata-live-addon: Palette has ad-1 ad-2, Terraform manages ad-1",
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

// ---- verify-report.sh ------------------------------------------------------------------------

/// A `strata` for verify-report.sh: `verify --summary ...` answers from summary.json and
/// summary.exit, and `verify <path> --format markdown` from <basename>.md, .err, and .exit, all
/// under FAKE_VERIFY; every call is logged.
const FAKE_VERIFY_STRATA: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_VERIFY/strata.log"
[ "$1" = verify ] || exit 9
if [ "$2" = --summary ]; then
  cat "$FAKE_VERIFY/summary.json" 2>/dev/null
  cat "$FAKE_VERIFY/summary.err" >&2 2>/dev/null
  exit "$(cat "$FAKE_VERIFY/summary.exit")"
fi
key=$(basename "$2" .json)
cat "$FAKE_VERIFY/$key.md" 2>/dev/null
[ -f "$FAKE_VERIFY/$key.err" ] && cat "$FAKE_VERIFY/$key.err" >&2
exit "$(cat "$FAKE_VERIFY/$key.exit")"
"#;

/// A `gh` for issues: logs each call, keeps each body file it is handed as body-<n>.md in call
/// order, lists the open issues from issues.json, and numbers a new issue from 41 up.
const FAKE_ISSUES_GH: &str = r#"#!/usr/bin/env bash
set -euo pipefail
echo "$*" >> "$FAKE_VERIFY/gh.log"
n=$(wc -l < "$FAKE_VERIFY/gh.log")
prev=""
for a in "$@"; do
  [ "$prev" = "--body-file" ] && cp "$a" "$FAKE_VERIFY/body-$n.md"
  [ "$prev" = "--comment" ] && printf '%s' "$a" > "$FAKE_VERIFY/comment-$n.md"
  prev="$a"
done
case "$*" in
  *" 99 "*) echo "GraphQL: Could not resolve to an issue with the number of 99." >&2; exit 1 ;;
  "issue close"*) echo "✓ Closed issue o/r#$3" >&2 ;;
  "label create"*) echo "✓ Label \"$3\" created in o/r" >&2 ;;
  "issue list"*) cat "$FAKE_VERIFY/issues.json" 2>/dev/null || echo "[]" ;;
  "issue create"*) c=$(cat "$FAKE_VERIFY/created" 2>/dev/null || echo 40); c=$((c + 1)); echo "$c" > "$FAKE_VERIFY/created"
                   echo "https://github.com/o/r/issues/$c" ;;
  "label list"*) cat "$FAKE_VERIFY/labels.json" 2>/dev/null || echo "[]" ;;
  "repo view"*) echo '{"nameWithOwner":"o/r"}' ;;
esac
"#;

struct Drift {
    dir: tempfile::TempDir,
}

impl Drift {
    fn new() -> Drift {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        executable(&bin.join("strata"), FAKE_VERIFY_STRATA);
        executable(&bin.join("gh"), FAKE_ISSUES_GH);
        Drift { dir }
    }

    fn put(&self, name: &str, body: &str) {
        std::fs::write(self.dir.path().join(name), body).unwrap();
    }

    /// The summary strata prints, from (path, status, name, version) per record.
    fn summary(&self, exit: i32, records: &[(&str, &str, &str, &str)]) {
        let records: Vec<serde_json::Value> = records
            .iter()
            .map(|(path, status, name, version)| {
                let mut r = serde_json::json!({"path": path, "status": status, "name": name,
                    "version": version, "uid": "64f0000000000000000000a1", "recordedAt": "2026-09-16T01:56:23Z"});
                if *status == "error" {
                    r["error"] = serde_json::json!({"kind": "api", "message": "GET /v1/clusterprofiles/64f0 failed with HTTP 404"});
                }
                r
            })
            .collect();
        self.put(
            "summary.json",
            &serde_json::json!({"schema": "strata-verify-summary/1", "records": records})
                .to_string(),
        );
        self.put("summary.exit", &exit.to_string());
    }

    /// What `strata verify <path> --format markdown` prints and exits with for one record.
    fn record(&self, key: &str, exit: i32, report: &str, err: &str) {
        self.put(&format!("{key}.md"), report);
        self.put(&format!("{key}.exit"), &exit.to_string());
        let e = self.dir.path().join(format!("{key}.err"));
        if err.is_empty() {
            let _ = std::fs::remove_file(e);
        } else {
            std::fs::write(e, err).unwrap();
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        for f in ["gh.log", "strata.log"] {
            let _ = std::fs::remove_file(self.dir.path().join(f));
        }
        Command::new("bash")
            .arg(script("verify-report.sh"))
            .args(args)
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.dir.path().join("bin").display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .env("FAKE_VERIFY", self.dir.path())
            .env_remove("STRATA_BIN")
            .env_remove("GITHUB_REPOSITORY")
            .output()
            .unwrap()
    }

    fn log(&self, name: &str) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join(name))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The body or comment handed to gh on the call with this 1-based number.
    fn handed(&self, kind: &str, call: usize) -> String {
        std::fs::read_to_string(self.dir.path().join(format!("{kind}-{call}.md")))
            .unwrap_or_else(|_| panic!("gh call {call} was handed no {kind}"))
    }
}

#[test]
fn verify_report_opens_one_issue_per_record_that_did_not_verify_and_closes_it_once_it_does() {
    let d = Drift::new();
    d.summary(
        2,
        &[
            ("releases/a.json", "unchanged", "lab-profile", "1.2.0"),
            ("releases/b.json", "drifted", "lab-profile", "1.3.0"),
            ("releases/c.json", "error", "lab-profile", "1.0.0"),
        ],
    );
    d.record("b", 1, "## lab-profile@1.3.0\n\n**Result:** CHANGED\n", "");
    d.record(
        "c",
        2,
        "",
        "strata: GET /v1/clusterprofiles/64f0 failed with HTTP 404\n",
    );

    // first run: no issues yet, and no label
    let o = d.run(&["releases/*.json", "--repo", "o/r"]);
    assert_eq!(
        o.status.code(),
        Some(2),
        "a record that could not be checked: {}",
        text(&o.stderr)
    );
    assert_eq!(
        d.log("strata.log"),
        vec![
            "verify --summary --quiet --format json releases/*.json",
            "verify releases/b.json --quiet --format markdown",
            "verify releases/c.json --quiet --format markdown",
        ],
        "one summary, then each record that did not verify on its own for its report"
    );
    let calls = d.log("gh.log");
    assert_eq!(calls.len(), 5, "{calls:#?}");
    assert!(
        calls[0].starts_with("issue list --repo o/r --label strata-drift --state open"),
        "{}",
        calls[0]
    );
    assert!(
        calls[1].starts_with("label list --repo o/r"),
        "{}",
        calls[1]
    );
    assert!(
        calls[2].starts_with("label create strata-drift --repo o/r"),
        "{}",
        calls[2]
    );
    assert!(
        calls[3].starts_with("issue create --repo o/r --title strata verify: lab-profile@1.3.0 drifted (releases/b.json)"),
        "{}",
        calls[3]
    );
    assert!(calls[3].contains("--label strata-drift"), "{}", calls[3]);
    let b = d.handed("body", 4);
    assert!(
        b.starts_with("<!-- strata-verify: releases/b.json -->\n"),
        "{b}"
    );
    assert!(
        b.contains("`strata verify releases/b.json` reports drift"),
        "{b}"
    );
    assert!(
        b.contains("**Result:** CHANGED"),
        "the report is in the issue: {b}"
    );
    assert!(
        calls[4].starts_with(
            "issue create --repo o/r --title strata verify: releases/c.json could not be checked"
        ),
        "{}",
        calls[4]
    );
    let c = d.handed("body", 5);
    assert!(
        c.starts_with("<!-- strata-verify: releases/c.json -->\n"),
        "{c}"
    );
    assert!(
        c.contains("api: GET /v1/clusterprofiles/64f0 failed with HTTP 404"),
        "{c}"
    );
    let err = text(&o.stderr);
    assert!(
        err.contains("3 records: 1 unchanged, 1 drifted, 1 could not be checked; opened #41 for releases/b.json; opened #42 for releases/c.json"),
        "{err}"
    );
    assert!(
        !err.contains('✓'),
        "gh's success messages stay out of the report: {err}"
    );

    // second run: b still drifts, c verifies again, a keeps verifying and has an old issue; an
    // issue for a record not given stays open
    d.summary(
        1,
        &[
            ("releases/a.json", "unchanged", "lab-profile", "1.2.0"),
            ("releases/b.json", "drifted", "lab-profile", "1.3.0"),
            ("releases/c.json", "error", "lab-profile", "1.0.0"),
        ],
    );
    d.record(
        "b",
        1,
        "## lab-profile@1.3.0\n\n**Result:** CHANGED, again\n",
        "",
    );
    d.record("c", 0, "", "");
    d.put(
        "issues.json",
        r#"[{"number":7,"title":"old","body":"<!-- strata-verify: releases/a.json -->\nold"},
            {"number":41,"title":"t","body":"<!-- strata-verify: releases/b.json -->\r\nedited on the web"},
            {"number":42,"title":"t","body":"<!-- strata-verify: releases/c.json -->\nold"},
            {"number":50,"title":"t","body":"<!-- strata-verify: releases/other.json -->\nold"}]"#,
    );
    let o = d.run(&["releases/*.json", "--repo", "o/r"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    let calls = d.log("gh.log");
    assert_eq!(calls.len(), 4, "{calls:#?}");
    assert!(
        calls[1].starts_with("issue close 7 --repo o/r --comment"),
        "{}",
        calls[1]
    );
    assert!(
        d.handed("comment", 2).contains("`releases/a.json` verifies again: lab-profile@1.2.0 is unchanged since 2026-09-16T01:56:23Z"),
        "{}",
        d.handed("comment", 2)
    );
    assert!(
        calls[2].starts_with("issue edit 41 --repo o/r --title strata verify: lab-profile@1.3.0 drifted (releases/b.json)"),
        "{}",
        calls[2]
    );
    assert!(d.handed("body", 3).contains("CHANGED, again"));
    assert!(
        calls[3].starts_with("issue close 42 --repo o/r --comment"),
        "{}",
        calls[3]
    );
    assert!(
        calls
            .iter()
            .all(|c| !c.contains(" 50 ") && !c.starts_with("issue create")),
        "exactly one issue per record, and none touched for a record not given: {calls:#?}"
    );
    assert!(
        text(&o.stderr).contains("3 records: 2 unchanged, 1 drifted, 0 could not be checked; closed #7 for releases/a.json; updated #41 for releases/b.json; closed #42 for releases/c.json"),
        "{}",
        text(&o.stderr)
    );
    assert!(!text(&o.stderr).contains('✓'), "{}", text(&o.stderr));

    // a host call that fails stops the script and shows what the host said
    d.put(
        "issues.json",
        r#"[{"number":99,"title":"t","body":"<!-- strata-verify: releases/a.json -->\nold"}]"#,
    );
    let o = d.run(&["releases/*.json", "--repo", "o/r"]);
    assert_ne!(o.status.code(), Some(0));
    assert!(
        text(&o.stderr).contains("Could not resolve to an issue with the number of 99"),
        "{}",
        text(&o.stderr)
    );

    // a clean run with no open issues only lists them
    d.summary(
        0,
        &[("releases/a.json", "unchanged", "lab-profile", "1.2.0")],
    );
    d.put("issues.json", "[]");
    let o = d.run(&["releases/a.json", "--repo", "o/r"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert_eq!(d.log("gh.log").len(), 1);
}

#[test]
fn verify_report_touches_no_issue_when_verify_checks_nothing() {
    let d = Drift::new();
    d.put(
        "summary.err",
        r#"{"error":{"kind":"pattern","message":"'releases/*.json' matched no files"}}"#,
    );
    d.put("summary.exit", "2");
    let o = d.run(&["releases/*.json", "--repo", "o/r"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(d.log("gh.log").is_empty(), "{:?}", d.log("gh.log"));
    let err = text(&o.stderr);
    assert!(err.contains("matched no files"), "{err}");
    assert!(
        err.contains("verify-report: strata verify checked nothing (exit 2)"),
        "{err}"
    );
}

#[test]
fn verify_report_dry_run_prints_each_report_and_calls_no_host() {
    let d = Drift::new();
    d.summary(
        1,
        &[
            ("releases/a.json", "unchanged", "lab-profile", "1.2.0"),
            ("releases/b.json", "drifted", "lab-profile", "1.3.0"),
        ],
    );
    d.record("b", 1, "**Result:** CHANGED\n", "");
    let o = d.run(&["releases/*.json", "--dry-run"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    assert!(d.log("gh.log").is_empty(), "{:?}", d.log("gh.log"));
    let out = text(&o.stdout);
    assert!(
        out.contains("<!-- strata-verify: releases/b.json -->"),
        "{out}"
    );
    assert!(out.contains("**Result:** CHANGED"), "{out}");
    let err = text(&o.stderr);
    assert!(
        err.contains("would close the open issue for releases/a.json, if any"),
        "{err}"
    );
    assert!(
        err.contains("would open or update the issue for releases/b.json"),
        "{err}"
    );
}

#[test]
fn live_scripts_refuse_a_strata_binary_older_than_the_sources() {
    let root = tempfile::tempdir().unwrap();
    let live = root.path().join("examples/live");
    let release = root.path().join("target/release");
    for d in [&live, &release, &root.path().join("src")] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::copy(live_script("lib.sh"), live.join("lib.sh")).unwrap();
    executable(
        &live.join("probe.sh"),
        "script=probe\n. \"$(dirname \"$0\")/lib.sh\"\necho ran \"$strata\"\n",
    );
    let strata = release.join("strata");
    executable(&strata, "#!/bin/sh\n");
    let source = root.path().join("src/lib.rs");
    std::fs::write(&source, "// changed after the build\n").unwrap();
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(600);
    std::fs::File::options()
        .write(true)
        .open(&strata)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let probe = || {
        Command::new("bash")
            .arg(live.join("probe.sh"))
            .env_remove("STRATA_BIN")
            .output()
            .unwrap()
    };

    let o = probe();
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stdout));
    assert!(
        text(&o.stderr).contains(&format!(
            "probe: {} is older than {}; run just build",
            strata.display(),
            source.display()
        )),
        "{}",
        text(&o.stderr)
    );

    // rebuilt: the binary is newer than every source
    std::fs::File::options()
        .write(true)
        .open(&strata)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5))
        .unwrap();
    let o = probe();
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));

    // a binary chosen with STRATA_BIN is the caller's to keep current
    std::fs::write(&source, "// changed again\n").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&source)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(60))
        .unwrap();
    let o = Command::new("bash")
        .arg(live.join("probe.sh"))
        .env("STRATA_BIN", &strata)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
}

/// A `terraform` for lib.sh's applies: logs each call and has no outputs.
const FAKE_TERRAFORM_APPLY: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_TF_LOG"
case "$*" in
  *"output -json"*) echo '{}' ;;
esac
"#;

#[test]
fn live_applies_take_the_script_flag_and_the_baseline_restore_turns_it_off() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    executable(&bin.join("terraform"), FAKE_TERRAFORM_APPLY);
    let log = root.path().join("tf.log");
    for flag in ["true", "false"] {
        let _ = std::fs::remove_file(&log);
        let probe = format!(
            "cd \"$WORK\" && script=probe flag={flag} && . \"{}\" && hold_versions && apply -var addon_version=1.0.1 && apply_baseline && echo done",
            live_script("lib.sh").display()
        );
        let o = Command::new("bash")
            .args(["-c", &probe])
            .env(
                "PATH",
                format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
            )
            .env("WORK", root.path())
            .env("HOME", root.path())
            .env("PALETTE_API_KEY", "k")
            .env("STRATA_BIN", "/bin/true")
            .env("FAKE_TF_LOG", &log)
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
        let calls = std::fs::read_to_string(&log).unwrap();
        // each call starts with tf.sh's -chdir
        let applies: Vec<&str> = calls
            .lines()
            .filter_map(|c| c.split_once(' ').map(|(_, rest)| rest))
            .filter(|c| c.starts_with("apply "))
            .collect();
        let with = "apply -input=false -auto-approve -no-color -var immutable_profiles=";
        assert_eq!(
            applies,
            [
                format!("{with}{flag}"),
                format!("{with}{flag} -var addon_version=1.0.1"),
                format!("{with}false"),
            ],
            "hold_versions and apply take the flag with no -target, and apply_baseline turns it off: {calls}"
        );
    }
}
