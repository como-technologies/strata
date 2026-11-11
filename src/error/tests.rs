//! The tests of the error kinds: their messages, their JSON, and their class.

use std::collections::BTreeSet;
use std::path::Path;

// The read of a table of the book. The tests of the book read the tables of the codes with it, so
// the two tests read a table in one way.
#[path = "../../tests/common/table.rs"]
mod table;
// The macro that ties the list of a test to each variant of an enum. Two more tests use it: the
// test of each edit of the stand-in in `xtask`, and the test of `Check` in `strata-preflight`. So
// the lists keep one rule.
#[path = "../../tests/common/variants.rs"]
mod variants;

use super::*;
use table::first_cells;
use variants::one_of_each;

fn bad_json() -> serde_json::Error {
    serde_json::from_str::<serde_json::Value>("{").unwrap_err()
}

/// One error of each variant. [`one_of_each!`] ties the list to each variant of [`Error`], so the
/// build fails on a new variant until the list holds a sample of it.
///
/// The tests keep each kind distinct, keep `to_json` in step with it, and keep each kind a row of
/// the table of `reference/errors.md`.
fn each_kind_of_error() -> Vec<Error> {
    one_of_each![
        Error::MissingCredential {
            primary: "PALETTE_API_KEY",
            alternate: "SPECTROCLOUD_APIKEY",
        },
        Error::Api {
            method: Method::Get,
            path: "/v1/x".into(),
            status: 503,
            body: "busy".into(),
            retry_after: Some(2),
        },
        Error::InvalidSetting {
            variable: "STRATA_RETRIES",
            value: "lots".into(),
            expected: "a whole number",
        },
        Error::RetryAfterTooLong {
            method: Method::Post,
            path: "/v1/y".into(),
            status: 429,
            retry_after: 900,
            limit: 300,
        },
        Error::Pattern {
            pattern: "releases/**/*.json".into(),
            message: "matched no files".into(),
        },
        Error::Transport {
            method: Method::Get,
            path: "/v1/x".into(),
            message: "refused".into(),
        },
        Error::NotJson {
            method: Method::Post,
            path: "/v1/x".into(),
            source: bad_json(),
        },
        Error::NotArchive {
            method: Method::Get,
            path: "/v1/clusterprofiles/u1/spc/download".into(),
            message: "no gzip".into(),
        },
        Error::ProfileNotFound {
            name: "aws-stack".into(),
            version: "9.9.9".into(),
            tenant_scope: false,
        },
        Error::ProfileNameAmbiguous {
            name: "aws-stack".into(),
            version: "1.1.1".into(),
            profiles: vec![
                crate::model::scope::ScopedUid {
                    uid: "64e0000000000000000000c3".into(),
                    scope: Some(crate::model::scope::ProfileScope::Project),
                },
                crate::model::scope::ScopedUid {
                    uid: "64e4000000000000000000c3".into(),
                    scope: Some(crate::model::scope::ProfileScope::Tenant),
                },
            ],
        },
        Error::PackNameNotFound {
            name: "no-such-pack".into(),
            unsynced: vec!["charts-auth".into()],
        },
        Error::ClusterProfileNotFound {
            cluster: "c1".into(),
            profile: "nope".into(),
            applied: vec!["infra".into(), "addons".into()],
        },
        Error::ProfileRefRequired {
            reference: "cluster:c1".into(),
        },
        Error::NoDraft {
            name: "aws-stack".into(),
            version: "1.1.1".into(),
            uid: "64e0000000000000000000c3".into(),
        },
        Error::Usage {
            message: "--repave needs one side that is a cluster and one that is a profile version"
                .into(),
        },
        Error::StaleRecord {
            cluster: "64f1000000000000000000e1".into(),
            name: "payments".into(),
            profile: "aws-stack@1.1.1".into(),
        },
        Error::ClusterTemplateGoverns {
            cluster: "64f1000000000000000000e1".into(),
            template: "64f8000000000000000000a1".into(),
            asked: "aws-stack@1.1.1: the cluster payments runs it".into(),
        },
        Error::NoCloudConfig {
            cluster: "64f1000000000000000000e1".into(),
        },
        Error::RecordNotASource {
            path: "releases/dev.json".into(),
        },
        Error::PaletteRefuses {
            reference: "aws-stack.yaml".into(),
            errors: vec![crate::model::validation::PaletteRefusal {
                pack: "kubernetes".into(),
                code: "DependencyPackWithMinVersionRequired".into(),
                message: "Pack 'cni-cilium-oss' is required with minimum compatible version \
                              '1.21.0'"
                    .into(),
                clusters: Vec::new(),
            }],
        },
        Error::CandidateInvalid {
            path: "profile.yaml".into(),
            rule: Some(CreateRule::TypeLayer),
            message: "layer 1 (ubuntu-aws) has the layer os; a profile of type add-on takes \
                          only the layers addon, cni, and csi"
                .into(),
        },
        Error::PackNotFound {
            name: "kubernetes".into(),
            version: "9.9.9".into(),
            tag: Some("2.x".into()),
            registry: "Public Repo".into(),
            unsynced: Vec::new(),
        },
        Error::PackUnavailable {
            name: "nginx".into(),
            version: "1.15.1".into(),
            registry: "Public Repo".into(),
            state: PackState::Disabled,
        },
        Error::PackInSeveralRegistries {
            name: "csi-aws-ebs".into(),
            version: "1.65.0".into(),
            registries: vec!["Public Repo".into(), "Mirror".into()],
        },
        Error::RegistryNotFound {
            registry: "Nope".into(),
            available: vec!["Public Repo".into()],
        },
        Error::RegistryNameAmbiguous {
            name: "Public Repo".into(),
            uids: vec!["reg-public".into(), "reg-system".into()],
        },
        Error::ProjectNotFound {
            name: "staging".into(),
            available: vec!["lab".into(), "prod".into()],
        },
        Error::ClusterNotFound {
            cluster: "nope".into(),
            available: vec![],
        },
        Error::ClusterNameAmbiguous {
            name: "twin".into(),
            uids: vec!["c7".into(), "c8".into()],
        },
        Error::EdgeHostNotFound {
            edge_host: "nope".into(),
            available: vec!["edge-1".into()],
        },
        Error::EdgeHostNameAmbiguous {
            name: "twin".into(),
            uids: vec!["e7".into(), "e8".into()],
        },
        Error::ClusterGroupNotFound {
            cluster_group: "nope".into(),
            available: vec!["group-1".into()],
        },
        Error::FilterNotFound {
            filter: "prod".into(),
            available: vec!["dev".into()],
        },
        Error::FilterNameAmbiguous {
            name: "twin".into(),
            uids: vec!["f7".into(), "f8".into()],
        },
        Error::AppProfileNotFound {
            name: "shop".into(),
            version: "9.9.9".into(),
            available: vec!["shop@1.0.0".into()],
        },
        Error::AppProfileNameAmbiguous {
            name: "shop".into(),
            version: "1.0.0".into(),
            uids: vec!["a1".into(), "a2".into()],
        },
        Error::ClusterGroupNameAmbiguous {
            name: "twin".into(),
            uids: vec!["g7".into(), "g8".into()],
        },
        Error::UnsupportedSchema {
            path: "newer.json".into(),
            found: "strata-snapshot/2".into(),
            supported: "strata-snapshot/1",
        },
        Error::NotSnapshot("profile.json".into()),
        Error::SnapshotSchema {
            path: "old.json".into(),
            schema: "strata-snapshot/1",
            field: "variables[0].name".into(),
            source: bad_json(),
        },
        Error::LogInvalid {
            path: "strata-unknown.jsonl".into(),
            line: 3,
            message: "unknown field `uid`".into(),
        },
        Error::ReportSend {
            endpoint: "https://report.example/".into(),
            message: "the endpoint answered HTTP 429".into(),
        },
        Error::NoUid,
        Error::Io {
            path: "rel.json".into(),
            source: std::io::Error::other("denied"),
        },
        Error::EmptyFile("variables.json".into()),
        Error::FileNotJson {
            path: "profile.json".into(),
            source: bad_json(),
        },
        Error::Serialize(bad_json()),
    ]
}

