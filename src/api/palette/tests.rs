//! The tests of the reads of Palette: the routes, the paging, and the documents they give.

use serde_json::json;

use super::*;
use crate::api::{MemoryApi, RawCluster, ReadErrors};
use crate::scope::ProfileScope;

fn listing() -> Value {
    json!({"items": [
        {"metadata": {"name": "aws-stack", "uid": "u1"}, "spec": {"version": "1.0.0"}},
        {"metadata": {"name": "aws-stack", "uid": "u2"}, "spec": {"version": "1.1.0"}},
    ]})
}

#[test]
fn the_copy_of_a_manifest_that_a_holder_keeps_reads_as_a_manifest_of_a_profile_does() {
    // the template of the holder names the copy, and the copy gives no content
    let templates = vec![json!({"uid": "p1", "packs": [
        {"name": "cfg", "manifests": [{"name": "app-config", "uid": "m9"}]}]})];
    let mut manifests = json!({"profiles": [{"spec": {"packs": [{"spec": {"name": "cfg"}}]}}]});
    let copy = json!({"metadata": {"name": "app-config-copy", "uid": "m9"},
        "spec": {"published": {"digest": "d9"}}});
    own_manifests(&templates, |_, _, _| Ok(copy.clone()), &mut manifests).unwrap();
    assert_eq!(
        manifests["profiles"][0]["spec"]["packs"][0]["spec"]["manifests"][0],
        json!({"name": "app-config", "uid": "m9", "digest": "d9", "content": ""}),
        "the name of the template and empty text, as a profile record has them"
    );
}

#[test]
fn a_name_and_a_version_that_two_app_profiles_have_are_ambiguous() {
    let api = MemoryApi::new().with(
        APP_PROFILE_LISTING,
        json!({"appProfiles": [
            {"metadata": {"name": "shop", "uid": "ap2"}, "spec": {"version": "1.0.0"}},
            {"metadata": {"name": "shop", "uid": "ap1"}, "spec": {"version": "1.0.0"}},
            {"metadata": {"name": "shop", "uid": "ap0"}, "spec": {"version": "0.9.0"}},
        ]}),
    );
    let palette = Palette::new(api);
    for asked in ["1.0.0", crate::reference::LATEST] {
        let err = palette.resolve_app_profile("shop", asked).unwrap_err();
        assert!(
            matches!(&err, Error::AppProfileNameAmbiguous { version, uids, .. }
                if version == "1.0.0" && uids == &["ap1", "ap2"]),
            "{asked}: {err}"
        );
    }
    assert_eq!(palette.resolve_app_profile("shop", "0.9.0").unwrap(), "ap0");
    let err = palette.resolve_app_profile("shop", "2.0.0").unwrap_err();
    assert!(matches!(err, Error::AppProfileNotFound { .. }), "{err}");
}

#[test]
fn a_pack_name_and_a_registry_uid_go_into_the_path_of_the_tags_escaped() {
    // with no escape, a `/` in the name ends the segment, and a `?` starts the query
    let p = Palette::new(
        MemoryApi::new()
            .with(
                "/v1/packs/a%2Fb%3Fc/registries/r%201",
                json!({"tags": [{"tag": "1.x"}]}),
            )
            .with(
                "/v1/packs/a%2Fb%3Fc/registries/r%201?states=disabled",
                json!({"tags": []}),
            )
            .with(
                "/v1/packs?filters=spec.name=a%2Fb%3Fc&limit=100&offset=100",
                json!({"items": [1]}),
            ),
    );
    assert_eq!(
        p.pack_tags("a/b?c", "r 1", None).unwrap()["tags"][0]["tag"],
        "1.x"
    );
    assert_eq!(
        p.pack_tags("a/b?c", "r 1", Some("disabled")).unwrap()["tags"],
        json!([])
    );
    assert_eq!(
        p.pack_listing_page("a/b?c", 100).unwrap()["items"],
        json!([1])
    );
    let err = p.pack_tags("a/b?c", "r 2", None).unwrap_err();
    assert!(matches!(err, Error::Api { status: 404, .. }), "{err}");
}

