//! The deploy of the endpoint of the report: `deploy/report/`. No network and no Google project:
//! a stand-in `gcloud` on the `PATH` records each call that it gets.

use std::path::Path;
use std::process::Command;

use crate::common::{Run, fake_bin, one_line, path_with, recipe, repo_file};

/// A `gcloud` that logs each call to `$FAKE_GCLOUD/calls`, and answers as the file `$FAKE_GCLOUD/*`
/// says. `no-secret` makes `secrets describe` fail, `no-registry` makes the registry missing, and
/// `no-service-account` or `no-build-account` makes that account missing.
const FAKE_GCLOUD: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_GCLOUD/calls"
case "$*" in
  "secrets describe"*) [ ! -e "$FAKE_GCLOUD/no-secret" ] || exit 1 ;;
  "iam service-accounts describe strata-report@"*) [ ! -e "$FAKE_GCLOUD/no-service-account" ] || exit 1 ;;
  "iam service-accounts describe strata-report-build@"*) [ ! -e "$FAKE_GCLOUD/no-build-account" ] || exit 1 ;;
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

/// The commands that `stderr` shows with the prefix `+ gcloud `, without the prefix.
fn printed(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .filter_map(|line| line.strip_prefix("+ gcloud "))
        .map(str::to_string)
        .collect()
}

/// The first of `commands` that starts with `prefix`.
fn command_with<'a>(commands: &'a [String], prefix: &str) -> &'a str {
    commands
        .iter()
        .find(|c| c.starts_with(prefix))
        .unwrap_or_else(|| panic!("no command starts with {prefix}: {commands:?}"))
}

/// The word that follows `option` in `command`.
fn option_value<'a>(command: &'a str, option: &str) -> Option<&'a str> {
    let mut words = command.split_whitespace();
    words.find(|word| *word == option)?;
    words.next()
}

/// The size in MiB of a memory value of Cloud Run, which has the unit `Mi` or `Gi`.
fn memory_mib(value: &str) -> u64 {
    let split = value
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(value.len());
    let (number, unit) = value.split_at(split);
    let number: u64 = number
        .parse()
        .unwrap_or_else(|_| panic!("no number in {value}"));
    match unit {
        "Mi" => number,
        "Gi" => number * 1024,
        _ => panic!("no unit Mi or Gi in {value}"),
    }
}

/// The `gcloud` commands in `text` that make an account, a repository, a bucket, or a role, each on
/// one line. `text` is a script or a page: the mark of a comment and the line continuations go.
fn account_commands(text: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut open: Option<String> = None;
    for line in text.lines() {
        let line = line.trim().trim_start_matches('#').trim();
        let command = match open.take() {
            Some(start) => format!("{start} {line}"),
            None if line.starts_with("gcloud ") => line.to_string(),
            None => continue,
        };
        match command.strip_suffix('\\') {
            Some(start) => open = Some(start.trim_end().to_string()),
            None => commands.push(command),
        }
    }
    commands.retain(|c| {
        [
            "service-accounts create",
            "repositories create",
            "buckets create",
            "add-iam-policy-binding",
        ]
        .iter()
        .any(|kind| c.contains(kind))
    });
    commands
}

/// `text` on one line, without the mark of a comment at the start of a line.
fn plain(text: &str) -> String {
    let lines: Vec<&str> = text
        .lines()
        .map(|line| line.trim_start().trim_start_matches('#'))
        .collect();
    one_line(&lines.join("\n"))
}

#[test]
fn the_script_prints_the_words_of_each_gcloud_command_that_it_sends_also_a_read() {
    let dir = tempfile::tempdir().unwrap();
    let run = deploy(dir.path(), &[]);
    assert_eq!(run.code, 0, "{}", run.err);
    let sent = calls(dir.path());
    assert_eq!(sent.len(), 9, "{sent:?}");
    assert_eq!(printed(&run.err), sent);
}

#[test]
fn the_dry_run_prints_each_command_that_a_real_run_sends_and_the_reads_too() {
    let real = tempfile::tempdir().unwrap();
    assert_eq!(deploy(real.path(), &[]).code, 0);
    let dry = tempfile::tempdir().unwrap();
    let run = deploy(dry.path(), &["--dry-run"]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert_eq!(printed(&run.err), calls(real.path()));
    assert_eq!(calls(dry.path()), Vec::<String>::new());
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
        "gcloud secrets describe strata-report-token --project como-strata-test",
        "gcloud iam service-accounts describe strata-report@como-strata-test.iam.gserviceaccount.com --project como-strata-test",
        "gcloud iam service-accounts describe strata-report-build@como-strata-test.iam.gserviceaccount.com --project como-strata-test",
        "gcloud artifacts repositories describe strata --project como-strata-test",
        "gcloud logging sinks describe _Default --project como-strata-test",
        "gcloud run services describe strata-report --project como-strata-test",
        "--no-cpu-throttling",
        "gcloud builds submit --project como-strata-test --config deploy/report/cloudbuild.yaml",
        "gcloud run deploy strata-report --project como-strata-test --region us-central1",
        "--set-secrets GITHUB_TOKEN=strata-report-token:latest",
        "--max-instances 1",
        "--set-env-vars TRUST_FORWARDED_FOR=1",
        "--allow-unauthenticated",
    ] {
        assert!(run.err.contains(part), "{part}:\n{}", run.err);
    }
}