#[test]
fn a_pack_that_is_not_found_names_the_registries_that_palette_does_not_sync() {
    let e = Error::PackNameNotFound {
        name: "podinfo".into(),
        unsynced: vec!["charts-auth".into(), "zarf".into()],
    };
    assert_eq!(
        e.to_string(),
        format!(
            "no registry that this key can see offers a pack named 'podinfo'; Palette does \
                 not sync the registries 'charts-auth' and 'zarf', and {UNSYNCED_LISTS}. `strata \
                 list --pack podinfo` finds the profiles that carry one"
        )
    );
    assert_eq!(
        e.to_json()["unsynced"],
        serde_json::json!(["charts-auth", "zarf"])
    );
    // no version was asked for, so the message names none
    let e = Error::PackNotFound {
        name: "podinfo".into(),
        version: String::new(),
        tag: None,
        registry: "charts-auth".into(),
        unsynced: vec!["charts-auth".into()],
    };
    assert_eq!(
        e.to_string(),
        format!(
            "no pack 'podinfo' in registry 'charts-auth'; Palette does not sync the registry \
                 'charts-auth', and {UNSYNCED_LISTS}"
        )
    );
    let e = Error::PackNotFound {
        name: "podinfo".into(),
        version: "1.0.0".into(),
        tag: Some("1.x".into()),
        registry: "charts".into(),
        unsynced: Vec::new(),
    };
    assert_eq!(
        e.to_string(),
        "no pack 'podinfo' at version '1.0.0' with tag '1.x' in registry 'charts'"
    );
    assert!(e.to_json().get("unsynced").is_none());
}

