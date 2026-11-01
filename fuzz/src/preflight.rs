//! The targets of strata-preflight: the rule format, and the jq engine.

use std::path::Path;

use serde_json::Value;
use strata_preflight::Result;
use strata_preflight::jq::Jq;
use strata_preflight::rule::Catalog;

use crate::support::{parts, utf8};

/// The value of an answer of strata-preflight. An error is an answer when it has a kind and a JSON
/// form with that kind. A panic is not an answer.
#[track_caller]
fn answer<T>(result: Result<T>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(e) => {
            assert!(!e.kind().is_empty(), "an error has a kind: {e}");
            assert_eq!(
                e.to_json()["kind"],
                e.kind(),
                "the JSON form names the kind: {e}"
            );
            None
        }
    }
}

/// Read the text of `bytes` as a rule file. Return true when the file is a section of rules.
///
/// A file that loads is a section whose rules write as YAML that loads as the same section: the
/// catalog of a team and the catalog of the binary share one format.
pub(crate) fn rule_file(bytes: &[u8]) -> bool {
    let Some(text) = utf8(bytes) else {
        return false;
    };
    let path = Path::new("rules/fuzz.yaml");
    let Some(section) = answer(Catalog::parse_section(text, path)) else {
        return false;
    };
    let written = serde_yaml::to_string(&section).expect("a section of rules is written as YAML");
    let mut again = answer(Catalog::parse_section(&written, path))
        .unwrap_or_else(|| panic!("the YAML of a section that loaded does not load:\n{written}"));
    // A load roots the path of each fixture at the directory of the file, so a second load roots
    // it again. The fixtures are no part of the rule that the check compares.
    let mut section = section;
    for rule in section.rules.iter_mut().chain(again.rules.iter_mut()) {
        rule.tests.clear();
    }
    assert_eq!(
        again, section,
        "a section and its YAML are two sections:\n{written}"
    );
    true
}

#[test]
fn a_rule_file_loads_as_the_section_it_writes_or_is_refused_with_a_kind() {
    bolero::check!().for_each(|bytes| {
        rule_file(bytes);
    });
}

/// The words of jq that start a program that can run for ever, or for more memory than a check
/// has: jq has no step limit. A rule that reads such a program is a program that loops, not a
/// fault of the engine, so the target compiles it and does not run it.
const LOOPS: [&str; 10] = [
    "range", "repeat", "recurse", "while", "until", "def", "limit", "..", "input", "env",
];

/// Compile the program of the first part of `bytes` with and without `$item`, read it as a
/// template, and run it on the JSON of the second part. Return true when the program compiles.
pub(crate) fn jq_program(bytes: &[u8]) -> bool {
    let mut inputs = parts(bytes).map(|p| utf8(p).unwrap_or_default());
    let program = inputs.next().unwrap_or_default();
    let input: Value =
        serde_json::from_str(inputs.next().unwrap_or_default()).unwrap_or(Value::Null);
    let jq = Jq::new();
    answer(jq.check_template("fuzz", program, true));
    answer(jq.check_template("fuzz", program, false));
    let compiles = answer(jq.check("fuzz", program, false)).is_some();
    if compiles && !LOOPS.iter().any(|word| program.contains(word)) {
        answer(jq.run("fuzz", program, &input, None));
        answer(jq.render("fuzz", program, &input, None));
    }
    compiles
}

#[test]
fn a_jq_program_compiles_and_runs_or_is_refused_with_a_kind() {
    bolero::check!().for_each(|bytes| {
        jq_program(bytes);
    });
}
