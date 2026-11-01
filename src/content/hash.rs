//! The hashes of a record: the content digest of its packs, and its root.

use serde_json::json;

use crate::content::canon::{canonical_json, sha256_hex};
use crate::snapshot::{CONTENT_DIGEST, Pack};

/// The content digest: what a version is, without the ids that Palette gives. It covers the
/// variables hash and, for each pack in order, the key, the version, the upstream hash, and the
/// authored hash. It omits the tag, the pack uid, the registry uid, and the pack type, so it
/// is equal for two publishes of one definition and for a copy of a pack to another registry.
pub(crate) fn content_of(variables_hash: &str, packs: &[Pack]) -> String {
    sha256_hex(&canonical_json(&json!({
        // The key keeps this digest apart from every other one, and gives the encoding a version.
        "digest": CONTENT_DIGEST,
        "variables": variables_hash,
        "packs": packs
            .iter()
            .map(|p| json!([p.key, p.version, p.hash.upstream, p.hash.authored]))
            .collect::<Vec<_>>(),
    })))
}

/// The root: one sha256 digest over the variables hash and the node hash of each pack, in order.
pub(crate) fn root_of(variables_hash: &str, packs: &[Pack]) -> String {
    sha256_hex(&canonical_json(&json!({
        "variables": variables_hash,
        "packs": packs.iter().map(|p| json!([p.key, p.hash.node])).collect::<Vec<_>>(),
    })))
}
