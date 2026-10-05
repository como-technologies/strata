//! Library-level tests for the operations, with an in-memory Palette API.

mod common;

use std::fs;
use std::path::Path;

use serde_json::Value;
use strata::api::{MemoryApi, Palette, RawProfile};
use strata::diff::VerifyStatus;
use strata::error::{Error, FailureClass};
use strata::ops;
use strata::progress::Progress;
use strata::reference::Ref;

fn read(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

/// An API that serves the raw files of fixture `name` under profile `uid`.
fn serve(api: MemoryApi, raw_root: &Path, name: &str, uid: &str) -> MemoryApi {
    let d = raw_root.join(name);
    api.with(
        format!("/v1/clusterprofiles/{uid}"),
        read(&d.join("profile.json")),
    )
    .with(
        format!("/v1/clusterprofiles/{uid}/packs/manifests"),
        read(&d.join("manifests.json")),
    )
    .with(
        format!("/v1/clusterprofiles/{uid}/variables"),
        read(&d.join("variables.json")),
    )
}

const UID_A: &str = "64f0000000000000000000a1";

#[test]
fn verify_reports_unchanged_when_the_live_profile_matches_the_record() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let palette = Palette::new(serve(MemoryApi::new(), dir.path(), "a", UID_A));
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
    let r = render::renderer(Format::Terminal, Paint::off(), false);
    let text = r.render(&v.diff, &recorded, &v.live, false).unwrap();
    assert!(!text.contains("edited in place"), "{text}");
    let verdict = r.verdict(ver, &recorded.source).unwrap();
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
    let palette = Palette::new(serve(MemoryApi::new(), dir.path(), "b", UID_A));
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
    let palette =
        Palette::new(MemoryApi::new().failing(format!("/v1/clusterprofiles/{UID_A}"), 500, "boom"));
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
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A)
        .with("/v1/dashboard/clusterprofiles/metadata", listing);
    let by_name = ops::snapshot_for(
        &Ref::NameVersion {
            name: "lab-profile".into(),
            version: "1.2.0".into(),
        },
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
    let by_uid = ops::snapshot_for(
        &Ref::Uid(UID_A.into()),
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(by_name.root, by_uid.root);
    assert_eq!(by_name.source.reference, "lab-profile@1.2.0");

    let file = dir.path().join("a.json");
    fs::write(&file, by_uid.to_json(true).unwrap()).unwrap();
    let from_file = ops::snapshot_for(
        &Ref::Snapshot(file),
        || -> strata::Result<Palette<MemoryApi>> { panic!("must not connect for a snapshot file") },
        "t9",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(from_file.root, by_uid.root);

    let missing = ops::snapshot_for(
        &Ref::NameVersion {
            name: "lab-profile".into(),
            version: "9.9.9".into(),
        },
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap_err();
    assert!(matches!(missing, Error::ProfileNotFound { .. }));
}

const UID_B: &str = "64f0000000000000000000b2";

#[test]
fn name_at_latest_records_the_resolved_version_and_keeps_latest_in_the_ref() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let listing = serde_json::json!({"items": [
        {"metadata": {"name": "lab-profile", "uid": UID_B}, "spec": {"version": "1.3.0"}},
        {"metadata": {"name": "lab-profile", "uid": UID_A}, "spec": {"version": "1.2.0"}},
    ]});
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A);
    let api =
        serve(api, dir.path(), "b", UID_B).with("/v1/dashboard/clusterprofiles/metadata", listing);
    let snap = ops::snapshot_for(
        &Ref::parse_with("lab-profile@latest", |_| false),
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(snap.source.reference, "lab-profile@latest");
    assert_eq!(snap.source.version, "1.3.0");
    assert_eq!(snap.source.uid, UID_B);
}

#[test]
fn raw_profile_from_directory_matches_the_api_path() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let raw = RawProfile::load_dir(&dir.path().join("b")).unwrap();
    let from_raw = strata::snapshot::Snapshot::from_raw(&raw, "b", "t").unwrap();
    let palette = Palette::new(serve(MemoryApi::new(), dir.path(), "b", "x"));
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
use strata::snapshot::Snapshot;

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
    serve(MemoryApi::new(), raw_root, "a", UID_A)
        .failing_n_times(profile_path(), 1, 429, "slow down")
        .with_retry_after(profile_path(), seconds)
}

fn profile_path() -> String {
    format!("/v1/clusterprofiles/{UID_A}")
}

#[test]
fn a_503_twice_then_success_yields_a_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A).failing_n_times(
        profile_path(),
        2,
        503,
        "busy",
    );
    let (retry, slept) = retrying(&api, 3);
    let snap = ops::snapshot_for(
        &Ref::Uid(UID_A.into()),
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

#[test]
fn a_permanent_500_fails_with_the_last_error_after_the_configured_attempts() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A).failing_n_times(
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
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A).failing_n_times(
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
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A).failing_n_times(
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
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A)
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
    let served = serve(MemoryApi::new(), dir.path(), "a", UID_A);
    let with_vars = Palette::new(served.clone())
        .fetch_profile(UID_A, &Progress::silent())
        .unwrap();
    assert!(!with_vars.variables_unavailable);

    let without = served.failing(
        format!("/v1/clusterprofiles/{UID_A}/variables"),
        404,
        "gone",
    );
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&d, &a, &b, false)
        .unwrap();
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
    let served = serve(MemoryApi::new(), dir.path(), "a", UID_A);
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
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A).failing_n_times(
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
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A);
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
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A).failing_n_times(
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

use strata::ops::{RecordStatus, SummaryStatus};

#[test]
fn snapshot_for_records_the_project_scope_of_the_client() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A);
    let scoped = ops::snapshot_for(
        &Ref::Uid(UID_A.into()),
        || Ok(Palette::new(api.clone().in_project("p1"))),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(scoped.source.project_uid, "p1");
    let tenant = ops::snapshot_for(
        &Ref::Uid(UID_A.into()),
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
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
    fs::write(&old, v.to_string()).unwrap();
    assert_eq!(Snapshot::load(&old).unwrap().source.project_uid, "");
}

#[test]
fn verify_many_checks_each_record_in_its_own_scope_and_counts_the_outcomes() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let base = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let write = |name: &str, snap: Snapshot| -> PathBuf {
        let p = dir.path().join(name);
        fs::write(&p, snap.to_json(true).unwrap()).unwrap();
        p
    };
    let rec_p1 = write("p1.json", base.clone().with_project(Some("p1")));
    let rec_p2 = write("p2.json", base.clone().with_project(Some("p2")));
    let not_a_record = dir.path().join("a/profile.json");

    // project p1 still serves A's content; in p2 the live object has B's content, an in-place edit
    let live_p1 = serve(MemoryApi::new(), dir.path(), "a", UID_A);
    let live_p2 = serve(MemoryApi::new(), dir.path(), "b", UID_A);
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
    assert_eq!(s.schema, "strata-verify-summary/1");
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
    assert_eq!(s.summary.status, SummaryStatus::Error);
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
    assert_eq!(s.summary.status, SummaryStatus::Unchanged);
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
    assert_eq!(s.summary.status, SummaryStatus::Drifted);
    assert_eq!(s.exit_code(), 1);
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

    let profile = Palette::new(serve(MemoryApi::new(), dir.path(), "a", UID_A).in_project("p1"));
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
        .fetch_cluster(CLUSTER_UID, &silent)
        .unwrap()
        .save_dir(&saved, "t0")
        .unwrap();
    assert_eq!(project_of(&saved, &format!("cluster:{CLUSTER_UID}")), "p1");

    let tenant = Palette::new(serve(MemoryApi::new(), dir.path(), "a", UID_A));
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
    let before = Palette::new(serve(MemoryApi::new(), dir.path(), "a", UID_A).in_project("p1"));
    let saved = dir.path().join("raw");
    before
        .fetch_profile(UID_A, &Progress::silent())
        .unwrap()
        .save_dir(&saved, "t0")
        .unwrap();
    let record = dir.path().join("rebuilt.json");
    let rebuilt = ops::snapshot_from_dir(&saved, UID_A, "t1").unwrap();
    fs::write(&record, rebuilt.to_json(true).unwrap()).unwrap();

    // after the edit, p1 serves B's content under the same uid; tenant scope does not see it
    let edited = serve(MemoryApi::new(), dir.path(), "b", UID_A).in_project("p1");
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

use strata::ops::{PairStatus, Policy};

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
    assert_eq!(c.schema, "strata-chain/1");
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
    let (from, to) = (raw_root.join("a"), raw_root.join(dir));
    fs::create_dir_all(&to).unwrap();
    let mut profile = read(&from.join("profile.json"));
    for pack in profile["spec"]["published"]["packs"]
        .as_array_mut()
        .unwrap()
    {
        if pack["name"] == "prometheus-operator" {
            pack["type"] = Value::from("manifest");
            pack["registryUid"] = Value::from("");
            pack["packUid"] = Value::from(pack_uid);
            pack.as_object_mut().unwrap().remove("digest");
        }
    }
    let mut manifests = read(&from.join("manifests.json"));
    for pack in manifests["spec"]["packs"].as_array_mut().unwrap() {
        if pack["spec"]["name"] == "prometheus-operator" {
            pack["metadata"]["uid"] = Value::from(pack_uid);
            pack["spec"]["registryUid"] = Value::from("");
            pack["spec"]["digest"] = Value::from("");
            if let Some(content) = rules {
                pack["spec"]["manifests"][0]["content"] = Value::from(content);
            }
        }
    }
    for (name, doc) in [("profile.json", &profile), ("manifests.json", &manifests)] {
        fs::write(to.join(name), serde_json::to_string_pretty(doc).unwrap()).unwrap();
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&d, &one, &two, false)
        .unwrap();
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

/// Fixture A with its `ubuntu-aws` pack changed by `edit` in the profile, saved as the raw
/// directory `dir`; the pack's uid in the manifests listing follows the profile's `packUid`.
fn with_os_pack(raw_root: &Path, dir: &str, edit: impl Fn(&mut Value)) -> Snapshot {
    let (from, to) = (raw_root.join("a"), raw_root.join(dir));
    fs::create_dir_all(&to).unwrap();
    let mut profile = read(&from.join("profile.json"));
    let mut uid = Value::Null;
    for pack in profile["spec"]["published"]["packs"]
        .as_array_mut()
        .unwrap()
    {
        if pack["name"] == "ubuntu-aws" {
            edit(pack);
            uid = pack["packUid"].clone();
        }
    }
    let mut manifests = read(&from.join("manifests.json"));
    for pack in manifests["spec"]["packs"].as_array_mut().unwrap() {
        if pack["spec"]["name"] == "ubuntu-aws" {
            pack["metadata"]["uid"] = uid.clone();
        }
    }
    for (name, doc) in [("profile.json", &profile), ("manifests.json", &manifests)] {
        fs::write(to.join(name), serde_json::to_string_pretty(doc).unwrap()).unwrap();
    }
    fs::copy(from.join("variables.json"), to.join("variables.json")).unwrap();
    ops::snapshot_from_dir(&to, dir, "t0").unwrap()
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

        let text = render::renderer(Format::Terminal, Paint::off(), false)
            .render(&d, &a, &retagged, false)
            .unwrap();
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
        let md = render::renderer(Format::Markdown, Paint::off(), false)
            .render(&d, &a, &retagged, false)
            .unwrap();
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
            let text = render::renderer(Format::Terminal, Paint::off(), false)
                .render_chain(&c)
                .unwrap();
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&d, &a, &moved, false)
        .unwrap();
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
    let palette = Palette::new(serve(MemoryApi::new(), dir.path(), "retagged", UID_A));
    let v = ops::verify(&recorded, &palette, "t1", &Progress::silent()).unwrap();
    assert!(!v.unchanged, "an edit to a published version is drift");
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.status, VerifyStatus::Drifted);
    assert!(v.diff.same_content);
    assert_eq!(v.diff.summary.max_impact, ImpactLevel::None);
    let text = render::renderer(Format::Terminal, Paint::off(), false)
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
    let verdict = render::renderer(Format::Terminal, Paint::off(), false)
        .verdict(ver, &recorded.source)
        .unwrap();
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

use common::{CLUSTER_UID, SECRET_VALUE};

/// An API that serves the cluster fixture `dir` under CLUSTER_UID.
fn serve_cluster(api: MemoryApi, raw_root: &Path, dir: &str) -> MemoryApi {
    let d = raw_root.join(dir);
    // the profile documents of the profiles the cluster runs, as they are published: verify of a
    // cluster record reads them for their current version
    let mut api = api;
    for template in read(&d.join("cluster.json"))["spec"]["clusterProfileTemplates"]
        .as_array()
        .unwrap()
    {
        let uid = template["uid"].as_str().unwrap();
        for fixture in ["a", "b", "addons"] {
            let profile = read(&raw_root.join(fixture).join("profile.json"));
            if profile["metadata"]["uid"] == uid {
                api = api.with(format!("/v1/clusterprofiles/{uid}"), profile);
                break;
            }
        }
    }
    api.with(
        format!("/v1/spectroclusters/{CLUSTER_UID}"),
        read(&d.join("cluster.json")),
    )
    .with(
        format!("/v1/spectroclusters/{CLUSTER_UID}/profiles/packs/manifests"),
        read(&d.join("manifests.json")),
    )
    .with(
        format!("/v1/spectroclusters/{CLUSTER_UID}/variables"),
        read(&d.join("variables.json")),
    )
}

fn cluster_ref(profile: Option<&str>) -> Ref {
    Ref::Cluster {
        uid: CLUSTER_UID.into(),
        profile: profile.map(str::to_string),
        pending: false,
    }
}

