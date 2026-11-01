//! The content that the hashes read: the canonical form, and the layers of Palette.

pub(crate) mod canon;
pub(crate) mod hash;
pub mod layer;

// The fuzz package writes this text and reads it again as YAML.
pub use canon::canonical_json;
