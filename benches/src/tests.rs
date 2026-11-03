//! The tests of the generator, at a size that a test build runs in seconds. The benchmarks run
//! the same code at the size that Palette permits.

use strata::api::Palette;
use strata::ops::{self, RecordStatus, VerifyOptions};
use strata::reference::Ref;
use strata::term::progress::Progress;

use crate::fleet::{VerifyFleet, all_running, consumer_fleet};
use crate::profile::{Change, PROFILE_UID, Shape, raw_profile, snapshot};
use crate::{GENERATED_AT, PALETTE_LIMIT, candidate, real};

#[test]
fn the_real_packs_are_the_five_packs_of_the_examples_with_values_at_each_version_one_time() {
    let families = real::families();
    let names: Vec<&str> = families.iter().map(|f| f.name).collect();
    assert_eq!(
        names,
        [
            "ubuntu-aws",
            "kubernetes",
            "cni-cilium-oss",
            "csi-aws-ebs",
            "cert-manager"
        ]
    );
    for family in families {
        assert!(
            family.versions.len() >= 2,
            "{}: a change needs two versions",
            family.name
        );
        let mut uids: Vec<&str> = family
            .versions
            .iter()
            .map(|v| v.pack_uid.as_str())
            .collect();
        uids.sort_unstable();
        uids.dedup();
        assert_eq!(uids.len(), family.versions.len(), "{}", family.name);
        assert!(family.versions.iter().all(|v| !v.values.is_empty()));
    }
}

#[test]
fn a_profile_has_the_packs_and_the_manifests_of_its_shape_and_each_pack_has_its_own_key() {
    let shape = Shape {
        packs: 12,
        manifests: 5,
    };
    let record = snapshot(shape, Change::None);
    assert_eq!(record.packs.len(), 12);
    let manifests: Vec<usize> = record.packs.iter().map(|p| p.manifests.len()).collect();
    assert_eq!(manifests.iter().sum::<usize>(), 5);
    assert!(
        manifests[..5].iter().all(|&n| n == 1),
        "the first packs hold the manifests"
    );
    let mut keys: Vec<&str> = record.packs.iter().map(|p| p.key.as_str()).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(
        keys.len(),
        12,
        "two packs with one key are one change in a comparison"
    );
    assert_eq!(
        record.root,
        snapshot(shape, Change::None).root,
        "the same call, the same root"
    );
}

#[test]
fn a_change_moves_the_packs_that_it_names_and_no_other_pack() {
    let shape = Shape {
        packs: 12,
        manifests: 12,
    };
    let before = snapshot(shape, Change::None);
    for (change, packs_changed) in [
        (Change::None, 0),
        (Change::OnePack, 1),
        (Change::EveryPack, 12),
    ] {
        let diff = ops::compare(&before, &snapshot(shape, change), GENERATED_AT);
        assert_eq!(diff.summary.packs_changed, packs_changed, "{change:?}");
        assert_eq!(diff.same, packs_changed == 0, "{change:?}");
    }
}

#[test]
fn a_profile_of_the_most_that_palette_takes_has_two_hundred_packs_and_two_hundred_manifests() {
    let raw = raw_profile(Shape::largest(), Change::None);
    let packs = raw
        .profile
        .pointer("/spec/published/packs")
        .and_then(|p| p.as_array());
    assert_eq!(packs.map(Vec::len), Some(PALETTE_LIMIT));
    let entries = raw
        .manifests
        .pointer("/spec/packs")
        .and_then(|p| p.as_array());
    assert_eq!(entries.map(Vec::len), Some(PALETTE_LIMIT));
}

#[test]
fn a_verify_fleet_has_a_record_of_each_cluster_and_palette_still_holds_each_one_as_recorded() {
    let fleet = VerifyFleet::new(3);
    assert_eq!(fleet.records.len(), 3);
    let summary = ops::verify_many(
        &fleet.records,
        None,
        |_| Ok(Palette::new(&fleet.api)),
        GENERATED_AT,
        &VerifyOptions::default(),
        &Progress::silent(),
    );
    assert_eq!(summary.summary.records, 3);
    assert_eq!(
        summary.summary.status,
        RecordStatus::Unchanged,
        "{:?}",
        summary.records
    );
}

#[test]
fn a_consumer_fleet_lists_each_cluster_with_its_state_and_no_error() {
    let api = consumer_fleet(4);
    let report = ops::consumers(
        &Ref::Uid(PROFILE_UID.to_string()),
        |_| Ok(Palette::new(&api)),
        GENERATED_AT,
        &Progress::silent(),
    )
    .expect("a report");
    assert_eq!(report.count, 4);
    assert!(all_running(&report));
}

#[test]
fn a_candidate_of_n_layers_resolves_to_n_packs_against_the_registry_of_the_real_packs() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("candidate.yaml");
    candidate::write(8, &path);
    let loaded = strata::candidate::load(&path).expect("a candidate");
    assert_eq!(loaded.candidate.layers.len(), 8);
    let api = candidate::registry();
    let resolved = strata::candidate::resolve(&loaded, &Palette::new(&api), &Progress::silent())
        .expect("a resolve");
    let packs = resolved
        .raw
        .profile
        .pointer("/spec/published/packs")
        .and_then(|p| p.as_array());
    assert_eq!(packs.map(Vec::len), Some(8));
}
