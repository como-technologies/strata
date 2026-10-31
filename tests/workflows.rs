//! Every workflow file under .github/workflows parses as YAML, and has the two keys a workflow
//! needs. GitHub reads each file on each push and records a failed run, with no job, for a file
//! it cannot parse; the reusable workflows never run here, so nothing else would show it. A
//! colon and a space inside an unquoted description is the fault this found.

use std::path::Path;
use std::process::Command;

use crate::common::{executable, git, text};

#[test]
fn every_workflow_file_parses_and_has_on_and_jobs() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows");
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "yml" && e != "yaml") {
            continue;
        }
        seen += 1;
        let text = std::fs::read_to_string(&path).unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&text)
            .unwrap_or_else(|e| panic!("{}: not YAML: {e}", path.display()));
        let map = doc
            .as_mapping()
            .unwrap_or_else(|| panic!("{}: not a mapping", path.display()));
        for key in ["on", "jobs"] {
            assert!(map.contains_key(key), "{}: no `{key}` key", path.display());
        }
    }
    assert!(
        seen >= 5,
        "only {seen} workflow files under {}",
        dir.display()
    );
}

/// The preflight workflow passes each record pattern to strata as the caller typed it. An
/// unquoted `$RECORDS` in a `for` list is expanded by bash first, which reads `**` as `*` and
/// keeps only the files that match, so a pattern such as `releases/**/*.json` loses records.
#[test]
fn the_preflight_workflow_does_not_let_the_shell_expand_the_record_patterns() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/preflight.yml");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        !text.contains("in $RECORDS"),
        "{}: the record patterns go through an unquoted expansion",
        path.display()
    );
    assert!(
        text.contains("read -ra records <<< \"$RECORDS\""),
        "{}: the record patterns are not split with read -ra",
        path.display()
    );
}

/// The verify reads one check run named `Gate` on the head commit of a pull request, and sets no
/// success while that run has none. The job needs `checks` and has no `if`, so GitHub runs it only
/// when each check passed, and skips it when one failed. A renamed job, a lost `needs`, or an
/// `if: always()` lets a verify pass a commit that a check refused.
#[test]
fn the_gate_job_of_ci_needs_the_checks_and_has_no_condition() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/ci.yml");
    let text = std::fs::read_to_string(&path).unwrap();
    let doc: serde_yaml::Value = serde_yaml::from_str(&text).unwrap();
    let jobs = doc["jobs"].as_mapping().expect("ci.yml has jobs");
    let gate = jobs
        .values()
        .find(|job| job["name"].as_str() == Some("Gate"))
        .expect("ci.yml has a job named Gate");
    let needs = gate["needs"].as_str().expect("the Gate needs one job");
    assert!(
        jobs.get(needs).is_some_and(|job| job.get("uses").is_some()),
        "the Gate needs `{needs}`, which is not the job that calls the checks workflow"
    );
    assert!(
        gate.get("if").is_none(),
        "the Gate has a condition: GitHub must skip it when a check fails"
    );
}

/// The tests of `xtask` hold the routes of the stand-in of Palette, and `cargo test` of the root
/// package does not run them. So the `test` recipe runs them by name, and the job `lint and test`
/// runs the recipe. The job has no step of its own for them: they would run two times. The line
/// of the recipe has no `-` before it, so a test that fails makes `just test` exit non-zero.
#[test]
fn the_lint_and_test_job_runs_the_tests_of_xtask() {
    let justfile =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("justfile")).unwrap();
    let recipe: Vec<&str> = justfile
        .lines()
        .skip_while(|line| !line.starts_with("test:"))
        .skip(1)
        .take_while(|line| line.starts_with(' '))
        .map(str::trim)
        .collect();
    assert!(recipe.contains(&"cargo test -p xtask"), "{recipe:?}");

    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/checks.yml");
    let text = std::fs::read_to_string(&path).unwrap();
    let doc: serde_yaml::Value = serde_yaml::from_str(&text).unwrap();
    let job = doc["jobs"]
        .as_mapping()
        .expect("checks.yml has jobs")
        .values()
        .find(|job| job["name"].as_str() == Some("lint and test"))
        .expect("checks.yml has a job named lint and test");
    let runs: Vec<&str> = job["steps"]
        .as_sequence()
        .expect("the job has steps")
        .iter()
        .filter_map(|step| step["run"].as_str())
        .collect();
    assert!(runs.contains(&"just test"), "{runs:?}");
    assert!(!runs.contains(&"cargo test -p xtask"), "{runs:?}");
}

