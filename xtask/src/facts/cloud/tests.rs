//! The tests of the facts of the clouds: each check fails on one fault, and a list that agrees
//! with the rules has none.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;

use super::super::check::check as faults_of;
use super::super::model::Fact;
use super::super::sources::Sources;
use super::{
    Answer, CloudRule, CloudSources, command_words, documents, literals, read_code, read_rules,
    without_tests,
};

/// A fact of a call that a rule sends, a table of zone ids, and a call that the lab cannot send.
const FACTS: &str = r#"
- id: aws-ec2-vcpu-quota
  kind: cloud
  call: [aws, service-quotas, get-service-quota, --service-code, ec2, --quota-code, L-1216C47A, --region, "{region}", --output, json]
  reads:
    fields: [Quota.Value]
  pages: [internals/preflight.md#the-calls-of-the-clouds]
  rules: [aws-quotas]
- id: aws-zones
  kind: cloud
  call: [aws, ec2, describe-availability-zones, --region, "{region}", --output, json]
  reads:
    fields: ["AvailabilityZones[].ZoneName"]
  pages: [internals/preflight.md#the-calls-of-the-clouds]
  rules: [eks-unsupported-zone]
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
"#;

fn facts() -> Vec<Fact> {
    serde_yaml::from_str(FACTS).expect("the facts are YAML")
}

/// A rule of the cloud tier with the words of its command and the answers of its fixtures.
fn rule(lab: bool, argv: &[&str], answers: &[(&str, serde_json::Value)]) -> CloudRule {
    CloudRule {
        lab,
        of_the_lab: false,
        argv: argv.iter().map(|w| (*w).to_string()).collect(),
        answers: answers
            .iter()
            .map(|(command, stdout)| Answer {
                command: (*command).to_string(),
                stdout: stdout.clone(),
            })
            .collect(),
    }
}

/// The rules in which [`FACTS`] has no fault.
fn rules() -> BTreeMap<String, CloudRule> {
    let quota = "aws service-quotas get-service-quota --service-code ec2 --quota-code L-1216C47A --region us-east-2 --output json";
    let zones = "aws ec2 describe-availability-zones --region us-east-2 --output json";
    BTreeMap::from([
        (
            "aws-quotas".to_string(),
            rule(
                true,
                &[
                    "aws",
                    "service-quotas",
                    "get-service-quota",
                    "--service-code",
                    "ec2",
                    "--quota-code",
                    "L-1216C47A",
                ],
                &[(quota, json!({"Quota": {"Value": 64.0}}))],
            ),
        ),
        (
            "eks-unsupported-zone".to_string(),
            rule(
                true,
                &[
                    "aws",
                    "ec2",
                    "describe-availability-zones",
                    "--region",
                    "{{ .region }}",
                ],
                &[(
                    zones,
                    json!({"AvailabilityZones": [{"ZoneName": "us-east-2a", "ZoneId": "use2-az1"}]}),
                )],
            ),
        ),
        (
            "azure-vcpu-quota".to_string(),
            rule(false, &["az", "vm", "list-usage"], &[]),
        ),
    ])
}

/// The text of the programs of the rules of [`rules`].
fn programs() -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "aws-quotas".to_string(),
            "aws service-quotas get-service-quota L-1216C47A .Quota.Value".to_string(),
        ),
        (
            "eks-unsupported-zone".to_string(),
            r#"["use1-az3"] .AvailabilityZones[].ZoneName"#.to_string(),
        ),
        ("azure-vcpu-quota".to_string(), ".limit".to_string()),
    ])
}

/// Replace the programs of the rule `id` in `sources`.
fn hold(sources: &mut Sources, id: &str, programs: &str) {
    sources
        .cloud
        .programs
        .insert(id.to_string(), programs.to_string());
}

/// The world in which [`FACTS`] has no fault.
fn world() -> Sources {
    let anchors = BTreeSet::from([
        "the-calls-of-the-clouds".to_string(),
        "the-tables-of-cloud-facts".to_string(),
    ]);
    Sources {
        facts: facts(),
        anchors: BTreeMap::from([("internals/preflight.md".to_string(), anchors)]),
        rule_ids: rules().keys().cloned().collect(),
        cloud: CloudSources {
            rules: rules(),
            programs: programs(),
            code: BTreeMap::new(),
        },
        ..Sources::default()
    }
}

