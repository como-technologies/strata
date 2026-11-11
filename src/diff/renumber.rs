//! A renumber: a pack whose version alone changed, with the same content on the two sides.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::reference::version;
use crate::snapshot::Pack;

/// One pack whose version alone changed: a renumber.
///
/// The name, the layer, the type, the registry, the values, the manifests, the params, and the
/// images stay on the two sides. The tag and the `packUid` follow the version, and the digest of
/// the archive changes with the build. Strata reads no suffix of a version, so a renumber has no
/// rule of names. `same-release --allow renumber` accepts each one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Renumber {
    /// `<layer>/<name>` of the pack.
    pub key: String,
    /// The pack name.
    pub name: String,
    /// The version of side A.
    pub old: String,
    /// The version of side B.
    pub new: String,
    /// True when `new` is the final version of the pre-release `old`, in Semantic Versioning. The
    /// two have the same numbers, and only `old` has a pre-release. It states a fact, and it is no
    /// condition of the renumber.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pre_release_to_final: bool,
}

impl Renumber {
    /// The renumber of one pack, from `x` on side A to `y` on side B.
    pub(crate) fn of(x: &Pack, y: &Pack) -> Renumber {
        Renumber {
            key: x.key.clone(),
            name: x.name.clone(),
            old: x.version.clone(),
            new: y.version.clone(),
            pre_release_to_final: version::is_final_of(&y.version, &x.version),
        }
    }
}

/// True when the two sides of one pack differ only by their numbers: a renumber, see
/// [`Renumber`].
///
/// The version differs. The name, the layer, the type, and the registry stay. The authored hash
/// stays, so the values, the manifests, and the params are identical. The images that the values
/// declare and the machine image in `imageId` stay too. The tag, the `packUid`, and the digest of
/// the archive can differ. `same-release --allow renumber` reads this one rule.
pub(crate) fn renumbered(x: &Pack, y: &Pack) -> bool {
    x.version != y.version
        && x.hash.authored == y.hash.authored
        && x.images == y.images
        && x.image_id() == y.image_id()
        && (&x.name, &x.layer, &x.kind, &x.registry_uid)
            == (&y.name, &y.layer, &y.kind, &y.registry_uid)
}

/// True when the impact of `compare` is `none (renumber only)`: a renumber whose version keeps its
/// numbers, see [`version::keeps_numbers`].
///
/// Strata does not compare the archives. The values do not always name the content: `ubuntu-aws`
/// `20.04` and `24.04` have the same values and are two releases of the OS. So a renumber to other
/// numbers keeps the impact of its layer. See [`crate::diff::PackDiff::renumbered`].
pub(crate) fn renumber_only(x: &Pack, y: &Pack) -> bool {
    renumbered(x, y) && version::keeps_numbers(&x.version, &y.version)
}
