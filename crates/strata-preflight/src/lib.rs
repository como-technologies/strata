//! strata-preflight answers one question before a cluster build: **can Palette build this
//! cluster?** It reads the entity of the create, the records of its profiles, and the answers of
//! Palette, runs each rule of a catalog, and reports what will stop the build, in seconds and not
//! after an hour of cloud time.
//!
//! The pieces:
//!
//! - [`rule`]: the rule format. A rule is data: a fault, where a check finds it (the tier), how
//!   much to trust the check (the level), and the check as a jq program. The catalog in the binary
//!   and a file of custom probes share the format.
//! - [`catalog`]: the rules the binary carries, one file per section.
//! - [`jq`]: the jq engine that runs the programs of the rules.
//! - [`curl`]: the start of the `curl` to Palette of a `reproduce` line.
//! - [`world`]: the one document each program reads: the cloud, the release, the entity, the
//!   change, the records, and what a check adds (the answer of Palette, the output of a command).
//! - [`probe`]: how each kind of check runs, and what it can answer.
//! - [`run`]: the run: which rules apply, in what order, and the exit code.
//! - [`confidence`]: the counts of the runs, the 95% lower bound of each rate, the ledger, and
//!   the promotion of a probe to a preflight check.
//! - [`fixture`]: a world with the answers written in, for the tests of the catalog.
//! - [`report`]: the report document, and its terminal, markdown, and JSON forms; [`html`] gives
//!   the page.

pub mod catalog;
pub mod confidence;
pub mod curl;
pub mod error;
pub mod fixture;
pub mod html;
pub mod jq;
pub mod probe;
pub mod report;
pub mod rule;
pub mod run;
pub mod world;

pub use error::{Error, Result};

/// The tool's name, in messages and in every document.
pub const TOOL_NAME: &str = "strata-preflight";
/// The crate version, in every document.
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

// The routes that the tests of strata serve, in the file that they use. The unit tests use some of
// them, so the others are dead code here.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../tests/common/palette.rs"]
mod routes;
