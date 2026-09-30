//! Every workflow file under .github/workflows parses as YAML, and has the two keys a workflow
//! needs. GitHub reads each file on each push and records a failed run, with no job, for a file
//! it cannot parse; the reusable workflows never run here, so nothing else would show it. A
//! colon and a space inside an unquoted description is the fault this found.

use std::path::Path;

#[test]
fn every_workflow_file_parses_and_has_on_and_jobs() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows");
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "yml" && e != "yaml") {
            continue;
        }
        seen += 1;
        let text = std::fs::read_to_string(&path).unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&text)
            .unwrap_or_else(|e| panic!("{}: not YAML: {e}", path.display()));
        let map = doc
            .as_mapping()
            .unwrap_or_else(|| panic!("{}: not a mapping", path.display()));
        for key in ["on", "jobs"] {
            assert!(map.contains_key(key), "{}: no `{key}` key", path.display());
        }
    }
    assert!(
        seen >= 5,
        "only {seen} workflow files under {}",
        dir.display()
    );
}

/// The preflight workflow passes each record pattern to strata as the caller typed it. An
/// unquoted `$RECORDS` in a `for` list is expanded by bash first, which reads `**` as `*` and
/// keeps only the files that match, so a pattern such as `releases/**/*.json` loses records.
#[test]
fn the_preflight_workflow_does_not_let_the_shell_expand_the_record_patterns() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/preflight.yml");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        !text.contains("in $RECORDS"),
        "{}: the record patterns go through an unquoted expansion",
        path.display()
    );
    assert!(
        text.contains("read -ra records <<< \"$RECORDS\""),
        "{}: the record patterns are not split with read -ra",
        path.display()
    );
}
