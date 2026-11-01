//! The comparison of two records of the largest profile that Palette takes: 200 packs and 200
//! manifests.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use strata::ops;
use strata_benches::GENERATED_AT;
use strata_benches::profile::{Change, Shape, snapshot};

fn compare(c: &mut Criterion) {
    let shape = Shape::largest();
    let before = snapshot(shape, Change::None);
    let mut group = c.benchmark_group("compare");
    group.sample_size(10);
    // each case, and how many packs the comparison finds changed
    for (name, change, changed) in [
        ("identical", Change::None, 0),
        ("one_change", Change::OnePack, 1),
        ("every_pack", Change::EveryPack, 200),
    ] {
        let after = snapshot(shape, change);
        let diff = ops::compare(&before, &after, GENERATED_AT);
        assert_eq!(diff.summary.packs_changed, changed, "{name}");
        group.bench_with_input(BenchmarkId::from_parameter(name), &after, |b, after| {
            b.iter(|| ops::compare(black_box(&before), black_box(after), GENERATED_AT))
        });
    }
    group.finish();
}

criterion_group!(benches, compare);
criterion_main!(benches);