#[test]
fn the_registry_listings_read_the_metadata_and_the_summary_of_a_kind() {
    let p = Palette::new(
        MemoryApi::new()
            .with("/v1/registries/metadata", json!({"items": [1]}))
            .with("/v1/registries/pack/summary", json!({"items": [2]}))
            .with("/v1/registries/oci/summary", json!({"items": [3]})),
    );
    assert_eq!(p.registry_metadata().unwrap()["items"], json!([1]));
    assert_eq!(
        p.registry_summary(RegistryKind::Pack).unwrap()["items"],
        json!([2])
    );
    assert_eq!(
        p.registry_summary(RegistryKind::Oci).unwrap()["items"],
        json!([3])
    );
    assert_eq!(
        RegistryKind::of_pack_type(PackType::Spectro),
        Some(RegistryKind::Pack)
    );
    assert_eq!(
        RegistryKind::of_pack_type(PackType::Helm),
        Some(RegistryKind::Helm)
    );
    assert_eq!(RegistryKind::of_pack_type(PackType::Manifest), None);
    for &kind in RegistryKind::ALL {
        assert_eq!(RegistryKind::from_text(kind.as_str()), Some(kind));
        assert_eq!(RegistryKind::of_pack_type(kind.pack_type()), Some(kind));
    }
    assert_eq!(RegistryKind::from_text("spectro"), None);
}

#[test]
fn resolve_finds_the_exact_name_and_version() {
    let p =
        Palette::new(MemoryApi::new().with("/v1/dashboard/clusterprofiles/metadata", listing()));
    assert_eq!(p.resolve("aws-stack", "1.1.0").unwrap(), "u2");
    let err = p.resolve("aws-stack", "9.9.9").unwrap_err();
    assert!(matches!(err, Error::ProfileNotFound { .. }));
    assert!(err.to_string().contains("'aws-stack' with version '9.9.9'"));
    // at tenant scope the listing advice is a dead end: the scope holds only tenant profiles
    assert!(
        err.to_string().contains("this is tenant scope"),
        "a miss at tenant scope asks for a project first: {err}"
    );

    // in a project, the listing is the right next command, and the name may be a pack's
    let in_project = Palette::new(
        MemoryApi::new()
            .with("/v1/dashboard/clusterprofiles/metadata", listing())
            .in_project("p1"),
    );
    let err = in_project.resolve("aws-stack", "9.9.9").unwrap_err();
    assert!(
        err.to_string().contains("`strata list --pack aws-stack`"),
        "a miss in a project points at the listing: {err}"
    );
}

/// A listing in a project with a profile of the tenant and a profile of the project named
/// `aws-stack`, both at 1.1.1, and the document of each with its scope.
fn two_scopes() -> MemoryApi {
    MemoryApi::new()
            .in_project("p1")
            .with(
                "/v1/dashboard/clusterprofiles/metadata",
                json!({"items": [
                    {"metadata": {"name": "aws-stack", "uid": "u-tenant"}, "spec": {"version": "1.1.1"}},
                    {"metadata": {"name": "aws-stack", "uid": "u-project"}, "spec": {"version": "1.1.1"}},
                    {"metadata": {"name": "aws-stack", "uid": "u-old"}, "spec": {"version": "1.0.0"}},
                ]}),
            )
            .with(
                "/v1/clusterprofiles/u-tenant",
                json!({"metadata": {"uid": "u-tenant", "annotations": {"scope": "tenant"}}}),
            )
            .with(
                "/v1/clusterprofiles/u-project",
                json!({"metadata": {"uid": "u-project", "annotations": {"scope": "project"}}}),
            )
}

#[test]
fn a_name_and_version_that_two_scopes_answer_is_an_error_that_names_each_uid_with_its_scope() {
    let p = Palette::new(two_scopes());
    let err = p.resolve("aws-stack", "1.1.1").unwrap_err();
    assert_eq!(err.kind(), "profile-name-ambiguous");
    let Error::ProfileNameAmbiguous {
        name,
        version,
        profiles,
    } = &err
    else {
        panic!("{err}");
    };
    assert_eq!((name.as_str(), version.as_str()), ("aws-stack", "1.1.1"));
    assert_eq!(
        profiles
            .iter()
            .map(|p| (p.uid.as_str(), p.scope))
            .collect::<Vec<_>>(),
        [
            ("u-project", Some(ProfileScope::Project)),
            ("u-tenant", Some(ProfileScope::Tenant)),
        ],
        "by uid, each with the scope of its document"
    );
    assert!(
        err.to_string()
            .contains("u-project (project), u-tenant (tenant); give the uid"),
        "{err}"
    );
    // the version that one scope holds still resolves
    assert_eq!(p.resolve("aws-stack", "1.0.0").unwrap(), "u-old");
}

