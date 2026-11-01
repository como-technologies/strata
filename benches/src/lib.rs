//! The input of the benchmarks of strata.
//!
//! A benchmark needs a profile, a comparison, or a fleet of the size that Palette permits. A
//! profile holds up to 200 packs and 200 manifests. A query of a fleet can read many clusters.
//! This crate makes that input with a generator. It reads the real pack data that is tracked in
//! `examples/packs/`, and it tracks no large file of its own.
//!
//! - [`real`] reads the real packs: their values, digests, and uids.
//! - [`profile`] makes the responses of Palette for a profile of any size, and the record of it.
//! - [`fleet`] makes a fleet of clusters in an in-memory Palette.
//! - [`candidate`] makes a candidate file of any size, and the registry that serves its packs.
//! - [`routes`] names the routes of Palette, and builds the documents of a profile. It is the file
//!   that the tests use, so no second list of the routes or of the documents exists.

pub mod candidate;
pub mod fleet;
pub mod profile;
pub mod real;

#[path = "../../tests/common/palette.rs"]
pub mod routes;

/// The most packs, and the most manifests, that Palette takes in one profile.
pub const PALETTE_LIMIT: usize = 200;

/// The time that the generated records state. A fixed time keeps the input the same on each run.
pub const FETCHED_AT: &str = "2026-01-01T00:00:00Z";

/// The time of a comparison, a check of a fleet, or a report.
pub const GENERATED_AT: &str = "2026-01-02T00:00:00Z";

#[cfg(test)]
mod tests;
