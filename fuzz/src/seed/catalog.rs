//! The seeds that come from the catalog of strata-preflight: its rule files, the programs of its
//! rules, and its fixtures.

use std::fs;
use std::path::PathBuf;

use strata_preflight::catalog;

/// The seeds of the rule file. They are the first and the last three rules of each file of the
/// catalog, each as a section of its own.
pub(super) fn rule_files() -> Vec<Vec<u8>> {
    let mut seeds = Vec::new();
    for entry in fs::read_dir(catalog::rules_dir())
        .expect("the rules are there")
        .flatten()
    {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "yaml") {
            continue;
        }
        let section: serde_yaml::Value =
            serde_yaml::from_slice(&fs::read(&path).expect("a rule file reads")).expect("YAML");
        let rules = section["rules"].as_sequence().cloned().unwrap_or_default();
        let first: Vec<_> = rules.iter().take(3).cloned().collect();
        let last: Vec<_> = rules.iter().rev().take(3).cloned().collect();
        for chunk in [first, last] {
            let mut part = section.clone();
            part["rules"] = serde_yaml::Value::Sequence(chunk);
            seeds.push(serde_yaml::to_string(&part).expect("YAML").into_bytes());
        }
    }
    seeds
}

/// The seeds of the jq engine. They are the program of the check of each rule of the catalog, with
/// the fixture of its test when it is short.
pub(super) fn jq_programs() -> Vec<Vec<u8>> {
    let catalog = catalog::builtin().expect("the catalog loads");
    let mut seeds = Vec::new();
    for rule in catalog.rules() {
        let fixture = rule
            .tests
            .iter()
            .filter_map(|t| t.fault.as_ref().or(t.clean.as_ref()))
            .find_map(|path| fs::read(path).ok())
            .filter(|bytes| bytes.len() <= 4096)
            .unwrap_or_else(|| b"{}".to_vec());
        for program in rule
            .check
            .iter()
            .map(|c| c.expr())
            .chain(rule.when.as_deref())
        {
            let mut input = program.as_bytes().to_vec();
            input.push(0);
            input.extend(&fixture);
            seeds.push(input);
        }
    }
    seeds
}

/// The worlds that a seed needs and no fixture of the catalog has. They are the envelope of a
/// body, a body in YAML, a fixture that names its base, and the worlds of an existing cluster.
const WORLD_CASES: [&str; 7] = [
    r#"{"cloudType": "eks", "entity": {"metadata": {"name": "demo"}, "spec": {"cloudConfig": {"region": "us-east-1"}}}}"#,
    "metadata:\n  name: demo\nspec:\n  cloudConfig:\n    region: us-east-1\n    vpcId: vpc-1\n",
    r#"{"_base": "clean.json", "_note": "one pool", "_patch": {"entity": {"spec": {"machinepoolconfig": [{"poolConfig": {"name": "worker-pool", "size": 0}}]}}}, "_delete": ["/entity/metadata/name"]}"#,
    r#"{"cloud": "aws", "clusterUid": "c1", "entity": {"metadata": {"name": "demo"}, "spec": {}}}"#,
    r#"{"cloud": "aws", "clusterUid": "c1", "entity": {"spec": {}}}"#,
    r#"{"cloud": "aws", "clusterUid": "c1", "entity": {"metadata": ["demo"], "spec": {}}}"#,
    r#"{"clusterUid": "c1", "entity": {"metadata": 5, "spec": {}}}"#,
];

/// The seeds of the world file. They are the clean world of each engine, the first small fixtures
/// of each engine, which name that world as their base, and the cases.
pub(super) fn world_files() -> Vec<Vec<u8>> {
    let mut seeds: Vec<Vec<u8>> = WORLD_CASES.iter().map(|t| t.as_bytes().to_vec()).collect();
    let fixtures = catalog::rules_dir().join("fixtures");
    let mut engines: Vec<PathBuf> = fs::read_dir(fixtures)
        .expect("the fixtures are there")
        .flatten()
        .map(|entry| entry.path())
        .collect();
    engines.sort();
    for engine in engines {
        let mut files: Vec<PathBuf> = fs::read_dir(&engine)
            .expect("the fixtures of an engine are there")
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|e| e == "json"))
            .collect();
        files.sort();
        let bytes = |path: &PathBuf| fs::read(path).expect("a fixture reads");
        seeds.extend(
            files
                .iter()
                .filter(|p| p.ends_with("clean.json"))
                .map(bytes),
        );
        seeds.extend(
            files
                .iter()
                .filter(|p| !p.ends_with("clean.json"))
                .map(bytes)
                .filter(|content| content.len() <= 1024)
                .take(5),
        );
    }
    seeds
}
