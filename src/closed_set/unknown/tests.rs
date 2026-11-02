//! The tests of the watch and of the log of the unknown values.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::{Value, json};

use super::log::{FIELDS, FILE, Line, SCHEMA};
use super::{Watch, note, seen, shown};
use crate::api::registry::Registries;
use crate::api::{CLUSTER_TEMPLATE, MemoryApi, PROFILE_TEMPLATE, Palette};
use crate::json::opt_set;
use crate::model::oci_provider::OciProvider;
use crate::model::pack_state::PackState;
use crate::snapshot::{PROVIDER_TYPE_ANNOTATION, Pack};
use crate::term::progress::Progress;

/// The lines of the log `file`, read as the type that a reader of the log uses.
fn lines_of(file: &Path) -> Vec<Line> {
    std::fs::read_to_string(file)
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// A watch that writes no note and logs to `FILE` in `dir`.
fn logging(dir: &Path) -> Watch {
    Watch::new(Progress::silent()).logging_to(dir.join(FILE))
}

#[test]
fn the_note_names_the_set_and_the_value_and_says_that_this_release_does_not_know_it() {
    assert_eq!(
        note("OciProvider", "oras"),
        "Palette gave the OciProvider value 'oras', which this release of strata does not know"
    );
}

#[test]
fn a_value_that_a_read_gives_and_the_set_does_not_name_is_one_line_of_the_log() {
    let dir = tempfile::tempdir().unwrap();
    let _watching = logging(dir.path()).start();
    let doc = json!({"spec": {"providerType": "oras"}});
    let value = opt_set::<OciProvider>(&doc, "/spec/providerType", "/v1/registries/oci/summary");
    // the read keeps the text, as it did before the watch
    assert_eq!(value.unwrap().as_str(), "oras");
    let lines = lines_of(&dir.path().join(FILE));
    assert_eq!(lines.len(), 1, "{lines:?}");
    let line = &lines[0];
    assert_eq!(line.schema, SCHEMA);
    assert_eq!(line.strata, crate::TOOL_VERSION);
    assert_eq!(line.set, "OciProvider");
    assert_eq!(line.value, "oras");
    assert_eq!(line.route, "/v1/registries/oci/summary");
    assert_eq!(
        (
            line.palette.as_deref(),
            line.status,
            line.error_code.as_deref()
        ),
        (None, None, None)
    );
}

#[test]
fn a_value_that_the_set_names_writes_no_line() {
    let dir = tempfile::tempdir().unwrap();
    let _watching = logging(dir.path()).start();
    let doc = json!({"spec": {"providerType": "zarf"}});
    assert!(opt_set::<OciProvider>(&doc, "/spec/providerType", "/v1/test").is_some());
    assert!(!dir.path().join(FILE).exists());
}

#[test]
fn a_run_notes_a_value_one_time_for_each_set_and_each_value() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(FILE);
    let _watching = logging(dir.path()).start();
    seen::<OciProvider>("oras", "/v1/a");
    seen::<OciProvider>("oras", "/v1/b");
    assert_eq!(
        lines_of(&file).len(),
        1,
        "the same set and value, from two routes"
    );
    seen::<OciProvider>("other", "/v1/a");
    seen::<PackState>("oras", "/v1/a");
    let told: BTreeSet<(String, String)> = lines_of(&file)
        .into_iter()
        .map(|l| (l.set, l.value))
        .collect();
    let expected: BTreeSet<(String, String)> = [
        ("OciProvider", "oras"),
        ("OciProvider", "other"),
        ("PackState", "oras"),
    ]
    .into_iter()
    .map(|(set, value)| (set.to_string(), value.to_string()))
    .collect();
    assert_eq!(told, expected);
}

#[test]
fn a_quiet_watch_still_writes_the_log() {
    let dir = tempfile::tempdir().unwrap();
    let _watching = Watch::new(Progress::new(true))
        .logging_to(dir.path().join(FILE))
        .start();
    seen::<OciProvider>("oras", "/v1/a");
    assert_eq!(lines_of(&dir.path().join(FILE)).len(), 1);
}

#[test]
fn a_program_with_no_watch_writes_no_line_and_a_dropped_watch_stops() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(FILE);
    seen::<OciProvider>("oras", "/v1/a");
    let watching = logging(dir.path()).start();
    drop(watching);
    seen::<OciProvider>("oras", "/v1/a");
    assert!(!file.exists());
}

#[test]
fn a_new_watch_notes_the_values_again() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(FILE);
    for _ in 0..2 {
        let _watching = logging(dir.path()).start();
        seen::<OciProvider>("oras", "/v1/a");
    }
    assert_eq!(lines_of(&file).len(), 2, "one line for each run");
}

#[test]
fn the_release_is_in_the_line_only_when_the_caller_gives_it() {
    let dir = tempfile::tempdir().unwrap();
    let _watching = logging(dir.path()).palette_release("4.10").start();
    seen::<OciProvider>("oras", "/v1/a");
    assert_eq!(
        lines_of(&dir.path().join(FILE))[0].palette.as_deref(),
        Some("4.10")
    );
}

#[test]
fn a_log_that_cannot_be_written_gives_no_error_and_the_read_goes_on() {
    let dir = tempfile::tempdir().unwrap();
    // a file where the folder of the log must be
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, "x").unwrap();
    let _watching = Watch::new(Progress::silent())
        .logging_to(blocker.join(FILE))
        .start();
    let doc = json!({"spec": {"providerType": "oras"}});
    let value = opt_set::<OciProvider>(&doc, "/spec/providerType", "/v1/test");
    assert_eq!(value.unwrap().as_str(), "oras");
}

