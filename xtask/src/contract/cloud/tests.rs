//! The tests of the calls of the clouds: the values of the lab, each outcome of a call, the table
//! of a fact, and the run of the list of the repository against the fixtures of the rules.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use strata::api::MemoryApi;
use strata_preflight::probe::{FixtureBackend, LiveBackend};

use super::{Lab, run};
use crate::contract::document::{Contract, DifferenceKind, Entry, Outcome, SCHEMA, Summary};
use crate::contract::shape::{JsonType, Path as Field};
use crate::facts::cloud::{Answer, CloudRule, documents};
use crate::facts::{self, Fact, Kind};

/// The command of the first fact of [`FACTS`] in the lab of [`lab`].
const QUOTA: &str = "aws service-quotas get-service-quota --service-code ec2 --quota-code L-1216C47A --region us-east-2 --output json";

/// The command of the table, which names its region.
const ZONES: &str =
    "aws ec2 describe-availability-zones --region us-east-1 --all-availability-zones --output json";

/// A call with a value of the lab, a table, and a fact that the lab cannot send.
const FACTS: &str = r#"
- id: aws-ec2-vcpu-quota
  kind: cloud
  call: [aws, service-quotas, get-service-quota, --service-code, ec2, --quota-code, L-1216C47A, --region, "{region}", --output, json]
  reads:
    fields: [Quota.Value, Quota.Unit]
  pages: [internals/preflight.md#the-calls-of-the-clouds]
  rules: [aws-quotas]
- id: eks-refused-zone-id-us-east-1
  kind: cloud
  call: [aws, ec2, describe-availability-zones, --region, us-east-1, --all-availability-zones, --output, json]
  reads:
    fields: ["AvailabilityZones[].ZoneId"]
  table:
    as_of: "2026-09-29"
    holds: [use1-az3]
  pages: [internals/preflight.md#the-tables-of-cloud-facts]
  rules: [eks-unsupported-zone]
- id: azure-vm-usage
  kind: cloud
  reads:
    fields: [limit]
  unchecked: The lab has no account of Azure.
  pages: [internals/preflight.md#the-calls-of-the-clouds]
  rules: [azure-vcpu-quota]
- id: cluster-listing
  kind: request
  request: GET /v1/dashboard/spectroclusters/meta
  reads:
    fields: [metadata.name]
  pages: [reference/palette-api.md#a-cluster]
  code: [src/api/palette/clusters.rs]
  live: [verify.sh:snapshot-cluster]
"#;

fn facts() -> Vec<Fact> {
    crate::facts::from_text(FACTS)
}

/// The lab of an account in one region and one project.
fn lab() -> Lab {
    Lab::new([
        ("AWS_REGION".to_string(), "us-east-2".to_string()),
        (
            "STRATA_CONTRACT_PROJECT".to_string(),
            "strata-lab".to_string(),
        ),
    ])
}

/// The rules whose fixtures answer the call of [`FACTS`]: the quota has the document of the fixture.
fn rules() -> BTreeMap<String, CloudRule> {
    let answer = |command: &str, stdout: Value| Answer {
        command: command.to_string(),
        stdout,
    };
    let rule = |answers: Vec<Answer>| CloudRule {
        lab: true,
        argv: Vec::new(),
        also: Vec::new(),
        of_the_lab: false,
        answers,
    };
    BTreeMap::from([(
        "aws-quotas".to_string(),
        rule(vec![answer(
            QUOTA,
            json!({"Quota": {"Value": 64.0, "Unit": "None"}}),
        )]),
    )])
}

/// A backend that knows the tools `tools` and answers the commands of `exec`.
fn backend(tools: &[&str], exec: Value) -> FixtureBackend {
    FixtureBackend::split(json!({"_tools": tools, "_exec": exec})).0
}

/// What a command printed, with exit 0.
fn printed(stdout: Value) -> Value {
    json!({"exit": 0, "stdout": stdout, "stderr": ""})
}

/// The entries of the run of [`FACTS`] on `backend`.
fn sent(backend: &FixtureBackend) -> Vec<Entry> {
    run(&facts(), &rules(), backend, &lab())
}

fn entry<'a>(entries: &'a [Entry], id: &str) -> &'a Entry {
    entries
        .iter()
        .find(|e| e.id == id)
        .unwrap_or_else(|| panic!("no entry for {id}"))
}

// ---- the values of the lab ---------------------------------------------------------------------

fn lab_of(variables: &[(&str, &str)]) -> Lab {
    Lab::new(
        variables
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string())),
    )
}