fn snapshot_cluster(api: &MemoryApi, profile: Option<&str>) -> strata::Result<Snapshot> {
    ops::snapshot_for(
        &cluster_ref(profile),
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
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
    assert_eq!(on_b.source.template, "applied");
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
        render::renderer(Format::Terminal, Paint::off(), false)
            .render(&d, a, b, false)
            .unwrap()
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
    let md = render::renderer(Format::Markdown, Paint::off(), false)
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
    assert_eq!(api.calls(&format!("/v1/clusterprofiles/{UID_A}")), 1);
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
    let mut renamed = read(&dir.path().join("a/profile.json"));
    renamed["spec"]["version"] = Value::from("1.3.0");
    let v = verify(
        serve_cluster(MemoryApi::new(), dir.path(), "cluster-a")
            .with(format!("/v1/clusterprofiles/{UID_A}"), renamed),
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
    let verdict = render::renderer(Format::Terminal, Paint::off(), false)
        .verdict(ver, &recorded.source)
        .unwrap();
    assert_eq!(
        verdict,
        "DRIFT: cluster lab-cluster runs lab-profile@1.2.0, but uid 64f0000000000000000000a1 now names 1.3.0: the version was overwritten in place"
    );

    // the profile was deleted
    let v = verify(
        MemoryApi::new()
            .with(
                format!("/v1/spectroclusters/{CLUSTER_UID}"),
                read(&dir.path().join("cluster-a/cluster.json")),
            )
            .with(
                format!("/v1/spectroclusters/{CLUSTER_UID}/profiles/packs/manifests"),
                read(&dir.path().join("cluster-a/manifests.json")),
            )
            .with(
                format!("/v1/spectroclusters/{CLUSTER_UID}/variables"),
                read(&dir.path().join("cluster-a/variables.json")),
            ),
    );
    assert!(!v.unchanged);
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.profiles_overwritten.len(), 1);
    assert_eq!(
        ver.profiles_overwritten[0].profile_version, "",
        "no version: the uid is gone"
    );
    let verdict = render::renderer(Format::Terminal, Paint::off(), false)
        .verdict(ver, &recorded.source)
        .unwrap();
    assert!(
        verdict.ends_with("but uid 64f0000000000000000000a1 no longer exists"),
        "{verdict}"
    );

    // any other error reading a profile stops verify rather than guessing
    let err = ops::verify(
        &recorded,
        &Palette::new(
            serve_cluster(MemoryApi::new(), dir.path(), "cluster-a").failing(
                format!("/v1/clusterprofiles/{UID_A}"),
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
            format!("/v1/spectroclusters/{CLUSTER_UID}"),
            read(&d.join("cluster.json")),
        )
        .with(
            format!("/v1/spectroclusters/{CLUSTER_UID}/profiles/packs/manifests"),
            read(&d.join("manifests.json")),
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
    format!("/v1/spectroclusters/{CLUSTER_UID}/config/rbacs")
}

/// The route of the namespaces of the fixture cluster.
fn namespaces_route() -> String {
    format!("/v1/spectroclusters/{CLUSTER_UID}/config/namespaces")
}

/// Cluster b with the five settings of the policy tier, and one role binding to `subjects` and
/// one namespace on their own routes, as Palette gives them.
fn serve_policy(dir: &Path, subjects: Value) -> MemoryApi {
    let mut cluster = read(&dir.join("cluster-b").join("cluster.json"));
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
        .with(format!("/v1/spectroclusters/{CLUSTER_UID}"), cluster)
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
    s.source.cluster.as_ref().unwrap().tiers["policy"]
        .fields
        .clone()
}

/// The comparison of the policy tiers in `d`.
fn policy_diff(d: &strata::diff::Diff) -> &strata::diff::TierDiff {
    d.cluster_tiers.iter().find(|t| t.tier == "policy").unwrap()
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
        .insert("policy".to_string(), serde_json::from_value(tier).unwrap());
    let saved = dir.path().join("older.json");
    fs::write(&saved, older.to_json(true).unwrap()).unwrap();
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
fn a_404_of_the_route_of_the_bindings_records_null_and_another_error_is_a_field_of_the_tier() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    // the route of the role bindings answers 404, and the cluster has no namespace
    let mut cluster = read(&dir.path().join("cluster-b").join("cluster.json"));
    cluster["spec"]["clusterConfig"] = serde_json::json!({"timezone": "Etc/UTC"});
    let no_route = serve_cluster(MemoryApi::new(), dir.path(), "cluster-b")
        .with(format!("/v1/spectroclusters/{CLUSTER_UID}"), cluster)
        .with(namespaces_route(), serde_json::json!({"items": []}));
    let p = policy_of(&snapshot_cluster(&no_route, None).unwrap());
    assert_eq!(p["rbacs"], Value::Null, "not read, and not an empty list");
    assert_eq!(p["namespaces"], serde_json::json!([]));

    // a 403 of the route of the role bindings does not stop the record
    let forbidden = serve_policy(dir.path(), barney()).failing(rbacs_route(), 403, "forbidden\n");
    let s = snapshot_cluster(&forbidden, None).unwrap();
    let tier = &s.source.cluster.as_ref().unwrap().tiers["policy"];
    assert_eq!(tier.fields["rbacs"], Value::Null);
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
    // the error is not part of the hash: a 404 gives the same fields, and no error
    let missing = serve_policy(dir.path(), barney()).failing(rbacs_route(), 404, "not found");
    let m = snapshot_cluster(&missing, None).unwrap();
    let missing_tier = &m.source.cluster.as_ref().unwrap().tiers["policy"];
    assert_eq!(missing_tier.hash, tier.hash);
    assert!(missing_tier.errors.is_empty());
    assert!(
        serde_json::to_value(&m).unwrap()["source"]["cluster"]["tiers"]["policy"]
            .get("errors")
            .is_none()
    );

    // the same for the route of the namespaces
    let forbidden =
        serve_policy(dir.path(), barney()).failing(namespaces_route(), 403, "forbidden");
    let s = snapshot_cluster(&forbidden, None).unwrap();
    let tier = &s.source.cluster.as_ref().unwrap().tiers["policy"];
    assert_eq!(tier.fields["namespaces"], Value::Null);
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&v.diff, &recorded, &v.live, false)
        .unwrap();
    assert!(text.contains(&format!("note       {note}\n")), "{text}");
    assert_eq!(text.matches(&note).count(), 1, "{text}");

    // compare of two records that both could not read the field gives one note
    let live = snapshot_cluster(&forbidden, None).unwrap();
    let d = ops::compare(&live, &live, "t1");
    assert!(policy_diff(&d).same);
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render(&d, &live, &live, false)
        .unwrap();
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
        .fetch_cluster(CLUSTER_UID, &silent)
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

use strata::color::Paint;
use strata::render::{self, Format};

/// Profile b's API, with `inUseClusters` naming one of its two consumers the way Palette does.
fn serve_b_with_named_consumer(dir: &Path) -> MemoryApi {
    let mut profile = read(&dir.join("b").join("profile.json"));
    profile["status"]["inUseClusters"] = serde_json::json!([
        {"uid": "cluster-aaa", "name": "alpha", "projectUid": "p1", "kind": "spectrocluster"}
    ]);
    serve(MemoryApi::new(), dir, "b", UID_B).with(format!("/v1/clusterprofiles/{UID_B}"), profile)
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
        "/v1/spectroclusters/cluster-aaa",
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
    assert!(text.contains("project p1"), "{text}");

    // with one, it names the project and keeps the uid
    let named = api.with(
        "/v1/dashboard/projects/metadata",
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
    assert!(text.contains("project dev (uid p1)"), "{text}");
}

#[test]
fn a_consumer_with_no_project_is_read_in_the_scope_the_profile_was_read_in() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());

    // the profile of project p1, with one consumer that Palette names no project for
    let mut profile = read(&dir.path().join("b").join("profile.json"));
    profile["status"]["inUseClusterUids"] = serde_json::json!(["cluster-aaa"]);
    profile["status"]["inUseClusters"] =
        serde_json::json!([{"uid": "cluster-aaa", "name": "alpha", "projectUid": ""}]);
    let in_p1 = serve(MemoryApi::new(), dir.path(), "b", UID_B)
        .with(format!("/v1/clusterprofiles/{UID_B}"), profile)
        .with(
            "/v1/spectroclusters/cluster-aaa",
            cluster_doc("cluster-aaa", "alpha", "", "Running"),
        );

    // a record of that profile, taken in p1
    let record = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0")
        .unwrap()
        .with_project(Some("p1"));
    let path = dir.path().join("record.json");
    std::fs::write(&path, record.to_json(false).unwrap()).unwrap();

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
        "/v1/spectroclusters/cluster-aaa",
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
    assert_eq!(r.schema, "strata-consumers/1");
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

    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
    assert!(
        text.starts_with("lab-profile@1.3.0 (uid 64f0000000000000000000b2): 2 consumers\n"),
        "{text}"
    );
    assert!(text.contains("alpha"), "{text}");
    assert!(text.contains("p1") && text.contains("Running"), "{text}");
    assert!(
        text.contains("cluster-bbb") && text.contains("unknown"),
        "{text}"
    );
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
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
    let mut profile = read(&dir.path().join("b").join("profile.json"));
    profile["status"]["inUseClusters"] = serde_json::json!([
        {"uid": "cluster-aaa", "name": "alpha", "projectUid": "p1"},
        {"uid": "cluster-bbb", "name": "beta", "projectUid": "p1"}
    ]);
    let tenant = serve(MemoryApi::new(), dir.path(), "b", UID_B)
        .with(format!("/v1/clusterprofiles/{UID_B}"), profile);
    // alpha runs the version the uid names; beta's copy still names 1.2.9, from before a legacy
    // version bump renamed the uid to 1.3.0
    let runs = |uid: &str, name: &str, version: &str| {
        let mut d = cluster_doc(uid, name, "p1", "Running");
        d["spec"] = serde_json::json!({"clusterProfileTemplates": [
            {"uid": "64f0000000000000000000d4", "name": "lab-addons", "profileVersion": "2.0.0"},
            {"uid": UID_B, "name": "lab-profile", "profileVersion": version}
        ]});
        d
    };
    let in_p1 = MemoryApi::new()
        .with(
            "/v1/spectroclusters/cluster-aaa",
            runs("cluster-aaa", "alpha", "1.3.0"),
        )
        .with(
            "/v1/spectroclusters/cluster-bbb",
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

    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
    let beta = text.lines().find(|l| l.contains("beta")).unwrap();
    assert!(
        beta.contains("runs lab-profile@1.2.9, which uid 64f0000000000000000000b2 no longer names"),
        "{text}"
    );
    let alpha = text.lines().find(|l| l.contains("alpha")).unwrap();
    assert!(!alpha.contains("no longer names"), "{text}");
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
    assert!(
        md.contains("`1.2.9`, which the uid no longer names"),
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
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A)
        .with(
            "/v1/dashboard/clusterprofiles/metadata",
            serde_json::json!({"items": [
                {"metadata": {"name": "lab-profile", "uid": UID_A}, "spec": {"version": "1.2.0"}}
            ]}),
        )
        // the second reading lists the clusters, and this tenant has none
        .with("/v1/dashboard/spectroclusters/meta", serde_json::json!([]));
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
    assert_eq!(
        text,
        "lab-profile@1.2.0 (uid 64f0000000000000000000a1): no consumers\n  second reading: 0 of 0 clusters read\n  edge hosts: 0 read\n"
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
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A)
        .with(
            "/v1/dashboard/spectroclusters/meta",
            serde_json::json!([{
                "metadata": {"name": "gamma", "uid": "cluster-ccc", "annotations": {"projectUid": "p1"}},
                "status": {"state": "Running"}
            }]),
        )
        .with("/v1/spectroclusters/cluster-ccc", applies);
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

    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
    assert!(
        text.contains("not in the consumer list of the profile"),
        "{text}"
    );
    assert!(
        text.ends_with("  second reading: 1 of 1 clusters read\n  edge hosts: 0 read\n"),
        "{text}"
    );
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
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

const CLUSTER_SEARCH: &str = "/v1/dashboard/spectroclusters/search?limit=50";

/// The body of the cluster search for the clusters that run a version of the profile `name`, and
/// with `nested` for the virtual clusters that run it, as the second reading of `consumers` sends
/// it. Palette takes the name of a profile in `clusterProfiles`, and not its uid (#181).
fn profile_search(name: &str, nested: bool) -> Value {
    let condition = |property: &str, value: &str| {
        serde_json::json!({
            "property": property, "type": "string",
            "condition": {"string": {"operator": "eq", "negation": false, "ignoreCase": false,
                "match": {"conjunction": "or", "values": [value]}}},
        })
    };
    let mut filters = vec![condition("clusterProfiles", name)];
    if nested {
        filters.push(condition("environment", "nested"));
    }
    serde_json::json!({
        "filter": {"conjunction": "and",
                   "filterGroups": [{"conjunction": "and", "filters": filters}]},
        "sort": [{"field": "clusterName", "order": "asc"}],
    })
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
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A)
        .with(
            "/v1/dashboard/spectroclusters/search/schema",
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
        .with(
            "/v1/spectroclusters/cluster-ccc",
            runs("cluster-ccc", "gamma"),
        )
        .with(
            "/v1/spectroclusters/cluster-vvv",
            runs("cluster-vvv", "vc-1"),
        );
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
    assert_eq!(api.calls("/v1/spectroclusters/cluster-ccc"), 1);
    assert_eq!(api.calls("/v1/spectroclusters/cluster-vvv"), 1);
    assert_eq!(api.calls("/v1/dashboard/spectroclusters/meta"), 0);

    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
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

/// The key under which the memory API keeps the POST of the virtual clusters of the group `g1`:
/// the path and the body.
static MEMBERS_POST: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    format!(
        "POST /v1/dashboard/clustergroups/g1/virtualClusters?limit=50 {}",
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
        "/v1/dashboard/spectroclusters/search/schema",
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
        "/v1/clustergroups/hostCluster/metadata",
        serde_json::json!({"items": [{"uid": "g1", "name": "dev-group"}]}),
    )
    .with(
        "/v1/clustergroups/g1",
        serde_json::json!({"metadata": {"uid": "g1", "name": "dev-group"},
            "spec": {"clusterProfileTemplates": [{"uid": UID_A, "name": "lab-profile"}]}}),
    )
    .with_post(
        "/v1/dashboard/clustergroups/g1/virtualClusters?limit=50",
        &serde_json::json!({"filter": {}, "sort": []}),
        serde_json::json!({"items": [{"metadata": {"uid": "vc-1", "name": "vc-1",
            "annotations": {"projectUid": "p1"}}}], "listmeta": {"continue": ""}}),
    )
    .with("/v1/spectroclusters/vc-1", vc)
}

#[test]
fn the_second_reading_finds_a_virtual_cluster_through_the_group_that_pins_the_version() {
    // the consumer list is empty, and the search names no cluster; the group g1 pins the version,
    // and its virtual cluster vc-1 runs it (#184)
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = with_pinning_group(serve(MemoryApi::new(), dir.path(), "a", UID_A));
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
    assert_eq!(api.calls("/v1/spectroclusters/vc-1"), 1);
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
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
    let mut profile = read(&dir.path().join("a").join("profile.json"));
    profile["status"]["inUseClusterUids"] = serde_json::json!(["vc-1"]);
    profile["status"]["inUseClusters"] =
        serde_json::json!([{"uid": "vc-1", "name": "vc-1", "projectUid": "p1"}]);
    let api = with_pinning_group(
        serve(MemoryApi::new(), dir.path(), "a", UID_A)
            .with(format!("/v1/clusterprofiles/{UID_A}"), profile),
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
    assert_eq!(api.calls("/v1/spectroclusters/vc-1"), 1);
}

#[test]
fn the_line_of_the_second_reading_counts_a_virtual_cluster_that_strata_cannot_read_apart() {
    // the search names c-9, which strata reads; the group g1 names vc-1, whose document answers
    // 500 (from the verify of 6d21758)
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let mut c9 = cluster_doc("c-9", "c-9", "p1", "Running");
    c9["spec"] = serde_json::json!({"clusterProfileTemplates": []});
    let api = with_pinning_group(serve(MemoryApi::new(), dir.path(), "a", UID_A))
        .with_post(
            CLUSTER_SEARCH,
            &profile_search("lab-profile", false),
            serde_json::json!({"items": [{"metadata": {"uid": "c-9", "name": "c-9",
                "annotations": {"projectUid": "p1"}}}], "listmeta": {"continue": ""}}),
        )
        .with("/v1/spectroclusters/c-9", c9)
        .failing("/v1/spectroclusters/vc-1", 500, "boom");
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
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
    let tenant = with_pinning_group(serve(MemoryApi::new(), dir.path(), "a", UID_A))
        .failing("/v1/clustergroups/g1", 403, "forbidden")
        .failing(MEMBERS_POST.as_str(), 403, "forbidden");
    let home = with_pinning_group(serve(MemoryApi::new(), dir.path(), "a", UID_A));
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
    assert_eq!(tenant.calls("/v1/clustergroups/g1"), 1);
    assert_eq!(home.calls("/v1/clustergroups/g1"), 1);
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
    let mut profile = read(&dir.path().join("a").join("profile.json"));
    profile["metadata"]["annotations"] =
        serde_json::json!({"scope": "project", "projectUid": "p9"});
    let mut doc = cluster_doc("cluster-ccc", "gamma", "p9", "Running");
    doc["spec"] = serde_json::json!({"clusterProfileTemplates": [
        {"uid": UID_A, "name": "lab-profile", "profileVersion": "1.2.0", "type": "cluster"}
    ]});
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A)
        .with(format!("/v1/clusterprofiles/{UID_A}"), profile)
        .with(
            "/v1/dashboard/spectroclusters/search/schema",
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
        .with("/v1/spectroclusters/cluster-ccc", doc);
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
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A)
        .with(
            "/v1/dashboard/spectroclusters/search/schema",
            serde_json::json!({"schema": {"properties": [{"name": "tags", "type": "string"}]}}),
        )
        .with(
            "/v1/dashboard/spectroclusters/meta",
            serde_json::json!([
                {"metadata": {"name": "gamma", "uid": "cluster-ccc",
                              "annotations": {"projectUid": "p1"}},
                 "status": {"state": "Running"}},
                {"metadata": {"name": "delta", "uid": "cluster-ddd",
                              "annotations": {"projectUid": "p1"}},
                 "status": {"state": "Running"}},
            ]),
        )
        .with("/v1/spectroclusters/cluster-ccc", applies)
        .with(
            "/v1/spectroclusters/cluster-ddd",
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
    assert!(api.calls("/v1/dashboard/spectroclusters/meta") >= 1);
    assert_eq!(api.calls("/v1/spectroclusters/cluster-ddd"), 1);
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
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
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A).with(
        "/v1/dashboard/spectroclusters/meta",
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
    assert!(
        text.ends_with("  second reading: not done (--list-only)\n  edge hosts: 0 read\n"),
        "{text}"
    );
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
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
}

#[test]
fn consumers_takes_a_profile_and_refuses_a_cluster() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let never = |_: Option<&str>| -> strata::Result<Palette<MemoryApi>> {
        panic!("a cluster ref is refused before connecting")
    };
    let err = ops::consumers(&cluster_ref(None), never, "t1", &Progress::silent()).unwrap_err();
    assert!(matches!(err, Error::ProfileRefRequired { .. }), "{err}");
    assert_eq!(err.kind(), "profile-ref-required");

    // a record of a cluster is refused the same way; a profile record uses its uid, fetched live
    let on_b = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-b"),
        None,
    )
    .unwrap();
    let file = dir.path().join("on-b.json");
    fs::write(&file, on_b.to_json(true).unwrap()).unwrap();
    let err = ops::consumers(&Ref::Snapshot(file), never, "t1", &Progress::silent()).unwrap_err();
    assert!(matches!(err, Error::ProfileRefRequired { .. }), "{err}");

    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let file = dir.path().join("a.json");
    fs::write(&file, recorded.to_json(true).unwrap()).unwrap();
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A);
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
            "/v1/dashboard/clusterprofiles/metadata",
            serde_json::json!({"items": [
                listed_profile("lab-profile", "1.10.0", "u3"),
                listed_profile("addons", "2.0.0", "u9"),
                listed_profile("lab-profile", "1.9.0", "u2"),
            ]}),
        )
        .with(
            "/v1/dashboard/spectroclusters/meta",
            serde_json::json!([
                listed_cluster("workload-b", "c2", "Pending"),
                listed_cluster("workload-a", "c1", "Running"),
            ]),
        )
        .with(
            "/v1/dashboard/projects/metadata",
            projects(&[("lab", "p1")]),
        );
    let scopes: RefCell<Vec<Option<String>>> = RefCell::new(Vec::new());
    let connect = |scope: Option<&str>| {
        scopes.borrow_mut().push(scope.map(str::to_string));
        Ok(Palette::new(api.clone()))
    };
    let l = ops::list(&ListOptions::default(), connect, "t1", &Progress::silent()).unwrap();
    assert_eq!(l.schema, "strata-list/1");
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
        api.calls("/v1/clusterprofiles/u2"),
        0,
        "no profile document without --packs"
    );

    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_list(&l)
        .unwrap();
    assert!(text.starts_with("project lab (uid p1)\n"), "{text}");
    assert!(text.contains("\n3 profiles\n"), "{text}");
    assert!(text.contains("  lab-profile@1.10.0  u3  aws\n"), "{text}");
    assert!(
        text.contains("  cluster:c1  workload-a  Running  aws\n"),
        "{text}"
    );
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render_list(&l)
        .unwrap();
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

#[test]
fn list_with_a_pack_reads_each_profile_and_keeps_those_that_carry_it() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve(
        serve(MemoryApi::new(), dir.path(), "a", UID_A),
        dir.path(),
        "b",
        UID_B,
    )
    .with(
        "/v1/dashboard/clusterprofiles/metadata",
        serde_json::json!({"items": [
            listed_profile("lab-profile", "1.3.0", UID_B),
            listed_profile("lab-profile", "1.2.0", UID_A),
            listed_profile("hidden", "1.0.0", "u-denied"),
        ]}),
    )
    .failing("/v1/clusterprofiles/u-denied", 403, "forbidden");
    let options = ListOptions {
        pack: Some("CERT-man".into()),
        ..ListOptions::default()
    };
    let l = ops::list(
        &options,
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(
        l.clusters.is_none(),
        "--pack lists profiles only unless kinds are named"
    );
    assert_eq!(api.calls("/v1/dashboard/spectroclusters/meta"), 0);
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
        (b.profile_type.as_deref(), b.consumers),
        (Some("cluster"), Some(2))
    );
    let packs = b.packs.as_ref().unwrap();
    assert_eq!(packs.len(), 7, "every layer is shown, not only the match");
    let cm = packs.iter().find(|p| p.name == "cert-manager").unwrap();
    assert_eq!(
        (cm.layer.as_str(), cm.version.as_str(), cm.tag.as_str()),
        ("addon", "1.14.0", "1.14.0")
    );

    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_list(&l)
        .unwrap();
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
        text.contains("    addon  cert-manager         1.14.0\n"),
        "{text}"
    );
    assert!(text.contains("forbidden"), "the error is shown: {text}");
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render_list(&l)
        .unwrap();
    assert!(md.contains("addon/cert-manager 1.14.0"), "{md}");
}

