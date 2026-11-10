//! The target of the report of `behind`: the file that `--since` names.
//!
//! `BehindReport::load` reads the file. A report that loads is the report of an earlier run. So the
//! target gives it to a run of `behind` as `--since`, as the command does. The run reads one record
//! against a registry with newer versions of its pack. So the input also reaches the code that
//! names the versions that arrived since the earlier run.

use std::path::Path;

use serde_json::{Value, json};
use strata::api::{MemoryApi, Palette};
use strata::ops::{self, BehindOptions, BehindReport, Pin};
use strata::render::{Format, renderer};
use strata::snapshot::{Pack, Snapshot};
use strata::term::color::Paint;
use strata::term::progress::Progress;

use crate::routes::{PackFixture, REGISTRY_LISTING, serve_registry_pack};
use crate::support::{AGAIN_FILE, AT, RECORD_FILE, answer, raw_profile, scratch, write};

/// The versions above the pinned one that the registry offers, newest first.
const NEWER: [&str; 2] = ["2.0.0", "1.1.0"];

/// The name of the file that `--since` names.
const SINCE: &str = "since.json";

/// A Palette whose registry offers the pack `pinned` at its version and at each of [`NEWER`], for
/// the cloud of the record.
fn registry(pinned: &Pack) -> MemoryApi {
    let versions = std::iter::once(pinned.version.as_str()).chain(NEWER);
    let (summaries, tags): (Vec<Value>, Vec<Value>) = versions
        .enumerate()
        .map(|(i, version)| {
            let uid = format!("build-{i}");
            let pack = PackFixture::new(
                &pinned.name,
                &pinned.layer,
                version,
                &uid,
                &pinned.registry_uid,
                "",
                "",
            );
            (
                pack.summary(&["aws"]),
                json!({"tag": version, "packUid": uid}),
            )
        })
        .unzip();
    let listing = json!({"items": [{"name": "Repo", "uid": pinned.registry_uid, "kind": "pack"}]});
    serve_registry_pack(
        MemoryApi::new().with(REGISTRY_LISTING, listing),
        &pinned.name,
        Value::Array(summaries),
        json!({"layer": pinned.layer, "tags": tags}),
        &pinned.registry_uid,
    )
}

/// The report of a run of `behind` with `since` as `--since`. The function writes a record of a
/// profile with one pack in the directory `dir`. The run reads it against the Palette of
/// [`registry`].
pub(crate) fn run(dir: &Path, since: Option<BehindReport>) -> BehindReport {
    let record = Snapshot::from_raw(&raw_profile("", ""), "fuzz", AT).expect("the record builds");
    let json = record.to_json(false).expect("a record is JSON");
    write(dir, RECORD_FILE, json.as_bytes());
    let api = registry(&record.packs[0]);
    let options = BehindOptions {
        since,
        ..BehindOptions::default()
    };
    ops::behind(
        &[dir.join(RECORD_FILE)],
        &options,
        |_| Ok(Palette::new(&api)),
        AT,
        &Progress::silent(),
    )
    .expect("a run of behind on a record that strata wrote gives a report")
}

/// Make sure that the JSON that `--json` writes for `report` loads as the same report. `dir` is the
/// directory for the file.
#[track_caller]
fn writes_and_loads_again(dir: &Path, report: &BehindReport) {
    let json = renderer(Format::Json, Paint::off(), false)
        .render_behind(report)
        .expect("a report is written as JSON");
    write(dir, AGAIN_FILE, json.as_bytes());
    let again = answer(BehindReport::load(&dir.join(AGAIN_FILE)))
        .unwrap_or_else(|| panic!("strata cannot load the report that it wrote:\n{json}"));
    assert_eq!(
        serde_json::to_value(&again).expect("a report is JSON"),
        serde_json::to_value(report).expect("a report is JSON"),
        "a report changes when strata writes it and loads it:\n{json}"
    );
}

/// Make sure that `report`, the run with `previous` as `--since`, names what arrived since.
///
/// The earlier report knows a version when a pin of the same pack and registry pins it, or names it
/// as newer. A pin whose pack the earlier report has in the registry names each newer version that
/// it does not know as arrived. Another pin names none.
#[track_caller]
fn check_arrived(previous: &BehindReport, report: &BehindReport) {
    assert_eq!(
        report.since.as_deref(),
        Some(previous.generated_at.as_str())
    );
    let mut count = 0;
    for pin in &report.pins {
        let earlier: Vec<&Pin> = previous
            .pins
            .iter()
            .filter(|p| p.name == pin.name && p.registry_uid == pin.registry_uid)
            .collect();
        let known = |version: &str| {
            earlier
                .iter()
                .any(|p| p.version == version || p.newer.iter().any(|n| n.version == version))
        };
        let arrived = (!earlier.is_empty()).then(|| {
            pin.newer
                .iter()
                .filter(|n| !known(&n.version))
                .map(|n| n.version.clone())
                .collect::<Vec<_>>()
        });
        assert_eq!(pin.arrived, arrived, "the arrived versions of {}", pin.name);
        count += arrived.map_or(0, |a| a.len());
    }
    assert_eq!(
        report.summary.arrived, count,
        "the count of the arrived versions"
    );
}

/// Load `bytes` as the file that `--since` names. Return true when the file is a report.
///
/// A report that loads writes as JSON that loads as the same report. As the `--since` of a run, it
/// gives a report that names what arrived since, and that each renderer shows.
pub(crate) fn since(bytes: &[u8]) -> bool {
    let dir = scratch();
    write(&dir, SINCE, bytes);
    let Some(previous) = answer(BehindReport::load(&dir.join(SINCE))) else {
        return false;
    };
    writes_and_loads_again(&dir, &previous);
    let report = run(&dir, Some(previous.clone()));
    check_arrived(&previous, &report);
    for &format in Format::ALL {
        let shown = renderer(format, Paint::off(), false).render_behind(&report);
        assert!(shown.is_ok(), "{format:?} cannot show the run: {shown:?}");
    }
    true
}

#[test]
fn a_behind_report_is_the_since_of_a_run_or_is_refused_with_a_kind() {
    bolero::check!().for_each(|bytes| {
        since(bytes);
    });
}
