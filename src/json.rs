//! The reads of a document of Palette.
//!
//! A document of Palette stays a [`serde_json::Value`]. Each module reads it through these
//! helpers, so one rule says what a field that is not there gives: empty text, no entry, or
//! `None`. No module keeps a pointer read of its own.

use serde_json::Value;

/// The text at the pointer `ptr` of `v`, or empty text when `v` has no text there.
pub(crate) fn str_at<'a>(v: &'a Value, ptr: &str) -> &'a str {
    v.pointer(ptr).and_then(Value::as_str).unwrap_or("")
}

/// A copy of the text at the pointer `ptr` of `v`, or empty text when `v` has no text there.
pub(crate) fn text(v: &Value, ptr: &str) -> String {
    str_at(v, ptr).to_string()
}

/// The text at the pointer `ptr` of `v`, when it is there and not empty.
pub(crate) fn opt_text(v: &Value, ptr: &str) -> Option<String> {
    Some(str_at(v, ptr))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// The entries of the array at the pointer `ptr` of `v`, or no entry when `v` has no array there.
pub(crate) fn arr<'a>(v: &'a Value, ptr: &str) -> &'a [Value] {
    v.pointer(ptr)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// The texts of the array at the pointer `ptr` of `v`. An entry that is not text is left out.
pub(crate) fn strings(v: &Value, ptr: &str) -> Vec<String> {
    arr(v, ptr)
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
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
    fn is_zero_is_true_for_zero_alone() {
        assert!(is_zero(&0));
        assert!(!is_zero(&1));
    }
}
