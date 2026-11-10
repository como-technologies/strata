//! The seeds of the file that `--since` names: the reports of the target and of the contract test
//! of the observer.

use serde_json::Value;
use strata::ops::{BEHIND_SCHEMA, BehindReport};

use crate::behind::run;
use crate::support::{RECORD_FILE, scratch};

use super::{file, pin_version};

/// The reports that `scripts/observer-contract.sh --write` writes with the strata binary, for the
/// tests of the observer.
const OBSERVER_REPORTS: [&str; 2] = [
    "observer/internal/docs/testdata/behind.json",
    "observer/internal/docs/testdata/ask-behind.json",
];

/// The report of the run of the target with `since` as `--since`. Each path of the record is the
/// name of its file, so that the seed is the same on each machine. The report names
/// [`SEED_VERSION`](super::SEED_VERSION), so that it is the same in each release.
fn report(since: Option<BehindReport>) -> BehindReport {
    let mut report = run(&scratch(), since);
    pin_version(&mut report.tool);
    report.records = vec![RECORD_FILE.to_string()];
    for by in report.pins.iter_mut().flat_map(|pin| &mut pin.pinned_by) {
        by.path = RECORD_FILE.to_string();
    }
    report
}

/// The seeds of the file that `--since` names.
///
/// They are the reports of the observer, and the report of the run of the target with no `--since`.
/// They are also two runs with `--since`: the first report, and an earlier report that knows only
/// the oldest newer version. Two seeds change one field of the first report, so that the load
/// refuses them. One has a later schema, and one has a number for `newer`.
pub(super) fn reports() -> Vec<Vec<u8>> {
    let first = report(None);
    let mut earlier = first.clone();
    for pin in &mut earlier.pins {
        pin.newer.drain(..pin.newer.len().saturating_sub(1));
    }
    let runs = [
        report(Some(first.clone())),
        report(Some(earlier.clone())),
        first,
        earlier,
    ];
    let mut documents: Vec<Value> = runs
        .iter()
        .map(|r| serde_json::to_value(r).expect("a report is JSON"))
        .collect();
    let mut later = documents[0].clone();
    later["schema"] = format!("{BEHIND_SCHEMA}0").into();
    let mut wrong = documents[0].clone();
    wrong["pins"][0]["newer"] = 5.into();
    documents.extend([later, wrong]);
    let mut seeds: Vec<Vec<u8>> = OBSERVER_REPORTS.iter().map(|f| file(f)).collect();
    seeds.extend(
        documents
            .iter()
            .map(|d| serde_json::to_vec(d).expect("a report is JSON")),
    );
    seeds
}