#[test]
fn latest_is_ambiguous_only_when_two_scopes_hold_the_highest_version() {
    let p = Palette::new(two_scopes());
    let err = p.resolve_latest("aws-stack").unwrap_err();
    assert!(matches!(err, Error::ProfileNameAmbiguous { .. }), "{err}");
    assert!(
        err.to_string().contains("with version '1.1.1'"),
        "the message names the highest version, not 'latest': {err}"
    );

    // two scopes hold an older version, and one holds the highest: no error
    let listing = json!({"items": [
        {"metadata": {"name": "aws-stack", "uid": "u-tenant"}, "spec": {"version": "1.0.0"}},
        {"metadata": {"name": "aws-stack", "uid": "u-project"}, "spec": {"version": "1.0.0"}},
        {"metadata": {"name": "aws-stack", "uid": "u-new"}, "spec": {"version": "1.1.0"}},
    ]});
    let p = Palette::new(MemoryApi::new().with("/v1/dashboard/clusterprofiles/metadata", listing));
    assert_eq!(p.resolve_latest("aws-stack").unwrap(), "u-new");
}

#[test]
fn the_profile_search_gives_the_uid_of_each_version_of_a_scope_over_each_page() {
    let body = json!({
        "filter": {"scope": "tenant"},
        "sort": [{"field": "profileName", "order": "asc"}],
    });
    // a first page of 50 names, each with one version, and a second page of one name
    let names = |from: usize, n: usize| -> Vec<Value> {
        (from..from + n)
                .map(|i| {
                    json!({"metadata": {"name": format!("p{i}")},
                           "specSummary": {"versions": [{"version": "1.0.0", "uid": format!("u{i}")}]}})
                })
                .collect()
    };
    let api = MemoryApi::new()
        .with_post(
            &with_limit(PROFILE_SEARCH),
            &body,
            json!({"items": names(0, 50), "listmeta": {"count": 51, "limit": 50, "offset": 0}}),
        )
        .with_post(
            &format!("{}&offset=50", with_limit(PROFILE_SEARCH)),
            &body,
            json!({"items": [{"metadata": {"name": "p50"}, "specSummary": {"versions": [
                    {"version": "1.0.0", "uid": "u50"}, {"version": "2.0.0", "uid": "u50b"}]}}],
                       "listmeta": {"count": 51, "limit": 50, "offset": 50}}),
        );
    let uids = Palette::new(&api)
        .profile_uids_in_scope(ProfileScope::Tenant)
        .unwrap();
    assert_eq!(uids.len(), 52, "{uids:?}");
    assert!(uids.contains("u0") && uids.contains("u50") && uids.contains("u50b"));
    assert_eq!(api.posted().len(), 2, "two pages, two requests");

    // a Palette with no profile search answers 404, and the caller decides
    let err = Palette::new(MemoryApi::new())
        .profile_uids_in_scope(ProfileScope::System)
        .unwrap_err();
    assert!(matches!(err, Error::Api { status: 404, .. }), "{err}");
}

#[test]
fn latest_resolves_to_the_highest_version_that_is_not_a_pre_release() {
    let listing = json!({"items": [
        {"metadata": {"name": "aws-stack", "uid": "u1"}, "spec": {"version": "1.9.0"}},
        {"metadata": {"name": "aws-stack", "uid": "u2"}, "spec": {"version": "1.10.0"}},
        {"metadata": {"name": "aws-stack", "uid": "u4"}, "spec": {"version": "1.10.0-g0123456789ab"}},
        {"metadata": {"name": "aws-stack", "uid": "u5"}, "spec": {"version": "1.11.0-rc.1"}},
        {"metadata": {"name": "aws-stack", "uid": "u0"}, "spec": {"version": "1.2.0"}},
        {"metadata": {"name": "other", "uid": "u3"}, "spec": {"version": "9.0.0"}},
    ]});
    let p = Palette::new(MemoryApi::new().with("/v1/dashboard/clusterprofiles/metadata", listing));
    assert_eq!(p.resolve_latest("aws-stack").unwrap(), "u2");
    assert_eq!(p.resolve("aws-stack", "latest").unwrap(), "u2");
    let err = p.resolve_latest("nope").unwrap_err();
    assert!(matches!(err, Error::ProfileNotFound { .. }), "{err}");
    assert!(err.to_string().contains("'nope'"));
    assert!(err.to_string().contains("'latest'"));
}

