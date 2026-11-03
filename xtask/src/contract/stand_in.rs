//! The stand-in side of the contract run: the documents that the stand-in serves for a fact.
//!
//! Each chapter of the book builds a tenant. A harvest keeps the documents of each tenant at the
//! end of its chapter, and this module reads them. A fact names the routes of the stand-in that
//! imitate its request. The documents of those routes, from every chapter, are what the book
//! assumes about Palette.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use super::shape::{JsonType, Path as Field};
use crate::book::{Mode, Runner};
use crate::chapters;
use crate::facts::{Fact, Kind, route_matches};
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
    ///
    /// # Errors
    ///
    /// The text of the error of the read when `root` or an entry of it cannot be read. A text that
    /// says the stand-in holds no document when the tenants hold none. A run with no document
    /// checks no fact, so it must not read as a run that found no difference.
    pub fn read(root: &Path) -> Result<StandIn, String> {
        let unreadable = |e: std::io::Error| format!("{}: {e}", root.display());
        let mut stand_in = StandIn::default();
        for entry in std::fs::read_dir(root).map_err(unreadable)? {
            let chapter = entry.map_err(unreadable)?.path();
            if chapter.is_dir() {
                stand_in.add_tenant(&chapter);
            }
        }
        if stand_in.documents.is_empty() {
            return Err(format!(
                "{}: the stand-in holds no document; give a directory with one directory for each chapter",
                root.display()
            ));
        }
        Ok(stand_in)
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

    /// The documents of each route that matches one of the `patterns`. The `patterns` are the
    /// `stand_in` of a fact: paths with `{name}` for a segment that varies.
    pub fn documents(&self, patterns: &[String]) -> Vec<&Value> {
        self.documents
            .iter()
            .filter(|(route, _)| patterns.iter().any(|p| route_matches(p, route)))
            .flat_map(|(_, documents)| documents)
            .collect()
    }

    /// One message for each field that a GET fact of the kind `request` reads and that no document of
    /// its stand-in holds. A document holds a field when it has a value there that is not `null`.
    /// A fact whose route the stand-in does not serve has no message: the check of the routes
    /// reports it.
    pub fn unheld_fields(&self, facts: &[Fact]) -> Vec<String> {
        let mut bad = Vec::new();
        for fact in facts
            .iter()
            .filter(|f| f.kind == Kind::Request && f.get_path().is_some())
        {
            let documents = self.documents(&fact.stand_in);
            if documents.is_empty() {
                continue;
            }
            for field in &fact.reads.fields {
                let path = Field::parse(field);
                let held = documents
                    .iter()
                    .any(|d| path.types(d).iter().any(|t| *t != JsonType::Absent));
                if !held {
                    bad.push(format!(
                        "fact `{}`: no document of the stand-in holds the field `{field}`",
                        fact.id
                    ));
                }
            }
        }
        bad
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
