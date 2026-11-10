//! The targets of a record, of a comparison, and of a raw directory, from bytes.

use strata::api::{RawCluster, RawProfile};
use strata::diff::{Diff, ImpactLevel, Status};
use strata::ops;
use strata::render::{Format, renderer};
use strata::snapshot::Snapshot;
use strata::term::color::Paint;

use crate::support::{
    AGAIN_FILE, AT, RECORD_FILE, answer, cluster_files, parts, profile_files, scratch, write,
};

/// Make sure that `a` and `b`, two records of one profile, compare as one. The roots are equal,
/// and both records hold the same packs. The order of the packs and the variables are the
/// same, and no macro or image differs. The comparison reads the packs, the summary, the order,
/// and the variables, and not only the root. Each renderer shows the comparison.
///
/// `a` and `b` must be two values. A record that strata compares with itself has the same root,
/// the same packs, and the same variables for any reader. So the comparison shows nothing.
#[track_caller]
pub(crate) fn compares_equal(a: &Snapshot, b: &Snapshot) {
    let diff = ops::compare(a, b, AT);
    assert_nothing_differs(&diff);
    for &format in Format::ALL {
        let shown = renderer(format, Paint::off(), false).render(&diff, a, b, true);
        assert!(shown.is_ok(), "{format:?} cannot show a record: {shown:?}");
    }
}

/// Make sure that `diff` shows no difference in any of its parts.
#[track_caller]
fn assert_nothing_differs(diff: &Diff) {
    assert!(
        diff.same && diff.same_content && diff.nothing_differs() && diff.root_a == diff.root_b,
        "the roots of two records of one profile differ: {} and {}",
        diff.root_a,
        diff.root_b
    );
    assert!(!diff.order_changed, "the order of the packs changed");
    assert!(!diff.edited_in_place, "the profile is edited in place");
    assert!(
        !diff.variables.changed && diff.variables.diff.is_empty(),
        "the variables differ: {:?}",
        diff.variables.diff
    );
    for pack in &diff.packs {
        assert!(
            pack.status == Status::Unchanged
                && !(pack.identity || pack.upstream || pack.authored || pack.params_changed),
            "the pack {} differs: {:?}",
            pack.key,
            pack.status
        );
    }
    let summary = &diff.summary;
    let counts = [
        summary.packs_added,
        summary.packs_removed,
        summary.packs_changed,
        summary.identity_changes,
        summary.upstream_changes,
        summary.authored_changes,
        summary.images_changed,
        summary.charts_changed,
    ];
    assert_eq!(counts, [0; 8], "the summary counts a change");
    assert_eq!(summary.packs_unchanged, diff.packs.len());
    assert!(!summary.variables_changed);
    assert_eq!(summary.max_impact, ImpactLevel::None);
}

/// Make sure that `record`, written and read again, is the record. Its JSON loads as a record with
/// the same root and the same content digest. The JSON of that record is the same JSON again.
/// Return the record that the JSON loaded.
#[track_caller]
pub(crate) fn writes_and_reads_again(record: &Snapshot) -> Snapshot {
    let json = answer(record.to_json(false)).expect("a record is written as JSON");
    let dir = scratch();
    write(&dir, RECORD_FILE, json.as_bytes());
    let back = answer(Snapshot::load(&dir.join(RECORD_FILE)))
        .unwrap_or_else(|| panic!("strata cannot load the record that it wrote:\n{json}"));
    assert_eq!(
        back.root, record.root,
        "the root changes when strata writes a record and reads it"
    );
    assert_eq!(back.content_digest(), record.content_digest());
    let again = answer(back.to_json(false)).expect("a loaded record is written as JSON");
    write(&dir, AGAIN_FILE, again.as_bytes());
    let third = answer(Snapshot::load(&dir.join(AGAIN_FILE))).expect("the second JSON loads");
    assert_eq!(
        answer(third.to_json(false)).as_deref(),
        Some(again.as_str()),
        "the JSON of a record is not stable after one write and one read"
    );
    back
}

