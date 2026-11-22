//! A git repository of a test, in a temporary directory. It goes when the test ends.

use std::path::Path;

use crate::git::git;

/// A git repository of a test, on the branch `main`.
pub struct Repo {
    dir: tempfile::TempDir,
}

impl Repo {
    /// A repository with no commit.
    pub fn new() -> Repo {
        let repo = Repo {
            dir: tempfile::tempdir().unwrap(),
        };
        git(repo.path(), &["init", "--quiet", "--initial-branch=main"]);
        repo
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Write `text` to the file `path` of the work tree, and make its directories.
    pub fn write(&self, path: &str, text: &str) {
        let file = self.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }

    /// Make the branch `name` at the commit of the work tree, and go to it.
    pub fn branch(&self, name: &str) {
        git(self.path(), &["checkout", "--quiet", "-b", name]);
    }

    /// Go to the branch `name`.
    pub fn to(&self, name: &str) {
        git(self.path(), &["checkout", "--quiet", name]);
    }

    /// Stage each file of the work tree.
    pub fn stage(&self) {
        git(self.path(), &["add", "."]);
    }

    /// Stage each file of the work tree, and commit it. A commit with no file is the empty base of
    /// a test.
    pub fn commit(&self, message: &str) {
        self.stage();
        git(
            self.path(),
            &["commit", "--quiet", "--allow-empty", "-m", message],
        );
    }
}