#[test]
fn an_item_with_no_version_or_no_uid_is_not_a_version_of_the_name() {
    // the one parse keeps each item of the listing. A resolve skips an item with no version or
    // no uid, so a name with no other item is profile-not-found, not an empty uid.
    let listing = json!({"items": [
        {"metadata": {"name": "x"}},
        {"metadata": {"name": "y", "uid": "u1"}},
        {"metadata": {"name": "y"}, "spec": {"version": "2.0.0"}},
        {"metadata": {"name": "y", "uid": "u2"}, "spec": {"version": "1.0.0"}},
    ]});
    let p = Palette::new(MemoryApi::new().with("/v1/dashboard/clusterprofiles/metadata", listing));
    let err = p.resolve_latest("x").unwrap_err();
    assert!(matches!(err, Error::ProfileNotFound { .. }), "{err}");
    assert_eq!(p.resolve_latest("y").unwrap(), "u2");
    assert_eq!(p.resolve("y", "1.0.0").unwrap(), "u2");
    let err = p.resolve("y", "2.0.0").unwrap_err();
    assert!(matches!(err, Error::ProfileNotFound { .. }), "{err}");
}

#[test]
fn fetch_profile_tolerates_a_missing_variables_endpoint_only() {
    let base = MemoryApi::new()
        .with("/v1/clusterprofiles/u1", json!({"metadata": {"uid": "u1"}}))
        .with(
            "/v1/clusterprofiles/u1/packs/manifests",
            json!({"spec": {"packs": []}}),
        );
    let raw = Palette::new(base.clone())
        .fetch_profile("u1", &Progress::silent())
        .unwrap();
    assert_eq!(raw.variables, json!({"variables": []}));

    let err = Palette::new(base.failing("/v1/clusterprofiles/u1/variables", 500, "boom"))
        .fetch_profile("u1", &Progress::silent())
        .unwrap_err();
    assert!(matches!(err, Error::Api { status: 500, .. }), "{err}");
}

#[test]
fn a_uid_with_a_slash_goes_into_each_route_escaped() {
    // with no escape, a `/` in a uid ends the segment, and a `?` starts the query
    assert_eq!(
        route(PROFILES, &["a/b", "packs", "x?y"]),
        "/v1/clusterprofiles/a%2Fb/packs/x%3Fy"
    );
    let uid = "a/b";
    let cluster = "/v1/spectroclusters/a%2Fb";
    let profile = "/v1/clusterprofiles/a%2Fb";
    let p = Palette::new(
        MemoryApi::new()
            .with(cluster, json!({"metadata": {"uid": uid}}))
            .with(
                format!("{cluster}/packs/resolvedValues"),
                json!({"profiles": []}),
            )
            .with(
                "/v1/notifications/spectrocluster/a%2Fb",
                json!({"items": [1], "listmeta": {"continue": ""}}),
            )
            .with(profile, json!({"metadata": {"uid": uid}}))
            .with(
                format!("{profile}/packs/manifests"),
                json!({"spec": {"packs": []}}),
            )
            .with(format!("{profile}/variables"), json!({"variables": []})),
    );
    let doc = p.subject_document(SubjectKind::Cluster, uid).unwrap();
    assert_eq!(text(&doc, "/metadata/uid"), uid);
    let values = p.resolved_values(SubjectKind::Cluster, uid).unwrap();
    assert_eq!(values["profiles"], json!([]));
    assert_eq!(p.cluster_notifications(uid).unwrap(), vec![json!(1)]);
    assert_eq!(
        text(&p.profile_document(uid).unwrap(), "/metadata/uid"),
        uid
    );
    let raw = p.fetch_profile(uid, &Progress::silent()).unwrap();
    assert_eq!(raw.variables, json!({"variables": []}));
}