#[test]
fn list_projects_reads_at_tenant_scope_and_a_project_override_scopes_the_rest() {
    let in_p2 = MemoryApi::new()
        .in_project("p2")
        .with(
            "/v1/dashboard/clusterprofiles/metadata",
            serde_json::json!({"items": [listed_profile("edge", "1.0.0", "u1")]}),
        )
        .with(
            "/v1/dashboard/projects/metadata",
            projects(&[("prod", "p2")]),
        );
    let tenant = MemoryApi::new().with(
        "/v1/dashboard/projects/metadata",
        projects(&[("prod", "p2"), ("dev", "p1")]),
    );
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
        in_p2.calls("/v1/dashboard/projects/metadata"),
        0,
        "the tenant listing already names the project"
    );
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_list(&l)
        .unwrap();
    assert!(
        text.contains("\n2 projects\n  dev   p1\n  prod  p2\n"),
        "{text}"
    );
}

#[test]
fn list_at_tenant_scope_names_the_project_of_each_cluster() {
    let mut other = listed_cluster("edge-1", "c7", "Running");
    other["metadata"]["annotations"]["projectUid"] = serde_json::json!("p2");
    let api = MemoryApi::new().with(
        "/v1/dashboard/spectroclusters/meta",
        serde_json::json!([listed_cluster("workload-a", "c1", "Running"), other]),
    );
    let options = ListOptions {
        kinds: vec![ListKind::Clusters],
        ..ListOptions::default()
    };
    let l = ops::list(
        &options,
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    let clusters = l.clusters.as_ref().unwrap();
    assert_eq!(
        clusters
            .iter()
            .map(|c| c.project_uid.as_str())
            .collect::<Vec<_>>(),
        ["p2", "p1"],
        "a cluster ref works only in its own project, so each carries it"
    );
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_list(&l)
        .unwrap();
    assert!(
        text.contains("  cluster:c7  edge-1      Running  aws  project p2\n"),
        "{text}"
    );
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render_list(&l)
        .unwrap();
    assert!(
        md.contains("| `cluster:c1` | workload-a | Running | aws | p1 |"),
        "{md}"
    );

    // With the project listing, each line names the project. A uid names no project to a reader,
    // and the listing is one request that `list projects` already makes.
    let named = api.clone().with(
        "/v1/dashboard/projects/metadata",
        serde_json::json!({"items": [
            {"metadata": {"name": "dev", "uid": "p1"}},
            {"metadata": {"name": "prod", "uid": "p2"}},
        ]}),
    );
    let l = ops::list(
        &options,
        |_| Ok(Palette::new(named.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_list(&l)
        .unwrap();
    assert!(
        text.contains("project prod (uid p2)"),
        "the line names the project: {text}"
    );
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
            "/v1/dashboard/spectroclusters/meta",
            serde_json::json!([
                listed_cluster("workload-b", "c2", "Running"),
                listed_cluster("workload-a", "c1", "Running"),
            ]),
        )
        .with(
            "/v1/spectroclusters/c1",
            cluster_document(
                "c1",
                &[
                    (
                        "lab-profile",
                        "1.3.0",
                        "64f0000000000000000000b2",
                        "cluster",
                    ),
                    ("lab-addons", "2.0.0", "64f0000000000000000000d4", "add-on"),
                ],
            ),
        )
        .with(
            "/v1/spectroclusters/c2",
            cluster_document(
                "c2",
                &[(
                    "lab-profile",
                    "1.2.0",
                    "64f0000000000000000000a1",
                    "cluster",
                )],
            ),
        )
        // an update of the add-on waits for c1; c2 serves no notifications, so it is unknown
        .with(
            "/v1/notifications/spectrocluster/c1",
            notifications_page(
                vec![notification(
                    "n1",
                    "2026-09-24T23:55:02.083Z",
                    false,
                    &[("64f0000000000000000000d4", "m")],
                )],
                "",
            ),
        );
    let list = |options: &ListOptions| {
        ops::list(
            options,
            |_| Ok(Palette::new(&api)),
            "t1",
            &Progress::silent(),
        )
        .unwrap()
    };
    let clusters = ListOptions {
        kinds: vec![ListKind::Clusters],
        ..ListOptions::default()
    };

    // the option is what costs the requests: without it, the listing alone answers
    let plain = list(&clusters);
    assert_eq!(api.calls("/v1/spectroclusters/c1"), 0);
    assert!(plain.clusters.as_ref().unwrap()[0].profiles.is_none());

    let l = list(&ListOptions {
        applied: true,
        ..clusters.clone()
    });
    assert_eq!(api.calls("/v1/spectroclusters/c1"), 1);
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
    assert_eq!(applied[0].uid, "64f0000000000000000000b2");

    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_list(&l)
        .unwrap();
    assert!(
        text.contains(
            "  cluster:c1  workload-a  Running  aws
    lab-profile@1.3.0  64f0000000000000000000b2  cluster
    lab-addons@2.0.0   64f0000000000000000000d4  add-on  behind: an update waits
  cluster:c2  workload-b  Running  aws
    lab-profile@1.2.0  64f0000000000000000000a1  cluster
"
        ),
        "{text}"
    );
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render_list(&l)
        .unwrap();
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
        "/v1/dashboard/spectroclusters/meta",
        serde_json::json!([
            listed_cluster("workload-a", "c1", "Running"),
            elsewhere,
            gone
        ]),
    );
    let p1 = listing.clone().with(
        "/v1/spectroclusters/c1",
        cluster_document(
            "c1",
            &[(
                "lab-profile",
                "1.3.0",
                "64f0000000000000000000b2",
                "cluster",
            )],
        ),
    );
    let p2 = MemoryApi::new()
        .with(
            "/v1/spectroclusters/c7",
            cluster_document(
                "c7",
                &[(
                    "lab-profile",
                    "1.2.0",
                    "64f0000000000000000000a1",
                    "cluster",
                )],
            ),
        )
        .failing("/v1/spectroclusters/c9", 404, "no such cluster");
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_list(&l)
        .unwrap();
    assert!(
        text.contains("gone"),
        "a cluster that cannot be read keeps its row: {text}"
    );
    assert!(text.contains("lab-profile@1.3.0"), "{text}");
}

#[test]
fn list_with_a_cluster_text_keeps_the_clusters_that_match_and_reads_only_those() {
    let mut elsewhere = listed_cluster("payments", "c7", "Running");
    elsewhere["metadata"]["annotations"]["projectUid"] = serde_json::json!("p2");
    let api = MemoryApi::new()
        .in_project("p1")
        .with(
            "/v1/dashboard/spectroclusters/meta",
            serde_json::json!([
                listed_cluster("payments", "c1", "Running"),
                listed_cluster("search", "c2", "Running"),
                elsewhere,
            ]),
        )
        .with(
            "/v1/spectroclusters/c1",
            cluster_document(
                "c1",
                &[(
                    "lab-profile",
                    "1.3.0",
                    "64f0000000000000000000b2",
                    "cluster",
                )],
            ),
        )
        .with(
            "/v1/spectroclusters/c2",
            cluster_document(
                "c2",
                &[(
                    "lab-profile",
                    "1.2.0",
                    "64f0000000000000000000a1",
                    "cluster",
                )],
            ),
        )
        .with(
            "/v1/spectroclusters/c7",
            cluster_document(
                "c7",
                &[(
                    "lab-profile",
                    "1.3.0",
                    "64f0000000000000000000b2",
                    "cluster",
                )],
            ),
        );
    let list = |text: &str| {
        ops::list(
            &ListOptions {
                project: Some(String::new()),
                cluster: Some(text.to_string()),
                ..ListOptions::default()
            },
            |_| Ok(Palette::new(&api)),
            "t1",
            &Progress::silent(),
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
        api.calls("/v1/spectroclusters/c2"),
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

    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_list(&list("ledger"))
        .unwrap();
    assert!(text.contains("no clusters matching 'ledger'"), "{text}");
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_list(&l)
        .unwrap();
    assert!(text.contains("2 clusters matching 'payments'"), "{text}");
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render_list(&l)
        .unwrap();
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
    let api = MemoryApi::new().failing("/v1/dashboard/clusterprofiles/metadata", 401, "no key");
    let err = ops::list(
        &ListOptions::default(),
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap_err();
    assert_eq!(err.kind(), "api", "{err}");
}

// ---- candidates ------------------------------------------------------------------------------

/// The registry endpoints strata reads for one pack, as the examples cached them from a live
/// tenant: the version listing filtered by name, and the pack entity with its tags.
fn serve_registry_pack(
    api: MemoryApi,
    name: &str,
    summaries: Value,
    entity: Value,
    registry: &str,
) -> MemoryApi {
    let count = summaries.as_array().map(Vec::len).unwrap_or(0);
    api.with(
        format!("/v1/packs?filters=spec.name={name}&limit=100&offset=0"),
        serde_json::json!({"items": summaries, "listmeta": {"count": count}}),
    )
    .with(format!("/v1/packs/{name}/registries/{registry}"), entity)
}

fn write_yaml(path: &Path, doc: &Value) {
    fs::write(path, serde_yaml::to_string(doc).unwrap()).unwrap();
}

fn never_connect() -> strata::Result<Palette<MemoryApi>> {
    panic!("local errors are found before connecting")
}

/// The examples that `just examples` builds, built into a temporary directory from the fetched
/// pack data the repository tracks, with this build of strata.
fn built_examples() -> tempfile::TempDir {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = tempfile::tempdir().unwrap();
    for entry in fs::read_dir(repo.join("examples/packs")).unwrap() {
        let pack = entry.unwrap().path();
        let to = dir.path().join("packs").join(pack.file_name().unwrap());
        fs::create_dir_all(&to).unwrap();
        for file in ["_entity.json", "_summaries.json"] {
            fs::copy(pack.join(file), to.join(file)).unwrap();
        }
    }
    let o = std::process::Command::new("bash")
        .arg(repo.join("examples/fetch-examples.sh"))
        .env("EXAMPLES_DIR", dir.path())
        .env("OFFLINE", "1")
        .env("STRATA_BIN", env!("CARGO_BIN_EXE_strata"))
        .output()
        .unwrap();
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
        let profile = read(&raw.join("profile.json"));
        let manifests = read(&raw.join("manifests.json"));
        let registry = profile["spec"]["published"]["packs"][0]["registryUid"]
            .as_str()
            .unwrap()
            .to_string();

        let mut api = MemoryApi::new().with(
            "/v1/registries/metadata",
            serde_json::json!({"items": [{"name": "Public Repo", "uid": registry, "kind": "pack"}]}),
        );
        let mut layers = Vec::new();
        for pack in profile["spec"]["published"]["packs"].as_array().unwrap() {
            let name = pack["name"].as_str().unwrap();
            let d = examples.join("packs").join(name);
            api = serve_registry_pack(
                api,
                name,
                read(&d.join("_summaries.json")),
                read(&d.join("_entity.json")),
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
            "variables": read(&raw.join("variables.json"))["variables"],
        });
        let file = dir.path().join(format!("aws-stack@{version}.yaml"));
        write_yaml(&file, &candidate);

        let snap = ops::snapshot_from_candidate(
            &file,
            false,
            || Ok(Palette::new(api.clone())),
            "t0",
            &Progress::silent(),
        )
        .unwrap();
        let published = read(
            &examples
                .join("profiles")
                .join(format!("aws-stack@{version}.json")),
        );
        assert_eq!(
            snap.root,
            published["root"].as_str().unwrap(),
            "aws-stack@{version}: the candidate is the release that gets published"
        );
        assert_eq!(snap.source.template, "candidate");
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
    let profile = read(&dir.join("b").join("profile.json"));
    let manifests = read(&dir.join("b").join("manifests.json"));
    let mut api = MemoryApi::new();
    for pack in profile["spec"]["published"]["packs"].as_array().unwrap() {
        let name = pack["name"].as_str().unwrap();
        let entry = manifests["spec"]["packs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["spec"]["name"] == name)
            .unwrap();
        let summary = serde_json::json!({
            "metadata": {"uid": pack["packUid"]},
            "spec": {"name": name, "version": pack["version"], "registryUid": "reg-public",
                     "digest": entry["spec"]["digest"], "layer": pack["layer"], "type": "spectro",
                     "values": "registry: defaults"},
        });
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
    let profile = read(&dir.path().join("b").join("profile.json"));
    let manifests = read(&dir.path().join("b").join("manifests.json"));

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
            "variables": read(&dir.path().join("b").join("variables.json"))["variables"],
        }),
    );
    let api = serve_registry_for_b(dir.path());
    let snap = ops::snapshot_from_candidate(
        &file,
        false,
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
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

    // a layer that leaves values out gets the registry defaults, which differ from b's
    let mut doc: Value = serde_yaml::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    doc["layers"][4].as_object_mut().unwrap().remove("values");
    let defaults = cand.join("defaults.yaml");
    write_yaml(&defaults, &doc);
    let snap = ops::snapshot_from_candidate(
        &defaults,
        false,
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
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
    let b = read(&dir.path().join("b").join("profile.json"));
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

    // registry problems need the registry
    let f = write(
        "unknown-version.yaml",
        k8s(serde_json::json!({"version": "9.9.9"})),
    );
    let err = ops::snapshot_from_candidate(
        &f,
        false,
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap_err();
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
    let err = ops::snapshot_from_candidate(
        &f,
        false,
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::PackNotFound { .. }), "{err}");
    assert!(err.to_string().contains("2.x"), "{err}");

    let named = api.clone().with(
        "/v1/registries/metadata",
        serde_json::json!({"items": [{"name": "Public Repo", "uid": "reg-public"}, {"name": "Internal", "uid": "reg-int"}]}),
    );
    let f = write(
        "unknown-registry.yaml",
        serde_json::json!({"name": "p", "version": "1", "registry": "Nope", "layers": [{"name": "kubernetes", "version": "1.30.2"}]}),
    );
    let err = ops::snapshot_from_candidate(
        &f,
        false,
        || Ok(Palette::new(named.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::RegistryNotFound { .. }), "{err}");
    assert!(err.to_string().contains("Public Repo, Internal"), "{err}");

    // an explicit tag is used when the pack version has it
    let f = write(
        "floating.yaml",
        k8s(serde_json::json!({"tag": "floating.x"})),
    );
    let snap = ops::snapshot_from_candidate(
        &f,
        false,
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(snap.packs[0].tag, "floating.x");
}

#[test]
fn an_explicit_tag_equal_to_the_version_is_accepted_as_palette_does() {
    // Palette stores the tag a profile was authored with. A profile built with Terraform pins the
    // version itself, `24.04`, although the registry names that version `LTS__24.x`; the live
    // example run of 2026-09-16 recorded exactly that.
    let summary = serde_json::json!([{
        "metadata": {"uid": "pk-ubuntu-2404"},
        "spec": {"name": "ubuntu-aws", "version": "24.04", "registryUid": "reg", "digest": "sha256:u",
                 "layer": "os", "type": "spectro", "values": ""},
    }]);
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
            serde_json::json!([{"metadata": {"uid": uid},
                "spec": {"name": name, "version": "1.0.0", "registryUid": "reg", "layer": layer,
                         "type": "spectro", "values": ""}}]),
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
        ops::snapshot_from_candidate(
            &file,
            false,
            || Ok(Palette::new(api.clone())),
            "t0",
            &Progress::silent(),
        )
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
    fs::write(&extra, doc.to_string()).unwrap();
    assert_eq!(Snapshot::load(&extra).unwrap().root, recorded.root);

    // a newer schema id is refused, naming both ids
    doc["schema"] = serde_json::json!("strata-snapshot/2");
    let newer = dir.path().join("newer.json");
    fs::write(&newer, doc.to_string()).unwrap();
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
    let tenant = Palette::new(MemoryApi::new().with(
        "/v1/dashboard/projects/metadata",
        projects(&[("prod", "p2"), ("lab", "p1")]),
    ));
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
        .with("/v1/dashboard/spectroclusters/meta", Value::Array(listing))
        .in_project("p1")
}

fn cluster_named(name: &str) -> Ref {
    Ref::Cluster {
        uid: name.into(),
        profile: None,
        pending: false,
    }
}

#[test]
fn a_cluster_ref_takes_a_uid_or_a_name() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = clusters_named(dir.path(), &[("lab-cluster", CLUSTER_UID), ("other", "c2")]);
    let by_uid = snapshot_cluster(&api, None).unwrap();
    assert_eq!(
        api.calls("/v1/dashboard/spectroclusters/meta"),
        0,
        "a uid needs no listing"
    );
    let by_name = ops::snapshot_for(
        &cluster_named("lab-cluster"),
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(
        by_name.source.cluster.as_ref().unwrap().uid,
        CLUSTER_UID,
        "the record carries the uid"
    );
    assert_eq!(by_name.root, by_uid.root);

    let err = ops::snapshot_for(
        &cluster_named("nope"),
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap_err();
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
    let err = ops::snapshot_for(
        &cluster_named("twin"),
        || Ok(Palette::new(twins.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap_err();
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
    assert_eq!(api.calls("/v1/dashboard/spectroclusters/meta"), 0);
}

#[test]
fn a_cluster_record_taken_by_name_verifies_by_uid_after_a_rename() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = clusters_named(dir.path(), &[("lab-cluster", CLUSTER_UID)]);
    let recorded = ops::snapshot_for(
        &cluster_named("lab-cluster"),
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap();

    let mut renamed_doc = read(&dir.path().join("cluster-a/cluster.json"));
    renamed_doc["metadata"]["name"] = Value::from("lab-cluster-renamed");
    let renamed = api
        .clone()
        .with(format!("/v1/spectroclusters/{CLUSTER_UID}"), renamed_doc)
        .with(
            "/v1/dashboard/spectroclusters/meta",
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

    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&d, &dev, &staging, false)
        .unwrap();
    assert!(
        text.contains(r#"~ lab-profile.cluster_env: "dev" -> "staging""#),
        "{text}"
    );
    let md = render::renderer(Format::Markdown, Paint::off(), false)
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
    let renderer = render::renderer(Format::Terminal, Paint::off(), false);
    let verdict = renderer
        .verdict(v.diff.verify.as_ref().unwrap(), &recorded.source)
        .unwrap();
    assert!(
        verdict.starts_with("OK:") && verdict.contains("1 variable value changed"),
        "{verdict}"
    );

    let path = dir.path().join("cluster.json");
    fs::write(&path, recorded.to_json(true).unwrap()).unwrap();
    let s = ops::verify_many(
        &[path],
        None,
        |_| Ok(Palette::new(live.clone())),
        "t1",
        &ops::VerifyOptions::default(),
        &Progress::silent(),
    );
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
        ("variables.json", serde_json::json!({"variables": []})),
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
    assert_eq!(snap.source.template, "candidate");
    assert_eq!(snap.source.reference, "b@1");

    // The rules of a candidate file hold for a value too, and need no network.
    let mut empty = candidate.clone();
    empty.layers.clear();
    let err = candidate::check(empty, Path::new("b@1")).unwrap_err();
    assert!(
        matches!(&err, Error::CandidateInvalid { path, message }
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
    let mut profile = read(&dir.path().join("b/profile.json"));
    for p in profile["spec"]["published"]["packs"]
        .as_array_mut()
        .unwrap()
    {
        if p["name"] == "kubernetes" {
            edit(&mut p["values"]);
        }
    }
    let mut manifests = read(&dir.path().join("b/manifests.json"));
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
    let mut profile = read(&raw_root.join(name).join("profile.json"));
    edit(&mut profile);
    serve(MemoryApi::new(), raw_root, name, uid).with(format!("/v1/clusterprofiles/{uid}"), profile)
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
    let verdict = render::renderer(Format::Terminal, Paint::off(), false)
        .verdict(ver, &recorded.source)
        .unwrap();
    assert!(
        verdict.starts_with("DRIFT:") && verdict.contains("now names lab-profile@1.2.1"),
        "{verdict}"
    );

    let path = dir.path().join("a.json");
    fs::write(&path, recorded.to_json(true).unwrap()).unwrap();
    let s = ops::verify_many(
        &[path],
        None,
        |_| Ok(Palette::new(live.clone())),
        "t1",
        &ops::VerifyOptions::default(),
        &Progress::silent(),
    );
    assert_eq!(
        (s.records[0].status, s.exit_code()),
        (RecordStatus::Drifted, 1)
    );
    let lines = render::renderer(Format::Terminal, Paint::off(), false)
        .render_verify_summary(&s)
        .unwrap();
    assert!(lines.contains("now names lab-profile@1.2.1"), "{lines}");
}

#[test]
fn compare_names_a_profile_uid_that_names_two_versions_whatever_the_roots() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let a = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let live = |edit: fn(&mut Value)| {
        ops::snapshot_for(
            &Ref::Uid(UID_A.into()),
            || Ok(Palette::new(serve_edited(dir.path(), "a", UID_A, edit))),
            "t1",
            &Progress::silent(),
        )
        .unwrap()
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&d, &a, &renamed, false)
        .unwrap();
    assert!(text.contains("IDENTICAL"), "{text}");
    assert!(
        text.contains("uid 64f0000000000000000000a1 is lab-profile@1.2.0 on one side and lab-profile@1.3.0 on the other: the version was overwritten in place"),
        "{text}"
    );
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render(&d, &a, &renamed, false)
        .unwrap();
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
    let edited = ops::snapshot_for(
        &Ref::Uid(UID_A.into()),
        || {
            Ok(Palette::new(serve_edited(dir.path(), "a", UID_A, |p| {
                p["spec"]["published"]["packs"][1]["values"] =
                    Value::from("pack:\n  k8sHardening: false\n");
            })))
        },
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(
        (edited.source.uid.as_str(), edited.source.version.as_str()),
        (a.source.uid.as_str(), a.source.version.as_str())
    );
    let d = ops::compare(&a, &edited, "t2");
    assert!(!d.same);
    assert!(d.edited_in_place, "one uid and version, two contents");
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&d, &a, &edited, false)
        .unwrap();
    assert!(
        text.contains("both sides are lab-profile@1.2.0 (uid 64f0000000000000000000a1): the version was edited in place"),
        "{text}"
    );
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render(&d, &a, &edited, false)
        .unwrap();
    assert!(md.contains("the version was edited in place"), "{md}");

    let b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    assert!(!ops::compare(&a, &b, "t2").edited_in_place, "two versions");
    assert!(!ops::compare(&a, &a, "t2").edited_in_place, "same content");
}

use strata::diff::ImpactLevel;

/// A pack key is `<layer>/<name>`, so a profile that exchanges one pack for a pack of another name
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&d, &a, &swapped, false)
        .unwrap();
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

fn pack_summary(uid: &str, version: &str, registry: &str, digest: &str, values: &str) -> Value {
    serde_json::json!({
        "metadata": {"uid": uid},
        "spec": {"name": "csi-aws-ebs", "version": version, "registryUid": registry,
                 "digest": digest, "layer": "csi", "type": "spectro", "values": values,
                 "cloudTypes": ["aws"]},
    })
}

/// Two versions of one pack in the Public Repo, and the registry listing that names it.
fn serve_registry(extra: Vec<Value>, entities: &[(&str, Value)]) -> MemoryApi {
    let mut summaries = vec![
        pack_summary("pk-63", "1.63.1", "reg-public", "sha256:aaa", CSI_63),
        pack_summary("pk-65", "1.65.0", "reg-public", "sha256:bbb", CSI_65),
    ];
    summaries.extend(extra);
    serve_summaries(summaries, entities)
}

/// The same, with the pack versions spelled out.
fn serve_summaries(summaries: Vec<Value>, entities: &[(&str, Value)]) -> MemoryApi {
    let mut api = MemoryApi::new()
        .with(
            "/v1/registries/metadata",
            serde_json::json!({"items": [
                {"name": "Public Repo", "uid": "reg-public", "kind": "pack"},
                {"name": "Mirror", "uid": "reg-mirror", "kind": "oci"},
            ]}),
        )
        .with(
            "/v1/packs?filters=spec.name=csi-aws-ebs&limit=100&offset=0",
            serde_json::json!({"items": summaries, "listmeta": {"count": 9}}),
        );
    for (registry, entity) in entities {
        api = api.with(
            format!("/v1/packs/csi-aws-ebs/registries/{registry}"),
            entity.clone(),
        );
    }
    api
}

fn public_entity() -> Value {
    serde_json::json!({"layer": "csi", "tags": [
        {"tag": "1.63.1", "packUid": "pk-63"},
        {"tag": "1.63.x", "packUid": "pk-63"},
        {"tag": "1.x", "packUid": "pk-65"},
        {"tag": "1.65.0", "packUid": "pk-65"},
    ]})
}

fn snapshot_pack(api: &MemoryApi, reference: &str) -> strata::Result<Snapshot> {
    ops::snapshot_for(
        &Ref::parse_with(reference, |_| false),
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
}

#[test]
fn a_candidate_needs_a_profile_and_refuses_a_pack_record() {
    let api = serve_registry(vec![], &[("reg-public", public_entity())]);
    let pack = snapshot_pack(&api, "pack:csi-aws-ebs@1.65.0").unwrap();
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
    assert_eq!(l.schema, "strata-packs/1");
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
    assert_eq!(r.versions[0].kind, "spectro");
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
            pack_summary(
                &format!("pk-{i}"),
                &format!("1.{i}.0"),
                "reg-public",
                "sha256:a",
                CSI_63,
            )
        })
        .collect();
    first.push(pack_summary(
        "pk-65",
        "1.65.0",
        "reg-public",
        "sha256:bbb",
        CSI_65,
    ));
    let second: Vec<Value> = vec![
        pack_summary("pk-65", "1.65.0", "reg-public", "sha256:bbb", CSI_65),
        pack_summary("pk-200", "1.200.0", "reg-public", "sha256:c", CSI_65),
    ];
    let api = MemoryApi::new()
        .with(
            "/v1/registries/metadata",
            serde_json::json!({"items": [{"name": "Public Repo", "uid": "reg-public", "kind": "pack"}]}),
        )
        .with(
            "/v1/packs?filters=spec.name=csi-aws-ebs&limit=100&offset=0",
            serde_json::json!({"items": first, "listmeta": {"count": 103}}),
        )
        .with(
            "/v1/packs?filters=spec.name=csi-aws-ebs&limit=100&offset=101",
            serde_json::json!({"items": second, "listmeta": {"count": 103}}),
        )
        .with(
            "/v1/packs/csi-aws-ebs/registries/reg-public",
            public_entity(),
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
    // Palette lists the Mirror's disabled version, and then answers 404 for the Mirror: it serves
    // no disabled or deleted pack. A deprecated version it still serves, with its tags.
    let api = serve_summaries(
        vec![
            pack_summary("pk-63", "1.63.1", "reg-public", "sha256:aaa", CSI_63),
            in_state(
                pack_summary("pk-65", "1.65.0", "reg-public", "sha256:bbb", CSI_65),
                "deprecated",
            ),
            in_state(
                pack_summary("pk-m", "1.64.0", "reg-mirror", "sha256:mmm", CSI_63),
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
    assert_eq!(public.versions[0].state.as_deref(), Some("deprecated"));
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
        mirror.state.as_deref(),
        Some("disabled"),
        "every version of the Mirror is disabled"
    );
    assert_eq!(mirror.layer, "csi", "the layer comes from the listing");
    assert_eq!(mirror.versions[0].version, "1.64.0");
    assert_eq!(mirror.versions[0].digest, "sha256:mmm");
    assert_eq!(mirror.versions[0].state.as_deref(), Some("disabled"));
    assert!(mirror.versions[0].tags.is_empty());

    // the heading and the row say so, in both text formats
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_packs(&l)
        .unwrap();
    assert!(
        text.contains("csi-aws-ebs in Mirror (1 version, disabled, no tags)"),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|l| l.contains("1.65.0") && l.trim_end().ends_with("deprecated")),
        "{text}"
    );
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render_packs(&l)
        .unwrap();
    assert!(
        md.contains("### csi-aws-ebs in Mirror (disabled, no tags)"),
        "{md}"
    );
    assert!(md.contains("`1.65.0` deprecated"), "{md}");
}

#[test]
fn packs_lists_a_registry_that_answers_404_with_no_state_and_stops_on_any_other_failure() {
    let api = serve_registry(
        vec![pack_summary(
            "pk-m",
            "1.64.0",
            "reg-mirror",
            "sha256:mmm",
            CSI_63,
        )],
        &[("reg-public", public_entity())],
    );
    let l = packs_of(&api, "csi-aws-ebs", ops::PackOptions::default()).unwrap();
    let mirror = &l.registries[1];
    assert!(mirror.tags_unavailable);
    assert_eq!(mirror.state, None);
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_packs(&l)
        .unwrap();
    assert!(
        text.contains("csi-aws-ebs in Mirror (1 version, no tags: the registry answered 404)"),
        "{text}"
    );

    let api = serve_registry(vec![], &[]).failing(
        "/v1/packs/csi-aws-ebs/registries/reg-public",
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
                pack_summary("pk-65", "1.65.0", "reg-public", "sha256:bbb", CSI_65),
                state,
            )],
            &[],
        );
        let err = snapshot_pack(&api, "pack:Public Repo/csi-aws-ebs@1.65.0").unwrap_err();
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
            pack_summary("pk-65", "1.65.0", "reg-public", "sha256:bbb", CSI_65),
            "deprecated",
        )],
        &[("reg-public", public_entity())],
    );
    let snap = snapshot_pack(&api, "pack:Public Repo/csi-aws-ebs@1.65.0").unwrap();
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

    let api = MemoryApi::new()
        .with(
            "/v1/registries/metadata",
            serde_json::json!({"items": [{"name": "Public Repo", "uid": "reg-public", "kind": "pack"}]}),
        )
        .with(
            "/v1/packs?filters=spec.name=nowhere&limit=100&offset=0",
            serde_json::json!({"items": [], "listmeta": {"count": 0}}),
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
    let old = snapshot_pack(&api, "pack:csi-aws-ebs@1.63.1").unwrap();
    let new = snapshot_pack(&api, "pack:csi-aws-ebs@1.65.0").unwrap();

    assert_eq!(old.packs.len(), 1, "a pack snapshot holds the one pack");
    assert_eq!(old.source.title(), "pack csi-aws-ebs@1.63.1");
    assert_eq!(old.source.template, "registry");
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
    let latest = snapshot_pack(&api, "pack:csi-aws-ebs@latest").unwrap();
    assert_eq!(latest.source.version, "1.65.0");
    assert_eq!(latest.source.reference, "pack:csi-aws-ebs@latest");
    assert_eq!(latest.root, new.root);
}

#[test]
fn a_pack_in_several_registries_needs_the_registry_named() {
    let mirrored = pack_summary("pk-65-mirror", "1.65.0", "reg-mirror", "sha256:ccc", CSI_65);
    let mirror_entity =
        serde_json::json!({"layer": "csi", "tags": [{"tag": "1.65.0", "packUid": "pk-65-mirror"}]});
    let api = serve_registry(
        vec![mirrored],
        &[
            ("reg-public", public_entity()),
            ("reg-mirror", mirror_entity),
        ],
    );

    let err = snapshot_pack(&api, "pack:csi-aws-ebs@1.65.0").unwrap_err();
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

    // by registry name, and by registry uid
    let by_name = snapshot_pack(&api, "pack:Mirror/csi-aws-ebs@1.65.0").unwrap();
    assert_eq!(by_name.packs[0].hash.upstream_raw, "sha256:ccc");
    assert_eq!(by_name.source.reference, "pack:Mirror/csi-aws-ebs@1.65.0");
    let by_uid = snapshot_pack(&api, "pack:reg-public/csi-aws-ebs@1.65.0").unwrap();
    assert_eq!(by_uid.packs[0].hash.upstream_raw, "sha256:bbb");

    // the same release in two registries differs by IDENTITY and UPSTREAM alone
    let d = ops::compare(&by_uid, &by_name, "t1");
    let pd = &d.packs[0];
    assert!(pd.identity && pd.upstream && !pd.authored);

    let unknown = snapshot_pack(&api, "pack:Nope/csi-aws-ebs@1.65.0").unwrap_err();
    assert!(
        matches!(unknown, Error::RegistryNotFound { .. }),
        "{unknown}"
    );
    let missing = snapshot_pack(&api, "pack:csi-aws-ebs@9.9.9").unwrap_err();
    assert!(matches!(missing, Error::PackNotFound { .. }), "{missing}");
}

#[test]
fn verify_rechecks_a_pack_version_against_its_registry() {
    let api = serve_registry(vec![], &[("reg-public", public_entity())]);
    let recorded = snapshot_pack(&api, "pack:csi-aws-ebs@1.65.0").unwrap();
    let palette = Palette::new(api.clone());
    let v = ops::verify(&recorded, &palette, "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged);

    // the registry now serves other content under the same version
    // the same version, with other content under it
    let mut summary = pack_summary("pk-65", "1.65.0", "reg-public", "sha256:moved", CSI_63);
    summary["metadata"]["lastModifiedTimestamp"] = Value::from("2026-07-30T22:45:47.109Z");
    let moved = serve_summaries(vec![summary], &[("reg-public", public_entity())]);
    let v = ops::verify(&recorded, &Palette::new(moved), "t1", &Progress::silent()).unwrap();
    assert!(!v.unchanged, "a pack version whose content moved is drift");
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.status, VerifyStatus::Drifted);
    // the time of the listing is the create time of the pack, not the time of the move
    assert_eq!(ver.last_modified, None);
    let line = verdict_of(&v, &recorded);
    assert!(!line.contains("Palette last changed it"), "{line}");
    let json = serde_json::to_value(&v.diff).unwrap();
    assert!(json["verify"].get("lastModified").is_none(), "{json}");
}

/// Verify a record of csi-aws-ebs@1.65.0 in the Public Repo, whose registry answers `sync` for
/// its sync status: the verdict line, and the diff as `--json` writes it.
fn verify_pack_with_sync(sync: Option<Value>) -> (String, Value) {
    let api = serve_registry(vec![], &[("reg-public", public_entity())]);
    let recorded = snapshot_pack(&api, "pack:csi-aws-ebs@1.65.0").unwrap();
    let api = match sync {
        Some(s) => api.with("/v1/registries/pack/reg-public/sync/status", s),
        None => api,
    };
    let v = ops::verify(&recorded, &Palette::new(api), "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged);
    let verdict = render::renderer(Format::Terminal, Paint::off(), false)
        .verdict(v.diff.verify.as_ref().unwrap(), &recorded.source)
        .unwrap();
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
         Palette synced the registry 2026-09-30T00:00:11.000Z"
    );
    assert_eq!(
        json["verify"]["registrySync"],
        serde_json::json!({"lastSyncedTime": "2026-09-30T00:00:11.000Z", "status": "Completed"})
    );
}

#[test]
fn verify_of_a_pack_record_names_a_failed_sync_of_the_registry_with_its_message() {
    let (verdict, json) = verify_pack_with_sync(Some(serde_json::json!({
        "isSyncSupported": true, "lastSyncedTime": "2025-10-16T08:00:00.000Z",
        "status": "Failed", "message": "invalid helm registry http status 404",
    })));
    assert!(
        verdict.ends_with(
            "is unchanged since t0; the last sync of the registry failed: \
             invalid helm registry http status 404"
        ),
        "{verdict}"
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
    let verdict = render::renderer(Format::Terminal, Paint::off(), false)
        .verdict(v.diff.verify.as_ref().unwrap(), &recorded.source)
        .unwrap();
    (verdict, serde_json::to_value(&v.diff).unwrap())
}

/// Answers the sync route of the Public Repo with a dropped connection, and delegates each other
/// request. The in-memory API cannot produce a transport error.
struct SyncDropped(MemoryApi);

impl PaletteApi for SyncDropped {
    fn get_json(&self, path: &str) -> strata::Result<Value> {
        if path == "/v1/registries/pack/reg-public/sync/status" {
            return Err(Error::Transport {
                path: path.to_string(),
                message: "connection reset".into(),
            });
        }
        self.0.get_json(path)
    }
}

#[test]
fn verify_of_a_pack_record_gives_the_verdict_when_the_sync_route_answers_500() {
    let api = serve_registry(vec![], &[("reg-public", public_entity())]);
    let recorded = snapshot_pack(&api, "pack:csi-aws-ebs@1.65.0").unwrap();
    let api = api.failing(
        "/v1/registries/pack/reg-public/sync/status",
        500,
        r#"{"code":"InternalError"}"#,
    );
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
fn verify_of_a_pack_record_gives_the_verdict_when_the_sync_request_does_not_complete() {
    let api = serve_registry(vec![], &[("reg-public", public_entity())]);
    let recorded = snapshot_pack(&api, "pack:csi-aws-ebs@1.65.0").unwrap();
    let (verdict, json) = verify_pack_on(&recorded, SyncDropped(api));
    assert_eq!(
        verdict,
        "OK: pack csi-aws-ebs@1.65.0 in registry Public Repo is unchanged since t0"
    );
    assert!(json["verify"].get("registrySync").is_none(), "{json}");
    assert_eq!(
        json["verify"]["registrySyncError"],
        "GET /v1/registries/pack/reg-public/sync/status: connection reset"
    );
    assert_eq!(json["verify"]["status"], "unchanged");
}

#[test]
fn verify_reports_drift_and_names_the_order_when_only_the_order_of_the_packs_changed() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let live = serve(MemoryApi::new(), dir.path(), "a-reversed", UID_A);
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

    let path = dir.path().join("a.json");
    fs::write(&path, recorded.to_json(true).unwrap()).unwrap();
    let s = ops::verify_many(
        &[path],
        None,
        |_| Ok(Palette::new(live.clone())),
        "t1",
        &ops::VerifyOptions::default(),
        &Progress::silent(),
    );
    assert_eq!(
        (s.records[0].status, s.exit_code()),
        (RecordStatus::Drifted, 1)
    );
    let lines = render::renderer(Format::Terminal, Paint::off(), false)
        .render_verify_summary(&s)
        .unwrap();
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

const PROFILE_AUDITS: &str = "/v1/audits?resourceKind=clusterprofile&resourceUid=64f0000000000000000000a1&startTime=t0&limit=100";

#[test]
fn verify_who_reads_every_page_of_the_audit_log_and_lists_the_writes_oldest_first() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    // Palette lists the newest first, 100 on a page, and names the next page in `continue`
    let api = serve(MemoryApi::new(), dir.path(), "b", UID_A)
        .with(
            PROFILE_AUDITS,
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
            format!("{PROFILE_AUDITS}&continue=tok"),
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
    let verdict = render::renderer(Format::Terminal, Paint::off(), false)
        .verdict(ver, &recorded.source)
        .unwrap();
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
    let api = serve(MemoryApi::new(), dir.path(), "b", UID_A)
        .with(PROFILE_AUDITS, audits_page(vec![], ""));
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
    let terminal = render::renderer(Format::Terminal, Paint::off(), false)
        .verdict(ver, &recorded.source)
        .unwrap();
    assert!(
        terminal.ends_with("\nwho: no audit entry since the record"),
        "{terminal}"
    );
    let json = serde_json::to_value(&v.diff).unwrap();
    assert_eq!(json["verify"]["who"], serde_json::json!([]));

    // an unchanged record reads no audit log, and without --who a drifted one reads none either
    let same = serve(MemoryApi::new(), dir.path(), "a", UID_A);
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
    assert_eq!(api.calls(PROFILE_AUDITS), 1);
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
    let path = format!("/v1/audits?resourceUid={CLUSTER_UID}&startTime=t0&limit=100");
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
    let path = format!("/v1/audits?resourceUid={CLUSTER_UID}&startTime=t0&limit=100");
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
    let line = verdict_of(&v, &recorded);
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
    let path = "/v1/audits?resourceKind=clusterprofile&resourceUid=64f0000000000000000000a1&startTime=2026-09-17T09:30:00%2B00:00&limit=100";
    let api = serve(MemoryApi::new(), dir.path(), "b", UID_A).with(path, audits_page(vec![], ""));
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
    let path = dir.path().join("rec.json");
    fs::write(&path, recorded.to_json(true).unwrap()).unwrap();
    let api = serve(MemoryApi::new(), dir.path(), "b", UID_A).with(
        PROFILE_AUDITS,
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
    let s = ops::verify_many(
        &[path],
        None,
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &ops::VerifyOptions {
            who: true,
            ..Default::default()
        },
        &Progress::silent(),
    );
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_verify_summary(&s)
        .unwrap();
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
    let mut doc = group_api.get_json("/v1/clustergroups/g1").unwrap();
    doc["spec"]["clusterProfileTemplates"][0]["packs"][0]["values"] = Value::from("edited: true\n");
    let group_log = "/v1/audits?resourceKind=clustergroup&resourceUid=g1&startTime=t0&limit=100";
    let edited = group_api.clone().with("/v1/clustergroups/g1", doc).with(
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
    let line = verdict_of(&v, &group);
    assert!(
        line.ends_with(
            "\nwho: 1 write in the audit log since the record\n  \
             2026-09-18T14:02:11.482Z  John Doe (user)  update"
        ),
        "the write to the group itself names no object: {line}"
    );

    // an app profile whose tier was edited after the record
    let app = ops::snapshot_for(
        &Ref::parse("appprofile:shop@1.0.0"),
        || Ok(Palette::new(serve_app_profile(MemoryApi::new(), "2"))),
        "t0",
        &Progress::silent(),
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
    let line = verdict_of(&v, &app);
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
    let mut bumped = read(&dir.path().join("a/profile.json"));
    bumped["spec"]["version"] = Value::from("1.3.0");
    let cluster_log = format!("/v1/audits?resourceUid={CLUSTER_UID}&startTime=t0&limit=100");
    let profile_log = format!(
        "/v1/audits?resourceKind=clusterprofile&resourceUid={UID_A}&startTime=t0&limit=100"
    );
    // an entry of the system that names no resource
    let mut bare = audit("au3", "2026-09-18T14:09:00.000Z", system(), "update", None);
    bare["spec"].as_object_mut().unwrap().remove("resource");
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a")
        .with(format!("/v1/clusterprofiles/{UID_A}"), bumped)
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
    let line = verdict_of(&v, &recorded);
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

#[test]
fn verify_gives_the_time_palette_last_changed_the_object_and_a_drift_verdict_ends_with_it() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let with_time = |from: &str| {
        let mut doc = read(&dir.path().join(from).join("profile.json"));
        doc["metadata"]["lastModifiedTimestamp"] = Value::from("2026-09-18T14:02:11.482Z");
        serve(MemoryApi::new(), dir.path(), from, UID_A)
            .with(format!("/v1/clusterprofiles/{UID_A}"), doc)
    };

    // the document carries the time: no request more
    let api = with_time("b");
    let v = ops::verify(&recorded, &Palette::new(&api), "t1", &Progress::silent()).unwrap();
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(
        ver.last_modified.as_deref(),
        Some("2026-09-18T14:02:11.482Z")
    );
    let line = verdict_of(&v, &recorded);
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
    assert!(verdict_of(&v, &recorded).ends_with("is unchanged since t0"));

    // each record of a summary gives it
    let path = dir.path().join("rec.json");
    fs::write(&path, recorded.to_json(true).unwrap()).unwrap();
    let s = ops::verify_many(
        &[path],
        None,
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &ops::VerifyOptions::default(),
        &Progress::silent(),
    );
    let json = serde_json::to_value(&s).unwrap();
    assert_eq!(
        json["records"][0]["lastModified"],
        "2026-09-18T14:02:11.482Z"
    );

    // a document with no time gives none
    let v = ops::verify(
        &recorded,
        &Palette::new(serve(MemoryApi::new(), dir.path(), "b", UID_A)),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(v.diff.verify.as_ref().unwrap().last_modified.is_none());
    assert!(!verdict_of(&v, &recorded).contains("last changed"));
}

// ---- the updates that wait for a cluster -----------------------------------------------------

const NOTIFICATIONS: &str = "/v1/notifications/spectrocluster/c1a5000000000000000000c1";

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
            NOTIFICATIONS,
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
            format!("{NOTIFICATIONS}?continue=tok"),
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
    assert_eq!(c.updates_pending_error, None);
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
        .with(NOTIFICATIONS, notifications_page(vec![], ""));
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
        c.updates_pending_error
            .as_deref()
            .unwrap()
            .starts_with("HTTP 404"),
        "{:?}",
        c.updates_pending_error
    );
    let json = serde_json::to_value(&recorded).unwrap();
    assert!(json["source"]["cluster"].get("updatesPending").is_none());
}

#[test]
fn a_raw_cluster_directory_keeps_the_notifications() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let (_, addon) = merged_uids(dir.path());
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-merged").with(
        NOTIFICATIONS,
        notifications_page(vec![notification("n1", "t", false, &[(&addon, "m")])], ""),
    );
    let saved = dir.path().join("raw");
    Palette::new(api)
        .fetch_cluster(CLUSTER_UID, &Progress::silent())
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
        NOTIFICATIONS,
        notifications_page(vec![values_notification(&addon)], ""),
    );
    let saved = dir.path().join("raw");
    Palette::new(api)
        .fetch_cluster(CLUSTER_UID, &Progress::silent())
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
        NOTIFICATIONS,
        notifications_page(vec![values_notification(&addon)], ""),
    );
    let fetched = Palette::new(api)
        .fetch_cluster(CLUSTER_UID, &Progress::silent())
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
        NOTIFICATIONS,
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
            .with(NOTIFICATIONS, notifications_page(vec![], "")),
        None,
    )
    .unwrap();
    let now = snapshot_cluster(&api, None).unwrap();
    let d = ops::compare(&before, &now, "t1");
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&d, &before, &now, false)
        .unwrap();
    assert!(
        text.contains(
            "note       cluster lab-cluster is behind: Palette says an update of lab-addons waits for it, since 2026-09-24T23:55:02.083Z\n"
        ),
        "{text}"
    );
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render(&d, &before, &now, false)
        .unwrap();
    assert!(
        md.contains("**Note:** cluster lab-cluster is behind"),
        "{md}"
    );
    // a side read when nothing waited gets no note
    let d = ops::compare(&before, &before, "t1");
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&d, &before, &before, false)
        .unwrap();
    assert!(!text.contains("is behind"), "{text}");
}

const REPAVE: &str = "/v1/spectroclusters/c1a5000000000000000000c1/repave/status";

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
    let mut cluster = read(&raw_root.join(dir).join("cluster.json"));
    cluster["status"]["repave"] = serde_json::json!({"state": state});
    serve_cluster(MemoryApi::new(), raw_root, dir)
        .with(format!("/v1/spectroclusters/{CLUSTER_UID}"), cluster)
        .with(REPAVE, repave_status(state))
        .with(NOTIFICATIONS, notifications_page(vec![], ""))
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
    let api =
        serve_repave(dir.path(), "cluster-merged", "Pending").failing(REPAVE, 403, "forbidden");
    let waits = snapshot_cluster(&api, None).unwrap();
    let c = waits.source.cluster.as_ref().unwrap();
    assert_eq!(c.repave_pending, Some(true), "the cluster document says it");
    assert!(c.pending_repave.is_none());
    assert_eq!(
        c.pending_repave_error.as_deref(),
        Some("HTTP 403: forbidden")
    );
    let json = serde_json::to_value(&waits).unwrap();
    assert_eq!(
        json["source"]["cluster"]["pendingRepaveError"],
        "HTTP 403: forbidden"
    );

    let v = ops::verify(&before, &Palette::new(api), "t1", &Progress::silent()).unwrap();
    assert_eq!(v.status, VerifyStatus::Unchanged);
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&v.diff, &before, &v.live, false)
        .unwrap();
    let note = "a repave of cluster lab-cluster waits for approval (strata could not read its \
                reasons: HTTP 403: forbidden). Palette refuses an update of its profiles until a \
                person approves it";
    assert!(text.contains(&format!("note       {note}\n")), "{text}");

    // the error says nothing about the profiles of a pending update, or of another kind
    let pending = waits.clone().mark_pending();
    assert!(
        pending
            .source
            .cluster
            .unwrap()
            .pending_repave_error
            .is_none()
    );
    let host = waits.with_kind(strata::snapshot::SubjectKind::EdgeHost);
    assert!(host.source.cluster.unwrap().pending_repave_error.is_none());
}

#[test]
fn a_profile_record_names_each_cluster_template_that_links_the_version_outside_the_root() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let plain =
        common::snapshot_of(&serve(MemoryApi::new(), dir.path(), "a", UID_A), UID_A).unwrap();
    let mut profile = read(&dir.path().join("a").join("profile.json"));
    profile["status"]["inUseClusterTemplates"] = serde_json::json!([
        {"uid": "tmpl-1", "name": "barney-template", "projectUid": "p1", "kind": "clustertemplate"},
        {"uid": "tmpl-2", "name": "fred-template", "projectUid": "p1", "kind": "clustertemplate"}
    ]);
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A)
        .with(format!("/v1/clusterprofiles/{UID_A}"), profile);
    let linked = common::snapshot_of(&api, UID_A).unwrap();
    assert_eq!(linked.root, plain.root);
    let json = serde_json::to_value(&linked).unwrap();
    assert_eq!(
        json["source"]["clusterTemplates"],
        serde_json::json!([
            {"uid": "tmpl-1", "name": "barney-template", "projectUid": "p1"},
            {"uid": "tmpl-2", "name": "fred-template", "projectUid": "p1"}
        ])
    );
    assert!(
        serde_json::to_value(&plain).unwrap()["source"]
            .get("clusterTemplates")
            .is_none()
    );
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&d, &before, &now, false)
        .unwrap();
    assert!(text.contains(&format!("note       {note}\n")), "{text}");
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render(&d, &before, &now, false)
        .unwrap();
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&v.diff, &before, &v.live, false)
        .unwrap();
    assert!(text.contains(&format!("note       {note}\n")), "{text}");

    // a side with no repave that waits gets no note, and two sides that say the same get one
    let d = ops::compare(&before, &before, "t1");
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&d, &before, &before, false)
        .unwrap();
    assert!(!text.contains("a repave of"), "{text}");
    let d = ops::compare(&now, &now, "t1");
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&d, &now, &now, false)
        .unwrap();
    assert_eq!(text.matches("a repave of").count(), 1, "{text}");
}

