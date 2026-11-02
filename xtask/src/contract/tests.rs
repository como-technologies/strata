//! The tests of the contract run: the shape of a document, the subject of a request, each outcome
//! of a fact, the exit code, and the run against a served stand-in.

use std::collections::BTreeSet;
use std::net::TcpListener;
use std::path::Path;

use serde_json::{Value, json};
use strata::api::codes::PROJECT_SCOPE_CODE;
use strata::api::{HttpApi, MemoryApi, RawCluster, cloud_config_path};
use strata::config::Config;

use super::document::{Contract, DifferenceKind, Entry, Outcome, Unknown};
use super::shape::{JsonType, Path as Field};
use super::stand_in::StandIn;
use super::{releases, run, subject};
use crate::facts::subjects::{Origin, Read, SOURCES, floor, request_of, unsourced};
use crate::facts::{Fact, Kind, Scope, expand, fill};
use crate::palette::{TENANT_ROUTES, serve_on};
use crate::tenant::{TENANT, Tenant};

/// The facts of a small list: the cluster listing, the document of a cluster, its cloud config, a
/// search that sends a body, and a host that the run cannot find.
const FACTS: &str = r#"
- id: cluster-listing
  kind: request
  request: GET /v1/dashboard/spectroclusters/meta
  reads:
    fields: [metadata.name, metadata.uid, spec.cloudType, status.state]
    values:
      status.state: [Running, Provisioning]
  stand_in: [v1/dashboard/spectroclusters/meta]
  live: [verify.sh:snapshot-cluster]
  pages: [reference/palette-api.md#a-cluster]
  code: [src/api/palette/clusters.rs]
- id: cluster-document
  kind: request
  request: GET /v1/spectroclusters/{uid}
  reads:
    fields:
      - metadata.uid
      - spec.cloudConfigRef.uid
      - status.state
      - status.fips.mode
      - spec.clusterProfileTemplates[].name
    values:
      status.state: [Running, Provisioning]
  stand_in: ["v1/spectroclusters/{uid}"]
  live: [verify.sh:snapshot-cluster]
  pages: [reference/palette-api.md#a-cluster]
  code: [src/api/palette/clusters.rs]
- id: cloud-config
  kind: request
  request: GET /v1/cloudconfigs/{cloudType}/{configUid}
  reads:
    fields: ["spec.machinePoolConfig[]"]
  stand_in: ["v1/cloudconfigs/{cloudType}/{configUid}"]
  live: [verify.sh:snapshot-cluster]
  pages: [reference/palette-api.md#a-cluster]
  code: [src/api/palette/clusters.rs]
- id: cluster-search
  kind: request
  request: POST /v1/dashboard/spectroclusters/search
  reads:
    fields: ["items[].metadata.uid"]
  stand_in: [v1/dashboard/spectroclusters/search]
  live: [fleet.sh:search]
  pages: [reference/palette-api.md#a-cluster]
  code: [src/api/palette/listings.rs]
- id: edge-host-document
  kind: request
  request: GET /v1/edgehosts/{uid}
  reads:
    fields: [metadata.uid]
  stand_in: ["v1/edgehosts/{uid}"]
  live: [edge-host.sh:host]
  pages: [reference/palette-api.md#an-edge-host]
  code: [src/api/palette/clusters.rs]
- id: validate-name
  kind: request
  request: GET /v1/clusterprofiles/validate/name
  reads:
    codes: [ResourceNameEmpty]
  stand_in: [v1/clusterprofiles/validate/name]
  live: [validate-packs.sh:validate-name-free]
  pages: [internals/palette/validate-routes.md#the-name]
  code: [src/api/palette/profiles.rs]
- id: cluster-summary
  kind: request
  request: GET /v1/dashboard/spectroclusters/{uid}
  reads:
    fields: [status.health.state]
  unchecked: no run
  pages: [reference/palette-api.md#a-cluster]
  code: [src/api/palette/clusters.rs]
- id: cluster-tier-fields
  kind: behavior
  reads:
    fields: [spec.clusterConfig]
  live: [tiers.sh:a-applied]
  pages: [internals/palette/clusters.md#cluster-tiers]
  code: [src/snapshot/cluster.rs]
"#;

/// The documents of the small tenant, by route: one cluster on AWS, and its cloud config.
fn documents(state: &str) -> Vec<(&'static str, Value)> {
    vec![
        (
            "v1/dashboard/spectroclusters/meta",
            json!([{
                "metadata": {"name": "payments", "uid": "c1"},
                "spec": {"cloudType": "aws"},
                "status": {"state": state},
            }]),
        ),
        (
            "v1/spectroclusters/c1",
            json!({
                "metadata": {"uid": "c1"},
                "spec": {"cloudType": "aws", "cloudConfigRef": {"uid": "cfg1"},
                         "clusterProfileTemplates": [{"name": "aws-stack"}]},
                "status": {"state": state},
            }),
        ),
        (
            "v1/cloudconfigs/aws/cfg1",
            json!({"spec": {"machinePoolConfig": [{"name": "control-plane-pool"}]}}),
        ),
    ]
}

fn facts() -> Vec<Fact> {
    crate::facts::from_text(FACTS)
}

/// Write the documents `docs` into the tenant directory `dir`, at tenant scope.
fn write_tenant(dir: &Path, docs: &[(&str, Value)]) {
    let tenant = Tenant::new(dir, Path::new("/nowhere"));
    for (route, document) in docs {
        tenant.put(TENANT, route, document);
    }
}

/// The stand-in of one chapter that holds `docs`.
fn stand_in(docs: &[(&str, Value)]) -> (tempfile::TempDir, StandIn) {
    let root = tempfile::tempdir().unwrap();
    write_tenant(&root.path().join("chapter"), docs);
    let stand_in = StandIn::read(root.path()).unwrap();
    (root, stand_in)
}

/// The address of the stand-in server on a tenant that holds `docs`.
fn serving(docs: &[(&str, Value)]) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    write_tenant(dir.path(), docs);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let root = dir.path().to_path_buf();
    std::thread::spawn(move || serve_on(&root, &listener));
    (dir, url)
}

/// A client of the stand-in server on a tenant that holds `docs`.
fn served(docs: &[(&str, Value)]) -> (tempfile::TempDir, HttpApi) {
    let (dir, url) = serving(docs);
    (dir, HttpApi::new(Config::new(url, "a-key")))
}

/// Two clients of the stand-in server on a tenant that holds `docs`: one that names the project
/// `p1`, as the environment of a run does, and one that names no project.
fn served_with_project(docs: &[(&str, Value)]) -> (tempfile::TempDir, HttpApi, HttpApi) {
    let (dir, url) = serving(docs);
    let config = Config::new(url, "a-key").with_project("p1");
    let tenant = HttpApi::new(config.clone().without_project());
    (dir, HttpApi::new(config), tenant)
}

/// The run of `facts` against the tenant that `api` talks to.
fn contract(stand_in: &StandIn, api: &impl strata::api::PaletteApi) -> Contract {
    contract_of(&facts(), stand_in, api)
}

/// The run of the facts `facts`, in place of the small list, with one client for both scopes.
fn contract_of(facts: &[Fact], stand_in: &StandIn, api: &impl strata::api::PaletteApi) -> Contract {
    contract_with(facts, stand_in, api, api)
}

/// The run of the facts `facts` with a client that names a project and a client that names none.
fn contract_with<A: strata::api::PaletteApi>(
    facts: &[Fact],
    stand_in: &StandIn,
    client: &A,
    tenant: &A,
) -> Contract {
    run(
        facts,
        stand_in,
        client,
        tenant,
        "https://tenant.example",
        "2026-11-02T06:00:00Z",
    )
}

/// The facts of the repository.
fn real_facts() -> Vec<Fact> {
    crate::facts::load(&crate::repository()).expect("the list of the facts")
}

/// The outcome of the fact `id` in `contract`.
fn outcome(contract: &Contract, id: &str) -> Outcome {
    contract
        .facts
        .iter()
        .find(|e| e.id == id)
        .unwrap_or_else(|| panic!("no entry for {id}"))
        .outcome
}

fn memory(docs: &[(&str, Value)]) -> MemoryApi {
    docs.iter().fold(MemoryApi::new(), |api, (route, doc)| {
        api.with(format!("/{route}"), doc.clone())
    })
}

// ---- the shape of a document ---------------------------------------------------------------

fn types(path: &str, doc: &Value) -> Vec<JsonType> {
    Field::parse(path).types(doc).into_iter().collect()
}

#[test]
fn a_field_has_the_type_of_its_value_and_a_missing_field_or_a_null_is_absent() {
    let doc = json!({"a": {"b": "text", "c": null, "d": 3, "e": [], "f": {}, "g": true}});
    assert_eq!(types("a.b", &doc), [JsonType::String]);
    assert_eq!(types("a.c", &doc), [JsonType::Absent]);
    assert_eq!(types("a.d", &doc), [JsonType::Number]);
    assert_eq!(types("a.e", &doc), [JsonType::Array]);
    assert_eq!(types("a.f", &doc), [JsonType::Object]);
    assert_eq!(types("a.g", &doc), [JsonType::Boolean]);
    assert_eq!(types("a.missing", &doc), [JsonType::Absent]);
    assert_eq!(types("missing.deeper.still", &doc), [JsonType::Absent]);
}

#[test]
fn a_path_through_a_list_reaches_each_entry_and_none_for_an_empty_list() {
    let doc = json!({"packs": [{"name": "a"}, {"name": 1}, {}], "none": [], "gone": null});
    assert_eq!(
        types("packs[].name", &doc),
        [JsonType::Absent, JsonType::Number, JsonType::String]
    );
    assert!(types("none[].name", &doc).is_empty());
    assert!(types("gone[].name", &doc).is_empty());
}

#[test]
fn a_path_reaches_the_values_of_an_object_with_a_star() {
    let doc =
        json!({"events": {"x": {"type": "PackValuesUpdate"}, "y": {"type": "PackManifestUpdate"}}});
    let texts = Field::parse("events.*.type").texts(&doc);
    assert_eq!(
        texts.into_iter().collect::<Vec<_>>(),
        ["PackManifestUpdate", "PackValuesUpdate"]
    );
}

#[test]
fn an_answer_that_is_a_list_is_read_with_a_path_that_starts_with_a_key_or_with_a_list() {
    let doc = json!([{"profileUid": "p1", "variables": []}, {"profileUid": "p2"}]);
    assert_eq!(types("[].profileUid", &doc), [JsonType::String]);
    assert_eq!(types("profileUid", &doc), [JsonType::String]);
    assert_eq!(
        types("variables", &doc),
        [JsonType::Absent, JsonType::Array]
    );
}

#[test]
fn the_texts_of_a_field_omit_a_value_that_is_no_text_and_an_empty_text() {
    let doc = json!({"items": [{"state": "Running"}, {"state": ""}, {"state": 1}, {}]});
    let texts = Field::parse("items[].state").texts(&doc);
    assert_eq!(texts.into_iter().collect::<Vec<_>>(), ["Running"]);
}

// ---- the subject of a request --------------------------------------------------------------

#[test]
fn the_subject_of_a_request_comes_from_the_first_entry_of_a_listing() {
    let api = memory(&documents("Running"));
    let probe = subject::Probe::new(&api, &api);
    let found = subject::find(&probe, &facts(), "/v1/spectroclusters/{uid}");
    assert_eq!(
        found,
        subject::Subject::Path("/v1/spectroclusters/c1".to_string())
    );
}

#[test]
fn the_placeholders_of_a_cloud_config_come_from_the_document_of_the_cluster() {
    let api = memory(&documents("Running"));
    let probe = subject::Probe::new(&api, &api);
    let found = subject::find(&probe, &facts(), "/v1/cloudconfigs/{cloudType}/{configUid}");
    assert_eq!(
        found,
        subject::Subject::Path("/v1/cloudconfigs/aws/cfg1".to_string())
    );
}

/// The documents of the small tenant, with `cluster` as the document of its cluster.
fn with_cluster(cluster: Value) -> Vec<(&'static str, Value)> {
    let mut docs = documents("Running");
    docs[1].1 = cluster;
    docs
}

#[test]
fn the_cloud_type_of_a_cluster_is_read_as_strata_reads_it() {
    // the kind of the cloud config comes first, then the cloud type of the spec, as the route of
    // the cloud config of strata reads them
    let cases = [
        (
            "the kind only",
            json!({"cloudConfigRef": {"kind": "eks", "uid": "cc1"}}),
        ),
        (
            "the kind and another cloud type",
            json!({"cloudType": "aws", "cloudConfigRef": {"kind": "eks", "uid": "cc1"}}),
        ),
        (
            "the cloud type only",
            json!({"cloudType": "aws", "cloudConfigRef": {"uid": "cc1"}}),
        ),
    ];
    for (name, spec) in cases {
        let cluster = json!({"metadata": {"uid": "c1"}, "spec": spec});
        let kind = RawCluster::cloud_type(&cluster).to_string();
        let api = memory(&with_cluster(cluster));
        let probe = subject::Probe::new(&api, &api);
        let found = subject::find(&probe, &facts(), "/v1/cloudconfigs/{cloudType}/{configUid}");
        assert!(!kind.is_empty(), "{name}");
        assert_eq!(
            found,
            subject::Subject::Path(cloud_config_path(&kind, "cc1")),
            "{name}"
        );
    }
}

#[test]
fn a_cluster_that_names_no_cloud_type_is_no_subject_for_its_cloud_config() {
    let cluster = json!({"metadata": {"uid": "c1"}, "spec": {"cloudConfigRef": {"uid": "cc1"}}});
    let api = memory(&with_cluster(cluster));
    let probe = subject::Probe::new(&api, &api);
    match subject::find(&probe, &facts(), "/v1/cloudconfigs/{cloudType}/{configUid}") {
        subject::Subject::Missing(why) => {
            assert!(why.contains("{cloudType}"), "{why}");
            assert!(why.contains("holds no cloud type"), "{why}");
        }
        other => panic!("expected no subject, got {other:?}"),
    }
}

#[test]
fn a_listing_is_read_once_for_all_the_requests_that_use_it() {
    let api = memory(&documents("Running"));
    let probe = subject::Probe::new(&api, &api);
    for _ in 0..3 {
        subject::find(&probe, &facts(), "/v1/spectroclusters/{uid}");
    }
    assert_eq!(
        api.calls("/v1/dashboard/spectroclusters/meta"),
        1,
        "the listing is read once"
    );
}

#[test]
fn a_tenant_with_no_cluster_has_no_subject_and_the_reason_says_what_the_listing_held() {
    let api = MemoryApi::new().with("/v1/dashboard/spectroclusters/meta", json!([]));
    let probe = subject::Probe::new(&api, &api);
    match subject::find(&probe, &facts(), "/v1/spectroclusters/{uid}") {
        subject::Subject::Missing(why) => {
            assert!(why.contains("/v1/dashboard/spectroclusters/meta"), "{why}");
            assert!(why.contains("/0/metadata/uid"), "{why}");
        }
        other => panic!("expected no subject, got {other:?}"),
    }
}

#[test]
fn a_listing_that_fails_is_a_failure_to_find_and_not_a_missing_object() {
    let api = MemoryApi::new().failing("/v1/dashboard/spectroclusters/meta", 403, "{}");
    let probe = subject::Probe::new(&api, &api);
    match subject::find(&probe, &facts(), "/v1/spectroclusters/{uid}") {
        subject::Subject::Failed(failure) => assert_eq!(failure.status, Some(403)),
        other => panic!("expected a failure, got {other:?}"),
    }
}

#[test]
fn a_value_that_a_path_cannot_hold_is_no_subject() {
    let listing = json!([{"metadata": {"uid": "a/b"}}]);
    let api = MemoryApi::new().with("/v1/dashboard/spectroclusters/meta", listing);
    let probe = subject::Probe::new(&api, &api);
    assert!(matches!(
        subject::find(&probe, &facts(), "/v1/spectroclusters/{uid}"),
        subject::Subject::Missing(_)
    ));
}

#[test]
fn each_get_request_of_the_list_has_a_subject_or_is_named_here_as_having_none() {
    // The requests whose placeholders no source gives. The run skips them with a reason. A new
    // fact that the stand-in serves, with a placeholder, must have a source or be named here with
    // the reason. A fact with no document of the stand-in is skipped before the run needs a subject.
    const NO_SUBJECT: &[(&str, &str)] = &[
        (
            "cloud-account",
            "the kind of the account is not in the listing",
        ),
        (
            "edge-host-document",
            "the hosts come from a search that sends a body",
        ),
        (
            "edge-host-manifest",
            "the hosts come from a search that sends a body",
        ),
        (
            "profile-manifest",
            "the name of the pack and the manifest need a profile with a manifest",
        ),
        (
            "app-deployment-document",
            "the fact has no listing of the deployments",
        ),
    ];
    let facts = real_facts();
    let mut lacking = Vec::new();
    for fact in facts.iter().filter(|f| f.kind == Kind::Request) {
        let Some(template) = fact.get_path() else {
            continue;
        };
        // a request with alternatives is checked when one of its choices has a source for each
        // placeholder: the choices of the other clouds skip with their reason
        let sourced = expand(template)
            .iter()
            .any(|(_, text)| unsourced(text).is_empty());
        let named = NO_SUBJECT.iter().any(|(id, _)| *id == fact.id);
        if !named && !fact.stand_in.is_empty() && !sourced {
            lacking.push(fact.id.clone());
        }
    }
    assert!(
        lacking.is_empty(),
        "no source gives a subject to these requests: {lacking:?}. Add a source in \
         subject.rs, or name the fact in NO_SUBJECT with the reason."
    );
    for (id, _) in NO_SUBJECT {
        assert!(
            facts.iter().any(|f| f.id == *id),
            "NO_SUBJECT names {id}, which is no fact"
        );
    }
}

#[test]
fn each_source_names_a_get_request_of_the_list_whose_own_placeholders_have_a_source() {
    let facts = real_facts();
    for source in SOURCES {
        let Origin::Fact { fact, read } = &source.origin else {
            continue;
        };
        let request = request_of(&facts, fact)
            .unwrap_or_else(|| panic!("`{fact}` is no GET request of the list"));
        assert!(
            unsourced(request).is_empty(),
            "the request of `{fact}` has a placeholder with no source"
        );
        match read {
            Read::Pointer(pointer) => assert!(pointer.starts_with('/'), "{pointer} is no pointer"),
            Read::Each { list, value } => {
                assert!(
                    list.starts_with('/') && value.starts_with('/'),
                    "{list} {value}"
                );
            }
            Read::CloudType => {}
        }
    }
}

// ---- the query of a request ----------------------------------------------------------------

/// The documents that answer the three requests which need a query, with the file names that the
/// stand-in uses: the query follows an `@`. The cloud account of the regions comes from a listing.
fn query_documents() -> Vec<(&'static str, Value)> {
    vec![
        (
            "v1/filters/metadata@filterType=tag",
            json!({"items": [{"metadata": {"name": "production", "uid": "f1"}}]}),
        ),
        (
            "v1/filters/tag/f1",
            json!({"spec": {"filterGroup": {"conjunction": "and",
                "filters": [{"key": "env", "values": ["prod"]}]}}}),
        ),
        (
            "v1/cloudaccounts/aws",
            json!({"items": [{"metadata": {"uid": "acc1"}}]}),
        ),
        (
            "v1/clouds/aws/regions@cloudAccountUid=acc1",
            json!({"regions": [{"name": "us-east-2"}]}),
        ),
    ]
}

#[test]
fn the_run_asks_the_tag_filters_and_the_regions_with_the_query_that_the_stand_in_serves() {
    let docs = query_documents();
    let (_root, stand_in) = stand_in(&docs);
    let (_dir, api) = served(&docs);
    let contract = contract_of(&real_facts(), &stand_in, &api);
    for id in ["tag-filter-listing", "tag-filter", "validate-aws-regions"] {
        let entry = contract.facts.iter().find(|e| e.id == id).unwrap();
        assert_eq!(entry.outcome, Outcome::Same, "{id}: {entry:?}");
    }
    assert_eq!(contract.exit_code(), 0);
}

#[test]
fn the_stand_in_serves_the_tag_filters_and_the_regions_only_with_their_query() {
    // a request with no query finds no document: the stand-in serves the query that strata sends.
    // Palette lists each filter of each type when the query has no `filterType`, and refuses the
    // regions with HTTP 422
    let (_dir, api) = served(&query_documents());
    let probe = subject::Probe::new(&api, &api);
    for path in ["/v1/filters/metadata", "/v1/clouds/aws/regions"] {
        let answer = probe.get(Scope::Client, path);
        let failure = answer.as_ref().as_ref().unwrap_err();
        assert_eq!(failure.status, Some(404), "{path}");
    }
}

#[test]
fn a_placeholder_in_the_query_gets_its_value_from_the_listing_of_the_tenant() {
    let api = memory(&query_documents()[2..3]);
    let probe = subject::Probe::new(&api, &api);
    let found = subject::find(
        &probe,
        &real_facts(),
        "/v1/clouds/aws/regions?cloudAccountUid={cloudAccountUid}",
    );
    assert_eq!(
        found,
        subject::Subject::Path("/v1/clouds/aws/regions?cloudAccountUid=acc1".to_string())
    );
}

#[test]
fn a_tenant_with_no_aws_account_has_no_subject_for_the_regions_and_the_reason_says_why() {
    let api = MemoryApi::new().with("/v1/cloudaccounts/aws", json!({"items": []}));
    let probe = subject::Probe::new(&api, &api);
    match subject::find(
        &probe,
        &real_facts(),
        "/v1/clouds/aws/regions?cloudAccountUid={cloudAccountUid}",
    ) {
        subject::Subject::Missing(why) => {
            assert!(why.contains("/v1/cloudaccounts/aws"), "{why}");
            assert!(why.contains("{cloudAccountUid}"), "{why}");
        }
        other => panic!("expected no subject, got {other:?}"),
    }
}

// ---- the scope of a route ------------------------------------------------------------------

/// The outcomes of the two entries of the saved tag filters in `contract`.
fn filter_outcomes(contract: &Contract) -> [Outcome; 2] {
    [
        outcome(contract, "tag-filter-listing"),
        outcome(contract, "tag-filter"),
    ]
}

#[test]
fn the_two_facts_of_the_saved_tag_filters_say_that_their_route_is_of_the_tenant_scope() {
    let facts = real_facts();
    for id in ["tag-filter-listing", "tag-filter"] {
        let fact = facts.iter().find(|f| f.id == id).unwrap();
        assert_eq!(fact.scope, Scope::Tenant, "{id}");
    }
}

#[test]
fn the_run_asks_the_saved_tag_filters_with_no_project() {
    // the tenant holds the filters at tenant scope, and the stand-in refuses a request for them
    // that names a project, as Palette does
    let docs = query_documents();
    let (_root, stand_in) = stand_in(&docs);
    let (_dir, client, tenant) = served_with_project(&docs);
    let contract = contract_with(&real_facts(), &stand_in, &client, &tenant);
    assert_eq!(filter_outcomes(&contract), [Outcome::Same, Outcome::Same]);
}

#[test]
fn the_run_that_asks_the_saved_tag_filters_with_the_project_gets_the_refusal_of_palette() {
    // the fault of the first contract run against a real tenant: the same facts with the scope of
    // the client are sent with the project, and the stand-in answers HTTP 403
    let docs = query_documents();
    let (_root, stand_in) = stand_in(&docs);
    let (_dir, client, tenant) = served_with_project(&docs);
    let mut facts = real_facts();
    for fact in facts.iter_mut().filter(|f| f.scope == Scope::Tenant) {
        fact.scope = Scope::Client;
    }
    let contract = contract_with(&facts, &stand_in, &client, &tenant);
    assert_eq!(
        filter_outcomes(&contract),
        [Outcome::Differs, Outcome::Error]
    );
    let listing = contract
        .facts
        .iter()
        .find(|e| e.id == "tag-filter-listing")
        .unwrap();
    assert_eq!(listing.differences[0].kind, DifferenceKind::Status);
    assert_eq!(listing.differences[0].found, ["403"]);
    let filter = contract
        .facts
        .iter()
        .find(|e| e.id == "tag-filter")
        .unwrap();
    let reason = filter.reason.as_deref().unwrap_or_default();
    assert!(reason.contains(PROJECT_SCOPE_CODE), "{reason}");
}

#[test]
fn the_stand_in_refuses_a_request_for_a_saved_filter_that_names_a_project_with_each_query() {
    let (_dir, client, tenant) = served_with_project(&query_documents());
    let probe = subject::Probe::new(&client, &tenant);
    for path in [
        "/v1/filters/metadata?filterType=tag",
        "/v1/filters/metadata",
        "/v1/filters/tag/f1",
    ] {
        let refused = probe.get(Scope::Client, path);
        let failure = refused.as_ref().as_ref().unwrap_err();
        assert_eq!(failure.status, Some(403), "{path}");
        assert!(failure.message.contains(PROJECT_SCOPE_CODE), "{path}");
        // the same request with no project reaches the filters
        let answered = probe.get(Scope::Tenant, path);
        assert_ne!(
            answered.as_ref().as_ref().err().and_then(|f| f.status),
            Some(403),
            "{path}"
        );
    }
}

#[test]
fn the_stand_in_refuses_a_project_on_the_route_of_each_fact_of_the_scope_tenant_and_on_no_other() {
    let facts = real_facts();
    let (_dir, client, _tenant) = served_with_project(&[]);
    let probe = subject::Probe::new(&client, &client);
    for fact in facts.iter().filter(|f| f.kind == Kind::Request) {
        let Some(template) = fact.get_path() else {
            continue;
        };
        for (_, text) in expand(template) {
            let path = fill(&text, |_| Ok::<_, ()>("x".to_string())).unwrap();
            let answer = probe.get(Scope::Client, &path);
            let refused = answer.as_ref().as_ref().is_err_and(|failure| {
                failure.status == Some(403) && failure.code.as_deref() == Some(PROJECT_SCOPE_CODE)
            });
            assert_eq!(
                refused,
                fact.scope == Scope::Tenant,
                "{}: the stand-in and the fact disagree on the scope of {path}",
                fact.id
            );
        }
    }
    // a route of the stand-in that no fact of the scope tenant names refuses a project for nothing
    for route in TENANT_ROUTES {
        assert!(
            facts
                .iter()
                .filter(|f| f.scope == Scope::Tenant)
                .any(|f| f.get_path().is_some_and(|path| path.starts_with(route))),
            "no fact of the scope tenant is under the route {route} of the stand-in"
        );
    }
}

#[test]
fn the_stand_in_answers_a_request_that_names_a_project_on_another_route_as_before() {
    let (_dir, client, _tenant) = served_with_project(&[]);
    let refused = subject::Probe::new(&client, &client).get(Scope::Client, "/v1/cloudaccounts/aws");
    let failure = refused.as_ref().as_ref().unwrap_err();
    assert_eq!(failure.status, Some(404));
}

#[test]
fn a_listing_that_holds_the_subject_of_a_fact_of_the_tenant_is_sent_with_no_project() {
    // `tag-filter` takes its uid from `tag-filter-listing`. Both are of the tenant scope, so the
    // listing is sent with the client that names no project, also when it finds a subject
    let (_dir, client, tenant) = served_with_project(&query_documents());
    let probe = subject::Probe::new(&client, &tenant);
    let found = subject::find(&probe, &real_facts(), "/v1/filters/tag/{uid}");
    assert_eq!(
        found,
        subject::Subject::Path("/v1/filters/tag/f1".to_string())
    );
}

#[test]
fn an_answer_is_kept_for_each_scope_of_the_same_path() {
    let (_dir, client, tenant) = served_with_project(&query_documents());
    let probe = subject::Probe::new(&client, &tenant);
    let path = "/v1/filters/tag/f1";
    assert!(probe.get(Scope::Client, path).is_err());
    assert!(probe.get(Scope::Tenant, path).is_ok());
}

// ---- the outcome of a fact -----------------------------------------------------------------

#[test]
fn a_tenant_that_answers_as_the_stand_in_does_gives_the_outcome_same_for_each_fact_it_can_ask() {
    let (_root, stand_in) = stand_in(&documents("Running"));
    let api = memory(&documents("Running"));
    let contract = contract(&stand_in, &api);
    assert_eq!(outcome(&contract, "cluster-listing"), Outcome::Same);
    assert_eq!(outcome(&contract, "cluster-document"), Outcome::Same);
    assert_eq!(outcome(&contract, "cloud-config"), Outcome::Same);
    assert_eq!(contract.exit_code(), 0);
}

#[test]
fn the_document_has_one_entry_for_each_fact_of_the_kind_request_and_none_for_another_kind() {
    let (_root, stand_in) = stand_in(&documents("Running"));
    let contract = contract(&stand_in, &memory(&documents("Running")));
    let ids: Vec<&str> = contract.facts.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "cluster-listing",
            "cluster-document",
            "cloud-config",
            "cluster-search",
            "edge-host-document",
            "validate-name",
            "cluster-summary",
        ]
    );
    assert_eq!(contract.schema, "strata-contract/1");
    let s = contract.summary;
    assert_eq!(s.same + s.differs + s.skipped + s.error, ids.len());
}

#[test]
fn a_fact_with_a_body_a_missing_stand_in_or_no_subject_is_skipped_with_a_reason() {
    let (_root, stand_in) = stand_in(&documents("Running"));
    let contract = contract(&stand_in, &memory(&documents("Running")));
    let reason = |id: &str| {
        contract
            .facts
            .iter()
            .find(|e| e.id == id)
            .and_then(|e| e.reason.clone())
            .unwrap_or_default()
    };
    assert_eq!(outcome(&contract, "cluster-search"), Outcome::Skipped);
    assert!(
        reason("cluster-search").contains("sends a body"),
        "{}",
        reason("cluster-search")
    );
    assert_eq!(outcome(&contract, "cluster-summary"), Outcome::Skipped);
    assert!(reason("cluster-summary").contains("no document"));
    assert_eq!(outcome(&contract, "edge-host-document"), Outcome::Skipped);
    assert!(reason("edge-host-document").contains("no document"));
}

#[test]
fn a_fact_that_reads_no_field_and_no_value_is_skipped_with_a_reason() {
    let (_root, stand_in) = stand_in(&documents("Running"));
    let contract = contract(&stand_in, &memory(&documents("Running")));
    let entry = contract
        .facts
        .iter()
        .find(|e| e.id == "validate-name")
        .unwrap();
    assert_eq!(entry.outcome, Outcome::Skipped);
    assert!(
        entry
            .reason
            .as_deref()
            .unwrap_or_default()
            .contains("reads no field")
    );
}

#[test]
fn a_skipped_fact_does_not_change_the_exit_code() {
    let (_root, stand_in) = stand_in(&documents("Running"));
    let empty = MemoryApi::new().with("/v1/dashboard/spectroclusters/meta", json!([]));
    let contract = contract(&stand_in, &empty);
    assert_eq!(outcome(&contract, "cluster-document"), Outcome::Skipped);
    assert_eq!(contract.summary.differs, 0);
    assert_eq!(contract.exit_code(), 0);
}

#[test]
fn a_run_that_checked_no_fact_exits_2_and_not_0() {
    // the stand-in serves no document, so each fact is skipped and the run sends no request
    let contract = contract(&StandIn::default(), &memory(&documents("Running")));
    assert_eq!(contract.summary.same + contract.summary.differs, 0);
    assert!(contract.summary.skipped > 0);
    assert_eq!(contract.summary.error, 0);
    assert_eq!(contract.exit_code(), 2);
}

#[test]
fn a_field_of_another_type_makes_the_fact_differ_and_the_entry_names_the_field() {
    let (_root, stand_in) = stand_in(&documents("Running"));
    let mut changed = documents("Running");
    changed[1].1["metadata"]["uid"] = json!({"id": "c1"});
    let contract = contract(&stand_in, &memory(&changed));
    assert_eq!(outcome(&contract, "cluster-document"), Outcome::Differs);
    let entry = contract
        .facts
        .iter()
        .find(|e| e.id == "cluster-document")
        .unwrap();
    let difference = &entry.differences[0];
    assert_eq!(difference.kind, DifferenceKind::Type);
    assert_eq!(difference.path, "metadata.uid");
    assert_eq!(difference.expected, ["string"]);
    assert_eq!(difference.found, ["object"]);
    assert_eq!(contract.exit_code(), 1);
}

#[test]
fn a_field_that_each_stand_in_document_has_and_no_answer_has_is_missing() {
    let (_root, stand_in) = stand_in(&documents("Running"));
    let mut changed = documents("Running");
    changed[1].1["spec"]["cloudConfigRef"] = json!({});
    let contract = contract(&stand_in, &memory(&changed));
    let entry = contract
        .facts
        .iter()
        .find(|e| e.id == "cluster-document")
        .unwrap();
    assert_eq!(entry.outcome, Outcome::Differs);
    assert_eq!(entry.differences[0].kind, DifferenceKind::Missing);
    assert_eq!(entry.differences[0].path, "spec.cloudConfigRef.uid");
}

#[test]
fn a_field_that_the_stand_in_lacks_in_one_document_can_be_missing_from_the_answer() {
    // status.fips.mode is in no document of the stand-in: nothing to compare. A field that one
    // document has and another lacks is optional, so an answer without it is the same.
    let mut with = documents("Running");
    with[1].1["metadata"]["labels"] = json!({"a": "b"});
    let root = tempfile::tempdir().unwrap();
    write_tenant(&root.path().join("one"), &documents("Running"));
    write_tenant(&root.path().join("two"), &with);
    let stand_in = StandIn::read(root.path()).unwrap();
    let contract = contract(&stand_in, &memory(&documents("Running")));
    assert_eq!(outcome(&contract, "cluster-document"), Outcome::Same);
}

#[test]
fn a_value_that_neither_the_fact_nor_the_stand_in_names_is_an_unknown_value_and_no_difference() {
    let (_root, stand_in) = stand_in(&documents("Running"));
    let contract = contract(&stand_in, &memory(&documents("Quarantined")));
    let entry = contract
        .facts
        .iter()
        .find(|e| e.id == "cluster-document")
        .unwrap();
    assert_eq!(entry.outcome, Outcome::Same);
    assert!(entry.differences.is_empty(), "{:?}", entry.differences);
    assert_eq!(
        entry.unknown_values,
        [Unknown {
            set: "status.state".to_string(),
            known: vec!["Provisioning".to_string(), "Running".to_string()],
            values: vec!["Quarantined".to_string()],
        }]
    );
    // the listing reads the same closed set, and gives the same value
    assert_eq!(contract.summary.differs, 0);
    assert_eq!(contract.summary.unknown_values, 2);
}

#[test]
fn the_exit_code_is_1_for_an_unknown_value_and_the_summary_counts_each_value() {
    let (_root, stand_in) = stand_in(&documents("Running"));
    assert_eq!(
        contract(&stand_in, &memory(&documents("Running"))).exit_code(),
        0
    );

    let mut two = documents("Running");
    two[0].1 = json!([
        {"metadata": {"name": "a", "uid": "c1"}, "spec": {"cloudType": "aws"},
         "status": {"state": "Quarantined"}},
        {"metadata": {"name": "b", "uid": "c2"}, "spec": {"cloudType": "aws"},
         "status": {"state": "Hibernating"}},
        {"metadata": {"name": "c", "uid": "c3"}, "spec": {"cloudType": "aws"},
         "status": {"state": "Hibernating"}},
    ]);
    let contract = contract(&stand_in, &memory(&two));
    let listing = contract
        .facts
        .iter()
        .find(|e| e.id == "cluster-listing")
        .unwrap();
    assert_eq!(
        listing.unknown_values[0].values,
        ["Hibernating", "Quarantined"]
    );
    assert_eq!(
        contract.summary.unknown_values, 2,
        "each value counts one time"
    );
    assert_eq!(contract.summary.differs, 0);
    assert_eq!(contract.exit_code(), 1);
}

#[test]
fn the_document_writes_the_unknown_values_of_a_fact_and_the_count_in_the_summary() {
    let (_root, stand_in) = stand_in(&documents("Running"));
    let contract = contract(&stand_in, &memory(&documents("Quarantined")));
    let document = serde_json::to_value(&contract).unwrap();
    assert_eq!(document["summary"]["unknownValues"], 2);
    let entries = document["facts"].as_array().unwrap();
    let entry = entries
        .iter()
        .find(|e| e["id"] == "cluster-document")
        .unwrap();
    assert_eq!(
        entry["unknownValues"],
        json!([{"set": "status.state", "known": ["Provisioning", "Running"],
                "values": ["Quarantined"]}])
    );
    let cloud_config = entries.iter().find(|e| e["id"] == "cloud-config").unwrap();
    assert!(
        cloud_config.get("unknownValues").is_none(),
        "{cloud_config}"
    );
}

#[test]
fn a_value_that_only_the_stand_in_names_is_not_new() {
    // The fact names two states. The stand-in has a third, and an answer with the third is not a
    // difference: the stand-in knows it.
    let (_root, stand_in) = stand_in(&documents("Importing"));
    let contract = contract(&stand_in, &memory(&documents("Importing")));
    assert_eq!(outcome(&contract, "cluster-document"), Outcome::Same);
}

// ---- the names that strata needs -----------------------------------------------------------

/// A fact for the search schema: the stand-in names `tags` and `environment`, and strata needs
/// both. The fact reads the name as a name, and not as a closed set.
const SCHEMA_FACT: &str = r#"
- id: cluster-search-schema
  kind: request
  request: GET /v1/dashboard/spectroclusters/search/schema
  reads:
    fields: ["schema.properties[].name"]
    names:
      schema.properties[].name: [tags, environment]
  stand_in: [v1/dashboard/spectroclusters/search/schema]
  live: [fleet.sh:tag-search]
  pages: [reference/palette-api.md#the-fleet]
  code: [src/api/palette/listings.rs]
"#;

/// The search schema with the properties `names`.
fn schema(names: &[&str]) -> Vec<(&'static str, Value)> {
    let properties: Vec<Value> = names
        .iter()
        .map(|n| json!({"name": n, "type": "string"}))
        .collect();
    vec![(
        "v1/dashboard/spectroclusters/search/schema",
        json!({"schema": {"properties": properties}}),
    )]
}

/// The run of `SCHEMA_FACT` against a tenant whose schema has the properties `names`.
fn schema_run(names: &[&str]) -> Contract {
    let facts: Vec<Fact> = serde_yaml::from_str(SCHEMA_FACT).expect("the fact is YAML");
    let (_root, stand_in) = stand_in(&schema(&["tags", "environment"]));
    contract_of(&facts, &stand_in, &memory(&schema(names)))
}

#[test]
fn a_name_that_the_fact_does_not_list_is_no_difference_and_no_unknown_value() {
    let contract = schema_run(&["clusterName", "tags", "environment", "cloudRegion"]);
    assert_eq!(outcome(&contract, "cluster-search-schema"), Outcome::Same);
    assert_eq!(contract.summary.unknown_values, 0);
    assert_eq!(contract.exit_code(), 0);
}

#[test]
fn a_name_that_strata_needs_and_the_answer_lacks_is_a_difference_that_lists_the_names_found() {
    let contract = schema_run(&["clusterName", "tags"]);
    let entry = &contract.facts[0];
    assert_eq!(entry.outcome, Outcome::Differs);
    let difference = &entry.differences[0];
    assert_eq!(difference.kind, DifferenceKind::Name);
    assert_eq!(difference.path, "schema.properties[].name");
    assert_eq!(difference.expected, ["environment"]);
    assert_eq!(difference.found, ["clusterName", "tags"]);
    assert_eq!(contract.exit_code(), 1);
}

#[test]
fn an_answer_with_no_value_at_the_field_of_the_names_has_nothing_to_compare() {
    let contract = schema_run(&[]);
    assert_eq!(outcome(&contract, "cluster-search-schema"), Outcome::Same);
}

#[test]
fn a_set_of_names_alone_makes_the_fact_comparable() {
    let mut facts: Vec<Fact> = serde_yaml::from_str(SCHEMA_FACT).expect("the fact is YAML");
    facts[0].reads.fields.clear();
    let (_root, stand_in) = stand_in(&schema(&["tags", "environment"]));
    let contract = contract_of(&facts, &stand_in, &memory(&schema(&["tags"])));
    assert_eq!(contract.facts[0].outcome, Outcome::Differs, "not skipped");
}

#[test]
fn a_closed_set_still_counts_a_value_it_does_not_name_as_an_unknown_value() {
    // the same schema, read as a closed set: another name is then new
    let mut facts: Vec<Fact> = serde_yaml::from_str(SCHEMA_FACT).expect("the fact is YAML");
    let names = std::mem::take(&mut facts[0].reads.names);
    facts[0].reads.values = names;
    let (_root, stand_in) = stand_in(&schema(&["tags", "environment"]));
    let contract = contract_of(
        &facts,
        &stand_in,
        &memory(&schema(&["tags", "cloudRegion"])),
    );
    assert_eq!(contract.facts[0].outcome, Outcome::Same);
    assert_eq!(contract.facts[0].unknown_values[0].values, ["cloudRegion"]);
}

#[test]
fn the_search_schema_is_a_set_of_names_and_no_set_is_both() {
    let facts = real_facts();
    let reads = |id: &str| &facts.iter().find(|f| f.id == id).unwrap().reads;
    assert!(
        reads("cluster-search-schema")
            .names
            .contains_key("schema.properties[].name")
    );
    for fact in &facts {
        for field in fact.reads.names.keys() {
            assert!(
                !fact.reads.values.contains_key(field),
                "`{}` names `{field}` as names and as a closed set",
                fact.id
            );
        }
    }
}

// ---- the kinds of the audit log ------------------------------------------------------------

/// A page of the audit log with one entry for each kind of `kinds`. Each entry holds each field
/// that the fact `audit-log` reads.
fn audit_page(kinds: &[&str]) -> Vec<(&'static str, Value)> {
    let entry = |kind: &str| {
        json!({
            "metadata": {"uid": "a1", "creationTimestamp": "2026-11-02T06:00:00Z"},
            "spec": {
                "actionType": "update",
                "actor": {"actorType": "user", "serviceName": "hubble",
                    "user": {"uid": "u1", "firstName": "Ada", "lastName": "Lovelace",
                        "emailId": "ada@example.com"}},
                "contentMsg": "a change",
                "userMsg": "a note",
                "resource": {"kind": kind, "name": "payments", "uid": "r1"},
            },
        })
    };
    let items: Vec<Value> = kinds.iter().map(|kind| entry(kind)).collect();
    vec![(
        "v1/audits",
        json!({"items": items, "listmeta": {"continue": ""}}),
    )]
}

/// The run of the fact `audit-log` against a stand-in that holds the kinds that strata acts on
/// and a tenant whose page holds the kinds `kinds`.
fn audit_run(kinds: &[&str]) -> Contract {
    let acts_on = [
        "clusterprofile",
        "appprofile",
        "spectrocluster",
        "edgehost",
        "clustergroup",
    ];
    let (_root, stand_in) = stand_in(&audit_page(&acts_on));
    let facts: Vec<Fact> = real_facts()
        .into_iter()
        .filter(|f| f.id == "audit-log")
        .collect();
    contract_of(&facts, &stand_in, &memory(&audit_page(kinds)))
}

#[test]
fn a_page_of_the_audit_log_that_holds_no_kind_that_strata_acts_on_is_no_difference() {
    // the log of the lab holds events of a profile now, and none of the app profiles, the edge
    // hosts, or the cluster groups
    let contract = audit_run(&["clusterprofile"]);
    assert_eq!(outcome(&contract, "audit-log"), Outcome::Same);
    assert!(contract.facts[0].differences.is_empty());
    assert_eq!(contract.exit_code(), 0);
}

#[test]
fn a_kind_of_the_audit_log_that_strata_does_not_act_on_is_no_difference_and_no_unknown_value() {
    // the lab holds the events of a cloud account and of a registry
    let contract = audit_run(&["clusterprofile", "cloudaccount", "registry"]);
    assert_eq!(outcome(&contract, "audit-log"), Outcome::Same);
    assert!(contract.facts[0].differences.is_empty());
    assert!(contract.facts[0].unknown_values.is_empty());
    assert_eq!(contract.exit_code(), 0);
}

#[test]
fn the_fact_of_the_audit_log_reads_the_type_of_the_kind_and_lists_no_kind() {
    let facts = real_facts();
    let reads = &facts.iter().find(|f| f.id == "audit-log").unwrap().reads;
    assert!(
        reads
            .fields
            .iter()
            .any(|f| f == "items[].spec.resource.kind")
    );
    assert!(reads.names.is_empty(), "{:?}", reads.names);
    assert!(
        !reads.values.contains_key("items[].spec.resource.kind"),
        "{:?}",
        reads.values
    );
}

#[test]
fn a_kind_of_the_audit_log_that_is_not_text_is_still_a_difference() {
    let mut page = audit_page(&["clusterprofile"]);
    page[0].1["items"][0]["spec"]["resource"]["kind"] = json!(7);
    let (_root, stand_in) = stand_in(&audit_page(&["clusterprofile"]));
    let facts: Vec<Fact> = real_facts()
        .into_iter()
        .filter(|f| f.id == "audit-log")
        .collect();
    let contract = contract_of(&facts, &stand_in, &memory(&page));
    assert_eq!(outcome(&contract, "audit-log"), Outcome::Differs);
    assert_eq!(
        contract.facts[0].differences[0].path,
        "items[].spec.resource.kind"
    );
}

// ---- the list routes of AWS ----------------------------------------------------------------

/// The documents that answer the routes behind `validate-security-groups`, with the file names of
/// the stand-in: the account, its regions, the VPCs of a region, and the groups of a VPC.
fn aws_list_documents() -> Vec<(&'static str, Value)> {
    vec![
        (
            "v1/cloudaccounts/aws",
            json!({"items": [{"metadata": {"uid": "acc1"}}]}),
        ),
        (
            "v1/clouds/aws/regions@cloudAccountUid=acc1",
            json!({"regions": [{"name": "us-east-2"}, {"name": "us-west-2"}]}),
        ),
        (
            "v1/clouds/aws/regions/us-east-2/vpcs@cloudAccountUid=acc1",
            json!({"vpcs": [{"vpcId": "vpc-1",
                "subnets": [{"subnetId": "subnet-1", "az": "us-east-2a"}]}]}),
        ),
        (
            "v1/clouds/aws/securitygroups@cloudAccountUid=acc1&region=us-east-2&vpcId=vpc-1",
            json!({"groups": [{"groupId": "sg-1", "groupName": "default", "ownerId": "1"}]}),
        ),
    ]
}

#[test]
fn the_security_groups_are_asked_with_the_account_the_region_and_the_vpc_of_the_tenant() {
    let docs = aws_list_documents();
    let (_root, stand_in) = stand_in(&docs);
    let (_dir, api) = served(&docs);
    let contract = contract_of(&real_facts(), &stand_in, &api);
    // the stand-in serves the groups only with the three values of the query, so the outcome
    // `same` shows that the run sent all of them
    for id in ["validate-security-groups", "validate-vpcs"] {
        let entry = contract.facts.iter().find(|e| e.id == id).unwrap();
        assert_eq!(entry.outcome, Outcome::Same, "{id}: {entry:?}");
    }
    assert_eq!(contract.exit_code(), 0);
}

#[test]
fn the_subject_of_the_security_groups_is_the_first_account_region_and_vpc() {
    let api = MemoryApi::new()
        .with(
            "/v1/cloudaccounts/aws",
            json!({"items": [{"metadata": {"uid": "acc1"}}]}),
        )
        .with(
            "/v1/clouds/aws/regions?cloudAccountUid=acc1",
            json!({"regions": [{"name": "us-east-2"}, {"name": "us-west-2"}]}),
        )
        .with(
            "/v1/clouds/aws/regions/us-east-2/vpcs?cloudAccountUid=acc1",
            json!({"vpcs": [{"vpcId": "vpc-1"}, {"vpcId": "vpc-2"}]}),
        );
    let probe = subject::Probe::new(&api, &api);
    let facts = real_facts();
    let template = request_of(&facts, "validate-security-groups").unwrap();
    assert_eq!(
        subject::find(&probe, &facts, template),
        subject::Subject::Path(
            "/v1/clouds/aws/securitygroups?cloudAccountUid=acc1&region=us-east-2&vpcId=vpc-1"
                .to_string()
        )
    );
}

#[test]
fn a_region_with_no_vpc_has_no_subject_for_the_security_groups_and_the_reason_says_why() {
    let api = MemoryApi::new()
        .with(
            "/v1/cloudaccounts/aws",
            json!({"items": [{"metadata": {"uid": "acc1"}}]}),
        )
        .with(
            "/v1/clouds/aws/regions?cloudAccountUid=acc1",
            json!({"regions": [{"name": "us-east-2"}]}),
        )
        .with(
            "/v1/clouds/aws/regions/us-east-2/vpcs?cloudAccountUid=acc1",
            json!({"vpcs": []}),
        );
    let probe = subject::Probe::new(&api, &api);
    let facts = real_facts();
    let template = request_of(&facts, "validate-security-groups").unwrap();
    match subject::find(&probe, &facts, template) {
        subject::Subject::Missing(why) => {
            assert!(why.contains("{vpcId}"), "{why}");
            assert!(why.contains("/vpcs/0/vpcId"), "{why}");
        }
        other => panic!("expected no subject, got {other:?}"),
    }
}

/// The documents behind `validate-key-pairs`: the account, its regions, and the key pairs of the
/// first region. The stand-in serves the key pairs only with the account in the query.
fn key_pair_documents() -> Vec<(&'static str, Value)> {
    let mut docs: Vec<_> = aws_list_documents().into_iter().take(2).collect();
    docs.push((
        "v1/clouds/aws/regions/us-east-2/keypairs@cloudAccountUid=acc1",
        json!({"keyNames": ["strata-lab"]}),
    ));
    docs
}

#[test]
fn the_key_pairs_are_asked_with_the_account_of_the_tenant() {
    let docs = key_pair_documents();
    let (_root, stand_in) = stand_in(&docs);
    let (_dir, api) = served(&docs);
    let contract = contract_of(&real_facts(), &stand_in, &api);
    // the stand-in answers the route only with `cloudAccountUid`, as Palette answers HTTP 422 with
    // no query, so the outcome `same` shows that the run sent it
    let entry = contract
        .facts
        .iter()
        .find(|e| e.id == "validate-key-pairs")
        .unwrap();
    assert_eq!(entry.outcome, Outcome::Same, "{entry:?}");
    assert_eq!(
        entry.request,
        "GET /v1/clouds/aws/regions/{region}/keypairs?cloudAccountUid={cloudAccountUid}"
    );
}

#[test]
fn the_stand_in_serves_the_key_pairs_only_with_the_account() {
    let (_dir, api) = served(&key_pair_documents());
    let probe = subject::Probe::new(&api, &api);
    let bare = probe.get(Scope::Client, "/v1/clouds/aws/regions/us-east-2/keypairs");
    assert_eq!(bare.as_ref().as_ref().unwrap_err().status, Some(404));
    let asked = probe.get(
        Scope::Client,
        "/v1/clouds/aws/regions/us-east-2/keypairs?cloudAccountUid=acc1",
    );
    assert!(asked.is_ok());
}

#[test]
fn a_tenant_with_no_aws_account_has_no_subject_for_the_key_pairs_and_the_reason_says_why() {
    let api = MemoryApi::new().with("/v1/cloudaccounts/aws", json!({"items": []}));
    let probe = subject::Probe::new(&api, &api);
    let facts = real_facts();
    let template = request_of(&facts, "validate-key-pairs").unwrap();
    match subject::find(&probe, &facts, template) {
        subject::Subject::Missing(why) => {
            assert!(why.contains("{cloudAccountUid}"), "{why}");
        }
        other => panic!("expected no subject, got {other:?}"),
    }
}

#[test]
fn a_status_that_is_not_2xx_makes_the_fact_differ() {
    let (_root, stand_in) = stand_in(&documents("Running"));
    let api = memory(&documents("Running")).failing("/v1/spectroclusters/c1", 403, "{}");
    let contract = contract(&stand_in, &api);
    let entry = contract
        .facts
        .iter()
        .find(|e| e.id == "cluster-document")
        .unwrap();
    assert_eq!(entry.outcome, Outcome::Differs);
    assert_eq!(entry.differences[0].kind, DifferenceKind::Status);
    assert_eq!(entry.differences[0].expected, ["200"]);
    assert_eq!(entry.differences[0].found, ["403"]);
}

#[test]
fn a_listing_that_fails_makes_the_fact_that_needs_it_an_error_and_the_exit_code_2() {
    let (_root, stand_in) = stand_in(&documents("Running"));
    let api = MemoryApi::new().failing("/v1/dashboard/spectroclusters/meta", 500, "{}");
    let contract = contract(&stand_in, &api);
    assert_eq!(outcome(&contract, "cluster-document"), Outcome::Error);
    assert_eq!(contract.exit_code(), 2);
}

#[test]
fn an_error_wins_over_a_difference_and_an_unknown_value_in_the_exit_code() {
    let (_root, stand_in) = stand_in(&documents("Running"));
    let mut changed = documents("Quarantined");
    changed[1].1["metadata"]["uid"] = json!({"id": "c1"});
    let api = memory(&changed).failing("/v1/cloudconfigs/aws/cfg1", 500, "{}");
    let contract = contract(&stand_in, &api);
    assert_eq!(contract.summary.differs, 1);
    assert!(contract.summary.unknown_values > 0);
    assert_eq!(contract.exit_code(), 2);
}

// ---- the run against a served stand-in -----------------------------------------------------

#[test]
fn the_stand_in_against_itself_exits_0() {
    let (_root, stand_in) = stand_in(&documents("Running"));
    let (_dir, api) = served(&documents("Running"));
    let contract = contract(&stand_in, &api);
    assert_eq!(contract.summary.same, 3, "{:?}", contract.facts);
    assert_eq!(contract.summary.differs, 0);
    assert_eq!(contract.exit_code(), 0);
}

#[test]
fn a_copy_of_the_stand_in_with_one_changed_field_exits_1_and_names_the_fact() {
    let (_root, stand_in) = stand_in(&documents("Running"));
    let mut changed = documents("Running");
    changed[0].1[0]["spec"]["cloudType"] = json!(["aws"]);
    let (_dir, api) = served(&changed);
    let contract = contract(&stand_in, &api);
    assert_eq!(contract.exit_code(), 1);
    let differing: Vec<&str> = contract
        .facts
        .iter()
        .filter(|e| e.outcome == Outcome::Differs)
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(differing, ["cluster-listing"]);
}

#[test]
fn a_tenant_that_refuses_a_document_the_stand_in_serves_exits_1_with_the_status() {
    let (_root, stand_in) = stand_in(&documents("Running"));
    let (dir, api) = served(&documents("Running"));
    // the stand-in server answers 403 when the file of the refusal is there and the file of the
    // document is not
    let route = dir.path().join("tenant/v1/cloudconfigs/aws");
    std::fs::rename(route.join("cfg1.json"), route.join("cfg1.403.json")).unwrap();
    let contract = contract(&stand_in, &api);
    let entry = contract
        .facts
        .iter()
        .find(|e| e.id == "cloud-config")
        .unwrap();
    assert_eq!(entry.differences[0].kind, DifferenceKind::Status);
    assert_eq!(entry.differences[0].found, ["403"]);
    assert_eq!(contract.exit_code(), 1);
}

#[test]
fn the_stand_in_omits_a_file_of_a_refusal_and_a_file_that_is_no_json() {
    let root = tempfile::tempdir().unwrap();
    let chapter = root.path().join("chapter");
    write_tenant(&chapter, &documents("Running"));
    std::fs::write(
        chapter.join("tenant/v1/spectroclusters/c1.403.json"),
        r#"{"code":"x"}"#,
    )
    .unwrap();
    std::fs::write(
        chapter.join("tenant/v1/spectroclusters/c2.json"),
        "not json",
    )
    .unwrap();
    let stand_in = StandIn::read(root.path()).unwrap();
    let found = stand_in.documents(&["v1/spectroclusters/{uid}".to_string()]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["metadata"]["uid"], "c1");
}

#[test]
fn a_stand_in_directory_that_is_not_there_is_an_error_and_not_an_empty_stand_in() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("no/such/dir");
    let error = StandIn::read(&missing).unwrap_err();
    assert!(error.contains(missing.to_str().unwrap()), "{error}");
}

#[test]
fn a_stand_in_directory_that_holds_no_document_is_an_error() {
    let root = tempfile::tempdir().unwrap();
    let error = StandIn::read(root.path()).unwrap_err();
    assert!(error.contains("the stand-in holds no document"), "{error}");

    // a chapter whose only file is no JSON gives no document either
    let tenant = root.path().join("chapter/tenant/v1");
    std::fs::create_dir_all(&tenant).unwrap();
    std::fs::write(tenant.join("clusters.json"), "not json").unwrap();
    assert!(StandIn::read(root.path()).is_err());
}

#[test]
fn the_command_stops_at_a_stand_in_that_is_not_there_before_it_sends_a_request() {
    use super::command::{Options, contract as run_command};
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("no/such/dir");
    let missing = missing.to_str().unwrap();
    let error = run_command(&Options::parse(&["--stand-in", missing]).unwrap()).unwrap_err();
    assert!(error.contains(missing), "{error}");

    let empty = root.path().to_str().unwrap();
    let error = run_command(&Options::parse(&["--stand-in", empty]).unwrap()).unwrap_err();
    assert!(error.contains("the stand-in holds no document"), "{error}");
}

// ---- the document ----------------------------------------------------------------------------

#[test]
fn the_document_names_the_schema_the_target_and_the_live_checks_of_each_fact() {
    let (_root, stand_in) = stand_in(&documents("Running"));
    let contract = contract(&stand_in, &memory(&documents("Running")));
    let document = serde_json::to_value(&contract).unwrap();
    assert_eq!(document["schema"], "strata-contract/1");
    assert_eq!(document["target"], "https://tenant.example");
    assert_eq!(document["checkedAt"], "2026-11-02T06:00:00Z");
    assert_eq!(document["summary"]["same"], 3);
    assert_eq!(document["facts"][0]["id"], "cluster-listing");
    assert_eq!(document["facts"][0]["outcome"], "same");
    assert_eq!(
        document["facts"][0]["live"],
        json!(["verify.sh:snapshot-cluster"])
    );
    assert!(document["facts"][0].get("differences").is_none());
    assert_eq!(document["releases"], json!([]));
}

// ---- the releases ----------------------------------------------------------------------------

/// A server of the GitHub API that holds the latest release of each repository.
fn github(tags: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let tenant = Tenant::new(dir.path(), Path::new("/nowhere"));
    for (repo, tag) in tags {
        let release = json!({"tag_name": tag, "published_at": "2026-10-29T09:20:20Z"});
        tenant.put(TENANT, &format!("repos/{repo}/releases/latest"), &release);
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let root = dir.path().to_path_buf();
    std::thread::spawn(move || serve_on(&root, &listener));
    (dir, url)
}

const BOTH: [(&str, &str); 2] = [
    ("spectrocloud/palette-sdk-go", "v4.10.b"),
    ("spectrocloud/terraform-provider-spectrocloud", "v0.30.4"),
];

#[test]
fn the_first_run_keeps_the_latest_tag_of_each_repository_and_notes_no_new_release() {
    let (_server, url) = github(&BOTH);
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state.json");
    let found = releases::read(&url, &state).unwrap();
    assert_eq!(found.len(), 2);
    assert!(
        found
            .iter()
            .all(|r| !r.new && r.seen.is_none() && r.error.is_none())
    );
    assert_eq!(found[0].tag.as_deref(), Some("v4.10.b"));
    assert_eq!(
        found[0].published_at.as_deref(),
        Some("2026-10-29T09:20:20Z")
    );
    let kept: Value = serde_json::from_str(&std::fs::read_to_string(&state).unwrap()).unwrap();
    assert_eq!(kept["schema"], "strata-contract-state/1");
    assert_eq!(kept["releases"]["spectrocloud/palette-sdk-go"], "v4.10.b");
}

#[test]
fn a_tag_that_is_not_the_tag_of_the_last_run_is_new_once() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state.json");
    let (_old, old) = github(&BOTH);
    releases::read(&old, &state).unwrap();
    let (_new, new) = github(&[
        ("spectrocloud/palette-sdk-go", "v4.10.b"),
        ("spectrocloud/terraform-provider-spectrocloud", "v0.30.5"),
    ]);
    let found = releases::read(&new, &state).unwrap();
    let flags: Vec<bool> = found.iter().map(|r| r.new).collect();
    assert_eq!(flags, [false, true]);
    assert_eq!(found[1].seen.as_deref(), Some("v0.30.4"));
    let again = releases::read(&new, &state).unwrap();
    assert!(
        again.iter().all(|r| !r.new),
        "the next run has seen the tag"
    );
}

#[test]
fn a_release_that_cannot_be_read_is_an_error_and_keeps_the_old_tag() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state.json");
    let (_old, old) = github(&BOTH);
    releases::read(&old, &state).unwrap();
    let (_partial, partial) = github(&[("spectrocloud/palette-sdk-go", "v4.11.0")]);
    let found = releases::read(&partial, &state).unwrap();
    assert!(found[0].new);
    let failed = &found[1];
    assert!(
        failed
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("HTTP 404"),
        "{failed:?}"
    );
    assert!(!failed.new);
    let kept: Value = serde_json::from_str(&std::fs::read_to_string(&state).unwrap()).unwrap();
    assert_eq!(
        kept["releases"]["spectrocloud/terraform-provider-spectrocloud"],
        "v0.30.4"
    );
    let (_empty, empty) = github(&[]);
    let contract = Contract {
        releases: releases::read(&empty, &state).unwrap(),
        ..run(
            &[],
            &StandIn::default(),
            &MemoryApi::new(),
            &MemoryApi::new(),
            "t",
            "now",
        )
    };
    assert_eq!(contract.exit_code(), 2);
}

#[test]
fn a_state_file_of_another_schema_is_refused_before_any_request() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state.json");
    std::fs::write(&state, r#"{"schema": "other/1", "releases": {}}"#).unwrap();
    let error = releases::read("http://127.0.0.1:1", &state).unwrap_err();
    assert!(error.contains("strata-contract-state/1"), "{error}");
}

// ---- the types -------------------------------------------------------------------------------

#[test]
fn the_names_of_the_types_are_the_names_that_the_document_writes() {
    let names: BTreeSet<String> = [
        JsonType::Absent,
        JsonType::Boolean,
        JsonType::Number,
        JsonType::String,
        JsonType::Array,
        JsonType::Object,
    ]
    .iter()
    .map(|t| t.name())
    .collect();
    assert_eq!(
        names.into_iter().collect::<Vec<_>>(),
        ["absent", "array", "boolean", "number", "object", "string"]
    );
}

#[test]
fn a_refused_key_a_limit_of_the_rate_and_a_fault_of_the_server_are_not_a_difference() {
    let (_root, stand_in) = stand_in(&documents("Running"));
    for status in [401, 429, 500, 503] {
        let api = memory(&documents("Running")).failing("/v1/spectroclusters/c1", status, "{}");
        let contract = contract(&stand_in, &api);
        assert_eq!(
            outcome(&contract, "cluster-document"),
            Outcome::Error,
            "HTTP {status}"
        );
        assert_eq!(contract.exit_code(), 2, "HTTP {status}");
    }
    for status in [400, 403, 404] {
        let api = memory(&documents("Running")).failing("/v1/spectroclusters/c1", status, "{}");
        let contract = contract(&stand_in, &api);
        assert_eq!(
            outcome(&contract, "cluster-document"),
            Outcome::Differs,
            "HTTP {status}"
        );
    }
}

// ---- the object that can be absent ---------------------------------------------------------

/// The code that Palette gives for an object that the tenant lacks.
const ABSENT: &str = "ResourceNotFound";

/// The small list, with the fact `id` naming `ResourceNotFound` as its absent code.
fn facts_naming_the_absent_code(id: &str) -> Vec<Fact> {
    let mut facts = facts();
    let named = facts.iter_mut().find(|f| f.id == id).unwrap();
    named.absent_codes = vec![ABSENT.to_string()];
    facts
}

/// The entry of the fact `cluster-document` when the tenant answers its route with `status` and
/// `body`.
fn document_entry(facts: &[Fact], status: u16, body: &str) -> super::document::Entry {
    let (_root, stand_in) = stand_in(&documents("Running"));
    let api = memory(&documents("Running")).failing("/v1/spectroclusters/c1", status, body);
    contract_of(facts, &stand_in, &api)
        .facts
        .into_iter()
        .find(|e| e.id == "cluster-document")
        .unwrap()
}

#[test]
fn a_404_with_a_code_that_the_fact_names_is_skipped_with_the_code_in_its_reason() {
    let body = format!(r#"{{"code":"{ABSENT}","message":"Resource not found for the type 'x'"}}"#);
    let entry = document_entry(
        &facts_naming_the_absent_code("cluster-document"),
        404,
        &body,
    );
    assert_eq!(entry.outcome, Outcome::Skipped);
    assert!(entry.differences.is_empty());
    let reason = entry.reason.unwrap_or_default();
    assert!(reason.contains(&format!("`{ABSENT}`")), "{reason}");
    assert!(reason.contains("HTTP 404"), "{reason}");
}

#[test]
fn a_404_with_another_code_or_with_no_code_differs_when_the_fact_names_a_code() {
    let facts = facts_naming_the_absent_code("cluster-document");
    for body in [
        r#"{"code":"ClusterNotFound","message":"m"}"#,
        r#"{"code":404}"#,
        "{}",
        "<html>404</html>",
        "",
    ] {
        let entry = document_entry(&facts, 404, body);
        assert_eq!(entry.outcome, Outcome::Differs, "{body}");
        assert_eq!(entry.differences[0].kind, DifferenceKind::Status, "{body}");
        assert_eq!(entry.differences[0].found, ["404"], "{body}");
    }
}

#[test]
fn a_fact_that_names_no_code_keeps_the_rule_for_a_404_with_the_code() {
    let body = format!(r#"{{"code":"{ABSENT}"}}"#);
    let entry = document_entry(&facts(), 404, &body);
    assert_eq!(entry.outcome, Outcome::Differs);
    assert_eq!(entry.differences[0].found, ["404"]);
}

#[test]
fn the_code_that_the_fact_names_does_not_change_the_outcome_of_another_status() {
    let facts = facts_naming_the_absent_code("cluster-document");
    let body = format!(r#"{{"code":"{ABSENT}"}}"#);
    for (status, outcome) in [
        (400, Outcome::Differs),
        (403, Outcome::Differs),
        (401, Outcome::Error),
        (429, Outcome::Error),
        (500, Outcome::Error),
    ] {
        assert_eq!(
            document_entry(&facts, status, &body).outcome,
            outcome,
            "HTTP {status}"
        );
    }
}

#[test]
fn a_skipped_fact_for_an_absent_object_leaves_the_exit_code_at_0() {
    // no other fact reads the cloud config, so the run checks each other fact as before
    let (_root, stand_in) = stand_in(&documents("Running"));
    let body = format!(r#"{{"code":"{ABSENT}"}}"#);
    let api = memory(&documents("Running")).failing("/v1/cloudconfigs/aws/cfg1", 404, &body);
    let contract = contract_of(
        &facts_naming_the_absent_code("cloud-config"),
        &stand_in,
        &api,
    );
    assert_eq!(outcome(&contract, "cloud-config"), Outcome::Skipped);
    assert_eq!(contract.summary.differs, 0);
    assert_eq!(contract.exit_code(), 0);
}

#[test]
fn the_fact_of_the_repave_record_names_the_code_for_an_object_that_is_absent() {
    let fact = real_facts()
        .into_iter()
        .find(|f| f.id == "cluster-repave-status")
        .unwrap();
    assert_eq!(fact.absent_codes, [ABSENT]);
}

#[test]
fn a_cluster_that_had_no_repave_does_not_make_the_fact_of_the_repave_record_differ() {
    // the tenant serves the listing and the document of the cluster, and no repave record: the
    // memory tenant answers HTTP 404 with the code `ResourceNotFound` for it, as Palette does
    let mut docs = documents("Running");
    docs.push((
        "v1/spectroclusters/c1/repave/status",
        json!({"spec": {"source": "hubble"}, "status": {"repaveTransitionTime": "2026-11-01T00:00:00Z"}}),
    ));
    let (_root, stand_in) = stand_in(&docs);
    let contract = contract_of(&real_facts(), &stand_in, &memory(&documents("Running")));
    let entry = contract
        .facts
        .iter()
        .find(|e| e.id == "cluster-repave-status")
        .unwrap();
    assert_eq!(entry.outcome, Outcome::Skipped);
    let reason = entry.reason.as_deref().unwrap_or_default();
    assert!(reason.contains(ABSENT), "{reason}");
}

#[test]
fn a_failure_reads_the_code_of_the_body_when_the_body_is_json_with_a_code() {
    let (_dir, client, tenant) = served_with_project(&[]);
    let probe = subject::Probe::new(&client, &tenant);
    let failure = probe.get(Scope::Client, "/v1/nothing");
    let failure = failure.as_ref().as_ref().unwrap_err();
    assert_eq!(failure.status, Some(404));
    assert_eq!(failure.code.as_deref(), Some(ABSENT));
    let named = [ABSENT.to_string()];
    assert_eq!(failure.absent(&named), Some(ABSENT));
    assert_eq!(failure.absent(&[]), None);
}

// ---- the harvest of the book -----------------------------------------------------------------

/// The stand-in that the harvest of the book builds: the tenant of every chapter. It needs the
/// offline examples and the xtask binary, which `just examples` and `just stand-in` build. Without
/// them it has nothing to harvest, and a test that needs it returns.
fn harvested() -> Option<StandIn> {
    let repo = crate::repository();
    let xtask = repo.join("target/debug/xtask");
    if crate::book::built_examples(&repo).is_err() || !xtask.is_file() {
        return None;
    }
    let into = tempfile::tempdir().unwrap();
    super::stand_in::harvest(&repo, &xtask, into.path()).unwrap();
    Some(StandIn::read(into.path()).expect("the harvest wrote the tenant of each chapter"))
}

/// The harvest gives the stand-in a document for each route of a fact.
#[test]
fn the_harvest_of_the_book_gives_the_stand_in_a_document_for_each_route_of_a_fact() {
    let Some(stand_in) = harvested() else {
        return;
    };
    let facts = real_facts();
    let without: Vec<&str> = facts
        .iter()
        .filter(|f| f.kind == Kind::Request && !f.stand_in.is_empty())
        .filter(|f| f.reads.reads_a_document())
        .filter(|f| stand_in.documents(&f.stand_in).is_empty())
        .map(|f| f.id.as_str())
        .collect();
    assert!(
        without.is_empty(),
        "no chapter of the book ends with a document for the routes of these facts: {without:?}"
    );
}

// ---- the fields that the stand-in holds ------------------------------------------------------

#[test]
fn a_field_that_a_fact_reads_and_no_document_of_the_stand_in_holds_is_reported() {
    // the documents of the small tenant have no `status.fips.mode`
    let (_root, stand_in) = stand_in(&documents("Running"));
    assert_eq!(
        stand_in.unheld_fields(&facts()),
        ["fact `cluster-document`: no document of the stand-in holds the field `status.fips.mode`"]
    );
}

#[test]
fn a_document_that_holds_the_field_with_a_value_ends_the_report() {
    let mut docs = documents("Running");
    docs[1].1["status"]["fips"] = json!({"mode": "enabled"});
    let (_root, stand_in) = stand_in(&docs);
    assert_eq!(stand_in.unheld_fields(&facts()), Vec::<String>::new());
}

#[test]
fn a_field_that_is_null_or_in_a_list_with_no_entry_is_not_held() {
    let mut docs = documents("Running");
    docs[1].1["status"]["fips"] = json!({"mode": null});
    docs[1].1["spec"]["clusterProfileTemplates"] = json!([]);
    let (_root, stand_in) = stand_in(&docs);
    let bad = stand_in.unheld_fields(&facts());
    assert_eq!(bad.len(), 2, "{bad:?}");
    assert!(
        bad.iter().any(|m| m.contains("`status.fips.mode`")),
        "{bad:?}"
    );
    assert!(
        bad.iter()
            .any(|m| m.contains("`spec.clusterProfileTemplates[].name`")),
        "{bad:?}"
    );
}

#[test]
fn a_post_fact_and_a_fact_with_no_document_of_the_stand_in_are_not_reported() {
    // the search sends a body, so the run never compares it; the host and the summary have no
    // document in the stand-in. Their fields are in no document, and no message follows.
    let (_root, stand_in) = stand_in(&documents("Running"));
    let bad = stand_in.unheld_fields(&facts());
    for id in ["cluster-search", "edge-host-document", "cluster-summary"] {
        assert!(!bad.iter().any(|m| m.contains(id)), "{id}: {bad:?}");
    }
}

/// The types at `field` in the documents that the stand-in has for the fact `id`.
fn held_types(stand_in: &StandIn, facts: &[Fact], id: &str, field: &str) -> Vec<JsonType> {
    let fact = facts.iter().find(|f| f.id == id).expect("a fact");
    let path = Field::parse(field);
    let types: BTreeSet<JsonType> = stand_in
        .documents(&fact.stand_in)
        .iter()
        .flat_map(|d| path.types(d))
        .collect();
    types.into_iter().collect()
}

/// The eight fields that a tenant gave as a string and that the stand-in never served (#530), and
/// the registry whose sync recovered with no message.
#[test]
fn the_stand_in_of_the_book_serves_the_eight_fields_with_a_string_and_a_registry_with_no_message() {
    let Some(stand_in) = harvested() else {
        return;
    };
    let facts = real_facts();
    for (id, field) in [
        ("app-profile-document", "metadata.lastModifiedTimestamp"),
        ("app-profile-tier-manifest", "spec.published.digest"),
        ("oci-registry-summary", "items[].spec.providerType"),
        ("audit-log", "items[].spec.actor.serviceName"),
        ("audit-log", "items[].spec.contentMsg"),
        ("profile-document", "metadata.annotations.projectUid"),
        ("profile-document", "spec.published.packs[].manifests[].uid"),
        ("profile-variables", "variables[].description"),
    ] {
        let types = held_types(&stand_in, &facts, id, field);
        assert!(types.contains(&JsonType::String), "{id} {field}: {types:?}");
    }
    // a registry with `syncRecovery` and no `message` is there beside one with a message
    let message = "items[].status.sync.syncRecovery.message";
    assert_eq!(
        held_types(&stand_in, &facts, "oci-registry-summary", message),
        [JsonType::Absent, JsonType::String]
    );
}

/// The fields that the API definition of Palette gives as a list, an object, or a boolean. A string
/// there would make the next contract run report a false `type` difference.
#[test]
fn the_stand_in_of_the_book_serves_a_list_an_object_or_a_boolean_where_palette_does() {
    let Some(stand_in) = harvested() else {
        return;
    };
    let facts = real_facts();
    for (id, field, expected) in [
        ("cluster-listing", "spec.archType", JsonType::Array),
        ("cluster-listing", "spec.tags", JsonType::Array),
        ("cluster-listing", "spec.location", JsonType::Object),
        (
            "cluster-group-document",
            "status.isActive",
            JsonType::Boolean,
        ),
        (
            "oci-registry-summary",
            "items[].status.sync.isSyncSupported",
            JsonType::Boolean,
        ),
        (
            "profile-variables",
            "variables[].required",
            JsonType::Boolean,
        ),
    ] {
        let types: Vec<_> = held_types(&stand_in, &facts, id, field)
            .into_iter()
            .filter(|t| *t != JsonType::Absent)
            .collect();
        assert_eq!(types, [expected], "{id} {field}");
    }
}

/// The stand-in of the book holds each field that a fact reads.
#[test]
fn the_stand_in_of_the_book_holds_each_field_that_a_fact_reads() {
    let Some(stand_in) = harvested() else {
        return;
    };
    let facts = real_facts();
    let bad = stand_in.unheld_fields(&facts);
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

// ---- the command line ------------------------------------------------------------------------

#[test]
fn the_options_of_the_command_line_have_defaults_and_each_takes_a_value() {
    use super::command::Options;
    let options = Options::parse(&[]).unwrap();
    assert!(options.stand_in.is_none() && options.out.is_none() && options.state.is_none());
    assert_eq!(options.github, releases::GITHUB);

    let options = Options::parse(&[
        "--stand-in",
        "/h",
        "--out",
        "c.json",
        "--state",
        "s.json",
        "--github",
        "http://127.0.0.1:9/",
    ])
    .unwrap();
    assert_eq!(options.stand_in.as_deref(), Some(Path::new("/h")));
    assert_eq!(options.out.as_deref(), Some(Path::new("c.json")));
    assert_eq!(options.state.as_deref(), Some(Path::new("s.json")));
    assert_eq!(options.github, "http://127.0.0.1:9", "no slash at the end");
}

#[test]
fn an_option_that_the_command_does_not_have_and_an_option_with_no_value_are_refused() {
    use super::command::Options;
    assert_eq!(
        Options::parse(&["--tenant"]).err().as_deref(),
        Some("unknown option --tenant")
    );
    assert_eq!(
        Options::parse(&["--state"]).err().as_deref(),
        Some("--state needs a value")
    );
}

// ---- the values of a profile --------------------------------------------------------------

/// The documents of a tenant with one profile: its listing, and the profile with the type `kind`
/// and one pack from a registry with the provider `provider`.
fn profile_documents(kind: &str, provider: &str) -> Vec<(&'static str, Value)> {
    vec![
        (
            "v1/dashboard/clusterprofiles/metadata",
            json!({"items": [{"metadata": {"uid": "p1", "name": "stack"}}]}),
        ),
        (
            "v1/clusterprofiles/p1",
            json!({
                "metadata": {"uid": "p1"},
                "spec": {"published": {"type": kind, "packs": [
                    {"name": "chart", "annotations": {"ociRegistryProviderType": provider}}]}},
            }),
        ),
    ]
}

/// The entry of the fact `profile-document` when the stand-in holds a profile of the type
/// `add-on` with the provider `zarf` and the tenant holds `tenant`.
fn profile_entry(tenant: &[(&str, Value)]) -> super::document::Entry {
    let (_root, stand_in) = stand_in(&profile_documents("add-on", "zarf"));
    let contract = contract_of(&real_facts(), &stand_in, &memory(tenant));
    contract
        .facts
        .into_iter()
        .find(|e| e.id == "profile-document")
        .expect("an entry for the profile")
}

#[test]
fn a_profile_type_and_a_provider_that_strata_does_not_know_are_unknown_values() {
    let entry = profile_entry(&profile_documents("edge-addon", "oras"));
    assert_eq!(entry.outcome, Outcome::Same, "{:?}", entry.differences);
    let found: Vec<(&str, &[String])> = entry
        .unknown_values
        .iter()
        .map(|u| (u.set.as_str(), u.values.as_slice()))
        .collect();
    assert_eq!(
        found,
        [
            (
                "spec.published.packs[].annotations.ociRegistryProviderType",
                ["oras".to_string()].as_slice()
            ),
            ("spec.published.type", ["edge-addon".to_string()].as_slice()),
        ]
    );
}

#[test]
fn a_profile_type_and_a_provider_that_the_fact_names_are_no_unknown_values() {
    // the stand-in holds `add-on` and `zarf`; the fact names the other values of the sets
    let entry = profile_entry(&profile_documents("infra", "helm"));
    assert_eq!(entry.outcome, Outcome::Same);
    assert!(
        entry.unknown_values.is_empty(),
        "{:?}",
        entry.unknown_values
    );
}

// ---- the regions, the packs, and the alternatives of a request ------------------------------

/// A tenant with one AWS account and the regions `regions`, with the VPCs of each region.
fn regions_with(regions: &[(&str, Value)]) -> MemoryApi {
    let names: Vec<Value> = regions
        .iter()
        .map(|(name, _)| json!({"name": name}))
        .collect();
    regions.iter().fold(
        MemoryApi::new()
            .with(
                "/v1/cloudaccounts/aws",
                json!({"items": [{"metadata": {"uid": "acc1"}}]}),
            )
            .with(
                "/v1/clouds/aws/regions?cloudAccountUid=acc1",
                json!({"regions": names}),
            ),
        |api, (name, vpcs)| {
            api.with(
                format!("/v1/clouds/aws/regions/{name}/vpcs?cloudAccountUid=acc1"),
                vpcs.clone(),
            )
        },
    )
}

#[test]
fn the_security_groups_take_the_first_region_that_holds_a_vpc() {
    let api = regions_with(&[
        ("ap-southeast-1", json!({"vpcs": []})),
        (
            "us-east-2",
            json!({"vpcs": [{"vpcId": "vpc-7"}, {"vpcId": "vpc-8"}]}),
        ),
        ("us-west-2", json!({"vpcs": [{"vpcId": "vpc-9"}]})),
    ]);
    let probe = subject::Probe::new(&api, &api);
    let facts = real_facts();
    let template = request_of(&facts, "validate-security-groups").unwrap();
    // the region of the groups is the region of the VPC, and not the first region of the account
    assert_eq!(
        subject::find(&probe, &facts, template),
        subject::Subject::Path(
            "/v1/clouds/aws/securitygroups?cloudAccountUid=acc1&region=us-east-2&vpcId=vpc-7"
                .to_string()
        )
    );
    // the regions after the first that holds a VPC stay unasked
    assert_eq!(
        api.calls("/v1/clouds/aws/regions/us-west-2/vpcs?cloudAccountUid=acc1"),
        0
    );
}

#[test]
fn the_reason_says_that_the_run_tried_each_region_when_no_region_holds_a_vpc() {
    let api = regions_with(&[
        ("us-east-2", json!({"vpcs": []})),
        ("us-west-2", json!({"vpcs": []})),
    ]);
    let probe = subject::Probe::new(&api, &api);
    let facts = real_facts();
    let template = request_of(&facts, "validate-security-groups").unwrap();
    match subject::find(&probe, &facts, template) {
        subject::Subject::Missing(why) => {
            assert!(why.contains("{vpcId}"), "{why}");
            assert!(why.contains("us-east-2/vpcs"), "{why}");
            assert!(
                why.contains("the run tried 2 values for `{region}`"),
                "{why}"
            );
        }
        other => panic!("expected no subject, got {other:?}"),
    }
    assert_eq!(
        api.calls("/v1/clouds/aws/regions/us-west-2/vpcs?cloudAccountUid=acc1"),
        1
    );
}

#[test]
fn a_failed_request_for_the_vpcs_of_a_region_stops_the_search() {
    let api = regions_with(&[
        ("us-east-2", json!({"vpcs": []})),
        ("us-west-2", json!({"vpcs": [{"vpcId": "vpc-9"}]})),
    ])
    .failing(
        "/v1/clouds/aws/regions/us-east-2/vpcs?cloudAccountUid=acc1",
        500,
        "{}",
    );
    let probe = subject::Probe::new(&api, &api);
    let facts = real_facts();
    let template = request_of(&facts, "validate-security-groups").unwrap();
    match subject::find(&probe, &facts, template) {
        subject::Subject::Failed(failure) => assert_eq!(failure.status, Some(500)),
        other => panic!("expected a failure, got {other:?}"),
    }
}

#[test]
fn a_region_that_palette_refuses_for_the_key_is_no_end_of_the_search() {
    // an account that has not turned on a region gets a 4xx for its VPCs
    let api = regions_with(&[
        ("af-south-1", json!({"vpcs": []})),
        ("us-west-2", json!({"vpcs": [{"vpcId": "vpc-9"}]})),
    ])
    .failing(
        "/v1/clouds/aws/regions/af-south-1/vpcs?cloudAccountUid=acc1",
        403,
        "{}",
    );
    let probe = subject::Probe::new(&api, &api);
    let facts = real_facts();
    let template = request_of(&facts, "validate-security-groups").unwrap();
    assert_eq!(
        subject::find(&probe, &facts, template),
        subject::Subject::Path(
            "/v1/clouds/aws/securitygroups?cloudAccountUid=acc1&region=us-west-2&vpcId=vpc-9"
                .to_string()
        )
    );
    // when no region serves, the reason is the first refusal
    let api = regions_with(&[("af-south-1", json!({"vpcs": []}))]).failing(
        "/v1/clouds/aws/regions/af-south-1/vpcs?cloudAccountUid=acc1",
        403,
        "{}",
    );
    let probe = subject::Probe::new(&api, &api);
    match subject::find(&probe, &facts, template) {
        subject::Subject::Failed(failure) => assert_eq!(failure.status, Some(403)),
        other => panic!("expected a failure, got {other:?}"),
    }
}

#[test]
fn the_registry_uid_is_the_subject_of_a_pack_in_a_registry_and_never_of_a_pack() {
    let api = MemoryApi::new().with(
        "/v1/packs",
        json!({"items": [{"spec": {"name": "aws-ebs", "registryUid": "reg1"}}]}),
    );
    let probe = subject::Probe::new(&api, &api);
    let facts = real_facts();
    assert_eq!(
        subject::find(&probe, &facts, "/v1/packs/{name}/registries/{uid}"),
        subject::Subject::Path("/v1/packs/aws-ebs/registries/reg1".to_string())
    );
    // `{uid}` names a pack in this request, and the listing holds no uid of a pack
    match subject::find(&probe, &facts, "/v1/packs/{uid}") {
        subject::Subject::Missing(why) => {
            assert_eq!(why, "the run has no way to find a value for `{uid}`");
        }
        other => panic!("expected no subject, got {other:?}"),
    }
}

#[test]
fn a_pair_of_the_query_that_the_route_does_not_take_goes_out_of_the_request() {
    let api = regions_with(&[("us-east-2", json!({"vpcs": []}))]);
    let probe = subject::Probe::new(&api, &api);
    let facts = real_facts();
    let found = subject::find(
        &probe,
        &facts,
        "/v1/clouds/aws/regions/{region}/instancetypes?cloudAccountUid={cloudAccountUid}&subscriptionId={subscriptionId}&cpuGtEq=0&memoryGtEq=0",
    );
    assert_eq!(
        found,
        subject::Subject::Path(
            "/v1/clouds/aws/regions/us-east-2/instancetypes?cloudAccountUid=acc1&cpuGtEq=0&memoryGtEq=0"
                .to_string()
        )
    );
}

/// The documents behind `validate-instance-types` for AWS: the account, the regions, and the types
/// of the first region. The stand-in serves the types only with the query that the rule sends.
fn instance_type_documents() -> Vec<(&'static str, Value)> {
    let mut docs: Vec<_> = aws_list_documents().into_iter().take(2).collect();
    docs.push((
        "v1/clouds/aws/regions/us-east-2/instancetypes@cloudAccountUid=acc1&cpuGtEq=0&memoryGtEq=0",
        json!({"instanceTypes": [
            {"type": "m5.large", "cpu": 2, "memory": 8, "nonSupportedZones": null}]}),
    ));
    docs
}

#[test]
fn the_instance_types_are_asked_for_the_clouds_of_the_lab_and_the_other_clouds_skip_with_a_reason()
{
    let docs = instance_type_documents();
    let (_root, stand_in) = stand_in(&docs);
    let (_dir, api) = served(&docs);
    let contract = contract_of(&real_facts(), &stand_in, &api);
    let entry = contract
        .facts
        .iter()
        .find(|e| e.id == "validate-instance-types")
        .unwrap();
    // the stand-in serves the route only with the query of the rule for AWS, so `same` shows that
    // the request of the choice `aws` went out without the subscription
    assert_eq!(entry.outcome, Outcome::Same, "{entry:?}");
    let reason = entry.reason.as_deref().unwrap_or_default();
    for cloud in ["gcp", "azure"] {
        assert!(
            reason.contains(&format!("{cloud}: the run has no way to find a value")),
            "{reason}"
        );
    }
    assert!(!reason.contains("aws:"), "{reason}");
}

/// An entry of a fact with the outcome `outcome`, the reason, and the difference at `field`.
fn entry_with(outcome: Outcome, reason: Option<&str>, field: Option<&str>) -> Entry {
    let mut entry = super::entry_of(&facts()[0], outcome, reason.map(str::to_string), Vec::new());
    if let Some(field) = field {
        entry.differences.push(super::Difference {
            kind: DifferenceKind::Type,
            path: field.to_string(),
            expected: vec!["string".to_string()],
            found: vec!["number".to_string()],
        });
    }
    entry
}

#[test]
fn a_fact_with_one_request_keeps_its_entry_and_with_more_it_takes_the_outcome_that_counts_most() {
    let one = entry_with(Outcome::Skipped, Some("why"), None);
    assert_eq!(
        super::merge(vec![(String::new(), one)]).unwrap().reason,
        Some("why".to_string())
    );
    let skipped = |why| entry_with(Outcome::Skipped, Some(why), None);
    let same = entry_with(Outcome::Same, None, None);
    let differs = entry_with(Outcome::Differs, None, Some("status"));
    let error = entry_with(Outcome::Error, Some("broke"), None);
    let merged = |parts: Vec<Entry>| {
        let labels = ["aws", "gcp", "azure"];
        super::merge(
            parts
                .into_iter()
                .enumerate()
                .map(|(i, e)| (labels[i].to_string(), e))
                .collect(),
        )
        .unwrap()
    };
    // a request that is the same counts more than a request that the run skipped
    let m = merged(vec![
        same.clone(),
        skipped("no account"),
        skipped("no region"),
    ]);
    assert_eq!(m.outcome, Outcome::Same);
    assert_eq!(
        m.reason.as_deref(),
        Some("gcp: no account; azure: no region")
    );
    // all skipped: the reasons say why, by cloud
    let m = merged(vec![skipped("a"), skipped("b")]);
    assert_eq!(m.outcome, Outcome::Skipped);
    assert_eq!(m.reason.as_deref(), Some("aws: a; gcp: b"));
    // an error counts more than a request that is the same, and a difference counts most
    assert_eq!(
        merged(vec![same.clone(), error.clone()]).outcome,
        Outcome::Error
    );
    let m = merged(vec![same.clone(), error, differs.clone()]);
    assert_eq!(m.outcome, Outcome::Differs);
    assert_eq!(m.reason, None);
    // a difference of a field names its cloud; a status names its request already
    let m = merged(vec![
        same,
        entry_with(Outcome::Differs, None, Some("instanceTypes[].cpu")),
        differs,
    ]);
    let paths: Vec<&str> = m.differences.iter().map(|d| d.path.as_str()).collect();
    assert_eq!(paths, ["gcp: instanceTypes[].cpu", "azure: status"]);
}

// ---- the floor of a window with a cluster ---------------------------------------------------

#[test]
fn a_window_with_a_cluster_turns_a_skipped_fact_of_a_cluster_into_an_error() {
    let (_root, stand_in) = stand_in(&documents("Running"));
    let empty = MemoryApi::new().with("/v1/dashboard/spectroclusters/meta", json!([]));
    let mut contract = contract(&stand_in, &empty);
    // the weekly run: a tenant with no cluster skips the facts of a cluster, and the exit code is 0
    assert_eq!(outcome(&contract, "cluster-document"), Outcome::Skipped);
    assert_eq!(outcome(&contract, "cloud-config"), Outcome::Skipped);
    assert_eq!(contract.exit_code(), 0);
    contract.hold_floor(&floor(&facts()));
    for id in ["cluster-document", "cloud-config"] {
        let entry = contract.facts.iter().find(|e| e.id == id).unwrap();
        assert_eq!(entry.outcome, Outcome::Error, "{id}");
        let reason = entry.reason.as_deref().unwrap();
        assert!(reason.starts_with("the window holds a cluster"), "{reason}");
        assert!(reason.contains("the tenant has no object"), "{reason}");
    }
    assert_eq!(contract.summary.error, 2);
    assert_eq!(contract.exit_code(), 2);
    // a fact that takes no subject from a cluster is not in the floor
    assert_eq!(outcome(&contract, "edge-host-document"), Outcome::Skipped);
}

#[test]
fn the_floor_changes_nothing_when_the_run_checked_each_fact_of_the_cluster() {
    let (_root, stand_in) = stand_in(&documents("Running"));
    let mut contract = contract(&stand_in, &memory(&documents("Running")));
    let before = serde_json::to_string(&contract).unwrap();
    contract.hold_floor(&floor(&facts()));
    assert_eq!(serde_json::to_string(&contract).unwrap(), before);
    assert_eq!(outcome(&contract, "cluster-document"), Outcome::Same);
}

#[test]
fn the_option_with_cluster_takes_no_value() {
    use super::command::Options;
    assert!(!Options::parse(&[]).unwrap().with_cluster);
    assert!(Options::parse(&["--with-cluster"]).unwrap().with_cluster);
    assert!(
        Options::parse(&["--with-cluster", "--no-cloud"])
            .unwrap()
            .no_cloud
    );
}

// ---- the summary line ---------------------------------------------------------------------

#[test]
fn the_line_that_the_run_prints_is_the_line_that_the_filter_of_the_scripts_writes() {
    use crate::process::CommandExt;
    use std::process::Command;
    let (_root, stand_in) = stand_in(&documents("Running"));
    // one fact is the same, one differs on its status, and the others skip: each count is not 0
    let api = memory(&documents("Running")).failing("/v1/spectroclusters/c1", 403, "{}");
    let contract = contract(&stand_in, &api);
    let summary = contract.summary;
    assert!(summary.same > 0 && summary.differs > 0 && summary.skipped > 0);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("contract.json");
    std::fs::write(&file, serde_json::to_string(&contract).unwrap()).unwrap();
    let filter = crate::repository().join("examples/live/contract-summary.jq");
    let written = Command::new("jq")
        .args(["-r", "-f"])
        .arg(&filter)
        .arg(&file)
        .output_locked()
        .unwrap();
    assert!(written.status.success(), "{written:?}");
    assert_eq!(
        String::from_utf8_lossy(&written.stdout).trim_end(),
        summary.line()
    );
    // the total is the count of the entries
    assert!(
        summary
            .line()
            .starts_with(&format!("{} facts:", contract.facts.len()))
    );
}
