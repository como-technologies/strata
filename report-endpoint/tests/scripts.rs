//! The scripts and the settings of the service keep one text for each label, repository, and
//! milestone that they share. A script is shell, so it holds its own copy of the text. These
//! tests make sure that the copy is the one that the service reads.

use strata_report_endpoint::settings::{DEFAULT_LABEL, DEFAULT_MILESTONE, Settings};

mod common;

use common::{assigned, contract_label, read};

/// The lines of `script` that are not comments.
fn code(script: &str) -> String {
    script
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_live_send_reads_the_issues_of_the_label_that_the_service_writes() {
    let script = read("../examples/live/report-send.sh");
    assert_eq!(assigned(&script, "label"), DEFAULT_LABEL);
}

#[test]
fn the_contract_run_has_a_label_of_its_own_and_the_milestone_of_the_service() {
    assert_ne!(
        contract_label(),
        DEFAULT_LABEL,
        "a shared label shares the limit of 1000 issues that the service reads"
    );
    let script = read("../examples/live/contract-report.sh");
    assert_eq!(assigned(&script, "milestone"), DEFAULT_MILESTONE);
}

#[test]
fn the_repository_has_one_text_in_the_manifest_and_the_deploy_and_the_live_send_do_not_copy_it() {
    let manifest = read("Cargo.toml");
    let url = assigned(&manifest, "repository");
    let name = url.strip_prefix("https://github.com/").unwrap();
    let settings = Settings::from_lookup(|var| (var == "GITHUB_TOKEN").then(|| "t".to_string()));
    assert_eq!(settings.unwrap().repository, name);
    let deploy = code(&read("../deploy/report/deploy.sh"));
    assert!(!deploy.contains(name), "deploy.sh names the repository");
    assert!(
        !deploy.contains("GITHUB_REPOSITORY"),
        "deploy.sh sets the repository"
    );
    let send = code(&read("../examples/live/report-send.sh"));
    assert!(!send.contains(name), "report-send.sh names the repository");
    assert!(send.contains("report-endpoint/Cargo.toml"));
}
