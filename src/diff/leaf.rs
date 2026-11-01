//! The leaf comparison: each scalar of two JSON documents, by its path.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::zip_by_key;
use crate::closed_set::impl_closed_set;
use crate::closed_set::legend::Described;
use crate::json::value_at;

/// What happened to one leaf or manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Op {
    /// Present on both sides with different values.
    Changed,
    /// Present only in B.
    Added,
    /// Present only in A.
    Removed,
}

impl_closed_set!(Op {
    Changed => "changed",
    Added => "added",
    Removed => "removed",
});

impl Described for Op {
    fn describe(self) -> &'static str {
        match self {
            Self::Changed => "The leaf exists on both sides with different values.",
            Self::Added => "The leaf exists only in B.",
            Self::Removed => "The leaf exists only in A.",
        }
    }
}

/// One scalar (or empty container) that differs.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Leaf {
    /// What happened.
    pub op: Op,
    /// Dotted path; array items as `[i]`, or `[name=<value>]` when every item has a unique name
    pub path: String,
    /// The same path as segments, in order
    pub segments: Vec<Segment>,
    /// Value in A, absent for additions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old: Option<Value>,
    /// Value in B, absent for removals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new: Option<Value>,
}

/// One step of the path of a leaf. JSON writes a key as text, an index as an integer, and a named
/// item as an object with its `name`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
#[schemars(inline)]
pub enum Segment {
    /// A key of an object.
    Key(String),
    /// The index of an item of a list.
    Index(usize),
    /// An item of a list that has a unique `name`, found by that name.
    Named {
        /// The name of the item.
        name: String,
    },
}

fn render_path(segs: &[Segment]) -> String {
    let mut out = String::new();
    for seg in segs {
        match seg {
            Segment::Key(k) => {
                if !out.is_empty() {
                    out.push('.');
                }
                out.push_str(k);
            }
            Segment::Index(i) => out.push_str(&format!("[{i}]")),
            Segment::Named { name } => out.push_str(&format!("[name={name}]")),
        }
    }
    out
}

/// When every item is an object with a unique string `name`, address items by name so that an
/// insertion shows as one addition rather than shifted neighbors.
fn name_keys(items: &[Value]) -> Option<Vec<String>> {
    let mut names = Vec::with_capacity(items.len());
    for it in items {
        let n = value_at(it, "/name").as_str()?;
        if names.iter().any(|x: &String| x == n) {
            return None;
        }
        names.push(n.to_string());
    }
    Some(names)
}

type Leaves = BTreeMap<String, (Vec<Segment>, Value)>;

fn flatten(v: &Value, path: &mut Vec<Segment>, out: &mut Leaves) {
    match v {
        Value::Object(m) if !m.is_empty() => {
            for (k, x) in m {
                path.push(Segment::Key(k.clone()));
                flatten(x, path, out);
                path.pop();
            }
        }
        Value::Array(a) if !a.is_empty() => {
            if let Some(names) = name_keys(a) {
                for (x, n) in a.iter().zip(names) {
                    path.push(Segment::Named { name: n });
                    flatten(x, path, out);
                    path.pop();
                }
            } else {
                for (i, x) in a.iter().enumerate() {
                    path.push(Segment::Index(i));
                    flatten(x, path, out);
                    path.pop();
                }
            }
        }
        _ => {
            out.insert(render_path(path), (path.clone(), v.clone()));
        }
    }
}

/// The path of each leaf of `v` that is the text `text`, sorted, in the form of [`leaf_diff`].
/// A document that is the text itself has no path, so it gives none.
pub(crate) fn paths_of_text(v: &Value, text: &str) -> Vec<String> {
    let mut leaves = Leaves::new();
    flatten(v, &mut Vec::new(), &mut leaves);
    leaves
        .into_iter()
        .filter(|(path, (_, x))| !path.is_empty() && x.as_str() == Some(text))
        .map(|(path, _)| path)
        .collect()
}