#[test]
fn a_raw_directory_keeps_the_repave_with_no_value_and_gives_the_record_of_the_fetch() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let fetched = Palette::new(serve_repave(dir.path(), "cluster-merged", "Pending"))
        .fetch_cluster(CLUSTER_UID, &Progress::silent())
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

// ---- a pack with no upstream digest ------------------------------------------------------------

/// Fixture `a` as `dir`, with no digest for the OS pack anywhere: not on its pack reference, and
/// not in the pack summary. Palette gives such a pack for a registry it does not sync.
fn without_digest(raw_root: &Path, dir: &str) -> Snapshot {
    let (from, to) = (raw_root.join("a"), raw_root.join(dir));
    fs::create_dir_all(&to).unwrap();
    let mut profile = read(&from.join("profile.json"));
    for pack in profile["spec"]["published"]["packs"]
        .as_array_mut()
        .unwrap()
    {
        if pack["name"] == "ubuntu-aws" {
            pack["digest"] = Value::from("");
        }
    }
    let mut manifests = read(&from.join("manifests.json"));
    for pack in manifests["spec"]["packs"].as_array_mut().unwrap() {
        if pack["spec"]["name"] == "ubuntu-aws" {
            pack["spec"]["digest"] = Value::from("");
        }
    }
    for (name, doc) in [("profile.json", &profile), ("manifests.json", &manifests)] {
        fs::write(to.join(name), serde_json::to_string_pretty(doc).unwrap()).unwrap();
    }
    fs::copy(from.join("variables.json"), to.join("variables.json")).unwrap();
    ops::snapshot_from_dir(&to, dir, "t0").unwrap()
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
    assert_eq!(os.hash.upstream_source, "none");
    assert_eq!(recorded.upstream_unknown(), ["os/ubuntu-aws"]);

    // the same content, and the empty digest on both sides: the upstream check cannot fail
    let api = serve(MemoryApi::new(), dir.path(), "nodigest", UID_A);
    let v = ops::verify(&recorded, &Palette::new(&api), "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged, "the content is the same");
    assert_eq!(v.status, VerifyStatus::Unverified);
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.upstream_unknown, ["os/ubuntu-aws"]);
    let verdict = render::renderer(Format::Terminal, Paint::off(), false)
        .verdict(ver, &recorded.source)
        .unwrap();
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&v.diff, &recorded, &v.live, false)
        .unwrap();
    assert!(
        text.contains("note       pack os/ubuntu-aws has no upstream digest: Palette gives none, so a change of the archive does not show\n"),
        "{text}"
    );

    // drift wins: a record that also changed is drifted
    let changed = serve(MemoryApi::new(), dir.path(), "b", UID_A);
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
fn many_records_count_an_unverified_one_and_exit_1() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = without_digest(dir.path(), "nodigest");
    let path = dir.path().join("rec.json");
    fs::write(&path, recorded.to_json(true).unwrap()).unwrap();
    let api = serve(MemoryApi::new(), dir.path(), "nodigest", UID_A);
    let s = ops::verify_many(
        std::slice::from_ref(&path),
        None,
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &ops::VerifyOptions::default(),
        &Progress::silent(),
    );
    assert_eq!(
        (s.records[0].status, s.summary.unverified, s.exit_code()),
        (RecordStatus::Unverified, 1, 1)
    );
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_verify_summary(&s)
        .unwrap();
    assert!(text.contains("UNVERIFIED"), "{text}");
    assert!(
        text.contains("same content as recorded t0, but pack os/ubuntu-aws has no upstream digest"),
        "{text}"
    );
    assert!(
        text.contains("1 record: 0 unchanged, 0 drifted, 1 unverified, no errors"),
        "{text}"
    );
    let allowed = ops::verify_many(
        &[path],
        None,
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &ops::VerifyOptions {
            allow_no_digest: true,
            ..Default::default()
        },
        &Progress::silent(),
    );
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&d, &a, &b, false)
        .unwrap();
    assert!(
        text.contains("upstream unknown: no digest, so a change of the archive does not show"),
        "{text}"
    );
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render(&d, &a, &b, false)
        .unwrap();
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
    pack.hash.upstream_source = "none".to_string();
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
    let allowed = ops::same_release_with(&records, Policy::Exact, true, "t1");
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&d, &a, &a, false)
        .unwrap();
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
    let mut cluster = read(&from.join("cluster.json"));
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
    let path = "/v1/spectroclusters/c1a5000000000000000000c1/packs/resolvedValues";
    let tenant = |region: &str| {
        serve_cluster(MemoryApi::new(), dir.path(), "cluster-macro").with(path, resolved(region))
    };

    // the record holds the digest of the value, and not the value
    let mut raw = Palette::new(tenant("us-east-2"))
        .fetch_cluster(CLUSTER_UID, &Progress::silent())
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
    let verdict = render::renderer(Format::Terminal, Paint::off(), false)
        .verdict(ver, &recorded.source)
        .unwrap();
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
    let path = format!("/v1/clusterprofiles/{UID_A}/packs/resolvedValues");
    let tenant =
        |answer: Value| serve(MemoryApi::new(), dir.path(), "macro", UID_A).with(&path, answer);

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
    let verdict = render::renderer(Format::Terminal, Paint::off(), false)
        .verdict(ver, &recorded.source)
        .unwrap();
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
        &Palette::new(serve(MemoryApi::new(), dir.path(), "macro", UID_A)),
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
    let path = format!("POST /v1/spectroclusters/{CLUSTER_UID}/validate/repave");
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
    assert_eq!(
        api.posted()[0].0,
        format!("/v1/spectroclusters/{CLUSTER_UID}/validate/repave")
    );
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
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
}