#[test]
fn three_registries_that_palette_does_not_sync_are_listed_with_a_comma_before_and() {
    let e = Error::PackNameNotFound {
        name: "podinfo".into(),
        unsynced: vec!["charts-auth".into(), "oci-auth".into(), "zarf".into()],
    };
    assert!(
        e.to_string()
            .contains("the registries 'charts-auth', 'oci-auth', and 'zarf', and "),
        "{e}"
    );
}

#[test]
fn the_class_of_a_5xx_reads_the_code_of_the_body_as_a_text_or_as_a_number() {
    let api = |status: u16, body: &str| Error::Api {
        method: Method::Post,
        path: "/v1/x".into(),
        status,
        body: body.into(),
        retry_after: None,
    };
    // a refusal by design stays while the inputs stay
    assert_eq!(
        api(500, r#"{"code":"OperationForbidden","message":"m"}"#).class(),
        FailureClass::Terminal
    );
    // another 5xx can go away
    assert_eq!(
        api(500, r#"{"code":"InternalError"}"#).class(),
        FailureClass::Transient
    );
    assert_eq!(api(500, "not json").class(), FailureClass::Transient);
    // the API schema refuses with the number 610: it is in no list, and a 422 stays
    assert_eq!(
        api(
            422,
            r#"{"code":610,"message":"packs in body shouldn't contain duplicates"}"#
        )
        .class(),
        FailureClass::Terminal
    );
    assert_eq!(api(500, r#"{"code":610}"#).class(), FailureClass::Transient);
}

#[test]
fn a_candidate_error_for_a_rule_of_the_create_names_the_code_and_a_format_problem_does_not() {
    let rule = Error::CandidateInvalid {
        path: "c.yaml".into(),
        rule: Some(CreateRule::PackTag),
        message: "the tag stable is not a semantic version".into(),
    };
    let text = rule.to_string();
    assert!(
        text.starts_with(&format!(
            "'c.yaml': the tag stable is not a semantic version ({}). ",
            CreateRule::PackTag.code()
        )),
        "{text}"
    );
    assert!(text.ends_with(CREATE_GAP), "{text}");
    assert_eq!(rule.to_json()["code"], CreateRule::PackTag.code());
    let format = Error::CandidateInvalid {
        path: "c.yaml".into(),
        rule: None,
        message: "a candidate needs at least one layer".into(),
    };
    assert_eq!(
        format.to_string(),
        "'c.yaml': a candidate needs at least one layer"
    );
    assert!(format.to_json().get("code").is_none());
}

#[test]
fn a_transport_error_and_a_body_that_is_not_json_name_the_method_of_the_request() {
    struct NoPost;
    impl crate::api::PaletteApi for NoPost {
        fn get_json(&self, _path: &str) -> Result<serde_json::Value> {
            Ok(serde_json::Value::Null)
        }
    }
    // the default POST of an API gives a transport error of a POST
    let post =
        crate::api::PaletteApi::post_json(&NoPost, "/v1/x", &serde_json::Value::Null).unwrap_err();
    assert_eq!(post.to_string(), "POST /v1/x: this API sends no POST");
    assert_eq!(post.to_json()["method"], "POST");
    let not_json = Error::NotJson {
        method: Method::Post,
        path: "/v1/x".into(),
        source: bad_json(),
    };
    assert!(
        not_json
            .to_string()
            .starts_with("POST /v1/x: response is not JSON: "),
        "{not_json}"
    );
    assert_eq!(not_json.to_json()["method"], "POST");
    assert_eq!(not_json.to_json()["path"], "/v1/x");
}

#[test]
fn an_empty_list_of_profiles_or_of_registries_reads_none_in_the_message() {
    let profile = Error::ClusterProfileNotFound {
        cluster: "c1".into(),
        profile: "nope".into(),
        applied: Vec::new(),
    };
    assert!(
        profile.to_string().ends_with("; applied: none"),
        "{profile}"
    );
    let registry = Error::RegistryNotFound {
        registry: "Mirror".into(),
        available: Vec::new(),
    };
    assert!(
        registry.to_string().ends_with("; registries: none"),
        "{registry}"
    );
}

#[test]
fn a_record_that_does_not_match_names_its_field_and_says_to_take_the_record_again() {
    let at = |schema: &'static str, field: &str| Error::SnapshotSchema {
        path: "old.json".into(),
        schema,
        field: field.into(),
        source: serde_json::from_str::<String>("null").unwrap_err(),
    };
    assert_eq!(
        at("strata-snapshot/1", "variables[0].name").to_string(),
        "'old.json' does not match strata-snapshot/1: variables[0].name: invalid type: null, \
         expected a string at line 1 column 4. Take the record again."
    );
    // a fault of the whole document has no field to name
    assert_eq!(
        at("strata-snapshot/1", "").to_string(),
        "'old.json' does not match strata-snapshot/1: invalid type: null, expected a string at \
         line 1 column 4. Take the record again."
    );
    // the report of `behind` is not a record
    let report = at("strata-behind/1", "records[2]").to_string();
    assert!(
        report.ends_with("Run the command that wrote the report again."),
        "{report}"
    );
}

#[test]
fn a_refusal_that_a_record_or_a_cluster_causes_has_a_kind_of_its_own_and_stays_terminal() {
    let stale = Error::StaleRecord {
        cluster: "64f1000000000000000000e1".into(),
        name: "payments".into(),
        profile: "aws-stack@1.1.1".into(),
    };
    assert_eq!(
        stale.to_string(),
        "the record of the update that waits holds aws-stack@1.1.1, and the cluster payments \
         does not run it now. Record cluster:payments@pending again"
    );
    let governed = Error::ClusterTemplateGoverns {
        cluster: "c1".into(),
        template: "t1".into(),
        asked: "aws-stack@1.1.1: the cluster payments runs it".into(),
    };
    assert!(
        governed.to_string().starts_with(
            "--repave cannot ask about aws-stack@1.1.1: the cluster payments runs it, and the \
             cluster template t1 governs the cluster."
        ),
        "{governed}"
    );
    let no_config = Error::NoCloudConfig {
        cluster: "c1".into(),
    };
    for e in [&stale, &governed, &no_config] {
        assert_ne!(e.kind(), "usage");
        assert_eq!(e.class(), FailureClass::Terminal, "{e}");
        assert_eq!(e.exit_code(), 2);
    }
}

#[test]
fn every_variant_has_a_distinct_kind() {
    let all = each_kind_of_error();
    let mut kinds: Vec<&str> = all.iter().map(Error::kind).collect();
    kinds.sort_unstable();
    kinds.dedup();
    assert_eq!(kinds.len(), all.len(), "kinds: {kinds:?}");
    for k in &kinds {
        assert!(
            k.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
            "kind {k} is not kebab-case"
        );
    }
}

#[test]
fn to_json_carries_kind_message_and_the_variant_fields() {
    for e in each_kind_of_error() {
        let j = e.to_json();
        assert_eq!(j["kind"], e.kind());
        assert_eq!(j["message"], e.to_string());
    }
    let by_kind = |k: &str| {
        each_kind_of_error()
            .into_iter()
            .find(|e| e.kind() == k)
            .unwrap()
            .to_json()
    };
    assert_eq!(by_kind("api")["status"], 503);
    assert_eq!(by_kind("api")["path"], "/v1/x");
    assert_eq!(by_kind("missing-credential")["variable"], "PALETTE_API_KEY");
    assert_eq!(
        by_kind("missing-credential")["alternate"],
        "SPECTROCLOUD_APIKEY"
    );
    assert_eq!(by_kind("not-snapshot")["path"], "profile.json");
    assert_eq!(by_kind("record-not-a-source")["path"], "releases/dev.json");
    assert_eq!(by_kind("profile-not-found")["name"], "aws-stack");
    let ambiguous = by_kind("profile-name-ambiguous");
    assert_eq!(ambiguous["version"], "1.1.1");
    assert_eq!(
        ambiguous["profiles"],
        serde_json::json!([
            {"uid": "64e0000000000000000000c3", "scope": "project"},
            {"uid": "64e4000000000000000000c3", "scope": "tenant"},
        ])
    );
    assert!(
        ambiguous["message"].as_str().unwrap().contains(
            "'aws-stack' with version '1.1.1': 64e0000000000000000000c3 (project), \
                 64e4000000000000000000c3 (tenant); give the uid"
        ),
        "{ambiguous}"
    );
    assert_eq!(by_kind("snapshot-schema")["schema"], "strata-snapshot/1");
    // the document names the field, so a script need not read it from the message
    assert_eq!(by_kind("snapshot-schema")["field"], "variables[0].name");
    let whole = Error::SnapshotSchema {
        path: "old.json".into(),
        schema: "strata-snapshot/1",
        field: String::new(),
        source: bad_json(),
    };
    assert!(whole.to_json().get("field").is_none());
    let log = by_kind("log-invalid");
    assert_eq!(log["path"], "strata-unknown.jsonl");
    assert_eq!(log["line"], 3);
    assert_eq!(
        log["message"],
        "strata-unknown.jsonl: line 3 is not a line of strata-unknown/1: unknown field `uid`"
    );
    let send = by_kind("report-send");
    assert_eq!(send["endpoint"], "https://report.example/");
    assert_eq!(
        send["message"],
        "could not send the report to https://report.example/: the endpoint answered HTTP 429"
    );
    assert_eq!(by_kind("invalid-setting")["variable"], "STRATA_RETRIES");
    assert_eq!(by_kind("invalid-setting")["value"], "lots");
    assert_eq!(by_kind("retry-after-too-long")["path"], "/v1/y");
    assert_eq!(by_kind("retry-after-too-long")["status"], 429);
    assert_eq!(by_kind("retry-after-too-long")["retryAfter"], 900);
    assert_eq!(by_kind("retry-after-too-long")["limit"], 300);
    assert_eq!(by_kind("project-not-found")["name"], "staging");
    assert_eq!(by_kind("project-not-found")["available"][1], "prod");
    assert_eq!(by_kind("cluster-not-found")["cluster"], "nope");
    assert!(
        by_kind("cluster-not-found")["message"]
            .as_str()
            .unwrap()
            .ends_with("clusters: none")
    );
    assert_eq!(by_kind("cluster-name-ambiguous")["uids"][0], "c7");
    assert_eq!(by_kind("pattern")["pattern"], "releases/**/*.json");
    let cp = by_kind("cluster-profile-not-found");
    assert_eq!(
        (cp["cluster"].clone(), cp["profile"].clone()),
        ("c1".into(), "nope".into())
    );
    assert_eq!(cp["applied"], serde_json::json!(["infra", "addons"]));
    assert_eq!(by_kind("profile-ref-required")["reference"], "cluster:c1");
    // `cluster` is the uid in the three errors of a cluster, and the name is a field of its own
    let stale = by_kind("stale-record");
    assert_eq!(
        (stale["cluster"].clone(), stale["profile"].clone()),
        ("64f1000000000000000000e1".into(), "aws-stack@1.1.1".into())
    );
    assert_eq!(stale["name"], "payments");
    let governed = by_kind("cluster-template-governs");
    assert_eq!(
        (governed["cluster"].clone(), governed["template"].clone()),
        (
            "64f1000000000000000000e1".into(),
            "64f8000000000000000000a1".into()
        )
    );
    assert_eq!(
        by_kind("no-cloud-config")["cluster"],
        "64f1000000000000000000e1"
    );
    assert_eq!(by_kind("candidate-invalid")["path"], "profile.yaml");
    assert_eq!(
        by_kind("candidate-invalid")["code"],
        CreateRule::TypeLayer.code()
    );
    assert_eq!(by_kind("unsupported-schema")["found"], "strata-snapshot/2");
    let pnf = by_kind("pack-not-found");
    assert_eq!(
        (pnf["name"].clone(), pnf["tag"].clone()),
        ("kubernetes".into(), "2.x".into())
    );
    assert_eq!(
        by_kind("registry-not-found")["available"],
        serde_json::json!(["Public Repo"])
    );
    assert_eq!(
        by_kind("pack-in-several-registries")["registries"],
        serde_json::json!(["Public Repo", "Mirror"])
    );
    assert_eq!(
        by_kind("registry-name-ambiguous")["uids"],
        serde_json::json!(["reg-public", "reg-system"])
    );
    let app = by_kind("app-profile-name-ambiguous");
    assert_eq!(
        (app["name"].clone(), app["version"].clone()),
        ("shop".into(), "1.0.0".into())
    );
    assert_eq!(app["uids"], serde_json::json!(["a1", "a2"]));
}

/// `to_json` writes the fields of a variant in one function for each family, and each of them
/// has a wildcard arm. So a variant that no function names gets `kind` and `message` only.
#[test]
fn to_json_writes_a_field_of_the_variant_for_each_kind_but_three() {
    let without_fields = ["no-uid", "serialize", "usage"];
    for e in each_kind_of_error() {
        let j = e.to_json();
        let fields = j.as_object().unwrap().len() - 2;
        assert_eq!(
            fields == 0,
            without_fields.contains(&e.kind()),
            "{}: {j}",
            e.kind()
        );
    }
}

#[test]
fn the_schema_of_an_error_document_names_the_keys_that_to_json_writes_for_every_kind() {
    let schema = serde_json::to_value(schemars::schema_for!(ErrorDocument)).unwrap();
    let named: Vec<&str> = schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(named, ["kind", "message"]);
    assert_eq!(schema["required"], serde_json::json!(["kind", "message"]));
    for e in each_kind_of_error() {
        let j = e.to_json();
        for key in named.iter() {
            assert!(j[key].is_string(), "{} has no text {key}: {j}", e.kind());
        }
    }
}

#[test]
fn each_kind_of_error_is_one_row_of_the_table_of_the_book_and_no_row_is_another_kind() {
    let kinds: BTreeSet<&str> = each_kind_of_error().iter().map(Error::kind).collect();
    let page = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/src/reference/errors.md"),
    )
    .unwrap();
    let rows = first_cells(&page, "| Kind |");
    for kind in &kinds {
        assert!(
            rows.iter().any(|row| row == kind),
            "the table of reference/errors.md has no row of the kind {kind}"
        );
    }
    // the table holds each kind, so a longer table holds a row of another text, or a kind two times
    assert_eq!(
        rows.len(),
        kinds.len(),
        "the table of reference/errors.md: {rows:?}"
    );
}

/// The `Display` of an `Api` error with `status` and `body`, for a route of the saved filters.
fn api_message(status: u16, body: &str) -> String {
    Error::Api {
        method: Method::Get,
        path: "/v1/filters/metadata?filterType=tag".into(),
        status,
        body: body.into(),
        retry_after: None,
    }
    .to_string()
}

#[test]
fn a_403_with_the_code_of_the_tenant_scope_names_the_scope_and_the_project_and_not_the_key() {
    let body = format!(
        r#"{{"code":"{PROJECT_SCOPE_CODE}","message":"Operation is not allowed in project scope and ProjectUid should be empty"}}"#
    );
    let text = api_message(403, &body);
    assert!(
        text.starts_with("GET /v1/filters/metadata?filterType=tag failed with HTTP 403: {"),
        "{text}"
    );
    assert!(text.contains("the route is of the tenant scope"), "{text}");
    assert!(text.contains("the request named a project"), "{text}");
    for key in KEY_VARS {
        assert!(!text.contains(key), "{text}");
    }
    assert!(!text.contains("the key can be wrong"), "{text}");
}

#[test]
fn a_401_or_another_403_still_names_the_key() {
    let tenant_scope = format!(r#"{{"code":"{PROJECT_SCOPE_CODE}"}}"#);
    for (status, body) in [
        (403, r#"{"code":"ResourceAccessDenied"}"#),
        (403, "forbidden"),
        (401, r#"{"code":"MissingAuthHeader"}"#),
        // the code of the tenant scope is a refusal of a 403 alone
        (401, tenant_scope.as_str()),
    ] {
        let text = api_message(status, body);
        assert!(text.contains(KEY_VARS[0]), "{status} {body}: {text}");
        assert!(
            text.contains("the key can be wrong"),
            "{status} {body}: {text}"
        );
        assert!(!text.contains("tenant scope"), "{status} {body}: {text}");
    }
}

#[test]
fn the_message_of_another_status_has_no_hint() {
    let text = api_message(404, r#"{"code":"ResourceNotFound"}"#);
    assert!(!text.contains("check "), "{text}");
    assert!(!text.contains("tenant scope"), "{text}");
}

#[test]
fn the_reason_of_an_answer_of_palette_is_its_status_and_its_body_with_no_blank_at_the_ends() {
    let api = Error::Api {
        method: Method::Get,
        path: "/v1/audits".into(),
        status: 403,
        body: "{\"code\":\"AccessDenied\"}\n".into(),
        retry_after: None,
    };
    assert_eq!(api.reason(), r#"HTTP 403: {"code":"AccessDenied"}"#);
    let transport = Error::Transport {
        method: Method::Get,
        path: "/v1/audits".into(),
        message: "connection reset".into(),
    };
    assert_eq!(transport.reason(), "GET /v1/audits: connection reset");
}
