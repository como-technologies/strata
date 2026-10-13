//! `strata snapshot`: record an object in a snapshot file, or each cluster of a set in a
//! directory.

use std::process::ExitCode;

use strata::error::{Error, Result};
use strata::ops;
use strata::reference::Ref;
use strata::render;
use strata::term::color::Paint;

use super::args::SnapshotArgs;
use super::{connect, now_utc, progress};

pub(crate) fn cmd_snapshot(a: &SnapshotArgs) -> Result<ExitCode> {
    let progress = progress(a.quiet);
    let now = now_utc();
    if a.validate && (a.from_raw.is_some() || a.dir.is_some()) {
        return Err(Error::Usage {
            message: "--validate asks Palette about the packs of one profile version or \
                      candidate. --from-raw sends no request, and --dir records clusters"
                .to_string(),
        });
    }
    // a set of clusters is one record for each, in a directory
    let reference = a.r#ref.as_deref().map(Ref::parse);
    match (&reference, &a.dir) {
        (Some(Ref::ClusterSet { tags, filter }), Some(dir)) => {
            let query = ops::ClusterQuery {
                tags: tags.clone(),
                filter: filter.clone(),
            };
            let report = ops::snapshot_set(
                &query,
                dir,
                a.prune,
                |scope| connect(progress, scope),
                &now,
                &progress,
            )?;
            // the lines of a set have no color, so the report takes no --color
            let renderer = render::renderer(a.format(), Paint::off(), a.compact);
            print!("{}", renderer.render_set(&report)?);
            progress.note(&render::set_counts_line(&report));
            return Ok(ExitCode::from(report.exit_code()));
        }
        (_, Some(_)) => {
            return Err(Error::Usage {
                message: "--dir takes a tag: or filter: ref, which names a set of clusters"
                    .to_string(),
            });
        }
        _ => {}
    }
    let snap = match (&a.from_candidate, &a.from_raw) {
        (Some(file), _) => ops::snapshot_from_candidate(
            file,
            a.validate,
            || connect(progress, None),
            &now,
            &progress,
        )?,
        (None, Some(dir)) => {
            let reference = a.r#ref.clone().unwrap_or_else(|| dir.display().to_string());
            ops::snapshot_from_dir(dir, &reference, &now)?
        }
        (None, None) => {
            let reference = reference.ok_or_else(|| Error::Usage {
                message: "snapshot needs a profile ref or --from-raw DIR".to_string(),
            })?;
            // `snapshot` writes a record, and reads none: a file-shaped ref reaches Palette
            // nowhere else, and a path sent as a uid asks for /v1/clusterprofiles/<path>, which
            // 404s after the retries
            if let Ref::Snapshot(path) = &reference {
                return Err(Error::RecordNotASource {
                    path: path.display().to_string(),
                });
            }
            ops::snapshot_for(
                &reference,
                &a.options(),
                || connect(progress, None),
                &now,
                &progress,
            )?
        }
    };
    // the answer of Palette is what --validate asks for, so -q does not hide it; it goes to
    // standard error, beside the record on standard output
    if let Some(v) = &snap.source.palette_validation {
        for line in strata::validation::answer_lines(v.errors()) {
            eprintln!("palette    {line}");
        }
    }
    ops::require_acceptance(&snap)?;
    for warning in render::invalid_pack_warnings(&snap) {
        progress.warn(&warning);
    }
    let text = snap.to_json(a.compact)?;
    match &a.output {
        Some(path) => {
            ops::write_file(path, &text)?;
            progress.note(&format!(
                "wrote {} (root {})",
                path.display(),
                &snap.root[..12]
            ));
        }
        None => print!("{text}"),
    }
    Ok(ExitCode::SUCCESS)
}
