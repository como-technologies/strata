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
//! - [`reference::version`]: the order of version strings, by Semantic Versioning, behind `name@latest`.
//! - [`candidate`]: a profile version described by a file, before it exists in Palette.
//! - [`snapshot`]: the `strata-snapshot/1` document and how it is built from raw API responses.
//! - [`closed_set`]: a value of a closed set in a document that strata writes and reads, also
//!   when a later release adds the value.
//! - [`diff`]: the `strata-diff/1` document, the leaf diff, and the impact rules.
//! - [`validation`]: the answers of Palette when strata asks whether it takes the packs.
//! - [`repave`]: the reasons of Palette for a repave, and a repave that waits for approval.
//! - [`cloud`]: the cloud types of Palette, and the rule for a pack that supports a cloud.
//! - [`cluster_state`]: the state of a cluster, as Palette gives it in `status.state`.
//! - [`cluster_template`]: the cluster templates of Palette.
//! - [`conjunction`]: how the conditions of a search of Palette join.
//! - [`pack_type`] and [`profile_type`]: the type of a pack, and the type of a profile.
//! - [`pack_state`] and [`oci_provider`]: the state of a pack version in a registry, and the
//!   provider type of an OCI registry.
//! - [`text`]: the texts that a message builds.
//! - [`json`]: the reads of a document of Palette, such as the text at a pointer.
//!   Each read gives a fixed value for a field that is not there.
//! - [`render`]: the terminal, markdown, and JSON renderers behind one trait.
//! - [`ops`]: the operations the CLI exposes, free of argument parsing and exit codes.
//! - [`error`]: the error type, with its exit codes and its machine-readable form.
//! - [`term::progress`] and [`term::color`]: the progress notes on stderr, and the color policy.
//! - [`content::layer`]: the layers of Palette.
//! - [`scope`]: the scope of a profile version.
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
pub mod cloud;
pub mod cluster_state;
pub mod cluster_template;
pub mod config;
pub mod conjunction;
pub mod content;
pub mod diff;
pub mod error;
pub(crate) mod file;
pub mod json;
pub mod oci_provider;
pub mod ops;
pub mod pack_state;
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
pub(crate) mod timestamp;
pub mod validation;

pub use error::{Error, Result};

/// The tool's name, used in messages and recorded in every document.
pub const TOOL_NAME: &str = "strata";
/// The crate version, recorded in every document.
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");
