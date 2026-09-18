//! The scripts under scripts/ci/, which ship in a release and run in a pipeline.
//! No network: every host and every command outside this repository is a recording stand-in.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::*;

/// The scripts a release carries. The release workflow installs the published tag and runs the
/// help of each one; this is the same check, before a tag exists.
const SHIPPED: [&str; 4] = [
    "pr-comment.sh",
    "merge-gate.sh",
    "verify-report.sh",
    "install-strata.sh",
];

#[test]
fn every_script_a_release_carries_prints_its_help_and_exits_0() {
    for name in SHIPPED {
        let o = Command::new("bash")
            .arg(script(&format!("ci/{name}")))
            .arg("--help")
            .output()
            .unwrap();
        assert_eq!(
            o.status.code(),
            Some(0),
            "{name} --help: {}",
            text(&o.stderr)
        );
        let help = text(&o.stdout);
        assert!(
            help.contains(name),
            "{name} --help names the script:\n{help}"
        );
        assert!(
            help.lines().count() >= 5,
            "{name} --help says what it does:\n{help}"
        );
        assert!(
            !help.contains("scripts/pr-comment.sh") && !help.contains("scripts/verify-report.sh"),
            "{name} --help names the path the script lives at now:\n{help}"
        );
    }
}

/// The files a team copies, and the reusable workflows they call. They name scripts, options and a
/// release version, and nothing else reads them: a renamed option or a moved script breaks a
/// drop-in silently.
const DROP_INS: [&str; 5] = [
    "examples/ci/github/strata.yml",
    "examples/ci/gitlab/strata.yml",
    ".github/workflows/comment.yml",
    ".github/workflows/verify.yml",
    ".github/workflows/merge-gate.yml",
];

