//! Every workflow file under .github/workflows parses as YAML, and has the two keys a workflow
//! needs. GitHub reads each file on each push and records a failed run, with no job, for a file
//! it cannot parse; the reusable workflows never run here, so nothing else would show it. A
//! colon and a space inside an unquoted description is the fault this found.

use std::process::Command;

use crate::common::{
    INSTALLING_WORKFLOWS, LOGGING_TOOL, executable, git, repo_file, steps, text, workflow,
    workflow_files, workflow_text,
};

#[test]
fn every_workflow_file_parses_and_has_on_and_jobs() {
    let files = workflow_files();
    for file in &files {
        let doc = workflow(file);
        let map = doc
            .as_mapping()
            .unwrap_or_else(|| panic!("{file}: not a mapping"));
        for key in ["on", "jobs"] {
            assert!(map.contains_key(key), "{file}: no `{key}` key");
        }
    }
    assert!(
        files.len() >= 5,
        "only {} workflow files under .github/workflows",
        files.len()
    );
}

#[test]
fn the_steps_of_a_workflow_are_the_steps_of_each_job_in_order_and_a_job_with_no_steps_adds_none() {
    let doc: serde_yaml::Value = serde_yaml::from_str(
        "jobs:\n  a:\n    steps:\n      - name: one\n      - name: two\n  b:\n    uses: ./x.yml\n  c:\n    steps:\n      - name: three\n",
    )
    .unwrap();
    let names: Vec<&str> = steps(&doc).filter_map(|s| s["name"].as_str()).collect();
    assert_eq!(names, ["one", "two", "three"]);
}

