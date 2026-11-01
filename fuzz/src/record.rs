//! The targets of a record, of a comparison, and of a raw directory, from bytes.

use strata::api::{RawCluster, RawProfile};
use strata::diff::Diff;
use strata::ops;
use strata::render::{Format, renderer};
use strata::snapshot::Snapshot;
use strata::term::color::Paint;

use crate::support::{AT, answer, cluster_files, parts, profile_files, scratch, write};

/// Make sure that `record` compares equal to itself: the comparison says it is the same, nothing
/// differs, and each renderer shows it.
#[track_caller]
pub(crate) fn compares_equal_to_itself(record: &Snapshot) {
    let diff = ops::compare(record, record, AT);
    assert!(
        diff.same && diff.nothing_differs(),
        "a record differs from itself"
    );
    for format in [Format::Terminal, Format::Markdown, Format::Json] {
        let shown = renderer(format, Paint::off(), false).render(&diff, record, record, true);
        assert!(shown.is_ok(), "{format:?} cannot show a record: {shown:?}");
    }
}

/// Make sure that `record`, written and read again, is the record: its JSON loads as a record with
/// the same root and the same content digest, and the JSON of that record is the same JSON again.
#[track_caller]
pub(crate) fn writes_and_reads_again(record: &Snapshot) {
    let json = answer(record.to_json(false)).expect("a record is written as JSON");
    let dir = scratch();
    write(&dir, "record.json", json.as_bytes());
    let back = answer(Snapshot::load(&dir.join("record.json")))
        .unwrap_or_else(|| panic!("strata cannot load the record that it wrote:\n{json}"));
    assert_eq!(
        back.root, record.root,
        "the root changes when strata writes a record and reads it"
    );
    assert_eq!(back.content_digest(), record.content_digest());
    let again = answer(back.to_json(false)).expect("a loaded record is written as JSON");
    write(&dir, "again.json", again.as_bytes());
    let third = answer(Snapshot::load(&dir.join("again.json"))).expect("the second JSON loads");
    assert_eq!(
        answer(third.to_json(false)).as_deref(),
        Some(again.as_str()),
        "the JSON of a record is not stable after one write and one read"
    );
}

/// Make sure that `record` holds what each record holds, whatever it describes: it compares equal
/// to itself, and it writes and reads again.
#[track_caller]
pub(crate) fn check_record(record: &Snapshot) {
    compares_equal_to_itself(record);
    writes_and_reads_again(record);
}

/// Load `bytes` as a record file. Return true when the file is a record.
pub(crate) fn snapshot(bytes: &[u8]) -> bool {
    let dir = scratch();
    write(&dir, "record.json", bytes);
    let Some(record) = answer(Snapshot::load(&dir.join("record.json"))) else {
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
/// empty part is a file that the directory does not have: a raw directory of an older build has
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