/// A workflow that downloads a release makes sure that the release workflow signed `SHA256SUMS`
/// before it trusts a file against the sums. The check names the repository, the workflow, and
/// the tag, so a release from another workflow or another tag does not pass.
#[test]
fn each_workflow_that_downloads_a_release_verifies_the_signature_of_the_sums_first() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows");
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        if !text.contains("gh release download") {
            continue;
        }
        seen += 1;
        let name = path.display();
        assert!(
            text.contains("uses: sigstore/cosign-installer@"),
            "{name}: no cosign in the job"
        );
        assert!(
            text.contains("--pattern SHA256SUMS --pattern SHA256SUMS.sigstore.json"),
            "{name}: the bundle of SHA256SUMS does not come down"
        );
        assert!(
            text.contains(
                "--certificate-identity \"https://github.com/como-technologies/strata/.github/workflows/release.yml@refs/tags/$VERSION\""
            ) && text.contains(
                "--certificate-oidc-issuer https://token.actions.githubusercontent.com"
            ),
            "{name}: the check does not name the identity of the release workflow"
        );
        let verify = text.find("cosign verify-blob SHA256SUMS --bundle SHA256SUMS.sigstore.json");
        let sums = text.find("sha256sum --check");
        assert!(
            verify.is_some() && verify < sums,
            "{name}: the signature check must come before the check of the sums"
        );
    }
    assert!(seen >= 7, "only {seen} workflows download a release");
}

/// The last job of the release workflow installs the tag that it published, as a consumer does.
/// It fails when the signature of `SHA256SUMS` does not verify, and when `cosign` is not there.
#[test]
fn the_release_check_installs_the_published_tag_with_a_required_signature() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml");
    let doc: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let steps = doc["jobs"]["check"]["steps"]
        .as_sequence()
        .expect("the release workflow has a job named check");
    let at = |wanted: &dyn Fn(&serde_yaml::Value) -> bool| steps.iter().position(wanted);
    let cosign = at(&|s| {
        s["uses"]
            .as_str()
            .is_some_and(|u| u.starts_with("sigstore/cosign-installer@"))
    })
    .expect("the check job installs cosign");
    let install = at(&|s| {
        s["run"].as_str().is_some_and(|r| {
            r.contains("scripts/ci/install-strata.sh") && r.contains("--require-signature")
        })
    })
    .expect("the check job installs the tag with --require-signature");
    assert!(cosign < install, "cosign comes before the installer runs");
}

// ---- the steps of the reusable workflows ---------------------------------------------------------
//
// `act` is not in every container, so these tests run the `run` script of a step in bash, as the
// runner of GitHub does, with a stand-in for each tool the step calls. The expressions of GitHub
// (`${{ ... }}`) do not run here: a test gives the value that an expression has.

/// A step that ran in a directory of its own, and the calls that its stand-ins logged.
struct StepRun {
    dir: tempfile::TempDir,
    output: std::process::Output,
}

impl StepRun {
    /// The calls of the stand-ins, one for each line: the name of the tool and its arguments.
    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("calls.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The text that the step appended to the summary of the job.
    fn summary(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("summary.md")).unwrap_or_default()
    }
}

/// A stand-in that logs its name and its arguments, prints `FAKE_OUT`, and exits with
/// `FAKE_EXIT`.
const LOGGING_TOOL: &str = r#"#!/usr/bin/env bash
echo "$(basename "$0") $*" >> "$FAKE_LOG"
printf '%s' "${FAKE_OUT-}"
exit "${FAKE_EXIT:-0}"
"#;