#[test]
fn the_service_gets_at_least_512_mib_when_the_cpu_stays_on_after_the_answer() {
    // Cloud Run refuses a service with less than 512 MiB when the CPU is always allocated
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(deploy(dir.path(), &[]).code, 0);
    let sent = calls(dir.path());
    let service = command_with(&sent, "run deploy");
    assert!(service.contains(" --no-cpu-throttling"), "{service}");
    let memory = option_value(service, "--memory")
        .unwrap_or_else(|| panic!("the deploy sets no memory: {service}"));
    assert!(
        memory_mib(memory) >= 512,
        "{memory} is less than 512 MiB with --no-cpu-throttling: {service}"
    );
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
            "iam service-accounts describe strata-report@lab-1.iam.gserviceaccount.com",
            "iam service-accounts describe strata-report-build@lab-1.iam.gserviceaccount.com",
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
    let build = command_with(&calls, "builds submit");
    assert!(build.contains(&format!("_IMAGE={image}")), "{build}");
    let service = command_with(&calls, "run deploy");
    assert!(service.contains(&format!("--image {image}")), "{service}");
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
fn the_build_and_the_service_each_run_as_their_own_account_also_in_a_dry_run() {
    let real = tempfile::tempdir().unwrap();
    let args = ["--project", "lab-1"];
    assert_eq!(deploy(real.path(), &args).code, 0);
    let dry = tempfile::tempdir().unwrap();
    let run = deploy(dry.path(), &[args[0], args[1], "--dry-run"]);
    assert_eq!(run.code, 0, "{}", run.err);
    for sent in [calls(real.path()), printed(&run.err)] {
        let build = command_with(&sent, "builds submit");
        assert!(
            build.contains(" --service-account projects/lab-1/serviceAccounts/strata-report-build@lab-1.iam.gserviceaccount.com "),
            "{build}"
        );
        let service = command_with(&sent, "run deploy");
        assert!(
            service.contains(" --service-account strata-report@lab-1.iam.gserviceaccount.com "),
            "{service}"
        );
        // no call names the default compute account of the project
        for call in &sent {
            assert!(!call.contains("compute@developer"), "{call}");
        }
    }
}

#[test]
fn a_missing_account_stops_the_deploy_before_the_build_and_names_the_account_and_the_page() {
    for (marker, account) in [
        (
            "no-service-account",
            "strata-report@como-strata-test.iam.gserviceaccount.com",
        ),
        (
            "no-build-account",
            "strata-report-build@como-strata-test.iam.gserviceaccount.com",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(marker), "").unwrap();
        let run = deploy(dir.path(), &[]);
        assert_eq!(run.code, 2, "{marker}: {}", run.err);
        assert!(
            run.err.contains(&format!(
                "the service account {account} does not exist in the project como-strata-test"
            )),
            "{marker}: {}",
            run.err
        );
        assert!(
            run.err
                .contains("\"Make the two accounts\" in docs/src/development/report-endpoint.md"),
            "{marker}: {}",
            run.err
        );
        let sent = calls(dir.path());
        assert_eq!(
            sent.last().map(String::as_str),
            Some(
                format!("iam service-accounts describe {account} --project como-strata-test")
                    .as_str()
            ),
            "{marker}: {sent:?}"
        );
        assert!(
            sent.iter()
                .all(|c| !["artifacts", "builds", "run", "logging"]
                    .iter()
                    .any(|kind| c.starts_with(kind))),
            "{marker}: {sent:?}"
        );
    }
    // the page that the message names has the section
    assert!(
        repo_file("docs/src/development/report-endpoint.md")
            .lines()
            .any(|line| line == "### Make the two accounts")
    );
}

#[test]
fn the_script_makes_no_account_and_grants_no_role() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(deploy(dir.path(), &[]).code, 0);
    for call in calls(dir.path()) {
        assert!(!call.contains("add-iam-policy-binding"), "{call}");
        assert!(!call.contains("service-accounts create"), "{call}");
    }
    // the commands in the header are comments, and no line of code holds one
    let script = repo_file("deploy/report/deploy.sh");
    for line in script.lines().filter(|l| !l.trim_start().starts_with('#')) {
        assert!(!line.contains("add-iam-policy-binding"), "{line}");
        assert!(!line.contains("service-accounts create"), "{line}");
    }
}

#[test]
fn the_header_and_the_page_give_the_same_commands_for_the_accounts_and_the_roles() {
    let script = repo_file("deploy/report/deploy.sh");
    let page = repo_file("docs/src/development/report-endpoint.md");
    let header = account_commands(&script);
    assert_eq!(header, account_commands(&page));
    // two accounts, the repository, the bucket, and four roles
    assert_eq!(header.len(), 8, "{header:#?}");
    for role in [
        "roles/secretmanager.secretAccessor",
        "roles/storage.objectViewer",
        "roles/logging.logWriter",
        "roles/artifactregistry.writer",
    ] {
        let grants = header
            .iter()
            .filter(|c| c.contains(&format!("--role {role}")))
            .count();
        assert_eq!(grants, 1, "{role}: {header:#?}");
    }
    for command in &header {
        assert!(!command.contains("compute@developer"), "{command}");
    }
    // both texts say that the default compute account gets nothing
    for text in [plain(&script), plain(&page)] {
        assert!(
            text.contains("gets no role and no access to the secret"),
            "{text}"
        );
    }
}

#[test]
fn the_build_file_sets_the_timeout_and_sends_the_log_to_cloud_logging_only() {
    let build: serde_yaml::Value =
        serde_yaml::from_str(&repo_file("deploy/report/cloudbuild.yaml")).unwrap();
    assert_eq!(build["timeout"].as_str(), Some("2400s"));
    assert_eq!(
        build["options"]["logging"].as_str(),
        Some("CLOUD_LOGGING_ONLY")
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
