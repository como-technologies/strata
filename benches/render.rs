//! Each renderer on the comparison of two records of the largest profile that Palette takes, with
//! a change in each pack, with and without `--full`.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use strata::ops;
use strata::render::{Format, renderer};
use strata::term::color::Paint;
use strata_benches::GENERATED_AT;
use strata_benches::profile::{Change, Shape, snapshot};

fn render(c: &mut Criterion) {
    let shape = Shape::largest();
    let before = snapshot(shape, Change::None);
    let after = snapshot(shape, Change::EveryPack);
    let diff = ops::compare(&before, &after, GENERATED_AT);
    let mut group = c.benchmark_group("render");
    group.sample_size(10);
    for &format in Format::ALL {
        let name = format.as_str();
        let renderer = renderer(format, Paint::off(), true);
        for (case, full) in [("summary", false), ("full", true)] {
            let text = renderer
                .render(&diff, &before, &after, full)
                .expect("a report");
            assert!(!text.is_empty(), "{name} {case}");
            group.bench_with_input(BenchmarkId::new(name, case), &full, |b, &full| {
                b.iter(|| {
                    renderer
                        .render(
                            black_box(&diff),
                            black_box(&before),
                            black_box(&after),
                            full,
                        )
                        .expect("a report")
                });
            });
        }
    }
    group.finish();
}

criterion_group!(benches, render);
criterion_main!(benches);
