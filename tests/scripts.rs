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
