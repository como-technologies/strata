//! The tests of the crate, in one crate: each file of `tests/` is a module of it.
//!
//! One crate gives each helper of [`common`] one set of callers, so a helper with no caller
//! warns as dead code. `cargo test NAME` runs the test `NAME`; `cargo test verify::` runs the
//! tests of `tests/verify.rs`.

mod common;

mod behind;
mod benches;
mod ci_scripts;
mod cli;
mod closed_set;
mod consumers;
mod content;
mod contract;
mod crate_root;
mod dev_scripts;
mod doc_lints;
mod docs;
mod draft;
mod draft_secret;
mod fuzz;
mod live_scripts;
mod masked;
mod observer_deploy;
mod observer_up;
mod presets;
mod projects;
mod registry_names;
mod renumber;
mod report;
mod report_deploy;
mod routes;
mod send;
mod spawn;
mod unknown;
mod unsynced;
mod verify;
mod version_order;
mod workflows;