fn repo_file(path: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
        .unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The `--help` of one script that ships.
fn help_of(name: &str) -> String {
    let o = Command::new("bash")
        .arg(script(&format!("ci/{name}")))
        .arg("--help")
        .output()
        .unwrap();
    assert!(o.status.success(), "{name} --help failed");
    text(&o.stdout)
}

#[test]
fn the_ci_drop_ins_name_only_scripts_that_ship() {
    for file in DROP_INS {
        for token in repo_file(file).split(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
            let token = token.trim_end_matches(&[',', ';', ')'][..]);
            if !token.ends_with(".sh") {
                continue;
            }
            let name = token.rsplit('/').next().unwrap();
            assert!(
                SHIPPED.contains(&name),
                "{file} names {token}, which no release carries"
            );
            // a path under scripts/ is this repository's copy, and must be where it says
            if let Some(rest) = token.split_once("scripts/").map(|(_, r)| r) {
                let at = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("scripts")
                    .join(rest);
                assert!(at.exists(), "{file} names {token}, which is not at {at:?}");
            }
        }
    }
}

#[test]
fn the_ci_drop_ins_pass_only_options_the_scripts_have() {
    let helps: Vec<(&str, String)> = SHIPPED.iter().map(|n| (*n, help_of(n))).collect();
    // as a word: --thresh is not --threshold, although one contains the other
    let has_flag = |help: &str, flag: &str| {
        help.match_indices(flag).any(|(i, _)| {
            help[i + flag.len()..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '-')
        })
    };
    let known = |flag: &str| helps.iter().any(|(_, h)| has_flag(h, flag));
    for file in DROP_INS {
        let body = repo_file(file);
        let mut lines = body.lines().peekable();
        while let Some(line) = lines.next() {
            let Some(name) = SHIPPED.iter().find(|n| line.contains(**n)) else {
                continue;
            };
            // the call, and the lines that continue it: a YAML scalar wraps without a backslash
            let mut block = line.to_string();
            let indent = line.len() - line.trim_start().len();
            while let Some(next) = lines.peek() {
                let deeper = next.len() - next.trim_start().len() > indent;
                // "- " starts a list item; "--label" continues the command above it
                let new_key = next.trim_start().starts_with("- ") || next.trim_end().ends_with(':');
                if !deeper || new_key || next.trim().is_empty() {
                    break;
                }
                block.push(' ');
                block.push_str(lines.next().unwrap());
            }
            for word in block.split(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
                if !word.starts_with("--") || word.len() < 4 {
                    continue;
                }
                let flag = word.trim_end_matches(&[',', ';', ')'][..]);
                assert!(
                    known(flag),
                    "{file} passes {flag} to {name}, and no script that ships has it"
                );
            }
        }
        // a flag the file carries another way: quoted in an environment value, such as
        // '--no-verify', or assigned to a shell variable, such as rollback=--allow-rollback.
        // Line by line, and never a comment, because prose holds apostrophes.
        for line in body.lines() {
            if line.trim_start().starts_with('#') {
                continue;
            }
            let mut carried: Vec<&str> = line
                .split('\'')
                .skip(1)
                .step_by(2)
                .filter(|q| q.starts_with("--") && !q.contains(' '))
                .collect();
            carried.extend(
                line.split(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                    .filter_map(|w| w.split_once("=--").map(|(_, f)| f))
                    .filter(|f| !f.is_empty())
                    .map(|f| {
                        let end = f
                            .find(|c: char| !c.is_ascii_alphanumeric() && c != '-')
                            .unwrap_or(f.len());
                        &f[..end]
                    })
                    .map(|f| Box::leak(format!("--{f}").into_boxed_str()) as &str),
            );
            for flag in carried {
                let flag = flag.trim_end_matches(&[';', ',', ')'][..]);
                if flag.len() < 4 {
                    continue;
                }
                assert!(
                    known(flag),
                    "{file} carries {flag}, and no script that ships has it"
                );
            }
        }
    }
}

#[test]
fn the_ci_drop_ins_name_the_version_this_repository_is_at() {
    let cargo = repo_file("Cargo.toml");
    let line = cargo
        .lines()
        .find(|l| l.starts_with("version = "))
        .expect("a version in Cargo.toml");
    let version = format!("v{}", line.split('"').nth(1).unwrap());
    for file in DROP_INS {
        for token in repo_file(file).split(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
            let token = token.trim_end_matches(&[',', ';', ')', '`'][..]);
            let named = token.rsplit('@').next().unwrap_or(token);
            let looks_like_a_tag = named.starts_with('v')
                && named.len() > 3
                && named[1..].split('.').count() == 3
                && named[1..]
                    .split('.')
                    .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
            if looks_like_a_tag {
                assert_eq!(
                    named, version,
                    "{file} names {named}, and this repository is at {version}: a release bump \
                     must move every drop-in with it"
                );
            }
        }
    }
}

/// The release workflow installs what it published and runs each script the installer installed.
/// The two lists are written in two files, so they can disagree: the check named
/// `install-strata.sh`, which the installer never installs, and the first release to run the check
/// failed on it. This holds them together.
#[test]
fn the_release_check_runs_the_scripts_the_installer_installs() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let installer = std::fs::read_to_string(root.join("scripts/ci/install-strata.sh")).unwrap();
    let release = std::fs::read_to_string(root.join(".github/workflows/release.yml")).unwrap();

    // `for name in pr-comment.sh merge-gate.sh verify-report.sh; do` in the installer
    let installs: Vec<&str> = installer
        .lines()
        .find(|l| l.trim_start().starts_with("for name in ") && l.contains(".sh"))
        .expect("the installer names the scripts it installs")
        .trim()
        .trim_start_matches("for name in ")
        .trim_end_matches("; do")
        .split_whitespace()
        .collect();

    // `for s in pr-comment.sh merge-gate.sh verify-report.sh; do` in the release check
    let checks: Vec<&str> = release
        .lines()
        .find(|l| l.trim_start().starts_with("for s in ") && l.contains(".sh"))
        .expect("the release workflow names the scripts it runs")
        .trim()
        .trim_start_matches("for s in ")
        .trim_end_matches("; do")
        .split_whitespace()
        .collect();

    assert!(
        !installs.is_empty() && !checks.is_empty(),
        "the parsing broke"
    );
    assert_eq!(
        checks, installs,
        "the release workflow runs scripts the installer does not install, or misses one it does"
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

/// A `curl` for the GitLab REST API: logs each call as "METHOD /path field field", keeps each
/// body file it is handed as body-<n>.md in call order, answers a GET from a gl-*.json file under
/// FAKE_GL when the test wrote one, numbers a new issue from 41 up, and prints the status code
/// FAKE_GL_STATUS (200 by default) the way `-w '%{http_code}'` does.
const FAKE_GL_CURL: &str = r#"#!/usr/bin/env bash
set -euo pipefail
out=/dev/null; method=GET; url=""; data=()
while [ $# -gt 0 ]; do
  case "$1" in
    -o) out=$2; shift 2 ;;
    -X) method=$2; shift 2 ;;
    -H|-w) shift 2 ;;
    --data-urlencode) data+=("$2"); shift 2 ;;
    -*) shift ;;
    *) url=$1; shift ;;
  esac