fn words(lab: &Lab, call: &[&str]) -> Result<Vec<String>, String> {
    let call: Vec<String> = call.iter().map(|w| (*w).to_string()).collect();
    lab.words(&call)
}

#[test]
fn a_value_of_the_lab_comes_from_its_own_variable_and_then_from_the_variables_of_the_tool() {
    let own = lab_of(&[
        ("STRATA_CONTRACT_REGION", "eu-west-1"),
        ("AWS_REGION", "us-east-2"),
    ]);
    assert_eq!(
        words(&own, &["--region", "{region}"]).unwrap(),
        ["--region", "eu-west-1"]
    );
    let tool = lab_of(&[
        ("AWS_REGION", "us-east-2"),
        ("AWS_DEFAULT_REGION", "us-west-2"),
    ]);
    assert_eq!(words(&tool, &["{region}"]).unwrap(), ["us-east-2"]);
    let later = lab_of(&[("AWS_DEFAULT_REGION", "us-west-2")]);
    assert_eq!(words(&later, &["{region}"]).unwrap(), ["us-west-2"]);
    let gcp = lab_of(&[
        ("CLOUDSDK_COMPUTE_REGION", "us-central1"),
        ("GOOGLE_CLOUD_PROJECT", "p"),
    ]);
    assert_eq!(
        words(&gcp, &["{gcp_region}", "{project}"]).unwrap(),
        ["us-central1", "p"]
    );
}

#[test]
fn a_value_that_the_lab_lacks_is_named_and_a_prefix_that_the_lab_lacks_is_empty() {
    let empty = lab_of(&[]);
    assert_eq!(words(&empty, &["{region}"]), Err("region".to_string()));
    assert_eq!(words(&empty, &["{role_arn}"]), Err("role_arn".to_string()));
    assert_eq!(
        words(
            &empty,
            &["{name_prefix}nodes.cluster-api-provider-aws.sigs.k8s.io"]
        )
        .unwrap(),
        ["nodes.cluster-api-provider-aws.sigs.k8s.io"]
    );
    let prefixed = lab_of(&[("STRATA_CONTRACT_NAME_PREFIX", "strata-live-")]);
    assert_eq!(
        words(&prefixed, &["{name_prefix}nodes"]).unwrap(),
        ["strata-live-nodes"]
    );
    // a variable with no text gives nothing, so the next source answers
    let blank = lab_of(&[("STRATA_CONTRACT_REGION", ""), ("AWS_REGION", "us-east-2")]);
    assert_eq!(words(&blank, &["{region}"]).unwrap(), ["us-east-2"]);
}

#[test]
fn a_name_with_a_capital_letter_is_a_name_of_the_call_as_it_is_of_a_request() {
    let lab = lab_of(&[("STRATA_CONTRACT_REGION", "eu-west-1")]);
    assert_eq!(words(&lab, &["{Region}"]).unwrap(), ["eu-west-1"]);
    // the word with no name stays as it is, and so does a group that the list refuses
    assert_eq!(words(&lab, &["{gcp-region}"]).unwrap(), ["{gcp-region}"]);
    assert_eq!(words(&lab, &["{Other}"]), Err("Other".to_string()));
}

#[test]
fn the_variable_of_a_value_is_its_name_in_upper_case_after_the_prefix() {
    assert_eq!(Lab::variable("role_arn"), "STRATA_CONTRACT_ROLE_ARN");
    let own = lab_of(&[("STRATA_CONTRACT_ROLE_ARN", "arn:aws:iam::1:role/r")]);
    assert_eq!(
        words(&own, &["{role_arn}", "{role_arn}"]).unwrap(),
        ["arn:aws:iam::1:role/r"; 2]
    );
}

// ---- the outcome of a call ---------------------------------------------------------------------

#[test]
fn a_call_that_the_cloud_answers_in_the_shape_of_the_fixtures_is_the_same() {
    let exec =
        json!({QUOTA: printed(json!({"Quota": {"Value": 12.0, "Unit": "Count"}, "Extra": 1}))});
    let entries = sent(&backend(&["aws"], exec));
    let quota = entry(&entries, "aws-ec2-vcpu-quota");
    assert_eq!(quota.outcome, Outcome::Same);
    assert_eq!(quota.kind, Kind::Cloud);
    assert_eq!(
        quota.request,
        "aws service-quotas get-service-quota --service-code ec2 --quota-code L-1216C47A --region {region} --output json"
    );
    assert_eq!(quota.rules, ["aws-quotas"]);
    assert!(quota.live.is_empty());
    assert!(quota.differences.is_empty());
}

