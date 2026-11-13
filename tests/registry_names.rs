//! The name of the registry of each pack: in a record, in a pack row of `list`, and in the saved
//! answers of `--raw-dir`.
//!
//! The fixtures `a` and `a3` pin the same pack versions from two registries, `PUBLIC_REGISTRY`
//! and `PROD_REGISTRY`. So each pack version is in two registries, and only the uid of the
//! registry on the pack ref says which one a profile takes.

use serde_json::{Value, json};
use strata::api::{MemoryApi, Palette};
use strata::ops::{self, ListOptions, SnapshotOptions};
use strata::reference::Ref;
use strata::snapshot::Snapshot;
use strata::term::progress::Progress;

use crate::common::{
    self, PROD_REGISTRY, PROFILE_LISTING, PUBLIC_REGISTRY, REGISTRY_LISTING, UID_A, listing,
    serve_fixture, snapshot_of_ref,
};

/// The uid that the tests give the fixture `a3`, beside `a` at `UID_A`.
const UID_A3: &str = "64f0000000000000000000a3";

/// The two registries that the key can see, as `GET /v1/registries/metadata` lists them.
fn two_registries() -> Value {
    json!({"items": [
        {"name": "Public Repo", "uid": PUBLIC_REGISTRY, "kind": "pack"},
        {"name": "Prod Repo", "uid": PROD_REGISTRY, "kind": "pack"},
    ]})
}

/// `a` at `UID_A` and `a3` at `UID_A3`, listed as `lab-profile@1.2.0` and `lab-prod@1.2.0`, with
/// the registry listing `registries` when it is given. The fixtures are written under `root`.
fn serve_two_copies(root: &std::path::Path, registries: Option<Value>) -> MemoryApi {
    common::make_fixtures(root);
    let api = serve_fixture(
        serve_fixture(MemoryApi::new(), root, "a", UID_A),
        root,
        "a3",
        UID_A3,
    )
    .with(
        PROFILE_LISTING,
        json!({"items": [
            {"metadata": {"name": "lab-profile", "uid": UID_A},
             "spec": {"cloudType": "aws", "version": "1.2.0"}},
            {"metadata": {"name": "lab-prod", "uid": UID_A3},
             "spec": {"cloudType": "aws", "version": "1.2.0"}},
        ]}),
    );
    match registries {
        Some(listing) => api.with(REGISTRY_LISTING, listing),
        None => api,
    }
}

/// The record of the profile `uid`, read from `api` at `t0`.
fn record(api: &MemoryApi, uid: &str) -> strata::Result<Snapshot> {
    snapshot_of_ref(api, &Ref::Uid(uid.to_string()), "t0")
}

/// The registry name of each pack of `snap`, by key.
fn names(snap: &Snapshot) -> Vec<(&str, &str)> {
    snap.packs
        .iter()
        .map(|p| (p.key.as_str(), p.registry_name.as_str()))
        .collect()
}

#[test]
fn a_record_names_the_registry_that_the_profile_takes_each_pack_from_and_the_root_stays() {
    let dir = tempfile::tempdir().unwrap();
    let api = serve_two_copies(dir.path(), Some(two_registries()));
    let public = record(&api, UID_A).unwrap();
    let prod = record(&api, UID_A3).unwrap();
    assert!(!public.packs.is_empty());
    for (snap, name, uid) in [
        (&public, "Public Repo", PUBLIC_REGISTRY),
        (&prod, "Prod Repo", PROD_REGISTRY),
    ] {
        for p in &snap.packs {
            assert_eq!(
                (p.registry_uid.as_str(), p.registry_name.as_str()),
                (uid, name),
                "{}: the record names the registry of the pack ref",
                p.key
            );
        }
    }
    assert_eq!(
        api.calls(REGISTRY_LISTING),
        2,
        "one read of the registry listing for each record"
    );

    // the name is outside the root: a Palette with no listing gives the same root, and no name
    let dir = tempfile::tempdir().unwrap();
    let unnamed = record(&serve_two_copies(dir.path(), None), UID_A).unwrap();
    assert!(names(&unnamed).iter().all(|(_, n)| n.is_empty()));
    assert_eq!(unnamed.root, public.root);
    assert_eq!(unnamed.content_digest(), public.content_digest());

    let json: Value = serde_json::from_str(&public.to_json(false).unwrap()).unwrap();
    assert_eq!(json["packs"][0]["registryUid"], PUBLIC_REGISTRY);
    assert_eq!(json["packs"][0]["registryName"], "Public Repo");
    let json: Value = serde_json::from_str(&unnamed.to_json(false).unwrap()).unwrap();
    assert!(
        json["packs"][0].get("registryName").is_none(),
        "no name, no field: {}",
        json["packs"][0]
    );
    let path = dir.path().join("public.json");
    std::fs::write(&path, public.to_json(false).unwrap()).unwrap();
    let back = Snapshot::load(&path).unwrap();
    assert_eq!(
        names(&back),
        names(&public),
        "a record reads its names again"
    );
}

