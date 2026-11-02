//! The scripts under scripts/dev/, and examples/fetch-examples.sh, which only this repository runs.
//! No network: every host and every command outside this repository is a recording stand-in.

use crate::common;

use std::path::Path;
use std::process::{Command, Output};

use common::*;

fn tag_release(dir: &Path) -> Output {
    let mut c = Command::new("bash");
    c.arg(script("dev/tag-release.sh")).current_dir(dir);
    isolate_git(&mut c).output_locked().unwrap()
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

// ---- set-version.sh ------------------------------------------------------------------------

/// A `just` that stands in for the recipe `docs-examples`. It writes the version of `Cargo.toml`
/// in the output of the book that names a version, as a build of that version does.
/// `FAKE_STALE` makes it change an output that names no version, and `FAKE_JUST_EXIT` makes it fail.
const FAKE_JUST_DOCS: &str = r#"#!/usr/bin/env bash
echo "just $*" >> "$FAKE_LOG"
[ "$1" = docs-examples ] || exit 2
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml)
sed -i "s/strata [0-9][0-9.]*$/strata $version/" docs/src/output/report.txt
[ -z "${FAKE_STALE:-}" ] || echo "changed" > docs/src/output/list.txt
exit "${FAKE_JUST_EXIT:-0}"
"#;

/// A repository at version `0.9.9` with `set-version.sh`, a lock file, and two outputs of the
/// book. `report.txt` prints `printed` as the version of strata. `list.txt` names no version of
/// strata. The commit has the subject `init`.
fn release_tree(printed: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    git(d, &["init", "-q", "-b", "main"]);
    std::fs::write(
        d.join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.9.9\"\n",
    )
    .unwrap();
    std::fs::write(d.join("Cargo.lock"), "# the lock\n").unwrap();
    copy_scripts(d, &["scripts/dev/set-version.sh"]);
    let output = d.join("docs/src/output");
    std::fs::create_dir_all(&output).unwrap();
    std::fs::write(
        output.join("report.txt"),
        format!(
            "$ strata report\n  ProfileType  later-type  /v1/packs  2 times  strata {printed}\n"
        ),
    )
    .unwrap();
    std::fs::write(output.join("list.txt"), "$ strata list\naws-stack@1.1.1\n").unwrap();
    git(d, &["add", "."]);
    git(d, &["commit", "-q", "-m", "init"]);
    dir
}

/// Run `set-version.sh` in `dir` with the stand-ins `cargo` and `just`. Gives the output and the
/// calls that the stand-ins got.
fn set_version(dir: &Path, new: &str, env: &[(&str, &str)]) -> (Output, Vec<String>) {
    let bin = fake_bin(dir, &[("cargo", LOGGING_TOOL), ("just", FAKE_JUST_DOCS)]);
    let log = dir.join("calls.log");
    let mut c = Command::new("bash");
    c.arg(dir.join("scripts/dev/set-version.sh"))
        .arg(new)
        .current_dir(dir)
        .env("PATH", path_with(&bin))
        .env("FAKE_LOG", &log)
        .envs(env.iter().copied());
    let o = isolate_git(&mut c).output_locked().unwrap();
    let calls = std::fs::read_to_string(&log)
        .unwrap_or_default()
        .lines()
        .map(String::from)
        .collect();
    (o, calls)
}

/// The text of `file` in the last commit of `dir`.
fn committed(dir: &Path, file: &str) -> String {
    text(&git(dir, &["show", &format!("HEAD:{file}")]).stdout)
}