done
path=${url#*/api/v4}
echo "$method $path ${data[*]-}" >> "$FAKE_GL/curl.log"
n=$(wc -l < "$FAKE_GL/curl.log")
for d in "${data[@]-}"; do
  f=${d#*@}
  case "$d" in *@*) [ -f "$f" ] && cp "$f" "$FAKE_GL/body-$n.md" ;; esac
done
answer='{}'
from() { [ -f "$FAKE_GL/$1" ] && answer=$(cat "$FAKE_GL/$1") || answer=$2; }
case "$method $path" in
  "GET "*"/notes"*) from gl-notes.json "[]" ;;
  "GET "*"/merge_requests/"*) from gl-mr.json '{"labels":[]}' ;;
  "GET "*"/labels"*) from gl-labels.json "[]" ;;
  "GET "*"/issues"*) from gl-issues.json "[]" ;;
  "POST /projects/"*"/issues") c=$(cat "$FAKE_GL/created" 2>/dev/null || echo 40); c=$((c + 1))
                                echo "$c" > "$FAKE_GL/created"; answer="{\"iid\":$c}" ;;
esac
printf '%s' "$answer" > "$out"
printf '%s' "${FAKE_GL_STATUS:-200}"
"#;

struct Host {
    _dir: tempfile::TempDir,
    bin: PathBuf,
    log: PathBuf,
    body: PathBuf,
    comments: PathBuf,
    labels: PathBuf,
    repo_labels: PathBuf,
    gl: PathBuf,
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
    for (name, body) in [
        ("gh", FAKE_GH),
        ("fake-strata", FAKE_STRATA),
        ("curl", FAKE_GL_CURL),
    ] {
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
        gl: {
            let g = dir.path().join("gitlab");
            std::fs::create_dir(&g).unwrap();
            g
        },
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
        c.arg(script("ci/pr-comment.sh"))
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

    /// Run the script the way a GitLab job does: the CI variables, and no --host.
    fn run_gitlab(&self, args: &[&str], envs: &[(&str, &str)]) -> Output {
        for f in ["curl.log", "body-1.md", "body-2.md", "body-3.md"] {
            let _ = std::fs::remove_file(self.gl.join(f));
        }
        let gl = self.gl.display().to_string();
        let mut all: Vec<(&str, &str)> = vec![
            ("FAKE_GL", &gl),
            ("GITLAB_CI", "true"),
            ("CI_PROJECT_ID", "42"),
            ("CI_MERGE_REQUEST_IID", "7"),
            ("CI_API_V4_URL", "https://gitlab.example/api/v4"),
            ("GITLAB_TOKEN", "a-token"),
        ];
        all.extend_from_slice(envs);
        self.run_with(args, &all)
    }

    fn put_gl(&self, name: &str, body: &str) {
        std::fs::write(self.gl.join(name), body).unwrap();
    }

    fn gl_calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.gl.join("curl.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The body file handed to curl on the call with this 1-based number.
    fn gl_handed(&self, call: usize) -> String {
        std::fs::read_to_string(self.gl.join(format!("body-{call}.md")))
            .unwrap_or_else(|_| panic!("curl call {call} was handed no body"))
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

#[test]
fn pr_comment_posts_one_marked_note_and_labels_the_merge_request_on_gitlab() {
    let h = host();
    // no --host and no --pr: the CI variables of a GitLab job say both
    let o = h.run_gitlab(&[&h.a, &h.b], &[]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    let calls = h.gl_calls();
    assert!(
        calls[0].starts_with("GET /projects/42/merge_requests/7/notes"),
        "{calls:?}"
    );
    assert!(
        calls[1].starts_with("POST /projects/42/merge_requests/7/notes body@"),
        "{calls:?}"
    );
    let body = h.gl_handed(2);
    assert!(
        body.starts_with("<!-- strata -->\n"),
        "marker first so the note can be found again:\n{body}"
    );
    assert!(
        body.contains("### lab-profile@1.2.0 → lab-profile@1.3.0"),
        "{body}"
    );
    let create = calls
        .iter()
        .position(|c| c.starts_with("POST /projects/42/labels name=repave"))
        .expect("a label the project lacks is created first");
    let add = calls
        .iter()
        .position(|c| c == "PUT /projects/42/merge_requests/7 add_labels=repave")
        .expect("repave reaches the default threshold");
    assert!(create < add, "{calls:?}");
    assert!(
        text(&o.stderr).contains("max impact: repave"),
        "{}",
        text(&o.stderr)
    );
}

#[test]
fn pr_comment_updates_the_marked_note_on_gitlab_and_leaves_a_label_it_has() {
    let h = host();
    h.put_gl(
        "gl-notes.json",
        r#"[{"id": 11, "body": "a review note"}, {"id": 55, "body": "<!-- strata -->\nan older report"}]"#,
    );
    h.put_gl("gl-mr.json", r#"{"labels": ["repave", "review"]}"#);
    let o = h.run_gitlab(&[&h.a, &h.b], &[]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    let calls = h.gl_calls();
    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("PUT /projects/42/merge_requests/7/notes/55 body@")),
        "the marked note is updated, not doubled: {calls:?}"
    );
    assert!(
        !calls
            .iter()
            .any(|c| c.starts_with("POST /projects/42/merge_requests/7/notes")),
        "{calls:?}"
    );
    assert!(
        !calls.iter().any(|c| c.contains("add_labels")),
        "the merge request already has the label: {calls:?}"
    );
    assert!(
        text(&o.stderr).contains("comment 55 updated"),
        "{}",
        text(&o.stderr)
    );
}

#[test]
fn pr_comment_stops_on_what_gitlab_answers_and_asks_for_a_token_that_can_write() {
    let h = host();
    let o = h.run_gitlab(&[&h.a, &h.b], &[("FAKE_GL_STATUS", "403")]);
    assert!(!o.status.success());
    assert!(
        text(&o.stderr).contains("GitLab answered HTTP 403"),
        "{}",
        text(&o.stderr)
    );

    let o = h.run_gitlab(&[&h.a, &h.b], &[("GITLAB_TOKEN", "")]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("set GITLAB_TOKEN"),
        "{}",
        text(&o.stderr)
    );
    assert!(h.gl_calls().is_empty(), "no call before the token is there");
}

// ---- install-strata.sh -------------------------------------------------------------------------

/// A `curl` for the release API of a code host: the release document, and each asset by its id.
const FAKE_RELEASE_CURL: &str = r#"#!/usr/bin/env bash
set -euo pipefail
out=""; url=""
while [ $# -gt 0 ]; do
  case "$1" in
    -o) out=$2; shift 2 ;;
    -H) shift 2 ;;
    -*) shift ;;
    *) url=$1; shift ;;
  esac
done
echo "$url" >> "$FAKE_REL/curl.log"
case "$url" in
  */releases/tags/*|*/releases/latest) cp "$FAKE_REL/release.json" "$out" ;;
  */releases/assets/*) cp "$FAKE_REL/assets/${url##*/}" "$out" ;;
  *) echo "no such URL: $url" >&2; exit 22 ;;
