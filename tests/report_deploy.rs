//! The deploy of the endpoint of the report: `deploy/report/`. No network and no Google project:
//! a `gcloud` on the `PATH` stands in, and writes down each call that it gets.

use std::path::Path;
use std::process::Command;

use crate::common::{Run, fake_bin, path_with, recipe, repo_file};

/// A `gcloud` that logs each call to `$FAKE_GCLOUD/calls`, and answers as the file `$FAKE_GCLOUD/*`
/// says: `no-secret` makes `secrets describe` fail, and `no-registry` makes the registry missing.
const FAKE_GCLOUD: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_GCLOUD/calls"
case "$*" in
  "secrets describe"*) [ ! -e "$FAKE_GCLOUD/no-secret" ] || exit 1 ;;
  "artifacts repositories describe"*) [ ! -e "$FAKE_GCLOUD/no-registry" ] || exit 1 ;;
  "logging sinks describe"*) [ ! -e "$FAKE_GCLOUD/has-exclusion" ] || echo "name: strata-report-requests" ;;
  "run services describe"*) echo "https://strata-report-abc-uc.a.run.app" ;;
esac
"#;

fn deploy(dir: &Path, args: &[&str]) -> Run {
    let bin = fake_bin(dir, &[("gcloud", FAKE_GCLOUD)]);
    let mut command = Command::new("bash");
    command
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("deploy/report/deploy.sh"))
        .args(args)
        .env("PATH", path_with(&bin))
        .env("FAKE_GCLOUD", dir)
        .env_remove("STRATA_REPORT_PROJECT")
        .env_remove("STRATA_REPORT_REGION");
    Run::of(&mut command)
}