#[test]
fn compare_asks_the_template_route_for_a_cluster_that_a_cluster_template_governs() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let mut doc = read(&dir.path().join("cluster-a").join("cluster.json"));
    doc["spec"]["clusterTemplate"] = serde_json::json!({"uid": "tmpl-1", "projectUid": "p1"});
    let cluster_api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a")
        .with(format!("/v1/spectroclusters/{CLUSTER_UID}"), doc);
    let cluster = snapshot_cluster(&cluster_api, None).unwrap();
    let target = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    // on the route of a cluster in a template, Palette compares nothing and answers no repave; it
    // answers for the template on the route of the template
    let template_route = "/v1/spectroclusters/clusterTemplates/tmpl-1/validate/repave";
    let api = cluster_api
        .clone()
        .with(
            format!("POST /v1/spectroclusters/{CLUSTER_UID}/validate/repave"),
            serde_json::json!({"isRepaveRequired": false, "reasons": []}),
        )
        .with(
            format!("POST {template_route}"),
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
            .cluster_document(CLUSTER_UID)
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
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
    assert!(matches!(err, Error::Usage { .. }), "{err}");
    assert!(err.to_string().contains("cluster template tmpl-1"), "{err}");
    assert_eq!(api.posted().len(), 1);
}

/// The update that waits for cluster-merged, from its templates: `edit` changes them.
fn serve_merged_pending(api: MemoryApi, raw_root: &Path, edit: impl Fn(&mut Value)) -> MemoryApi {
    let m = raw_root.join("cluster-merged");
    let mut templates = read(&m.join("cluster.json"))["spec"]["clusterProfileTemplates"].clone();
    edit(&mut templates);
    let mut api = api.with(
        format!("/v1/spectroclusters/{CLUSTER_UID}/profileUpdates"),
        serde_json::json!({ "profiles": templates }),
    );
    for m in read(&m.join("manifests.json"))["profiles"]
        .as_array()
        .unwrap()
    {
        let uid = m["metadata"]["uid"].as_str().unwrap();
        api = api.with(
            format!("/v1/clusterprofiles/{uid}/packs/manifests"),
            m.clone(),
        );
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
    let path = format!("POST /v1/spectroclusters/{CLUSTER_UID}/validate/repave");
    let answer = serde_json::json!({"isRepaveRequired": true, "reasons": [{
        "code": "PackValuesUpdated", "message": "Pack 'kubernetes' values are updated",
        "pack": {"current": {"name": "kubernetes"}, "diffConfigKeys": ["kubeadmconfig.postKubeadmCommands"]}}]});
    let pending_of = |api: &MemoryApi, profile: Option<&str>| {
        ops::snapshot_for(
            &Ref::Cluster {
                uid: CLUSTER_UID.into(),
                profile: profile.map(str::to_string),
                pending: true,
            },
            || Ok(Palette::new(api.clone())),
            "t0",
            &Progress::silent(),
        )
        .unwrap()
    };
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
        assert_eq!(
            posted[0].0,
            format!("/v1/spectroclusters/{CLUSTER_UID}/validate/repave")
        );
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
    let mut b_names: Vec<String> = read(&dir.path().join("cluster-merged").join("cluster.json"))
        ["spec"]["clusterProfileTemplates"]
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
    let api = serve_merged_pending(running.clone(), dir.path(), edit_values);
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
    let api = serve_merged_pending(running.clone(), dir.path(), add_pack);
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
    let api = serve_merged_pending(running.clone(), dir.path(), edit_values);
    let body = ask(&api, &cluster, &pending_of(&api, Some("lab-addons"))).unwrap();
    assert!(
        uid_and_packs(&body)
            .iter()
            .all(|(_, packs)| packs.is_none())
    );

    // a record of a profile that the cluster no longer runs is older than the cluster
    let moved = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a");
    let err = ask(&moved, &cluster, &pending).unwrap_err();
    assert!(
        matches!(&err, Error::Usage { message } if message.contains("Record cluster:lab-cluster@pending again")),
        "{err}"
    );

    // a cluster in a template: the route of the template takes no pack values. So an update that
    // changes a profile is a usage error before the POST, and an update that changes nothing asks
    // the route of the template by profile uid
    let mut doc = read(&dir.path().join("cluster-merged").join("cluster.json"));
    doc["spec"]["clusterTemplate"] = serde_json::json!({"uid": "tmpl-1", "projectUid": "p1"});
    let governed = running
        .clone()
        .with(format!("/v1/spectroclusters/{CLUSTER_UID}"), doc);
    let err = ask(&governed, &cluster, &pending).unwrap_err();
    assert!(
        matches!(&err, Error::Usage { message }
            if message.contains("the update that waits for the cluster lab-cluster")
                && message.contains("cluster template tmpl-1")),
        "{err}"
    );
    let api = serve_merged_pending(governed, dir.path(), |_| {}).with(
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
    let path = format!("POST /v1/spectroclusters/{CLUSTER_UID}/validate/packs");
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
    assert!(!v.accepts);
    assert_eq!(v.cluster_uid, CLUSTER_UID);
    assert_eq!(v.errors.len(), 1, "{v:?}");
    assert_eq!(v.errors[0].pack, "strata-live-pack");
    // one POST, with the body of --repave, in either order of the sides
    let posted = api.posted();
    assert_eq!(posted.len(), 1);
    assert_eq!(
        posted[0].0,
        format!("/v1/spectroclusters/{CLUSTER_UID}/validate/packs")
    );
    let running = strata::snapshot::AppliedProfile::of_cluster(
        &Palette::new(&cluster_api)
            .cluster_document(CLUSTER_UID)
            .unwrap(),
    );
    assert_eq!(posted[0].1, ops::validate_body(&running, &target));
    assert_eq!(
        ops::palette_validation(&palette, &target, &cluster).unwrap(),
        v
    );

    d.palette_validation = Some(v);
    let text = render::renderer(Format::Terminal, Paint::off(), false)
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
    assert!(d.palette_validation.as_ref().unwrap().accepts);
    let text = render::renderer(Format::Terminal, Paint::off(), false)
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
    assert!(!v.accepts);
    assert_eq!(v.errors.len(), 1);
    assert_eq!(v.errors[0].pack, "");
    assert_eq!(v.errors[0].code, "PackSpectroIntegrationNotSupported");
    assert_eq!(
        v.errors[0].message,
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
            "PackValuesOverrideForbidden",
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
            (v.errors[0].code.as_str(), v.errors[0].message.as_str()),
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
    host.source.cluster.as_mut().unwrap().kind = strata::snapshot::SubjectKind::EdgeHost;
    let err = ops::palette_validation(&palette, &host, &target).unwrap_err();
    assert!(matches!(err, Error::Usage { .. }), "{err}");
    assert!(
        err.to_string().contains(
            "--validate needs a cluster: Palette answers it for a cluster, and not for an edge host"
        ),
        "{err}"
    );
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
        assert_eq!(p.registry_provider_type.as_deref(), Some(kind));
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
    let mut oci = pack_summary("pk-m", "1.64.0", "reg-mirror", "sha256:mmm", CSI_63);
    oci["spec"]["type"] = serde_json::json!("oci");
    let mirror =
        serde_json::json!({"layer": "csi", "tags": [{"tag": "1.64.0", "packUid": "pk-m"}]});
    let api = serve_registry(
        vec![oci],
        &[("reg-public", public_entity()), ("reg-mirror", mirror)],
    );
    let listed = api.clone().with(
        "/v1/registries/oci/summary",
        serde_json::json!({"items": [
            {"metadata": {"uid": "reg-mirror", "name": "Mirror"}, "spec": {"providerType": "helm"}},
        ]}),
    );
    let pack = snapshot_pack(&listed, "pack:Mirror/csi-aws-ebs@1.64.0").unwrap();
    assert_eq!(
        pack.packs[0].registry_provider_type.as_deref(),
        Some("helm")
    );
    // a Palette that does not answer the listing gives a record that names no provider type
    let unnamed = snapshot_pack(&api, "pack:Mirror/csi-aws-ebs@1.64.0").unwrap();
    assert_eq!(unnamed.packs[0].registry_provider_type, None);
    // and a pack from a pack registry names none, with no read of the listing
    let spectro = snapshot_pack(&listed, "pack:Public Repo/csi-aws-ebs@1.65.0").unwrap();
    assert_eq!(spectro.packs[0].registry_provider_type, None);
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
    serve(MemoryApi::new(), raw_root, dir, UID_A)
}

#[test]
fn a_pack_record_keeps_the_annotations_of_the_pack_ref_outside_the_root() {
    use strata::snapshot::Pack;
    let pack = |annotations: Value| {
        serde_json::json!({"name": "ubuntu-aws", "type": "spectro", "layer": "os", "tag": "22.04",
            "packUid": "p1", "registryUid": "r1", "values": "a: 1", "annotations": annotations})
    };
    let byoi = Pack::from_api(&pack(os_annotations(Some("ami-0aaa"))), &[]);
    assert_eq!(byoi.image_id(), Some("ami-0aaa"));
    assert_eq!(byoi.annotations["osName"], "ubuntu");
    let doc = serde_json::to_value(&byoi).unwrap();
    assert_eq!(doc["annotations"]["imageId"], "ami-0aaa");
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
    let mut old = doc.clone();
    old.as_object_mut().unwrap().remove("annotations");
    let read: Pack = serde_json::from_value(old).unwrap();
    assert!(read.annotations.is_empty());
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&d, &a, &b, false)
        .unwrap();
    assert!(text.contains(&format!("note       {note}")), "{text}");
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render(&d, &a, &b, false)
        .unwrap();
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
        ops::same_release_with(&records, policy, allow_no_digest, "t1")
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_chain(&c)
        .unwrap();
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
            .verdict(ver, &recorded.source)
            .unwrap();
        assert!(
            verdict.contains("DRIFT") && verdict.contains(why),
            "{verdict}"
        );
    }
    let path = dir.path().join("ami.json");
    fs::write(&path, recorded.to_json(true).unwrap()).unwrap();
    let s = ops::verify_many(
        &[path],
        None,
        |_| Ok(Palette::new(live.clone())),
        "t1",
        &ops::VerifyOptions::default(),
        &Progress::silent(),
    );
    assert_eq!(
        (s.records[0].status, s.exit_code()),
        (RecordStatus::Drifted, 1)
    );
    let lines = render::renderer(Format::Terminal, Paint::off(), false)
        .render_verify_summary(&s)
        .unwrap();
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
    let live = serve(MemoryApi::new(), dir.path(), "upgraded", UID_A);
    let v = ops::verify(&recorded, &Palette::new(live), "t1", &Progress::silent()).unwrap();
    assert!(!v.diff.same);
    assert_eq!(v.diff.image_ids_changed.len(), 1);
    let ver = v.diff.verify.as_ref().unwrap();
    assert!(ver.image_ids_changed.is_empty());
    let verdict = render::renderer(Format::Terminal, Paint::off(), false)
        .verdict(ver, &recorded.source)
        .unwrap();
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
    let mut doc = read(&path);
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
    common::make_fixtures(dir.path());
    with_cluster_status(dir.path());
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a");
    let snap = snapshot_cluster(&api, None).unwrap();
    let c = snap.source.cluster.as_ref().unwrap();
    // shape costs a request, so a record without --shape does not carry it
    assert_eq!(
        c.tiers.keys().collect::<Vec<_>>(),
        ["applied", "policy", "running"]
    );
    assert_eq!(c.labels.get("env").map(String::as_str), Some("dev"));
    assert_eq!(c.tiers["applied"].fields["spcHash"], "h1");
    // running holds the covered profile alone, without the condition
    let running = c.tiers["running"].fields.as_array().unwrap();
    assert!(
        running
            .iter()
            .all(|r| r["name"] != "other" && r.get("condition").is_none())
    );
    assert_eq!(running.len(), snap.packs.len());
    assert_eq!(c.tiers["policy"].fields["timezone"], "Etc/UTC");

    // --shape: the machine pools and the cloud part, without the ids of the cluster
    let palette = Palette::new(
        api.clone()
            .with("/v1/cloudconfigs/aws/cc1", cloud_config(2)),
    );
    let mut raw = palette
        .fetch_cluster(CLUSTER_UID, &Progress::silent())
        .unwrap();
    raw.cloud_config = Some(palette.cloud_config(&raw.cluster).unwrap());
    let shaped = Snapshot::from_cluster(&raw, None, "cluster", "t0").unwrap();
    let shape = &shaped.source.cluster.as_ref().unwrap().tiers["shape"].fields;
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
    let shape = &shaped.source.cluster.as_ref().unwrap().tiers["shape"].fields;
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
    let shape = &shaped.source.cluster.as_ref().unwrap().tiers["shape"].fields;
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

#[test]
fn a_cluster_record_without_tiers_still_verifies() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    with_cluster_status(dir.path());
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a");
    let mut doc = serde_json::to_value(snapshot_cluster(&api, None).unwrap()).unwrap();
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
        .fetch_cluster(CLUSTER_UID, &Progress::silent())
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
    let tier = |name: &str| d.cluster_tiers.iter().find(|t| t.tier == name).unwrap();
    assert!(tier("applied").same);
    assert!(!tier("policy").same);
    assert_eq!(tier("policy").diff[0].path, "timezone");
    assert_eq!(tier("shape").diff[0].path, "pools[name=worker-pool].size");
    assert!(!tier("running").same);
    assert!(tier("labels").same);
    assert_eq!(d.running_behind.len(), 1);
    let behind = &d.running_behind[0];
    assert_eq!(
        (behind.side.as_str(), behind.pack.as_str()),
        ("b", first.as_str())
    );
    assert_eq!(
        (behind.declared.as_str(), behind.running.as_str()),
        (declared.as_str(), "0.0.1")
    );
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&d, &a, &b, false)
        .unwrap();
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
        .fetch_cluster(CLUSTER_UID, &Progress::silent())
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
        .find(|t| t.tier == "applied")
        .unwrap();
    assert!(applied.same, "{:?}", applied.diff);
    // the same cluster with another spcHash has changed
    raw.cluster["metadata"]["uid"] = Value::from(CLUSTER_UID);
    let again = Snapshot::from_cluster(&raw, None, "a", "t1").unwrap();
    let d = strata::diff::compute(&a, &again, "t1", None);
    let applied = d
        .cluster_tiers
        .iter()
        .find(|t| t.tier == "applied")
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
        .fetch_cluster(CLUSTER_UID, &Progress::silent())
        .unwrap();
    raw.cluster["spec"]["cloudConfigRef"]["kind"] = Value::from(kind);
    raw.cloud_config = Some(palette.cloud_config(&raw.cluster).unwrap());
    let snap = Snapshot::from_cluster(&raw, None, "cluster", "t0").unwrap();
    snap.source.cluster.unwrap().tiers["shape"].fields.clone()
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
    let mut doc = read(&path);
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
    let shape = v.diff.cluster_tiers.iter().find(|t| t.tier == "shape");
    assert!(shape.unwrap().same, "{kind}: {:?}", shape.unwrap().diff);
    // the live record holds the fields of its cloud, and compares with the old record
    let live = &v.live.source.cluster.as_ref().unwrap().tiers["shape"].fields;
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
        .find(|t| t.tier == "shape")
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
            pack_summary("pk-63", "1.63.1", "reg-public", "sha256:aaa", CSI_IMAGES_63),
            pack_summary("pk-65", "1.65.0", "reg-public", "sha256:bbb", CSI_IMAGES_65),
        ],
        &[("reg-public", public_entity())],
    )
}