#[test]
fn the_run_sends_the_call_of_the_fact_with_the_values_of_the_lab() {
    // the stand-in backend answers this command only, and fails any other
    let exec = json!({QUOTA: printed(json!({"Quota": {"Value": 12.0, "Unit": "Count"}}))});
    let entries = sent(&backend(&["aws"], exec));
    assert_eq!(entry(&entries, "aws-ec2-vcpu-quota").outcome, Outcome::Same);
    let other =
        json!({"aws service-quotas get-service-quota --region us-west-2": printed(json!({}))});
    let entries = sent(&backend(&["aws"], other));
    let failed = entry(&entries, "aws-ec2-vcpu-quota");
    assert_eq!(failed.outcome, Outcome::Error);
    assert!(
        failed
            .reason
            .as_deref()
            .unwrap()
            .contains("the fixture has no output for")
    );
}

#[test]
fn a_field_that_changes_its_type_in_the_answer_of_the_cloud_is_a_difference() {
    let exec = json!({QUOTA: printed(json!({"Quota": {"Value": "12", "Unit": "None"}}))});
    let entries = sent(&backend(&["aws"], exec));
    let quota = entry(&entries, "aws-ec2-vcpu-quota");
    assert_eq!(quota.outcome, Outcome::Differs);
    assert_eq!(quota.differences.len(), 1);
    let difference = &quota.differences[0];
    assert_eq!(difference.kind, DifferenceKind::Type);
    assert_eq!(difference.path, "Quota.Value");
    assert_eq!(difference.expected, ["number"]);
    assert_eq!(difference.found, ["string"]);
}

#[test]
fn a_field_that_the_answer_of_the_cloud_lost_is_a_difference() {
    let exec = json!({QUOTA: printed(json!({"Quota": {"Unit": "None"}}))});
    let entries = sent(&backend(&["aws"], exec));
    let quota = entry(&entries, "aws-ec2-vcpu-quota");
    assert_eq!(quota.outcome, Outcome::Differs);
    assert_eq!(quota.differences[0].kind, DifferenceKind::Missing);
    assert_eq!(quota.differences[0].path, "Quota.Value");
}

#[test]
fn a_call_is_skipped_when_the_tool_is_not_on_the_machine() {
    let entries = sent(&backend(&[], json!({})));
    let quota = entry(&entries, "aws-ec2-vcpu-quota");
    assert_eq!(quota.outcome, Outcome::Skipped);
    assert_eq!(
        quota.reason.as_deref(),
        Some("aws is not on the PATH of the run")
    );
}

#[test]
fn a_call_is_skipped_when_the_lab_gives_no_value_that_the_call_names() {
    let facts = facts();
    let entries = run(
        &facts,
        &rules(),
        &backend(&["aws"], json!({})),
        &lab_of(&[]),
    );
    let quota = entry(&entries, "aws-ec2-vcpu-quota");
    assert_eq!(quota.outcome, Outcome::Skipped);
    assert_eq!(
        quota.reason.as_deref(),
        Some("the lab gives no value for {region}: set STRATA_CONTRACT_REGION")
    );
}

#[test]
fn a_call_that_fails_or_prints_no_json_is_an_error_and_not_a_difference() {
    let failed = json!({QUOTA: {"exit": 254, "stdout": "", "stderr": "An error occurred (ExpiredToken) when calling the GetServiceQuota operation"}});
    let entries = sent(&backend(&["aws"], failed));
    let quota = entry(&entries, "aws-ec2-vcpu-quota");
    assert_eq!(quota.outcome, Outcome::Error);
    assert_eq!(
        quota.reason.as_deref(),
        Some(
            "aws exited with 254: An error occurred (ExpiredToken) when calling the GetServiceQuota operation"
        )
    );
    let text = json!({QUOTA: printed(json!("not json"))});
    let entries = sent(&backend(&["aws"], text));
    assert_eq!(
        entry(&entries, "aws-ec2-vcpu-quota").outcome,
        Outcome::Error
    );
    assert_eq!(
        entry(&entries, "aws-ec2-vcpu-quota").reason.as_deref(),
        Some("aws did not print JSON")
    );
}

#[test]
fn the_error_of_a_tool_is_cut_at_two_hundred_characters_of_text() {
    let long = "é".repeat(300);
    let failed = json!({QUOTA: {"exit": 1, "stdout": "", "stderr": long}});
    let entries = sent(&backend(&["aws"], failed));
    let reason = entry(&entries, "aws-ec2-vcpu-quota")
        .reason
        .clone()
        .unwrap();
    assert_eq!(reason.chars().filter(|c| *c == 'é').count(), 200);
}

