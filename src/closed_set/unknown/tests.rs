//! The tests of the watch and of the log of the unknown values.

use std::collections::BTreeSet;
use std::path::Path;

use chrono::{DateTime, TimeDelta, Utc};
use serde_json::{Value, json};

use super::hint::{FILE as HINT_FILE, Hint, TEXT};
use super::log::{
    FIELDS, FILE, Line, SCHEMA, SENT_FILE, Valid, mark_sent, read, read_valid, sent_count,
    sent_path,
};
use super::report::{Entry, Report, SCHEMA as REPORT_SCHEMA};
use super::sighting::shown;
use super::{Watch, note, seen};
use crate::api::registry::Registries;
use crate::api::{CLUSTER_TEMPLATE, MemoryApi, OCI_SUMMARY_TEMPLATE, PROFILE_TEMPLATE, Palette};
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
    let value = opt_set::<OciProvider>(&doc, "/spec/providerType", OCI_SUMMARY_TEMPLATE);
    // the read keeps the text, as it did before the watch
    assert_eq!(value.unwrap().as_str(), "oras");
    let lines = lines_of(&dir.path().join(FILE));
    assert_eq!(lines.len(), 1, "{lines:?}");
    let line = &lines[0];
    assert_eq!(line.schema, SCHEMA);
    assert_eq!(line.sighting.strata, crate::TOOL_VERSION);
    assert_eq!(line.sighting.set, "OciProvider");
    assert_eq!(line.sighting.value, "oras");
    assert_eq!(line.sighting.route, OCI_SUMMARY_TEMPLATE);
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
        .map(|l| (l.sighting.set, l.sighting.value))
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
fn a_value_shows_at_most_64_characters_and_only_printable_ascii() {
    assert_eq!(shown("quarantined"), "quarantined");
    assert_eq!(shown(&"a".repeat(64)), "a".repeat(64), "64 characters fit");
    assert_eq!(shown(&"a".repeat(100)), format!("{}...", "a".repeat(61)));
    assert_eq!(shown("a\u{1b}[31mb\n"), "a?[31mb?");
    // a character of two bytes is one character, and no cut stops in it
    assert_eq!(shown(&"é".repeat(70)), format!("{}...", "?".repeat(61)));
}

#[test]
fn a_format_character_and_a_letter_outside_ascii_show_as_a_mark() {
    // U+200B is a zero width space, and U+202E sets the direction of the text from there on
    assert_eq!(shown("pending\u{200b}"), "pending?");
    assert_eq!(shown("later\u{202e}gnp.exe"), "later?gnp.exe");
    assert_eq!(
        shown("p\u{0435}nding"),
        "p?nding",
        "a Cyrillic letter that looks like e"
    );
    assert_ne!(shown("pending\u{200b}"), shown("pending"));
}

#[test]
fn a_space_shows_as_a_mark_so_that_no_value_that_strata_writes_has_a_space() {
    // markdown drops one space at each end of a code span: ` pending ` and `pending` look equal
    assert_eq!(shown(" pending "), "?pending?");
    assert_eq!(shown("two words"), "two?words");
    for value in [" pending ", "two words", "\u{a0}", "a b\tc"] {
        assert_eq!(
            super::sighting::value_fault(&shown(value)),
            None,
            "{value:?}"
        );
    }
}

// ---- the line ------------------------------------------------------------------------------

/// A line of the log.
fn full_line() -> Line {
    Line::new("OciProvider", "oras", OCI_SUMMARY_TEMPLATE)
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
fn a_line_and_an_entry_flatten_one_sighting_and_still_refuse_each_field_outside_the_list() {
    let line = serde_json::to_value(full_line()).unwrap();
    let entry = serde_json::to_value(&Report::of(&[full_line()]).unknown[0]).unwrap();
    // `status`, `errorCode`, and `palette` were fields of release 0.27.0 that no code wrote
    for extra in [
        "uid",
        "name",
        "host",
        "message",
        "values",
        "status",
        "errorCode",
        "palette",
    ] {
        let mut line = line.clone();
        line[extra] = json!("x");
        let error = serde_json::from_value::<Line>(line)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(&format!("unknown field `{extra}`")),
            "{error}"
        );
        let mut entry = entry.clone();
        entry[extra] = json!("x");
        let error = serde_json::from_value::<Entry>(entry)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(&format!("unknown field `{extra}`")),
            "{error}"
        );
    }
    // the fields of the sighting are the same in the line and in the entry
    for field in ["strata", "set", "value", "route"] {
        assert_eq!(line[field], entry[field], "{field}");
    }
}

