//! `behind`: the documents of the profile versions of one scope, each read one time.
//!
//! Three parts of `behind` read the document of a profile version: the newer versions of a
//! profile, the move to the newest version, and the names of the clusters in a refusal of Palette.
//! They read the same versions, so the scope keeps each document that it read.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use serde_json::Value;

use crate::api::{Palette, PaletteApi};
use crate::error::Result;

/// The documents of the profile versions that `behind` read in one scope, by uid.
pub(super) struct ProfileDocuments<'a, A: PaletteApi> {
    pub(super) palette: &'a Palette<A>,
    read: BTreeMap<String, Value>,
}

impl<'a, A: PaletteApi> ProfileDocuments<'a, A> {
    pub(super) fn new(palette: &'a Palette<A>) -> Self {
        ProfileDocuments {
            palette,
            read: BTreeMap::new(),
        }
    }

    /// The document of the profile version `uid`, `GET /v1/clusterprofiles/{uid}`, read one time.
    ///
    /// A read that fails is not kept, so the next call reads again.
    pub(super) fn get(&mut self, uid: &str) -> Result<&Value> {
        let doc = match self.read.entry(uid.to_string()) {
            Entry::Occupied(known) => known.into_mut(),
            Entry::Vacant(new) => new.insert(self.palette.profile_document(uid)?),
        };
        Ok(doc)
    }
}