#[test]
fn set_version_writes_the_outputs_of_the_book_that_print_the_version_in_the_release_commit() {
    // the outputs of the book print the version of strata, so `just docs-check` failed on the
    // release commit until a person wrote them again by hand
    let dir = release_tree("0.9.9");
    let (o, calls) = set_version(dir.path(), "0.9.10", &[]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert_eq!(
        calls,
        ["cargo update --offline --workspace", "just docs-examples"]
    );
    let d = dir.path();
    assert_eq!(
        text(&git(d, &["log", "-1", "--format=%s"]).stdout).trim(),
        "chore: release 0.9.10"
    );
    assert!(committed(d, "docs/src/output/report.txt").ends_with("2 times  strata 0.9.10\n"));
    assert!(committed(d, "Cargo.toml").contains("version = \"0.9.10\""));
    assert_eq!(
        committed(d, "docs/src/output/list.txt"),
        "$ strata list\naws-stack@1.1.1\n"
    );
    assert_eq!(
        text(&git(d, &["status", "--porcelain", "--untracked-files=no"]).stdout),
        "",
        "the release commit holds each change"
    );
}

#[test]
fn set_version_runs_the_book_only_when_an_output_names_the_old_version() {
    let dir = release_tree("1.0.0");
    let (o, calls) = set_version(dir.path(), "0.9.10", &[]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert_eq!(
        calls,
        ["cargo update --offline --workspace"],
        "no output names 0.9.9"
    );
}

#[test]
fn set_version_commits_nothing_when_an_output_that_names_no_version_changes() {
    // the book was not current before the release: the commit must not hide that
    let dir = release_tree("0.9.9");
    let (o, _) = set_version(dir.path(), "0.9.10", &[("FAKE_STALE", "1")]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        text(&o.stderr).contains("they did not name 0.9.9:\n  docs/src/output/list.txt\n"),
        "{}",
        text(&o.stderr)
    );
    let d = dir.path();
    assert_eq!(
        text(&git(d, &["log", "-1", "--format=%s"]).stdout).trim(),
        "init"
    );
}

#[test]
fn set_version_commits_nothing_when_the_book_does_not_build() {
    let dir = release_tree("0.9.9");
    let (o, _) = set_version(dir.path(), "0.9.10", &[("FAKE_JUST_EXIT", "1")]);
    assert_eq!(o.status.code(), Some(1));
    assert!(text(&o.stderr).contains("just docs-examples failed; nothing is committed"));
    assert_eq!(
        text(&git(dir.path(), &["log", "-1", "--format=%s"]).stdout).trim(),
        "init"
    );
}

#[test]
fn set_version_refuses_an_output_of_the_book_that_holds_uncommitted_changes() {
    let dir = release_tree("0.9.9");
    std::fs::write(
        dir.path().join("docs/src/output/report.txt"),
        "edited by hand strata 0.9.9\n",
    )
    .unwrap();
    let (o, calls) = set_version(dir.path(), "0.9.10", &[]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        text(&o.stderr).contains("docs/src/output"),
        "{}",
        text(&o.stderr)
    );
    assert!(calls.is_empty(), "{calls:?}");
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
        .output_locked()
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

// ---- the fuzz targets -----------------------------------------------------------------------

/// The output of `cargo bolero list` for the targets `names`: a line of JSON for each one, with a
/// blank line after it.
fn bolero_list(names: &[&str]) -> String {
    names
        .iter()
        .map(|name| format!("{{\"package\":\"strata-fuzz\",\"test\":\"{name}\"}}\n\n"))
        .collect()
}

/// Run `fuzz.sh` with a stand-in `cargo` that prints the list of `targets` and logs its calls.
/// Gives the output of the script, and the calls of the stand-in.
fn fuzz(args: &[&str], targets: &[&str], toolchain: Option<&str>) -> (Output, String) {
    let dir = tempfile::tempdir().unwrap();
    let bin = fake_bin(dir.path(), &[("cargo", LOGGING_TOOL)]);
    let calls = dir.path().join("calls.log");
    let mut c = Command::new("bash");
    c.arg(script("dev/fuzz.sh"))
        .args(args)
        .env("PATH", path_with(&bin))
        .env("FAKE_LOG", &calls)
        .env("FAKE_OUT", bolero_list(targets))
        .env_remove("FUZZ_TOOLCHAIN");
    if let Some(toolchain) = toolchain {
        c.env("FUZZ_TOOLCHAIN", toolchain);
    }
    let o = c.output_locked().unwrap();
    (o, std::fs::read_to_string(calls).unwrap_or_default())
}

const FUZZ_TARGETS: [&str; 4] = [
    "mask::a_draft_pack_keeps_no_secret",
    "mask::a_saved_cluster_keeps_no_password",
    "reference::a_ref",
    "reference::a_ref_prints_as_the_ref_it_is",
];

#[test]
fn fuzz_list_names_each_target_of_the_fuzz_package_in_order_and_nothing_else() {
    // cargo-bolero lists the targets in the order that the test threads finish
    let mut shuffled = FUZZ_TARGETS;
    shuffled.reverse();
    let (o, calls) = fuzz(&["list"], &shuffled, None);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert_eq!(text(&o.stdout), FUZZ_TARGETS.join("\n") + "\n");
    assert_eq!(calls, "cargo bolero list -p strata-fuzz\n");
}

#[test]
fn fuzz_runs_the_one_target_that_a_start_of_its_name_names_on_nightly_for_the_time_given() {
    let (o, calls) = fuzz(&["run", "mask::a_saved", "10m"], &FUZZ_TARGETS, None);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert_eq!(
        calls,
        "cargo bolero list -p strata-fuzz\n\
         cargo bolero test mask::a_saved_cluster_keeps_no_password -p strata-fuzz \
         --toolchain nightly -T 10m -l 65536\n"
    );
}

#[test]
fn fuzz_runs_for_a_minute_by_default_and_on_the_toolchain_that_the_variable_names() {
    let (o, calls) = fuzz(
        &["run", "reference::a_ref_prints"],
        &FUZZ_TARGETS,
        Some("nightly-2026-09-01"),
    );
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        calls.ends_with(
            "reference::a_ref_prints_as_the_ref_it_is -p strata-fuzz --toolchain nightly-2026-09-01 -T 60s -l 65536\n"
        ),
        "{calls}"
    );
}

#[test]
fn fuzz_runs_the_whole_name_that_is_also_the_start_of_another_name() {
    let (o, calls) = fuzz(&["run", "reference::a_ref"], &FUZZ_TARGETS, None);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        calls.contains("bolero test reference::a_ref -p strata-fuzz"),
        "{calls}"
    );
}

#[test]
fn fuzz_refuses_a_start_of_a_name_that_two_targets_have_and_runs_nothing() {
    let (o, calls) = fuzz(&["run", "mask::"], &FUZZ_TARGETS, None);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stderr));
    let stderr = text(&o.stderr);
    assert!(
        stderr.contains("more than one target starts with 'mask::'"),
        "{stderr}"
    );
    assert!(
        stderr.contains("mask::a_saved_cluster_keeps_no_password"),
        "{stderr}"
    );
    assert!(!calls.contains("bolero test"), "{calls}");
}

