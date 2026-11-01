//! The scripts under scripts/dev/, which only this repository runs.
//! No network: every host and every command outside this repository is a recording stand-in.

use crate::common;

use std::path::Path;
use std::process::{Command, Output};

use common::*;

fn tag_release(dir: &Path) -> Output {
    let mut c = Command::new("bash");
    c.arg(script("dev/tag-release.sh")).current_dir(dir);
    isolate_git(&mut c).output().unwrap()
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

// ---- the build of the book -----------------------------------------------------------------

/// Run `build-book.sh` with a stand-in `mdbook` that prints `log` and exits with `exit`. Gives the
/// output of the script, and the arguments that the stand-in got.
fn build_book(args: &[&str], log: &str, exit: &str) -> (Output, String) {
    let dir = tempfile::tempdir().unwrap();
    let bin = fake_bin(dir.path(), &[("mdbook", LOGGING_TOOL)]);
    let calls = dir.path().join("calls.log");
    let o = Command::new("bash")
        .arg(script("dev/build-book.sh"))
        .args(args)
        .env("PATH", path_with(&bin))
        .env("FAKE_LOG", &calls)
        .env("FAKE_OUT", log)
        .env("FAKE_EXIT", exit)
        .output()
        .unwrap();
    (o, std::fs::read_to_string(calls).unwrap_or_default())
}

#[test]
fn build_book_fails_when_mdbook_prints_a_warning_and_exits_0() {
    let log = " INFO Book building has started\n WARN unclosed HTML tag `<name>` found in `a.md` while exiting TableCell\nHTML tags must be closed before exiting a markdown element.\n INFO HTML book written to `book`\n";
    let (o, calls) = build_book(&["docs"], log, "0");
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    let stderr = text(&o.stderr);
    assert!(
        stderr.contains("unclosed HTML tag `<name>`"),
        "the script shows the warning of mdbook:\n{stderr}"
    );
    assert!(
        stderr.contains("build-book: mdbook printed a warning"),
        "{stderr}"
    );
    assert_eq!(
        calls, "mdbook build docs\n",
        "the script builds the book it was given"
    );
}

#[test]
fn build_book_fails_on_a_warning_that_mdbook_colors() {
    let log = " INFO Book building has started\n\x1b[33m WARN\x1b[0m a page is wrong\n";
    let (o, _) = build_book(&[], log, "0");
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
}

#[test]
fn build_book_passes_a_log_with_no_warning_of_mdbook_and_builds_docs_by_default() {
    // The note of mdbook-mermaid is not a log line of mdbook: a new mdbook-mermaid fixes it.
    let log = " INFO Book building has started\nWarning: The mdbook-mermaid preprocessor was built against version 0.5.0 of mdbook, but we're being called from version 0.5.4\n INFO Running the html backend\n INFO a WARN in the text of an INFO line\n INFO HTML book written to `book`\n";
    let (o, calls) = build_book(&[], log, "0");
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert_eq!(calls, "mdbook build docs\n");
    assert!(
        text(&o.stderr).contains("HTML book written"),
        "the script shows the log of mdbook"
    );
}

#[test]
fn build_book_gives_the_exit_code_of_a_build_that_fails() {
    let (o, _) = build_book(&["docs"], "ERROR the book has no SUMMARY.md\n", "3");
    assert_eq!(o.status.code(), Some(3), "{}", text(&o.stderr));
    assert!(text(&o.stderr).contains("ERROR the book has no SUMMARY.md"));
}

// ---- xtask playground ----------------------------------------------------------------------

/// The playground: `xtask playground CHAPTER` builds the tenant of one chapter, serves it, and
/// waits. The test needs the offline examples and the xtask binary, which `just examples` and
/// `just stand-in` build; without them it has nothing to serve and returns.
#[test]
fn the_playground_serves_the_tenant_of_one_chapter() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let built = repo.join("examples/profiles/raw/aws-stack@1.1.1/profile.json");
    let stand_in = repo.join("target/debug/xtask");
    if !built.exists() || !stand_in.exists() {
        return;
    }
    // The LAST chapter. Serving it builds every chapter before it, so a statement that changes
    // the tenant outside a guarded helper runs with no tenant to change and kills the run.
    // Serving an early chapter builds none of them, which is why an earlier form of this test
    // passed while `just playground packs` failed for eleven chapters.
    let last = "automation";
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("serve.log");
    let mut child = Command::new(&stand_in)
        .args(["playground", last])
        .env("STRATA_BIN", strata_bin())
        .stdout(std::fs::File::create(&log).unwrap())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();

    // the settings it prints are its contract: the address, the key, and the project uid
    let mut printed = String::new();
    for _ in 0..600 {
        printed = std::fs::read_to_string(&log).unwrap_or_default();
        if printed.contains("PALETTE_PROJECT_UID=") && printed.contains("Ctrl-C") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let value = |key: &str| -> String {
        printed
            .split_whitespace()
            .find(|w| w.starts_with(key))
            .unwrap_or_else(|| panic!("no {key} in:\n{printed}"))
            .trim_start_matches(key)
            .to_string()
    };
    let api = value("PALETTE_API=");
    let uid = value("PALETTE_PROJECT_UID=");

    // the commands of the book run against it, by name, with no key of a real tenant
    let o = Command::new(strata_bin())
        .args([
            "compare",
            "aws-stack@1.0.0",
            "aws-stack@1.1.0",
            "--quiet",
            "--no-color",
        ])
        .env("PALETTE_API", &api)
        .env("PALETTE_API_KEY", "docs-example-key")
        .env("PALETTE_PROJECT_UID", &uid)
        .env("HOME", dir.path())
        .output()
        .unwrap();
    let text = text(&o.stdout);
    // stop the script first, so a failed assertion leaves no server behind
    let _ = Command::new("kill").arg(child.id().to_string()).status();
    let _ = child.wait();

    assert_eq!(o.status.code(), Some(1), "the two versions differ: {text}");
    assert!(
        text.contains("aws-stack@1.0.0") && text.contains("aws-stack@1.1.0"),
        "the playground answered by name:\n{text}"
    );
    assert!(
        !printed.contains("api.spectrocloud.com"),
        "nothing points at a real tenant:\n{printed}"
    );
}
