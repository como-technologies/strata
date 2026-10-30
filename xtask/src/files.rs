//! The scans of a directory that the tools of the book share.

use std::path::{Path, PathBuf};

/// The entries of `dir` in name order. A directory that cannot be read has none.
fn entries(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    entries
}

fn has_extension(path: &Path, ext: &str) -> bool {
    path.extension().is_some_and(|e| e == ext)
}

/// The files directly in `dir` with the extension `ext`, in name order.
///
/// The sort gives the same listing on each file system.
pub fn files_with_ext(dir: &Path, ext: &str) -> Vec<PathBuf> {
    entries(dir)
        .into_iter()
        .filter(|p| p.is_file() && has_extension(p, ext))
        .collect()
}

/// Every markdown page under `dir`, at any depth, in name order.
pub fn pages(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for path in entries(dir) {
        if path.is_dir() {
            out.extend(pages(&path));
        } else if has_extension(&path, "md") {
            out.push(path);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory for the test `test`, with `b.md`, `a.md`, `c.txt`, and `sub/d.md`.
    fn tree(test: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("strata-files-{test}-{}", std::process::id()));
        std::fs::create_dir_all(root.join("sub")).unwrap();
        for file in ["b.md", "a.md", "c.txt", "sub/d.md"] {
            std::fs::write(root.join(file), "").unwrap();
        }
        root
    }

    fn names(root: &Path, files: Vec<PathBuf>) -> Vec<String> {
        files
            .iter()
            .map(|f| f.strip_prefix(root).unwrap().display().to_string())
            .collect()
    }

    #[test]
    fn the_files_of_a_directory_with_an_extension_come_in_name_order_without_the_subdirectories() {
        let root = tree("files");
        let found = names(&root, files_with_ext(&root, "md"));
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(found, ["a.md", "b.md"]);
    }

    #[test]
    fn the_pages_of_a_directory_come_in_name_order_at_any_depth() {
        let root = tree("pages");
        let found = names(&root, pages(&root));
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(found, ["a.md", "b.md", "sub/d.md"]);
    }

    #[test]
    fn a_directory_that_does_not_exist_has_no_file() {
        let missing =
            std::env::temp_dir().join(format!("strata-files-missing-{}", std::process::id()));
        assert!(files_with_ext(&missing, "md").is_empty());
        assert!(pages(&missing).is_empty());
    }
}
