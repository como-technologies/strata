//! Library-level tests for the operations, with an in-memory Palette API.
//!
//! A test of the document that an operation writes compares its schema id with the `const` of the
//! crate. A test of a refusal pins the id on purpose. It writes the text of a document of a later
//! version, such as `strata-snapshot/2`. Then it reads the message that names both ids.

use crate::common;

use std::fs;
use std::path::Path;

use serde_json::Value;
use strata::api::codes::{
    CreateRule, OPERATION_FORBIDDEN_CODE, PACK_FORBIDDEN_CODE, PACK_MACRO_UNDEFINED_CODE,
    PROFILE_EXISTS_CODE, PROFILE_IMPORT_MATCH_CODES, UPGRADE_NOT_SUPPORTED_CODE, ValidateRoute,
};
use strata::api::{MemoryApi, Method, Palette, RawProfile};
use strata::closed_set::{ClosedSet, Open};
use strata::diff::{ChangeFlag, Side, VerifyStatus, VersionMove};
use strata::error::{Error, FailureClass};
use strata::model::cloud::{CLOUD_ALL, CLOUD_CUSTOM};
use strata::model::profile_type::ProfileType;
use strata::ops;
use strata::reference::Ref;
use strata::term::progress::Progress;

use common::CommandExt;
use common::{
    CLUSTER_LISTING, CLUSTER_SEARCH, CLUSTER_SEARCH_SCHEMA, CLUSTER_UID, EDGE_HOST_SEARCH,
    OCI_REGISTRY_SUMMARY, PROFILE_LISTING, PROFILE_SEARCH, PROJECT_LISTING, PackFixture,
    REGISTRY_LISTING, SECRET_VALUE, UID_A, UID_ADDONS, UID_B, WITH_IMPACT, app_profile_route,
    behind_of, candidate_snapshot, cluster_manifests_route, cluster_namespaces_route,
    cluster_rbacs_route, cluster_ref, cluster_repave_route, cluster_resolved_values_route,
    cluster_route, cluster_variables_route, edge_host_route, fetch_examples,
    group_resolved_values_route, group_route, listing, manifest_route, manifests_route,
    no_manifests, no_template, no_variables, pack_entity_route, pack_listing_route, post,
    profile_document, profile_route, read_json, resolved_values_route, serve_fixture,
    serve_pack_listing, serve_profile, serve_registry_pack, snapshot_of, snapshot_of_at,
    snapshot_of_ref, strata_command, template, variables_route, write_record, write_yaml,
};

#[test]
fn verify_reports_unchanged_when_the_live_profile_matches_the_record() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let palette = Palette::new(serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A));
    let v = ops::verify(&recorded, &palette, "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged);
    assert!(v.diff.same);
    let ver = v.diff.verify.unwrap();
    assert_eq!(ver.status, VerifyStatus::Unchanged);
    assert_eq!(
        (
            ver.recorded_at.as_str(),
            ver.checked_at.as_str(),
            ver.uid.as_str()
        ),
        ("t0", "t1", UID_A)
    );
}

#[test]
fn verify_reports_a_deleted_version_as_gone_and_not_as_an_error() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    // Palette no longer has the uid: the version was deleted, as a retire does
    let palette = Palette::new(MemoryApi::new());
    let v = ops::verify(&recorded, &palette, "t1", &Progress::silent()).unwrap();
    assert!(!v.unchanged);
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.status, VerifyStatus::Drifted);
    assert!(ver.gone);
    assert!(
        v.diff
            .packs
            .iter()
            .all(|p| p.status == strata::diff::Status::Removed),
        "each pack of the record is removed"
    );
    let r = common::terminal();
    let text = r.render(&v.diff, &recorded, &v.live, false).unwrap();
    assert!(!text.contains("edited in place"), "{text}");
    let verdict = r.verdict(&v.diff).unwrap();
    assert!(
        verdict.contains(&format!(
            "no longer exists: uid {UID_A} is gone from Palette"
        )),
        "{verdict}"
    );
    assert_eq!(
        serde_json::to_value(ver).unwrap()["gone"],
        serde_json::json!(true)
    );
}

#[test]
fn verify_reports_drift_when_the_published_version_was_edited() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    // the live object under A's uid now has B's content: an in-place edit
    let palette = Palette::new(serve_fixture(MemoryApi::new(), dir.path(), "b", UID_A));
    let v = ops::verify(&recorded, &palette, "t1", &Progress::silent()).unwrap();
    assert!(!v.unchanged);
    assert_eq!(
        v.diff.verify.as_ref().unwrap().status,
        VerifyStatus::Drifted
    );
    assert!(
        v.diff
            .packs
            .iter()
            .any(|p| p.name == "kubernetes" && p.identity && p.authored)
    );
    assert_eq!(v.diff.summary.max_impact, strata::diff::ImpactLevel::Repave);
}

#[test]
fn verify_needs_a_uid_and_surfaces_api_errors() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let mut recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    // a 404 is a version that is gone (a verdict); any other failure is an error
    let palette = Palette::new(MemoryApi::new().failing(profile_route(UID_A), 500, "boom"));
    let err = ops::verify(&recorded, &palette, "t1", &Progress::silent()).unwrap_err();
    assert!(matches!(err, Error::Api { status: 500, .. }), "{err}");
    recorded.source.uid.clear();
    let err = ops::verify(&recorded, &palette, "t1", &Progress::silent()).unwrap_err();
    assert!(matches!(err, Error::NoUid));
}

#[test]
fn snapshot_for_resolves_names_and_never_connects_for_files() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let listing = serde_json::json!({"items": [
        {"metadata": {"name": "lab-profile", "uid": UID_A}, "spec": {"version": "1.2.0"}}
    ]});
    let api =
        serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A).with(PROFILE_LISTING, listing);
    let by_name = snapshot_of(&api, "lab-profile@1.2.0").unwrap();
    let by_uid = snapshot_of(&api, UID_A).unwrap();
    assert_eq!(by_name.root, by_uid.root);
    assert_eq!(by_name.source.reference, "lab-profile@1.2.0");

    let file = dir.path().join("a.json");
    common::write_snapshot(&file, &by_uid, true);
    let from_file = ops::snapshot_for(
        &Ref::Snapshot(file),
        &ops::SnapshotOptions::default(),
        || -> strata::Result<Palette<MemoryApi>> { panic!("must not connect for a snapshot file") },
        "t9",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(from_file.root, by_uid.root);

    let missing = snapshot_of(&api, "lab-profile@9.9.9").unwrap_err();
    assert!(matches!(missing, Error::ProfileNotFound { .. }));
}

#[test]
fn name_at_latest_records_the_resolved_version_and_keeps_latest_in_the_ref() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let listing = serde_json::json!({"items": [
        {"metadata": {"name": "lab-profile", "uid": UID_B}, "spec": {"version": "1.3.0"}},
        {"metadata": {"name": "lab-profile", "uid": UID_A}, "spec": {"version": "1.2.0"}},
    ]});
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A);
    let api = serve_fixture(api, dir.path(), "b", UID_B).with(PROFILE_LISTING, listing);
    let snap = snapshot_of(&api, "lab-profile@latest").unwrap();
    assert_eq!(snap.source.reference, "lab-profile@latest");
    assert_eq!(snap.source.version, "1.3.0");
    assert_eq!(snap.source.uid, UID_B);
}

/// Issue #616. A field test recorded a develop build, such as `1.2.0.develop`, for `@latest`, with
/// exit 0. Palette reads it as `1.2.0`. Strata puts it below `1.2.0`, and `@latest` skips it.
/// Issue #714: `1.4.develop` holds no three numbers. Strata put it above each version. Now it is a
/// pre-release of `1.4.0`, above `1.3.0`, and `@latest` skips it because it is a pre-release.
#[test]
fn name_at_latest_records_the_highest_version_and_not_a_build_with_a_tail_of_text() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let listing = serde_json::json!({"items": [
        {"metadata": {"name": "lab-profile", "uid": UID_A}, "spec": {"version": "1.2.0"}},
        {"metadata": {"name": "lab-profile", "uid": "uid-dev-2"}, "spec": {"version": "1.2.0.develop"}},
        {"metadata": {"name": "lab-profile", "uid": UID_B}, "spec": {"version": "1.3.0"}},
        {"metadata": {"name": "lab-profile", "uid": "uid-dev-4"}, "spec": {"version": "1.4.0.develop"}},
        {"metadata": {"name": "lab-profile", "uid": "uid-dev-5"}, "spec": {"version": "1.4.develop"}},
    ]});
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A);
    let api = serve_fixture(api, dir.path(), "b", UID_B).with(PROFILE_LISTING, listing);
    let snap = snapshot_of(&api, "lab-profile@latest").unwrap();
    assert_eq!(
        (snap.source.version.as_str(), snap.source.uid.as_str()),
        ("1.3.0", UID_B)
    );
    assert_eq!(snap.source.reference, "lab-profile@latest");
}

#[test]
fn name_at_latest_takes_the_highest_version_that_is_not_a_pre_release() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let listing = serde_json::json!({"items": [
        {"metadata": {"name": "lab-profile", "uid": "uid-rc"}, "spec": {"version": "1.4.0-rc.1"}},
        {"metadata": {"name": "lab-profile", "uid": UID_B}, "spec": {"version": "1.3.0"}},
        {"metadata": {"name": "lab-profile", "uid": "uid-g"}, "spec": {"version": "1.3.0-g0123456789ab"}},
        {"metadata": {"name": "lab-profile", "uid": UID_A}, "spec": {"version": "1.2.0"}},
        {"metadata": {"name": "only-rc", "uid": "uid-only-2"}, "spec": {"version": "2.0.0-rc.2"}},
        {"metadata": {"name": "only-rc", "uid": "uid-only-1"}, "spec": {"version": "2.0.0-rc.1"}},
    ]});
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A);
    let api = serve_fixture(api, dir.path(), "b", UID_B).with(PROFILE_LISTING, listing);
    let snap = snapshot_of(&api, "lab-profile@latest").unwrap();
    assert_eq!(
        (snap.source.version.as_str(), snap.source.uid.as_str()),
        ("1.3.0", UID_B)
    );
    // a name with no other version gives its highest pre-release
    let palette = Palette::new(&api);
    assert_eq!(palette.resolve_latest("only-rc").unwrap(), "uid-only-2");
    assert_eq!(palette.resolve_latest("lab-profile").unwrap(), UID_B);
}

/// The record of each kind of ref names the ref as typed. The record drops an empty part after
/// a separator: `cluster:<uid>/` records `cluster:<uid>`.
#[test]
fn a_record_of_each_kind_of_ref_holds_the_ref_as_typed() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let a = dir.path().join("a");
    let profile = read_json(&a.join("profile.json"));
    let published = profile["spec"]["published"].clone();
    let document = profile_document(
        UID_A,
        "lab-profile",
        "1.2.0",
        published.clone(),
        Some(published),
    );
    let profiles = serve_profile(
        MemoryApi::new(),
        UID_A,
        document,
        read_json(&a.join("manifests.json")),
        read_json(&a.join("variables.json")),
    )
    .with(
        PROFILE_LISTING,
        serde_json::json!({"items": [
            {"metadata": {"name": "lab-profile", "uid": UID_A}, "spec": {"version": "1.2.0"}}
        ]}),
    );
    let cluster = serve_cluster(MemoryApi::new(), dir.path(), "cluster-merged");
    let pending = serve_pending(
        serve_cluster(MemoryApi::new(), dir.path(), "cluster-b"),
        dir.path(),
        "cluster-a",
        |_| {},
    )
    .with(profile_route(UID_A), profile);
    let edge = serve_edge_host(MemoryApi::new(), dir.path());
    let group = serve_cluster_group(MemoryApi::new(), dir.path());
    let app = serve_app_profile(MemoryApi::new(), "2");
    let registry = serve_registry(vec![], &[("reg-public", public_entity())]);
    let cases = [
        (&profiles, "lab-profile@1.2.0".to_string()),
        (&profiles, UID_A.to_string()),
        (&profiles, format!("{UID_A}@draft")),
        (&profiles, "lab-profile@1.2.0@draft".to_string()),
        (&registry, "pack:csi-aws-ebs@1.65.0".to_string()),
        (&registry, "pack:reg-public/csi-aws-ebs@1.65.0".to_string()),
        (&cluster, format!("cluster:{CLUSTER_UID}")),
        (&cluster, format!("cluster:{CLUSTER_UID}/lab-profile")),
        (&pending, format!("cluster:{CLUSTER_UID}@pending")),
        (&edge, "edgehost:e1".to_string()),
        (&edge, "edgehost:edge-1".to_string()),
        (&group, "clustergroup:g1".to_string()),
        (&app, "appprofile:ap1".to_string()),
        (&app, "appprofile:shop@1.0.0".to_string()),
    ];
    for (api, text) in &cases {
        let snap = snapshot_of(api, text).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert_eq!(snap.source.reference, *text, "{text}");
    }
    let trimmed = snapshot_of(&cluster, &format!("cluster:{CLUSTER_UID}/")).unwrap();
    assert_eq!(trimmed.source.reference, format!("cluster:{CLUSTER_UID}"));
}

/// Each option of `snapshot` fits some kinds of ref. Another kind is a usage error before any
/// request, and so is a set of clusters.
#[test]
fn an_option_of_snapshot_that_does_not_fit_the_ref_is_a_usage_error_before_any_request() {
    use strata::ops::SnapshotOptions;
    let no_request = || -> strata::Result<Palette<MemoryApi>> {
        panic!("a usage error comes before any request")
    };
    let macros = SnapshotOptions {
        macros: true,
        ..SnapshotOptions::default()
    };
    let shape = SnapshotOptions {
        shape: true,
        ..SnapshotOptions::default()
    };
    let raw_dir = SnapshotOptions {
        raw_dir: Some(PathBuf::from("raw")),
        ..SnapshotOptions::default()
    };
    let validate = SnapshotOptions {
        validate: true,
        ..SnapshotOptions::default()
    };
    let content = SnapshotOptions::default();
    let cases = [
        ("edgehost:e1", &macros, "--macros takes <name>@<version>"),
        ("pack:csi-aws-ebs@1.65.0", &macros, "--macros takes"),
        (
            "aws-stack@1.1.1@draft",
            &validate,
            "--validate takes a profile version or --from-candidate, not a draft",
        ),
        (
            "cluster:c1",
            &validate,
            "--validate takes a profile version or --from-candidate: Palette checks",
        ),
        ("aws-stack@1.1.1", &shape, "--shape needs a cluster ref"),
        (
            "cluster:c1@pending",
            &raw_dir,
            "--raw-dir takes a profile or a cluster",
        ),
        (
            "edgehost:e1",
            &raw_dir,
            "--raw-dir takes a profile or a cluster",
        ),
        (
            "pack:csi-aws-ebs@1.65.0",
            &raw_dir,
            "--raw-dir takes a profile or a cluster. A record that strata reads from the directory \
             does not say pending, draft, edge host, cluster group, app profile, or pack",
        ),
        (
            "tag:env=dev",
            &content,
            "tag:env=dev names a set of clusters",
        ),
        ("filter:prod", &macros, "--macros takes"),
    ];
    for (text, options, message) in cases {
        let err = ops::snapshot_for(
            &Ref::parse(text),
            options,
            no_request,
            "t0",
            &Progress::silent(),
        )
        .unwrap_err();
        assert!(matches!(err, Error::Usage { .. }), "{text}: {err}");
        assert!(err.to_string().starts_with(message), "{text}: {err}");
    }
    // an option with a ref that it takes connects
    let connected = || -> strata::Result<Palette<MemoryApi>> {
        Err(Error::Usage {
            message: "connected".to_string(),
        })
    };
    for (text, options) in [
        ("cluster:c1", &shape),
        ("cluster:c1", &macros),
        ("clustergroup:g1", &macros),
        ("aws-stack@1.1.1", &validate),
        ("cluster:c1/aws-stack", &raw_dir),
        ("aws-stack@1.1.1", &raw_dir),
    ] {
        let err = ops::snapshot_for(
            &Ref::parse(text),
            options,
            connected,
            "t0",
            &Progress::silent(),
        )
        .unwrap_err();
        assert_eq!(err.to_string(), "connected", "{text}");
    }
}

#[test]
fn raw_profile_from_directory_matches_the_api_path() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let raw = RawProfile::load_dir(&dir.path().join("b")).unwrap();
    let from_raw = strata::snapshot::Snapshot::from_raw(&raw, "b", "t").unwrap();
    let palette = Palette::new(serve_fixture(MemoryApi::new(), dir.path(), "b", "x"));
    let fetched = palette.fetch_profile("x", &Progress::silent()).unwrap();
    let from_api = strata::snapshot::Snapshot::from_raw(&fetched, "b", "t").unwrap();
    assert_eq!(from_raw.root, from_api.root);
    assert_eq!(from_api.source.consumers.as_ref().unwrap().count, 2);
}

// ---- retries ---------------------------------------------------------------------------------

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use strata::api::{PaletteApi, Retry, RetryPolicy};
use strata::config::Config;
use strata::snapshot::{Snapshot, SubjectKind, Template, Tier, UpstreamSource};

/// A retrying wrapper over a borrowed API, with a recorder in place of `sleep`.
fn retrying(api: &MemoryApi, retries: u32) -> (Retry<&MemoryApi>, Rc<RefCell<Vec<Duration>>>) {
    retrying_with(api, RetryPolicy::new(retries))
}

/// [`retrying`] with a policy of the caller's.
fn retrying_with(
    api: &MemoryApi,
    policy: RetryPolicy,
) -> (Retry<&MemoryApi>, Rc<RefCell<Vec<Duration>>>) {
    let slept = Rc::new(RefCell::new(Vec::new()));
    let s = slept.clone();
    let retry = Retry::with_sleep(api, policy, Progress::silent(), move |d| {
        s.borrow_mut().push(d)
    });
    (retry, slept)
}

/// Fixture `a` served under [`UID_A`], with one 429 carrying `Retry-After: seconds` first.
fn throttled_once(raw_root: &Path, seconds: u64) -> MemoryApi {
    serve_fixture(MemoryApi::new(), raw_root, "a", UID_A)
        .failing_n_times(profile_path(), 1, 429, "slow down")
        .with_retry_after(profile_path(), seconds)
}

/// The document route of the fixture a.
fn profile_path() -> String {
    profile_route(UID_A)
}

#[test]
fn a_503_twice_then_success_yields_a_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A).failing_n_times(
        profile_path(),
        2,
        503,
        "busy",
    );
    let (retry, slept) = retrying(&api, 3);
    let snap = ops::snapshot_for(
        &Ref::Uid(UID_A.into()),
        &ops::SnapshotOptions::default(),
        || Ok(Palette::new(retry)),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(snap.source.uid, UID_A);
    assert_eq!(api.calls(&profile_path()), 3);
    assert_eq!(
        *slept.borrow(),
        vec![Duration::from_secs(1), Duration::from_secs(2)],
        "exponential backoff from one second"
    );
}

/// Palette takes one download of a profile version in 10 seconds.
#[test]
fn a_429_of_the_download_of_a_profile_version_waits_10_seconds_before_the_retry() {
    let download = common::download_route(UID_A);
    let refused = common::DOWNLOAD_LIMIT_REFUSAL;
    let api = MemoryApi::new()
        .with_file(download.as_str(), b"file".to_vec())
        .failing_n_times(download.as_str(), 1, 429, refused)
        .with(profile_path(), Value::Null)
        .failing_n_times(profile_path(), 1, 429, r#"{"code":"TooManyRequests"}"#);
    let (retry, slept) = retrying(&api, 3);
    assert_eq!(retry.get_file(&download).unwrap(), b"file");
    assert_eq!(retry.get_json(&profile_path()).unwrap(), Value::Null);
    // the first wait of another route is one second
    assert_eq!(
        *slept.borrow(),
        vec![Duration::from_secs(10), Duration::from_secs(1)]
    );
    // a Retry-After gives the wait of the download too
    let api = MemoryApi::new()
        .with_file(download.as_str(), b"file".to_vec())
        .failing_n_times(download.as_str(), 1, 429, refused)
        .with_retry_after(download.as_str(), 3);
    let (retry, slept) = retrying(&api, 3);
    assert_eq!(retry.get_file(&download).unwrap(), b"file");
    assert_eq!(*slept.borrow(), vec![Duration::from_secs(3)]);
}

#[test]
fn a_permanent_500_fails_with_the_last_error_after_the_configured_attempts() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A).failing_n_times(
        profile_path(),
        99,
        500,
        "broken",
    );
    let (retry, slept) = retrying(&api, 3);
    let palette = Palette::new(retry);
    let err = palette
        .fetch_profile(UID_A, &Progress::silent())
        .unwrap_err();
    assert!(matches!(err, Error::Api { status: 500, .. }), "{err}");
    assert_eq!(err.exit_code(), 2);
    assert_eq!(
        api.calls(&profile_path()),
        4,
        "one attempt plus three retries"
    );
    assert_eq!(slept.borrow().len(), 3);
}

#[test]
fn a_500_that_palette_gives_for_a_refusal_is_not_retried() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    // The body of a live refusal: Palette does not delete a profile that a template uses.
    let body = r#"{"code":"DeletionResourceInUseError","details":null,"message":"Unable to delete the resource as strata-live-template clustertemplate(s) in-use","ref":"x"}"#;
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A).failing_n_times(
        profile_path(),
        99,
        500,
        body,
    );
    let (retry, slept) = retrying(&api, 3);
    let err = Palette::new(retry)
        .fetch_profile(UID_A, &Progress::silent())
        .unwrap_err();
    assert!(matches!(err, Error::Api { status: 500, .. }), "{err}");
    assert_eq!(err.class(), FailureClass::Terminal);
    assert_eq!(api.calls(&profile_path()), 1, "one attempt and no retry");
    assert!(slept.borrow().is_empty());
}

#[test]
fn a_500_with_a_code_that_is_not_a_refusal_is_retried() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A).failing_n_times(
        profile_path(),
        99,
        500,
        r#"{"code":"DataReadError","message":"Data access error"}"#,
    );
    let (retry, slept) = retrying(&api, 3);
    let err = Palette::new(retry)
        .fetch_profile(UID_A, &Progress::silent())
        .unwrap_err();
    assert_eq!(err.class(), FailureClass::Transient);
    assert_eq!(
        api.calls(&profile_path()),
        4,
        "one attempt plus three retries"
    );
    assert_eq!(slept.borrow().len(), 3);
}

#[test]
fn retry_after_is_honored_on_429() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A)
        .failing_n_times(profile_path(), 1, 429, "slow down")
        .with_retry_after(profile_path(), 2);
    let (retry, slept) = retrying(&api, 3);
    let raw = Palette::new(retry)
        .fetch_profile(UID_A, &Progress::silent())
        .unwrap();
    assert!(Snapshot::from_raw(&raw, UID_A, "t0").is_ok());
    assert_eq!(*slept.borrow(), vec![Duration::from_secs(2)]);
}

#[test]
fn a_retry_after_above_the_ceiling_fails_at_once_without_waiting() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = throttled_once(dir.path(), 900);
    let (retry, slept) = retrying(&api, 3);
    let err = Palette::new(retry)
        .fetch_profile(UID_A, &Progress::silent())
        .unwrap_err();
    assert!(
        matches!(
            err,
            Error::RetryAfterTooLong {
                status: 429,
                retry_after: 900,
                limit: 300,
                ..
            }
        ),
        "{err}"
    );
    assert_eq!(err.kind(), "retry-after-too-long");
    assert_eq!(err.exit_code(), 2);
    let message = err.to_string();
    assert!(
        message.contains("900") && message.contains("300"),
        "names what the server asked for and the ceiling: {message}"
    );
    assert_eq!(api.calls(&profile_path()), 1, "no second call");
    assert!(slept.borrow().is_empty(), "nothing slept");
}

#[test]
fn a_retry_after_above_the_ceiling_names_the_method_of_the_request() {
    let key = "POST /v1/x";
    let api = MemoryApi::new()
        .failing_n_times(key, 1, 429, "slow down")
        .with_retry_after(key, 900);
    let (retry, _) = retrying(&api, 3);
    let err = retry.post_json("/v1/x", &Value::Null).unwrap_err();
    assert!(
        matches!(
            err,
            Error::RetryAfterTooLong {
                method: Method::Post,
                ..
            }
        ),
        "{err}"
    );
    assert!(
        err.to_string()
            .starts_with("POST /v1/x failed with HTTP 429 and Retry-After 900s"),
        "{err}"
    );
    assert_eq!(err.to_json()["method"], "POST");
}

#[test]
fn a_retry_after_at_the_ceiling_is_honored_as_sent() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = throttled_once(dir.path(), 300);
    let (retry, slept) = retrying(&api, 3);
    Palette::new(retry)
        .fetch_profile(UID_A, &Progress::silent())
        .unwrap();
    assert_eq!(*slept.borrow(), vec![Duration::from_secs(300)]);
}

#[test]
fn the_retry_after_ceiling_comes_from_the_config() {
    let config = Config::from_lookup(|key| match key {
        "PALETTE_API_KEY" => Some("k".to_string()),
        "STRATA_RETRIES" => Some("2".to_string()),
        "STRATA_MAX_RETRY_AFTER" => Some("900".to_string()),
        _ => None,
    })
    .unwrap();
    let policy = RetryPolicy::from_config(&config);
    assert_eq!(policy.retries, 2);
    assert_eq!(policy.max_retry_after, Duration::from_secs(900));

    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = throttled_once(dir.path(), 900);
    let (retry, slept) = retrying_with(&api, policy);
    Palette::new(retry)
        .fetch_profile(UID_A, &Progress::silent())
        .unwrap();
    assert_eq!(
        *slept.borrow(),
        vec![Duration::from_secs(900)],
        "fifteen minutes is within the raised ceiling"
    );
}

/// Palette 404s the variables endpoint on older versions, and strata reads the version as one with
/// no variables. That gives the root of a version with no variables, which is a real root for a
/// different thing. Nothing in the record used to say so, so a record taken before a Palette
/// upgrade failed `verify` afterwards and read as an edit in place. The record now carries the
/// flag, and the report prints a note.
#[test]
fn an_unavailable_variables_endpoint_is_recorded_and_reported() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let served = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A);
    let with_vars = Palette::new(served.clone())
        .fetch_profile(UID_A, &Progress::silent())
        .unwrap();
    assert!(!with_vars.variables_unavailable);

    let without = served.failing(variables_route(UID_A), 404, "gone");
    let raw = Palette::new(without)
        .fetch_profile(UID_A, &Progress::silent())
        .unwrap();
    assert!(
        raw.variables_unavailable,
        "the fallback must say that it happened"
    );

    let a = Snapshot::from_raw(&with_vars, "a", "t0").unwrap();
    let b = Snapshot::from_raw(&raw, "b", "t0").unwrap();
    assert!(b.source.variables_unavailable, "the record carries it");
    assert!(
        serde_json::to_value(&b).unwrap()["source"]["variablesUnavailable"] == true,
        "and the JSON does too"
    );

    // the roots differ for an infrastructure reason, and the report must say which
    let d = ops::compare(&a, &b, "t1");
    assert!(!d.same, "a missing endpoint changes the root");
    let text = common::terminal().render(&d, &a, &b, false).unwrap();
    assert!(
        text.contains("side B: Palette did not serve the variables endpoint"),
        "{text}"
    );
}

/// Answers the profile document once, then HTTP 404 for each path of the profile. This is a
/// profile that a person deletes after the first read and before strata reads its variables.
struct DeletedAfterFirstRead<'a> {
    inner: &'a MemoryApi,
    profile_reads: RefCell<u32>,
}

impl PaletteApi for DeletedAfterFirstRead<'_> {
    fn get_json(&self, path: &str) -> strata::Result<Value> {
        if path == profile_path() {
            *self.profile_reads.borrow_mut() += 1;
        }
        if *self.profile_reads.borrow() > 1 || path.ends_with("/variables") {
            return MemoryApi::new().get_json(path);
        }
        self.inner.get_json(path)
    }
}

#[test]
fn a_404_of_the_variables_route_of_a_deleted_profile_reads_as_gone_and_not_as_no_variables() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let served = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A);
    let api = || DeletedAfterFirstRead {
        inner: &served,
        profile_reads: RefCell::new(0),
    };

    // the record fails with the 404 of the profile document, and records no empty variables
    let err = Palette::new(api())
        .fetch_profile(UID_A, &Progress::silent())
        .unwrap_err();
    assert!(
        matches!(&err, Error::Api { status: 404, path, .. } if *path == profile_path()),
        "{err}"
    );

    // verify reports the version as gone, not as a version whose variables are unavailable
    let v = ops::verify(&recorded, &Palette::new(api()), "t1", &Progress::silent()).unwrap();
    let ver = v.diff.verify.as_ref().unwrap();
    assert!(ver.gone, "the profile is gone");
    assert!(!v.live.source.variables_unavailable, "{:?}", v.live.source);
}

#[test]
fn other_4xx_are_not_retried() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A).failing_n_times(
        profile_path(),
        5,
        404,
        "gone",
    );
    let (retry, slept) = retrying(&api, 3);
    let err = Palette::new(retry)
        .fetch_profile(UID_A, &Progress::silent())
        .unwrap_err();
    assert!(matches!(err, Error::Api { status: 404, .. }), "{err}");
    assert_eq!(api.calls(&profile_path()), 1);
    assert!(slept.borrow().is_empty());
}

/// Drops the connection `n` times, then delegates. Transport errors come from the network layer,
/// so the in-memory API cannot produce them; any [`PaletteApi`] can sit under [`Retry`].
struct Flaky<'a> {
    inner: &'a MemoryApi,
    drops: RefCell<u32>,
}

impl PaletteApi for Flaky<'_> {
    fn get_json(&self, path: &str) -> strata::Result<Value> {
        let mut left = self.drops.borrow_mut();
        if *left > 0 {
            *left -= 1;
            return Err(Error::Transport {
                method: Method::Get,
                path: path.to_string(),
                message: "connection reset".into(),
            });
        }
        self.inner.get_json(path)
    }
}

#[test]
fn transport_errors_are_retried() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A);
    let flaky = Flaky {
        inner: &api,
        drops: RefCell::new(2),
    };
    let retry = Retry::with_sleep(flaky, RetryPolicy::new(2), Progress::silent(), |_| {});
    assert!(
        Palette::new(retry)
            .fetch_profile(UID_A, &Progress::silent())
            .is_ok()
    );
}

#[test]
fn zero_retries_means_one_attempt() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A).failing_n_times(
        profile_path(),
        1,
        503,
        "busy",
    );
    let (retry, slept) = retrying(&api, 0);
    assert!(
        Palette::new(retry)
            .fetch_profile(UID_A, &Progress::silent())
            .is_err()
    );
    assert_eq!(api.calls(&profile_path()), 1);
    assert!(slept.borrow().is_empty());
}

// ---- project scope and bulk verify ------------------------------------------------------------

use std::path::PathBuf;

use strata::ops::RecordStatus;

#[test]
fn snapshot_for_records_the_project_scope_of_the_client() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A);
    let scoped = snapshot_of(&api.clone().in_project("p1"), UID_A).unwrap();
    assert_eq!(scoped.source.project_uid, "p1");
    let tenant = snapshot_of(&api, UID_A).unwrap();
    assert_eq!(tenant.source.project_uid, "");
    assert_eq!(
        scoped.root, tenant.root,
        "the scope is metadata, not part of the root"
    );

    // a record written before the field existed loads with an empty scope
    let mut v: Value = serde_json::from_str(&scoped.to_json(true).unwrap()).unwrap();
    v["source"]
        .as_object_mut()
        .unwrap()
        .remove("projectUid")
        .expect("the field is written");
    let old = dir.path().join("old.json");
    write_record(&old, &v);
    assert_eq!(Snapshot::load(&old).unwrap().source.project_uid, "");
}

#[test]
fn verify_many_checks_each_record_in_its_own_scope_and_counts_the_outcomes() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let base = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let write = |name: &str, snap: Snapshot| -> PathBuf {
        let p = dir.path().join(name);
        common::write_snapshot(&p, &snap, true);
        p
    };
    let rec_p1 = write("p1.json", base.clone().with_project(Some("p1")));
    let rec_p2 = write("p2.json", base.clone().with_project(Some("p2")));
    let not_a_record = dir.path().join("a/profile.json");

    // project p1 still serves A's content; in p2 the live object has B's content, an in-place edit
    let live_p1 = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A);
    let live_p2 = serve_fixture(MemoryApi::new(), dir.path(), "b", UID_A);
    let scopes: RefCell<Vec<Option<String>>> = RefCell::new(Vec::new());
    let connect = |project: Option<&str>| {
        scopes.borrow_mut().push(project.map(str::to_string));
        Ok(Palette::new(match project {
            Some("p1") => live_p1.clone(),
            _ => live_p2.clone(),
        }))
    };

    let paths = [rec_p1.clone(), rec_p2.clone(), not_a_record.clone()];
    let s = ops::verify_many(
        &paths,
        None,
        connect,
        "t1",
        &ops::VerifyOptions::default(),
        &Progress::silent(),
    );
    assert_eq!(s.schema, ops::VERIFY_SUMMARY_SCHEMA);
    let statuses: Vec<RecordStatus> = s.records.iter().map(|r| r.status).collect();
    assert_eq!(
        statuses,
        vec![
            RecordStatus::Unchanged,
            RecordStatus::Drifted,
            RecordStatus::Error
        ]
    );
    assert_eq!(
        (
            s.summary.records,
            s.summary.unchanged,
            s.summary.drifted,
            s.summary.errors
        ),
        (3, 1, 1, 1)
    );
    assert_eq!(s.summary.status, RecordStatus::Error);
    assert_eq!(s.exit_code(), 2, "an error outranks a drift");
    assert_eq!(
        *scopes.borrow(),
        vec![Some("p1".to_string()), Some("p2".to_string())],
        "one client per scope, and a file that is not a record never connects"
    );
    let drifted = &s.records[1];
    assert_eq!(drifted.path, rec_p2.display().to_string());
    assert_eq!(drifted.project_uid, "p2");
    assert_eq!(drifted.max_impact, Some(strata::diff::ImpactLevel::Repave));
    assert!(
        drifted
            .diff
            .as_ref()
            .expect("a drifted record carries its diff")
            .packs
            .iter()
            .any(|p| p.name == "kubernetes" && p.identity)
    );
    assert!(
        s.records[0].diff.is_none(),
        "an unchanged record carries no diff"
    );
    let failed = &s.records[2];
    assert_eq!(failed.error.as_ref().unwrap()["kind"], "not-snapshot");
    assert!(failed.name.is_empty());

    // every record unchanged is exit 0
    let s = ops::verify_many(
        &paths[..1],
        None,
        connect,
        "t1",
        &ops::VerifyOptions::default(),
        &Progress::silent(),
    );
    assert_eq!(s.summary.status, RecordStatus::Unchanged);
    assert_eq!(s.exit_code(), 0);

    // an explicit project overrides every recorded scope, so one client serves every record
    scopes.borrow_mut().clear();
    let s = ops::verify_many(
        &paths[..2],
        Some("p9"),
        connect,
        "t1",
        &ops::VerifyOptions::default(),
        &Progress::silent(),
    );
    assert_eq!(*scopes.borrow(), vec![Some("p9".to_string())]);
    assert_eq!(s.summary.status, RecordStatus::Drifted);
    assert_eq!(s.exit_code(), 1);

    // an empty project is the tenant scope: it also overrides the scope of each record
    scopes.borrow_mut().clear();
    ops::verify_many(
        &paths[..2],
        Some(""),
        connect,
        "t1",
        &ops::VerifyOptions::default(),
        &Progress::silent(),
    );
    assert_eq!(*scopes.borrow(), vec![Some(String::new())]);
}

#[test]
fn a_raw_directory_keeps_the_project_a_profile_or_cluster_was_fetched_in() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let silent = Progress::silent();
    let project_of = |saved: &Path, reference: &str| {
        ops::snapshot_from_dir(saved, reference, "t1")
            .unwrap()
            .source
            .project_uid
    };

    let profile =
        Palette::new(serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A).in_project("p1"));
    let saved = dir.path().join("raw-profile");
    profile
        .fetch_profile(UID_A, &silent)
        .unwrap()
        .save_dir(&saved, "t0")
        .unwrap();
    assert_eq!(project_of(&saved, UID_A), "p1");

    let cluster =
        Palette::new(serve_cluster(MemoryApi::new(), dir.path(), "cluster-a").in_project("p1"));
    let saved = dir.path().join("raw-cluster");
    cluster
        .fetch_subject(SubjectKind::Cluster, CLUSTER_UID, &silent)
        .unwrap()
        .save_dir(&saved, "t0")
        .unwrap();
    assert_eq!(project_of(&saved, &format!("cluster:{CLUSTER_UID}")), "p1");

    let tenant = Palette::new(serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A));
    let saved = dir.path().join("raw-tenant");
    tenant
        .fetch_profile(UID_A, &silent)
        .unwrap()
        .save_dir(&saved, "t0")
        .unwrap();
    assert_eq!(project_of(&saved, UID_A), "", "fetched at tenant scope");
    assert_eq!(
        project_of(&dir.path().join("a"), "a"),
        "",
        "a raw directory written before the scope was saved still loads, at tenant scope"
    );
}

#[test]
fn a_record_rebuilt_from_a_raw_directory_is_verified_in_its_own_project() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    // saved in project p1 before the version was edited in place
    let before =
        Palette::new(serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A).in_project("p1"));
    let saved = dir.path().join("raw");
    before
        .fetch_profile(UID_A, &Progress::silent())
        .unwrap()
        .save_dir(&saved, "t0")
        .unwrap();
    let record = dir.path().join("rebuilt.json");
    let rebuilt = ops::snapshot_from_dir(&saved, UID_A, "t1").unwrap();
    common::write_snapshot(&record, &rebuilt, true);

    // after the edit, p1 serves B's content under the same uid; tenant scope does not see it
    let edited = serve_fixture(MemoryApi::new(), dir.path(), "b", UID_A).in_project("p1");
    let scopes: RefCell<Vec<Option<String>>> = RefCell::new(Vec::new());
    let connect = |project: Option<&str>| {
        scopes.borrow_mut().push(project.map(str::to_string));
        Ok(Palette::new(match project {
            Some("p1") => edited.clone(),
            _ => MemoryApi::new(),
        }))
    };
    let s = ops::verify_many(
        &[record],
        None,
        connect,
        "t2",
        &ops::VerifyOptions::default(),
        &Progress::silent(),
    );
    assert_eq!(
        *scopes.borrow(),
        vec![Some("p1".to_string())],
        "checked in the saved project, with none given"
    );
    assert_eq!(
        s.records[0].status,
        RecordStatus::Drifted,
        "{:?}",
        s.records[0].error
    );
}

// ---- same release ----------------------------------------------------------------------------

use strata::ops::{PairStatus, Policy, SameReleaseOptions};

fn chain_input(dir: &Path, names: &[&str]) -> Vec<(String, Snapshot)> {
    names
        .iter()
        .map(|n| {
            (
                format!("{n}.json"),
                ops::snapshot_from_dir(&dir.join(n), n, "t0").unwrap(),
            )
        })
        .collect()
}

#[test]
fn same_release_passes_when_every_root_is_equal() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let c = ops::same_release(
        &chain_input(dir.path(), &["a", "a2", "a"]),
        Policy::Exact,
        "t1",
    );
    assert_eq!(c.schema, ops::CHAIN_SCHEMA);
    assert!(c.same);
    assert_eq!(c.exit_code(), 0);
    assert_eq!(c.pairs.len(), 2, "consecutive pairs in the order given");
    assert!(c.pairs.iter().all(|p| p.status == PairStatus::Identical));
    assert_eq!(
        (c.pairs[0].a.as_str(), c.pairs[0].b.as_str()),
        ("a.json", "a2.json")
    );
    assert_eq!(c.records.len(), 3);
    assert_eq!(c.records[0].label, "a.json");
    assert_eq!(c.summary.passed, 2);
}

#[test]
fn a_root_difference_fails_and_names_the_two_records() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let c = ops::same_release(&chain_input(dir.path(), &["a", "b"]), Policy::Exact, "t1");
    assert!(!c.same);
    assert_eq!(c.exit_code(), 1);
    let p = &c.pairs[0];
    assert_eq!(p.status, PairStatus::Different);
    assert_eq!((p.a.as_str(), p.b.as_str()), ("a.json", "b.json"));
    assert!(
        p.reasons.iter().any(|r| r.contains("variables changed")),
        "{:?}",
        p.reasons
    );
    assert!(
        p.reasons
            .iter()
            .any(|r| r.contains("addon/cert-manager added")),
        "{:?}",
        p.reasons
    );
    assert!(
        p.reasons
            .iter()
            .any(|r| r.contains("addon/external-dns removed")),
        "{:?}",
        p.reasons
    );
    assert!(
        p.reasons
            .iter()
            .any(|r| r.contains("k8s/kubernetes") && r.contains("1.29.4 -> 1.30.2")),
        "{:?}",
        p.reasons
    );
}

/// Fixture a with prometheus-operator stored the way Palette stores a manifest pack: no registry,
/// no digest, and a `packUid` minted for the profile. `rules` replaces its manifest's content.
fn with_manifest_pack(raw_root: &Path, dir: &str, pack_uid: &str, rules: Option<&str>) -> Snapshot {
    edited_a(raw_root, dir, "prometheus-operator", |pack, entry| {
        pack["type"] = Value::from("manifest");
        pack["registryUid"] = Value::from("");
        pack["packUid"] = Value::from(pack_uid);
        pack.as_object_mut().unwrap().remove("digest");
        entry["metadata"]["uid"] = Value::from(pack_uid);
        entry["spec"]["registryUid"] = Value::from("");
        entry["spec"]["digest"] = Value::from("");
        if let Some(content) = rules {
            entry["spec"]["manifests"][0]["content"] = Value::from(content);
        }
    })
}

/// Fixture a with its pack `name` changed by `edit`, saved as the raw directory `dir` and read
/// as a record.
///
/// `edit` gets the pack reference of the profile and the entry of the pack in the manifests
/// listing. The variables stay those of a.
fn edited_a(
    raw_root: &Path,
    dir: &str,
    name: &str,
    edit: impl FnOnce(&mut Value, &mut Value),
) -> Snapshot {
    let (from, to) = (raw_root.join("a"), raw_root.join(dir));
    fs::create_dir_all(&to).unwrap();
    let mut profile = read_json(&from.join("profile.json"));
    let mut manifests = read_json(&from.join("manifests.json"));
    let pack = profile["spec"]["published"]["packs"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["name"] == name)
        .unwrap();
    let entry = manifests["spec"]["packs"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["spec"]["name"] == name)
        .unwrap();
    edit(pack, entry);
    for (file, doc) in [("profile.json", &profile), ("manifests.json", &manifests)] {
        fs::write(to.join(file), serde_json::to_string_pretty(doc).unwrap()).unwrap();
    }
    fs::copy(from.join("variables.json"), to.join("variables.json")).unwrap();
    ops::snapshot_from_dir(&to, dir, "t0").unwrap()
}

#[test]
fn two_profiles_with_the_same_manifest_content_are_one_release_whatever_uid_palette_minted() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let one = with_manifest_pack(dir.path(), "m1", "pk-minted-1", None);
    let two = with_manifest_pack(dir.path(), "m2", "pk-minted-2", None);
    let edited = with_manifest_pack(dir.path(), "m3", "pk-minted-3", Some("groups: []\n"));

    let d = ops::compare(&one, &two, "t1");
    assert!(
        !d.same,
        "the minted uid is part of IDENTITY, so the roots differ"
    );
    assert!(d.same_content, "and nothing else differs");
    let prom = d
        .packs
        .iter()
        .find(|p| p.name == "prometheus-operator")
        .unwrap();
    assert!(prom.identity && prom.minted_uid_only, "{prom:?}");
    let text = common::terminal().render(&d, &one, &two, false).unwrap();
    assert!(
        text.contains("the roots differ only by the packUid that Palette mints: the same content"),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|l| l.starts_with("root ") && l.ends_with("SAME CONTENT")),
        "{text}"
    );

    for policy in [Policy::Exact, Policy::RegistryMove] {
        let c = ops::same_release(
            &[
                ("m1.json".into(), one.clone()),
                ("m2.json".into(), two.clone()),
            ],
            policy,
            "t1",
        );
        assert!(c.same, "{:?}", c.pairs[0].reasons);
        assert_eq!(c.pairs[0].status, PairStatus::SameContent);
        assert!(c.pairs[0].reasons.is_empty());
    }

    // a manifest content change is still a different release
    let d = ops::compare(&one, &edited, "t1");
    assert!(!d.same_content);
    let c = ops::same_release(
        &[("m1.json".into(), one.clone()), ("m3.json".into(), edited)],
        Policy::Exact,
        "t1",
    );
    assert!(!c.same);
    assert!(
        c.pairs[0]
            .reasons
            .iter()
            .any(|r| r.contains("prometheus-operator") && r.contains("AUTHORED")),
        "{:?}",
        c.pairs[0].reasons
    );
    assert!(
        c.pairs[0].reasons.iter().all(|r| !r.contains("packUid")),
        "the minted uid is not a reason: {:?}",
        c.pairs[0].reasons
    );

    // a pack from a registry whose packUid changed is not a minted uid
    let a = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let a3 = ops::snapshot_from_dir(&dir.path().join("a3"), "a3", "t0").unwrap();
    assert!(!ops::compare(&a, &a3, "t1").same_content);
}

/// A uid that Palette mints changes nothing on a cluster. So `compare` prints `IDENTITY` and no
/// impact for the pack, and exits 1 for the roots (#624).
#[test]
fn a_pack_whose_only_difference_is_a_uid_that_palette_mints_has_no_impact() {
    use strata::diff::ImpactLevel;
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let one = with_manifest_pack(dir.path(), "m1", "pk-minted-1", None);
    let two = with_manifest_pack(dir.path(), "m2", "pk-minted-2", None);
    let edited = with_manifest_pack(dir.path(), "m3", "pk-minted-3", Some("groups: []\n"));

    let d = ops::compare(&one, &two, "t1");
    assert!(d.same_content && !d.nothing_differs());
    let prom = d
        .packs
        .iter()
        .find(|p| p.name == "prometheus-operator")
        .unwrap();
    assert!(prom.identity && prom.minted_uid_only, "{prom:?}");
    assert_eq!(
        (prom.impact.level, prom.impact.text.as_str()),
        (ImpactLevel::None, "")
    );
    assert_eq!(d.summary.max_impact, ImpactLevel::None);
    let text = common::terminal().render(&d, &one, &two, false).unwrap();
    assert!(
        text.lines()
            .any(|l| l.contains("prometheus-operator") && l.ends_with("IDENTITY")),
        "{text}"
    );
    assert!(!text.contains("impact:"), "{text}");
    let md = common::markdown().render(&d, &one, &two, false).unwrap();
    assert!(!md.contains("in-place"), "{md}");

    // a changed manifest still has the impact of its layer
    let d = ops::compare(&one, &edited, "t1");
    let prom = d
        .packs
        .iter()
        .find(|p| p.name == "prometheus-operator")
        .unwrap();
    assert!(!prom.minted_uid_only);
    assert_eq!(prom.impact.level, ImpactLevel::InPlace);
}

/// Fixture A with its `ubuntu-aws` pack changed by `edit` in the profile, saved as the raw
/// directory `dir`; the pack's uid in the manifests listing follows the profile's `packUid`.
fn with_os_pack(raw_root: &Path, dir: &str, edit: impl Fn(&mut Value)) -> Snapshot {
    edited_a(raw_root, dir, "ubuntu-aws", |pack, entry| {
        edit(pack);
        entry["metadata"]["uid"] = pack["packUid"].clone();
    })
}

#[test]
fn two_profiles_that_pin_one_pack_build_under_different_tag_names_are_the_same_content() {
    use strata::diff::ImpactLevel;
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let a = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();

    // the registry's other names for version 22.04: an exact tag and a floating one
    for tag in ["LTS__22.04", "LTS__22.x"] {
        let retagged = with_os_pack(dir.path(), tag, |p| p["tag"] = Value::from(tag));
        let d = ops::compare(&a, &retagged, "t1");
        assert!(!d.same, "the tag is part of IDENTITY, so the roots differ");
        assert!(d.same_content, "and nothing else differs");
        let os = d.packs.iter().find(|p| p.name == "ubuntu-aws").unwrap();
        assert!(os.identity && os.tag_renamed, "{os:?}");
        assert_eq!(os.impact.level, ImpactLevel::None);
        assert_eq!(os.impact.text, "none (tag renamed, same pack build)");
        assert_eq!(d.summary.max_impact, ImpactLevel::None);

        let text = common::terminal().render(&d, &a, &retagged, false).unwrap();
        assert!(text.contains(&format!("tag 22.04 -> {tag}")), "{text}");
        assert!(
            text.lines()
                .any(|l| l.starts_with("root ") && l.ends_with("SAME CONTENT")),
            "{text}"
        );
        assert!(
            text.contains(
                "the roots differ only by the tag name a pack is pinned under: the same pack build"
            ),
            "{text}"
        );
        assert!(
            text.contains("impact: none (tag renamed, same pack build)"),
            "{text}"
        );
        assert!(!text.contains("repave"), "{text}");
        let md = common::markdown().render(&d, &a, &retagged, false).unwrap();
        assert!(md.contains(&format!("tag 22.04 → {tag}")), "{md}");
        assert!(md.contains("**Result:** SAME CONTENT"), "{md}");
        let json: Value = serde_json::from_str(
            &render::renderer(Format::Json, Paint::off(), true)
                .render(&d, &a, &retagged, false)
                .unwrap(),
        )
        .unwrap();
        let os = json["packs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == "ubuntu-aws")
            .unwrap();
        assert_eq!(os["tagRenamed"], true, "{os}");

        for policy in [Policy::Exact, Policy::RegistryMove] {
            let c = ops::same_release(
                &[
                    ("a.json".into(), a.clone()),
                    ("retagged.json".into(), retagged.clone()),
                ],
                policy,
                "t1",
            );
            assert!(c.same, "{:?}", c.pairs[0].reasons);
            assert_eq!(c.pairs[0].status, PairStatus::SameContent);
            assert!(c.pairs[0].reasons.is_empty());
            assert_eq!(c.pairs[0].packs_retagged, 1);
            let text = common::terminal().render_chain(&c).unwrap();
            assert!(
                text.contains("1 pack pinned under another tag name, the same build"),
                "{text}"
            );
        }
    }

    // a tag rename with a values change: the values set the impact, and the rename is no reason
    let edited = with_os_pack(dir.path(), "retag-values", |p| {
        p["tag"] = Value::from("LTS__22.04");
        p["values"] = Value::from("kubeadmconfig:\n  preKubeadmCommands:\n  - echo strata\n");
    });
    let d = ops::compare(&a, &edited, "t1");
    assert!(!d.same_content);
    let os = d.packs.iter().find(|p| p.name == "ubuntu-aws").unwrap();
    assert!(os.tag_renamed && os.authored, "{os:?}");
    assert_eq!(os.impact.level, ImpactLevel::Repave);
    let c = ops::same_release(
        &[("a.json".into(), a.clone()), ("edited.json".into(), edited)],
        Policy::Exact,
        "t1",
    );
    assert_eq!(c.pairs[0].status, PairStatus::Different);
    assert!(
        c.pairs[0]
            .reasons
            .iter()
            .any(|r| r.contains("os/ubuntu-aws") && r.contains("AUTHORED")),
        "{:?}",
        c.pairs[0].reasons
    );
    assert!(
        c.pairs[0].reasons.iter().all(|r| !r.contains("tag")),
        "a renamed tag is not a reason: {:?}",
        c.pairs[0].reasons
    );

    // another build under another tag is still an IDENTITY change: a node repave
    let other = with_os_pack(dir.path(), "other-build", |p| {
        p["tag"] = Value::from("LTS__24.04");
        p["version"] = Value::from("24.04");
        p["packUid"] = Value::from("pk-os-2404");
    });
    let d = ops::compare(&a, &other, "t1");
    assert!(!d.same_content);
    let os = d.packs.iter().find(|p| p.name == "ubuntu-aws").unwrap();
    assert!(os.identity && !os.tag_renamed, "{os:?}");
    assert_eq!(os.impact.level, ImpactLevel::Repave);
    let c = ops::same_release(
        &[("a.json".into(), a.clone()), ("other.json".into(), other)],
        Policy::Exact,
        "t1",
    );
    assert!(
        c.pairs[0]
            .reasons
            .iter()
            .any(|r| r == "pack os/ubuntu-aws IDENTITY: version 22.04 -> 24.04"),
        "{:?}",
        c.pairs[0].reasons
    );
}

#[test]
fn an_os_pack_from_another_registry_at_the_same_version_is_no_node_repave() {
    use strata::diff::ImpactLevel;
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let a = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();

    // the registry uid alone moved
    let moved = with_os_pack(dir.path(), "registry-only", |p| {
        p["registryUid"] = Value::from("reg-oci");
    });
    let d = ops::compare(&a, &moved, "t1");
    let os = d.packs.iter().find(|p| p.name == "ubuntu-aws").unwrap();
    assert!(os.identity && !os.upstream && !os.authored, "{os:?}");
    assert_eq!(os.impact.level, ImpactLevel::None);
    assert_eq!(
        os.impact.text,
        "none (the same version from another pack reference)"
    );
    assert_eq!(d.summary.max_impact, ImpactLevel::None);

    // a move between two kinds of registry: the type, the uids, and the digest move
    let moved = with_os_pack(dir.path(), "registry-kind", |p| {
        p["type"] = Value::from("oci");
        p["registryUid"] = Value::from("reg-oci");
        p["packUid"] = Value::from("pk-os-oci");
        p["digest"] = Value::from("sha256:e5982cf2");
    });
    let d = ops::compare(&a, &moved, "t1");
    let os = d.packs.iter().find(|p| p.name == "ubuntu-aws").unwrap();
    assert!(os.identity && os.upstream && !os.authored, "{os:?}");
    assert_eq!(os.impact.level, ImpactLevel::Sync);
    assert_eq!(d.summary.max_impact, ImpactLevel::Sync);
    let text = common::terminal().render(&d, &a, &moved, false).unwrap();
    assert!(
        text.contains(
            "impact: the same version from another pack reference, applied on next profile sync"
        ),
        "{text}"
    );
    assert!(!text.contains("repave"), "{text}");
}

#[test]
fn verify_reports_a_tag_renamed_in_place_as_drift_with_no_node_impact() {
    use strata::diff::ImpactLevel;
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    // the published version under A's uid now pins the same build under another tag name
    with_os_pack(dir.path(), "retagged", |p| {
        p["tag"] = Value::from("LTS__22.04")
    });
    let palette = Palette::new(serve_fixture(
        MemoryApi::new(),
        dir.path(),
        "retagged",
        UID_A,
    ));
    let v = ops::verify(&recorded, &palette, "t1", &Progress::silent()).unwrap();
    assert!(!v.unchanged, "an edit to a published version is drift");
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.status, VerifyStatus::Drifted);
    assert!(v.diff.same_content);
    assert_eq!(v.diff.summary.max_impact, ImpactLevel::None);
    let text = common::terminal()
        .render(&v.diff, &recorded, &v.live, false)
        .unwrap();
    assert!(text.contains("tag 22.04 -> LTS__22.04"), "{text}");
    assert!(
        text.contains(
            "the roots differ only by the tag name a pack is pinned under: the same pack build"
        ),
        "{text}"
    );
    assert!(!text.contains("repave"), "{text}");
    let verdict = common::terminal().verdict(&v.diff).unwrap();
    assert!(verdict.starts_with("DRIFT: "), "{verdict}");
}

#[test]
fn a_registry_move_passes_only_when_allowed() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    // a3 is a re-pinned from another registry: every pack has IDENTITY UPSTREAM and nothing else
    let exact = ops::same_release(&chain_input(dir.path(), &["a", "a3"]), Policy::Exact, "t1");
    assert_eq!(exact.pairs[0].status, PairStatus::Different);
    assert!(
        exact.pairs[0]
            .reasons
            .iter()
            .any(|r| r.contains("os/ubuntu-aws")
                && r.contains("moved registry reg-public -> reg-prod")),
        "{:?}",
        exact.pairs[0].reasons
    );
    let allowed = ops::same_release(
        &chain_input(dir.path(), &["a", "a3"]),
        Policy::RegistryMove,
        "t1",
    );
    assert!(allowed.same);
    assert_eq!(allowed.exit_code(), 0);
    assert_eq!(allowed.pairs[0].status, PairStatus::RegistryMove);
    assert_eq!(allowed.pairs[0].packs_repinned, 7);
    assert!(allowed.pairs[0].reasons.is_empty());
    // roots still differ, and the document says so
    assert_ne!(allowed.records[0].root, allowed.records[1].root);
}

#[test]
fn a_registry_move_passes_when_the_pack_type_follows_the_new_registry() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    // a5 is a3 with every pack typed oci, as a real move from a pack registry to an OCI one is
    let allowed = ops::same_release(
        &chain_input(dir.path(), &["a", "a5"]),
        Policy::RegistryMove,
        "t1",
    );
    assert!(
        allowed.pairs[0].reasons.is_empty(),
        "{:?}",
        allowed.pairs[0].reasons
    );
    assert_eq!(allowed.pairs[0].status, PairStatus::RegistryMove);
    assert_eq!(allowed.pairs[0].packs_repinned, 7);
    let exact = ops::same_release(&chain_input(dir.path(), &["a", "a5"]), Policy::Exact, "t1");
    assert!(
        exact.pairs[0]
            .reasons
            .iter()
            .any(|r| r.contains("os/ubuntu-aws")
                && r.contains("moved registry reg-public -> reg-prod")),
        "{:?}",
        exact.pairs[0].reasons
    );
    // a pack that came from no registry did not move: a manifest turned registry pack is not a move
    let c = ops::same_release(
        &chain_input(dir.path(), &["a6", "a3"]),
        Policy::RegistryMove,
        "t1",
    );
    assert_eq!(c.pairs[0].status, PairStatus::Different);
    assert!(
        c.pairs[0]
            .reasons
            .iter()
            .any(|r| r.contains("type manifest -> spectro")),
        "{:?}",
        c.pairs[0].reasons
    );
}

#[test]
fn an_authored_change_during_a_registry_move_fails_and_names_the_pack_and_the_leaf() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    // a4 is a3 with one value edited on the kubernetes pack
    let c = ops::same_release(
        &chain_input(dir.path(), &["a", "a4"]),
        Policy::RegistryMove,
        "t1",
    );
    assert!(!c.same);
    assert_eq!(c.exit_code(), 1);
    let p = &c.pairs[0];
    assert_eq!(p.status, PairStatus::Different);
    assert_eq!(
        p.reasons.len(),
        1,
        "only the violation is a reason: {:?}",
        p.reasons
    );
    assert!(p.reasons[0].contains("k8s/kubernetes"), "{}", p.reasons[0]);
    assert!(p.reasons[0].contains("AUTHORED"), "{}", p.reasons[0]);
    assert!(
        p.reasons[0].contains("kubeadmconfig.clusterConfiguration.networking.podSubnet"),
        "{}",
        p.reasons[0]
    );
    // the other direction of the same edit, and a plain content change, are not registry moves either
    let c = ops::same_release(
        &chain_input(dir.path(), &["a", "b"]),
        Policy::RegistryMove,
        "t1",
    );
    assert_eq!(c.pairs[0].status, PairStatus::Different);
}

// ---- clusters --------------------------------------------------------------------------------

/// An API that serves the cluster fixture `dir` under `CLUSTER_UID`.
fn serve_cluster(api: MemoryApi, raw_root: &Path, dir: &str) -> MemoryApi {
    let d = raw_root.join(dir);
    // the profile documents of the profiles the cluster runs, as they are published: verify of a
    // cluster record reads them for their current version
    let mut api = api;
    for template in read_json(&d.join("cluster.json"))["spec"]["clusterProfileTemplates"]
        .as_array()
        .unwrap()
    {
        let uid = template["uid"].as_str().unwrap();
        for fixture in ["a", "b", "addons"] {
            let profile = read_json(&raw_root.join(fixture).join("profile.json"));
            if profile["metadata"]["uid"] == uid {
                api = api.with(profile_route(uid), profile);
                break;
            }
        }
    }
    api.with(
        cluster_route(CLUSTER_UID),
        read_json(&d.join("cluster.json")),
    )
    .with(
        cluster_manifests_route(CLUSTER_UID),
        read_json(&d.join("manifests.json")),
    )
    .with(
        cluster_variables_route(CLUSTER_UID),
        read_json(&d.join("variables.json")),
    )
}

/// The record of the cluster under `CLUSTER_UID`, or of its profile `profile` when the test names
/// one.
fn snapshot_cluster(api: &MemoryApi, profile: Option<&str>) -> strata::Result<Snapshot> {
    snapshot_of_ref(api, &cluster_ref(CLUSTER_UID, profile, false), "t0")
}

/// The record of the update that waits for the cluster under `CLUSTER_UID`, or of its profile
/// `profile` when the test names one.
fn snapshot_pending(api: &MemoryApi, profile: Option<&str>) -> strata::Result<Snapshot> {
    snapshot_of_ref(api, &cluster_ref(CLUSTER_UID, profile, true), "t0")
}

#[test]
fn a_cluster_running_the_recorded_version_is_identical_to_the_record() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded_b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    let recorded_a = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();

    let on_b = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-b"),
        None,
    )
    .unwrap();
    assert_eq!(on_b.root, recorded_b.root, "same release, same root");
    assert!(ops::compare(&recorded_b, &on_b, "t1").same);
    let d = ops::compare(&recorded_a, &on_b, "t1");
    assert!(!d.same);
    assert!(d.packs.iter().any(|p| p.name == "kubernetes" && p.identity));

    let c = on_b
        .source
        .cluster
        .as_ref()
        .expect("a cluster snapshot says so");
    assert_eq!(
        (c.uid.as_str(), c.name.as_str()),
        (CLUSTER_UID, "lab-cluster")
    );
    assert_eq!(c.profiles.len(), 1);
    assert_eq!(
        (c.profiles[0].name.as_str(), c.profiles[0].version.as_str()),
        ("lab-profile", "1.3.0")
    );
    assert_eq!(c.profile, None);
    assert_eq!(on_b.source.template, Template::Applied);
    assert_eq!(on_b.source.reference, format!("cluster:{CLUSTER_UID}"));
    assert_eq!(
        (on_b.source.name.as_str(), on_b.source.version.as_str()),
        ("lab-profile", "1.3.0")
    );
    assert_eq!(
        on_b.source.title(),
        "lab-profile@1.3.0 on cluster lab-cluster"
    );
    assert!(
        !on_b.to_json(true).unwrap().contains(SECRET_VALUE),
        "a sensitive variable value is never recorded"
    );
    assert!(recorded_b.source.cluster.is_none());
    assert_eq!(recorded_b.source.title(), "lab-profile@1.3.0");
}

#[test]
fn a_profile_on_the_cluster_is_selected_by_name_and_the_rest_are_merged() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded_b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    let addons = ops::snapshot_from_dir(&dir.path().join("addons"), "addons", "t0").unwrap();
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-merged");

    let selected = snapshot_cluster(&api, Some("lab-profile")).unwrap();
    assert_eq!(selected.root, recorded_b.root);
    assert_eq!(
        selected.source.cluster.as_ref().unwrap().profile.as_deref(),
        Some("lab-profile")
    );
    assert_eq!(
        selected.source.reference,
        format!("cluster:{CLUSTER_UID}/lab-profile")
    );
    let other = snapshot_cluster(&api, Some("lab-addons")).unwrap();
    assert_eq!(other.root, addons.root);

    let merged = snapshot_cluster(&api, None).unwrap();
    assert_eq!(
        merged.packs.len(),
        recorded_b.packs.len() + addons.packs.len(),
        "every applied profile, in order"
    );
    assert_eq!(merged.packs.last().unwrap().name, "nginx-ingress");
    assert_eq!(merged.variables.len(), recorded_b.variables.len() + 1);
    let c = merged.source.cluster.as_ref().unwrap();
    assert_eq!(c.profiles.len(), 2);
    assert_eq!(
        merged.source.uid, "",
        "no single profile uid for a merged snapshot"
    );
    assert_eq!(merged.source.name, "lab-profile+lab-addons");
    assert_eq!(merged.source.version, "1.3.0+2.0.0");

    let err = snapshot_cluster(&api, Some("nope")).unwrap_err();
    match &err {
        Error::ClusterProfileNotFound {
            cluster,
            profile,
            applied,
        } => {
            assert_eq!((cluster.as_str(), profile.as_str()), (CLUSTER_UID, "nope"));
            assert_eq!(
                applied,
                &vec!["lab-profile".to_string(), "lab-addons".to_string()]
            );
        }
        other => panic!("unexpected error {other}"),
    }
    assert_eq!(err.exit_code(), 2);
    assert!(
        err.to_string().contains("applied: lab-profile, lab-addons"),
        "{err}"
    );
}

/// The record of a cluster that runs two profiles joins their names and their versions, and a
/// reader cannot pair each name with its version. So the heading and the verdict give the count
/// of the profiles (#624).
#[test]
fn the_heading_of_a_record_of_two_profiles_gives_the_count_of_the_profiles() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-merged");
    let merged = snapshot_cluster(&api, None).unwrap();
    let selected = snapshot_cluster(&api, Some("lab-profile")).unwrap();
    assert_eq!(merged.source.name, "lab-profile+lab-addons");
    assert_eq!(merged.source.title(), "2 profiles on cluster lab-cluster");
    assert_eq!(
        selected.source.title(),
        "lab-profile@1.3.0 on cluster lab-cluster"
    );

    let (terminal, markdown) = (common::terminal(), common::markdown());
    let d = ops::compare(&selected, &merged, "t1");
    let text = terminal.render(&d, &selected, &merged, false).unwrap();
    let heading = text.lines().next().unwrap();
    assert!(
        heading.starts_with("lab-profile@1.3.0 on cluster lab-cluster (uid ")
            && heading.contains("  ->  2 profiles on cluster lab-cluster (uid "),
        "{heading}"
    );
    let md = markdown.render(&d, &selected, &merged, false).unwrap();
    assert!(
        md.contains(
            "### lab-profile@1.3.0 on cluster lab-cluster → 2 profiles on cluster lab-cluster\n"
        ),
        "{md}"
    );
    for out in [&text, &md] {
        assert!(!out.contains("lab-profile+lab-addons"), "{out}");
    }

    let v = ops::verify(&merged, &Palette::new(&api), "t1", &Progress::silent()).unwrap();
    for r in [&terminal, &markdown] {
        let verdict = r.verdict(&v.diff).unwrap();
        assert!(
            verdict.contains("cluster lab-cluster running 2 profiles"),
            "{verdict}"
        );
    }
}

#[test]
fn a_compare_against_a_cluster_gives_the_consumers_of_the_profile_side_alone() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let mut recorded = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    let on_cluster = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-b"),
        None,
    )
    .unwrap();
    assert_eq!(recorded.source.consumers.as_ref().unwrap().count, 2);
    assert!(
        on_cluster.source.consumers.is_none(),
        "no request asks which clusters use a cluster, so the record carries no count at all"
    );
    let text = |a: &Snapshot, b: &Snapshot| {
        let d = ops::compare(a, b, "t1");
        common::terminal().render(&d, a, b, false).unwrap()
    };

    // Palette does not list this cluster for the version: no arrow, and the count of the cluster
    // side stays out of the report.
    let t = text(&recorded, &on_cluster);
    assert!(
        t.contains(
            "consumers  lab-profile@1.3.0 has 2 consumers; lab-cluster is not one of them\n"
        ),
        "{t}"
    );
    assert!(!t.contains("-> 0 clusters"), "{t}");

    // Palette lists it, whichever side the cluster is on.
    let consumers = recorded.source.consumers.as_mut().unwrap();
    consumers.clusters.push(CLUSTER_UID.to_string());
    consumers.count = 3;
    let t = text(&recorded, &on_cluster);
    assert!(
        t.contains("consumers  lab-profile@1.3.0 has 3 consumers; lab-cluster is one of them\n"),
        "{t}"
    );
    let t = text(&on_cluster, &recorded);
    assert!(
        t.contains("consumers  lab-profile@1.3.0 has 3 consumers; lab-cluster is one of them\n"),
        "{t}"
    );

    // Two clusters, or two packs, have no consumers at all: the report says nothing.
    let t = text(&on_cluster, &on_cluster);
    assert!(!t.contains("consumers"), "{t}");

    // The markdown report drops the row of counts for the same reason.
    let d = ops::compare(&recorded, &on_cluster, "t1");
    let md = common::markdown()
        .render(&d, &recorded, &on_cluster, false)
        .unwrap();
    assert!(!md.contains("| consumers |"), "{md}");
    assert!(
        md.contains("**Consumers:** lab-profile@1.3.0 has 3 consumers; lab-cluster is one of them"),
        "{md}"
    );

    // Two profile versions still compare their counts.
    let other = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let t = text(&other, &recorded);
    assert!(t.contains("consumers  0 -> 3 clusters\n"), "{t}");
}

#[test]
fn verify_reports_a_cluster_record_whose_profile_version_was_overwritten_or_deleted() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-a"),
        None,
    )
    .unwrap();
    let verify = |api: MemoryApi| {
        ops::verify(&recorded, &Palette::new(api), "t1", &Progress::silent()).unwrap()
    };

    // the cluster and its profile as recorded: one extra request, for the one profile it runs
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a");
    let v = ops::verify(&recorded, &Palette::new(&api), "t1", &Progress::silent()).unwrap();
    assert_eq!(api.calls(&profile_route(UID_A)), 1);
    assert!(v.unchanged);
    assert!(
        v.diff
            .verify
            .as_ref()
            .unwrap()
            .profiles_overwritten
            .is_empty()
    );

    // a legacy version bump renamed the profile the cluster runs; the cluster did not change
    let mut renamed = read_json(&dir.path().join("a/profile.json"));
    renamed["spec"]["version"] = Value::from("1.3.0");
    let v = verify(
        serve_cluster(MemoryApi::new(), dir.path(), "cluster-a")
            .with(profile_route(UID_A), renamed),
    );
    assert_eq!(v.live.root, recorded.root, "the cluster runs what it ran");
    assert!(!v.unchanged, "the version it runs is gone from Palette");
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.status, VerifyStatus::Drifted);
    let named: Vec<(&str, &str, &str, &str)> = ver
        .profiles_overwritten
        .iter()
        .map(|o| {
            (
                o.uid.as_str(),
                o.name.as_str(),
                o.cluster_version.as_str(),
                o.profile_version.as_str(),
            )
        })
        .collect();
    assert_eq!(named, [(UID_A, "lab-profile", "1.2.0", "1.3.0")]);
    let verdict = common::terminal().verdict(&v.diff).unwrap();
    assert_eq!(
        verdict,
        format!(
            "DRIFT: cluster lab-cluster runs lab-profile@1.2.0, but uid {UID_A} now names 1.3.0: the version was overwritten in place"
        )
    );

    // the profile was deleted
    let v = verify(
        MemoryApi::new()
            .with(
                cluster_route(CLUSTER_UID),
                read_json(&dir.path().join("cluster-a/cluster.json")),
            )
            .with(
                cluster_manifests_route(CLUSTER_UID),
                read_json(&dir.path().join("cluster-a/manifests.json")),
            )
            .with(
                cluster_variables_route(CLUSTER_UID),
                read_json(&dir.path().join("cluster-a/variables.json")),
            ),
    );
    assert!(!v.unchanged);
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.profiles_overwritten.len(), 1);
    assert_eq!(
        ver.profiles_overwritten[0].profile_version, "",
        "no version: the uid is gone"
    );
    let verdict = common::terminal().verdict(&v.diff).unwrap();
    assert!(
        verdict.ends_with(&format!("but uid {UID_A} no longer exists")),
        "{verdict}"
    );

    // any other error reading a profile stops verify rather than guessing
    let err = ops::verify(
        &recorded,
        &Palette::new(
            serve_cluster(MemoryApi::new(), dir.path(), "cluster-a").failing(
                profile_route(UID_A),
                500,
                "boom",
            ),
        ),
        "t1",
        &Progress::silent(),
    );
    assert!(err.is_err(), "{err:?}");
}

#[test]
fn verify_refetches_the_cluster_for_a_cluster_record() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-b"),
        None,
    )
    .unwrap();

    let still_b = Palette::new(serve_cluster(MemoryApi::new(), dir.path(), "cluster-b"));
    let v = ops::verify(&recorded, &still_b, "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged);
    assert_eq!(v.diff.verify.as_ref().unwrap().uid, CLUSTER_UID);

    // the same cluster was moved back to a: the run no longer matches its record
    let now_a = Palette::new(serve_cluster(MemoryApi::new(), dir.path(), "cluster-a"));
    let v = ops::verify(&recorded, &now_a, "t1", &Progress::silent()).unwrap();
    assert!(!v.unchanged);
    assert_eq!(
        v.diff.verify.as_ref().unwrap().status,
        VerifyStatus::Drifted
    );
    assert!(v.live.source.cluster.is_some());

    // a cluster without the variables endpoint still snapshots, with no variables
    let d = dir.path().join("cluster-b");
    let no_vars = MemoryApi::new()
        .with(
            cluster_route(CLUSTER_UID),
            read_json(&d.join("cluster.json")),
        )
        .with(
            cluster_manifests_route(CLUSTER_UID),
            read_json(&d.join("manifests.json")),
        );
    assert!(
        snapshot_cluster(&no_vars, None)
            .unwrap()
            .variables
            .is_empty()
    );
}

// ---- the policy tier -------------------------------------------------------------------------

/// The route of the role bindings of the fixture cluster.
fn rbacs_route() -> String {
    cluster_rbacs_route(CLUSTER_UID)
}

/// The route of the namespaces of the fixture cluster.
fn namespaces_route() -> String {
    cluster_namespaces_route(CLUSTER_UID)
}

/// Cluster b with the five settings of the policy tier, and one role binding to `subjects` and
/// one namespace on their own routes, as Palette gives them.
fn serve_policy(dir: &Path, subjects: Value) -> MemoryApi {
    let mut cluster = read_json(&dir.join("cluster-b").join("cluster.json"));
    cluster["metadata"]["annotations"] =
        serde_json::json!({"spectroComponentsUpgradeForbidden": "true"});
    cluster["spec"]["clusterConfig"] = serde_json::json!({
        "timezone": "Etc/UTC",
        "updateWorkerPoolsInParallel": true,
        "controlPlaneHealthCheckTimeout": "10m",
        "clusterMetaAttribute": "team=payments",
        "clusterResources": {"rbacs": [{"uid": "r1", "name": "editors"}],
            "namespaces": [{"uid": "n1", "name": "payments"}]},
    });
    cluster["status"]["spcApply"] = serde_json::json!({
        "actionType": "DownloadAndInstallLater", "spcHash": "a1", "spcInfraHash": "b1"});
    serve_cluster(MemoryApi::new(), dir, "cluster-b")
        .with(cluster_route(CLUSTER_UID), cluster)
        .with(
            rbacs_route(),
            serde_json::json!({"items": [{
                "metadata": {"uid": "r1", "name": "editors", "creationTimestamp": "t0"},
                "spec": {"isSystem": false, "bindings": [{"type": "RoleBinding",
                    "namespace": "payments", "role": {"kind": "Role", "name": "editor"},
                    "subjects": subjects}]}}]}),
        )
        .with(
            namespaces_route(),
            serde_json::json!({"items": [{
                "metadata": {"uid": "n1", "name": "payments", "creationTimestamp": "t0"},
                "spec": {"isRegex": false,
                    "resourceAllocation": {"cpuCores": 2, "memoryMiB": 4096}}}]}),
        )
}

fn barney() -> Value {
    serde_json::json!([{"type": "User", "name": "barney"}])
}

fn policy_of(s: &Snapshot) -> Value {
    s.source
        .cluster
        .as_ref()
        .unwrap()
        .tier(Tier::Policy)
        .unwrap()
        .fields
        .clone()
}

/// The comparison of the policy tiers in `d`.
fn policy_diff(d: &strata::diff::Diff) -> &strata::diff::TierDiff {
    d.cluster_tiers
        .iter()
        .find(|t| t.tier == Tier::Policy)
        .unwrap()
}

#[test]
fn a_cluster_record_holds_the_bindings_the_namespaces_and_the_five_settings_in_the_policy_tier() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let s = snapshot_cluster(&serve_policy(dir.path(), barney()), None).unwrap();
    let p = policy_of(&s);
    assert_eq!(
        p["rbacs"],
        serde_json::json!([{"namespace": "payments", "role": {"kind": "Role", "name": "editor"},
            "subjects": [{"name": "barney", "type": "User"}], "type": "RoleBinding"}]),
        "each binding without its uid and its time"
    );
    assert_eq!(
        p["namespaces"],
        serde_json::json!([{"name": "payments", "isRegex": false,
            "resourceAllocation": {"cpuCores": 2, "memoryMiB": 4096}}])
    );
    assert_eq!(p["updateWorkerPoolsInParallel"], true);
    assert_eq!(p["controlPlaneHealthCheckTimeout"], "10m");
    assert_eq!(p["clusterMetaAttribute"], "team=payments");
    assert_eq!(p["applySetting"], "DownloadAndInstallLater");
    assert_eq!(
        p["spectroComponentsUpgradeForbidden"], "true",
        "the annotation as Palette gives it"
    );
    assert!(p.get("agentUpgrades").is_none());
    assert!(
        p.get("resources").is_none() && p.get("rbac").is_none(),
        "the bindings and the namespaces take the place of their references"
    );
}

#[test]
fn an_edit_of_the_subjects_of_a_binding_changes_the_policy_tier_and_verify_and_compare_name_it() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = snapshot_cluster(&serve_policy(dir.path(), barney()), None).unwrap();
    // the binding keeps its uid in Palette, and gets one more subject
    let edited = serve_policy(
        dir.path(),
        serde_json::json!([{"type": "User", "name": "barney"}, {"type": "User", "name": "fred"}]),
    );

    let v = ops::verify(
        &recorded,
        &Palette::new(edited.clone()),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(v.unchanged, "the verdict follows the release");
    let tier = policy_diff(&v.diff);
    assert!(!tier.same);
    let paths: Vec<&str> = tier.diff.iter().map(|l| l.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "rbacs[0].subjects[name=fred].name",
            "rbacs[0].subjects[name=fred].type"
        ]
    );

    let live = snapshot_cluster(&edited, None).unwrap();
    let d = ops::compare(&recorded, &live, "t1");
    assert!(!policy_diff(&d).same, "compare names the policy tier");
    assert!(
        d.nothing_differs(),
        "a tier does not change the exit of compare"
    );
}

#[test]
fn a_policy_tier_from_before_the_bindings_were_read_still_verifies() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_policy(dir.path(), barney());
    let mut older = snapshot_cluster(&api, None).unwrap();
    // the policy tier as an older version of strata wrote it: the references, with their uids
    let fields = serde_json::json!({
        "osPatch": null, "lifecycle": null, "machineHealth": null, "timezone": "Etc/UTC",
        "resources": {"rbacs": [{"uid": "r1", "name": "editors"}], "namespaces": []},
        "rbac": null, "fips": "",
    });
    let tier = serde_json::json!({"hash": "0", "fields": fields});
    older
        .source
        .cluster
        .as_mut()
        .unwrap()
        .tiers
        .insert(Tier::Policy.into(), serde_json::from_value(tier).unwrap());
    let saved = dir.path().join("older.json");
    common::write_snapshot(&saved, &older, true);
    let older = Snapshot::load(&saved).unwrap();

    let v = ops::verify(&older, &Palette::new(api), "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged);
    let tier = policy_diff(&v.diff);
    assert!(
        tier.same,
        "only the fields that the two forms share: {:?}",
        tier.diff
    );
}

#[test]
fn a_policy_tier_of_strata_0_18_with_agent_upgrades_still_verifies() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_policy(dir.path(), barney());
    let mut older = serde_json::to_value(snapshot_cluster(&api, None).unwrap()).unwrap();
    // the policy tier as strata 0.18 wrote it: a state in place of the annotation
    let fields = &mut older["source"]["cluster"]["tiers"]["policy"]["fields"];
    let annotation = fields
        .as_object_mut()
        .unwrap()
        .remove("spectroComponentsUpgradeForbidden");
    assert_eq!(annotation, Some(Value::from("true")));
    fields["agentUpgrades"] = Value::from("paused");
    let older: Snapshot = serde_json::from_value(older).unwrap();

    let v = ops::verify(
        &older,
        &Palette::new(api.clone()),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(v.unchanged);
    let tier = policy_diff(&v.diff);
    assert!(
        tier.same,
        "each field less the agent upgrades: {:?}",
        tier.diff
    );

    // a change of another field still shows
    let edited = serve_policy(
        dir.path(),
        serde_json::json!([{"type": "User", "name": "barney"}, {"type": "User", "name": "fred"}]),
    );
    let live = snapshot_cluster(&edited, None).unwrap();
    assert!(!policy_diff(&ops::compare(&older, &live, "t1")).same);
}

#[test]
fn a_missing_route_of_the_bindings_records_null_and_another_error_gives_no_field() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    // the route of the role bindings answers 404, and the cluster has no namespace
    let mut cluster = read_json(&dir.path().join("cluster-b").join("cluster.json"));
    cluster["spec"]["clusterConfig"] = serde_json::json!({"timezone": "Etc/UTC"});
    let no_route = serve_cluster(MemoryApi::new(), dir.path(), "cluster-b")
        .with(cluster_route(CLUSTER_UID), cluster)
        .with(namespaces_route(), serde_json::json!({"items": []}));
    let p = policy_of(&snapshot_cluster(&no_route, None).unwrap());
    assert_eq!(p["rbacs"], Value::Null, "not read, and not an empty list");
    assert_eq!(p["namespaces"], serde_json::json!([]));

    // a 403 of the route of the role bindings does not stop the record
    let forbidden = serve_policy(dir.path(), barney()).failing(rbacs_route(), 403, "forbidden\n");
    let s = snapshot_cluster(&forbidden, None).unwrap();
    let tier = &s
        .source
        .cluster
        .as_ref()
        .unwrap()
        .tier(Tier::Policy)
        .unwrap();
    assert!(
        tier.fields.get("rbacs").is_none(),
        "an error is not a value"
    );
    assert_eq!(
        tier.fields["timezone"], "Etc/UTC",
        "the other fields are read"
    );
    assert_eq!(
        tier.errors,
        std::collections::BTreeMap::from([(
            "rbacs".to_string(),
            "HTTP 403: forbidden".to_string()
        )])
    );
    let json = serde_json::to_value(&s).unwrap();
    assert_eq!(
        json["source"]["cluster"]["tiers"]["policy"]["errors"]["rbacs"],
        "HTTP 403: forbidden"
    );
    // a route that Palette does not have (HTTP 404 or 405) records null and no error. The hash
    // of a 403 does not cover that null.
    for status in [404, 405] {
        let missing = serve_policy(dir.path(), barney()).failing(rbacs_route(), status, "no route");
        let m = snapshot_cluster(&missing, None).unwrap();
        let missing_tier = &m
            .source
            .cluster
            .as_ref()
            .unwrap()
            .tier(Tier::Policy)
            .unwrap();
        assert_eq!(missing_tier.fields["rbacs"], Value::Null, "HTTP {status}");
        assert_ne!(missing_tier.hash, tier.hash, "HTTP {status}");
        assert!(missing_tier.errors.is_empty(), "HTTP {status}");
        assert!(
            serde_json::to_value(&m).unwrap()["source"]["cluster"]["tiers"]["policy"]
                .get("errors")
                .is_none(),
            "HTTP {status}"
        );
    }

    // the same for the route of the namespaces
    let forbidden =
        serve_policy(dir.path(), barney()).failing(namespaces_route(), 403, "forbidden");
    let s = snapshot_cluster(&forbidden, None).unwrap();
    let tier = &s
        .source
        .cluster
        .as_ref()
        .unwrap()
        .tier(Tier::Policy)
        .unwrap();
    assert!(tier.fields.get("namespaces").is_none());
    assert_ne!(tier.fields["rbacs"], Value::Null);
    assert_eq!(
        tier.errors,
        std::collections::BTreeMap::from([(
            "namespaces".to_string(),
            "HTTP 403: forbidden".to_string()
        )])
    );
}

#[test]
fn verify_and_compare_do_not_compare_a_policy_field_that_strata_could_not_read_and_name_it() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = snapshot_cluster(&serve_policy(dir.path(), barney()), None).unwrap();
    let name = recorded.source.cluster.as_ref().unwrap().name.clone();
    // the bindings changed, and Palette now refuses their route: strata cannot see the change
    let forbidden = serve_policy(
        dir.path(),
        serde_json::json!([{"type": "User", "name": "fred"}]),
    )
    .failing(rbacs_route(), 403, "forbidden");
    let note = format!(
        "strata could not read policy.rbacs of cluster {name} (HTTP 403: forbidden), so it does \
         not compare it"
    );

    let v = ops::verify(
        &recorded,
        &Palette::new(forbidden.clone()),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(v.status, VerifyStatus::Unchanged);
    assert!(policy_diff(&v.diff).same, "{:?}", policy_diff(&v.diff).diff);
    let text = common::terminal()
        .render(&v.diff, &recorded, &v.live, false)
        .unwrap();
    assert!(text.contains(&format!("note       {note}\n")), "{text}");
    assert_eq!(text.matches(&note).count(), 1, "{text}");

    // compare of two records that both could not read the field gives one note
    let live = snapshot_cluster(&forbidden, None).unwrap();
    let d = ops::compare(&live, &live, "t1");
    assert!(policy_diff(&d).same);
    let md = common::markdown().render(&d, &live, &live, false).unwrap();
    assert_eq!(
        md.matches(&format!("**Note:** {note}\n")).count(),
        1,
        "{md}"
    );

    // a field that both records read still compares
    let fred = snapshot_cluster(
        &serve_policy(
            dir.path(),
            serde_json::json!([{"type": "User", "name": "fred"}]),
        )
        .failing(namespaces_route(), 403, "forbidden"),
        None,
    )
    .unwrap();
    let d = ops::compare(&recorded, &fred, "t1");
    let paths: Vec<&str> = policy_diff(&d)
        .diff
        .iter()
        .map(|l| l.path.as_str())
        .collect();
    assert!(
        paths.iter().all(|p| p.starts_with("rbacs[")) && !paths.is_empty(),
        "{paths:?}"
    );
}

#[test]
fn a_raw_directory_keeps_the_bindings_and_the_namespaces_so_the_record_is_the_same() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let silent = Progress::silent();
    let palette = Palette::new(serve_policy(dir.path(), barney()));
    let saved = dir.path().join("raw-policy");
    palette
        .fetch_subject(SubjectKind::Cluster, CLUSTER_UID, &silent)
        .unwrap()
        .save_dir(&saved, "t0")
        .unwrap();
    assert!(saved.join("rbacs.json").is_file() && saved.join("namespaces.json").is_file());
    let reference = format!("cluster:{CLUSTER_UID}");
    let rebuilt = ops::snapshot_from_dir(&saved, &reference, "t0").unwrap();
    let fetched = snapshot_cluster(&serve_policy(dir.path(), barney()), None).unwrap();
    assert_eq!(policy_of(&rebuilt), policy_of(&fetched));

    // a directory that an older build saved has neither file
    fs::remove_file(saved.join("rbacs.json")).unwrap();
    let older = ops::snapshot_from_dir(&saved, &reference, "t0").unwrap();
    assert_eq!(policy_of(&older)["rbacs"], Value::Null);
}

// ---- consumers -------------------------------------------------------------------------------

use strata::render::{self, Format};
use strata::term::color::Paint;

/// Profile b's API, with `inUseClusters` naming one of its two consumers the way Palette does.
fn serve_b_with_named_consumer(dir: &Path) -> MemoryApi {
    let mut profile = read_json(&dir.join("b").join("profile.json"));
    profile["status"]["inUseClusters"] = serde_json::json!([
        {"uid": "cluster-aaa", "name": "alpha", "projectUid": "p1", "kind": "spectrocluster"}
    ]);
    serve_fixture(MemoryApi::new(), dir, "b", UID_B).with(profile_route(UID_B), profile)
}

fn cluster_doc(uid: &str, name: &str, project: &str, state: &str) -> Value {
    serde_json::json!({
        "metadata": {"uid": uid, "name": name, "annotations": {"projectUid": project}},
        "status": {"state": state},
    })
}

/// A uid names no project to a reader, so each row carries the project's name as well. The name
/// is a convenience: a project listing that cannot be read leaves the uid alone.
#[test]
fn a_consumer_row_names_its_project_and_falls_back_to_the_uid() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_b_with_named_consumer(dir.path()).with(
        cluster_route("cluster-aaa"),
        cluster_doc("cluster-aaa", "alpha", "p1", "Running"),
    );

    // with no project listing, the row prints the uid it has
    let r = ops::consumers(
        &Ref::Uid(UID_B.into()),
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(r.consumers[0].project_uid, "p1");
    assert_eq!(r.consumers[0].project_name, "");
    let text = common::terminal().render_consumers(&r).unwrap();
    assert!(text.contains("project p1"), "{text}");

    // with one, it names the project and keeps the uid
    let named = api.with(
        PROJECT_LISTING,
        serde_json::json!({"items": [{"metadata": {"name": "dev", "uid": "p1"}}]}),
    );
    let r = ops::consumers(
        &Ref::Uid(UID_B.into()),
        |_| Ok(Palette::new(named.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(r.consumers[0].project_name, "dev");
    let text = common::terminal().render_consumers(&r).unwrap();
    assert!(text.contains("project dev (uid p1)"), "{text}");
}

#[test]
fn a_consumer_with_no_project_is_read_in_the_scope_the_profile_was_read_in() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());

    // the profile of project p1, with one consumer that Palette names no project for
    let mut profile = read_json(&dir.path().join("b").join("profile.json"));
    profile["status"]["inUseClusterUids"] = serde_json::json!(["cluster-aaa"]);
    profile["status"]["inUseClusters"] =
        serde_json::json!([{"uid": "cluster-aaa", "name": "alpha", "projectUid": ""}]);
    let in_p1 = serve_fixture(MemoryApi::new(), dir.path(), "b", UID_B)
        .with(profile_route(UID_B), profile)
        .with(
            cluster_route("cluster-aaa"),
            cluster_doc("cluster-aaa", "alpha", "", "Running"),
        );

    // a record of that profile, taken in p1
    let record = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0")
        .unwrap()
        .with_project(Some("p1"));
    let path = dir.path().join("record.json");
    common::write_snapshot(&path, &record, false);

    let scopes: RefCell<Vec<Option<String>>> = RefCell::new(Vec::new());
    let connect = |scope: Option<&str>| {
        scopes.borrow_mut().push(scope.map(str::to_string));
        Ok(Palette::new(in_p1.clone()))
    };
    let r = ops::consumers(&Ref::Snapshot(path), connect, "t1", &Progress::silent()).unwrap();

    assert_eq!(r.count, 1);
    assert_eq!(r.consumers[0].state, "Running", "the cluster was read");
    assert!(r.consumers[0].error.is_none());
    assert_eq!(
        *scopes.borrow(),
        vec![Some("p1".to_string()), Some(String::new())],
        "the record names p1, so its consumers are read there, and never in the scope of the \
         environment; then the second reading lists the clusters at tenant scope"
    );
    assert!(
        r.clusters_read.error.is_some(),
        "this tenant serves no cluster listing, so the second reading says it was not done"
    );
}

#[test]
fn consumers_lists_each_cluster_with_name_project_and_state_from_its_own_project() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let tenant = serve_b_with_named_consumer(dir.path());
    let in_p1 = MemoryApi::new().with(
        cluster_route("cluster-aaa"),
        cluster_doc("cluster-aaa", "alpha", "p1", "Running"),
    );
    let scopes: RefCell<Vec<Option<String>>> = RefCell::new(Vec::new());
    let connect = |scope: Option<&str>| {
        scopes.borrow_mut().push(scope.map(str::to_string));
        Ok(Palette::new(match scope {
            Some("p1") => in_p1.clone(),
            _ => tenant.clone(),
        }))
    };
    let r = ops::consumers(&Ref::Uid(UID_B.into()), connect, "t1", &Progress::silent()).unwrap();
    assert_eq!(r.schema, ops::CONSUMERS_SCHEMA);
    assert_eq!(
        (
            r.profile.uid.as_str(),
            r.profile.name.as_str(),
            r.profile.version.as_str()
        ),
        (UID_B, "lab-profile", "1.3.0")
    );
    assert_eq!(r.count, 2);
    assert_eq!(
        r.exit_code(),
        1,
        "consumers exist: a delete step must not proceed"
    );
    let alpha = &r.consumers[0];
    assert_eq!(
        (
            alpha.uid.as_str(),
            alpha.name.as_str(),
            alpha.project_uid.as_str(),
            alpha.state.as_str()
        ),
        ("cluster-aaa", "alpha", "p1", "Running")
    );
    assert!(alpha.error.is_none());
    // cluster-bbb is only a uid on the profile, and it cannot be read in the tenant scope
    let unknown = &r.consumers[1];
    assert_eq!(
        (unknown.uid.as_str(), unknown.state.as_str()),
        ("cluster-bbb", "unknown")
    );
    assert_eq!(unknown.error.as_ref().unwrap()["kind"], "api");
    assert_eq!(
        *scopes.borrow(),
        vec![None, Some("p1".to_string()), Some(String::new())],
        "one client per project; the profile and unscoped clusters share one; the second reading \
         lists at tenant scope"
    );

    let text = common::terminal().render_consumers(&r).unwrap();
    assert!(
        text.starts_with(&format!("lab-profile@1.3.0 (uid {UID_B}): 2 consumers\n")),
        "{text}"
    );
    assert!(text.contains("alpha"), "{text}");
    assert!(text.contains("p1") && text.contains("Running"), "{text}");
    assert!(
        text.contains("cluster-bbb") && text.contains("unknown"),
        "{text}"
    );
    let md = common::markdown().render_consumers(&r).unwrap();
    assert!(
        md.contains("| `alpha` | `cluster-aaa` | `p1` | Running |"),
        "{md}"
    );
    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_consumers(&r)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["consumers"][0]["projectUid"], "p1");
    assert_eq!(json["count"], 2);
}

#[test]
fn consumers_shows_the_version_each_cluster_runs_and_marks_one_the_uid_no_longer_names() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let mut profile = read_json(&dir.path().join("b").join("profile.json"));
    profile["status"]["inUseClusters"] = serde_json::json!([
        {"uid": "cluster-aaa", "name": "alpha", "projectUid": "p1"},
        {"uid": "cluster-bbb", "name": "beta", "projectUid": "p1"}
    ]);
    let tenant =
        serve_fixture(MemoryApi::new(), dir.path(), "b", UID_B).with(profile_route(UID_B), profile);
    // alpha runs the version the uid names; beta's copy still names 1.2.9, from before a legacy
    // version bump renamed the uid to 1.3.0
    let runs = |uid: &str, name: &str, version: &str| {
        let mut d = cluster_doc(uid, name, "p1", "Running");
        d["spec"] = serde_json::json!({"clusterProfileTemplates": [
            {"uid": UID_ADDONS, "name": "lab-addons", "profileVersion": "2.0.0"},
            {"uid": UID_B, "name": "lab-profile", "profileVersion": version}
        ]});
        d
    };
    let in_p1 = MemoryApi::new()
        .with(
            cluster_route("cluster-aaa"),
            runs("cluster-aaa", "alpha", "1.3.0"),
        )
        .with(
            cluster_route("cluster-bbb"),
            runs("cluster-bbb", "beta", "1.2.9"),
        );
    let connect = |scope: Option<&str>| {
        Ok(Palette::new(match scope {
            Some("p1") => in_p1.clone(),
            _ => tenant.clone(),
        }))
    };
    let r = ops::consumers(&Ref::Uid(UID_B.into()), connect, "t1", &Progress::silent()).unwrap();
    let versions: Vec<(&str, &str)> = r
        .consumers
        .iter()
        .map(|c| (c.name.as_str(), c.version.as_str()))
        .collect();
    assert_eq!(versions, [("alpha", "1.3.0"), ("beta", "1.2.9")]);
    assert_eq!(r.overwritten, 1);

    let text = common::terminal().render_consumers(&r).unwrap();
    let beta = text.lines().find(|l| l.contains("beta")).unwrap();
    assert!(
        beta.contains(&format!(
            "runs lab-profile@1.2.9, which uid {UID_B} no longer names"
        )),
        "{text}"
    );
    let alpha = text.lines().find(|l| l.contains("alpha")).unwrap();
    assert!(!alpha.contains("no longer names"), "{text}");
    let md = common::markdown().render_consumers(&r).unwrap();
    // the markdown names the version and the uid as the terminal does
    assert!(
        md.contains(&format!(
            "runs `lab-profile@1.2.9`, which uid `{UID_B}` no longer names"
        )),
        "{md}"
    );
    // a row with an error states the version that the copy names, too, in each report
    let mut doc = serde_json::to_value(&r).unwrap();
    doc["consumers"][1]["error"] = serde_json::json!({"kind": "transport", "message": "boom"});
    let failed: ops::ConsumersReport = serde_json::from_value(doc).unwrap();
    let text = common::terminal().render_consumers(&failed).unwrap();
    let beta = text.lines().find(|l| l.contains("beta")).unwrap();
    assert!(
        beta.contains("runs lab-profile@1.2.9, which uid") && beta.contains("(boom)"),
        "{text}"
    );
    let md = common::markdown().render_consumers(&failed).unwrap();
    let beta = md.lines().find(|l| l.contains("beta")).unwrap();
    assert!(
        beta.contains("runs `lab-profile@1.2.9`, which uid") && beta.contains("(not read: boom)"),
        "{md}"
    );
    // a line break in the message of an error does not end the row of the table
    let mut doc = serde_json::to_value(&r).unwrap();
    doc["consumers"][1]["error"] =
        serde_json::json!({"kind": "transport", "message": "first\nsecond | third"});
    let broken: ops::ConsumersReport = serde_json::from_value(doc).unwrap();
    let md = common::markdown().render_consumers(&broken).unwrap();
    let beta = md.lines().find(|l| l.contains("beta")).unwrap();
    assert!(
        beta.contains("(not read: first second \\| third)") && beta.ends_with('|'),
        "{md}"
    );
    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_consumers(&r)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["consumers"][1]["version"], "1.2.9");
    assert_eq!(json["overwritten"], 1);
}

#[test]
fn a_profile_without_consumers_exits_zero_and_fetches_no_cluster() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A)
        .with(
            PROFILE_LISTING,
            serde_json::json!({"items": [
                {"metadata": {"name": "lab-profile", "uid": UID_A}, "spec": {"version": "1.2.0"}}
            ]}),
        )
        // the second reading lists the clusters, and this tenant has none
        .with(CLUSTER_LISTING, serde_json::json!([]));
    let r = ops::consumers(
        &Ref::NameVersion {
            name: "lab-profile".into(),
            version: "1.2.0".into(),
        },
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!((r.count, r.exit_code()), (0, 0));
    assert!(r.consumers.is_empty());
    assert_eq!((r.clusters_read.read, r.clusters_read.unread), (0, 0));
    assert!(r.clusters_read.error.is_none());
    let text = common::terminal().render_consumers(&r).unwrap();
    assert_eq!(
        text,
        format!(
            "lab-profile@1.2.0 (uid {UID_A}): no consumers\n  second reading: 0 of 0 clusters read\n  edge hosts: 0 read\n"
        )
    );
}

/// Palette gives an empty consumer list on an error too. So an empty list with a second reading
/// that failed is not "nothing uses it". A consumer can be missing, and a delete step must not
/// run on exit 0 (#194).
#[test]
fn consumers_exits_2_when_the_list_is_empty_and_the_second_reading_fails() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    // the search of Palette is not served, so the second reading reads the listing, which fails
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A).failing(
        CLUSTER_LISTING,
        500,
        "boom",
    );
    let r = ops::consumers(
        &Ref::Uid(UID_A.into()),
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(r.consumers.is_empty());
    assert!(r.clusters_read.error.is_some());
    assert!(
        r.edge_hosts_read.error.is_none(),
        "no edge host search: no edge host"
    );
    assert!(r.incomplete());
    assert_eq!(
        (r.count, r.exit_code()),
        (0, 2),
        "a reading failed: the answer is not 'nothing uses it'"
    );
    let text = common::terminal().render_consumers(&r).unwrap();
    assert!(
        text.starts_with(
            &format!("lab-profile@1.2.0 (uid {UID_A}): no consumer found, and a reading failed\n  second reading: not done (GET /v1/dashboard/spectroclusters/meta failed with HTTP 500: boom")
        ),
        "{text}"
    );
    let md = common::markdown().render_consumers(&r).unwrap();
    assert!(
        md.starts_with(
            &format!("### consumers: lab-profile@1.2.0 (uid {UID_A}): no consumer found, and a reading failed\n")
        ),
        "{md}"
    );
}

/// A cluster that the listing names and strata cannot read can be a consumer. So an empty list
/// with such a cluster is not "nothing uses it" either (#194).
#[test]
fn consumers_exits_2_when_the_list_is_empty_and_a_cluster_cannot_be_read() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    // the listing names gamma, and its document is not served
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A).with(
        CLUSTER_LISTING,
        serde_json::json!([{
            "metadata": {"name": "gamma", "uid": "cluster-ccc", "annotations": {"projectUid": "p1"}},
            "status": {"state": "Running"}
        }]),
    );
    let r = ops::consumers(
        &Ref::Uid(UID_A.into()),
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(r.consumers.is_empty());
    assert_eq!((r.clusters_read.read, r.clusters_read.unread), (0, 1));
    assert!(r.clusters_read.error.is_none(), "the listing was read");
    assert_eq!((r.count, r.exit_code()), (0, 2), "gamma can be a consumer");
    let text = common::terminal().render_consumers(&r).unwrap();
    assert_eq!(
        text,
        format!(
            "lab-profile@1.2.0 (uid {UID_A}): no consumer found, and a reading failed\n  second reading: 0 of 1 clusters read\n  edge hosts: 0 read\n"
        )
    );
}

/// A Palette with no edge host search answers 404 or 405. Each is no edge host, and not a reading
/// that failed.
#[test]
fn consumers_reads_no_edge_host_when_palette_has_no_edge_host_search() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A)
        .with(CLUSTER_LISTING, serde_json::json!([]))
        .failing(post(EDGE_HOST_SEARCH), 405, "{}");
    let r = ops::consumers(
        &Ref::Uid(UID_A.into()),
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(r.consumers.is_empty());
    assert!(!r.edge_hosts_read.incomplete(), "{:?}", r.edge_hosts_read);
    assert_eq!((r.count, r.exit_code()), (0, 0));
}

/// An edge host that the version is bound to is a consumer, so a failed search of the edge hosts
/// leaves the answer incomplete too (#194).
#[test]
fn consumers_exits_2_when_the_list_is_empty_and_the_edge_hosts_cannot_be_read() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A)
        .with(CLUSTER_LISTING, serde_json::json!([]))
        .failing(post(EDGE_HOST_SEARCH), 500, "boom");
    let r = ops::consumers(
        &Ref::Uid(UID_A.into()),
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(r.consumers.is_empty());
    assert!(
        !r.clusters_read.incomplete(),
        "the second reading read no cluster, and failed not"
    );
    assert!(r.edge_hosts_read.incomplete());
    assert_eq!((r.count, r.exit_code()), (0, 2));
    let text = common::terminal().render_consumers(&r).unwrap();
    assert!(
        text.contains("  edge hosts: not read (") && text.contains("failed with HTTP 500: boom"),
        "{text}"
    );
}

/// A consumer that was found decides the exit, whatever else failed: the delete must not run,
/// and exit 1 says why (#194).
#[test]
fn consumers_exits_1_when_a_consumer_is_found_and_the_second_reading_fails() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_b_with_named_consumer(dir.path())
        .with(
            cluster_route("cluster-aaa"),
            cluster_doc("cluster-aaa", "alpha", "p1", "Running"),
        )
        .failing(CLUSTER_LISTING, 500, "boom");
    let r = ops::consumers(
        &Ref::Uid(UID_B.into()),
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(r.incomplete());
    // the profile names two clusters: alpha, and one that strata cannot read
    assert_eq!((r.count, r.exit_code()), (2, 1), "alpha uses the version");
    let text = common::terminal().render_consumers(&r).unwrap();
    assert!(
        text.starts_with(&format!("lab-profile@1.3.0 (uid {UID_B}): 2 consumers\n")),
        "{text}"
    );
}

#[test]
fn the_second_reading_finds_a_consumer_the_profile_does_not_list() {
    // Palette computes the consumer list of a profile when it is asked, in the projects the key
    // can read, and gives an empty list on an error too. So a cluster that applies the version
    // can be missing from it. The second reading reads each cluster the key can list, and finds
    // it.
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let mut applies = cluster_doc("cluster-ccc", "gamma", "p1", "Running");
    applies["spec"] = serde_json::json!({"clusterProfileTemplates": [
        {"uid": UID_A, "name": "lab-profile", "profileVersion": "1.2.0", "type": "cluster"}
    ]});
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A)
        .with(
            CLUSTER_LISTING,
            serde_json::json!([{
                "metadata": {"name": "gamma", "uid": "cluster-ccc", "annotations": {"projectUid": "p1"}},
                "status": {"state": "Running"}
            }]),
        )
        .with(cluster_route("cluster-ccc"), applies);
    let r = ops::consumers(
        &Ref::Uid(UID_A.into()),
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(
        (r.count, r.exit_code()),
        (1, 1),
        "the second reading found a consumer, so a delete step must not proceed"
    );
    let found = &r.consumers[0];
    assert!(found.unlisted, "the profile did not list it");
    assert_eq!(
        (
            found.uid.as_str(),
            found.name.as_str(),
            found.project_uid.as_str(),
            found.state.as_str(),
            found.version.as_str()
        ),
        ("cluster-ccc", "gamma", "p1", "Running", "1.2.0")
    );
    assert_eq!((r.clusters_read.read, r.clusters_read.unread), (1, 0));

    let text = common::terminal().render_consumers(&r).unwrap();
    assert!(
        text.contains("not in the consumer list of the profile"),
        "{text}"
    );
    assert!(
        text.ends_with("  second reading: 1 of 1 clusters read\n  edge hosts: 0 read\n"),
        "{text}"
    );
    let md = common::markdown().render_consumers(&r).unwrap();
    assert!(
        md.contains("(not in the consumer list of the profile)")
            && md.contains("Second reading: 1 of 1 clusters read."),
        "{md}"
    );
    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_consumers(&r)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["consumers"][0]["unlisted"], true);
    assert_eq!(json["clustersRead"]["read"], 1);
}

/// A condition of the cluster search, as strata sends it: the string `property` equals `value`.
fn search_eq(property: &str, value: &str) -> Value {
    serde_json::json!({
        "property": property, "type": "string",
        "condition": {"string": {"operator": "eq", "negation": false, "ignoreCase": false,
            "match": {"conjunction": "or", "values": [value]}}},
    })
}

/// The body of the cluster search with the filter groups `groups`, as strata sends it.
fn search_of(groups: Vec<Value>) -> Value {
    serde_json::json!({
        "filter": {"conjunction": "and", "filterGroups": groups},
        "sort": [{"field": "clusterName", "order": "asc"}],
    })
}

/// The body of the search of the virtual clusters that `groups` select. It has the condition
/// `environment eq nested` as one more group, since the search omits each virtual cluster
/// unless a condition asks for it.
fn virtual_search(mut groups: Vec<Value>) -> Value {
    groups.push(serde_json::json!({"conjunction": "and",
        "filters": [search_eq("environment", "nested")]}));
    search_of(groups)
}

/// The schema of a cluster search that takes the string `properties`.
fn search_schema(properties: &[&str]) -> Value {
    let properties: Vec<Value> = properties
        .iter()
        .map(|name| serde_json::json!({"name": name, "type": "string"}))
        .collect();
    serde_json::json!({"schema": {"properties": properties}})
}

/// The body of the cluster search for the clusters that run a version of the profile `name`, and
/// with `nested` for the virtual clusters that run it, as the second reading of `consumers` sends
/// it. Palette takes the name of a profile in `clusterProfiles`, and not its uid (#181).
fn profile_search(name: &str, nested: bool) -> Value {
    let groups = vec![serde_json::json!({"conjunction": "and",
        "filters": [search_eq("clusterProfiles", name)]})];
    if nested {
        virtual_search(groups)
    } else {
        search_of(groups)
    }
}

/// A cluster as the search gives it: a summary, with the project in `specSummary`.
fn search_item(uid: &str, name: &str, project: &str, cloud: &str) -> Value {
    serde_json::json!({
        "metadata": {"uid": uid, "name": name},
        "specSummary": {"cloudConfig": {"cloudType": cloud},
                        "projectMeta": {"uid": project, "name": "lab"}},
        "status": {"state": "Running"},
    })
}

/// A virtual cluster as the search gives it: the cloud type `nested`, and `status.virtual` with
/// its cluster group and its host cluster, each `(uid, name)`.
fn virtual_item(
    uid: &str,
    name: &str,
    project: &str,
    group: (&str, &str),
    host: (&str, &str),
) -> Value {
    let mut item = search_item(uid, name, project, "nested");
    item["status"]["virtual"] = serde_json::json!({
        "clusterGroup": {"uid": group.0, "name": group.1, "kind": "clustergroup"},
        "hostCluster": {"uid": host.0, "name": host.1, "kind": "spectrocluster"},
    });
    item
}

#[test]
fn the_second_reading_asks_the_cluster_search_and_finds_a_virtual_cluster() {
    // the consumer list is empty; the search names one cluster, and with environment nested one
    // virtual cluster, both with the profile
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let runs = |uid: &str, name: &str| {
        let mut doc = cluster_doc(uid, name, "p1", "Running");
        doc["spec"] = serde_json::json!({"clusterProfileTemplates": [
            {"uid": UID_A, "name": "lab-profile", "profileVersion": "1.2.0", "type": "cluster"}
        ]});
        doc
    };
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A)
        .with(
            CLUSTER_SEARCH_SCHEMA,
            serde_json::json!({"schema": {"properties": [
                {"name": "clusterProfiles", "type": "string"},
                {"name": "environment", "type": "string"}]}}),
        )
        .with_post(
            CLUSTER_SEARCH,
            &profile_search("lab-profile", false),
            serde_json::json!({"items": [search_item("cluster-ccc", "gamma", "p1", "aws")],
                               "listmeta": {"continue": ""}}),
        )
        .with_post(
            CLUSTER_SEARCH,
            &profile_search("lab-profile", true),
            serde_json::json!({"items": [search_item("cluster-vvv", "vc-1", "p1", "nested")],
                               "listmeta": {"continue": ""}}),
        )
        .with(cluster_route("cluster-ccc"), runs("cluster-ccc", "gamma"))
        .with(cluster_route("cluster-vvv"), runs("cluster-vvv", "vc-1"));
    let r = ops::consumers(
        &Ref::Uid(UID_A.into()),
        |_| Ok(Palette::new(&api)),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!((r.count, r.exit_code()), (2, 1));
    let found: Vec<(&str, &str, bool)> = r
        .consumers
        .iter()
        .map(|c| (c.uid.as_str(), c.project_uid.as_str(), c.unlisted))
        .collect();
    assert_eq!(
        found,
        [("cluster-ccc", "p1", true), ("cluster-vvv", "p1", true)]
    );
    assert_eq!(r.clusters_read.resolved, Some(ops::Resolved::Search));
    assert_eq!((r.clusters_read.read, r.clusters_read.unread), (2, 0));
    // two searches and two cluster documents, and no listing
    let searches = api
        .posted()
        .into_iter()
        .filter(|(path, _)| path == CLUSTER_SEARCH)
        .count();
    assert_eq!(searches, 2);
    assert_eq!(api.calls(&cluster_route("cluster-ccc")), 1);
    assert_eq!(api.calls(&cluster_route("cluster-vvv")), 1);
    assert_eq!(api.calls(CLUSTER_LISTING), 0);

    let text = common::terminal().render_consumers(&r).unwrap();
    assert_eq!(
        text.matches("not in the consumer list of the profile")
            .count(),
        2,
        "{text}"
    );
    assert!(
        text.contains(
            "  second reading: the cluster search names 2 other clusters with the profile, 2 read\n"
        ),
        "{text}"
    );
    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_consumers(&r)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["clustersRead"]["resolved"], "search");
    // each search sends the name of the profile, and never its uid: Palette changes a name into
    // the uids of the profiles with that name, and a uid matches no name (#181)
    let bodies: Vec<String> = api
        .posted()
        .into_iter()
        .filter(|(path, _)| path == CLUSTER_SEARCH)
        .map(|(_, body)| body.to_string())
        .collect();
    assert_eq!(bodies.len(), 2);
    for body in &bodies {
        assert!(body.contains("\"lab-profile\""), "{body}");
        assert!(!body.contains(UID_A), "{body}");
    }
}

/// The listing of the cluster groups of a project.
const GROUP_LISTING: &str = "/v1/clustergroups/hostCluster/metadata";

/// The search of the virtual clusters of the group `g1`.
const GROUP_MEMBERS: &str = "/v1/dashboard/clustergroups/g1/virtualClusters?limit=50";

/// The key under which the memory API keeps the POST of the virtual clusters of the group `g1`:
/// the path and the body.
static MEMBERS_POST: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    format!(
        "{} {}",
        post(GROUP_MEMBERS),
        serde_json::json!({"filter": {}, "sort": []})
    )
});

/// A Palette with one cluster group `g1` that pins the profile `UID_A`, and one virtual cluster
/// `vc-1` in it that runs the profile through the group. The search answers no cluster: it does
/// not name a virtual cluster that runs the version through its group (#184).
fn with_pinning_group(api: MemoryApi) -> MemoryApi {
    let mut vc = cluster_doc("vc-1", "vc-1", "p1", "Running");
    vc["spec"] = serde_json::json!({"cloudType": "nested", "clusterProfileTemplates": [
        {"uid": UID_A, "name": "lab-profile", "profileVersion": "1.2.0", "type": "add-on"}
    ]});
    api.with(
        CLUSTER_SEARCH_SCHEMA,
        serde_json::json!({"schema": {"properties": [
            {"name": "clusterProfiles", "type": "string"},
            {"name": "environment", "type": "string"}]}}),
    )
    .with_post(
        CLUSTER_SEARCH,
        &profile_search("lab-profile", false),
        serde_json::json!({"items": [], "listmeta": {"continue": ""}}),
    )
    .with_post(
        CLUSTER_SEARCH,
        &profile_search("lab-profile", true),
        serde_json::json!({"items": [], "listmeta": {"continue": ""}}),
    )
    .with(
        GROUP_LISTING,
        serde_json::json!({"items": [{"uid": "g1", "name": "dev-group"}]}),
    )
    .with(
        group_route("g1"),
        serde_json::json!({"metadata": {"uid": "g1", "name": "dev-group"},
            "spec": {"clusterProfileTemplates": [{"uid": UID_A, "name": "lab-profile"}]}}),
    )
    .with_post(
        GROUP_MEMBERS,
        &serde_json::json!({"filter": {}, "sort": []}),
        serde_json::json!({"items": [{"metadata": {"uid": "vc-1", "name": "vc-1",
            "annotations": {"projectUid": "p1"}}}], "listmeta": {"continue": ""}}),
    )
    .with(cluster_route("vc-1"), vc)
}

#[test]
fn the_second_reading_asks_for_no_virtual_cluster_when_the_search_takes_no_environment() {
    // an older Palette: its search takes clusterProfiles, and not environment. Strata sends the
    // one search, and not the second one with environment eq nested, as list clusters and
    // cluster:<name> do (#321). Here the second one gets a 404, and the reading fails
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let mut runs = cluster_doc("cluster-ccc", "gamma", "p1", "Running");
    runs["spec"] = serde_json::json!({"clusterProfileTemplates": [
        {"uid": UID_A, "name": "lab-profile", "profileVersion": "1.2.0", "type": "cluster"}
    ]});
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A)
        .with(CLUSTER_SEARCH_SCHEMA, search_schema(&["clusterProfiles"]))
        .with_post(
            CLUSTER_SEARCH,
            &profile_search("lab-profile", false),
            serde_json::json!({"items": [search_item("cluster-ccc", "gamma", "p1", "aws")],
                               "listmeta": {"continue": ""}}),
        )
        .with(cluster_route("cluster-ccc"), runs);
    let r = ops::consumers(
        &Ref::Uid(UID_A.into()),
        |_| Ok(Palette::new(&api)),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(
        r.clusters_read.error.is_none(),
        "{:?}",
        r.clusters_read.error
    );
    assert_eq!((r.count, r.exit_code()), (1, 1));
    assert_eq!(r.consumers[0].uid, "cluster-ccc");
    let searches: Vec<Value> = api
        .posted()
        .into_iter()
        .filter(|(path, _)| path == CLUSTER_SEARCH)
        .map(|(_, body)| body)
        .collect();
    assert_eq!(searches, vec![profile_search("lab-profile", false)]);
}

#[test]
fn the_second_reading_finds_a_virtual_cluster_through_the_group_that_pins_the_version() {
    // the consumer list is empty, and the search names no cluster; the group g1 pins the version,
    // and its virtual cluster vc-1 runs it (#184)
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = with_pinning_group(serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A));
    let r = ops::consumers(
        &Ref::Uid(UID_A.into()),
        |_| Ok(Palette::new(&api)),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!((r.count, r.exit_code()), (1, 1));
    assert_eq!(r.consumers[0].uid, "vc-1");
    assert!(r.consumers[0].unlisted);
    assert_eq!(
        (
            r.clusters_read.groups,
            r.clusters_read.members,
            r.clusters_read.read
        ),
        (1, 1, 1)
    );
    assert_eq!(api.calls(&cluster_route("vc-1")), 1);
    let text = common::terminal().render_consumers(&r).unwrap();
    assert!(
        text.contains("1 cluster group pins the version, 1 other virtual cluster, 1 read"),
        "{text}"
    );
    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_consumers(&r)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["clustersRead"]["groups"], 1);
    assert_eq!(json["clustersRead"]["members"], 1);
}

#[test]
fn the_second_reading_does_not_read_again_a_virtual_cluster_that_the_list_names() {
    // the consumer list names vc-1, so the group reading counts the group and reads no cluster
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let mut profile = read_json(&dir.path().join("a").join("profile.json"));
    profile["status"]["inUseClusterUids"] = serde_json::json!(["vc-1"]);
    profile["status"]["inUseClusters"] =
        serde_json::json!([{"uid": "vc-1", "name": "vc-1", "projectUid": "p1"}]);
    let api = with_pinning_group(
        serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A).with(profile_route(UID_A), profile),
    );
    let r = ops::consumers(
        &Ref::Uid(UID_A.into()),
        |_| Ok(Palette::new(&api)),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(r.count, 1);
    assert!(!r.consumers[0].unlisted);
    assert_eq!((r.clusters_read.groups, r.clusters_read.members), (1, 0));
    // read once, for the consumer list, and not again by the group reading
    assert_eq!(api.calls(&cluster_route("vc-1")), 1);
}

#[test]
fn the_line_of_the_second_reading_counts_a_virtual_cluster_that_strata_cannot_read_apart() {
    // the search names c-9, which strata reads; the group g1 names vc-1, whose document answers
    // 500 (from the verify of 6d21758)
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let mut c9 = cluster_doc("c-9", "c-9", "p1", "Running");
    c9["spec"] = serde_json::json!({"clusterProfileTemplates": []});
    let api = with_pinning_group(serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A))
        .with_post(
            CLUSTER_SEARCH,
            &profile_search("lab-profile", false),
            serde_json::json!({"items": [{"metadata": {"uid": "c-9", "name": "c-9",
                "annotations": {"projectUid": "p1"}}}], "listmeta": {"continue": ""}}),
        )
        .with(cluster_route("c-9"), c9)
        .failing(cluster_route("vc-1"), 500, "boom");
    let r = ops::consumers(
        &Ref::Uid(UID_A.into()),
        |_| Ok(Palette::new(&api)),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(
        (
            r.clusters_read.read,
            r.clusters_read.unread,
            r.clusters_read.members,
            r.clusters_read.members_unread
        ),
        (1, 1, 1, 1)
    );
    let text = common::terminal().render_consumers(&r).unwrap();
    assert!(
        text.contains(
            "the cluster search names 1 other cluster with the profile, 1 read; \
             1 cluster group pins the version, 1 other virtual cluster, 0 read"
        ),
        "{text}"
    );
}

#[test]
fn the_second_reading_reads_a_group_in_the_scope_of_the_key_when_the_tenant_cannot() {
    // the tenant lists g1 but cannot read it (403); the scope of the key reads it and its
    // virtual clusters, and vc-1 is found (#184)
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let tenant = with_pinning_group(serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A))
        .failing(group_route("g1"), 403, "forbidden")
        .failing(MEMBERS_POST.as_str(), 403, "forbidden");
    let home = with_pinning_group(serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A));
    let r = ops::consumers(
        &Ref::Uid(UID_A.into()),
        |scope| match scope {
            Some(ops::TENANT_SCOPE) => Ok(Palette::new(&tenant)),
            _ => Ok(Palette::new(&home)),
        },
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!((r.count, r.exit_code()), (1, 1));
    assert_eq!(r.consumers[0].uid, "vc-1");
    assert_eq!((r.clusters_read.groups, r.clusters_read.members), (1, 1));
    assert!(r.clusters_read.groups_error.is_none());
    assert_eq!(tenant.calls(&group_route("g1")), 1);
    assert_eq!(home.calls(&group_route("g1")), 1);
    // the tenant refuses the virtual clusters of the group too, and the scope of the key lists them
    assert_eq!(tenant.calls(&MEMBERS_POST), 1);
    assert_eq!(home.calls(&MEMBERS_POST), 1);
}

#[test]
fn the_second_reading_searches_a_project_profile_in_its_own_project() {
    // a profile of the project p9: the search goes to p9, where Palette knows the name, and where
    // each cluster that can run the profile is (#181)
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let mut profile = read_json(&dir.path().join("a").join("profile.json"));
    profile["metadata"]["annotations"] =
        serde_json::json!({"scope": "project", "projectUid": "p9"});
    let mut doc = cluster_doc("cluster-ccc", "gamma", "p9", "Running");
    doc["spec"] = serde_json::json!({"clusterProfileTemplates": [
        {"uid": UID_A, "name": "lab-profile", "profileVersion": "1.2.0", "type": "cluster"}
    ]});
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A)
        .with(profile_route(UID_A), profile)
        .with(
            CLUSTER_SEARCH_SCHEMA,
            serde_json::json!({"schema": {"properties": [
                {"name": "clusterProfiles", "type": "string"},
                {"name": "environment", "type": "string"}]}}),
        )
        .with_post(
            CLUSTER_SEARCH,
            &profile_search("lab-profile", false),
            serde_json::json!({"items": [search_item("cluster-ccc", "gamma", "p9", "aws")],
                               "listmeta": {"continue": ""}}),
        )
        .with_post(
            CLUSTER_SEARCH,
            &profile_search("lab-profile", true),
            serde_json::json!({"items": [], "listmeta": {"continue": ""}}),
        )
        .with(cluster_route("cluster-ccc"), doc);
    let scopes = std::cell::RefCell::new(Vec::new());
    let r = ops::consumers(
        &Ref::Uid(UID_A.into()),
        |scope| {
            scopes.borrow_mut().push(scope.map(str::to_string));
            Ok(Palette::new(&api))
        },
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!((r.count, r.exit_code()), (1, 1));
    assert_eq!(r.consumers[0].uid, "cluster-ccc");
    assert!(r.consumers[0].unlisted);
    assert!(
        scopes.borrow().contains(&Some("p9".to_string())),
        "the search goes to the project of the profile: {:?}",
        scopes.borrow()
    );
}

#[test]
fn the_second_reading_reads_the_listing_when_the_search_takes_no_profile() {
    // the search schema names no clusterProfiles: strata reads the listing and each cluster, as
    // before the search
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let mut applies = cluster_doc("cluster-ccc", "gamma", "p1", "Running");
    applies["spec"] = serde_json::json!({"clusterProfileTemplates": [
        {"uid": UID_A, "name": "lab-profile", "profileVersion": "1.2.0", "type": "cluster"}
    ]});
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A)
        .with(
            CLUSTER_SEARCH_SCHEMA,
            serde_json::json!({"schema": {"properties": [{"name": "tags", "type": "string"}]}}),
        )
        .with(
            CLUSTER_LISTING,
            serde_json::json!([
                {"metadata": {"name": "gamma", "uid": "cluster-ccc",
                              "annotations": {"projectUid": "p1"}},
                 "status": {"state": "Running"}},
                {"metadata": {"name": "delta", "uid": "cluster-ddd",
                              "annotations": {"projectUid": "p1"}},
                 "status": {"state": "Running"}},
            ]),
        )
        .with(cluster_route("cluster-ccc"), applies)
        .with(
            cluster_route("cluster-ddd"),
            cluster_doc("cluster-ddd", "delta", "p1", "Running"),
        );
    let r = ops::consumers(
        &Ref::Uid(UID_A.into()),
        |_| Ok(Palette::new(&api)),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!((r.count, r.exit_code()), (1, 1));
    assert_eq!(r.consumers[0].uid, "cluster-ccc");
    assert_eq!(r.clusters_read.resolved, Some(ops::Resolved::Listing));
    assert_eq!((r.clusters_read.read, r.clusters_read.unread), (2, 0));
    assert!(api.posted().iter().all(|(path, _)| path != CLUSTER_SEARCH));
    assert!(api.calls(CLUSTER_LISTING) >= 1);
    assert_eq!(api.calls(&cluster_route("cluster-ddd")), 1);
    let text = common::terminal().render_consumers(&r).unwrap();
    assert!(
        text.contains("  second reading: 2 of 2 clusters read\n"),
        "{text}"
    );
}

#[test]
fn consumers_list_only_reads_the_list_of_palette_and_says_that_it_did_not_walk() {
    // the fleet of the test above: one cluster applies the version, and the profile lists none
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A).with(
        CLUSTER_LISTING,
        serde_json::json!([{
            "metadata": {"name": "gamma", "uid": "cluster-ccc", "annotations": {"projectUid": "p1"}},
            "status": {"state": "Running"}
        }]),
    );
    let r = ops::consumers_with(
        &Ref::Uid(UID_A.into()),
        &ops::ConsumersOptions { list_only: true },
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    // no cluster document is served, so a read of one would be an error: none was read
    assert_eq!(r.count, 0, "the list of Palette alone");
    assert!(r.clusters_read.skipped);
    assert_eq!((r.clusters_read.read, r.clusters_read.unread), (0, 0));
    // a reading that was not done is not a failed reading: the list alone answers
    assert_eq!(r.exit_code(), 0);
    let text = common::terminal().render_consumers(&r).unwrap();
    assert!(
        text.ends_with("  second reading: not done (--list-only)\n  edge hosts: 0 read\n"),
        "{text}"
    );
    let md = common::markdown().render_consumers(&r).unwrap();
    assert!(
        md.contains("Second reading: not done (`--list-only`)."),
        "{md}"
    );
    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_consumers(&r)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["clustersRead"]["skipped"], true);
    // the default still walks: the same fleet without the flag has no skip
    let walked = ops::consumers(
        &Ref::Uid(UID_A.into()),
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(!walked.clusters_read.skipped);
    assert_eq!(walked.clusters_read.unread, 1, "the walk tried the cluster");
    assert_eq!(
        walked.exit_code(),
        2,
        "the cluster it could not read can be a consumer"
    );
}

#[test]
fn consumers_takes_a_profile_and_refuses_a_cluster() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let never = |_: Option<&str>| -> strata::Result<Palette<MemoryApi>> {
        panic!("a cluster ref is refused before connecting")
    };
    let err = ops::consumers(
        &cluster_ref(CLUSTER_UID, None, false),
        never,
        "t1",
        &Progress::silent(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::ProfileRefRequired { .. }), "{err}");
    assert_eq!(err.kind(), "profile-ref-required");

    // a record of a cluster is refused the same way; a profile record uses its uid, fetched live
    let on_b = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-b"),
        None,
    )
    .unwrap();
    let file = dir.path().join("on-b.json");
    common::write_snapshot(&file, &on_b, true);
    let err = ops::consumers(&Ref::Snapshot(file), never, "t1", &Progress::silent()).unwrap_err();
    assert!(matches!(err, Error::ProfileRefRequired { .. }), "{err}");

    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let file = dir.path().join("a.json");
    common::write_snapshot(&file, &recorded, true);
    let api = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A);
    let r = ops::consumers(
        &Ref::Snapshot(file),
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(r.profile.uid, UID_A);
}

// ---- list ------------------------------------------------------------------------------------

use strata::model::scope::ProfileScope;
use strata::ops::{ListKind, ListOptions};

/// A profile listing entry, as `GET /v1/dashboard/clusterprofiles/metadata` returns it.
fn listed_profile(name: &str, version: &str, uid: &str) -> Value {
    serde_json::json!({"metadata": {"name": name, "uid": uid}, "spec": {"cloudType": "aws", "version": version}})
}

/// A cluster listing entry, as `GET /v1/dashboard/spectroclusters/meta` returns it.
fn listed_cluster(name: &str, uid: &str, state: &str) -> Value {
    serde_json::json!({
        "metadata": {"name": name, "uid": uid, "annotations": {"projectUid": "p1", "scope": "project"}},
        "spec": {"cloudType": "aws", "cloudRegion": "us-east-2"},
        "status": {"state": state},
    })
}

fn projects(names: &[(&str, &str)]) -> Value {
    let items: Vec<Value> = names
        .iter()
        .map(|(name, uid)| serde_json::json!({"metadata": {"name": name, "uid": uid}}))
        .collect();
    serde_json::json!({ "items": items })
}

#[test]
fn list_names_each_profile_and_cluster_as_a_ref_in_sorted_order() {
    let api = MemoryApi::new()
        .in_project("p1")
        .with(
            PROFILE_LISTING,
            serde_json::json!({"items": [
                listed_profile("lab-profile", "1.10.0", "u3"),
                listed_profile("addons", "2.0.0", "u9"),
                listed_profile("lab-profile", "1.9.0", "u2"),
            ]}),
        )
        .with(
            CLUSTER_LISTING,
            serde_json::json!([
                listed_cluster("workload-b", "c2", "Pending"),
                listed_cluster("workload-a", "c1", "Running"),
            ]),
        )
        .with(PROJECT_LISTING, projects(&[("lab", "p1")]));
    let scopes: RefCell<Vec<Option<String>>> = RefCell::new(Vec::new());
    let connect = |scope: Option<&str>| {
        scopes.borrow_mut().push(scope.map(str::to_string));
        Ok(Palette::new(api.clone()))
    };
    let l = ops::list(&ListOptions::default(), connect, "t1", &Progress::silent()).unwrap();
    assert_eq!(l.schema, ops::LIST_SCHEMA);
    assert_eq!(
        (l.project_uid.as_str(), l.project_name.as_str()),
        ("p1", "lab")
    );
    let refs = |v: &[ops::ListedProfile]| v.iter().map(|p| p.reference.clone()).collect::<Vec<_>>();
    let profiles = l.profiles.as_ref().expect("profiles are listed by default");
    assert_eq!(
        refs(profiles),
        ["addons@2.0.0", "lab-profile@1.9.0", "lab-profile@1.10.0"],
        "by name, then natural version order"
    );
    assert_eq!(
        (profiles[1].uid.as_str(), profiles[1].cloud_type.as_str()),
        ("u2", "aws")
    );
    assert!(
        profiles
            .iter()
            .all(|p| p.packs.is_none() && p.error.is_none())
    );
    let clusters = l.clusters.as_ref().expect("clusters are listed by default");
    assert_eq!(
        clusters
            .iter()
            .map(|c| (c.reference.as_str(), c.name.as_str(), c.state.as_str()))
            .collect::<Vec<_>>(),
        [
            ("cluster:c1", "workload-a", "Running"),
            ("cluster:c2", "workload-b", "Pending")
        ]
    );
    assert!(l.projects.is_none(), "projects only when asked");
    assert_eq!(
        *scopes.borrow(),
        vec![None],
        "one client, in the configured scope"
    );
    assert_eq!(
        api.calls(&profile_route("u2")),
        0,
        "no profile document without --packs"
    );

    let text = common::terminal().render_list(&l).unwrap();
    assert!(text.starts_with("project lab (uid p1)\n"), "{text}");
    assert!(text.contains("\n3 profiles\n"), "{text}");
    assert!(text.contains("  lab-profile@1.10.0  u3  aws\n"), "{text}");
    assert!(
        text.contains("  cluster:c1  workload-a  Running  aws\n"),
        "{text}"
    );
    let md = common::markdown().render_list(&l).unwrap();
    assert!(md.contains("| `lab-profile@1.9.0` | `u2` | aws |"), "{md}");
    assert!(
        md.contains("| `cluster:c2` | workload-b | Pending | aws |"),
        "{md}"
    );
    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_list(&l)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["profiles"][0]["ref"], "addons@2.0.0");
    assert_eq!(json["clusters"][1]["cloudType"], "aws");
    assert!(json.get("projects").is_none(), "{json}");
}

/// Issue #616. `list` reads a version that does not parse by its first three numbers, as
/// `@latest` does: `1.1.8.develop` is below `1.1.8`, and `10.4.0.1` is above `10.4.0`. Issue #714:
/// `1.2.develop` is a pre-release of `1.2.0`, so it is above `1.1.10`.
#[test]
fn list_orders_a_version_that_does_not_parse_by_its_three_numbers() {
    let listing = serde_json::json!({"items": [
        listed_profile("barney", "1.1.10", "u1"),
        listed_profile("barney", "1.1.8.develop", "u2"),
        listed_profile("barney", "1.2.develop", "u7"),
        listed_profile("barney", "1.1.8", "u3"),
        listed_profile("four", "10.5.0", "u4"),
        listed_profile("four", "10.4.0.1", "u5"),
        listed_profile("four", "10.4.0", "u6"),
    ]});
    let api = MemoryApi::new()
        .in_project("p1")
        .with(PROFILE_LISTING, listing)
        .with(CLUSTER_LISTING, serde_json::json!([]))
        .with(PROJECT_LISTING, projects(&[("lab", "p1")]));
    let connect = |_: Option<&str>| Ok(Palette::new(api.clone()));
    let l = ops::list(&ListOptions::default(), connect, "t1", &Progress::silent()).unwrap();
    let refs: Vec<&str> = l
        .profiles
        .as_ref()
        .expect("profiles are listed by default")
        .iter()
        .map(|p| p.reference.as_str())
        .collect();
    assert_eq!(
        refs,
        [
            "barney@1.1.8.develop",
            "barney@1.1.8",
            "barney@1.1.10",
            "barney@1.2.develop",
            "four@10.4.0",
            "four@10.4.0.1",
            "four@10.5.0",
        ]
    );
}

#[test]
fn list_with_a_pack_reads_each_profile_and_keeps_those_that_carry_it() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_fixture(
        serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A),
        dir.path(),
        "b",
        UID_B,
    )
    .with(
        PROFILE_LISTING,
        serde_json::json!({"items": [
            listed_profile("lab-profile", "1.3.0", UID_B),
            listed_profile("lab-profile", "1.2.0", UID_A),
            listed_profile("hidden", "1.0.0", "u-denied"),
        ]}),
    )
    .with(
        REGISTRY_LISTING,
        serde_json::json!({"items": [
            {"name": "Public Repo", "uid": common::PUBLIC_REGISTRY, "kind": "pack"},
        ]}),
    )
    .failing(profile_route("u-denied"), 403, "forbidden");
    let options = ListOptions {
        pack: Some("CERT-man".into()),
        ..ListOptions::default()
    };
    let l = listing(&api, &options).unwrap();
    assert!(
        l.clusters.is_none(),
        "--pack lists profiles only unless kinds are named"
    );
    assert_eq!(api.calls(CLUSTER_LISTING), 0);
    assert_eq!(
        (l.project_uid.as_str(), l.project_name.as_str()),
        ("", ""),
        "tenant scope has no project"
    );
    let profiles = l.profiles.as_ref().unwrap();
    assert_eq!(
        profiles
            .iter()
            .map(|p| p.reference.as_str())
            .collect::<Vec<_>>(),
        ["hidden@1.0.0", "lab-profile@1.3.0"],
        "1.2.0 has no cert-manager; a profile that cannot be read stays, with its error"
    );
    let denied = &profiles[0];
    assert_eq!(denied.error.as_ref().unwrap()["kind"], "api");
    assert!(denied.packs.is_none());
    let b = &profiles[1];
    assert_eq!(
        (b.profile_type.as_ref().map(Open::as_str), b.consumers),
        (Some("cluster"), Some(2))
    );
    let packs = b.packs.as_ref().unwrap();
    assert_eq!(packs.len(), 7, "every layer is shown, not only the match");
    let cm = packs.iter().find(|p| p.name == "cert-manager").unwrap();
    assert_eq!(
        (cm.layer.as_str(), cm.version.as_str(), cm.tag.as_str()),
        ("addon", "1.14.0", "1.14.0")
    );
    assert_eq!(
        (cm.registry_uid.as_str(), cm.registry.as_str()),
        (common::PUBLIC_REGISTRY, "Public Repo"),
        "the row names the registry that the profile takes the pack from"
    );

    let text = common::terminal().render_list(&l).unwrap();
    assert!(text.starts_with("tenant scope\n"), "{text}");
    assert!(
        text.contains("\n2 profiles with a pack matching 'CERT-man'\n"),
        "{text}"
    );
    assert!(
        text.contains(&format!(
            "  lab-profile@1.3.0  {UID_B}  aws  cluster  2 consumers\n"
        )),
        "{text}"
    );
    assert!(
        text.contains("    addon  cert-manager         1.14.0  Public Repo\n"),
        "{text}"
    );
    assert!(text.contains("forbidden"), "the error is shown: {text}");
    let md = common::markdown().render_list(&l).unwrap();
    assert!(
        md.contains("addon/cert-manager 1.14.0 in Public Repo"),
        "{md}"
    );
}

/// The profile document of `lab-profile@<version>` with one CNI pack at `cni`. A version that
/// nobody published has its packs in `spec.draft`, an empty `spec.published`, and
/// `status.isPublished` false, as Palette gives it after a create with no publish.
fn draft_or_published(uid: &str, version: &str, cni: &str, published: bool) -> Value {
    let with_cni = template(
        "aws",
        "cluster",
        serde_json::json!([
            {"name": "cni-cilium-oss", "layer": "cni", "type": "spectro", "tag": cni, "version": cni}
        ]),
    );
    let (published_template, draft) = if published {
        (with_cni, no_template())
    } else {
        (no_template(), with_cni)
    };
    profile_document(uid, "lab-profile", version, published_template, Some(draft))
}

/// A version with no pack in either template: `list` and a record read its type from the draft.
/// A version that nobody published reads the same template.
#[test]
fn list_and_a_record_read_the_type_of_a_version_with_no_pack_from_one_template() {
    let doc = profile_document(
        "u-e1",
        "lab-profile",
        "1.2.0",
        template("aws", "cluster", serde_json::json!([])),
        Some(template("aws", "add-on", serde_json::json!([]))),
    );
    let api = serve_profile(
        MemoryApi::new().with(
            PROFILE_LISTING,
            serde_json::json!({"items": [listed_profile("lab-profile", "1.2.0", "u-e1")]}),
        ),
        "u-e1",
        doc,
        no_manifests(),
        no_variables(),
    );
    let l = listing(
        &api,
        &ListOptions {
            kinds: vec![ListKind::Profiles],
            packs: true,
            ..ListOptions::default()
        },
    )
    .unwrap();
    let record = snapshot_of(&api, "u-e1").unwrap();
    assert_eq!(record.source.kind, "add-on");
    assert_eq!(
        l.profiles.unwrap()[0]
            .profile_type
            .as_ref()
            .map(Open::as_str),
        Some(record.source.kind.as_str())
    );
}

/// A project with `lab-profile@1.0.0`, which is published, and `lab-profile@1.1.0`, which
/// nobody published.
fn a_published_and_a_draft_version() -> MemoryApi {
    MemoryApi::new()
        .with(
            PROFILE_LISTING,
            serde_json::json!({"items": [
                listed_profile("lab-profile", "1.1.0", "u-d1"),
                listed_profile("lab-profile", "1.0.0", "u-p1"),
            ]}),
        )
        .with(
            profile_route("u-p1"),
            draft_or_published("u-p1", "1.0.0", "1.19.6", true),
        )
        .with(
            profile_route("u-d1"),
            draft_or_published("u-d1", "1.1.0", "1.20.1", false),
        )
}

#[test]
fn list_with_packs_says_for_each_version_whether_it_is_published_and_marks_a_draft() {
    let api = a_published_and_a_draft_version();
    let list = |options: &ListOptions| listing(&api, options).unwrap();
    let plain = list(&ListOptions {
        kinds: vec![ListKind::Profiles],
        ..ListOptions::default()
    });
    assert!(
        plain
            .profiles
            .unwrap()
            .iter()
            .all(|p| p.published.is_none()),
        "with no --packs, strata reads no profile document and does not know"
    );

    let l = list(&ListOptions {
        kinds: vec![ListKind::Profiles],
        packs: true,
        ..ListOptions::default()
    });
    let profiles = l.profiles.as_ref().unwrap();
    assert_eq!(
        profiles
            .iter()
            .map(|p| (p.reference.as_str(), p.published, p.is_draft()))
            .collect::<Vec<_>>(),
        [
            ("lab-profile@1.0.0", Some(true), false),
            ("lab-profile@1.1.0", Some(false), true),
        ]
    );
    assert_eq!(
        profiles[1].packs.as_ref().unwrap()[0].version,
        "1.20.1",
        "the packs of a version that nobody published come from its draft"
    );

    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_list(&l)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["profiles"][0]["published"], true, "{json}");
    assert_eq!(json["profiles"][1]["published"], false, "{json}");

    let text = common::terminal().render_list(&l).unwrap();
    assert!(
        text.contains("  lab-profile@1.0.0  u-p1  aws  cluster  no consumers\n"),
        "{text}"
    );
    assert!(
        text.contains("  lab-profile@1.1.0  u-d1  aws  cluster  no consumers  draft\n"),
        "{text}"
    );
    let md = common::markdown().render_list(&l).unwrap();
    assert!(
        md.contains("| `lab-profile@1.0.0` | `u-p1` | aws |"),
        "{md}"
    );
    assert!(
        md.contains("| `lab-profile@1.1.0` draft | `u-d1` | aws |"),
        "{md}"
    );
}

/// The body of the profile search for one scope, as strata sends it.
fn profile_search_body(scope: &str) -> Value {
    serde_json::json!({
        "filter": {"scope": scope},
        "sort": [{"field": "profileName", "order": "asc"}],
    })
}

/// An answer of the profile search: one profile name with the versions `(version, uid)`.
fn profile_search_answer(name: &str, scope: &str, versions: &[(&str, &str)]) -> Value {
    let versions: Vec<Value> = versions
        .iter()
        .map(|(v, uid)| serde_json::json!({"version": v, "uid": uid}))
        .collect();
    serde_json::json!({
        "items": [{"metadata": {"name": name, "annotations": {"scope": scope}},
                   "specSummary": {"versions": versions}}],
        "listmeta": {"count": 1, "limit": 50, "offset": 0},
    })
}

/// A profile document of `aws-stack@1.1.1` under `uid`, with one pack, published or a draft, and
/// with `metadata.annotations.scope` when `scope` names one.
fn scoped_document(uid: &str, scope: Option<&str>, published: bool) -> Value {
    let packs = serde_json::json!([{
        "name": "kubernetes", "layer": "k8s", "type": "spectro", "tag": "1.19.6",
        "version": "1.19.6", "packUid": "pk-1", "registryUid": "r1", "digest": "sha256:0123",
        "values": "a: 1",
    }]);
    let (in_published, in_draft) = if published {
        (packs, serde_json::json!([]))
    } else {
        (serde_json::json!([]), packs)
    };
    let mut doc = profile_document(
        uid,
        "aws-stack",
        "1.1.1",
        template("aws", "cluster", in_published),
        Some(template("aws", "cluster", in_draft)),
    );
    if let Some(scope) = scope {
        doc["metadata"]["annotations"]["scope"] = Value::from(scope);
    }
    doc
}

/// A listing in the project `p1` with a profile of the tenant and a profile of the project, both
/// `aws-stack@1.1.1`, and `addons@2.0.0` of the project. The profile search names the tenant
/// version. The document of each version of `aws-stack` gives its scope; the document of `addons`
/// gives none.
fn a_tenant_and_a_project_version() -> MemoryApi {
    let empty =
        serde_json::json!({"items": [], "listmeta": {"count": 0, "limit": 50, "offset": 0}});
    let api = MemoryApi::new()
        .in_project("p1")
        .with(
            PROFILE_LISTING,
            serde_json::json!({"items": [
                listed_profile("aws-stack", "1.1.1", "u-tenant"),
                listed_profile("addons", "2.0.0", "u-addons"),
                listed_profile("aws-stack", "1.1.1", "u-project"),
            ]}),
        )
        .with(PROJECT_LISTING, projects(&[("lab", "p1")]))
        .with_post(
            PROFILE_SEARCH,
            &profile_search_body("tenant"),
            profile_search_answer("aws-stack", "tenant", &[("1.1.1", "u-tenant")]),
        )
        .with_post(PROFILE_SEARCH, &profile_search_body("system"), empty)
        .with(
            profile_route("u-project"),
            scoped_document("u-project", Some("project"), true),
        )
        .with(
            profile_route("u-addons"),
            scoped_document("u-addons", None, true),
        );
    serve_profile(
        api,
        "u-tenant",
        scoped_document("u-tenant", Some("tenant"), false),
        no_manifests(),
        no_variables(),
    )
}

#[test]
fn list_gives_each_version_its_scope_from_the_profile_search_and_marks_a_version_of_another_scope()
{
    let api = a_tenant_and_a_project_version();
    let l = listing(
        &api,
        &ListOptions {
            kinds: vec![ListKind::Profiles],
            ..ListOptions::default()
        },
    )
    .unwrap();
    let profiles = l.profiles.as_ref().unwrap();
    assert_eq!(
        profiles
            .iter()
            .map(|p| (p.reference.as_str(), p.uid.as_str(), p.scope))
            .collect::<Vec<_>>(),
        [
            ("addons@2.0.0", "u-addons", Some(ProfileScope::Project)),
            ("aws-stack@1.1.1", "u-project", Some(ProfileScope::Project)),
            ("aws-stack@1.1.1", "u-tenant", Some(ProfileScope::Tenant)),
        ],
        "the search names the tenant version; each other version is of the project"
    );
    assert_eq!(
        api.posted()
            .iter()
            .map(|(_, body)| body["filter"]["scope"].clone())
            .collect::<Vec<_>>(),
        ["tenant", "system"],
        "in a project, one search for the tenant versions and one for the system versions"
    );
    assert_eq!(
        api.calls(&profile_route("u-tenant")),
        0,
        "no profile document without --packs"
    );

    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_list(&l)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["profiles"][0]["scope"], "project", "{json}");
    assert_eq!(json["profiles"][1]["scope"], "project", "{json}");
    assert_eq!(json["profiles"][2]["scope"], "tenant", "{json}");

    let text = common::terminal().render_list(&l).unwrap();
    assert!(
        text.contains("  aws-stack@1.1.1  u-project  aws\n"),
        "a version of the scope of the listing has no mark: {text}"
    );
    assert!(
        text.contains("  aws-stack@1.1.1  u-tenant   aws  tenant\n"),
        "the row of the tenant version ends with its scope: {text}"
    );
    let md = common::markdown().render_list(&l).unwrap();
    assert!(
        md.contains("| `aws-stack@1.1.1` | `u-project` | aws |"),
        "{md}"
    );
    assert!(
        md.contains("| `aws-stack@1.1.1` tenant | `u-tenant` | aws |"),
        "{md}"
    );
}

#[test]
fn list_with_packs_reads_the_scope_in_each_document_and_joins_the_marks_of_a_tenant_draft() {
    let api = a_tenant_and_a_project_version();
    let l = listing(
        &api,
        &ListOptions {
            kinds: vec![ListKind::Profiles],
            packs: true,
            ..ListOptions::default()
        },
    )
    .unwrap();
    let profiles = l.profiles.as_ref().unwrap();
    assert_eq!(
        profiles
            .iter()
            .map(|p| (p.uid.as_str(), p.scope, p.is_draft()))
            .collect::<Vec<_>>(),
        [
            ("u-addons", None, false),
            ("u-project", Some(ProfileScope::Project), false),
            ("u-tenant", Some(ProfileScope::Tenant), true),
        ],
        "the scope comes from the annotation of each document, and a document with none gives none"
    );
    assert!(
        api.posted().is_empty(),
        "with --packs, strata sends no search"
    );
    let text = common::terminal().render_list(&l).unwrap();
    assert!(
        text.contains("  aws-stack@1.1.1  u-project  aws  cluster  no consumers\n"),
        "{text}"
    );
    assert!(
        text.contains("  aws-stack@1.1.1  u-tenant   aws  cluster  no consumers  tenant, draft\n"),
        "the marks end the row, after the consumers: {text}"
    );
    let md = common::markdown().render_list(&l).unwrap();
    assert!(
        md.contains("| `aws-stack@1.1.1` tenant, draft | `u-tenant` | aws |"),
        "{md}"
    );
    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_list(&l)
            .unwrap(),
    )
    .unwrap();
    assert!(
        json["profiles"][0].get("scope").is_none(),
        "no scope when the document gives none: {json}"
    );
    assert_eq!(json["profiles"][2]["scope"], "tenant", "{json}");
}

#[test]
fn list_keeps_the_listing_alone_when_palette_has_no_profile_search() {
    let api = MemoryApi::new()
        .in_project("p1")
        .with(
            PROFILE_LISTING,
            serde_json::json!({"items": [
                listed_profile("aws-stack", "1.1.1", "u-tenant"),
                listed_profile("aws-stack", "1.1.1", "u-project"),
            ]}),
        )
        .with(PROJECT_LISTING, projects(&[("lab", "p1")]));
    // Palette answers 404 or 405 for a route that it does not have
    let not_allowed = api.clone().failing(post(PROFILE_SEARCH), 405, "{}");
    let l = listing(
        &not_allowed,
        &ListOptions {
            kinds: vec![ListKind::Profiles],
            ..ListOptions::default()
        },
    )
    .unwrap();
    assert!(
        l.profiles
            .as_ref()
            .unwrap()
            .iter()
            .all(|p| p.scope.is_none()),
        "a 405 is no search, as a 404 is"
    );
    let l = listing(
        &api,
        &ListOptions {
            kinds: vec![ListKind::Profiles],
            ..ListOptions::default()
        },
    )
    .unwrap();
    let profiles = l.profiles.as_ref().unwrap();
    assert_eq!(profiles.len(), 2);
    assert!(
        profiles.iter().all(|p| p.scope.is_none()),
        "the search answered 404, so the scopes stay unknown"
    );
    assert_eq!(
        api.posted().len(),
        1,
        "the 404 of the first search ends the question"
    );
    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_list(&l)
            .unwrap(),
    )
    .unwrap();
    assert!(json["profiles"][0].get("scope").is_none(), "{json}");
    let text = common::terminal().render_list(&l).unwrap();
    assert!(
        !text.contains("  tenant\n"),
        "no mark without a scope: {text}"
    );
}

#[test]
fn list_at_tenant_scope_asks_for_the_system_versions_and_marks_no_tenant_version() {
    let api = MemoryApi::new()
        .with(
            PROFILE_LISTING,
            serde_json::json!({"items": [listed_profile("aws-stack", "1.1.1", "u-tenant")]}),
        )
        .with_post(
            PROFILE_SEARCH,
            &profile_search_body("system"),
            serde_json::json!({"items": [], "listmeta": {"count": 0, "limit": 50, "offset": 0}}),
        );
    let l = listing(
        &api,
        &ListOptions {
            kinds: vec![ListKind::Profiles],
            project: Some(String::new()),
            ..ListOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        l.profiles.as_ref().unwrap()[0].scope,
        Some(ProfileScope::Tenant),
        "at tenant scope, a version that the system search does not name is of the tenant"
    );
    assert_eq!(
        api.posted()
            .iter()
            .map(|(_, body)| body["filter"]["scope"].clone())
            .collect::<Vec<_>>(),
        ["system"],
        "tenant scope lists no profile of a project, so strata searches the system versions alone"
    );
    let text = common::terminal().render_list(&l).unwrap();
    assert!(
        text.contains("  aws-stack@1.1.1  u-tenant  aws\n"),
        "a tenant version has no mark at tenant scope: {text}"
    );
}

#[test]
fn list_sends_no_profile_search_when_the_listing_holds_no_profile() {
    // The searches only give a scope to the versions of the listing, so an empty listing needs
    // none of them (#317).
    let empty =
        serde_json::json!({"items": [], "listmeta": {"count": 0, "limit": 50, "offset": 0}});
    let api = MemoryApi::new()
        .in_project("p1")
        .with(PROFILE_LISTING, serde_json::json!({"items": []}))
        .with_post(
            PROFILE_SEARCH,
            &profile_search_body("tenant"),
            empty.clone(),
        )
        .with_post(PROFILE_SEARCH, &profile_search_body("system"), empty);
    let l = listing(
        &api,
        &ListOptions {
            kinds: vec![ListKind::Profiles],
            ..ListOptions::default()
        },
    )
    .unwrap();
    assert!(l.profiles.as_ref().unwrap().is_empty());
    assert_eq!(api.posted().len(), 0, "{:?}", api.posted());
}

#[test]
fn snapshot_of_a_name_and_version_that_a_tenant_and_a_project_profile_share_names_both_uids() {
    let api = a_tenant_and_a_project_version();
    let err = snapshot_of_at(&api, "aws-stack@1.1.1", "t1").unwrap_err();
    assert_eq!(err.kind(), "profile-name-ambiguous");
    assert_eq!(err.exit_code(), 2);
    assert_eq!(
        err.to_json()["profiles"],
        serde_json::json!([
            {"uid": "u-project", "scope": "project"},
            {"uid": "u-tenant", "scope": "tenant"},
        ]),
        "{err}"
    );
    assert!(
        err.to_string()
            .contains("u-project (project), u-tenant (tenant); give the uid in place of the name"),
        "{err}"
    );
    assert_eq!(
        api.calls(PROFILE_SEARCH),
        0,
        "the error reads the two documents, and no search"
    );

    // the uid names one version, and records it
    let record = snapshot_of_at(&api, "u-tenant", "t1").unwrap();
    assert_eq!(record.source.uid, "u-tenant");
}

#[test]
fn list_with_a_pack_keeps_a_draft_version_that_carries_the_pack_with_its_mark() {
    let api = a_published_and_a_draft_version();
    let l = listing(
        &api,
        &ListOptions {
            pack: Some("cilium".into()),
            ..ListOptions::default()
        },
    )
    .unwrap();
    let profiles = l.profiles.as_ref().unwrap();
    assert_eq!(
        profiles
            .iter()
            .map(|p| (p.reference.as_str(), p.is_draft()))
            .collect::<Vec<_>>(),
        [("lab-profile@1.0.0", false), ("lab-profile@1.1.0", true)],
        "the draft carries the pack, so it stays"
    );
    let text = common::terminal().render_list(&l).unwrap();
    assert!(
        text.contains("\n2 profiles with a pack matching 'cilium'\n"),
        "{text}"
    );
    assert!(
        text.contains("  lab-profile@1.1.0  u-d1  aws  cluster  no consumers  draft\n"),
        "{text}"
    );
}

/// `lab-profile@1.0.0` is linked by a cluster template and run by no cluster. `lab-profile@1.1.0`
/// is run by the cluster `fred` and linked by the same template. Palette names both kinds in the
/// profile document, and refuses the delete of each version.
fn a_linked_and_a_shared_version() -> MemoryApi {
    let template = serde_json::json!([
        {"uid": "tmpl-1", "name": "barney-template", "projectUid": "p1", "kind": "clustertemplate"}
    ]);
    let mut linked = draft_or_published("u-l1", "1.0.0", "1.19.6", true);
    linked["status"]["inUseClusters"] = serde_json::json!([]);
    linked["status"]["inUseClusterTemplates"] = template.clone();
    let mut shared = draft_or_published("u-s1", "1.1.0", "1.20.1", true);
    shared["status"]["inUseClusterUids"] = serde_json::json!(["c-fred"]);
    shared["status"]["inUseClusters"] =
        serde_json::json!([{"uid": "c-fred", "name": "fred", "projectUid": "p1"}]);
    shared["status"]["inUseClusterTemplates"] = template;
    MemoryApi::new()
        .with(
            PROFILE_LISTING,
            serde_json::json!({"items": [
                listed_profile("lab-profile", "1.0.0", "u-l1"),
                listed_profile("lab-profile", "1.1.0", "u-s1"),
            ]}),
        )
        .with(profile_route("u-l1"), linked)
        .with(profile_route("u-s1"), shared)
        .with(
            cluster_route("c-fred"),
            serde_json::json!({
                "metadata": {"name": "fred", "uid": "c-fred", "annotations": {"projectUid": "p1"}},
                "spec": {"clusterProfileTemplates": [{"uid": "u-s1", "profileVersion": "1.1.0"}]},
                "status": {"state": "Running"},
            }),
        )
}

/// `list --packs` counts each consumer that the profile document names, and not the clusters
/// alone. A version that a template links and no cluster runs has one consumer, because Palette
/// refuses its delete. `--json` gives the number as before, and the count by kind in
/// `consumersByKind`.
#[test]
fn list_with_packs_counts_the_cluster_templates_of_a_version_with_its_clusters() {
    let api = a_linked_and_a_shared_version();
    let l = listing(
        &api,
        &ListOptions {
            kinds: vec![ListKind::Profiles],
            packs: true,
            ..ListOptions::default()
        },
    )
    .unwrap();
    let profiles = l.profiles.as_ref().unwrap();
    assert_eq!(
        profiles
            .iter()
            .map(|p| (p.reference.as_str(), p.consumers, p.consumers_by_kind))
            .collect::<Vec<_>>(),
        [
            (
                "lab-profile@1.0.0",
                Some(1),
                Some(ops::ConsumerCounts {
                    clusters: 0,
                    templates: 1
                })
            ),
            (
                "lab-profile@1.1.0",
                Some(2),
                Some(ops::ConsumerCounts {
                    clusters: 1,
                    templates: 1
                })
            ),
        ]
    );

    let text = common::terminal().render_list(&l).unwrap();
    assert!(
        text.contains("  lab-profile@1.0.0  u-l1  aws  cluster  1 consumer\n"),
        "{text}"
    );
    assert!(
        text.contains("  lab-profile@1.1.0  u-s1  aws  cluster  2 consumers\n"),
        "{text}"
    );
    let md = common::markdown().render_list(&l).unwrap();
    assert!(
        md.contains("| `u-l1` | aws | cluster | 1 consumer |"),
        "{md}"
    );
    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_list(&l)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        (
            &json["profiles"][0]["consumers"],
            &json["profiles"][0]["consumersByKind"]
        ),
        (
            &serde_json::json!(1),
            &serde_json::json!({"clusters": 0, "templates": 1})
        ),
        "{json}"
    );
    assert_eq!(
        (
            &json["profiles"][1]["consumers"],
            &json["profiles"][1]["consumersByKind"]
        ),
        (
            &serde_json::json!(2),
            &serde_json::json!({"clusters": 1, "templates": 1})
        ),
        "{json}"
    );
}

/// `list --packs` and `consumers --list-only` read one profile document, so they give one number
/// for a version: the clusters of its consumer list and the cluster templates that link it.
#[test]
fn list_with_packs_gives_the_count_of_consumers_with_the_list_of_palette_alone() {
    let api = a_linked_and_a_shared_version();
    let l = listing(
        &api,
        &ListOptions {
            kinds: vec![ListKind::Profiles],
            packs: true,
            ..ListOptions::default()
        },
    )
    .unwrap();
    for p in l.profiles.unwrap() {
        let r = ops::consumers_with(
            &Ref::Uid(p.uid.clone()),
            &ops::ConsumersOptions { list_only: true },
            |_| Ok(Palette::new(api.clone())),
            "t1",
            &Progress::silent(),
        )
        .unwrap();
        let counts = p.consumers_by_kind.unwrap();
        assert_eq!(
            (p.consumers, counts.count()),
            (Some(r.count), r.count),
            "{}: list --packs and consumers --list-only differ",
            p.reference
        );
        let of_kind =
            |kind: ops::ConsumerKind| r.consumers.iter().filter(|c| c.kind == kind).count();
        assert_eq!(
            (
                of_kind(ops::ConsumerKind::Cluster),
                of_kind(ops::ConsumerKind::Template)
            ),
            (counts.clusters, counts.templates),
            "{}: the kinds differ",
            p.reference
        );
    }
}

#[test]
fn list_projects_reads_at_tenant_scope_and_a_project_override_scopes_the_rest() {
    let in_p2 = MemoryApi::new()
        .in_project("p2")
        .with(
            PROFILE_LISTING,
            serde_json::json!({"items": [listed_profile("edge", "1.0.0", "u1")]}),
        )
        .with(PROJECT_LISTING, projects(&[("prod", "p2")]));
    let tenant = MemoryApi::new().with(PROJECT_LISTING, projects(&[("prod", "p2"), ("dev", "p1")]));
    let scopes: RefCell<Vec<Option<String>>> = RefCell::new(Vec::new());
    let connect = |scope: Option<&str>| {
        scopes.borrow_mut().push(scope.map(str::to_string));
        Ok(Palette::new(match scope {
            Some(ops::TENANT_SCOPE) => tenant.clone(),
            _ => in_p2.clone(),
        }))
    };
    let options = ListOptions {
        kinds: vec![ListKind::Projects, ListKind::Profiles],
        project: Some("p2".into()),
        ..ListOptions::default()
    };
    let l = ops::list(&options, connect, "t1", &Progress::silent()).unwrap();
    assert_eq!(
        l.projects
            .as_ref()
            .unwrap()
            .iter()
            .map(|p| (p.name.as_str(), p.uid.as_str()))
            .collect::<Vec<_>>(),
        [("dev", "p1"), ("prod", "p2")],
        "every project the key can see, by name"
    );
    assert_eq!(l.profiles.as_ref().unwrap()[0].reference, "edge@1.0.0");
    assert!(l.clusters.is_none(), "named kinds only");
    assert_eq!(
        (l.project_uid.as_str(), l.project_name.as_str()),
        ("p2", "prod")
    );
    assert_eq!(
        *scopes.borrow(),
        vec![Some("p2".to_string()), Some(String::new())],
        "the override scopes profiles; projects are read at tenant scope"
    );
    assert_eq!(
        in_p2.calls(PROJECT_LISTING),
        0,
        "the tenant listing already names the project"
    );
    let text = common::terminal().render_list(&l).unwrap();
    assert!(
        text.contains("\n2 projects\n  dev   p1\n  prod  p2\n"),
        "{text}"
    );
}

#[test]
fn list_reads_the_project_listing_one_time_for_the_clusters_and_the_projects() {
    // the project of each cluster has a name, and the projects are a kind of their own
    let mut other = listed_cluster("edge-1", "c7", "Running");
    other["metadata"]["annotations"]["projectUid"] = serde_json::json!("p2");
    let api = MemoryApi::new()
        .with(
            CLUSTER_LISTING,
            serde_json::json!([listed_cluster("workload-a", "c1", "Running"), other]),
        )
        .with(PROJECT_LISTING, projects(&[("dev", "p1"), ("prod", "p2")]));
    let options = ListOptions {
        kinds: vec![ListKind::Clusters, ListKind::Projects],
        ..ListOptions::default()
    };
    let l = listing(&api, &options).unwrap();
    let named: Vec<_> = l
        .clusters
        .as_ref()
        .unwrap()
        .iter()
        .map(|c| c.project_name.as_str())
        .collect();
    assert_eq!(named, ["prod", "dev"]);
    assert_eq!(l.projects.as_ref().unwrap().len(), 2);
    assert_eq!(api.calls(PROJECT_LISTING), 1);
}

#[test]
fn list_in_a_project_reads_the_project_listing_one_time_for_the_clusters_and_the_scope() {
    // the clusters need the name of their project, and the scope needs its own name
    let api = two_clusters_with_search(&[]);
    let l = clusters_listing(&api, ops::ClusterQuery::default());
    assert_eq!(l.project_name, "lab");
    assert!(
        l.clusters
            .as_ref()
            .unwrap()
            .iter()
            .all(|c| c.project_name == "lab")
    );
    assert_eq!(api.calls(PROJECT_LISTING), 1);
}

#[test]
fn list_with_a_query_reads_the_project_listing_one_time_for_the_clusters_and_the_projects() {
    let payments = fleet_row(
        "c-pay",
        "payments",
        "p2",
        serde_json::json!({"env": "prod"}),
    );
    let api = serve_fleet(MemoryApi::new(), vec![payments.clone()], vec![payments]);
    let options = ListOptions {
        kinds: vec![ListKind::Clusters, ListKind::Projects],
        query: ops::ClusterQuery {
            tags: vec!["env=prod".into()],
            filter: None,
        },
        ..ListOptions::default()
    };
    let l = listing(&api, &options).unwrap();
    assert_eq!(l.clusters.as_ref().unwrap()[0].project_name, "prod");
    assert_eq!(l.projects.as_ref().unwrap().len(), 2);
    assert_eq!(api.calls(PROJECT_LISTING), 1);
}

#[test]
fn list_at_tenant_scope_names_the_project_of_each_cluster() {
    let mut other = listed_cluster("edge-1", "c7", "Running");
    other["metadata"]["annotations"]["projectUid"] = serde_json::json!("p2");
    let api = MemoryApi::new().with(
        CLUSTER_LISTING,
        serde_json::json!([listed_cluster("workload-a", "c1", "Running"), other]),
    );
    let options = ListOptions {
        kinds: vec![ListKind::Clusters],
        ..ListOptions::default()
    };
    let l = listing(&api, &options).unwrap();
    let clusters = l.clusters.as_ref().unwrap();
    assert_eq!(
        clusters
            .iter()
            .map(|c| c.project_uid.as_str())
            .collect::<Vec<_>>(),
        ["p2", "p1"],
        "a cluster ref works only in its own project, so each carries it"
    );
    let text = common::terminal().render_list(&l).unwrap();
    assert!(
        text.contains("  cluster:c7  edge-1      Running  aws  project p2\n"),
        "{text}"
    );
    let md = common::markdown().render_list(&l).unwrap();
    assert!(
        md.contains("| `cluster:c1` | workload-a | Running | aws | p1 |"),
        "{md}"
    );

    // With the project listing, each line names the project. A uid names no project to a reader,
    // and the listing is one request that `list projects` already makes.
    let named = api.clone().with(
        PROJECT_LISTING,
        serde_json::json!({"items": [
            {"metadata": {"name": "dev", "uid": "p1"}},
            {"metadata": {"name": "prod", "uid": "p2"}},
        ]}),
    );
    let l = listing(&named, &options).unwrap();
    let text = common::terminal().render_list(&l).unwrap();
    assert!(
        text.contains("project prod (uid p2)"),
        "the line names the project: {text}"
    );
}

/// The listing of the project `p1` with two clusters, and the search schema `properties`.
fn two_clusters_with_search(properties: &[&str]) -> MemoryApi {
    MemoryApi::new()
        .in_project("p1")
        .with(
            CLUSTER_LISTING,
            serde_json::json!([
                listed_cluster("workload-a", "c1", "Running"),
                listed_cluster("workload-b", "c2", "Running"),
            ]),
        )
        .with(CLUSTER_SEARCH_SCHEMA, search_schema(properties))
        .with(PROJECT_LISTING, projects(&[("lab", "p1")]))
}

/// The cluster listing that `query` selects, from `api`.
fn clusters_listing(api: &MemoryApi, query: ops::ClusterQuery) -> ops::Listing {
    listing(
        api,
        &ListOptions {
            kinds: vec![ListKind::Clusters],
            query,
            ..ListOptions::default()
        },
    )
    .unwrap()
}

#[test]
fn list_clusters_names_a_virtual_cluster_with_its_cluster_group_from_the_search() {
    // The listing omits each virtual cluster. So strata asks the search for them, with the
    // one condition environment eq nested, in the scope of the listing (#116).
    let api = two_clusters_with_search(&["clusterName", "tags", "environment"]).with_post(
        CLUSTER_SEARCH,
        &virtual_search(vec![]),
        serde_json::json!({"items": [virtual_item("c-v", "vc-1", "p1", ("g1", "dev-group"),
            ("c1", "workload-a"))], "listmeta": {"continue": ""}}),
    );
    let l = clusters_listing(&api, ops::ClusterQuery::default());
    let clusters = l.clusters.as_ref().unwrap();
    assert_eq!(
        clusters.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        ["vc-1", "workload-a", "workload-b"],
        "the virtual cluster sorts with the others"
    );
    let vc = &clusters[0];
    assert_eq!(
        (vc.reference.as_str(), vc.cloud_type.as_str()),
        ("cluster:c-v", "nested")
    );
    let v = vc
        .virtual_cluster
        .as_ref()
        .expect("the virtual cluster says so");
    let named = |o: &Option<ops::NamedObject>| o.as_ref().map(|o| (o.uid.clone(), o.name.clone()));
    assert_eq!(
        named(&v.cluster_group),
        Some(("g1".to_string(), "dev-group".to_string()))
    );
    assert_eq!(
        named(&v.host_cluster),
        Some(("c1".to_string(), "workload-a".to_string()))
    );
    assert!(clusters[1..].iter().all(|c| c.virtual_cluster.is_none()));
    assert_eq!(l.includes_virtual, Some(true));
    let posted = api.posted();
    assert_eq!(posted.len(), 1, "one search, for the virtual clusters");
    assert_eq!(
        posted[0].1["filter"]["filterGroups"]
            .as_array()
            .map(Vec::len),
        Some(1),
        "with the one condition: {}",
        posted[0].1
    );

    let text = common::terminal().render_list(&l).unwrap();
    assert!(text.contains("\n3 clusters\n"), "{text}");
    assert!(
        text.contains("  cluster:c-v  vc-1        Running  nested  virtual (group dev-group)\n"),
        "{text}"
    );
    assert!(
        text.contains("  cluster:c1   workload-a  Running  aws\n"),
        "{text}"
    );
    assert_eq!(text.matches("virtual").count(), 1, "{text}");
    let md = common::markdown().render_list(&l).unwrap();
    assert!(
        md.contains("| ref | name | state | cloud | virtual |\n"),
        "{md}"
    );
    assert!(
        md.contains("| `cluster:c-v` | vc-1 | Running | nested | group dev-group |\n"),
        "{md}"
    );
    assert!(
        md.contains("| `cluster:c1` | workload-a | Running | aws | - |\n"),
        "{md}"
    );
    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_list(&l)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["includesVirtual"], true);
    assert_eq!(
        json["clusters"][0]["virtual"]["clusterGroup"]["name"],
        "dev-group"
    );
    assert_eq!(json["clusters"][0]["virtual"]["hostCluster"]["uid"], "c1");
    assert!(
        json["clusters"][1].get("virtual").is_none(),
        "a cluster that is not virtual has no virtual: {json}"
    );
}

#[test]
fn list_clusters_says_that_it_holds_no_virtual_cluster_when_the_search_takes_no_environment() {
    // An older Palette: its search takes no environment, or it has no search at all. Strata
    // asks it for no virtual cluster, and the heading says so, as a query does (#116).
    let older = two_clusters_with_search(&["clusterName", "tags"]);
    let l = clusters_listing(&older, ops::ClusterQuery::default());
    assert_eq!(l.includes_virtual, Some(false));
    assert!(older.posted().is_empty(), "no search");
    assert_eq!(l.clusters.as_ref().unwrap().len(), 2);
    let text = common::terminal().render_list(&l).unwrap();
    assert!(
        text.contains(
            "\n2 clusters\nthe list holds no virtual cluster: Palette has no search, or its \
             search takes no environment\n"
        ),
        "{text}"
    );
    let md = common::markdown().render_list(&l).unwrap();
    assert!(
        md.contains(
            "The list holds no virtual cluster: Palette has no search, or its search takes no \
             environment.\n"
        ),
        "{md}"
    );
    assert!(!md.contains("| virtual |"), "{md}");
    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_list(&l)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["includesVirtual"], false);

    // no search: Palette answers 404 for the schema
    let none = two_clusters_with_search(&[]).failing(CLUSTER_SEARCH_SCHEMA, 404, "{}");
    let l = clusters_listing(&none, ops::ClusterQuery::default());
    assert_eq!(l.includes_virtual, Some(false));
    assert!(none.posted().is_empty());

    // another failure of the schema is an error, as for a query
    let broken = two_clusters_with_search(&[]).failing(CLUSTER_SEARCH_SCHEMA, 500, "{}");
    let err = listing(
        &broken,
        &ListOptions {
            kinds: vec![ListKind::Clusters],
            ..ListOptions::default()
        },
    )
    .unwrap_err();
    assert!(matches!(err, Error::Api { status: 500, .. }), "{err}");
}

#[test]
fn a_cluster_name_resolves_a_virtual_cluster_through_the_search() {
    // cluster:<name> reads the listing, which omits each virtual cluster, and the search
    // with environment eq nested too (#116)
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = clusters_named(dir.path(), &[("lab-cluster", CLUSTER_UID), ("other", "c2")])
        .with(
            CLUSTER_SEARCH_SCHEMA,
            search_schema(&["clusterName", "environment"]),
        )
        .with_post(
            CLUSTER_SEARCH,
            &virtual_search(vec![]),
            serde_json::json!({"items": [virtual_item("c-v", "vc-1", "p1", ("g1", "dev-group"),
                (CLUSTER_UID, "lab-cluster"))], "listmeta": {"continue": ""}}),
        );
    let palette = Palette::new(&api);
    assert_eq!(palette.cluster_uid_named("vc-1").unwrap(), "c-v");
    assert_eq!(
        palette.cluster_uid_named("lab-cluster").unwrap(),
        CLUSTER_UID
    );
    let err = palette.cluster_uid_named("nope").unwrap_err();
    assert!(matches!(err, Error::ClusterNotFound { .. }), "{err}");
    assert!(
        err.to_string().contains("lab-cluster, other, vc-1"),
        "the names on offer hold the virtual cluster: {err}"
    );
    assert_eq!(api.posted().len(), 3, "one search for each lookup");

    // a virtual cluster and a cluster with one name: the name is ambiguous
    let twins = api.clone().with(
        CLUSTER_LISTING,
        serde_json::json!([listed_cluster("vc-1", "c8", "Running")]),
    );
    let err = Palette::new(twins).cluster_uid_named("vc-1").unwrap_err();
    assert!(matches!(err, Error::ClusterNameAmbiguous { .. }), "{err}");
    assert!(err.to_string().contains("c-v, c8"), "{err}");

    // the search takes no environment: the listing alone, and no search
    let older = clusters_named(dir.path(), &[("lab-cluster", CLUSTER_UID)])
        .with(CLUSTER_SEARCH_SCHEMA, search_schema(&["clusterName"]));
    let err = Palette::new(&older).cluster_uid_named("vc-1").unwrap_err();
    assert!(matches!(err, Error::ClusterNotFound { .. }), "{err}");
    assert!(older.posted().is_empty());
}

/// A cluster document, as `GET /v1/spectroclusters/{uid}` returns it. The applied profiles are in
/// `spec.clusterProfileTemplates`, in the order Palette applies them.
fn cluster_document(uid: &str, applied: &[(&str, &str, &str, &str)]) -> Value {
    let templates: Vec<Value> = applied
        .iter()
        .map(|(name, version, profile_uid, kind)| {
            serde_json::json!({"name": name, "profileVersion": version, "uid": profile_uid, "type": kind})
        })
        .collect();
    serde_json::json!({
        "metadata": {"uid": uid, "annotations": {"projectUid": "p1"}},
        "spec": {"clusterProfileTemplates": templates},
        "status": {"state": "Running"},
    })
}

#[test]
fn list_with_applied_reads_each_cluster_for_the_profiles_it_applies() {
    let api = MemoryApi::new()
        .in_project("p1")
        .with(
            CLUSTER_LISTING,
            serde_json::json!([
                listed_cluster("workload-b", "c2", "Running"),
                listed_cluster("workload-a", "c1", "Running"),
            ]),
        )
        .with(
            cluster_route("c1"),
            cluster_document(
                "c1",
                &[
                    ("lab-profile", "1.3.0", UID_B, "cluster"),
                    ("lab-addons", "2.0.0", UID_ADDONS, "add-on"),
                ],
            ),
        )
        .with(
            cluster_route("c2"),
            cluster_document("c2", &[("lab-profile", "1.2.0", UID_A, "cluster")]),
        )
        // an update of the add-on waits for c1; c2 serves no notifications, so it is unknown
        .with(
            "/v1/notifications/spectrocluster/c1",
            notifications_page(
                vec![notification(
                    "n1",
                    "2026-09-24T23:55:02.083Z",
                    false,
                    &[(UID_ADDONS, "m")],
                )],
                "",
            ),
        );
    let list = |options: &ListOptions| listing(&api, options).unwrap();
    let clusters = ListOptions {
        kinds: vec![ListKind::Clusters],
        ..ListOptions::default()
    };

    // the option is what costs the requests: without it, the listing alone answers
    let plain = list(&clusters);
    assert_eq!(api.calls(&cluster_route("c1")), 0);
    assert!(plain.clusters.as_ref().unwrap()[0].profiles.is_none());

    let l = list(&ListOptions {
        applied: true,
        ..clusters.clone()
    });
    assert_eq!(api.calls(&cluster_route("c1")), 1);
    let listed = l.clusters.as_ref().unwrap();
    assert_eq!(
        listed.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        ["workload-a", "workload-b"],
        "the listing keeps its sort order"
    );
    let applied = listed[0].profiles.as_ref().unwrap();
    assert_eq!(
        applied
            .iter()
            .map(|a| (a.name.as_str(), a.version.as_str(), a.kind.as_str()))
            .collect::<Vec<_>>(),
        [
            ("lab-profile", "1.3.0", "cluster"),
            ("lab-addons", "2.0.0", "add-on")
        ],
        "in the order Palette applies them"
    );
    assert_eq!(applied[0].uid, UID_B);

    let text = common::terminal().render_list(&l).unwrap();
    assert!(
        text.contains(&format!(
            "  cluster:c1  workload-a  Running  aws
    lab-profile@1.3.0  {UID_B}  cluster
    lab-addons@2.0.0   {UID_ADDONS}  add-on  behind: an update waits
  cluster:c2  workload-b  Running  aws
    lab-profile@1.2.0  {UID_A}  cluster
"
        )),
        "{text}"
    );
    let md = common::markdown().render_list(&l).unwrap();
    assert!(
        md.contains(
            "| ref | name | state | cloud | profiles | behind |\n|---|---|---|---|---|---|\n"
        ),
        "{md}"
    );
    assert!(
        md.contains("| `cluster:c1` | workload-a | Running | aws | lab-profile@1.3.0 (cluster)<br>lab-addons@2.0.0 (add-on) | yes: lab-addons |"),
        "{md}"
    );
    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_list(&l)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["clusters"][0]["profiles"][1]["name"], "lab-addons");
}

#[test]
fn list_with_applied_keeps_a_cluster_it_cannot_read_and_reads_each_project_once() {
    let mut elsewhere = listed_cluster("edge-1", "c7", "Running");
    elsewhere["metadata"]["annotations"]["projectUid"] = serde_json::json!("p2");
    let mut gone = listed_cluster("gone", "c9", "Deleting");
    gone["metadata"]["annotations"]["projectUid"] = serde_json::json!("p2");
    let listing = MemoryApi::new().with(
        CLUSTER_LISTING,
        serde_json::json!([
            listed_cluster("workload-a", "c1", "Running"),
            elsewhere,
            gone
        ]),
    );
    let p1 = listing.clone().with(
        cluster_route("c1"),
        cluster_document("c1", &[("lab-profile", "1.3.0", UID_B, "cluster")]),
    );
    let p2 = MemoryApi::new()
        .with(
            cluster_route("c7"),
            cluster_document("c7", &[("lab-profile", "1.2.0", UID_A, "cluster")]),
        )
        .failing(cluster_route("c9"), 404, "no such cluster");
    let scopes: RefCell<Vec<Option<String>>> = RefCell::new(Vec::new());
    let connect = |project: Option<&str>| {
        scopes.borrow_mut().push(project.map(str::to_string));
        Ok(Palette::new(match project {
            Some("p2") => p2.clone(),
            _ => p1.clone(),
        }))
    };
    let options = ListOptions {
        kinds: vec![ListKind::Clusters],
        project: Some(String::new()),
        applied: true,
        ..ListOptions::default()
    };
    let l = ops::list(&options, connect, "t1", &Progress::silent()).unwrap();
    assert_eq!(
        *scopes.borrow(),
        vec![
            Some(String::new()),
            Some("p2".to_string()),
            Some("p1".to_string())
        ],
        "each project gets one client, whatever the scope the listing was read in"
    );
    let listed = l.clusters.as_ref().unwrap();
    let by_name = |name: &str| listed.iter().find(|c| c.name == name).unwrap();
    assert_eq!(
        by_name("edge-1").profiles.as_ref().unwrap()[0].version,
        "1.2.0",
        "a cluster in another project is read with that project's client"
    );
    let failed = by_name("gone");
    assert!(failed.profiles.is_none());
    assert_eq!(failed.error.as_ref().unwrap()["kind"], "api");
    let text = common::terminal().render_list(&l).unwrap();
    assert!(
        text.contains("gone"),
        "a cluster that cannot be read keeps its row: {text}"
    );
    assert!(text.contains("lab-profile@1.3.0"), "{text}");
}

/// A cluster listing entry with the cluster type `kind`: `PureManage` for a cluster that Palette
/// built, `AlloyMonitor` for one that a person imported with no cloud account.
fn typed_cluster(name: &str, uid: &str, kind: &str) -> Value {
    let mut row = listed_cluster(name, uid, "Running");
    row["spec"]["clusterType"] = serde_json::json!(kind);
    row
}

#[test]
fn list_clusters_gives_the_cluster_type_of_each_cluster_as_palette_lists_it() {
    let api = MemoryApi::new().in_project("p1").with(
        CLUSTER_LISTING,
        serde_json::json!([
            typed_cluster("payments", "c1", "PureManage"),
            typed_cluster("legacy", "c3", "AlloyMonitor"),
        ]),
    );
    let l = listing(
        &api,
        &ListOptions {
            kinds: vec![ListKind::Clusters],
            ..ListOptions::default()
        },
    )
    .unwrap();
    let listed = l.clusters.as_ref().unwrap();
    assert_eq!(
        listed
            .iter()
            .map(|c| (c.name.as_str(), c.cluster_type.as_str()))
            .collect::<Vec<_>>(),
        [("legacy", "AlloyMonitor"), ("payments", "PureManage")]
    );
    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_list(&l)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["clusters"][0]["clusterType"], "AlloyMonitor");
    assert_eq!(json["clusters"][1]["clusterType"], "PureManage");
    // the type is in the listing, so it costs no request
    assert_eq!(api.calls(&cluster_route("c1")), 0);
}

#[test]
fn list_with_applied_says_why_an_imported_cluster_has_no_profile_and_a_built_cluster_does_not() {
    let api = MemoryApi::new()
        .in_project("p1")
        .with(
            CLUSTER_LISTING,
            serde_json::json!([
                typed_cluster("legacy", "c3", "AlloyMonitor"),
                typed_cluster("bare", "c4", "PureManage"),
                typed_cluster("shared", "c5", "AlloyAssist"),
                typed_cluster("gone", "c6", "AlloyMonitor"),
            ]),
        )
        // read-only mode: an imported cluster with no profile
        .with(cluster_route("c3"), cluster_document("c3", &[]))
        // a cluster that Palette built, with no profile: the listing cannot say why
        .with(cluster_route("c4"), cluster_document("c4", &[]))
        // full permission mode: an imported cluster with an add-on profile
        .with(
            cluster_route("c5"),
            cluster_document("c5", &[("lab-addons", "2.0.0", UID_ADDONS, "add-on")]),
        )
        .failing(cluster_route("c6"), 404, "no such cluster");
    let l = listing(
        &api,
        &ListOptions {
            kinds: vec![ListKind::Clusters],
            applied: true,
            ..ListOptions::default()
        },
    )
    .unwrap();
    let note = "imported: Palette applies no add-on profile in read-only mode";

    let text = common::terminal().render_list(&l).unwrap();
    let line = |uid: &str| {
        text.lines()
            .find(|l| l.contains(uid))
            .unwrap_or_else(|| panic!("no line of {uid}: {text}"))
    };
    assert_eq!(
        line("cluster:c3"),
        format!("  cluster:c3  legacy  Running  aws  {note}"),
        "{text}"
    );
    assert!(!line("cluster:c4").contains("imported"), "{text}");
    assert!(!line("cluster:c5").contains("imported"), "{text}");
    assert!(text.contains("    lab-addons@2.0.0"), "{text}");
    let gone = line("cluster:c6");
    assert!(
        gone.contains("no such cluster)") && !gone.contains("imported"),
        "{text}"
    );

    let md = common::markdown().render_list(&l).unwrap();
    assert!(
        md.contains(&format!(
            "| `cluster:c3` | legacy | Running | aws | {note} | - |"
        )),
        "{md}"
    );
    assert!(
        md.contains("| `cluster:c4` | bare | Running | aws | - | - |"),
        "{md}"
    );
    assert!(
        md.contains("| `cluster:c5` | shared | Running | aws | lab-addons@2.0.0 (add-on) | - |"),
        "{md}"
    );
    assert!(
        md.contains("| `cluster:c6` | gone | Running | aws | error: GET /v1/spectroclusters/c6 failed with HTTP 404: no such cluster | - |"),
        "{md}"
    );

    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_list(&l)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["clusters"][2]["name"], "legacy");
    assert_eq!(json["clusters"][2]["clusterType"], "AlloyMonitor");
    assert_eq!(json["clusters"][2]["profiles"], serde_json::json!([]));
}

#[test]
fn list_with_a_cluster_text_keeps_the_clusters_that_match_and_reads_only_those() {
    let mut elsewhere = listed_cluster("payments", "c7", "Running");
    elsewhere["metadata"]["annotations"]["projectUid"] = serde_json::json!("p2");
    let api = MemoryApi::new()
        .in_project("p1")
        .with(
            CLUSTER_LISTING,
            serde_json::json!([
                listed_cluster("payments", "c1", "Running"),
                listed_cluster("search", "c2", "Running"),
                elsewhere,
            ]),
        )
        .with(
            cluster_route("c1"),
            cluster_document("c1", &[("lab-profile", "1.3.0", UID_B, "cluster")]),
        )
        .with(
            cluster_route("c2"),
            cluster_document("c2", &[("lab-profile", "1.2.0", UID_A, "cluster")]),
        )
        .with(
            cluster_route("c7"),
            cluster_document("c7", &[("lab-profile", "1.3.0", UID_B, "cluster")]),
        );
    let list = |text: &str| {
        listing(
            &api,
            &ListOptions {
                project: Some(String::new()),
                cluster: Some(text.to_string()),
                ..ListOptions::default()
            },
        )
        .unwrap()
    };

    let l = list("payments");
    assert!(
        l.profiles.is_none(),
        "--cluster lists clusters only, as --pack lists profiles only"
    );
    let clusters = l.clusters.as_ref().unwrap();
    assert_eq!(
        clusters
            .iter()
            .map(|c| (c.uid.as_str(), c.project_uid.as_str()))
            .collect::<Vec<_>>(),
        [("c1", "p1"), ("c7", "p2")],
        "one name can be a cluster of two projects, and both are the answer"
    );
    assert_eq!(
        clusters[0].profiles.as_ref().unwrap()[0].version,
        "1.3.0",
        "--cluster reads the profiles, as --pack reads the packs"
    );
    assert_eq!(
        api.calls(&cluster_route("c2")),
        0,
        "a cluster the text does not match is never read"
    );
    assert_eq!(l.cluster.as_deref(), Some("payments"));

    // the uid, and the case of the text, are the same filter
    assert_eq!(
        list("C7").clusters.unwrap().len(),
        1,
        "by uid, ignoring case"
    );
    assert_eq!(list("PAY").clusters.unwrap().len(), 2, "ignoring case");
    assert!(list("ledger").clusters.unwrap().is_empty());

    let text = common::terminal().render_list(&list("ledger")).unwrap();
    assert!(text.contains("no clusters matching 'ledger'"), "{text}");
    let text = common::terminal().render_list(&l).unwrap();
    assert!(text.contains("2 clusters matching 'payments'"), "{text}");
    let md = common::markdown().render_list(&l).unwrap();
    assert!(md.contains("**2 clusters matching 'payments'**"), "{md}");
    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_list(&l)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        json["cluster"], "payments",
        "the document says what it kept"
    );
}

#[test]
fn list_fails_when_a_listing_cannot_be_read() {
    let api = MemoryApi::new().failing(PROFILE_LISTING, 401, "no key");
    let err = listing(&api, &ListOptions::default()).unwrap_err();
    assert_eq!(err.kind(), "api", "{err}");
}

// ---- candidates ------------------------------------------------------------------------------

fn never_connect() -> strata::Result<Palette<MemoryApi>> {
    panic!("local errors are found before connecting")
}

/// The examples that `just examples` builds, built into a temporary directory from the fetched
/// pack data the repository tracks, with this build of strata.
fn built_examples() -> tempfile::TempDir {
    let (dir, o) = fetch_examples(None, &[]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    dir
}

#[test]
fn a_candidate_built_from_real_registry_responses_has_the_root_of_the_published_profile() {
    let built = built_examples();
    let examples = built.path();
    let dir = tempfile::tempdir().unwrap();
    for version in ["1.0.0", "1.1.0", "1.1.1"] {
        let raw = examples
            .join("profiles/raw")
            .join(format!("aws-stack@{version}"));
        let profile = read_json(&raw.join("profile.json"));
        let manifests = read_json(&raw.join("manifests.json"));
        let registry = profile["spec"]["published"]["packs"][0]["registryUid"]
            .as_str()
            .unwrap()
            .to_string();

        let mut api = MemoryApi::new().with(
            REGISTRY_LISTING,
            serde_json::json!({"items": [{"name": "Public Repo", "uid": registry, "kind": "pack"}]}),
        );
        let mut layers = Vec::new();
        for pack in profile["spec"]["published"]["packs"].as_array().unwrap() {
            let name = pack["name"].as_str().unwrap();
            let d = examples.join("packs").join(name);
            api = serve_registry_pack(
                api,
                name,
                read_json(&d.join("_summaries.json")),
                read_json(&d.join("_entity.json")),
                &registry,
            );
            // only the version is given; layer, tag, packUid, digest, and default values come
            // from the registry, and the manifest the example attaches is carried inline
            let mut layer = serde_json::json!({"name": name, "version": pack["version"]});
            let attached: Vec<Value> = manifests["spec"]["packs"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|m| m["spec"]["name"] == name)
                .flat_map(|m| {
                    m["spec"]["manifests"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                })
                .map(|m| serde_json::json!({"name": m["name"], "content": m["content"]}))
                .collect();
            if !attached.is_empty() {
                layer["manifests"] = Value::Array(attached);
            }
            layers.push(layer);
        }
        let candidate = serde_json::json!({
            "name": "aws-stack", "version": version, "cloudType": "aws", "registry": "Public Repo",
            "layers": layers,
            "variables": read_json(&raw.join("variables.json"))["variables"],
        });
        let file = dir.path().join(format!("aws-stack@{version}.yaml"));
        write_yaml(&file, &candidate);

        let snap = candidate_snapshot(&file, &api).unwrap();
        let published = read_json(
            &examples
                .join("profiles")
                .join(format!("aws-stack@{version}.json")),
        );
        assert_eq!(
            snap.root,
            published["root"].as_str().unwrap(),
            "aws-stack@{version}: the candidate is the release that gets published"
        );
        assert_eq!(snap.source.template, Template::Candidate);
        assert_eq!(snap.source.uid, "");
        assert_eq!(
            (snap.source.name.as_str(), snap.source.version.as_str()),
            ("aws-stack", version)
        );
        assert_eq!(snap.source.reference, file.display().to_string());
    }
}

/// Registry responses for every pack of fixture `b`, so a candidate that spells out b's values
/// and manifests resolves to b's packs.
fn serve_registry_for_b(dir: &Path) -> MemoryApi {
    let profile = read_json(&dir.join("b").join("profile.json"));
    let manifests = read_json(&dir.join("b").join("manifests.json"));
    let mut api = MemoryApi::new();
    for pack in profile["spec"]["published"]["packs"].as_array().unwrap() {
        let name = pack["name"].as_str().unwrap();
        let entry = manifests["spec"]["packs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["spec"]["name"] == name)
            .unwrap();
        let summary = PackFixture::public(
            name,
            pack["layer"].as_str().unwrap(),
            pack["version"].as_str().unwrap(),
            pack["packUid"].as_str().unwrap(),
            entry["spec"]["digest"].as_str().unwrap_or_default(),
            "registry: defaults",
        )
        .summary(&[]);
        let entity = serde_json::json!({"tags": [
            {"tag": "floating.x", "packUid": pack["packUid"]},
            {"tag": pack["tag"], "packUid": pack["packUid"]},
        ]});
        api = serve_registry_pack(
            api,
            name,
            serde_json::json!([summary]),
            entity,
            "reg-public",
        );
    }
    api
}

#[test]
fn a_candidate_carries_values_and_manifests_inline_or_from_files_next_to_it() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded_b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    let profile = read_json(&dir.path().join("b").join("profile.json"));
    let manifests = read_json(&dir.path().join("b").join("manifests.json"));

    let cand = dir.path().join("candidate");
    fs::create_dir_all(cand.join("values")).unwrap();
    fs::create_dir_all(cand.join("manifests")).unwrap();
    let mut layers = Vec::new();
    for pack in profile["spec"]["published"]["packs"].as_array().unwrap() {
        let name = pack["name"].as_str().unwrap();
        let mut layer =
            serde_json::json!({"name": name, "version": pack["version"], "layer": pack["layer"]});
        if name == "kubernetes" {
            fs::write(
                cand.join("values/kubernetes.yaml"),
                pack["values"].as_str().unwrap(),
            )
            .unwrap();
            layer["valuesFile"] = serde_json::json!("values/kubernetes.yaml");
        } else {
            layer["values"] = pack["values"].clone();
        }
        let entry = manifests["spec"]["packs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["spec"]["name"] == name)
            .unwrap();
        let mut ms = Vec::new();
        for m in entry["spec"]["manifests"].as_array().unwrap() {
            let mname = m["name"].as_str().unwrap();
            let file = format!("manifests/{mname}.yaml");
            fs::write(cand.join(&file), m["content"].as_str().unwrap()).unwrap();
            ms.push(serde_json::json!({"name": mname, "contentFile": file}));
        }
        if !ms.is_empty() {
            layer["manifests"] = Value::Array(ms);
        }
        layers.push(layer);
    }
    let file = cand.join("profile.yaml");
    write_yaml(
        &file,
        &serde_json::json!({
            "name": "lab-profile", "version": "1.3.0", "type": "cluster", "cloudType": "aws",
            "registryUid": "reg-public", "layers": layers,
            "variables": read_json(&dir.path().join("b").join("variables.json"))["variables"],
        }),
    );
    let api = serve_registry_for_b(dir.path());
    let snap = candidate_snapshot(&file, &api).unwrap();
    assert_eq!(snap.root, recorded_b.root);
    assert!(ops::compare(&recorded_b, &snap, "t1").same);

    // Palette does not hold a candidate, so nothing asked which clusters use it. The field is
    // absent, and the JSON a team commits carries no count that nothing counted.
    assert!(
        snap.source.consumers.is_none(),
        "a candidate carries no consumer count"
    );
    let json = serde_json::to_value(&snap).unwrap();
    assert!(
        json["source"].get("consumers").is_none(),
        "and the serialized record has no consumers key: {}",
        json["source"]
    );

    // a layer that omits values gets the registry defaults, which differ from b's
    let mut doc: Value = serde_yaml::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    doc["layers"][4].as_object_mut().unwrap().remove("values");
    let defaults = cand.join("defaults.yaml");
    write_yaml(&defaults, &doc);
    let snap = candidate_snapshot(&defaults, &api).unwrap();
    let d = ops::compare(&recorded_b, &snap, "t1");
    let changed: Vec<&str> = d
        .packs
        .iter()
        .filter(|p| p.authored)
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(changed, vec!["prometheus-operator"]);
}

#[test]
fn candidate_errors_name_what_is_wrong() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_registry_for_b(dir.path());
    let cand = dir.path().join("errors");
    fs::create_dir_all(&cand).unwrap();
    let write = |name: &str, doc: Value| -> std::path::PathBuf {
        let p = cand.join(name);
        write_yaml(&p, &doc);
        p
    };
    // Palette refuses a cluster profile with no os, cni, or csi layer, so each candidate has the
    // other core layers of b after kubernetes
    let b = read_json(&dir.path().join("b").join("profile.json"));
    let core: Vec<Value> = b["spec"]["published"]["packs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| ["os", "cni", "csi"].contains(&p["layer"].as_str().unwrap()))
        .map(|p| serde_json::json!({"name": p["name"], "version": p["version"]}))
        .collect();
    let k8s = |extra: Value| {
        let mut l = serde_json::json!({"name": "kubernetes", "version": "1.30.2"});
        for (k, v) in extra.as_object().unwrap() {
            l[k] = v.clone();
        }
        let layers: Vec<Value> = std::iter::once(l).chain(core.iter().cloned()).collect();
        serde_json::json!({"name": "p", "version": "1", "registryUid": "reg-public", "layers": layers})
    };

    // local problems are found before connecting
    let f = write(
        "missing-values.yaml",
        k8s(serde_json::json!({"valuesFile": "values/nope.yaml"})),
    );
    let err = ops::snapshot_from_candidate(&f, false, never_connect, "t0", &Progress::silent())
        .unwrap_err();
    assert!(
        matches!(&err, Error::Io { path, .. } if path.ends_with("values/nope.yaml")),
        "{err}"
    );
    assert_eq!(err.exit_code(), 2);

    let f = write(
        "both.yaml",
        k8s(serde_json::json!({"values": "a: 1", "valuesFile": "v.yaml"})),
    );
    let err = ops::snapshot_from_candidate(&f, false, never_connect, "t0", &Progress::silent())
        .unwrap_err();
    assert!(matches!(err, Error::CandidateInvalid { .. }), "{err}");
    assert!(err.to_string().contains("kubernetes"), "{err}");

    let f = write(
        "typo.yaml",
        k8s(serde_json::json!({"valuesfile": "v.yaml"})),
    );
    let err = ops::snapshot_from_candidate(&f, false, never_connect, "t0", &Progress::silent())
        .unwrap_err();
    assert!(matches!(err, Error::CandidateInvalid { .. }), "{err}");
    assert!(err.to_string().contains("valuesfile"), "{err}");

    let f = write(
        "no-registry.yaml",
        serde_json::json!({"name": "p", "version": "1", "layers": [{"name": "kubernetes", "version": "1.30.2"}]}),
    );
    let err = ops::snapshot_from_candidate(&f, false, never_connect, "t0", &Progress::silent())
        .unwrap_err();
    assert!(matches!(err, Error::CandidateInvalid { .. }), "{err}");

    // registry problems need the registry. A version that the listing does not have asks whether
    // Palette syncs the registry, and the registry listing is the first read of that check. An
    // answer of Palette that is an error is an error of the record (#327).
    let f = write(
        "unknown-version.yaml",
        k8s(serde_json::json!({"version": "9.9.9"})),
    );
    let err = candidate_snapshot(&f, &api).unwrap_err();
    assert!(matches!(err, Error::Api { status: 404, .. }), "{err}");
    let api = api.with(
        REGISTRY_LISTING,
        serde_json::json!({"items": [{"name": "Public Repo", "uid": "reg-public", "kind": "pack"}]}),
    );
    let err = candidate_snapshot(&f, &api).unwrap_err();
    match &err {
        Error::PackNotFound {
            name,
            version,
            registry,
            ..
        } => {
            assert_eq!(
                (name.as_str(), version.as_str(), registry.as_str()),
                ("kubernetes", "9.9.9", "reg-public")
            );
        }
        other => panic!("unexpected {other}"),
    }
    assert_eq!(err.kind(), "pack-not-found");

    let f = write("unknown-tag.yaml", k8s(serde_json::json!({"tag": "2.x"})));
    let err = candidate_snapshot(&f, &api).unwrap_err();
    assert!(matches!(err, Error::PackNotFound { .. }), "{err}");
    assert!(err.to_string().contains("2.x"), "{err}");

    let named = api.clone().with(
        REGISTRY_LISTING,
        serde_json::json!({"items": [{"name": "Public Repo", "uid": "reg-public"}, {"name": "Internal", "uid": "reg-int"}]}),
    );
    let f = write(
        "unknown-registry.yaml",
        serde_json::json!({"name": "p", "version": "1", "registry": "Nope", "layers": [{"name": "kubernetes", "version": "1.30.2"}]}),
    );
    let err = candidate_snapshot(&f, &named).unwrap_err();
    assert!(matches!(err, Error::RegistryNotFound { .. }), "{err}");
    assert!(err.to_string().contains("Public Repo, Internal"), "{err}");

    // an explicit tag is used when the pack version has it
    let f = write(
        "floating.yaml",
        k8s(serde_json::json!({"tag": "floating.x"})),
    );
    let snap = candidate_snapshot(&f, &api).unwrap();
    assert_eq!(snap.packs[0].tag, "floating.x");
}

#[test]
fn the_registry_field_of_a_candidate_takes_a_name_or_a_uid() {
    // `registry` names the registry as the listing names it, by name or by uid (#327): the three
    // candidates give one record
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_registry_for_b(dir.path()).with(
        REGISTRY_LISTING,
        serde_json::json!({"items": [{"name": "Public Repo", "uid": "reg-public", "kind": "pack"}]}),
    );
    let b = read_json(&dir.path().join("b").join("profile.json"));
    let layers: Vec<Value> = b["spec"]["published"]["packs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| serde_json::json!({"name": p["name"], "version": p["version"]}))
        .collect();
    let root_with = |field: &str, value: &str| {
        let mut doc = serde_json::json!({"name": "p", "version": "1", "layers": layers.clone()});
        doc[field] = Value::String(value.to_string());
        let f = dir.path().join(format!("{field}.yaml"));
        write_yaml(&f, &doc);
        candidate_snapshot(&f, &api).unwrap().root
    };
    let by_uid = root_with("registryUid", "reg-public");
    assert_eq!(root_with("registry", "Public Repo"), by_uid);
    assert_eq!(root_with("registry", "reg-public"), by_uid);
}

#[test]
fn an_explicit_tag_equal_to_the_version_is_accepted_as_palette_does() {
    // Palette stores the tag a profile was authored with. A profile built with Terraform pins the
    // version itself, `24.04`, although the registry names that version `LTS__24.x`; the live
    // example run of 2026-09-16 recorded exactly that.
    let ubuntu = PackFixture::new(
        "ubuntu-aws",
        "os",
        "24.04",
        "pk-ubuntu-2404",
        "reg",
        "sha256:u",
        "",
    );
    let summary = serde_json::json!([ubuntu.summary(&[])]);
    let entity = serde_json::json!({"tags": [
        {"tag": "LTS__24.x", "packUid": "pk-ubuntu-2404"},
        {"tag": "LTS__24.04", "packUid": "pk-ubuntu-2404"},
    ]});
    let mut api = serve_registry_pack(MemoryApi::new(), "ubuntu-aws", summary, entity, "reg");
    // Palette refuses a cluster profile with no k8s, cni, or csi layer: serve one of each
    let core = [
        ("kubernetes", "k8s"),
        ("cni-cilium-oss", "cni"),
        ("csi-aws-ebs", "csi"),
    ];
    for (name, layer) in core {
        let uid = format!("pk-{layer}");
        api = serve_registry_pack(
            api,
            name,
            serde_json::json!([
                PackFixture::new(name, layer, "1.0.0", &uid, "reg", "", "").summary(&[])
            ]),
            serde_json::json!({"tags": [{"tag": "1.0.0", "packUid": uid}]}),
            "reg",
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let candidate = |name: &str, tag: Option<&str>| {
        let mut layer = serde_json::json!({"name": "ubuntu-aws", "version": "24.04"});
        if let Some(t) = tag {
            layer["tag"] = serde_json::json!(t);
        }
        let layers: Vec<Value> = std::iter::once(layer)
            .chain(
                core.iter()
                    .map(|(name, _)| serde_json::json!({"name": name, "version": "1.0.0"})),
            )
            .collect();
        let file = dir.path().join(name);
        write_yaml(
            &file,
            &serde_json::json!({"name": "p", "version": "1", "registryUid": "reg", "layers": layers}),
        );
        candidate_snapshot(&file, &api)
    };

    assert_eq!(
        candidate("version.yaml", Some("24.04")).unwrap().packs[0].tag,
        "24.04"
    );
    assert_eq!(
        candidate("registry.yaml", Some("LTS__24.04"))
            .unwrap()
            .packs[0]
            .tag,
        "LTS__24.04"
    );
    assert_eq!(
        candidate("default.yaml", None).unwrap().packs[0].tag,
        "LTS__24.x",
        "without a tag, the registry's first tag for the version"
    );
    let err = candidate("typo.yaml", Some("LTS__99")).unwrap_err();
    assert!(matches!(err, Error::PackNotFound { .. }), "{err}");
}

#[test]
fn a_record_from_another_snapshot_schema_is_refused_by_name_and_extra_fields_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let mut doc: Value = serde_json::from_str(&recorded.to_json(true).unwrap()).unwrap();

    // a field this strata does not know: additive changes keep the schema id and still load
    doc["source"]["addedLater"] = serde_json::json!("x");
    let extra = dir.path().join("extra.json");
    write_record(&extra, &doc);
    assert_eq!(Snapshot::load(&extra).unwrap().root, recorded.root);

    // a newer schema id is refused, naming both ids
    doc["schema"] = serde_json::json!("strata-snapshot/2");
    let newer = dir.path().join("newer.json");
    write_record(&newer, &doc);
    let err = Snapshot::load(&newer).unwrap_err();
    match &err {
        Error::UnsupportedSchema {
            found, supported, ..
        } => {
            assert_eq!(
                (found.as_str(), *supported),
                ("strata-snapshot/2", "strata-snapshot/1")
            );
        }
        other => panic!("unexpected {other}"),
    }
    assert_eq!(err.kind(), "unsupported-schema");
    assert!(
        err.to_string().contains("strata-snapshot/2")
            && err.to_string().contains("strata-snapshot/1"),
        "{err}"
    );

    // a file that is not a strata document at all stays not-snapshot
    let other = dir.path().join("other.json");
    fs::write(&other, r#"{"schema": "something-else/1"}"#).unwrap();
    assert!(matches!(
        Snapshot::load(&other).unwrap_err(),
        Error::NotSnapshot(_)
    ));
}

// ---- names for projects and clusters -----------------------------------------------------------

#[test]
fn a_project_name_resolves_to_its_uid_and_an_unknown_name_lists_the_projects() {
    let tenant = Palette::new(
        MemoryApi::new().with(PROJECT_LISTING, projects(&[("prod", "p2"), ("lab", "p1")])),
    );
    assert_eq!(tenant.project_uid_named("prod").unwrap(), "p2");

    let err = tenant.project_uid_named("staging").unwrap_err();
    assert!(matches!(err, Error::ProjectNotFound { .. }), "{err}");
    assert_eq!((err.kind(), err.exit_code()), ("project-not-found", 2));
    assert!(
        err.to_string().contains("'staging'") && err.to_string().contains("lab, prod"),
        "names the visible projects in order: {err}"
    );
    assert_eq!(
        err.to_json()["available"],
        serde_json::json!(["lab", "prod"])
    );
}

/// Cluster fixture `cluster-a` under [`CLUSTER_UID`], and a scope listing of `clusters`.
fn clusters_named(raw_root: &Path, clusters: &[(&str, &str)]) -> MemoryApi {
    let listing: Vec<Value> = clusters
        .iter()
        .map(|(name, uid)| listed_cluster(name, uid, "Running"))
        .collect();
    serve_cluster(MemoryApi::new(), raw_root, "cluster-a")
        .with(CLUSTER_LISTING, Value::Array(listing))
        .in_project("p1")
}

#[test]
fn a_cluster_ref_takes_a_uid_or_a_name() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = clusters_named(dir.path(), &[("lab-cluster", CLUSTER_UID), ("other", "c2")]);
    let by_uid = snapshot_cluster(&api, None).unwrap();
    assert_eq!(api.calls(CLUSTER_LISTING), 0, "a uid needs no listing");
    let by_name = snapshot_of(&api, "cluster:lab-cluster").unwrap();
    assert_eq!(
        by_name.source.cluster.as_ref().unwrap().uid,
        CLUSTER_UID,
        "the record carries the uid"
    );
    assert_eq!(by_name.root, by_uid.root);

    let err = snapshot_of(&api, "cluster:nope").unwrap_err();
    assert!(matches!(err, Error::ClusterNotFound { .. }), "{err}");
    assert_eq!((err.kind(), err.exit_code()), ("cluster-not-found", 2));
    assert!(
        err.to_string().contains("'nope'") && err.to_string().contains("lab-cluster, other"),
        "{err}"
    );
}

#[test]
fn a_cluster_name_that_matches_several_clusters_is_an_error_and_a_uid_wins_over_a_name() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let twins = clusters_named(dir.path(), &[("twin", "c7"), ("twin", "c8")]);
    let err = snapshot_of(&twins, "cluster:twin").unwrap_err();
    assert!(matches!(err, Error::ClusterNameAmbiguous { .. }), "{err}");
    assert_eq!(err.kind(), "cluster-name-ambiguous");
    assert!(
        err.to_string().contains("c7, c8"),
        "lists the candidates: {err}"
    );

    // another cluster is named after this cluster's uid; the uid is fetched first
    let api = clusters_named(
        dir.path(),
        &[("lab-cluster", CLUSTER_UID), (CLUSTER_UID, "c9")],
    );
    let snap = snapshot_cluster(&api, None).unwrap();
    assert_eq!(snap.source.cluster.unwrap().uid, CLUSTER_UID);
    assert_eq!(api.calls(CLUSTER_LISTING), 0);
}

#[test]
fn a_cluster_record_taken_by_name_verifies_by_uid_after_a_rename() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = clusters_named(dir.path(), &[("lab-cluster", CLUSTER_UID)]);
    let recorded = snapshot_of(&api, "cluster:lab-cluster").unwrap();

    let mut renamed_doc = read_json(&dir.path().join("cluster-a/cluster.json"));
    renamed_doc["metadata"]["name"] = Value::from("lab-cluster-renamed");
    let renamed = api
        .clone()
        .with(cluster_route(CLUSTER_UID), renamed_doc)
        .with(
            CLUSTER_LISTING,
            Value::Array(vec![listed_cluster(
                "lab-cluster-renamed",
                CLUSTER_UID,
                "Running",
            )]),
        );
    let v = ops::verify(&recorded, &Palette::new(renamed), "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged, "the old name is gone, the uid is not");
}

// ---- cluster variable values -------------------------------------------------------------------

fn cluster_snapshot(raw_root: &Path, dir: &str) -> Snapshot {
    ops::snapshot_from_dir(&raw_root.join(dir), &format!("cluster:{CLUSTER_UID}"), "t0").unwrap()
}

#[test]
fn a_cluster_snapshot_records_variable_values_outside_the_root_and_never_a_secret() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let dev = cluster_snapshot(dir.path(), "cluster-a");
    let cluster = dev.source.cluster.as_ref().unwrap();
    let values: Vec<(&str, &str, Option<&str>, bool)> = cluster
        .variables
        .iter()
        .map(|v| {
            (
                v.profile.as_str(),
                v.name.as_str(),
                v.value.as_deref(),
                v.sensitive,
            )
        })
        .collect();
    assert_eq!(
        values,
        vec![
            ("lab-profile", "cluster_env", Some("dev"), false),
            ("lab-profile", "grafana_password", None, true),
        ]
    );
    assert_eq!(cluster.variables[0].profile_uid, UID_A);

    let json = dev.to_json(true).unwrap();
    assert!(
        !json.contains(SECRET_VALUE),
        "a sensitive value is in no document"
    );
    let staging = cluster_snapshot(dir.path(), "cluster-a-staging");
    assert_eq!(staging.root, dev.root, "values are outside the root");

    let path = dir.path().join("dev.json");
    fs::write(&path, json).unwrap();
    assert_eq!(
        Snapshot::load(&path)
            .unwrap()
            .source
            .cluster
            .unwrap()
            .variables,
        dev.source.cluster.unwrap().variables
    );
}

#[test]
fn two_clusters_on_one_release_with_different_values_compare_identical_and_show_the_values() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let dev = cluster_snapshot(dir.path(), "cluster-a");
    let staging = cluster_snapshot(dir.path(), "cluster-a-staging");
    let d = ops::compare(&dev, &staging, "t1");
    assert!(d.same, "one release");
    let values = d
        .cluster_variables
        .as_ref()
        .expect("both sides are clusters");
    assert!(values.changed);
    assert_eq!(values.values.len(), 1, "{:?}", values.values);
    let c = &values.values[0];
    assert_eq!(
        (c.profile.as_str(), c.name.as_str(), c.op),
        ("lab-profile", "cluster_env", strata::diff::Op::Changed)
    );
    assert_eq!(
        (c.old.as_deref(), c.new.as_deref(), c.sensitive),
        (Some("dev"), Some("staging"), false)
    );

    let text = common::terminal()
        .render(&d, &dev, &staging, false)
        .unwrap();
    assert!(
        text.contains(r#"~ lab-profile.cluster_env: "dev" -> "staging""#),
        "{text}"
    );
    let md = common::markdown()
        .render(&d, &dev, &staging, false)
        .unwrap();
    assert!(
        md.contains("- lab-profile.cluster_env: \"dev\"\n+ lab-profile.cluster_env: \"staging\"\n"),
        "the markdown diff style: {md}"
    );
    let json = render::renderer(Format::Json, Paint::off(), true)
        .render(&d, &dev, &staging, false)
        .unwrap();
    assert!(json.contains("\"clusterVariables\""));
    assert!(!json.contains(SECRET_VALUE));

    let same = ops::compare(&dev, &dev, "t1");
    assert!(!same.cluster_variables.unwrap().changed);
    let profile = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    assert!(
        ops::compare(&profile, &dev, "t1")
            .cluster_variables
            .is_none(),
        "a profile has no cluster values to compare"
    );
}

#[test]
fn the_unified_text_of_two_clusters_holds_no_value_of_a_cluster_variable() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    // the two clusters run two releases, so their packs have text to compare. The value `staging`
    // of the variable `cluster_env` is in clear on side A
    let a = cluster_snapshot(dir.path(), "cluster-a-staging");
    let b = cluster_snapshot(dir.path(), "cluster-b");
    let d = ops::compare(&a, &b, "t1");
    let json = render::renderer(Format::Json, Paint::off(), true)
        .render(&d, &a, &b, true)
        .unwrap();
    let doc: Value = serde_json::from_str(&json).unwrap();
    assert!(
        doc["a"]["cluster"]["variables"]
            .to_string()
            .contains("staging"),
        "side A holds the clear value, so this test proves something"
    );
    let mut texts = Vec::new();
    for pack in doc["packs"].as_array().unwrap() {
        texts.extend(pack["values"]["unified"].as_str());
        for m in pack["manifests"].as_array().unwrap() {
            texts.extend(m["unified"].as_str());
        }
    }
    assert!(
        texts.len() >= 3,
        "the values and the manifests have text: {texts:?}"
    );
    for text in texts {
        assert!(!text.contains("staging"), "{text}");
    }
}

#[test]
fn verify_names_a_cluster_value_changed_since_the_record_without_calling_it_drift() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-a"),
        None,
    )
    .unwrap();
    let live = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a-staging");

    let v = ops::verify(
        &recorded,
        &Palette::new(live.clone()),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(v.unchanged, "a value is not part of the release");
    assert!(v.diff.cluster_variables.as_ref().unwrap().changed);
    let renderer = common::terminal();
    let verdict = renderer.verdict(&v.diff).unwrap();
    assert!(
        verdict.starts_with("OK:") && verdict.contains("1 variable value changed"),
        "{verdict}"
    );

    let s = common::verify_summary(&recorded, &live);
    assert_eq!(
        (s.records[0].status, s.exit_code()),
        (RecordStatus::Unchanged, 0)
    );
    let named = s.records[0]
        .cluster_variables
        .as_ref()
        .expect("the summary names the changed value");
    assert_eq!(named.values[0].name, "cluster_env");
    let lines = renderer.render_verify_summary(&s).unwrap();
    assert!(lines.contains("1 variable value changed"), "{lines}");
    assert!(!serde_json::to_string(&s).unwrap().contains(SECRET_VALUE));
}

// ---- candidate files written from records ------------------------------------------------------

use strata::candidate::{self, Candidate};

#[test]
fn a_candidate_written_from_a_record_reproduces_its_root_inline_or_with_files() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded_b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    let c = Candidate::from_snapshot(&recorded_b).unwrap();
    assert!(
        c.layers
            .iter()
            .all(|l| l.tag.is_some() && l.registry_uid.is_some()),
        "every layer pins its tag and registry"
    );

    let inline = dir.path().join("inline.yaml");
    candidate::write(&c, &inline, None).unwrap();
    let repo = dir.path().join("repo");
    let with_files = repo.join("profile.yaml");
    candidate::write(&c, &with_files, Some(&repo.join("files"))).unwrap();

    let doc: Value = serde_yaml::from_str(&fs::read_to_string(&with_files).unwrap()).unwrap();
    for layer in doc["layers"].as_array().unwrap() {
        let values_file = layer["valuesFile"].as_str().expect("values go to a file");
        assert!(layer.get("values").is_none());
        assert!(
            values_file.starts_with("files/") && repo.join(values_file).is_file(),
            "{values_file}"
        );
        for m in layer["manifests"].as_array().into_iter().flatten() {
            let content_file = m["contentFile"].as_str().expect("manifests go to files");
            assert!(repo.join(content_file).is_file(), "{content_file}");
        }
    }

    let api = serve_registry_for_b(dir.path());
    for file in [&inline, &with_files] {
        let snap = ops::snapshot_from_candidate(
            file,
            false,
            || Ok(Palette::new(api.clone())),
            "t1",
            &Progress::silent(),
        )
        .unwrap();
        assert_eq!(snap.root, recorded_b.root, "{}", file.display());
    }

    let on_cluster = ops::snapshot_from_dir(
        &dir.path().join("cluster-b"),
        &format!("cluster:{CLUSTER_UID}"),
        "t0",
    )
    .unwrap();
    assert!(matches!(
        Candidate::from_snapshot(&on_cluster).unwrap_err(),
        Error::ProfileRefRequired { .. }
    ));
}

#[test]
fn a_candidate_of_a_manifest_pack_is_the_published_version_but_for_the_minted_uid() {
    // an add-on profile with one manifest pack, as Palette returns it: no registry, no digest,
    // and the pack uid that Palette minted at the create
    let dir = tempfile::tempdir().unwrap();
    let raw = dir.path().join("addons");
    fs::create_dir_all(&raw).unwrap();
    let content = "apiVersion: v1\nkind: Namespace\nmetadata:\n  name: payments";
    for (name, doc) in [
        (
            "profile.json",
            serde_json::json!({"metadata": {"name": "platform-addons", "uid": "p1"},
                "spec": {"version": "1.0.0", "published": {"cloudType": "all", "type": "add-on",
                    "packs": [{"name": "namespace-policy", "layer": "addon", "type": "manifest",
                        "tag": "1.0.0", "version": "1.0.0", "packUid": "pk-minted-1",
                        "manifests": [{"name": "namespace-policy"}]}]}}}),
        ),
        (
            "manifests.json",
            serde_json::json!({"metadata": {"name": "platform-addons", "uid": "p1"},
                "spec": {"packs": [{"metadata": {"name": "namespace-policy", "uid": "pk-minted-1"},
                    "spec": {"name": "namespace-policy", "layer": "addon", "version": "1.0.0",
                        "manifests": [{"name": "namespace-policy", "content": content}]}}]}}),
        ),
        ("variables.json", no_variables()),
    ] {
        fs::write(raw.join(name), doc.to_string()).unwrap();
    }
    let published = ops::snapshot_from_dir(&raw, "platform-addons@1.0.0", "t0").unwrap();

    let c = Candidate::from_snapshot(&published).unwrap();
    let file = dir.path().join("c.yaml");
    candidate::write(&c, &file, None).unwrap();
    let doc: Value = serde_yaml::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    let layer = &doc["layers"][0];
    assert_eq!(layer["type"], "manifest", "{doc}");
    assert!(layer.get("registryUid").is_none() && layer.get("registry").is_none());

    // the API knows no path: a manifest layer asks the registry nothing
    let snap = ops::snapshot_from_candidate(
        &file,
        false,
        || Ok(Palette::new(MemoryApi::new())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_ne!(snap.root, published.root, "the candidate has no minted uid");

    let chain = ops::same_release(
        &[("c.json".into(), snap), ("dev.json".into(), published)],
        Policy::Exact,
        "t1",
    );
    assert!(chain.same, "{:?}", chain.pairs[0].reasons);
    assert_eq!(chain.pairs[0].status, PairStatus::SameContent);
    assert_eq!(chain.pairs[0].minted_uid_only, ["addon/namespace-policy"]);
}

/// A candidate file of one manifest layer, which asks Palette nothing. `layer` and `variables` are
/// lines of YAML that go after the name of the layer and after `variables:`.
fn write_manifest_candidate(dir: &Path, layer: &str, variables: &str) -> std::path::PathBuf {
    let file = dir.join("c.yaml");
    fs::write(
        &file,
        format!(
            "name: platform-addons\nversion: 1.0.0\ntype: add-on\ncloudType: all\n\
             layers:\n- name: namespace-policy\n  version: 1.0.0\n  type: manifest\n{layer}  \
             manifests:\n  - name: namespace-policy\n    content: 'kind: Namespace'\n\
             variables:\n{variables}"
        ),
    )
    .unwrap();
    file
}

#[test]
fn a_number_in_the_default_of_a_candidate_variable_is_in_the_record_as_text_and_in_the_root() {
    let dir = tempfile::tempdir().unwrap();
    let record = |default: &str| {
        let file = write_manifest_candidate(
            dir.path(),
            "",
            &format!("- name: port\n  defaultValue: {default}\n"),
        );
        candidate_snapshot(&file, &MemoryApi::new()).unwrap()
    };
    let (number, text, empty) = (record("8080"), record("'8080'"), record("''"));
    assert_eq!(number.variables[0].default_value, "8080");
    assert_eq!(
        number.root, text.root,
        "the root is that of the text Palette holds"
    );
    assert_ne!(number.root, empty.root, "0.24.0 recorded empty text");
}

#[test]
fn a_null_in_a_candidate_variable_is_in_the_record_as_the_default_of_the_field() {
    let dir = tempfile::tempdir().unwrap();
    let record = |variables: &str| {
        let file = write_manifest_candidate(dir.path(), "", variables);
        candidate_snapshot(&file, &MemoryApi::new()).unwrap()
    };
    let plain = record("- name: port\n");
    for field in [
        "description: null",
        "description: ~",
        "format: 3",
        "required: ~",
    ] {
        let given = record(&format!("- name: port\n  {field}\n"));
        assert_eq!(given.variables, plain.variables, "{field}");
        assert_eq!(given.root, plain.root, "{field}");
    }
}

#[test]
fn the_record_of_a_candidate_holds_text_params_and_a_param_that_is_not_text_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let file =
        write_manifest_candidate(dir.path(), "  params:\n    replicas: '3'\n", "- name: a\n");
    let snap = candidate_snapshot(&file, &MemoryApi::new()).unwrap();
    assert_eq!(snap.packs[0].params, serde_json::json!({"replicas": "3"}));

    let file = write_manifest_candidate(dir.path(), "  params:\n    replicas: 3\n", "- name: a\n");
    let err = ops::snapshot_from_candidate(&file, false, never_connect, "t0", &Progress::silent())
        .unwrap_err();
    assert_eq!(err.kind(), "candidate-invalid");
    assert_eq!(err.exit_code(), 2);
    assert!(
        err.to_string()
            .contains("layer 1 (namespace-policy): the param replicas is 3, and it must be text"),
        "{err}"
    );
}

#[test]
fn a_tier_whose_install_order_is_not_an_integer_has_no_install_order_in_the_record() {
    let mut tiers = app_tiers(Some(2));
    tiers["spec"]["appTiers"][0]["spec"]["installOrder"] = serde_json::json!("2");
    let app = app_record(tiers);
    for p in &app.packs {
        assert!(p.params.get("installOrder").is_none(), "{}", p.key);
    }
    assert_eq!(
        app.packs.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(),
        ["app/web", "app/db-config", "app/cache"],
        "a tier with no install order keeps its place in the listing"
    );
}

#[test]
fn a_variable_with_no_name_has_an_empty_name_and_a_record_that_holds_a_null_name_does_not_load() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let raw = dir.path().join("a");
    let mut variables = read_json(&raw.join("variables.json"));
    variables["variables"] = serde_json::json!([{"displayName": "no name"}]);
    fs::write(raw.join("variables.json"), variables.to_string()).unwrap();
    let snap = ops::snapshot_from_dir(&raw, "a", "t0").unwrap();
    assert_eq!(snap.variables[0].name, "");

    // a record of 0.24.0 holds `null` for this name: the older form of the variable
    let record = dir.path().join("record.json");
    let mut doc: Value = serde_json::from_str(&snap.to_json(true).unwrap()).unwrap();
    assert_eq!(doc["variables"][0]["name"], "");
    doc["variables"][0]["name"] = Value::Null;
    write_record(&record, &doc);
    let err = Snapshot::load(&record).unwrap_err();
    assert_eq!(err.kind(), "snapshot-schema");
    assert_eq!(err.exit_code(), 2);
    assert!(
        err.to_string().ends_with(
            "variables[0].name: invalid type: null, expected a string. Take the record again."
        ),
        "the message names the field and says what to do: {err}"
    );
}

#[test]
fn a_candidate_in_memory_reproduces_the_root_with_no_file() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded_b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    let candidate = Candidate::from_snapshot(&recorded_b).unwrap();

    // The name is the ref of the record, as the path of a candidate file would be.
    let loaded = candidate::check(candidate.clone(), Path::new("b@1")).unwrap();
    let snap = ops::snapshot_from_loaded(
        &loaded,
        || Ok(Palette::new(serve_registry_for_b(dir.path()))),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(snap.root, recorded_b.root);
    assert_eq!(snap.source.template, Template::Candidate);
    assert_eq!(snap.source.reference, "b@1");

    // The rules of a candidate file hold for a value too, and need no network.
    let mut empty = candidate.clone();
    empty.layers.clear();
    let err = candidate::check(empty, Path::new("b@1")).unwrap_err();
    assert!(
        matches!(&err, Error::CandidateInvalid { path, message, rule: None }
            if path == Path::new("b@1") && message.contains("at least one layer")),
        "{err}"
    );
    let mut both = candidate;
    both.registry = Some("Public Repo".into());
    both.registry_uid = Some("r1".into());
    let err = candidate::check(both, Path::new("b@1")).unwrap_err();
    assert!(
        matches!(&err, Error::CandidateInvalid { message, .. }
            if message.contains("both registry and registryUid")),
        "{err}"
    );
}

#[test]
fn a_candidate_stops_matching_a_version_edited_in_place_and_same_release_names_the_edit() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded_b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    let file = dir.path().join("profile.yaml");
    candidate::write(&Candidate::from_snapshot(&recorded_b).unwrap(), &file, None).unwrap();
    let from_candidate = ops::snapshot_from_candidate(
        &file,
        false,
        || Ok(Palette::new(serve_registry_for_b(dir.path()))),
        "t1",
        &Progress::silent(),
    )
    .unwrap();

    // version b edited in place: one kubernetes value, in the pack ref and the manifests entry
    let edited_dir = dir.path().join("b-edited");
    fs::create_dir_all(&edited_dir).unwrap();
    let edit = |v: &mut Value| {
        let s = v
            .as_str()
            .unwrap()
            .replace("audit-log-maxage: \"90\"", "audit-log-maxage: \"120\"");
        *v = Value::from(s);
    };
    let mut profile = read_json(&dir.path().join("b/profile.json"));
    for p in profile["spec"]["published"]["packs"]
        .as_array_mut()
        .unwrap()
    {
        if p["name"] == "kubernetes" {
            edit(&mut p["values"]);
        }
    }
    let mut manifests = read_json(&dir.path().join("b/manifests.json"));
    for p in manifests["spec"]["packs"].as_array_mut().unwrap() {
        if p["spec"]["name"] == "kubernetes" {
            edit(&mut p["spec"]["values"]);
        }
    }
    fs::write(edited_dir.join("profile.json"), profile.to_string()).unwrap();
    fs::write(edited_dir.join("manifests.json"), manifests.to_string()).unwrap();
    fs::copy(
        dir.path().join("b/variables.json"),
        edited_dir.join("variables.json"),
    )
    .unwrap();
    let edited = ops::snapshot_from_dir(&edited_dir, "b-edited", "t2").unwrap();

    let chain = ops::same_release(
        &[
            ("candidate".to_string(), from_candidate),
            ("edited".to_string(), edited),
        ],
        Policy::Exact,
        "t3",
    );
    assert_eq!(chain.exit_code(), 1);
    let reasons = &chain.pairs[0].reasons;
    assert!(
        reasons.iter().any(|r| r.contains("audit-log-maxage")),
        "names the edited leaf: {reasons:?}"
    );
}

// ---- versions changed under the same uid ---------------------------------------------------------

/// Fixture `name`'s three raw files under `uid`, with `edit` applied to the profile document.
fn serve_edited(raw_root: &Path, name: &str, uid: &str, edit: impl Fn(&mut Value)) -> MemoryApi {
    let mut profile = read_json(&raw_root.join(name).join("profile.json"));
    edit(&mut profile);
    serve_fixture(MemoryApi::new(), raw_root, name, uid).with(profile_route(uid), profile)
}

#[test]
fn verify_reports_a_version_overwritten_under_the_same_uid_even_when_the_root_matches() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    assert_eq!(recorded.source.version, "1.2.0");
    // a legacy version bump: the same uid and content, now called 1.2.1
    let live = serve_edited(dir.path(), "a", UID_A, |p| {
        p["spec"]["version"] = Value::from("1.2.1");
    });

    let v = ops::verify(
        &recorded,
        &Palette::new(live.clone()),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(v.live.root, recorded.root, "the content did not change");
    assert!(!v.unchanged, "the recorded version no longer exists");
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.status, VerifyStatus::Drifted);
    assert_eq!(ver.overwritten_by.as_deref(), Some("lab-profile@1.2.1"));
    let verdict = common::terminal().verdict(&v.diff).unwrap();
    assert!(
        verdict.starts_with("DRIFT:") && verdict.contains("now names lab-profile@1.2.1"),
        "{verdict}"
    );

    let s = common::verify_summary(&recorded, &live);
    assert_eq!(
        (s.records[0].status, s.exit_code()),
        (RecordStatus::Drifted, 1)
    );
    let lines = common::terminal().render_verify_summary(&s).unwrap();
    assert!(lines.contains("now names lab-profile@1.2.1"), "{lines}");
}

#[test]
fn compare_names_a_profile_uid_that_names_two_versions_whatever_the_roots() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let a = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let live = |edit: fn(&mut Value)| {
        snapshot_of_at(&serve_edited(dir.path(), "a", UID_A, edit), UID_A, "t1").unwrap()
    };
    // a legacy version bump that changes no content: the uid now names 1.3.0
    let renamed = live(|p| p["spec"]["version"] = Value::from("1.3.0"));
    let d = ops::compare(&a, &renamed, "t2");
    assert!(d.same, "the content did not change");
    assert!(
        !d.edited_in_place,
        "two version names are not an in-place edit"
    );
    let overwritten: Vec<(&str, &str, &str, &str)> = d
        .overwritten_in_place
        .iter()
        .map(|o| {
            (
                o.uid.as_str(),
                o.name.as_str(),
                o.version_a.as_str(),
                o.version_b.as_str(),
            )
        })
        .collect();
    assert_eq!(overwritten, [(UID_A, "lab-profile", "1.2.0", "1.3.0")]);
    let text = common::terminal().render(&d, &a, &renamed, false).unwrap();
    assert!(text.contains("IDENTICAL"), "{text}");
    assert!(
        text.contains(&format!("uid {UID_A} is lab-profile@1.2.0 on one side and lab-profile@1.3.0 on the other: the version was overwritten in place")),
        "{text}"
    );
    let md = common::markdown().render(&d, &a, &renamed, false).unwrap();
    assert!(md.contains("the version was overwritten in place"), "{md}");
    let json: Value = serde_json::to_value(&d).unwrap();
    assert_eq!(json["overwrittenInPlace"][0]["versionB"], "1.3.0", "{json}");

    // the same bump with a content change: still named, and the roots differ
    let bumped = live(|p| {
        p["spec"]["version"] = Value::from("1.3.0");
        p["spec"]["published"]["packs"][1]["values"] =
            Value::from("pack:\n  k8sHardening: false\n");
    });
    let d = ops::compare(&a, &bumped, "t2");
    assert!(!d.same);
    assert_eq!(d.overwritten_in_place.len(), 1);

    // a cluster that runs the uid under its old version name, against the renamed version
    let cluster = cluster_snapshot(dir.path(), "cluster-a");
    let runs: Vec<(&str, &str)> = cluster
        .source
        .cluster
        .as_ref()
        .unwrap()
        .profiles
        .iter()
        .map(|p| (p.uid.as_str(), p.version.as_str()))
        .collect();
    assert!(runs.contains(&(UID_A, "1.2.0")), "{runs:?}");
    let d = ops::compare(&renamed, &cluster, "t2");
    assert_eq!(
        d.overwritten_in_place
            .iter()
            .map(|o| (o.uid.as_str(), o.version_a.as_str(), o.version_b.as_str()))
            .collect::<Vec<_>>(),
        [(UID_A, "1.3.0", "1.2.0")]
    );

    // different uids under different versions are two profiles, not an overwrite
    let b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    assert!(ops::compare(&a, &b, "t2").overwritten_in_place.is_empty());
    assert!(ops::compare(&a, &a, "t2").overwritten_in_place.is_empty());
}

#[test]
fn compare_says_when_both_sides_are_one_profile_version_with_different_content() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let a = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    // version a edited in place: same uid and version, b's kubernetes values
    let live = serve_edited(dir.path(), "a", UID_A, |p| {
        p["spec"]["published"]["packs"][1]["values"] =
            Value::from("pack:\n  k8sHardening: false\n");
    });
    let edited = snapshot_of_at(&live, UID_A, "t1").unwrap();
    assert_eq!(
        (edited.source.uid.as_str(), edited.source.version.as_str()),
        (a.source.uid.as_str(), a.source.version.as_str())
    );
    let d = ops::compare(&a, &edited, "t2");
    assert!(!d.same);
    assert!(d.edited_in_place, "one uid and version, two contents");
    let text = common::terminal().render(&d, &a, &edited, false).unwrap();
    assert!(
        text.contains(&format!(
            "both sides are lab-profile@1.2.0 (uid {UID_A}): the version was edited in place"
        )),
        "{text}"
    );
    let md = common::markdown().render(&d, &a, &edited, false).unwrap();
    assert!(md.contains("the version was edited in place"), "{md}");

    let b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    assert!(!ops::compare(&a, &b, "t2").edited_in_place, "two versions");
    assert!(!ops::compare(&a, &a, "t2").edited_in_place, "same content");
}

use strata::diff::ImpactLevel;

/// A pack key is `<layer>/<name>`. So a profile that exchanges one pack for a pack of another name
/// gives one removed pack and one added pack. The impact must still name the repave (#53).
#[test]
fn a_pack_that_only_one_side_has_carries_the_impact_of_its_layer() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let a = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();

    // the OS pack of another vendor, under the same layer
    let mut swapped = a.clone();
    let os = swapped
        .packs
        .iter_mut()
        .find(|p| p.name == "ubuntu-aws")
        .unwrap();
    os.name = "rhel-aws".to_string();
    os.key = "os/rhel-aws".to_string();

    let d = ops::compare(&a, &swapped, "t1");
    assert_eq!(d.summary.packs_added, 1);
    assert_eq!(d.summary.packs_removed, 1);
    assert_eq!(d.summary.max_impact, ImpactLevel::Repave, "{:?}", d.summary);
    let added = d.packs.iter().find(|p| p.name == "rhel-aws").unwrap();
    let removed = d.packs.iter().find(|p| p.name == "ubuntu-aws").unwrap();
    assert_eq!(added.impact.level, ImpactLevel::Repave);
    assert_eq!(added.impact.text, "node repave (os pack added)");
    assert_eq!(removed.impact.level, ImpactLevel::Repave);
    assert_eq!(removed.impact.text, "node repave (os pack removed)");

    // the terminal report gives the impact under the pack line
    let text = common::terminal().render(&d, &a, &swapped, false).unwrap();
    assert!(text.contains("node repave (os pack added)"), "{text}");
}

/// An add-on that comes or goes is an in-place change; a cni or csi pack is cluster-wide (#53).
#[test]
fn an_added_addon_is_in_place_and_an_added_infra_pack_is_cluster_wide() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let a = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();

    // fixture b adds cert-manager and removes external-dns, both add-ons
    let d = ops::compare(&a, &b, "t1");
    let added = d.packs.iter().find(|p| p.name == "cert-manager").unwrap();
    let removed = d.packs.iter().find(|p| p.name == "external-dns").unwrap();
    assert_eq!(added.impact.level, ImpactLevel::InPlace);
    assert_eq!(added.impact.text, "addon added in place");
    assert_eq!(removed.impact.level, ImpactLevel::InPlace);
    assert_eq!(removed.impact.text, "addon removed in place");

    // a cni pack that only one side has affects every node
    let mut without_cni = a.clone();
    without_cni.packs.retain(|p| p.layer != "cni");
    let d = ops::compare(&without_cni, &a, "t1");
    let cni = d.packs.iter().find(|p| p.layer == "cni").unwrap();
    assert_eq!(cni.impact.level, ImpactLevel::ClusterWide);
    assert_eq!(d.summary.max_impact, ImpactLevel::ClusterWide);
}

// ---- packs from a registry -------------------------------------------------------------------

use strata::diff::ImpactLevel as Impact;

const CSI_63: &str = "charts:\n  aws-ebs-csi-driver:\n    storageClasses:\n      - name: spectro-storage-class\n        reclaimPolicy: Delete\n";
const CSI_65: &str = "charts:\n  aws-ebs-csi-driver:\n    storageClasses:\n      - name: spectro-storage-class\n        reclaimPolicy: Retain\n    controller:\n      replicas: 2\n";

/// The build `uid` of csi-aws-ebs at `version` in the registry `registry`, for aws, as the pack
/// listing gives it.
fn csi_summary(uid: &str, version: &str, registry: &str, digest: &str, values: &str) -> Value {
    PackFixture::new("csi-aws-ebs", "csi", version, uid, registry, digest, values).summary(&["aws"])
}

/// Two versions of one pack in the Public Repo, and the registry listing that names it.
fn serve_registry(extra: Vec<Value>, entities: &[(&str, Value)]) -> MemoryApi {
    let mut summaries = vec![
        csi_summary("pk-63", "1.63.1", "reg-public", "sha256:aaa", CSI_63),
        csi_summary("pk-65", "1.65.0", "reg-public", "sha256:bbb", CSI_65),
    ];
    summaries.extend(extra);
    serve_summaries(summaries, entities)
}

/// The same, with the pack versions spelled out.
fn serve_summaries(summaries: Vec<Value>, entities: &[(&str, Value)]) -> MemoryApi {
    let api = MemoryApi::new().with(
        REGISTRY_LISTING,
        serde_json::json!({"items": [
            {"name": "Public Repo", "uid": "reg-public", "kind": "pack"},
            {"name": "Mirror", "uid": "reg-mirror", "kind": "oci"},
        ]}),
    );
    let mut api = serve_pack_listing(api, "csi-aws-ebs", Value::Array(summaries));
    for (registry, entity) in entities {
        api = api.with(pack_entity_route("csi-aws-ebs", registry), entity.clone());
    }
    api
}

/// `entity` with a tag row for the build `uid` of `version`. Palette gives a tag row to each
/// version that its registry route serves.
fn with_tag(mut entity: Value, version: &str, uid: &str) -> Value {
    entity["tags"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"tag": version, "packUid": uid}));
    entity
}

fn public_entity() -> Value {
    serde_json::json!({"layer": "csi", "tags": [
        {"tag": "1.63.1", "packUid": "pk-63"},
        {"tag": "1.63.x", "packUid": "pk-63"},
        {"tag": "1.x", "packUid": "pk-65"},
        {"tag": "1.65.0", "packUid": "pk-65"},
    ]})
}

#[test]
fn a_candidate_needs_a_profile_and_refuses_a_pack_record() {
    let api = serve_registry(vec![], &[("reg-public", public_entity())]);
    let pack = snapshot_of(&api, "pack:csi-aws-ebs@1.65.0").unwrap();
    // a pack is one layer, so a candidate built from it would name a profile that does not exist
    let err = strata::candidate::Candidate::from_snapshot(&pack).unwrap_err();
    assert!(matches!(err, Error::ProfileRefRequired { .. }), "{err}");
    assert_eq!(err.kind(), "profile-ref-required");
}

fn packs_of(
    api: &MemoryApi,
    name: &str,
    options: ops::PackOptions,
) -> strata::Result<ops::PackListing> {
    ops::registry_packs(
        name,
        &options,
        |_| Ok(Palette::new(api.clone())),
        "2026-09-18T00:00:00Z",
        &Progress::silent(),
    )
}

/// One build under an exact tag and the floating tags above it, as a registry answers it: each
/// row names every tag above it, so the exact tag names the most and the top floating tag names
/// none.
fn tagged_entity() -> Value {
    serde_json::json!({"layer": "csi", "tags": [
        {"tag": "LTS__24.x", "packUid": "pk-65", "parentTags": null},
        {"tag": "LTS__24.04", "packUid": "pk-65", "parentTags": ["LTS__24.4.x", "LTS__24.x"]},
        {"tag": "LTS__24.4.x", "packUid": "pk-65", "parentTags": ["LTS__24.x"]},
    ]})
}

#[test]
fn the_exact_tag_leads_the_floating_tags_above_it_whatever_their_length() {
    let api = serve_registry(vec![], &[("reg-public", tagged_entity())]);
    let l = packs_of(&api, "csi-aws-ebs", ops::PackOptions::default()).unwrap();
    let newest = &l.registries[0].versions[0];
    assert_eq!(newest.version, "1.65.0");
    // the exact tag is the shortest of the three, so a sort by length buries it
    assert_eq!(
        newest.tags,
        vec!["LTS__24.04", "LTS__24.4.x", "LTS__24.x"],
        "the exact tag first, then each floating tag above it"
    );
}

#[test]
fn packs_lists_what_a_registry_offers_newest_first_with_the_tags_of_each_version() {
    let api = serve_registry(vec![], &[("reg-public", public_entity())]);
    let l = packs_of(&api, "csi-aws-ebs", ops::PackOptions::default()).unwrap();
    assert_eq!(l.schema, ops::PACKS_SCHEMA);
    assert_eq!(l.registries.len(), 1, "only the registry that has it");
    let r = &l.registries[0];
    assert_eq!(
        (r.registry.as_str(), r.layer.as_str()),
        ("Public Repo", "csi")
    );
    let versions: Vec<&str> = r.versions.iter().map(|v| v.version.as_str()).collect();
    assert_eq!(versions, vec!["1.65.0", "1.63.1"], "newest first");
    assert!(r.versions[0].latest, "the newest is marked");
    assert!(!r.versions[1].latest);
    assert_eq!(
        r.versions[0].tags,
        vec!["1.65.0", "1.x"],
        "the exact tag first"
    );
    assert_eq!(r.versions[0].pack_uid, "pk-65");
    assert_eq!(r.versions[0].digest, "sha256:bbb");
    assert_eq!(r.versions[0].kind.as_str(), "spectro");
    assert_eq!(r.versions[0].cloud_types, vec!["aws"]);

    // --latest keeps one line per registry
    let one = packs_of(
        &api,
        "csi-aws-ebs",
        ops::PackOptions {
            latest: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(one.registries[0].versions.len(), 1);
    assert_eq!(one.registries[0].versions[0].version, "1.65.0");
}

#[test]
fn packs_pages_past_a_hundred_versions_and_counts_a_repeated_one_once() {
    // 100 on the first page, 30 on the second, and 1.65.0 served twice, as Palette does
    let mut first: Vec<Value> = (0..100)
        .map(|i| {
            csi_summary(
                &format!("pk-{i}"),
                &format!("1.{i}.0"),
                "reg-public",
                "sha256:a",
                CSI_63,
            )
        })
        .collect();
    first.push(csi_summary(
        "pk-65",
        "1.65.0",
        "reg-public",
        "sha256:bbb",
        CSI_65,
    ));
    let second: Vec<Value> = vec![
        csi_summary("pk-65", "1.65.0", "reg-public", "sha256:bbb", CSI_65),
        csi_summary("pk-200", "1.200.0", "reg-public", "sha256:c", CSI_65),
    ];
    let api = MemoryApi::new()
        .with(
            REGISTRY_LISTING,
            serde_json::json!({"items": [{"name": "Public Repo", "uid": "reg-public", "kind": "pack"}]}),
        )
        .with(
            pack_listing_route("csi-aws-ebs"),
            serde_json::json!({"items": first, "listmeta": {"count": 103}}),
        )
        .with(
            "/v1/packs?filters=spec.name=csi-aws-ebs&limit=100&offset=101",
            serde_json::json!({"items": second, "listmeta": {"count": 103}}),
        )
        .with(
            pack_entity_route("csi-aws-ebs", "reg-public"),
            (0..100).fold(with_tag(public_entity(), "1.200.0", "pk-200"), |e, i| {
                with_tag(e, &format!("1.{i}.0"), &format!("pk-{i}"))
            }),
        );
    let l = packs_of(&api, "csi-aws-ebs", ops::PackOptions::default()).unwrap();
    let versions = &l.registries[0].versions;
    // 1.0.0 to 1.99.0 hold 1.65.0 already, so the pages carry 101 distinct versions in all
    assert_eq!(
        versions.len(),
        101,
        "every distinct version, the repeat counted once"
    );
    assert_eq!(
        versions[0].version, "1.200.0",
        "natural order, not text order"
    );
    assert_eq!(
        versions.iter().filter(|v| v.version == "1.65.0").count(),
        1,
        "the repeated version appears once"
    );
}

/// A listing row with the state Palette gives a pack that Spectro Cloud retired.
fn in_state(mut summary: Value, state: &str) -> Value {
    summary["spec"]["annotations"] = serde_json::json!({
        "system_state": state,
        "system_deprecatedTimestamp": "2025-03-28T19:00:15",
    });
    summary
}

#[test]
fn packs_lists_a_disabled_registry_from_the_listing_and_marks_each_retired_version() {
    // Palette lists the Mirror's disabled version, and then answers 404 for its tags, with and
    // without the states query. A deprecated version it still serves, with its tags.
    let api = serve_summaries(
        vec![
            csi_summary("pk-63", "1.63.1", "reg-public", "sha256:aaa", CSI_63),
            in_state(
                csi_summary("pk-65", "1.65.0", "reg-public", "sha256:bbb", CSI_65),
                "deprecated",
            ),
            in_state(
                csi_summary("pk-m", "1.64.0", "reg-mirror", "sha256:mmm", CSI_63),
                "disabled",
            ),
        ],
        &[("reg-public", public_entity())],
    );
    let l = packs_of(&api, "csi-aws-ebs", ops::PackOptions::default()).unwrap();
    assert_eq!(
        l.registries.len(),
        2,
        "the 404 does not take the listing down"
    );
    let public = &l.registries[0];
    assert_eq!(public.registry, "Public Repo");
    assert!(!public.tags_unavailable);
    assert_eq!(
        public.state, None,
        "the versions of Public Repo differ in state"
    );
    assert_eq!(
        public.versions[0].state.as_ref().map(Open::as_str),
        Some("deprecated")
    );
    assert_eq!(
        public.versions[0].tags,
        vec!["1.65.0", "1.x"],
        "a deprecated version keeps its tags"
    );
    assert_eq!(public.versions[1].state, None);
    let mirror = &l.registries[1];
    assert_eq!(mirror.registry, "Mirror");
    assert!(mirror.tags_unavailable);
    assert_eq!(
        mirror.state.as_ref().map(Open::as_str),
        Some("disabled"),
        "every version of the Mirror is disabled"
    );
    assert_eq!(mirror.layer, "csi", "the layer comes from the listing");
    assert_eq!(mirror.versions[0].version, "1.64.0");
    assert_eq!(mirror.versions[0].digest, "sha256:mmm");
    assert_eq!(
        mirror.versions[0].state.as_ref().map(Open::as_str),
        Some("disabled")
    );
    assert!(mirror.versions[0].tags.is_empty());

    // the heading and the row say so, in both text formats
    let text = common::terminal().render_packs(&l).unwrap();
    assert!(
        text.contains("csi-aws-ebs in Mirror (1 version, disabled, no tags)"),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|l| l.contains("1.65.0") && l.trim_end().ends_with("deprecated")),
        "{text}"
    );
    let md = common::markdown().render_packs(&l).unwrap();
    assert!(
        md.contains("### csi-aws-ebs in Mirror (1 version, disabled, no tags)"),
        "{md}"
    );
    assert!(md.contains("`1.65.0` deprecated"), "{md}");
}

/// A listing row with the mark of a version that Palette refuses in a profile. Spectro Cloud sets
/// the mark with no `system_state`.
fn marked(mut summary: Value) -> Value {
    summary["spec"]["annotations"] = serde_json::json!({"disabled": "true"});
    summary
}

#[test]
fn packs_gives_the_state_disabled_to_a_version_with_the_mark_and_reads_its_tags_with_no_states_query()
 {
    // Palette serves the tags of a marked version, so the plain answer has them, and strata
    // sends no states query. A record of the version still writes: the validate route of Palette
    // refuses the profile, and strata does not copy that rule
    let api = serve_summaries(
        vec![
            csi_summary("pk-63", "1.63.1", "reg-public", "sha256:aaa", CSI_63),
            marked(csi_summary(
                "pk-65",
                "1.65.0",
                "reg-public",
                "sha256:bbb",
                CSI_65,
            )),
            marked(csi_summary(
                "pk-m",
                "1.64.0",
                "reg-mirror",
                "sha256:mmm",
                CSI_63,
            )),
        ],
        &[
            ("reg-public", public_entity()),
            (
                "reg-mirror",
                serde_json::json!({"layer": "csi", "tags": [{"tag": "1.64.0", "packUid": "pk-m"}]}),
            ),
        ],
    );
    let l = packs_of(&api, "csi-aws-ebs", ops::PackOptions::default()).unwrap();
    let public = &l.registries[0];
    assert_eq!(
        public.state, None,
        "the versions of Public Repo differ in state"
    );
    assert_eq!(public.versions[0].version, "1.65.0");
    assert_eq!(
        public.versions[0].state.as_ref().map(Open::as_str),
        Some("disabled")
    );
    assert_eq!(
        public.versions[0].tags,
        vec!["1.65.0", "1.x"],
        "the plain answer gives the tags of a marked version"
    );
    assert_eq!(public.versions[1].state, None);
    let mirror = &l.registries[1];
    assert_eq!(
        mirror.state.as_ref().map(Open::as_str),
        Some("disabled"),
        "every version of the Mirror has the mark"
    );
    assert!(!mirror.tags_unavailable);
    assert_eq!(mirror.versions[0].tags, vec!["1.64.0"]);
    for registry in ["reg-public", "reg-mirror"] {
        assert_eq!(
            api.calls(&format!(
                "{}?states=deprecated,disabled,deleted",
                pack_entity_route("csi-aws-ebs", registry)
            )),
            0,
            "{registry}: no states query for a version that Palette serves"
        );
    }

    let text = common::terminal().render_packs(&l).unwrap();
    assert!(
        text.contains("csi-aws-ebs in Mirror (1 version, disabled)"),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|l| l.contains("1.65.0") && l.trim_end().ends_with("disabled")),
        "{text}"
    );

    let snap = snapshot_of(&api, "pack:Public Repo/csi-aws-ebs@1.65.0").unwrap();
    assert_eq!(snap.packs[0].version, "1.65.0");
}

#[test]
fn packs_lists_only_the_retired_versions_of_a_registry_that_answers_404_and_stops_on_any_other_failure()
 {
    // the Mirror answers 404 with and without the states query: it serves no version. Its
    // disabled and deleted versions differ in state
    let mirror =
        |uid: &str, version: &str| csi_summary(uid, version, "reg-mirror", "sha256:mmm", CSI_63);
    let api = serve_registry(
        vec![
            mirror("pk-m4", "1.64.0"),
            in_state(mirror("pk-m2", "1.62.0"), "disabled"),
            in_state(mirror("pk-m1", "1.61.0"), "deleted"),
        ],
        &[("reg-public", public_entity())],
    );
    let l = packs_of(&api, "csi-aws-ebs", ops::PackOptions::default()).unwrap();
    let mirror = &l.registries[1];
    assert!(mirror.tags_unavailable);
    assert_eq!(mirror.state, None);
    assert_eq!(
        mirror
            .versions
            .iter()
            .map(|v| v.version.as_str())
            .collect::<Vec<_>>(),
        ["1.62.0", "1.61.0"],
        "the version that Palette does not serve is not in the listing"
    );
    assert_eq!(mirror.versions_offered, 2);
    let text = common::terminal().render_packs(&l).unwrap();
    assert!(
        text.contains("csi-aws-ebs in Mirror (2 versions, no tags: the registry answered 404)"),
        "{text}"
    );

    let api = serve_registry(vec![], &[]).failing(
        pack_entity_route("csi-aws-ebs", "reg-public"),
        403,
        "forbidden",
    );
    let err = packs_of(&api, "csi-aws-ebs", ops::PackOptions::default()).unwrap_err();
    assert_eq!(err.kind(), "api");
}

#[test]
fn a_disabled_or_deleted_pack_version_is_unavailable_and_a_deprecated_one_resolves() {
    for state in ["disabled", "deleted"] {
        let api = serve_summaries(
            vec![in_state(
                csi_summary("pk-65", "1.65.0", "reg-public", "sha256:bbb", CSI_65),
                state,
            )],
            &[],
        );
        let err = snapshot_of(&api, "pack:Public Repo/csi-aws-ebs@1.65.0").unwrap_err();
        assert_eq!(err.kind(), "pack-unavailable", "{state}: {err}");
        assert_eq!(err.exit_code(), 2);
        let msg = format!("{err}");
        assert!(
            msg.contains(&format!("is {state} in registry 'Public Repo'")),
            "{msg}"
        );
        let json = err.to_json();
        assert_eq!(json["state"], state);
        assert_eq!(json["version"], "1.65.0");
    }
    let api = serve_summaries(
        vec![in_state(
            csi_summary("pk-65", "1.65.0", "reg-public", "sha256:bbb", CSI_65),
            "deprecated",
        )],
        &[("reg-public", public_entity())],
    );
    let snap = snapshot_of(&api, "pack:Public Repo/csi-aws-ebs@1.65.0").unwrap();
    assert_eq!(snap.packs[0].tag, "1.65.0");
}

#[test]
fn packs_keeps_the_versions_of_one_cloud_and_answers_with_none_when_no_version_has_it() {
    let api = serve_registry(vec![], &[("reg-public", public_entity())]);
    let aws = packs_of(
        &api,
        "csi-aws-ebs",
        ops::PackOptions {
            cloud: Some("aws".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(aws.registries[0].versions.len(), 2);

    // a cloud no version supports is an answer, and an empty one: exit 0, not an error
    let none = packs_of(
        &api,
        "csi-aws-ebs",
        ops::PackOptions {
            cloud: Some("edge-native".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(none.registries.is_empty());
    assert_eq!(none.cloud.as_deref(), Some("edge-native"));
}

#[test]
fn packs_says_in_each_format_that_no_version_supports_the_cloud() {
    let api = serve_registry(vec![], &[("reg-public", public_entity())]);
    let none = packs_of(
        &api,
        "csi-aws-ebs",
        ops::PackOptions {
            cloud: Some("edge-native".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(none.registries.is_empty());
    let text = common::terminal().render_packs(&none).unwrap();
    assert_eq!(text, "no version of csi-aws-ebs supports edge-native\n");
    let md = common::markdown().render_packs(&none).unwrap();
    assert_eq!(md, "No version of csi-aws-ebs supports edge-native.\n");
}

#[test]
fn packs_says_in_each_format_that_palette_serves_no_version_of_the_pack() {
    // the registry answers 404 for the tags: Palette serves no version of the pack, and the
    // listing keeps no registry
    let api = serve_registry(vec![], &[]);
    let none = packs_of(&api, "csi-aws-ebs", ops::PackOptions::default()).unwrap();
    assert!(none.registries.is_empty());
    assert_eq!(none.cloud, None);
    let text = common::terminal().render_packs(&none).unwrap();
    assert_eq!(text, "Palette serves no version of csi-aws-ebs\n");
    let md = common::markdown().render_packs(&none).unwrap();
    assert_eq!(md, "Palette serves no version of csi-aws-ebs.\n");
}

#[test]
fn packs_refuses_a_registry_that_is_not_visible_and_a_pack_no_registry_has() {
    let api = serve_registry(vec![], &[("reg-public", public_entity())]);
    let err = packs_of(
        &api,
        "csi-aws-ebs",
        ops::PackOptions {
            registry: Some("Nope".to_string()),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(err.kind(), "registry-not-found");
    assert_eq!(err.exit_code(), 2);
    assert!(
        format!("{err}").contains("Public Repo"),
        "the error lists what is visible: {err}"
    );

    let api = serve_pack_listing(
        MemoryApi::new().with(
            REGISTRY_LISTING,
            serde_json::json!({"items": [{"name": "Public Repo", "uid": "reg-public", "kind": "pack"}]}),
        ),
        "nowhere",
        serde_json::json!([]),
    );
    // no registry was named and no version was asked for, so the error states neither: it names
    // the pack name alone, and points at the command that finds a pack inside a profile
    let err = packs_of(&api, "nowhere", ops::PackOptions::default()).unwrap_err();
    assert_eq!(err.kind(), "pack-name-not-found");
    assert_eq!(err.exit_code(), 2);
    let text = err.to_string();
    assert!(text.contains("'nowhere'"), "{text}");
    assert!(!text.contains("any visible registry"), "{text}");
    assert!(!text.contains("version ''"), "{text}");
}

#[test]
fn comparing_two_pack_versions_says_what_changed_between_them() {
    let api = serve_registry(vec![], &[("reg-public", public_entity())]);
    let old = snapshot_of(&api, "pack:csi-aws-ebs@1.63.1").unwrap();
    let new = snapshot_of(&api, "pack:csi-aws-ebs@1.65.0").unwrap();

    assert_eq!(old.packs.len(), 1, "a pack snapshot holds the one pack");
    assert_eq!(old.source.title(), "pack csi-aws-ebs@1.63.1");
    assert_eq!(old.source.template, Template::Registry);
    assert_eq!(old.source.uid, "", "a registry pack has no profile uid");
    let src = old.source.pack.as_ref().expect("a pack snapshot says so");
    assert_eq!(
        (
            src.name.as_str(),
            src.version.as_str(),
            src.registry_uid.as_str(),
            src.registry_name.as_str()
        ),
        ("csi-aws-ebs", "1.63.1", "reg-public", "Public Repo")
    );
    assert_eq!(
        src.pack_uid, "pk-63",
        "the uid of the pack version in the registry"
    );
    assert_eq!(old.packs[0].tag, "1.63.1");
    assert_eq!(old.packs[0].layer, "csi");
    assert_eq!(old.packs[0].hash.upstream_raw, "sha256:aaa");

    let d = ops::compare(&old, &new, "t1");
    assert!(!d.same);
    let pd = &d.packs[0];
    assert_eq!(pd.name, "csi-aws-ebs");
    assert_eq!(pd.version_move, Some(VersionMove::Forward));
    assert_eq!(
        d.version_move,
        Some(VersionMove::Forward),
        "the source of a pack record has the version of the pack"
    );
    let back = ops::compare(&new, &old, "t1");
    assert_eq!(back.packs[0].version_move, Some(VersionMove::Back));
    assert!(
        pd.identity && pd.upstream && pd.authored,
        "version, digest, and values all moved"
    );
    let paths: Vec<&str> = pd.values.diff.iter().map(|l| l.path.as_str()).collect();
    assert!(
        paths.iter().any(|p| p.ends_with("reclaimPolicy")),
        "{paths:?}"
    );
    assert!(
        paths.iter().any(|p| p.ends_with("controller.replicas")),
        "{paths:?}"
    );
    assert_eq!(
        pd.impact.level,
        Impact::ClusterWide,
        "a CSI pack affects every node"
    );
    assert_eq!(d.summary.max_impact, Impact::ClusterWide);

    // the newest version of the pack in the registry
    let latest = snapshot_of(&api, "pack:csi-aws-ebs@latest").unwrap();
    assert_eq!(latest.source.version, "1.65.0");
    assert_eq!(latest.source.reference, "pack:csi-aws-ebs@latest");
    assert_eq!(latest.root, new.root);
}

#[test]
fn a_pack_in_several_registries_needs_the_registry_named() {
    let mirrored = csi_summary("pk-65-mirror", "1.65.0", "reg-mirror", "sha256:ccc", CSI_65);
    let mirror_entity =
        serde_json::json!({"layer": "csi", "tags": [{"tag": "1.65.0", "packUid": "pk-65-mirror"}]});
    let api = serve_registry(
        vec![mirrored],
        &[
            ("reg-public", public_entity()),
            ("reg-mirror", mirror_entity),
        ],
    );

    let err = snapshot_of(&api, "pack:csi-aws-ebs@1.65.0").unwrap_err();
    match &err {
        Error::PackInSeveralRegistries {
            name,
            version,
            registries,
        } => {
            assert_eq!((name.as_str(), version.as_str()), ("csi-aws-ebs", "1.65.0"));
            // sorted by name, not in the order Palette listed them: a live run of packs.sh saw
            // the two orders on two runs, which made the message differ for one tenant state
            assert_eq!(
                registries,
                &vec!["Mirror".to_string(), "Public Repo".to_string()]
            );
        }
        other => panic!("unexpected {other}"),
    }
    assert_eq!(err.kind(), "pack-in-several-registries");
    assert_eq!(err.exit_code(), 2);
    assert!(
        err.to_string().ends_with(
            "name one, as in pack:<registry>/csi-aws-ebs@1.65.0. `strata list --pack csi-aws-ebs` \
             names the registry of the pack in each profile version"
        ),
        "the refusal points to the command that names the registry of a profile: {err}"
    );

    // by registry name, and by registry uid
    let before = api.calls(REGISTRY_LISTING);
    let by_name = snapshot_of(&api, "pack:Mirror/csi-aws-ebs@1.65.0").unwrap();
    assert_eq!(by_name.packs[0].hash.upstream_raw, "sha256:ccc");
    assert_eq!(by_name.source.reference, "pack:Mirror/csi-aws-ebs@1.65.0");
    assert_eq!(by_name.packs[0].registry_name, "Mirror");
    assert_eq!(
        api.calls(REGISTRY_LISTING),
        before + 1,
        "the name of the registry comes from the listing that the ref read"
    );
    let by_uid = snapshot_of(&api, "pack:reg-public/csi-aws-ebs@1.65.0").unwrap();
    assert_eq!(by_uid.packs[0].hash.upstream_raw, "sha256:bbb");
    assert_eq!(by_uid.packs[0].registry_name, "Public Repo");

    // the same release in two registries differs by IDENTITY and UPSTREAM alone
    let d = ops::compare(&by_uid, &by_name, "t1");
    let pd = &d.packs[0];
    assert!(pd.identity && pd.upstream && !pd.authored);

    let unknown = snapshot_of(&api, "pack:Nope/csi-aws-ebs@1.65.0").unwrap_err();
    assert!(
        matches!(unknown, Error::RegistryNotFound { .. }),
        "{unknown}"
    );
    let missing = snapshot_of(&api, "pack:csi-aws-ebs@9.9.9").unwrap_err();
    assert!(matches!(missing, Error::PackNotFound { .. }), "{missing}");
}

#[test]
fn verify_rechecks_a_pack_version_against_its_registry() {
    let api = serve_registry(vec![], &[("reg-public", public_entity())]);
    let recorded = snapshot_of(&api, "pack:csi-aws-ebs@1.65.0").unwrap();
    let palette = Palette::new(api.clone());
    let v = ops::verify(&recorded, &palette, "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged);

    // the registry now serves other content under the same version
    // the same version, with other content under it
    let mut summary = csi_summary("pk-65", "1.65.0", "reg-public", "sha256:moved", CSI_63);
    summary["metadata"]["lastModifiedTimestamp"] = Value::from("2026-07-30T22:45:47.109Z");
    let moved = serve_summaries(vec![summary], &[("reg-public", public_entity())]);
    let v = ops::verify(&recorded, &Palette::new(moved), "t1", &Progress::silent()).unwrap();
    assert!(!v.unchanged, "a pack version whose content moved is drift");
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.status, VerifyStatus::Drifted);
    // the time of the listing is the create time of the pack, not the time of the move
    assert_eq!(ver.last_modified, None);
    let line = verdict_of(&v);
    assert!(!line.contains("Palette last changed it"), "{line}");
    let json = serde_json::to_value(&v.diff).unwrap();
    assert!(json["verify"].get("lastModified").is_none(), "{json}");
}

#[test]
fn verify_of_an_oci_pack_record_reads_the_oci_listing_one_time() {
    // the record reads the listing for the provider type, and the sync note for the last sync
    let mirror_api = || {
        let mut oci = csi_summary("pk-m", "1.64.0", "reg-mirror", "sha256:mmm", CSI_63);
        oci["spec"]["type"] = serde_json::json!("oci");
        let mirror =
            serde_json::json!({"layer": "csi", "tags": [{"tag": "1.64.0", "packUid": "pk-m"}]});
        serve_registry(
            vec![oci],
            &[("reg-public", public_entity()), ("reg-mirror", mirror)],
        )
        .with(
            OCI_REGISTRY_SUMMARY,
            serde_json::json!({"items": [{"metadata": {"uid": "reg-mirror", "name": "Mirror"},
                "spec": {"providerType": "helm"}}]}),
        )
    };
    let recorded = snapshot_of(&mirror_api(), "pack:Mirror/csi-aws-ebs@1.64.0").unwrap();
    let live = mirror_api();
    let v = ops::verify(&recorded, &Palette::new(&live), "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged);
    // the record and the sync note use one cache of the listings of the command
    assert_eq!(live.calls(OCI_REGISTRY_SUMMARY), 1);
    assert_eq!(live.calls(REGISTRY_LISTING), 1);
}

/// The summary listing of the pack registries, which gives the sync of each registry.
const PACK_REGISTRY_SUMMARY: &str = "/v1/registries/pack/summary";

/// The summary listing of the pack registries with the Public Repo, whose sync is `sync`.
fn public_repo_summary(sync: Value) -> Value {
    serde_json::json!({"items": [{"metadata": {"uid": "reg-public"}, "status": {"sync": sync}}]})
}

/// Verify a record of csi-aws-ebs@1.65.0 in the Public Repo, whose registry has `sync` for
/// its sync status: the verdict line, and the diff as `--json` writes it.
fn verify_pack_with_sync(sync: Option<Value>) -> (String, Value) {
    let api = serve_registry(vec![], &[("reg-public", public_entity())]);
    let recorded = snapshot_of(&api, "pack:csi-aws-ebs@1.65.0").unwrap();
    let api = match sync {
        Some(s) => api.with(PACK_REGISTRY_SUMMARY, public_repo_summary(s)),
        None => api,
    };
    let v = ops::verify(&recorded, &Palette::new(api), "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged);
    let verdict = common::terminal().verdict(&v.diff).unwrap();
    (verdict, serde_json::to_value(&v.diff).unwrap())
}

#[test]
fn verify_of_a_pack_record_ends_the_verdict_with_the_last_sync_of_the_registry() {
    let (verdict, json) = verify_pack_with_sync(Some(serde_json::json!({
        "isSyncSupported": true, "lastRunTime": "2026-09-30T00:10:00.000Z",
        "lastSyncedTime": "2026-09-30T00:00:11.000Z", "status": "Completed",
    })));
    assert_eq!(
        verdict,
        "OK: pack csi-aws-ebs@1.65.0 in registry Public Repo is unchanged since t0; \
         the last complete sync of the registry started 2026-09-30T00:00:11.000Z"
    );
    assert_eq!(
        json["verify"]["registrySync"],
        serde_json::json!({"lastSyncedTime": "2026-09-30T00:00:11.000Z",
            "lastRunTime": "2026-09-30T00:10:00.000Z", "status": "Completed"})
    );
}

#[test]
fn verify_of_a_pack_record_says_that_a_sync_of_the_registry_is_in_progress_and_not_that_it_synced()
{
    let (verdict, json) = verify_pack_with_sync(Some(serde_json::json!({
        "isSyncSupported": true, "lastRunTime": "2026-10-05T16:03:58.598Z",
        "lastSyncedTime": "2026-08-29T00:00:11.032Z", "status": "InProgress",
        "syncRecovery": {"message": "sync interrupted", "syncRecoveryState": "Recovering"},
    })));
    assert_eq!(
        verdict,
        "OK: pack csi-aws-ebs@1.65.0 in registry Public Repo is unchanged since t0; \
         a sync of the registry is in progress since 2026-10-05T16:03:58.598Z, and the last \
         complete sync of the registry started 2026-08-29T00:00:11.032Z; the recovery of the \
         sync is Recovering: sync interrupted"
    );
    assert!(!verdict.contains("synced"), "{verdict}");
    assert_eq!(json["verify"]["registrySync"]["status"], "InProgress");
    assert_eq!(
        json["verify"]["registrySync"]["syncRecovery"],
        serde_json::json!({"syncRecoveryState": "Recovering", "message": "sync interrupted"})
    );
}

#[test]
fn verify_of_a_pack_record_names_a_failed_sync_of_the_registry_with_its_message() {
    let (verdict, json) = verify_pack_with_sync(Some(serde_json::json!({
        "isSyncSupported": true, "lastSyncedTime": "2025-10-16T08:00:00.000Z",
        "status": "Failed", "message": "invalid helm registry http status 404",
    })));
    assert_eq!(
        verdict,
        "OK: pack csi-aws-ebs@1.65.0 in registry Public Repo is unchanged since t0; the last \
         complete sync of the registry started 2025-10-16T08:00:00.000Z; the last sync of the \
         registry failed: invalid helm registry http status 404"
    );
    assert_eq!(json["verify"]["registrySync"]["status"], "Failed");
    assert_eq!(
        json["verify"]["registrySync"]["message"],
        "invalid helm registry http status 404"
    );
    assert_eq!(
        json["verify"]["registrySync"]["lastSyncedTime"],
        "2025-10-16T08:00:00.000Z"
    );
}

#[test]
fn verify_of_a_pack_record_gives_no_sync_note_for_a_registry_that_does_not_sync() {
    let plain = "OK: pack csi-aws-ebs@1.65.0 in registry Public Repo is unchanged since t0";
    // the registry does not sync
    let (verdict, json) = verify_pack_with_sync(Some(serde_json::json!({
        "isSyncSupported": false, "lastSyncedTime": "2026-09-30T00:00:11.000Z",
        "status": "Completed",
    })));
    assert_eq!(verdict, plain);
    assert!(json["verify"].get("registrySync").is_none(), "{json}");
    // Palette does not serve the route: a 404, no note and no error
    let (verdict, json) = verify_pack_with_sync(None);
    assert_eq!(verdict, plain);
    assert!(json["verify"].get("registrySync").is_none(), "{json}");
}

/// Verify a record of csi-aws-ebs@1.65.0 in the Public Repo on `api`, which has the pack: the
/// verdict line, and the diff as `--json` writes it. The verify must succeed with `unchanged`,
/// the status that exits 0.
fn verify_pack_on<A: PaletteApi>(recorded: &Snapshot, api: A) -> (String, Value) {
    let v = ops::verify(recorded, &Palette::new(api), "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged);
    assert_eq!(v.status, VerifyStatus::Unchanged);
    let verdict = common::terminal().verdict(&v.diff).unwrap();
    (verdict, serde_json::to_value(&v.diff).unwrap())
}

/// Answers the summary listing of the pack registries with a dropped connection, and delegates
/// each other request. The in-memory API cannot produce a transport error.
struct SyncDropped(MemoryApi);

impl PaletteApi for SyncDropped {
    fn get_json(&self, path: &str) -> strata::Result<Value> {
        if path == PACK_REGISTRY_SUMMARY {
            return Err(Error::Transport {
                method: Method::Get,
                path: path.to_string(),
                message: "connection reset".into(),
            });
        }
        self.0.get_json(path)
    }
}

#[test]
fn verify_of_a_pack_record_gives_the_verdict_when_the_registry_summary_answers_500() {
    let api = serve_registry(vec![], &[("reg-public", public_entity())]);
    let recorded = snapshot_of(&api, "pack:csi-aws-ebs@1.65.0").unwrap();
    let api = api.failing(PACK_REGISTRY_SUMMARY, 500, r#"{"code":"InternalError"}"#);
    let (verdict, json) = verify_pack_on(&recorded, api);
    assert_eq!(
        verdict,
        "OK: pack csi-aws-ebs@1.65.0 in registry Public Repo is unchanged since t0"
    );
    assert!(json["verify"].get("registrySync").is_none(), "{json}");
    assert_eq!(
        json["verify"]["registrySyncError"],
        r#"HTTP 500: {"code":"InternalError"}"#
    );
    assert_eq!(json["verify"]["status"], "unchanged");
}

#[test]
fn verify_keeps_no_newline_of_the_body_in_the_reason_of_a_sync_that_it_could_not_read() {
    // Palette ends a body with a newline. The reason on the record has none, as the reason of an
    // unread part of a cluster record has none (#321)
    let api = serve_registry(vec![], &[("reg-public", public_entity())]);
    let recorded = snapshot_of(&api, "pack:csi-aws-ebs@1.65.0").unwrap();
    let api = api.failing(PACK_REGISTRY_SUMMARY, 500, "{\"code\":\"InternalError\"}\n");
    let (_, json) = verify_pack_on(&recorded, api);
    assert_eq!(
        json["verify"]["registrySyncError"],
        r#"HTTP 500: {"code":"InternalError"}"#
    );
}

#[test]
fn verify_of_a_pack_record_gives_the_verdict_when_the_sync_request_does_not_complete() {
    let api = serve_registry(vec![], &[("reg-public", public_entity())]);
    let recorded = snapshot_of(&api, "pack:csi-aws-ebs@1.65.0").unwrap();
    let (verdict, json) = verify_pack_on(&recorded, SyncDropped(api));
    assert_eq!(
        verdict,
        "OK: pack csi-aws-ebs@1.65.0 in registry Public Repo is unchanged since t0"
    );
    assert!(json["verify"].get("registrySync").is_none(), "{json}");
    assert_eq!(
        json["verify"]["registrySyncError"],
        "GET /v1/registries/pack/summary: connection reset"
    );
    assert_eq!(json["verify"]["status"], "unchanged");
}

#[test]
fn verify_reports_drift_and_names_the_order_when_only_the_order_of_the_packs_changed() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let live = serve_fixture(MemoryApi::new(), dir.path(), "a-reversed", UID_A);
    let v = ops::verify(
        &recorded,
        &Palette::new(live.clone()),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(
        !v.unchanged,
        "Palette keeps the order given, so two orders are two roots"
    );
    assert!(v.diff.order_changed);
    assert_eq!(v.diff.summary.packs_changed, 0, "every pack is as it was");
    assert!(!v.diff.same_content, "a reorder is not the same content");

    let s = common::verify_summary(&recorded, &live);
    assert_eq!(
        (s.records[0].status, s.exit_code()),
        (RecordStatus::Drifted, 1)
    );
    let lines = common::terminal().render_verify_summary(&s).unwrap();
    assert!(
        lines.contains("no packs changed, the packs in another order"),
        "{lines}"
    );
}

// ---- verify --who --------------------------------------------------------------------------

/// One item of `GET /v1/audits`, in the shape Palette gives it.
fn audit(uid: &str, time: &str, actor: Value, action: &str, content: Option<&str>) -> Value {
    let mut spec = serde_json::json!({
        "actionType": action,
        "actor": actor,
        "resource": {"kind": "clusterprofile", "uid": UID_A},
    });
    if let Some(c) = content {
        spec["contentMsg"] = Value::from(c);
    }
    serde_json::json!({
        "metadata": {"uid": uid, "creationTimestamp": time},
        "spec": spec,
    })
}

fn person() -> Value {
    serde_json::json!({
        "actorType": "user",
        "user": {"firstName": "John", "lastName": "Doe", "emailId": "john.doe@example.com", "uid": "u1"}
    })
}

fn system() -> Value {
    serde_json::json!({"actorType": "service", "serviceName": "system", "user": {}})
}

fn audits_page(items: Vec<Value>, next: &str) -> Value {
    serde_json::json!({"items": items, "listmeta": {"continue": next, "limit": 100}})
}

/// The audit log of the profile version `UID_A` from the time `since`, as `verify --who` reads it.
fn profile_audits(since: &str) -> String {
    format!(
        "/v1/audits?resourceKind=clusterprofile&resourceUid={UID_A}&startTime={since}&limit=100"
    )
}

/// The audit log of the cluster under `CLUSTER_UID` from the time `since`.
fn cluster_audits(since: &str) -> String {
    format!("/v1/audits?resourceUid={CLUSTER_UID}&startTime={since}&limit=100")
}

#[test]
fn verify_who_reads_every_page_of_the_audit_log_and_lists_the_writes_oldest_first() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    // Palette lists the newest first, 100 on a page, and names the next page in `continue`
    let api = serve_fixture(MemoryApi::new(), dir.path(), "b", UID_A)
        .with(
            profile_audits("t0"),
            audits_page(
                vec![audit(
                    "au3",
                    "2026-09-18T15:00:00.000Z",
                    system(),
                    "update",
                    Some("strata pack version is updated\n1.0.1 > 1.0.2"),
                )],
                "tok",
            ),
        )
        .with(
            format!("{}&continue=tok", profile_audits("t0")),
            audits_page(
                vec![audit(
                    "au1",
                    "2026-09-18T14:02:11.482Z",
                    person(),
                    "update",
                    None,
                )],
                "",
            ),
        );
    let v = ops::verify_with(
        &recorded,
        &Palette::new(&api),
        "t1",
        &ops::VerifyOptions {
            who: true,
            ..Default::default()
        },
        &Progress::silent(),
    )
    .unwrap();
    let ver = v.diff.verify.as_ref().unwrap();
    let who = ver.who.as_ref().unwrap();
    let got: Vec<(&str, &str, &str, &str, &str)> = who
        .iter()
        .map(|e| {
            (
                e.audit_uid.as_str(),
                e.actor.as_str(),
                e.actor_type.as_str(),
                e.actor_uid.as_str(),
                e.summary.as_str(),
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            ("au1", "John Doe", "user", "u1", ""),
            (
                "au3",
                "system",
                "service",
                "",
                "strata pack version is updated 1.0.1 > 1.0.2"
            ),
        ]
    );
    let verdict = common::terminal().verdict(&v.diff).unwrap();
    assert!(
        verdict.ends_with(
            "\nwho: 2 writes in the audit log since the record\n  \
             2026-09-18T14:02:11.482Z  John Doe (user)  update\n  \
             2026-09-18T15:00:00.000Z  system (service)  update: strata pack version is updated 1.0.1 > 1.0.2"
        ),
        "{verdict}"
    );
    let json = serde_json::to_value(&v.diff).unwrap();
    assert_eq!(json["verify"]["who"][0]["auditUid"], "au1");
    assert_eq!(json["verify"]["who"][0]["actorType"], "user");
    assert!(json["verify"]["who"][0].get("summary").is_none());
}

#[test]
fn verify_who_says_so_when_the_log_holds_no_write_and_reads_nothing_for_an_unchanged_record() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let api = serve_fixture(MemoryApi::new(), dir.path(), "b", UID_A)
        .with(profile_audits("t0"), audits_page(vec![], ""));
    let v = ops::verify_with(
        &recorded,
        &Palette::new(&api),
        "t1",
        &ops::VerifyOptions {
            who: true,
            ..Default::default()
        },
        &Progress::silent(),
    )
    .unwrap();
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.who.as_deref(), Some(&[][..]));
    let terminal = common::terminal().verdict(&v.diff).unwrap();
    assert!(
        terminal.ends_with("\nwho: no audit entry since the record"),
        "{terminal}"
    );
    let json = serde_json::to_value(&v.diff).unwrap();
    assert_eq!(json["verify"]["who"], serde_json::json!([]));

    // an unchanged record reads no audit log, and without --who a drifted one reads none either
    let same = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A);
    let v = ops::verify_with(
        &recorded,
        &Palette::new(&same),
        "t1",
        &ops::VerifyOptions {
            who: true,
            ..Default::default()
        },
        &Progress::silent(),
    )
    .unwrap();
    assert!(v.unchanged && v.diff.verify.unwrap().who.is_none());
    let v = ops::verify(&recorded, &Palette::new(&api), "t1", &Progress::silent()).unwrap();
    assert!(v.diff.verify.unwrap().who.is_none());
    assert_eq!(api.calls(&profile_audits("t0")), 1);
}

#[test]
fn verify_who_reads_the_audit_log_of_the_cluster_for_a_cluster_record() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-a"),
        None,
    )
    .unwrap();
    let path = cluster_audits("t0");
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-b").with(
        path.clone(),
        audits_page(
            vec![audit(
                "au1",
                "2026-09-18T14:02:11.482Z",
                person(),
                "update",
                None,
            )],
            "",
        ),
    );
    let v = ops::verify_with(
        &recorded,
        &Palette::new(&api),
        "t1",
        &ops::VerifyOptions {
            who: true,
            ..Default::default()
        },
        &Progress::silent(),
    )
    .unwrap();
    assert!(!v.unchanged);
    assert_eq!(api.calls(&path), 1);
    let who = v.diff.verify.unwrap().who.unwrap();
    assert_eq!((who.len(), who[0].actor.as_str()), (1, "John Doe"));
}

#[test]
fn verify_who_of_a_virtual_cluster_record_names_the_create_that_palette_logs_under_virtual_cluster()
{
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-a"),
        None,
    )
    .unwrap();
    // Palette logs the create of a virtual cluster under virtualCluster, not spectrocluster
    let created = serde_json::json!({"kind": "virtualCluster", "label": "Virtual Cluster",
        "name": "lab-cluster", "uid": CLUSTER_UID});
    let mut entry = audit_of("au1", "2026-09-18T14:02:11.482Z", created, None);
    entry["spec"]["actionType"] = Value::from("create");
    let path = cluster_audits("t0");
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-b")
        .with(path.clone(), audits_page(vec![entry], ""));
    let v = ops::verify_with(
        &recorded,
        &Palette::new(&api),
        "t1",
        &ops::VerifyOptions {
            who: true,
            ..Default::default()
        },
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(api.calls(&path), 1, "the log is read by the uid alone");
    let who = v.diff.verify.as_ref().unwrap().who.as_ref().unwrap();
    let resource = who[0].resource.as_ref().unwrap();
    assert_eq!(
        (who.len(), who[0].action.as_str(), resource.kind.as_str()),
        (1, "create", "virtualCluster")
    );
    let line = verdict_of(&v);
    assert!(
        line.contains("John Doe (user)  create"),
        "the who line names the create: {line}"
    );
}

#[test]
fn verify_who_encodes_a_time_with_an_offset_in_the_query() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded =
        ops::snapshot_from_dir(&dir.path().join("a"), "a", "2026-09-17T09:30:00+00:00").unwrap();
    let key = profile_audits("2026-09-17T09:30:00%2B00:00");
    let path = key.as_str();
    let api =
        serve_fixture(MemoryApi::new(), dir.path(), "b", UID_A).with(path, audits_page(vec![], ""));
    ops::verify_with(
        &recorded,
        &Palette::new(&api),
        "t1",
        &ops::VerifyOptions {
            who: true,
            ..Default::default()
        },
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(api.calls(path), 1);
}

#[test]
fn verify_who_names_the_last_write_on_the_summary_line_of_a_drifted_record() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let api = serve_fixture(MemoryApi::new(), dir.path(), "b", UID_A).with(
        profile_audits("t0"),
        audits_page(
            vec![audit(
                "au1",
                "2026-09-18T14:02:11.482Z",
                person(),
                "update",
                None,
            )],
            "",
        ),
    );
    let options = ops::VerifyOptions {
        who: true,
        ..Default::default()
    };
    let s = common::verify_summary_with(&recorded, &api, &options);
    let text = common::terminal().render_verify_summary(&s).unwrap();
    assert!(
        text.contains("; last write: John Doe (user), update at 2026-09-18T14:02:11.482Z\n"),
        "{text}"
    );
}

/// `audit`, with the object of the write in `spec.resource`, and the note of a person.
fn audit_of(uid: &str, time: &str, resource: Value, user_msg: Option<&str>) -> Value {
    let mut e = audit(uid, time, person(), "update", None);
    e["spec"]["resource"] = resource;
    if let Some(m) = user_msg {
        e["spec"]["userMsg"] = Value::from(m);
    }
    e
}

#[test]
fn verify_who_reads_the_log_of_a_cluster_group_as_clustergroup_and_of_an_app_profile_as_appprofile()
{
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let options = ops::VerifyOptions {
        who: true,
        ..Default::default()
    };

    // a cluster group whose pin was edited after the record
    let group_api = serve_cluster_group(MemoryApi::new(), dir.path());
    let group = common::snapshot_of(&group_api, "clustergroup:g1").unwrap();
    let mut doc = group_api.get_json(&group_route("g1")).unwrap();
    doc["spec"]["clusterProfileTemplates"][0]["packs"][0]["values"] = Value::from("edited: true\n");
    let group_log = "/v1/audits?resourceKind=clustergroup&resourceUid=g1&startTime=t0&limit=100";
    let edited = group_api.clone().with(group_route("g1"), doc).with(
        group_log,
        audits_page(
            vec![audit_of(
                "au7",
                "2026-09-18T14:02:11.482Z",
                serde_json::json!({"kind": "clustergroup", "name": "group-1", "uid": "g1"}),
                None,
            )],
            "",
        ),
    );
    let v = ops::verify_with(
        &group,
        &Palette::new(&edited),
        "t1",
        &options,
        &Progress::silent(),
    )
    .unwrap();
    assert!(!v.unchanged, "the edit is a drift");
    assert_eq!(edited.calls(group_log), 1);
    assert_eq!(
        edited.calls("/v1/audits?resourceUid=g1&startTime=t0&limit=100"),
        0,
        "a group is not read as a cluster, by its uid alone"
    );
    let line = verdict_of(&v);
    assert!(
        line.ends_with(
            "\nwho: 1 write in the audit log since the record\n  \
             2026-09-18T14:02:11.482Z  John Doe (user)  update"
        ),
        "the write to the group itself names no object: {line}"
    );

    // an app profile whose tier was edited after the record
    let app = snapshot_of(
        &serve_app_profile(MemoryApi::new(), "2"),
        "appprofile:shop@1.0.0",
    )
    .unwrap();
    let app_log = "/v1/audits?resourceKind=appprofile&resourceUid=ap1&startTime=t0&limit=100";
    let api = serve_app_profile(MemoryApi::new(), "3").with(
        app_log,
        audits_page(
            vec![audit_of(
                "au8",
                "2026-09-18T14:03:11.482Z",
                serde_json::json!({"kind": "appprofile", "name": "shop", "uid": "ap1"}),
                Some("CHG-42: three replicas"),
            )],
            "",
        ),
    );
    let v = ops::verify_with(
        &app,
        &Palette::new(&api),
        "t1",
        &options,
        &Progress::silent(),
    )
    .unwrap();
    assert!(!v.unchanged, "the edit is a drift");
    assert_eq!(api.calls(app_log), 1);
    assert_eq!(
        api.calls("/v1/audits?resourceKind=clusterprofile&resourceUid=ap1&startTime=t0&limit=100"),
        0,
        "an app profile is not a cluster profile in the log"
    );
    let line = verdict_of(&v);
    assert!(
        line.ends_with(
            "\n  2026-09-18T14:03:11.482Z  John Doe (user)  update; note: CHG-42: three replicas"
        ),
        "{line}"
    );
    let json = serde_json::to_value(&v.diff).unwrap();
    assert_eq!(
        json["verify"]["who"][0]["userMsg"],
        "CHG-42: three replicas"
    );
    assert_eq!(
        json["verify"]["who"][0]["resource"],
        serde_json::json!({"kind": "appprofile", "name": "shop", "uid": "ap1"})
    );
}

#[test]
fn verify_who_of_a_cluster_record_names_the_write_that_overwrote_the_profile_it_runs() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-a"),
        None,
    )
    .unwrap();
    // a legacy version bump took the uid of the version that the cluster runs
    let mut bumped = read_json(&dir.path().join("a/profile.json"));
    bumped["spec"]["version"] = Value::from("1.3.0");
    let cluster_log = cluster_audits("t0");
    let profile_log = profile_audits("t0");
    // an entry of the system that names no resource
    let mut bare = audit("au3", "2026-09-18T14:09:00.000Z", system(), "update", None);
    bare["spec"].as_object_mut().unwrap().remove("resource");
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a")
        .with(profile_route(UID_A), bumped)
        .with(
            cluster_log.clone(),
            audits_page(
                vec![audit_of(
                    "au2",
                    "2026-09-18T14:05:00.000Z",
                    serde_json::json!({"kind": "spectrocluster", "name": "lab-cluster",
                        "uid": CLUSTER_UID}),
                    None,
                )],
                "",
            ),
        )
        // Palette lists the newest first
        .with(
            profile_log.clone(),
            audits_page(
                vec![
                    bare,
                    audit_of(
                        "au1",
                        "2026-09-18T14:02:11.482Z",
                        serde_json::json!({"kind": "clusterprofile", "label": "Profile",
                            "name": "lab-profile", "uid": UID_A}),
                        None,
                    ),
                ],
                "",
            ),
        );
    let v = ops::verify_with(
        &recorded,
        &Palette::new(&api),
        "t1",
        &ops::VerifyOptions {
            who: true,
            ..Default::default()
        },
        &Progress::silent(),
    )
    .unwrap();
    assert!(!v.unchanged);
    assert_eq!((api.calls(&cluster_log), api.calls(&profile_log)), (1, 1));
    let line = verdict_of(&v);
    assert!(
        line.ends_with(
            "\nwho: 3 writes in the audit log since the record\n  \
             2026-09-18T14:02:11.482Z  John Doe (user)  update clusterprofile lab-profile\n  \
             2026-09-18T14:05:00.000Z  John Doe (user)  update\n  \
             2026-09-18T14:09:00.000Z  system (service)  update clusterprofile lab-profile"
        ),
        "the writes of both logs, oldest first, each to the profile named: {line}"
    );
    let json = serde_json::to_value(&v.diff).unwrap();
    assert_eq!(
        json["verify"]["who"][0]["resource"],
        serde_json::json!({"kind": "clusterprofile", "name": "lab-profile", "uid": UID_A})
    );
    assert_eq!(
        json["verify"]["who"][2]["resource"],
        serde_json::json!({"kind": "clusterprofile", "name": "lab-profile", "uid": UID_A}),
        "strata names the profile when the entry names no resource"
    );
    assert!(json["verify"]["who"][1].get("userMsg").is_none());
}

fn who_options() -> ops::VerifyOptions {
    ops::VerifyOptions {
        who: true,
        ..Default::default()
    }
}

#[test]
fn verify_who_keeps_the_verdict_when_palette_refuses_the_audit_read_with_403_or_500() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let denied = r#"{"code":"AccessDenied","message":"no permission"}"#;
    let fault = r#"{"code":"InternalError"}"#;
    for (status, body) in [(403, denied), (500, fault)] {
        // Palette ends a body with a newline: the reason has none
        let api = serve_fixture(MemoryApi::new(), dir.path(), "b", UID_A).failing(
            profile_audits("t0"),
            status,
            format!("{body}\n"),
        );
        let v = ops::verify_with(
            &recorded,
            &Palette::new(&api),
            "t1",
            &who_options(),
            &Progress::silent(),
        )
        .unwrap();
        let reason = format!("HTTP {status}: {body}");
        assert!(!v.unchanged, "{status}");
        assert_eq!(v.status, VerifyStatus::Drifted, "{status}");
        let ver = v.diff.verify.as_ref().unwrap();
        assert_eq!(ver.status, VerifyStatus::Drifted, "{status}");
        assert!(ver.who.is_none(), "{status}");
        assert_eq!(ver.who_error.as_deref(), Some(reason.as_str()));
        let line = verdict_of(&v);
        assert!(line.starts_with("DRIFT: "), "{line}");
        assert!(
            line.ends_with(&format!("\nwho: not read: {reason}")),
            "the verdict ends with the one line: {line}"
        );
        let markdown = common::markdown().verdict(&v.diff).unwrap();
        assert!(
            markdown.contains(&format!("\n\nwho: not read: {reason}")),
            "{markdown}"
        );
        let json = serde_json::to_value(&v.diff).unwrap();
        assert_eq!(json["verify"]["whoError"], reason.as_str());
        assert_eq!(json["verify"]["status"], "drifted");
        assert!(json["verify"].get("who").is_none(), "{json}");
    }
}

#[test]
fn verify_who_counts_a_drifted_record_with_a_refused_audit_read_as_drifted_over_several_records() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let denied = r#"{"code":"AccessDenied"}"#;
    let api = serve_fixture(MemoryApi::new(), dir.path(), "b", UID_A).failing(
        profile_audits("t0"),
        403,
        denied,
    );
    let s = common::verify_summary_with(&recorded, &api, &who_options());
    assert_eq!(s.summary.status, ops::RecordStatus::Drifted);
    assert_eq!((s.summary.drifted, s.summary.errors), (1, 0));
    assert_eq!(
        s.exit_code(),
        1,
        "drift is exit 1, and not the exit 2 of an error"
    );
    let record = &s.records[0];
    assert_eq!(record.status, ops::RecordStatus::Drifted);
    assert!(record.error.is_none(), "{:?}", record.error);
    let text = common::terminal().render_verify_summary(&s).unwrap();
    assert!(
        text.starts_with("DRIFT  ")
            && text.contains(&format!("; who: not read: HTTP 403: {denied}\n")),
        "{text}"
    );
    let json = serde_json::to_value(&s).unwrap();
    assert_eq!(
        json["records"][0]["diff"]["verify"]["whoError"],
        format!("HTTP 403: {denied}").as_str()
    );
}

#[test]
fn verify_who_drops_the_writes_already_read_when_the_log_of_an_overwritten_profile_fails() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-a"),
        None,
    )
    .unwrap();
    // a legacy version bump took the uid of the version that the cluster runs
    let mut bumped = read_json(&dir.path().join("a/profile.json"));
    bumped["spec"]["version"] = Value::from("1.3.0");
    let cluster_log = cluster_audits("t0");
    let profile_log = profile_audits("t0");
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a")
        .with(profile_route(UID_A), bumped)
        .with(
            cluster_log.clone(),
            audits_page(
                vec![audit_of(
                    "au2",
                    "2026-09-18T14:05:00.000Z",
                    serde_json::json!({"kind": "spectrocluster", "name": "lab-cluster",
                        "uid": CLUSTER_UID}),
                    None,
                )],
                "",
            ),
        )
        .failing(profile_log.clone(), 500, r#"{"code":"InternalError"}"#);
    let v = ops::verify_with(
        &recorded,
        &Palette::new(&api),
        "t1",
        &who_options(),
        &Progress::silent(),
    )
    .unwrap();
    // the log of the cluster worked, and the log of the profile did not
    assert_eq!((api.calls(&cluster_log), api.calls(&profile_log)), (1, 1));
    assert_eq!(v.status, VerifyStatus::Drifted);
    let ver = v.diff.verify.as_ref().unwrap();
    assert!(
        ver.who.is_none(),
        "a part of the list reads as the whole list"
    );
    assert_eq!(
        ver.who_error.as_deref(),
        Some(r#"HTTP 500: {"code":"InternalError"}"#)
    );
    assert!(verdict_of(&v).ends_with("\nwho: not read: HTTP 500: {\"code\":\"InternalError\"}"));
}

#[test]
fn verify_who_names_a_request_that_does_not_complete_as_the_reason_and_keeps_the_verdict() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let live = serve_fixture(MemoryApi::new(), dir.path(), "b", UID_A);
    let path = profile_audits("t0");
    let v = ops::verify_with(
        &recorded,
        &Palette::new(AuditsDropped(live, path.clone())),
        "t1",
        &who_options(),
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(v.status, VerifyStatus::Drifted);
    let ver = v.diff.verify.as_ref().unwrap();
    assert!(ver.who.is_none());
    assert_eq!(
        ver.who_error.as_deref(),
        Some(format!("GET {path}: connection reset").as_str())
    );
}

/// A Palette whose audit log at `.1` drops the connection.
struct AuditsDropped(MemoryApi, String);

impl strata::api::PaletteApi for AuditsDropped {
    fn get_json(&self, path: &str) -> strata::Result<Value> {
        if path == self.1 {
            return Err(Error::Transport {
                method: Method::Get,
                path: path.to_string(),
                message: "connection reset".into(),
            });
        }
        self.0.get_json(path)
    }
}

#[test]
fn verify_gives_the_time_palette_last_changed_the_object_and_a_drift_verdict_ends_with_it() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let with_time = |from: &str| {
        let mut doc = read_json(&dir.path().join(from).join("profile.json"));
        doc["metadata"]["lastModifiedTimestamp"] = Value::from("2026-09-18T14:02:11.482Z");
        serve_fixture(MemoryApi::new(), dir.path(), from, UID_A).with(profile_route(UID_A), doc)
    };

    // the document carries the time: no request more
    let api = with_time("b");
    let v = ops::verify(&recorded, &Palette::new(&api), "t1", &Progress::silent()).unwrap();
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(
        ver.last_modified.as_deref(),
        Some("2026-09-18T14:02:11.482Z")
    );
    let line = verdict_of(&v);
    assert!(
        line.ends_with("; Palette last changed it 2026-09-18T14:02:11.482Z"),
        "{line}"
    );
    let json = serde_json::to_value(&v.diff).unwrap();
    assert_eq!(json["verify"]["lastModified"], "2026-09-18T14:02:11.482Z");

    // an unchanged record gives the time too, and its verdict does not end with it
    let v = ops::verify(
        &recorded,
        &Palette::new(with_time("a")),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(v.unchanged);
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(
        ver.last_modified.as_deref(),
        Some("2026-09-18T14:02:11.482Z")
    );
    assert!(verdict_of(&v).ends_with("is unchanged since t0"));

    // each record of a summary gives it
    let s = common::verify_summary(&recorded, &api);
    let json = serde_json::to_value(&s).unwrap();
    assert_eq!(
        json["records"][0]["lastModified"],
        "2026-09-18T14:02:11.482Z"
    );

    // a document with no time gives none
    let v = ops::verify(
        &recorded,
        &Palette::new(serve_fixture(MemoryApi::new(), dir.path(), "b", UID_A)),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(v.diff.verify.as_ref().unwrap().last_modified.is_none());
    assert!(!verdict_of(&v).contains("last changed"));
}

// ---- the updates that wait for a cluster -----------------------------------------------------

/// The notifications of the cluster fixture: the updates that wait for it.
fn notifications_route() -> String {
    format!("/v1/notifications/spectrocluster/{CLUSTER_UID}")
}

/// One notification of a profile update, in the shape Palette gives it.
fn notification(uid: &str, at: &str, done: bool, profiles: &[(&str, &str)]) -> Value {
    let events: serde_json::Map<String, Value> = profiles
        .iter()
        .enumerate()
        .map(|(i, (profile_uid, message))| {
            (
                format!("e{i}"),
                serde_json::json!({"profileUid": profile_uid, "message": message, "type": "PackManifestUpdate"}),
            )
        })
        .collect();
    serde_json::json!({
        "metadata": {"uid": uid, "creationTimestamp": at},
        "type": "NotificationPackUpdate",
        "action": {"actionType": "NotifyActionClusterProfileUpdate", "isDone": done, "events": events},
    })
}

fn notifications_page(items: Vec<Value>, next: &str) -> Value {
    let items = if items.is_empty() {
        Value::Null // Palette gives null, not [], when a cluster has none
    } else {
        Value::Array(items)
    };
    serde_json::json!({"items": items, "listmeta": {"continue": next}})
}

/// The uids of the two profiles that cluster-merged runs: the cluster profile, then the add-on.
fn merged_uids(dir: &Path) -> (String, String) {
    let recorded = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir, "cluster-merged"),
        None,
    )
    .unwrap();
    let p = &recorded.source.cluster.unwrap().profiles;
    (p[0].uid.clone(), p[1].uid.clone())
}

#[test]
fn a_cluster_record_says_which_update_waits_from_the_open_notifications() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let (infra, addon) = merged_uids(dir.path());
    // newest first, on two pages; a done notification and one of another profile are not pending
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-merged")
        .with(
            notifications_route(),
            notifications_page(
                vec![notification(
                    "n3",
                    "2026-09-24T23:55:02.083Z",
                    false,
                    &[(&addon, "Manifest 'm' is updated in the pack 'm(m)'")],
                )],
                "tok",
            ),
        )
        .with(
            format!("{}?continue=tok", notifications_route()),
            notifications_page(
                vec![
                    notification(
                        "n2",
                        "2026-09-24T23:50:00.000Z",
                        false,
                        &[("gone-uid", "x")],
                    ),
                    notification("n1", "2026-09-24T23:48:38.990Z", true, &[(&addon, "old")]),
                ],
                "",
            ),
        );
    let all = snapshot_cluster(&api, None).unwrap();
    let c = all.source.cluster.as_ref().unwrap();
    assert_eq!(c.updates_pending, Some(true));
    assert_eq!(c.updates_pending_error(), None);
    assert_eq!(
        c.pending_updates
            .iter()
            .map(|u| (
                u.profile_uid.as_str(),
                u.profile.as_str(),
                u.message.as_str(),
                u.since.as_str()
            ))
            .collect::<Vec<_>>(),
        [(
            addon.as_str(),
            "lab-addons",
            "Manifest 'm' is updated in the pack 'm(m)'",
            "2026-09-24T23:55:02.083Z"
        )]
    );
    // the statement is outside the root: the copy is the same with or without it
    let quiet = serve_cluster(MemoryApi::new(), dir.path(), "cluster-merged")
        .with(notifications_route(), notifications_page(vec![], ""));
    let none = snapshot_cluster(&quiet, None).unwrap();
    assert_eq!(none.root, all.root);
    assert_eq!(
        none.source.cluster.as_ref().unwrap().updates_pending,
        Some(false)
    );
    // a record of the other profile alone has no update waiting
    let infra_only = snapshot_cluster(&api, Some(&infra)).unwrap();
    assert_eq!(
        infra_only.source.cluster.unwrap().updates_pending,
        Some(false)
    );

    let json = serde_json::to_value(&all).unwrap();
    assert_eq!(json["source"]["cluster"]["updatesPending"], true);
    assert_eq!(
        json["source"]["cluster"]["pendingUpdates"][0]["profile"],
        "lab-addons"
    );
}

#[test]
fn a_cluster_record_says_when_it_could_not_read_whether_an_update_waits() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    // no notifications path: the in-memory API answers 404, and the record is still taken
    let recorded = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-a"),
        None,
    )
    .unwrap();
    let c = recorded.source.cluster.as_ref().unwrap();
    assert_eq!(c.updates_pending, None);
    assert!(
        c.updates_pending_error().unwrap().starts_with("HTTP 404"),
        "{:?}",
        c.updates_pending_error()
    );
    let json = serde_json::to_value(&recorded).unwrap();
    assert!(json["source"]["cluster"].get("updatesPending").is_none());
    assert_eq!(
        json["source"]["cluster"]["errors"]["updatesPending"],
        c.updates_pending_error().unwrap()
    );
    assert!(
        json["source"]["cluster"]
            .get("updatesPendingError")
            .is_none()
    );
}

#[test]
fn a_raw_cluster_directory_keeps_the_notifications() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let (_, addon) = merged_uids(dir.path());
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-merged").with(
        notifications_route(),
        notifications_page(vec![notification("n1", "t", false, &[(&addon, "m")])], ""),
    );
    let saved = dir.path().join("raw");
    Palette::new(api)
        .fetch_subject(SubjectKind::Cluster, CLUSTER_UID, &Progress::silent())
        .unwrap()
        .save_dir(&saved, "t0")
        .unwrap();
    let loaded = strata::api::RawCluster::load_dir(&saved).unwrap();
    let snap = Snapshot::from_cluster(&loaded, None, "raw", "t0").unwrap();
    assert_eq!(snap.source.cluster.unwrap().updates_pending, Some(true));
}

/// A notification whose one event changes the values of a pack, with the fields that Palette
/// writes for it: the full values, and the names of the password fields.
fn values_notification(addon: &str) -> Value {
    serde_json::json!({
        "metadata": {"uid": "n1", "creationTimestamp": "2026-09-24T23:55:02.083Z"},
        "type": "NotificationPackUpdate",
        "action": {
            "actionType": "NotifyActionClusterProfileUpdate",
            "isDone": false,
            "events": {"e0": {
                "profileUid": addon,
                "packName": "csi-aws-ebs",
                "type": "PackValuesUpdate",
                "message": "csi-aws-ebs 1.30.0 values are updated",
                "packValues": "pack:\n  adminPassword: barney-clear-secret\n  replicas: 2\n",
                "passwordFields": "pack.adminPassword",
                "appTierValues": "",
            }},
        },
    })
}

#[test]
fn a_raw_cluster_directory_masks_the_values_of_each_pack_in_the_notifications() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let (_, addon) = merged_uids(dir.path());
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-merged").with(
        notifications_route(),
        notifications_page(vec![values_notification(&addon)], ""),
    );
    let saved = dir.path().join("raw");
    Palette::new(api)
        .fetch_subject(SubjectKind::Cluster, CLUSTER_UID, &Progress::silent())
        .unwrap()
        .save_dir(&saved, "t0")
        .unwrap();
    let text = fs::read_to_string(saved.join("notifications.json")).unwrap();
    assert!(!text.contains("barney-clear-secret"), "{text}");
    assert!(!text.contains("replicas"), "{text}");
    let saved: Value = serde_json::from_str(&text).unwrap();
    let event = &saved[0]["action"]["events"]["e0"];
    assert_eq!(event["packValues"], "********");
    // the names of the fields are no secret, and an empty value stays empty
    assert_eq!(event["passwordFields"], "pack.adminPassword");
    assert_eq!(event["appTierValues"], "");
    assert_eq!(event["message"], "csi-aws-ebs 1.30.0 values are updated");
}

#[test]
fn a_record_from_a_raw_directory_with_masked_notifications_is_the_record_of_the_fetch() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let (_, addon) = merged_uids(dir.path());
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-merged").with(
        notifications_route(),
        notifications_page(vec![values_notification(&addon)], ""),
    );
    let fetched = Palette::new(api)
        .fetch_subject(SubjectKind::Cluster, CLUSTER_UID, &Progress::silent())
        .unwrap();
    let saved = dir.path().join("raw");
    fetched.save_dir(&saved, "t0").unwrap();
    let direct = Snapshot::from_cluster(&fetched, None, "raw", "t0").unwrap();
    let from_raw = ops::snapshot_from_dir(&saved, "raw", "t1").unwrap();
    assert_eq!(
        serde_json::to_value(&from_raw).unwrap(),
        serde_json::to_value(&direct).unwrap()
    );
    let c = from_raw.source.cluster.unwrap();
    assert_eq!(c.updates_pending, Some(true));
    assert_eq!(
        c.pending_updates[0].message,
        "csi-aws-ebs 1.30.0 values are updated"
    );
}

#[test]
fn list_and_compare_name_a_cluster_that_is_behind() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let (_, addon) = merged_uids(dir.path());
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-merged").with(
        notifications_route(),
        notifications_page(
            vec![notification(
                "n1",
                "2026-09-24T23:55:02.083Z",
                false,
                &[(&addon, "m")],
            )],
            "",
        ),
    );
    let before = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-merged")
            .with(notifications_route(), notifications_page(vec![], "")),
        None,
    )
    .unwrap();
    let now = snapshot_cluster(&api, None).unwrap();
    let d = ops::compare(&before, &now, "t1");
    let text = common::terminal().render(&d, &before, &now, false).unwrap();
    assert!(
        text.contains(
            "note       cluster lab-cluster is behind: Palette says an update of lab-addons waits for it, since 2026-09-24T23:55:02.083Z\n"
        ),
        "{text}"
    );
    let md = common::markdown().render(&d, &before, &now, false).unwrap();
    assert!(
        md.contains("**Note:** cluster lab-cluster is behind"),
        "{md}"
    );
    // a side read when nothing waited gets no note
    let d = ops::compare(&before, &before, "t1");
    let text = common::terminal()
        .render(&d, &before, &before, false)
        .unwrap();
    assert!(!text.contains("is behind"), "{text}");
}

/// The repave status of the cluster fixture.
fn repave_route() -> String {
    cluster_repave_route(CLUSTER_UID)
}

/// The route that asks Palette if a change of the cluster fixture repaves it.
fn validate_repave_route() -> String {
    format!("{}/validate/repave", cluster_route(CLUSTER_UID))
}

/// The route that asks Palette if it takes the packs of a change of the cluster fixture.
fn validate_packs_route() -> String {
    format!("{}/validate/packs", cluster_route(CLUSTER_UID))
}

/// The update that waits for the cluster fixture.
fn profile_updates_route() -> String {
    format!("{}/profileUpdates", cluster_route(CLUSTER_UID))
}

/// The repave of a macro change in a field that repaves, in the shape that Palette gives it. Each
/// side of the reason holds the resolved values of the pack.
fn repave_status(state: &str) -> Value {
    serde_json::json!({
        "metadata": {"uid": "r1", "creationTimestamp": "2026-09-25T03:05:12Z"},
        "spec": {
            "spectroClusterUid": CLUSTER_UID,
            "source": "hubble",
            "reasons": [{
                "code": "PackValuesUpdated",
                "message": "Pack 'kubernetes' values are updated",
                "pack": {
                    "current": {"name": "kubernetes", "values": "postKubeadmCommands:\n- echo barney-one"},
                    "target": {"name": "kubernetes", "values": "postKubeadmCommands:\n- echo barney-two"},
                    "diffConfigKeys": ["kubeadmconfig.postKubeadmCommands"],
                },
            }],
        },
        "status": {"state": state, "repaveTransitionTime": "2026-09-25T03:20:10Z"},
    })
}

/// The cluster of `dir` with a repave in `state`, the route of that repave, and no notification.
fn serve_repave(raw_root: &Path, dir: &str, state: &str) -> MemoryApi {
    let mut cluster = read_json(&raw_root.join(dir).join("cluster.json"));
    cluster["status"]["repave"] = serde_json::json!({"state": state});
    serve_cluster(MemoryApi::new(), raw_root, dir)
        .with(cluster_route(CLUSTER_UID), cluster)
        .with(repave_route(), repave_status(state))
        .with(notifications_route(), notifications_page(vec![], ""))
}

#[test]
fn a_cluster_record_holds_the_repave_that_waits_for_approval_and_its_reasons() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let quiet = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-merged"),
        None,
    )
    .unwrap();
    let waits =
        snapshot_cluster(&serve_repave(dir.path(), "cluster-merged", "Pending"), None).unwrap();
    // outside the root: the copy that the cluster runs is the same
    assert_eq!(waits.root, quiet.root);

    let c = waits.source.cluster.as_ref().unwrap();
    assert_eq!(c.repave_pending, Some(true));
    let r = c.pending_repave.as_ref().unwrap();
    assert_eq!(
        (r.source.as_str(), r.since.as_str()),
        ("hubble", "2026-09-25T03:20:10Z")
    );
    assert_eq!(r.reasons.len(), 1);
    assert_eq!(r.reasons[0].code, "PackValuesUpdated");
    assert_eq!(r.reasons[0].pack, "kubernetes");
    assert_eq!(r.reasons[0].keys, ["kubeadmconfig.postKubeadmCommands"]);
    // the record names the keys, and holds no value of either side
    let text = serde_json::to_string(&waits).unwrap();
    assert!(
        !text.contains("barney-one") && !text.contains("barney-two"),
        "{text}"
    );

    // no repave waits: false, and no repave in the record
    let c = quiet.source.cluster.as_ref().unwrap();
    assert_eq!(c.repave_pending, Some(false));
    assert!(c.pending_repave.is_none());
    let json = serde_json::to_value(&quiet).unwrap();
    assert_eq!(json["source"]["cluster"]["repavePending"], false);
    assert!(json["source"]["cluster"].get("pendingRepave").is_none());

    // an approved repave waits for nothing, and strata does not read its route
    let approved = snapshot_cluster(
        &serve_repave(dir.path(), "cluster-merged", "Approved"),
        None,
    )
    .unwrap();
    let c = approved.source.cluster.unwrap();
    assert_eq!(c.repave_pending, Some(false));
    assert!(c.pending_repave.is_none());
}

#[test]
fn mark_pending_and_with_kind_clear_the_pending_repave() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let waits =
        snapshot_cluster(&serve_repave(dir.path(), "cluster-merged", "Pending"), None).unwrap();
    assert!(
        waits
            .source
            .cluster
            .as_ref()
            .unwrap()
            .pending_repave
            .is_some()
    );

    // the profiles of a pending update say nothing about a repave of the cluster as it runs
    let pending = waits.clone().mark_pending();
    let c = pending.source.cluster.as_ref().unwrap();
    assert_eq!(c.repave_pending, None);
    assert!(c.pending_repave.is_none());

    // Palette repaves only a cluster
    for kind in [
        strata::snapshot::SubjectKind::EdgeHost,
        strata::snapshot::SubjectKind::ClusterGroup,
    ] {
        let other = waits.clone().with_kind(kind);
        let c = other.source.cluster.as_ref().unwrap();
        assert_eq!(c.repave_pending, None, "{kind:?}");
        assert!(c.pending_repave.is_none(), "{kind:?}");
    }
}

#[test]
fn a_cluster_record_holds_the_error_of_the_route_of_the_repave_and_verify_names_it() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let before = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-merged"),
        None,
    )
    .unwrap();
    let api = serve_repave(dir.path(), "cluster-merged", "Pending").failing(
        repave_route(),
        403,
        "forbidden",
    );
    let waits = snapshot_cluster(&api, None).unwrap();
    let c = waits.source.cluster.as_ref().unwrap();
    assert_eq!(c.repave_pending, Some(true), "the cluster document says it");
    assert!(c.pending_repave.is_none());
    assert_eq!(c.pending_repave_error(), Some("HTTP 403: forbidden"));
    let json = serde_json::to_value(&waits).unwrap();
    assert_eq!(
        json["source"]["cluster"]["errors"]["pendingRepave"],
        "HTTP 403: forbidden"
    );
    assert!(
        json["source"]["cluster"]
            .get("pendingRepaveError")
            .is_none()
    );

    let v = ops::verify(&before, &Palette::new(api), "t1", &Progress::silent()).unwrap();
    assert_eq!(v.status, VerifyStatus::Unchanged);
    let text = common::terminal()
        .render(&v.diff, &before, &v.live, false)
        .unwrap();
    let note = "a repave of cluster lab-cluster waits for approval (strata could not read its \
                reasons: HTTP 403: forbidden). Palette refuses an update of its profiles until a \
                person approves it";
    assert!(text.contains(&format!("note       {note}\n")), "{text}");

    // the error says nothing about the profiles of a pending update, or of another kind
    let pending = waits.clone().mark_pending();
    assert!(pending.source.cluster.unwrap().errors.is_empty());
    let host = waits.with_kind(strata::snapshot::SubjectKind::EdgeHost);
    assert!(
        host.source
            .cluster
            .unwrap()
            .pending_repave_error()
            .is_none()
    );
}

/// The record `current` as strata 0.24 and before wrote it: the read errors of the cluster are in
/// `updatesPendingError` and `pendingRepaveError`, and there is no `errors`.
fn as_written_by_0_24(current: &Snapshot) -> Value {
    let mut older = serde_json::to_value(current).unwrap();
    let cluster = older["source"]["cluster"].as_object_mut().unwrap();
    let errors = cluster.remove("errors").unwrap();
    cluster.insert(
        "updatesPendingError".into(),
        errors["updatesPending"].clone(),
    );
    cluster.insert("pendingRepaveError".into(), errors["pendingRepave"].clone());
    older
}

/// Strata 0.24 and before wrote why it could not read the notifications and the repave in
/// `updatesPendingError` and `pendingRepaveError`. A record now holds both in `errors`, as a tier
/// does, and an older record gives the same answer.
#[test]
fn an_older_record_with_the_read_errors_of_a_cluster_in_two_fields_loads_into_errors() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_repave(dir.path(), "cluster-merged", "Pending")
        .failing(repave_route(), 403, "forbidden")
        .failing(notifications_route(), 404, "not found");
    let current = snapshot_cluster(&api, None).unwrap();
    let c = current.source.cluster.as_ref().unwrap();
    assert!(c.updates_pending_error().is_some() && c.pending_repave_error().is_some());

    let saved = dir.path().join("older.json");
    write_record(&saved, &as_written_by_0_24(&current));

    let loaded = Snapshot::load(&saved).unwrap();
    assert_eq!(
        serde_json::to_value(&loaded).unwrap()["source"]["cluster"],
        serde_json::to_value(&current).unwrap()["source"]["cluster"],
        "the record is the record that strata writes now"
    );

    // a program that reads the record with serde, and not with `load`, gets the same errors
    let by_serde: Snapshot = serde_json::from_str(&fs::read_to_string(&saved).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(&by_serde).unwrap()["source"]["cluster"]["errors"],
        serde_json::to_value(&current).unwrap()["source"]["cluster"]["errors"],
    );
    let c = by_serde.source.cluster.as_ref().unwrap();
    assert_eq!(c.pending_repave_error(), Some("HTTP 403: forbidden"));
    assert_eq!(c.updates_pending_error(), Some("HTTP 404: not found"));
    let c = loaded.source.cluster.as_ref().unwrap();
    assert_eq!(c.pending_repave_error(), Some("HTTP 403: forbidden"));
    assert_eq!(c.updates_pending_error(), Some("HTTP 404: not found"));

    // and verify says the same about it
    let verified = |recorded: &Snapshot| {
        let v = ops::verify(
            recorded,
            &Palette::new(api.clone()),
            "t1",
            &Progress::silent(),
        )
        .unwrap();
        common::terminal()
            .render(&v.diff, recorded, &v.live, false)
            .unwrap()
    };
    let text = verified(&loaded);
    assert!(
        text.contains("(strata could not read its reasons: HTTP 403: forbidden)"),
        "{text}"
    );
    assert_eq!(text, verified(&current));
}

/// The record of a cluster is also in `strata-diff/1` and in `strata-verify-summary/1`. Both name
/// the read errors in `errors`, also when they read an older record. Neither holds the two fields
/// of strata 0.24, so a script that reads those gets no value and no error.
#[test]
fn the_diff_and_the_summary_hold_the_read_errors_of_a_cluster_in_errors_and_not_in_two_fields() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_repave(dir.path(), "cluster-merged", "Pending")
        .failing(repave_route(), 403, "forbidden")
        .failing(notifications_route(), 404, "not found");
    let current = snapshot_cluster(&api, None).unwrap();
    let saved = dir.path().join("older.json");
    write_record(&saved, &as_written_by_0_24(&current));
    let older = Snapshot::load(&saved).unwrap();

    let in_errors = |cluster: &Value| {
        assert_eq!(
            cluster["errors"],
            serde_json::json!({
                "updatesPending": "HTTP 404: not found",
                "pendingRepave": "HTTP 403: forbidden",
            }),
            "{cluster}"
        );
        for field in ["updatesPendingError", "pendingRepaveError"] {
            assert!(cluster.get(field).is_none(), "{field}: {cluster}");
        }
    };

    // `compare --format json`: the older record is side a
    let diff = ops::compare(&older, &current, "t1");
    let json = render::renderer(Format::Json, Paint::off(), true)
        .render(&diff, &older, &current, false)
        .unwrap();
    let json: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(json["schema"], strata::diff::DIFF_SCHEMA);
    in_errors(&json["a"]["cluster"]);
    in_errors(&json["b"]["cluster"]);

    // `verify --summary --format json`: a record that drifted holds its diff in the row. Palette
    // now has the cluster on one profile, where the older record has two.
    let live = serve_repave(dir.path(), "cluster-a", "Pending")
        .failing(repave_route(), 403, "forbidden")
        .failing(notifications_route(), 404, "not found");
    let summary = serde_json::to_value(common::verify_summary(&older, &live)).unwrap();
    assert_eq!(summary["schema"], ops::VERIFY_SUMMARY_SCHEMA);
    let row = &summary["records"][0];
    assert_eq!(row["status"], "drifted", "{row}");
    in_errors(&row["diff"]["a"]["cluster"]);
    in_errors(&row["diff"]["b"]["cluster"]);
}

/// Palette shows the templates that link a version, and `consumers` reads them live. So a record
/// does not keep them, and an older record that keeps them still loads and verifies.
#[test]
fn a_profile_record_keeps_no_cluster_template_and_an_older_record_with_one_still_verifies() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let plain = common::snapshot_of(
        &serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A),
        UID_A,
    )
    .unwrap();
    let mut profile = read_json(&dir.path().join("a").join("profile.json"));
    profile["status"]["inUseClusterTemplates"] = serde_json::json!([
        {"uid": "tmpl-1", "name": "barney-template", "projectUid": "p1", "kind": "clustertemplate"},
        {"uid": "tmpl-2", "name": "fred-template", "projectUid": "p1", "kind": "clustertemplate"}
    ]);
    let api =
        serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A).with(profile_route(UID_A), profile);
    let linked = common::snapshot_of(&api, UID_A).unwrap();
    assert_eq!(linked.root, plain.root);
    let mut older = serde_json::to_value(&linked).unwrap();
    assert!(older["source"].get("clusterTemplates").is_none());

    // the record as strata 0.18 and 0.19 wrote it
    older["source"]["clusterTemplates"] = serde_json::json!([
        {"uid": "tmpl-1", "name": "barney-template", "projectUid": "p1"},
        {"uid": "tmpl-2", "name": "fred-template", "projectUid": "p1"}
    ]);
    let saved = dir.path().join("older.json");
    write_record(&saved, &older);
    let older = Snapshot::load(&saved).unwrap();
    assert_eq!(older.root, linked.root);
    let v = ops::verify(&older, &Palette::new(api), "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged);
}

#[test]
fn compare_and_verify_name_the_repave_that_waits_for_approval() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let before = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-merged"),
        None,
    )
    .unwrap();
    let api = serve_repave(dir.path(), "cluster-merged", "Pending");
    let now = snapshot_cluster(&api, None).unwrap();
    let note = "a repave of cluster lab-cluster waits for approval, since 2026-09-25T03:20:10Z: \
                PackValuesUpdated kubernetes (kubeadmconfig.postKubeadmCommands). Palette \
                refuses an update of its profiles until a person approves it";

    let d = ops::compare(&before, &now, "t1");
    assert!(d.same);
    let text = common::terminal().render(&d, &before, &now, false).unwrap();
    assert!(text.contains(&format!("note       {note}\n")), "{text}");
    let md = common::markdown().render(&d, &before, &now, false).unwrap();
    assert!(md.contains(&format!("**Note:** {note}\n")), "{md}");
    let json = render::renderer(Format::Json, Paint::off(), false)
        .render(&d, &before, &now, false)
        .unwrap();
    let json: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(json["b"]["cluster"]["repavePending"], true);
    assert_eq!(
        json["b"]["cluster"]["pendingRepave"]["reasons"][0]["pack"],
        "kubernetes"
    );

    // verify of the record taken before the repave: the content is the same, and the report
    // names the repave
    let v = ops::verify(&before, &Palette::new(api), "t1", &Progress::silent()).unwrap();
    assert_eq!(v.status, VerifyStatus::Unchanged);
    let text = common::terminal()
        .render(&v.diff, &before, &v.live, false)
        .unwrap();
    assert!(text.contains(&format!("note       {note}\n")), "{text}");

    // a side with no repave that waits gets no note, and two sides that say the same get one
    let d = ops::compare(&before, &before, "t1");
    let text = common::terminal()
        .render(&d, &before, &before, false)
        .unwrap();
    assert!(!text.contains("a repave of"), "{text}");
    let d = ops::compare(&now, &now, "t1");
    let text = common::terminal().render(&d, &now, &now, false).unwrap();
    assert_eq!(text.matches("a repave of").count(), 1, "{text}");
}

#[test]
fn a_raw_directory_keeps_the_repave_with_no_value_and_gives_the_record_of_the_fetch() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let fetched = Palette::new(serve_repave(dir.path(), "cluster-merged", "Pending"))
        .fetch_subject(SubjectKind::Cluster, CLUSTER_UID, &Progress::silent())
        .unwrap();
    let saved = dir.path().join("raw");
    fetched.save_dir(&saved, "t0").unwrap();
    let text = fs::read_to_string(saved.join("repave.json")).unwrap();
    assert!(
        !text.contains("barney-one") && !text.contains("barney-two"),
        "{text}"
    );
    let saved_doc: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        saved_doc["spec"]["reasons"][0]["pack"]["current"]["values"],
        "********"
    );
    let direct = Snapshot::from_cluster(&fetched, None, "raw", "t0").unwrap();
    let from_raw = ops::snapshot_from_dir(&saved, "raw", "t1").unwrap();
    assert_eq!(
        serde_json::to_value(&from_raw).unwrap(),
        serde_json::to_value(&direct).unwrap()
    );
    assert_eq!(
        from_raw
            .source
            .cluster
            .unwrap()
            .pending_repave
            .unwrap()
            .reasons[0]
            .pack,
        "kubernetes"
    );
}

#[test]
fn a_raw_directory_keeps_each_read_error_so_the_record_is_the_record_of_the_fetch() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let mut cluster = read_json(&dir.path().join("cluster-merged").join("cluster.json"));
    cluster["status"]["repave"] = serde_json::json!({"state": "Pending"});
    cluster["spec"]["clusterConfig"] = serde_json::json!({"timezone": "Etc/UTC"});
    // Palette refuses each document beside the copy, and the record still comes
    let api = serve_repave(dir.path(), "cluster-merged", "Pending")
        .with(cluster_route(CLUSTER_UID), cluster)
        .failing(repave_route(), 403, "forbidden")
        .failing(notifications_route(), 403, "forbidden")
        .failing(rbacs_route(), 403, "forbidden")
        .failing(namespaces_route(), 403, "forbidden");
    let fetched = Palette::new(api)
        .fetch_subject(SubjectKind::Cluster, CLUSTER_UID, &Progress::silent())
        .unwrap();
    let saved = dir.path().join("raw");
    fetched.save_dir(&saved, "t0").unwrap();
    assert!(saved.join("errors.json").is_file());
    let direct = Snapshot::from_cluster(&fetched, None, "raw", "t0").unwrap();
    let from_raw = ops::snapshot_from_dir(&saved, "raw", "t1").unwrap();
    assert_eq!(
        serde_json::to_value(&from_raw).unwrap(),
        serde_json::to_value(&direct).unwrap()
    );
    let c = from_raw.source.cluster.unwrap();
    assert_eq!(c.updates_pending_error(), Some("HTTP 403: forbidden"));
    assert_eq!(c.pending_repave_error(), Some("HTTP 403: forbidden"));
    let policy = &c.tier(Tier::Policy).unwrap();
    assert_eq!(
        policy.errors.keys().collect::<Vec<_>>(),
        ["namespaces", "rbacs"]
    );
    // the tier holds no value for a field that strata could not read
    assert!(policy.fields.get("rbacs").is_none() && policy.fields.get("namespaces").is_none());

    // a fetch with no error saves no errors file
    let clean = Palette::new(serve_repave(dir.path(), "cluster-merged", "Pending"))
        .fetch_subject(SubjectKind::Cluster, CLUSTER_UID, &Progress::silent())
        .unwrap();
    let saved = dir.path().join("raw-clean");
    clean.save_dir(&saved, "t0").unwrap();
    assert!(!saved.join("errors.json").exists());
}

#[test]
fn a_refusal_to_a_role_without_cluster_get_is_terminal_so_strata_asks_once_and_records_it() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let mut cluster = read_json(&dir.path().join("cluster-merged").join("cluster.json"));
    cluster["status"]["repave"] = serde_json::json!({"state": "Pending"});
    cluster["spec"]["clusterConfig"] = serde_json::json!({"timezone": "Etc/UTC"});
    let routes = [repave_route(), rbacs_route(), namespaces_route()];
    // each route answers `body` with HTTP 500. `calls` is the count of the requests of each route
    let fetch = |body: &str| {
        let api = routes.iter().fold(
            serve_repave(dir.path(), "cluster-merged", "Pending")
                .with(cluster_route(CLUSTER_UID), cluster.clone()),
            |api, route| api.failing(route.as_str(), 500, body),
        );
        let (retry, slept) = retrying(&api, 3);
        let raw = Palette::new(retry)
            .fetch_subject(SubjectKind::Cluster, CLUSTER_UID, &Progress::silent())
            .unwrap();
        let calls: Vec<u32> = routes.iter().map(|route| api.calls(route)).collect();
        (raw, calls, slept.borrow().len())
    };
    // the answer that the code of Palette shows for each of the three routes
    let refusal = format!(
        r#"{{"code":"{OPERATION_FORBIDDEN_CODE}","message":"Operation 'cluster.get' is forbidden"}}"#
    );
    let classes = routes.clone().map(|path| {
        Error::Api {
            method: Method::Get,
            path,
            status: 500,
            body: refusal.clone(),
            retry_after: None,
        }
        .class()
    });
    assert_eq!(classes, [FailureClass::Terminal; 3]);
    let (raw, calls, slept) = fetch(&refusal);
    assert_eq!(calls, [1, 1, 1], "a refusal gets no second request");
    assert_eq!(slept, 0, "strata does not wait to ask again");
    let reason = format!("HTTP 500: {refusal}");
    let c = Snapshot::from_cluster(&raw, None, "live", "t0")
        .unwrap()
        .source
        .cluster
        .unwrap();
    assert_eq!(c.pending_repave_error(), Some(reason.as_str()));
    assert_eq!(
        c.tier(Tier::Policy).unwrap().errors,
        std::collections::BTreeMap::from([
            ("namespaces".to_string(), reason.clone()),
            ("rbacs".to_string(), reason),
        ])
    );

    // a 500 with no code of a refusal is a fault of the server, and strata asks again
    let (_, calls, slept) = fetch(r#"{"code":"InternalError"}"#);
    assert_eq!(calls, [4, 4, 4], "one request and three retries");
    assert_eq!(slept, 9);
}

// ---- a pack with no upstream digest ------------------------------------------------------------

/// Fixture `a` as `dir`, with no digest for the OS pack anywhere: not on its pack reference, and
/// not in the pack summary. Palette gives such a pack for a registry it does not sync.
fn without_digest(raw_root: &Path, dir: &str) -> Snapshot {
    edited_a(raw_root, dir, "ubuntu-aws", |pack, entry| {
        pack["digest"] = Value::from("");
        entry["spec"]["digest"] = Value::from("");
    })
}

#[test]
fn a_pack_with_no_digest_makes_verify_unverified_and_names_the_pack() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = without_digest(dir.path(), "nodigest");
    let os = recorded
        .packs
        .iter()
        .find(|p| p.name == "ubuntu-aws")
        .unwrap();
    assert_eq!(os.hash.upstream_source, UpstreamSource::NoDigest);
    assert_eq!(recorded.upstream_unknown(), ["os/ubuntu-aws"]);

    // the same content, and the empty digest on both sides: the upstream check cannot fail
    let api = serve_fixture(MemoryApi::new(), dir.path(), "nodigest", UID_A);
    let v = ops::verify(&recorded, &Palette::new(&api), "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged, "the content is the same");
    assert_eq!(v.status, VerifyStatus::Unverified);
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.upstream_unknown, ["os/ubuntu-aws"]);
    let verdict = common::terminal().verdict(&v.diff).unwrap();
    assert!(
        verdict.starts_with("UNVERIFIED: published version lab-profile@1.2.0 has the content recorded t0, but pack os/ubuntu-aws has no upstream digest, so a change of the archive does not show; --allow-no-digest accepts it"),
        "{verdict}"
    );
    let json = serde_json::to_value(&v.diff).unwrap();
    assert_eq!(json["verify"]["status"], "unverified");
    assert_eq!(json["upstreamUnknown"][0], "os/ubuntu-aws");

    // a team that accepts it gets unchanged, and the report still names the pack
    let allow = ops::VerifyOptions {
        allow_no_digest: true,
        ..Default::default()
    };
    let v = ops::verify_with(
        &recorded,
        &Palette::new(&api),
        "t1",
        &allow,
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(v.status, VerifyStatus::Unchanged);
    let text = common::terminal()
        .render(&v.diff, &recorded, &v.live, false)
        .unwrap();
    assert!(
        text.contains("note       pack os/ubuntu-aws has no upstream digest: Palette gives none, so a change of the archive does not show\n"),
        "{text}"
    );

    // drift wins: a record that also changed is drifted
    let changed = serve_fixture(MemoryApi::new(), dir.path(), "b", UID_A);
    let v = ops::verify(
        &recorded,
        &Palette::new(&changed),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(v.status, VerifyStatus::Drifted);
}

#[test]
fn each_report_states_the_verdict_of_a_record_in_the_same_words() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let base = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let write = |name: &str, snap: Snapshot| -> PathBuf {
        let p = dir.path().join(name);
        common::write_snapshot(&p, &snap, true);
        p
    };
    let same = write("same.json", base.clone().with_project(Some("p1")));
    let edited = write("edited.json", base.clone().with_project(Some("p2")));
    // project p1 still serves A's content; in p2 the live object has B's content
    let live_p1 = serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A);
    let live_p2 = serve_fixture(MemoryApi::new(), dir.path(), "b", UID_A);
    let connect = |project: Option<&str>| {
        Ok(Palette::new(match project {
            Some("p1") => live_p1.clone(),
            _ => live_p2.clone(),
        }))
    };
    let s = ops::verify_many(
        &[same, edited],
        None,
        connect,
        "t1",
        &ops::VerifyOptions::default(),
        &Progress::silent(),
    );
    let statuses: Vec<RecordStatus> = s.records.iter().map(|r| r.status).collect();
    assert_eq!(
        statuses,
        vec![RecordStatus::Unchanged, RecordStatus::Drifted]
    );

    let terminal = common::terminal();
    let markdown = common::markdown();
    let lines = terminal.render_verify_summary(&s).unwrap();
    let table = markdown.render_verify_summary(&s).unwrap();
    // a row states the verdict of its record in the words of the report of one record
    let unchanged = "is unchanged since t0";
    let drifted = "now names lab-profile@1.3.0: the version recorded t0 was overwritten in place";
    assert!(
        lines.contains(&format!(
            "OK     {}",
            dir.path().join("same.json").display()
        )),
        "{lines}"
    );
    assert!(lines.contains(unchanged), "{lines}");
    assert!(lines.contains(drifted), "{lines}");
    assert!(table.contains("| OK | "), "{table}");
    assert!(table.contains(&format!("| {unchanged} |")), "{table}");
    assert!(table.contains(&format!("| {drifted} |")), "{table}");

    // the report of one record puts the subject before the same sentence
    for (live, sentence) in [(&live_p1, unchanged), (&live_p2, drifted)] {
        let v = ops::verify(
            &base,
            &Palette::new(live.clone()),
            "t1",
            &Progress::silent(),
        )
        .unwrap();
        for r in [&terminal, &markdown] {
            let verdict = r.verdict(&v.diff).unwrap();
            assert!(
                verdict.contains(&format!("published version lab-profile@1.2.0 {sentence}")),
                "{verdict}"
            );
        }
    }
}

#[test]
fn each_report_says_in_the_same_words_that_the_values_did_not_parse() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let a = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    let d = strata::diff::compute(&a, &b, "t1", None);
    let metrics = d.packs.iter().find(|p| p.name == "metrics-server").unwrap();
    assert!(metrics.values.changed && !metrics.values.parseable);
    let sentence =
        "values changed (not parseable as YAML on one side); use --full for a unified diff";
    let text = common::terminal().render(&d, &a, &b, false).unwrap();
    assert!(text.contains(sentence), "{text}");
    let md = common::markdown().render(&d, &a, &b, false).unwrap();
    assert!(md.contains(&sentence.replace("--full", "`--full`")), "{md}");
    // same-release says it in the same words, with no flag
    let c = ops::same_release(&chain_input(dir.path(), &["a", "b"]), Policy::Exact, "t1");
    let reason = c.pairs[0]
        .reasons
        .iter()
        .find(|r| r.starts_with("pack addon/metrics-server AUTHORED: "))
        .unwrap();
    let detail = reason.trim_start_matches("pack addon/metrics-server AUTHORED: ");
    assert!(
        sentence.starts_with(&format!("{detail}; ")),
        "{detail:?} is not the start of {sentence:?}"
    );
}

#[test]
fn each_report_states_the_upstream_digest_of_a_pack_in_the_same_line() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let a = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    let d = strata::diff::compute(&a, &b, "t1", None);
    let line = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|l| l.contains("upstream digest:"))
            .map(|l| l.trim().replace('`', ""))
            .collect()
    };
    let terminal = common::terminal().render(&d, &a, &b, false).unwrap();
    let markdown = common::markdown().render(&d, &a, &b, false).unwrap();
    assert_eq!(
        line(&terminal),
        [
            "upstream digest: sha256:k8s1294 -> sha256:k8s1302 (packRef.digest)",
            "upstream digest: sha256:cni-aaaa -> sha256:cni-bbbb (packRef.digest)"
        ],
        "{terminal}"
    );
    assert_eq!(line(&markdown), line(&terminal), "{markdown}");
}

#[test]
fn same_release_names_a_manifest_as_the_pack_names_it_and_the_change_as_the_documents_do() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let mut records = chain_input(dir.path(), &["a", "b"]);
    for (_, snap) in &mut records {
        for m in snap.packs.iter_mut().flat_map(|p| p.manifests.iter_mut()) {
            if m.name == "extra-rules" {
                m.name = "ExtraRules".to_string();
            }
        }
    }
    let c = ops::same_release(&records, Policy::Exact, "t1");
    let reason = c.pairs[0]
        .reasons
        .iter()
        .find(|r| r.starts_with("pack addon/prometheus-operator AUTHORED: "))
        .unwrap();
    assert_eq!(
        reason,
        "pack addon/prometheus-operator AUTHORED: manifest ExtraRules changed"
    );
    // the markdown of the diff names the manifest and the change in the same words
    let (a, b) = (&records[0].1, &records[1].1);
    let d = strata::diff::compute(a, b, "t1", None);
    let md = common::markdown().render(&d, a, b, false).unwrap();
    assert!(md.contains("- manifest `ExtraRules` changed\n"), "{md}");
}

#[test]
fn a_verdict_names_the_object_of_a_record_as_the_heading_of_a_diff_does() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    // the update that waits for the cluster, which the cluster does not run yet
    let waiting = serve_pending(
        serve_cluster(MemoryApi::new(), dir.path(), "cluster-b"),
        dir.path(),
        "cluster-a",
        |_| {},
    )
    .with(
        profile_route(UID_A),
        read_json(&dir.path().join("a/profile.json")),
    );
    let pending = snapshot_pending(&waiting, None).unwrap();
    // a cluster that applies no profile
    let mut doc = read_json(&dir.path().join("cluster-a/cluster.json"));
    doc["spec"]["clusterProfileTemplates"] = serde_json::json!([]);
    let bare_api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a")
        .with(cluster_route(CLUSTER_UID), doc);
    let bare = snapshot_cluster(&bare_api, None).unwrap();
    assert!(
        pending
            .source
            .title()
            .ends_with("pending for cluster lab-cluster"),
        "{}",
        pending.source.title()
    );
    assert_eq!(bare.source.title(), "cluster lab-cluster");

    let terminal = common::terminal();
    let markdown = common::markdown();
    for (record, api) in [(&pending, &waiting), (&bare, &bare_api)] {
        let v = ops::verify(record, &Palette::new(api), "t1", &Progress::silent()).unwrap();
        let unchanged = format!("{} is unchanged since t0", record.source.title());
        for r in [&terminal, &markdown] {
            let verdict = r.verdict(&v.diff).unwrap();
            assert!(verdict.contains(&unchanged), "{verdict}");
            assert!(!verdict.contains("running"), "{verdict}");
        }
    }
}

#[test]
fn the_markdown_of_a_verify_states_the_status_in_the_word_of_the_verdict() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let live = serve_fixture(MemoryApi::new(), dir.path(), "b", UID_A);
    let v = ops::verify(&recorded, &Palette::new(live), "t1", &Progress::silent()).unwrap();
    assert_eq!(
        v.diff.verify.as_ref().unwrap().status,
        VerifyStatus::Drifted
    );
    let verdict = common::terminal().verdict(&v.diff).unwrap();
    let word = verdict.split(':').next().unwrap();
    assert_eq!(word, "DRIFT", "{verdict}");
    let md = common::markdown()
        .render(&v.diff, &recorded, &v.live, false)
        .unwrap();
    assert!(
        md.contains(&format!("**Verify:** `{word}` for uid")),
        "{md}"
    );
}

#[test]
fn many_records_count_an_unverified_one_and_exit_1() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = without_digest(dir.path(), "nodigest");
    let api = serve_fixture(MemoryApi::new(), dir.path(), "nodigest", UID_A);
    let s = common::verify_summary(&recorded, &api);
    assert_eq!(
        (s.records[0].status, s.summary.unverified, s.exit_code()),
        (RecordStatus::Unverified, 1, 1)
    );
    let text = common::terminal().render_verify_summary(&s).unwrap();
    assert!(text.contains("UNVERIFIED"), "{text}");
    // the row states the verdict of the record in the words of its own report
    assert!(
        text.contains("has the content recorded t0, but pack os/ubuntu-aws has no upstream digest"),
        "{text}"
    );
    assert!(
        text.contains("1 record: 0 unchanged, 0 drifted, 1 unverified, no errors"),
        "{text}"
    );
    let options = ops::VerifyOptions {
        allow_no_digest: true,
        ..Default::default()
    };
    let allowed = common::verify_summary_with(&recorded, &api, &options);
    assert_eq!(allowed.exit_code(), 0);
}

#[test]
fn compare_names_a_pack_with_no_digest_beside_its_line() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let a = without_digest(dir.path(), "nodigest");
    let b = without_digest(dir.path(), "nodigest2");
    let d = ops::compare(&a, &b, "t1");
    assert!(
        d.same,
        "equal roots: the empty digest hashes the same on both sides"
    );
    assert_eq!(d.upstream_unknown, ["os/ubuntu-aws"]);
    let os = d.packs.iter().find(|p| p.name == "ubuntu-aws").unwrap();
    assert!(os.upstream_unknown);
    let text = common::terminal().render(&d, &a, &b, false).unwrap();
    assert!(
        text.contains("upstream unknown: no digest, so a change of the archive does not show"),
        "{text}"
    );
    let md = common::markdown().render(&d, &a, &b, false).unwrap();
    assert!(
        md.contains("**Note:** pack os/ubuntu-aws has no upstream digest"),
        "{md}"
    );
    // a record with every digest names nothing
    let full = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    assert!(ops::compare(&full, &full, "t1").upstream_unknown.is_empty());
}

#[test]
fn a_manifest_pack_has_no_archive_and_is_never_upstream_unknown() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let mut recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let pack = &mut recorded.packs[0];
    pack.hash.upstream_source = UpstreamSource::NoDigest.into();
    pack.kind = "manifest".to_string();
    assert!(!pack.upstream_unknown());
    pack.kind = "helm".to_string();
    assert!(pack.upstream_unknown());
}

#[test]
fn same_release_refuses_a_pair_with_no_digest_unless_allowed() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let records = vec![
        ("a.json".to_string(), without_digest(dir.path(), "nodigest")),
        (
            "b.json".to_string(),
            without_digest(dir.path(), "nodigest2"),
        ),
    ];
    let c = ops::same_release(&records, Policy::Exact, "t1");
    assert_eq!(c.pairs[0].status, PairStatus::UnknownUpstream);
    assert!(!c.same);
    assert_eq!(c.exit_code(), 1);
    assert!(
        c.pairs[0].reasons.iter().any(|r| r
            == "pack os/ubuntu-aws has no upstream digest, so an equal root does not show an equal archive; --allow no-digest accepts it"),
        "{:?}",
        c.pairs[0].reasons
    );
    let allowed = common::same_release_offline(&records, common::no_digest_allowed(), "t1");
    assert_eq!(allowed.pairs[0].status, PairStatus::Identical);
    assert_eq!(allowed.pairs[0].upstream_unknown, ["os/ubuntu-aws"]);
    assert!(allowed.same);

    // a move from a registry that gives a digest to one that gives none: different, and it says so
    let full = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let moved = vec![("a.json".to_string(), full), records[1].clone()];
    let c = ops::same_release(&moved, Policy::Exact, "t1");
    assert_eq!(c.pairs[0].status, PairStatus::Different);
    assert!(
        c.pairs[0]
            .reasons
            .iter()
            .any(|r| r.contains("has no upstream digest")),
        "{:?}",
        c.pairs[0].reasons
    );
}

// ---- macros ------------------------------------------------------------------------------------

#[test]
fn a_macro_is_found_by_name_in_values_and_manifests_and_a_system_macro_is_not() {
    use strata::snapshot::macro_names;
    let names = macro_names(
        "region: {{.spectro.macro.region}}\nid: {{ .spectro.macro.cloudanix.partnerIdentifier }}\n\
         name: {{.spectro.system.cluster.name}}\nagain: {{.spectro.macro.region}}\nnot: {{ .Values.x }}",
    );
    assert_eq!(
        names.into_iter().collect::<Vec<_>>(),
        ["cloudanix.partnerIdentifier", "region"]
    );
    assert!(macro_names("no macro here").is_empty());
}

/// Fixture `a` as `dir`, with the kubernetes values reading a macro.
fn with_macro(raw_root: &Path, dir: &str) -> Snapshot {
    with_os_pack(raw_root, dir, |p| {
        let values = p["values"].as_str().unwrap_or_default();
        p["values"] = Value::from(format!(
            "{values}\nregion: \"{{{{.spectro.macro.region}}}}\"\n"
        ));
    })
}

#[test]
fn every_report_names_the_macros_the_content_uses() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let a = with_macro(dir.path(), "macro");
    assert_eq!(a.macros(), ["region"]);
    let d = ops::compare(&a, &a, "t1");
    assert_eq!(d.macros, ["region"]);
    let text = common::terminal().render(&d, &a, &a, false).unwrap();
    assert!(
        text.contains("note       the content uses macro region: the root covers its name and not its value, so an edit of a macro does not show\n"),
        "{text}"
    );
    let plain = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    assert!(ops::compare(&plain, &plain, "t1").macros.is_empty());
}

/// A resolvedValues answer for the one profile of cluster-a: `region` as given.
fn resolved(region: &str) -> Value {
    serde_json::json!({"profiles": [{"uid": UID_A, "resolved": {
        "{{.spectro.macro.region}}": region,
        "{{.spectro.system.cluster.name}}": "lab-cluster",
    }}]})
}

/// cluster-a as `dir`, with the values of each applied pack reading the macro `region`.
fn cluster_with_macro(raw_root: &Path, dir: &str) {
    let (from, to) = (raw_root.join("cluster-a"), raw_root.join(dir));
    fs::create_dir_all(&to).unwrap();
    for f in ["manifests.json", "variables.json"] {
        fs::copy(from.join(f), to.join(f)).unwrap();
    }
    let mut cluster = read_json(&from.join("cluster.json"));
    for t in cluster["spec"]["clusterProfileTemplates"]
        .as_array_mut()
        .unwrap()
    {
        for p in t["packs"].as_array_mut().unwrap() {
            let values = p["values"].as_str().unwrap_or_default().to_string();
            p["values"] = Value::from(format!(
                "{values}\nregion: \"{{{{.spectro.macro.region}}}}\"\n"
            ));
        }
    }
    fs::write(
        to.join("cluster.json"),
        serde_json::to_string_pretty(&cluster).unwrap(),
    )
    .unwrap();
}

#[test]
fn a_cluster_record_with_macro_digests_drifts_when_a_macro_value_moves() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    cluster_with_macro(dir.path(), "cluster-macro");
    let path = cluster_resolved_values_route(CLUSTER_UID);
    let tenant = |region: &str| {
        serve_cluster(MemoryApi::new(), dir.path(), "cluster-macro")
            .with(path.as_str(), resolved(region))
    };

    // the record holds the digest of the value, and not the value
    let mut raw = Palette::new(tenant("us-east-2"))
        .fetch_subject(SubjectKind::Cluster, CLUSTER_UID, &Progress::silent())
        .unwrap();
    raw.resolved_values = Some(resolved("us-east-2"));
    let recorded = Snapshot::from_cluster(&raw, None, "cluster:lab", "t0").unwrap();
    assert_eq!(recorded.macros(), ["region"]);
    let digests = recorded
        .source
        .cluster
        .as_ref()
        .unwrap()
        .macro_values
        .clone()
        .unwrap();
    assert!(digests["region"].starts_with("sha256:"), "{digests:?}");
    assert!(!recorded.to_json(false).unwrap().contains("us-east-2"));

    // the same value: unchanged
    let v = ops::verify(
        &recorded,
        &Palette::new(tenant("us-east-2")),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(v.status, VerifyStatus::Unchanged);

    // another value: the root is the same, and the record drifts, and the verdict names the macro
    let v = ops::verify(
        &recorded,
        &Palette::new(tenant("eu-west-1")),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(
        v.diff.same,
        "the content names the macro, so the root does not move"
    );
    assert_eq!(v.status, VerifyStatus::Drifted);
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.macros_changed, ["region"]);
    let verdict = common::terminal().verdict(&v.diff).unwrap();
    assert!(
        verdict.contains("but macro region has another value now"),
        "{verdict}"
    );
    assert_eq!(v.diff.macros_changed, ["region"]);

    // a record without the digests cannot see it: that is the hole the digests close
    let mut plain = recorded.clone();
    plain.source.cluster.as_mut().unwrap().macro_values = None;
    let v = ops::verify(
        &plain,
        &Palette::new(tenant("eu-west-1")),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(v.status, VerifyStatus::Unchanged);

    // a raw directory keeps the values, so a record rebuilt from it keeps the digests
    let saved = dir.path().join("raw-macro");
    raw.save_dir(&saved, "t0").unwrap();
    let again = ops::snapshot_from_dir(&saved, "cluster:lab", "t0").unwrap();
    assert_eq!(again.source.cluster.unwrap().macro_values, Some(digests));
}

/// The resolvedValues answer for one profile version: one map, with `region` as given.
fn profile_resolved(region: &str) -> Value {
    serde_json::json!({"resolved": {
        "{{.spectro.macro.region}}": region,
        "{{.spectro.system.cluster.name}}": "",
    }})
}

#[test]
fn a_profile_record_with_macro_digests_drifts_when_a_macro_value_moves() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    with_macro(dir.path(), "macro");
    let path = resolved_values_route(UID_A);
    let tenant = |answer: Value| {
        serve_fixture(MemoryApi::new(), dir.path(), "macro", UID_A).with(&path, answer)
    };

    // the record holds the digest of the value at the scope of the version, and not the value
    let palette = Palette::new(tenant(profile_resolved("us-east-2")));
    let mut raw = palette.fetch_profile(UID_A, &Progress::silent()).unwrap();
    raw.resolved_values = Some(palette.profile_resolved_values(UID_A).unwrap());
    let recorded = Snapshot::from_raw(&raw, "aws-stack@1.1.1", "t0").unwrap();
    let digests = recorded.source.macro_values.clone().unwrap();
    assert_eq!(digests.keys().collect::<Vec<_>>(), ["region"]);
    assert!(digests["region"].starts_with("sha256:"), "{digests:?}");
    assert_eq!(recorded.source.macro_values(), Some(&digests));
    let json = recorded.to_json(false).unwrap();
    assert!(!json.contains("us-east-2"), "{json}");
    assert!(json.contains("\"macroValues\""), "{json}");

    // the same value: unchanged
    let v = ops::verify(
        &recorded,
        &Palette::new(tenant(profile_resolved("us-east-2"))),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(v.status, VerifyStatus::Unchanged);

    // another value: the root is the same, and the record drifts, and the verdict names the macro
    let v = ops::verify(
        &recorded,
        &Palette::new(tenant(profile_resolved("eu-west-1"))),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(v.diff.same, "the root covers the name and not the value");
    assert_eq!(v.status, VerifyStatus::Drifted);
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.macros_changed, ["region"]);
    let verdict = common::terminal().verdict(&v.diff).unwrap();
    assert!(
        verdict.contains("published version lab-profile@1.2.0 has the content recorded t0, but macro region has another value now"),
        "{verdict}"
    );

    // a macro that the scope of the version does not give is unresolved, and that is a change too
    let v = ops::verify(
        &recorded,
        &Palette::new(tenant(serde_json::json!({"resolved": {}}))),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(v.live.source.macro_values.unwrap()["region"], "unresolved");
    assert_eq!(v.status, VerifyStatus::Drifted);

    // a record without the digests asks for no values, and cannot see the change
    let mut plain = recorded.clone();
    plain.source.macro_values = None;
    let v = ops::verify(
        &plain,
        &Palette::new(serve_fixture(MemoryApi::new(), dir.path(), "macro", UID_A)),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(v.status, VerifyStatus::Unchanged);

    // a raw directory keeps the values, so a record rebuilt from it keeps the digests
    let saved = dir.path().join("raw-profile-macro");
    raw.save_dir(&saved, "t0").unwrap();
    let again = ops::snapshot_from_dir(&saved, "aws-stack@1.1.1", "t0").unwrap();
    assert_eq!(again.source.macro_values, Some(digests));
}

#[test]
fn each_report_says_that_the_root_moved_when_a_macro_value_moved_too() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    with_macro(dir.path(), "macro");
    // the live version: the same macro, and one more line in the values of the OS pack
    with_os_pack(dir.path(), "macro-edited", |p| {
        let values = p["values"].as_str().unwrap_or_default();
        p["values"] = Value::from(format!(
            "{values}\nregion: \"{{{{.spectro.macro.region}}}}\"\nextra: 1\n"
        ));
    });
    let path = resolved_values_route(UID_A);
    let palette = Palette::new(
        serve_fixture(MemoryApi::new(), dir.path(), "macro", UID_A)
            .with(&path, profile_resolved("us-east-2")),
    );
    let mut raw = palette.fetch_profile(UID_A, &Progress::silent()).unwrap();
    raw.resolved_values = Some(palette.profile_resolved_values(UID_A).unwrap());
    let recorded = Snapshot::from_raw(&raw, "aws-stack@1.1.1", "t0").unwrap();
    let live = serve_fixture(MemoryApi::new(), dir.path(), "macro-edited", UID_A)
        .with(&path, profile_resolved("eu-west-1"));

    // the root moved, and the verify names the macro too
    let v = ops::verify(
        &recorded,
        &Palette::new(live.clone()),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(!v.diff.same, "the values of the OS pack moved");
    assert_eq!(v.status, VerifyStatus::Drifted);
    assert_eq!(v.diff.verify.as_ref().unwrap().macros_changed, ["region"]);

    // the verdict states the root: the book gives the macro verdict to an equal root only
    let sentence = "published version lab-profile@1.2.0 differs from the copy recorded t0";
    for format in [Format::Terminal, Format::Markdown] {
        let verdict = render::renderer(format, Paint::off(), false)
            .verdict(&v.diff)
            .unwrap();
        assert!(verdict.contains(sentence), "{verdict}");
        assert!(!verdict.contains("macro region"), "{verdict}");
    }

    // the row of the summary states the same, with the impact of the diff
    let s = common::verify_summary(&recorded, &live);
    let row = "differs from the copy recorded t0; max impact ";
    let lines = common::terminal().render_verify_summary(&s).unwrap();
    assert!(lines.contains(row), "{lines}");
    assert!(lines.contains(", 1 pack changed"), "{lines}");
    assert!(!lines.contains("macro region"), "{lines}");
    let table = common::markdown().render_verify_summary(&s).unwrap();
    assert!(table.contains(row), "{table}");
}

// ---- compare --repave ------------------------------------------------------------------

fn applied(uid: &str, name: &str, kind: &str) -> strata::snapshot::AppliedProfile {
    strata::snapshot::AppliedProfile {
        uid: uid.into(),
        name: name.into(),
        version: "1.0.0".into(),
        kind: kind.into(),
    }
}

#[test]
fn the_repave_request_names_each_profile_that_the_target_cluster_gets() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let a = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let uids = |body: &Value| -> Vec<String> {
        body["profiles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["uid"].as_str().unwrap().to_string())
            .collect()
    };
    let running = [
        applied(&a.source.uid, &a.source.name, "cluster"),
        applied("addon-uid", "lab-addons", "add-on"),
    ];

    // the same uid: an edit of the profile the cluster runs, so its packs carry the target values
    let body = ops::validate_body(&running, &a);
    assert_eq!(uids(&body), [a.source.uid.as_str(), "addon-uid"]);
    let packs = body["profiles"][0]["packValues"].as_array().unwrap();
    assert_eq!(packs.len(), a.packs.len());
    assert!(
        packs
            .iter()
            .all(|p| p.get("tag").is_some() && p.get("values").is_some())
    );
    assert!(body["profiles"][1].get("packValues").is_none());

    // another uid with the same name: the new version replaces the one the cluster runs
    let mut v2 = a.clone();
    v2.source.uid = "v2-uid".into();
    assert_eq!(
        uids(&ops::validate_body(&running, &v2)),
        ["v2-uid", "addon-uid"]
    );

    // another name, and a cluster profile: it replaces the one that builds the cluster
    let mut other = v2.clone();
    other.source.name = "other-stack".into();
    other.source.kind = "cluster".into();
    assert_eq!(
        uids(&ops::validate_body(&running, &other)),
        ["v2-uid", "addon-uid"]
    );

    // an add-on with a new name replaces nothing: it is added
    let mut addon = v2.clone();
    addon.source.name = "new-addon".into();
    addon.source.kind = "add-on".into();
    assert_eq!(
        uids(&ops::validate_body(&running, &addon)),
        ["v2-uid", a.source.uid.as_str(), "addon-uid"]
    );
}

#[test]
fn compare_asks_palette_and_names_a_disagreement_with_the_impact_table() {
    use strata::diff::{ImpactLevel, PaletteRepave};
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let cluster_api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a");
    let cluster = snapshot_cluster(&cluster_api, None).unwrap();
    let target = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    let path = post(&validate_repave_route());
    let answer = serde_json::json!({"isRepaveRequired": true, "reasons": [{
        "code": "PackValuesUpdated", "message": "Pack 'kubernetes' values are updated",
        "pack": {"current": {"name": "kubernetes"}, "diffConfigKeys": ["kubeadmconfig.postKubeadmCommands"]}}]});
    let api = cluster_api.clone().with(path.clone(), answer.clone());
    let palette = Palette::new(&api);

    // either order: the cluster side is what runs now
    let d = ops::compare(&cluster, &target, "t1");
    let r = ops::palette_repave(&palette, &cluster, &target, d.summary.max_impact).unwrap();
    assert!(r.required);
    assert_eq!(r.reasons[0].keys, ["kubeadmconfig.postKubeadmCommands"]);
    assert_eq!(api.posted().len(), 1);
    assert_eq!(api.posted()[0].0, validate_repave_route());
    ops::palette_repave(&palette, &target, &cluster, d.summary.max_impact).unwrap();

    // the table and Palette: a repave both ways agrees; in-place against a repave does not
    assert!(PaletteRepave::from_palette("c", &answer, ImpactLevel::Repave).agrees);
    assert!(PaletteRepave::from_palette("c", &answer, ImpactLevel::MayRepave).agrees);
    let r = PaletteRepave::from_palette("c", &answer, ImpactLevel::InPlace);
    assert!(!r.agrees);
    let none = serde_json::json!({"isRepaveRequired": false});
    assert!(!PaletteRepave::from_palette("c", &none, ImpactLevel::Repave).agrees);

    let mut d = ops::compare(&cluster, &target, "t1");
    d.summary.max_impact = ImpactLevel::InPlace;
    d.palette_repave = Some(r);
    let text = common::terminal()
        .render(&d, &cluster, &target, false)
        .unwrap();
    assert!(
        text.contains(
            "palette    repave: PackValuesUpdated kubernetes (kubeadmconfig.postKubeadmCommands)\n"
        ),
        "{text}"
    );
    assert!(
        text.contains(
            "note       the impact table says in-place, and Palette says repave for this cluster"
        ),
        "{text}"
    );

    // two profile versions, and no cluster: a usage error
    let err = ops::palette_repave(&palette, &target, &target, d.summary.max_impact).unwrap_err();
    assert!(matches!(err, Error::Usage { .. }), "{err}");

    // a record of an app profile is no profile version that a cluster can run: a usage error
    let mut app = target.clone();
    app.source.kind = ProfileType::App.as_str().to_string();
    let err = ops::palette_repave(&palette, &cluster, &app, d.summary.max_impact).unwrap_err();
    assert!(
        matches!(&err, Error::Usage { message } if message.contains("profile side")),
        "{err}"
    );
}

#[test]
fn the_check_of_the_questions_reads_each_record_file_and_refuses_the_pair_it_shows_with_no_request()
{
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let cluster_api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a");
    let cluster = snapshot_cluster(&cluster_api, None).unwrap();
    let a = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    let file = |name: &str, snap: &strata::snapshot::Snapshot| {
        let path = dir.path().join(format!("{name}.json"));
        common::write_snapshot(&path, snap, false);
        Ref::Snapshot(path)
    };
    let repave = ops::PaletteQuestions {
        repave: true,
        validate: false,
    };
    let validate = ops::PaletteQuestions {
        repave: false,
        validate: true,
    };
    let refused = |questions: ops::PaletteQuestions, x: &Ref, y: &Ref| {
        let err = questions.check(x, y).unwrap_err();
        assert!(matches!(err, Error::Usage { .. }), "{err}");
        err.to_string()
    };

    let (cluster_file, a_file, b_file) = (file("cluster", &cluster), file("a", &a), file("b", &b));
    // a cluster record and a profile record: either order, and a ref that is not a file
    for questions in [repave, validate] {
        assert!(questions.check(&cluster_file, &a_file).is_ok());
        assert!(questions.check(&b_file, &cluster_file).is_ok());
        assert!(
            questions
                .check(&cluster_file, &Ref::parse("aws-stack@1.1.1"))
                .is_ok()
        );
    }
    // two profile records: the records show no cluster, and the other side is a ref
    assert!(
        refused(repave, &a_file, &b_file).starts_with("--repave needs one side that is a cluster")
    );
    assert!(
        refused(validate, &a_file, &Ref::parse("aws-stack@1.1.6"))
            .starts_with("--validate needs one side that is a cluster")
    );
    // a record of an edge host is no cluster, and a record with no uid is no profile that
    // Palette holds
    let mut host = cluster.clone();
    host.source.cluster.as_mut().unwrap().kind = strata::snapshot::SubjectKind::EdgeHost.into();
    let host_file = file("host", &host);
    assert!(
        refused(repave, &host_file, &a_file).contains("not for an edge host"),
        "an edge host is not a cluster"
    );
    let mut candidate = b.clone();
    candidate.source.uid.clear();
    let candidate_file = file("candidate", &candidate);
    assert!(
        refused(validate, &cluster_file, &candidate_file)
            .contains("a profile version that Palette holds")
    );
    // a record of the update that waits: the same cluster, or two uids that show two clusters
    let mut waiting = cluster.clone();
    waiting.source.cluster.as_mut().unwrap().pending = true;
    let waiting_file = file("waiting", &waiting);
    assert!(repave.check(&cluster_file, &waiting_file).is_ok());
    assert!(repave.check(&waiting_file, &cluster_file).is_ok());
    assert!(
        refused(validate, &cluster_file, &waiting_file).starts_with("--validate needs one side")
    );
    waiting.source.cluster.as_mut().unwrap().uid = "another-cluster".into();
    let other_file = file("other", &waiting);
    assert!(
        refused(repave, &cluster_file, &other_file).contains("cluster:<c> cluster:<c>@pending")
    );
    // a live ref names a cluster by a name or by a uid, so it can name the same cluster
    assert!(
        repave
            .check(&cluster_file, &Ref::parse("cluster:lab-cluster@pending"))
            .is_ok()
    );
    // a record file that is missing is the error of the read
    let missing = Ref::Snapshot(dir.path().join("nothing.json"));
    assert_eq!(repave.check(&missing, &a_file).unwrap_err().kind(), "io");
    assert_eq!(
        cluster_api.calls(&cluster_route(CLUSTER_UID)),
        1,
        "only the record of the cluster read the document; the check sent nothing"
    );
}

#[test]
fn compare_asks_the_template_route_for_a_cluster_that_a_cluster_template_governs() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let mut doc = read_json(&dir.path().join("cluster-a").join("cluster.json"));
    doc["spec"]["clusterTemplate"] = serde_json::json!({"uid": "tmpl-1", "projectUid": "p1"});
    let cluster_api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a")
        .with(cluster_route(CLUSTER_UID), doc);
    let cluster = snapshot_cluster(&cluster_api, None).unwrap();
    let target = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    // on the route of a cluster in a template, Palette compares nothing and answers no repave; it
    // answers for the template on the route of the template
    let template_route = "/v1/spectroclusters/clusterTemplates/tmpl-1/validate/repave";
    let api = cluster_api
        .clone()
        .with(
            post(&validate_repave_route()),
            serde_json::json!({"isRepaveRequired": false, "reasons": []}),
        )
        .with(
            post(template_route),
            serde_json::json!({"isRepaveRequired": true, "reasons": [{
                "code": "PackUpgraded", "message": "Pack 'kubernetes' is upgraded",
                "pack": {"current": {"name": "kubernetes"}}}]}),
        );
    let palette = Palette::new(&api);

    let mut d = ops::compare(&cluster, &target, "t1");
    let r = ops::palette_repave(&palette, &cluster, &target, d.summary.max_impact).unwrap();
    assert!(r.required);
    assert_eq!(r.cluster_template.as_ref().unwrap().uid, "tmpl-1");
    // one POST, to the route of the template: the profile uids of the cluster route, with no
    // pack values, and not for an upgrade now, which ignores the profiles of the request
    let posted = api.posted();
    assert_eq!(posted.len(), 1);
    assert_eq!(posted[0].0, template_route);
    let running = strata::snapshot::AppliedProfile::of_cluster(
        &Palette::new(&cluster_api)
            .subject_document(SubjectKind::Cluster, CLUSTER_UID)
            .unwrap(),
    );
    let uids: Vec<Value> = ops::validate_body(&running, &target)["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| serde_json::json!({"uid": p["uid"]}))
        .collect();
    assert_eq!(
        posted[0].1,
        serde_json::json!({"profiles": uids, "isUpgradeNow": false})
    );

    d.palette_repave = Some(r);
    let text = common::terminal()
        .render(&d, &cluster, &target, false)
        .unwrap();
    assert!(!text.contains("palette    no repave"), "{text}");
    assert!(
        text.contains(
            "palette    the cluster template tmpl-1 governs the cluster: Palette answers for the \
             template, by profile uid\n\
             palette    repave: PackUpgraded kubernetes\n"
        ),
        "{text}"
    );
    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), false)
            .render(&d, &cluster, &target, false)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["paletteRepave"]["required"], true);
    assert_eq!(json["paletteRepave"]["clusterTemplate"]["uid"], "tmpl-1");

    // a version that the cluster runs: the route of the template takes no values, so it compares
    // nothing, and strata stops before the POST
    let running_version = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let err = ops::palette_repave(&palette, &cluster, &running_version, d.summary.max_impact)
        .unwrap_err();
    assert!(
        matches!(&err, Error::ClusterTemplateGoverns { cluster, template, .. }
            if cluster == CLUSTER_UID && template == "tmpl-1"),
        "{err}"
    );
    assert_eq!(err.kind(), "cluster-template-governs");
    assert!(err.to_string().contains("cluster template tmpl-1"), "{err}");
    assert_eq!(api.posted().len(), 1);
}

/// The update that waits for the cluster under `CLUSTER_UID`.
///
/// `edit` changes the templates of the cluster fixture `fixture`. The pack manifests of each
/// profile come from the same fixture.
fn serve_pending(
    api: MemoryApi,
    raw_root: &Path,
    fixture: &str,
    edit: impl Fn(&mut Value),
) -> MemoryApi {
    let d = raw_root.join(fixture);
    let mut templates =
        read_json(&d.join("cluster.json"))["spec"]["clusterProfileTemplates"].clone();
    edit(&mut templates);
    let mut api = api.with(
        profile_updates_route(),
        serde_json::json!({ "profiles": templates }),
    );
    for m in read_json(&d.join("manifests.json"))["profiles"]
        .as_array()
        .unwrap()
    {
        let uid = m["metadata"]["uid"].as_str().unwrap();
        api = api.with(manifests_route(uid), m.clone());
    }
    api
}

#[test]
fn the_repave_request_for_the_update_that_waits_gives_the_pending_values_to_each_profile_it_edits()
{
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let (infra, addon) = merged_uids(dir.path());
    let running = serve_cluster(MemoryApi::new(), dir.path(), "cluster-merged");
    let cluster = snapshot_cluster(&running, None).unwrap();
    let path = post(&validate_repave_route());
    let answer = serde_json::json!({"isRepaveRequired": true, "reasons": [{
        "code": "PackValuesUpdated", "message": "Pack 'kubernetes' values are updated",
        "pack": {"current": {"name": "kubernetes"}, "diffConfigKeys": ["kubeadmconfig.postKubeadmCommands"]}}]});
    let pending_of =
        |api: &MemoryApi, profile: Option<&str>| snapshot_pending(api, profile).unwrap();
    let profile_of = |template: &Value| template["uid"] != addon.as_str();
    let edited = "kubeadmconfig:\n  postKubeadmCommands:\n  - echo strata-pending\n";
    // an edit in place of the values of the Kubernetes pack of the cluster profile
    let edit_values = |templates: &mut Value| {
        for t in templates.as_array_mut().unwrap() {
            if profile_of(t) {
                for p in t["packs"].as_array_mut().unwrap() {
                    if p["layer"] == "k8s" {
                        p["values"] = edited.into();
                    }
                }
            }
        }
    };
    let ask = |api: &MemoryApi, a: &Snapshot, b: &Snapshot| -> strata::Result<Value> {
        let api = api.clone().with(path.clone(), answer.clone());
        let d = ops::compare(a, b, "t1");
        let r = ops::palette_repave(&Palette::new(&api), a, b, d.summary.max_impact)?;
        assert!(r.required);
        let posted = api.posted();
        assert_eq!(posted.len(), 1);
        assert_eq!(posted[0].0, validate_repave_route());
        Ok(posted[0].1.clone())
    };
    let uid_and_packs = |body: &Value| -> Vec<(String, Option<Vec<String>>)> {
        body["profiles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                let packs = p.get("packValues").map(|v| {
                    v.as_array()
                        .unwrap()
                        .iter()
                        .map(|p| p["name"].as_str().unwrap().to_string())
                        .collect()
                });
                (p["uid"].as_str().unwrap().to_string(), packs)
            })
            .collect()
    };
    let mut b_names: Vec<String> = read_json(
        &dir.path().join("cluster-merged").join("cluster.json"),
    )["spec"]["clusterProfileTemplates"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| profile_of(t))
        .flat_map(|t| t["packs"].as_array().unwrap())
        .map(|p| p["name"].as_str().unwrap().to_string())
        .collect();
    b_names.sort();

    // one profile edited in place: it alone carries the pack values, each pack of it, in either
    // order of the two sides
    let api = serve_pending(running.clone(), dir.path(), "cluster-merged", edit_values);
    let pending = pending_of(&api, None);
    assert_ne!(pending.root, cluster.root);
    let body = ask(&api, &cluster, &pending).unwrap();
    let k8s = body["profiles"][0]["packValues"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["values"] == edited)
        .expect("the pending values of the Kubernetes pack");
    assert!(k8s.get("tag").is_some() && k8s.get("type").is_some());
    let names = uid_and_packs(&body);
    let mut sent = names[0].1.clone().unwrap();
    sent.sort();
    assert_eq!(sent, b_names, "each pack of the edited profile");
    assert_eq!(
        names[1],
        (addon.clone(), None),
        "the add-on profile does not change"
    );
    assert_eq!(ask(&api, &pending, &cluster).unwrap(), body);

    // a pack that is new to the cluster: Palette takes it from the profile that holds it now,
    // so each profile of the record gets it
    let add_pack = |templates: &mut Value| {
        for t in templates.as_array_mut().unwrap() {
            if !profile_of(t) {
                let mut new = t["packs"][0].clone();
                new["name"] = "strata-new-pack".into();
                t["packs"].as_array_mut().unwrap().push(new);
            }
        }
    };
    let api = serve_pending(running.clone(), dir.path(), "cluster-merged", add_pack);
    let body = ask(&api, &cluster, &pending_of(&api, None)).unwrap();
    for (uid, packs) in uid_and_packs(&body) {
        assert!(
            packs
                .as_ref()
                .unwrap()
                .contains(&"strata-new-pack".to_string()),
            "{uid}: {packs:?}"
        );
    }

    // a record of one profile gives the pack values to that profile alone
    let api = serve_pending(running.clone(), dir.path(), "cluster-merged", edit_values);
    let body = ask(&api, &cluster, &pending_of(&api, Some("lab-addons"))).unwrap();
    assert!(
        uid_and_packs(&body)
            .iter()
            .all(|(_, packs)| packs.is_none())
    );

    // a record of a profile that the cluster no longer runs is older than the cluster. The
    // document of the cluster shows it, so the command is correct and the record has its own kind
    let moved = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a");
    let err = ask(&moved, &cluster, &pending).unwrap_err();
    assert!(
        matches!(&err, Error::StaleRecord { cluster, name, .. }
            if cluster == CLUSTER_UID && name == "lab-cluster"),
        "{err}"
    );
    assert_eq!(err.kind(), "stale-record");
    // `cluster` holds the uid in each error of a cluster, so a script reads one kind of value
    assert_eq!(err.to_json()["cluster"], CLUSTER_UID);
    assert_eq!(err.to_json()["name"], "lab-cluster");
    assert!(
        err.to_string()
            .ends_with("Record cluster:lab-cluster@pending again"),
        "{err}"
    );
    assert!(moved.posted().is_empty(), "no POST goes out for the record");

    // a cluster in a template: the route of the template takes no pack values. So strata refuses
    // an update that changes a profile before the POST. An update that changes nothing asks the
    // route of the template by profile uid
    let mut doc = read_json(&dir.path().join("cluster-merged").join("cluster.json"));
    doc["spec"]["clusterTemplate"] = serde_json::json!({"uid": "tmpl-1", "projectUid": "p1"});
    let governed = running.clone().with(cluster_route(CLUSTER_UID), doc);
    let err = ask(&governed, &cluster, &pending).unwrap_err();
    assert!(
        matches!(&err, Error::ClusterTemplateGoverns { template, asked, .. }
            if template == "tmpl-1"
                && asked.contains("the update that waits for the cluster lab-cluster")),
        "{err}"
    );
    assert_eq!(err.kind(), "cluster-template-governs");
    assert!(
        governed.posted().is_empty(),
        "no POST goes out for the template"
    );
    let api = serve_pending(governed, dir.path(), "cluster-merged", |_| {}).with(
        "POST /v1/spectroclusters/clusterTemplates/tmpl-1/validate/repave",
        serde_json::json!({"isRepaveRequired": false, "reasons": []}),
    );
    let unchanged = pending_of(&api, None);
    let d = ops::compare(&cluster, &unchanged, "t1");
    let r = ops::palette_repave(
        &Palette::new(&api),
        &cluster,
        &unchanged,
        d.summary.max_impact,
    )
    .unwrap();
    assert!(!r.required);
    assert_eq!(r.cluster_template.unwrap().uid, "tmpl-1");
    assert_eq!(
        api.posted(),
        [(
            "/v1/spectroclusters/clusterTemplates/tmpl-1/validate/repave".to_string(),
            serde_json::json!({"profiles": [{"uid": infra}, {"uid": addon}], "isUpgradeNow": false})
        )]
    );

    // two records of what runs, or two of the update that waits: a usage error, before a request
    for (a, b) in [(&cluster, &cluster), (&pending, &pending)] {
        let err = ask(&running, a, b).unwrap_err();
        assert!(matches!(err, Error::Usage { .. }), "{err}");
    }
}

// ---- compare --validate ----------------------------------------------------------------

#[test]
fn compare_validate_prints_each_error_of_palette_and_keeps_the_exit_of_the_comparison() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let cluster_api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a");
    let cluster = snapshot_cluster(&cluster_api, None).unwrap();
    let target = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    let path = post(&validate_packs_route());
    let message = "Pack 'strata-live-dep' is required with minimum compatible version '1.0.0'";
    // Palette gives the error under each profile that holds a pack of that name
    let error = serde_json::json!({"packs": {"results": [{"name": "strata-live-pack",
        "displayName": "strata-live-pack",
        "errors": [{"code": "PackDependencyNotFound", "message": message}]}]}});
    let answer = serde_json::json!({"profiles": [
        {"uid": "p1", "packs": error["packs"]}, {"uid": "p2", "packs": error["packs"]}]});
    let api = cluster_api.clone().with(path.clone(), answer);
    let palette = Palette::new(&api);

    let mut d = ops::compare(&cluster, &target, "t1");
    let same = d.same;
    let v = ops::palette_validation(&palette, &cluster, &target).unwrap();
    assert!(!v.answer.accepts());
    assert_eq!(v.cluster_uid, CLUSTER_UID);
    assert_eq!(v.answer.errors().len(), 1, "{v:?}");
    assert_eq!(v.answer.errors()[0].pack, "strata-live-pack");
    // one POST, with the body of --repave, in either order of the sides
    let posted = api.posted();
    assert_eq!(posted.len(), 1);
    assert_eq!(posted[0].0, validate_packs_route());
    let running = strata::snapshot::AppliedProfile::of_cluster(
        &Palette::new(&cluster_api)
            .subject_document(SubjectKind::Cluster, CLUSTER_UID)
            .unwrap(),
    );
    assert_eq!(posted[0].1, ops::validate_body(&running, &target));
    assert_eq!(
        ops::palette_validation(&palette, &target, &cluster).unwrap(),
        v
    );

    d.palette_validation = Some(v);
    let text = common::terminal()
        .render(&d, &cluster, &target, false)
        .unwrap();
    assert!(
        text.contains(&format!(
            "palette    refuses: PackDependencyNotFound strata-live-pack: {message}\n"
        )),
        "{text}"
    );
    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), false)
            .render(&d, &cluster, &target, false)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["paletteValidation"]["accepts"], false);
    assert_eq!(
        json["paletteValidation"]["errors"][0]["code"],
        "PackDependencyNotFound"
    );
    // the answer of Palette does not change the result of the comparison
    assert_eq!(d.same, same);
    assert!(!d.same);

    // a 200 with no error: Palette accepts
    let api = cluster_api
        .clone()
        .with(path.clone(), serde_json::json!({}));
    let mut d = ops::compare(&cluster, &target, "t1");
    d.palette_validation =
        Some(ops::palette_validation(&Palette::new(&api), &cluster, &target).unwrap());
    assert!(d.palette_validation.as_ref().unwrap().answer.accepts());
    let text = common::terminal()
        .render(&d, &cluster, &target, false)
        .unwrap();
    assert!(text.contains("palette    accepts\n"), "{text}");

    // Palette refuses the whole request with HTTP 500 and a code of the route: an answer, with no
    // pack, the code, and the message, and the retry does not send it again
    let api = cluster_api.clone().failing_n_times(
        path.clone(),
        99,
        500,
        r#"{"code":"PackSpectroIntegrationNotSupported","details":null,"message":"Pack 'spectro-k8s-dashboard' is not supported for imported cluster","ref":"x"}"#,
    );
    let (retry, slept) = retrying(&api, 3);
    let v = ops::palette_validation(&Palette::new(retry), &cluster, &target).unwrap();
    assert!(!v.answer.accepts());
    assert_eq!(v.answer.errors().len(), 1);
    assert_eq!(v.answer.errors()[0].pack, "");
    assert_eq!(
        v.answer.errors()[0].code,
        "PackSpectroIntegrationNotSupported"
    );
    assert_eq!(
        v.answer.errors()[0].message,
        "Pack 'spectro-k8s-dashboard' is not supported for imported cluster"
    );
    assert_eq!(api.calls(&path), 1, "one attempt and no retry");
    assert!(slept.borrow().is_empty());
    // the check of the OIDC identity provider of an integration pack refuses with a 500 too, and
    // the check of a read-only dependency of an integration pack with a 403
    for (status, code, message) in [
        (
            500,
            "ClusterProfileOidcNotConfigured",
            "Pack 'Spectro Proxy' OIDC is not configured at kubernetes pack",
        ),
        (
            403,
            PACK_FORBIDDEN_CODE,
            "Pack 'nginx' values override is forbidden",
        ),
    ] {
        let body = serde_json::json!({"code": code, "message": message}).to_string();
        let api = cluster_api
            .clone()
            .failing_n_times(path.clone(), 99, status, body);
        let (retry, _) = retrying(&api, 3);
        let v = ops::palette_validation(&Palette::new(retry), &cluster, &target).unwrap();
        assert_eq!(
            (
                v.answer.errors()[0].code.as_str(),
                v.answer.errors()[0].message.as_str()
            ),
            (code, message)
        );
        assert_eq!(api.calls(&path), 1, "{code}: one attempt and no retry");
    }
    // a 500 with a code that is not a refusal, and a refused key, are errors of strata
    for (status, body) in [
        (
            500,
            r#"{"code":"DataReadError","message":"Data access error"}"#,
        ),
        (403, r#"{"code":"Forbidden"}"#),
        // a refusal code of the route with another status is not its refusal
        (403, r#"{"code":"ClusterProfileOidcNotConfigured"}"#),
    ] {
        let api = cluster_api.clone().failing(path.clone(), status, body);
        let err = ops::palette_validation(&Palette::new(&api), &cluster, &target).unwrap_err();
        assert!(
            matches!(&err, Error::Api { status: s, .. } if *s == status),
            "{err}"
        );
    }

    // two profile versions, and an edge host: a usage error that names the option
    let err = ops::palette_validation(&palette, &target, &target).unwrap_err();
    assert!(matches!(err, Error::Usage { .. }), "{err}");
    assert!(
        err.to_string().contains("--validate needs one side"),
        "{err}"
    );
    let mut host = cluster.clone();
    host.source.cluster.as_mut().unwrap().kind = strata::snapshot::SubjectKind::EdgeHost.into();
    let err = ops::palette_validation(&palette, &host, &target).unwrap_err();
    assert!(matches!(err, Error::Usage { .. }), "{err}");
    assert!(
        err.to_string().contains(
            "--validate needs a cluster: Palette answers it for a cluster, and not for an edge host"
        ),
        "{err}"
    );
}

#[test]
fn compare_validate_asks_the_route_of_the_profile_version_when_the_cluster_runs_that_version() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let cluster_api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a");
    let cluster = snapshot_cluster(&cluster_api, None).unwrap();
    // fixture A is the profile version that the cluster runs
    let target = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let uid = target.source.uid.clone();
    let message = "Pack 'strata-live-dep' is required with minimum compatible version '1.0.0'";
    let error = serde_json::json!({"packs": {"results": [{"name": "strata-live-pack",
        "errors": [{"code": "PackDependencyNotFound", "message": message}]}]}});
    let route = update_validate_packs_route(&uid);
    let api = cluster_api
        .clone()
        .with(post(&route), error)
        // the route of the cluster accepts a move of the tag: it does not check the sent tag
        .with(post(&validate_packs_route()), serde_json::json!({}));
    let palette = Palette::new(&api);

    let v = ops::palette_validation(&palette, &cluster, &target).unwrap();
    assert_eq!(v.cluster_uid, CLUSTER_UID);
    assert_eq!(v.answer.errors().len(), 1, "{v:?}");
    assert_eq!(v.answer.errors()[0].pack, "strata-live-pack");
    assert_eq!(v.answer.errors()[0].code, "PackDependencyNotFound");
    // one POST, to the route of the version, with the packs of the version
    let posted = api.posted();
    assert_eq!(posted.len(), 1, "{posted:?}");
    assert_eq!(posted[0].0, route);
    assert_eq!(posted[0].1, ops::profile_validate_body(&target));
    // the sides in the other order give the same answer
    assert_eq!(
        ops::palette_validation(&palette, &target, &cluster).unwrap(),
        v
    );

    // a 200 with no error: Palette accepts
    let accepting = cluster_api.with(post(&route), serde_json::json!({"packs": {"results": []}}));
    let v = ops::palette_validation(&Palette::new(&accepting), &cluster, &target).unwrap();
    assert!(v.answer.accepts());
    assert_eq!(accepting.calls(&validate_packs_route()), 0);
}

// ---- the provider type of an OCI registry ---------------------------------------------------------

#[test]
fn a_pack_from_an_oci_registry_records_the_provider_type_outside_the_root() {
    use strata::snapshot::Pack;
    let pack = |annotations: Value| {
        serde_json::json!({"name": "argocd", "type": "oci", "layer": "addon", "tag": "10.4.0.1",
            "packUid": "p1", "registryUid": "r1", "values": "a: 1", "annotations": annotations})
    };
    for kind in ["pack", "helm", "zarf"] {
        let p = Pack::from_api(
            &pack(serde_json::json!({"ociRegistryProviderType": kind})),
            &[],
        );
        assert_eq!(
            p.registry_provider_type.as_ref().map(Open::as_str),
            Some(kind)
        );
        let doc = serde_json::to_value(&p).unwrap();
        assert_eq!(doc["registryProviderType"], kind);
    }
    // a pack ref with no such annotation, as a spectro or helm pack has, records none
    let plain = Pack::from_api(&pack(serde_json::json!({"source": "community"})), &[]);
    assert_eq!(plain.registry_provider_type, None);
    assert!(
        serde_json::to_value(&plain)
            .unwrap()
            .get("registryProviderType")
            .is_none()
    );
    // and the node hash is the same with and without it: an old record verifies as before
    let helm = Pack::from_api(
        &pack(serde_json::json!({"ociRegistryProviderType": "helm"})),
        &[],
    );
    assert_eq!(helm.hash.node, plain.hash.node);
}

#[test]
fn a_record_of_a_pack_from_an_oci_registry_names_its_provider_type() {
    // the pack listing does not name the provider type: the listing of the OCI registries does
    let mut oci = csi_summary("pk-m", "1.64.0", "reg-mirror", "sha256:mmm", CSI_63);
    oci["spec"]["type"] = serde_json::json!("oci");
    let mirror =
        serde_json::json!({"layer": "csi", "tags": [{"tag": "1.64.0", "packUid": "pk-m"}]});
    let api = serve_registry(
        vec![oci],
        &[("reg-public", public_entity()), ("reg-mirror", mirror)],
    );
    let listed = api.clone().with(
        OCI_REGISTRY_SUMMARY,
        serde_json::json!({"items": [
            {"metadata": {"uid": "reg-mirror", "name": "Mirror"}, "spec": {"providerType": "helm"}},
        ]}),
    );
    let pack = snapshot_of(&listed, "pack:Mirror/csi-aws-ebs@1.64.0").unwrap();
    assert_eq!(
        pack.packs[0]
            .registry_provider_type
            .as_ref()
            .map(Open::as_str),
        Some("helm")
    );
    // a Palette that answers the listing with a 4xx gives a record that names no provider type
    let unnamed = snapshot_of(&api, "pack:Mirror/csi-aws-ebs@1.64.0").unwrap();
    assert_eq!(unnamed.packs[0].registry_provider_type, None);
    // and a pack from a pack registry names none, with no read of the listing
    let spectro = snapshot_of(&listed, "pack:Public Repo/csi-aws-ebs@1.65.0").unwrap();
    assert_eq!(spectro.packs[0].registry_provider_type, None);
    assert_eq!(
        listed.calls(OCI_REGISTRY_SUMMARY),
        1,
        "one read for the two records"
    );
    // a 5xx of the listing is an error of the record. The provider type is content, so a record
    // with none in its place is wrong (#327)
    let broken = api.failing(OCI_REGISTRY_SUMMARY, 500, r#"{"code":"InternalError"}"#);
    let err = snapshot_of(&broken, "pack:Mirror/csi-aws-ebs@1.64.0").unwrap_err();
    assert!(matches!(err, Error::Api { status: 500, .. }), "{err}");
}

// ---- the machine image of the OS pack (#189) ----------------------------------------------------

/// The annotations of the OS pack of fixture A, with `imageId` set to `image` when it is given.
fn os_annotations(image: Option<&str>) -> Value {
    let mut a = serde_json::json!({"osName": "ubuntu", "os_version": "u-2204"});
    if let Some(image) = image {
        a[strata::snapshot::IMAGE_ID_ANNOTATION] = Value::from(image);
    }
    a
}

/// An API that serves fixture A under `UID_A`, with `imageId` of its OS pack set to `image`.
fn serve_os_image(raw_root: &Path, image: Option<&str>) -> MemoryApi {
    let dir = image.unwrap_or("no-image");
    with_os_pack(raw_root, dir, |p| p["annotations"] = os_annotations(image));
    serve_fixture(MemoryApi::new(), raw_root, dir, UID_A)
}

#[test]
fn a_pack_record_keeps_only_the_image_id_annotation_outside_the_root() {
    use strata::snapshot::Pack;
    let pack = |annotations: Value| {
        serde_json::json!({"name": "ubuntu-aws", "type": "spectro", "layer": "os", "tag": "22.04",
            "packUid": "p1", "registryUid": "r1", "values": "a: 1", "annotations": annotations})
    };
    let byoi = Pack::from_api(&pack(os_annotations(Some("ami-0aaa"))), &[]);
    assert_eq!(byoi.image_id(), Some("ami-0aaa"));
    // another annotation can hold a secret: the record keeps only `imageId`
    let doc = serde_json::to_value(&byoi).unwrap();
    assert_eq!(
        doc["annotations"],
        serde_json::json!({"imageId": "ami-0aaa"})
    );
    let secret = serde_json::json!({"imageId": "ami-0aaa", "registryToken": "s3cr3t"});
    let doc = serde_json::to_value(Pack::from_api(&pack(secret), &[])).unwrap();
    assert_eq!(
        doc["annotations"],
        serde_json::json!({"imageId": "ami-0aaa"})
    );
    let doc = serde_json::to_value(Pack::from_api(&pack(os_annotations(None)), &[])).unwrap();
    assert!(doc.get("annotations").is_none(), "{doc}");
    // the annotations are outside the root: a new image keeps the node hash
    let other = Pack::from_api(&pack(os_annotations(Some("ami-0bbb"))), &[]);
    let none = Pack::from_api(&pack(Value::Null), &[]);
    assert_eq!(byoi.hash.node, other.hash.node);
    assert_eq!(byoi.hash.node, none.hash.node);
    // a pack ref with no annotations writes none, and an empty imageId names no image
    assert_eq!(none.image_id(), None);
    assert!(
        serde_json::to_value(&none)
            .unwrap()
            .get("annotations")
            .is_none()
    );
    let empty = Pack::from_api(&pack(serde_json::json!({"imageId": ""})), &[]);
    assert_eq!(empty.image_id(), None);
    // a record written before the field reads as one with no annotations
    let mut old = serde_json::to_value(&byoi).unwrap();
    old.as_object_mut().unwrap().remove("annotations");
    let read: Pack = serde_json::from_value(old.clone()).unwrap();
    assert!(read.annotations.is_empty());
    // and a record of strata 0.17 to 0.19, with each annotation, reads its image
    old["annotations"] = os_annotations(Some("ami-0aaa"));
    let read: Pack = serde_json::from_value(old).unwrap();
    assert_eq!(read.image_id(), Some("ami-0aaa"));
}

#[test]
fn an_older_record_that_holds_each_annotation_of_the_os_pack_verifies() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_os_image(dir.path(), Some("ami-0aaa"));
    let new = common::snapshot_of_at(&api, UID_A, "t0").unwrap();
    // a record of strata 0.17 to 0.19 kept each annotation of the pack ref
    let mut doc = serde_json::to_value(&new).unwrap();
    for pack in doc["packs"].as_array_mut().unwrap() {
        if pack["layer"] == "os" {
            pack["annotations"] = os_annotations(Some("ami-0aaa"));
        }
    }
    let older: Snapshot = serde_json::from_value(doc).unwrap();
    let os = older.packs.iter().find(|p| p.layer == "os").unwrap();
    assert_eq!(os.annotations["osName"], "ubuntu");
    let v = ops::verify(&older, &Palette::new(api), "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged, "{:?}", v.diff.image_ids_changed);
    let d = ops::compare(&older, &new, "t2");
    assert!(d.nothing_differs(), "{:?}", d.image_ids_changed);
}

#[test]
fn compare_names_the_change_of_the_image_id_of_the_os_pack_when_the_roots_are_equal() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let a =
        common::snapshot_of_at(&serve_os_image(dir.path(), Some("ami-0aaa")), UID_A, "t0").unwrap();
    let b =
        common::snapshot_of_at(&serve_os_image(dir.path(), Some("ami-0bbb")), UID_A, "t1").unwrap();
    let d = ops::compare(&a, &b, "t2");
    assert!(d.same, "the root does not cover the annotations");
    assert_eq!(
        d.image_ids_changed,
        vec![strata::diff::ImageIdChange {
            key: "os/ubuntu-aws".into(),
            a: "ami-0aaa".into(),
            b: "ami-0bbb".into(),
        }]
    );
    assert!(!d.nothing_differs(), "compare exits 1 for it");
    let note = "pack os/ubuntu-aws names image ami-0aaa on one side and ami-0bbb on the other: \
                the root does not cover the annotations";
    let text = common::terminal().render(&d, &a, &b, false).unwrap();
    assert!(text.contains(&format!("note       {note}")), "{text}");
    let md = common::markdown().render(&d, &a, &b, false).unwrap();
    assert!(md.contains(&format!("**Note:** {note}")), "{md}");
    let json = serde_json::to_value(&d).unwrap();
    assert_eq!(
        json["imageIdsChanged"],
        serde_json::json!([{"key": "os/ubuntu-aws", "a": "ami-0aaa", "b": "ami-0bbb"}])
    );

    // one image on each side: nothing to name
    let same = ops::compare(&a, &a, "t2");
    assert!(same.image_ids_changed.is_empty() && same.nothing_differs());
    // a side that names no image is not compared: an older record, or a pack whose image
    // Palette chooses
    let plain = common::snapshot_of_at(&serve_os_image(dir.path(), None), UID_A, "t1").unwrap();
    for (x, y) in [(&a, &plain), (&plain, &a)] {
        let d = ops::compare(x, y, "t2");
        assert!(
            d.image_ids_changed.is_empty() && d.nothing_differs(),
            "{:?}",
            d.image_ids_changed
        );
    }
}

#[test]
fn same_release_refuses_two_records_whose_os_pack_names_two_machine_images() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let record =
        |image| common::snapshot_of_at(&serve_os_image(dir.path(), image), UID_A, "t0").unwrap();
    let (a, b, plain) = (
        record(Some("ami-0aaa")),
        record(Some("ami-0bbb")),
        record(None),
    );
    let pair = |x: &Snapshot, y: &Snapshot, policy, allow_no_digest| {
        let records = [
            ("a.json".to_string(), x.clone()),
            ("b.json".to_string(), y.clone()),
        ];
        common::same_release_offline(
            &records,
            SameReleaseOptions {
                policy,
                allow_no_digest,
                ..SameReleaseOptions::default()
            },
            "t1",
        )
    };
    assert_eq!(a.root, b.root, "the root does not cover the annotations");
    let reason = "pack os/ubuntu-aws names image ami-0aaa on one side and ami-0bbb on the \
                  other: the root does not cover the annotations";
    for policy in [Policy::Exact, Policy::RegistryMove] {
        for allow_no_digest in [false, true] {
            let c = pair(&a, &b, policy, allow_no_digest);
            assert_eq!((c.same, c.exit_code()), (false, 1));
            assert_eq!(c.pairs[0].status, PairStatus::Different);
            assert!(
                c.pairs[0].reasons.iter().any(|r| r == reason),
                "{:?}",
                c.pairs[0].reasons
            );
        }
    }
    let c = pair(&a, &b, Policy::Exact, true);
    let text = common::terminal().render_chain(&c).unwrap();
    assert!(
        text.starts_with("DIFFERENT") && text.contains(reason),
        "{text}"
    );

    // a side that names no image is not compared: the answer is that of one image on both sides
    for policy in [Policy::Exact, Policy::RegistryMove] {
        for allow_no_digest in [false, true] {
            let same = pair(&a, &a, policy, allow_no_digest);
            for (x, y) in [(&a, &plain), (&plain, &a), (&plain, &plain)] {
                let c = pair(x, y, policy, allow_no_digest);
                assert_eq!(
                    (c.same, c.pairs[0].status, &c.pairs[0].reasons),
                    (same.same, same.pairs[0].status, &same.pairs[0].reasons)
                );
            }
        }
    }
}

#[test]
fn verify_reports_a_new_image_id_of_the_os_pack_as_drift_with_the_recorded_content() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded =
        common::snapshot_of_at(&serve_os_image(dir.path(), Some("ami-0aaa")), UID_A, "t0").unwrap();
    let live = serve_os_image(dir.path(), Some("ami-0bbb"));
    let v = ops::verify(
        &recorded,
        &Palette::new(live.clone()),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(!v.unchanged);
    assert!(v.diff.same);
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.status, VerifyStatus::Drifted);
    assert_eq!(ver.image_ids_changed, v.diff.image_ids_changed);
    let why = "has the content recorded t0, but pack os/ubuntu-aws names image ami-0bbb now, not ami-0aaa";
    for format in [Format::Terminal, Format::Markdown] {
        let verdict = render::renderer(format, Paint::off(), false)
            .verdict(&v.diff)
            .unwrap();
        assert!(
            verdict.contains("DRIFT") && verdict.contains(why),
            "{verdict}"
        );
    }
    let s = common::verify_summary(&recorded, &live);
    assert_eq!(
        (s.records[0].status, s.exit_code()),
        (RecordStatus::Drifted, 1)
    );
    let lines = common::terminal().render_verify_summary(&s).unwrap();
    assert!(lines.contains(why), "{lines}");

    // the same image: unchanged
    let v = ops::verify(
        &recorded,
        &Palette::new(serve_os_image(dir.path(), Some("ami-0aaa"))),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(v.unchanged, "{:?}", v.diff.image_ids_changed);
}

#[test]
fn verify_names_a_new_image_id_in_the_diff_and_not_in_the_verdict_when_the_root_moved() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded =
        common::snapshot_of_at(&serve_os_image(dir.path(), Some("ami-0aaa")), UID_A, "t0").unwrap();
    // a new pack build of the OS layer, with its own image
    with_os_pack(dir.path(), "upgraded", |p| {
        p["annotations"] = os_annotations(Some("ami-0bbb"));
        p["packUid"] = Value::from("os-pack-2");
    });
    let live = serve_fixture(MemoryApi::new(), dir.path(), "upgraded", UID_A);
    let v = ops::verify(&recorded, &Palette::new(live), "t1", &Progress::silent()).unwrap();
    assert!(!v.diff.same);
    assert_eq!(v.diff.image_ids_changed.len(), 1);
    let ver = v.diff.verify.as_ref().unwrap();
    assert!(ver.image_ids_changed.is_empty());
    let verdict = common::terminal().verdict(&v.diff).unwrap();
    assert!(
        verdict.starts_with("DRIFT: ") && verdict.contains("differs from"),
        "{verdict}"
    );
}

// ---- the tiers of a cluster record (#76) ---------------------------------------------------------

/// cluster-a, with what Palette reports of a running cluster beside its profiles: its hashes of
/// the applied spec, the pack version each pack reconciled, its policy, and its labels. One more
/// reconciled pack belongs to a profile that the record does not cover.
fn with_cluster_status(raw_root: &Path) {
    let path = raw_root.join("cluster-a").join("cluster.json");
    let mut doc = read_json(&path);
    let template = doc["spec"]["clusterProfileTemplates"][0].clone();
    let mut packs: Vec<Value> = template["packs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            serde_json::json!({"name": p["name"], "version": p["version"].as_str().or(p["tag"].as_str()),
                "profileUid": template["uid"], "condition": {"type": "Ready", "status": "True"}})
        })
        .collect();
    packs.push(
        serde_json::json!({"name": "other", "version": "9.9.9", "profileUid": "not-covered"}),
    );
    doc["status"]["spcApply"] = serde_json::json!({"spcHash": "h1", "spcInfraHash": "i1",
        "lastModifiedTime": "2026-09-25T00:00:00Z"});
    doc["status"]["packs"] = Value::Array(packs);
    doc["status"]["fips"] = serde_json::json!({"mode": "none"});
    doc["spec"]["clusterConfig"] = serde_json::json!({"timezone": "Etc/UTC",
        "lifecycleConfig": {"pause": false}, "machineHealthConfig": {}});
    doc["spec"]["cloudConfigRef"] = serde_json::json!({"kind": "aws", "uid": "cc1"});
    doc["metadata"]["labels"] = serde_json::json!({"env": "dev"});
    fs::write(&path, serde_json::to_string(&doc).unwrap()).unwrap();
}

/// A cluster record of cluster-a with the tiers that Palette gives, and the stand-in that serves
/// the cluster.
fn cluster_record_with_tiers(root: &Path) -> (MemoryApi, Snapshot) {
    common::make_fixtures(root);
    with_cluster_status(root);
    let api = serve_cluster(MemoryApi::new(), root, "cluster-a");
    let snap = snapshot_cluster(&api, None).unwrap();
    (api, snap)
}

fn cloud_config(size: u64) -> Value {
    serde_json::json!({"spec": {
    "clusterConfig": {"region": "us-east-2", "vpcId": "vpc-mine"},
    "machinePoolConfig": [
        {"name": "worker-pool", "instanceType": "t3.xlarge", "size": size, "azs": ["us-east-2a"],
         "subnetIds": {"us-east-2a": "subnet-mine"}},
        {"name": "cp-pool", "isControlPlane": true, "instanceType": "t3.xlarge", "size": 1},
    ]}})
}

#[test]
fn a_cluster_record_carries_the_tiers_that_palette_gives_and_names_them() {
    let dir = tempfile::tempdir().unwrap();
    let (api, snap) = cluster_record_with_tiers(dir.path());
    let c = snap.source.cluster.as_ref().unwrap();
    // shape costs a request, so a record without --shape does not carry it
    assert_eq!(
        c.tiers.keys().collect::<Vec<_>>(),
        [&Tier::Applied, &Tier::Policy, &Tier::Running]
    );
    assert_eq!(c.labels.get("env").map(String::as_str), Some("dev"));
    assert_eq!(c.tier(Tier::Applied).unwrap().fields["spcHash"], "h1");
    // running holds the covered profile alone, without the condition
    let running = c.tier(Tier::Running).unwrap().fields.as_array().unwrap();
    assert!(
        running
            .iter()
            .all(|r| r["name"] != "other" && r.get("condition").is_none())
    );
    assert_eq!(running.len(), snap.packs.len());
    assert_eq!(c.tier(Tier::Policy).unwrap().fields["timezone"], "Etc/UTC");

    // --shape: the machine pools and the cloud part, without the ids of the cluster
    let palette = Palette::new(
        api.clone()
            .with("/v1/cloudconfigs/aws/cc1", cloud_config(2)),
    );
    let mut raw = palette
        .fetch_subject(SubjectKind::Cluster, CLUSTER_UID, &Progress::silent())
        .unwrap();
    raw.cloud_config = Some(palette.cloud_config(&raw.cluster).unwrap());
    let shaped = Snapshot::from_cluster(&raw, None, "cluster", "t0").unwrap();
    let shape = &shaped
        .source
        .cluster
        .as_ref()
        .unwrap()
        .tier(Tier::Shape)
        .unwrap()
        .fields;
    assert_eq!(shape["cloudType"], "aws");
    assert_eq!(shape["cloud"], serde_json::json!({"region": "us-east-2"}));
    assert_eq!(shape["pools"][0]["name"], "cp-pool", "pools in name order");
    assert!(shape["pools"][1].get("subnetIds").is_none());
    // and the tiers are outside the root
    assert_eq!(shaped.root, snap.root);

    // an EKS cluster: the image family and the launch template of a managed node group, and
    // who reaches the managed control plane, are chosen, so the tier holds them; the size that
    // Palette derives for an instance type is not
    let eks = serde_json::json!({"spec": {
        "clusterConfig": {"region": "us-east-2", "endpointAccess": {"public": true, "private": false},
            "addons": [{"name": "vpc-cni", "version": "v1.19.0"}], "vpcId": "vpc-mine"},
        "machinePoolConfig": [{"name": "worker-pool", "instanceType": "t3.xlarge", "size": 2,
            "amiType": "AL2023_x86_64_STANDARD", "awsLaunchTemplate": {"ami": {"id": "ami-1"}},
            "instanceConfig": {"cpuSet": 4, "memoryMiB": 16384}}]}});
    raw.cluster["spec"]["cloudConfigRef"]["kind"] = Value::from("eks");
    let palette = Palette::new(api.clone().with("/v1/cloudconfigs/eks/cc1", eks));
    raw.cloud_config = Some(palette.cloud_config(&raw.cluster).unwrap());
    let shaped = Snapshot::from_cluster(&raw, None, "cluster", "t0").unwrap();
    let shape = &shaped
        .source
        .cluster
        .as_ref()
        .unwrap()
        .tier(Tier::Shape)
        .unwrap()
        .fields;
    assert_eq!(shape["pools"][0]["amiType"], "AL2023_x86_64_STANDARD");
    assert_eq!(shape["pools"][0]["awsLaunchTemplate"]["ami"]["id"], "ami-1");
    assert!(shape["pools"][0].get("instanceConfig").is_none());
    assert_eq!(shape["cloud"]["endpointAccess"]["public"], true);
    assert_eq!(shape["cloud"]["addons"][0]["name"], "vpc-cni");
    assert!(shape["cloud"].get("vpcId").is_none());

    // a GKE cluster: the managed cluster settings and the cluster API override, which sets the
    // release channel, are chosen, so the tier holds them
    raw.cluster["spec"]["cloudConfigRef"]["kind"] = Value::from("gke");
    raw.cloud_config = Some(serde_json::json!({"spec": {
        "clusterConfig": {"project": "p1", "region": "us-east1",
            "managedClusterConfig": {"location": "us-east1"},
            "overrideClusterAPIConfig": "gcpManagedControlPlane:\n  spec:\n    releaseChannel: regular\n"},
        "machinePoolConfig": [{"name": "worker-pool", "instanceType": "e2-standard-4", "size": 3}]}}));
    let shaped = Snapshot::from_cluster(&raw, None, "cluster", "t0").unwrap();
    let shape = &shaped
        .source
        .cluster
        .as_ref()
        .unwrap()
        .tier(Tier::Shape)
        .unwrap()
        .fields;
    assert_eq!(
        shape["cloud"]["managedClusterConfig"]["location"],
        "us-east1"
    );
    assert!(
        shape["cloud"]["overrideClusterAPIConfig"]
            .as_str()
            .unwrap()
            .contains("releaseChannel: regular")
    );
}

/// Make sure that each key of the object `written` has a property in `named`, the schema of an
/// object. `at` names the place, for the message.
fn assert_keys_named(written: &Value, named: &Value, at: &str) {
    for key in written.as_object().unwrap().keys() {
        assert!(named["properties"].get(key).is_some(), "{at} has {key}");
    }
}

#[test]
fn the_schema_of_the_tiers_names_each_key_of_the_fields_that_a_record_holds() {
    let dir = tempfile::tempdir().unwrap();
    let (api, _) = cluster_record_with_tiers(dir.path());
    let palette = Palette::new(api.with("/v1/cloudconfigs/aws/cc1", cloud_config(2)));
    let mut raw = palette
        .fetch_subject(SubjectKind::Cluster, CLUSTER_UID, &Progress::silent())
        .unwrap();
    raw.cluster["metadata"]["annotations"] =
        serde_json::json!({"spectroComponentsUpgradeForbidden": "true"});
    raw.cloud_config = Some(palette.cloud_config(&raw.cluster).unwrap());
    let snap = Snapshot::from_cluster(&raw, None, "cluster", "t0").unwrap();
    let cluster = snap.source.cluster.as_ref().unwrap();
    assert_eq!(
        cluster.tiers.len(),
        4,
        "applied, policy, running, and shape"
    );

    let schema = serde_json::to_value(schemars::schema_for!(Snapshot)).unwrap();
    let fields = |tier: Tier| common::tier_fields(&schema, tier);
    let written = |tier: Tier| &cluster.tier(tier).unwrap().fields;
    assert_keys_named(written(Tier::Applied), fields(Tier::Applied), "applied");
    for pack in written(Tier::Running).as_array().unwrap() {
        assert_keys_named(pack, &fields(Tier::Running)["items"], "running");
    }
    assert_keys_named(written(Tier::Policy), fields(Tier::Policy), "policy");
    let shape = written(Tier::Shape);
    assert_keys_named(shape, fields(Tier::Shape), "shape");
    // the keys that only some clouds have, such as the region, stay open
    for pool in shape["pools"].as_array().unwrap() {
        assert_keys_named(
            pool,
            &fields(Tier::Shape)["properties"]["pools"]["items"],
            "shape.pools",
        );
    }
}

#[test]
fn the_schema_of_the_policy_tier_names_each_key_of_a_cluster_group() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let mut doc = group_doc(dir.path());
    // values that do not parse give the key `valuesParseError`
    doc["spec"]["clustersConfig"] = clusters_config(6000, "a: [");
    let api = serve_cluster_group(MemoryApi::new(), dir.path()).with(group_route("g1"), doc);
    let group = common::snapshot_of(&api, "clustergroup:g1").unwrap();
    let written = &group
        .source
        .cluster
        .as_ref()
        .unwrap()
        .tier(Tier::Policy)
        .unwrap()
        .fields;
    assert!(written["valuesParseError"].is_string(), "{written}");
    let schema = serde_json::to_value(schemars::schema_for!(Snapshot)).unwrap();
    assert_keys_named(
        written,
        common::tier_fields(&schema, Tier::Policy),
        "the policy of a group",
    );
}

#[test]
fn a_cluster_record_without_tiers_still_verifies() {
    let dir = tempfile::tempdir().unwrap();
    let (api, snap) = cluster_record_with_tiers(dir.path());
    let mut doc = serde_json::to_value(snap).unwrap();
    // a record written before the tiers: no tiers, no labels
    let cluster = doc["source"]["cluster"].as_object_mut().unwrap();
    cluster.remove("tiers");
    cluster.remove("labels");
    let old: Snapshot = serde_json::from_value(doc).unwrap();
    let v = ops::verify(&old, &Palette::new(api), "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged);
    assert_eq!(v.status, VerifyStatus::Unchanged);
    assert!(v.diff.cluster_tiers.is_empty(), "no tier to compare");
}

#[test]
fn compare_names_the_override_that_a_person_sets_on_a_pool_and_the_root_stays() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    with_cluster_status(dir.path());
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a");
    let palette = Palette::new(
        api.clone()
            .with("/v1/cloudconfigs/aws/cc1", cloud_config(2)),
    );
    let mut raw = palette
        .fetch_subject(SubjectKind::Cluster, CLUSTER_UID, &Progress::silent())
        .unwrap();
    raw.cloud_config = Some(palette.cloud_config(&raw.cluster).unwrap());
    let before = Snapshot::from_cluster(&raw, None, "before", "t0").unwrap();

    // the worker pool gets an override: Palette gives the field flat, in the pool
    let pool = &mut raw.cloud_config.as_mut().unwrap()["spec"]["machinePoolConfig"][0];
    assert_eq!(pool["name"], "worker-pool");
    pool["overrideHealthCheckConfiguration"] = Value::from("machineHealthCheck: {}\n");
    let after = Snapshot::from_cluster(&raw, None, "after", "t0").unwrap();

    let shape = |s: &Snapshot| {
        s.source
            .cluster
            .as_ref()
            .unwrap()
            .tier(Tier::Shape)
            .unwrap()
            .clone()
    };
    assert_eq!(
        shape(&after).fields["pools"][1]["overrideHealthCheckConfiguration"],
        "machineHealthCheck: {}\n"
    );
    assert_ne!(shape(&before).hash, shape(&after).hash);

    let d = strata::diff::compute(&before, &after, "t1", None);
    assert!(d.same, "a tier is outside the root");
    let tier = d
        .cluster_tiers
        .iter()
        .find(|t| t.tier == Tier::Shape)
        .unwrap();
    assert!(!tier.same);
    assert_eq!(
        tier.diff[0].path,
        "pools[name=worker-pool].overrideHealthCheckConfiguration"
    );
    let text = common::terminal()
        .render(&d, &before, &after, false)
        .unwrap();
    assert!(
        text.contains("+ shape.pools[name=worker-pool].overrideHealthCheckConfiguration:"),
        "{text}"
    );
}

#[test]
fn compare_names_the_tier_and_the_field_that_differ_and_a_pack_behind_its_profile() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    with_cluster_status(dir.path());
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a");
    let palette = Palette::new(
        api.clone()
            .with("/v1/cloudconfigs/aws/cc1", cloud_config(2)),
    );
    let mut raw = palette
        .fetch_subject(SubjectKind::Cluster, CLUSTER_UID, &Progress::silent())
        .unwrap();
    raw.cloud_config = Some(palette.cloud_config(&raw.cluster).unwrap());
    let a = Snapshot::from_cluster(&raw, None, "a", "t0").unwrap();

    // the other cluster: a larger worker pool, another timezone, and one pack behind
    raw.cloud_config = Some(cloud_config(3));
    raw.cluster["spec"]["clusterConfig"]["timezone"] = Value::from("America/Chicago");
    let first = raw.cluster["status"]["packs"][0]["name"]
        .as_str()
        .unwrap()
        .to_string();
    let declared = a
        .packs
        .iter()
        .find(|p| p.name == first)
        .unwrap()
        .version
        .clone();
    raw.cluster["status"]["packs"][0]["version"] = Value::from("0.0.1");
    let b = Snapshot::from_cluster(&raw, None, "b", "t0").unwrap();

    let d = strata::diff::compute(&a, &b, "t1", None);
    assert!(d.same, "the release is the same");
    let tier = |name: Tier| d.cluster_tiers.iter().find(|t| t.tier == name).unwrap();
    assert!(tier(Tier::Applied).same);
    assert!(!tier(Tier::Policy).same);
    assert_eq!(tier(Tier::Policy).diff[0].path, "timezone");
    assert_eq!(
        tier(Tier::Shape).diff[0].path,
        "pools[name=worker-pool].size"
    );
    assert!(!tier(Tier::Running).same);
    assert!(tier(Tier::Labels).same);
    assert_eq!(d.running_behind.len(), 1);
    let behind = &d.running_behind[0];
    assert_eq!(
        (behind.side, behind.pack.as_str()),
        (Side::B, first.as_str())
    );
    assert_eq!(
        (behind.declared.as_str(), behind.running.as_str()),
        (declared.as_str(), "0.0.1")
    );
    let text = common::terminal().render(&d, &a, &b, false).unwrap();
    assert!(text.contains("policy CHANGED"), "{text}");
    assert!(
        text.contains("~ shape.pools[name=worker-pool].size: 2 -> 3"),
        "{text}"
    );
    assert!(
        text.contains(&format!(
            "runs {first} 0.0.1, and its profile declares {declared}"
        )),
        "{text}"
    );
}

#[test]
fn between_two_clusters_the_applied_tier_compares_the_infra_hash_alone() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    with_cluster_status(dir.path());
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a");
    let palette = Palette::new(api);
    let mut raw = palette
        .fetch_subject(SubjectKind::Cluster, CLUSTER_UID, &Progress::silent())
        .unwrap();
    let a = Snapshot::from_cluster(&raw, None, "a", "t0").unwrap();
    // another cluster from the same definition: Palette gives it its own spcHash
    raw.cluster["metadata"]["uid"] = Value::from("another-cluster");
    raw.cluster["status"]["spcApply"]["spcHash"] = Value::from("h2");
    let b = Snapshot::from_cluster(&raw, None, "b", "t0").unwrap();
    let d = strata::diff::compute(&a, &b, "t1", None);
    let applied = d
        .cluster_tiers
        .iter()
        .find(|t| t.tier == Tier::Applied)
        .unwrap();
    assert!(applied.same, "{:?}", applied.diff);
    // the same cluster with another spcHash has changed
    raw.cluster["metadata"]["uid"] = Value::from(CLUSTER_UID);
    let again = Snapshot::from_cluster(&raw, None, "a", "t1").unwrap();
    let d = strata::diff::compute(&a, &again, "t1", None);
    let applied = d
        .cluster_tiers
        .iter()
        .find(|t| t.tier == Tier::Applied)
        .unwrap();
    assert!(!applied.same);
    assert_eq!(applied.diff[0].path, "spcHash");
}

// ---- the shape tier on each cloud (#105) -------------------------------------------------------

/// A cloud config of AWS, as Palette gives it: each pool has its architecture, and the ids that
/// Palette gives the cluster.
fn aws_cloud_config(size: u64) -> Value {
    serde_json::json!({"spec": {
    "clusterConfig": {"region": "us-east-2", "sshKeyName": "platform", "vpcId": "vpc-fred",
        "hybridConfig": {"iamRolesAnywhere": {}}},
    "machinePoolConfig": [
        {"name": "worker-pool", "isControlPlane": false, "useControlPlaneAsWorker": false,
         "instanceType": "t3.xlarge", "size": size, "azs": ["us-east-2a"], "labels": ["worker"],
         "taints": [], "rootDeviceSize": 65, "capacityType": "on-demand",
         "updateStrategy": {"type": "RollingUpdateScaleOut"}, "skipK8sUpgrade": "disabled",
         "machinePoolProperties": {"archType": "amd64"},
         "instanceConfig": {"name": "t3.xlarge", "numCPUs": 4, "memoryMiB": 16384},
         "additionalSecurityGroups": [{"id": "sg-fred"}],
         "subnetIds": {"us-east-2a": "subnet-fred"}},
        {"name": "cp-pool", "isControlPlane": true, "useControlPlaneAsWorker": false,
         "instanceType": "t3.xlarge", "size": 1, "azs": ["us-east-2a"],
         "labels": ["control-plane"], "taints": [], "rootDeviceSize": 65,
         "capacityType": "on-demand", "updateStrategy": {"type": "RollingUpdateScaleOut"},
         "machinePoolProperties": {"archType": "amd64"}, "additionalSecurityGroups": [],
         "subnetIds": {"us-east-2a": "subnet-fred"}},
    ]}})
}

/// A cloud config of EKS, as Palette gives it.
fn eks_cloud_config() -> Value {
    serde_json::json!({"spec": {
        "clusterConfig": {"region": "us-east-2", "sshKeyName": "platform", "bastionDisabled": true,
            "endpointAccess": {"public": true, "publicCIDRs": [], "privateCIDRs": []},
            "addons": [], "encryptionConfig": {"isEnabled": true, "provider": "arn:aws:kms:key-fred"},
            "vpcId": "vpc-fred"},
        "machinePoolConfig": [{"name": "worker-pool", "isControlPlane": false,
            "useControlPlaneAsWorker": false, "instanceType": "t3.xlarge", "size": 2,
            "azs": ["us-east-2a", "us-east-2b"], "labels": ["worker"], "taints": [],
            "rootDeviceSize": 60, "capacityType": "on-demand", "amiType": "AL2023_x86_64_STANDARD",
            "awsLaunchTemplate": {"additionalSecurityGroups": [], "ami": {}, "rootVolume": {}},
            "updateStrategy": {"type": "RollingUpdateScaleOut"},
            "machinePoolProperties": {"archType": "amd64"},
            "subnetIds": {"us-east-2a": "subnet-fred"}}]}})
}

/// A cloud config of GKE, as Palette gives it, in the model of GCP.
fn gke_cloud_config() -> Value {
    serde_json::json!({"spec": {
        "clusterConfig": {"project": "barney-project", "region": "us-east1",
            "managedClusterConfig": {"location": "us-east1"}, "network": "barney-net",
            "overrideClusterAPIConfig": "gcpManagedControlPlane:\n  spec:\n    releaseChannel: regular\n"},
        "machinePoolConfig": [{"name": "worker-pool", "isControlPlane": false,
            "instanceType": "e2-standard-4", "size": 3, "azs": null, "labels": ["worker"],
            "taints": [], "rootDeviceSize": 60, "skipK8sUpgrade": "disabled",
            "updateStrategy": {"type": "RollingUpdateScaleOut"}, "subnet": "barney-subnet",
            "machinePoolProperties": {"archType": "amd64"}, "subnetIds": {"us-east1": "subnet-1"}}]}})
}

/// A cloud config of vSphere, in the model of the API reference of Palette.
fn vsphere_cloud_config() -> Value {
    let network = serde_json::json!({"networkName": "VM Network", "staticIp": true,
        "parentPoolRef": {"uid": "pool-barney"}});
    serde_json::json!({"spec": {
    "clusterConfig": {
        "placement": {"uid": "placement-of-cluster", "datacenter": "dc-east",
            "folder": "spectro/barney", "imageTemplateFolder": "spectro-templates",
            "network": network},
        "ntpServers": ["pool.ntp.org"], "staticIp": true, "sshKeys": ["ssh-ed25519 AAAA barney"],
        "controlPlaneEndpoint": {"type": "IP", "host": "10.10.0.10"}},
    "machinePoolConfig": [
        {"name": "worker-pool", "isControlPlane": false, "size": 2,
         "instanceType": {"diskGiB": 80, "memoryMiB": 16384, "numCPUs": 8},
         "placements": [{"uid": "placement-of-pool", "cluster": "compute-1",
            "datastore": "ds-fast", "resourcePool": "rp-barney", "folder": "spectro/barney",
            "network": network}],
         "machinePoolProperties": {"archType": "amd64"}, "labels": ["worker"], "taints": []},
        {"name": "cp-pool", "isControlPlane": true, "size": 1,
         "instanceType": {"diskGiB": 60, "memoryMiB": 8192, "numCPUs": 4},
         "placements": [{"uid": "placement-of-cp", "cluster": "compute-1",
            "datastore": "ds-fast", "resourcePool": "rp-barney", "folder": "spectro/barney",
            "network": network}],
         "machinePoolProperties": {"archType": "amd64"}, "labels": ["control-plane"],
         "taints": []},
    ]}})
}

/// A cloud config of Azure, in the model of the API reference of Palette.
fn azure_cloud_config() -> Value {
    serde_json::json!({"spec": {
        "clusterConfig": {"location": "eastus", "resourceGroup": "rg-fred",
            "subscriptionId": "sub-fred", "sshKey": "ssh-ed25519 AAAA fred",
            "vnetName": "vnet-fred", "vnetResourceGroup": "rg-net", "vnetCidrBlock": "10.0.0.0/16",
            "controlPlaneSubnet": {"name": "cp-subnet", "cidrBlock": "10.0.0.0/24"},
            "workerSubnet": {"name": "worker-subnet", "cidrBlock": "10.0.1.0/24"},
            "storageAccountName": "fredstore", "containerName": "fred"},
        "machinePoolConfig": [{"name": "worker-pool", "isControlPlane": false, "size": 2,
            "instanceType": "Standard_D4s_v3", "azs": ["1", "2"], "isSystemNodePool": true,
            "osType": "Linux", "osDisk": {"diskSizeGB": 60, "osType": "Linux",
                "managedDisk": {"storageAccountType": "Premium_LRS"}},
            "machinePoolProperties": {"archType": "amd64"}, "labels": ["worker"], "taints": []}]}})
}

/// A cloud config of edge native, in the model of the API reference of Palette.
fn edge_native_cloud_config() -> Value {
    serde_json::json!({"spec": {
        "clusterConfig": {"controlPlaneEndpoint": {"type": "IP", "host": "10.20.0.5"},
            "isTwoNodeCluster": false, "ntpServers": ["pool.ntp.org"], "staticIp": false,
            "sshKeys": ["ssh-ed25519 AAAA barney"],
            "overlayNetworkConfiguration": {"enable": true, "cidr": "100.64.192.0/23"}},
        "machinePoolConfig": [{"name": "edge-pool", "isControlPlane": true,
            "useControlPlaneAsWorker": true, "size": 1, "osType": "linux",
            "hosts": [{"hostUid": "edge-barney", "hostAddress": "10.20.0.11",
                "hostName": "barney-edge-1"}],
            "machinePoolProperties": {"archType": "arm64"}, "labels": ["control-plane"],
            "taints": []}]}})
}

/// A cloud config of OpenStack, in the model of the API reference of Palette.
fn openstack_cloud_config() -> Value {
    let pool = |name: &str, cp: bool, size: u32, label: &str| {
        serde_json::json!({"name": name, "isControlPlane": cp, "useControlPlaneAsWorker": false,
            "size": size, "azs": ["nova"], "flavor": "m1.large",
            "flavorConfig": {"name": "m1.large", "numCPUs": 4, "memoryMiB": 8192, "diskGiB": 80},
            "image": "ubuntu-22.04-barney", "diskGiB": 80,
            "subnet": {"id": "subnet-barney", "name": "barney-subnet"},
            "machinePoolProperties": {"archType": "amd64"}, "labels": [label], "taints": []})
    };
    serde_json::json!({"spec": {
        "clusterConfig": {"region": "RegionOne", "project": {"id": "p-barney", "name": "barney"},
            "domain": {"id": "default", "name": "Default"}, "sshKeyName": "barney-key",
            "bastionDisabled": true, "nodeCidr": "10.6.0.0/24", "dnsNameservers": ["10.6.0.2"]},
        "machinePoolConfig": [pool("cp-pool", true, 1, "control-plane"),
            pool("worker-pool", false, 2, "worker")]}})
}

/// A cloud config of CloudStack, in the model of the API reference of Palette.
fn cloudstack_cloud_config() -> Value {
    serde_json::json!({"spec": {
        "clusterConfig": {"project": {"id": "p-fred", "name": "fred"},
            "zones": [{"id": "zone-1", "name": "zone-fred", "network": {"name": "fred-net"}}],
            "sshKeyName": "fred-key", "controlPlaneEndpoint": "10.30.0.5", "syncWithCKS": false},
        "machinePoolConfig": [{"name": "worker-pool", "isControlPlane": false, "size": 2,
            "offering": {"name": "Medium Instance"}, "template": {"name": "ubuntu-22.04-fred"},
            "networks": [{"id": "net-1", "name": "fred-net"}],
            "instanceConfig": {"numCPUs": 2, "memoryMiB": 4096},
            "machinePoolProperties": {"archType": "amd64"}, "labels": ["worker"], "taints": []}]}})
}

/// The shape tier of the cluster of the fixtures on the cloud type `kind`, from the cloud config
/// `cloud_config` that Palette serves at `/v1/cloudconfigs/<kind>/cc1`.
fn shape_on(kind: &str, cloud_config: Value) -> Value {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    with_cluster_status(dir.path());
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a")
        .with(format!("/v1/cloudconfigs/{kind}/cc1"), cloud_config);
    let palette = Palette::new(api);
    let mut raw = palette
        .fetch_subject(SubjectKind::Cluster, CLUSTER_UID, &Progress::silent())
        .unwrap();
    raw.cluster["spec"]["cloudConfigRef"]["kind"] = Value::from(kind);
    raw.cloud_config = Some(palette.cloud_config(&raw.cluster).unwrap());
    let snap = Snapshot::from_cluster(&raw, None, "cluster", "t0").unwrap();
    snap.source
        .cluster
        .unwrap()
        .tier(Tier::Shape)
        .unwrap()
        .fields
        .clone()
}

#[test]
fn shape_of_a_cluster_that_names_no_cloud_config_gives_its_own_kind_and_not_usage() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    // the document of the fixture names no cloud config
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a");
    let shape = ops::SnapshotOptions {
        shape: true,
        ..ops::SnapshotOptions::default()
    };
    let err = ops::snapshot_for(
        &cluster_ref(CLUSTER_UID, None, false),
        &shape,
        || Ok(Palette::new(&api)),
        "t0",
        &Progress::silent(),
    )
    .unwrap_err();
    assert!(
        matches!(&err, Error::NoCloudConfig { cluster } if cluster == CLUSTER_UID),
        "{err}"
    );
    assert_eq!(err.kind(), "no-cloud-config");
    assert_eq!(err.to_json()["cluster"], CLUSTER_UID);
    // the document is the only request: the cloud config route is not asked
    assert_eq!(api.calls(&cluster_route(CLUSTER_UID)), 1);
}

#[test]
fn the_shape_tier_of_a_vsphere_cluster_holds_the_placements_and_the_cloud_part() {
    let shape = shape_on("vsphere", vsphere_cloud_config());
    assert_eq!(shape["cloudType"], "vsphere");
    for pool in shape["pools"].as_array().unwrap() {
        let placement = &pool["placements"][0];
        assert_eq!(placement["datastore"], "ds-fast");
        assert_eq!(placement["resourcePool"], "rp-barney");
        assert!(placement.get("uid").is_none(), "Palette gives the uid");
        assert!(pool["instanceType"]["numCPUs"].is_number());
    }
    let cloud = &shape["cloud"];
    assert_eq!(cloud["placement"]["datacenter"], "dc-east");
    assert!(cloud["placement"].get("uid").is_none());
    assert_eq!(cloud["ntpServers"][0], "pool.ntp.org");
    assert_eq!(cloud["staticIp"], true);
    assert_eq!(cloud["controlPlaneEndpoint"]["host"], "10.10.0.10");
}

#[test]
fn the_shape_tier_of_an_azure_cluster_holds_the_os_disk_and_the_network_that_a_person_chooses() {
    for kind in ["azure", "aks"] {
        let shape = shape_on(kind, azure_cloud_config());
        let pool = &shape["pools"][0];
        assert_eq!(pool["osDisk"]["diskSizeGB"], 60, "{kind}");
        assert_eq!(pool["isSystemNodePool"], true);
        assert_eq!(pool["osType"], "Linux");
        let cloud = &shape["cloud"];
        assert_eq!(cloud["location"], "eastus");
        assert_eq!(cloud["resourceGroup"], "rg-fred");
        assert_eq!(cloud["vnetName"], "vnet-fred");
        assert_eq!(cloud["workerSubnet"]["name"], "worker-subnet");
        assert!(cloud.get("storageAccountName").is_none());
    }
}

#[test]
fn the_shape_tier_of_an_edge_native_cluster_holds_the_hosts_and_the_os_type_of_each_pool() {
    let shape = shape_on("edge-native", edge_native_cloud_config());
    let pool = &shape["pools"][0];
    assert_eq!(pool["hosts"][0]["hostUid"], "edge-barney");
    assert_eq!(pool["osType"], "linux");
    assert_eq!(shape["cloud"]["isTwoNodeCluster"], false);
    assert_eq!(
        shape["cloud"]["overlayNetworkConfiguration"]["enable"],
        true
    );
}

#[test]
fn each_pool_on_each_cloud_holds_its_architecture_and_no_id_that_palette_gives_the_cluster() {
    let clouds = [
        ("aws", aws_cloud_config(2)),
        ("eks", eks_cloud_config()),
        ("gke", gke_cloud_config()),
        ("gcp", gke_cloud_config()),
        ("vsphere", vsphere_cloud_config()),
        ("azure", azure_cloud_config()),
        ("edge-native", edge_native_cloud_config()),
        ("openstack", openstack_cloud_config()),
        ("apache-cloudstack", cloudstack_cloud_config()),
    ];
    for (kind, cc) in clouds {
        let shape = shape_on(kind, cc);
        assert_eq!(shape["cloudType"], kind);
        for pool in shape["pools"].as_array().unwrap() {
            let arch = &pool["machinePoolProperties"]["archType"];
            assert!(arch == "amd64" || arch == "arm64", "{kind}: {pool}");
            for id in ["subnetIds", "additionalSecurityGroups"] {
                assert!(pool.get(id).is_none(), "{kind}: {id}");
            }
        }
        assert!(shape["cloud"].get("vpcId").is_none(), "{kind}");
    }
    // the cloud part of EKS holds the encryption of its secrets, and a GCP pool its subnet
    let eks = shape_on("eks", eks_cloud_config());
    assert_eq!(eks["cloud"]["encryptionConfig"]["isEnabled"], true);
    let gke = shape_on("gke", gke_cloud_config());
    assert_eq!(gke["pools"][0]["subnet"], "barney-subnet");
    assert_eq!(gke["cloud"]["network"], "barney-net");
}

#[test]
fn the_shape_tier_of_an_openstack_cluster_holds_the_cloud_part_that_an_older_record_holds() {
    let shape = shape_on("openstack", openstack_cloud_config());
    let cloud = &shape["cloud"];
    assert_eq!(cloud["region"], "RegionOne");
    assert_eq!(cloud["project"]["name"], "barney");
    assert_eq!(cloud["sshKeyName"], "barney-key");
    assert_eq!(cloud["bastionDisabled"], true);
    assert_eq!(cloud["nodeCidr"], "10.6.0.0/24");
    let pool = &shape["pools"][0];
    assert_eq!(pool["flavorConfig"]["name"], "m1.large");
    assert_eq!(pool["image"], "ubuntu-22.04-barney");
    assert_eq!(pool["subnet"]["name"], "barney-subnet");
}

#[test]
fn the_shape_tier_of_a_cloudstack_cluster_holds_the_offering_and_not_the_instance_config() {
    let shape = shape_on("apache-cloudstack", cloudstack_cloud_config());
    let pool = &shape["pools"][0];
    assert_eq!(pool["offering"]["name"], "Medium Instance");
    assert_eq!(pool["template"]["name"], "ubuntu-22.04-fred");
    assert_eq!(pool["networks"][0]["name"], "fred-net");
    assert!(
        pool.get("instanceConfig").is_none(),
        "Palette gives it from the offering"
    );
    let cloud = &shape["cloud"];
    assert_eq!(cloud["project"]["name"], "fred");
    assert_eq!(cloud["zones"][0]["name"], "zone-fred");
    assert_eq!(cloud["sshKeyName"], "fred-key");
}

#[test]
fn a_cloud_type_that_strata_does_not_name_holds_the_fields_of_an_older_record() {
    let shape = shape_on("custom", openstack_cloud_config());
    assert_eq!(shape["cloudType"], "custom");
    let cloud = &shape["cloud"];
    assert_eq!(cloud["region"], "RegionOne");
    assert_eq!(cloud["project"]["name"], "barney");
    assert_eq!(cloud["sshKeyName"], "barney-key");
    assert_eq!(cloud["bastionDisabled"], true);
    assert!(cloud.get("nodeCidr").is_none());
    let pool = &shape["pools"][0];
    assert_eq!(pool["azs"][0], "nova");
    assert_eq!(pool["machinePoolProperties"]["archType"], "amd64");
    assert!(pool.get("flavorConfig").is_none());
}

/// The shape fields that strata recorded before it read them by cloud type, for each cloud of
/// the live example, from the cloud configs above.
fn shape_before_the_cloud_type(kind: &str) -> Value {
    let update = serde_json::json!({"type": "RollingUpdateScaleOut"});
    match kind {
        "aws" => serde_json::json!({"cloud": {"region": "us-east-2", "sshKeyName": "platform"},
            "pools": [
                {"name": "cp-pool", "isControlPlane": true, "useControlPlaneAsWorker": false,
                 "instanceType": "t3.xlarge", "size": 1, "azs": ["us-east-2a"],
                 "labels": ["control-plane"], "taints": [], "rootDeviceSize": 65,
                 "capacityType": "on-demand", "updateStrategy": update},
                {"name": "worker-pool", "isControlPlane": false, "useControlPlaneAsWorker": false,
                 "instanceType": "t3.xlarge", "size": 2, "azs": ["us-east-2a"],
                 "labels": ["worker"], "taints": [], "rootDeviceSize": 65,
                 "capacityType": "on-demand", "updateStrategy": update,
                 "skipK8sUpgrade": "disabled"}]}),
        "eks" => serde_json::json!({"cloud": {"region": "us-east-2", "sshKeyName": "platform",
                "bastionDisabled": true, "addons": [],
                "endpointAccess": {"public": true, "publicCIDRs": [], "privateCIDRs": []}},
            "pools": [{"name": "worker-pool", "isControlPlane": false,
                "useControlPlaneAsWorker": false, "instanceType": "t3.xlarge", "size": 2,
                "azs": ["us-east-2a", "us-east-2b"], "labels": ["worker"], "taints": [],
                "rootDeviceSize": 60, "capacityType": "on-demand",
                "amiType": "AL2023_x86_64_STANDARD",
                "awsLaunchTemplate": {"additionalSecurityGroups": [], "ami": {}, "rootVolume": {}},
                "updateStrategy": update}]}),
        "openstack" | "custom" => {
            let pool = |name: &str, cp: bool, size: u32, label: &str| {
                serde_json::json!({"name": name, "isControlPlane": cp,
                    "useControlPlaneAsWorker": false, "size": size, "azs": ["nova"],
                    "labels": [label], "taints": []})
            };
            serde_json::json!({"cloud": {"region": "RegionOne",
                    "project": {"id": "p-barney", "name": "barney"}, "sshKeyName": "barney-key",
                    "bastionDisabled": true},
                "pools": [pool("cp-pool", true, 1, "control-plane"),
                    pool("worker-pool", false, 2, "worker")]})
        }
        _ => serde_json::json!({"cloud": {"project": "barney-project", "region": "us-east1",
                "managedClusterConfig": {"location": "us-east1"},
                "overrideClusterAPIConfig": "gcpManagedControlPlane:\n  spec:\n    releaseChannel: regular\n"},
            "pools": [{"name": "worker-pool", "isControlPlane": false,
                "instanceType": "e2-standard-4", "size": 3, "labels": ["worker"], "taints": [],
                "rootDeviceSize": 60, "skipK8sUpgrade": "disabled", "updateStrategy": update}]}),
    }
}

/// Verify a record that an older strata wrote of a cluster on the cloud type `kind`, with the
/// cloud config `cc`. The cluster did not change, so the shape tier is the same.
fn an_older_record_verifies(kind: &str, cc: Value) {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    with_cluster_status(dir.path());
    let path = dir.path().join("cluster-a").join("cluster.json");
    let mut doc = read_json(&path);
    doc["spec"]["cloudConfigRef"]["kind"] = Value::from(kind);
    fs::write(&path, serde_json::to_string(&doc).unwrap()).unwrap();
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a")
        .with(format!("/v1/cloudconfigs/{kind}/cc1"), cc);

    // the record as an older strata wrote it: a shape tier without the cloud type
    let mut record = serde_json::to_value(snapshot_cluster(&api, None).unwrap()).unwrap();
    let fields = shape_before_the_cloud_type(kind);
    // compare and verify read the fields of a tier, and not its hash
    record["source"]["cluster"]["tiers"]["shape"] =
        serde_json::json!({"hash": "0c27a8402b75", "fields": fields});
    let old: Snapshot = serde_json::from_value(record).unwrap();

    let v = ops::verify(&old, &Palette::new(api.clone()), "t1", &Progress::silent()).unwrap();
    assert_eq!(v.status, VerifyStatus::Unchanged, "{kind}");
    let shape = v.diff.cluster_tiers.iter().find(|t| t.tier == Tier::Shape);
    assert!(shape.unwrap().same, "{kind}: {:?}", shape.unwrap().diff);
    // the live record holds the fields of its cloud, and compares with the old record
    let live = &v
        .live
        .source
        .cluster
        .as_ref()
        .unwrap()
        .tier(Tier::Shape)
        .unwrap()
        .fields;
    assert_eq!(live["cloudType"], kind);
    let d = strata::diff::compute(&old, &v.live, "t1", None);
    assert!(d.cluster_tiers.iter().all(|t| t.same), "{kind}");
}

#[test]
fn a_record_of_openstack_or_another_cloud_from_before_the_shape_by_cloud_type_verifies() {
    an_older_record_verifies("openstack", openstack_cloud_config());
    an_older_record_verifies("custom", openstack_cloud_config());
}

#[test]
fn a_record_of_aws_eks_or_gke_from_before_the_shape_by_cloud_type_loads_and_verifies() {
    an_older_record_verifies("aws", aws_cloud_config(2));
    an_older_record_verifies("eks", eks_cloud_config());
    an_older_record_verifies("gke", gke_cloud_config());

    // a change of a field that the old record holds still shows
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    with_cluster_status(dir.path());
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a")
        .with("/v1/cloudconfigs/aws/cc1", aws_cloud_config(3));
    let mut record = serde_json::to_value(snapshot_cluster(&api, None).unwrap()).unwrap();
    let fields = shape_before_the_cloud_type("aws");
    record["source"]["cluster"]["tiers"]["shape"] =
        serde_json::json!({"hash": "0c27a8402b75", "fields": fields});
    let old: Snapshot = serde_json::from_value(record).unwrap();
    let v = ops::verify(&old, &Palette::new(api), "t1", &Progress::silent()).unwrap();
    let shape = v
        .diff
        .cluster_tiers
        .iter()
        .find(|t| t.tier == Tier::Shape)
        .unwrap();
    assert!(!shape.same);
    assert_eq!(shape.diff.len(), 1, "{:?}", shape.diff);
    assert_eq!(shape.diff[0].path, "pools[name=worker-pool].size");
}

// ---- the images and the charts of a pack (#77) ---------------------------------------------------

const CSI_IMAGES_63: &str = "pack:\n  content:\n    images:\n      - image: registry.k8s.io/sig-storage/csi-provisioner:v5.1.0\n      - image: public.ecr.aws/ebs-csi-driver/aws-ebs-csi-driver:v1.63.1\n      - image: registry.k8s.io/sig-storage/livenessprobe:v2.14.0\n    charts:\n      - repo: https://kubernetes-sigs.github.io/aws-ebs-csi-driver\n        name: aws-ebs-csi-driver\n        version: 2.63.1\ncharts:\n  aws-ebs-csi-driver:\n    storageClasses: []\n";
const CSI_IMAGES_65: &str = "pack:\n  content:\n    images:\n      - image: public.ecr.aws/ebs-csi-driver/aws-ebs-csi-driver:v1.65.0\n      - image: registry.k8s.io/sig-storage/csi-provisioner:v5.1.0\n      - image: registry.k8s.io/sig-storage/csi-snapshotter:v8.2.0\n    charts:\n      - repo: https://kubernetes-sigs.github.io/aws-ebs-csi-driver\n        name: aws-ebs-csi-driver\n        version: 2.65.0\ncharts:\n  aws-ebs-csi-driver:\n    storageClasses: []\n";

fn serve_image_versions() -> MemoryApi {
    serve_summaries(
        vec![
            csi_summary("pk-63", "1.63.1", "reg-public", "sha256:aaa", CSI_IMAGES_63),
            csi_summary("pk-65", "1.65.0", "reg-public", "sha256:bbb", CSI_IMAGES_65),
        ],
        &[("reg-public", public_entity())],
    )
}

#[test]
fn a_record_carries_the_images_and_the_charts_of_a_pack() {
    let api = serve_image_versions();
    let old = snapshot_of(&api, "pack:csi-aws-ebs@1.63.1").unwrap();
    let p = &old.packs[0];
    assert_eq!(
        p.images,
        [
            "public.ecr.aws/ebs-csi-driver/aws-ebs-csi-driver:v1.63.1",
            "registry.k8s.io/sig-storage/csi-provisioner:v5.1.0",
            "registry.k8s.io/sig-storage/livenessprobe:v2.14.0",
        ],
        "sorted"
    );
    assert_eq!(p.charts.len(), 1);
    assert_eq!(
        (p.charts[0].name.as_str(), p.charts[0].version.as_str()),
        ("aws-ebs-csi-driver", "2.63.1")
    );
    // a pack whose values declare none carries none, and writes no field
    let plain = snapshot_of(
        &serve_registry(vec![], &[("reg-public", public_entity())]),
        "pack:csi-aws-ebs@1.65.0",
    )
    .unwrap();
    assert!(plain.packs[0].images.is_empty());
    assert!(
        serde_json::to_value(&plain).unwrap()["packs"][0]
            .get("images")
            .is_none()
    );
}

#[test]
fn compare_of_two_pack_versions_names_the_images_and_the_charts_that_moved() {
    let api = serve_image_versions();
    let a = snapshot_of(&api, "pack:csi-aws-ebs@1.63.1").unwrap();
    let b = snapshot_of(&api, "pack:csi-aws-ebs@1.65.0").unwrap();
    let d = strata::diff::compute(&a, &b, "t1", None);
    let pd = &d.packs[0];
    assert!(
        pd.authored && pd.images && pd.charts,
        "IMAGES and CHARTS beside AUTHORED"
    );
    let moved: Vec<(String, &str, &str)> = pd
        .image_changes
        .iter()
        .map(|c| {
            (
                format!("{:?}", c.op),
                c.name.as_str(),
                c.new.as_deref().or(c.old.as_deref()).unwrap(),
            )
        })
        .collect();
    assert_eq!(
        moved,
        [
            (
                "Changed".to_string(),
                "public.ecr.aws/ebs-csi-driver/aws-ebs-csi-driver",
                "v1.65.0"
            ),
            (
                "Added".to_string(),
                "registry.k8s.io/sig-storage/csi-snapshotter",
                "v8.2.0"
            ),
            (
                "Removed".to_string(),
                "registry.k8s.io/sig-storage/livenessprobe",
                "v2.14.0"
            ),
        ]
    );
    assert_eq!(d.summary.images_changed, 3);
    assert_eq!(d.summary.charts_changed, 1);
    let text = common::terminal().render(&d, &a, &b, false).unwrap();
    assert!(text.contains("AUTHORED IMAGES CHARTS"), "{text}");
    assert!(
        text.contains(
            "~ image public.ecr.aws/ebs-csi-driver/aws-ebs-csi-driver: v1.63.1 -> v1.65.0"
        ),
        "{text}"
    );
    assert!(
        text.contains("+ image registry.k8s.io/sig-storage/csi-snapshotter:v8.2.0"),
        "{text}"
    );
    assert!(
        text.contains("~ chart aws-ebs-csi-driver: 2.63.1 -> 2.65.0"),
        "{text}"
    );
    assert!(
        text.contains("images     3 images, 1 chart changed in 1 pack"),
        "{text}"
    );
    // zero reads `no charts` here, as it does in each other line of a report
    let mut no_chart = d.clone();
    no_chart.summary.charts_changed = 0;
    let text = common::terminal().render(&no_chart, &a, &b, false).unwrap();
    assert!(
        text.contains("images     3 images, no charts changed in 1 pack"),
        "{text}"
    );
}

#[test]
fn a_record_written_before_the_image_lists_gets_them_from_its_values_and_still_verifies() {
    let api = serve_image_versions();
    let fresh = snapshot_of(&api, "pack:csi-aws-ebs@1.63.1").unwrap();
    let mut doc = serde_json::to_value(&fresh).unwrap();
    let pack = doc["packs"][0].as_object_mut().unwrap();
    pack.remove("images");
    pack.remove("charts");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.json");
    write_record(&path, &doc);
    let old = Snapshot::load(&path).unwrap();
    assert_eq!(old.packs[0].images, fresh.packs[0].images);
    assert_eq!(old.root, fresh.root);
    let v = ops::verify(&old, &Palette::new(api), "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged);
    assert!(!v.diff.packs[0].images);
}

#[test]
fn an_image_under_a_path_that_holds_the_pack_version_keeps_one_repository() {
    // Palette mirrors the images of a pack version under a path that holds the version
    let values = |v: &str, tag: &str| {
        format!(
            "pack:\n  content:\n    images:\n      - image: us-docker.pkg.dev/palette-images/packs/csi-aws-ebs/{v}/aws-ebs-csi-driver:{tag}\ncharts: {{}}\n"
        )
    };
    let api = serve_summaries(
        vec![
            csi_summary(
                "pk-63",
                "1.63.1",
                "reg-public",
                "sha256:aaa",
                &values("1.63.1", "v1.63.1"),
            ),
            csi_summary(
                "pk-65",
                "1.65.0",
                "reg-public",
                "sha256:bbb",
                &values("1.65.0", "v1.65.0"),
            ),
        ],
        &[("reg-public", public_entity())],
    );
    let a = snapshot_of(&api, "pack:csi-aws-ebs@1.63.1").unwrap();
    let b = snapshot_of(&api, "pack:csi-aws-ebs@1.65.0").unwrap();
    let d = strata::diff::compute(&a, &b, "t1", None);
    let changes = &d.packs[0].image_changes;
    assert_eq!(
        changes.len(),
        1,
        "one image retagged, not one removed and one added"
    );
    assert_eq!(
        changes[0].name,
        "us-docker.pkg.dev/palette-images/packs/csi-aws-ebs/<version>/aws-ebs-csi-driver"
    );
    assert_eq!(changes[0].old.as_deref(), Some("v1.63.1"));
    assert_eq!(changes[0].new.as_deref(), Some("v1.65.0"));
}

#[test]
fn a_pending_cluster_ref_records_the_release_that_the_waiting_update_brings() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let on_a = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-a"),
        None,
    )
    .unwrap();
    let api = serve_pending(
        serve_cluster(MemoryApi::new(), dir.path(), "cluster-b"),
        dir.path(),
        "cluster-a",
        |_| {},
    )
    .with(
        profile_route(UID_A),
        read_json(&dir.path().join("a/profile.json")),
    );
    let now = snapshot_cluster(&api, None).unwrap();
    let pending = snapshot_pending(&api, None).unwrap();
    // the packs of the release that the update brings; the cluster holds no variable values for
    // a profile that it does not run yet, so the record has none
    let d = ops::compare(&on_a, &pending, "t1");
    assert_eq!(
        d.summary.packs_changed, 0,
        "the release that the update brings"
    );
    assert!(
        pending
            .source
            .cluster
            .as_ref()
            .unwrap()
            .variables
            .is_empty()
    );
    assert_ne!(pending.root, now.root, "and not the release that runs now");
    let c = pending.source.cluster.as_ref().unwrap();
    assert!(c.pending);
    assert_eq!(
        (c.updates_pending, c.updates_pending_error()),
        (None, None),
        "whether an update waits is a statement about the copy that runs"
    );
    assert!(
        pending
            .source
            .title()
            .ends_with("pending for cluster lab-cluster"),
        "{}",
        pending.source.title()
    );
    assert_eq!(
        serde_json::to_value(&pending).unwrap()["source"]["cluster"]["pending"],
        true
    );
    assert!(
        serde_json::to_value(&now).unwrap()["source"]["cluster"]
            .get("pending")
            .is_none(),
        "a record of what runs now writes no pending field"
    );

    // compare names what the update changes
    let d = ops::compare(&now, &pending, "t1");
    assert!(!d.same);

    // verify reads the update again, and not what runs now
    let v = ops::verify(
        &pending,
        &Palette::new(api.clone()),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(v.unchanged, "{:?}", v.status);
    // once the cluster takes the update, nothing waits
    let taken = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a").with(
        profile_updates_route(),
        serde_json::json!({ "profiles": [] }),
    );
    let v = ops::verify(&pending, &Palette::new(taken), "t1", &Progress::silent()).unwrap();
    assert!(!v.unchanged, "no update waits, so no profile is pending");
}

// ---- edge hosts -------------------------------------------------------------------------------

/// An edge host `e1`, `edge-1`, bound to the profile of cluster-a, with the manifests, the
/// variables, and the document of that profile, and the search that lists it.
fn serve_edge_host(api: MemoryApi, raw_root: &Path) -> MemoryApi {
    let a = raw_root.join("cluster-a");
    let templates = read_json(&a.join("cluster.json"))["spec"]["clusterProfileTemplates"].clone();
    let manifests = read_json(&a.join("manifests.json"))["profiles"].clone();
    let host = serde_json::json!({
        "metadata": {"uid": "e1", "name": "edge-1", "annotations": {"projectUid": "p1"}},
        "spec": {"clusterProfileTemplates": templates},
        "status": {"state": "ready"},
    });
    let summary: Vec<Value> = templates
        .as_array()
        .unwrap()
        .iter()
        .map(|t| serde_json::json!({"uid": t["uid"], "name": t["name"], "version": t["profileVersion"]}))
        .collect();
    let mut api = api.with(edge_host_route("e1"), host).with(
        post(EDGE_HOST_SEARCH),
        serde_json::json!({"items": [{
            "metadata": {"uid": "e1", "name": "edge-1", "annotations": {"projectUid": "p1"}},
            "spec": {"clusterProfileTemplates": summary},
            "status": {"state": "ready"},
        }], "listmeta": {"continue": ""}}),
    );
    for (t, m) in templates
        .as_array()
        .unwrap()
        .iter()
        .zip(manifests.as_array().unwrap())
    {
        api = serve_profile(
            api,
            t["uid"].as_str().unwrap(),
            read_json(&raw_root.join("a").join("profile.json")),
            m.clone(),
            read_json(&raw_root.join("a").join("variables.json")),
        );
    }
    api
}

#[test]
fn an_edge_host_record_holds_the_profiles_bound_to_it() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded_a = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let api = serve_edge_host(MemoryApi::new(), dir.path());
    let host = snapshot_of(&api, "edgehost:e1").unwrap();
    assert_eq!(
        host.root, recorded_a.root,
        "the packs and the variable definitions of the bound profile"
    );
    let c = host.source.cluster.as_ref().unwrap();
    assert_eq!(c.kind, strata::snapshot::SubjectKind::EdgeHost);
    assert_eq!((c.uid.as_str(), c.name.as_str()), ("e1", "edge-1"));
    assert_eq!(c.updates_pending, None, "an edge host has no notifications");
    assert!(c.variables.is_empty(), "an edge host gives no values");
    assert!(
        host.source.title().ends_with("on edge host edge-1"),
        "{}",
        host.source.title()
    );
    assert_eq!(
        serde_json::to_value(&host).unwrap()["source"]["cluster"]["kind"],
        "edgeHost"
    );

    // by name, through the search
    let by_name = snapshot_of(&api, "edgehost:edge-1").unwrap();
    assert_eq!(by_name.root, host.root);
    assert_eq!(by_name.source.cluster.unwrap().uid, "e1");
    let err = snapshot_of(&api, "edgehost:edge-9").unwrap_err();
    assert_eq!(err.kind(), "edge-host-not-found");

    // verify reads the edge host again, and not a cluster of that uid
    let v = ops::verify(&host, &Palette::new(api.clone()), "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged, "{:?}", v.status);
    // the host is bound to another profile: its record drifts
    let b =
        read_json(&dir.path().join("cluster-b").join("cluster.json"))["spec"]["clusterProfileTemplates"]
            .clone();
    let b_manifests =
        read_json(&dir.path().join("cluster-b").join("manifests.json"))["profiles"][0].clone();
    let b_uid = b[0]["uid"].as_str().unwrap().to_string();
    let rebound = serve_edge_host(MemoryApi::new(), dir.path())
        .with(
            edge_host_route("e1"),
            serde_json::json!({
                "metadata": {"uid": "e1", "name": "edge-1"},
                "spec": {"clusterProfileTemplates": b},
            }),
        )
        .with(manifests_route(&b_uid), b_manifests)
        .with(
            profile_route(&b_uid),
            read_json(&dir.path().join("b").join("profile.json")),
        );
    let v = ops::verify(&host, &Palette::new(rebound), "t1", &Progress::silent()).unwrap();
    assert!(!v.unchanged);
}

/// The profile of cluster-a with the content of manifest `extra-rules` replaced by `content`, as
/// `GET /v1/clusterprofiles/{uid}/packs/manifests` gives it.
fn manifests_with_rules(raw_root: &Path, content: &str) -> Value {
    let mut m =
        read_json(&raw_root.join("cluster-a").join("manifests.json"))["profiles"][0].clone();
    for pack in m["spec"]["packs"].as_array_mut().unwrap() {
        for man in pack["spec"]["manifests"]
            .as_array_mut()
            .into_iter()
            .flatten()
        {
            if man["name"] == "extra-rules" {
                man["content"] = Value::String(content.to_string());
            }
        }
    }
    m
}

/// A manifest as the route of a holder gives it: `spec.published.content`.
fn own_copy(uid: &str, content: &str) -> Value {
    serde_json::json!({"metadata": {"uid": uid, "name": "extra-rules"},
                       "spec": {"published": {"content": content, "digest": ""}}})
}

#[test]
fn an_edge_host_record_holds_its_own_copy_of_each_manifest() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded_a = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let a = dir.path().join("cluster-a");
    let rules = read_json(&a.join("manifests.json"))["profiles"][0]["spec"]["packs"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| {
            p["spec"]["manifests"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .find(|m| m["name"] == "extra-rules")
        .unwrap()["content"]
        .as_str()
        .unwrap()
        .to_string();
    // Palette names the copy of the host by a uid of its own, with no content
    let mut templates =
        read_json(&a.join("cluster.json"))["spec"]["clusterProfileTemplates"].clone();
    let puid = templates[0]["uid"].as_str().unwrap().to_string();
    for pack in templates[0]["packs"].as_array_mut().unwrap() {
        if pack["name"] == "prometheus-operator" {
            pack["manifests"] = serde_json::json!([{"uid": "host-rules-1", "name": "extra-rules"}]);
        }
    }
    let host = |copy: &str, profile: &str| {
        serve_edge_host(MemoryApi::new(), dir.path())
            .with(
                edge_host_route("e1"),
                serde_json::json!({
                    "metadata": {"uid": "e1", "name": "edge-1"},
                    "spec": {"clusterProfileTemplates": templates.clone()},
                }),
            )
            .with(
                "/v1/edgehosts/e1/pack/manifests/host-rules-1",
                own_copy("host-rules-1", copy),
            )
            .with(
                manifests_route(&puid),
                manifests_with_rules(dir.path(), profile),
            )
    };
    let record = snapshot_of(&host(&rules, &rules), "edgehost:e1").unwrap();
    assert_eq!(
        record.root, recorded_a.root,
        "the copy holds what the profile held"
    );

    // an edit of the manifest of the profile does not reach the copy of the host
    let edited = rules.replace("for: 5m", "for: 10m");
    assert_ne!(edited, rules);
    let v = ops::verify(
        &record,
        &Palette::new(host(&rules, &edited)),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(v.unchanged, "the host keeps its copy: {:?}", v.status);

    // a change of the copy itself is a drift
    let v = ops::verify(
        &record,
        &Palette::new(host(&edited, &rules)),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(!v.unchanged);
}

#[test]
fn a_cluster_group_record_holds_its_own_copy_of_each_manifest() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded_a = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let a = dir.path().join("cluster-a");
    let rules = read_json(&a.join("manifests.json"))["profiles"][0]["spec"]["packs"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| {
            p["spec"]["manifests"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .find(|m| m["name"] == "extra-rules")
        .unwrap()["content"]
        .as_str()
        .unwrap()
        .to_string();
    let mut templates =
        read_json(&a.join("cluster.json"))["spec"]["clusterProfileTemplates"].clone();
    let puid = templates[0]["uid"].as_str().unwrap().to_string();
    for pack in templates[0]["packs"].as_array_mut().unwrap() {
        if pack["name"] == "prometheus-operator" {
            pack["manifests"] =
                serde_json::json!([{"uid": "group-rules-1", "name": "extra-rules"}]);
        }
    }
    // the profile of cluster-a, served as for an edge host, and the group that pins it
    let group = |copy: &str, profile: &str| {
        serve_edge_host(MemoryApi::new(), dir.path())
            .with(
                group_route("g1"),
                serde_json::json!({
                    "metadata": {"uid": "g1", "name": "group-1"},
                    "spec": {"type": "hostCluster", "clusterProfileTemplates": templates.clone()},
                }),
            )
            .with(
                post(GROUP_MEMBERS),
                serde_json::json!({"items": [], "listmeta": {"continue": ""}}),
            )
            .with(
                manifest_route(&puid, "prometheus-operator", "group-rules-1"),
                own_copy("group-rules-1", copy),
            )
            .with(
                manifests_route(&puid),
                manifests_with_rules(dir.path(), profile),
            )
    };
    let snap = |api: MemoryApi| snapshot_of(&api, "clustergroup:g1");
    let record = snap(group(&rules, &rules)).unwrap();
    assert_eq!(
        record.root, recorded_a.root,
        "the copy holds what the profile held"
    );

    let edited = rules.replace("for: 5m", "for: 10m");
    let kept = snap(group(&rules, &edited)).unwrap();
    assert_eq!(
        kept.root, record.root,
        "an edit of the profile does not reach the copy"
    );
    let changed = snap(group(&edited, &rules)).unwrap();
    assert_ne!(
        changed.root, record.root,
        "a change of the copy is a change"
    );
}

#[test]
fn consumers_counts_an_edge_host_bound_to_the_profile() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_edge_host(
        serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A),
        dir.path(),
    );
    let r = ops::consumers(
        &Ref::Uid(UID_A.into()),
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(
        (r.count, r.exit_code()),
        (1, 1),
        "an edge host uses the profile, so a delete step must not proceed"
    );
    let host = &r.consumers[0];
    assert_eq!(
        (
            host.kind,
            host.uid.as_str(),
            host.name.as_str(),
            host.project_uid.as_str(),
            host.state.as_str(),
            host.version.as_str()
        ),
        (
            ops::ConsumerKind::EdgeHost,
            "e1",
            "edge-1",
            "p1",
            "ready",
            "1.2.0"
        )
    );
    assert_eq!(r.edge_hosts_read.read, 1);
    // --list-only reads the edge hosts too: the consumer list of Palette names no edge host
    let listed = ops::consumers_with(
        &Ref::Uid(UID_A.into()),
        &ops::ConsumersOptions { list_only: true },
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(listed.count, 1);

    let text = common::terminal().render_consumers(&r).unwrap();
    assert!(
        text.contains("edge-1  e1  project p1  ready  edge host"),
        "{text}"
    );
    assert!(text.ends_with("  edge hosts: 1 read\n"), "{text}");
    let md = common::markdown().render_consumers(&r).unwrap();
    assert!(
        md.contains("(edge host)") && md.contains("Edge hosts: 1 read."),
        "{md}"
    );
    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_consumers(&r)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["consumers"][0]["kind"], "edgehost");
    assert_eq!(json["edgeHostsRead"]["read"], 1);
}

/// Palette refuses the delete of a version that a cluster template links, and the profile document
/// names each such template. A version that a template links and no cluster runs has a consumer.
#[test]
fn consumers_counts_a_cluster_template_that_links_the_version() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let mut profile = read_json(&dir.path().join("a").join("profile.json"));
    profile["status"]["inUseClusterUids"] = serde_json::json!([]);
    profile["status"]["inUseClusters"] = serde_json::json!([]);
    profile["status"]["inUseClusterTemplates"] = serde_json::json!([
        {"uid": "tmpl-1", "name": "barney-template", "projectUid": "p1", "kind": "clustertemplate"}
    ]);
    let api =
        serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A).with(profile_route(UID_A), profile);
    for options in [
        ops::ConsumersOptions::default(),
        ops::ConsumersOptions { list_only: true },
    ] {
        let r = ops::consumers_with(
            &Ref::Uid(UID_A.into()),
            &options,
            |_| Ok(Palette::new(api.clone())),
            "t1",
            &Progress::silent(),
        )
        .unwrap();
        assert_eq!(
            (r.count, r.exit_code()),
            (1, 1),
            "a template links the version, so Palette refuses the delete: {options:?}"
        );
        let t = &r.consumers[0];
        assert_eq!(
            (
                t.kind,
                t.uid.as_str(),
                t.name.as_str(),
                t.project_uid.as_str(),
                t.state.as_str(),
                t.version.as_str()
            ),
            (
                ops::ConsumerKind::Template,
                "tmpl-1",
                "barney-template",
                "p1",
                "",
                ""
            )
        );
        assert!(t.error.is_none() && !t.unlisted);
        assert_eq!(r.overwritten, 0, "a template keeps no copy of the version");
    }

    let r = ops::consumers(
        &Ref::Uid(UID_A.into()),
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    let text = common::terminal().render_consumers(&r).unwrap();
    assert!(
        text.contains("barney-template  tmpl-1  project p1  cluster template\n"),
        "{text}"
    );
    let md = common::markdown().render_consumers(&r).unwrap();
    assert!(
        md.contains("| `barney-template` | `tmpl-1` | `p1` | - | (cluster template) |"),
        "{md}"
    );
    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_consumers(&r)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["count"], 1);
    assert_eq!(json["consumers"][0]["kind"], "template");
    assert_eq!(json["consumers"][0]["projectUid"], "p1");
}

// ---- cluster groups ---------------------------------------------------------------------------

/// The template of the add-on profile in the document of cluster-merged.
fn merged_addon_template(raw_root: &Path) -> Value {
    let (_, addon) = merged_uids(raw_root);
    let cluster = read_json(&raw_root.join("cluster-merged").join("cluster.json"));
    strata::snapshot::AppliedProfile::template_of(&cluster, &addon)
        .unwrap()
        .clone()
}

/// The document of the cluster group `g1`, `group-1`, that pins the add-on profile of
/// cluster-merged.
fn group_doc(raw_root: &Path) -> Value {
    let template = merged_addon_template(raw_root);
    serde_json::json!({
        "metadata": {"uid": "g1", "name": "group-1"},
        "spec": {"type": "hostCluster", "clusterProfileTemplates": [template]},
    })
}

/// A cluster group `g1`, `group-1`, that pins the add-on profile of cluster-merged, with that
/// cluster as its one virtual cluster, and the profile documents it needs.
fn serve_cluster_group(api: MemoryApi, raw_root: &Path) -> MemoryApi {
    let m = raw_root.join("cluster-merged");
    let (_, addon) = merged_uids(raw_root);
    let manifests = read_json(&m.join("manifests.json"))["profiles"].clone();
    let addon_manifests = manifests
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["metadata"]["uid"] == addon.as_str())
        .unwrap()
        .clone();
    serve_cluster(api, raw_root, "cluster-merged")
        .with(group_route("g1"), group_doc(raw_root))
        .with(
            GROUP_LISTING,
            serde_json::json!({"items": [{"uid": "g1", "name": "group-1", "scope": "project"}]}),
        )
        .with(
            post(GROUP_MEMBERS),
            serde_json::json!({"items": [{"metadata": {"uid": CLUSTER_UID, "name": "lab-cluster",
                "annotations": {"projectUid": "p1"}}}], "listmeta": {"continue": ""}}),
        )
        .with(manifests_route(&addon), addon_manifests)
        .with(
            variables_route(&addon),
            read_json(&raw_root.join("addons").join("variables.json")),
        )
}

/// The search of the members of a group can name the project of a member only in its summary.
/// The record reads it there, as `list clusters` and `consumers` do for the same items.
#[test]
fn a_member_of_a_cluster_group_takes_its_project_from_the_search_when_it_has_no_annotation() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_cluster_group(MemoryApi::new(), dir.path()).with(
        post(GROUP_MEMBERS),
        serde_json::json!({"items": [{"metadata": {"uid": CLUSTER_UID, "name": "lab-cluster"},
            "specSummary": {"projectMeta": {"uid": "p2", "name": "lab"}}}],
            "listmeta": {"continue": ""}}),
    );
    let group = snapshot_of(&api, "clustergroup:g1").unwrap();
    assert_eq!(
        group.source.cluster.as_ref().unwrap().members[0].project_uid,
        "p2"
    );
}

#[test]
fn a_cluster_group_record_holds_the_pin_and_compares_against_a_member() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let (infra, addon) = merged_uids(dir.path());
    let api = serve_cluster_group(MemoryApi::new(), dir.path());
    let connect = || Ok(Palette::new(api.clone()));
    let group = snapshot_of(&api, "clustergroup:g1").unwrap();
    let c = group.source.cluster.as_ref().unwrap();
    assert_eq!(c.kind, strata::snapshot::SubjectKind::ClusterGroup);
    assert_eq!(
        c.profiles
            .iter()
            .map(|p| p.uid.as_str())
            .collect::<Vec<_>>(),
        [addon.as_str()],
        "the pin"
    );
    assert_eq!(
        c.members,
        [strata::snapshot::GroupMember {
            uid: CLUSTER_UID.into(),
            name: "lab-cluster".into(),
            project_uid: "p1".into(),
        }]
    );
    assert!(
        group
            .source
            .title()
            .ends_with("pinned by cluster group group-1"),
        "{}",
        group.source.title()
    );
    let json = serde_json::to_value(&group).unwrap();
    assert_eq!(json["source"]["cluster"]["kind"], "clusterGroup");
    assert_eq!(
        json["source"]["cluster"]["members"][0]["name"],
        "lab-cluster"
    );

    // by name
    let by_name = snapshot_of(&api, "clustergroup:group-1").unwrap();
    assert_eq!(by_name.root, group.root);
    let err = snapshot_of(&api, "clustergroup:group-9").unwrap_err();
    assert_eq!(err.kind(), "cluster-group-not-found");

    // compare with the member: the cluster side holds only the pinned profile, in either order
    let member = cluster_ref(CLUSTER_UID, None, false);
    let group_ref = Ref::parse("clustergroup:g1");
    for (a, b) in [(&group_ref, &member), (&member, &group_ref)] {
        let (sa, sb) = ops::snapshot_pair(a, b, |_| connect(), "t1", &Progress::silent()).unwrap();
        assert_eq!(sa.root, sb.root, "the member runs the pin");
        let side = if matches!(a, Ref::Cluster { .. }) {
            &sa
        } else {
            &sb
        };
        let covered = &side.source.cluster.as_ref().unwrap().profiles;
        assert_eq!(
            covered.len(),
            1,
            "the infra profile {infra} is not in the question"
        );
        let d = ops::compare(&sa, &sb, "t1");
        assert!(d.same);
        assert!(
            d.cluster_variables.as_ref().is_none_or(|v| !v.changed),
            "a group gives no values, so it states no value that differs: {:?}",
            d.cluster_variables
        );
    }
    // a cluster ref that selects a profile is not narrowed
    let (_, whole) = ops::snapshot_pair(
        &group_ref,
        &Ref::parse(&format!("cluster:{CLUSTER_UID}/{infra}")),
        |_| connect(),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(whole.source.cluster.unwrap().profiles[0].uid, infra);

    // verify reads the cluster group again
    let v = ops::verify(
        &group,
        &Palette::new(api.clone()),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(v.unchanged, "{:?}", v.status);
}

/// The cluster side and the diff of a comparison of a profile version with the cluster, in each
/// order of the two refs.
fn beside_the_cluster(api: &MemoryApi, version: &str) -> Vec<(Snapshot, strata::diff::Diff)> {
    let cluster = cluster_ref(CLUSTER_UID, None, false);
    let version = Ref::parse(version);
    let connect = || Ok(Palette::new(api.clone()));
    [(&version, &cluster), (&cluster, &version)]
        .into_iter()
        .map(|(a, b)| {
            let (sa, sb) =
                ops::snapshot_pair(a, b, |_| connect(), "t1", &Progress::silent()).unwrap();
            let d = ops::compare(&sa, &sb, "t1");
            let side = if sa.source.cluster.is_some() { sa } else { sb };
            (side, d)
        })
        .collect()
}

/// A cluster that runs two profiles answers for the profile of the other side (#615). A team asks
/// whether the cluster runs a version, and not what else it runs.
#[test]
fn a_cluster_beside_a_profile_version_holds_only_the_profile_that_the_version_replaces() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let (infra, addon) = merged_uids(dir.path());
    // the profile version of the fixture `fixture`, under the uid `uid` and the name `name`
    let serve = |api: MemoryApi, fixture: &str, uid: &str, name: &str| {
        let d = dir.path().join(fixture);
        let mut doc = read_json(&d.join("profile.json"));
        doc["metadata"]["uid"] = Value::from(uid);
        doc["metadata"]["name"] = Value::from(name);
        let manifests = read_json(&d.join("manifests.json"));
        serve_profile(
            api,
            uid,
            doc,
            manifests,
            read_json(&d.join("variables.json")),
        )
    };
    let (other_stack, other_addons) = ("64f0000000000000000000e5", "64f0000000000000000000e6");
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-merged");
    let api = serve(api, "b", UID_B, "lab-profile");
    let api = serve(api, "addons", UID_ADDONS, "lab-addons");
    let api = serve(api, "a", UID_A, "lab-profile");
    let api = serve(api, "a", other_stack, "other-stack");
    let api = serve(api, "addons", other_addons, "other-addons");
    let covered = |side: &Snapshot| {
        let c = side.source.cluster.as_ref().unwrap();
        let uids: Vec<String> = c.profiles.iter().map(|p| p.uid.clone()).collect();
        (uids, side.source.reference.clone())
    };
    let addon_pack = |d: &strata::diff::Diff| d.packs.iter().any(|p| p.name == "nginx-ingress");

    // the cluster runs each of its two profiles: the root is that of the version
    for (uid, name, other) in [
        (infra.as_str(), "lab-profile", &addon),
        (addon.as_str(), "lab-addons", &infra),
    ] {
        for (side, d) in beside_the_cluster(&api, uid) {
            let reference = format!("cluster:{CLUSTER_UID}/{name}");
            assert_eq!(covered(&side), (vec![uid.to_string()], reference));
            assert!(
                d.same,
                "the cluster runs {name}, and {other} is not in the question"
            );
        }
    }
    // another version of a profile that the cluster runs: only the packs of that profile differ.
    // The add-on of the cluster gives no ADDED or REMOVED line
    for (side, d) in beside_the_cluster(&api, UID_A) {
        assert_eq!(covered(&side).0, [infra.as_str()]);
        assert!(!d.same, "the cluster runs lab-profile@1.3.0, not 1.2.0");
        assert!(!addon_pack(&d), "{:?}", d.packs);
    }
    // a version of another name that builds a cluster replaces the profile that builds it, as
    // the body of --repave and --validate says
    for (side, d) in beside_the_cluster(&api, other_stack) {
        assert_eq!(covered(&side).0, [infra.as_str()]);
        assert!(!addon_pack(&d), "{:?}", d.packs);
    }
    // an add-on that the cluster does not run replaces nothing: the move adds it, so the cluster
    // stays whole
    for (side, d) in beside_the_cluster(&api, other_addons) {
        assert_eq!(
            covered(&side),
            (
                vec![infra.clone(), addon.clone()],
                format!("cluster:{CLUSTER_UID}")
            )
        );
        assert!(!d.same);
    }
    // two clusters stay whole
    let connect = || Ok(Palette::new(api.clone()));
    let whole = cluster_ref(CLUSTER_UID, None, false);
    let (sa, sb) =
        ops::snapshot_pair(&whole, &whole, |_| connect(), "t1", &Progress::silent()).unwrap();
    for side in [&sa, &sb] {
        assert_eq!(covered(side).0, [infra.as_str(), addon.as_str()]);
    }
    // a cluster ref that selects a profile keeps its selection
    let (_, selected) = ops::snapshot_pair(
        &Ref::parse(UID_ADDONS),
        &cluster_ref(CLUSTER_UID, Some("lab-profile"), false),
        |_| connect(),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(covered(&selected).0, [infra.as_str()]);
    // the update that waits for the cluster keeps the same selection
    let waits = serve_pending(api.clone(), dir.path(), "cluster-merged", |_| {});
    let (_, pending) = ops::snapshot_pair(
        &Ref::parse(UID_ADDONS),
        &cluster_ref(CLUSTER_UID, None, true),
        |_| Ok(Palette::new(waits.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    let reference = format!("cluster:{CLUSTER_UID}/lab-addons@pending");
    assert_eq!(covered(&pending), (vec![addon.clone()], reference));
    assert!(pending.source.cluster.unwrap().pending);
}

#[test]
fn a_cluster_group_record_with_macro_digests_reads_the_values_of_the_group() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let (_, addon) = merged_uids(dir.path());
    // the pin reads the macro region in the values of its first pack
    let base = serve_cluster_group(MemoryApi::new(), dir.path());
    let mut doc = base.get_json(&group_route("g1")).unwrap();
    let pack = &mut doc["spec"]["clusterProfileTemplates"][0]["packs"][0];
    let values = pack["values"].as_str().unwrap_or_default().to_string();
    pack["values"] = Value::from(format!(
        "{values}\nregion: \"{{{{.spectro.macro.region}}}}\"\n"
    ));
    // only the route of the group answers: a request to the route of a cluster is a 404
    let tenant = |region: &str| {
        base.clone().with(group_route("g1"), doc.clone()).with(
            group_resolved_values_route("g1"),
            serde_json::json!({"profiles": [{"uid": addon, "resolved": {
                "{{.spectro.macro.region}}": region,
            }}]}),
        )
    };

    let group = ops::cluster_group_snapshot(
        &Palette::new(tenant("us-east-2")),
        "g1",
        true,
        "clustergroup:g1",
        "t0",
        &Progress::silent(),
    )
    .unwrap();
    let c = group.source.cluster.as_ref().unwrap();
    let digests = c.macro_values.clone().unwrap();
    assert!(digests["region"].starts_with("sha256:"), "{digests:?}");
    assert!(
        group.source.macro_values.is_none(),
        "a group record holds them in cluster"
    );
    let json = serde_json::to_value(&group).unwrap();
    assert!(
        json["source"]["cluster"]["macroValues"].is_object(),
        "{json}"
    );

    // without --macros, no request for the values
    let plain = ops::cluster_group_snapshot(
        &Palette::new(base.clone().with(group_route("g1"), doc.clone())),
        "g1",
        false,
        "clustergroup:g1",
        "t0",
        &Progress::silent(),
    )
    .unwrap();
    assert!(plain.source.cluster.unwrap().macro_values.is_none());

    // verify reads the values of the group again, and names the macro that has another value
    let v = ops::verify(
        &group,
        &Palette::new(tenant("us-east-2")),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(v.status, VerifyStatus::Unchanged);
    let v = ops::verify(
        &group,
        &Palette::new(tenant("eu-west-1")),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(v.status, VerifyStatus::Drifted);
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.macros_changed, ["region"]);
    let verdict = common::terminal().verdict(&v.diff).unwrap();
    assert!(
        verdict.contains("pinned by cluster group group-1 has the content recorded t0, but macro region has another value now"),
        "the verdict names the group, and not a cluster: {verdict}"
    );
}

/// The config that the cluster group `g1` gives each virtual cluster, as Palette gives it in
/// `spec.clustersConfig`. `cpu` is the CPU limit in millicores, and `values` is YAML.
fn clusters_config(cpu: u64, values: &str) -> Value {
    let endpoint = serde_json::json!({"ingressConfig": {"host": "*", "port": 6443}});
    serde_json::json!({
        "endpointType": "LoadBalancer",
        "kubernetesDistroType": "k3s",
        "limitConfig": {"cpu": cpu / 1000, "cpuMilliCore": cpu, "memory": 12288,
            "memoryMiB": 12288, "overSubscription": 120, "storageGiB": 10},
        "values": values,
        "hostClustersConfig": [
            {"clusterUid": "host-2", "endpointConfig": endpoint},
            {"clusterUid": "host-1", "endpointConfig": endpoint},
        ],
    })
}

const GROUP_VALUES: &str =
    "# sync to the host\nsync:\n  toHost:\n    services:\n      enabled: true\n";

#[test]
fn a_cluster_group_record_holds_the_config_of_each_virtual_cluster_in_a_policy_tier_outside_the_root()
 {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    // the member has a policy tier of its own
    let member_path = dir.path().join("cluster-merged").join("cluster.json");
    let mut member_doc = read_json(&member_path);
    member_doc["spec"]["clusterConfig"] = serde_json::json!({"timezone": "Etc/UTC"});
    fs::write(&member_path, serde_json::to_string(&member_doc).unwrap()).unwrap();
    let serve = |config: Option<Value>| {
        let mut doc = group_doc(dir.path());
        if let Some(c) = config {
            doc["spec"]["clustersConfig"] = c;
        }
        serve_cluster_group(MemoryApi::new(), dir.path()).with(group_route("g1"), doc)
    };
    let snap = |api: &MemoryApi| common::snapshot_of(api, "clustergroup:g1").unwrap();
    let api = serve(Some(clusters_config(6000, GROUP_VALUES)));
    let group = snap(&api);
    let cluster = group.source.cluster.as_ref().unwrap();
    assert_eq!(cluster.tiers.keys().collect::<Vec<_>>(), [&Tier::Policy]);
    let f = &cluster.tier(Tier::Policy).unwrap().fields;
    assert_eq!(f["endpointType"], "LoadBalancer");
    assert_eq!(f["kubernetesDistroType"], "k3s");
    assert_eq!(f["limitConfig"]["cpuMilliCore"], 6000);
    assert_eq!(f["limitConfig"]["memoryMiB"], 12288);
    assert_eq!(f["limitConfig"]["storageGiB"], 10);
    assert_eq!(f["limitConfig"]["overSubscription"], 120);
    assert_eq!(
        f["values"],
        serde_json::json!({"sync": {"toHost": {"services": {"enabled": true}}}}),
        "the values in canonical form"
    );
    assert_eq!(f["valuesHash"].as_str().map(str::len), Some(64));
    assert_eq!(f["hostClusters"][0]["uid"], "host-1", "sorted by uid");
    assert_eq!(f["hostClusters"][1]["uid"], "host-2");
    assert_eq!(
        f["hostClusters"][0]["endpointConfig"]["ingressConfig"]["port"],
        6443
    );
    let json = serde_json::to_value(&group).unwrap();
    assert_eq!(
        json["source"]["cluster"]["tiers"]["policy"]["fields"]["limitConfig"]["cpuMilliCore"],
        6000
    );

    // outside the root: a group with no config has the same root, and no tier
    let bare = snap(&serve(None));
    assert_eq!(bare.root, group.root);
    assert!(bare.source.cluster.as_ref().unwrap().tiers.is_empty());

    // a comment in the values is not a change of the config
    let commented = snap(&serve(Some(clusters_config(
        6000,
        &format!("# another comment\n{GROUP_VALUES}"),
    ))));
    assert_eq!(
        commented
            .source
            .cluster
            .as_ref()
            .unwrap()
            .tier(Tier::Policy)
            .unwrap()
            .hash,
        cluster.tier(Tier::Policy).unwrap().hash
    );

    // values that are not YAML are kept by the hash of their text, with the error
    let broken = snap(&serve(Some(clusters_config(6000, "sync: [\n"))));
    let f = &broken
        .source
        .cluster
        .as_ref()
        .unwrap()
        .tier(Tier::Policy)
        .unwrap()
        .fields;
    assert_eq!(f["values"], Value::Null);
    assert_eq!(f["valuesHash"].as_str().map(str::len), Some(64));
    assert!(f["valuesParseError"].is_string(), "{f}");

    // a group and its member have no tier to compare: their policy tiers say different things
    let member = cluster_ref(CLUSTER_UID, None, false);
    let connect = || Ok(Palette::new(api.clone()));
    let (sa, sb) = ops::snapshot_pair(
        &Ref::parse("clustergroup:g1"),
        &member,
        |_| connect(),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(
        sb.source
            .cluster
            .as_ref()
            .unwrap()
            .tier(Tier::Policy)
            .is_some()
    );
    let d = ops::compare(&sa, &sb, "t1");
    assert!(
        d.same && d.cluster_tiers.is_empty(),
        "{:?}",
        d.cluster_tiers
    );

    // the group gives each virtual cluster less CPU now
    let less = serve(Some(clusters_config(4000, GROUP_VALUES)));
    let named = |tiers: &[strata::diff::TierDiff]| {
        let policy = tiers.iter().find(|t| t.tier == Tier::Policy).unwrap();
        assert!(!policy.same);
        policy
            .diff
            .iter()
            .map(|l| l.path.clone())
            .collect::<Vec<_>>()
    };
    let d = ops::compare(&group, &snap(&less), "t1");
    assert!(d.same, "the root covers the pin alone");
    assert_eq!(
        named(&d.cluster_tiers),
        ["limitConfig.cpu", "limitConfig.cpuMilliCore"]
    );
    let v = ops::verify(
        &group,
        &Palette::new(less.clone()),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(v.status, VerifyStatus::Unchanged);
    assert_eq!(
        named(&v.diff.cluster_tiers),
        ["limitConfig.cpu", "limitConfig.cpuMilliCore"]
    );

    // a record of the group from before the tier loads, and verifies with no tier to compare
    let mut doc = serde_json::to_value(&group).unwrap();
    doc["source"]["cluster"]
        .as_object_mut()
        .unwrap()
        .remove("tiers");
    let old: Snapshot = serde_json::from_value(doc).unwrap();
    let v = ops::verify(&old, &Palette::new(less), "t1", &Progress::silent()).unwrap();
    assert_eq!(v.status, VerifyStatus::Unchanged);
    assert!(v.diff.cluster_tiers.is_empty());
}

#[test]
fn a_raw_directory_of_a_cluster_group_masks_each_password_and_gives_the_record_of_the_fetch() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let values = "# the admin\nsync:\n  adminPassword: barney-clear-secret\n  replicas: 1\n\
                  users:\n- name: fred\n  password: pebbles-clear-secret\n";
    let mut doc = group_doc(dir.path());
    doc["spec"]["clustersConfig"] = clusters_config(6000, values);
    let api = serve_cluster_group(MemoryApi::new(), dir.path()).with(group_route("g1"), doc);
    let fetched = Palette::new(api)
        .fetch_subject(SubjectKind::ClusterGroup, "g1", &Progress::silent())
        .unwrap();
    let saved = dir.path().join("raw");
    fetched.save_dir(&saved, "t0").unwrap();
    let text = fs::read_to_string(saved.join("cluster.json")).unwrap();
    assert!(!text.contains("clear-secret"), "{text}");
    assert_eq!(text.matches("********").count(), 2, "{text}");
    // each other value stays
    let saved_doc: Value = serde_json::from_str(&text).unwrap();
    let saved_values = saved_doc["spec"]["clustersConfig"]["values"]
        .as_str()
        .unwrap();
    assert!(saved_values.contains("replicas: 1"), "{saved_values}");
    assert!(saved_values.contains("name: fred"), "{saved_values}");

    // the record of the directory is the record of the fetch, and holds the mask too
    let direct = Snapshot::from_cluster(&fetched, None, "raw", "t0").unwrap();
    let from_raw = ops::snapshot_from_dir(&saved, "raw", "t1").unwrap();
    assert_eq!(
        serde_json::to_value(&from_raw).unwrap(),
        serde_json::to_value(&direct).unwrap()
    );
    assert_eq!(
        from_raw
            .source
            .cluster
            .unwrap()
            .tier(Tier::Policy)
            .unwrap()
            .fields["values"],
        serde_json::json!({"sync": {"adminPassword": "********", "replicas": 1},
            "users": [{"name": "fred", "password": "********"}]})
    );
}

#[test]
fn a_cluster_group_with_values_that_have_no_canonical_form_records_the_mask_and_not_the_password() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    // an unquoted template is a map in a key, so the values have no canonical form
    let take = |password: &str, name: &str| {
        let values = format!("adminPassword: {password}\nhost: {{{{ .spectro.var.host }}}}\n");
        let mut doc = group_doc(dir.path());
        doc["spec"]["clustersConfig"] = clusters_config(6000, &values);
        let api = serve_cluster_group(MemoryApi::new(), dir.path()).with(group_route("g1"), doc);
        let fetched = Palette::new(api)
            .fetch_subject(SubjectKind::ClusterGroup, "g1", &Progress::silent())
            .unwrap();
        let direct = Snapshot::from_cluster(&fetched, None, "raw", "t0").unwrap();
        let saved = dir.path().join(name);
        fetched.save_dir(&saved, "t0").unwrap();
        let from_raw = ops::snapshot_from_dir(&saved, "raw", "t0").unwrap();
        (direct, from_raw)
    };
    let policy = |snap: &Snapshot| {
        let tier = snap.source.cluster.as_ref().unwrap().tier(Tier::Policy);
        let tier = tier.unwrap();
        (tier.hash.clone(), tier.fields["values"].clone())
    };
    let (direct, from_raw) = take("barney-clear-secret", "raw-one");
    let (hash, values) = policy(&direct);
    assert_eq!(values, Value::Null, "the values have no canonical form");
    let json = serde_json::to_string(&direct).unwrap();
    assert!(!json.contains("barney-clear-secret"), "{json}");

    // the record of the saved directory is the record of the fetch, and covers the mask
    assert_eq!(policy(&from_raw).0, hash);
    assert_eq!(
        serde_json::to_value(&from_raw).unwrap(),
        serde_json::to_value(&direct).unwrap()
    );

    // a new password changes no hash, so it changes no tier
    let (other, other_raw) = take("pebbles-clear-secret", "raw-two");
    assert_eq!(policy(&other).0, hash);
    assert_eq!(policy(&other_raw).0, hash);
    let d = ops::compare(&direct, &other, "t1");
    assert!(
        d.same && d.cluster_tiers.iter().all(|t| t.same),
        "{:?}",
        d.cluster_tiers
    );
}

/// Review of Wave 14, A8. `verify` and the snapshot that `compare` reads read the group live. When
/// strata cannot mask a password in the values, both give `serialize`, and a record taken again
/// gives the same error. Quote the template, or change the block scalar, and both work.
#[test]
fn verify_and_a_snapshot_of_a_group_with_a_password_that_strata_cannot_mask_give_serialize() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let serve = |values: &str| {
        let mut doc = group_doc(dir.path());
        doc["spec"]["clustersConfig"] = clusters_config(6000, values);
        serve_cluster_group(MemoryApi::new(), dir.path()).with(group_route("g1"), doc)
    };
    // a block scalar has no line to change, and the unquoted template is a map in a key
    let unmaskable = "adminPassword: |\n  s3cr3t\nhost: {{ .spectro.var.host }}\n";
    let quoted = "adminPassword: |\n  s3cr3t\nhost: \"{{ .spectro.var.host }}\"\n";
    let plain = "adminPassword: s3cr3t\nhost: {{ .spectro.var.host }}\n";

    let recorded = common::snapshot_of(&serve(quoted), "clustergroup:g1").unwrap();
    let verify =
        |api: MemoryApi| ops::verify(&recorded, &Palette::new(api), "t1", &Progress::silent());
    assert!(matches!(
        verify(serve(unmaskable)),
        Err(Error::Serialize(_))
    ));
    // the snapshot is the read that `compare` of a record with the live group makes
    assert!(matches!(
        common::snapshot_of(&serve(unmaskable), "clustergroup:g1"),
        Err(Error::Serialize(_))
    ));

    // the quoted template, and a password on one line, both work
    assert_eq!(
        verify(serve(quoted)).unwrap().status,
        VerifyStatus::Unchanged
    );
    assert!(common::snapshot_of(&serve(plain), "clustergroup:g1").is_ok());
}

// ---- app profiles -----------------------------------------------------------------------------

/// An app profile `shop@1.0.0`, `ap1`, with a helm tier and a manifest tier, the tier with the
/// property `replicas` at `replicas`.
fn serve_app_profile(api: MemoryApi, replicas: &str) -> MemoryApi {
    api.with(
        "/v1/dashboard/appProfiles/metadata",
        serde_json::json!({"appProfiles": [
            {"metadata": {"name": "shop", "uid": "ap1"}, "spec": {"version": "1.0.0"}},
            {"metadata": {"name": "shop", "uid": "ap0"}, "spec": {"version": "0.9.0"}},
        ]}),
    )
    .with(
        app_profile_route("ap1"),
        serde_json::json!({"metadata": {"name": "shop", "uid": "ap1"},
            "spec": {"version": "1.0.0", "template": {"appTiers": [
                {"name": "web", "uid": "t1", "type": "helm", "version": "2.1.0"},
                {"name": "db-config", "uid": "t2", "type": "manifest", "version": "1.0.0"}]}},
            "status": {"inUseApps": [{"kind": "appdeployment", "uid": "d1", "name": "shop-dev"}]}}),
    )
    .with(
        "/v1/appProfiles/ap1/tiers",
        serde_json::json!({"spec": {"appTiers": [
            {"metadata": {"name": "web", "uid": "t1"},
             "spec": {"type": "helm", "version": "2.1.0", "registryUid": "r1",
                "sourceAppTierUid": "s1", "values": "replicaCount: 2\n",
                "properties": [{"name": "replicas", "type": "number", "value": replicas}]}},
            {"metadata": {"name": "db-config", "uid": "t2"},
             "spec": {"type": "manifest", "version": "1.0.0",
                "manifests": [{"kind": "manifest", "name": "db", "uid": "m1"}]}},
        ]}}),
    )
    .with(
        "/v1/appProfiles/ap1/tiers/t2/manifests/m1",
        serde_json::json!({"metadata": {"name": "db", "uid": "m1"},
            "spec": {"published": {"content": "kind: ConfigMap\n", "digest": "abc"}}}),
    )
}

#[test]
fn an_app_profile_record_holds_each_tier_as_a_pack() {
    let api = serve_app_profile(MemoryApi::new(), "2");
    let take = common::snapshot_of;
    let app = take(&api, "appprofile:shop@1.0.0").unwrap();
    assert_eq!(app.source.kind, "app");
    assert_eq!(
        (app.source.name.as_str(), app.source.version.as_str()),
        ("shop", "1.0.0")
    );
    assert!(
        app.source.consumers.is_none(),
        "its consumers are app deployments"
    );
    assert_eq!(
        app.packs
            .iter()
            .map(|p| (p.key.as_str(), p.kind.as_str(), p.tag.as_str()))
            .collect::<Vec<_>>(),
        [
            ("app/web", "helm", "2.1.0"),
            ("app/db-config", "manifest", "1.0.0")
        ]
    );
    assert_eq!(app.packs[0].params, serde_json::json!({"replicas": "2"}));
    assert_eq!(app.packs[1].manifests[0].content, "kind: ConfigMap\n");
    assert!(app.packs[0].upstream_unknown(), "a tier has no digest");
    // by uid, and the highest version by name
    assert_eq!(take(&api, "appprofile:ap1").unwrap().root, app.root);
    assert_eq!(take(&api, "appprofile:shop@latest").unwrap().root, app.root);
    assert_eq!(
        take(&api, "appprofile:shop@2.0.0").unwrap_err().kind(),
        "app-profile-not-found"
    );

    // verify reads the app profile again
    let v = ops::verify(&app, &Palette::new(api.clone()), "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged, "{:?}", v.status);
    // a plain update of a property: the record drifts, and the tier is AUTHORED
    let updated = serve_app_profile(MemoryApi::new(), "3");
    let v = ops::verify(
        &app,
        &Palette::new(updated.clone()),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(!v.unchanged);
    let now = take(&updated, "appprofile:ap1").unwrap();
    let d = ops::compare(&app, &now, "t1");
    let web = d.packs.iter().find(|p| p.name == "web").unwrap();
    assert!(
        web.authored && web.params_changed && !web.identity,
        "{web:?}"
    );
    assert_eq!(
        serde_json::to_value(&web.param_changes).unwrap(),
        serde_json::json!([{"op": "changed", "name": "replicas", "old": "2", "new": "3"}])
    );
    let text = common::terminal().render(&d, &app, &now, false).unwrap();
    assert!(text.contains("      ~ param replicas: 2 -> 3\n"), "{text}");
}

/// The tiers of `ap1` as `GET /v1/appProfiles/ap1/tiers` gives them: `web`, `db-config`, and
/// `cache`, in that order, with the install order of `web` at `web_order`, or none. Palette gives
/// no `installOrder` for a tier whose order is 0, as for `db-config` and `cache`.
fn app_tiers(web_order: Option<i64>) -> Value {
    let mut web = serde_json::json!({"metadata": {"name": "web", "uid": "t1"},
        "spec": {"type": "helm", "version": "2.1.0", "registryUid": "r1",
            "sourceAppTierUid": "s1", "values": "replicaCount: 2\n",
            "properties": [{"name": "replicas", "type": "number", "value": "2"}]}});
    if let Some(order) = web_order {
        web["spec"]["installOrder"] = order.into();
    }
    serde_json::json!({"spec": {"appTiers": [
        web,
        {"metadata": {"name": "db-config", "uid": "t2"},
         "spec": {"type": "manifest", "version": "1.0.0",
            "manifests": [{"kind": "manifest", "name": "db", "uid": "m1"}]}},
        {"metadata": {"name": "cache", "uid": "t3"},
         "spec": {"type": "container", "version": "7.2.0", "containerRegistryUid": "cr1"}},
    ]}})
}

/// A record of `appprofile:ap1` from an API that gives `tiers` for its tiers.
fn app_record(tiers: Value) -> Snapshot {
    let api = serve_app_profile(MemoryApi::new(), "2").with("/v1/appProfiles/ap1/tiers", tiers);
    common::snapshot_of(&api, "appprofile:ap1").unwrap()
}

/// The root that strata 0.16.0 gives `appprofile:ap1` of [`serve_app_profile`] with the replicas
/// `2`, before a record read the install order of a tier.
const APP_ROOT_BEFORE_INSTALL_ORDER: &str =
    "0486a3007718b8c492a8fc059906794594fee3923a6f4e01b100d7e72c769972";

#[test]
fn an_app_profile_whose_tiers_have_no_install_order_keeps_the_root_of_an_older_record() {
    let api = serve_app_profile(MemoryApi::new(), "2");
    let app = common::snapshot_of(&api, "appprofile:ap1").unwrap();
    assert_eq!(app.root, APP_ROOT_BEFORE_INSTALL_ORDER);
}

#[test]
fn an_app_profile_record_lists_the_tiers_by_install_order_and_keeps_the_listing_order_at_a_tie() {
    let app = app_record(app_tiers(Some(2)));
    assert_eq!(
        app.packs
            .iter()
            .map(|p| (p.key.as_str(), p.params.get("installOrder")))
            .collect::<Vec<_>>(),
        [
            ("app/db-config", None),
            ("app/cache", None),
            ("app/web", Some(&Value::from(2))),
        ],
        "a tier with no installOrder is at 0, and keeps its place in the listing"
    );
    assert_eq!(
        app.packs[1].params,
        serde_json::json!({"containerRegistryUid": "cr1"}),
        "a container tier keeps the registry of its image"
    );
    assert_eq!(
        app.packs[2].params,
        serde_json::json!({"replicas": "2", "installOrder": 2})
    );
    let listed = app_record(app_tiers(None));
    assert_eq!(
        listed
            .packs
            .iter()
            .map(|p| p.key.as_str())
            .collect::<Vec<_>>(),
        ["app/web", "app/db-config", "app/cache"],
        "with no installOrder, the record keeps the order of the listing"
    );
}

/// The text of `compare` from `a` to `b`, from the line of the tier `web` on.
fn web_lines(a: &Snapshot, b: &Snapshot) -> String {
    let d = ops::compare(a, b, "t1");
    assert!(!d.same, "compare exits 1");
    let web = d.packs.iter().find(|p| p.name == "web").unwrap();
    assert!(
        web.authored && web.params_changed && !web.identity,
        "{web:?}"
    );
    let text = common::terminal().render(&d, a, b, false).unwrap();
    let tier = text.find("] web ").unwrap_or_else(|| panic!("{text}"));
    text[tier..].to_string()
}

#[test]
fn two_app_profiles_whose_tiers_differ_in_install_order_alone_have_two_roots() {
    let (none, first, second) = (
        app_record(app_tiers(None)),
        app_record(app_tiers(Some(1))),
        app_record(app_tiers(Some(2))),
    );
    assert_ne!(first.root, second.root);
    assert_ne!(none.root, first.root);
    let lines = web_lines(&first, &second);
    assert!(
        lines.contains("      ~ param installOrder: 1 -> 2\n"),
        "{lines}"
    );
    let lines = web_lines(&none, &first);
    assert!(lines.contains("      + param installOrder: 1\n"), "{lines}");
}

/// The consumers of an app profile are its app deployments: `status.inUseApps`, each read with
/// `GET /v1/appDeployments/{uid}` for its state and the version its copy names.
#[test]
fn consumers_of_an_app_profile_are_its_app_deployments() {
    let api = serve_app_profile(MemoryApi::new(), "2").with(
        "/v1/appDeployments/d1",
        serde_json::json!({
            "metadata": {"name": "shop-dev", "uid": "d1", "annotations": {"projectUid": "p1"}},
            "spec": {"profile": {"metadata": {"name": "shop", "uid": "ap1", "version": "1.0.0"}}},
            "status": {"state": "Deployed"},
        }),
    );
    let consumers = |api: &MemoryApi, r: &Ref| {
        // by reference, so that `calls` counts each request of each client
        ops::consumers(r, |_| Ok(Palette::new(api)), "t1", &Progress::silent()).unwrap()
    };

    let r = consumers(&api, &Ref::parse("appprofile:shop@1.0.0"));
    assert_eq!(r.exit_code(), 1, "a deployment uses the version");
    assert_eq!(
        (r.profile.uid.as_str(), r.profile.kind),
        ("ap1", Some(ProfileType::App))
    );
    assert_eq!(r.count, 1);
    assert_eq!(r.overwritten, 0);
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(json["profile"]["type"], "app");
    assert_eq!(
        json["consumers"][0],
        serde_json::json!({"kind": "app", "uid": "d1", "name": "shop-dev", "projectUid": "p1",
            "state": "Deployed", "version": "1.0.0"})
    );
    assert_eq!(api.calls(&app_profile_route("ap1")), 1);
    assert_eq!(api.calls("/v1/appDeployments/d1"), 1);
    assert_eq!(
        api.calls(CLUSTER_LISTING),
        0,
        "no second reading: the listing of the app deployments filters no profile"
    );
    assert!(r.clusters_read.skipped && r.edge_hosts_read.skipped);
    assert_eq!(json["clustersRead"]["reason"], ops::APP_NO_SECOND_READING);
    assert_eq!(json["edgeHostsRead"]["reason"], ops::APP_NO_EDGE_HOSTS);

    let text = common::terminal().render_consumers(&r).unwrap();
    assert!(
        text.starts_with("shop@1.0.0 (uid ap1): 1 consumer\n"),
        "{text}"
    );
    assert!(
        text.contains("  shop-dev  d1  project p1  Deployed  app deployment\n"),
        "{text}"
    );
    assert!(
        text.contains(&format!(
            "second reading: not done ({})",
            ops::APP_NO_SECOND_READING
        )),
        "{text}"
    );
    let md = common::markdown().render_consumers(&r).unwrap();
    assert!(
        md.contains("| `shop-dev` | `d1` | `p1` | Deployed | `1.0.0` (app deployment) |"),
        "{md}"
    );

    // by uid and by the highest version, the same answer
    for text in ["appprofile:ap1", "appprofile:shop@latest"] {
        let by = consumers(&api, &Ref::parse(text));
        assert_eq!(serde_json::to_value(&by).unwrap(), json, "{text}");
    }

    // a record of the app profile gives the same answer as its ref
    let dir = tempfile::tempdir().unwrap();
    let record = common::snapshot_of(&api, "appprofile:shop@1.0.0").unwrap();
    let path = dir.path().join("shop.json");
    common::write_snapshot(&path, &record, false);
    let from_record = consumers(&api, &Ref::Snapshot(path));
    assert_eq!(serde_json::to_value(&from_record).unwrap(), json);

    // a deployment that Palette updated to another version of the profile runs a copy that names
    // it, and one that cannot be read still counts
    let moved = api
        .clone()
        .with(
            "/v1/appDeployments/d1",
            serde_json::json!({"metadata": {"name": "shop-dev", "uid": "d1"},
                "spec": {"profile": {"metadata": {"version": "0.9.0"}}},
                "status": {"state": "Updating"}}),
        )
        .failing("/v1/appDeployments/d2", 403, "no access");
    let mut doc: Value = moved.get_json(&app_profile_route("ap1")).unwrap();
    doc["status"]["inUseApps"].as_array_mut().unwrap().push(
        serde_json::json!({"kind": "appdeployment", "uid": "d2", "name": "shop-qa",
            "projectUid": "p2"}),
    );
    let moved = moved.with(app_profile_route("ap1"), doc);
    let r = consumers(&moved, &Ref::parse("appprofile:ap1"));
    assert_eq!((r.count, r.overwritten, r.exit_code()), (2, 1, 1));
    assert_eq!(
        (
            r.consumers[0].state.as_str(),
            r.consumers[0].version.as_str()
        ),
        ("Updating", "0.9.0")
    );
    assert_eq!(r.consumers[1].state, ops::UNKNOWN_STATE);
    assert!(r.consumers[1].error.is_some());

    // no deployment: no consumers, exit 0
    let mut doc: Value = api.get_json(&app_profile_route("ap1")).unwrap();
    doc["status"]["inUseApps"] = serde_json::json!([]);
    let free = api.clone().with(app_profile_route("ap1"), doc);
    let r = consumers(&free, &Ref::parse("appprofile:shop@1.0.0"));
    assert_eq!((r.count, r.exit_code()), (0, 0));
    let text = common::terminal().render_consumers(&r).unwrap();
    assert!(
        text.starts_with("shop@1.0.0 (uid ap1): no consumers\n"),
        "{text}"
    );
}

// ---- behind -----------------------------------------------------------------------------------

/// A version of csi-aws-ebs in the Public Repo with a state, or for other clouds.
fn csi_summary_with(version: &str, state: Option<&str>, clouds: &[&str]) -> Value {
    let mut s = csi_summary(
        &format!("pk-{version}"),
        version,
        "reg-public",
        "sha256:ccc",
        CSI_65,
    );
    s["spec"]["cloudTypes"] = serde_json::json!(clouds);
    s["metadata"]["creationTimestamp"] = serde_json::json!("2026-09-20T08:00:00.000Z");
    if let Some(state) = state {
        s["spec"]["annotations"] = serde_json::json!({ "system_state": state });
    }
    s
}

/// A registry with 1.63.1 and 1.65.0, and 1.64.0 deprecated, 1.66.0 disabled, and 1.67.0 for GCP
/// alone.
fn serve_behind() -> MemoryApi {
    serve_registry(
        vec![
            csi_summary_with("1.64.0", Some("deprecated"), &["aws"]),
            csi_summary_with("1.66.0", Some("disabled"), &["aws"]),
            csi_summary_with("1.67.0", None, &["gcp"]),
        ],
        // Palette serves no disabled version, so 1.66.0 has no tag
        &[(
            "reg-public",
            with_tag(
                with_tag(public_entity(), "1.64.0", "pk-1.64.0"),
                "1.67.0",
                "pk-1.67.0",
            ),
        )],
    )
}

/// What the three commands that ask if a pack supports a cloud say for the cloud `cloud`.
/// The registry has csi-aws-ebs at 1.63.1 for the clouds `pinned` and at 1.65.0 for the
/// clouds `newer`. The answers, in order: `packs --cloud` keeps 1.65.0; `behind` names 1.65.0
/// as newer for a record of 1.63.1; a candidate of the cloud takes 1.65.0.
fn a_pack_supports_the_cloud(pinned: &[&str], newer: &[&str], cloud: &str) -> [bool; 3] {
    let mut old = csi_summary("pk-63", "1.63.1", "reg-public", "sha256:aaa", CSI_63);
    old["spec"]["cloudTypes"] = serde_json::json!(pinned);
    let mut new = csi_summary("pk-65", "1.65.0", "reg-public", "sha256:bbb", CSI_65);
    new["spec"]["cloudTypes"] = serde_json::json!(newer);
    let api = serve_summaries(vec![old, new], &[("reg-public", public_entity())]);
    let listing = packs_of(
        &api,
        "csi-aws-ebs",
        ops::PackOptions {
            cloud: Some(cloud.to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    let packs = listing
        .registries
        .iter()
        .flat_map(|r| &r.versions)
        .any(|v| v.version == "1.65.0");
    let dir = tempfile::tempdir().unwrap();
    let record = snapshot_of(&api, "pack:csi-aws-ebs@1.63.1").unwrap();
    let path = dir.path().join("pin.json");
    common::write_snapshot(&path, &record, false);
    let report = behind_of(&api, &[path], &ops::BehindOptions::default());
    let behind = report.pins[0].newer.iter().any(|n| n.version == "1.65.0");
    let file = dir.path().join("c.yaml");
    write_yaml(
        &file,
        &serde_json::json!({"name": "probe", "version": "1.0.0", "type": "add-on",
            "cloudType": cloud,
            "layers": [{"name": "csi-aws-ebs", "version": "1.65.0", "registry": "Public Repo",
                "values": "a: 1\n"}]}),
    );
    let candidate = match candidate_snapshot(&file, &api) {
        Ok(_) => true,
        Err(e) => {
            assert!(
                matches!(
                    e,
                    Error::CandidateInvalid {
                        rule: Some(CreateRule::Cloud),
                        ..
                    }
                ),
                "{e}"
            );
            false
        }
    };
    [packs, behind, candidate]
}

#[test]
fn packs_behind_and_the_candidate_check_agree_on_the_clouds_that_a_pack_supports() {
    // each row: the cloud types of the version 1.65.0, then whether it supports the cloud aws, a
    // custom cloud, and the cloud all. The pinned version is of the cloud that the row asks
    let table: &[(&[&str], [bool; 3])] = &[
        (&[], [true, true, true]),
        (&[CLOUD_ALL], [true, true, true]),
        (&["aws"], [true, false, true]),
        (&["gcp"], [false, false, true]),
        (&[CLOUD_CUSTOM], [false, true, true]),
    ];
    for (newer, expected) in table {
        for (cloud, want) in ["aws", "my-cloud", CLOUD_ALL].into_iter().zip(expected) {
            let got = a_pack_supports_the_cloud(&[cloud], newer, cloud);
            assert_eq!(
                got, [*want; 3],
                "{newer:?} on {cloud}: packs, behind, the candidate"
            );
        }
    }
}

/// The record of the cluster document `cluster`, which runs no profile, from a raw directory.
fn record_of_cluster_document(cluster: &Value) -> strata::snapshot::Snapshot {
    let dir = tempfile::tempdir().unwrap();
    let raw = dir.path().join("c");
    fs::create_dir(&raw).unwrap();
    fs::write(raw.join("cluster.json"), cluster.to_string()).unwrap();
    fs::write(raw.join("manifests.json"), "{}").unwrap();
    fs::write(raw.join("variables.json"), "[]").unwrap();
    ops::snapshot_from_dir(&raw, "c", "t0").unwrap()
}

/// A cluster that runs no profile gets the cloud type of its document: the kind of its cloud
/// config. The route of the cloud config reads the same kind.
#[test]
fn a_cluster_record_reads_the_cloud_type_of_the_document_from_its_cloud_config() {
    let snap = record_of_cluster_document(&serde_json::json!({
        "metadata": {"uid": "c1", "name": "barney"},
        "spec": {"cloudConfigRef": {"kind": "eks", "uid": "cc1"}, "clusterProfileTemplates": []},
    }));
    assert_eq!(snap.source.cloud_type, "eks");
}

/// A label of Palette is text. So the record of a cluster omits a label of another type,
/// as `list clusters` does, and `--tag` matches the same labels in both.
#[test]
fn a_cluster_record_leaves_out_a_label_whose_value_is_not_text() {
    let snap = record_of_cluster_document(&serde_json::json!({
        "metadata": {"uid": "c1", "name": "barney", "labels": {"env": "dev", "tier": 1}},
        "spec": {"cloudType": "aws", "clusterProfileTemplates": []},
    }));
    let labels = &snap.source.cluster.as_ref().unwrap().labels;
    assert_eq!(
        labels.iter().collect::<Vec<_>>(),
        vec![(&"env".to_string(), &"dev".to_string())]
    );
}

/// A listing entry with no `spec.type` is a pack of a pack registry. So `packs`, a record, and
/// `behind` all give it the type `spectro`, and `behind` names the last sync of its registry.
#[test]
fn packs_a_record_and_behind_agree_on_the_type_of_a_listing_entry_with_no_type() {
    let untyped = |mut summary: Value| {
        summary["spec"].as_object_mut().unwrap().remove("type");
        summary
    };
    let api = serve_summaries(
        vec![
            untyped(csi_summary(
                "pk-63",
                "1.63.1",
                "reg-public",
                "sha256:aaa",
                CSI_63,
            )),
            untyped(csi_summary(
                "pk-65",
                "1.65.0",
                "reg-public",
                "sha256:bbb",
                CSI_65,
            )),
        ],
        &[("reg-public", public_entity())],
    )
    .with(
        PACK_REGISTRY_SUMMARY,
        public_repo_summary(serde_json::json!({
            "isSyncSupported": true, "status": "Failed", "message": "registry gone",
        })),
    );
    let listing = packs_of(&api, "csi-aws-ebs", ops::PackOptions::default()).unwrap();
    let kinds: Vec<&str> = listing.registries[0]
        .versions
        .iter()
        .map(|v| v.kind.as_str())
        .collect();
    assert_eq!(kinds, vec!["spectro", "spectro"]);
    let record = snapshot_of(&api, "pack:csi-aws-ebs@1.65.0").unwrap();
    assert_eq!(record.packs[0].kind, "spectro");
    let (r, _) = behind_current_on(&api);
    assert_eq!(
        r.pins[0].registry_sync.as_ref().map(|s| s.status.as_str()),
        Some("Failed"),
        "{:?}",
        r.pins[0]
    );
}

/// The listing gives the layer of each version, and the registry route gives one layer for the
/// whole pack. `packs` and a record both take the layer of the version first.
#[test]
fn packs_and_a_record_take_the_layer_of_the_version_before_the_layer_of_the_registry_route() {
    let mut entity = public_entity();
    entity["layer"] = serde_json::json!("addon");
    let api = serve_registry(vec![], &[("reg-public", entity)]);
    let listing = packs_of(&api, "csi-aws-ebs", ops::PackOptions::default()).unwrap();
    assert_eq!(listing.registries[0].layer, "csi");
    let record = snapshot_of(&api, "pack:csi-aws-ebs@1.65.0").unwrap();
    assert_eq!(record.packs[0].layer, "csi");
}

/// Behind on a record of csi-aws-ebs@1.65.0, which is current, against `api`: the report, and
/// its terminal text.
fn behind_current_on(api: &MemoryApi) -> (ops::BehindReport, String) {
    let dir = tempfile::tempdir().unwrap();
    let current = snapshot_of(api, "pack:csi-aws-ebs@1.65.0").unwrap();
    let path = dir.path().join("current.json");
    common::write_snapshot(&path, &current, false);
    let r = behind_of(api, &[path], &ops::BehindOptions::default());
    let text = common::terminal().render_behind(&r).unwrap();
    (r, text)
}

#[test]
fn a_pin_that_is_current_names_the_last_sync_of_its_registry_when_that_sync_did_not_complete() {
    // the last sync failed: the copy of the registry in Palette is the copy of the last
    // complete sync, and the pin is current against that copy
    let api = serve_behind().with(
        PACK_REGISTRY_SUMMARY,
        public_repo_summary(serde_json::json!({
            "isSyncSupported": true, "lastSyncedTime": "2025-10-16T08:00:00.000Z",
            "lastRunTime": "2026-10-05T00:00:08.683Z", "status": "Failed",
            "message": "invalid helm registry http status 404",
        })),
    );
    let (r, text) = behind_current_on(&api);
    let pin = &r.pins[0];
    assert!(!pin.behind && pin.error.is_none(), "{pin:?}");
    assert_eq!(
        pin.registry_sync.as_ref().map(|s| s.status.as_str()),
        Some("Failed")
    );
    assert!(pin.registry_sync_error.is_none());
    assert_eq!(r.exit_code(), 0);
    let note = "current; the last complete sync of the registry started \
                2025-10-16T08:00:00.000Z; the last sync of the registry failed: invalid helm \
                registry http status 404";
    assert!(
        text.contains(&format!(
            "  [csi  ] csi-aws-ebs  1.65.0  Public Repo  {note}\n"
        )),
        "{text}"
    );
    let md = common::markdown().render_behind(&r).unwrap();
    assert!(
        md.contains(&format!(
            "| `csi-aws-ebs` | `1.65.0` | Public Repo | {note} |"
        )),
        "{md}"
    );
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(
        json["pins"][0]["registrySync"],
        serde_json::json!({"lastSyncedTime": "2025-10-16T08:00:00.000Z",
            "lastRunTime": "2026-10-05T00:00:08.683Z", "status": "Failed",
            "message": "invalid helm registry http status 404"})
    );
    assert!(json["pins"][0].get("registrySyncError").is_none(), "{json}");

    // a sync runs now, after a sync that stopped: the copy is the copy of the last complete sync
    let api = serve_behind().with(
        PACK_REGISTRY_SUMMARY,
        public_repo_summary(serde_json::json!({
            "isSyncSupported": true, "lastRunTime": "2026-10-05T18:04:27.776Z",
            "lastSyncedTime": "2026-08-29T00:00:11.032Z", "status": "InProgress",
            "syncRecovery": {"message": "sync interrupted", "syncRecoveryState": "Recovering"},
        })),
    );
    let (r, text) = behind_current_on(&api);
    assert!(
        text.contains(
            "current; a sync of the registry is in progress since 2026-10-05T18:04:27.776Z, \
             and the last complete sync of the registry started 2026-08-29T00:00:11.032Z; the \
             recovery of the sync is Recovering: sync interrupted\n"
        ),
        "{text}"
    );
    assert!(
        r.pins[0]
            .registry_sync
            .as_ref()
            .is_some_and(|s| s.in_progress())
    );
}

#[test]
fn a_pin_has_no_sync_note_when_the_last_sync_completed_when_palette_gives_no_sync_or_when_it_is_behind()
 {
    // the last sync completed: Palette shows the sync, and the copy is the copy of the last run
    let completed = serde_json::json!({
        "isSyncSupported": true, "lastRunTime": "2026-09-30T00:00:11.000Z",
        "lastSyncedTime": "2026-09-30T00:00:11.000Z", "status": "Completed",
    });
    let api = serve_behind().with(PACK_REGISTRY_SUMMARY, public_repo_summary(completed));
    let (r, text) = behind_current_on(&api);
    assert!(r.pins[0].registry_sync.is_none() && r.pins[0].registry_sync_error.is_none());
    assert!(text.contains("  Public Repo  current\n"), "{text}");
    let json = serde_json::to_value(&r).unwrap();
    assert!(json["pins"][0].get("registrySync").is_none(), "{json}");

    // Palette does not serve the summary listing: no note, and no error
    let (r, text) = behind_current_on(&serve_behind());
    assert!(r.pins[0].registry_sync.is_none() && r.pins[0].registry_sync_error.is_none());
    assert!(text.contains("  Public Repo  current\n"), "{text}");

    // Palette answers the summary listing with a 5xx: the pin stays current, and the report
    // names the reason
    let api = serve_behind().failing(PACK_REGISTRY_SUMMARY, 500, r#"{"code":"InternalError"}"#);
    let (r, text) = behind_current_on(&api);
    assert!(!r.pins[0].behind && r.pins[0].registry_sync.is_none());
    assert_eq!(
        r.pins[0].registry_sync_error.as_deref(),
        Some(r#"HTTP 500: {"code":"InternalError"}"#)
    );
    assert_eq!(r.exit_code(), 0);
    assert!(text.contains("  Public Repo  current\n"), "{text}");
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(
        json["pins"][0]["registrySyncError"],
        r#"HTTP 500: {"code":"InternalError"}"#
    );

    // a pin that is behind: a newer version in the copy is a newer version in the registry
    let failed = serde_json::json!({
        "isSyncSupported": true, "lastSyncedTime": "2025-10-16T08:00:00.000Z",
        "status": "Failed", "message": "invalid helm registry http status 404",
    });
    let api = serve_behind().with(PACK_REGISTRY_SUMMARY, public_repo_summary(failed));
    let dir = tempfile::tempdir().unwrap();
    let old = snapshot_of(&api, "pack:csi-aws-ebs@1.63.1").unwrap();
    let path = dir.path().join("old.json");
    common::write_snapshot(&path, &old, false);
    let r = behind_of(&api, &[path], &ops::BehindOptions::default());
    assert!(
        r.pins[0].behind && r.pins[0].registry_sync.is_none(),
        "{:?}",
        r.pins[0]
    );
    let text = common::terminal().render_behind(&r).unwrap();
    assert!(!text.contains("sync"), "{text}");
}

/// The Mirror registry with two OCI packs: `other-pack` at 1.0.0 and 2.0.0, and `third-pack` at
/// 1.0.0. Palette gives the registry in the OCI summary listing.
fn serve_two_packs_of_the_oci_registry() -> MemoryApi {
    let build = |name: &str, uid: &str, version: &str| {
        let mut s = csi_summary(uid, version, "reg-mirror", "sha256:mmm", CSI_63);
        s["spec"]["name"] = serde_json::json!(name);
        s["spec"]["type"] = serde_json::json!("oci");
        s
    };
    let serve_pack = |api: MemoryApi, name: &str, builds: Vec<Value>| {
        let tags: Vec<Value> = builds
            .iter()
            .map(|b| serde_json::json!({"tag": b["spec"]["version"], "packUid": b["metadata"]["uid"]}))
            .collect();
        serve_pack_listing(api, name, Value::Array(builds)).with(
            pack_entity_route(name, "reg-mirror"),
            serde_json::json!({"layer": "csi", "tags": tags}),
        )
    };
    let api = serve_registry(vec![], &[("reg-public", public_entity())]).with(
        OCI_REGISTRY_SUMMARY,
        serde_json::json!({"items": [{"metadata": {"uid": "reg-mirror", "name": "Mirror"},
            "spec": {"providerType": "helm"}}]}),
    );
    let api = serve_pack(
        api,
        "other-pack",
        vec![
            build("other-pack", "pk-o1", "1.0.0"),
            build("other-pack", "pk-o2", "2.0.0"),
        ],
    );
    serve_pack(
        api,
        "third-pack",
        vec![build("third-pack", "pk-t1", "1.0.0")],
    )
}

/// One record in `dir` for each pack ref in `refs`, taken in the project of the pair.
fn pack_records(dir: &Path, refs: &[(&str, &str)]) -> Vec<std::path::PathBuf> {
    let seed = serve_two_packs_of_the_oci_registry();
    refs.iter()
        .map(|(pack, project)| {
            let record = snapshot_of(&seed, pack)
                .unwrap()
                .with_project(ops::scope_of(project));
            let path = dir.join(format!("{}.json", pack.replace([':', '@', '/'], "-")));
            common::write_snapshot(&path, &record, false);
            path
        })
        .collect()
}

#[test]
fn behind_and_packs_read_each_registry_listing_one_time_for_two_packs_of_one_registry() {
    let dir = tempfile::tempdir().unwrap();
    // other-pack is behind, and the compare of its bump reads the two versions. third-pack is
    // current, and its pin reads the sync of the registry
    let records = pack_records(
        dir.path(),
        &[
            ("pack:Mirror/other-pack@1.0.0", ""),
            ("pack:Mirror/third-pack@1.0.0", ""),
        ],
    );
    let api = serve_two_packs_of_the_oci_registry();
    let connects = RefCell::new(0);
    let r = ops::behind(
        &records,
        &WITH_IMPACT,
        |_| {
            *connects.borrow_mut() += 1;
            Ok(Palette::new(&api))
        },
        "2026-09-26T00:00:00Z",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(
        (r.summary.pins, r.summary.behind, r.summary.errors),
        (2, 1, 0)
    );
    assert!(r.pins[0].bump.is_some(), "the impact read the two versions");
    assert_eq!(*connects.borrow(), 1, "one client for the one scope");
    assert_eq!(api.calls(REGISTRY_LISTING), 1);
    assert_eq!(api.calls(OCI_REGISTRY_SUMMARY), 1);

    // packs reads the listing of the one pack that it names
    let api = serve_two_packs_of_the_oci_registry();
    ops::registry_packs(
        "other-pack",
        &ops::PackOptions::default(),
        |_| Ok(Palette::new(&api)),
        "2026-09-26T00:00:00Z",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(api.calls(REGISTRY_LISTING), 1);
}

#[test]
fn behind_connects_one_time_for_each_project_of_its_pins() {
    let dir = tempfile::tempdir().unwrap();
    let records = pack_records(
        dir.path(),
        &[
            ("pack:Mirror/other-pack@1.0.0", "p1"),
            ("pack:Mirror/other-pack@2.0.0", "p1"),
            ("pack:Mirror/third-pack@1.0.0", "p2"),
        ],
    );
    let api = serve_two_packs_of_the_oci_registry();
    let scopes: RefCell<Vec<Option<String>>> = RefCell::new(Vec::new());
    let connect = |scope: Option<&str>| {
        scopes.borrow_mut().push(scope.map(str::to_string));
        Ok(Palette::new(&api))
    };
    let r = ops::behind(
        &records,
        &WITH_IMPACT,
        connect,
        "2026-09-26T00:00:00Z",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(r.summary.pins, 3);
    assert_eq!(
        *scopes.borrow(),
        [Some("p1".to_string()), Some("p2".to_string())],
        "one client for each project, for the offers, the sync notes, and the bumps"
    );
    // the pins keep the order of the pack name and the version, whatever the project
    let versions: Vec<_> = r
        .pins
        .iter()
        .map(|p| (p.name.as_str(), p.version.as_str()))
        .collect();
    assert_eq!(
        versions,
        [
            ("other-pack", "1.0.0"),
            ("other-pack", "2.0.0"),
            ("third-pack", "1.0.0")
        ]
    );

    // a project that strata cannot connect to gives its pins the error. Strata reads the others
    let r = ops::behind(
        &records,
        &WITH_IMPACT,
        |scope| match scope {
            Some("p2") => Err(Error::Usage {
                message: "no key for p2".to_string(),
            }),
            _ => Ok(Palette::new(&api)),
        },
        "2026-09-26T00:00:00Z",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(
        (r.summary.pins, r.summary.behind, r.summary.errors),
        (3, 1, 1)
    );
    assert!(r.pins[2].error.is_some() && r.pins[0].error.is_none());
}

#[test]
fn behind_marks_the_newer_version_that_palette_calls_the_latest_and_names_it_on_the_bump_line() {
    // Palette reads a version with four numbers as its first three, so it reads 1.65.0.1 and
    // 1.65.0 as one version, and here it calls 1.65.0 the latest. Strata puts 1.65.0.1 above it
    let mut release = csi_summary("pk-65", "1.65.0", "reg-public", "sha256:bbb", CSI_65);
    release["metadata"]["annotations"] = serde_json::json!({"latestVersion": "true"});
    let api = serve_summaries(
        vec![
            csi_summary("pk-63", "1.63.1", "reg-public", "sha256:aaa", CSI_63),
            release,
            csi_summary_with("1.65.0.1", None, &["aws"]),
        ],
        &[(
            "reg-public",
            with_tag(public_entity(), "1.65.0.1", "pk-1.65.0.1"),
        )],
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dev.json");
    let old = snapshot_of(&api, "pack:csi-aws-ebs@1.63.1").unwrap();
    common::write_snapshot(&path, &old, false);
    let r = behind_of(&api, &[path], &WITH_IMPACT);
    let pin = &r.pins[0];
    assert_eq!(
        pin.newer
            .iter()
            .map(|n| (n.version.as_str(), n.palette_latest))
            .collect::<Vec<_>>(),
        [("1.65.0.1", false), ("1.65.0", true)]
    );
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(json["pins"][0]["newer"][1]["paletteLatest"], true);
    assert!(json["pins"][0]["newer"][0].get("paletteLatest").is_none());
    assert_eq!(pin.bump.as_ref().map(|b| b.to.as_str()), Some("1.65.0.1"));
    let text = common::terminal().render_behind(&r).unwrap();
    assert!(
        text.lines()
            .any(|l| l.starts_with("      bump to 1.65.0.1: ")
                && l.ends_with("; Palette calls 1.65.0 the latest")),
        "{text}"
    );
}

/// A registry with 1.63.1 and 1.65.0, and the pre-releases 1.65.0-rev1, 1.65.0-g0123456789ab, and
/// 1.66.0-rc.1. Palette calls 1.65.0 the latest.
fn serve_pre_releases() -> MemoryApi {
    let mut release = csi_summary("pk-65", "1.65.0", "reg-public", "sha256:bbb", CSI_65);
    release["metadata"]["annotations"] = serde_json::json!({"latestVersion": "true"});
    let pre = ["1.65.0-rev1", "1.65.0-g0123456789ab", "1.66.0-rc.1"];
    let mut entity = public_entity();
    for v in pre {
        entity = with_tag(entity, v, &format!("pk-{v}"));
    }
    let mut summaries = vec![
        csi_summary("pk-63", "1.63.1", "reg-public", "sha256:aaa", CSI_63),
        release,
    ];
    summaries.extend(pre.map(|v| csi_summary_with(v, None, &["aws"])));
    serve_summaries(summaries, &[("reg-public", entity)])
}

/// The versions that `behind` names as newer than the pack version `pin`, in the registry of
/// `serve_pre_releases`, with the option `pre_release`.
fn newer_than(pin: &str, pre_release: bool) -> Vec<String> {
    let api = serve_pre_releases();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dev.json");
    let record = snapshot_of(&api, &format!("pack:csi-aws-ebs@{pin}")).unwrap();
    common::write_snapshot(&path, &record, false);
    let r = behind_of(
        &api,
        &[path],
        &ops::BehindOptions {
            pre_release,
            ..Default::default()
        },
    );
    assert_eq!(r.pins[0].behind, !r.pins[0].newer.is_empty());
    r.pins[0].newer.iter().map(|n| n.version.clone()).collect()
}

#[test]
fn a_pack_at_latest_is_the_highest_version_that_is_not_a_pre_release() {
    let api = serve_pre_releases();
    let latest = snapshot_of(&api, "pack:csi-aws-ebs@latest").unwrap();
    assert_eq!(latest.source.version, "1.65.0");
    assert_eq!(latest.source.reference, "pack:csi-aws-ebs@latest");
    // an exact version names a pre-release
    let rc = snapshot_of(&api, "pack:csi-aws-ebs@1.66.0-rc.1").unwrap();
    assert_eq!(rc.source.version, "1.66.0-rc.1");
}

/// Review of Wave 14, A7. A version in the listing with no tag in the answer of the registry is a
/// version that Palette does not serve, for example in a FIPS tenant. `packs` omits it, and
/// `pack:<name>@latest` skips it, so the two name the same version. Another registry that serves
/// only such a version does not hold the pack for `@latest`.
#[test]
fn a_pack_at_latest_is_the_version_that_packs_marks_when_the_highest_version_has_no_tag() {
    let mut mirror = csi_summary("pk-66", "1.66.0", "reg-mirror", "sha256:ccc", CSI_65);
    mirror["spec"]["type"] = serde_json::json!("oci");
    let api = serve_registry(
        vec![
            csi_summary("pk-66", "1.66.0", "reg-public", "sha256:ccc", CSI_65),
            mirror,
        ],
        &[
            ("reg-public", public_entity()),
            (
                "reg-mirror",
                serde_json::json!({"layer": "csi", "tags": []}),
            ),
        ],
    );
    let listing = packs_of(&api, "csi-aws-ebs", ops::PackOptions::default()).unwrap();
    let marked: Vec<(&str, &str)> = listing
        .registries
        .iter()
        .flat_map(|r| {
            r.versions
                .iter()
                .filter(|v| v.latest)
                .map(|v| (r.registry.as_str(), v.version.as_str()))
        })
        .collect();
    assert_eq!(marked, [("Public Repo", "1.65.0")]);

    let latest = snapshot_of(&api, "pack:csi-aws-ebs@latest").unwrap();
    assert_eq!(latest.source.version, "1.65.0");
    assert_eq!(latest.source.reference, "pack:csi-aws-ebs@latest");
}

#[test]
fn behind_names_no_pre_release_as_newer_than_a_pin_that_is_not_one() {
    assert_eq!(newer_than("1.63.1", false), ["1.65.0"]);
    // a pre-release is below its version, so the pin of the version has nothing newer
    assert!(newer_than("1.65.0", false).is_empty());
}

#[test]
fn behind_names_a_pre_release_as_newer_when_the_option_asks_for_it() {
    assert_eq!(
        newer_than("1.63.1", true),
        [
            "1.66.0-rc.1",
            "1.65.0",
            "1.65.0-rev1",
            "1.65.0-g0123456789ab"
        ]
    );
    // the pin of a version still has a pre-release of a later version above it
    assert_eq!(newer_than("1.65.0", true), ["1.66.0-rc.1"]);
}

#[test]
fn behind_names_the_final_version_and_each_later_version_for_a_pin_that_is_a_pre_release() {
    assert_eq!(
        newer_than("1.65.0-g0123456789ab", false),
        ["1.66.0-rc.1", "1.65.0", "1.65.0-rev1"]
    );
}

/// A registry with 1.63.1 and 1.65.0, and four versions that do not parse: 1.63.1.1, with a tail
/// of numbers, the develop builds 1.65.0.develop and 1.66.0.develop, with a tail of text, and
/// 1.66.develop, with two numbers and a tail of text.
fn serve_develop_builds() -> MemoryApi {
    let more = [
        "1.63.1.1",
        "1.65.0.develop",
        "1.66.0.develop",
        "1.66.develop",
    ];
    let mut entity = public_entity();
    for v in more {
        entity = with_tag(entity, v, &format!("pk-{v}"));
    }
    serve_registry(
        more.map(|v| csi_summary_with(v, None, &["aws"])).to_vec(),
        &[("reg-public", entity)],
    )
}

/// The versions that `behind` names as newer than the pack version `pin`, in the registry of
/// `serve_develop_builds`, with the option `pre_release`.
fn newer_than_in_develop_builds(pin: &str, pre_release: bool) -> Vec<String> {
    let api = serve_develop_builds();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dev.json");
    let record = snapshot_of(&api, &format!("pack:csi-aws-ebs@{pin}")).unwrap();
    common::write_snapshot(&path, &record, false);
    let options = ops::BehindOptions {
        pre_release,
        ..Default::default()
    };
    let r = behind_of(&api, &[path], &options);
    r.pins[0].newer.iter().map(|n| n.version.clone()).collect()
}

/// Issue #616. `pack:<name>@latest` and `packs` read a version that does not parse by its first
/// three numbers. A tail of text is a pre-release, so neither picks a develop build. Issue #714:
/// `1.66.develop` is a pre-release of `1.66.0`, below `1.66.0.develop`, so neither picks it.
#[test]
fn a_pack_at_latest_and_packs_skip_a_develop_build_and_read_a_fourth_number_below_the_next_version()
{
    let api = serve_develop_builds();
    let latest = snapshot_of(&api, "pack:csi-aws-ebs@latest").unwrap();
    assert_eq!(latest.source.version, "1.65.0");
    let listing = packs_of(&api, "csi-aws-ebs", ops::PackOptions::default()).unwrap();
    let order: Vec<(&str, bool)> = listing.registries[0]
        .versions
        .iter()
        .map(|v| (v.version.as_str(), v.latest))
        .collect();
    assert_eq!(
        order,
        [
            ("1.66.0.develop", false),
            ("1.66.develop", false),
            ("1.65.0", true),
            ("1.65.0.develop", false),
            ("1.63.1.1", false),
            ("1.63.1", false),
        ]
    );
}

/// Issue #616. With `10.4.0.1` and `10.5.0`, `behind` said nothing. Now `1.65.0` is newer than the
/// pin `1.63.1.1`, and a develop build is newer only when the option asks for a pre-release. Issue
/// #714: `1.66.develop` is a pre-release of `1.66.0`, so `--pre-release` names it as newer than
/// `1.65.0`.
#[test]
fn behind_reads_a_version_that_does_not_parse_by_its_three_numbers_and_a_develop_build_as_a_pre_release()
 {
    assert_eq!(newer_than_in_develop_builds("1.63.1.1", false), ["1.65.0"]);
    assert_eq!(
        newer_than_in_develop_builds("1.63.1", false),
        ["1.65.0", "1.63.1.1"]
    );
    assert_eq!(
        newer_than_in_develop_builds("1.63.1", true),
        [
            "1.66.0.develop",
            "1.66.develop",
            "1.65.0",
            "1.65.0.develop",
            "1.63.1.1"
        ]
    );
    // a pin of a develop build follows the pre-releases, as a pin of `1.65.0-rc.1` does
    assert_eq!(
        newer_than_in_develop_builds("1.65.0.develop", false),
        ["1.66.0.develop", "1.66.develop", "1.65.0"]
    );
    assert!(newer_than_in_develop_builds("1.65.0", false).is_empty());
    assert_eq!(
        newer_than_in_develop_builds("1.65.0", true),
        ["1.66.0.develop", "1.66.develop"]
    );
    assert_eq!(
        newer_than_in_develop_builds("1.66.develop", false),
        ["1.66.0.develop"]
    );
}

#[test]
fn behind_says_what_a_move_to_a_newer_version_drops_when_it_supports_fewer_architectures() {
    // the pinned version supports both architectures, the newest supports amd64 alone, and the
    // deprecated one names none
    let mut both = csi_summary("pk-63", "1.63.1", "reg-public", "sha256:aaa", CSI_63);
    both["spec"]["annotations"] = serde_json::json!({"architecture": "arm64,amd64"});
    let mut one = csi_summary("pk-65", "1.65.0", "reg-public", "sha256:bbb", CSI_65);
    one["spec"]["annotations"] = serde_json::json!({"architecture": "amd64"});
    let api = serve_summaries(
        vec![
            both,
            one,
            csi_summary_with("1.64.0", Some("deprecated"), &["aws"]),
        ],
        &[(
            "reg-public",
            with_tag(public_entity(), "1.64.0", "pk-1.64.0"),
        )],
    );
    let dir = tempfile::tempdir().unwrap();
    let write = |name: &str, r: &str| {
        let path = dir.path().join(name);
        let snap = snapshot_of(&api, r).unwrap();
        common::write_snapshot(&path, &snap, false);
        path
    };
    let records = vec![
        write("dev.json", "pack:csi-aws-ebs@1.63.1"),
        write("old.json", "pack:csi-aws-ebs@1.64.0"),
    ];
    let r = behind_of(&api, &records, &ops::BehindOptions::default());
    let pin = &r.pins[0];
    assert_eq!(pin.version, "1.63.1");
    assert_eq!(
        pin.newer
            .iter()
            .map(|n| (n.version.as_str(), n.drops.clone()))
            .collect::<Vec<_>>(),
        [("1.65.0", vec!["arm64".to_string()]), ("1.64.0", vec![])]
    );
    // a pinned version that names no architecture says nothing about a move
    let old = &r.pins[1];
    assert_eq!(old.version, "1.64.0");
    assert!(
        old.behind && old.newer.iter().all(|n| n.drops.is_empty()),
        "{:?}",
        old.newer
    );
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(
        json["pins"][0]["newer"][0]["drops"],
        serde_json::json!(["arm64"])
    );
    assert!(json["pins"][0]["newer"][1].get("drops").is_none());
    let text = common::terminal().render_behind(&r).unwrap();
    assert!(
        text.contains("behind: 1.65.0 (drops arm64), 1.64.0 (deprecated)"),
        "{text}"
    );
    let md = common::markdown().render_behind(&r).unwrap();
    assert!(
        md.contains("behind: 1.65.0 (drops arm64), 1.64.0 (deprecated)"),
        "{md}"
    );
}

#[test]
fn in_a_fips_tenant_packs_and_behind_leave_out_a_version_that_palette_does_not_serve() {
    // 1.68.0 is not FIPS compliant. The listing applies no FIPS rule, so it holds 1.68.0 in each
    // tenant. The registry route applies the FIPS rule of the tenant. In a FIPS tenant, it gives
    // no tag for 1.68.0. In another tenant, it gives the tag.
    let mut not_fips = csi_summary_with("1.68.0", None, &["aws"]);
    not_fips["spec"]["annotations"] = serde_json::json!({"fips": "false"});
    let serve = |entity: Value| serve_registry(vec![not_fips.clone()], &[("reg-public", entity)]);
    let fips = serve(public_entity());
    let open = serve(with_tag(public_entity(), "1.68.0", "pk-1.68.0"));

    let versions = |api: &MemoryApi| {
        let l = packs_of(api, "csi-aws-ebs", ops::PackOptions::default()).unwrap();
        let r = &l.registries[0];
        let v: Vec<String> = r.versions.iter().map(|v| v.version.clone()).collect();
        (v, r.versions_offered)
    };
    assert_eq!(
        versions(&fips),
        (vec!["1.65.0".to_string(), "1.63.1".to_string()], 2),
        "packs omits the version that the registry route does not serve"
    );
    assert_eq!(versions(&open).0[0], "1.68.0", "another tenant offers it");

    let dir = tempfile::tempdir().unwrap();
    let record = dir.path().join("dev.json");
    let old = snapshot_of(&fips, "pack:csi-aws-ebs@1.63.1").unwrap();
    common::write_snapshot(&record, &old, false);
    let newer = |api: &MemoryApi| {
        let r = behind_of(
            api,
            std::slice::from_ref(&record),
            &ops::BehindOptions::default(),
        );
        r.pins[0]
            .newer
            .iter()
            .map(|n| n.version.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        newer(&fips),
        ["1.65.0"],
        "behind offers no version that Palette refuses in a profile"
    );
    assert_eq!(newer(&open), ["1.68.0", "1.65.0"]);
}

#[test]
fn in_a_fips_tenant_packs_and_behind_offer_no_version_of_a_pack_with_no_fips_version() {
    // No version is FIPS compliant. So in a FIPS tenant, the registry route of Palette serves no
    // version, and answers 404. The listing still holds each version.
    let not_fips = |uid: &str, version: &str| {
        let mut s = csi_summary(uid, version, "reg-public", "sha256:ccc", CSI_63);
        s["spec"]["annotations"] = serde_json::json!({"fips": "false"});
        s
    };
    let fips = serve_summaries(
        vec![not_fips("pk-63", "1.63.1"), not_fips("pk-65", "1.65.0")],
        &[],
    );
    let l = packs_of(&fips, "csi-aws-ebs", ops::PackOptions::default()).unwrap();
    assert!(
        l.registries.is_empty(),
        "packs offers no version: {:?}",
        l.registries
    );

    // a record that another tenant made pins 1.63.1
    let dir = tempfile::tempdir().unwrap();
    let record = dir.path().join("dev.json");
    let old = snapshot_of(
        &serve_registry(vec![], &[("reg-public", public_entity())]),
        "pack:csi-aws-ebs@1.63.1",
    )
    .unwrap();
    common::write_snapshot(&record, &old, false);
    let report = behind_of(
        &fips,
        std::slice::from_ref(&record),
        &ops::BehindOptions::default(),
    );
    let pin = &report.pins[0];
    assert!(pin.newer.is_empty(), "{:?}", pin.newer);
    assert_eq!(pin.pinned_state.as_ref().map(Open::as_str), Some("absent"));
}

#[test]
fn behind_names_the_newer_versions_of_each_pin_and_the_impact_of_the_bump() {
    let dir = tempfile::tempdir().unwrap();
    let api = serve_behind();
    let write = |name: &str, snap: &Snapshot| {
        let path = dir.path().join(name);
        common::write_snapshot(&path, snap, false);
        path
    };
    let old = snapshot_of(&api, "pack:csi-aws-ebs@1.63.1").unwrap();
    let new = snapshot_of(&api, "pack:csi-aws-ebs@1.65.0").unwrap();
    let records = vec![
        write("dev.json", &old),
        write("prod.json", &old),
        write("next.json", &new),
    ];
    let r = behind_of(&api, &records, &WITH_IMPACT);
    assert_eq!(r.schema, ops::BEHIND_SCHEMA);
    assert_eq!(
        (r.summary.pins, r.summary.behind, r.summary.errors),
        (2, 1, 0)
    );
    assert_eq!(r.exit_code(), 1, "a pin is behind");
    let pin = &r.pins[0];
    assert_eq!(
        (
            pin.name.as_str(),
            pin.version.as_str(),
            pin.registry.as_str()
        ),
        ("csi-aws-ebs", "1.63.1", "Public Repo")
    );
    assert_eq!(pin.pinned_by.len(), 2, "one pin for the two records");
    // newest first; a disabled version is not one to move to, and a version for GCP alone is not
    // for this pin
    assert_eq!(
        pin.newer
            .iter()
            .map(|n| (n.version.as_str(), n.state.as_ref().map(Open::as_str)))
            .collect::<Vec<_>>(),
        [("1.65.0", None), ("1.64.0", Some("deprecated"))]
    );
    // when Palette first listed a version, from the listing
    assert_eq!(pin.newer[1].listed, "2026-09-20T08:00:00.000Z");
    assert_eq!(
        pin.newer[0].listed, "",
        "the listing gives no time for 1.65.0"
    );
    let bump = pin.bump.as_ref().unwrap();
    assert_eq!(bump.to, "1.65.0");
    assert!(
        bump.flags.iter().any(|f| *f == ChangeFlag::Identity),
        "{:?}",
        bump.flags
    );
    assert_eq!(bump.impact, strata::diff::impact::ImpactLevel::ClusterWide);
    let current = &r.pins[1];
    assert_eq!(current.version, "1.65.0");
    assert!(!current.behind && current.bump.is_none());

    let text = common::terminal().render_behind(&r).unwrap();
    assert!(
        text.starts_with("2 pins from 3 records: 1 behind\n"),
        "{text}"
    );
    assert!(
        text.contains("behind: 1.65.0, 1.64.0 (deprecated)")
            && text.contains("      bump to 1.65.0: cluster-wide IDENTITY")
            && text.contains("pinned by ")
            && text.contains("current"),
        "{text}"
    );
    let md = common::markdown().render_behind(&r).unwrap();
    assert!(
        md.contains("| `csi-aws-ebs` | `1.63.1` | Public Repo | behind: 1.65.0"),
        "{md}"
    );

    // --no-impact reads no pack version for the bump
    let quick = behind_of(&api, &records, &ops::BehindOptions::default());
    assert!(quick.pins[0].bump.is_none() && quick.pins[0].behind);

    // the next run names what arrived since this one: here the previous run did not know 1.64.0
    let mut previous = r.clone();
    previous.pins[0].newer.retain(|n| n.version != "1.64.0");
    let next = behind_of(
        &api,
        &records,
        &ops::BehindOptions {
            impact: false,
            since: Some(previous),
            pin: None,
            pre_release: false,
        },
    );
    assert_eq!(
        next.pins[0].arrived.as_deref(),
        Some(&["1.64.0".to_string()][..])
    );
    assert_eq!(next.pins[1].arrived.as_deref(), Some(&[][..]));
    assert_eq!(next.summary.arrived, 1);
    assert_eq!(next.since.as_deref(), Some("2026-09-26T00:00:00Z"));
    let json = serde_json::to_value(&next).unwrap();
    assert_eq!(json["pins"][0]["arrived"], serde_json::json!(["1.64.0"]));
}

// ---- behind asks Palette about the bump ------------------------------------------------------

/// The uid of the profile version that the records of the question pin csi-aws-ebs in.
const BUMP_PROFILE_UID: &str = "pr1";

/// The name of the cluster that the tests of the question run.
const BUMP_CLUSTER: &str = "payments";

/// What Palette says about a move of csi-aws-ebs from 1.63.1 to 1.65.0 on the cluster `payments`.
const UPGRADE_REFUSAL: &str = "Pack 'csi-aws-ebs' cannot move from 1.63.1 to 1.65.0 on the \
                               clusters: payments";

/// The answer of `validate/packs` of a profile with the error of a move that the pack does not
/// allow, for the pack csi-aws-ebs.
fn upgrade_refusal(message: &str) -> Value {
    serde_json::json!({"packs": {"results": [{"name": "csi-aws-ebs",
        "errors": [{"code": UPGRADE_NOT_SUPPORTED_CODE, "message": message}]}]}})
}

/// The record of a profile version that pins csi-aws-ebs 1.63.1: the record of the pack, with the
/// uid and the name of a profile version.
fn profile_pinning(api: &MemoryApi) -> strata::snapshot::Snapshot {
    let mut snap = snapshot_of(api, "pack:csi-aws-ebs@1.63.1").unwrap();
    snap.source.pack = None;
    snap.source.uid = BUMP_PROFILE_UID.to_string();
    snap.source.name = "csi-stack".to_string();
    snap.source.version = "1.0.0".to_string();
    snap.source.kind = ProfileType::AddOn.as_str().to_string();
    snap
}

/// The record of the cluster `payments`, which runs `profile_pinning`: the same pack, in a record
/// of a cluster with the uid `c1`.
fn cluster_pinning(api: &MemoryApi) -> strata::snapshot::Snapshot {
    let mut snap = profile_pinning(api);
    snap.source.cluster = Some(strata::snapshot::ClusterSource {
        uid: "c1".to_string(),
        name: BUMP_CLUSTER.to_string(),
        ..Default::default()
    });
    snap
}

/// The document of the cluster `c1`: it runs `profile_pinning`, and its template names the pack
/// with the tag and the values of the record.
fn cluster_document_running(snap: &strata::snapshot::Snapshot) -> Value {
    let pack = &snap.packs[0];
    serde_json::json!({"metadata": {"name": BUMP_CLUSTER, "uid": "c1"},
        "spec": {"clusterProfileTemplates": [
            {"uid": "infra1", "name": "base", "profileVersion": "2.0.0", "type": "infra",
             "packs": [{"name": "kubernetes", "tag": "1.35.3", "type": "spectro",
                        "values": "k: 1"}]},
            {"uid": BUMP_PROFILE_UID, "name": "csi-stack", "profileVersion": "1.0.0",
             "type": "add-on",
             "packs": [{"name": pack.name, "tag": pack.tag, "type": pack.kind,
                        "values": "cluster: 1"}]}]}})
}

/// The profile document of `profile_pinning`: it names the two clusters that run the version.
fn profile_document_running() -> Value {
    serde_json::json!({"metadata": {"name": "csi-stack", "uid": BUMP_PROFILE_UID},
        "status": {"inUseClusters": [
            {"uid": "c1", "name": BUMP_CLUSTER}, {"uid": "c2", "name": "billing"}]}})
}

#[test]
fn behind_prints_the_refusal_of_palette_for_the_bump_of_a_record_of_a_profile_with_its_cluster() {
    let dir = tempfile::tempdir().unwrap();
    let base = serve_behind();
    let record = profile_pinning(&base);
    let target = snapshot_of(&base, "pack:csi-aws-ebs@1.65.0").unwrap();
    let route = post(&update_validate_packs_route(BUMP_PROFILE_UID));
    let api = base
        .with(route.clone(), upgrade_refusal(UPGRADE_REFUSAL))
        .with(profile_route(BUMP_PROFILE_UID), profile_document_running());
    let csi = dir.path().join("csi.json");
    common::write_snapshot(&csi, &record, false);
    let r = behind_of(&api, &[csi], &WITH_IMPACT);
    // issue #708: the newer versions of the profile and the names of the clusters of the refusal
    // read one document
    assert_eq!(api.calls(&profile_route(BUMP_PROFILE_UID)), 1);

    // one request to the route of the version, with its packs and the bumped pack in the place of
    // the pin: the version, the tag, the uids, and the values of 1.65.0
    let posted = api.posted();
    assert_eq!(posted.len(), 1, "{posted:?}");
    assert_eq!(posted[0].0, update_validate_packs_route(BUMP_PROFILE_UID));
    let pack = &posted[0].1["packs"][0];
    assert_eq!(posted[0].1["packs"].as_array().unwrap().len(), 1);
    assert_eq!(pack["name"], "csi-aws-ebs");
    assert_eq!(pack["tag"], target.packs[0].tag.as_str());
    assert_eq!(pack["uid"], target.packs[0].pack_uid.as_str());
    assert_eq!(pack["registryUid"], target.packs[0].registry_uid.as_str());
    assert_eq!(pack["values"], target.packs[0].values.as_str());
    assert_eq!(pack["layer"], record.packs[0].layer.as_str());
    assert_ne!(target.packs[0].pack_uid, record.packs[0].pack_uid);

    // the document of the profile names the clusters, and the message names one of them
    let validation = r.pins[0].bump.as_ref().unwrap();
    let answer = validation.palette_validation.as_ref().unwrap();
    assert!(!answer.accepts());
    assert_eq!(answer.errors()[0].code, UPGRADE_NOT_SUPPORTED_CODE);
    assert_eq!(answer.errors()[0].clusters, [BUMP_CLUSTER]);
    let text = common::terminal().render_behind(&r).unwrap();
    let refusal = format!("      palette refuses: {UPGRADE_NOT_SUPPORTED_CODE} on payments\n");
    assert!(text.contains(&refusal), "{text}");
    // the answer of Palette is under the bump line
    let (bump_at, refusal_at) = (
        text.find("bump to 1.65.0").unwrap(),
        text.find(&refusal).unwrap(),
    );
    assert!(bump_at < refusal_at, "{text}");
    let md = common::markdown().render_behind(&r).unwrap();
    assert!(
        md.contains(&format!(
            "<br>palette refuses: {UPGRADE_NOT_SUPPORTED_CODE} on payments"
        )),
        "{md}"
    );

    // --json carries the code, the message, and the cluster
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(
        json["pins"][0]["bump"]["paletteValidation"],
        serde_json::json!({"accepts": false, "errors": [{
            "pack": "csi-aws-ebs", "code": UPGRADE_NOT_SUPPORTED_CODE,
            "message": UPGRADE_REFUSAL, "clusters": [BUMP_CLUSTER]}]})
    );
    assert_eq!(
        r.exit_code(),
        1,
        "the pin is behind, and the question changes no exit"
    );
    assert!(
        json["pins"][0]["bump"]
            .get("paletteValidationError")
            .is_none()
    );
}

#[test]
fn behind_prints_that_palette_accepts_when_it_gives_no_error_for_the_bump() {
    let dir = tempfile::tempdir().unwrap();
    let base = serve_behind();
    let record = profile_pinning(&base);
    let api = base.with(
        post(&update_validate_packs_route(BUMP_PROFILE_UID)),
        serde_json::json!({"packs": {"results": []}}),
    );
    let csi = dir.path().join("csi.json");
    common::write_snapshot(&csi, &record, false);
    let r = behind_of(&api, &[csi], &WITH_IMPACT);
    let answer = r.pins[0].bump.as_ref().unwrap().palette_validation.as_ref();
    assert!(answer.unwrap().accepts());
    let text = common::terminal().render_behind(&r).unwrap();
    assert!(text.contains("      palette accepts\n"), "{text}");
    assert!(!text.contains("palette refuses"), "{text}");
    assert_eq!(
        serde_json::to_value(&r).unwrap()["pins"][0]["bump"]["paletteValidation"],
        serde_json::json!({"accepts": true})
    );
    // Palette gave no error, so strata read no document of the profile for the names. It reads
    // the document one time, for the versions of the profile
    assert_eq!(api.calls(&profile_route(BUMP_PROFILE_UID)), 1);
}

#[test]
fn behind_with_no_impact_makes_no_validate_request() {
    let dir = tempfile::tempdir().unwrap();
    let base = serve_behind();
    let record = profile_pinning(&base);
    let cluster = cluster_pinning(&base);
    let api = base
        .with(
            post(&update_validate_packs_route(BUMP_PROFILE_UID)),
            upgrade_refusal(UPGRADE_REFUSAL),
        )
        .with(cluster_route("c1"), cluster_document_running(&cluster));
    let csi_file = dir.path().join("csi.json");
    let cluster_file = dir.path().join("cluster.json");
    common::write_snapshot(&csi_file, &record, false);
    common::write_snapshot(&cluster_file, &cluster, false);
    let r = behind_of(
        &api,
        &[csi_file, cluster_file],
        &ops::BehindOptions::default(),
    );
    assert!(r.pins[0].behind && r.pins[0].bump.is_none());
    assert!(api.posted().is_empty(), "{:?}", api.posted());
    assert_eq!(api.calls(&cluster_route("c1")), 0);
    // the two records hold one version of the profile. Strata reads its document one time, for the
    // versions of the profile, and not for the bump
    assert_eq!(api.calls(&profile_route(BUMP_PROFILE_UID)), 1);
    let text = common::terminal().render_behind(&r).unwrap();
    assert!(!text.contains("palette "), "{text}");
}

/// `serve_behind`, and a second pack in the Public Repo: `csi-extra` at 1.0.0 and 1.1.0.
fn serve_behind_two_packs() -> MemoryApi {
    let summary = |version: &str| {
        let mut s = csi_summary(
            &format!("pk-extra-{version}"),
            version,
            "reg-public",
            "sha256:eee",
            "a: 1\n",
        );
        s["spec"]["name"] = Value::from("csi-extra");
        s
    };
    let entity = serde_json::json!({"layer": "csi", "tags": [
        {"tag": "1.0.0", "packUid": "pk-extra-1.0.0"},
        {"tag": "1.1.0", "packUid": "pk-extra-1.1.0"}]});
    common::serve_registry_pack(
        serve_behind(),
        "csi-extra",
        serde_json::json!([summary("1.0.0"), summary("1.1.0")]),
        entity,
        "reg-public",
    )
}

#[test]
fn behind_with_a_pin_compares_and_asks_palette_about_that_pack_only() {
    let dir = tempfile::tempdir().unwrap();
    let base = serve_behind_two_packs();
    // the record of a profile version that pins two packs, and both are behind
    let mut record = profile_pinning(&base);
    let extra = snapshot_of(&base, "pack:csi-extra@1.0.0")
        .unwrap()
        .packs
        .remove(0);
    let key = extra.key.clone();
    record.packs.push(extra);
    let path = dir.path().join("csi.json");
    common::write_snapshot(&path, &record, false);
    let api = base
        .with(
            post(&update_validate_packs_route(BUMP_PROFILE_UID)),
            upgrade_refusal(UPGRADE_REFUSAL),
        )
        .with(profile_route(BUMP_PROFILE_UID), profile_document_running());

    let every = api.clone();
    let r = behind_of(&every, std::slice::from_ref(&path), &WITH_IMPACT);
    assert_eq!(r.pins.len(), 2);
    assert_eq!(every.posted().len(), 2, "one request for each pin");
    assert_eq!(r.profiles.len(), 1, "the profile version of the record");

    let one = api.clone();
    let options = ops::BehindOptions {
        pin: Some(key),
        ..WITH_IMPACT
    };
    let r = behind_of(&one, &[path], &options);
    assert_eq!(r.pins.len(), 1);
    assert_eq!(r.pins[0].name, "csi-extra");
    assert_eq!(r.pins[0].bump.as_ref().unwrap().to, "1.1.0");
    assert_eq!(one.posted().len(), 1, "{:?}", one.posted());
    assert_eq!(
        (r.summary.pins, r.summary.behind, r.summary.errors),
        (1, 1, 0)
    );
    // one pack alone: strata reads no profile version
    assert!(r.profiles.is_empty() && r.summary.profiles == 0);
}

#[test]
fn behind_refuses_a_pin_that_no_record_holds_before_any_request() {
    let dir = tempfile::tempdir().unwrap();
    let base = serve_behind();
    let path = dir.path().join("csi.json");
    common::write_snapshot(&path, &profile_pinning(&base), false);
    let connects = std::cell::Cell::new(0);
    for pin in ["csi/nothing", "addon/csi-aws-ebs", "csi-aws-ebs"] {
        let err = ops::behind(
            std::slice::from_ref(&path),
            &ops::BehindOptions {
                pin: Some(pin.to_string()),
                ..WITH_IMPACT
            },
            |_| {
                connects.set(connects.get() + 1);
                Ok(Palette::new(&base))
            },
            "2026-09-26T00:00:00Z",
            &Progress::silent(),
        )
        .unwrap_err();
        assert!(matches!(err, Error::Usage { .. }), "{pin}: {err}");
        assert!(err.to_string().contains(&format!("--pin {pin}")), "{err}");
    }
    assert_eq!(connects.get(), 0, "strata reads the records first");
}

#[test]
fn behind_asks_the_route_of_the_profile_version_that_a_cluster_runs_about_the_bump_of_a_record_of_a_cluster()
 {
    let dir = tempfile::tempdir().unwrap();
    let base = serve_behind();
    let cluster = cluster_pinning(&base);
    let target = snapshot_of(&base, "pack:csi-aws-ebs@1.65.0").unwrap();
    let route = update_validate_packs_route(BUMP_PROFILE_UID);
    // Palette checks the move against each cluster that runs the version, and names them
    let api = base
        .with(cluster_route("c1"), cluster_document_running(&cluster))
        .with(profile_route(BUMP_PROFILE_UID), profile_document_running())
        .with(post(&route), upgrade_refusal(UPGRADE_REFUSAL));
    let cluster_file = dir.path().join("cluster.json");
    common::write_snapshot(&cluster_file, &cluster, false);
    let accepting = api.clone().with(post(&route), serde_json::json!({}));
    let r = behind_of(&api, std::slice::from_ref(&cluster_file), &WITH_IMPACT);

    // one request, to the route of the profile version that holds the pin: the packs of its
    // template as the cluster runs them, and the pack as 1.65.0. The other profile of the cluster
    // is not in the body. The route of the cluster gets none: it does not check the sent tag
    let posted = api.posted();
    assert_eq!(posted.len(), 1, "{posted:?}");
    assert_eq!(posted[0].0, route);
    let moved = serde_json::json!({"name": "csi-aws-ebs", "tag": target.packs[0].tag,
        "uid": target.packs[0].pack_uid, "registryUid": target.packs[0].registry_uid,
        "type": target.packs[0].kind, "layer": "addon", "values": target.packs[0].values,
        "manifests": []});
    assert_eq!(
        posted[0].1,
        serde_json::json!({"cloudType": "", "type": "add-on", "packs": [moved]})
    );
    let text = common::terminal().render_behind(&r).unwrap();
    assert!(
        text.contains(&format!(
            "      palette refuses: {UPGRADE_NOT_SUPPORTED_CODE} on payments\n"
        )),
        "{text}"
    );
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(
        json["pins"][0]["bump"]["paletteValidation"],
        serde_json::json!({"accepts": false, "errors": [{
            "pack": "csi-aws-ebs", "code": UPGRADE_NOT_SUPPORTED_CODE,
            "message": UPGRADE_REFUSAL, "clusters": [BUMP_CLUSTER]}]})
    );

    // a 200 with no error: Palette accepts, and strata reads no document of the profile
    let r = behind_of(&accepting, &[cluster_file], &WITH_IMPACT);
    assert!(
        r.pins[0]
            .bump
            .as_ref()
            .unwrap()
            .palette_validation
            .as_ref()
            .unwrap()
            .accepts()
    );
    assert_eq!(accepting.calls(&profile_route(BUMP_PROFILE_UID)), 0);
}

/// The pack csi-aws-ebs 1.63.1 of the registry of the tests, as a profile and a cluster hold it in
/// their templates.
fn csi_template_pack(api: &MemoryApi) -> Value {
    let pack = &snapshot_of(api, "pack:csi-aws-ebs@1.63.1").unwrap().packs[0];
    serde_json::json!({"name": pack.name, "layer": pack.layer, "type": pack.kind,
        "tag": pack.tag, "version": pack.version, "registryUid": pack.registry_uid,
        "packUid": pack.pack_uid, "values": pack.values})
}

#[test]
fn behind_names_the_same_bump_for_a_pin_in_the_record_of_a_cluster_and_in_the_record_of_its_profile()
 {
    let dir = tempfile::tempdir().unwrap();
    let base = serve_behind();
    let pack = csi_template_pack(&base);
    // the profile version, and the cluster that runs it: both hold the pack as Palette gives it
    let mut profile = common::profile_document(
        BUMP_PROFILE_UID,
        "csi-stack",
        "1.0.0",
        common::template("aws", "add-on", serde_json::json!([pack])),
        None,
    );
    profile["status"]["inUseClusters"] = serde_json::json!([{"uid": "c1", "name": BUMP_CLUSTER}]);
    let cluster_doc = serde_json::json!({
        "metadata": {"name": BUMP_CLUSTER, "uid": "c1"},
        "spec": {"clusterProfileTemplates": [{"uid": BUMP_PROFILE_UID, "name": "csi-stack",
            "profileVersion": "1.0.0", "type": "add-on", "cloudType": "aws",
            "packs": [pack]}]}});
    let refusal = upgrade_refusal(UPGRADE_REFUSAL);
    let api = common::serve_profile(
        base,
        BUMP_PROFILE_UID,
        profile,
        common::no_manifests(),
        common::no_variables(),
    )
    .with(cluster_route("c1"), cluster_doc)
    .with(
        common::cluster_manifests_route("c1"),
        serde_json::json!({"profiles": []}),
    )
    .with(
        common::cluster_variables_route("c1"),
        common::no_variables(),
    )
    .with(
        post(&update_validate_packs_route(BUMP_PROFILE_UID)),
        refusal,
    );

    let profile_record = common::snapshot_of_ref(
        &api,
        &strata::reference::Ref::Uid(BUMP_PROFILE_UID.to_string()),
        "t0",
    )
    .unwrap();
    let cluster_record = snapshot_of(&api, "cluster:c1").unwrap();
    let bump_of = |record: &strata::snapshot::Snapshot, file: &str| {
        let path = dir.path().join(file);
        common::write_snapshot(&path, record, false);
        let report = behind_of(&api, &[path], &WITH_IMPACT);
        assert!(report.pins[0].error.is_none(), "{:?}", report.pins[0].error);
        serde_json::to_value(&report.pins[0].bump).unwrap()
    };
    let of_profile = bump_of(&profile_record, "profile.json");
    let of_cluster = bump_of(&cluster_record, "cluster.json");
    assert_eq!(of_profile["to"], "1.65.0", "{of_profile}");
    // the answer is the same too: Palette refuses the move on the cluster
    assert_eq!(of_cluster, of_profile);
    // both records send one request to the route of the profile version, with one body
    let posted = api.posted();
    assert_eq!(posted.len(), 2, "{posted:?}");
    assert_eq!(posted[0].0, update_validate_packs_route(BUMP_PROFILE_UID));
    assert_eq!(posted[1], posted[0]);
}

#[test]
fn behind_asks_each_record_of_a_pin_and_gathers_the_errors() {
    let dir = tempfile::tempdir().unwrap();
    let base = serve_behind();
    let record = profile_pinning(&base);
    let cluster = cluster_pinning(&base);
    // the cluster runs another version of the profile than the record of the profile pins, so the
    // two records ask the routes of two versions
    let other = "pr2";
    let mut other_doc = cluster_document_running(&cluster);
    other_doc["spec"]["clusterProfileTemplates"][1]["uid"] = Value::from(other);
    let profile_route_packs = post(&update_validate_packs_route(BUMP_PROFILE_UID));
    let cluster_route_packs = post(&update_validate_packs_route(other));
    // a pack record: no cluster runs it, so it is not asked
    let pack = snapshot_of(&base, "pack:csi-aws-ebs@1.63.1").unwrap();
    let records = ["csi.json", "cluster.json", "pack.json"].map(|name| dir.path().join(name));
    for (file, snap) in records.iter().zip([&record, &cluster, &pack]) {
        common::write_snapshot(file, snap, false);
    }
    let api = base
        .with(profile_route(BUMP_PROFILE_UID), profile_document_running())
        .with(cluster_route("c1"), other_doc);

    // the profile refuses and the cluster accepts: the pin holds the refusal
    let both = api
        .clone()
        .with(
            profile_route_packs.clone(),
            upgrade_refusal(UPGRADE_REFUSAL),
        )
        .with(cluster_route_packs.clone(), serde_json::json!({}));
    let r = behind_of(&both, &records, &WITH_IMPACT);
    let routes: Vec<String> = both.posted().into_iter().map(|(route, _)| route).collect();
    assert_eq!(
        routes,
        [
            update_validate_packs_route(BUMP_PROFILE_UID),
            update_validate_packs_route(other)
        ]
    );
    let answer = r.pins[0]
        .bump
        .as_ref()
        .unwrap()
        .palette_validation
        .as_ref()
        .unwrap();
    assert_eq!(answer.errors().len(), 1);

    // both accept: Palette accepts
    let accepting = api
        .clone()
        .with(profile_route_packs.clone(), serde_json::json!({}))
        .with(cluster_route_packs.clone(), serde_json::json!({}));
    let r = behind_of(&accepting, &records, &WITH_IMPACT);
    assert!(
        r.pins[0]
            .bump
            .as_ref()
            .unwrap()
            .palette_validation
            .as_ref()
            .unwrap()
            .accepts()
    );

    // one request fails and the other accepts: strata cannot say that Palette accepts, and says
    // why. The pin stays behind and the exit stays 1
    let failing = api
        .clone()
        .failing(profile_route_packs.clone(), 403, "no access")
        .with(cluster_route_packs.clone(), serde_json::json!({}));
    let r = behind_of(&failing, &records, &WITH_IMPACT);
    let bump = r.pins[0].bump.as_ref().unwrap();
    assert!(bump.palette_validation.is_none());
    assert!(
        bump.palette_validation_error
            .as_deref()
            .is_some_and(|e| e.contains("403")),
        "{:?}",
        bump.palette_validation_error
    );
    assert!(r.pins[0].behind && r.pins[0].error.is_none());
    assert_eq!(r.exit_code(), 1);
    let text = common::terminal().render_behind(&r).unwrap();
    assert!(text.contains("      palette gave no answer: "), "{text}");
    assert!(!text.contains("palette accepts"), "{text}");

    // one request fails and the other refuses: the refusal stands, with the message of the failure
    let refusing = api
        .clone()
        .failing(profile_route_packs, 403, "no access")
        .with(cluster_route_packs, upgrade_refusal(UPGRADE_REFUSAL));
    let r = behind_of(&refusing, &records, &WITH_IMPACT);
    let bump = r.pins[0].bump.as_ref().unwrap();
    assert!(!bump.palette_validation.as_ref().unwrap().accepts());
    assert!(bump.palette_validation_error.is_some());
}

#[test]
fn behind_gives_one_error_for_a_record_of_a_profile_and_a_record_of_a_cluster_that_runs_it() {
    let dir = tempfile::tempdir().unwrap();
    let base = serve_behind();
    let record = profile_pinning(&base);
    let cluster = cluster_pinning(&base);
    let api = base
        .with(profile_route(BUMP_PROFILE_UID), profile_document_running())
        .with(cluster_route("c1"), cluster_document_running(&cluster))
        .with(
            post(&update_validate_packs_route(BUMP_PROFILE_UID)),
            upgrade_refusal(UPGRADE_REFUSAL),
        );
    let records = ["csi.json", "cluster.json"].map(|name| dir.path().join(name));
    for (file, snap) in records.iter().zip([&record, &cluster]) {
        common::write_snapshot(file, snap, false);
    }
    let r = behind_of(&api, &records, &WITH_IMPACT);
    // each record sends its request, and Palette answers both with one error
    assert_eq!(api.posted().len(), 2, "{:?}", api.posted());
    let answer = r.pins[0]
        .bump
        .as_ref()
        .unwrap()
        .palette_validation
        .as_ref()
        .unwrap();
    assert_eq!(answer.errors().len(), 1, "{answer:?}");
    assert_eq!(answer.errors()[0].clusters, [BUMP_CLUSTER]);
}

#[test]
fn behind_gives_the_message_of_palette_when_an_error_names_no_cluster_that_runs_the_profile() {
    let dir = tempfile::tempdir().unwrap();
    let base = serve_behind();
    let record = profile_pinning(&base);
    // Palette names a cluster that the document of the profile does not list
    let message = "Pack 'csi-aws-ebs' cannot move on the clusters: orders";
    let api = base
        .with(
            post(&update_validate_packs_route(BUMP_PROFILE_UID)),
            upgrade_refusal(message),
        )
        .with(profile_route(BUMP_PROFILE_UID), profile_document_running());
    let csi = dir.path().join("csi.json");
    common::write_snapshot(&csi, &record, false);
    let r = behind_of(&api, std::slice::from_ref(&csi), &WITH_IMPACT);
    let text = common::terminal().render_behind(&r).unwrap();
    assert!(
        text.contains(&format!(
            "      palette refuses: {UPGRADE_NOT_SUPPORTED_CODE} csi-aws-ebs: {message}\n"
        )),
        "{text}"
    );

    // the document of the profile is not readable: the refusal stands, with the message
    let unread = api.failing(profile_route(BUMP_PROFILE_UID), 500, "boom");
    let r = behind_of(&unread, &[csi], &WITH_IMPACT);
    let bump = r.pins[0].bump.as_ref().unwrap();
    assert!(!bump.palette_validation.as_ref().unwrap().accepts());
    assert!(bump.palette_validation_error.is_none());
}

#[test]
fn behind_does_not_ask_about_a_record_that_no_cluster_runs() {
    let dir = tempfile::tempdir().unwrap();
    let base = serve_behind();
    // a candidate has no uid, and a pack record is no profile
    let mut candidate = profile_pinning(&base);
    candidate.source.uid = String::new();
    let pack = snapshot_of(&base, "pack:csi-aws-ebs@1.63.1").unwrap();
    // the cluster of the record runs no profile with the pin now
    let cluster = cluster_pinning(&base);
    let mut moved_doc = cluster_document_running(&cluster);
    moved_doc["spec"]["clusterProfileTemplates"][1]["packs"] = serde_json::json!([]);
    // the update that waits for a cluster is not what the cluster runs
    let mut pending = cluster_pinning(&base);
    pending.source.cluster.as_mut().unwrap().pending = true;
    let api = base.with(cluster_route("c1"), moved_doc);
    let records = [
        "candidate.json",
        "pack.json",
        "cluster.json",
        "pending.json",
    ]
    .map(|name| dir.path().join(name));
    for (file, snap) in records.iter().zip([&candidate, &pack, &cluster, &pending]) {
        common::write_snapshot(file, snap, false);
    }
    let r = behind_of(&api, &records, &WITH_IMPACT);
    assert!(api.posted().is_empty(), "{:?}", api.posted());
    let bump = r.pins[0].bump.as_ref().unwrap();
    assert!(bump.palette_validation.is_none() && bump.palette_validation_error.is_none());
    let json = serde_json::to_value(&r).unwrap();
    assert!(json["pins"][0]["bump"].get("paletteValidation").is_none());
    let text = common::terminal().render_behind(&r).unwrap();
    assert!(!text.contains("palette "), "{text}");
}

/// The record of a profile version `uid` that pins csi-aws-ebs 1.63.1, taken in `project`.
fn profile_pinning_in(
    api: &MemoryApi,
    uid: &str,
    project: Option<&str>,
) -> strata::snapshot::Snapshot {
    let mut snap = profile_pinning(api).with_project(project);
    snap.source.uid = uid.to_string();
    snap
}

#[test]
fn behind_does_not_say_that_palette_accepts_when_it_did_not_ask_a_record_of_another_project() {
    let dir = tempfile::tempdir().unwrap();
    let base = serve_behind();
    // dev and prod both record the release, so the pin is in dev: the project of the first record
    let dev = profile_pinning_in(&base, BUMP_PROFILE_UID, Some("dev"));
    let prod = profile_pinning_in(&base, "pr2", Some("prod"));
    let api = base.clone().with(
        post(&update_validate_packs_route(BUMP_PROFILE_UID)),
        serde_json::json!({"packs": {"results": []}}),
    );
    let files = ["dev.json", "prod.json"].map(|name| dir.path().join(name));
    common::write_snapshot(&files[0], &dev, false);
    common::write_snapshot(&files[1], &prod, false);
    let r = behind_of(&api, &files, &WITH_IMPACT);

    // the request for the record of dev accepts, and strata sent none for the record of prod
    let posted = api.posted();
    assert_eq!(posted.len(), 1, "{posted:?}");
    assert_eq!(posted[0].0, update_validate_packs_route(BUMP_PROFILE_UID));
    let bump = r.pins[0].bump.as_ref().unwrap();
    assert!(
        bump.palette_validation.is_none(),
        "Palette did not check the record of prod"
    );
    assert_eq!(
        bump.palette_validation_error.as_deref(),
        Some("1 record of project prod was not asked")
    );
    let text = common::terminal().render_behind(&r).unwrap();
    assert!(
        text.contains("      palette gave no answer: 1 record of project prod was not asked\n"),
        "{text}"
    );
    assert!(!text.contains("palette accepts"), "{text}");
    let json = serde_json::to_value(&r).unwrap();
    assert!(json["pins"][0]["bump"].get("paletteValidation").is_none());
    assert_eq!(
        r.exit_code(),
        1,
        "the pin stays behind, and the exit stays 1"
    );

    // the count is for each project, in the order of the project, and a record of the tenant
    // scope counts too. A pack record in another project gets no request, so it adds nothing
    let prod_cluster = cluster_pinning(&base).with_project(Some("prod"));
    let lab = profile_pinning_in(&base, "pr3", None);
    let pack = snapshot_of(&base, "pack:csi-aws-ebs@1.63.1")
        .unwrap()
        .with_project(Some("stage"));
    let files = [
        "dev.json",
        "prod.json",
        "cluster.json",
        "lab.json",
        "pack.json",
    ]
    .map(|name| dir.path().join(name));
    for (file, snap) in files.iter().zip([&dev, &prod, &prod_cluster, &lab, &pack]) {
        common::write_snapshot(file, snap, false);
    }
    let r = behind_of(&api, &files, &WITH_IMPACT);
    assert_eq!(
        r.pins[0]
            .bump
            .as_ref()
            .unwrap()
            .palette_validation_error
            .as_deref(),
        Some(
            "1 record of the tenant scope was not asked; 2 records of project prod were not asked"
        )
    );

    // only a pack record is in another project: Palette checked each record that it can check
    let files = ["dev.json", "pack.json"].map(|name| dir.path().join(name));
    let r = behind_of(&api, &files, &WITH_IMPACT);
    let bump = r.pins[0].bump.as_ref().unwrap();
    assert!(bump.palette_validation.as_ref().unwrap().accepts());
    assert!(bump.palette_validation_error.is_none());
}

#[test]
fn behind_keeps_the_refusal_of_palette_and_says_that_it_did_not_ask_a_record_of_another_project() {
    let dir = tempfile::tempdir().unwrap();
    let base = serve_behind();
    let dev = profile_pinning_in(&base, BUMP_PROFILE_UID, Some("dev"));
    let prod = profile_pinning_in(&base, "pr2", Some("prod"));
    let api = base
        .with(
            post(&update_validate_packs_route(BUMP_PROFILE_UID)),
            upgrade_refusal(UPGRADE_REFUSAL),
        )
        .with(profile_route(BUMP_PROFILE_UID), profile_document_running());
    let files = ["dev.json", "prod.json"].map(|name| dir.path().join(name));
    common::write_snapshot(&files[0], &dev, false);
    common::write_snapshot(&files[1], &prod, false);
    let r = behind_of(&api, &files, &WITH_IMPACT);

    // the refusal stands: it is an error that Palette gave for a record that strata asked
    let bump = r.pins[0].bump.as_ref().unwrap();
    let answer = bump.palette_validation.as_ref().unwrap();
    assert!(!answer.accepts());
    assert_eq!(answer.errors()[0].clusters, [BUMP_CLUSTER]);
    assert_eq!(
        bump.palette_validation_error.as_deref(),
        Some("1 record of project prod was not asked")
    );
    let text = common::terminal().render_behind(&r).unwrap();
    let refusal = format!("      palette refuses: {UPGRADE_NOT_SUPPORTED_CODE} on payments\n");
    let missing = "      palette gave no answer: 1 record of project prod was not asked\n";
    assert!(text.contains(&format!("{refusal}{missing}")), "{text}");
}
#[test]
fn a_bump_of_a_report_written_before_the_ask_reads_with_no_answer_of_palette() {
    let old = serde_json::json!({"to": "1.65.0", "impact": "cluster-wide", "flags": ["IDENTITY"]});
    let bump: ops::Bump = serde_json::from_value(old).unwrap();
    assert!(bump.palette_validation.is_none() && bump.palette_validation_error.is_none());
}

#[test]
fn a_bump_gives_a_renamed_tag_as_the_flag_identity_and_not_as_the_words_of_the_rename() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let a = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let retagged = with_os_pack(dir.path(), "LTS__22.04", |p| {
        p["tag"] = Value::from("LTS__22.04")
    });
    let d = ops::compare(&a, &retagged, "t1");
    let text = common::terminal().render(&d, &a, &retagged, false).unwrap();
    assert!(text.contains("tag 22.04 -> LTS__22.04"), "{text}");

    // the text of the diff names the two tags in place of IDENTITY; the bump keeps the flag
    let bump = ops::Bump::of("22.04", &d, "ubuntu-aws").unwrap();
    assert_eq!(bump.flags, [ChangeFlag::Identity]);
    assert_eq!(
        serde_json::to_value(&bump).unwrap()["flags"],
        serde_json::json!(["IDENTITY"])
    );
    assert!(ops::Bump::of("22.04", &d, "no-such-pack").is_none());
}

#[test]
fn each_report_of_behind_says_since_when_the_newest_version_is_listed_and_what_arrived() {
    let dir = tempfile::tempdir().unwrap();
    let api = serve_behind();
    let path = dir.path().join("dev.json");
    let old = snapshot_of(&api, "pack:csi-aws-ebs@1.63.1").unwrap();
    common::write_snapshot(&path, &old, false);
    let r = behind_of(
        &api,
        &[path],
        &ops::BehindOptions {
            impact: false,
            since: None,
            pin: None,
            pre_release: false,
        },
    );
    // the newest version with the time of its listing, and the versions since a report
    let mut doc = serde_json::to_value(&r).unwrap();
    doc["pins"][0]["newer"][0]["listed"] = Value::from("2026-09-21T08:00:00.000Z");
    doc["pins"][0]["arrived"] = serde_json::json!(["1.65.0"]);
    let r: ops::BehindReport = serde_json::from_value(doc).unwrap();
    let listed = "1.65.0 listed since 2026-09-21T08:00:00.000Z";
    let arrived = "arrived: 1.65.0";
    let text = common::terminal().render_behind(&r).unwrap();
    assert!(
        text.contains(&format!("      {listed}\n")) && text.contains(&format!("      {arrived}\n")),
        "{text}"
    );
    let md = common::markdown().render_behind(&r).unwrap();
    let row = md.lines().find(|l| l.contains("csi-aws-ebs")).unwrap();
    assert!(
        row.contains(&format!("; {listed}")) && row.contains(&format!("; {arrived}")),
        "{md}"
    );
}

#[test]
fn behind_since_takes_a_report_of_its_id_and_refuses_another_version_or_another_document() {
    // the rule of the records (#321): a strata-behind id of another version is unsupported-schema,
    // which names both ids. A file of another document, or of none, is not a report.
    let dir = tempfile::tempdir().unwrap();
    let report = dir.path().join("report.json");
    fs::write(
        &report,
        r#"{"schema":"strata-behind/1","tool":{"name":"strata","version":"0.0.0"},
            "generatedAt":"2026-09-26T00:00:00Z","records":[],"pins":[],
            "summary":{"pins":0,"behind":0,"floating":0,"errors":0,"arrived":0}}"#,
    )
    .unwrap();
    let previous = ops::BehindReport::load(&report).unwrap();
    assert_eq!(previous.generated_at, "2026-09-26T00:00:00Z");

    let newer = dir.path().join("newer.json");
    fs::write(&newer, r#"{"schema":"strata-behind/2","pins":[]}"#).unwrap();
    let err = ops::BehindReport::load(&newer).unwrap_err();
    assert!(
        matches!(&err, Error::UnsupportedSchema { found, supported, .. }
            if found.as_str() == "strata-behind/2" && *supported == ops::BEHIND_SCHEMA),
        "{err}"
    );
    assert_eq!(err.kind(), "unsupported-schema");

    // a record is another document, so not a report: the message says what to give
    common::make_fixtures(dir.path());
    let record = dir.path().join("record.json");
    let snap = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    common::write_snapshot(&record, &snap, true);
    let err = ops::BehindReport::load(&record).unwrap_err();
    assert_eq!(err.kind(), "usage");
    assert!(
        err.to_string()
            .contains("is not a strata-behind/1 document: give the JSON of a previous run"),
        "{err}"
    );

    // no schema id, and not JSON
    let no_id = dir.path().join("no-id.json");
    fs::write(&no_id, "{}").unwrap();
    assert_eq!(ops::BehindReport::load(&no_id).unwrap_err().kind(), "usage");
    let text = dir.path().join("text.json");
    fs::write(&text, "not json").unwrap();
    assert_eq!(ops::BehindReport::load(&text).unwrap_err().kind(), "usage");

    // the id of this strata, with fields that do not match it
    let bad = dir.path().join("bad.json");
    fs::write(&bad, r#"{"schema":"strata-behind/1"}"#).unwrap();
    let err = ops::BehindReport::load(&bad).unwrap_err();
    assert_eq!(err.kind(), "snapshot-schema");
    assert!(
        err.to_string().contains("does not match strata-behind/1"),
        "{err}"
    );
}

#[test]
fn behind_does_not_offer_a_version_with_the_mark_and_says_so_of_a_pinned_one() {
    // 1.66.0 carries the mark. Palette refuses a profile that holds it, so it is not a version to
    // move to. A pin of 1.66.0 itself says that the pinned version is disabled
    let dir = tempfile::tempdir().unwrap();
    let api = serve_registry(
        vec![
            marked(csi_summary_with("1.66.0", None, &["aws"])),
            csi_summary_with("1.67.0", None, &["aws"]),
        ],
        &[(
            "reg-public",
            serde_json::json!({"layer": "csi", "tags": [
                {"tag": "1.63.1", "packUid": "pk-63"},
                {"tag": "1.65.0", "packUid": "pk-65"},
                {"tag": "1.66.0", "packUid": "pk-1.66.0"},
                {"tag": "1.67.0", "packUid": "pk-1.67.0"},
            ]}),
        )],
    );
    let write = |name: &str, snap: &Snapshot| {
        let path = dir.path().join(name);
        common::write_snapshot(&path, snap, false);
        path
    };
    let records = vec![
        write(
            "dev.json",
            &snapshot_of(&api, "pack:csi-aws-ebs@1.63.1").unwrap(),
        ),
        write(
            "marked.json",
            &snapshot_of(&api, "pack:csi-aws-ebs@1.66.0").unwrap(),
        ),
    ];
    let r = behind_of(&api, &records, &ops::BehindOptions::default());
    let pin = &r.pins[0];
    assert_eq!(pin.version, "1.63.1");
    assert_eq!(
        pin.newer
            .iter()
            .map(|n| n.version.as_str())
            .collect::<Vec<_>>(),
        ["1.67.0", "1.65.0"],
        "a marked version is not one to move to"
    );
    let marked_pin = &r.pins[1];
    assert_eq!(marked_pin.version, "1.66.0");
    assert_eq!(
        marked_pin.pinned_state.as_ref().map(Open::as_str),
        Some("disabled")
    );
    assert_eq!(
        marked_pin
            .newer
            .iter()
            .map(|n| n.version.as_str())
            .collect::<Vec<_>>(),
        ["1.67.0"]
    );
    let text = common::terminal().render_behind(&r).unwrap();
    assert!(
        text.contains("1.66.0") && text.contains("; the pinned version is disabled"),
        "{text}"
    );
}

#[test]
fn a_floating_pin_is_behind_the_version_its_tag_names_and_a_pin_that_cannot_be_read_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let api = serve_behind();
    let mut floating = snapshot_of(&api, "pack:csi-aws-ebs@1.63.1").unwrap();
    floating.packs[0].tag = "1.x".into();
    let mut narrow = snapshot_of(&api, "pack:csi-aws-ebs@1.63.1").unwrap();
    narrow.packs[0].tag = "1.63.x".into();
    let mut moved = snapshot_of(&api, "pack:csi-aws-ebs@1.65.0").unwrap();
    moved.packs[0].tag = "1.x".into();
    let mut lost = snapshot_of(&api, "pack:csi-aws-ebs@1.63.1").unwrap();
    lost.packs[0].registry_uid = "reg-gone".into();
    let mut records = Vec::new();
    for (name, snap) in [
        ("floating.json", &floating),
        ("narrow.json", &narrow),
        ("moved.json", &moved),
        ("lost.json", &lost),
    ] {
        let path = dir.path().join(name);
        common::write_snapshot(&path, snap, false);
        records.push(path);
    }
    let r = behind_of(&api, &records, &ops::BehindOptions::default());
    // 1.x names 1.65.0 now, and the record pins 1.63.1: a profile with values of its own stays
    // there until a person applies the update, so the pin is behind
    let float = r
        .pins
        .iter()
        .find(|p| p.tag == "1.x" && p.version == "1.63.1")
        .unwrap();
    assert!(float.floating && float.behind, "{float:?}");
    assert_eq!(float.resolved.as_deref(), Some("1.65.0"));
    let newer: Vec<&str> = float.newer.iter().map(|n| n.version.as_str()).collect();
    assert_eq!(newer, ["1.65.0", "1.64.0"]);
    // a pin of the version that the tag names now is current
    let at = r
        .pins
        .iter()
        .find(|p| p.tag == "1.x" && p.version == "1.65.0")
        .unwrap();
    assert!(at.floating && !at.behind && at.newer.is_empty(), "{at:?}");
    // 1.63.x names 1.63.1, and cannot reach 1.64.0 or 1.65.0: it is behind them
    let narrow = r.pins.iter().find(|p| p.tag == "1.63.x").unwrap();
    assert!(narrow.floating && narrow.behind, "{narrow:?}");
    assert_eq!(narrow.resolved.as_deref(), Some("1.63.1"));
    assert_eq!(narrow.newer[0].version, "1.65.0");
    let gone = r
        .pins
        .iter()
        .find(|p| p.registry_uid == "reg-gone")
        .unwrap();
    assert!(gone.error.is_some());
    assert_eq!((r.summary.floating, r.summary.errors), (3, 1));
    // the terminal names the version that the tag names now beside each one
    let text = common::terminal().render_behind(&r).unwrap();
    assert!(
        text.contains(
            "1.63.1  Public Repo  behind: 1.65.0, 1.64.0 (deprecated) (floating tag 1.x, now 1.65.0)"
        ),
        "{text}"
    );
    assert!(
        text.contains("1.65.0  Public Repo  current (floating tag 1.x, now 1.65.0)"),
        "{text}"
    );
    assert_eq!(r.exit_code(), 2, "a pin that strata could not check");
    assert!(ops::is_floating("LTS__24.x") && ops::is_floating("1.30.x"));
    assert!(!ops::is_floating("1.30.6") && !ops::is_floating("LTS__24.04"));
}

// ---- the fleet --------------------------------------------------------------------------------

/// A cluster row, as the search and the listing give one.
fn fleet_row(uid: &str, name: &str, project: &str, labels: Value) -> Value {
    serde_json::json!({"metadata": {"uid": uid, "name": name, "labels": labels,
        "annotations": {"projectUid": project}},
        "spec": {"cloudType": "aws"}, "status": {"state": "Running"}})
}

/// A tenant whose search takes tags: the search answers `found`, the listing holds `all`, and
/// the saved filter `prod` selects env=prod.
fn serve_fleet(api: MemoryApi, found: Vec<Value>, all: Vec<Value>) -> MemoryApi {
    api.with(
        CLUSTER_SEARCH_SCHEMA,
        serde_json::json!({"schema": {"properties": [
            {"name": "clusterName", "type": "string"}, {"name": "tags", "type": "string"}]}}),
    )
    .with(
        post(CLUSTER_SEARCH),
        serde_json::json!({"items": found, "listmeta": {"continue": ""}}),
    )
    .with(CLUSTER_LISTING, Value::Array(all))
    .with(
        "/v1/filters/metadata?filterType=tag",
        serde_json::json!({"items": [{"metadata": {"name": "prod", "uid": "f1"}}]}),
    )
    .with(
        "/v1/filters/tag/f1",
        serde_json::json!({"metadata": {"name": "prod", "uid": "f1"},
            "spec": {"filterGroup": {"conjunction": "and",
                "filters": [{"key": "env", "values": ["prod"], "negation": false},
                            {"key": "legacy", "values": [], "negation": true}]}}}),
    )
    .with(
        PROJECT_LISTING,
        serde_json::json!({"items": [{"metadata": {"name": "dev", "uid": "p1"}},
            {"metadata": {"name": "prod", "uid": "p2"}}]}),
    )
}

#[test]
fn list_clusters_with_a_tag_searches_palette_and_names_the_project_of_each() {
    let payments = fleet_row(
        "c-pay",
        "payments",
        "p2",
        serde_json::json!({"env": "prod", "team": "pay"}),
    );
    let search = fleet_row(
        "c-search",
        "search",
        "p1",
        serde_json::json!({"env": "dev", "prod": "spectro__tag"}),
    );
    let api = serve_fleet(
        MemoryApi::new(),
        vec![payments.clone()],
        vec![payments, search],
    );
    let l = clusters_listing(
        &api,
        ops::ClusterQuery {
            tags: vec!["env=prod".into()],
            filter: None,
        },
    );
    assert_eq!(l.resolved, Some(ops::Resolved::Search));
    assert_eq!(l.project_uid, "", "a query is at tenant scope");
    let c = &l.clusters.as_ref().unwrap()[0];
    assert_eq!(
        (
            c.name.as_str(),
            c.project_uid.as_str(),
            c.project_name.as_str()
        ),
        ("payments", "p2", "prod")
    );
    assert_eq!(c.labels["env"], "prod");
    // the body is a tags condition that the search reads as key:value
    let posted = api.posted();
    let (path, body) = posted.last().unwrap();
    assert_eq!(path, CLUSTER_SEARCH);
    let item = &body["filter"]["filterGroups"][0]["filters"][0];
    assert_eq!(item["property"], "tags");
    assert_eq!(item["condition"]["string"]["operator"], "eq");
    assert_eq!(
        item["condition"]["string"]["match"]["values"],
        serde_json::json!(["env:prod"])
    );

    let text = common::terminal().render_list(&l).unwrap();
    assert!(text.starts_with("tenant scope\n"), "{text}");
    assert!(
        text.contains("the clusters with env=prod, from the search of Palette, at tenant scope"),
        "{text}"
    );
    assert!(
        text.contains("project prod (uid p2)") && text.contains("env=prod, team=pay"),
        "{text}"
    );
}

#[test]
fn list_clusters_with_a_tag_names_a_virtual_cluster_that_carries_the_label() {
    // The search omits each virtual cluster unless a condition asks for it. So a query
    // searches a second time with environment eq nested added to its groups (#116).
    let payments = fleet_row(
        "c-pay",
        "payments",
        "p2",
        serde_json::json!({"env": "prod"}),
    );
    let mut vc = virtual_item(
        "c-v",
        "vc-prod",
        "p2",
        ("g1", "prod-group"),
        ("c-pay", "payments"),
    );
    vc["metadata"]["labels"] = serde_json::json!({"env": "prod"});
    let tags = vec![serde_json::json!({"conjunction": "and",
        "filters": [search_eq("tags", "env:prod")]})];
    let api = serve_fleet(
        MemoryApi::new(),
        vec![payments.clone()],
        vec![payments.clone()],
    )
    .with(
        CLUSTER_SEARCH_SCHEMA,
        search_schema(&["clusterName", "tags", "environment"]),
    )
    .with_post(
        CLUSTER_SEARCH,
        &virtual_search(tags),
        serde_json::json!({"items": [vc], "listmeta": {"continue": ""}}),
    );
    let query = ops::ClusterQuery {
        tags: vec!["env=prod".into()],
        filter: None,
    };
    let l = clusters_listing(&api, query.clone());
    assert_eq!(l.resolved, Some(ops::Resolved::Search));
    assert_eq!(l.includes_virtual, Some(true));
    let clusters = l.clusters.as_ref().unwrap();
    assert_eq!(
        clusters.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        ["payments", "vc-prod"]
    );
    let vc = &clusters[1];
    assert_eq!(vc.labels["env"], "prod");
    assert_eq!(
        vc.virtual_cluster
            .as_ref()
            .and_then(|v| v.cluster_group.as_ref())
            .map(|g| g.name.as_str()),
        Some("prod-group")
    );
    assert!(clusters[0].virtual_cluster.is_none());
    // two searches: the query, and the query with the condition on the environment
    let posted = api.posted();
    assert_eq!(posted.len(), 2, "{posted:?}");
    let groups = |i: usize| posted[i].1["filter"]["filterGroups"].clone();
    assert_eq!(groups(0).as_array().map(Vec::len), Some(1));
    assert_eq!(groups(1)[0], groups(0)[0], "the same query");
    assert_eq!(groups(1)[1]["filters"][0]["property"], "environment");
    assert_eq!(
        groups(1)[1]["filters"][0]["condition"]["string"]["match"]["values"],
        serde_json::json!(["nested"])
    );

    let text = common::terminal().render_list(&l).unwrap();
    assert!(
        text.contains("nested  project prod (uid p2)  env=prod  virtual (group prod-group)\n"),
        "{text}"
    );
    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_list(&l)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["includesVirtual"], true);
    assert_eq!(json["clusters"][1]["virtual"]["clusterGroup"]["uid"], "g1");

    // the search takes tags and no environment: one search, and the list says so
    let older = serve_fleet(MemoryApi::new(), vec![payments.clone()], vec![payments]);
    let l = clusters_listing(&older, query);
    assert_eq!(l.includes_virtual, Some(false));
    assert_eq!(older.posted().len(), 1);
    let text = common::terminal().render_list(&l).unwrap();
    assert!(
        text.contains("the list holds no virtual cluster: Palette has no search"),
        "{text}"
    );
}

#[test]
fn list_clusters_gives_one_row_to_a_cluster_that_both_searches_name() {
    // A query sends two searches, and each answer holds the virtual cluster here. The list
    // joins the two answers, and gives the cluster one row (#317).
    let payments = fleet_row(
        "c-pay",
        "payments",
        "p2",
        serde_json::json!({"env": "prod"}),
    );
    let mut vc = virtual_item(
        "c-v",
        "vc-prod",
        "p2",
        ("g1", "prod-group"),
        ("c-pay", "payments"),
    );
    vc["metadata"]["labels"] = serde_json::json!({"env": "prod"});
    let tags = vec![serde_json::json!({"conjunction": "and",
        "filters": [search_eq("tags", "env:prod")]})];
    let api = serve_fleet(
        MemoryApi::new(),
        vec![payments.clone(), vc.clone()],
        vec![payments],
    )
    .with(
        CLUSTER_SEARCH_SCHEMA,
        search_schema(&["clusterName", "tags", "environment"]),
    )
    .with_post(
        CLUSTER_SEARCH,
        &virtual_search(tags),
        serde_json::json!({"items": [vc], "listmeta": {"continue": ""}}),
    );
    let l = clusters_listing(
        &api,
        ops::ClusterQuery {
            tags: vec!["env=prod".into()],
            filter: None,
        },
    );
    assert_eq!(api.posted().len(), 2, "two searches");
    assert_eq!(
        l.clusters
            .as_ref()
            .unwrap()
            .iter()
            .map(|c| c.reference.as_str())
            .collect::<Vec<_>>(),
        ["cluster:c-pay", "cluster:c-v"],
        "one row for each cluster"
    );
}

#[test]
fn a_saved_tag_filter_is_translated_into_a_search_and_a_tenant_without_a_search_is_matched_by_strata()
 {
    let payments = fleet_row(
        "c-pay",
        "payments",
        "p2",
        serde_json::json!({"env": "prod"}),
    );
    let old = fleet_row(
        "c-old",
        "old",
        "p2",
        serde_json::json!({"env": "prod", "legacy": "spectro__tag"}),
    );
    let search = fleet_row(
        "c-search",
        "search",
        "p1",
        serde_json::json!({"env": "dev"}),
    );
    let api = serve_fleet(
        MemoryApi::new(),
        vec![payments.clone()],
        vec![payments.clone(), old.clone(), search.clone()],
    );
    let l = clusters_listing(
        &api,
        ops::ClusterQuery {
            tags: vec![],
            filter: Some("prod".into()),
        },
    );
    let posted = api.posted();
    let filters = &posted.last().unwrap().1["filter"]["filterGroups"][0];
    assert_eq!(filters["conjunction"], "and");
    assert_eq!(
        filters["filters"][1]["condition"]["string"]["match"]["values"],
        serde_json::json!(["legacy"]),
        "a key with no value: the label must be there"
    );
    assert_eq!(
        filters["filters"][1]["condition"]["string"]["negation"],
        true
    );
    assert_eq!(
        l.query_filter.as_ref().unwrap().expression(),
        "env=prod and not legacy"
    );

    // no search: strata matches the labels of the listing itself, with the same rules
    let bare = MemoryApi::new()
        .with(CLUSTER_LISTING, Value::Array(vec![payments, old, search]))
        .with(
            "/v1/filters/metadata?filterType=tag",
            serde_json::json!({"items": [{"metadata": {"name": "prod", "uid": "f1"}}]}),
        )
        .with(
            "/v1/filters/tag/f1",
            serde_json::json!({"spec": {"filterGroup": {"conjunction": "and",
                "filters": [{"key": "env", "values": ["prod"]},
                            {"key": "legacy", "values": [], "negation": true}]}}}),
        );
    let l = clusters_listing(
        &bare,
        ops::ClusterQuery {
            tags: vec![],
            filter: Some("prod".into()),
        },
    );
    assert_eq!(l.resolved, Some(ops::Resolved::Listing));
    assert_eq!(
        l.clusters
            .unwrap()
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["payments"],
        "env=prod, and not legacy"
    );
    let err = listing(
        &bare,
        &ListOptions {
            kinds: vec![ListKind::Clusters],
            query: ops::ClusterQuery {
                tags: vec![],
                filter: Some("nope".into()),
            },
            ..ListOptions::default()
        },
    )
    .unwrap_err();
    assert_eq!(err.kind(), "filter-not-found");

    // list filters names each filter with what it selects
    let l = listing(
        &api,
        &ListOptions {
            kinds: vec![ListKind::Filters],
            ..ListOptions::default()
        },
    )
    .unwrap();
    let text = common::terminal().render_list(&l).unwrap();
    assert!(
        text.contains("filter:prod  f1  env=prod and not legacy"),
        "{text}"
    );
}

/// `snapshot tag:env=prod --dir fleet --prune` over two clusters: `lab-cluster`, which strata
/// records, and `gone`, which Palette cannot serve. The directory holds a record of a cluster
/// that left the set, and a file that is not a record. Gives the report, the directory, and the
/// record of `lab-cluster`.
fn snapshot_prod_set(dir: &Path) -> (ops::SetReport, std::path::PathBuf, Snapshot) {
    common::make_fixtures(dir);
    let lab = fleet_row(
        CLUSTER_UID,
        "lab-cluster",
        "",
        serde_json::json!({"env": "prod"}),
    );
    let gone = fleet_row("c-gone", "gone", "", serde_json::json!({"env": "prod"}));
    let mut doc = read_json(&dir.join("cluster-a").join("cluster.json"));
    doc["metadata"]["labels"] = serde_json::json!({"env": "prod"});
    let api = serve_fleet(
        serve_cluster(MemoryApi::new(), dir, "cluster-a"),
        vec![lab, gone],
        vec![],
    )
    .with(cluster_route(CLUSTER_UID), doc);
    let out = dir.join("fleet");
    fs::create_dir_all(&out).unwrap();
    // a record of a cluster that left the set, and a file that is not a record
    let old = snapshot_cluster(&serve_cluster(MemoryApi::new(), dir, "cluster-a"), None).unwrap();
    common::write_snapshot(&out.join("left.json"), &old, false);
    fs::write(out.join("notes.json"), "{\"a\": 1}").unwrap();
    let report = ops::snapshot_set(
        &ops::ClusterQuery {
            tags: vec!["env=prod".into()],
            filter: None,
        },
        &out,
        true,
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    (report, out, old)
}

#[test]
fn snapshot_of_a_set_writes_a_record_for_each_cluster_and_prune_removes_the_ones_that_left() {
    let dir = tempfile::tempdir().unwrap();
    let (report, out, old) = snapshot_prod_set(dir.path());
    assert_eq!(report.records.len(), 2);
    let lab = &report.records[1];
    assert_eq!(lab.path, out.join("lab-cluster.json"));
    assert_eq!(lab.root.as_deref(), Some(old.root.as_str()));
    let written = Snapshot::load(&lab.path).unwrap();
    assert_eq!(
        written.source.cluster.unwrap().labels["env"],
        "prod",
        "the record carries the labels"
    );
    let gone = &report.records[0];
    assert!(
        gone.error.is_some(),
        "a cluster that cannot be read is an error of its row"
    );
    assert_eq!(report.exit_code(), 2);
    assert_eq!(report.removed, [out.join("left.json")]);
    assert!(
        out.join("notes.json").exists(),
        "a file that is not a record stays"
    );
}

#[test]
fn snapshot_of_a_set_is_a_strata_set_document_with_each_cluster_its_root_or_error_and_each_removed_file()
 {
    let dir = tempfile::tempdir().unwrap();
    let (report, out, old) = snapshot_prod_set(dir.path());
    let doc = serde_json::to_value(&report).unwrap();
    assert_eq!(doc["schema"], ops::SET_SCHEMA);
    assert_eq!(doc["tool"]["name"], strata::TOOL_NAME);
    assert_eq!(doc["generatedAt"], "t1");
    assert_eq!(doc["query"], serde_json::json!({"tags": ["env=prod"]}));
    assert_eq!(doc["resolved"], "search");
    assert_eq!(
        doc["summary"],
        serde_json::json!({"clusters": 2, "recorded": 1, "failed": 1, "removed": 1})
    );
    let lab = &doc["records"][1];
    assert_eq!(
        (&lab["cluster"]["name"], &lab["cluster"]["uid"]),
        (&Value::from("lab-cluster"), &Value::from(CLUSTER_UID))
    );
    assert_eq!(
        lab["path"],
        out.join("lab-cluster.json").display().to_string()
    );
    assert_eq!(lab["root"], old.root.as_str());
    assert!(lab.get("error").is_none(), "{lab}");
    let gone = &doc["records"][0];
    assert_eq!(gone["cluster"]["name"], "gone");
    assert!(gone.get("root").is_none(), "{gone}");
    assert_eq!(gone["error"]["kind"], "cluster-not-found", "{gone}");
    assert_eq!(
        doc["removed"],
        serde_json::json!([out.join("left.json").display().to_string()])
    );
    let back: ops::SetReport = serde_json::from_value(doc).unwrap();
    assert_eq!(back.exit_code(), 2);

    // the JSON format writes the document, and the terminal format keeps the lines
    let json = render::renderer(Format::Json, Paint::off(), true)
        .render_set(&report)
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&json).unwrap()["schema"],
        ops::SET_SCHEMA
    );
    let text = common::terminal().render_set(&report).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3, "{text}");
    assert!(
        lines[0].starts_with("could not record cluster gone (uid c-gone): "),
        "{text}"
    );
    assert_eq!(
        lines[1],
        format!(
            "wrote {}  cluster lab-cluster (uid {CLUSTER_UID})  root {}",
            out.join("lab-cluster.json").display(),
            &old.root[..12]
        )
    );
    assert_eq!(
        lines[2],
        format!("removed {}", out.join("left.json").display())
    );
    assert_eq!(
        render::set_counts_line(&report),
        "1 of 2 clusters recorded, 1 removed; found by the search of Palette"
    );
    let md = common::markdown().render_set(&report).unwrap();
    assert!(
        md.starts_with("### snapshot: 1 of 2 clusters recorded, 1 removed;"),
        "{md}"
    );
    assert!(md.contains("| gone | `c-gone` |"), "{md}");
}

// ---- what happened to the object of a record --------------------------------------------------

/// The cluster of fixture `cluster-a`, whose document says `state`, with `extra` merged into its
/// metadata.
fn cluster_in_state(dir: &Path, state: &str, extra: Value) -> MemoryApi {
    let mut doc = read_json(&dir.join("cluster-a/cluster.json"));
    doc["status"]["state"] = Value::from(state);
    for (k, v) in extra.as_object().unwrap() {
        doc["metadata"][k] = v.clone();
    }
    serve_cluster(MemoryApi::new(), dir, "cluster-a").with(cluster_route(CLUSTER_UID), doc)
}

/// The verdict line of a verify, as the terminal prints it.
fn verdict_of(v: &ops::Verification) -> String {
    common::terminal().verdict(&v.diff).unwrap()
}

#[test]
fn verify_reports_a_deleted_cluster_as_gone_with_the_time_of_the_delete() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-a"),
        None,
    )
    .unwrap();
    // Palette keeps the document of a deleted cluster, with its spec and the state Deleted
    let deleted = cluster_in_state(
        dir.path(),
        "Deleted",
        serde_json::json!({"isDeleted": true, "deletionTimestamp": "2026-09-30T10:00:00Z"}),
    );
    let v = ops::verify(&recorded, &Palette::new(deleted), "t1", &Progress::silent()).unwrap();
    assert!(!v.unchanged);
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.status, VerifyStatus::Drifted);
    assert!(ver.gone);
    assert_eq!(ver.deleted_at.as_deref(), Some("2026-09-30T10:00:00Z"));
    assert_eq!(ver.state.as_deref(), Some("Deleted"));
    assert!(
        v.diff
            .packs
            .iter()
            .all(|p| p.status == strata::diff::Status::Removed),
        "each pack of the record is removed"
    );
    assert_eq!(
        verdict_of(&v),
        "DRIFT: cluster lab-cluster no longer exists: deleted 2026-09-30T10:00:00Z"
    );
    let doc = serde_json::to_value(ver).unwrap();
    assert_eq!(
        (&doc["gone"], &doc["deletedAt"], &doc["state"]),
        (
            &Value::from(true),
            &Value::from("2026-09-30T10:00:00Z"),
            &Value::from("Deleted")
        )
    );

    // metadata.isDeleted alone marks it gone too
    let marked = cluster_in_state(
        dir.path(),
        "Running",
        serde_json::json!({"isDeleted": true}),
    );
    let v = ops::verify(&recorded, &Palette::new(marked), "t1", &Progress::silent()).unwrap();
    assert!(v.diff.verify.as_ref().unwrap().gone);

    // over several records, a gone cluster counts as drifted, and its line says so
    let deleted = cluster_in_state(
        dir.path(),
        "Deleted",
        serde_json::json!({"deletionTimestamp": "2026-09-30T10:00:00Z"}),
    );
    let s = common::verify_summary(&recorded, &deleted);
    assert_eq!((s.summary.drifted, s.exit_code()), (1, 1));
    let text = common::terminal().render_verify_summary(&s).unwrap();
    assert!(
        text.contains("cluster lab-cluster no longer exists: deleted 2026-09-30T10:00:00Z"),
        "{text}"
    );
    assert!(!text.contains("the state is"), "{text}");
}

#[test]
fn verify_ends_the_verdict_of_a_cluster_record_with_a_state_other_than_running() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-a"),
        None,
    )
    .unwrap();
    let running = cluster_in_state(dir.path(), "Running", serde_json::json!({}));
    let v = ops::verify(&recorded, &Palette::new(running), "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged);
    assert_eq!(
        v.diff.verify.as_ref().unwrap().state.as_deref(),
        Some("Running")
    );
    assert_eq!(
        verdict_of(&v),
        "OK: cluster lab-cluster running lab-profile@1.2.0 is unchanged since t0"
    );

    // the cluster goes: the copy is the same, and the verdict says the state
    let deleting = cluster_in_state(dir.path(), "Deleting", serde_json::json!({}));
    let v = ops::verify(
        &recorded,
        &Palette::new(deleting),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(v.unchanged, "a state alone is not a drift");
    assert_eq!(
        verdict_of(&v),
        "OK: cluster lab-cluster running lab-profile@1.2.0 is unchanged since t0; the cluster is Deleting"
    );
    let md = common::markdown().verdict(&v.diff).unwrap();
    assert!(md.ends_with("; the cluster is Deleting"), "{md}");

    // the state comes last, after the other parts of a drift
    let mut moved_doc = read_json(&dir.path().join("cluster-b/cluster.json"));
    moved_doc["status"]["state"] = Value::from("Error");
    let moved = serve_cluster(MemoryApi::new(), dir.path(), "cluster-b")
        .with(cluster_route(CLUSTER_UID), moved_doc);
    let v = ops::verify(&recorded, &Palette::new(moved), "t1", &Progress::silent()).unwrap();
    assert!(!v.unchanged);
    let line = verdict_of(&v);
    assert!(line.starts_with("DRIFT: "), "{line}");
    assert!(line.ends_with("; the cluster is Error"), "{line}");

    // over several records, the line of each record names the state
    let deleting = cluster_in_state(dir.path(), "Deleting", serde_json::json!({}));
    let s = common::verify_summary(&recorded, &deleting);
    assert_eq!(s.records[0].status, RecordStatus::Unchanged);
    assert_eq!(s.records[0].state.as_deref(), Some("Deleting"));
    let text = common::terminal().render_verify_summary(&s).unwrap();
    assert!(
        text.contains("unchanged since t0; the state is Deleting"),
        "{text}"
    );
}

#[test]
fn verify_reports_a_gone_edge_host_or_cluster_group_as_drift_and_not_as_an_error() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let host = snapshot_of(
        &serve_edge_host(MemoryApi::new(), dir.path()),
        "edgehost:e1",
    )
    .unwrap();
    let group_api = serve_cluster_group(MemoryApi::new(), dir.path());
    let group = common::snapshot_of(&group_api, "clustergroup:g1").unwrap();

    // Palette answers 404 for each uid: the host and the group were deleted
    for (recorded, line) in [
        (
            &host,
            "DRIFT: edge host edge-1 no longer exists: uid e1 is gone from Palette since the record of t0",
        ),
        (
            &group,
            "DRIFT: cluster group group-1 no longer exists: uid g1 is gone from Palette since the record of t0",
        ),
    ] {
        let v = ops::verify(
            recorded,
            &Palette::new(MemoryApi::new()),
            "t1",
            &Progress::silent(),
        )
        .unwrap();
        let ver = v.diff.verify.as_ref().unwrap();
        assert_eq!(ver.status, VerifyStatus::Drifted);
        assert!(ver.gone && ver.deleted_at.is_none());
        assert_eq!(verdict_of(&v), line);
    }

    // a host that is not in a cluster, and a group that is not active, say their state
    let v = ops::verify(
        &host,
        &Palette::new(serve_edge_host(MemoryApi::new(), dir.path())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(v.unchanged);
    let line = verdict_of(&v);
    assert!(line.ends_with("; the edge host is ready"), "{line}");
    for (active, suffix) in [(true, ""), (false, "; the cluster group is inactive")] {
        let api = serve_cluster_group(MemoryApi::new(), dir.path());
        let mut doc = api.get_json(&group_route("g1")).unwrap();
        doc["status"] = serde_json::json!({"isActive": active});
        let api = api.with(group_route("g1"), doc);
        let v = ops::verify(&group, &Palette::new(api), "t1", &Progress::silent()).unwrap();
        assert!(v.unchanged);
        let line = verdict_of(&v);
        assert!(line.ends_with(&format!("since t0{suffix}")), "{line}");
    }
}

#[test]
fn snapshot_reads_the_virtual_clusters_of_a_cluster_group_and_verify_sends_no_such_request() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_cluster_group(MemoryApi::new(), dir.path());
    let recorded = common::snapshot_of(&api, "clustergroup:g1").unwrap();
    let members = |s: &strata::snapshot::Snapshot| -> Vec<String> {
        s.source
            .cluster
            .as_ref()
            .unwrap()
            .members
            .iter()
            .map(|m| m.name.clone())
            .collect()
    };
    assert_eq!(members(&recorded), ["lab-cluster"]);
    assert_eq!(api.calls(&post(GROUP_MEMBERS)), 1);
    let v = ops::verify(&recorded, &Palette::new(&api), "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged);
    assert!(members(&v.live).is_empty());
    assert_eq!(api.calls(&post(GROUP_MEMBERS)), 1);
}

#[test]
fn verify_reports_a_renamed_version_as_renamed_and_not_as_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    // PATCH /v1/clusterprofiles/{uid}/metadata renames the profile: the uid and the version stay
    let mut doc = read_json(&dir.path().join("a/profile.json"));
    doc["metadata"]["name"] = Value::from("lab-profile-renamed");
    let api =
        serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A).with(profile_route(UID_A), doc);
    let v = ops::verify(&recorded, &Palette::new(api), "t1", &Progress::silent()).unwrap();
    assert!(!v.unchanged);
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.status, VerifyStatus::Drifted);
    assert_eq!(ver.renamed_to.as_deref(), Some("lab-profile-renamed@1.2.0"));
    assert!(ver.overwritten_by.is_none());
    let line = verdict_of(&v);
    assert_eq!(
        line,
        "DRIFT: published version lab-profile@1.2.0 was renamed to lab-profile-renamed@1.2.0"
    );
    assert_eq!(
        serde_json::to_value(ver).unwrap()["renamedTo"],
        Value::from("lab-profile-renamed@1.2.0")
    );

    // a legacy version bump still overwrites the version in place
    let mut doc = read_json(&dir.path().join("a/profile.json"));
    doc["spec"]["version"] = Value::from("1.3.0");
    let api =
        serve_fixture(MemoryApi::new(), dir.path(), "a", UID_A).with(profile_route(UID_A), doc);
    let v = ops::verify(&recorded, &Palette::new(api), "t1", &Progress::silent()).unwrap();
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.overwritten_by.as_deref(), Some("lab-profile@1.3.0"));
    assert!(ver.renamed_to.is_none());
    assert!(
        verdict_of(&v).contains("was overwritten in place"),
        "{}",
        verdict_of(&v)
    );
}

// ---- snapshot --validate ---------------------------------------------------------------

/// The route of the check of the name and the version of `snap`.
fn name_route(snap: &strata::snapshot::Snapshot) -> String {
    format!(
        "/v1/clusterprofiles/validate/name?name={}&version={}",
        snap.source.name, snap.source.version
    )
}

/// The import validate route.
const IMPORT_ROUTE: &str = "/v1/clusterprofiles/import/validate";

/// The route that asks Palette if it takes the packs of a new profile.
const PROFILE_VALIDATE_PACKS: &str = "/v1/clusterprofiles/validate/packs";

/// The route that asks Palette if it takes the packs of an update of the profile version `uid`.
fn update_validate_packs_route(uid: &str) -> String {
    format!("{}/validate/packs", profile_route(uid))
}

/// The registry listing that `snapshot --validate` reads for the body of the import validate
/// route: the registry of the packs of fixture B.
fn registry_listing() -> Value {
    serde_json::json!({"items": [{"uid": "reg-public", "name": "Public Repo", "kind": "pack",
        "isPrivate": false, "isDefault": true, "scope": "cluster"}]})
}

/// `api` with the name route and the import validate route, which `snapshot --validate` asks
/// for a candidate before the packs route. Both take `snap`.
fn serve_candidate_checks(api: MemoryApi, snap: &strata::snapshot::Snapshot) -> MemoryApi {
    api.with(name_route(snap), Value::Null)
        .with(post(IMPORT_ROUTE), serde_json::json!({}))
        .with(REGISTRY_LISTING, registry_listing())
}

#[test]
fn snapshot_validate_of_a_candidate_that_names_its_registry_reads_the_listing_one_time() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded_b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    let file = dir.path().join("c.yaml");
    candidate::write(&Candidate::from_snapshot(&recorded_b).unwrap(), &file, None).unwrap();
    // a registry by name: the resolve reads the listing, and so does the import body
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("registryUid: reg-public"), "{text}");
    fs::write(
        &file,
        text.replace("registryUid: reg-public", "registry: Public Repo"),
    )
    .unwrap();
    let key = post(PROFILE_VALIDATE_PACKS);
    let api = serve_candidate_checks(serve_registry_for_b(dir.path()), &recorded_b)
        .with(key.as_str(), serde_json::json!({"packs": {"results": []}}));
    let snap = ops::snapshot_from_candidate(
        &file,
        true,
        || Ok(Palette::new(&api)),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(snap.source.palette_validation.unwrap().accepts());
    // the resolve and the import body use one cache of the listings of the command
    assert_eq!(api.calls(REGISTRY_LISTING), 1);
}

#[test]
fn snapshot_validate_asks_palette_whether_it_takes_the_packs_of_a_candidate() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded_b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    let file = dir.path().join("c.yaml");
    candidate::write(&Candidate::from_snapshot(&recorded_b).unwrap(), &file, None).unwrap();
    let registry = serve_registry_for_b(dir.path());
    let key = post(PROFILE_VALIDATE_PACKS);
    let path = key.as_str();
    let message = "Pack 'strata-live-dep' is required with minimum compatible version '1.0.0'";
    let answer = serde_json::json!({"packs": {"results": [{"name": "kubernetes",
        "displayName": "kubernetes",
        "errors": [{"code": "DependencyPackWithMinVersionRequired", "message": message}]}]}});
    let registry = serve_candidate_checks(registry, &recorded_b);
    let api = registry.clone().with(path, answer);
    let snap = ops::snapshot_from_candidate(
        &file,
        true,
        || Ok(Palette::new(&api)),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    let v = snap.source.palette_validation.clone().unwrap();
    assert!(!v.accepts());
    assert_eq!(v.errors().len(), 1, "{v:?}");
    assert_eq!(
        v.errors()[0].line(),
        format!("DependencyPackWithMinVersionRequired kubernetes: {message}")
    );
    // one GET of the name and the version
    assert_eq!(api.calls(&name_route(&snap)), 1);
    // one POST to the import validate route, and one to the route of a create, with the packs
    // of the candidate in profile order
    let posted = api.posted();
    assert_eq!(posted.len(), 2);
    assert_eq!(posted[0].0, IMPORT_ROUTE);
    let entries = registry_listing()["items"].as_array().unwrap().clone();
    assert_eq!(posted[0].1, ops::profile_import_body(&snap, &entries));
    let import = &posted[0].1;
    assert_eq!(import["metadata"]["name"], snap.source.name.as_str());
    assert_eq!(import["spec"]["version"], snap.source.version.as_str());
    for (sent, pack) in import["spec"]["template"]["packs"]
        .as_array()
        .unwrap()
        .iter()
        .zip(&recorded_b.packs)
    {
        assert_eq!(sent["name"], pack.name.as_str());
        assert_eq!(sent["version"], pack.version.as_str());
        assert!(sent.get("uid").is_none(), "{sent}");
        assert_eq!(
            sent["registry"]["metadata"],
            serde_json::json!({"uid": "reg-public", "name": "Public Repo", "kind": "pack",
                "isPrivate": false})
        );
    }
    assert_eq!(posted[1].0, PROFILE_VALIDATE_PACKS);
    assert_eq!(posted[1].1, ops::profile_validate_body(&snap));
    let body = &posted[1].1;
    assert_eq!(body["cloudType"], recorded_b.source.cloud_type.as_str());
    assert_eq!(body["type"], recorded_b.source.kind.as_str());
    let names: Vec<&str> = body["packs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    let expected: Vec<&str> = recorded_b.packs.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, expected);
    for (sent, pack) in body["packs"]
        .as_array()
        .unwrap()
        .iter()
        .zip(&recorded_b.packs)
    {
        assert_eq!(sent["uid"], pack.pack_uid.as_str());
        assert_eq!(sent["registryUid"], pack.registry_uid.as_str());
        assert_eq!(sent["tag"], pack.tag.as_str());
        assert_eq!(sent["values"], pack.values.as_str());
        assert_eq!(
            sent["manifests"].as_array().unwrap().len(),
            pack.manifests.len()
        );
    }
    // the command line makes a refusal an error: exit 2, kind palette-refuses, and no record
    let err = Error::PaletteRefuses {
        reference: snap.source.reference.clone(),
        errors: v.errors().to_vec(),
    };
    assert_eq!(err.kind(), "palette-refuses");
    assert_eq!(err.exit_code(), 2);
    assert_eq!(
        err.to_json()["errors"][0]["code"],
        "DependencyPackWithMinVersionRequired"
    );

    // a 200 with no error: Palette accepts, and the record holds the answer and the same root
    let api = registry
        .clone()
        .with(path, serde_json::json!({"packs": {"results": []}}));
    let snap = ops::snapshot_from_candidate(
        &file,
        true,
        || Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(snap.root, recorded_b.root);
    let json = serde_json::to_value(&snap).unwrap();
    assert_eq!(
        json["source"]["paletteValidation"],
        serde_json::json!({"accepts": true})
    );
    // without --validate, the record has no such field
    let plain = ops::snapshot_from_candidate(
        &file,
        false,
        || Ok(Palette::new(registry.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(
        serde_json::to_value(&plain).unwrap()["source"]
            .get("paletteValidation")
            .is_none()
    );
}

#[test]
fn snapshot_validate_takes_a_refusal_of_the_whole_request_as_an_answer() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    let mut candidate = b.clone();
    candidate.source.uid = String::new();
    let key = post(PROFILE_VALIDATE_PACKS);
    let path = key.as_str();
    // Palette sends each refusal of the whole request as HTTP 500
    for code in ValidateRoute::ProfilePacks.refusal_codes() {
        let body = format!(r#"{{"code":"{code}","message":"refused"}}"#);
        // the retry does not send it again: Palette gives the same refusal
        let error = Error::Api {
            method: Method::Post,
            path: path.into(),
            status: 500,
            body: body.clone(),
            retry_after: None,
        };
        assert_eq!(error.class(), FailureClass::Terminal, "{code}");
        let api = serve_candidate_checks(MemoryApi::new(), &candidate).failing(path, 500, body);
        let v = ops::profile_validation(&Palette::new(&api), &candidate).unwrap();
        assert!(!v.accepts(), "{code}");
        assert_eq!(v.errors().len(), 1);
        assert_eq!(v.errors()[0].pack, "");
        assert_eq!(v.errors()[0].line(), format!("{code}: refused"));
    }
    // the API schema refuses a list with two equal packs with HTTP 422, before the route
    let body = r#"{"code":610,"message":"packs in body shouldn't contain duplicates"}"#;
    let api = serve_candidate_checks(MemoryApi::new(), &candidate).failing(path, 422, body);
    let v = ops::profile_validation(&Palette::new(&api), &candidate).unwrap();
    assert_eq!(
        v.errors()[0].line(),
        "610: packs in body shouldn't contain duplicates"
    );
    // a 500 with another code is a fault of Palette, a 403 is the key, and a 400 is a body with
    // an error of strata: errors of strata
    for (status, code) in [
        (500, "InternalError"),
        (403, "Forbidden"),
        (400, "InvalidPayload"),
    ] {
        let body = format!(r#"{{"code":"{code}"}}"#);
        let api = serve_candidate_checks(MemoryApi::new(), &candidate).failing(path, status, body);
        let err = ops::profile_validation(&Palette::new(&api), &candidate).unwrap_err();
        assert!(
            matches!(err, Error::Api { status: s, .. } if s == status),
            "{err}"
        );
    }
}

#[test]
fn snapshot_validate_of_a_version_asks_the_route_of_an_update_of_that_version() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    assert!(!b.source.uid.is_empty());
    let route = update_validate_packs_route(&b.source.uid);
    let api = MemoryApi::new().with(post(&route), serde_json::json!({}));
    let v = ops::profile_validation(&Palette::new(&api), &b).unwrap();
    assert!(v.accepts());
    let posted = api.posted();
    assert_eq!(posted.len(), 1);
    assert_eq!(posted[0].0, route);
    assert_eq!(posted[0].1, ops::profile_validate_body(&b));

    // a record of a cluster is not a profile: a usage error, and no request
    let cluster_api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a");
    let cluster = snapshot_cluster(&cluster_api, None).unwrap();
    let api = MemoryApi::new();
    let err = ops::profile_validation(&Palette::new(&api), &cluster).unwrap_err();
    assert!(matches!(err, Error::Usage { .. }), "{err}");
    assert!(api.posted().is_empty());

    // a record of an app profile is not one either: its tiers are no packs of a profile
    let mut app = b.clone();
    app.source.kind = ProfileType::App.as_str().to_string();
    let err = ops::profile_validation(&Palette::new(&api), &app).unwrap_err();
    assert!(
        matches!(&err, Error::Usage { message } if message.contains("an app profile")),
        "{err}"
    );
    assert!(api.posted().is_empty());
}

#[test]
fn snapshot_validate_of_a_candidate_takes_a_refusal_of_the_name_route_or_the_import_route() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    let mut candidate = b.clone();
    candidate.source.uid = String::new();
    let accepting = || {
        serve_candidate_checks(MemoryApi::new(), &candidate).with(
            post(PROFILE_VALIDATE_PACKS),
            serde_json::json!({"packs": {"results": []}}),
        )
    };
    let import = post(IMPORT_ROUTE);
    let refusal = |api: MemoryApi| ops::profile_validation(&Palette::new(&api), &candidate);
    let body_of = |code: &str| format!(r#"{{"code":"{code}","message":"refused"}}"#);
    let class_of = |status: u16, code: &str| {
        Error::Api {
            method: Method::Post,
            path: "/".into(),
            status,
            body: body_of(code),
            retry_after: None,
        }
        .class()
    };
    assert!(refusal(accepting()).unwrap().accepts());

    // the name route: a version that Palette does not parse, an empty name, and a version that
    // the project holds. The retry does not send them again
    let name_refusals = ValidateRoute::ProfileName
        .refusal_codes()
        .map(|code| (500, code))
        .chain([(409, PROFILE_EXISTS_CODE)]);
    for (status, code) in name_refusals {
        assert_eq!(class_of(status, code), FailureClass::Terminal, "{code}");
        let v =
            refusal(accepting().failing(name_route(&candidate), status, body_of(code))).unwrap();
        assert!(!v.accepts(), "{code}");
        assert_eq!(v.errors().len(), 1, "{code}");
        assert_eq!(v.errors()[0].line(), format!("{code}: refused"));
    }

    // the import route: each code of a check, a version that the project holds, and a macro
    // that Palette does not define
    let refusals = ValidateRoute::ProfileImport
        .refusal_codes()
        .map(|code| (500, code))
        .chain([(409, PROFILE_EXISTS_CODE), (404, PACK_MACRO_UNDEFINED_CODE)]);
    for (status, code) in refusals {
        assert_eq!(class_of(status, code), FailureClass::Terminal, "{code}");
        let v = refusal(accepting().failing(&import, status, body_of(code))).unwrap();
        assert!(!v.accepts(), "{code}");
        assert_eq!(v.errors()[0].line(), format!("{code}: refused"));
    }
    // the API schema refuses a list with two equal items, with a number as the code
    let body = r#"{"code":610,"message":"packs in body shouldn't contain duplicates"}"#;
    let v = refusal(accepting().failing(&import, 422, body)).unwrap();
    assert_eq!(
        v.errors()[0].line(),
        "610: packs in body shouldn't contain duplicates"
    );

    // the two routes give the same refusal of a version: the answer holds it once
    let body =
        r#"{"code":"VersionParseFailure","message":"Unable to parse the version '1.2.3.4'"}"#;
    let api = accepting()
        .failing(name_route(&candidate), 500, body)
        .failing(&import, 500, body);
    assert_eq!(refusal(api).unwrap().errors().len(), 1);

    // the import route did not match a pack to a registry, so it checked nothing: an error of
    // strata, which the retry does not send again. A 403 and a 500 with another code are errors
    for code in PROFILE_IMPORT_MATCH_CODES {
        assert_eq!(class_of(500, code), FailureClass::Terminal, "{code}");
    }
    let match_errors = PROFILE_IMPORT_MATCH_CODES.map(|code| (500, code));
    for (status, code) in match_errors
        .into_iter()
        .chain([(403, "Forbidden"), (500, "InternalError")])
    {
        let err = refusal(accepting().failing(&import, status, body_of(code))).unwrap_err();
        assert!(
            matches!(err, Error::Api { status: s, .. } if s == status),
            "{err}"
        );
    }
    // a 400 of the name route is an error
    let err = refusal(accepting().failing(name_route(&candidate), 400, body_of("x"))).unwrap_err();
    assert!(matches!(err, Error::Api { status: 400, .. }), "{err}");

    // a version that Palette holds goes only to the route of an update
    let route = post(&update_validate_packs_route(&b.source.uid));
    let api = MemoryApi::new().with(route, serde_json::json!({}));
    assert!(
        ops::profile_validation(&Palette::new(&api), &b)
            .unwrap()
            .accepts()
    );
    assert_eq!(api.posted().len(), 1);
    assert_eq!(api.calls(&name_route(&b)), 0);
}

#[test]
fn snapshot_validate_of_a_candidate_with_a_disabled_pack_prints_the_refusal_of_the_import_route() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded_b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    let file = dir.path().join("c.yaml");
    candidate::write(&Candidate::from_snapshot(&recorded_b).unwrap(), &file, None).unwrap();
    // the import validate route runs the check of a disabled pack before the route of the packs,
    // and gives the refusal as HTTP 500, word for word
    let pack = recorded_b.packs[0].name.as_str();
    let message = format!("Disabled packs '{pack}' are forbidden");
    let body = format!(
        r#"{{"code":"DisabledPacksForbidden","details":null,"message":"{message}","ref":"r"}}"#
    );
    let api = serve_candidate_checks(serve_registry_for_b(dir.path()), &recorded_b)
        .failing(post(IMPORT_ROUTE), 500, body)
        .with(
            post(PROFILE_VALIDATE_PACKS),
            serde_json::json!({"packs": {"results": []}}),
        );
    let snap = ops::snapshot_from_candidate(
        &file,
        true,
        || Ok(Palette::new(&api)),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    // an answer of Palette, not an error of the request: the route is not asked again
    assert_eq!(api.calls(&post(IMPORT_ROUTE)), 1);
    let v = snap.source.palette_validation.clone().unwrap();
    assert!(!v.accepts());
    let lines = strata::model::validation::answer_lines(v.errors());
    assert_eq!(
        lines,
        [format!("refuses: DisabledPacksForbidden: {message}")]
    );
    assert_eq!(
        render::palette_line(&lines[0]),
        format!("palette    refuses: DisabledPacksForbidden: {message}")
    );
    // the command line exits as it does for each refusal of Palette, and writes no record
    let err = ops::require_acceptance(&snap).unwrap_err();
    assert!(matches!(err, Error::PaletteRefuses { .. }), "{err}");
    assert_eq!(err.kind(), "palette-refuses");
    assert_eq!(err.exit_code(), 2);
}

#[test]
fn a_candidate_with_a_fault_that_only_a_route_answers_gives_a_record_with_the_same_root() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded_b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    let registry = serve_registry_for_b(dir.path());
    let mut c = Candidate::from_snapshot(&recorded_b).unwrap();
    c.version = "2.2.2.develop".into();
    let file = dir.path().join("c.yaml");
    candidate::write(&c, &file, None).unwrap();
    let snap = ops::snapshot_from_candidate(
        &file,
        false,
        || Ok(Palette::new(registry.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(snap.source.version, "2.2.2.develop");
    assert_eq!(snap.root, recorded_b.root);
    // a pack twice gives a record with each layer of the candidate
    let mut twice = Candidate::from_snapshot(&recorded_b).unwrap();
    twice.layers.push(twice.layers[0].clone());
    let twice_file = dir.path().join("twice.yaml");
    candidate::write(&twice, &twice_file, None).unwrap();
    let snap_twice = ops::snapshot_from_candidate(
        &twice_file,
        false,
        || Ok(Palette::new(registry.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(snap_twice.packs.len(), recorded_b.packs.len() + 1);

    // with --validate, Palette refuses the version, and the record holds the refusal
    let body =
        r#"{"code":"VersionParseFailure","message":"Unable to parse the version '2.2.2.develop'"}"#;
    let api = serve_candidate_checks(registry, &snap)
        .failing(name_route(&snap), 500, body)
        .with(
            post(PROFILE_VALIDATE_PACKS),
            serde_json::json!({"packs": {"results": []}}),
        );
    let refused = ops::snapshot_from_candidate(
        &file,
        true,
        || Ok(Palette::new(&api)),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(refused.root, recorded_b.root);
    let v = refused.source.palette_validation.clone().unwrap();
    assert_eq!(
        strata::model::validation::answer_lines(v.errors()),
        ["refuses: VersionParseFailure: Unable to parse the version '2.2.2.develop'"]
    );
    assert!(matches!(
        ops::require_acceptance(&refused),
        Err(Error::PaletteRefuses { .. })
    ));
}

// ---- a pack that Palette marks invalid (#108) ---------------------------------------------------

/// The reason that the tests give an invalid pack ref, in the form of the message of Palette.
const INVALID_REASON: &str = "ubuntu-aws 22.04 tag is deleted";

/// An API that serves fixture A under `UID_A`. Palette marks its OS pack invalid when `invalid`
/// is true.
fn serve_invalid(raw_root: &Path, invalid: bool) -> MemoryApi {
    let dir = if invalid { "invalid" } else { "valid" };
    with_os_pack(raw_root, dir, |p| {
        if invalid {
            p["isInvalid"] = Value::Bool(true);
            p["inValidReason"] = Value::from(INVALID_REASON);
        }
    });
    serve_fixture(MemoryApi::new(), raw_root, dir, UID_A)
}

#[test]
fn a_record_keeps_the_invalid_flag_and_the_reason_of_palette_outside_the_root() {
    use strata::snapshot::Pack;
    let pack = |invalid: Value, reason: &str| {
        serde_json::json!({"name": "ubuntu-aws", "type": "spectro", "layer": "os", "tag": "22.04",
            "packUid": "p1", "registryUid": "r1", "values": "a: 1", "isInvalid": invalid,
            "inValidReason": reason})
    };
    let invalid = Pack::from_api(&pack(Value::Bool(true), INVALID_REASON), &[]);
    assert!(invalid.invalid);
    assert_eq!(invalid.invalid_reason, INVALID_REASON);
    let doc = serde_json::to_value(&invalid).unwrap();
    assert_eq!(doc["invalid"], true);
    assert_eq!(doc["invalidReason"], INVALID_REASON);
    // the flag is outside the root: the node hash is the node hash of the valid pack
    let valid = Pack::from_api(&pack(Value::Null, ""), &[]);
    assert_eq!(invalid.hash.node, valid.hash.node);
    let doc = serde_json::to_value(&valid).unwrap();
    assert!(doc.get("invalid").is_none() && doc.get("invalidReason").is_none());
    // a reason without the flag is not kept: Palette clears the flag when the tag is back
    let cleared = Pack::from_api(&pack(Value::Bool(false), INVALID_REASON), &[]);
    assert!(!cleared.invalid && cleared.invalid_reason.is_empty());
    // a record written before the fields reads as one with a valid pack
    let mut old = serde_json::to_value(&invalid).unwrap();
    let fields = old.as_object_mut().unwrap();
    fields.remove("invalid");
    fields.remove("invalidReason");
    let read: Pack = serde_json::from_value(old).unwrap();
    assert!(!read.invalid && read.invalid_reason.is_empty());

    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let with_flag = common::snapshot_of_at(&serve_invalid(dir.path(), true), UID_A, "t0").unwrap();
    let without = common::snapshot_of_at(&serve_invalid(dir.path(), false), UID_A, "t0").unwrap();
    assert_eq!(with_flag.root, without.root);
    assert_eq!(with_flag.content_digest(), without.content_digest());
    let os = with_flag.packs.iter().find(|p| p.layer == "os").unwrap();
    assert!(os.invalid);
    assert_eq!(os.invalid_reason, INVALID_REASON);
}

#[test]
fn compare_marks_an_invalid_pack_on_its_line_and_counts_it_in_the_summary() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let valid = common::snapshot_of_at(&serve_invalid(dir.path(), false), UID_A, "t0").unwrap();
    let invalid = common::snapshot_of_at(&serve_invalid(dir.path(), true), UID_A, "t1").unwrap();
    let terminal = |d: &strata::diff::Diff, a: &Snapshot, b: &Snapshot| {
        common::terminal().render(d, a, b, false).unwrap()
    };
    let d = ops::compare(&valid, &invalid, "t2");
    assert!(
        d.same && d.nothing_differs(),
        "the flag is outside the root"
    );
    assert_eq!(d.summary.packs_invalid, 1);
    let mark = format!("INVALID on side B: {INVALID_REASON}");
    let text = terminal(&d, &valid, &invalid);
    let line = text
        .lines()
        .find(|l| l.contains("ubuntu-aws"))
        .expect("a line for the OS pack");
    assert!(
        line.contains("unchanged") && line.ends_with(&mark),
        "{text}"
    );
    let md = common::markdown()
        .render(&d, &valid, &invalid, false)
        .unwrap();
    assert!(md.contains(&format!("unchanged **{mark}**")), "{md}");
    let json = serde_json::to_value(&d).unwrap();
    assert_eq!(json["summary"]["packsInvalid"], 1);
    let os = json["packs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["key"] == "os/ubuntu-aws")
        .unwrap();
    assert_eq!(os["b"]["invalid"], true);
    assert_eq!(os["b"]["invalidReason"], INVALID_REASON);
    assert!(os["a"].get("invalid").is_none());

    // the other side, and the two sides
    let d = ops::compare(&invalid, &valid, "t2");
    assert_eq!(d.summary.packs_invalid, 1);
    let text = terminal(&d, &invalid, &valid);
    assert!(text.contains("INVALID on side A: "), "{text}");
    let d = ops::compare(&invalid, &invalid, "t2");
    assert_eq!(d.summary.packs_invalid, 1, "one pack, on the two sides");
    let text = terminal(&d, &invalid, &invalid);
    assert!(text.contains("INVALID on both sides: "), "{text}");
    // no invalid pack: no mark, and the count is 0
    let d = ops::compare(&valid, &valid, "t2");
    assert_eq!(d.summary.packs_invalid, 0);
    assert!(!terminal(&d, &valid, &valid).contains("INVALID"));
}

#[test]
fn verify_reports_each_pack_that_palette_marks_invalid_now_and_keeps_the_status_of_the_content() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = common::snapshot_of_at(&serve_invalid(dir.path(), false), UID_A, "t0").unwrap();
    let live = serve_invalid(dir.path(), true);
    let v = ops::verify(
        &recorded,
        &Palette::new(live.clone()),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(v.unchanged, "the content is the recorded content");
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.status, VerifyStatus::Unchanged);
    assert_eq!(
        ver.invalid_packs,
        vec![strata::diff::InvalidPack {
            key: "os/ubuntu-aws".into(),
            reason: INVALID_REASON.into(),
        }]
    );
    let sentence = format!("Palette marks pack os/ubuntu-aws invalid now ({INVALID_REASON})");
    let verdict = common::terminal().verdict(&v.diff).unwrap();
    assert!(verdict.starts_with("OK: "), "{verdict}");
    assert!(
        verdict.contains(&format!("\nINVALID: {sentence}")),
        "{verdict}"
    );
    let verdict = common::markdown().verdict(&v.diff).unwrap();
    assert!(
        verdict.contains(&format!("**INVALID**: {sentence}")),
        "{verdict}"
    );
    let report = common::terminal()
        .render(&v.diff, &recorded, &v.live, false)
        .unwrap();
    assert!(
        report.contains(&format!("INVALID on side B: {INVALID_REASON}")),
        "{report}"
    );
    let json = serde_json::to_value(&v.diff).unwrap();
    assert_eq!(
        json["verify"]["invalidPacks"],
        serde_json::json!([{"key": "os/ubuntu-aws", "reason": INVALID_REASON}])
    );

    // the summary of many records names the pack too, and the exit stays 0
    let s = common::verify_summary(&recorded, &live);
    assert_eq!(
        (s.records[0].status, s.exit_code()),
        (RecordStatus::Unchanged, 0)
    );
    let lines = common::terminal().render_verify_summary(&s).unwrap();
    assert!(lines.contains(&format!("; {sentence}")), "{lines}");

    // a pack that is valid again: nothing to report
    let v = ops::verify(
        &recorded,
        &Palette::new(serve_invalid(dir.path(), false)),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    let ver = v.diff.verify.as_ref().unwrap();
    assert!(ver.invalid_packs.is_empty());
    let verdict = common::terminal().verdict(&v.diff).unwrap();
    assert!(!verdict.contains("INVALID"), "{verdict}");
}

/// The root and the content digest that strata 0.21.0 gives each kind of record from the
/// fixtures of these tests.
///
/// A change of a root is a change of the schema id, so no release moves one.
const ROOT_OF_EACH_KIND: [(&str, &str, &str); 8] = [
    (
        "published",
        "4016d4f43ffab583718586a83a7af9e5c2e736ca810daef5655b962c91eca423",
        "44680519d410b0a10413a10c3668323d9b30e991529eae3d4b0a0464b50a15ea",
    ),
    (
        "draft",
        "28dbbe988abc43f0a6db6dd469128941cec942a974d8fadb41cd1b6e05897c36",
        "936a07d7141b3d4e667676807e31a2b0fc34193e97b2ccc55cc812197f98d599",
    ),
    (
        "cluster",
        "4016d4f43ffab583718586a83a7af9e5c2e736ca810daef5655b962c91eca423",
        "44680519d410b0a10413a10c3668323d9b30e991529eae3d4b0a0464b50a15ea",
    ),
    (
        "cluster group",
        "e7cb1ae941a41a9d2c6fc956050beeb2c6a2db7b9fdf12f2aa250f48b6dc0c75",
        "c7db0abf7eb675c7cc83bc1cac8139b67cdefad380d0ce244b34ebe68156a8b7",
    ),
    (
        "app profile",
        "0486a3007718b8c492a8fc059906794594fee3923a6f4e01b100d7e72c769972",
        "7cae758527f7d0245c9b7c2bf1accadccdfc1c8fa34aba6d33914f21dc50f34f",
    ),
    (
        "candidate",
        "2ef663237156bd1206ca52209bc621be227c6f5f8277101ec34e45d346431b9d",
        "e191139e603fed9251e0a65216ed073d529af255977ec797af817cba10682ef7",
    ),
    (
        "registry pack",
        "2ef663237156bd1206ca52209bc621be227c6f5f8277101ec34e45d346431b9d",
        "e191139e603fed9251e0a65216ed073d529af255977ec797af817cba10682ef7",
    ),
    (
        "gone",
        "487f5e18aa696e54a372da998bc4f0c38c7585d302664fb03e31464769d35187",
        "0b54e29bb56949df405262d22e4122d46d00fe2e594f5b34ad6577ea76a81020",
    ),
];

#[test]
fn the_root_of_each_kind_of_record_does_not_change_between_releases() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let a = dir.path().join("a");
    let published = ops::snapshot_from_dir(&a, "a", "t0").unwrap();
    // the order of the variables in the answer of Palette does not change the root: the record
    // sorts them by name
    let reversed = {
        let to = dir.path().join("a-variables-reversed");
        fs::create_dir_all(&to).unwrap();
        for f in ["profile.json", "manifests.json"] {
            fs::copy(a.join(f), to.join(f)).unwrap();
        }
        let mut vars = read_json(&a.join("variables.json"));
        vars["variables"].as_array_mut().unwrap().reverse();
        fs::write(
            to.join("variables.json"),
            serde_json::to_string_pretty(&vars).unwrap(),
        )
        .unwrap();
        ops::snapshot_from_dir(&to, "a", "t0").unwrap()
    };
    assert_eq!(reversed.variables, published.variables);
    assert_eq!(reversed.root, published.root, "the sort of the variables");
    // the draft of `a`: one more line in the values of its Kubernetes pack
    let draft = {
        let profile = read_json(&a.join("profile.json"));
        let mut draft = profile["spec"]["published"].clone();
        let values = draft["packs"][1]["values"].as_str().unwrap().to_string();
        draft["packs"][1]["values"] = Value::from(format!("{values}\nextra: 1\n"));
        let document = profile_document(
            UID_A,
            "lab-profile",
            "1.2.0",
            profile["spec"]["published"].clone(),
            Some(draft),
        );
        let api = serve_profile(
            MemoryApi::new(),
            UID_A,
            document,
            read_json(&a.join("manifests.json")),
            read_json(&a.join("variables.json")),
        );
        snapshot_of(&api, &format!("{UID_A}@draft")).unwrap()
    };
    let cluster = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-a"),
        None,
    )
    .unwrap();
    let group = snapshot_of(
        &serve_cluster_group(MemoryApi::new(), dir.path()),
        "clustergroup:g1",
    )
    .unwrap();
    let app = snapshot_of(&serve_app_profile(MemoryApi::new(), "2"), "appprofile:ap1").unwrap();
    let registry = serve_registry(vec![], &[("reg-public", public_entity())]);
    // a candidate of the one pack that the pack record holds, so the two share a root
    let candidate = {
        let file = dir.path().join("fred.yaml");
        write_yaml(
            &file,
            &serde_json::json!({
                "name": "fred", "version": "1.0.0", "type": "add-on", "registryUid": "reg-public",
                "layers": [{"name": "csi-aws-ebs", "version": "1.63.1"}],
            }),
        );
        candidate_snapshot(&file, &registry).unwrap()
    };
    let pack = snapshot_of(&registry, "pack:csi-aws-ebs@1.63.1").unwrap();
    let gone = published.gone("t1");

    let records = [
        (&published, Template::Published),
        (&draft, Template::Draft),
        (&cluster, Template::Applied),
        (&group, Template::Applied),
        (&app, Template::Published),
        (&candidate, Template::Candidate),
        (&pack, Template::Registry),
        (&gone, Template::Published),
    ];
    for ((kind, root, content), (record, template)) in ROOT_OF_EACH_KIND.iter().zip(records) {
        assert_eq!(record.source.template, template, "{kind}");
        assert_eq!(record.root, *root, "the root of the {kind} record");
        assert_eq!(
            record.content_digest(),
            *content,
            "the content digest of the {kind} record"
        );
    }
}

// ---- a value that a later release adds to a closed set of a record (#348) ----------------------

#[test]
fn a_record_with_values_of_a_later_release_reads_with_no_error_and_writes_the_same_text() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let snap = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let mut doc = serde_json::to_value(&snap).unwrap();
    doc["source"]["template"] = Value::from("future");
    doc["tool"]["impl"] = Value::from("go");
    doc["packs"][0]["hash"]["upstreamSource"] = Value::from("packArchive.digest");
    let path = dir.path().join("later.json");
    write_record(&path, &doc);

    let read = Snapshot::load(&path).unwrap();
    assert_eq!(read.source.template.known(), None);
    assert_eq!(read.source.template.as_str(), "future");
    assert_eq!(
        read.tool.implementation.as_ref().map(|i| i.as_str()),
        Some("go")
    );
    assert_eq!(
        read.packs[0].hash.upstream_source.as_str(),
        "packArchive.digest"
    );
    // the record that strata writes from it holds the same text, and the same root
    assert_eq!(serde_json::to_value(&read).unwrap(), doc);
    assert_eq!(read.root, snap.root);
    // a template of a later release is not a draft, and a source of a later release gives a digest
    assert!(!read.source.is_draft());
    assert!(!read.packs[0].upstream_unknown());

    // compare reads the record, and the diff holds the text of each value
    let out = strata_command()
        .args(["compare", "--json"])
        .args([&path, &path])
        .output_locked()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", common::text(&out.stderr));
    let d: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(d["a"]["template"], "future");
    assert_eq!(d["packs"][0]["a"]["upstreamSource"], "packArchive.digest");
}

#[test]
fn a_cluster_record_of_a_later_kind_compares_by_its_text_and_verify_refuses_it_with_no_request() {
    let dir = tempfile::tempdir().unwrap();
    let (api, snap) = cluster_record_with_tiers(dir.path());
    let mut doc = serde_json::to_value(&snap).unwrap();
    doc["source"]["cluster"]["kind"] = Value::from("virtualMachine");
    let path = dir.path().join("later-kind.json");
    write_record(&path, &doc);

    let read = Snapshot::load(&path).unwrap();
    let c = read.source.cluster.as_ref().unwrap();
    assert_eq!(c.kind.as_str(), "virtualMachine");
    assert!(!c.kind.is_cluster());
    assert_eq!(c.kind.noun(), "virtualMachine");
    assert!(
        read.source
            .title()
            .ends_with(&format!("on virtualMachine {}", c.name)),
        "{}",
        read.source.title()
    );
    assert_eq!(serde_json::to_value(&read).unwrap(), doc);

    // two records of the same kind compare their tiers, as two cluster records do
    let d = ops::compare(&read, &read, "t1");
    assert!(d.same);
    assert!(!d.cluster_tiers.is_empty());
    assert!(d.cluster_tiers.iter().all(|t| t.same));
    // only two cluster records compare the values of their variables
    assert!(ops::compare(&snap, &snap, "t1").cluster_variables.is_some());
    assert!(d.cluster_variables.is_none());

    // verify cannot read an object of a kind that it does not know, so it asks Palette nothing
    let before = api.calls(&cluster_route(CLUSTER_UID));
    let err =
        ops::verify(&read, &Palette::new(api.clone()), "t1", &Progress::silent()).unwrap_err();
    assert_eq!(err.kind(), "usage");
    assert!(
        err.to_string()
            .contains("this strata cannot read a virtualMachine from Palette"),
        "{err}"
    );
    assert_eq!(api.calls(&cluster_route(CLUSTER_UID)), before);
}

#[test]
fn verify_refuses_a_record_that_it_cannot_check_before_it_needs_a_key() {
    let dir = tempfile::tempdir().unwrap();
    let (_, snap) = cluster_record_with_tiers(dir.path());
    let mut later = serde_json::to_value(&snap).unwrap();
    later["source"]["cluster"]["kind"] = Value::from("virtualMachine");
    let mut no_uid = serde_json::to_value(&snap).unwrap();
    no_uid["source"]["cluster"]["uid"] = Value::from("");
    let (later_path, no_uid_path) = (dir.path().join("later.json"), dir.path().join("none.json"));
    write_record(&later_path, &later);
    write_record(&no_uid_path, &no_uid);
    let valid_path = dir.path().join("valid.json");
    write_record(&valid_path, &serde_json::to_value(&snap).unwrap());

    // the run has no key, so a request gives missing-credential
    let home = tempfile::tempdir().unwrap();
    let run = |args: &[&Path]| {
        let mut c = strata_command();
        c.args(["verify", "--json"])
            .args(args)
            .env("STRATA_QUIET", "1")
            .env("HOME", home.path())
            .env_remove("PALETTE_API_KEY")
            .env_remove("SPECTROCLOUD_APIKEY");
        common::Run::of(&mut c)
    };
    let kind_of = |text: &str| -> Value { serde_json::from_str(text).unwrap() };
    for (path, kind) in [(&later_path, "usage"), (&no_uid_path, "no-uid")] {
        // one record: the error of the command
        let one = run(&[path]);
        assert_eq!(one.code, 2, "{}", one.err);
        assert_eq!(
            kind_of(one.err.trim())["error"]["kind"],
            kind,
            "{}",
            one.err
        );
        // several records: the error of the row, and the other record still shows its own
        let many = run(&[path, &valid_path]);
        assert_eq!(many.code, 2, "{}", many.err);
        let rows = kind_of(&many.out);
        assert_eq!(rows["records"][0]["error"]["kind"], kind, "{}", many.out);
        assert_eq!(
            rows["records"][1]["error"]["kind"], "missing-credential",
            "the record that strata can check needs the key: {}",
            many.out
        );
    }
}

#[test]
fn the_check_of_a_record_wants_a_uid_of_a_profile_record_and_none_of_a_pack_record() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let mut pack = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    pack.source.uid.clear();
    assert_eq!(
        ops::check_verifiable(&pack).unwrap_err().kind(),
        "no-uid",
        "a profile record with no uid"
    );
    pack.source.pack = Some(strata::snapshot::PackSource {
        registry_uid: "r1".into(),
        registry_name: "Public Repo".into(),
        pack_uid: String::new(),
        name: "kubernetes".into(),
        version: "1.30.0".into(),
    });
    assert!(
        ops::check_verifiable(&pack).is_ok(),
        "a pack record names a registry pack, and verify reads the registry"
    );
}

#[test]
fn a_tier_of_a_later_release_compares_by_its_fields_and_verify_names_it_and_does_not_compare_it() {
    let dir = tempfile::tempdir().unwrap();
    let (api, snap) = cluster_record_with_tiers(dir.path());
    let mut doc = serde_json::to_value(&snap).unwrap();
    doc["source"]["cluster"]["tiers"]["network"] =
        serde_json::json!({"hash": "h-network", "fields": {"zone": "us-east-2a"}});
    let path = dir.path().join("later-tier.json");
    write_record(&path, &doc);

    let read = Snapshot::load(&path).unwrap();
    let c = read.source.cluster.as_ref().unwrap();
    // the map keeps the order of the names, with the tier of the later release among them
    assert_eq!(
        c.tiers.keys().map(|t| t.as_str()).collect::<Vec<_>>(),
        ["applied", "network", "policy", "running"]
    );
    assert_eq!(serde_json::to_value(&read).unwrap(), doc);

    // between two records that carry it, the tier compares by its fields
    let mut moved = doc.clone();
    moved["source"]["cluster"]["tiers"]["network"]["fields"]["zone"] = Value::from("us-east-2b");
    let moved_path = dir.path().join("moved.json");
    write_record(&moved_path, &moved);
    let moved = Snapshot::load(&moved_path).unwrap();
    let d = ops::compare(&read, &moved, "t1");
    let network = d
        .cluster_tiers
        .iter()
        .find(|t| t.tier.as_str() == "network")
        .unwrap();
    assert!(!network.same);
    assert_eq!(network.diff[0].path, "zone");

    // Palette gives no such tier, so verify does not compare it, and the report says so
    let v = ops::verify(&read, &Palette::new(api), "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged);
    assert!(
        v.diff
            .cluster_tiers
            .iter()
            .all(|t| t.tier.as_str() != "network")
    );
    let text = common::terminal()
        .render(&v.diff, &read, &v.live, false)
        .unwrap();
    assert!(
        text.contains(&format!(
            "a later release of strata wrote the tier network of cluster {}, so strata does not \
             compare it",
            c.name
        )),
        "{text}"
    );
}

#[test]
fn a_behind_report_with_values_of_a_later_release_reads_with_no_error_and_writes_the_same_text() {
    let dir = tempfile::tempdir().unwrap();
    let doc = serde_json::json!({
        "schema": "strata-behind/1",
        "tool": {"name": "strata", "version": "9.0.0", "impl": "go"},
        "generatedAt": "2026-10-13T00:00:00Z",
        "records": ["dev.json"],
        "pins": [{
            "name": "cert-manager", "layer": "addon", "registry": "Public Repo",
            "registryUid": "r1", "version": "1.20.0", "tag": "1.20.0",
            "pinnedBy": [{"path": "dev.json", "subject": "aws-stack@1.0.0"}],
            "newer": [{"version": "1.21.1"}],
            "behind": true,
            "bump": {"to": "1.21.1", "impact": "fleet-wide", "flags": ["IDENTITY", "PROVENANCE"]},
        }],
        "summary": {"pins": 1, "behind": 1, "floating": 0, "errors": 0, "arrived": 0},
    });
    let path = dir.path().join("report.json");
    write_record(&path, &doc);

    let report = ops::BehindReport::load(&path).unwrap();
    assert_eq!(
        report.tool.implementation.as_ref().map(|i| i.as_str()),
        Some("go")
    );
    let bump = report.pins[0].bump.as_ref().unwrap();
    assert_eq!(bump.impact.known(), None);
    assert_eq!(bump.impact.as_str(), "fleet-wide");
    assert_eq!(bump.flags[0], ChangeFlag::Identity);
    assert_eq!(bump.flags[1].as_str(), "PROVENANCE");
    assert_eq!(serde_json::to_value(&report).unwrap(), doc);
}
