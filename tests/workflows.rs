//! Every workflow file under .github/workflows parses as YAML, and has the two keys a workflow
//! needs. GitHub reads each file on each push and records a failed run, with no job, for a file
//! it cannot parse; the reusable workflows never run here, so nothing else would show it. A
//! colon and a space inside an unquoted description is the fault this found.

use std::path::Path;

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
/// package does not run them. So the job `lint and test` runs them by name.
#[test]
fn the_lint_and_test_job_runs_the_tests_of_xtask() {
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
    assert!(runs.contains(&"cargo test -p xtask"), "{runs:?}");
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
