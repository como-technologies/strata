//! The tests of the table of the sources: which request a source serves, the placeholder that two
//! sources read, and the facts that need a cluster.

use super::{Origin, Read, SOURCES, Source, ambiguous, floor, serves, source, stale, unsourced};
use crate::facts::{Fact, from_text, load};

/// The two requests of `/v1/packs/` that read `{uid}` in two ways.
fn pack_facts() -> Vec<Fact> {
    from_text(
        "
- id: pack-tags
  kind: request
  request: GET /v1/packs/{name}/registries/{uid}
  reads: {fields: [spec.name]}
  pages: [reference/palette-api.md]
  code: [src/lib.rs]
- id: pack-of-draft
  kind: request
  request: GET /v1/packs/{uid}
  reads: {fields: [spec.name]}
  pages: [reference/palette-api.md]
  code: [src/lib.rs]
- id: pack-listing
  kind: request
  request: GET /v1/packs
  reads: {fields: [items]}
  pages: [reference/palette-api.md]
  code: [src/lib.rs]
",
    )
}

fn source_of(route: &'static str, param: &'static str, fact: &'static str) -> Source {
    Source {
        route,
        param,
        origin: Origin::Fact {
            fact,
            read: Read::Pointer("/items/0/uid"),
        },
    }
}

#[test]
fn a_source_serves_a_request_that_goes_on_from_its_route_by_whole_segments() {
    assert!(serves("/v1/packs/{name}", "/v1/packs/{name}"));
    assert!(serves(
        "/v1/packs/{name}",
        "/v1/packs/{name}/registries/{uid}"
    ));
    assert!(serves(
        "/v1/clouds/aws/regions",
        "/v1/clouds/aws/regions?cloudAccountUid={cloudAccountUid}"
    ));
    assert!(serves(
        "/v1/clouds/aws/regions",
        "/v1/clouds/aws/regions/{region}/vpcs?cloudAccountUid={cloudAccountUid}"
    ));
    // a text that goes on in the middle of a segment is another route
    assert!(!serves("/v1/packs/{name}", "/v1/packs/{name}x"));
    // `{uid}` names a pack here and a registry there
    assert!(!serves(
        "/v1/packs/{name}/registries/{uid}",
        "/v1/packs/{uid}"
    ));
}

#[test]
fn the_uid_of_a_registry_has_a_source_and_the_uid_of_a_pack_has_none() {
    let registry = source("/v1/packs/{name}/registries/{uid}", "uid").unwrap();
    match &registry.origin {
        Origin::Fact { fact, read } => {
            assert_eq!(*fact, "pack-listing");
            assert!(matches!(read, Read::Pointer("/items/0/spec/registryUid")));
        }
        Origin::Omitted => panic!("the registry is in the listing"),
    }
    // the registry uid is a wrong value for a pack: the run sends no request
    assert!(source("/v1/packs/{uid}", "uid").is_none());
    assert_eq!(unsourced("/v1/packs/{uid}"), ["uid"]);
    assert!(unsourced("/v1/packs/{name}/registries/{uid}").is_empty());
}

#[test]
fn a_source_for_a_prefix_of_two_requests_reads_their_placeholder_in_two_ways() {
    // the source that the list had: the route `/v1/packs/`, and `{uid}` for each request under it
    let table = [source_of("/v1/packs", "uid", "pack-listing")];
    let bad = ambiguous(&table, &pack_facts());
    assert_eq!(bad.len(), 2, "{bad:#?}");
    for (message, id) in bad.iter().zip(["pack-tags", "pack-of-draft"]) {
        assert!(message.contains(&format!("fact `{id}`")), "{bad:#?}");
        assert!(message.contains("stops before the placeholder"), "{bad:#?}");
    }
    // two sources for one placeholder are a fault too
    let table = [
        source_of("/v1/packs/{name}/registries/{uid}", "uid", "pack-listing"),
        source_of("/v1/packs/{name}/registries/{uid}", "uid", "pack-listing"),
    ];
    let bad = ambiguous(&table, &pack_facts());
    assert_eq!(bad.len(), 1, "{bad:#?}");
    assert!(bad[0].contains("2 sources read `{uid}`"), "{bad:#?}");
    // one source for each request is no fault
    let table = [source_of(
        "/v1/packs/{name}/registries/{uid}",
        "uid",
        "pack-listing",
    )];
    assert!(ambiguous(&table, &pack_facts()).is_empty());
}

#[test]
fn a_source_that_serves_no_request_or_reads_no_get_request_is_a_fault() {
    let table = [
        source_of("/v1/gone/{uid}", "uid", "pack-listing"),
        source_of("/v1/packs/{name}/registries/{uid}", "uid", "missing-fact"),
        source_of("/v1/packs/{name}/registries/{uid}", "uid", "pack-listing"),
    ];
    let bad = stale(&table, &pack_facts());
    assert_eq!(bad.len(), 2, "{bad:#?}");
    assert!(
        bad[0].contains("`/v1/gone/{uid}` serves no request of the list"),
        "{bad:#?}"
    );
    assert!(bad[1].contains("reads the fact `missing-fact`"), "{bad:#?}");
}

#[test]
fn the_table_of_the_sources_agrees_with_the_list_of_the_repository() {
    let facts = load(&crate::repository()).unwrap();
    assert_eq!(super::check(&facts), Vec::<String>::new());
    assert_eq!(super::check_table(&facts), Vec::<String>::new());
    assert!(!SOURCES.is_empty());
}

#[test]
fn the_facts_that_need_a_cluster_are_the_get_requests_that_take_their_subject_from_one() {
    let facts = load(&crate::repository()).unwrap();
    let floor = floor(&facts);
    for id in [
        "cluster-document",
        "cluster-variables",
        "cluster-rbacs",
        "cluster-namespaces",
        "cluster-profile-updates",
        "cluster-notifications",
        // the cloud config takes the cloud type and its uid from the document of the cluster
        "cloud-config",
    ] {
        assert!(floor.contains(id), "{id} is not in {floor:?}");
    }
    for (id, why) in [
        ("cluster-listing", "an empty listing is a result"),
        (
            "cluster-repave-status",
            "a cluster can lack the record: Palette answers with a code",
        ),
        ("tag-filter", "the subject is a saved filter"),
        ("pack-tags", "the subject is a pack"),
        (
            "validate-cluster-packs",
            "the request sends a body, so the run does not send it",
        ),
    ] {
        assert!(!floor.contains(id), "{id} is in the floor, and {why}");
    }
}

#[test]
fn a_fact_with_no_document_of_the_stand_in_is_out_of_the_floor() {
    let mut facts = load(&crate::repository()).unwrap();
    assert!(floor(&facts).contains("cluster-variables"));
    let variables = facts
        .iter_mut()
        .find(|f| f.id == "cluster-variables")
        .unwrap();
    variables.stand_in.clear();
    assert!(!floor(&facts).contains("cluster-variables"));
}