#[test]
fn fuzz_refuses_a_name_that_no_target_has_and_a_call_with_no_name() {
    let (o, calls) = fuzz(&["run", "nothing::here"], &FUZZ_TARGETS, None);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stderr));
    assert!(text(&o.stderr).contains("no target starts with 'nothing::here'"));
    assert!(!calls.contains("bolero test"), "{calls}");

    for args in [&["run"][..], &[][..], &["start"][..]] {
        let (o, calls) = fuzz(args, &FUZZ_TARGETS, None);
        assert_eq!(o.status.code(), Some(2), "{args:?}: {}", text(&o.stderr));
        assert!(!calls.contains("bolero test"), "{args:?}: {calls}");
    }
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
    let mut playground = Command::new(&stand_in);
    give_strata(&mut playground, "STRATA_BIN");
    let mut child = playground
        .args(["playground", last])
        .stdout(std::fs::File::create(&log).unwrap())
        .stderr(std::process::Stdio::null())
        .spawn_locked()
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
    let o = strata_command()
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
        .output_locked()
        .unwrap();
    let text = text(&o.stdout);
    // stop the script first, so a failed assertion leaves no server behind
    let _ = Command::new("kill")
        .arg(child.id().to_string())
        .output_locked();
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

// ---- the fetch of the examples -------------------------------------------------------------