#[test]
fn one_pager_walks_a_get_listing_and_a_post_search_by_continue() {
    let notifications = "/v1/notifications/spectrocluster/c1";
    let search = with_limit(EDGE_HOST_SEARCH);
    let api = MemoryApi::new()
        .with(
            notifications,
            json!({"items": [1, 2], "listmeta": {"continue": "t1"}}),
        )
        .with(
            format!("{notifications}?continue=t1"),
            json!({"items": [3], "listmeta": {"continue": ""}}),
        )
        .with(
            format!("POST {search}"),
            json!({"items": [4], "listmeta": {"continue": "t 2"}}),
        )
        .with(
            format!("POST {search}&continue=t%202"),
            json!({"items": null, "listmeta": {"continue": "t3"}}),
        );
    let p = Palette::new(&api);
    assert_eq!(
        p.cluster_notifications("c1").unwrap(),
        vec![json!(1), json!(2), json!(3)]
    );
    // a page with no items ends the search, also with a token
    assert_eq!(p.edge_hosts().unwrap(), vec![json!(4)]);
    assert_eq!(api.posted().len(), 2);
    assert_eq!(page_after("/v1/x", ""), "/v1/x");
    assert_eq!(page_after("/v1/x", "a b"), "/v1/x?continue=a%20b");
    assert_eq!(
        page_after("/v1/x?limit=50", "t"),
        "/v1/x?limit=50&continue=t"
    );
}

#[test]
fn fetch_subject_reads_each_kind_by_uid_and_then_by_name_after_a_404() {
    // the three kinds share one lookup: the uid first, then the one object of the name
    let templates =
        |uid: &str| json!({"metadata": {"uid": uid}, "spec": {"clusterProfileTemplates": []}});
    let api = MemoryApi::new()
        .with(
            CLUSTER_LISTING,
            json!([{"metadata": {"name": "lab", "uid": "c1"}}]),
        )
        .with(CLUSTER_SEARCH_SCHEMA, json!({"schema": {"properties": []}}))
        .with("/v1/spectroclusters/c1", templates("c1"))
        .with(
            "/v1/spectroclusters/c1/profiles/packs/manifests",
            json!({"profiles": []}),
        )
        .with("/v1/spectroclusters/c1/variables", json!([]))
        .with(
            CLUSTER_GROUP_LISTING,
            json!({"items": [{"name": "dev", "uid": "g1"}]}),
        )
        .with("/v1/clustergroups/g1", templates("g1"))
        .with(
            format!("POST {}", with_limit(EDGE_HOST_SEARCH)),
            json!({"items": [{"metadata": {"name": "store", "uid": "e1"}}],
                    "listmeta": {"continue": ""}}),
        )
        .with("/v1/edgehosts/e1", templates("e1"));
    let p = Palette::new(&api);
    let quiet = Progress::silent();
    for (kind, name, uid) in [
        (SubjectKind::Cluster, "lab", "c1"),
        (SubjectKind::ClusterGroup, "dev", "g1"),
        (SubjectKind::EdgeHost, "store", "e1"),
    ] {
        let by_uid = p.fetch_subject(kind, uid, &quiet).unwrap();
        assert_eq!(text(&by_uid.cluster, "/metadata/uid"), uid);
        let by_name = p.fetch_subject(kind, name, &quiet).unwrap();
        assert_eq!(text(&by_name.cluster, "/metadata/uid"), uid);
        let err = p.fetch_subject(kind, "nope", &quiet).unwrap_err();
        let expected = match kind {
            SubjectKind::Cluster => "cluster-not-found",
            SubjectKind::ClusterGroup => "cluster-group-not-found",
            SubjectKind::EdgeHost => "edge-host-not-found",
        };
        assert_eq!(err.kind(), expected, "{err}");
    }
    // another error of the document by uid returns, and strata tries no name
    let lookups = api.calls(CLUSTER_LISTING);
    let broken = api.clone().failing("/v1/spectroclusters/c9", 500, "{}");
    let err = Palette::new(&broken)
        .fetch_subject(SubjectKind::Cluster, "c9", &quiet)
        .unwrap_err();
    assert!(matches!(err, Error::Api { status: 500, .. }), "{err}");
    assert_eq!(broken.calls(CLUSTER_LISTING), lookups);
}