esac
"#;

/// A release with the four files a job downloads, and the checksums over them.
fn release() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let assets = d.join("assets");
    std::fs::create_dir(&assets).unwrap();
    let files = [
        (
            "1",
            "strata-x86_64-unknown-linux-musl",
            "#!/bin/sh\necho strata 9.9.9\n",
        ),
        ("2", "pr-comment.sh", "#!/bin/sh\necho comment\n"),
        ("3", "verify-report.sh", "#!/bin/sh\necho report\n"),
        ("4", "install-strata.sh", "#!/bin/sh\necho installer\n"),
        ("6", "merge-gate.sh", "#!/bin/sh\necho gate\n"),
    ];
    let mut sums = String::new();
    let mut listed = Vec::new();
    for (id, name, body) in files {
        std::fs::write(assets.join(id), body).unwrap();
        let sum = Command::new("sha256sum")
            .arg(assets.join(id))
            .output()
            .unwrap();
        let sum = text(&sum.stdout);
        sums.push_str(&format!(
            "{}  {}\n",
            sum.split_whitespace().next().unwrap(),
            name
        ));
        listed.push(serde_json::json!({"name": name, "id": id.parse::<u32>().unwrap()}));
    }
    std::fs::write(assets.join("5"), &sums).unwrap();
    listed.push(serde_json::json!({"name": "SHA256SUMS", "id": 5}));
    std::fs::write(
        d.join("release.json"),
        serde_json::json!({"tag_name": "v9.9.9", "assets": listed}).to_string(),
    )
    .unwrap();
    let bin = d.join("bin");
    std::fs::create_dir(&bin).unwrap();
    executable(&bin.join("curl"), FAKE_RELEASE_CURL);
    dir
}

