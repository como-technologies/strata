//! The tests of the crate, in one crate: each file of `tests/` is a module of it.
//!
//! One crate gives each helper of [`common`] one set of callers, so a helper with no caller
//! warns as dead code. `cargo test NAME` runs the test `NAME`; `cargo test verify::` runs the
//! tests of `tests/verify.rs`.

mod common;

mod benches;
mod ci_scripts;
mod cli;
mod closed_set;
mod content;
mod dev_scripts;
mod docs;
mod draft;
mod draft_secret;
mod fuzz;
mod live_scripts;
mod masked;
mod presets;
mod unsynced;
mod verify;
mod workflows;
