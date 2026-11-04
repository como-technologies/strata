//! What a run of the book needs on disk before it starts: the examples, and the observer.

use std::path::Path;

/// The first profile that `just examples` writes: the tenants of the book read their pack data from
/// the raw profiles.
pub(super) const FIRST_EXAMPLE: &str = "examples/profiles/raw/aws-stack@1.1.1/profile.json";

/// The observer that `just observer-build` writes. A step of the book runs it as `strata-observer`.
pub(super) const OBSERVER_BINARY: &str = "target/observer/strata-observer";

/// Check that `just examples` ran in the repository `repo`.
///
/// # Errors
///
/// The message to show when the user did not build the examples.
pub(crate) fn built_examples(repo: &Path) -> Result<(), String> {
    if repo.join(FIRST_EXAMPLE).is_file() {
        Ok(())
    } else {
        Err("the examples are not built. Run: just examples".to_string())
    }
}

/// Check that `just observer-build` ran in the repository `repo`.
///
/// # Errors
///
/// The message to show when the user did not build the observer.
pub(crate) fn built_observer(repo: &Path) -> Result<(), String> {
    if repo.join(OBSERVER_BINARY).is_file() {
        Ok(())
    } else {
        Err("the observer is not built. Run: just observer-build".to_string())
    }
}
