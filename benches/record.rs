//! The record: the canonical form and the root of a profile, and the build of a record from the
//! responses of Palette.
//!
//! The canonical form, the hashes, and the root are not public items of the library. So each
//! benchmark reaches them through `Snapshot::from_raw`, the one function that computes them.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use strata::snapshot::Snapshot;
use strata_benches::FETCHED_AT;
use strata_benches::profile::{Change, Shape, raw_profile, snapshot};

/// The sizes of a profile for the canonical form and the root: one pack, a usual stack, and the
/// most that Palette takes.
const PACKS: [usize; 3] = [1, 50, 200];

/// The canonical form of the values of each pack, and the hashes up to the root, with no manifest.
fn canon_and_root(c: &mut Criterion) {
    let mut group = c.benchmark_group("canon_root");
    group.sample_size(10);
    for packs in PACKS {
        let raw = raw_profile(Shape::packs(packs), Change::None);
        group.bench_with_input(
            BenchmarkId::new("build", format!("{packs}_packs")),
            &raw,
            |b, raw| {
                b.iter(|| {
                    Snapshot::from_raw(black_box(raw), "bench", FETCHED_AT).expect("a record")
                });
            },
        );
    }
    // the digest over the packs of a record that exists: the canonical form of the hashes alone
    let record = snapshot(Shape::packs(200), Change::None);
    group.bench_function("content_digest/200_packs", |b| {
        b.iter(|| black_box(&record).content_digest());
    });
    group.finish();
}

/// A record from the responses of the largest profile that Palette takes: 200 packs, 200
/// manifests, and its variables.
fn snapshot_largest(c: &mut Criterion) {
    let raw = raw_profile(Shape::largest(), Change::None);
    let record = Snapshot::from_raw(&raw, "bench", FETCHED_AT).expect("a record");
    assert_eq!(record.packs.len(), 200);
    assert_eq!(
        record
            .packs
            .iter()
            .map(|p| p.manifests.len())
            .sum::<usize>(),
        200
    );
    let mut group = c.benchmark_group("snapshot");
    group.sample_size(10);
    group.bench_function("from_raw/200_packs_200_manifests", |b| {
        b.iter(|| Snapshot::from_raw(black_box(&raw), "bench", FETCHED_AT).expect("a record"));
    });
    group.finish();
}

criterion_group!(benches, canon_and_root, snapshot_largest);
criterion_main!(benches);
