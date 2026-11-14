//! The types of Palette: what Palette names, answers, and decides. A type that two modules
//! share, and that is a fact of Palette, goes here.
//!
//! - [`cloud`]: the cloud types of Palette, and the rule for a pack that supports a cloud.
//! - [`cluster_state`]: the state of a cluster, as Palette gives it in `status.state`.
//! - [`cluster_template`]: the cluster templates of Palette.
//! - [`conjunction`]: how the conditions of a search of Palette join.
//! - [`layer`]: the layers of Palette.
//! - [`oci_provider`]: the provider type of an OCI registry.
//! - [`pack_state`] and [`pack_type`]: the state of a pack version in a registry, and the type
//!   of a pack.
//! - [`profile_type`]: the type of a profile.
//! - [`repave`]: the reasons of Palette for a repave, and a repave that waits for approval.
//! - [`scope`]: the scope of a profile version.
//! - [`update`]: the rules of Palette for the update of a cluster that no validate route checks.
//! - [`validation`]: the answers of Palette when strata asks whether it takes the packs.

pub mod cloud;
pub mod cluster_state;
pub mod cluster_template;
pub mod conjunction;
pub(crate) mod input_type;
pub mod layer;
pub mod oci_provider;
pub mod pack_state;
pub mod pack_type;
pub mod profile_type;
pub mod repave;
pub mod scope;
pub mod update;
pub mod validation;
