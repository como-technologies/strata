//! The reads of the kinds of a listing, and each kind that strata could not read.

use std::collections::BTreeMap;

use serde_json::Value;

use super::{ListKind, ListOptions};
use crate::closed_set::{ClosedSet, Open};
use crate::error::{Error, Result};
use crate::term::progress::Progress;

/// The kinds of a listing that strata could not read, each with its error, in the order of the
/// reads. `options` says which kinds the listing asks for.
pub(super) struct Unread<'a> {
    options: &'a ListOptions,
    progress: &'a Progress,
    kinds: Vec<(ListKind, Error)>,
}

impl<'a> Unread<'a> {
    /// No kind read yet, for a listing of `options`. `progress` takes the note of each read.
    pub(super) fn new(options: &'a ListOptions, progress: &'a Progress) -> Self {
        Unread {
            options,
            progress,
            kinds: Vec::new(),
        }
    }

    /// The answer of `read`, the read of `kind`. When Palette refuses the read, or the read does
    /// not complete, the answer is `None`, and the error stays here under `kind`. So a kind that
    /// strata cannot read leaves the other kinds of the listing.
    ///
    /// # Errors
    ///
    /// Each other error of `read`, such as a saved tag filter that does not exist.
    pub(super) fn read<T>(
        &mut self,
        kind: ListKind,
        read: impl FnOnce() -> Result<T>,
    ) -> Result<Option<T>> {
        match read() {
            Ok(answer) => Ok(Some(answer)),
            Err(e) if e.is_failed_read() => {
                self.kinds.push((kind, e));
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// [`Unread::read`] of `kind`, after the note `note`, when the options ask for `kind`. `None`
    /// when they do not ask for it.
    ///
    /// # Errors
    ///
    /// The errors of [`Unread::read`].
    pub(super) fn wanted<T>(
        &mut self,
        kind: ListKind,
        note: &str,
        read: impl FnOnce() -> Result<T>,
    ) -> Result<Option<T>> {
        if !self.options.wants(kind) {
            return Ok(None);
        }
        self.progress.note(note);
        self.read(kind, read)
    }

    /// The error document of each kind that strata could not read, for
    /// [`Listing::errors`](super::Listing::errors).
    ///
    /// # Errors
    ///
    /// The error of the first kind, when strata read no kind that the options ask for. Then the
    /// listing holds nothing to print, and the command fails as one read.
    pub(super) fn into_errors(self) -> Result<BTreeMap<Open<ListKind>, Value>> {
        let asked = ListKind::ALL
            .iter()
            .filter(|k| self.options.wants(**k))
            .count();
        let read_none = self.kinds.len() == asked;
        let mut errors = BTreeMap::new();
        for (kind, error) in self.kinds {
            if read_none {
                return Err(error);
            }
            errors.insert(Open::Known(kind), error.to_json());
        }
        Ok(errors)
    }
}
