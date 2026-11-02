//! The list of the facts of Palette that strata depends on, and the checks that keep it true.
//!
//! A fact is a request of Palette, a behavior of Palette, or a gap that strata fills. Each fact
//! names its request, what strata reads, the route of the stand-in that imitates it, the live
//! check that proves it, the page of the book that states it, and the code or the rule of
//! preflight that uses it. [The list of the facts](../../../docs/src/development/facts.md)
//! says how to read it and how to add a fact.
//!
//! The files of the list are in `xtask/facts/`. [`check()`] compares them with the book, the live
//! scripts, the rules of strata-preflight, and the stand-in. [`print()`] gives them as JSON for a
//! script.

use std::collections::BTreeSet;
use std::path::Path;

mod check;
mod model;
mod sources;
#[cfg(test)]
mod tests;

use sources::Sources;

/// Every fault of the list in the repository at `repo`, as messages.
///
/// `served` holds the routes that the stand-in serves for the book. A caller with no run of the
/// book gives `None`, and the check of the stand-in is not made.
pub fn check(repo: &Path, served: Option<&BTreeSet<String>>) -> Vec<String> {
    let (sources, mut bad) = Sources::read(repo, served.cloned());
    bad.extend(check::check(&sources));
    bad
}

/// The list as JSON: the schema id and the facts, for a script that reads them with `jq`. An
/// error is the messages about the files of the list.
pub fn print(repo: &Path) -> Result<String, Vec<String>> {
    let (facts, bad) = model::load(&repo.join(model::DIRECTORY));
    if !bad.is_empty() {
        return Err(bad);
    }
    let document = serde_json::json!({"schema": model::SCHEMA, "facts": facts});
    serde_json::to_string_pretty(&document).map_err(|e| vec![e.to_string()])
}