/// Assert that `sources` has exactly one fault, and that its message holds `part`.
fn assert_one_fault(sources: &Sources, part: &str) {
    let found = faults_of(sources);
    assert!(
        found.len() == 1 && found[0].contains(part),
        "expected one fault with `{part}`, got {found:#?}"
    );
}

#[test]
fn a_list_of_cloud_facts_that_agrees_with_the_rules_has_no_fault() {
    assert_eq!(faults_of(&world()), Vec::<String>::new());
}

#[test]
fn a_rule_of_the_cloud_tier_that_no_fact_names_is_a_fault() {
    let mut sources = world();
    sources.cloud.rules.insert(
        "kms".to_string(),
        rule(true, &["aws", "kms", "describe-key"], &[]),
    );
    sources.rule_ids.insert("kms".to_string());
    assert_one_fault(
        &sources,
        "the rule `kms` is of the cloud tier, and no fact of the kind cloud names it",
    );
}

#[test]
fn a_cloud_fact_that_names_a_rule_outside_the_cloud_tier_is_a_fault() {
    let mut sources = world();
    sources.rule_ids.insert("eks-one-zone".to_string());
    sources.facts[0].rules.push("eks-one-zone".to_string());
    assert_one_fault(&sources, "the rule `eks-one-zone` is not of the cloud tier");
}

#[test]
fn the_call_of_a_fact_is_the_command_of_each_of_its_rules() {
    let mut sources = world();
    sources.cloud.rules.get_mut("aws-quotas").unwrap().argv =
        ["aws", "service-quotas", "list-services"]
            .iter()
            .map(|w| (*w).to_string())
            .collect();
    assert_one_fault(
        &sources,
        "the command of the rule `aws-quotas` is not `aws service-quotas get-service-quota`",
    );
}

#[test]
fn a_call_that_no_fixture_of_the_rules_answers_is_a_fault() {
    let mut sources = world();
    sources
        .cloud
        .rules
        .get_mut("aws-quotas")
        .unwrap()
        .answers
        .clear();
    assert_one_fault(
        &sources,
        "no fixture of its rules answers `aws service-quotas get-service-quota`",
    );
}

#[test]
fn two_facts_with_one_call_are_a_fault_and_two_calls_with_one_command_are_not() {
    let mut sources = world();
    let mut copy = sources.facts[0].clone();
    copy.id = "aws-eks-node-quota".to_string();
    copy.call[6] = "L-BD136A63".to_string();
    sources.facts.push(copy);
    assert_eq!(faults_of(&sources), Vec::<String>::new(), "two quotas");
    let mut same = sources.facts[0].clone();
    same.id = "aws-quota-again".to_string();
    sources.facts.push(same);
    assert_one_fault(
        &sources,
        "send one call, `aws service-quotas get-service-quota",
    );
}

#[test]
fn a_cloud_fact_has_a_call_or_a_reason_and_not_both() {
    let mut sources = world();
    // the rule of the fact has no command, so the missing call is the one fault
    sources.facts[0].call.clear();
    hold(&mut sources, "aws-quotas", ".Quota.Value");
    sources
        .cloud
        .rules
        .get_mut("aws-quotas")
        .unwrap()
        .argv
        .clear();
    assert_one_fault(
        &sources,
        "no call proves it, and `unchecked` gives no reason",
    );
    let mut sources = world();
    sources.facts[0].unchecked = Some("no".to_string());
    assert_one_fault(&sources, "it has a call, so `unchecked` must go");
    let mut sources = world();
    sources.facts[3].unchecked = Some(" ".to_string());
    assert_one_fault(&sources, "`unchecked` is empty");
}

#[test]
fn a_cloud_fact_that_no_call_proves_needs_no_field_of_the_answer() {
    let mut sources = world();
    sources.facts[3].reads = Default::default();
    assert_eq!(faults_of(&sources), Vec::<String>::new());
}

