//! Strata records, compares, and verifies Spectro Cloud Palette cluster profile versions as
//! immutable, layered releases.
//!
//! A profile is a stack of layers (packs). Every layer gets three hashes so a report can name the
//! kind of change: identity (which pack build the layer references), upstream (the digest Palette
//! stamps on the pack ref), and authored (values, manifests, params). A snapshot is the record of
//! one published version; a diff reads two records; verify checks a record against the live profile.
//!
//! The crate is a library with a thin CLI on top. The pieces:
//!
//! - [`config`] and [`api`]: how to reach Palette. [`api::PaletteApi`] is the trait; [`api::HttpApi`]
//!   talks HTTP and [`api::MemoryApi`] serves canned responses for tests.
//! - [`reference`]: what a user-supplied ref means (snapshot file, `name@version`, uid).
//! - [`version`]: natural ordering of version strings, behind `name@latest`.
//! - [`candidate`]: a profile version described by a file, before it exists in Palette.
//! - [`canon`]: canonical form and hashing.
//! - [`snapshot`]: the `strata-snapshot/1` document and how it is built from raw API responses.
//! - [`diff`]: the `strata-diff/1` document, the leaf diff, and the impact rules.
//! - [`render`]: the terminal, markdown, and JSON renderers behind one trait.
//! - [`ops`]: the operations the CLI exposes, free of argument parsing and exit codes.

pub mod api;
pub mod candidate;
pub mod canon;
pub mod color;
pub mod config;
pub mod diff;
pub mod error;
pub mod layer;
pub mod ops;
pub mod progress;
pub mod reference;
pub mod render;
pub mod snapshot;
pub mod version;

pub use error::{Error, Result};

/// The tool's name, used in messages and recorded in every document.
pub const TOOL_NAME: &str = "strata";
/// The crate version, recorded in every document.
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Which implementation wrote a document.
pub const TOOL_IMPL: &str = "rust";
