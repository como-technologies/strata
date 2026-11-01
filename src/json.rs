//! The reads of a document of Palette.
//!
//! A document of Palette stays a [`serde_json::Value`]. Each module reads it through these
//! helpers, so one rule says what a field that is not there gives: empty text, no entry, `false`,
//! `null`, or `None`. No module keeps a pointer read of text, of an array, of a flag, or of a
//! value of its own. The read of a profile export is not a read of this kind: it refuses a value
//! of another type.
//!
//! The helpers that `strata-preflight` and the knurl adapter use are public, so that those
//! packages do not write a read of their own. The other helpers are for this crate.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::closed_set::{ClosedSet, Open};

/// The null that a read gives for a value that is not there.
static NULL: Value = Value::Null;

/// The text at the pointer `ptr` of `v`, or empty text when `v` has no text there.
pub fn str_at<'a>(v: &'a Value, ptr: &str) -> &'a str {
    v.pointer(ptr).and_then(Value::as_str).unwrap_or("")
}

/// A copy of the text at the pointer `ptr` of `v`, or empty text when `v` has no text there.
pub fn text(v: &Value, ptr: &str) -> String {
    str_at(v, ptr).to_string()
}

/// The text at the pointer `ptr` of `v`, or `default` when `v` has no text there. An empty text
/// stays empty: this is not a search for the first text that is not empty.
pub fn text_or<'a>(v: &'a Value, ptr: &str, default: &'a str) -> &'a str {
    v.pointer(ptr).and_then(Value::as_str).unwrap_or(default)
}

/// The text at the pointer `ptr` of `v`, when it is there and not empty.
pub fn opt_str<'a>(v: &'a Value, ptr: &str) -> Option<&'a str> {
    Some(str_at(v, ptr)).filter(|s| !s.is_empty())
}

/// The value of the closed set `T` that the text at the pointer `ptr` of `v` names, when the text is
/// there and not empty.
///
/// A text that names no value of `T` stays as text in [`Open::Unknown`]: it is a value of a later
/// release, and not an error.
pub(crate) fn opt_set<T: ClosedSet>(v: &Value, ptr: &str) -> Option<Open<T>> {
    opt_str(v, ptr).map(Open::from_text)
}

/// A copy of the text at the pointer `ptr` of `v`, when it is there and not empty.
pub fn opt_text(v: &Value, ptr: &str) -> Option<String> {
    opt_str(v, ptr).map(str::to_string)
}

/// The bool at the pointer `ptr` of `v`, or `None` when `v` has no bool there. Use it when a field
/// that is not there differs from `false`.
pub fn opt_flag(v: &Value, ptr: &str) -> Option<bool> {
    v.pointer(ptr).and_then(Value::as_bool)
}

/// True when `v` has the bool `true` at the pointer `ptr`. A field that is `false`, is not a bool,
/// or is not there gives `false`: Palette does not send a flag that is `false`.
pub fn flag(v: &Value, ptr: &str) -> bool {
    opt_flag(v, ptr) == Some(true)
}

/// The value at the pointer `ptr` of `v`, or `null` when `v` has none there.
pub fn value_at<'a>(v: &'a Value, ptr: &str) -> &'a Value {
    v.pointer(ptr).unwrap_or(&NULL)
}

/// A copy of the value at the pointer `ptr` of `v`, or `null` when `v` has none there.
pub fn value_or_null(v: &Value, ptr: &str) -> Value {
    value_at(v, ptr).clone()
}

/// A copy of the value at the pointer `ptr` of `v`, or an empty object when `v` has none there.
///
/// It is the default of a part of a document that a reader walks as an object, such as a config.
pub fn value_or_empty_object(v: &Value, ptr: &str) -> Value {
    v.pointer(ptr)
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()))
}

