//! The scripts around the contract run: contract.sh, contract-report.sh, and the weekly timer of
//! deploy/contract/. No network and no tenant: the commands outside this repository are
//! recording stand-ins. The comparison itself is tested in `xtask/src/contract/`.

use crate::common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::*;
use serde_json::{Value, json};

// ---- the document ------------------------------------------------------------------------------

/// The entry of a fact in a contract document.
fn entry(id: &str, outcome: &str, differences: Value) -> Value {
    json!({
        "id": id,
        "request": format!("GET /v1/{id}/{{uid}}"),
        "outcome": outcome,
        "differences": differences,
        "live": ["verify.sh:snapshot-cluster", "tiers.sh:a-applied"],
        "pages": ["reference/palette-api.md#a-cluster"],
    })
}

/// A fact whose `metadata.uid` is an object in the tenant and a string in the stand-in.
fn differing(id: &str) -> Value {
    entry(
        id,
        "differs",
        json!([{"kind": "type", "path": "metadata.uid", "expected": ["string"], "found": ["object"]}]),
    )
}

fn release(repo: &str, tag: &str, seen: Option<&str>, new: bool) -> Value {
    let mut release = json!({"repo": repo, "tag": tag, "new": new});
    if let Some(seen) = seen {
        release["seen"] = json!(seen);
    }
    release
}

fn document(facts: Vec<Value>, releases: Vec<Value>) -> Value {
    json!({
        "schema": "strata-contract/1",
        "checkedAt": "2026-11-02T05:45:00Z",
        "target": "https://api.spectrocloud.com",
        "summary": {"same": 0, "differs": 0, "skipped": 0, "error": 0, "unknownValues": 0},
        "facts": facts,
        "releases": releases,
    })
}

// ---- contract-report.sh ------------------------------------------------------------------------

/// The stand-in `gh` and the files it reads, for contract-report.sh.
struct Report {
    dir: tempfile::TempDir,
}

impl Report {
    fn new() -> Report {
        let dir = tempfile::tempdir().unwrap();
        fake_bin(dir.path(), &[("gh", FAKE_ISSUES_GH)]);
        Report { dir }
    }

    /// Write the file `name` where the stand-in reads it.
    fn put(&self, name: &str, body: &str) {
        std::fs::write(self.dir.path().join(name), body).unwrap();
    }

    /// The open issues that the stand-in lists: (number, first line of the body).
    fn open_issues(&self, issues: &[(u64, &str)]) {
        let list: Vec<Value> = issues
            .iter()
            .map(|(number, marker)| json!({"number": number, "title": "t", "body": format!("{marker}\nold")}))
            .collect();
        self.put("issues.json", &Value::Array(list).to_string());
    }

    /// The path of a contract document that holds `document`.
    fn document(&self, document: &Value) -> PathBuf {
        let path = self.dir.path().join("contract.json");
        std::fs::write(&path, document.to_string()).unwrap();
        path
    }

    fn run(&self, args: &[&str]) -> Output {
        std::fs::write(self.dir.path().join("gh.log"), "").unwrap();
        Command::new("bash")
            .arg(live_script("contract-report.sh"))
            .args(args)
            .env("PATH", path_with(&self.dir.path().join("bin")))
            .env("FAKE_VERIFY", self.dir.path())
            .env_remove("GITHUB_REPOSITORY")
            .output_locked()
            .unwrap()
    }

    /// Run the script on `document` with `--repo o/r`, and the arguments `more`.
    fn report(&self, document: &Value, more: &[&str]) -> Output {
        let path = self.document(document);
        let mut args = vec![path.to_str().unwrap(), "--repo", "o/r"];
        args.extend_from_slice(more);
        self.run(&args)
    }

    fn calls(&self) -> Vec<String> {
        lines_of(&self.dir.path().join("gh.log"))
    }

    /// The call with this 1-based number that starts with `start`.
    fn call(&self, start: &str) -> String {
        self.calls()
            .into_iter()
            .find(|c| c.starts_with(start))
            .unwrap_or_else(|| panic!("no call `{start}` in {:?}", self.calls()))
    }

    /// The body that the call with this 1-based number was handed.
    fn body(&self, number: usize) -> String {
        std::fs::read_to_string(self.dir.path().join(format!("body-{number}.md")))
            .unwrap_or_else(|_| panic!("no body from call {number}"))
    }

    fn comment(&self, number: usize) -> String {
        std::fs::read_to_string(self.dir.path().join(format!("comment-{number}.md")))
            .unwrap_or_else(|_| panic!("no comment from call {number}"))
    }
}

#[test]
fn contract_report_opens_one_issue_for_each_fact_that_differs_in_the_backlog() {
    let r = Report::new();
    let o = r.report(
        &document(
            vec![
                differing("cluster-document"),
                entry("cluster-listing", "same", json!([])),
            ],
            vec![],
        ),
        &[],
    );
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    let calls = r.calls();
    let creates: Vec<&String> = calls
        .iter()
        .filter(|c| c.starts_with("issue create"))
        .collect();
    assert_eq!(creates.len(), 1, "{calls:?}");
    assert!(
        creates[0].contains("--label palette-drift"),
        "{}",
        creates[0]
    );
    assert!(
        creates[0].contains("--title Palette drift: `cluster-document` differs from the stand-in"),
        "{}",
        creates[0]
    );
    assert!(
        creates[0].contains("--milestone Backlog"),
        "the new issue goes in the milestone Backlog: {}",
        creates[0]
    );
    assert!(
        !calls.iter().any(|c| c.starts_with("issue edit")),
        "the milestone is set at the create: {calls:?}"
    );
    let n = calls
        .iter()
        .position(|c| c.starts_with("issue create"))
        .unwrap()
        + 1;
    let body = r.body(n);
    assert_eq!(
        body.lines().next(),
        Some("<!-- strata-contract: cluster-document -->")
    );
    assert!(
        body.contains("| type | `metadata.uid` | string | object |"),
        "{body}"
    );
    assert!(
        body.contains("`verify.sh:snapshot-cluster`, `tiers.sh:a-applied`"),
        "{body}"
    );
    assert!(
        body.contains("`docs/src/reference/palette-api.md#a-cluster`"),
        "{body}"
    );
    assert!(
        body.contains(
            "https://github.com/o/r/blob/main/docs/src/development/facts.md#the-weekly-contract-run"
        ),
        "{body}"
    );
    assert!(
        text(&o.stderr).contains("1 same, 1 differ"),
        "{}",
        text(&o.stderr)
    );
}