#[test]
fn the_log_adds_a_line_to_the_lines_it_has_and_makes_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("state").join("strata").join(FILE);
    full_line().append(&file).unwrap();
    Line::new("PackState", "quarantined", "/v1/packs")
        .append(&file)
        .unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(text.lines().count(), 2, "{text}");
    assert!(text.ends_with('\n'));
    assert_eq!(lines_of(&file)[1].sighting.set, "PackState");
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
    assert_eq!(line.sighting.route, PROFILE_TEMPLATE);
    assert_eq!(line.sighting.value, "later-type");
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
        (
            lines[0].sighting.set.as_str(),
            lines[0].sighting.route.as_str()
        ),
        ("PackState", "/v1/packs")
    );
}

#[test]
fn a_provider_type_that_the_oci_summary_gives_names_the_oci_summary() {
    let dir = tempfile::tempdir().unwrap();
    let _watching = logging(dir.path()).start();
    let api = MemoryApi::new().with(
        OCI_SUMMARY_TEMPLATE,
        json!({"items": [{"metadata": {"uid": "r1"}, "spec": {"providerType": "oras"}}]}),
    );
    let palette = Palette::new(api);
    let found = Registries::new(&palette).oci_provider_type("r1").unwrap();
    assert_eq!(found.unwrap().as_str(), "oras");
    let lines = lines_of(&dir.path().join(FILE));
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(
        (
            lines[0].sighting.set.as_str(),
            lines[0].sighting.route.as_str()
        ),
        ("OciProvider", OCI_SUMMARY_TEMPLATE)
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
    assert_eq!(lines[0].sighting.route, CLUSTER_TEMPLATE);
}

// ---- the read of the log ---------------------------------------------------------------------

/// The text of a log with `lines`, one for each element.
fn log_of(lines: &[&str]) -> String {
    lines.iter().map(|l| format!("{l}\n")).collect()
}

/// A line of the log as strata writes it, with the value `value`.
fn written(value: &str) -> String {
    serde_json::to_string(&Line::new("PackState", value, "/v1/packs")).unwrap()
}

#[test]
fn a_log_that_is_not_there_is_an_empty_log() {
    let dir = tempfile::tempdir().unwrap();
    assert!(read(&dir.path().join(FILE)).unwrap().is_empty());
}

#[test]
fn a_read_for_the_send_keeps_the_valid_lines_and_names_the_numbers_of_the_others() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(FILE);
    let mut with_uid = serde_json::to_value(full_line()).unwrap();
    with_uid["uid"] = json!("64f0000000000000000000a1");
    std::fs::write(
        &file,
        format!(
            "{}\n\n{}\nnot json\n{}\n  \n{}\n",
            written("a"),
            with_uid,
            written("b"),
            written("two words")
        ),
    )
    .unwrap();
    let valid = read_valid(&file).unwrap();
    // a blank line has a number but is no line, and the numbers count the rows of the file
    assert_eq!(valid.invalid, [3, 4, 7]);
    let values: Vec<&str> = valid
        .lines
        .iter()
        .map(|l| l.sighting.value.as_str())
        .collect();
    assert_eq!(values, ["a", "b"]);
    // `read` stops at the first line that is not valid, and names its number
    assert!(read(&file).unwrap_err().to_string().contains("line 3"));
}

#[test]
fn a_read_for_the_send_of_a_log_that_is_not_there_or_that_is_valid_names_no_line() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(FILE);
    assert_eq!(read_valid(&file).unwrap(), Valid::default());
    std::fs::write(&file, log_of(&[&written("a")])).unwrap();
    let valid = read_valid(&file).unwrap();
    assert_eq!(valid.lines, read(&file).unwrap());
    assert!(valid.invalid.is_empty());
}

