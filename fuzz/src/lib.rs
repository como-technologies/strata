//! The fuzz targets and the property tests of strata, strata-preflight, and
//! strata-report-endpoint.
//!
//! Each target is a test that calls `bolero::check!`. Under `cargo test`, on stable Rust, a target
//! runs on each input of its `crashes` and `corpus` directories, and then on random bytes for one
//! second. Under `cargo bolero`, on nightly Rust, the same test runs under libFuzzer, which grows
//! the corpus from the coverage of the code. `just fuzz-list` names the targets, and `just fuzz`
//! runs one: see the section "The fuzzing" of the book, in `docs/src/development/build-test.md`.
//!
//! A target calls the public API of the packages, as a program that uses them does. The
//! directory of each target is `src/__fuzz__/<module>__<test>/`, which is where bolero looks.
#![cfg(any(test, doc))]

mod candidate;
mod canon;
mod mask;
mod palette;
mod preflight;
mod properties;
mod record;
mod reference;
mod report;
mod seed;
mod support;