/// The object at the pointer `ptr` of `v`, or `None` when `v` has no object there.
pub(crate) fn object_at<'a>(v: &'a Value, ptr: &str) -> Option<&'a Map<String, Value>> {
    v.pointer(ptr).and_then(Value::as_object)
}

/// The entries of the array at the pointer `ptr` of `v`, or no entry when `v` has no array there.
pub fn arr<'a>(v: &'a Value, ptr: &str) -> &'a [Value] {
    v.pointer(ptr)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// The texts of the array at the pointer `ptr` of `v`.
///
/// The list skips an entry that is not text.
pub fn strings(v: &Value, ptr: &str) -> Vec<String> {
    arr(v, ptr)
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

/// The texts of the object at the pointer `ptr` of `v`, by key, such as the labels of a
/// document.
///
/// The map skips an entry whose value is not text, as [`strings`] skips an entry of an array. No
/// entry when `v` has no object there.
///
/// Palette gives a label as text. So a value of another type is no label, and a tag filter must
/// not match it as a label with an empty value.
pub fn text_map(v: &Value, ptr: &str) -> BTreeMap<String, String> {
    object_at(v, ptr)
        .into_iter()
        .flatten()
        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
        .collect()
}

/// The entries of a listing of Palette: its `items`, or the listing itself when it is an array.
///
/// The cluster listing, `GET /v1/dashboard/spectroclusters/meta`, answers an array at the top
/// level and not an object with `items`. Each other listing answers an object.
pub(crate) fn items(listing: &Value) -> impl Iterator<Item = &Value> {
    listing
        .get("items")
        .unwrap_or(listing)
        .as_array()
        .into_iter()
        .flatten()
}

/// The `items` of `listing`, taken out of it. No entry when it has none.
pub(crate) fn take_items(mut listing: Value) -> Vec<Value> {
    match listing.get_mut("items").map(Value::take) {
        Some(Value::Array(items)) => items,
        _ => Vec::new(),
    }
}

/// The first text of `candidates` that is not empty, or empty text when each one is empty.
pub(crate) fn first_nonempty(candidates: &[&str]) -> String {
    candidates
        .iter()
        .find(|c| !c.is_empty())
        .map_or_else(String::new, |c| (*c).to_string())
}

/// Put `value` at the object keys `keys` of `doc`. A value on the way that is not an object becomes
/// one, as a null does for the index of `serde_json`. That index panics for any other value, and
/// Palette can answer a document whose `spec` is a number.
pub(crate) fn set_at(doc: &mut Value, keys: &[&str], value: Value) {
    let Some((first, rest)) = keys.split_first() else {
        *doc = value;
        return;
    };
    if !doc.is_object() {
        *doc = Value::Object(serde_json::Map::new());
    }
    if let Value::Object(map) = doc {
        set_at(map.entry(*first).or_insert(Value::Null), rest, value);
    }
}

/// True for zero, for `skip_serializing_if` on a count.
pub(crate) fn is_zero(n: &usize) -> bool {
    *n == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn text_gives_the_text_at_a_pointer_and_empty_text_for_a_value_that_is_not_text() {
        let v = json!({"metadata": {"name": "demo", "uid": 7, "none": null}});
        assert_eq!(str_at(&v, "/metadata/name"), "demo");
        assert_eq!(text(&v, "/metadata/name"), "demo");
        assert_eq!(text(&v, "/metadata/uid"), "");
        assert_eq!(text(&v, "/metadata/none"), "");
        assert_eq!(text(&v, "/metadata/gone"), "");
        assert_eq!(text(&Value::Null, "/metadata/name"), "");
    }

    #[test]
    fn opt_text_gives_none_for_empty_text_and_for_a_field_that_is_not_there() {
        let v = json!({"state": "Running", "empty": "", "n": 1});
        assert_eq!(opt_text(&v, "/state").as_deref(), Some("Running"));
        assert_eq!(opt_text(&v, "/empty"), None);
        assert_eq!(opt_text(&v, "/n"), None);
        assert_eq!(opt_text(&v, "/gone"), None);
    }

    #[test]
    fn text_or_gives_the_default_for_a_value_that_is_not_text_and_keeps_an_empty_text() {
        let v = json!({"a": "x", "empty": "", "n": 1});
        assert_eq!(text_or(&v, "/a", "d"), "x");
        assert_eq!(text_or(&v, "/empty", "d"), "");
        assert_eq!(text_or(&v, "/n", "d"), "d");
        assert_eq!(text_or(&v, "/gone", "d"), "d");
        assert_eq!(opt_str(&v, "/a"), Some("x"));
        assert_eq!(opt_str(&v, "/empty"), None);
    }

    #[test]
    fn flag_is_true_for_the_bool_true_alone_and_opt_flag_tells_false_from_absent() {
        let v = json!({"on": true, "off": false, "text": "true", "n": 1});
        assert!(flag(&v, "/on"));
        for ptr in ["/off", "/text", "/n", "/gone"] {
            assert!(!flag(&v, ptr), "{ptr}");
        }
        assert_eq!(opt_flag(&v, "/on"), Some(true));
        assert_eq!(opt_flag(&v, "/off"), Some(false));
        assert_eq!(opt_flag(&v, "/text"), None);
        assert_eq!(opt_flag(&v, "/gone"), None);
    }

    #[test]
    fn value_or_null_copies_the_value_at_a_pointer_and_gives_null_for_none() {
        let v = json!({"spec": {"a": [1], "b": null}});
        assert_eq!(value_or_null(&v, "/spec/a"), json!([1]));
        assert_eq!(value_or_null(&v, "/spec/b"), Value::Null);
        assert_eq!(value_or_null(&v, "/spec/gone"), Value::Null);
        assert_eq!(value_at(&v, "/spec/a"), &json!([1]));
        assert_eq!(value_at(&v, "/gone"), &Value::Null);
    }

    #[test]
    fn value_or_empty_object_copies_the_value_and_gives_an_empty_object_for_none() {
        let v = json!({"spec": {"config": {"a": 1}, "none": null}});
        assert_eq!(value_or_empty_object(&v, "/spec/config"), json!({"a": 1}));
        assert_eq!(value_or_empty_object(&v, "/spec/none"), Value::Null);
        assert_eq!(value_or_empty_object(&v, "/spec/gone"), json!({}));
    }

    #[test]
    fn object_at_gives_an_object_and_none_for_another_value() {
        let v = json!({"a": {"x": 1}, "list": [1], "text": "t"});
        assert_eq!(object_at(&v, "/a").map(Map::len), Some(1));
        assert!(object_at(&v, "/list").is_none());
        assert!(object_at(&v, "/text").is_none());
        assert!(object_at(&v, "/gone").is_none());
    }

    #[test]
    fn opt_set_reads_a_value_of_a_set_and_keeps_the_text_of_a_value_that_is_not_in_it() {
        use crate::oci_provider::OciProvider;
        let v = json!({"spec": {"providerType": "zarf", "later": "oras", "empty": ""}});
        let zarf: Option<Open<OciProvider>> = opt_set(&v, "/spec/providerType");
        assert_eq!(zarf, Some(Open::Known(OciProvider::Zarf)));
        let later: Option<Open<OciProvider>> = opt_set(&v, "/spec/later");
        assert_eq!(later, Some(Open::Unknown("oras".to_string())));
        assert_eq!(opt_set::<OciProvider>(&v, "/spec/empty"), None);
        assert_eq!(opt_set::<OciProvider>(&v, "/spec/gone"), None);
    }

    #[test]
    fn arr_gives_the_entries_of_an_array_and_no_entry_for_another_value() {
        let v = json!({"spec": {"packs": [{"name": "a"}, {"name": "b"}], "name": "x"}});
        assert_eq!(arr(&v, "/spec/packs").len(), 2);
        assert_eq!(arr(&v, "/spec/packs")[1]["name"], "b");
        assert!(arr(&v, "/spec/name").is_empty());
        assert!(arr(&v, "/spec/gone").is_empty());
    }

    #[test]
    fn strings_keeps_the_texts_of_an_array_and_leaves_out_each_other_entry() {
        let v = json!({"spec": {"cloudTypes": ["aws", 1, null, "gcp"]}});
        assert_eq!(strings(&v, "/spec/cloudTypes"), vec!["aws", "gcp"]);
        assert!(strings(&v, "/spec/gone").is_empty());
    }

    #[test]
    fn text_map_keeps_the_texts_of_an_object_and_leaves_out_each_other_value() {
        let v = json!({"metadata": {"labels": {"env": "dev", "n": 1, "none": null, "": "x"},
            "name": "demo"}});
        let labels = text_map(&v, "/metadata/labels");
        assert_eq!(
            labels.into_iter().collect::<Vec<_>>(),
            vec![
                (String::new(), "x".to_string()),
                ("env".to_string(), "dev".to_string())
            ]
        );
        assert!(text_map(&v, "/metadata/name").is_empty());
        assert!(text_map(&v, "/metadata/gone").is_empty());
    }

    #[test]
    fn items_reads_the_items_of_an_object_and_the_entries_of_a_bare_array() {
        let object = json!({"items": [{"uid": "a"}, {"uid": "b"}], "listmeta": {"count": 2}});
        let bare = json!([{"uid": "c"}]);
        let uids = |v: &Value| items(v).map(|e| text(e, "/uid")).collect::<Vec<_>>();
        assert_eq!(uids(&object), vec!["a", "b"]);
        assert_eq!(uids(&bare), vec!["c"]);
        assert!(items(&json!({"items": "x"})).next().is_none());
        assert!(items(&json!({"count": 0})).next().is_none());
        assert!(items(&Value::Null).next().is_none());
    }

    #[test]
    fn take_items_moves_the_items_out_of_a_listing() {
        let listing = json!({"items": [{"uid": "a"}], "listmeta": {"count": 1}});
        assert_eq!(take_items(listing), vec![json!({"uid": "a"})]);
        assert!(take_items(json!({"items": {}})).is_empty());
        assert!(take_items(json!([{"uid": "a"}])).is_empty());
        assert!(take_items(Value::Null).is_empty());
    }

    #[test]
    fn first_nonempty_gives_the_first_text_that_is_not_empty() {
        assert_eq!(first_nonempty(&["", "b", "c"]), "b");
        assert_eq!(first_nonempty(&["a", "b"]), "a");
        assert_eq!(first_nonempty(&["", ""]), "");
        assert_eq!(first_nonempty(&[]), "");
    }

    #[test]
    fn set_at_puts_a_value_at_the_keys_and_makes_an_object_of_each_value_on_the_way_that_is_not_one()
     {
        let mut v = json!({"spec": {"keep": 1}});
        set_at(&mut v, &["spec", "packs"], json!([1]));
        assert_eq!(v, json!({"spec": {"keep": 1, "packs": [1]}}));
        for junk in [
            json!(5),
            json!("text"),
            json!([1]),
            json!(true),
            Value::Null,
        ] {
            let mut v = junk.clone();
            set_at(&mut v, &["spec", "packs"], json!([]));
            assert_eq!(v, json!({"spec": {"packs": []}}), "{junk}");
            let mut v = json!({"spec": junk});
            set_at(&mut v, &["spec", "packs"], json!([]));
            assert_eq!(v, json!({"spec": {"packs": []}}));
        }
        let mut v = json!(1);
        set_at(&mut v, &[], json!(2));
        assert_eq!(v, json!(2));
    }

    #[test]
    fn is_zero_is_true_for_zero_alone() {
        assert!(is_zero(&0));
        assert!(!is_zero(&1));
    }
}