#[test]
fn a_fact_with_no_call_is_skipped_with_its_reason_and_sends_nothing() {
    let entries = sent(&backend(&["aws"], json!({})));
    let azure = entry(&entries, "azure-vm-usage");
    assert_eq!(azure.outcome, Outcome::Skipped);
    assert_eq!(
        azure.reason.as_deref(),
        Some("The lab has no account of Azure.")
    );
    assert_eq!(azure.request, "");
}

#[test]
fn a_call_that_no_fixture_answers_is_skipped() {
    let facts = facts();
    let entries = run(
        &facts,
        &BTreeMap::new(),
        &backend(&["aws"], json!({QUOTA: printed(json!({}))})),
        &lab(),
    );
    let quota = entry(&entries, "aws-ec2-vcpu-quota");
    assert_eq!(quota.outcome, Outcome::Skipped);
    assert_eq!(
        quota.reason.as_deref(),
        Some("no fixture of its rules answers the call")
    );
}

#[test]
fn the_run_makes_an_entry_for_each_cloud_fact_and_none_for_a_request_of_palette() {
    let entries = sent(&backend(&["aws"], json!({})));
    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "aws-ec2-vcpu-quota",
            "eks-refused-zone-id-us-east-1",
            "azure-vm-usage"
        ]
    );
}

// ---- the table of a fact -----------------------------------------------------------------------

fn zones(ids: &[&str]) -> Value {
    json!({"AvailabilityZones": ids.iter().map(|id| json!({"ZoneId": id})).collect::<Vec<_>>()})
}

fn table_entry(exec: Value) -> Entry {
    let entries = sent(&backend(&["aws"], exec));
    entry(&entries, "eks-refused-zone-id-us-east-1").clone()
}

#[test]
fn a_table_that_the_answer_lists_is_the_same_and_needs_no_fixture() {
    let found = table_entry(json!({ZONES: printed(zones(&["use1-az1", "use1-az3"]))}));
    assert_eq!(found.outcome, Outcome::Same, "{found:?}");
}

#[test]
fn a_value_of_a_table_that_the_answer_does_not_list_is_a_difference() {
    let found = table_entry(json!({ZONES: printed(zones(&["use1-az1", "use1-az2"]))}));
    assert_eq!(found.outcome, Outcome::Differs);
    let difference = &found.differences[0];
    assert_eq!(difference.kind, DifferenceKind::Table);
    assert_eq!(difference.path, "AvailabilityZones[].ZoneId");
    assert_eq!(difference.expected, ["use1-az3"]);
    assert_eq!(difference.found, ["use1-az1", "use1-az2"]);
}

#[test]
fn the_answer_that_a_difference_lists_is_a_sample_of_ten_values() {
    let ids: Vec<String> = (10..40).map(|n| format!("use1-az{n}")).collect();
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let found = table_entry(json!({ZONES: printed(zones(&refs))}));
    assert_eq!(found.differences[0].found.len(), 11);
    assert_eq!(found.differences[0].found[10], "and 20 more");
}

/// The entry of the table of the zone ids that `holds` lists, when the account lists the zone ids
/// `listed`. The table is one that each account sees for itself.
fn account_entry(holds: &[&str], listed: &[&str]) -> Entry {
    let mut fact = facts().remove(1);
    let table = fact.table.as_mut().unwrap();
    table.holds = holds.iter().map(|v| (*v).to_string()).collect();
    table.per_account = true;
    let backend = backend(&["aws"], json!({ZONES: printed(zones(listed))}));
    run(&[fact], &rules(), &backend, &lab()).remove(0)
}

#[test]
fn a_zone_id_that_the_account_does_not_have_is_no_difference_and_the_account_cannot_check_it() {
    // the lab has usw1-az1 and usw1-az3 in us-west-1, and the table holds usw1-az2
    let found = account_entry(&["usw1-az2"], &["usw1-az1", "usw1-az3"]);
    assert_eq!(found.outcome, Outcome::Skipped, "{found:?}");
    assert!(found.differences.is_empty(), "{found:?}");
    let reason = found.reason.expect("the entry says why");
    assert!(reason.contains("usw1-az2"), "{reason}");
    assert!(reason.contains("cannot check"), "{reason}");
}