#[test]
fn a_record_carries_the_images_and_the_charts_of_a_pack() {
    let api = serve_image_versions();
    let old = snapshot_pack(&api, "pack:csi-aws-ebs@1.63.1").unwrap();
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
    let plain = snapshot_pack(
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
    let a = snapshot_pack(&api, "pack:csi-aws-ebs@1.63.1").unwrap();
    let b = snapshot_pack(&api, "pack:csi-aws-ebs@1.65.0").unwrap();
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&d, &a, &b, false)
        .unwrap();
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
}

#[test]
fn a_record_written_before_the_image_lists_gets_them_from_its_values_and_still_verifies() {
    let api = serve_image_versions();
    let fresh = snapshot_pack(&api, "pack:csi-aws-ebs@1.63.1").unwrap();
    let mut doc = serde_json::to_value(&fresh).unwrap();
    let pack = doc["packs"][0].as_object_mut().unwrap();
    pack.remove("images");
    pack.remove("charts");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.json");
    fs::write(&path, serde_json::to_string(&doc).unwrap()).unwrap();
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
            pack_summary(
                "pk-63",
                "1.63.1",
                "reg-public",
                "sha256:aaa",
                &values("1.63.1", "v1.63.1"),
            ),
            pack_summary(
                "pk-65",
                "1.65.0",
                "reg-public",
                "sha256:bbb",
                &values("1.65.0", "v1.65.0"),
            ),
        ],
        &[("reg-public", public_entity())],
    );
    let a = snapshot_pack(&api, "pack:csi-aws-ebs@1.63.1").unwrap();
    let b = snapshot_pack(&api, "pack:csi-aws-ebs@1.65.0").unwrap();
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