#[test]
fn contract_report_creates_the_label_only_when_the_repository_has_none() {
    let r = Report::new();
    let doc = document(vec![differing("cluster-document")], vec![]);
    r.report(&doc, &[]);
    assert!(
        r.calls()
            .iter()
            .any(|c| c.starts_with("label create palette-drift")),
        "{:?}",
        r.calls()
    );

    r.put("labels.json", r#"[{"name": "palette-drift"}]"#);
    // the stand-in lists names one on a line: the script reads `jq -r '.[].name'`
    r.report(&doc, &[]);
    assert!(
        !r.calls().iter().any(|c| c.starts_with("label create")),
        "{:?}",
        r.calls()
    );
}

#[test]
fn contract_report_updates_the_issue_of_a_fact_that_still_differs_and_opens_no_second_one() {
    let r = Report::new();
    r.open_issues(&[(7, "<!-- strata-contract: cluster-document -->")]);
    let o = r.report(&document(vec![differing("cluster-document")], vec![]), &[]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    let calls = r.calls();
    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("issue edit 7 --repo o/r --title")),
        "{calls:?}"
    );
    assert!(
        !calls.iter().any(|c| c.starts_with("issue create")),
        "{calls:?}"
    );
    assert!(
        !calls.iter().any(|c| c.contains("--milestone")),
        "an update keeps the milestone: {calls:?}"
    );
    assert!(text(&o.stderr).contains("updated #7 for cluster-document"));
}

#[test]
fn contract_report_closes_the_issue_of_a_fact_that_is_the_same_again() {
    let r = Report::new();
    r.open_issues(&[
        (7, "<!-- strata-contract: cluster-document -->"),
        (8, "<!-- strata-contract: cluster-listing -->"),
    ]);
    let o = r.report(
        &document(vec![entry("cluster-document", "same", json!([]))], vec![]),
        &[],
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let calls = r.calls();
    let close = calls
        .iter()
        .position(|c| c.starts_with("issue close 7"))
        .expect("the issue closes");
    assert!(
        r.comment(close + 1).contains(
            "`cluster-document` is the same as the stand-in again (checked 2026-11-02T05:45:00Z)"
        ),
        "{}",
        r.comment(close + 1)
    );
    assert!(
        !calls.iter().any(|c| c.starts_with("issue close 8")),
        "a fact that the document lacks keeps its issue: {calls:?}"
    );
}

#[test]
fn contract_report_leaves_the_issue_of_a_fact_that_the_run_skipped_or_could_not_check() {
    for (outcome, code) in [("skipped", 0), ("error", 2)] {
        let r = Report::new();
        r.open_issues(&[(7, "<!-- strata-contract: cluster-document -->")]);
        let o = r.report(
            &document(vec![entry("cluster-document", outcome, json!([]))], vec![]),
            &[],
        );
        assert_eq!(
            o.status.code(),
            Some(code),
            "{outcome}: {}",
            text(&o.stderr)
        );
        let calls = r.calls();
        assert!(
            calls
                .iter()
                .all(|c| c.starts_with("issue list") || c.starts_with("repo view")),
            "{outcome}: {calls:?}"
        );
    }
}

// ---- the unknown values ------------------------------------------------------------------------

/// The entry `entry` with the closed set `set` that holds the unknown `values`.
fn with_values(mut entry: Value, set: &str, values: &[&str]) -> Value {
    let list = entry["unknownValues"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut list = list;
    list.push(json!({"set": set, "known": ["Provisioning", "Running"], "values": values}));
    entry["unknownValues"] = Value::Array(list);
    entry
}

/// A fact that the run found the same, with an unknown value of `status.state`.
fn new_state(id: &str, values: &[&str]) -> Value {
    with_values(entry(id, "same", json!([])), "status.state", values)
}

const VALUES_MARKER: &str = "<!-- strata-contract: values cluster-document status.state -->";

#[test]
fn contract_report_opens_one_issue_for_the_new_values_of_a_set_and_names_each_value() {
    let r = Report::new();
    let o = r.report(
        &document(
            vec![new_state(
                "cluster-document",
                &["Hibernating", "Quarantined"],
            )],
            vec![],
        ),
        &[],
    );
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    let calls = r.calls();
    let creates: Vec<&String> = calls
        .iter()
        .filter(|c| c.starts_with("issue create"))
        .collect();
    assert_eq!(
        creates.len(),
        1,
        "one issue for the set, not one for each value: {calls:?}"
    );
    assert!(
        creates[0]
            .contains("--title Palette has a new value of `status.state` in `cluster-document`"),
        "{}",
        creates[0]
    );
    assert!(
        creates[0].contains("--label palette-drift"),
        "{}",
        creates[0]
    );
    assert!(creates[0].contains("--milestone Backlog"), "{}", creates[0]);
    let n = calls
        .iter()
        .position(|c| c.starts_with("issue create"))
        .unwrap()
        + 1;
    let body = r.body(n);
    assert_eq!(body.lines().next(), Some(VALUES_MARKER));
    for expected in [
        "| `Hibernating` |",
        "| `Quarantined` |",
        "The fact and the stand-in name: Provisioning, Running.",
        "`GET /v1/cluster-document/{uid}`",
        "`verify.sh:snapshot-cluster`, `tiers.sh:a-applied`",
        "`docs/src/reference/palette-api.md#a-cluster`",
        "https://github.com/o/r/blob/main/docs/src/development/facts.md#what-to-do-with-a-new-value",
    ] {
        assert!(
            body.contains(expected),
            "`{expected}` is not in the body: {body}"
        );
    }
    assert!(
        text(&o.stderr).contains(
            "1 facts: 1 same, 0 differ, 0 skipped, 0 could not be checked, 2 unknown values"
        ),
        "{}",
        text(&o.stderr)
    );
}

#[test]
fn contract_report_opens_an_issue_for_each_set_and_for_each_fact_that_holds_a_new_value() {
    let r = Report::new();
    let two_sets = with_values(
        new_state("cluster-document", &["Hibernating"]),
        "spec.cloudType",
        &["gcp"],
    );
    let o = r.report(
        &document(
            vec![two_sets, new_state("cluster-listing", &["Hibernating"])],
            vec![],
        ),
        &[],
    );
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    let titles: Vec<String> = r
        .calls()
        .into_iter()
        .filter(|c| c.starts_with("issue create"))
        .collect();
    assert_eq!(titles.len(), 3, "{titles:?}");
    for title in [
        "Palette has a new value of `status.state` in `cluster-document`",
        "Palette has a new value of `spec.cloudType` in `cluster-document`",
        "Palette has a new value of `status.state` in `cluster-listing`",
    ] {
        assert!(
            titles.iter().any(|c| c.contains(title)),
            "{title}: {titles:?}"
        );
    }
}

#[test]
fn contract_report_updates_the_issue_of_a_set_that_still_holds_a_new_value() {
    let r = Report::new();
    r.open_issues(&[(7, VALUES_MARKER)]);
    let o = r.report(
        &document(
            vec![new_state("cluster-document", &["Hibernating"])],
            vec![],
        ),
        &[],
    );
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    let calls = r.calls();
    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("issue edit 7 --repo o/r --title")),
        "{calls:?}"
    );
    assert!(
        !calls.iter().any(|c| c.starts_with("issue create")),
        "{calls:?}"
    );
    assert!(
        !calls.iter().any(|c| c.starts_with("issue close")),
        "{calls:?}"
    );
    assert!(text(&o.stderr).contains("updated #7 for cluster-document status.state"));
}

