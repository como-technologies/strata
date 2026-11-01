//! The content that the hashes read: the canonical form, the digest, and the root.

pub(crate) mod canon;
pub(crate) mod hash;

// The fuzz package writes this text and reads it again as YAML.
pub use canon::canonical_json;

/// The old path of [`model::layer`](crate::model::layer). It goes in release 0.27.
pub use crate::model::layer;