#[test]
fn a_cloud_fact_names_no_live_check_no_route_of_the_stand_in_and_no_request() {
    let mut sources = world();
    sources.facts[0]
        .live
        .push("verify.sh:snapshot-cluster".to_string());
    sources.facts[0].stand_in.push("v1/packs".to_string());
    sources.facts[0].request = Some("GET /v1/packs".to_string());
    let found = faults_of(&sources);
    assert!(
        found.iter().any(|f| f.contains("names no live check")),
        "{found:#?}"
    );
    assert!(
        found.iter().any(|f| f.contains("names no route")),
        "{found:#?}"
    );
    assert!(
        found
            .iter()
            .any(|f| f.contains("only a request fact has a request")),
        "{found:#?}"
    );
}

#[test]
fn only_a_cloud_fact_has_a_call_or_a_table() {
    let mut sources = world();
    sources.facts = serde_yaml::from_str(
        r"
- id: limit-release-route
  kind: behavior
  reads: {fields: [version.installedVersion]}
  live: [release.sh:release-no-key]
  pages: [internals/preflight.md#the-calls-of-the-clouds]
  code: [preflight/src/main.rs]
  call: [aws, sts, get-caller-identity]
",
    )
    .unwrap();
    sources.cloud.rules.clear();
    let found = faults_of(&sources);
    assert!(
        found
            .iter()
            .any(|f| f.contains("only a fact of the kind cloud has a call or a table")),
        "{found:#?}"
    );
}

#[test]
fn a_zone_id_in_a_rule_that_no_fact_proves_is_a_fault() {
    let mut sources = world();
    hold(
        &mut sources,
        "eks-unsupported-zone",
        r#"["use1-az3", "usw1-az2"]"#,
    );
    assert_one_fault(
        &sources,
        "the rule `eks-unsupported-zone` holds a zone id, `usw1-az2`, and no fact proves it",
    );
}

#[test]
fn a_zone_id_is_proven_by_the_table_of_a_fact_of_the_rule_and_not_by_the_table_of_another_rule() {
    let mut sources = world();
    hold(&mut sources, "aws-quotas", r#"["use1-az3"]"#);
    assert_one_fault(
        &sources,
        "the rule `aws-quotas` holds a zone id, `use1-az3`, and no fact proves it",
    );
}

#[test]
fn a_quota_code_is_proven_by_the_call_of_a_fact_of_the_rule() {
    let mut sources = world();
    hold(
        &mut sources,
        "aws-quotas",
        "the quota L-BD136A63 and L-1216C47A",
    );
    assert_one_fault(&sources, "holds a quota code, `L-BD136A63`");
}

#[test]
fn a_text_that_names_a_cloud_fact_in_the_code_of_strata_is_a_fault() {
    let mut sources = world();
    sources.cloud.code.insert(
        "src/zones.rs".to_string(),
        "const REFUSED: [&str; 1] = [\"use1-az3\"];".to_string(),
    );
    assert_one_fault(
        &sources,
        "the file `src/zones.rs` holds a zone id, `use1-az3`: read it from the cloud",
    );
}

#[test]
fn the_patterns_find_each_kind_of_text_that_names_a_cloud_fact_and_not_a_plain_word() {
    let found = |text: &str| -> Vec<String> {
        literals(text)
            .into_iter()
            .map(|(_, literal)| literal)
            .collect()
    };
    assert_eq!(
        found("use1-az3 and use1-bos1-az1"),
        ["use1-az3", "use1-bos1-az1"]
    );
    assert_eq!(
        found("us-east-2 and ca-central-1"),
        ["ca-central-1", "us-east-2"]
    );
    assert_eq!(
        found("us-central1 europe-west4"),
        ["europe-west4", "us-central1"]
    );
    assert_eq!(
        found("t3.xlarge m5.2xlarge t3.micro"),
        ["m5.2xlarge", "t3.micro", "t3.xlarge"]
    );
    assert_eq!(
        found("e2-standard-4 and n2-highmem-8 and e2-medium"),
        ["e2-medium", "e2-standard-4", "n2-highmem-8"]
    );
    assert_eq!(
        found("Standard_D4s_v3 and Basic_A1"),
        ["Basic_A1", "Standard_D4s_v3"]
    );
    assert_eq!(found("ami-0123456789abcdef0"), ["ami-0123456789abcdef0"]);
    assert_eq!(
        found("AL2023_x86_64_STANDARD BOTTLEROCKET_ARM_64"),
        ["AL2023_x86_64_STANDARD", "BOTTLEROCKET_ARM_64"]
    );
    assert_eq!(found("AL2_x86_64"), ["AL2_x86_64"]);
    assert_eq!(found("L-1216C47A"), ["L-1216C47A"]);
    // the regions, the types, and the families that a first reading of the patterns missed
    assert_eq!(
        found("southamerica-east1 northamerica-northeast1 cn-north-1 us-gov-west-1"),
        [
            "northamerica-northeast1",
            "southamerica-east1",
            "cn-north-1",
            "us-gov-west-1"
        ]
    );
    assert_eq!(
        found("inf1.xlarge trn1.32xlarge hpc6a.48xlarge mac1.metal"),
        [
            "hpc6a.48xlarge",
            "inf1.xlarge",
            "mac1.metal",
            "trn1.32xlarge"
        ]
    );
    assert_eq!(
        found("c3d-standard-8 n2-custom-8"),
        ["c3d-standard-8", "n2-custom-8"]
    );
    // words of a program that look near to a pattern
    for plain in [
        "xlarge",
        "2xlarge",
        r#"sub("^(Standard_|Basic_)?[A-Za-z]+"; "")"#,
        "us-east",
        "compute.googleapis.com",
        "L-1216",
        "az3",
    ] {
        assert_eq!(found(plain), Vec::<String>::new(), "{plain}");
    }
}

#[test]
fn a_table_needs_a_date_a_value_and_one_field() {
    let mut sources = world();
    sources.facts[2].table.as_mut().unwrap().as_of = "last week".to_string();
    assert_one_fault(&sources, "`as_of` must be a date");
    let mut sources = world();
    sources.facts[2].table.as_mut().unwrap().holds.clear();
    hold(
        &mut sources,
        "eks-unsupported-zone",
        ".AvailabilityZones[].ZoneName",
    );
    assert_one_fault(&sources, "the table holds no value");
    let mut sources = world();
    sources.facts[2]
        .reads
        .fields
        .push("AvailabilityZones[].State".to_string());
    assert_one_fault(&sources, "a table reads one field");
}

#[test]
fn a_table_has_a_call_unless_every_rule_of_the_fact_is_for_a_cloud_that_the_lab_lacks() {
    let mut sources = world();
    sources.facts[2].call.clear();
    sources.facts[2].unchecked = Some("The lab has no account of Azure.".to_string());
    assert_one_fault(
        &sources,
        "the table has no call, and a rule of the fact is for a cloud of the lab",
    );
    // the same table of a rule for Azure alone
    let mut sources = world();
    let mut azure = sources.facts[2].clone();
    azure.id = "azure-sizes".to_string();
    azure.call.clear();
    azure.unchecked = Some("The lab has no account of Azure.".to_string());
    azure.rules = vec!["azure-vcpu-quota".to_string()];
    azure.table.as_mut().unwrap().holds = vec!["4".to_string()];
    sources.facts.push(azure);
    assert_eq!(faults_of(&sources), Vec::<String>::new());
}

#[test]
fn a_text_value_of_a_table_must_be_in_a_program_of_the_rule_and_a_number_need_not() {
    let mut sources = world();
    sources.facts[2].table.as_mut().unwrap().holds =
        vec!["use1-az3".to_string(), "use1-az4".to_string()];
    assert_one_fault(
        &sources,
        "the table holds `use1-az4`, and no rule of the fact does",
    );
    let mut sources = world();
    sources.facts[2]
        .table
        .as_mut()
        .unwrap()
        .holds
        .push("10".to_string());
    assert_eq!(faults_of(&sources), Vec::<String>::new());
}

#[test]
fn the_words_of_a_command_stop_at_the_first_flag_and_at_the_first_value_of_the_lab() {
    let words = |text: &str| -> Vec<String> {
        command_words(text.split_whitespace())
            .into_iter()
            .map(String::from)
            .collect()
    };
    assert_eq!(
        words("aws ec2 describe-subnets --region us-east-2"),
        ["aws", "ec2", "describe-subnets"]
    );
    assert_eq!(
        words("gcloud projects get-iam-policy {project} --format json"),
        ["gcloud", "projects", "get-iam-policy"]
    );
    assert_eq!(
        words("gcloud projects get-iam-policy {{ .p }} --format json"),
        ["gcloud", "projects", "get-iam-policy"]
    );
    assert_eq!(words("--region x"), Vec::<String>::new());
}

#[test]
fn the_answers_of_a_fact_are_the_answers_of_its_rules_that_its_command_starts() {
    let mut rules = rules();
    rules.get_mut("aws-quotas").unwrap().answers.push(Answer {
        command: "aws service-quotas list-services --region us-east-2".to_string(),
        stdout: json!({"Services": []}),
    });
    rules
        .get_mut("eks-unsupported-zone")
        .unwrap()
        .answers
        .push(Answer {
            command: "aws service-quotas get-service-quota --service-code eks".to_string(),
            stdout: json!({"Quota": {"Value": 450.0}}),
        });
    let fact = &facts()[0];
    let found = documents(fact, &rules);
    // the answer of the same command in another rule is not an answer of this fact
    assert_eq!(found, [&json!({"Quota": {"Value": 64.0}})]);
    // a value after the command, such as a project, does not stop the match
    let mut rules = BTreeMap::new();
    rules.insert(
        "gcp-quotas".to_string(),
        rule(
            true,
            &["gcloud", "compute", "regions", "describe"],
            &[(
                "gcloud compute regions describe us-central1 --project strata-lab --format json",
                json!({"quotas": []}),
            )],
        ),
    );
    let mut fact = facts()[0].clone();
    fact.rules = vec!["gcp-quotas".to_string()];
    fact.call = [
        "gcloud",
        "compute",
        "regions",
        "describe",
        "{gcp_region}",
        "--format",
        "json",
    ]
    .iter()
    .map(|w| (*w).to_string())
    .collect();
    assert_eq!(documents(&fact, &rules).len(), 1);
}

// ---- the catalog and the code of the repository ------------------------------------------------

#[test]
fn the_rules_of_the_cloud_tier_are_read_with_their_command_their_cloud_and_their_fixtures() {
    let catalog = strata_preflight::catalog::builtin().unwrap();
    let (read, unread) = read_rules(&catalog);
    assert_eq!(unread, Vec::<String>::new());
    let rules = &read.rules;
    let quota = &rules["aws-quotas"];
    assert!(quota.lab);
    assert_eq!(
        quota.argv[..3],
        ["aws", "service-quotas", "get-service-quota"]
    );
    assert!(read.programs["aws-quotas"].contains("L-1216C47A"));
    assert!(
        quota.answers.iter().all(|a| a.stdout.is_object()),
        "an answer has a document"
    );
    assert!(!quota.answers.is_empty());
    // a rule of Azure only is not for the lab, and a rule with no check has no command
    assert!(!rules["azure-vcpu-quota"].lab);
    assert!(rules["gcp-org-policies"].argv.is_empty());
    // a rule that checks the lab itself is marked, and a rule of a cloud is not
    assert!(rules["tools-present"].of_the_lab);
    assert!(rules["ecr-token"].of_the_lab);
    assert!(!rules["aws-quotas"].of_the_lab);
    // a rule of a static or a palette check is not of the cloud tier, and its programs are read
    assert!(!rules.contains_key("eks-one-zone"));
    assert!(!rules.contains_key("instance-offering"));
    assert!(read.programs.contains_key("eks-one-zone"));
    assert!(read.programs["instance-offering"].contains("instancetypes"));
}

#[test]
fn a_fixture_that_failed_has_no_document_to_compare() {
    let catalog = strata_preflight::catalog::builtin().unwrap();
    let (read, _) = read_rules(&catalog);
    // the answer of a failed `gcloud compute images describe` is text, so it is not an answer
    let image = &read.rules["gcp-image-not-shared"];
    assert!(image.answers.iter().all(|a| !a.stdout.is_string()));
    assert!(!image.answers.is_empty());
}

#[test]
fn the_code_that_the_scan_reads_has_no_test_module_no_test_file_and_no_comment() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    std::fs::create_dir_all(src.join("tests")).unwrap();
    std::fs::write(
        src.join("lib.rs"),
        "// us-east-2 in a comment\nconst A: &str = \"x\";\n#[cfg(test)]\nmod tests {\n    const R: &str = \"us-east-1\";\n}\n",
    )
    .unwrap();
    std::fs::write(src.join("tests.rs"), "const R: &str = \"eu-west-1\";").unwrap();
    std::fs::write(src.join("tests/data.rs"), "const R: &str = \"eu-west-2\";").unwrap();
    let (code, unread) = read_code(dir.path());
    assert_eq!(unread, Vec::<String>::new());
    assert_eq!(code.keys().collect::<Vec<_>>(), ["src/lib.rs"]);
    assert_eq!(code["src/lib.rs"], "const A: &str = \"x\";");
}

#[test]
fn an_item_for_the_tests_that_is_no_module_does_not_end_the_code_that_the_scan_reads() {
    let text = "const A: &str = \"x\";\n#[cfg(test)]\nimpl Call {\n}\nconst B: &str = \"us-east-1\";\n#[cfg(test)]\n#[allow(dead_code)]\nmod tests;\nconst C: &str = \"eu-west-1\";\n";
    let kept = without_tests(text);
    assert!(kept.contains("const B"), "{kept}");
    assert!(kept.contains("#[cfg(test)]\n#[allow(dead_code)]"), "{kept}");
    // the module of tests ends the code, also when an attribute stands between them
    let plain = "const A: &str = \"x\";\n#[cfg(test)]\nmod tests {\n    const R: &str = \"us-east-1\";\n}\n";
    assert_eq!(without_tests(plain), "const A: &str = \"x\";");
}

#[test]
fn a_file_of_the_code_that_cannot_be_read_is_named_and_not_taken_for_clean() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    // a file with a name of a Rust file whose bytes are no text
    std::fs::write(src.join("binary.rs"), [0xff_u8, 0xfe, 0xfd]).unwrap();
    let (code, unread) = read_code(dir.path());
    assert!(code.is_empty());
    assert_eq!(unread.len(), 1);
    assert!(unread[0].starts_with("src/binary.rs: "), "{unread:?}");
}