/// Make sure that `record` holds what each record holds, whatever it describes. It writes and
/// reads again. It compares equal to the record that the read gives.
#[track_caller]
pub(crate) fn check_record(record: &Snapshot) {
    let back = writes_and_reads_again(record);
    compares_equal(record, &back);
}

/// Load `bytes` as a record file. Return true when the file is a record.
pub(crate) fn snapshot(bytes: &[u8]) -> bool {
    let dir = scratch();
    write(&dir, RECORD_FILE, bytes);
    let Some(record) = answer(Snapshot::load(&dir.join(RECORD_FILE))) else {
        return false;
    };
    check_record(&record);
    true
}

#[test]
fn a_record_file_holds_what_each_record_holds_or_is_refused_with_a_kind() {
    bolero::check!().for_each(|bytes| {
        snapshot(bytes);
    });
}

/// Read `bytes` as the document `strata-diff/1`. Return true when the bytes are a comparison.
pub(crate) fn comparison(bytes: &[u8]) -> bool {
    let Ok(diff) = serde_json::from_slice::<Diff>(bytes) else {
        return false;
    };
    let _ = diff.nothing_differs();
    let json = serde_json::to_value(&diff).expect("a comparison serializes");
    let back: Diff =
        serde_json::from_value(json.clone()).expect("a comparison that was written reads");
    assert_eq!(
        serde_json::to_value(&back).expect("a comparison serializes"),
        json,
        "a comparison changes when strata writes it and reads it"
    );
    true
}

#[test]
fn a_comparison_writes_the_document_it_reads() {
    bolero::check!().for_each(|bytes| {
        comparison(bytes);
    });
}

/// Write the parts of `bytes` as the files `names` of a directory, and give the directory. An
/// empty part is a file that the directory does not have. A raw directory of an older build has
/// no file for a document that a newer build adds.
fn raw_dir(bytes: &[u8], names: &[String]) -> std::path::PathBuf {
    let dir = scratch();
    for (name, part) in names
        .iter()
        .zip(parts(bytes))
        .filter(|(_, p)| !p.is_empty())
    {
        write(&dir, name, part);
    }
    dir
}

/// Read the parts of `bytes` as the files of a raw directory of a profile. Return true when the
/// directory holds a record.
pub(crate) fn profile_dir(bytes: &[u8]) -> bool {
    let dir = raw_dir(bytes, profile_files());
    let Some(raw) = answer(RawProfile::load_dir(&dir)) else {
        return false;
    };
    let Some(record) = answer(Snapshot::from_raw(&raw, "raw", AT)) else {
        return false;
    };
    check_record(&record);
    true
}

#[test]
fn a_raw_directory_of_a_profile_builds_a_record_or_is_refused_with_a_kind() {
    bolero::check!().for_each(|bytes| {
        profile_dir(bytes);
    });
}

/// Read the parts of `bytes` as the files of a raw directory of a cluster. Return true when the
/// directory holds a record.
pub(crate) fn cluster_dir(bytes: &[u8]) -> bool {
    let dir = raw_dir(bytes, cluster_files());
    let Some(raw) = answer(RawCluster::load_dir(&dir)) else {
        return false;
    };
    let Some(records) = answer(Snapshot::each_from_cluster(&raw, "raw", AT)) else {
        return false;
    };
    for record in &records {
        check_record(record);
    }
    if let Some(record) = answer(Snapshot::from_cluster(&raw, None, "raw", AT)) {
        check_record(&record);
    }
    true
}

#[test]
fn a_raw_directory_of_a_cluster_builds_a_record_or_is_refused_with_a_kind() {
    bolero::check!().for_each(|bytes| {
        cluster_dir(bytes);
    });
}

/// The check of a comparison can fail: two records of two profiles that differ are not two records
/// of one profile. A check that compares a record with itself never fails.
#[test]
#[should_panic(expected = "the roots of two records of one profile differ")]
fn the_check_of_two_records_fails_for_two_profiles_that_differ() {
    let record = |values: &str| {
        Snapshot::from_raw(&crate::support::raw_profile(values, ""), "fuzz", AT)
            .expect("a profile of one pack builds")
    };
    compares_equal(&record("replicas: 1\n"), &record("replicas: 2\n"));
}