#[test]
fn the_pack_the_cloud_account_and_the_events_of_a_cluster_go_into_the_path_escaped() {
    // with no escape, a `/` in a uid ends the segment, and a `?` starts the query
    let p = Palette::new(
        MemoryApi::new()
            .with("/v1/packs/a%2Fb%3Fc", json!({"metadata": {"uid": "a/b?c"}}))
            .with(
                "/v1/cloudaccounts/aws/a%2Fb%3Fc",
                json!({"metadata": {"uid": "a/b?c"}}),
            )
            .with(
                "/v1/events/components/spectrocluster/a%2Fb%3Fc?limit=50",
                json!({"items": [1, 2]}),
            ),
    );
    assert_eq!(
        text(&p.pack_document("a/b?c").unwrap(), "/metadata/uid"),
        "a/b?c"
    );
    assert_eq!(
        text(&p.cloud_account("aws", "a/b?c").unwrap(), "/metadata/uid"),
        "a/b?c"
    );
    assert_eq!(p.cluster_events("a/b?c").unwrap(), vec![json!(1), json!(2)]);
    let err = p.cloud_account("gcp", "a/b?c").unwrap_err();
    assert!(matches!(err, Error::Api { status: 404, .. }), "{err}");
}

#[test]
fn the_events_of_a_cluster_with_items_null_or_with_no_items_are_an_empty_list() {
    let events = "/v1/events/components/spectrocluster/c1?limit=50";
    for answer in [json!({"items": null}), json!({}), json!({"items": {}})] {
        let p = Palette::new(MemoryApi::new().with(events, answer.clone()));
        assert!(p.cluster_events("c1").unwrap().is_empty(), "{answer}");
    }
}

#[test]
fn the_path_functions_give_the_route_that_the_read_sends() {
    assert_eq!(
        subject_path(SubjectKind::Cluster, "a/b"),
        "/v1/spectroclusters/a%2Fb"
    );
    assert_eq!(
        subject_path(SubjectKind::EdgeHost, "u1"),
        "/v1/edgehosts/u1"
    );
    assert_eq!(
        cluster_summary_path("u1"),
        "/v1/dashboard/spectroclusters/u1"
    );
    assert_eq!(
        cluster_events_path("u1"),
        "/v1/events/components/spectrocluster/u1?limit=50"
    );
    // the read sends the path that the function gives
    let api = MemoryApi::new()
        .with(cluster_summary_path("u1"), json!({"status": {}}))
        .with(
            subject_path(SubjectKind::Cluster, "u1"),
            json!({"spec": {}}),
        );
    let p = Palette::new(&api);
    p.fetch_cluster_summary("u1").unwrap();
    p.subject_document(SubjectKind::Cluster, "u1").unwrap();
    assert_eq!(api.calls(&cluster_summary_path("u1")), 1);
    assert_eq!(api.calls(&subject_path(SubjectKind::Cluster, "u1")), 1);
}

#[test]
fn the_variables_of_a_cluster_come_with_a_flag_for_a_missing_route() {
    let entries = json!([{"profileUid": "p1", "variables": [{"name": "a", "value": "1"}]}]);
    let api = MemoryApi::new()
        .with("/v1/spectroclusters/c1", json!({"metadata": {"uid": "c1"}}))
        .with("/v1/spectroclusters/c1/variables", entries.clone());
    let read = Palette::new(&api)
        .cluster_variables("c1", &Progress::silent())
        .unwrap();
    assert_eq!(
        read,
        VariablesRead {
            answer: entries,
            route_missing: false
        }
    );
    // an older Palette answers 404 or 405 for the route: no variable, and the flag
    for status in [404, 405] {
        let older = MemoryApi::new()
            .with("/v1/spectroclusters/c1", json!({"metadata": {"uid": "c1"}}))
            .failing("/v1/spectroclusters/c1/variables", status, "{}");
        let read = Palette::new(&older)
            .cluster_variables("c1", &Progress::silent())
            .unwrap();
        assert_eq!(
            read,
            VariablesRead {
                answer: json!([]),
                route_missing: true
            },
            "{status}"
        );
    }
    // a cluster that Palette does not have is an error, and so is any other failure
    let gone = MemoryApi::new();
    let err = Palette::new(&gone)
        .cluster_variables("c1", &Progress::silent())
        .unwrap_err();
    assert!(matches!(err, Error::Api { status: 404, .. }), "{err}");
    let broken = MemoryApi::new().failing("/v1/spectroclusters/c1/variables", 500, "boom");
    let err = Palette::new(&broken)
        .cluster_variables("c1", &Progress::silent())
        .unwrap_err();
    assert!(matches!(err, Error::Api { status: 500, .. }), "{err}");
}