#[test]
fn the_code_of_strata_holds_no_text_that_names_a_cloud_fact() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let (code, unread) = read_code(&repo);
    assert_eq!(unread, Vec::<String>::new());
    assert!(
        code.contains_key("src/lib.rs"),
        "{:?}",
        code.keys().take(3).collect::<Vec<_>>()
    );
    assert!(code.contains_key("preflight/src/lib.rs"));
    // the scan reads the code after an item for the tests, such as the types of the outcome of a check
    assert!(code["preflight/src/probe/outcome.rs"].contains("pub enum Ran"));
    let found: Vec<String> = code
        .iter()
        .flat_map(|(path, text)| {
            literals(text)
                .into_iter()
                .map(move |(kind, l)| format!("{path}: {kind} {l}"))
        })
        .collect();
    assert_eq!(found, Vec::<String>::new());
}

// ---- the call must not change the account -------------------------------------------------------

#[test]
fn a_call_must_list_describe_get_show_or_simulate() {
    for call in [
        "aws ec2 describe-subnets --region {region}",
        "aws iam list-policies",
        "aws sts get-caller-identity",
        "aws iam simulate-principal-policy --policy-source-arn {role_arn}",
        "gcloud projects get-iam-policy {project} --format json",
        "gcloud compute regions describe {gcp_region}",
        "gcloud services list --enabled",
        "az account show",
    ] {
        let mut sources = world();
        sources.facts[0].call = call.split_whitespace().map(String::from).collect();
        // the rule of the fact has another command, which is a fault of its own
        let found: Vec<String> = faults_of(&sources)
            .into_iter()
            .filter(|f| f.contains("must list, describe, get, show, or simulate"))
            .collect();
        assert_eq!(found, Vec::<String>::new(), "{call}");
    }
}

