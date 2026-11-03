//! The versions of a pack that one registry serves: the one list that `packs` marks and that
//! `pack:<name>@latest` picks from.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::api::registry::retired_state;
use crate::api::{Palette, PaletteApi};
use crate::closed_set::ClosedSet;
use crate::error::{Result, found};
use crate::json::{arr, text};
use crate::model::pack_state::PackState;

/// The value of the `states` query that asks Palette for the tags of the retired versions of a
/// pack: each state that a listing gives. With it, Palette answers with the versions in these
/// states only.
fn listed_states() -> String {
    let listed = PackState::ALL.iter().filter(|s| s.listed());
    listed.map(|s| s.as_str()).collect::<Vec<_>>().join(",")
}

/// What one registry serves of one pack, from its entries in the pack listing and its own answer.
///
/// Palette applies the FIPS rule of the tenant to the answer of the registry, and not to the
/// listing. So a version with no tag in the answer is a version that Palette does not serve, and a
/// profile that pins it is refused with `PackForbidden`. A version that Spectro Cloud retired as
/// disabled or deleted has no tag in the plain answer either, and stays in the list: its state
/// says why Palette refuses it.
pub(crate) struct Served<'a> {
    /// The entries of the listing that the registry serves, in the order of the listing.
    pub(crate) entries: Vec<&'a Value>,
    /// The answers of the registry. None when it answered 404 to each request.
    pub(crate) answers: Vec<Value>,
    /// The tag rows of `answers`, each one once.
    tags: Vec<Value>,
}

impl<'a> Served<'a> {
    /// Read what the registry `uid` serves of the pack `name`. `listed` holds the entries of the
    /// pack listing that name this registry.
    ///
    /// The registry's own answer holds the tags, which the listing does not. With no query, the
    /// answer does not include the disabled and the deleted versions, and Palette answers 404 when
    /// it has no version. So a registry with a retired version in `listed` gets a second request
    /// with the states query. A registry that answers 404 to both serves only the disabled and the
    /// deleted versions of the listing, with no tags.
    ///
    /// # Errors
    ///
    /// Each error of a request, except HTTP 404.
    pub(crate) fn read<A: PaletteApi>(
        palette: &Palette<A>,
        name: &str,
        uid: &str,
        listed: Vec<&'a Value>,
    ) -> Result<Self> {
        let mut answers = vec![found(palette.pack_tags(name, uid, None))?];
        if listed.iter().any(|e| retired_state(e).is_some()) {
            answers.push(found(palette.pack_tags(name, uid, Some(&listed_states())))?);
        }
        let answers: Vec<Value> = answers.into_iter().flatten().collect();
        let tags = merged_tags(&answers);
        let entries = listed
            .into_iter()
            .filter(|e| tags.iter().any(|t| points_at(t, e)) || retired_state(e).is_some())
            .collect();
        Ok(Served {
            entries,
            answers,
            tags,
        })
    }

    /// The tag rows of the answers that point at the listing entry `e`.
    pub(crate) fn tag_rows(&self, e: &Value) -> Vec<&Value> {
        self.tags.iter().filter(|t| points_at(t, e)).collect()
    }
}

/// Join the tag rows of the `answers` of one registry, and keep each row one time.
///
/// A deprecated version is in the answer with no query and in the answer with the states query,
/// so its rows come two times.
fn merged_tags(answers: &[Value]) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    answers
        .iter()
        .flat_map(|a| arr(a, "/tags"))
        .filter(|t| seen.insert((text(t, "/packUid"), text(t, "/version"), text(t, "/tag"))))
        .cloned()
        .collect()
}

/// True when the tag row `t` points at the listing entry `e`. A tag row names its version and the
/// build that it points at. Match on either: a registry that answers with one of the two still
/// says enough.
fn points_at(t: &Value, e: &Value) -> bool {
    let v = text(t, "/version");
    if v.is_empty() {
        text(t, "/packUid") == text(e, "/metadata/uid")
    } else {
        v == text(e, "/spec/version")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_states_query_names_each_state_that_a_listing_gives_and_not_absent() {
        assert_eq!(listed_states(), "deprecated,disabled,deleted");
    }
}
