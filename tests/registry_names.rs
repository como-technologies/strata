//! The name of the registry of each pack: in a record, in a pack row of `list`, and in the saved
//! answers of `--raw-dir`.
//!
//! The fixtures `a` and `a3` pin the same pack versions from two registries, `PUBLIC_REGISTRY`
//! and `PROD_REGISTRY`. So each pack version is in two registries, and only the uid of the
//! registry on the pack ref says which one a profile takes.

use serde_json::{Value, json};
use strata::api::{MemoryApi, Palette};
use strata::closed_set::Open;
use strata::ops::{self, ListKind, ListOptions, SnapshotOptions};
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

/// "What registries do we even have to search?" `list registries` names each registry that the
/// key can see, by name, with its uid and its kind. `--registry` of `packs` and a `pack:` ref take
/// the name or the uid.
#[test]
fn list_registries_names_each_registry_that_the_key_can_see_with_its_uid_and_kind() {
    let api = MemoryApi::new().with(
        REGISTRY_LISTING,
        json!({"items": [
            {"name": "Public Repo", "uid": PUBLIC_REGISTRY, "kind": "pack"},
            {"name": "Bitnami", "uid": "reg-helm", "kind": "helm"},
            {"name": "Palette Registry", "uid": "reg-oci", "kind": "oci"},
            // a kind of a later Palette release keeps its text
            {"name": "Zarf Repo", "uid": "reg-later", "kind": "later-kind"},
            {"name": "Prod Repo", "uid": PROD_REGISTRY},
        ]}),
    );
    let options = ListOptions {
        kinds: vec![ListKind::Registries],
        ..ListOptions::default()
    };
    let l = listing(&api, &options).unwrap();
    assert_eq!(api.calls(REGISTRY_LISTING), 1);
    assert!(l.profiles.is_none() && l.clusters.is_none() && l.projects.is_none());
    let rows: Vec<(&str, &str, &str)> = l
        .registries
        .as_ref()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r.name.as_str(),
                r.uid.as_str(),
                r.kind.as_ref().map_or("", Open::as_str),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("Bitnami", "reg-helm", "helm"),
            ("Palette Registry", "reg-oci", "oci"),
            ("Prod Repo", PROD_REGISTRY, ""),
            ("Public Repo", PUBLIC_REGISTRY, "pack"),
            ("Zarf Repo", "reg-later", "later-kind"),
        ],
        "by name"
    );

    let text = common::terminal().render_list(&l).unwrap();
    for line in [
        "\n5 registries\n",
        "\n  Bitnami           reg-helm    helm\n",
        "\n  Prod Repo         reg-prod    -\n",
        "\n  Zarf Repo         reg-later   later-kind\n",
    ] {
        assert!(text.contains(line), "{line:?} in {text}");
    }
    let md = common::markdown().render_list(&l).unwrap();
    assert!(md.contains("**5 registries**"), "{md}");
    assert!(md.contains("| Bitnami | `reg-helm` | helm |"), "{md}");
    let json: Value = serde_json::to_value(&l).unwrap();
    assert_eq!(
        json["registries"][0],
        json!({"name": "Bitnami", "uid": "reg-helm", "kind": "helm"})
    );
    assert!(json["registries"][2].get("kind").is_none(), "{json}");

    let one = ListOptions {
        kinds: vec![ListKind::Registries],
        ..ListOptions::default()
    };
    let single = MemoryApi::new().with(
        REGISTRY_LISTING,
        json!({"items": [{"name": "Public Repo", "uid": PUBLIC_REGISTRY, "kind": "pack"}]}),
    );
    let text = common::terminal()
        .render_list(&listing(&single, &one).unwrap())
        .unwrap();
    assert!(text.contains("\n1 registry\n"), "{text}");
    let empty = MemoryApi::new().with(REGISTRY_LISTING, json!({"items": []}));
    let text = common::terminal()
        .render_list(&listing(&empty, &one).unwrap())
        .unwrap();
    assert!(text.contains("\nno registries\n"), "{text}");
}

/// The pack rows and the registries read one registry listing. The list of the registries is the
/// answer, so a Palette with no listing is its error, where the pack rows show the uid alone.
#[test]
fn list_reads_the_registry_listing_one_time_and_a_missing_listing_is_an_error_of_the_registries()
{
    let dir = tempfile::tempdir().unwrap();
    let api = serve_two_copies(dir.path(), Some(two_registries()));
    let both = ListOptions {
        kinds: vec![ListKind::Profiles, ListKind::Registries],
        packs: true,
        ..ListOptions::default()
    };
    let l = listing(&api, &both).unwrap();
    assert_eq!(api.calls(REGISTRY_LISTING), 1, "one read for both");
    assert_eq!(l.registries.as_ref().unwrap().len(), 2);
    let pack = &l.profiles.as_ref().unwrap()[0].packs.as_ref().unwrap()[3];
    assert_eq!(pack.registry, "Prod Repo");

    let api = serve_two_copies(dir.path(), None).failing(REGISTRY_LISTING, 404, "not found");
    let packs = ListOptions {
        kinds: vec![ListKind::Profiles],
        packs: true,
        ..ListOptions::default()
    };
    let l = listing(&api, &packs).unwrap();
    let pack = &l.profiles.as_ref().unwrap()[0].packs.as_ref().unwrap()[3];
    assert_eq!((pack.registry.as_str(), pack.registry_uid.as_str()), ("", PROD_REGISTRY));
    let err = listing(&api, &both).unwrap_err();
    assert!(
        matches!(err, strata::Error::Api { status: 404, .. }),
        "{err}"
    );
}

/// `ops::list` refuses `ListKind::Packs` with `Usage`, before it connects, as the command line
/// does.
#[test]
fn list_refuses_the_kind_packs_with_usage_before_it_connects() {
    let options = ListOptions {
        kinds: vec![ListKind::Profiles, ListKind::Packs],
        ..ListOptions::default()
    };
    let mut connects = 0;
    let err = ops::list(
        &options,
        |_| -> strata::Result<Palette<MemoryApi>> {
            connects += 1;
            Ok(Palette::new(MemoryApi::new()))
        },
        "t1",
        &Progress::silent(),
    )
    .unwrap_err();
    assert_eq!(connects, 0);
    let strata::Error::Usage { message } = err else {
        panic!("{err}")
    };
    assert!(
        message.contains("`strata list --detail`") && message.contains("`strata packs <name>`"),
        "{message}"
    );
}