#[test]
fn a_call_that_could_change_the_account_is_a_fault() {
    for call in [
        "aws ec2 terminate-instances --instance-ids {id}",
        "aws iam delete-role --role-name {role}",
        "aws s3 rm s3://bucket",
        "gcloud compute instances delete {name}",
        "aws",
    ] {
        let mut sources = world();
        sources.facts[0].call = call.split_whitespace().map(String::from).collect();
        let found: Vec<String> = faults_of(&sources)
            .into_iter()
            .filter(|f| f.contains("must list, describe, get, show, or simulate"))
            .collect();
        assert_eq!(found.len(), 1, "{call}: {found:?}");
    }
}

#[test]
fn the_call_of_a_table_must_not_change_the_account_either() {
    let mut sources = world();
    sources.facts[2].call = ["aws", "ec2", "delete-subnet", "--subnet-id", "s"]
        .iter()
        .map(|w| (*w).to_string())
        .collect();
    let found = faults_of(&sources);
    assert!(
        found
            .iter()
            .any(|f| f
                .contains("fact `eks-refused-zone-id-us-east-1`: the call `aws ec2 delete-subnet")),
        "{found:#?}"
    );
}

// ---- a rule that runs a command on a cloud of the lab needs a call ------------------------------

#[test]
fn a_fact_with_no_call_cannot_name_a_rule_that_runs_a_command_on_a_cloud_of_the_lab() {
    let mut sources = world();
    sources.facts[0].call.clear();
    sources.facts[0].unchecked = Some("no".to_string());
    hold(&mut sources, "aws-quotas", ".Quota.Value");
    assert_one_fault(
        &sources,
        "the rule `aws-quotas` runs `aws service-quotas get-service-quota` on a cloud of the lab, and the fact has no call",
    );
}