#[test]
fn the_read_gives_the_lines_that_strata_wrote_in_order_and_skips_a_blank_line() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(FILE);
    full_line().append(&file).unwrap();
    std::fs::write(
        &file,
        format!(
            "{}\n\n{}\n  \n",
            serde_json::to_string(&full_line()).unwrap(),
            written("retired")
        ),
    )
    .unwrap();
    let lines = read(&file).unwrap();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], full_line());
    assert_eq!(lines[1].sighting.value, "retired");
}

#[test]
fn a_log_with_a_uid_in_a_line_fails_the_read_and_names_the_line() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(FILE);
    let mut with_uid = serde_json::to_value(full_line()).unwrap();
    with_uid["uid"] = json!("64f0000000000000000000a1");
    std::fs::write(&file, log_of(&[&written("a"), &with_uid.to_string()])).unwrap();
    let error = read(&file).unwrap_err();
    assert_eq!(error.kind(), "log-invalid");
    let message = error.to_string();
    assert!(message.contains("line 2"), "{message}");
    assert!(message.contains("unknown field `uid`"), "{message}");
    assert!(
        !message.contains("64f0000000000000000000a1"),
        "the message holds the uid: {message}"
    );
    let document = error.to_json();
    assert_eq!(document["line"], 2);
    assert_eq!(document["path"], file.display().to_string());
}

#[test]
fn a_log_fails_the_read_for_each_field_that_the_list_does_not_name() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(FILE);
    for extra in ["uid", "name", "host", "message", "values", "project"] {
        let mut line = serde_json::to_value(full_line()).unwrap();
        line[extra] = json!("x");
        std::fs::write(&file, log_of(&[&line.to_string()])).unwrap();
        let error = read(&file).unwrap_err();
        assert_eq!(error.kind(), "log-invalid", "`{extra}`");
    }
}

#[test]
fn a_line_that_is_not_json_or_has_a_field_of_the_wrong_type_fails_the_read() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(FILE);
    let only_the_schema = json!({"schema": SCHEMA}).to_string();
    for bad in ["not json", "[1, 2]", only_the_schema.as_str()] {
        std::fs::write(&file, log_of(&[bad])).unwrap();
        let error = read(&file).unwrap_err();
        assert_eq!(error.kind(), "log-invalid", "{bad}");
    }
    let mut wrong = serde_json::to_value(full_line()).unwrap();
    wrong["value"] = json!(404);
    std::fs::write(&file, log_of(&[&wrong.to_string()])).unwrap();
    assert_eq!(read(&file).unwrap_err().kind(), "log-invalid");
}

#[test]
fn a_line_of_another_schema_fails_the_read() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(FILE);
    let mut other = serde_json::to_value(full_line()).unwrap();
    other["schema"] = json!("strata-unknown/2");
    std::fs::write(&file, log_of(&[&other.to_string()])).unwrap();
    let message = read(&file).unwrap_err().to_string();
    assert!(
        message.contains(&format!("the schema is not {SCHEMA}")),
        "{message}"
    );
}

#[test]
fn a_route_that_is_the_path_of_a_real_request_fails_the_read() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(FILE);
    for route in [
        "/v1/clusterprofiles/64f0000000000000000000a1",
        "/v1/spectroclusters/64F0000000000000000000A1/validate",
        "/v1/things/123e4567-e89b-12d3-a456-426614174000",
        "v1/packs",
        "/packs",
        "",
    ] {
        let mut line = serde_json::to_value(full_line()).unwrap();
        line["route"] = json!(route);
        std::fs::write(&file, log_of(&[&line.to_string()])).unwrap();
        assert_eq!(read(&file).unwrap_err().kind(), "log-invalid", "{route:?}");
    }
    // the templates that strata writes stay valid
    for route in [PROFILE_TEMPLATE, CLUSTER_TEMPLATE, OCI_SUMMARY_TEMPLATE] {
        let mut line = serde_json::to_value(full_line()).unwrap();
        line["route"] = json!(route);
        std::fs::write(&file, log_of(&[&line.to_string()])).unwrap();
        assert_eq!(read(&file).unwrap().len(), 1, "{route}");
    }
}