fn install(d: &Path, args: &[&str]) -> Output {
    Command::new("bash")
        .arg(script("ci/install-strata.sh"))
        .args(args)
        .current_dir(d)
        .env(
            "PATH",
            format!(
                "{}:{}",
                d.join("bin").display(),
                std::env::var("PATH").unwrap()
            ),
        )
        .env("FAKE_REL", d)
        .env("STRATA_API", "https://host.example/api")
        .env("STRATA_REPO", "o/r")
        .env_remove("STRATA_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .output()
        .unwrap()
}

#[test]
fn install_strata_takes_the_binary_and_the_scripts_of_one_release_and_checks_them() {
    let r = release();
    let d = r.path();
    let o = install(
        d,
        &[
            "--version",
            "v9.9.9",
            "--dir",
            "here",
            "--target",
            "x86_64-unknown-linux-musl",
        ],
    );
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    assert_eq!(
        text(&o.stdout).trim(),
        "here/strata",
        "the path of the binary, for STRATA_BIN"
    );
    for name in [
        "strata",
        "pr-comment.sh",
        "merge-gate.sh",
        "verify-report.sh",
        "SHA256SUMS",
    ] {
        assert!(d.join("here").join(name).exists(), "{name} is installed");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for name in [
            "strata",
            "pr-comment.sh",
            "merge-gate.sh",
            "verify-report.sh",
        ] {
            let mode = std::fs::metadata(d.join("here").join(name))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111, "{name} is executable");
        }
    }
    assert!(
        text(&o.stderr).contains("v9.9.9 installed in here, checksums verified"),
        "{}",
        text(&o.stderr)
    );
}

#[test]
fn install_strata_installs_nothing_when_a_file_does_not_match_the_checksums() {
    let r = release();
    let d = r.path();
    // the release serves another binary than the one SHA256SUMS signed
    std::fs::write(d.join("assets/1"), "#!/bin/sh\necho another build\n").unwrap();
    let o = install(
        d,
        &[
            "--version",
            "v9.9.9",
            "--dir",
            "here",
            "--target",
            "x86_64-unknown-linux-musl",
        ],
    );
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("do not match SHA256SUMS"),
        "{}",
        text(&o.stderr)
    );
    assert!(
        !d.join("here/strata").exists(),
        "the check comes before anything is installed"
    );
}

#[test]
fn install_strata_says_which_file_a_release_does_not_have() {
    let r = release();
    let d = r.path();
    let doc = std::fs::read_to_string(d.join("release.json")).unwrap();
    let doc = doc.replace("\"pr-comment.sh\"", "\"something-else.sh\"");
    std::fs::write(d.join("release.json"), doc).unwrap();
    let o = install(
        d,
        &[
            "--version",
            "v9.9.9",
            "--dir",
            "here",
            "--target",
            "x86_64-unknown-linux-musl",
        ],
    );
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("the release v9.9.9 has no pr-comment.sh"),
        "{}",
        text(&o.stderr)
    );
}

// ---- merge-gate.sh ----------------------------------------------------------------------------

/// A repository with a main branch that holds `releases/dev.json`, and a branch `work` that
/// changes it. `main_side` is what another merge request put on main in the meantime.
struct Gate {
    dir: tempfile::TempDir,
}

impl Gate {
    fn new(base: &str, branch: &str, main_side: &str) -> Gate {
        Gate::build(base, branch, main_side, false)
    }

    /// The same, and the branch then merged main and kept its own record, which is what a hand
    /// merge of a conflict does. The branch carries main, and still pins its own version.
    fn merged_keeping_its_own(base: &str, branch: &str, main_side: &str) -> Gate {
        Gate::build(base, branch, main_side, true)
    }

    fn build(base: &str, branch: &str, main_side: &str, merge_ours: bool) -> Gate {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        git(d, &["init", "-q", "-b", "main"]);
        std::fs::create_dir(d.join("releases")).unwrap();
        std::fs::write(d.join("releases/dev.json"), base).unwrap();
        git(d, &["add", "."]);
        git(d, &["commit", "-q", "-m", "the record"]);
        git(d, &["checkout", "-q", "-b", "work"]);
        std::fs::write(d.join("releases/dev.json"), branch).unwrap();
        git(d, &["commit", "-qam", "the branch moves dev"]);
        git(d, &["checkout", "-q", "main"]);
        if main_side != base {
            std::fs::write(d.join("releases/dev.json"), main_side).unwrap();
            git(d, &["commit", "-qam", "another merge request moved dev"]);
        }
        // a remote named origin is this repository itself, which is what a fetch in CI reads
        git(d, &["remote", "add", "origin", d.to_str().unwrap()]);
        git(d, &["checkout", "-q", "work"]);
        if merge_ours {
            git(d, &["fetch", "-q", "origin", "main"]);
            git(
                d,
                &["merge", "-q", "-X", "ours", "--no-edit", "origin/main"],
            );
        }
        Gate { dir }
    }