#[test]
fn a_fact_with_no_call_can_name_a_row_a_rule_of_another_cloud_and_a_check_of_the_lab() {
    // a row: the rule has no command
    let mut sources = world();
    sources.facts[0].call.clear();
    sources.facts[0].unchecked = Some("The rule has no check.".to_string());
    hold(&mut sources, "aws-quotas", ".Quota.Value");
    sources
        .cloud
        .rules
        .get_mut("aws-quotas")
        .unwrap()
        .argv
        .clear();
    assert_eq!(faults_of(&sources), Vec::<String>::new(), "a row");
    // a rule of Azure: the lab has no account of it
    let mut sources = world();
    assert!(!sources.cloud.rules["azure-vcpu-quota"].lab);
    sources.facts[3].unchecked = Some("The lab has no account of Azure.".to_string());
    assert_eq!(faults_of(&sources), Vec::<String>::new(), "azure");
    // a check of the lab itself, such as the tools on the PATH
    let mut sources = world();
    sources.facts[0].call.clear();
    sources.facts[0].unchecked = Some("The rule reads this machine.".to_string());
    hold(&mut sources, "aws-quotas", ".Quota.Value");
    sources
        .cloud
        .rules
        .get_mut("aws-quotas")
        .unwrap()
        .of_the_lab = true;
    assert_eq!(faults_of(&sources), Vec::<String>::new(), "the lab");
}