/// Run the `run` script of the step `step` of the workflow `file` in a fresh directory.
///
/// Each variable of the `env` of the step is empty unless `env` gives it a value. `tools` are the
/// paths, under the directory, of the stand-ins that [`LOGGING_TOOL`] describes.
fn run_step(file: &str, step: &str, env: &[(&str, &str)], tools: &[&str]) -> StepRun {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".github/workflows")
        .join(file);
    let doc: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let step = doc["jobs"]
        .as_mapping()
        .unwrap()
        .values()
        .flat_map(|job| job["steps"].as_sequence().into_iter().flatten())
        .find(|s| s["name"].as_str() == Some(step))
        .unwrap_or_else(|| panic!("{file} has no step named {step}"));
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    for tool in tools {
        let at = dir.path().join(tool);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        executable(&at, LOGGING_TOOL);
    }
    let mut command = Command::new("bash");
    command
        .args(["--noprofile", "--norc", "-eo", "pipefail", "-c"])
        .arg(step["run"].as_str().expect("the step runs a script"))
        .current_dir(dir.path())
        .env("FAKE_LOG", dir.path().join("calls.log"))
        .env("GITHUB_STEP_SUMMARY", dir.path().join("summary.md"));
    for name in step["env"].as_mapping().into_iter().flat_map(|e| e.keys()) {
        command.env(name.as_str().unwrap(), "");
    }
    command.envs(env.iter().copied());
    let output = command.output().unwrap();
    StepRun { dir, output }
}

#[test]
fn the_preflight_step_passes_the_options_of_a_watch_to_strata() {
    let run = run_step(
        "preflight.yml",
        "preflight",
        &[
            ("SOURCE", "cluster:64f0"),
            ("WATCH", "true"),
            ("EVERY", "5"),
            ("LIMIT", "60"),
            ("NO_EXEC", "true"),
            ("RULE", "eks-subnets eks-roles"),
            ("PALETTE_API_KEY", "a-key"),
            ("OUT", "out"),
            ("FAKE_OUT", "# the report\n"),
        ],
        &[".strata/bin/strata"],
    );
    assert!(run.output.status.success(), "{}", text(&run.output.stderr));
    assert_eq!(
        run.calls(),
        [
            "strata preflight cluster:64f0 --every 5 --limit 60 --no-exec --watch --quiet \
             --rule eks-subnets --rule eks-roles --out out --format markdown"
        ]
    );
    assert_eq!(run.summary(), "# the report\n");
}

#[test]
fn the_preflight_step_without_a_key_reads_nothing_and_a_verbose_run_omits_quiet() {
    let run = run_step(
        "preflight.yml",
        "preflight",
        &[
            ("SOURCE", "clusters/payments.json"),
            ("CLOUD", "eks"),
            ("TIER", "static"),
            ("RECORDS", "releases/**/*.json other/*.json"),
            ("VERBOSE", "true"),
            ("OUT", "preflight"),
        ],
        &[".strata/bin/strata"],
    );
    assert!(run.output.status.success(), "{}", text(&run.output.stderr));
    assert_eq!(
        run.calls(),
        [
            "strata preflight clusters/payments.json --cloud eks --tier static --verbose \
             --offline --records releases/**/*.json --records other/*.json \
             --out preflight --format markdown"
        ]
    );
}

#[test]
fn the_preflight_step_keeps_the_report_and_the_exit_status_of_a_failed_run() {
    let run = run_step(
        "preflight.yml",
        "preflight",
        &[
            ("SOURCE", "clusters/payments.json"),
            ("OUT", "preflight"),
            ("FAKE_OUT", "# 1 check failed\n"),
            ("FAKE_EXIT", "1"),
        ],
        &[".strata/bin/strata"],
    );
    assert_eq!(run.output.status.code(), Some(1));
    assert_eq!(run.summary(), "# 1 check failed\n");
}

#[test]
fn the_preflight_comment_step_posts_the_report_of_the_out_directory() {
    let run = run_step(
        "preflight.yml",
        "comment on the pull request",
        &[("OUT", "out"), ("PR", "7"), ("REPO", "o/r")],
        &[".strata/scripts/ci/pr-comment.sh"],
    );
    assert!(run.output.status.success(), "{}", text(&run.output.stderr));
    assert_eq!(
        run.calls(),
        ["pr-comment.sh --preflight out/report.md --pr 7 --repo o/r"]
    );
}