    /// Write the record and commit it on the branch that is checked out.
    fn pin(&self, content: &str, message: &str) {
        std::fs::write(self.dir.path().join("releases/dev.json"), content).unwrap();
        git(self.dir.path(), &["commit", "-qam", message]);
    }

    /// What the gate reads to find a rollback a commit declares: the messages of the commits the
    /// branch carries and the target branch does not. A failure of a rollback test says nothing
    /// without this, because the gate only reports that it found no declaration.
    fn branch_messages(&self) -> String {
        let d = self.dir.path();
        let refs = git(d, &["log", "--oneline", "--all", "--decorate"]);
        let range = git(d, &["log", "--format=%B", "HEAD", "--not", "origin/main"]);
        format!(
            "\n-- git log --format=%B HEAD --not origin/main --\n{}\n-- every commit --\n{}",
            text(&range.stdout),
            text(&refs.stdout)
        )
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new("bash")
            .arg(script("ci/merge-gate.sh"))
            .args(args)
            .current_dir(self.dir.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .env("STRATA_BIN", env!("CARGO_BIN_EXE_strata"))
            .env("STRATA_QUIET", "1")
            .env_remove("PALETTE_API_KEY")
            .env_remove("SPECTROCLOUD_APIKEY")
            .output()
            .unwrap()
    }
}

/// The record of fixture a, the record the branch pins, and the later record another merge
/// request put on main. All three carry one header, the way records written by one job do.
fn gate_records() -> (String, String, String) {
    let dir = tempfile::tempdir().unwrap();
    let raw = dir.path().join("raw");
    common::make_fixtures(&raw);
    common::rewrite(&raw.join("a"), &raw.join("branch"), &[("1.29.4", "1.30.2")]);
    common::rewrite(
        &raw.join("a"),
        &raw.join("main-side"),
        &[("1.29.4", "1.31.0")],
    );
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
        // one header for all three: the ref and the time are what the job wrote, not the change
        let doc = std::fs::read_to_string(&out).unwrap();
        let doc = field(&doc, "\"ref\": \"", "releases/dev.json");
        field(&doc, "\"fetchedAt\": \"", "2026-09-17T00:00:00Z")
    };
    let (base, branch, main_side) = (snap("a"), snap("branch"), snap("main-side"));
    assert_ne!(base, branch);
    assert_ne!(branch, main_side);
    (base, branch, main_side)
}

/// Replace the value that follows the first `key` in a JSON document.
fn field(doc: &str, key: &str, value: &str) -> String {
    let i = doc
        .find(key)
        .unwrap_or_else(|| panic!("no {key} in the document"))
        + key.len();
    let j = doc[i..].find('"').unwrap() + i;
    format!("{}{}{}", &doc[..i], value, &doc[j..])
}

#[test]
fn merge_gate_stops_a_branch_that_would_replace_a_newer_pin() {
    let (base, branch, main_side) = gate_records();
    // the branch settled the conflict by keeping its own record, so git merges and says nothing
    let g = Gate::merged_keeping_its_own(&base, &branch, &main_side);
    let o = g.run(&["releases/*.json", "--base", "main", "--no-verify"]);
    assert_eq!(
        o.status.code(),
        Some(1),
        "the gate is red:\n{}{}",
        text(&o.stdout),
        text(&o.stderr)
    );
    let out = text(&o.stdout);
    assert!(
        out.contains(
            "BEHIND: releases/dev.json would replace what main pinned with an older version"
        ),
        "{out}"
    );
    assert!(
        out.contains("k8s/kubernetes: main has 1.31.0, the merge result has 1.30.2"),
        "the line names the pack and both versions:\n{out}"
    );
    assert!(
        out.contains("main never pinned this release in the last 50 commits of releases/dev.json"),
        "the evidence says this is not a return to anything main ran:\n{out}"
    );
    assert!(
        out.contains("the branch wrote this record before main moved to"),
        "the evidence says the author never saw the later release:\n{out}"
    );
    assert!(
        out.contains("Rollback: releases/dev.json to lab-profile@1.2.0"),
        "the report gives the line that declares a rollback:\n{out}"
    );
    assert!(
        text(&o.stderr).contains("1 records: 1 behind main"),
        "{}",
        text(&o.stderr)
    );
}