/// The update that waits for cluster-b: the templates and the manifests of cluster-a, and the
/// profile document of a.
fn serve_pending(api: MemoryApi, raw_root: &Path) -> MemoryApi {
    let a = raw_root.join("cluster-a");
    let templates = read(&a.join("cluster.json"))["spec"]["clusterProfileTemplates"].clone();
    let manifests = read(&a.join("manifests.json"))["profiles"].clone();
    let mut api = api.with(
        format!("/v1/spectroclusters/{CLUSTER_UID}/profileUpdates"),
        serde_json::json!({ "profiles": templates }),
    );
    for (t, m) in templates
        .as_array()
        .unwrap()
        .iter()
        .zip(manifests.as_array().unwrap())
    {
        let uid = t["uid"].as_str().unwrap();
        api = api.with(
            format!("/v1/clusterprofiles/{uid}/packs/manifests"),
            m.clone(),
        );
        api = api.with(
            format!("/v1/clusterprofiles/{uid}"),
            read(&raw_root.join("a").join("profile.json")),
        );
    }
    api
}

fn pending_ref() -> Ref {
    Ref::Cluster {
        uid: CLUSTER_UID.into(),
        profile: None,
        pending: true,
    }
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
    );
    let now = snapshot_cluster(&api, None).unwrap();
    let pending = ops::snapshot_for(
        &pending_ref(),
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
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
        (c.updates_pending, c.updates_pending_error.as_deref()),
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
        format!("/v1/spectroclusters/{CLUSTER_UID}/profileUpdates"),
        serde_json::json!({ "profiles": [] }),
    );
    let v = ops::verify(&pending, &Palette::new(taken), "t1", &Progress::silent()).unwrap();
    assert!(!v.unchanged, "no update waits, so no profile is pending");
}

// ---- edge hosts -------------------------------------------------------------------------------

const EDGE_HOST_SEARCH: &str = "POST /v1/dashboard/edgehosts/search?limit=50";

/// An edge host `e1`, `edge-1`, bound to the profile of cluster-a, with the manifests, the
/// variables, and the document of that profile, and the search that lists it.
fn serve_edge_host(api: MemoryApi, raw_root: &Path) -> MemoryApi {
    let a = raw_root.join("cluster-a");
    let templates = read(&a.join("cluster.json"))["spec"]["clusterProfileTemplates"].clone();
    let manifests = read(&a.join("manifests.json"))["profiles"].clone();
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
    let mut api = api.with("/v1/edgehosts/e1", host).with(
        EDGE_HOST_SEARCH,
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
        let uid = t["uid"].as_str().unwrap();
        api = api
            .with(
                format!("/v1/clusterprofiles/{uid}/packs/manifests"),
                m.clone(),
            )
            .with(
                format!("/v1/clusterprofiles/{uid}"),
                read(&raw_root.join("a").join("profile.json")),
            )
            .with(
                format!("/v1/clusterprofiles/{uid}/variables"),
                read(&raw_root.join("a").join("variables.json")),
            );
    }
    api
}

fn snapshot_edge_host(api: &MemoryApi, host: &str) -> strata::Result<Snapshot> {
    ops::snapshot_for(
        &Ref::parse(&format!("edgehost:{host}")),
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
}

#[test]
fn an_edge_host_record_holds_the_profiles_bound_to_it() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded_a = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let api = serve_edge_host(MemoryApi::new(), dir.path());
    let host = snapshot_edge_host(&api, "e1").unwrap();
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
    let by_name = snapshot_edge_host(&api, "edge-1").unwrap();
    assert_eq!(by_name.root, host.root);
    assert_eq!(by_name.source.cluster.unwrap().uid, "e1");
    let err = snapshot_edge_host(&api, "edge-9").unwrap_err();
    assert_eq!(err.kind(), "edge-host-not-found");

    // verify reads the edge host again, and not a cluster of that uid
    let v = ops::verify(&host, &Palette::new(api.clone()), "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged, "{:?}", v.status);
    // the host is bound to another profile: its record drifts
    let b =
        read(&dir.path().join("cluster-b").join("cluster.json"))["spec"]["clusterProfileTemplates"]
            .clone();
    let b_manifests =
        read(&dir.path().join("cluster-b").join("manifests.json"))["profiles"][0].clone();
    let b_uid = b[0]["uid"].as_str().unwrap().to_string();
    let rebound = serve_edge_host(MemoryApi::new(), dir.path())
        .with(
            "/v1/edgehosts/e1",
            serde_json::json!({
                "metadata": {"uid": "e1", "name": "edge-1"},
                "spec": {"clusterProfileTemplates": b},
            }),
        )
        .with(
            format!("/v1/clusterprofiles/{b_uid}/packs/manifests"),
            b_manifests,
        )
        .with(
            format!("/v1/clusterprofiles/{b_uid}"),
            read(&dir.path().join("b").join("profile.json")),
        );
    let v = ops::verify(&host, &Palette::new(rebound), "t1", &Progress::silent()).unwrap();
    assert!(!v.unchanged);
}

/// The profile of cluster-a with the content of manifest `extra-rules` replaced by `content`, as
/// `GET /v1/clusterprofiles/{uid}/packs/manifests` gives it.
fn manifests_with_rules(raw_root: &Path, content: &str) -> Value {
    let mut m = read(&raw_root.join("cluster-a").join("manifests.json"))["profiles"][0].clone();
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
    let rules = read(&a.join("manifests.json"))["profiles"][0]["spec"]["packs"]
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
    let mut templates = read(&a.join("cluster.json"))["spec"]["clusterProfileTemplates"].clone();
    let puid = templates[0]["uid"].as_str().unwrap().to_string();
    for pack in templates[0]["packs"].as_array_mut().unwrap() {
        if pack["name"] == "prometheus-operator" {
            pack["manifests"] = serde_json::json!([{"uid": "host-rules-1", "name": "extra-rules"}]);
        }
    }
    let host = |copy: &str, profile: &str| {
        serve_edge_host(MemoryApi::new(), dir.path())
            .with(
                "/v1/edgehosts/e1",
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
                format!("/v1/clusterprofiles/{puid}/packs/manifests"),
                manifests_with_rules(dir.path(), profile),
            )
    };
    let record = snapshot_edge_host(&host(&rules, &rules), "e1").unwrap();
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
    let rules = read(&a.join("manifests.json"))["profiles"][0]["spec"]["packs"]
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
    let mut templates = read(&a.join("cluster.json"))["spec"]["clusterProfileTemplates"].clone();
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
                "/v1/clustergroups/g1",
                serde_json::json!({
                    "metadata": {"uid": "g1", "name": "group-1"},
                    "spec": {"type": "hostCluster", "clusterProfileTemplates": templates.clone()},
                }),
            )
            .with(
                "POST /v1/dashboard/clustergroups/g1/virtualClusters?limit=50",
                serde_json::json!({"items": [], "listmeta": {"continue": ""}}),
            )
            .with(
                format!(
                    "/v1/clusterprofiles/{puid}/packs/prometheus-operator/manifests/group-rules-1"
                ),
                own_copy("group-rules-1", copy),
            )
            .with(
                format!("/v1/clusterprofiles/{puid}/packs/manifests"),
                manifests_with_rules(dir.path(), profile),
            )
    };
    let snap = |api: MemoryApi| {
        ops::snapshot_for(
            &Ref::parse("clustergroup:g1"),
            || Ok(Palette::new(api.clone())),
            "t0",
            &Progress::silent(),
        )
    };
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
    let api = serve_edge_host(serve(MemoryApi::new(), dir.path(), "a", UID_A), dir.path());
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
            host.kind.as_str(),
            host.uid.as_str(),
            host.name.as_str(),
            host.project_uid.as_str(),
            host.state.as_str(),
            host.version.as_str()
        ),
        ("edgehost", "e1", "edge-1", "p1", "ready", "1.2.0")
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

    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
    assert!(
        text.contains("edge-1  e1  project p1  ready  edge host"),
        "{text}"
    );
    assert!(text.ends_with("  edge hosts: 1 read\n"), "{text}");
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
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
    let mut profile = read(&dir.path().join("a").join("profile.json"));
    profile["status"]["inUseClusterUids"] = serde_json::json!([]);
    profile["status"]["inUseClusters"] = serde_json::json!([]);
    profile["status"]["inUseClusterTemplates"] = serde_json::json!([
        {"uid": "tmpl-1", "name": "barney-template", "projectUid": "p1", "kind": "clustertemplate"}
    ]);
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A)
        .with(format!("/v1/clusterprofiles/{UID_A}"), profile);
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
                t.kind.as_str(),
                t.uid.as_str(),
                t.name.as_str(),
                t.project_uid.as_str(),
                t.state.as_str(),
                t.version.as_str()
            ),
            ("template", "tmpl-1", "barney-template", "p1", "", "")
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
    assert!(
        text.contains("barney-template  tmpl-1  project p1  cluster template\n"),
        "{text}"
    );
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
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

/// The template of the add-on profile of cluster-merged, and its place in the templates of the
/// cluster.
fn merged_addon_template(raw_root: &Path) -> (usize, Value) {
    let (_, addon) = merged_uids(raw_root);
    let templates = read(&raw_root.join("cluster-merged").join("cluster.json"))["spec"]
        ["clusterProfileTemplates"]
        .clone();
    templates
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .enumerate()
        .find(|(_, t)| t["uid"] == addon.as_str())
        .unwrap()
}

/// The document of the cluster group `g1`, `group-1`, that pins the add-on profile of
/// cluster-merged.
fn group_doc(raw_root: &Path) -> Value {
    let (_, template) = merged_addon_template(raw_root);
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
    let manifests = read(&m.join("manifests.json"))["profiles"].clone();
    let (i, _) = merged_addon_template(raw_root);
    serve_cluster(api, raw_root, "cluster-merged")
        .with("/v1/clustergroups/g1", group_doc(raw_root))
        .with(
            "/v1/clustergroups/hostCluster/metadata",
            serde_json::json!({"items": [{"uid": "g1", "name": "group-1", "scope": "project"}]}),
        )
        .with(
            "POST /v1/dashboard/clustergroups/g1/virtualClusters?limit=50",
            serde_json::json!({"items": [{"metadata": {"uid": CLUSTER_UID, "name": "lab-cluster",
                "annotations": {"projectUid": "p1"}}}], "listmeta": {"continue": ""}}),
        )
        .with(
            format!("/v1/clusterprofiles/{addon}/packs/manifests"),
            manifests[i].clone(),
        )
        .with(
            format!("/v1/clusterprofiles/{addon}/variables"),
            read(&raw_root.join("addons").join("variables.json")),
        )
}

#[test]
fn a_cluster_group_record_holds_the_pin_and_compares_against_a_member() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let (infra, addon) = merged_uids(dir.path());
    let api = serve_cluster_group(MemoryApi::new(), dir.path());
    let connect = || Ok(Palette::new(api.clone()));
    let group = ops::snapshot_for(
        &Ref::parse("clustergroup:g1"),
        connect,
        "t0",
        &Progress::silent(),
    )
    .unwrap();
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
    let by_name = ops::snapshot_for(
        &Ref::parse("clustergroup:group-1"),
        connect,
        "t0",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(by_name.root, group.root);
    let err = ops::snapshot_for(
        &Ref::parse("clustergroup:group-9"),
        connect,
        "t0",
        &Progress::silent(),
    )
    .unwrap_err();
    assert_eq!(err.kind(), "cluster-group-not-found");

    // compare with the member: the cluster side holds only the pinned profile, in either order
    let member = Ref::Cluster {
        uid: CLUSTER_UID.into(),
        profile: None,
        pending: false,
    };
    let group_ref = Ref::parse("clustergroup:g1");
    for (a, b) in [(&group_ref, &member), (&member, &group_ref)] {
        let (sa, sb) = ops::snapshot_pair(a, b, connect, "t1", &Progress::silent()).unwrap();
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
        connect,
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

#[test]
fn a_cluster_group_record_with_macro_digests_reads_the_values_of_the_group() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let (_, addon) = merged_uids(dir.path());
    // the pin reads the macro region in the values of its first pack
    let base = serve_cluster_group(MemoryApi::new(), dir.path());
    let mut doc = base.get_json("/v1/clustergroups/g1").unwrap();
    let pack = &mut doc["spec"]["clusterProfileTemplates"][0]["packs"][0];
    let values = pack["values"].as_str().unwrap_or_default().to_string();
    pack["values"] = Value::from(format!(
        "{values}\nregion: \"{{{{.spectro.macro.region}}}}\"\n"
    ));
    // only the route of the group answers: a request to the route of a cluster is a 404
    let tenant = |region: &str| {
        base.clone().with("/v1/clustergroups/g1", doc.clone()).with(
            "/v1/clustergroups/g1/packs/resolvedValues",
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
        &Palette::new(base.clone().with("/v1/clustergroups/g1", doc.clone())),
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
    let verdict = render::renderer(Format::Terminal, Paint::off(), false)
        .verdict(ver, &group.source)
        .unwrap();
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
    let mut member_doc = read(&member_path);
    member_doc["spec"]["clusterConfig"] = serde_json::json!({"timezone": "Etc/UTC"});
    fs::write(&member_path, serde_json::to_string(&member_doc).unwrap()).unwrap();
    let serve = |config: Option<Value>| {
        let mut doc = group_doc(dir.path());
        if let Some(c) = config {
            doc["spec"]["clustersConfig"] = c;
        }
        serve_cluster_group(MemoryApi::new(), dir.path()).with("/v1/clustergroups/g1", doc)
    };
    let snap = |api: &MemoryApi| common::snapshot_of(api, "clustergroup:g1").unwrap();
    let api = serve(Some(clusters_config(6000, GROUP_VALUES)));
    let group = snap(&api);
    let tiers = &group.source.cluster.as_ref().unwrap().tiers;
    assert_eq!(tiers.keys().collect::<Vec<_>>(), ["policy"]);
    let f = &tiers["policy"].fields;
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
        commented.source.cluster.as_ref().unwrap().tiers["policy"].hash,
        tiers["policy"].hash
    );

    // values that are not YAML are kept by the hash of their text, with the error
    let broken = snap(&serve(Some(clusters_config(6000, "sync: [\n"))));
    let f = &broken.source.cluster.as_ref().unwrap().tiers["policy"].fields;
    assert_eq!(f["values"], Value::Null);
    assert_eq!(f["valuesHash"].as_str().map(str::len), Some(64));
    assert!(f["valuesParseError"].is_string(), "{f}");

    // a group and its member have no tier to compare: their policy tiers say different things
    let member = Ref::Cluster {
        uid: CLUSTER_UID.into(),
        profile: None,
        pending: false,
    };
    let connect = || Ok(Palette::new(api.clone()));
    let (sa, sb) = ops::snapshot_pair(
        &Ref::parse("clustergroup:g1"),
        &member,
        connect,
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(
        sb.source
            .cluster
            .as_ref()
            .unwrap()
            .tiers
            .contains_key("policy")
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
        let policy = tiers.iter().find(|t| t.tier == "policy").unwrap();
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
        "/v1/appProfiles/ap1",
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&d, &app, &now, false)
        .unwrap();
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&d, a, b, false)
        .unwrap();
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
        (r.profile.uid.as_str(), r.profile.kind.as_str()),
        ("ap1", "app")
    );
    assert_eq!(r.count, 1);
    assert_eq!(r.overwritten, 0);
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(
        json["consumers"][0],
        serde_json::json!({"kind": "app", "uid": "d1", "name": "shop-dev", "projectUid": "p1",
            "state": "Deployed", "version": "1.0.0"})
    );
    assert_eq!(api.calls("/v1/appProfiles/ap1"), 1);
    assert_eq!(api.calls("/v1/appDeployments/d1"), 1);
    assert_eq!(
        api.calls("/v1/dashboard/spectroclusters/meta"),
        0,
        "no second reading: the listing of the app deployments filters no profile"
    );
    assert!(r.clusters_read.skipped && r.edge_hosts_read.skipped);
    assert_eq!(json["clustersRead"]["reason"], ops::APP_NO_SECOND_READING);
    assert_eq!(json["edgeHostsRead"]["reason"], ops::APP_NO_EDGE_HOSTS);

    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
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
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
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
    std::fs::write(&path, record.to_json(false).unwrap()).unwrap();
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
    let mut doc: Value = moved.get_json("/v1/appProfiles/ap1").unwrap();
    doc["status"]["inUseApps"].as_array_mut().unwrap().push(
        serde_json::json!({"kind": "appdeployment", "uid": "d2", "name": "shop-qa",
            "projectUid": "p2"}),
    );
    let moved = moved.with("/v1/appProfiles/ap1", doc);
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
    let mut doc: Value = api.get_json("/v1/appProfiles/ap1").unwrap();
    doc["status"]["inUseApps"] = serde_json::json!([]);
    let free = api.clone().with("/v1/appProfiles/ap1", doc);
    let r = consumers(&free, &Ref::parse("appprofile:shop@1.0.0"));
    assert_eq!((r.count, r.exit_code()), (0, 0));
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
    assert!(
        text.starts_with("shop@1.0.0 (uid ap1): no consumers\n"),
        "{text}"
    );
}

// ---- behind -----------------------------------------------------------------------------------

/// A version of csi-aws-ebs in the Public Repo with a state, or for other clouds.
fn pack_summary_with(version: &str, state: Option<&str>, clouds: &[&str]) -> Value {
    let mut s = pack_summary(
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
            pack_summary_with("1.64.0", Some("deprecated"), &["aws"]),
            pack_summary_with("1.66.0", Some("disabled"), &["aws"]),
            pack_summary_with("1.67.0", None, &["gcp"]),
        ],
        &[("reg-public", public_entity())],
    )
}

