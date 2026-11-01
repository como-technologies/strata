//! The text of each value of each closed set of the rule format and of the report. A rule file and
//! a report name these values by text, so a release that renames or removes a value breaks the
//! files of the users. These tests hold the values of each set, and make sure that serde, the JSON
//! Schema, and `ClosedSet::as_str` give one text for each value.

use std::path::{Path, PathBuf};

use serde_json::Value;
use strata::closed_set::ClosedSet;
use strata::closed_set::legend::{Described, legend};
use strata_preflight::probe::Verdict;
use strata_preflight::rule::{Check, CheckKind, EvidenceKind, Level, Severity, Status, Tier};

/// The values of one set: its name as `impl_closed_set!` writes it, the texts that a release
/// wrote, and the texts of `ClosedSet::ALL`.
struct Set {
    name: &'static str,
    released: &'static [&'static str],
    names: Vec<String>,
    all: Vec<&'static str>,
    /// For each text in `names`: the text serde writes, and the text of the value that
    /// `ClosedSet::parse` reads.
    texts: Vec<(Value, Option<&'static str>)>,
}

fn set<T: ClosedSet>(name: &'static str, released: &'static [&'static str]) -> Set {
    let schema = schemars::schema_for!(T);
    // schemars writes each unit variant that has a doc comment as a `const` of `oneOf`
    let names: Vec<String> = schema.as_value()["oneOf"]
        .as_array()
        .expect("the schema of a closed set is a oneOf")
        .iter()
        .map(|v| {
            v["const"]
                .as_str()
                .expect("each variant is a const")
                .to_string()
        })
        .collect();
    let texts = names
        .iter()
        .map(|name| {
            let value: T = serde_json::from_value(Value::from(name.as_str()))
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            (
                serde_json::to_value(value).unwrap(),
                T::parse(name).map(T::as_str),
            )
        })
        .collect();
    Set {
        name,
        released,
        names,
        all: T::ALL.iter().map(|v| v.as_str()).collect(),
        texts,
    }
}

/// Each closed set of the crate, with the text of each value that a release wrote.
///
/// This list is a copy of the text on purpose. When a release adds a value to a set, add its text
/// here. A release must not rename or remove a value, because a file of an earlier release then
/// stops loading.
fn sets() -> Vec<Set> {
    vec![
        set::<Tier>(
            "Tier",
            &["static", "palette", "cloud", "watch", "race", "tool"],
        ),
        set::<Level>("Level", &["probe", "preflight"]),
        set::<Severity>("Severity", &["fail", "warn", "note"]),
        set::<Status>("Status", &["seen", "source", "inferred", "proposed"]),
        set::<EvidenceKind>("EvidenceKind", &["run", "source", "doc", "report"]),
        set::<CheckKind>("CheckKind", &["static", "palette", "exec", "watch"]),
        set::<Verdict>(
            "Verdict",
            &[
                "passed",
                "failed",
                "not-checked",
                "not-run",
                "skipped",
                "error",
            ],
        ),
    ]
}

#[test]
fn serde_the_schema_and_as_str_give_one_text_for_each_value_of_a_closed_set() {
    for set in sets() {
        for (name, (serde, parsed)) in set.names.iter().zip(&set.texts) {
            assert_eq!(serde, &Value::from(name.as_str()), "{}: serde", set.name);
            assert_eq!(*parsed, Some(name.as_str()), "{}: parse", set.name);
        }
    }
}

#[test]
fn the_list_of_a_closed_set_holds_each_value_of_its_schema_in_order() {
    for set in sets() {
        assert_eq!(set.all, set.names, "{}: ALL", set.name);
    }
}

#[test]
fn a_closed_set_holds_each_value_that_a_release_wrote_and_no_other() {
    for set in sets() {
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

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}

#[test]
fn the_list_of_closed_sets_names_each_closed_set_of_the_crate() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut in_code = Vec::new();
    for file in walk(&src) {
        if file.extension().is_none_or(|e| e != "rs") {
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

/// The legend of `T` has one line for each value, by the text of the value in the report.
fn legend_names<T: Described>() -> Vec<String> {
    legend::<T>().into_keys().collect()
}

#[test]
fn the_legend_of_the_report_describes_each_value_of_its_set_by_the_text_of_the_json() {
    for (names, set) in [
        (legend_names::<Verdict>(), "Verdict"),
        (legend_names::<Level>(), "Level"),
        (legend_names::<Tier>(), "Tier"),
    ] {
        let expected = sets()
            .into_iter()
            .find(|s| s.name == set)
            .expect("a listed set");
        let mut all: Vec<String> = expected.all.iter().map(|s| s.to_string()).collect();
        all.sort();
        assert_eq!(names, all, "{set}");
    }
    for (name, text) in legend::<Verdict>() {
        assert!(!text.is_empty(), "{name}");
    }
}

#[test]
fn the_kind_of_a_check_is_the_tag_that_the_rule_files_write() {
    let checks = [
        "kind: static\nexpr: 'true'",
        "kind: palette\npath: /v1/x\nexpr: 'true'",
        "kind: exec\nargv: [aws]\nexpr: 'true'",
        "kind: watch\nexpr: '[]'",
    ];
    for text in checks {
        let check: Check = serde_yaml::from_str(text).unwrap();
        let tag = serde_json::to_value(&check).unwrap()["kind"].clone();
        assert_eq!(tag, Value::from(check.kind().as_str()), "{text}");
        assert_eq!(check.tier(), check.kind().tier(), "{text}");
    }
}