// ---- a table in a rule of another tier ----------------------------------------------------------

#[test]
fn a_zone_id_in_a_rule_of_another_tier_is_a_fault_unless_a_table_proves_it() {
    let mut sources = world();
    sources.rule_ids.insert("eks-one-zone".to_string());
    hold(
        &mut sources,
        "eks-one-zone",
        r#"[ .azs[] | select(. == "use1-az3") ]"#,
    );
    assert_one_fault(
        &sources,
        "the rule `eks-one-zone` holds a zone id, `use1-az3`, and no fact proves it",
    );
    // a table names the rule of any tier
    sources.facts[2].rules.push("eks-one-zone".to_string());
    assert_eq!(faults_of(&sources), Vec::<String>::new());
}

#[test]
fn a_cloud_fact_with_no_table_cannot_name_a_rule_of_another_tier() {
    let mut sources = world();
    sources.rule_ids.insert("eks-one-zone".to_string());
    sources.facts[1].rules.push("eks-one-zone".to_string());
    assert_one_fault(&sources, "the rule `eks-one-zone` is not of the cloud tier");
}

#[test]
fn the_catalog_holds_a_text_that_names_a_cloud_fact_in_a_cloud_tier_rule_alone() {
    let catalog = strata_preflight::catalog::builtin().unwrap();
    let (read, _) = read_rules(&catalog);
    let outside: Vec<String> = read
        .programs
        .iter()
        .filter(|(id, _)| !read.rules.contains_key(*id))
        .flat_map(|(id, text)| {
            literals(text)
                .into_iter()
                .map(move |(kind, l)| format!("{id}: {kind} {l}"))
        })
        .collect();
    assert_eq!(outside, Vec::<String>::new());
    assert!(
        read.programs.len() > read.rules.len(),
        "the programs of every rule"
    );
}
