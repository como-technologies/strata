//! The target of the YAML check of values and manifests, and of the canonical form.
//!
//! The canonical form is not public: the record of a profile holds it. So the target builds the
//! record of a profile whose pack has the input as its values and as its manifest.

use strata::snapshot::{Pack, Snapshot};

use crate::support::{AT, answer, raw_profile, utf8};

/// The record of a profile whose one pack has `values` as its values and `manifest` as the
/// content of its one manifest.
pub(crate) fn record(values: &str, manifest: &str) -> Option<Snapshot> {
    answer(Snapshot::from_raw(
        &raw_profile(values, manifest),
        "fuzz",
        AT,
    ))
}

/// The one pack of a record that [`record`] built.
pub(crate) fn pack(snapshot: &Snapshot) -> &Pack {
    snapshot
        .packs
        .first()
        .expect("a profile of one pack has a record of one pack")
}

/// Make sure that the canonical form of `values` is stable: the record is the same each time, and
/// the canonical form, written as text and read again, is itself.
pub(crate) fn canon_is_idempotent(values: &str) -> bool {
    let Some(first) = record(values, "") else {
        return false;
    };
    let again = record(values, "").expect("a record that built once builds again");
    assert_eq!(first.root, again.root, "the root of one profile changes");

    let pack = pack(&first);
    if pack.values_parse_error.is_some() {
        return true;
    }
    // JSON is YAML, so the canonical form is the values of a pack that means the same
    let written = serde_json::to_string(&pack.values_canon).expect("a JSON value serializes");
    let second = record(&written, "").expect("the canonical form of values builds a record");
    assert_eq!(
        pack.values_hash,
        self::pack(&second).values_hash,
        "the canonical form of {values:?} is not its own canonical form: {written}"
    );
    true
}

/// Build the record of a profile whose pack and manifest hold the text of `bytes`, and check the
/// canonical form of both. Return true when the bytes are UTF-8 and give a record.
pub(crate) fn values(bytes: &[u8]) -> bool {
    let Some(text) = utf8(bytes) else {
        return false;
    };
    let Some(snapshot) = record(text, text) else {
        return false;
    };
    let manifest = pack(&snapshot)
        .manifests
        .first()
        .expect("a profile of one manifest has a record of one manifest");
    // the same text, read as values and as a manifest, gives the same hash and the same verdict
    assert_eq!(
        manifest.content_parse_error.is_some(),
        pack(&snapshot).values_parse_error.is_some(),
        "{text:?} is YAML as values and not as a manifest, or the reverse"
    );
    canon_is_idempotent(text)
}

#[test]
fn any_text_has_a_stable_canonical_form_as_values_and_as_a_manifest() {
    bolero::check!().for_each(|bytes| {
        values(bytes);
    });
}
