//! The fuzz targets and the property tests of strata, strata-preflight, and
//! strata-report-endpoint.
//!
//! Each target is a test that calls `bolero::check!`. Under `cargo test`, on stable Rust, a target
//! runs on each input of its `crashes` and `corpus` directories. Then it runs on random bytes for
//! one second. Under `cargo bolero`, on nightly Rust, the same test runs under libFuzzer, which
//! grows the corpus from the coverage of the code. `just fuzz-list` names the targets, and
//! `just fuzz` runs one: see the section "The fuzzing" of the book, in
//! `docs/src/development/build-test.md`.
//!
//! A target calls the public API of the packages, as a program that uses them does. The
//! directory of each target is `src/__fuzz__/<module>__<test>/`, which is where bolero looks.
//!
//! - [`archive`] writes the download of a profile version, as Palette writes it. It is the file
//!   that the tests and the stand-in use, so no second writer of the layout exists.
//! - [`answer`] checks an answer and a ref. It is the file that `tests/fuzz.rs` uses, so no second
//!   check of an answer exists.
//! - [`routes`] names the routes of Palette, and builds the documents of a profile. It is the file
//!   that the tests and the benchmarks use, so no second list of the routes exists.
#![cfg(any(test, doc))]

mod behind;
mod candidate;
mod canon;
mod config;
mod download;
mod mask;
mod palette;
mod preflight;
mod properties;
mod record;
mod reference;
mod report;
mod seed;
mod support;
mod unknown;

#[path = "../../tests/common/download.rs"]
pub mod archive;

#[path = "../../tests/common/answer.rs"]
pub mod answer;

#[path = "../../tests/common/palette.rs"]
pub mod routes;