fn calls(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("calls"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn a_dry_run_prints_each_gcloud_command_and_runs_none() {
    let dir = tempfile::tempdir().unwrap();
    let run = deploy(dir.path(), &["--dry-run"]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert_eq!(
        calls(dir.path()),
        Vec::<String>::new(),
        "a dry run calls no gcloud"
    );
    for part in [
        "gcloud builds submit --project como-strata-test --config deploy/report/cloudbuild.yaml",
        "gcloud run deploy strata-report --project como-strata-test --region us-central1",
        "--set-secrets GITHUB_TOKEN=strata-report-token:latest",
        "--max-instances 1",
        "--set-env-vars GITHUB_REPOSITORY=como-technologies/strata,TRUST_FORWARDED_FOR=1",
        "--allow-unauthenticated",
    ] {
        assert!(run.err.contains(part), "{part}:\n{}", run.err);
    }
}

#[test]
fn the_deploy_builds_the_image_deploys_it_and_prints_the_address_and_reads_no_secret() {
    let dir = tempfile::tempdir().unwrap();
    let run = deploy(
        dir.path(),
        &["--project", "lab-1", "--region", "europe-west1"],
    );
    assert_eq!(run.code, 0, "{}", run.err);
    let calls = calls(dir.path());
    let kinds: Vec<&str> = calls
        .iter()
        .map(|c| c.split(" --").next().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "secrets describe strata-report-token",
            "artifacts repositories describe strata",
            "builds submit",
            "run deploy strata-report",
            "logging sinks describe _Default",
            "logging sinks update _Default",
            "run services describe strata-report",
        ],
        "{calls:?}"
    );
    let image = "europe-west1-docker.pkg.dev/lab-1/strata/report-endpoint:";
    assert!(
        calls[2].contains(&format!("_IMAGE={image}")),
        "{}",
        calls[2]
    );
    assert!(
        calls[3].contains(&format!("--image {image}")),
        "{}",
        calls[3]
    );
    assert!(
        calls.iter().all(|c| c.contains("--project lab-1")
            || c.starts_with("builds")
            || c.starts_with("run"))
    );
    for call in &calls {
        // the script names the secret, and it never reads the value
        assert!(!call.contains("versions"), "{call}");
        assert!(!call.contains("access"), "{call}");
    }
    assert!(
        run.out
            .contains("strata-report is at https://strata-report-abc-uc.a.run.app"),
        "{}",
        run.out
    );
    assert!(
        run.out
            .contains("STRATA_REPORT_URL=https://strata-report-abc-uc.a.run.app")
    );
}

#[test]
fn a_missing_registry_is_made_and_a_missing_secret_stops_the_deploy_before_the_build() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("no-registry"), "").unwrap();
    let run = deploy(dir.path(), &[]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(
        calls(dir.path())
            .iter()
            .any(|c| c.starts_with("artifacts repositories create strata ")),
        "{:?}",
        calls(dir.path())
    );

    let missing = tempfile::tempdir().unwrap();
    std::fs::write(missing.path().join("no-secret"), "").unwrap();
    let run = deploy(missing.path(), &[]);
    assert_eq!(run.code, 2, "{}", run.err);
    assert!(
        run.err.contains(
            "the secret strata-report-token does not exist in the project como-strata-test"
        ),
        "{}",
        run.err
    );
    assert_eq!(
        calls(missing.path()),
        ["secrets describe strata-report-token --project como-strata-test"]
    );
}

#[test]
fn the_request_log_is_excluded_once_and_only_for_the_service() {
    let dir = tempfile::tempdir().unwrap();
    let run = deploy(dir.path(), &[]);
    assert_eq!(run.code, 0, "{}", run.err);
    let update = calls(dir.path())
        .into_iter()
        .find(|c| c.starts_with("logging sinks update"))
        .unwrap();
    assert!(
        update.contains("--add-exclusion name=strata-report-requests,"),
        "{update}"
    );
    assert!(
        update.contains(r#"resource.labels.service_name="strata-report""#),
        "{update}"
    );
    assert!(
        update.contains(r#"log_id("run.googleapis.com/requests")"#),
        "{update}"
    );

    // the exclusion is there, so the next deploy does not add it again
    let again = tempfile::tempdir().unwrap();
    std::fs::write(again.path().join("has-exclusion"), "").unwrap();
    assert_eq!(deploy(again.path(), &[]).code, 0);
    assert!(
        !calls(again.path())
            .iter()
            .any(|c| c.starts_with("logging sinks update"))
    );
}

#[test]
fn an_option_with_no_value_and_an_unknown_option_stop_the_script() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(deploy(dir.path(), &["--project"]).code, 2);
    let run = deploy(dir.path(), &["--bogus"]);
    assert_eq!(run.code, 2);
    assert!(run.err.contains("unknown option --bogus"), "{}", run.err);
    assert_eq!(calls(dir.path()), Vec::<String>::new());
}

#[test]
fn the_recipe_calls_the_script_and_the_image_holds_no_secret() {
    assert_eq!(
        recipe("report-deploy *args"),
        ["deploy/report/deploy.sh {{args}}"]
    );
    assert!(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("deploy/report/deploy.sh")
            .is_file()
    );
    let dockerfile = repo_file("deploy/report/Dockerfile");
    assert!(dockerfile.contains("cargo build --release --locked -p strata-report-endpoint"));
    assert!(dockerfile.contains("ENTRYPOINT [\"/strata-report-endpoint\"]"));
    let build = repo_file("deploy/report/cloudbuild.yaml");
    assert!(
        build.contains("\"-f\", \"deploy/report/Dockerfile\""),
        "{build}"
    );
    for file in ["Dockerfile", "cloudbuild.yaml", "deploy.sh"] {
        let text = repo_file(&format!("deploy/report/{file}"));
        for secret in ["ghp_", "github_pat_", "gho_"] {
            assert!(!text.contains(secret), "{file} holds {secret}");
        }
        assert!(
            !text
                .lines()
                .any(|l| l.trim_start().starts_with("ENV") && l.contains("GITHUB_TOKEN")),
            "{file} sets the token in the image"
        );
    }
}

#[test]
fn the_deploy_script_is_executable() {
    use std::os::unix::fs::PermissionsExt;
    let mode =
        std::fs::metadata(Path::new(env!("CARGO_MANIFEST_DIR")).join("deploy/report/deploy.sh"))
            .unwrap()
            .permissions()
            .mode();
    assert_eq!(mode & 0o111, 0o111, "deploy.sh is executable");
}
