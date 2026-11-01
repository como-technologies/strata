//! `strata snapshot`: record an object in a snapshot file, or each cluster of a set in a
//! directory.

use std::process::ExitCode;

use strata::error::Result;
use strata::ops::{self, ObjectSource, SnapshotSource};
use strata::render;
use strata::term::color::Paint;
use strata::term::progress::Progress;

use super::args::SnapshotArgs;
use super::{connect, now_utc};

pub(crate) fn cmd_snapshot(a: &SnapshotArgs) -> Result<ExitCode> {
    let progress = a.quiet.progress();
    let now = now_utc();
    let options = a.options();
    match a.request(&options).source()? {
        SnapshotSource::Set { query, dir, prune } => {
            let report = ops::snapshot_set(
                &query,
                dir,
                prune,
                |scope| connect(progress, scope),
                &now,
                &progress,
            )?;
            // the lines of a set have no color, so the report takes no --color
            let renderer = render::renderer(a.format(), Paint::off(), a.compact.compact);
            print!("{}", renderer.render_set(&report)?);
            progress.note(&render::set_counts_line(&report));
            Ok(ExitCode::from(report.exit_code()))
        }
        SnapshotSource::Object(source) => record_object(a, &source, &options, &now, progress),
    }
}

/// Read one record and write it, to the file of `--output` or to standard output.
fn record_object(
    a: &SnapshotArgs,
    source: &ObjectSource<'_>,
    options: &ops::SnapshotOptions,
    now: &str,
    progress: Progress,
) -> Result<ExitCode> {
    let snap = ops::snapshot_object(source, options, || connect(progress, None), now, &progress)?;
    // the answer of Palette is what --validate asks for, so -q does not hide it; it goes to
    // standard error, beside the record on standard output
    if let Some(v) = &snap.source.palette_validation {
        for line in strata::validation::answer_lines(v.errors()) {
            eprintln!("{}", render::palette_line(&line));
        }
    }
    ops::require_acceptance(&snap)?;
    for warning in render::invalid_pack_warnings(&snap) {
        progress.warn(&warning);
    }
    let text = snap.to_json(a.compact.compact)?;
    match &a.file.output {
        Some(path) => {
            ops::write_file(path, &text)?;
            progress.note(&format!(
                "wrote {} (root {})",
                path.display(),
                render::short(&snap.root)
            ));
        }
        None => print!("{text}"),
    }
    Ok(ExitCode::SUCCESS)
}
