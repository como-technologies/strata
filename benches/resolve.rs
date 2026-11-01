//! The read of a candidate file of 200 layers, and its resolve against the registry.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use strata::api::Palette;
use strata::candidate;
use strata::term::progress::Progress;
use strata_benches::PALETTE_LIMIT;
use strata_benches::candidate::{registry, write};

fn resolve(c: &mut Criterion) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("candidate.yaml");
    write(PALETTE_LIMIT, &path);
    let api = registry();
    let quiet = Progress::silent();

    let loaded = candidate::load(&path).expect("a candidate");
    let resolved = candidate::resolve(&loaded, &Palette::new(&api), &quiet).expect("a resolve");
    assert_eq!(
        resolved
            .raw
            .profile
            .pointer("/spec/published/packs")
            .and_then(|p| p.as_array())
            .map(Vec::len),
        Some(PALETTE_LIMIT)
    );

    let mut group = c.benchmark_group("resolve");
    group.sample_size(10);
    group.bench_function("load/200_layers", |b| {
        b.iter(|| candidate::load(black_box(&path)).expect("a candidate"));
    });
    group.bench_function("resolve/200_layers", |b| {
        b.iter(|| {
            candidate::resolve(black_box(&loaded), &Palette::new(&api), &quiet).expect("a resolve")
        });
    });
    group.finish();
}

criterion_group!(benches, resolve);
criterion_main!(benches);
