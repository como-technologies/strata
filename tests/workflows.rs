//! Every workflow file under .github/workflows parses as YAML, and has the two keys a workflow
//! needs. GitHub reads each file on each push and records a failed run, with no job, for a file
//! it cannot parse; the reusable workflows never run here, so nothing else shows it. A
//! colon and a space inside an unquoted description is the fault this found.

use std::process::Command;

use crate::common::{
    CommandExt, INSTALLING_WORKFLOWS, LOGGING_TOOL, MACHINE_GIT_USER, executable, git, isolate_git,
    recipe, repo_file, runs_of, script, steps, text, toml_value, workflow, workflow_files,
    workflow_text,
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
    let test = recipe("test");
    assert!(
        test.contains(&"cargo test -p xtask".to_string()),
        "{test:?}"
    );

    let runs = runs_of("checks.yml", "lint and test");
    assert!(runs.contains(&"just test".to_string()), "{runs:?}");
    assert!(
        !runs.contains(&"cargo test -p xtask".to_string()),
        "{runs:?}"
    );
}

/// The build of the public items does not read the comment of a private item. A code span that
/// breaks over a line in such a comment failed `cargo doc --document-private-items`, and no check
/// of CI saw it. So `just lint` builds the rustdoc of each package a second time with all items.
/// It keeps the build of the public items too, because only that build warns when a public item
/// links to a private item.
#[test]
fn just_lint_builds_the_rustdoc_of_each_package_with_the_public_items_and_with_all_items() {
    let lint = recipe("lint");
    let docs: Vec<&String> = lint
        .iter()
        .filter(|line| line.contains("cargo doc"))
        .collect();
    assert_eq!(docs.len(), 2, "{lint:?}");
    for line in &docs {
        assert!(line.contains(r#"RUSTDOCFLAGS="-D warnings""#), "{line}");
        let words: Vec<&str> = line.split_whitespace().collect();
        for package in ["strata", "strata-preflight", "xtask"] {
            assert!(
                words.windows(2).any(|pair| pair == ["-p", package]),
                "{line}"
            );
        }
    }
    let all_items = docs
        .iter()
        .filter(|line| line.contains("--document-private-items"))
        .count();
    assert_eq!(
        all_items, 1,
        "one build of all items, one of the public items: {lint:?}"
    );

    assert!(
        runs_of("checks.yml", "lint and test").contains(&"just lint".to_string()),
        "the job lint and test runs the recipe"
    );
}

/// mdbook exits with code 0 on a warning, so `mdbook build docs` alone passes a page that it
/// warns about. The recipe `docs` runs the script that reads the log, and the job `docs` runs the
/// recipe.
#[test]
fn just_docs_builds_the_book_with_the_script_that_fails_on_a_warning_and_the_docs_job_runs_it() {
    assert_eq!(recipe("docs"), ["scripts/dev/build-book.sh docs"]);
    assert!(
        runs_of("checks.yml", "docs").contains(&"just docs".to_string()),
        "the job docs runs the recipe"
    );
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

/// The log of Sigstore can refuse one try of a signature: it stopped v0.30.0 three times with a
/// 409. So the release signs through `scripts/ci/sign-blobs.sh`, which tries again, and no step
/// calls `cosign sign-blob` by itself. The step that checks each bundle stays.
#[test]
fn the_release_signs_each_file_through_the_script_that_tries_again_and_still_verifies_each_bundle()
{
    let doc = workflow("release.yml");
    let run_of = |name: &str| -> String {
        steps(&doc)
            .find(|s| s["name"].as_str() == Some(name))
            .unwrap_or_else(|| panic!("the release has no step named {name}"))["run"]
            .as_str()
            .unwrap_or_else(|| panic!("the step {name} has no run"))
            .to_string()
    };
    let signing = run_of("checksums and signatures");
    let sums = signing
        .find("> SHA256SUMS")
        .expect("the step writes SHA256SUMS");
    let sign = signing
        .find("../scripts/ci/sign-blobs.sh \"${files[@]}\"")
        .expect("the step signs each file through the script");
    assert!(sums < sign, "the sums come before the signatures");
    assert!(
        !workflow_text("release.yml").contains("cosign sign-blob"),
        "a step calls cosign sign-blob and does not try again"
    );
    assert!(run_of("verify what will be published").contains("cosign verify-blob"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(script("ci/sign-blobs.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o111,
            0o111,
            "the workflow runs the script, so it is executable"
        );
    }
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
/// paths, under the directory, of the stand-ins that [`LOGGING_TOOL`] describes. The step gets no
/// git setting of the machine, but `env` can give one.
fn run_step(file: &str, step: &str, env: &[(&str, &str)], tools: &[&str]) -> StepRun {
    let tools: Vec<(&str, &str)> = tools.iter().map(|tool| (*tool, LOGGING_TOOL)).collect();
    run_step_with(file, step, env, &tools)
}

/// [`run_step`] with a stand-in of your own for each tool: its path under the directory, and the
/// body of its script.
fn run_step_with(file: &str, step: &str, env: &[(&str, &str)], tools: &[(&str, &str)]) -> StepRun {
    let doc = workflow(file);
    let step = steps(&doc)
        .find(|s| s["name"].as_str() == Some(step))
        .unwrap_or_else(|| panic!("{file} has no step named {step}"));
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    for (tool, body) in tools {
        let at = dir.path().join(tool);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        executable(&at, body);
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
    isolate_git(&mut command).envs(env.iter().copied());
    let output = command.output_locked().unwrap();
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

/// The fuzz targets are tests of the package `strata-fuzz`, and `cargo test` of the root package
/// does not run them. So the `test` recipe runs them by name, and the job `lint and test` runs the
/// recipe. They run on stable Rust, on the corpus and on random bytes.
#[test]
fn the_test_recipe_runs_the_fuzz_targets_and_the_property_tests() {
    assert!(
        recipe("test").contains(&"cargo test -p strata-fuzz".to_string()),
        "{:?}",
        recipe("test")
    );
}

/// The run of the targets under libFuzzer needs a nightly toolchain and ten minutes for each
/// target. So no pull request and no push starts it. A schedule starts it, and so does a person.
/// The list of targets comes from the recipe, so a new target needs no change of the workflow. A
/// crash fails the job of its target, and the artifact keeps the files that the run added.
#[test]
fn the_fuzz_workflow_runs_each_target_on_nightly_on_a_schedule_and_keeps_the_new_files() {
    let doc = workflow("fuzz.yml");
    let on = doc["on"].as_mapping().expect("the workflow has triggers");
    assert!(
        on.contains_key("schedule") && on.contains_key("workflow_dispatch"),
        "{on:?}"
    );
    for trigger in ["push", "pull_request"] {
        assert!(
            !on.contains_key(trigger),
            "the fuzz workflow starts on {trigger}"
        );
    }

    let runs = |job: &str| runs_of("fuzz.yml", job);
    assert!(
        runs("targets").iter().any(|r| r.contains("just fuzz-list")),
        "the list of targets comes from `just fuzz-list`"
    );
    assert_eq!(
        doc["jobs"]["fuzz"]["strategy"]["matrix"]["target"].as_str(),
        Some("${{ fromJSON(needs.targets.outputs.targets) }}")
    );
    assert!(
        runs("fuzz")
            .iter()
            .any(|r| r.contains(r#"just fuzz "$TARGET" "$FUZZ_TIME""#)),
        "{:?}",
        runs("fuzz")
    );
    let uses: Vec<&str> = doc["jobs"]["fuzz"]["steps"]
        .as_sequence()
        .unwrap()
        .iter()
        .filter_map(|step| step["uses"].as_str())
        .collect();
    assert!(uses.contains(&"dtolnay/rust-toolchain@nightly"), "{uses:?}");
    let upload = doc["jobs"]["fuzz"]["steps"]
        .as_sequence()
        .unwrap()
        .iter()
        .find(|step| {
            step["uses"]
                .as_str()
                .is_some_and(|u| u.starts_with("actions/upload-artifact@"))
        })
        .expect("the job uploads the new files");
    assert_eq!(
        upload["if"].as_str(),
        Some("always()"),
        "a crash fails the job first"
    );
}

/// cargo-bolero and the crate bolero talk with a version of their protocol, and a pair that
/// differs in the minor version fails with "version mismatch". The workflow and the book name the
/// version of cargo-bolero, and it matches the minor version of the crate.
#[test]
fn the_version_of_cargo_bolero_matches_the_crate_and_the_book() {
    let manifest = repo_file("fuzz/Cargo.toml");
    let krate = toml_value(manifest.lines(), "bolero")
        .expect("fuzz/Cargo.toml names the version of bolero");
    let workflow = workflow_text("fuzz.yml");
    let tool = workflow
        .lines()
        .find_map(|line| line.trim().strip_prefix("CARGO_BOLERO_VERSION: "))
        .expect("fuzz.yml names the version of cargo-bolero");
    assert!(
        tool.starts_with(&format!("{krate}.")),
        "cargo-bolero {tool} does not match bolero {krate}"
    );
    let book = repo_file("docs/src/development/build-test.md");
    assert!(
        book.contains(&format!(
            "cargo install cargo-bolero --version {tool} --locked"
        )),
        "the book names another version of cargo-bolero than the workflow"
    );
}

/// The benchmarks run in CI with a short measure time (#214). A benchmark that does not build, or
/// that panics, fails the job. No step reads a time, so no time fails it. The recipe `test` does
/// not run them: they take minutes, and the job `lint and test` is the one every change waits for.
#[test]
fn the_benchmarks_job_runs_the_short_recipe_and_the_tests_do_not_run_the_benchmarks() {
    assert_eq!(runs_of("checks.yml", "benchmarks"), ["just bench-short"]);

    let short = recipe("bench-short");
    assert_eq!(short.len(), 1, "{short:?}");
    let words: Vec<&str> = short[0].split_whitespace().collect();
    assert_eq!(
        &words[..5],
        ["cargo", "bench", "-p", "strata-benches", "--"],
        "{short:?}"
    );
    for flag in ["--warm-up-time", "--measurement-time"] {
        assert!(
            words.contains(&flag),
            "{flag} keeps the run to minutes: {short:?}"
        );
    }

    // `just bench` hands its arguments to criterion, for a baseline or for one benchmark
    assert_eq!(
        recipe("bench *args"),
        ["cargo bench -p strata-benches -- {{args}}"]
    );
    for line in recipe("test").iter().chain(&recipe("lint")) {
        assert!(!line.contains("cargo bench"), "{line}");
    }
}

/// The members of the workspace, as the root `Cargo.toml` lists them: paths from the root.
fn workspace_members() -> Vec<String> {
    let root = repo_file("Cargo.toml");
    root.lines()
        .find_map(|l| l.strip_prefix("members = ["))
        .expect("the workspace lists its members")
        .trim_end_matches(']')
        .split(',')
        .map(|member| member.trim().trim_matches('"').to_string())
        .collect()
}

/// The floor of the Rust version is one line of the root `Cargo.toml`, and the job `the lowest
/// Rust version` reads it. Each member of the workspace takes it with `rust-version.workspace =
/// true`. A member with a floor of its own names a number that the job does not build with.
#[test]
fn each_member_of_the_workspace_takes_the_rust_version_that_the_msrv_job_reads() {
    let root = repo_file("Cargo.toml");
    let floors: Vec<&str> = root
        .lines()
        .filter(|l| l.starts_with("rust-version = "))
        .collect();
    assert_eq!(floors.len(), 1, "the msrv job reads one line: {floors:?}");
    let members = workspace_members();
    assert!(members.len() >= 6, "{members:?}");
    for member in members {
        let manifest = repo_file(&format!("{member}/Cargo.toml"));
        assert!(
            manifest
                .lines()
                .any(|l| l == "rust-version.workspace = true"),
            "{member} does not take the floor of the workspace"
        );
        assert!(
            member == "." || !manifest.lines().any(|l| l.starts_with("rust-version = ")),
            "{member} names a floor of its own"
        );
    }
    let msrv = runs_of("checks.yml", "the lowest Rust version");
    assert!(
        msrv.iter().any(|r| r.contains("^rust-version = ")),
        "the msrv job does not read the line of the root: {msrv:?}"
    );
}

/// The address of the repository is one line of the root `Cargo.toml`. `strata report --link` opens
/// an issue there, and the endpoint of the report writes its issues there. A member with an address
/// of its own could send the two kinds of report to two repositories.
#[test]
fn each_member_of_the_workspace_takes_the_repository_that_the_link_and_the_endpoint_use() {
    let root = repo_file("Cargo.toml");
    let addresses: Vec<&str> = root
        .lines()
        .filter_map(|l| l.strip_prefix("repository = "))
        .collect();
    assert_eq!(
        addresses.len(),
        1,
        "the workspace names one address: {addresses:?}"
    );
    assert_eq!(
        addresses[0].trim_matches('"'),
        env!("CARGO_PKG_REPOSITORY"),
        "the link of `strata report` opens an issue at another address"
    );
    for member in workspace_members() {
        let manifest = repo_file(&format!("{member}/Cargo.toml"));
        assert!(
            member == "." || !manifest.lines().any(|l| l.starts_with("repository = ")),
            "{member} names an address of its own"
        );
    }
    // the manifests that named the address before it was one line
    for member in [
        ".",
        "crates/strata-preflight",
        "fuzz",
        "crates/strata-report-endpoint",
    ] {
        let manifest = repo_file(&format!("{member}/Cargo.toml"));
        assert!(
            manifest.lines().any(|l| l == "repository.workspace = true"),
            "{member} does not take the address of the workspace"
        );
    }
}

/// The observer is a Go module with no `require` line, so it builds with the standard library
/// alone. The last line of the recipe `lint` makes sure of this. The test runs that line in a
/// directory with a `go.mod` of each kind, so a line that no longer fails on a module fails here.
#[test]
fn just_lint_fails_when_a_line_of_the_go_module_of_the_observer_starts_with_require() {
    let lint = recipe("lint");
    let check = lint
        .iter()
        .find(|line| line.contains("grep") && line.contains("observer/go.mod"))
        .unwrap_or_else(|| panic!("the recipe lint reads observer/go.mod: {lint:?}"));
    let exit_code = |go_mod: &str| {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("observer")).unwrap();
        std::fs::write(dir.path().join("observer/go.mod"), go_mod).unwrap();
        Command::new("bash")
            .args(["--noprofile", "--norc", "-c", check])
            .current_dir(dir.path())
            .output_locked()
            .unwrap()
            .status
            .code()
    };

    let module = repo_file("observer/go.mod");
    assert_eq!(exit_code(&module), Some(0), "the module of the repository");
    let with_require = format!("{module}\nrequire example.com/x v1.0.0\n");
    assert_eq!(exit_code(&with_require), Some(1), "a module with a require");
    let with_block = format!("{module}\nrequire (\n\texample.com/x v1.0.0\n)\n");
    assert_eq!(exit_code(&with_block), Some(1), "a module with a block");
}

/// The recipe `lint` also formats and vets the Go code, and the recipe `test` runs its tests. The
/// job `lint and test` runs the two recipes, and has no Go step of its own for them.
#[test]
fn the_lint_and_test_recipes_check_the_go_code_of_the_observer() {
    let lint = recipe("lint");
    for line in [
        r#"test -z "$(gofmt -l observer)""#,
        "go -C observer vet ./...",
    ] {
        assert!(lint.contains(&line.to_string()), "{line}: {lint:?}");
    }
    let test = recipe("test");
    assert!(
        test.contains(&"go -C observer test ./...".to_string()),
        "{test:?}"
    );

    let runs = runs_of("checks.yml", "lint and test");
    assert!(runs.iter().all(|run| !run.contains("go ")), "{runs:?}");
}

/// The runner of GitHub has Go, but not the version of the module. The job `check` installs the
/// version that `observer/go.mod` names, before the first recipe that runs Go, so the version of
/// the module is the one place that names it. The cache is off: the module has no dependency.
#[test]
fn the_check_job_installs_go_from_the_module_of_the_observer_before_it_runs_the_recipes() {
    let doc = workflow("checks.yml");
    let job = doc["jobs"]["check"]["steps"]
        .as_sequence()
        .expect("the job check has steps");
    let position = |matches: &dyn Fn(&serde_yaml::Value) -> bool| job.iter().position(matches);

    let setup = position(&|step| {
        step["uses"]
            .as_str()
            .is_some_and(|uses| uses.starts_with("actions/setup-go@"))
    })
    .expect("the job check installs Go");
    assert_eq!(
        job[setup]["with"]["go-version-file"].as_str(),
        Some("observer/go.mod")
    );
    assert_eq!(job[setup]["with"]["cache"].as_bool(), Some(false));

    for recipe in ["just lint", "just test"] {
        let run = position(&|step| step["run"].as_str() == Some(recipe))
            .unwrap_or_else(|| panic!("the job check runs {recipe}"));
        assert!(
            setup < run,
            "the step that runs {recipe} comes before the install of Go"
        );
    }
}

/// The observer is one static binary at `target/observer/strata-observer`. `CGO_ENABLED=0` makes it
/// static, and the version comes out of the first `version` line of the root `Cargo.toml`, so the
/// binary and the crate cannot name two versions.
#[test]
fn the_observer_build_writes_a_static_binary_that_names_the_version_of_cargo_toml() {
    let build = recipe("observer-build");
    assert_eq!(build.len(), 1, "{build:?}");
    let line = &build[0];
    for part in [
        "CGO_ENABLED=0 go -C observer build",
        "-trimpath",
        r#"-ldflags "-X main.version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)""#,
        "-o ../target/observer/strata-observer .",
    ] {
        assert!(line.contains(part), "{part}: {line}");
    }
    assert!(
        repo_file("Cargo.toml")
            .lines()
            .any(|l| l.starts_with("version = \"")),
        "Cargo.toml has the line that the recipe reads"
    );
}

/// The contract test of the observer needs the strata binary and the stand-in of this commit. The
/// recipe `observer-contract` builds both and runs the script, and gives it its arguments, so
/// `--write` reaches it. The justfile names the recipe with `*args`, and the helper `recipe` does
/// not read such a name.
#[test]
fn the_observer_contract_recipe_builds_strata_and_the_stand_in_and_runs_the_script() {
    let justfile = repo_file("justfile");
    let mut lines = justfile
        .lines()
        .skip_while(|line| !line.starts_with("observer-contract "));
    assert_eq!(
        lines.next(),
        Some("observer-contract *args: build stand-in"),
        "the recipe names its arguments and its two dependencies"
    );
    assert_eq!(
        lines.next().map(str::trim),
        Some("scripts/observer-contract.sh {{args}}")
    );
    assert!(
        lines.next().is_none_or(|line| !line.starts_with(' ')),
        "the recipe has one command"
    );

    let script = Command::new("bash")
        .args(["-n", "scripts/observer-contract.sh"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output_locked()
        .unwrap();
    assert!(
        script.status.success(),
        "{}",
        String::from_utf8_lossy(&script.stderr)
    );
}

/// The job `check` runs the contract test of the observer after `just examples`, because the
/// stand-in reads the examples that the recipe `examples` builds. The job has no step of its own
/// for Go: the recipe starts it.
#[test]
fn the_check_job_runs_the_contract_of_the_observer_after_the_examples() {
    let runs = runs_of("checks.yml", "lint and test");
    let at = |recipe: &str| {
        runs.iter()
            .position(|run| run == recipe)
            .unwrap_or_else(|| panic!("the job check runs {recipe}: {runs:?}"))
    };
    assert!(
        at("just examples") < at("just observer-contract"),
        "{runs:?}"
    );
}

/// The runner of GitHub has `kubectl`, so the job `check` renders the manifests of the observer: the
/// base, and the base with each component. A layout that kustomize refuses, such as a kustomization
/// inside its own base, then fails CI and not a live run.
#[test]
fn the_check_job_renders_the_manifests_of_the_observer_with_and_without_each_component() {
    let runs = runs_of("checks.yml", "lint and test");
    for recipe in [
        "just observer-manifests",
        "just observer-manifests-pods",
        "just observer-manifests-alerts",
    ] {
        assert!(runs.contains(&recipe.to_string()), "{recipe}: {runs:?}");
    }
}

// ---- the demo of the observer in the book (#645) ---------------------------------------------------

/// A step of the book runs `strata-observer`, so the recipes that write and check the output build
/// it first. The recipe `observer-playground` builds both binaries and runs the script with the
/// chapter. The justfile names each recipe with an argument or a dependency, which the helper
/// `recipe` does not read.
#[test]
fn the_book_recipes_build_the_observer_and_the_playground_recipe_runs_its_script() {
    let justfile = repo_file("justfile");
    let line = |start: &str| {
        justfile
            .lines()
            .find(|l| l.starts_with(start))
            .unwrap_or_else(|| panic!("the justfile has no recipe {start}"))
            .to_string()
    };
    assert_eq!(
        line("docs-examples:"),
        "docs-examples: examples stand-in observer-build"
    );
    assert_eq!(
        line("docs-check:"),
        "docs-check: examples stand-in observer-build"
    );
    assert_eq!(
        line("observer-playground "),
        r#"observer-playground chapter="observer-2": build observer-build"#
    );
    let mut lines = justfile
        .lines()
        .skip_while(|l| !l.starts_with("observer-playground "))
        .skip(1);
    assert_eq!(
        lines.next().map(str::trim),
        Some("scripts/observer-playground.sh {{chapter}}")
    );
    assert!(
        lines.next().is_none_or(|l| !l.starts_with(' ')),
        "the recipe has one command"
    );
}

/// The job `docs` runs the steps of the book, and a step runs the observer, which the recipe builds
/// with Go. The job installs the version of the module, before the first recipe that builds.
#[test]
fn the_docs_job_installs_go_from_the_module_of_the_observer_before_it_checks_the_book() {
    let doc = workflow("checks.yml");
    let steps = doc["jobs"]["docs"]["steps"]
        .as_sequence()
        .expect("the job docs has steps");
    let setup = steps
        .iter()
        .position(|s| {
            s["uses"]
                .as_str()
                .is_some_and(|u| u.starts_with("actions/setup-go@"))
        })
        .expect("the job docs installs Go");
    assert_eq!(
        steps[setup]["with"]["go-version-file"].as_str(),
        Some("observer/go.mod")
    );
    assert_eq!(steps[setup]["with"]["cache"].as_bool(), Some(false));
    let check = steps
        .iter()
        .position(|s| s["run"].as_str() == Some("just docs-check"))
        .expect("the job docs runs just docs-check");
    assert!(setup < check, "Go comes before the check of the output");
}

/// The contract script and the playground script keep one copy of four functions: they start the
/// stand-in, stop it, write the records, and find the uid of a cluster. Both source
/// `observer-lib.sh`, and no script defines them again or reads the list of the tenant by itself.
#[test]
fn the_scripts_of_the_observer_keep_one_copy_of_each_function_of_the_library() {
    let functions = ["start()", "stop()", "records_dir()", "cluster_uid()"];
    let scripts = ["observer-contract.sh", "observer-playground.sh"];
    for name in scripts {
        let text = repo_file(&format!("scripts/{name}"));
        assert!(
            text.contains(r#"source "$root/scripts/observer-lib.sh""#),
            "{name} sources the library"
        );
        for function in functions {
            assert!(!text.contains(function), "{name} defines {function} again");
        }
        assert!(
            !text.contains(r#""$strata" list"#),
            "{name} reads the list of the tenant, and cluster_uid does it"
        );
        let syntax = Command::new("bash")
            .args(["-n", &format!("scripts/{name}")])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output_locked()
            .unwrap();
        assert!(
            syntax.status.success(),
            "{}",
            String::from_utf8_lossy(&syntax.stderr)
        );
    }
    let lib = repo_file("scripts/observer-lib.sh");
    for function in functions {
        assert_eq!(lib.matches(function).count(), 1, "{function} once");
    }
}

// ---- the image of the observer -------------------------------------------------------------------

/// The rank of a permission level of GitHub.
fn permission_rank(level: &str) -> u8 {
    match level {
        "none" => 0,
        "read" => 1,
        "write" => 2,
        other => panic!("{other} is no level of permission"),
    }
}

/// The permissions that the workflow `doc` asks for: the ones of the workflow, and the ones of each
/// job, as pairs of a permission and its level.
fn permissions_asked(doc: &serde_yaml::Value) -> Vec<(String, String)> {
    let mut asked = Vec::new();
    let jobs = doc["jobs"].as_mapping().expect("a workflow has jobs");
    for scope in
        std::iter::once(&doc["permissions"]).chain(jobs.values().map(|j| &j["permissions"]))
    {
        for (name, level) in scope.as_mapping().into_iter().flatten() {
            asked.push((
                name.as_str().unwrap().to_string(),
                level.as_str().unwrap().to_string(),
            ));
        }
    }
    asked
}

/// A called workflow cannot have more permission than the job that calls it. When the job grants
/// less, GitHub refuses the whole run of the release at its start, and the next release does not
/// ship. The workflow of the release has fewer permissions than the image needs, so the job sets
/// its own and does not take the default of the workflow.
#[test]
fn the_observer_image_job_of_the_release_grants_what_the_called_workflow_needs() {
    let release = workflow("release.yml");
    let job = &release["jobs"]["observer-image"];
    assert_eq!(job["needs"].as_str(), Some("release"));
    assert_eq!(
        job["uses"].as_str(),
        Some("./.github/workflows/observer-image.yml")
    );
    let granted = job["permissions"]
        .as_mapping()
        .expect("the job sets its permissions");
    for (permission, level) in [
        ("contents", "read"),
        ("packages", "write"),
        ("id-token", "write"),
    ] {
        assert_eq!(
            granted.get(permission).and_then(|l| l.as_str()),
            Some(level),
            "the job grants {permission}: {level}"
        );
    }
    let level_of = |permission: &str| {
        granted
            .get(permission)
            .and_then(|l| l.as_str())
            .map_or(0, permission_rank)
    };
    let asked = permissions_asked(&workflow("observer-image.yml"));
    assert!(
        !asked.is_empty(),
        "the called workflow asks for permissions"
    );
    for (permission, level) in asked {
        assert!(
            level_of(&permission) >= permission_rank(&level),
            "the called workflow asks for {permission}: {level}, and the job grants less"
        );
    }
    // the release gives the tag that it published as the release of the binaries and as the tag
    assert_eq!(
        job["with"]["version"].as_str(),
        Some("${{ github.ref_name }}")
    );
    assert_eq!(job["with"]["tag"].as_str(), Some("${{ github.ref_name }}"));
}

/// The release calls the workflow, a person starts it from the default branch, and a pull request
/// that changes the observer starts it with a test tag. A push builds no image.
#[test]
fn the_observer_image_workflow_runs_on_a_call_a_dispatch_and_a_pull_request_that_changes_the_observer()
 {
    let doc = workflow("observer-image.yml");
    let on = &doc["on"];
    for trigger in ["workflow_call", "workflow_dispatch"] {
        for input in ["version", "tag"] {
            assert!(
                on[trigger]["inputs"][input].is_mapping(),
                "{trigger} takes the input {input}"
            );
        }
    }
    assert_eq!(
        on["workflow_dispatch"]["inputs"]["version"]["default"],
        "latest"
    );
    assert_eq!(on["workflow_call"]["inputs"]["version"]["required"], true);
    let paths: Vec<&str> = on["pull_request"]["paths"]
        .as_sequence()
        .expect("the pull request has paths")
        .iter()
        .filter_map(|p| p.as_str())
        .collect();
    assert_eq!(
        paths,
        [
            "observer/**",
            "deploy/observer/**",
            ".github/workflows/observer-image.yml"
        ]
    );
    for trigger in ["push", "schedule"] {
        assert!(on.get(trigger).is_none(), "{trigger} builds no image");
    }
}

/// The steps of the job `image`, in order: the installer gets the signed binaries of both targets,
/// buildx builds both platforms and pushes, and cosign signs the digest and then verifies the
/// signature. The installer holds the rule of the signature, so the workflow has no copy of it.
#[test]
fn the_observer_image_installs_the_signed_binaries_builds_two_platforms_then_signs_and_verifies() {
    let doc = workflow("observer-image.yml");
    let steps: Vec<&serde_yaml::Value> = steps(&doc).collect();
    let position = |what: &str, wanted: &dyn Fn(&serde_yaml::Value) -> bool| {
        steps
            .iter()
            .copied()
            .position(wanted)
            .unwrap_or_else(|| panic!("a step is missing: {what}"))
    };
    let uses = |prefix: &'static str| {
        move |s: &serde_yaml::Value| s["uses"].as_str().is_some_and(|u| u.starts_with(prefix))
    };
    let runs = |part: &'static str| {
        move |s: &serde_yaml::Value| s["run"].as_str().is_some_and(|r| r.contains(part))
    };
    let cosign = position("cosign", &uses("sigstore/cosign-installer@"));
    let install = position("the installer", &runs("scripts/ci/install-strata.sh"));
    let build = position("the build", &uses("docker/build-push-action@"));
    let sign = position("the signature", &runs("cosign sign --yes"));
    let verify = position("the check of the signature", &runs("cosign verify "));
    assert!(
        cosign < install && install < build && build < sign && sign < verify,
        "cosign, the installer, the build, the signature, and its check come in that order"
    );

    let run = steps[install]["run"].as_str().unwrap();
    for part in [
        "x86_64-unknown-linux-musl",
        "aarch64-unknown-linux-musl",
        "--bin-only",
        "--require-signature",
    ] {
        assert!(run.contains(part), "the installer gets {part}: {run}");
    }
    let with = &steps[build]["with"];
    assert_eq!(with["platforms"], "linux/amd64,linux/arm64");
    assert_eq!(with["push"], true);
    assert_eq!(with["file"], "deploy/observer/Dockerfile");
    assert_eq!(with["context"], ".");

    // the signature covers the digest that the build gave, not a tag that can move
    for step in [sign, verify] {
        let run = steps[step]["run"].as_str().unwrap();
        assert!(run.contains(r#""$IMAGE@$DIGEST""#), "{run}");
        assert_eq!(
            steps[step]["env"]["DIGEST"].as_str(),
            Some("${{ steps.build.outputs.digest }}")
        );
    }
    let verify_run = steps[verify]["run"].as_str().unwrap();
    assert!(
        verify_run.contains("observer-image\\.yml@")
            && verify_run.contains("https://token.actions.githubusercontent.com"),
        "the check names the workflow that signs: {verify_run}"
    );

    let text = workflow_text("observer-image.yml");
    for copy in ["gh release download", "cosign verify-blob", "sha256sum"] {
        assert!(
            !text.contains(copy),
            "the workflow has its own copy of `{copy}`; run scripts/ci/install-strata.sh"
        );
    }
}

/// The tag of the image is the tag that the caller gives. A pull request and a manual run with no
/// tag get `test-` and the first seven characters of the commit. The name of the image is in
/// lower case, as the registry needs.
#[test]
fn the_observer_image_step_names_the_image_from_the_repository_and_the_tag_of_the_call() {
    let named = |tag: &str| {
        let run = run_step(
            "observer-image.yml",
            "name the image",
            &[
                ("TAG", tag),
                ("HEAD_SHA", "0123456789abcdef"),
                ("GITHUB_REPOSITORY", "Como-Technologies/Strata"),
                ("GITHUB_ENV", "env.txt"),
            ],
            &[],
        );
        assert!(run.output.status.success(), "{}", text(&run.output.stderr));
        std::fs::read_to_string(run.dir.path().join("env.txt")).unwrap()
    };
    assert_eq!(
        named(""),
        "IMAGE=ghcr.io/como-technologies/strata-observer\nIMAGE_TAG=test-0123456\n"
    );
    assert_eq!(
        named("v0.29.0"),
        "IMAGE=ghcr.io/como-technologies/strata-observer\nIMAGE_TAG=v0.29.0\n"
    );
}

/// The step gets the binary of each platform from the installer, and puts it where the Dockerfile
/// copies it from. It asks for the release that the caller names, and for `latest` when the pull
/// request gives none.
#[test]
fn the_observer_image_step_puts_the_binary_of_each_platform_where_the_dockerfile_copies_it() {
    // an installer that puts a binary in the directory of --dir, as the real one does
    let installer = r#"#!/usr/bin/env bash
echo "install-strata.sh $*" >> "$FAKE_LOG"
dir=""; target=""; prev=""
for a in "$@"; do
  [ "$prev" = --dir ] && dir=$a
  [ "$prev" = --target ] && target=$a
  prev=$a
done
mkdir -p "$dir"
echo "binary of $target" > "$dir/strata"
"#;
    for (release, asked) in [("v0.29.0", "v0.29.0"), ("", "latest")] {
        let run = run_step_with(
            "observer-image.yml",
            "install the binaries of the release",
            &[("RELEASE", asked)],
            &[("scripts/ci/install-strata.sh", installer)],
        );
        assert!(run.output.status.success(), "{}", text(&run.output.stderr));
        let calls = run.calls();
        assert_eq!(calls.len(), 2, "{calls:?}");
        for call in &calls {
            assert!(
                call.contains(&format!("--version {asked} "))
                    && call.contains("--bin-only")
                    && call.contains("--require-signature"),
                "{release}: {call}"
            );
        }
        for (arch, target) in [
            ("amd64", "x86_64-unknown-linux-musl"),
            ("arm64", "aarch64-unknown-linux-musl"),
        ] {
            let copied =
                std::fs::read_to_string(run.dir.path().join(format!("dist/strata-{arch}")))
                    .unwrap();
            assert_eq!(copied, format!("binary of {target}\n"));
        }
    }
}

/// The step reads the version out of `strata --version`, so the observer names the version of the
/// binary that it carries, also when the caller asks for `latest`.
#[test]
fn the_observer_image_step_reads_the_version_of_the_binary_it_carries() {
    let run = run_step_with(
        "observer-image.yml",
        "read the version of the binary",
        &[("GITHUB_OUTPUT", "output.txt")],
        &[(
            "dist/strata-amd64",
            "#!/usr/bin/env bash\necho strata 0.29.1\n",
        )],
    );
    assert!(run.output.status.success(), "{}", text(&run.output.stderr));
    assert_eq!(
        std::fs::read_to_string(run.dir.path().join("output.txt")).unwrap(),
        "number=0.29.1\n"
    );
}

/// A stand-in that logs its name, its arguments, the kubeconfig that it reads with the mode of
/// that file, and what it got on standard input.
const KUBE_TOOL: &str = r#"#!/usr/bin/env bash
in=""
[ -t 0 ] || in=$(cat)
echo "$(basename "$0") $* | $(cat "$KUBECONFIG") $(stat -c %a "$KUBECONFIG") | $in" >> "$FAKE_LOG"
"#;

/// The step writes the secret kubeconfig to a file that only the job reads, expands each pattern
/// of the records, and gives the output of `observer-records.sh` to `kubectl replace`. A pattern
/// that finds no file stops the step before kubectl runs.
#[test]
fn the_observer_publish_step_replaces_the_secrets_with_the_records_of_each_pattern() {
    let records =
        "#!/usr/bin/env bash\necho \"observer-records.sh $*\" >> \"$FAKE_LOG\"\necho the-secrets\n";
    let temp = tempfile::tempdir().unwrap();
    let temp_path = temp.path().display().to_string();
    let path = format!("bin:{}", std::env::var("PATH").unwrap());
    let tools = [
        (".strata/scripts/ci/observer-records.sh", records),
        ("bin/kubectl", KUBE_TOOL),
        ("releases/observer/a.json", "{}"),
        ("releases/observer/b.json", "{}"),
        ("releases/team/c.json", "{}"),
    ];
    let env = [
        ("RECORDS", "releases/observer/*.json\nreleases/**/c.json"),
        ("NAMESPACE", "team-a"),
        ("REVISION", "abc123"),
        ("KUBECONFIG_TEXT", "the-kubeconfig"),
        ("RUNNER_TEMP", temp_path.as_str()),
        ("PATH", path.as_str()),
    ];
    let run = run_step_with("observer-publish.yml", "publish the records", &env, &tools);
    assert!(run.output.status.success(), "{}", text(&run.output.stderr));
    assert_eq!(
        run.calls(),
        [
            "observer-records.sh --revision abc123 --namespace team-a releases/observer/a.json releases/observer/b.json releases/team/c.json",
            "kubectl replace -f - | the-kubeconfig 600 | the-secrets",
        ]
    );

    let mut none = env.to_vec();
    none[0] = ("RECORDS", "releases/observer/*.json releases/none/*.json");
    let run = run_step_with("observer-publish.yml", "publish the records", &none, &tools);
    assert_eq!(run.output.status.code(), Some(2));
    assert!(
        text(&run.output.stderr).contains("no record matches releases/none/*.json"),
        "{}",
        text(&run.output.stderr)
    );
    assert!(run.calls().is_empty(), "{:#?}", run.calls());
}

/// Run the step of `observer-propose.yml` that proposes the records, with its inputs and `more`.
fn propose_the_records(more: &[(&str, &str)]) -> StepRun {
    let temp = tempfile::tempdir().unwrap();
    let temp_path = temp.path().display().to_string();
    let mut env = vec![
        ("DIR", "releases/observer"),
        ("NAMESPACE", "strata-observer"),
        ("REPO", "o/r"),
        ("KUBECONFIG_TEXT", "the-kubeconfig"),
        ("RUNNER_TEMP", temp_path.as_str()),
    ];
    env.extend_from_slice(more);
    let tools = [(".strata/scripts/ci/observer-propose.sh", KUBE_TOOL)];
    let run = run_step_with("observer-propose.yml", "propose the records", &env, &tools);
    assert!(run.output.status.success(), "{}", text(&run.output.stderr));
    run
}

/// The step gives `--base` only when the input names a branch.
#[test]
fn the_observer_propose_step_gives_the_base_only_when_the_input_names_one() {
    assert_eq!(
        propose_the_records(&[]).calls(),
        [
            "observer-propose.sh --dir releases/observer --namespace strata-observer --repo o/r | the-kubeconfig 600 | "
        ]
    );
    assert_eq!(
        propose_the_records(&[("BASE", "dev")]).calls(),
        [
            "observer-propose.sh --dir releases/observer --namespace strata-observer --repo o/r --base dev | the-kubeconfig 600 | "
        ]
    );
}

/// The step writes the name of the author of the commit into the config of the repository. A git
/// user in the environment of the machine wins over that config. So the test reads the name with
/// [`isolate_git`], which removes that user.
#[test]
fn the_observer_propose_step_sets_the_author_also_when_the_machine_gives_git_another_user() {
    for machine in MACHINE_GIT_USER {
        let run = propose_the_records(machine);
        let mut read = Command::new("git");
        read.args(["config", "user.name"])
            .current_dir(run.dir.path())
            .envs(machine.iter().copied());
        let name = isolate_git(&mut read).output_locked().unwrap();
        assert_eq!(
            text(&name.stdout).trim(),
            "github-actions[bot]",
            "{machine:?}"
        );
    }
}