#[test]
fn a_route_with_a_character_outside_the_closed_set_of_a_template_fails_the_read() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(FILE);
    for route in [
        "/v1/packs\n\n@someone",
        "/v1/packs\r",
        "/v1/packs\t",
        "/v1/pa cks",
        "/v1/packs`x`",
        "/v1/packs|x",
        "/v1/@someone",
        "/v1/[a](http://example.invalid)",
        "/v1/<b>packs</b>",
        "/v1/packs?x=1",
        "/v1/packs#1",
        "/v1/pàcks",
        "/v1/packs\u{202e}",
    ] {
        let mut line = serde_json::to_value(full_line()).unwrap();
        line["route"] = json!(route);
        std::fs::write(&file, log_of(&[&line.to_string()])).unwrap();
        let message = read(&file).unwrap_err().to_string();
        assert!(
            message.contains("the route is not a template of a route"),
            "{route:?}: {message}"
        );
    }
    // the characters of a template stay valid
    let mut line = serde_json::to_value(full_line()).unwrap();
    line["route"] = json!("/v1/clusterprofiles/{uid}/pack_a-b.c/{name}");
    std::fs::write(&file, log_of(&[&line.to_string()])).unwrap();
    assert_eq!(read(&file).unwrap().len(), 1);
}

#[test]
fn a_value_that_is_empty_long_or_not_text_fails_the_read() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(FILE);
    for value in [
        String::new(),
        "a".repeat(65),
        "a\u{1b}b".to_string(),
        "pending\u{200b}".to_string(),
        "later\u{202e}gnp.exe".to_string(),
        "p\u{0435}nding".to_string(),
        " pending ".to_string(),
        "pending ".to_string(),
        "two words".to_string(),
    ] {
        let mut line = serde_json::to_value(full_line()).unwrap();
        line["value"] = json!(value);
        std::fs::write(&file, log_of(&[&line.to_string()])).unwrap();
        assert_eq!(read(&file).unwrap_err().kind(), "log-invalid", "{value:?}");
    }
    // the longest value that a watch writes is 64 characters, with the mark of a cut
    let mut line = serde_json::to_value(full_line()).unwrap();
    line["value"] = json!(shown(&"a".repeat(100)));
    std::fs::write(&file, log_of(&[&line.to_string()])).unwrap();
    assert_eq!(read(&file).unwrap().len(), 1);
}

#[test]
fn every_line_that_a_watch_writes_passes_the_read() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(FILE);
    let _watching = logging(dir.path()).start();
    seen::<OciProvider>("oras", OCI_SUMMARY_TEMPLATE);
    seen::<PackState>(&"é".repeat(100), PROFILE_TEMPLATE);
    seen::<PackState>("a\nb", CLUSTER_TEMPLATE);
    assert_eq!(read(&file).unwrap().len(), 3);
}

// ---- the mark of the lines that the user sent -------------------------------------------------

#[test]
fn the_mark_is_a_file_beside_the_log_and_a_missing_or_damaged_mark_counts_none() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("strata").join(FILE);
    let sent = sent_path(&log);
    assert_eq!(sent, dir.path().join("strata").join(SENT_FILE));
    assert_eq!(sent_count(&sent), 0);
    mark_sent(&sent, 3).unwrap();
    assert_eq!(sent_count(&sent), 3);
    for damaged in ["", "many\n", "-1\n", "3 4\n"] {
        std::fs::write(&sent, damaged).unwrap();
        assert_eq!(sent_count(&sent), 0, "{damaged:?}");
    }
}

// ---- the report ------------------------------------------------------------------------------

/// A line with the set `set`, the value `value`, and the version `strata` of strata.
fn line_of(set: &str, value: &str, strata: &str) -> Line {
    let mut line = Line::new(set, value, "/v1/packs");
    line.sighting.strata = strata.to_string();
    line
}