/// A branch that deliberately returns the record to the release that main pinned before.
fn rollback_gate() -> (Gate, String) {
    let (base, branch, main_side) = gate_records();
    let g = Gate::merged_keeping_its_own(&base, &branch, &main_side);
    (g, base)
}

#[test]
fn merge_gate_passes_a_rollback_that_a_commit_declares() {
    let (g, base) = rollback_gate();
    g.pin(
        &base,
        "pin the release of last week\n\nRollback: releases/dev.json to lab-profile@1.2.0\n",
    );
    let o = g.run(&["releases/*.json", "--base", "main", "--no-verify"]);
    assert!(
        o.status.success(),
        "{}{}{}",
        text(&o.stdout),
        text(&o.stderr),
        g.branch_messages()
    );
    let out = text(&o.stdout);
    assert!(
        out.contains(
            "ROLLBACK: releases/dev.json returns to lab-profile@1.2.0, declared in a commit of the branch"
        ),
        "{out}{}",
        g.branch_messages()
    );
    assert!(
        out.contains("main pinned this release before, on "),
        "the evidence confirms that main ran this release:\n{out}"
    );
    assert!(
        text(&o.stderr).contains("1 records: 0 behind main, 1 declared rollback"),
        "{}",
        text(&o.stderr)
    );
}