#[test]
fn contract_report_closes_the_issue_of_a_set_that_holds_no_new_value_now() {
    let r = Report::new();
    r.open_issues(&[
        (7, VALUES_MARKER),
        (
            8,
            "<!-- strata-contract: values cluster-document spec.cloudType -->",
        ),
        (
            9,
            "<!-- strata-contract: values cluster-listing status.state -->",
        ),
    ]);
    let o = r.report(
        &document(
            vec![
                new_state("cluster-document", &["Hibernating"]),
                entry("cluster-listing", "same", json!([])),
            ],
            vec![],
        ),
        &[],
    );
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    let calls = r.calls();
    assert!(
        !calls.iter().any(|c| c.starts_with("issue close 7")),
        "{calls:?}"
    );
    let close = calls
        .iter()
        .position(|c| c.starts_with("issue close 8"))
        .expect("the set with no new value closes");
    assert!(
        r.comment(close + 1).contains(
            "No answer of the tenant holds a new value for `spec.cloudType` in `cluster-document` now (checked 2026-11-02T05:45:00Z)"
        ),
        "{}",
        r.comment(close + 1)
    );
    assert!(
        calls.iter().any(|c| c.starts_with("issue close 9")),
        "{calls:?}"
    );
}

#[test]
fn contract_report_keeps_the_issue_of_a_set_when_the_run_did_not_check_the_fact() {
    for outcome in ["skipped", "error"] {
        let r = Report::new();
        r.open_issues(&[(7, VALUES_MARKER)]);
        r.report(
            &document(vec![entry("cluster-document", outcome, json!([]))], vec![]),
            &[],
        );
        assert!(
            !r.calls().iter().any(|c| c.starts_with("issue close")),
            "{outcome}: {:?}",
            r.calls()
        );
    }
}

#[test]
fn contract_report_exits_0_for_a_fact_that_holds_no_new_value_and_1_for_a_fact_that_does() {
    let r = Report::new();
    let same = r.report(
        &document(vec![entry("cluster-document", "same", json!([]))], vec![]),
        &[],
    );
    assert_eq!(same.status.code(), Some(0), "{}", text(&same.stderr));
    assert!(
        text(&same.stderr).contains("0 unknown values"),
        "{}",
        text(&same.stderr)
    );
    let new = r.report(
        &document(
            vec![new_state("cluster-document", &["Hibernating"])],
            vec![],
        ),
        &[],
    );
    assert_eq!(new.status.code(), Some(1), "{}", text(&new.stderr));
}