#[test]
fn the_report_holds_each_different_line_once_with_its_count() {
    let lines = vec![
        line_of("PackState", "retired", "0.26.0"),
        line_of("OciProvider", "oras", "0.26.0"),
        line_of("PackState", "retired", "0.26.0"),
        line_of("PackState", "retired", "0.26.0"),
        line_of("PackState", "retired", "0.27.0"),
    ];
    let report = Report::of(&lines);
    assert_eq!(report.schema, REPORT_SCHEMA);
    let found: Vec<(&str, &str, &str, usize)> = report
        .unknown
        .iter()
        .map(|e| {
            let s = &e.sighting;
            (s.set.as_str(), s.value.as_str(), s.strata.as_str(), e.count)
        })
        .collect();
    assert_eq!(
        found,
        vec![
            ("OciProvider", "oras", "0.26.0", 1),
            ("PackState", "retired", "0.26.0", 3),
            ("PackState", "retired", "0.27.0", 1),
        ]
    );
    assert_eq!(
        report.unknown.iter().map(|e| e.count).sum::<usize>(),
        lines.len(),
        "the counts add up to the lines of the log"
    );
}

#[test]
fn the_report_orders_the_entries_by_set_value_route_and_then_by_version_not_by_text() {
    let lines = vec![
        line_of("B", "x", "0.9.0"),
        line_of("A", "y", "0.26.0"),
        line_of("A", "x", "0.26.0"),
        line_of("A", "x", "0.9.0"),
    ];
    let report = Report::of(&lines);
    let order: Vec<(&str, &str, &str)> = report
        .unknown
        .iter()
        .map(|e| {
            let s = &e.sighting;
            (s.set.as_str(), s.value.as_str(), s.strata.as_str())
        })
        .collect();
    assert_eq!(
        order,
        vec![
            ("A", "x", "0.9.0"),
            ("A", "x", "0.26.0"),
            ("A", "y", "0.26.0"),
            ("B", "x", "0.9.0"),
        ],
        "0.9.0 comes before 0.26.0"
    );
}

#[test]
fn two_lines_with_one_version_order_and_two_texts_are_two_entries_and_the_equal_ones_merge() {
    let lines = vec![
        line_of("A", "x", "1.0.1+2"),
        line_of("A", "x", "1.0.1"),
        line_of("A", "x", "1.0.1+2"),
    ];
    let report = Report::of(&lines);
    assert_eq!(report.unknown.len(), 2);
    assert_eq!(report.unknown.iter().map(|e| e.count).sum::<usize>(), 3);
}

#[test]
fn lines_that_differ_in_the_route_are_separate_entries() {
    let base = line_of("A", "x", "0.26.0");
    let mut other = base.clone();
    other.sighting.route = PROFILE_TEMPLATE.to_string();
    let report = Report::of(&[base.clone(), other, base]);
    assert_eq!(report.unknown.len(), 2);
    assert_eq!(report.unknown.iter().map(|e| e.count).sum::<usize>(), 3);
}

#[test]
fn an_entry_holds_the_fields_of_the_log_and_a_count_and_no_other() {
    let report = Report::of(&[full_line()]);
    let written = serde_json::to_value(&report.unknown[0]).unwrap();
    let names: BTreeSet<&str> = written
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let mut expected: BTreeSet<&str> = FIELDS.into_iter().filter(|f| *f != "schema").collect();
    expected.insert("count");
    assert_eq!(names, expected);
    let document = serde_json::to_value(&report).unwrap();
    let top: BTreeSet<&str> = document
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(top, BTreeSet::from(["schema", "unknown"]));
}

#[test]
fn an_empty_log_gives_an_empty_report_and_a_value_counts_once_for_each_set() {
    assert!(Report::of(&[]).unknown.is_empty());
    assert_eq!(Report::of(&[]).distinct_values(), 0);
    let lines = vec![
        line_of("A", "x", "0.26.0"),
        line_of("A", "x", "0.27.0"),
        line_of("B", "x", "0.26.0"),
    ];
    assert_eq!(Report::of(&lines).distinct_values(), 2);
}