#[test]
fn the_merge_gate_step_passes_allow_no_digest_when_the_input_is_on() {
    let tools = [".strata/scripts/ci/merge-gate.sh"];
    let on = run_step(
        "merge-gate.yml",
        "check the merge result",
        &[
            ("RECORDS", "releases/**/*.json"),
            ("ALLOW_NO_DIGEST", "--allow-no-digest"),
        ],
        &tools,
    );
    assert!(on.output.status.success(), "{}", text(&on.output.stderr));
    assert_eq!(
        on.calls(),
        ["merge-gate.sh releases/**/*.json --allow-no-digest"]
    );
    let off = run_step(
        "merge-gate.yml",
        "check the merge result",
        &[("RECORDS", "releases/**/*.json")],
        &tools,
    );
    assert_eq!(off.calls(), ["merge-gate.sh releases/**/*.json"]);
}

#[test]
fn the_verify_step_passes_allow_no_digest_and_who_when_the_inputs_are_on() {
    let tools = [".strata/scripts/ci/verify-report.sh"];
    let env = [
        ("RECORDS", "releases/*.json"),
        ("LABEL", "strata-drift"),
        ("REPO", "o/r"),
    ];
    let off = run_step("verify.yml", "verify and report", &env, &tools);
    assert_eq!(
        off.calls(),
        ["verify-report.sh releases/*.json --repo o/r --label strata-drift"]
    );
    let mut on_env = env.to_vec();
    on_env.extend([("ALLOW_NO_DIGEST", "--allow-no-digest"), ("WHO", "--who")]);
    let on = run_step("verify.yml", "verify and report", &on_env, &tools);
    assert!(on.output.status.success(), "{}", text(&on.output.stderr));
    assert_eq!(
        on.calls(),
        [
            "verify-report.sh releases/*.json --repo o/r --label strata-drift --allow-no-digest --who"
        ]
    );
}

#[test]
fn the_upstream_step_passes_no_impact_and_save_when_the_inputs_are_on() {
    let tools = [".strata/scripts/ci/upstream-report.sh"];
    let env = [
        ("RECORDS", "releases/*.json"),
        ("LABEL", "strata-behind"),
        ("REPO", "o/r"),
    ];
    let off = run_step("upstream.yml", "check the pins and report", &env, &tools);
    assert!(off.output.status.success(), "{}", text(&off.output.stderr));
    assert_eq!(
        off.calls(),
        ["upstream-report.sh releases/*.json --repo o/r --label strata-behind"]
    );
    let mut on_env = env.to_vec();
    on_env.extend([("NO_IMPACT", "--no-impact"), ("SAVE", "behind.json")]);
    let on = run_step("upstream.yml", "check the pins and report", &on_env, &tools);
    assert_eq!(
        on.calls(),
        [
            "upstream-report.sh releases/*.json --repo o/r --label strata-behind --no-impact --save behind.json"
        ]
    );
}

#[test]
fn the_promote_gate_step_gives_each_allowed_difference_its_own_option() {
    let tools = [".strata/bin/strata"];
    let env = [("RECORDS", "dev.json prod.json")];
    let none = run_step(
        "promote-gate.yml",
        "the records are one release",
        &env,
        &tools,
    );
    assert!(
        none.output.status.success(),
        "{}",
        text(&none.output.stderr)
    );
    assert_eq!(none.calls(), ["strata same-release dev.json prod.json"]);

    // a list that the caller separates by spaces, or by lines
    for allow in ["registry-move no-digest", "registry-move\nno-digest"] {
        let mut env = env.to_vec();
        env.push(("ALLOW", allow));
        let run = run_step(
            "promote-gate.yml",
            "the records are one release",
            &env,
            &tools,
        );
        assert_eq!(
            run.calls(),
            ["strata same-release dev.json prod.json --allow registry-move --allow no-digest"]
        );
    }
}