/// Every scalar or empty container that differs between two JSON documents, sorted by path.
pub fn leaf_diff(a: &Value, b: &Value) -> Vec<Leaf> {
    let (mut la, mut lb) = (Leaves::new(), Leaves::new());
    flatten(a, &mut Vec::new(), &mut la);
    flatten(b, &mut Vec::new(), &mut lb);
    // An empty or scalar document on one side against a structured document on the other reads
    // as additions or removals of the structure, not as a change at the root path.
    if la.keys().chain(lb.keys()).any(|k| !k.is_empty()) {
        la.remove("");
        lb.remove("");
    }
    let mut out = Vec::new();
    for (path, x, y) in zip_by_key(la, lb) {
        match (x, y) {
            (Some((segments, x)), Some((_, y))) => {
                if x != y {
                    out.push(Leaf {
                        op: Op::Changed,
                        path,
                        segments,
                        old: Some(x),
                        new: Some(y),
                    });
                }
            }
            (Some((segments, x)), None) => out.push(Leaf {
                op: Op::Removed,
                path,
                segments,
                old: Some(x),
                new: None,
            }),
            (None, Some((segments, y))) => out.push(Leaf {
                op: Op::Added,
                path,
                segments,
                old: None,
                new: Some(y),
            }),
            (None, None) => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn paths_of_text_gives_each_leaf_that_is_the_text_in_the_form_of_leaf_diff() {
        let v = json!({
            "auth": {"password": "********", "user": "fred"},
            "users": [{"name": "barney", "token": "********"}],
            "keys": ["********", "*********"],
            "count": 8
        });
        assert_eq!(
            paths_of_text(&v, "********"),
            ["auth.password", "keys[0]", "users[name=barney].token"]
        );
        assert!(paths_of_text(&json!("********"), "********").is_empty());
        assert!(paths_of_text(&Value::Null, "********").is_empty());
    }

    #[test]
    fn leaf_diff_reports_changed_added_removed_and_keeps_false_and_null() {
        let a = json!({"x": {"on": true, "n": null, "gone": 1}, "list": [1, 2]});
        let b = json!({"x": {"on": false, "n": null, "new": "v"}, "list": [1, 2, 3]});
        let d = leaf_diff(&a, &b);
        let paths: Vec<(Op, &str)> = d.iter().map(|l| (l.op, l.path.as_str())).collect();
        assert_eq!(
            paths,
            vec![
                (Op::Added, "list[2]"),
                (Op::Removed, "x.gone"),
                (Op::Added, "x.new"),
                (Op::Changed, "x.on"),
            ]
        );
    }

    #[test]
    fn arrays_of_named_objects_are_addressed_by_name() {
        let a = json!({"env": [{"name": "A", "value": "1"}, {"name": "B", "value": "2"}]});
        let b = json!({"env": [{"name": "Z", "value": "0"}, {"name": "A", "value": "1"}, {"name": "B", "value": "3"}]});
        let d = leaf_diff(&a, &b);
        let paths: Vec<(Op, &str)> = d.iter().map(|l| (l.op, l.path.as_str())).collect();
        assert_eq!(
            paths,
            vec![
                (Op::Changed, "env[name=B].value"),
                (Op::Added, "env[name=Z].name"),
                (Op::Added, "env[name=Z].value"),
            ]
        );
        assert_eq!(
            d[0].segments,
            vec![
                Segment::Key("env".into()),
                Segment::Named { name: "B".into() },
                Segment::Key("value".into())
            ]
        );
    }

    #[test]
    fn a_segment_is_text_for_a_key_an_integer_for_an_index_and_an_object_for_a_named_item() {
        let listed = leaf_diff(&json!({"a": [1, 2]}), &json!({"a": [1, 3]}));
        let named = leaf_diff(
            &json!({"e": [{"name": "A", "v": 1}]}),
            &json!({"e": [{"name": "A", "v": 2}]}),
        );
        assert_eq!(
            serde_json::to_value(&listed[0].segments).unwrap(),
            json!(["a", 1])
        );
        assert_eq!(
            serde_json::to_value(&named[0].segments).unwrap(),
            json!(["e", {"name": "A"}, "v"])
        );
        let read: Vec<Segment> = serde_json::from_value(json!(["e", 0, {"name": "A"}])).unwrap();
        assert_eq!(
            read,
            [
                Segment::Key("e".into()),
                Segment::Index(0),
                Segment::Named { name: "A".into() }
            ]
        );
    }

    #[test]
    fn empty_or_null_root_against_structure_shows_only_the_structure() {
        for empty in [json!({}), json!(null), json!([])] {
            let d = leaf_diff(&empty, &json!({"a": {"b": 1}}));
            assert_eq!(d.len(), 1, "from {empty}");
            assert_eq!((d[0].op, d[0].path.as_str()), (Op::Added, "a.b"));
            let d = leaf_diff(&json!({"a": {"b": 1}}), &empty);
            assert_eq!((d[0].op, d[0].path.as_str()), (Op::Removed, "a.b"));
        }
    }

    #[test]
    fn empty_containers_are_leaves() {
        let d = leaf_diff(&json!({"a": {}}), &json!({"a": []}));
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].op, Op::Changed);
    }
}