#[test]
fn each_validate_post_of_the_client_goes_to_the_route_that_the_codes_name() {
    use crate::api::Method;
    use crate::api::codes::ValidateRoute;

    let api = MemoryApi::new();
    let p = Palette::new(&api);
    let body = json!({});
    // the answers are 404: the requests are what the test reads
    let _ = p.validate_packs("u1", &body);
    let _ = p.validate_profile_packs(None, &body);
    let _ = p.validate_profile_packs(Some("u2"), &body);
    let _ = p.validate_profile_import(&body);
    let routes: Vec<_> = api
        .posted()
        .iter()
        .map(|(path, _)| ValidateRoute::of(Method::Post, path))
        .collect();
    assert_eq!(
        routes,
        [
            Some(ValidateRoute::ClusterPacks),
            Some(ValidateRoute::ProfilePacks),
            Some(ValidateRoute::ProfilePacks),
            Some(ValidateRoute::ProfileImport),
        ]
    );
}

#[test]
fn the_copy_of_a_manifest_goes_into_a_pack_whose_spec_is_not_an_object() {
    let templates = vec![json!({"uid": "p1", "packs": [
        {"name": "cfg", "manifests": [{"name": "app-config", "uid": "m9"}]}]})];
    let mut manifests = json!({"profiles": [{"spec": {"packs": [
        {"metadata": {"name": "cfg"}, "spec": 5}]}}]});
    let copy = json!({"metadata": {"name": "app-config-copy", "uid": "m9"}});
    own_manifests(&templates, |_, _, _| Ok(copy.clone()), &mut manifests).unwrap();
    assert_eq!(
        manifests["profiles"][0]["spec"]["packs"][0]["spec"]["manifests"][0]["uid"],
        "m9"
    );
}

#[test]
fn the_update_that_waits_goes_into_a_cluster_whose_spec_is_not_an_object() {
    let api = MemoryApi::new().with(
        "/v1/spectroclusters/c1/profileUpdates",
        json!({"profiles": []}),
    );
    let raw = RawCluster {
        cluster: json!({"metadata": {"uid": "c1"}, "spec": "junk"}),
        manifests: Value::Null,
        variables: Value::Null,
        variables_unavailable: false,
        project_uid: None,
        fetched_at: None,
        notifications: None,
        repave: None,
        resolved_values: None,
        cloud_config: None,
        rbacs: None,
        namespaces: None,
        errors: ReadErrors::default(),
    };
    let pending = Palette::new(api).with_pending(raw).unwrap();
    assert_eq!(
        pending.cluster["spec"]["clusterProfileTemplates"],
        json!([])
    );
}

#[test]
fn a_listing_whose_next_page_is_the_page_it_is_ends_there_and_does_not_ask_again() {
    let palette = Palette::new(MemoryApi::new());
    let asked = std::cell::Cell::new(0);
    let page = json!({"items": [{"metadata": {"uid": "a"}}], "listmeta": {"continue": "tok"}});
    let items = palette
        .each_continue_page("/v1/things", |_| {
            asked.set(asked.get() + 1);
            assert!(asked.get() < 10, "the walk asks for the same page for ever");
            Ok(page.clone())
        })
        .unwrap();
    assert_eq!(
        asked.get(),
        2,
        "the first page, and the page that the token names"
    );
    assert_eq!(items.len(), 2);
}

#[test]
fn a_listing_by_offset_ends_at_a_page_that_starts_with_the_item_the_page_before_started_with() {
    let palette = Palette::new(MemoryApi::new());
    let asked = std::cell::Cell::new(0);
    let full = json!({"items": [{"uid": "a"}, {"uid": "b"}], "listmeta": {"count": 1_000_000}});
    let mut seen = Vec::new();
    palette
        .each_offset_page(
            2,
            |_| {
                asked.set(asked.get() + 1);
                assert!(asked.get() < 10, "the walk asks for the same page for ever");
                Ok(full.clone())
            },
            |item| {
                seen.push(item);
                true
            },
        )
        .unwrap();
    assert_eq!(asked.get(), 2, "the second page repeats the first");
    assert_eq!(seen.len(), 2, "the repeated page gives no item");
}
