//! The walk of a directory, for the tests of the root package and of `strata-preflight`.

use std::fs;
use std::path::{Path, PathBuf};

/// Each file under a directory, at any depth. A directory that the function cannot read has no
/// file.
pub fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}