fn behind_of(
    api: &MemoryApi,
    records: &[std::path::PathBuf],
    options: &ops::BehindOptions,
) -> ops::BehindReport {
    ops::behind(
        records,
        options,
        |_| Ok(Palette::new(api.clone())),
        "2026-09-26T00:00:00Z",
        &Progress::silent(),
    )
    .unwrap()
}

#[test]
fn behind_names_the_newer_versions_of_each_pin_and_the_impact_of_the_bump() {
    let dir = tempfile::tempdir().unwrap();
    let api = serve_behind();
    let write = |name: &str, snap: &Snapshot| {
        let path = dir.path().join(name);
        fs::write(&path, snap.to_json(false).unwrap()).unwrap();
        path
    };
    let old = snapshot_pack(&api, "pack:csi-aws-ebs@1.63.1").unwrap();
    let new = snapshot_pack(&api, "pack:csi-aws-ebs@1.65.0").unwrap();
    let records = vec![
        write("dev.json", &old),
        write("prod.json", &old),
        write("next.json", &new),
    ];
    let r = behind_of(
        &api,
        &records,
        &ops::BehindOptions {
            impact: true,
            since: None,
        },
    );
    assert_eq!(r.schema, "strata-behind/1");
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
            .map(|n| (n.version.as_str(), n.state.as_deref()))
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
        bump.flags.contains(&"IDENTITY".to_string()),
        "{:?}",
        bump.flags
    );
    assert_eq!(bump.impact, strata::impact::ImpactLevel::ClusterWide);
    let current = &r.pins[1];
    assert_eq!(current.version, "1.65.0");
    assert!(!current.behind && current.bump.is_none());

    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_behind(&r)
        .unwrap();
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
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render_behind(&r)
        .unwrap();
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

#[test]
fn a_floating_pin_is_behind_only_what_its_tag_cannot_reach_and_a_pin_that_cannot_be_read_is_an_error()
 {
    let dir = tempfile::tempdir().unwrap();
    let api = serve_behind();
    let mut floating = snapshot_pack(&api, "pack:csi-aws-ebs@1.63.1").unwrap();
    floating.packs[0].tag = "1.x".into();
    let mut narrow = snapshot_pack(&api, "pack:csi-aws-ebs@1.63.1").unwrap();
    narrow.packs[0].tag = "1.63.x".into();
    let mut lost = snapshot_pack(&api, "pack:csi-aws-ebs@1.63.1").unwrap();
    lost.packs[0].registry_uid = "reg-gone".into();
    let mut records = Vec::new();
    for (name, snap) in [
        ("floating.json", &floating),
        ("narrow.json", &narrow),
        ("lost.json", &lost),
    ] {
        let path = dir.path().join(name);
        fs::write(&path, snap.to_json(false).unwrap()).unwrap();
        records.push(path);
    }
    let r = behind_of(&api, &records, &ops::BehindOptions::default());
    // 1.x names 1.65.0 now: the tag reaches each newer version for AWS, so it is not behind
    let float = r.pins.iter().find(|p| p.tag == "1.x").unwrap();
    assert!(
        float.floating && !float.behind && float.newer.is_empty(),
        "{float:?}"
    );
    assert_eq!(float.resolved.as_deref(), Some("1.65.0"));
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
    assert_eq!((r.summary.floating, r.summary.errors), (2, 1));
    assert_eq!(r.exit_code(), 2, "a pin that strata could not check");
    assert!(ops::is_floating("LTS__24.x") && ops::is_floating("1.30.x"));
    assert!(!ops::is_floating("1.30.6") && !ops::is_floating("LTS__24.04"));
}

// ---- the fleet --------------------------------------------------------------------------------

const SEARCH: &str = "POST /v1/dashboard/spectroclusters/search?limit=50";

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
        "/v1/dashboard/spectroclusters/search/schema",
        serde_json::json!({"schema": {"properties": [
            {"name": "clusterName", "type": "string"}, {"name": "tags", "type": "string"}]}}),
    )
    .with(
        SEARCH,
        serde_json::json!({"items": found, "listmeta": {"continue": ""}}),
    )
    .with("/v1/dashboard/spectroclusters/meta", Value::Array(all))
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
        "/v1/dashboard/projects/metadata",
        serde_json::json!({"items": [{"metadata": {"name": "dev", "uid": "p1"}},
            {"metadata": {"name": "prod", "uid": "p2"}}]}),
    )
}

fn fleet_listing(api: &MemoryApi, query: ops::ClusterQuery) -> ops::Listing {
    ops::list(
        &ListOptions {
            kinds: vec![ListKind::Clusters],
            query,
            ..ListOptions::default()
        },
        |_| Ok(Palette::new(api)),
        "t1",
        &Progress::silent(),
    )
    .unwrap()
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
    let l = fleet_listing(
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
    assert_eq!(path, "/v1/dashboard/spectroclusters/search?limit=50");
    let item = &body["filter"]["filterGroups"][0]["filters"][0];
    assert_eq!(item["property"], "tags");
    assert_eq!(item["condition"]["string"]["operator"], "eq");
    assert_eq!(
        item["condition"]["string"]["match"]["values"],
        serde_json::json!(["env:prod"])
    );

    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_list(&l)
        .unwrap();
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
    let l = fleet_listing(
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
        .with(
            "/v1/dashboard/spectroclusters/meta",
            Value::Array(vec![payments, old, search]),
        )
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
    let l = fleet_listing(
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
    let err = ops::list(
        &ListOptions {
            kinds: vec![ListKind::Clusters],
            query: ops::ClusterQuery {
                tags: vec![],
                filter: Some("nope".into()),
            },
            ..ListOptions::default()
        },
        |_| Ok(Palette::new(bare.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap_err();
    assert_eq!(err.kind(), "filter-not-found");

    // list filters names each filter with what it selects
    let l = ops::list(
        &ListOptions {
            kinds: vec![ListKind::Filters],
            ..ListOptions::default()
        },
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_list(&l)
        .unwrap();
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
    let mut doc = read(&dir.join("cluster-a").join("cluster.json"));
    doc["metadata"]["labels"] = serde_json::json!({"env": "prod"});
    let api = serve_fleet(
        serve_cluster(MemoryApi::new(), dir, "cluster-a"),
        vec![lab, gone],
        vec![],
    )
    .with(format!("/v1/spectroclusters/{CLUSTER_UID}"), doc);
    let out = dir.join("fleet");
    fs::create_dir_all(&out).unwrap();
    // a record of a cluster that left the set, and a file that is not a record
    let old = snapshot_cluster(&serve_cluster(MemoryApi::new(), dir, "cluster-a"), None).unwrap();
    fs::write(out.join("left.json"), old.to_json(false).unwrap()).unwrap();
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_set(&report)
        .unwrap();
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
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render_set(&report)
        .unwrap();
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
    let mut doc = read(&dir.join("cluster-a/cluster.json"));
    doc["status"]["state"] = Value::from(state);
    for (k, v) in extra.as_object().unwrap() {
        doc["metadata"][k] = v.clone();
    }
    serve_cluster(MemoryApi::new(), dir, "cluster-a")
        .with(format!("/v1/spectroclusters/{CLUSTER_UID}"), doc)
}

/// The verdict line of a verify, as the terminal prints it.
fn verdict_of(v: &ops::Verification, recorded: &Snapshot) -> String {
    render::renderer(Format::Terminal, Paint::off(), false)
        .verdict(v.diff.verify.as_ref().unwrap(), &recorded.source)
        .unwrap()
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
        verdict_of(&v, &recorded),
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
    let path = dir.path().join("lab.json");
    fs::write(&path, recorded.to_json(true).unwrap()).unwrap();
    let deleted = cluster_in_state(
        dir.path(),
        "Deleted",
        serde_json::json!({"deletionTimestamp": "2026-09-30T10:00:00Z"}),
    );
    let s = ops::verify_many(
        &[path],
        None,
        |_| Ok(Palette::new(deleted.clone())),
        "t1",
        &ops::VerifyOptions::default(),
        &Progress::silent(),
    );
    assert_eq!((s.summary.drifted, s.exit_code()), (1, 1));
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_verify_summary(&s)
        .unwrap();
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
        verdict_of(&v, &recorded),
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
        verdict_of(&v, &recorded),
        "OK: cluster lab-cluster running lab-profile@1.2.0 is unchanged since t0; the cluster is Deleting"
    );
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .verdict(v.diff.verify.as_ref().unwrap(), &recorded.source)
        .unwrap();
    assert!(md.ends_with("; the cluster is Deleting"), "{md}");

    // the state comes last, after the other parts of a drift
    let mut moved_doc = read(&dir.path().join("cluster-b/cluster.json"));
    moved_doc["status"]["state"] = Value::from("Error");
    let moved = serve_cluster(MemoryApi::new(), dir.path(), "cluster-b")
        .with(format!("/v1/spectroclusters/{CLUSTER_UID}"), moved_doc);
    let v = ops::verify(&recorded, &Palette::new(moved), "t1", &Progress::silent()).unwrap();
    assert!(!v.unchanged);
    let line = verdict_of(&v, &recorded);
    assert!(line.starts_with("DRIFT: "), "{line}");
    assert!(line.ends_with("; the cluster is Error"), "{line}");

    // over several records, the line of each record names the state
    let path = dir.path().join("lab.json");
    fs::write(&path, recorded.to_json(true).unwrap()).unwrap();
    let deleting = cluster_in_state(dir.path(), "Deleting", serde_json::json!({}));
    let s = ops::verify_many(
        &[path],
        None,
        |_| Ok(Palette::new(deleting.clone())),
        "t1",
        &ops::VerifyOptions::default(),
        &Progress::silent(),
    );
    assert_eq!(s.records[0].status, RecordStatus::Unchanged);
    assert_eq!(s.records[0].state.as_deref(), Some("Deleting"));
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_verify_summary(&s)
        .unwrap();
    assert!(
        text.contains("unchanged since t0; the state is Deleting"),
        "{text}"
    );
}

#[test]
fn verify_reports_a_gone_edge_host_or_cluster_group_as_drift_and_not_as_an_error() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let host = snapshot_edge_host(&serve_edge_host(MemoryApi::new(), dir.path()), "e1").unwrap();
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
        assert_eq!(verdict_of(&v, recorded), line);
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
    let line = verdict_of(&v, &host);
    assert!(line.ends_with("; the edge host is ready"), "{line}");
    for (active, suffix) in [(true, ""), (false, "; the cluster group is inactive")] {
        let api = serve_cluster_group(MemoryApi::new(), dir.path());
        let mut doc = api.get_json("/v1/clustergroups/g1").unwrap();
        doc["status"] = serde_json::json!({"isActive": active});
        let api = api.with("/v1/clustergroups/g1", doc);
        let v = ops::verify(&group, &Palette::new(api), "t1", &Progress::silent()).unwrap();
        assert!(v.unchanged);
        let line = verdict_of(&v, &group);
        assert!(line.ends_with(&format!("since t0{suffix}")), "{line}");
    }
}

#[test]
fn verify_reports_a_renamed_version_as_renamed_and_not_as_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    // PATCH /v1/clusterprofiles/{uid}/metadata renames the profile: the uid and the version stay
    let mut doc = read(&dir.path().join("a/profile.json"));
    doc["metadata"]["name"] = Value::from("lab-profile-renamed");
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A)
        .with(format!("/v1/clusterprofiles/{UID_A}"), doc);
    let v = ops::verify(&recorded, &Palette::new(api), "t1", &Progress::silent()).unwrap();
    assert!(!v.unchanged);
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.status, VerifyStatus::Drifted);
    assert_eq!(ver.renamed_to.as_deref(), Some("lab-profile-renamed@1.2.0"));
    assert!(ver.overwritten_by.is_none());
    let line = verdict_of(&v, &recorded);
    assert_eq!(
        line,
        "DRIFT: published version lab-profile@1.2.0 was renamed to lab-profile-renamed@1.2.0"
    );
    assert_eq!(
        serde_json::to_value(ver).unwrap()["renamedTo"],
        Value::from("lab-profile-renamed@1.2.0")
    );

    // a legacy version bump still overwrites the version in place
    let mut doc = read(&dir.path().join("a/profile.json"));
    doc["spec"]["version"] = Value::from("1.3.0");
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A)
        .with(format!("/v1/clusterprofiles/{UID_A}"), doc);
    let v = ops::verify(&recorded, &Palette::new(api), "t1", &Progress::silent()).unwrap();
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.overwritten_by.as_deref(), Some("lab-profile@1.3.0"));
    assert!(ver.renamed_to.is_none());
    assert!(
        verdict_of(&v, &recorded).contains("was overwritten in place"),
        "{}",
        verdict_of(&v, &recorded)
    );
}

// ---- snapshot --validate ---------------------------------------------------------------

#[test]
fn snapshot_validate_asks_palette_whether_it_takes_the_packs_of_a_candidate() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded_b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    let file = dir.path().join("c.yaml");
    candidate::write(&Candidate::from_snapshot(&recorded_b).unwrap(), &file, None).unwrap();
    let registry = serve_registry_for_b(dir.path());
    let path = "POST /v1/clusterprofiles/validate/packs";
    let message = "Pack 'strata-live-dep' is required with minimum compatible version '1.0.0'";
    let answer = serde_json::json!({"packs": {"results": [{"name": "kubernetes",
        "displayName": "kubernetes",
        "errors": [{"code": "DependencyPackWithMinVersionRequired", "message": message}]}]}});
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
    assert!(!v.accepts);
    assert_eq!(v.errors.len(), 1, "{v:?}");
    assert_eq!(
        v.errors[0].line(),
        format!("DependencyPackWithMinVersionRequired kubernetes: {message}")
    );
    // one POST to the route of a create, with the packs of the candidate in profile order
    let posted = api.posted();
    assert_eq!(posted.len(), 1);
    assert_eq!(posted[0].0, "/v1/clusterprofiles/validate/packs");
    assert_eq!(posted[0].1, ops::profile_validate_body(&snap));
    let body = &posted[0].1;
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
        errors: v.errors.clone(),
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
    let path = "POST /v1/clusterprofiles/validate/packs";
    // Palette sends each refusal of the whole request as HTTP 500
    for code in strata::error::PROFILE_PACK_REFUSAL_CODES {
        let body = format!(r#"{{"code":"{code}","message":"refused"}}"#);
        // the retry does not send it again: Palette gives the same refusal
        let error = Error::Api {
            method: "POST".into(),
            path: path.into(),
            status: 500,
            body: body.clone(),
            retry_after: None,
        };
        assert_eq!(error.class(), FailureClass::Terminal, "{code}");
        let api = MemoryApi::new().failing(path, 500, body);
        let v = ops::profile_validation(&Palette::new(&api), &candidate).unwrap();
        assert!(!v.accepts, "{code}");
        assert_eq!(v.errors.len(), 1);
        assert_eq!(v.errors[0].pack, "");
        assert_eq!(v.errors[0].line(), format!("{code}: refused"));
    }
    // a 500 with another code is a fault of Palette, a 403 is the key, and a 400 is a body with
    // an error of strata: errors of strata
    for (status, code) in [
        (500, "InternalError"),
        (403, "Forbidden"),
        (400, "InvalidPayload"),
    ] {
        let body = format!(r#"{{"code":"{code}"}}"#);
        let api = MemoryApi::new().failing(path, status, body);
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
    let route = format!("/v1/clusterprofiles/{}/validate/packs", b.source.uid);
    let api = MemoryApi::new().with(format!("POST {route}"), serde_json::json!({}));
    let v = ops::profile_validation(&Palette::new(&api), &b).unwrap();
    assert!(v.accepts);
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
    serve(MemoryApi::new(), raw_root, dir, UID_A)
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
        render::renderer(Format::Terminal, Paint::off(), false)
            .render(d, a, b, false)
            .unwrap()
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
    let md = render::renderer(Format::Markdown, Paint::off(), false)
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
    let verdict = render::renderer(Format::Terminal, Paint::off(), false)
        .verdict(ver, &recorded.source)
        .unwrap();
    assert!(verdict.starts_with("OK: "), "{verdict}");
    assert!(
        verdict.contains(&format!("\nINVALID: {sentence}")),
        "{verdict}"
    );
    let verdict = render::renderer(Format::Markdown, Paint::off(), false)
        .verdict(ver, &recorded.source)
        .unwrap();
    assert!(
        verdict.contains(&format!("**INVALID**: {sentence}")),
        "{verdict}"
    );
    let report = render::renderer(Format::Terminal, Paint::off(), false)
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
    let path = dir.path().join("invalid.json");
    fs::write(&path, recorded.to_json(true).unwrap()).unwrap();
    let s = ops::verify_many(
        &[path],
        None,
        |_| Ok(Palette::new(live.clone())),
        "t1",
        &ops::VerifyOptions::default(),
        &Progress::silent(),
    );
    assert_eq!(
        (s.records[0].status, s.exit_code()),
        (RecordStatus::Unchanged, 0)
    );
    let lines = render::renderer(Format::Terminal, Paint::off(), false)
        .render_verify_summary(&s)
        .unwrap();
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
    let verdict = render::renderer(Format::Terminal, Paint::off(), false)
        .verdict(ver, &recorded.source)
        .unwrap();
    assert!(!verdict.contains("INVALID"), "{verdict}");
}
