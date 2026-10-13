//! The reads and the writes of a file, with the path in each error.
//!
//! Each layer reads and writes its files here, so no layer below [`crate::ops`] calls `ops`.

use std::path::Path;

use crate::error::{Error, Result};

/// Write `text` to `path`, and make the directory that `path` names when it does not exist.
///
/// Each file that strata writes goes through here, so `-o releases/1.2.0/dev.json` needs no
/// `mkdir`.
///
/// # Errors
///
/// `Io`, with `path`, when strata cannot make the directory or write the file.
pub fn write_file(path: &Path, text: &str) -> Result<()> {
    let io = |source| Error::Io {
        path: path.to_path_buf(),
        source,
    };
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    std::fs::write(path, text).map_err(io)
}

/// Read the text of the file at `path`.
///
/// Each text file that strata reads goes through here.
///
/// # Errors
///
/// `Io`, with `path`, when strata cannot read the file.
pub(crate) fn read_file(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}
