//! Uses the strata-preflight library as another Rust program does: no command line, no
//! network, no key. Loads the catalog, reads the entity of a create from a fixture, runs one
//! static rule over it, and prints the verdict. Exits 1 if the library does not report that the
//! rule passes.

use std::path::Path;

use strata::api::MemoryApi;
use strata::term::color::Paint;
use strata::term::progress::Progress;
use strata_preflight::catalog;
use strata_preflight::probe::{LiveBackend, Verdict};
use strata_preflight::rule::Tier;
use strata_preflight::run::{self, RunInputs, Selection};
use strata_preflight::world::World;

/// The rule that this program runs. It reads the entity alone, so it needs no record and no key.
const RULE: &str = "aws-local-zone-letter";

/// The time of the report. A fixed time keeps the output the same on each run.
const AT: &str = "2026-09-16T00:00:00Z";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/strata-preflight/rules/fixtures/aws/clean.json");

    // ANCHOR: run
    // The rules, and the runs that prove them, are data that the library carries.
    let catalog = catalog::builtin()?;
    let ledger = catalog::builtin_ledger()?;
    // The entity of a create, read from the fixture. The second argument names the cloud.
    let world = World::from_file(&fixture, Some("aws"))?;

    let selection = Selection {
        tiers: vec![Tier::Static],
        ids: vec![RULE.to_string()],
        ..Selection::default()
    };
    selection.require_known(&catalog)?;

    // There is no Palette client, so a rule of another tier is NOT RUN. A static rule needs none.
    let backend: LiveBackend<MemoryApi> = LiveBackend::new(None, true);
    let report = run::run(RunInputs {
        catalog: &catalog,
        world: &world,
        backend: &backend,
        ledger: &ledger,
        selection: &selection,
        now: AT,
        out_dir: None,
        out_world: false,
        progress: &Progress::silent(),
        verbose: false,
        paint: Paint::off(),
    })?;

    for check in &report.checks {
        println!("{} {}: {}", check.verdict.name(), check.id, check.title);
    }
    // ANCHOR_END: run

    let passed =
        matches!(report.checks.as_slice(), [c] if c.id == RULE && c.verdict == Verdict::Passed);
    if !passed || report.exit_code() != 0 {
        eprintln!("unexpected result from the strata-preflight library");
        std::process::exit(1);
    }
    eprintln!("ok: a clean entity passes the static rule, and the exit code is 0");
    Ok(())
}
