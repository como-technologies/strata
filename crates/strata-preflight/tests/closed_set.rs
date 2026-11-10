//! The text of each value of each closed set of the rule format and of the report. A rule file and
//! a report name these values by text. A release that renames or removes a value breaks the files
//! of the users. These tests hold the values of each set. They make sure that serde, the JSON
//! Schema, and `ClosedSet::as_str` give one text for each value. The checks are the ones of
//! `tests/closed_set.rs` of the root package, in `tests/common/closed_set.rs`.

use std::path::Path;

use serde_json::Value;
use strata::closed_set::ClosedSet;
use strata::closed_set::legend::{Described, legend};
use strata_preflight::confidence::RunResult;
use strata_preflight::probe::Verdict;
use strata_preflight::rule::{
    Check, CheckKind, EvidenceKind, Level, Owner, Saves, Severity, Status, Tier,
};
use strata_preflight::world::cloud_list;

#[path = "../../../tests/common/closed_set.rs"]
mod closed_set;
#[path = "../../../tests/common/walk.rs"]
mod walk;

use closed_set::{
    Set, check_all_in_order, check_listed, check_one_text_for_each_value, check_released, set,
};

/// Each closed set of the crate, with the text of each value that a release wrote.
///
/// This list is a copy of the text on purpose. When a release adds a value to a set, add its text
/// here. A release must not rename or remove a value, because a file of an earlier release then
/// stops loading.
fn sets() -> [Set; 10] {
    [
        set::<Tier>(&["static", "palette", "cloud", "watch", "race", "tool"]),
        set::<Level>(&["probe", "preflight"]),
        set::<Severity>(&["fail", "warn", "note"]),
        set::<Status>(&["seen", "source", "inferred", "proposed"]),
        set::<EvidenceKind>(&["run", "source", "doc", "report"]),
        set::<CheckKind>(&["static", "palette", "exec", "watch"]),
        set::<Saves>(&["console-address"]),
        set::<Owner>(&["palette", "provider", "lab", "strata", "operator", "none"]),
        set::<RunResult>(&["caught", "missed", "passed", "false-fail"]),
        set::<Verdict>(&[
            "passed",
            "failed",
            "not-checked",
            "not-run",
            "skipped",
            "error",
        ]),
    ]
}

#[test]
fn serde_the_schema_and_as_str_give_one_text_for_each_value_of_a_closed_set() {
    check_one_text_for_each_value(&sets());
}

#[test]
fn the_list_of_a_closed_set_holds_each_value_of_its_schema_in_order() {
    check_all_in_order(&sets());
}

#[test]
fn a_closed_set_holds_each_value_that_a_release_wrote_and_no_other() {
    check_released(&sets());
}

#[test]
fn the_list_of_closed_sets_names_each_closed_set_of_the_crate() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    check_listed(&sets(), &walk::walk(&src));
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

#[test]
fn the_live_script_of_the_plan_accepts_the_results_that_the_ledger_knows() {
    let script = include_str!("../../../examples/live/preflight-plan.sh");
    let results: Vec<&str> = script
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_suffix(r#") results+=("$1|$2|$3") ;;"#))
        .expect("the script has a case for the results that the ledger knows")
        .split('|')
        .collect();
    let known: Vec<&str> = RunResult::ALL.iter().map(|r| r.as_str()).collect();
    assert_eq!(results, known);
}

#[test]
fn the_workflow_lists_the_engines_of_the_world_in_order() {
    let workflow: serde_yaml::Value =
        serde_yaml::from_str(include_str!("../../../.github/workflows/preflight.yml")).unwrap();
    let description = workflow["on"]["workflow_call"]["inputs"]["cloud"]["description"]
        .as_str()
        .expect("the input `cloud` has a description");
    let list = description
        .strip_prefix("The engine of a file: ")
        .and_then(|rest| rest.split_once(". "))
        .map(|(list, _)| list.replace(", or ", ", "))
        .expect("the description starts with the list of the engines");
    assert_eq!(list, cloud_list());
}
