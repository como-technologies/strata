//! The reads and the writes of a file, with the path in each error.
//!
//! Each layer reads and writes its files here, so no layer below [`crate::ops`] calls `ops`.
//!
//! It also names the record files that a command line gives: a path, or a pattern that strata
//! expands itself.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// The `Io` error of `source`, with the path `path`.
fn io_error(path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Make the directory that `path` names, when `path` has one and it does not exist.
fn make_parent(path: &Path) -> Result<()> {
    match path.parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(parent) => std::fs::create_dir_all(parent).map_err(|source| io_error(path, source)),
        None => Ok(()),
    }
}

/// Write `text` to `path`, and make the directory that `path` names when it does not exist.
///
/// Each file that strata writes goes through here, so `-o releases/1.2.0/dev.json` needs no
/// `mkdir`.
///
/// # Errors
///
/// `Io`, with `path`, when strata cannot make the directory or write the file.
pub fn write_file(path: &Path, text: &str) -> Result<()> {
    make_parent(path)?;
    std::fs::write(path, text).map_err(|source| io_error(path, source))
}

/// Add `text` to the end of the file at `path`. The call makes the file and the directory that
/// `path` names when they do not exist.
///
/// A log goes through here. The call writes `text` with one write, so two runs that add a line
/// do not mix their lines.
///
/// # Errors
///
/// `Io`, with `path`, when strata cannot make the directory or write the file.
pub(crate) fn append_file(path: &Path, text: &str) -> Result<()> {
    make_parent(path)?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .map_err(|source| io_error(path, source))
}

/// Read the text of the file at `path`.
///
/// Each text file that strata reads goes through here.
///
/// # Errors
///
/// `Io`, with `path`, when strata cannot read the file.
pub(crate) fn read_file(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|source| io_error(path, source))
}

/// True when `arg` is a file pattern: it holds `*`, `?`, or `[`.
pub fn is_pattern(arg: &str) -> bool {
    arg.contains(['*', '?', '['])
}

/// The record files that the arguments `args` name, in the order given.
///
/// A path stays as it is. A pattern is expanded by strata, so that a quoted pattern works from any
/// shell or scheduler: `releases/**/*.json` names the files at every depth, and the ones directly
/// under `releases/` too. The files of one pattern come in the order of their names.
///
/// # Errors
///
/// `Pattern` when an argument is a pattern that is not valid, or one that no file matches.
pub fn expand_records(args: &[String]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for arg in args {
        if !is_pattern(arg) {
            out.push(PathBuf::from(arg));
            continue;
        }
        let matches: Vec<PathBuf> = glob::glob(arg)
            .map_err(|e| Error::Pattern {
                pattern: arg.clone(),
                message: format!("is not a valid pattern: {}", e.msg),
            })?
            .filter_map(std::result::Result::ok)
            .filter(|p| p.is_file())
            .collect();
        if matches.is_empty() {
            return Err(Error::Pattern {
                pattern: arg.clone(),
                message: "matched no files".to_string(),
            });
        }
        out.extend(matches);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pattern_matches_at_every_depth_and_a_plain_path_stays() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        for f in [
            "releases/x.json",
            "releases/aws/a.json",
            "releases/aws/2026/b.json",
        ] {
            let path = dir.join(f);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "{}").unwrap();
        }
        let pattern = dir.join("releases/**/*.json").display().to_string();
        let found = expand_records(&[pattern]).unwrap();
        let names: Vec<String> = found
            .iter()
            .map(|p| p.strip_prefix(dir).unwrap().display().to_string())
            .collect();
        assert_eq!(
            names,
            vec![
                "releases/aws/2026/b.json",
                "releases/aws/a.json",
                "releases/x.json"
            ],
            "`**` matches zero directories as well as many"
        );
        let plain = dir.join("releases/x.json").display().to_string();
        assert_eq!(
            expand_records(std::slice::from_ref(&plain)).unwrap(),
            vec![PathBuf::from(plain)]
        );
    }

    #[test]
    fn a_pattern_that_matches_no_file_or_is_not_valid_is_a_pattern_error() {
        let tmp = tempfile::tempdir().unwrap();
        let none = tmp.path().join("*.yaml").display().to_string();
        let err = expand_records(std::slice::from_ref(&none)).unwrap_err();
        assert_eq!(err.kind(), "pattern");
        assert_eq!(err.to_string(), format!("'{none}' matched no files"));
        let invalid = "releases/[".to_string();
        let err = expand_records(std::slice::from_ref(&invalid)).unwrap_err();
        assert_eq!(err.kind(), "pattern");
        assert!(err.to_string().contains("is not a valid pattern"), "{err}");
    }

    #[test]
    fn a_text_is_a_pattern_when_it_holds_a_star_a_mark_or_a_bracket() {
        for arg in ["*.json", "a?.json", "a[12].json", "releases/**/*.json"] {
            assert!(is_pattern(arg), "{arg}");
        }
        assert!(!is_pattern("releases/1.1.1/dev.json"));
    }
}
