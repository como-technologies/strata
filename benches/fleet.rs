//! `verify` and `consumers` against an in-memory Palette, with a fleet of 1, 100, and 1000
//! clusters.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use strata::api::Palette;
use strata::ops::{self, RecordStatus, VerifyOptions};
use strata::reference::Ref;
use strata::term::progress::Progress;
use strata_benches::GENERATED_AT;
use strata_benches::fleet::{VerifyFleet, all_running, consumer_fleet};
use strata_benches::profile::PROFILE_UID;

/// The sizes of a fleet.
const CLUSTERS: [usize; 3] = [1, 100, 1000];

/// `verify` of one record for each cluster. The records are files, so the time includes their read.
fn verify(c: &mut Criterion) {
    let mut group = c.benchmark_group("verify");
    group.sample_size(10);
    let quiet = Progress::silent();
    for clusters in CLUSTERS {
        let fleet = VerifyFleet::new(clusters);
        let run = || {
            ops::verify_many(
                &fleet.records,
                None,
                |_| Ok(Palette::new(&fleet.api)),
                GENERATED_AT,
                &VerifyOptions::default(),
                &quiet,
            )
        };
        // a fleet that Palette still holds as recorded: each record is unchanged
        let summary = run();
        assert_eq!(summary.summary.records, clusters);
        assert_eq!(
            summary.summary.status,
            RecordStatus::Unchanged,
            "{:?}",
            summary.records.first()
        );
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{clusters}_clusters")),
            &clusters,
            |b, _| b.iter(|| black_box(run())),
        );
    }
    group.finish();
}

/// `consumers` of a profile version that each cluster of the fleet runs.
fn consumers(c: &mut Criterion) {
    let mut group = c.benchmark_group("consumers");
    group.sample_size(10);
    let quiet = Progress::silent();
    for clusters in CLUSTERS {
        let api = consumer_fleet(clusters);
        let reference = Ref::Uid(PROFILE_UID.to_string());
        let run = || {
            ops::consumers(&reference, |_| Ok(Palette::new(&api)), GENERATED_AT, &quiet)
                .expect("a report")
        };
        let report = run();
        assert_eq!(report.count, clusters);
        assert!(all_running(&report));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{clusters}_clusters")),
            &clusters,
            |b, _| b.iter(|| black_box(run())),
        );
    }
    group.finish();
}

criterion_group!(benches, verify, consumers);
criterion_main!(benches);
