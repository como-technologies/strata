//! The client of each scope that an operation reads. A scope is the project of a record or of a
//! cluster, the tenant, or the scope of the command.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use crate::api::{Palette, PaletteApi};
use crate::error::Result;

/// The scope that the project uid `project` names: `None`, the scope of the connection, when it
/// is empty. A record of the tenant scope and a cluster with no project both give an empty uid.
pub fn scope_of(project: &str) -> Option<&str> {
    Some(project).filter(|p| !p.is_empty())
}

/// The client that reads a cluster of project `project`: the one already connected for that
/// project, or a new one. It is `home` when the cluster names no project of its own. A cluster
/// with no project is in the scope the operation already works in, so `home` reads it.
/// `consumers` and `list clusters --detail` answered that differently until they shared this.
pub(super) fn client_for<'a, A, F>(
    clients: &'a mut HashMap<Option<String>, Palette<A>>,
    home: &'a Palette<A>,
    project: &str,
    connect: &mut F,
) -> Result<&'a Palette<A>>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    if project.is_empty() || Some(project) == home.project_uid() {
        return Ok(home);
    }
    connected(clients, Some(project), connect)
}

/// The client of the scope `scope`: the one that `clients` holds for it, or a new one from
/// `connect`, which `clients` then holds.
///
/// `None` is the scope of the command, and `Some` with the empty text is the tenant. `verify` reads
/// each record in its own scope with it. An operation that has a home client reads a cluster
/// through [`client_for`].
pub(super) fn connected<'a, A, F>(
    clients: &'a mut HashMap<Option<String>, Palette<A>>,
    scope: Option<&str>,
    connect: &mut F,
) -> Result<&'a Palette<A>>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    match clients.entry(scope.map(str::to_string)) {
        Entry::Occupied(e) => Ok(e.into_mut()),
        Entry::Vacant(v) => connect(scope).map(|p| &*v.insert(p)),
    }
}