#[test]
fn a_registry_that_the_key_cannot_see_has_no_name_and_the_pack_row_gives_its_uid() {
    let dir = tempfile::tempdir().unwrap();
    let only_public = json!({"items": [
        {"name": "Public Repo", "uid": PUBLIC_REGISTRY, "kind": "pack"},
    ]});
    let api = serve_two_copies(dir.path(), Some(only_public));
    let prod = record(&api, UID_A3).unwrap();
    assert!(
        names(&prod).iter().all(|(_, n)| n.is_empty()),
        "{:?}",
        names(&prod)
    );

    let options = ListOptions {
        pack: Some("csi".into()),
        ..ListOptions::default()
    };
    let l = listing(&api, &options).unwrap();
    let text = common::terminal().render_list(&l).unwrap();
    assert!(
        text.contains("    csi    csi-aws-ebs          1.28.0  Public Repo\n"),
        "{text}"
    );
    assert!(
        text.contains(&format!(
            "    csi    csi-aws-ebs          1.28.0  {PROD_REGISTRY}\n"
        )),
        "the uid works in pack:<registry>/<name>@<version>: {text}"
    );
}

#[test]
fn a_pack_row_of_list_names_the_registry_that_each_profile_version_takes_the_pack_from() {
    let dir = tempfile::tempdir().unwrap();
    let api = serve_two_copies(dir.path(), Some(two_registries()));
    let options = ListOptions {
        pack: Some("csi-aws-ebs".into()),
        ..ListOptions::default()
    };
    let l = listing(&api, &options).unwrap();
    assert_eq!(api.calls(REGISTRY_LISTING), 1, "one read for the listing");
    let rows: Vec<(&str, &str, &str, &str)> = l
        .profiles
        .as_ref()
        .unwrap()
        .iter()
        .flat_map(|p| {
            p.packs
                .iter()
                .flatten()
                .filter(|pk| pk.name == "csi-aws-ebs")
                .map(|pk| {
                    (
                        p.reference.as_str(),
                        pk.version.as_str(),
                        pk.registry_uid.as_str(),
                        pk.registry.as_str(),
                    )
                })
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("lab-prod@1.2.0", "1.28.0", PROD_REGISTRY, "Prod Repo"),
            (
                "lab-profile@1.2.0",
                "1.28.0",
                PUBLIC_REGISTRY,
                "Public Repo"
            ),
        ],
        "one pack version, two registries"
    );

    let text = common::terminal().render_list(&l).unwrap();
    for line in [
        "    csi    csi-aws-ebs          1.28.0  Prod Repo\n",
        "    csi    csi-aws-ebs          1.28.0  Public Repo\n",
    ] {
        assert!(text.contains(line), "{line:?} in {text}");
    }
    let md = common::markdown().render_list(&l).unwrap();
    assert!(md.contains("csi/csi-aws-ebs 1.28.0 in Prod Repo"), "{md}");
    assert!(md.contains("csi/csi-aws-ebs 1.28.0 in Public Repo"), "{md}");
    let json: Value = serde_json::to_value(&l).unwrap();
    let pack = &json["profiles"][0]["packs"][3];
    assert_eq!(
        (&pack["name"], &pack["registryUid"], &pack["registry"]),
        (
            &json!("csi-aws-ebs"),
            &json!(PROD_REGISTRY),
            &json!("Prod Repo")
        )
    );
}

#[test]
fn a_key_that_cannot_read_the_registry_listing_gets_its_error_and_no_record_or_listing() {
    let dir = tempfile::tempdir().unwrap();
    let api = serve_two_copies(dir.path(), None).failing(REGISTRY_LISTING, 403, "forbidden");
    let err = record(&api, UID_A).unwrap_err();
    assert!(
        matches!(err, strata::Error::Api { status: 403, .. }),
        "a record holds the names, so it does not lose them with no error: {err}"
    );
    let options = ListOptions {
        packs: true,
        ..ListOptions::default()
    };
    let err = listing(&api, &options).unwrap_err();
    assert!(
        matches!(err, strata::Error::Api { status: 403, .. }),
        "{err}"
    );
}

#[test]
fn a_raw_directory_keeps_the_registries_and_from_raw_gives_the_same_names() {
    let fixtures = tempfile::tempdir().unwrap();
    let api = serve_two_copies(fixtures.path(), Some(two_registries()));
    let raw = tempfile::tempdir().unwrap();
    let options = SnapshotOptions {
        raw_dir: Some(raw.path().to_path_buf()),
        ..SnapshotOptions::default()
    };
    let fetched = ops::snapshot_for(
        &Ref::Uid(UID_A3.to_string()),
        &options,
        || Ok(Palette::new(&api)),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
    assert!(raw.path().join("registries.json").is_file());
    let rebuilt = ops::snapshot_from_dir(raw.path(), UID_A3, "t1").unwrap();
    assert_eq!(names(&rebuilt), names(&fetched));
    assert!(names(&rebuilt).iter().all(|(_, n)| *n == "Prod Repo"));
    assert_eq!(rebuilt.root, fetched.root);

    // a directory that an older build saved has no registries.json, and its record no names
    std::fs::remove_file(raw.path().join("registries.json")).unwrap();
    let older = ops::snapshot_from_dir(raw.path(), UID_A3, "t1").unwrap();
    assert!(names(&older).iter().all(|(_, n)| n.is_empty()));
    assert_eq!(older.root, fetched.root);
}
