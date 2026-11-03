//! The scripts and the settings of the service keep one text for each label, repository, marker, and
//! milestone that they share. A script is shell, so it holds its own copy of the text. These
//! tests make sure that the copy is the one that the service reads or writes.

use serde_json::json;
use strata::closed_set::unknown::report::{Report, SCHEMA};
use strata_report_endpoint::merge::merge;
use strata_report_endpoint::settings::{DEFAULT_LABEL, DEFAULT_MILESTONE, Settings};
use strata_report_endpoint::tracker::{Issue, Tracker, TrackerError};

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
fn the_repository_has_one_text_in_the_workspace_and_the_deploy_and_the_live_send_do_not_copy_it() {
    let manifest = read("../Cargo.toml");
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
    assert!(
        send.contains("\"$live/../../Cargo.toml\""),
        "report-send.sh reads the manifest of the workspace"
    );
}

/// A tracker with no issue, which keeps the body of each issue that the merge opens.
#[derive(Default)]
struct Opened(Vec<String>);

impl Tracker for Opened {
    fn issues(&mut self) -> Result<Vec<Issue>, TrackerError> {
        Ok(Vec::new())
    }

    fn open(&mut self, _title: &str, body: &str) -> Result<u64, TrackerError> {
        self.0.push(body.to_string());
        Ok(self.0.len() as u64)
    }

    fn update(&mut self, _number: u64, _body: &str) -> Result<(), TrackerError> {
        Ok(())
    }
}

#[test]
fn the_live_send_reads_the_first_line_and_the_row_of_lines_that_the_service_writes() {
    let script = read("../examples/live/report-send.sh");
    let (marker, row) = (assigned(&script, "marker"), assigned(&script, "lines_row"));
    // the made-up value of the chapter unknown-value of the book, which the script sends
    let report: Report = serde_json::from_value(json!({"schema": SCHEMA, "unknown": [
        {"strata": "0.27.0", "set": "ProfileType", "value": "later-type",
         "route": "/v1/clusterprofiles/{uid}", "count": 1}]}))
    .unwrap();
    let mut tracker = Opened::default();
    merge(&mut tracker, &report).unwrap();
    let body = &tracker.0[0];
    assert_eq!(body.lines().next(), Some(marker.as_str()));
    // the script reads the count as the fourth field of the row, split at the spaces
    let count = body
        .lines()
        .find(|line| line.starts_with(&row))
        .and_then(|line| line.split(' ').nth(3));
    assert_eq!(count, Some("1"), "{body}");
}
