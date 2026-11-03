//! The targets of strata-preflight: the rule format, the jq engine, and the world.

use std::path::Path;

use serde_json::Value;
use strata_preflight::fixture;
use strata_preflight::jq::Jq;
use strata_preflight::rule::Catalog;
use strata_preflight::world::World;

use crate::support::{answer, parts, scratch, utf8, write};

/// Read the text of `bytes` as a rule file. Return true when the file is a section of rules.
///
/// A file that loads is a section whose rules write as YAML that loads as the same section. The
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
/// has. There is no step limit in jq. A rule that reads such a program is a program that loops,
/// and not a fault of the engine. So the target compiles it and does not run it.
const LOOPS: [&str; 10] = [
    "range", "repeat", "recurse", "while", "until", "def", "limit", "..", "input", "env",
];

/// Compile the program of the first part of `bytes` with and without `$item`. Read it as a
/// template. Run it on the JSON of the second part. Return true when the program compiles.
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

/// The cluster that a world of an existing cluster names.
const CLUSTER_UID: &str = "64f1000000000000000000e1";

/// The base that a fixture of an input names with `"_base": "clean.json"`: a small world, so that
/// the fixtures of the catalog reach the merge.
const BASE: &str = r#"{"cloud": "eks", "entity": {"metadata": {"name": "base"},
    "spec": {"machinepoolconfig": [{"poolConfig": {"name": "worker-pool", "size": 2}}]}}}"#;

/// True when `value` has a place for a key: an object, or `null`, which a write makes an object.
fn holds_keys(value: &Value) -> bool {
    value.is_null() || value.is_object()
}

/// The document of a run for `world`, for a new cluster and then for the existing cluster
/// [`CLUSTER_UID`].
///
/// A new cluster sends its entity to the validate route as it is. An existing cluster sends it with
/// a name of its own. Its world is an error when the entity, or its `metadata`, is a value that is
/// not an object. The name has no place there, and it must not replace the value.
fn run_documents(mut world: World) {
    let new = answer(world.to_value()).expect("a world with no cluster gives its document");
    assert_eq!(
        new["validateBody"], new["entity"],
        "a new cluster sends its entity as it is"
    );
    let entity = new["entity"].clone();
    world.cluster_uid = Some(CLUSTER_UID.to_string());
    let existing = answer(world.to_value());
    assert_eq!(
        existing.is_some(),
        holds_keys(&entity) && holds_keys(&entity["metadata"]),
        "an existing cluster has a document exactly when its entity and its metadata hold keys: {entity}"
    );
    if let Some(document) = existing {
        let name = document["validateBody"]["metadata"]["name"]
            .as_str()
            .expect("the name of the validate body is text");
        // The suffix is the text of the const of strata-preflight, on purpose: the check states the
        // rule of the validate route on its own. The package does not make the const public for a
        // target.
        assert!(name.ends_with("-preflight"), "{name:?}");
    }
}

/// Read the text of `bytes` as a world on disk, as the fixture of a test, and as the body of a
/// create. Return true when the text is either one.
///
/// A world on disk gets the fields that the engine decides, and the body of a create is the world
/// of a run. Both give an error with a kind for a text that is no world. The world of a body gives
/// its document for a new cluster and for an existing cluster: see `run_documents`.
pub(crate) fn world_file(bytes: &[u8]) -> bool {
    let dir = scratch();
    write(&dir, "clean.json", BASE.as_bytes());
    write(&dir, "world.json", bytes);
    let path = dir.join("world.json");
    let fixture = answer(fixture::load(&path));
    if let Some(document) = &fixture {
        assert!(
            document.get("validateBody").is_some(),
            "a fixture that loaded has the derived fields: {document}"
        );
    }
    let worlds = [Some("eks"), None]
        .into_iter()
        .filter_map(|cloud| answer(World::from_file(&path, cloud)))
        .collect::<Vec<_>>();
    let read = !worlds.is_empty();
    worlds.into_iter().for_each(run_documents);
    fixture.is_some() || read
}

#[test]
fn a_world_file_gives_the_document_of_a_run_or_is_refused_with_a_kind() {
    bolero::check!().for_each(|bytes| {
        world_file(bytes);
    });
}