#[test]
fn contract_report_dry_run_prints_the_body_of_a_new_value_and_calls_no_host() {
    let r = Report::new();
    let doc = r.document(&document(
        vec![new_state("cluster-document", &["Hibernating"])],
        vec![],
    ));
    let o = r.run(&[doc.to_str().unwrap(), "--dry-run"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    assert!(
        text(&o.stdout).contains(VALUES_MARKER),
        "{}",
        text(&o.stdout)
    );
    assert!(
        text(&o.stderr)
            .contains("dry run: opens or updates the issue for cluster-document status.state"),
        "{}",
        text(&o.stderr)
    );
    assert!(r.calls().is_empty(), "{:?}", r.calls());
}

#[test]
fn contract_report_opens_one_issue_for_a_new_release_and_none_for_a_release_that_is_not_new() {
    let r = Report::new();
    let releases = vec![
        release(
            "spectrocloud/palette-sdk-go",
            "v4.10.b",
            Some("v4.10.a"),
            true,
        ),
        release(
            "spectrocloud/terraform-provider-spectrocloud",
            "v0.30.4",
            Some("v0.30.4"),
            false,
        ),
    ];
    let o = r.report(
        &document(vec![entry("cluster-listing", "same", json!([]))], releases),
        &[],
    );
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    let calls = r.calls();
    let creates: Vec<&String> = calls
        .iter()
        .filter(|c| c.starts_with("issue create"))
        .collect();
    assert_eq!(creates.len(), 1, "{calls:?}");
    assert!(
        creates[0].contains("--title Prove the facts again on spectrocloud/palette-sdk-go v4.10.b"),
        "{}",
        creates[0]
    );
    assert!(creates[0].contains("--milestone Backlog"), "{}", creates[0]);
    let n = calls
        .iter()
        .position(|c| c.starts_with("issue create"))
        .unwrap()
        + 1;
    let body = r.body(n);
    assert_eq!(
        body.lines().next(),
        Some("<!-- strata-contract: release spectrocloud/palette-sdk-go v4.10.b -->")
    );
    assert!(
        body.contains("The last contract run saw `v4.10.a`"),
        "{body}"
    );
}

#[test]
fn contract_report_opens_no_second_issue_for_a_release_that_has_one() {
    let r = Report::new();
    r.open_issues(&[(
        9,
        "<!-- strata-contract: release spectrocloud/palette-sdk-go v4.10.b -->",
    )]);
    let releases = vec![release(
        "spectrocloud/palette-sdk-go",
        "v4.10.b",
        Some("v4.10.a"),
        true,
    )];
    let o = r.report(&document(vec![], releases), &[]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    let calls = r.calls();
    assert!(
        !calls
            .iter()
            .any(|c| c.starts_with("issue create") || c.starts_with("issue edit")),
        "{calls:?}"
    );
}

#[test]
fn contract_report_takes_the_label_the_milestone_and_the_repository_from_its_options() {
    let r = Report::new();
    let doc = r.document(&document(vec![differing("cluster-document")], vec![]));
    let o = r.run(&[
        doc.to_str().unwrap(),
        "--repo",
        "acme/strata",
        "--label",
        "cloud-drift",
        "--milestone",
        "Wave 14",
    ]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    assert!(
        r.call("issue list")
            .contains("--repo acme/strata --label cloud-drift")
    );
    assert!(r.call("issue create").contains("--label cloud-drift"));
    assert!(r.call("issue create").contains("--milestone Wave 14"));
}

#[test]
fn contract_report_dry_run_prints_each_body_and_calls_no_host() {
    let r = Report::new();
    let doc = r.document(&document(
        vec![
            differing("cluster-document"),
            entry("cluster-listing", "same", json!([])),
        ],
        vec![release(
            "spectrocloud/palette-sdk-go",
            "v4.10.b",
            Some("v4.10.a"),
            true,
        )],
    ));
    let o = r.run(&[doc.to_str().unwrap(), "--dry-run"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    assert!(
        out.contains("<!-- strata-contract: cluster-document -->"),
        "{out}"
    );
    assert!(
        out.contains("<!-- strata-contract: release spectrocloud/palette-sdk-go v4.10.b -->"),
        "{out}"
    );
    assert!(text(&o.stderr).contains("dry run: closes the open issue for cluster-listing"));
    assert!(
        r.calls().is_empty(),
        "a dry run calls no host: {:?}",
        r.calls()
    );
}

#[test]
fn contract_report_touches_no_issue_for_a_document_of_another_schema() {
    let r = Report::new();
    let o = r.report(
        &json!({"schema": "strata-verify-summary/1", "records": []}),
        &[],
    );
    assert_eq!(o.status.code(), Some(2));
    assert!(text(&o.stderr).contains("is not a strata-contract/1 document; no issue was touched"));
    assert!(r.calls().is_empty(), "{:?}", r.calls());

    r.put("contract.json", "");
    let o = r.run(&[
        r.dir.path().join("contract.json").to_str().unwrap(),
        "--repo",
        "o/r",
    ]);
    assert_eq!(o.status.code(), Some(2), "an empty file is no document");
    assert!(r.calls().is_empty());
}

#[test]
fn contract_report_exits_2_for_a_release_that_could_not_be_read_after_it_handled_the_issues() {
    let r = Report::new();
    let failed =
        json!({"repo": "spectrocloud/palette-sdk-go", "new": false, "error": "GET: HTTP 403"});
    let o = r.report(
        &document(vec![differing("cluster-document")], vec![failed]),
        &[],
    );
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stderr));
    assert!(
        r.calls().iter().any(|c| c.starts_with("issue create")),
        "the issues are handled first"
    );
}

#[test]
fn contract_report_exits_2_when_a_host_call_fails_and_not_1() {
    // the weekly run reads 1 as "the issues are handled": a failed host call must not look like it
    let r = Report::new();
    r.open_issues(&[(99, "<!-- strata-contract: cluster-document -->")]);
    let o = r.report(&document(vec![differing("cluster-document")], vec![]), &[]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stderr));
    assert!(text(&o.stderr).contains("Could not resolve to an issue"));
}

#[test]
fn contract_report_reads_the_host_layer_of_verify_report_and_stops_when_it_is_not_there() {
    // a copy of the repository with the two scripts, and a verify-report.sh with no host layer
    let tree = tempfile::tempdir().unwrap();
    for file in [
        "examples/live/contract-report.sh",
        "scripts/ci/verify-report.sh",
    ] {
        let to = tree.path().join(file);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join(file), &to).unwrap();
    }
    let copy = |dir: &Path| {
        Command::new("bash")
            .arg(dir.join("examples/live/contract-report.sh"))
            .arg("--help")
            .output_locked()
            .unwrap()
    };
    assert_eq!(
        copy(tree.path()).status.code(),
        Some(0),
        "the layer is found"
    );

    std::fs::write(
        tree.path().join("scripts/ci/verify-report.sh"),
        "#!/usr/bin/env bash\n",
    )
    .unwrap();
    let o = copy(tree.path());
    assert_eq!(o.status.code(), Some(2));
    assert!(text(&o.stderr).contains("scripts/ci/verify-report.sh has no host layer to read"));
}

// ---- contract.sh -------------------------------------------------------------------------------

/// A stand-in `xtask` that logs its arguments, writes the document of `FAKE_DOCUMENT` where
/// `--out` says, and exits with `FAKE_EXIT`.
const FAKE_XTASK: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_LOG"
prev=""
for a in "$@"; do
  [ "$prev" = --out ] && cp "$FAKE_DOCUMENT" "$a"
  prev="$a"
done
echo "contract: 2 facts: 1 same, 1 differ, 0 skipped, 0 could not be checked" >&2
exit "${FAKE_EXIT:-0}"
"#;

struct Run {
    dir: tempfile::TempDir,
}

impl Run {
    fn new() -> Run {
        let dir = tempfile::tempdir().unwrap();
        executable(&dir.path().join("xtask"), FAKE_XTASK);
        let doc = document(vec![differing("cluster-document")], vec![]);
        std::fs::write(dir.path().join("document.json"), doc.to_string()).unwrap();
        std::fs::write(dir.path().join("xtask.log"), "").unwrap();
        Run { dir }
    }

    fn contract(&self, args: &[&str], exit: &str) -> Output {
        Command::new("bash")
            .arg(live_script("contract.sh"))
            .args(args)
            .env("XTASK_BIN", self.dir.path().join("xtask"))
            .env("OUT", self.dir.path().join("out"))
            .env("FAKE_LOG", self.dir.path().join("xtask.log"))
            .env("FAKE_DOCUMENT", self.dir.path().join("document.json"))
            .env("FAKE_EXIT", exit)
            .output_locked()
            .unwrap()
    }

    fn calls(&self) -> Vec<String> {
        lines_of(&self.dir.path().join("xtask.log"))
    }
}

#[test]
fn contract_sh_runs_the_contract_command_writes_the_result_and_names_each_difference() {
    let r = Run::new();
    let o = r.contract(
        &[
            "--state",
            "/tmp/state.json",
            "--github",
            "http://127.0.0.1:1",
        ],
        "1",
    );
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    let out = r.dir.path().join("out");
    assert_eq!(
        r.calls(),
        [format!(
            "contract --out {}/contract.json --state /tmp/state.json --github http://127.0.0.1:1",
            out.display()
        )]
    );
    let written: Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("contract.json")).unwrap()).unwrap();
    assert_eq!(written["schema"], "strata-contract/1");
    let err = text(&o.stderr);
    assert!(err.contains("differs: cluster-document"), "{err}");
    assert!(
        err.contains("  type metadata.uid: the stand-in has string; the tenant has object"),
        "{err}"
    );
    assert!(
        err.contains(&format!("the result is {}/contract.json", out.display())),
        "{err}"
    );
    assert!(out.join("contract.log").is_file());
}

