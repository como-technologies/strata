//! The files that a declaration reaches: where a line can name it in place of a copy of its value.

use std::collections::BTreeSet;

use super::Scope;
use super::source::dir_of;

/// The files that a declaration reaches. A directory has a `/` at its end, or is empty for the
/// root of the repository.
///
/// The value of a declaration of test code reaches only the test code of these files: `tests` is
/// true for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reach {
    /// Each file under `dir`, apart from the files under each directory of `apart`: a Rust package
    /// without the packages inside it.
    Tree {
        dir: String,
        apart: Vec<String>,
        tests: bool,
    },
    /// Each file under `dir`, apart from the files directly in each directory of `apart`. It is a Go
    /// module, without the packages that the package of the const imports.
    Module {
        dir: String,
        apart: BTreeSet<String>,
        tests: bool,
    },
    /// The files directly in `dir`: a Go package.
    Package { dir: String, tests: bool },
    /// The file `file` and each file under `dir`: a Rust module and its children.
    File {
        file: String,
        dir: String,
        tests: bool,
    },
}

impl Reach {
    /// True when the declaration reaches the file `path`. `test` is true for a place of test code.
    pub fn holds(&self, path: &str, test: bool) -> bool {
        let (tests, reaches) = match self {
            Reach::Tree { dir, apart, tests } => (
                tests,
                path.starts_with(dir.as_str())
                    && !apart.iter().any(|a| path.starts_with(a.as_str())),
            ),
            Reach::Module { dir, apart, tests } => (
                tests,
                path.starts_with(dir.as_str()) && !apart.contains(dir_of(path)),
            ),
            Reach::Package { dir, tests } => (tests, dir_of(path) == dir),
            Reach::File { file, dir, tests } => {
                (tests, path == file || path.starts_with(dir.as_str()))
            }
        };
        reaches && (test || !tests)
    }
}

/// The reach of a Rust declaration of the file `path` in the package at `root`, with the scope
/// `scope`. `roots` are the roots of each package of the repository, and `test` is true for a
/// declaration of test code.
///
/// A module file `mod.rs`, `lib.rs`, or `main.rs` holds the module of its directory. Another file
/// `name.rs` holds the module `name`, whose children are under `name/`.
pub fn rust_reach(path: &str, root: &str, roots: &[String], scope: Scope, test: bool) -> Reach {
    let dir = dir_of(path);
    let module_file = ["mod.rs", "lib.rs", "main.rs"]
        .iter()
        .any(|name| path.strip_prefix(dir) == Some(name));
    let tree = |dir: &str| Reach::Tree {
        dir: dir.to_string(),
        apart: roots
            .iter()
            .filter(|other| {
                other.len() > dir.len() && other.starts_with(dir) && other.as_str() != root
            })
            .cloned()
            .collect(),
        tests: test,
    };
    match scope {
        Scope::Package => tree(root),
        Scope::Module if module_file => tree(dir),
        Scope::Module => Reach::File {
            file: path.to_string(),
            dir: format!("{}/", path.trim_end_matches(".rs")),
            tests: test,
        },
        Scope::Parent if module_file => tree(dir_of(dir.trim_end_matches('/'))),
        Scope::Parent => tree(dir),
    }
}