#[test]
fn a_table_of_an_account_that_lists_some_of_its_values_is_the_same_and_names_the_others() {
    let found = account_entry(&["use1-az2", "use1-az3"], &["use1-az1", "use1-az2"]);
    assert_eq!(found.outcome, Outcome::Same, "{found:?}");
    assert!(found.differences.is_empty(), "{found:?}");
    let reason = found
        .reason
        .expect("the entry says what the account cannot check");
    assert!(reason.contains("use1-az3"), "{reason}");
    assert!(!reason.contains("use1-az2"), "{reason}");
}

#[test]
fn a_table_of_an_account_that_lists_each_value_is_the_same_with_no_reason() {
    let found = account_entry(&["use1-az3"], &["use1-az1", "use1-az3"]);
    assert_eq!(found.outcome, Outcome::Same, "{found:?}");
    assert_eq!(found.reason, None);
}

#[test]
fn a_zone_id_that_the_account_cannot_check_leaves_the_exit_code_at_0_when_the_run_checked_a_fact() {
    let mut contract = document();
    contract.add(vec![
        account_entry(&["use1-az3"], &["use1-az1", "use1-az3"]),
        account_entry(&["usw1-az2"], &["usw1-az1"]),
    ]);
    assert_eq!(contract.summary.same, 1);
    assert_eq!(contract.summary.skipped, 1);
    assert_eq!(contract.exit_code(), 0);
}

#[test]
fn only_the_tables_of_the_zone_ids_are_tables_of_an_account() {
    let facts = facts::load(&crate::repository()).expect("the list of the facts");
    let of_account: Vec<&str> = facts
        .iter()
        .filter(|f| f.table.as_ref().is_some_and(|t| t.per_account))
        .map(|f| f.id.as_str())
        .collect();
    assert_eq!(
        of_account,
        [
            "eks-refused-zone-id-us-east-1",
            "eks-refused-zone-id-us-west-1",
            "eks-refused-zone-id-ca-central-1"
        ]
    );
    // a table of another fact keeps the check: a value that the answer lacks is a difference
    assert!(
        facts
            .iter()
            .any(|f| f.id == "aws-policies-per-role-limit" && f.table.is_some())
    );
}

fn limit_fact(holds: &[&str]) -> Fact {
    let mut fact = facts().remove(1);
    fact.table.as_mut().unwrap().holds = holds.iter().map(|v| (*v).to_string()).collect();
    fact.reads.fields = vec!["Quota.Value".to_string()];
    fact.call = [
        "aws",
        "service-quotas",
        "get-aws-default-service-quota",
        "--region",
        "us-east-1",
    ]
    .iter()
    .map(|w| (*w).to_string())
    .collect();
    fact
}

fn limit_entry(holds: &[&str], value: Value) -> Entry {
    let command = "aws service-quotas get-aws-default-service-quota --region us-east-1";
    let backend = backend(
        &["aws"],
        json!({command: printed(json!({"Quota": {"Value": value}}))}),
    );
    run(&[limit_fact(holds)], &rules(), &backend, &lab()).remove(0)
}

#[test]
fn a_number_of_a_table_is_equal_to_a_listed_number_and_not_a_part_of_it() {
    assert_eq!(limit_entry(&["10"], json!(10.0)).outcome, Outcome::Same);
    assert_eq!(limit_entry(&["10"], json!(10)).outcome, Outcome::Same);
    assert_eq!(limit_entry(&["10"], json!(100.0)).outcome, Outcome::Differs);
    assert_eq!(limit_entry(&["2.5"], json!(2.5)).outcome, Outcome::Same);
    let found = limit_entry(&["10"], json!(20.0));
    assert_eq!(found.differences[0].expected, ["10"]);
    assert_eq!(found.differences[0].found, ["20"]);
}

#[test]
fn a_text_of_a_table_is_equal_to_a_listed_text_and_not_a_part_of_it() {
    let same = limit_entry(&["use1-az3"], json!("use1-az3"));
    assert_eq!(same.outcome, Outcome::Same);
    // a zone id that starts with the id of the table is another zone
    let longer = limit_entry(&["use1-az3"], json!("use1-az30"));
    assert_eq!(longer.outcome, Outcome::Differs);
}

// ---- the document ------------------------------------------------------------------------------

fn document() -> Contract {
    Contract {
        schema: SCHEMA,
        checked_at: "2026-11-02T06:00:00Z".to_string(),
        target: "https://tenant.example".to_string(),
        summary: Summary::default(),
        facts: Vec::new(),
        releases: Vec::new(),
    }
}

