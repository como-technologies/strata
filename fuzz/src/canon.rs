//! The target of the YAML check of values and manifests, and of the canonical form.
//!
//! The record of a profile holds the canonical form. So the target builds the record of a profile
//! whose pack has the input as its values and as its manifest. It writes the canonical text with
//! `canonical_json`, which the library makes public for this check, and reads that text again.

use serde_json::Value;
use strata::content::canonical_json;
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
    // values that are not YAML have no canonical form, and a long key has no JSON form that a
    // parser reads back
    if pack.values_parse_error.is_some() || has_a_long_key(&pack.values_canon) {
        return true;
    }
    // the canonical text is JSON, and JSON is YAML: the text must be values that mean the same
    let written = canonical_json(&pack.values_canon);
    let second = record(&written, "").expect("the canonical form of values builds a record");
    assert_eq!(
        pack.values_hash,
        self::pack(&second).values_hash,
        "the canonical form of {values:?} is not its own canonical form: {written}"
    );
    true
}

/// The most characters that a YAML parser reads in a key of one line, with its quotes. YAML 1.2
/// limits an implicit key to 1024 characters, and a flow mapping has no other form of key.
const MAX_KEY: usize = 1024;

/// True when the canonical text of a key of `value`, at any depth, is longer than [`MAX_KEY`]. A
/// parser cannot read that text back, so the check of the canonical form leaves such values out.
fn has_a_long_key(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, item)| {
            canonical_json(&Value::String(key.clone())).chars().count() > MAX_KEY
                || has_a_long_key(item)
        }),
        Value::Array(items) => items.iter().any(has_a_long_key),
        _ => false,
    }
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

#[test]
fn a_key_of_more_than_1024_characters_is_left_out_of_the_check_and_one_of_1024_is_not() {
    // 1022 characters and two quotes make a key of 1024, which a parser reads back
    for (length, left_out) in [(1022, false), (1023, true)] {
        let values = format!("? {}\n: 1\n", "k".repeat(length));
        let snapshot = record(&values, "").expect("an explicit key of any length is YAML");
        assert_eq!(has_a_long_key(&pack(&snapshot).values_canon), left_out);
        assert!(canon_is_idempotent(&values));
    }
}
