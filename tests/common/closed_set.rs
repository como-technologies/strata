//! The checks of a closed set that the root package and `strata-preflight` share. The tests of
//! `tests/closed_set.rs` and of `preflight/tests/closed_set.rs` list their sets, and call these
//! checks, so that one reader of the schema and one rule for each check exist.

use std::path::PathBuf;

use serde_json::Value;
use strata::closed_set::ClosedSet;

/// One closed set with no type parameter, so that one list holds each set.
pub struct Set {
    /// The name of the type, as `impl ClosedSet for` names it.
    pub name: &'static str,
    /// The text of each value that a release wrote.
    released: &'static [&'static str],
    /// The text of each value, as the JSON Schema of the type names it.
    names: Vec<String>,
    /// The text of each value of `ClosedSet::ALL`.
    pub all: Vec<&'static str>,
    /// For the text of each value in `names`: the text that serde writes, the text of `as_str`,
    /// and the text of the value that `parse` reads.
    texts: Vec<(Value, &'static str, Option<&'static str>)>,
}

/// The set of the type `T`, with the text of each value that a release wrote.
pub fn set<T: ClosedSet>(released: &'static [&'static str]) -> Set {
    let path = std::any::type_name::<T>();
    let names = schema_values(schemars::schema_for!(T).as_value());
    let texts = names
        .iter()
        .map(|text| {
            let value: T = serde_json::from_value(Value::from(text.as_str()))
                .unwrap_or_else(|e| panic!("{text}: {e}"));
            (
                serde_json::to_value(value).unwrap(),
                value.as_str(),
                T::parse(text).map(T::as_str),
            )
        })
        .collect();
    Set {
        name: path.rsplit("::").next().unwrap_or(path),
        released,
        names,
        all: T::ALL.iter().map(|v| v.as_str()).collect(),
        texts,
    }
}

/// The text of each value of a closed set, from its node in a JSON Schema.
///
/// The node is a `oneOf` of one `const` for each value, in the order of the enum. The root of the
/// schema of a type is such a node, and so is a definition under `$defs`.
pub fn schema_values(node: &Value) -> Vec<String> {
    node["oneOf"]
        .as_array()
        .unwrap_or_else(|| panic!("the schema of a closed set is a oneOf: {node}"))
        .iter()
        .map(|v| {
            v["const"]
                .as_str()
                .unwrap_or_else(|| panic!("each value is a const: {v}"))
                .to_string()
        })
        .collect()
}

/// Make sure that serde, the schema, `as_str`, and `parse` give one text for each value.
pub fn check_one_text_for_each_value(sets: &[Set]) {
    for set in sets {
        for (name, (serde, as_str, parsed)) in set.names.iter().zip(&set.texts) {
            assert_eq!(serde, &Value::from(name.as_str()), "{}: serde", set.name);
            assert_eq!(as_str, name, "{}: as_str", set.name);
            assert_eq!(*parsed, Some(*as_str), "{}: parse", set.name);
        }
    }
}

/// Make sure that `ALL` holds each value of the schema, in its order.
pub fn check_all_in_order(sets: &[Set]) {
    for set in sets {
        assert_eq!(set.all, set.names, "{}: ALL", set.name);
    }
}

/// Make sure that a set holds each value that a release wrote, and no other.
pub fn check_released(sets: &[Set]) {
    for set in sets {
        let mut names = set.names.clone();
        names.sort();
        let mut released = set.released.to_vec();
        released.sort();
        assert_eq!(
            names, released,
            "{}: a release must not rename or remove a value. Add a new value to `sets`.",
            set.name
        );
    }
}

/// Make sure that `sets` names each closed set that the Rust files in `files` declare with
/// `impl_closed_set!`.
pub fn check_listed(sets: &[Set], files: &[PathBuf]) {
    let mut in_code = Vec::new();
    for file in files
        .iter()
        .filter(|f| f.extension().is_some_and(|e| e == "rs"))
    {
        let text = std::fs::read_to_string(file).unwrap();
        for line in text.lines() {
            if let Some(name) = line.strip_prefix("impl_closed_set!(") {
                in_code.push(name.trim_end_matches(" {").to_string());
            }
        }
    }
    in_code.sort();
    let mut listed: Vec<&str> = sets.iter().map(|s| s.name).collect();
    listed.sort();
    assert_eq!(in_code, listed, "add each closed set to `sets`");
}