#[test]
fn contract_sh_names_each_unknown_value_with_the_values_that_the_fact_names() {
    let r = Run::new();
    let doc = document(
        vec![new_state(
            "cluster-document",
            &["Hibernating", "Quarantined"],
        )],
        vec![],
    );
    std::fs::write(r.dir.path().join("document.json"), doc.to_string()).unwrap();
    let o = r.contract(&[], "1");
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    assert!(
        text(&o.stderr).contains(
            "new value: cluster-document status.state: Hibernating, Quarantined; the fact and the stand-in name Provisioning, Running"
        ),
        "{}",
        text(&o.stderr)
    );
}

#[test]
fn contract_sh_gives_the_exit_code_of_the_run_and_names_what_could_not_be_checked() {
    for code in [0, 1, 2] {
        let r = Run::new();
        let o = r.contract(&[], &code.to_string());
        assert_eq!(o.status.code(), Some(code), "{}", text(&o.stderr));
    }
    let r = Run::new();
    let broken = document(
        vec![
            json!({"id": "cluster-document", "outcome": "error", "reason": "GET /v1/x: HTTP 500"}),
        ],
        vec![
            json!({"repo": "spectrocloud/palette-sdk-go", "tag": "v4.10.b", "new": true, "seen": "v4.10.a"}),
            json!({"repo": "spectrocloud/terraform-provider-spectrocloud", "new": false, "error": "HTTP 403"}),
        ],
    );
    std::fs::write(r.dir.path().join("document.json"), broken.to_string()).unwrap();
    let o = r.contract(&[], "2");
    let err = text(&o.stderr);
    assert!(
        err.contains("could not check: cluster-document: GET /v1/x: HTTP 500"),
        "{err}"
    );
    assert!(
        err.contains("new release: spectrocloud/palette-sdk-go v4.10.b, the last run saw v4.10.a"),
        "{err}"
    );
    assert!(
        err.contains(
            "could not read the release of spectrocloud/terraform-provider-spectrocloud: HTTP 403"
        ),
        "{err}"
    );
}