// ---- the hint --------------------------------------------------------------------------------

/// A time on the day of the tests, `hours` after midnight.
fn at(hours: i64) -> DateTime<Utc> {
    "2026-11-02T00:00:00Z".parse::<DateTime<Utc>>().unwrap() + TimeDelta::hours(hours)
}

#[test]
fn the_hint_names_strata_report_and_what_the_user_can_send() {
    // the book quotes the words of the hint, so the page and the const must agree
    let page = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/src/reference/report.md"),
    )
    .unwrap();
    let page = page.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(page.contains(TEXT), "reference/report.md lacks {TEXT:?}");
    assert!(TEXT.starts_with("`strata report` "), "{TEXT}");
}

#[test]
fn the_hint_shows_in_a_terminal_and_records_the_time() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("state").join(HINT_FILE);
    assert_eq!(Hint::new(file.clone(), at(9), true).show(), Some(TEXT));
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(text.trim(), at(9).to_rfc3339());
}

#[test]
fn the_hint_does_not_show_when_standard_error_is_not_a_terminal_and_records_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(HINT_FILE);
    assert_eq!(Hint::new(file.clone(), at(9), false).show(), None);
    assert!(!file.exists(), "a run in a pipe must not record the time");
}

#[test]
fn the_hint_shows_one_time_in_a_day_at_most() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(HINT_FILE);
    let run = |hours: i64| Hint::new(file.clone(), at(hours), true).show();
    assert_eq!(run(9), Some(TEXT));
    assert_eq!(run(9), None, "the same minute");
    assert_eq!(run(10), None);
    assert_eq!(run(32), None, "23 hours after the hint");
    assert_eq!(run(33), Some(TEXT), "24 hours after the hint");
    assert_eq!(run(34), None);
}

#[test]
fn the_hint_shows_again_for_a_time_in_the_future_and_for_a_file_that_holds_no_time() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(HINT_FILE);
    std::fs::write(&file, format!("{}\n", at(100).to_rfc3339())).unwrap();
    assert_eq!(Hint::new(file.clone(), at(9), true).show(), Some(TEXT));
    for damaged in ["", "yesterday\n", "1762041600\n"] {
        std::fs::write(&file, damaged).unwrap();
        assert_eq!(
            Hint::new(file.clone(), at(9), true).show(),
            Some(TEXT),
            "{damaged:?}"
        );
    }
}

#[test]
fn the_hint_stays_back_when_strata_cannot_record_the_time() {
    let dir = tempfile::tempdir().unwrap();
    // a file where the folder of the hint must be
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, "x").unwrap();
    assert_eq!(Hint::new(blocker.join(HINT_FILE), at(9), true).show(), None);
}

#[test]
fn a_watch_shows_the_hint_one_time_in_a_run_and_never_when_quiet() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(HINT_FILE);
    let hint = || Hint::new(file.clone(), at(9), true);
    // --quiet hides the note, so it hides the hint, and it does not record the time
    let mut quiet = Watch::new(Progress::new(true)).hinting(hint());
    assert_eq!(quiet.hint_text(), None);
    assert!(!file.exists());
    let mut watch = Watch::new(Progress::new(false)).hinting(hint());
    assert_eq!(watch.hint_text(), Some(TEXT));
    assert_eq!(watch.hint_text(), None, "one try in a run");
    assert!(file.exists());
}

#[test]
fn a_watch_with_no_hint_shows_none() {
    let mut watch = Watch::new(Progress::new(false));
    assert_eq!(watch.hint_text(), None);
}

#[test]
fn the_schema_of_the_report_refuses_a_field_outside_the_list_in_the_report_and_in_an_entry() {
    // the entry flattens the sighting, and the schema still lists the fields of both
    let schema = schemars::schema_for!(Report);
    let value = schema.as_value();
    assert_eq!(value["additionalProperties"], false);
    let entry = &value["$defs"]["Entry"];
    assert_eq!(entry["additionalProperties"], false);
    let names: BTreeSet<&str> = entry["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        names,
        BTreeSet::from(["strata", "set", "value", "route", "count"])
    );
}
