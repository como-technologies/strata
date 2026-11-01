//! `strata-preflight`: the jq engine over the full catalog, for the entity of one create.

use std::hint::black_box;
use std::path::Path;

use criterion::{Criterion, criterion_group, criterion_main};
use strata::api::MemoryApi;
use strata::term::color::Paint;
use strata::term::progress::Progress;
use strata_preflight::catalog;
use strata_preflight::confidence::Ledger;
use strata_preflight::probe::{LiveBackend, Verdict};
use strata_preflight::run::{self, RunInputs, Selection};
use strata_preflight::world::World;

const AT: &str = "2026-01-02T00:00:00Z";

fn preflight(c: &mut Criterion) {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../preflight/rules/fixtures/aws/clean.json");
    let catalog = catalog::builtin().expect("the catalog");
    let ledger =
        Ledger::parse(catalog::LEDGER, Path::new("evidence/runs.yaml")).expect("the ledger");
    let world = World::from_file(&fixture, Some("aws")).expect("an entity");
    let selection = Selection::default();
    // no Palette client and no command: a rule of another tier is not run, and the jq engine runs
    // the `when` of each rule of the catalog and the check of each static rule
    let backend: LiveBackend<MemoryApi> = LiveBackend::new(None, true);
    let quiet = Progress::silent();
    let run = || {
        run::run(RunInputs {
            catalog: &catalog,
            world: &world,
            backend: &backend,
            ledger: &ledger,
            selection: &selection,
            now: AT,
            out_dir: None,
            progress: &quiet,
            verbose: false,
            paint: Paint::off(),
        })
        .expect("a report")
    };
    let report = run();
    assert!(report.checks.len() > 100, "{} checks", report.checks.len());
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.verdict == Verdict::Passed)
    );

    let mut group = c.benchmark_group("preflight");
    group.sample_size(10);
    group.bench_function("full_catalog", |b| b.iter(|| black_box(run())));
    group.finish();
}

criterion_group!(benches, preflight);
criterion_main!(benches);