/// The verify reads one check run named `Gate` on the head commit of a pull request, and sets no
/// success while that run has none. The job needs `checks` and has no `if`, so GitHub runs it only
/// when each check passed, and skips it when one failed. A renamed job, a lost `needs`, or an
/// `if: always()` lets a verify pass a commit that a check refused.
#[test]
fn the_gate_job_of_ci_needs_the_checks_and_has_no_condition() {
    let doc = workflow("ci.yml");
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
/// runs the recipe. The job has no step of its own for them, because then they run two times.
/// The line of the recipe has no `-` before it, so a test that fails makes `just test` exit
/// non-zero.
#[test]
fn the_lint_and_test_job_runs_the_tests_of_xtask() {
    let justfile = repo_file("justfile");
    let recipe: Vec<&str> = justfile
        .lines()
        .skip_while(|line| !line.starts_with("test:"))
        .skip(1)
        .take_while(|line| line.starts_with(' '))
        .map(str::trim)
        .collect();
    assert!(recipe.contains(&"cargo test -p xtask"), "{recipe:?}");

    let doc = workflow("checks.yml");
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

/// A workflow that installs a release runs `install-strata.sh` from this repository at the tag
/// of `strata-version`. The installer makes sure that the release workflow signed `SHA256SUMS`,
/// and checks each file against the sums. A workflow has no copy of that check, so one path
/// holds the rule of the signature. The checkout of the tag and cosign come before the install.
#[test]
fn each_workflow_that_installs_a_release_runs_the_installer_with_cosign_and_the_tag_checked_out() {
    for file in INSTALLING_WORKFLOWS {
        let doc = workflow(file);
        let steps: Vec<&serde_yaml::Value> = steps(&doc).collect();
        let position = |wanted: &dyn Fn(&serde_yaml::Value) -> bool| {
            steps
                .iter()
                .copied()
                .position(wanted)
                .unwrap_or_else(|| panic!("{file}: a step is missing"))
        };
        let checkout = position(&|s| {
            s["uses"]
                .as_str()
                .is_some_and(|u| u.starts_with("actions/checkout@"))
                && s["with"]["repository"].as_str() == Some("como-technologies/strata")
                && s["with"]["ref"].as_str() == Some("${{ inputs.strata-version }}")
                && s["with"]["path"].as_str() == Some(".strata")
        });
        let cosign = position(&|s| {
            s["uses"]
                .as_str()
                .is_some_and(|u| u.starts_with("sigstore/cosign-installer@"))
        });
        let install = position(&|s| {
            s["run"].as_str().is_some_and(|r| {
                r.contains(".strata/scripts/ci/install-strata.sh")
                    && r.contains("--require-signature")
            })
        });
        assert!(
            checkout < install && cosign < install,
            "{file}: the checkout of the tag and cosign come before the installer"
        );
        // the installer reads the token from GH_TOKEN, and the version from the call
        assert_eq!(
            steps[install]["env"]["GH_TOKEN"].as_str(),
            Some("${{ secrets.strata-token || github.token }}"),
            "{file}"
        );
        assert_eq!(
            steps[install]["env"]["VERSION"].as_str(),
            Some("${{ inputs.strata-version }}"),
            "{file}"
        );
    }
}

/// The check of the signature is in the installer alone. A workflow that copies it, or that
/// downloads a release by hand, can drift from the rule that the installer holds. `release.yml`
/// signs the files and checks them, so it is not one of these workflows.
#[test]
fn no_workflow_copies_the_download_or_the_signature_check_of_a_release() {
    for file in INSTALLING_WORKFLOWS {
        let text = workflow_text(file);
        for copy in ["gh release download", "cosign verify-blob", "sha256sum"] {
            assert!(
                !text.contains(copy),
                "{file}: has its own copy of `{copy}`; run scripts/ci/install-strata.sh"
            );
        }
    }
}

/// `sha256sum` reads a name that starts with a dash as an option, and the shell check SC2035 asks
/// for `./*.sh` in place of `*.sh`. Each name that the release writes is safe today, so this keeps
/// the check clean. The sums then name a script as `./name`: the installer test of that form is
/// `install_strata_checks_the_sums_that_name_a_script_with_a_leading_dot_and_slash`.
#[test]
fn the_release_writes_the_sums_of_the_scripts_with_a_glob_that_starts_with_dot_and_slash() {
    let doc = workflow("release.yml");
    let run = steps(&doc)
        .filter_map(|step| step["run"].as_str())
        .find(|run| run.contains("> SHA256SUMS"))
        .expect("a step writes SHA256SUMS");
    let line = run.lines().find(|l| l.contains("> SHA256SUMS")).unwrap();
    assert!(line.contains(" ./*.sh"), "{line}");
    assert!(!line.contains(" *.sh"), "{line}");
}

/// The last job of the release workflow installs the tag that it published, as a consumer does.
/// It fails when the signature of `SHA256SUMS` does not verify, and when `cosign` is not there.
#[test]
fn the_release_check_installs_the_published_tag_with_a_required_signature() {
    let doc = workflow("release.yml");
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
// `act` is not in every container. So these tests run the `run` script of a step in bash, as the
// runner of GitHub does. Each tool that the step calls has a stand-in. The expressions of GitHub
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

/// Run the `run` script of the step `step` of the workflow `file` in a fresh directory.
///
/// Each variable of the `env` of the step is empty unless `env` gives it a value. `tools` are the
/// paths, under the directory, of the stand-ins that [`LOGGING_TOOL`] describes.
fn run_step(file: &str, step: &str, env: &[(&str, &str)], tools: &[&str]) -> StepRun {
    let doc = workflow(file);
    let step = steps(&doc)
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

/// The step runs `preflight.sh` and appends the report to the summary of the job. `shell: bash`
/// sets `pipefail`, so the pipe to `tee` leaves the exit status of the script as the status of the
/// step. A failed run keeps its report in the summary and its status.
#[test]
fn the_preflight_step_appends_the_report_to_the_summary_and_keeps_the_exit_status_of_a_failed_run()
{
    let doc = workflow("preflight.yml");
    let step = steps(&doc)
        .find(|s| s["name"].as_str() == Some("preflight"))
        .unwrap();
    assert_eq!(
        step["shell"].as_str(),
        Some("bash"),
        "without pipefail the pipe hides the status of the script"
    );

    for (exit, summary) in [("0", "# the report\n"), ("1", "# 1 check failed\n")] {
        let run = run_step(
            "preflight.yml",
            "preflight",
            &[("FAKE_OUT", summary), ("FAKE_EXIT", exit)],
            &[".strata/scripts/ci/preflight.sh"],
        );
        assert_eq!(run.output.status.code(), exit.parse::<i32>().ok());
        // the script takes no argument: the step gives it the inputs as variables
        assert_eq!(run.calls(), ["preflight.sh "]);
        assert_eq!(run.summary(), summary);
    }
}

#[test]
fn the_preflight_comment_step_posts_the_report_that_the_preflight_kept() {
    let run = run_step(
        "preflight-comment.yml",
        "comment on the pull request",
        &[("PR", "7"), ("REPO", "o/r")],
        &[".strata/scripts/ci/pr-comment.sh"],
    );
    assert!(run.output.status.success(), "{}", text(&run.output.stderr));
    assert_eq!(
        run.calls(),
        ["pr-comment.sh --preflight preflight/report.md --pr 7 --repo o/r"]
    );
}

/// A caller that grants `contents: read` alone calls `preflight.yml`, as it did before the
/// comment. GitHub fails a call from a caller that grants less than the workflow asks for. It
/// fails the call also when the step that needs the permission does not run. So only
/// `preflight-comment.yml` asks for `pull-requests: write`, and `preflight.yml` has no input that
/// posts a comment.
#[test]
fn the_preflight_workflow_asks_for_no_write_permission_and_posts_no_comment() {
    let preflight = workflow("preflight.yml");
    let permissions = preflight["permissions"]
        .as_mapping()
        .expect("preflight.yml sets its permissions");
    assert_eq!(
        permissions.len(),
        1,
        "preflight.yml asks for more than it did: {permissions:?}"
    );
    assert_eq!(permissions["contents"].as_str(), Some("read"));
    assert!(
        preflight["jobs"]["preflight"].get("permissions").is_none(),
        "the job of preflight.yml sets permissions of its own"
    );
    assert!(
        preflight["on"]["workflow_call"]["inputs"]
            .get("comment")
            .is_none(),
        "preflight.yml has an input `comment`: the comment belongs to preflight-comment.yml"
    );

    let comment = workflow("preflight-comment.yml");
    assert_eq!(
        comment["permissions"]["pull-requests"].as_str(),
        Some("write")
    );
}

/// The comment takes the report from the artifact of the preflight. So the workflow that keeps
/// the report and the workflow that reads it must name the same artifact and file.
#[test]
fn the_comment_workflow_reads_the_artifact_that_the_preflight_workflow_keeps() {
    let artifact = |file: &str, action: &str| -> serde_yaml::Value {
        let doc = workflow(file);
        let step = steps(&doc)
            .find(|s| s["uses"].as_str().is_some_and(|u| u.starts_with(action)))
            .unwrap_or_else(|| panic!("{file} has no step that uses {action}"));
        step["with"].clone()
    };
    let kept = artifact("preflight.yml", "actions/upload-artifact@");
    let read = artifact("preflight-comment.yml", "actions/download-artifact@");
    assert_eq!(kept["name"], read["name"]);

    let step = workflow("preflight-comment.yml")["jobs"]["comment"]["steps"]
        .as_sequence()
        .unwrap()
        .iter()
        .find_map(|s| s["run"].as_str().map(str::to_string))
        .expect("the comment workflow runs a script");
    let dir = read["path"].as_str().unwrap();
    assert!(
        step.contains(&format!("--preflight {dir}/report.md")),
        "the comment does not read report.md from the directory of the download: {step}"
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

/// Each workflow gives the installer the tag of `strata-version`, a directory that holds the
/// binaries beside each other, and `--require-signature`: a runner with no cosign fails the job
/// and does not install a binary that nothing signed.
#[test]
fn the_install_step_of_each_workflow_requires_the_signature_of_the_version_it_was_given() {
    for file in INSTALLING_WORKFLOWS {
        let doc = workflow(file);
        let name = steps(&doc)
            .find(|s| {
                s["run"]
                    .as_str()
                    .is_some_and(|r| r.contains("install-strata.sh"))
            })
            .and_then(|s| s["name"].as_str().map(str::to_string))
            .unwrap_or_else(|| panic!("{file} has no step that runs the installer"));
        let run = run_step(
            file,
            &name,
            &[("VERSION", "v1.2.3")],
            &[".strata/scripts/ci/install-strata.sh"],
        );
        assert!(
            run.output.status.success(),
            "{file}: {}",
            text(&run.output.stderr)
        );
        assert_eq!(
            run.calls(),
            ["install-strata.sh --version v1.2.3 --dir .strata/bin --bin-only --require-signature"],
            "{file}"
        );
    }
}