#[test]
fn merge_gate_passes_a_rollback_that_the_label_allows() {
    let (g, base) = rollback_gate();
    g.pin(&base, "pin the release of last week");
    let o = g.run(&[
        "releases/*.json",
        "--base",
        "main",
        "--no-verify",
        "--allow-rollback",
    ]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    assert!(
        text(&o.stdout).contains(
            "ROLLBACK: releases/dev.json returns to lab-profile@1.2.0, by the label on the merge request"
        ),
        "{}",
        text(&o.stdout)
    );
}

#[test]
fn merge_gate_stays_red_when_the_declaration_names_another_release() {
    let (g, base) = rollback_gate();
    g.pin(
        &base,
        "pin the release of last week\n\nRollback: releases/dev.json to lab-profile@9.9.9\n",
    );
    let o = g.run(&["releases/*.json", "--base", "main", "--no-verify"]);
    assert_eq!(
        o.status.code(),
        Some(1),
        "a declaration counts only for the release it names"
    );
    let out = text(&o.stdout);
    assert!(out.contains("BEHIND: releases/dev.json"), "{out}");
    // and the evidence still tells the reviewer that main ran this release before
    assert!(
        out.contains("main pinned this release before, on "),
        "{out}"
    );
}

#[test]
fn merge_gate_passes_a_branch_that_carries_the_target_branch_and_moves_forward() {
    let (base, branch, main_side) = gate_records();
    // the target branch did not move, and the branch moves the pin forward
    let g = Gate::new(&base, &branch, &base);
    let o = g.run(&["releases/*.json", "--base", "main", "--no-verify"]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    assert!(
        text(&o.stderr).contains("the merge result is clear against main"),
        "{}",
        text(&o.stderr)
    );

    // and a branch that settled the conflict by taking the later record is clear too
    let g = Gate::merged_keeping_its_own(&base, &main_side, &main_side);
    let o = g.run(&["releases/*.json", "--base", "main", "--no-verify"]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
}

#[test]
fn merge_gate_reads_a_conflict_as_a_release_and_leaves_the_checkout_alone() {
    let (base, branch, main_side) = gate_records();
    let g = Gate::new(&base, &branch, &main_side);
    let o = g.run(&["releases/*.json", "--base", "main", "--no-verify"]);
    assert_eq!(o.status.code(), Some(1));
    let out = text(&o.stdout);
    assert!(
        out.contains("CONFLICT: this branch does not merge origin/main cleanly"),
        "{out}"
    );
    assert!(out.contains("releases/dev.json"), "{out}");
    assert!(
        out.contains("1.31.0") && out.contains("1.30.2"),
        "the conflict is explained as the two releases:\n{out}"
    );
    let status = git(g.dir.path(), &["status", "--porcelain"]);
    assert!(
        text(&status.stdout).trim().is_empty(),
        "the merge is aborted, so the job leaves a clean checkout: {}",
        text(&status.stdout)
    );
}

#[test]
fn merge_gate_needs_a_target_branch_and_a_record() {
    let (base, branch, main_side) = gate_records();
    let g = Gate::new(&base, &branch, &main_side);
    let o = g.run(&["releases/*.json", "--no-verify"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("pass --base"),
        "{}",
        text(&o.stderr)
    );

    // a branch that merges cleanly, so the patterns are what is left to fail on
    let clean = Gate::new(&base, &branch, &base);
    let o = clean.run(&["releases/none/*.json", "--base", "main", "--no-verify"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("no record in the merge result matches"),
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
        executable(&bin.join("curl"), FAKE_GL_CURL);
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
            .arg(script("ci/verify-report.sh"))
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

    /// Run the script the way a GitLab job does; the stand-in curl answers from the same
    /// directory, so its log and the bodies it is handed sit beside the rest.
    fn run_gitlab(&self, args: &[&str]) -> Output {
        let _ = std::fs::remove_file(self.dir.path().join("curl.log"));
        Command::new("bash")
            .arg(script("ci/verify-report.sh"))
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
            .env("FAKE_GL", self.dir.path())
            .env("GITLAB_CI", "true")
            .env("CI_PROJECT_ID", "42")
            .env("CI_API_V4_URL", "https://gitlab.example/api/v4")
            .env("GITLAB_TOKEN", "a-token")
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
fn verify_report_opens_updates_and_closes_gitlab_issues_the_same_way() {
    let d = Drift::new();
    d.summary(
        2,
        &[
            ("releases/a.json", "unchanged", "lab-profile", "1.2.0"),
            ("releases/b.json", "drifted", "lab-profile", "1.3.0"),
            ("releases/c.json", "error", "lab-profile", "1.0.0"),
        ],
    );
    d.record("a", 0, "", "");
    d.record("b", 1, "| `~` | k8s | `kubernetes` |\n", "");
    d.record(
        "c",
        2,
        "",
        "api: GET /v1/clusterprofiles/64f0 failed with HTTP 404\n",
    );
    // one issue is already open for the record that verifies again, and one for the record that
    // still cannot be checked
    d.put(
        "gl-issues.json",
        r#"[{"iid": 7, "title": "strata verify: lab-profile@1.2.0 drifted (releases/a.json)",
             "description": "<!-- strata-verify: releases/a.json -->\nan older report"},
            {"iid": 9, "title": "strata verify: releases/c.json could not be checked",
             "description": "<!-- strata-verify: releases/c.json -->\nan older report"}]"#,
    );

    let o = d.run_gitlab(&["releases/*.json"]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stderr));
    let calls = d.log("curl.log");
    assert!(
        calls[0].starts_with("GET /projects/42/issues?labels=strata-drift&state=opened"),
        "{calls:?}"
    );
    // the record that verifies again is answered with a note and then closed
    let note = calls
        .iter()
        .position(|c| {
            c.starts_with("POST /projects/42/issues/7/notes body=`releases/a.json` verifies again")
        })
        .expect("the closing comment says why");
    let close = calls
        .iter()
        .position(|c| c == "PUT /projects/42/issues/7 state_event=close")
        .expect("and then the issue closes");
    assert!(note < close, "{calls:?}");
    // the record that drifted has no issue yet, so the label comes first and the issue is new
    let label = calls
        .iter()
        .position(|c| c.starts_with("POST /projects/42/labels name=strata-drift"))
        .expect("a label the project lacks is created first");
    let open = calls
        .iter()
        .position(|c| {
            c.starts_with("POST /projects/42/issues title=strata verify: lab-profile@1.3.0 drifted (releases/b.json)")
                && c.contains("labels=strata-drift")
        })
        .expect("the new issue carries the label");
    assert!(label < open, "{calls:?}");
    // the record that still cannot be checked updates the issue it already has
    assert!(
        calls.iter().any(|c| c.starts_with(
            "PUT /projects/42/issues/9 title=strata verify: releases/c.json could not be checked"
        )),
        "{calls:?}"
    );
    assert!(
        !calls.iter().any(|c| c.starts_with("POST /projects/42/issues title=strata verify: releases/c.json")),
        "no second issue for a record that has one: {calls:?}"
    );
    assert!(
        text(&o.stderr).contains("3 records: 1 unchanged, 1 drifted, 1 could not be checked"),
        "{}",
        text(&o.stderr)
    );
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

/// A `tofu` for lib.sh's applies: logs each call and has no outputs.
const FAKE_TOFU_APPLY: &str = r#"#!/usr/bin/env bash
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
    executable(&bin.join("tofu"), FAKE_TOFU_APPLY);
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
