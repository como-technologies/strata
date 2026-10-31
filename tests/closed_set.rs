//! The text of each value of each closed set in a document that strata writes and reads.
//!
//! Strata reads a value that it does not know as `Open::Unknown`, with no error. So when a
//! release renames or removes a value, a record of an earlier release gives no error. These tests
//! make that change fail. They hold the values of each set, and make sure that serde, the JSON
//! Schema, and `ClosedSet::as_str` give one text for each value.

use std::path::Path;

use crate::common::walk;
use serde_json::Value;
use strata::api::Method;
use strata::closed_set::ClosedSet;
use strata::conjunction::Conjunction;
use strata::diff::{ChangeFlag, ImpactLevel, Op, Status};
use strata::oci_provider::OciProvider;
use strata::ops::{PairStatus, Policy, RecordStatus};
use strata::pack_state::PackState;
use strata::pack_type::PackType;
use strata::profile_type::ProfileType;
use strata::scope::ProfileScope;
use strata::snapshot::{ClusterRead, Implementation, SubjectKind, Template, Tier, UpstreamSource};

/// One closed set with no type parameter, so that one list holds each set.
struct Set {
    /// The name of the type, as `impl ClosedSet for` names it.
    name: &'static str,
    /// The text of each value that a release wrote.
    released: &'static [&'static str],
    /// The text of each value, as the JSON Schema of the type names it.
    names: fn() -> Vec<String>,
    /// The text of each value of `ClosedSet::ALL`.
    all: fn() -> Vec<&'static str>,
    /// For the text of a value: the text that serde writes, the text of `as_str`, and the text
    /// of the value that `parse` reads.
    texts: fn(&str) -> (Value, &'static str, Option<&'static str>),
}

fn set<T: ClosedSet>(released: &'static [&'static str]) -> Set {
    let path = std::any::type_name::<T>();
    Set {
        name: path.rsplit("::").next().unwrap_or(path),
        released,
        names: schema_names::<T>,
        all: all_texts::<T>,
        texts: texts_of::<T>,
    }
}

/// Each closed set in a document that strata writes and reads, with the text of each value that
/// a release wrote.
///
/// This list is a copy of the text on purpose. When a release adds a value to a set, add its text
/// here. A release must not rename or remove a value and keep the schema id, because a record of
/// an earlier release then reads the value as unknown.
fn sets() -> [Set; 20] {
    [
        set::<Method>(&["GET", "POST"]),
        set::<Implementation>(&["rust", "bash"]),
        set::<Template>(&["published", "draft", "applied", "candidate", "registry"]),
        set::<SubjectKind>(&["cluster", "edgeHost", "clusterGroup"]),
        set::<Tier>(&["applied", "labels", "policy", "running", "shape"]),
        set::<ClusterRead>(&["updatesPending", "pendingRepave"]),
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
        set::<Status>(&["unchanged", "changed", "added", "removed"]),
        set::<Op>(&["changed", "added", "removed"]),
        set::<RecordStatus>(&["unchanged", "drifted", "unverified", "error"]),
        set::<PairStatus>(&[
            "identical",
            "same-content",
            "registry-move",
            "different",
            "unknown-upstream",
        ]),
        set::<Policy>(&["exact", "registry-move"]),
        set::<PackState>(&["deprecated", "disabled", "deleted", "absent"]),
        set::<OciProvider>(&["pack", "helm", "zarf"]),
        set::<PackType>(&["spectro", "helm", "oci", "manifest"]),
        set::<ProfileType>(&["cluster", "infra", "add-on", "system", "app", "pack"]),
        set::<ProfileScope>(&["project", "tenant", "system"]),
        set::<Conjunction>(&["and", "or"]),
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

/// The text of each value of `ClosedSet::ALL`.
fn all_texts<T: ClosedSet>() -> Vec<&'static str> {
    T::ALL.iter().map(|v| v.as_str()).collect()
}

/// For the value that `text` names, three texts. The first is the text that serde writes. The
/// second is the text of its `as_str`. The third is the text of the value that `parse` reads.
fn texts_of<T: ClosedSet>(text: &str) -> (Value, &'static str, Option<&'static str>) {
    let value: T =
        serde_json::from_value(Value::from(text)).unwrap_or_else(|e| panic!("{text}: {e}"));
    (
        serde_json::to_value(value).unwrap(),
        value.as_str(),
        T::parse(text).map(T::as_str),
    )
}

#[test]
fn serde_the_schema_and_as_str_give_one_text_for_each_value_of_a_closed_set() {
    for set in sets() {
        for name in (set.names)() {
            let (serde, as_str, parsed) = (set.texts)(&name);
            assert_eq!(serde, Value::from(name.as_str()), "{}: serde", set.name);
            assert_eq!(as_str, name, "{}: as_str", set.name);
            assert_eq!(parsed, Some(as_str), "{}: parse", set.name);
        }
    }
}

#[test]
fn the_list_of_a_closed_set_holds_each_value_of_its_schema_in_order() {
    for set in sets() {
        assert_eq!((set.all)(), (set.names)(), "{}: ALL", set.name);
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

#[test]
fn the_list_of_closed_sets_names_each_closed_set_of_the_crate() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut in_code = Vec::new();
    for file in walk(&src) {
        // the trait module implements the trait for a set of its own tests
        if file == src.join("closed_set").join("mod.rs")
            || file.extension().is_none_or(|e| e != "rs")
        {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap();
        for line in text.lines() {
            if let Some(name) = line.strip_prefix("impl_closed_set!(") {
                in_code.push(name.trim_end_matches(" {").to_string());
            }
        }
    }
    in_code.sort();
    let mut listed: Vec<&str> = sets().iter().map(|s| s.name).collect();
    listed.sort();
    assert_eq!(in_code, listed, "add each closed set to `sets`");
}
