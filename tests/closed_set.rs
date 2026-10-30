//! The text of each value of each closed set in a document that strata writes and reads.
//!
//! Strata reads a value that it does not know as `Open::Unknown`, with no error. So when a
//! release renames or removes a value, a record of an earlier release gives no error. These tests
//! make that change fail. They hold the values of each set, and make sure that serde, the JSON
//! Schema, and `ClosedSet::as_str` give one text for each value.

use std::path::{Path, PathBuf};

use serde_json::Value;
use strata::closed_set::ClosedSet;
use strata::diff::{ChangeFlag, ImpactLevel};
use strata::snapshot::{Implementation, SubjectKind, Template, Tier, UpstreamSource};

/// One closed set with no type parameter, so that one list holds each set.
struct Set {
    /// The name of the type, as `impl ClosedSet for` names it.
    name: &'static str,
    /// The text of each value that a release wrote.
    released: &'static [&'static str],
    /// The text of each value, as the JSON Schema of the type names it.
    names: fn() -> Vec<String>,
    /// For the text of a value: the text that serde writes for it, and the text of `as_str`.
    texts: fn(&str) -> (Value, &'static str),
}

fn set<T: ClosedSet>(released: &'static [&'static str]) -> Set {
    let path = std::any::type_name::<T>();
    Set {
        name: path.rsplit("::").next().unwrap_or(path),
        released,
        names: schema_names::<T>,
        texts: texts_of::<T>,
    }
}

/// Each closed set in a document that strata writes and reads, with the text of each value that
/// a release wrote.
///
/// This list is a copy of the text on purpose. When a release adds a value to a set, add its text
/// here. A release must not rename or remove a value and keep the schema id, because a record of
/// an earlier release then reads the value as unknown.
fn sets() -> [Set; 7] {
    [
        set::<Implementation>(&["rust", "bash"]),
        set::<Template>(&["published", "draft", "applied", "candidate", "registry"]),
        set::<SubjectKind>(&["cluster", "edgeHost", "clusterGroup"]),
        set::<Tier>(&["applied", "labels", "policy", "running", "shape"]),
        set::<UpstreamSource>(&["packRef.digest", "packSummary.digest", "none"]),
        set::<ImpactLevel>(&[
            "none",
            "sync",
            "in-place",
            "cluster-wide",
            "may-repave",
            "repave",
        ]),
        set::<ChangeFlag>(&["IDENTITY", "UPSTREAM", "AUTHORED", "IMAGES", "CHARTS"]),
    ]
}

/// The text of each value of `T`, as its JSON Schema names it.
fn schema_names<T: ClosedSet>() -> Vec<String> {
    let schema = schemars::schema_for!(T);
    // schemars writes each unit variant that has a doc comment as a `const` of `oneOf`
    let variants = schema.as_value()["oneOf"]
        .as_array()
        .expect("the schema of a closed set is a oneOf");
    variants
        .iter()
        .map(|v| {
            v["const"]
                .as_str()
                .expect("each variant is a const")
                .to_string()
        })
        .collect()
}

/// The text that serde writes for the value that `text` names, and the text of its `as_str`.
fn texts_of<T: ClosedSet>(text: &str) -> (Value, &'static str) {
    let value: T =
        serde_json::from_value(Value::from(text)).unwrap_or_else(|e| panic!("{text}: {e}"));
    (serde_json::to_value(value).unwrap(), value.as_str())
}

#[test]
fn serde_the_schema_and_as_str_give_one_text_for_each_value_of_a_closed_set() {
    for set in sets() {
        for name in (set.names)() {
            let (serde, as_str) = (set.texts)(&name);
            assert_eq!(serde, Value::from(name.as_str()), "{}: serde", set.name);
            assert_eq!(as_str, name, "{}: as_str", set.name);
        }
    }
}

#[test]
fn a_closed_set_holds_each_value_that_a_release_wrote_and_no_other() {
    for set in sets() {
        let mut names = (set.names)();
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

/// Each file under a directory, at any depth.
fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

#[test]
fn the_list_of_closed_sets_names_each_closed_set_of_the_crate() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut in_code = Vec::new();
    for file in walk(&src) {
        // the trait module implements the trait for a set of its own tests
        if file == src.join("closed_set.rs") || file.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap();
        for line in text.lines() {
            if let Some(name) = line.strip_prefix("impl ClosedSet for ") {
                in_code.push(name.trim_end_matches(" {").to_string());
            }
        }
    }
    in_code.sort();
    let mut listed: Vec<&str> = sets().iter().map(|s| s.name).collect();
    listed.sort();
    assert_eq!(in_code, listed, "add each closed set to `sets`");
}