#[test]
fn the_entries_of_the_clouds_join_the_document_and_count_in_its_summary_and_its_exit_code() {
    let mut contract = document();
    let same = json!({QUOTA: printed(json!({"Quota": {"Value": 12.0, "Unit": "None"}}))});
    contract.add(sent(&backend(&["aws"], same)));
    assert_eq!(
        contract.summary,
        Summary {
            same: 1,
            differs: 0,
            skipped: 1,
            error: 1,
            unknown_values: 0
        }
    );
    // the table has no answer in this backend, so the run could not check it
    assert_eq!(contract.exit_code(), 2);
    let mut contract = document();
    let changed = json!({
        QUOTA: printed(json!({"Quota": {"Value": "12", "Unit": "None"}})),
        ZONES: printed(zones(&["use1-az3"])),
    });
    contract.add(sent(&backend(&["aws"], changed)));
    assert_eq!(
        contract.summary,
        Summary {
            same: 1,
            differs: 1,
            skipped: 1,
            error: 0,
            unknown_values: 0
        }
    );
    assert_eq!(contract.exit_code(), 1);
    let json = serde_json::to_value(&contract).unwrap();
    let quota = &json["facts"][0];
    assert_eq!(quota["kind"], "cloud");
    assert_eq!(quota["rules"], json!(["aws-quotas"]));
    assert_eq!(quota["differences"][0]["kind"], "type");
    assert!(json["facts"][1].get("differences").is_none());
}

#[test]
fn the_kind_of_a_difference_in_a_table_is_table() {
    let json = serde_json::to_value(DifferenceKind::Table).unwrap();
    assert_eq!(json, "table");
}

// ---- the list of the repository against the fixtures of the rules -------------------------------

/// The calls of the list that the run sends: a fact of the kind cloud with a call and no table.
fn readings(facts: &[Fact]) -> Vec<&Fact> {
    facts
        .iter()
        .filter(|f| f.kind == Kind::Cloud && !f.call.is_empty() && f.table.is_none())
        .collect()
}

/// A backend that answers the call of each reading with the first answer of the fixtures of its
/// rules, after `change` has changed it, for the lab of [`full_lab`].
fn fixtures_backend(
    facts: &[Fact],
    rules: &BTreeMap<String, CloudRule>,
    change: impl Fn(&Fact, Value) -> Value,
) -> FixtureBackend {
    let mut exec = serde_json::Map::new();
    for fact in readings(facts) {
        let document = documents(fact, rules)[0].clone();
        let command = full_lab().words(&fact.call).unwrap().join(" ");
        exec.insert(command, printed(change(fact, document)));
    }
    backend(&["aws", "gcloud"], Value::Object(exec))
}

/// A lab that gives every value.
fn full_lab() -> Lab {
    lab_of(&[
        ("AWS_REGION", "us-east-2"),
        ("STRATA_CONTRACT_PROJECT", "strata-lab"),
        ("STRATA_CONTRACT_GCP_REGION", "us-central1"),
        (
            "STRATA_CONTRACT_ROLE_ARN",
            "arn:aws:iam::123456789012:role/lab",
        ),
        ("STRATA_CONTRACT_CLUSTER", "lab-gke"),
        ("STRATA_CONTRACT_IMAGE", "u-2404-0-k-1324-0"),
    ])
}

#[test]
fn the_fixtures_of_the_rules_answer_the_call_of_each_fact_of_the_list_in_the_shape_the_fact_reads()
{
    let facts = facts::load(&crate::repository()).unwrap();
    let rules = facts::cloud_rules().unwrap();
    let mut bad = Vec::new();
    for fact in readings(&facts) {
        let found = documents(fact, &rules);
        if found.is_empty() {
            bad.push(format!("{}: no fixture answers the call", fact.id));
        }
        for field in &fact.reads.fields {
            let path = Field::parse(field);
            if found
                .iter()
                .all(|d| path.types(d).iter().all(|t| *t == JsonType::Absent))
            {
                bad.push(format!("{}: no fixture has `{field}`", fact.id));
            }
        }
    }
    assert_eq!(bad, Vec::<String>::new());
}