#[test]
fn contract_sh_prints_its_header_as_its_help_and_refuses_what_it_does_not_know() {
    let r = Run::new();
    let o = r.contract(&["--help"], "0");
    assert_eq!(o.status.code(), Some(0));
    assert!(
        text(&o.stdout).contains("The contract run: ask a real tenant"),
        "{}",
        text(&o.stdout)
    );
    assert!(r.calls().is_empty());

    let o = r.contract(&["--nothing"], "0");
    assert_eq!(o.status.code(), Some(2));
    assert!(text(&o.stderr).contains("unknown argument --nothing"));
    let o = r.contract(&["--state"], "0");
    assert_eq!(o.status.code(), Some(2));
    assert!(text(&o.stderr).contains("--state needs a value"));
}

#[test]
fn contract_sh_stops_before_any_request_when_xtask_is_not_built() {
    let o = Command::new("bash")
        .arg(live_script("contract.sh"))
        .env("XTASK_BIN", "/nowhere/xtask")
        .output_locked()
        .unwrap();
    assert_eq!(o.status.code(), Some(2));
    assert!(text(&o.stderr).contains("no xtask binary at /nowhere/xtask; run just stand-in"));
}

// ---- the weekly run ----------------------------------------------------------------------------

/// A stand-in that logs its name and arguments, and fails for the names in `FAKE_FAIL`.
const LOGGING: &str = r#"#!/usr/bin/env bash
echo "$(basename "$0") $*" >> "$FAKE_LOG"
case " ${FAKE_FAIL:-} " in *" $(basename "$0") "*) exit 1 ;; esac
"#;

/// A copy of the repository with the weekly script, a stand-in contract.sh and contract-report.sh,
/// and stand-in `git` and `just`.
struct Weekly {
    dir: tempfile::TempDir,
}

impl Weekly {
    fn new() -> Weekly {
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("repo");
        let weekly = tree.join("deploy/contract/weekly.sh");
        std::fs::create_dir_all(weekly.parent().unwrap()).unwrap();
        executable(&weekly, &repo_file("deploy/contract/weekly.sh"));
        std::fs::create_dir_all(tree.join("examples/live")).unwrap();
        // the stand-in contract.sh writes the result where OUT says, as the script does. It logs
        // the tag that its state file holds, and keeps the tag v2 in the file, as the run does
        executable(
            &tree.join("examples/live/contract.sh"),
            r#"#!/usr/bin/env bash
echo "contract.sh $*" >> "$FAKE_LOG"
echo "OUT=$OUT" >> "$FAKE_LOG"
echo "state-in=$(cat "$2" 2> /dev/null || echo none)" >> "$FAKE_LOG"
echo v2 > "$2"
[ -z "${FAKE_NO_RESULT:-}" ] && echo '{"schema":"strata-contract/1","checkedAt":"t","summary":{"same":1,"differs":0,"skipped":0,"error":0}}' > "$OUT/contract.json"
exit "${FAKE_CONTRACT_EXIT:-0}"
"#,
        );
        executable(
            &tree.join("examples/live/contract-report.sh"),
            r#"#!/usr/bin/env bash
echo "contract-report.sh $*" >> "$FAKE_LOG"
exit "${FAKE_REPORT_EXIT:-0}"
"#,
        );
        fake_bin(dir.path(), &[("git", LOGGING), ("just", LOGGING)]);
        std::fs::create_dir_all(dir.path().join("home")).unwrap();
        Weekly { dir }
    }

    fn data(&self) -> PathBuf {
        self.dir.path().join("data")
    }

