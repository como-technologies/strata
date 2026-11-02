//! The stand-in side of the contract run: the documents that the stand-in serves for a fact.
//!
//! Each chapter of the book builds a tenant. A harvest keeps the documents of each tenant at the
//! end of its chapter, and this module reads them. A fact names the routes of the stand-in that
//! imitate its request. The documents of those routes, from every chapter, are what the book
//! assumes about Palette.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::book::{Mode, Runner};
use crate::chapters;
use crate::facts::route_matches;
use crate::files::files_under;
use crate::palette::{is_refusal, route_of};

/// The documents of the stand-in, by route.
#[derive(Debug, Default)]
pub struct StandIn {
    documents: BTreeMap<String, Vec<Value>>,
}

impl StandIn {
    /// Read the tenants under `root`: one directory for each chapter, with the layout of a tenant.
    /// A file that is no JSON, and the file of a refusal, are omitted.
    pub fn read(root: &Path) -> StandIn {
        let mut stand_in = StandIn::default();
        let chapters = std::fs::read_dir(root).into_iter().flatten().flatten();
        for chapter in chapters.filter(|c| c.path().is_dir()) {
            stand_in.add_tenant(&chapter.path());
        }
        stand_in
    }

    /// Add the documents of the tenant in the directory `tenant`.
    pub fn add_tenant(&mut self, tenant: &Path) {
        for file in files_under(tenant, "json") {
            let Some(relative) = file.strip_prefix(tenant).ok().and_then(Path::to_str) else {
                continue;
            };
            if is_refusal(relative) {
                continue;
            }
            // a file that no request reads, or that cannot be read, is no document of the stand-in
            let (Some(route), Ok(text)) = (route_of(relative), std::fs::read_to_string(&file))
            else {
                continue;
            };
            // a file that holds no JSON is not a document that Palette could answer with
            if let Ok(document) = serde_json::from_str(&text) {
                self.documents.entry(route).or_default().push(document);
            }
        }
    }

    /// The documents of each route that matches one of the `patterns`, which are the `stand_in`
    /// of a fact: paths with `{name}` for a segment that varies.
    pub fn documents(&self, patterns: &[String]) -> Vec<&Value> {
        self.documents
            .iter()
            .filter(|(route, _)| patterns.iter().any(|p| route_matches(p, route)))
            .flat_map(|(_, documents)| documents)
            .collect()
    }
}

/// Build the tenant of every chapter of the book, and keep the documents of each one in a
/// directory of `into`. It runs no command of the book. The examples must be built: `just
/// examples`. `xtask` is the binary that serves a tenant.
///
/// # Errors
///
/// The error of the stand-in server that the runner starts.
pub fn harvest(repo: &Path, xtask: &Path, into: &Path) -> std::io::Result<()> {
    let mode = Mode::Harvest {
        into: into.to_path_buf(),
    };
    // no command runs, so the runner never calls this strata
    let mut runner = Runner::start(repo, Path::new("strata"), xtask, mode)?;
    chapters::run(&mut runner);
    // the code of a harvest is always success: it has no page to check
    let _ = runner.finish();
    Ok(())
}