#[test]
fn the_run_of_the_list_against_the_fixtures_of_the_rules_finds_no_difference() {
    let facts = facts::load(&crate::repository()).unwrap();
    let rules = facts::cloud_rules().unwrap();
    let backend = fixtures_backend(&facts, &rules, |_, document| document);
    let only: Vec<Fact> = facts
        .iter()
        .filter(|f| f.table.is_none())
        .cloned()
        .collect();
    let entries = run(&only, &rules, &backend, &full_lab());
    let sent: Vec<&Entry> = entries.iter().filter(|e| !e.request.is_empty()).collect();
    assert_eq!(sent.len(), readings(&facts).len());
    assert!(sent.len() > 20, "{}", sent.len());
    let wrong: Vec<String> = sent
        .iter()
        .filter(|e| e.outcome != Outcome::Same)
        .map(|e| {
            format!(
                "{}: {:?} {:?} {:?}",
                e.id, e.outcome, e.reason, e.differences
            )
        })
        .collect();
    assert_eq!(wrong, Vec::<String>::new());
    // the facts that the lab cannot send are skipped with their reason
    let skipped: Vec<&Entry> = entries
        .iter()
        .filter(|e| e.outcome == Outcome::Skipped)
        .collect();
    assert!(
        skipped
            .iter()
            .all(|e| e.reason.as_deref().is_some_and(|r| !r.is_empty()))
    );
    assert!(skipped.iter().any(|e| e.id == "azure-vm-usage"));
}

/// AWS gave 10 managed policies for a role by default, and gives 20 now (#540). The rule uses the
/// default when it cannot read the quota of the account, so the table must hold what AWS gives.
#[test]
fn the_default_quota_of_the_managed_policies_for_a_role_is_20_and_the_run_finds_it_the_same() {
    let facts = facts::load(&crate::repository()).unwrap();
    let rules = facts::cloud_rules().unwrap();
    let find = |id: &str| facts.iter().find(|f| f.id == id).unwrap();
    let limit = find("aws-policies-per-role-limit");
    assert_eq!(limit.table.as_ref().unwrap().holds, ["20"]);
    let run_with = |value: f64| {
        let command = full_lab().words(&limit.call).unwrap().join(" ");
        let answer = printed(json!({"Quota": {"Value": value, "Adjustable": true}}));
        let backend = backend(&["aws"], json!({ command: answer }));
        run(std::slice::from_ref(limit), &rules, &backend, &full_lab()).remove(0)
    };
    assert_eq!(run_with(20.0).outcome, Outcome::Same);
    let older = run_with(10.0);
    assert_eq!(older.outcome, Outcome::Differs);
    assert_eq!(older.differences[0].expected, ["20"]);
    // the fixtures of the rule answer the quota of the account with the same default
    let quota = find("aws-policies-per-role-quota");
    assert!(
        documents(quota, &rules)
            .iter()
            .any(|document| document["Quota"]["Value"] == 20.0),
        "no fixture of the rule gives the default quota"
    );
}

/// `gcloud` omits the key `secondaryIpRanges` for a subnet that has no secondary range (#550).
/// The fixtures of the rule show a subnet with the key and a subnet without it, so an answer that
/// lacks the key is no difference.
#[test]
fn a_subnet_with_no_secondary_range_is_no_difference_when_gcloud_omits_the_key() {
    let facts = facts::load(&crate::repository()).unwrap();
    let rules = facts::cloud_rules().unwrap();
    let subnets = facts.iter().find(|f| f.id == "gcp-subnets").unwrap();
    let backend = fixtures_backend(&facts, &rules, |fact, mut document| {
        if fact.id == subnets.id {
            for subnet in document.as_array_mut().unwrap() {
                subnet.as_object_mut().unwrap().remove("secondaryIpRanges");
            }
        }
        document
    });
    let entry = run(std::slice::from_ref(subnets), &rules, &backend, &full_lab()).remove(0);
    assert_eq!(entry.outcome, Outcome::Same, "{:?}", entry.differences);
    assert!(entry.differences.is_empty(), "{:?}", entry.differences);
}

#[test]
fn a_field_that_changes_in_one_answer_makes_the_fact_differ_and_no_other_fact() {
    let facts = facts::load(&crate::repository()).unwrap();
    let rules = facts::cloud_rules().unwrap();
    let backend = fixtures_backend(&facts, &rules, |fact, mut document| {
        if fact.id == "aws-ec2-vcpu-quota" {
            document["Quota"]["Value"] = json!("sixty-four");
        }
        document
    });
    let only: Vec<Fact> = facts
        .iter()
        .filter(|f| f.table.is_none())
        .cloned()
        .collect();
    let entries = run(&only, &rules, &backend, &full_lab());
    let differing: Vec<&str> = entries
        .iter()
        .filter(|e| e.outcome == Outcome::Differs)
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(differing, ["aws-ec2-vcpu-quota"]);
    let mut contract = document();
    contract.add(entries);
    assert_eq!(contract.exit_code(), 1);
}

// ---- the document that the report script reads --------------------------------------------------