#[test]
fn a_value_shows_at_most_64_characters_and_no_control_character() {
    assert_eq!(shown("quarantined"), "quarantined");
    assert_eq!(shown(&"a".repeat(100)), format!("{}…", "a".repeat(63)));
    assert_eq!(shown("a\u{1b}[31mb\n"), "a?[31mb?");
    // the cut falls on a character and not on a byte
    assert_eq!(shown(&"é".repeat(70)).chars().count(), 64);
}

// ---- the line ------------------------------------------------------------------------------

/// A line with each field set.
fn full_line() -> Line {
    let mut line = Line::new(
        "OciProvider",
        "oras",
        "/v1/registries/oci/summary",
        Some("4.10"),
    );
    line.status = Some(200);
    line.error_code = Some("Code".to_string());
    line
}

#[test]
fn a_line_holds_only_the_listed_fields() {
    let written: Value = serde_json::to_value(full_line()).unwrap();
    let names: BTreeSet<&str> = written
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(names, BTreeSet::from(FIELDS));
}

#[test]
fn a_line_with_another_field_is_refused() {
    let mut line = serde_json::to_value(full_line()).unwrap();
    for extra in ["uid", "name", "host", "message", "values"] {
        line[extra] = json!("x");
        assert!(
            serde_json::from_value::<Line>(line.clone()).is_err(),
            "a line with `{extra}` was read"
        );
        line.as_object_mut().unwrap().remove(extra);
    }
    assert!(serde_json::from_value::<Line>(line).is_ok());
}

#[test]
fn a_field_that_has_no_value_is_not_written() {
    let line = Line::new("OciProvider", "oras", "/v1/test", None);
    let written = serde_json::to_value(&line).unwrap();
    let names: BTreeSet<&str> = written
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        names,
        BTreeSet::from(["schema", "strata", "set", "value", "route"])
    );
}

#[test]
fn the_log_adds_a_line_to_the_lines_it_has_and_makes_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("state").join("strata").join(FILE);
    full_line().append(&file).unwrap();
    Line::new("PackState", "quarantined", "/v1/packs", None)
        .append(&file)
        .unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(text.lines().count(), 2, "{text}");
    assert!(text.ends_with('\n'));
    assert_eq!(lines_of(&file)[1].set, "PackState");
}

// ---- no name and no uid ----------------------------------------------------------------------

#[test]
fn a_document_with_a_uid_and_a_name_writes_neither_to_the_log() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(FILE);
    let _watching = logging(dir.path()).start();
    let doc = json!({
        "metadata": {"uid": "64f0000000000000000000a1", "name": "payments-prod"},
        "spec": {"published": {"type": "later-type", "packs": [{"name": "secret-pack"}]}},
    });
    let found = opt_set::<crate::model::profile_type::ProfileType>(
        &doc,
        "/spec/published/type",
        PROFILE_TEMPLATE,
    );
    assert_eq!(found.unwrap().as_str(), "later-type");
    let text = std::fs::read_to_string(&file).unwrap();
    for secret in [
        "64f0000000000000000000a1",
        "payments-prod",
        "secret-pack",
        "metadata",
    ] {
        assert!(!text.contains(secret), "the log holds `{secret}`: {text}");
    }
    let line = &lines_of(&file)[0];
    assert_eq!(line.route, "/v1/clusterprofiles/{uid}");
    assert_eq!(line.value, "later-type");
}

// ---- the reads that give a value of a closed set ---------------------------------------------

#[test]
fn a_pack_state_that_the_listing_gives_and_the_set_does_not_name_names_the_pack_listing() {
    let dir = tempfile::tempdir().unwrap();
    let _watching = logging(dir.path()).start();
    let summary = json!({"spec": {"annotations": {"system_state": "quarantined"}}});
    let state = crate::api::registry::pack_state(&summary);
    assert_eq!(state.unwrap().as_str(), "quarantined");
    let lines = lines_of(&dir.path().join(FILE));
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(
        (lines[0].set.as_str(), lines[0].route.as_str()),
        ("PackState", "/v1/packs")
    );
}

#[test]
fn a_provider_type_that_the_oci_summary_gives_names_the_oci_summary() {
    let dir = tempfile::tempdir().unwrap();
    let _watching = logging(dir.path()).start();
    let api = MemoryApi::new().with(
        "/v1/registries/oci/summary",
        json!({"items": [{"metadata": {"uid": "r1"}, "spec": {"providerType": "oras"}}]}),
    );
    let palette = Palette::new(api);
    let found = Registries::new(&palette).oci_provider_type("r1").unwrap();
    assert_eq!(found.unwrap().as_str(), "oras");
    let lines = lines_of(&dir.path().join(FILE));
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(
        (lines[0].set.as_str(), lines[0].route.as_str()),
        ("OciProvider", "/v1/registries/oci/summary")
    );
}

#[test]
fn a_provider_type_on_the_pack_ref_of_a_cluster_names_the_cluster_route() {
    let dir = tempfile::tempdir().unwrap();
    let _watching = logging(dir.path()).start();
    let pack_ref = json!({"name": "chart", "annotations": {PROVIDER_TYPE_ANNOTATION: "oras"}});
    let pack = Pack::from_api_with(&pack_ref, &[], None, CLUSTER_TEMPLATE);
    assert_eq!(pack.registry_provider_type.unwrap().as_str(), "oras");
    let lines = lines_of(&dir.path().join(FILE));
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0].route, "/v1/spectroclusters/{uid}");
}