    fn run(&self, envs: &[(&str, &str)]) -> Output {
        Command::new("bash")
            .arg(self.dir.path().join("repo/deploy/contract/weekly.sh"))
            .env("PATH", path_with(&self.dir.path().join("bin")))
            .env("HOME", self.dir.path().join("home"))
            .env("STRATA_CONTRACT_DIR", self.data())
            .env("FAKE_LOG", self.dir.path().join("calls.log"))
            .envs(envs.iter().copied())
            .output_locked()
            .unwrap()
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("calls.log"))
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    fn runs(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.data().join("runs"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

#[test]
fn the_weekly_run_updates_builds_runs_reports_and_keeps_the_result() {
    let w = Weekly::new();
    let o = w.run(&[("FAKE_CONTRACT_EXIT", "1")]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "a difference is a report and not a failure: {}",
        text(&o.stderr)
    );
    let runs = w.runs();
    assert_eq!(runs.len(), 1);
    let run = w.data().join("runs").join(&runs[0]);
    let repo = w.dir.path().join("repo");
    let calls = w.calls();
    assert_eq!(calls[0], "git pull --ff-only");
    assert_eq!(calls[1], "just examples stand-in");
    assert_eq!(
        calls[2],
        "contract.sh --state ".to_string() + run.join("state.json").to_str().unwrap()
    );
    assert_eq!(calls[3], format!("OUT={}", run.display()));
    assert_eq!(
        calls[5],
        format!("contract-report.sh {}/contract.json", run.display())
    );
    assert_eq!(
        std::fs::read_to_string(w.data().join("last.json")).unwrap(),
        std::fs::read_to_string(run.join("contract.json")).unwrap()
    );
    assert!(repo.join("deploy/contract/weekly.sh").is_file());
}

/// The tags that the runs of `w` read from their state file, in the order of the runs.
fn state_read(w: &Weekly) -> Vec<String> {
    w.calls()
        .into_iter()
        .filter(|c| c.starts_with("state-in="))
        .collect()
}

#[test]
fn the_weekly_run_keeps_the_new_tags_of_the_releases_when_the_report_is_done() {
    // the report exits 1 when it opened an issue: its work is done, and the tag must not come again
    for report in ["0", "1"] {
        let w = Weekly::new();
        std::fs::create_dir_all(w.data()).unwrap();
        std::fs::write(w.data().join("state.json"), "v1\n").unwrap();
        let o = w.run(&[("FAKE_REPORT_EXIT", report)]);
        assert_eq!(o.status.code(), Some(0), "{report}: {}", text(&o.stderr));
        assert_eq!(state_read(&w), ["state-in=v1"], "{report}");
        assert_eq!(
            std::fs::read_to_string(w.data().join("state.json")).unwrap(),
            "v2\n",
            "{report}"
        );
        let run = w.data().join("runs").join(&w.runs()[0]);
        assert!(!run.join("state.json").exists(), "{report}: the copy moved");
    }
}

#[test]
fn a_report_that_fails_keeps_the_old_state_and_the_next_run_notes_the_tag_as_new_again() {
    let w = Weekly::new();
    std::fs::create_dir_all(w.data()).unwrap();
    std::fs::write(w.data().join("state.json"), "v1\n").unwrap();
    let state = || std::fs::read_to_string(w.data().join("state.json")).unwrap();

    let o = w.run(&[("FAKE_REPORT_EXIT", "2")]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stderr));
    assert_eq!(state(), "v1\n", "the tags of the failed week are not kept");

    // the next run starts from the old tags, so it notes the new tag again
    let o = w.run(&[]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert_eq!(state(), "v2\n");

    // and the week after that has nothing new
    let o = w.run(&[]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert_eq!(
        state_read(&w),
        ["state-in=v1", "state-in=v1", "state-in=v2"]
    );
}

#[test]
fn the_weekly_run_keeps_no_state_from_a_run_that_writes_no_result() {
    let w = Weekly::new();
    let o = w.run(&[("FAKE_NO_RESULT", "1"), ("FAKE_CONTRACT_EXIT", "2")]);
    assert_eq!(o.status.code(), Some(2));
    assert!(!w.data().join("state.json").exists());
    assert_eq!(state_read(&w), ["state-in=none"]);
}

#[test]
fn the_weekly_run_goes_on_with_the_checkout_it_has_when_the_update_fails() {
    let w = Weekly::new();
    let o = w.run(&[("FAKE_FAIL", "git")]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert!(text(&o.stderr).contains("git pull failed; the run uses the checkout as it is"));
    assert!(w.calls().iter().any(|c| c.starts_with("contract.sh")));
}

#[test]
fn the_weekly_run_stops_when_the_build_fails_and_when_the_run_writes_no_result() {
    let w = Weekly::new();
    let o = w.run(&[("FAKE_FAIL", "just")]);
    assert_eq!(o.status.code(), Some(2));
    assert!(text(&o.stderr).contains("just examples stand-in failed"));
    assert!(!w.calls().iter().any(|c| c.starts_with("contract.sh")));

    let w = Weekly::new();
    let o = w.run(&[("FAKE_NO_RESULT", "1"), ("FAKE_CONTRACT_EXIT", "2")]);
    assert_eq!(o.status.code(), Some(2));
    assert!(text(&o.stderr).contains("the run wrote no result"));
    assert!(
        !w.calls()
            .iter()
            .any(|c| c.starts_with("contract-report.sh"))
    );
}

#[test]
fn the_weekly_run_fails_when_the_run_could_not_check_or_the_report_could_not_be_written() {
    for (name, value) in [("FAKE_CONTRACT_EXIT", "2"), ("FAKE_REPORT_EXIT", "2")] {
        let w = Weekly::new();
        let o = w.run(&[(name, value)]);
        assert_eq!(o.status.code(), Some(2), "{name}: {}", text(&o.stderr));
        // the issues are written for what the run did check, and the result is kept
        assert!(
            w.calls()
                .iter()
                .any(|c| c.starts_with("contract-report.sh")),
            "{name}"
        );
        assert!(w.data().join("last.json").is_file(), "{name}");
    }
}

#[test]
fn the_weekly_run_keeps_the_twelve_newest_runs() {
    let w = Weekly::new();
    for day in 1..=14 {
        std::fs::create_dir_all(w.data().join(format!("runs/202610{day:02}T053000Z"))).unwrap();
    }
    let o = w.run(&[]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let runs = w.runs();
    assert_eq!(runs.len(), 12, "{runs:?}");
    assert_eq!(
        runs[0], "20261004T053000Z",
        "the three oldest runs went: {runs:?}"
    );
    assert!(
        runs[11].as_str() > "20261014T053000Z",
        "the new run stays: {runs:?}"
    );
}

// ---- the timer ---------------------------------------------------------------------------------

/// A `systemctl` and a `loginctl` that log each call. `loginctl` says that the user has linger
/// when `FAKE_LINGER` is `yes`.
const FAKE_SYSTEMCTL: &str = r#"#!/usr/bin/env bash
echo "systemctl $*" >> "$FAKE_LOG"
"#;
const FAKE_LOGINCTL: &str = r#"#!/usr/bin/env bash
echo "${FAKE_LINGER:-no}"
"#;

struct Timer {
    dir: tempfile::TempDir,
}

impl Timer {
    fn new() -> Timer {
        let dir = tempfile::tempdir().unwrap();
        fake_bin(
            dir.path(),
            &[("systemctl", FAKE_SYSTEMCTL), ("loginctl", FAKE_LOGINCTL)],
        );
        std::fs::create_dir_all(dir.path().join("home")).unwrap();
        Timer { dir }
    }

    fn run(&self, args: &[&str], envs: &[(&str, &str)]) -> Output {
        Command::new("bash")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("deploy/contract/timer.sh"))
            .args(args)
            .env("PATH", path_with(&self.dir.path().join("bin")))
            .env("HOME", self.dir.path().join("home"))
            .env("FAKE_LOG", self.dir.path().join("calls.log"))
            .env_remove("XDG_CONFIG_HOME")
            .envs(envs.iter().copied())
            .output_locked()
            .unwrap()
    }

    fn units(&self) -> PathBuf {
        self.dir.path().join("home/.config/systemd/user")
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("calls.log"))
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }
}

#[test]
fn the_timer_installs_a_service_and_a_weekly_timer_for_this_checkout() {
    let t = Timer::new();
    let o = t.run(&["install"], &[("FAKE_LINGER", "yes")]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let repo = env!("CARGO_MANIFEST_DIR");
    let service = std::fs::read_to_string(t.units().join("strata-contract.service")).unwrap();
    assert!(
        service.contains(&format!("ExecStart={repo}/deploy/contract/weekly.sh")),
        "{service}"
    );
    assert!(
        service.contains(&format!("WorkingDirectory={repo}")),
        "{service}"
    );
    assert!(
        service.contains("Environment=AWS_PROFILE=spectro-admin"),
        "{service}"
    );
    assert!(!service.contains("@REPO@"), "{service}");
    let timer = std::fs::read_to_string(t.units().join("strata-contract.timer")).unwrap();
    assert!(timer.contains("OnCalendar=Mon *-*-* 05:30:00"), "{timer}");
    assert!(timer.contains("Persistent=true"), "{timer}");
    assert_eq!(
        t.calls(),
        [
            "systemctl --user daemon-reload",
            "systemctl --user enable --now strata-contract.timer"
        ]
    );
    assert!(!text(&o.stderr).contains("linger"), "{}", text(&o.stderr));
}

#[test]
fn the_timer_says_when_the_user_has_no_linger_and_installs_it_again_without_harm() {
    let t = Timer::new();
    let o = t.run(&["install"], &[]);
    assert_eq!(o.status.code(), Some(0));
    assert!(
        text(&o.stderr).contains("loginctl enable-linger"),
        "{}",
        text(&o.stderr)
    );
    let o = t.run(&["install"], &[]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "a second install is the first install again"
    );
    assert_eq!(std::fs::read_dir(t.units()).unwrap().count(), 2);
}

#[test]
fn the_timer_removes_its_units_and_keeps_the_results() {
    let t = Timer::new();
    let data = t.dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("last.json"), "{}").unwrap();
    t.run(&["install"], &[("FAKE_LINGER", "yes")]);
    let o = t.run(
        &["remove"],
        &[("STRATA_CONTRACT_DIR", data.to_str().unwrap())],
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert_eq!(std::fs::read_dir(t.units()).unwrap().count(), 0);
    assert!(data.join("last.json").is_file());
    let calls = t.calls();
    assert!(
        calls.contains(&"systemctl --user disable --now strata-contract.timer".to_string()),
        "{calls:?}"
    );
    assert!(text(&o.stdout).contains(&format!("the results stay in {}", data.display())));
    // removing what is not there is no fault
    let o = t.run(&["remove"], &[]);
    assert_eq!(o.status.code(), Some(0));
}

#[test]
fn the_timer_shows_the_last_result_and_refuses_a_command_it_does_not_have() {
    let t = Timer::new();
    let data = t.dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let o = t.run(
        &["status"],
        &[("STRATA_CONTRACT_DIR", data.to_str().unwrap())],
    );
    assert!(
        text(&o.stdout).contains("no result in"),
        "{}",
        text(&o.stdout)
    );
    let last = document(vec![], vec![]);
    std::fs::write(data.join("last.json"), last.to_string()).unwrap();
    let o = t.run(
        &["status"],
        &[("STRATA_CONTRACT_DIR", data.to_str().unwrap())],
    );
    assert!(
        text(&o.stdout).contains("the last run: 2026-11-02T05:45:00Z, 0 same, 0 differ, 0 skipped, 0 could not be checked, 0 unknown values"),
        "{}",
        text(&o.stdout)
    );
    let o = t.run(&["start"], &[]);
    assert_eq!(o.status.code(), Some(2));
    assert!(text(&o.stderr).contains("deploy/contract/timer.sh install"));
}

#[test]
fn the_units_hold_no_secret_and_name_only_files_that_exist() {
    for unit in ["strata-contract.service", "strata-contract.timer"] {
        // the comments say that the units hold none; the lines of the unit hold none
        let text: String = repo_file(&format!("deploy/contract/{unit}"))
            .lines()
            .filter(|line| !line.starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");
        for secret in ["API_KEY", "APIKEY", "TOKEN", "PASSWORD", "SECRET"] {
            assert!(
                !text.to_uppercase().contains(secret),
                "{unit} names {secret}"
            );
        }
    }
    for script in ["weekly.sh", "timer.sh"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("deploy/contract")
            .join(script);
        assert!(path.is_file(), "{script}");
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(
            path.metadata().unwrap().permissions().mode() & 0o111,
            0,
            "{script} runs"
        );
    }
    let service = repo_file("deploy/contract/strata-contract.service");
    assert!(service.contains("ExecStart=@REPO@/deploy/contract/weekly.sh"));
    assert!(service.contains("Type=oneshot"));
}

#[test]
fn the_recipes_of_the_contract_run_call_the_scripts_that_exist() {
    for (name, command) in [
        ("live-contract *args", "examples/live/contract.sh {{args}}"),
        (
            "contract-report *args",
            "examples/live/contract-report.sh {{args}}",
        ),
        ("contract-timer-install", "deploy/contract/timer.sh install"),
        ("contract-timer-remove", "deploy/contract/timer.sh remove"),
        ("contract-timer-status", "deploy/contract/timer.sh status"),
    ] {
        assert_eq!(recipe(name), [command], "{name}");
        let script = command.split_whitespace().next().unwrap();
        assert!(
            Path::new(env!("CARGO_MANIFEST_DIR")).join(script).is_file(),
            "{script}"
        );
    }
}
