//! Strata records, compares, and verifies what Spectro Cloud Palette holds. It reads a cluster
//! profile version, an immutable and layered release, the copy that a cluster runs, and a pack
//! version in a registry.
//!
//! A profile is a stack of layers (packs). Every layer gets three hashes, so a report can name the
//! kind of change:
//!
//! - identity: which pack build the layer references.
//! - upstream: the digest that Palette stamps on the pack ref.
//! - authored: the values, the manifests, and the params.
//!
//! A snapshot is the record of one published version. A diff reads two records. `verify` checks a
//! record against the live profile.
//!
//! The crate is a library with a thin CLI on top. The pieces:
//!
//! - [`config`] and [`api`]: how to reach Palette. [`api::PaletteApi`] is the trait; [`api::HttpApi`]
//!   talks HTTP and [`api::MemoryApi`] serves canned responses for tests.
//! - [`reference`](mod@reference): what a user-supplied ref means: a snapshot file,
//!   `name@version`, a uid, or `cluster:<uid>`.
//! - [`reference::version`]: the order of version strings, by Semantic Versioning, behind `name@latest`.
//! - [`model`]: the types of Palette: the cloud types, the layers, the answers to the validate
//!   routes, the reasons for a repave, and the closed sets that Palette names.
//! - [`candidate`]: a profile version described by a file, before it exists in Palette.
//! - [`snapshot`]: the `strata-snapshot/1` document and how it is built from raw API responses.
//! - [`closed_set`]: a value of a closed set in a document that strata writes and reads, also
//!   when a later release adds the value.
//! - [`diff`]: the `strata-diff/1` document, the leaf diff, and the impact rules.
//! - [`text`]: the texts that a message builds.
//! - [`json`]: the reads of a document of Palette, such as the text at a pointer.
//!   Each read gives a fixed value for a field that is not there.
//! - [`render`]: the terminal, markdown, and JSON renderers behind one trait.
//! - [`ops`]: the operations the CLI exposes, free of argument parsing and exit codes.
//! - [`error`]: the error type, with its exit codes and its machine-readable form.
//! - [`term::progress`] and [`term::color`]: the progress notes on stderr, and the color policy.
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
pub mod closed_set;
pub mod config;
pub mod content;
pub mod diff;
pub mod error;
pub(crate) mod file;
pub mod json;
pub mod model;
pub mod ops;
pub mod reference;
pub mod render;
pub mod snapshot;
pub mod term;
pub mod text;
pub(crate) mod timestamp;

/// The old path of [`model::cloud`]. It goes in release 0.27.
pub use model::cloud;
/// The old path of [`model::cluster_template`]. It goes in release 0.27.
pub use model::cluster_template;
/// The old path of [`model::conjunction`]. It goes in release 0.27.
pub use model::conjunction;
/// The old path of [`model::oci_provider`]. It goes in release 0.27.
pub use model::oci_provider;
/// The old path of [`model::pack_state`]. It goes in release 0.27.
pub use model::pack_state;
/// The old path of [`model::pack_type`]. It goes in release 0.27.
pub use model::pack_type;
/// The old path of [`model::profile_type`]. It goes in release 0.27.
pub use model::profile_type;
/// The old path of [`model::repave`]. It goes in release 0.27.
pub use model::repave;
/// The old path of [`model::scope`]. It goes in release 0.27.
pub use model::scope;
/// The old path of [`model::validation`]. It goes in release 0.27.
pub use model::validation;

pub use error::{Error, Result};

/// The tool's name, used in messages and recorded in every document.
pub const TOOL_NAME: &str = "strata";
/// The crate version, recorded in every document.
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");