/// The document of a run that finds a difference in a call and in a table. `tests/contract.rs`
/// gives this file to `contract-report.sh`, so the document that the run writes and the document
/// that the script reads are one.
const WRITTEN: &str = include_str!("document.json");

/// The run of [`FACTS`] on a backend whose answers differ from the fixtures in a call and in a
/// table.
fn written() -> Contract {
    let mut contract = document();
    let changed = json!({
        QUOTA: printed(json!({"Quota": {"Value": "12", "Unit": "None"}})),
        ZONES: printed(zones(&["use1-az1", "use1-az2"])),
    });
    contract.add(sent(&backend(&["aws"], changed)));
    contract
}

#[test]
fn the_document_that_the_run_writes_is_the_document_that_the_report_script_reads() {
    let golden: Value = serde_json::from_str(WRITTEN).expect("document.json is JSON");
    assert_eq!(
        serde_json::to_value(written()).unwrap(),
        golden,
        "the document of the run changed: write it again in xtask/src/contract/cloud/document.json"
    );
}

// ---- the real tool ------------------------------------------------------------------------------

/// A backend that runs the command on this machine, as the contract run does.
fn live() -> LiveBackend<'static, MemoryApi> {
    LiveBackend::new(None, false)
}

/// A fact whose call is `call`, and the rule that holds one fixture answer for it.
fn tool_fact(call: &[&str], fixture: Value) -> (Fact, BTreeMap<String, CloudRule>) {
    let mut fact = facts().remove(0);
    fact.call = call.iter().map(|w| (*w).to_string()).collect();
    let rule = CloudRule {
        lab: true,
        of_the_lab: false,
        argv: Vec::new(),
        also: Vec::new(),
        answers: vec![Answer {
            command: call[0].to_string(),
            stdout: fixture,
        }],
    };
    (fact, BTreeMap::from([("aws-quotas".to_string(), rule)]))
}

#[test]
fn a_tool_on_this_machine_runs_and_its_json_is_compared_with_the_fixture() {
    let printed = r#"{"Quota":{"Value":12.0,"Unit":"None"}}"#;
    let (fact, rules) = tool_fact(
        &["echo", printed],
        json!({"Quota": {"Value": 64.0, "Unit": "None"}}),
    );
    let entries = run(&[fact], &rules, &live(), &lab());
    assert_eq!(entries[0].outcome, Outcome::Same, "{:?}", entries[0]);
    let changed = r#"{"Quota":{"Value":"12","Unit":"None"}}"#;
    let (fact, rules) = tool_fact(
        &["echo", changed],
        json!({"Quota": {"Value": 64.0, "Unit": "None"}}),
    );
    let entries = run(&[fact], &rules, &live(), &lab());
    assert_eq!(entries[0].outcome, Outcome::Differs);
    assert_eq!(entries[0].differences[0].path, "Quota.Value");
}

#[test]
fn a_tool_that_exits_with_an_error_or_prints_no_json_is_an_error() {
    let (fact, rules) = tool_fact(&["false"], json!({"Quota": {"Value": 64.0}}));
    let entries = run(&[fact], &rules, &live(), &lab());
    assert_eq!(entries[0].outcome, Outcome::Error);
    assert_eq!(entries[0].reason.as_deref(), Some("false exited with 1: "));
    let (fact, rules) = tool_fact(
        &["echo", "-n", "not json"],
        json!({"Quota": {"Value": 64.0}}),
    );
    let entries = run(&[fact], &rules, &live(), &lab());
    assert_eq!(
        entries[0].reason.as_deref(),
        Some("echo did not print JSON")
    );
}

#[test]
fn a_tool_that_is_not_on_this_machine_skips_the_call() {
    let (fact, rules) = tool_fact(&["strata-no-such-tool"], json!({}));
    let entries = run(&[fact], &rules, &live(), &lab());
    assert_eq!(entries[0].outcome, Outcome::Skipped);
    assert_eq!(
        entries[0].reason.as_deref(),
        Some("strata-no-such-tool is not on the PATH of the run")
    );
}

#[test]
fn a_table_that_reads_two_fields_is_an_error_and_the_run_does_not_pass_it_as_the_same() {
    let mut fact = facts().remove(1);
    fact.reads
        .fields
        .push("AvailabilityZones[].State".to_string());
    let entries = run(&[fact], &rules(), &backend(&["aws"], json!({})), &lab());
    assert_eq!(entries[0].outcome, Outcome::Error);
    assert!(
        entries[0]
            .reason
            .as_deref()
            .unwrap()
            .contains("a table reads one field")
    );
}