/// The versions that `oldest_first` of `examples/fetch-examples.sh` gives for `versions`. The test
/// reads the function from the script and runs it alone, so the script itself fetches nothing.
fn oldest_first(versions: &[&str]) -> Vec<String> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(repo.join("examples/fetch-examples.sh")).unwrap();
    let start = source.find("\noldest_first() {").expect("the function") + 1;
    let function = &source[start..start + source[start..].find("\n}\n").expect("its end") + 3];
    let o = Command::new("bash")
        .args([
            "-c",
            &format!("{function}\nprintf '%s\\n' \"$VERSIONS\" | oldest_first"),
        ])
        .env("VERSIONS", versions.join("\n"))
        .output_locked()
        .unwrap();
    assert!(o.status.success(), "{}", text(&o.stderr));
    text(&o.stdout).lines().map(str::to_string).collect()
}

#[test]
fn the_examples_fetch_orders_plain_versions_as_strata_does() {
    let plain = [
        "1.9.0", "1.10.0", "1.63.1", "1.30", "1.30.0", "22.04", "22.4", "1.2.10", "1.2.3", "10",
        "9", "0.1", "1.65.0",
    ];
    let got = oldest_first(&plain);
    assert_eq!(got.len(), plain.len(), "no plain version is lost: {got:?}");
    for pair in got.windows(2) {
        assert!(
            strata::reference::version::compare(&pair[0], &pair[1]).is_le(),
            "{} comes before {}: {got:?}",
            pair[0],
            pair[1]
        );
    }
    // the examples omit a version that is not plain numbers, as the comment of the function says
    let mixed = [
        "1.0.1-rc.1",
        "v1.20.0",
        "LTS__24.x",
        "1.2.3.4",
        "1.0.1",
        "1.0.0",
    ];
    assert_eq!(oldest_first(&mixed), ["1.0.0", "1.0.1"]);
}

#[test]
fn the_examples_fetch_stops_the_send_of_the_report_whatever_the_machine_says() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("send.log");
    // a strata that writes down the setting it got, and then runs the real one
    let wrapper = dir.path().join("strata");
    executable(
        &wrapper,
        "#!/usr/bin/env bash\necho \"$STRATA_REPORT\" >> \"$FAKE_LOG\"\nexec \"$REAL_STRATA\" \"$@\"\n",
    );
    let (_, o) = fetch_examples(
        Some(&wrapper),
        &[("STRATA_REPORT", "1"), ("FAKE_LOG", log.to_str().unwrap())],
    );
    assert!(o.status.success(), "{}", text(&o.stderr));
    let runs = lines_of(&log);
    assert!(!runs.is_empty(), "the script ran strata");
    assert!(runs.iter().all(|setting| setting == "0"), "{runs:?}");
}

/// The scripts of this repository that run the binary of strata, but not the drop-ins that a
/// pipeline of a user runs: those obey the settings of the user. `adapter/scripts/by-hand.sh` is
/// not in the list: it serves two chapters of the book, and their tenants hold no unknown value.
fn scripts_that_run_strata() -> Vec<std::path::PathBuf> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let own = ["examples", "deploy", "scripts/dev"];
    // the binary by its path, by the variable of the script, or by the variable of lib.sh
    let runs = [
        "STRATA_BIN",
        "target/release/strata",
        "target/debug/strata",
        "\"$strata\"",
    ];
    own.iter()
        .flat_map(|dir| walk(&repo.join(dir)))
        .filter(|file| file.extension().is_some_and(|e| e == "sh"))
        .filter(|file| !file.starts_with(repo.join("examples/ci")))
        .filter(|file| {
            let text = std::fs::read_to_string(file).unwrap();
            runs.iter().any(|word| text.contains(word))
        })
        .collect()
}

#[test]
fn each_script_that_runs_strata_stops_the_send_of_the_report_or_sources_lib_sh_which_does() {
    let scripts = scripts_that_run_strata();
    assert!(scripts.len() > 20, "{scripts:?}");
    let lib = live_script("lib.sh");
    assert!(scripts.contains(&lib));
    let without: Vec<_> = scripts
        .iter()
        .filter(|file| {
            let text = std::fs::read_to_string(file).unwrap();
            !text.contains("/lib.sh") && !text.contains("STRATA_REPORT=0")
        })
        .collect();
    assert!(
        without.is_empty(),
        "a run of these scripts can send a report of the machine of the person: {without:?}"
    );
}
