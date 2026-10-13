//! Strata records, compares, and verifies what Spectro Cloud Palette holds: a cluster profile
//! version as an immutable, layered release, the copy that a cluster runs, and a pack version in
//! a registry.
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
//! - [`reference`](mod@reference): what a user-supplied ref means: a snapshot file,
//!   `name@version`, a uid, or `cluster:<uid>`.
//! - [`reference::version`]: natural ordering of version strings, behind `name@latest`.
//! - [`candidate`]: a profile version described by a file, before it exists in Palette.
//! - [`snapshot`]: the `strata-snapshot/1` document and how it is built from raw API responses.
//! - [`diff`]: the `strata-diff/1` document, the leaf diff, and the impact rules.
//! - [`validation`]: the answers of Palette when strata asks whether it takes the packs.
//! - [`repave`]: the reasons of Palette for a repave, and a repave that waits for approval.
//! - [`render`]: the terminal, markdown, and JSON renderers behind one trait.
//! - [`ops`]: the operations the CLI exposes, free of argument parsing and exit codes.
//! - [`error`], [`term::progress`], [`term::color`], [`content::layer`], and [`scope`]: the error
//!   type with its exit codes and machine-readable form, progress notes on stderr, the color
//!   policy, Palette's layers, and the scope of a profile version.
//!
//! # Example
//!
//! Record a published profile version, record it again later, and compare. [`api::MemoryApi`]
//! stands in for Palette here; a program that talks to Palette uses [`api::HttpApi`] with a
//! [`config::Config`] instead, usually wrapped in [`api::Retry`].
//!
//! ```
//! use serde_json::json;
//! use strata::api::{MemoryApi, Palette};
//! use strata::term::progress::Progress;
//! use strata::reference::Ref;
//! use strata::ops;
//!
//! // One published profile with one pack, in the shapes Palette returns.
//! let uid = "64f0000000000000000000a1";
//! let api = MemoryApi::new()
//!     .with(
//!         format!("/v1/clusterprofiles/{uid}"),
//!         json!({
//!             "metadata": {"uid": uid, "name": "demo"},
//!             "spec": {"version": "1.0.0", "published": {"cloudType": "aws", "type": "add-on", "packs": [{
//!                 "name": "cert-manager", "layer": "addon", "type": "spectro",
//!                 "tag": "1.21.1", "version": "1.21.1", "packUid": "p1", "registryUid": "r1",
//!                 "digest": "sha256:0123", "values": "installCRDs: true",
//!             }]}},
//!         }),
//!     )
//!     .with(format!("/v1/clusterprofiles/{uid}/packs/manifests"), json!({"spec": {"packs": []}}))
//!     .with(format!("/v1/clusterprofiles/{uid}/variables"), json!({"variables": []}));
//!
//! let reference = Ref::Uid(uid.to_string());
//! let quiet = Progress::silent();
//! let content = ops::SnapshotOptions::default();
//! let record = ops::snapshot_for(&reference, &content, || Ok(Palette::new(api.clone())), "2026-09-16T00:00:00Z", &quiet)?;
//! let later = ops::snapshot_for(&reference, &content, || Ok(Palette::new(api.clone())), "2026-09-17T00:00:00Z", &quiet)?;
//!
//! let diff = ops::compare(&record, &later, "2026-09-17T00:00:00Z");
//! assert!(diff.same, "nothing changed, so the roots are equal");
//! assert_eq!(record.packs[0].key, "addon/cert-manager");
//! # Ok::<(), strata::Error>(())
//! ```

pub mod api;
pub mod candidate;
pub mod cloud;
pub mod cluster_template;
pub mod config;
pub mod conjunction;
pub mod content;
pub mod diff;
pub mod error;
pub(crate) mod json;
pub(crate) mod legend;
pub mod ops;
pub mod pack_type;
pub mod profile_type;
pub mod reference;
pub(crate) mod registry;
pub mod render;
pub mod repave;
pub mod scope;
pub mod snapshot;
pub mod term;
pub mod text;
pub mod validation;

/// The old path of [`content::layer`]. It goes in release 0.23.
pub use content::layer;
/// The old path of [`diff::impact`](mod@diff::impact). It goes in release 0.23.
pub use diff::impact;
/// The old path of [`reference::version`]. It goes in release 0.23.
pub use reference::version;
/// The old path of [`term::color`]. It goes in release 0.23.
pub use term::color;
/// The old path of [`term::progress`]. It goes in release 0.23.
pub use term::progress;

pub use error::{Error, Result};

/// The tool's name, used in messages and recorded in every document.
pub const TOOL_NAME: &str = "strata";
/// The crate version, recorded in every document.
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");
