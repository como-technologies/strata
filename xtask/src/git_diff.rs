//! The git commands of the checks of a change: `xtask comment-words` and `xtask copies`.
//!
//! The two checks read the lines that a diff adds in one way. So they judge the same lines of a
//! change.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::Path;
use std::process::Command;

use crate::process::CommandExt;

/// The output of `git -C repo` with `args`. The error names the command and gives its message.
pub fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    git_bytes(repo, args).map(|out| String::from_utf8_lossy(&out).into_owned())
}

/// The output of `git -C repo` with `args`, as bytes, such as an archive.
pub fn git_bytes(repo: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let command = format!("git {}", args.join(" "));
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output_locked()
        .map_err(|e| format!("{command}: {e}"))?;
    if !out.status.success() {
        let message = String::from_utf8_lossy(&out.stderr);
        return Err(format!("{command}: {}", message.trim()));
    }
    Ok(out.stdout)
}

/// The numbers of the lines that `git diff` adds, for each file of the new side. `revisions` names
/// the two sides, such as a commit for the work tree, or `BASE...HEAD`. `paths` limits the files.
///
/// A line of a file that git finds as moved is not an added line.
pub fn added(
    repo: &Path,
    revisions: &str,
    paths: &[&str],
) -> Result<BTreeMap<String, BTreeSet<usize>>, String> {
    let mut args = vec![
        "diff",
        "--unified=0",
        "--no-color",
        "--no-ext-diff",
        "--find-renames",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        revisions,
        "--",
    ];
    args.extend(paths);
    Ok(added_lines(&git(repo, &args)?))
}

/// The numbers of the lines that `diff` adds, for each file of the new side. `diff` is the output
/// of `git diff --unified=0`, so a hunk holds only the lines that it adds or removes.
///
/// The header of a file starts at `diff --git`, and its first hunk ends it. So an added line that
/// starts with `++ ` is no header.
fn added_lines(diff: &str) -> BTreeMap<String, BTreeSet<usize>> {
    let mut added: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    let mut file = None;
    let mut header = false;
    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            header = true;
            file = None;
        } else if header && let Some(path) = line.strip_prefix("+++ ") {
            // a deleted file has `/dev/null` as its new side
            file = path.strip_prefix("b/").map(str::to_string);
        } else if let Some(hunk) = line.strip_prefix("@@ ") {
            header = false;
            if let (Some(path), Some(lines)) = (&file, new_lines(hunk)) {
                added.entry(path.clone()).or_default().extend(lines);
            }
        }
    }
    added
}

/// The lines of the new side of a hunk, from its header without the first `@@ `. The header
/// `-3,2 +4,5 @@` gives the lines 4 to 8. A count of 0 gives no line, and no count gives one line.
fn new_lines(hunk: &str) -> Option<Range<usize>> {
    let new = hunk.split(' ').find_map(|part| part.strip_prefix('+'))?;
    let (start, count) = match new.split_once(',') {
        Some((start, count)) => (start.parse().ok()?, count.parse().ok()?),
        None => (new.parse().ok()?, 1),
    };
    Some(start..start + count)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The numbers of the lines that `lines` names, as a set.
    fn set(lines: &[usize]) -> BTreeSet<usize> {
        lines.iter().copied().collect()
    }

    #[test]
    fn the_added_lines_of_a_diff_follow_the_new_side_of_each_hunk_and_skip_a_deleted_file() {
        let diff = "\
diff --git a/src/a.rs b/src/a.rs
index 1..2 100644
--- a/src/a.rs
+++ b/src/a.rs
@@ -3,2 +3,3 @@ fn main() {
+// one
+// two
+// three
@@ -10 +11 @@
+++ an added line that starts with two plus marks
@@ -20,2 +21,0 @@
diff --git a/gone.sh b/gone.sh
deleted file mode 100644
--- a/gone.sh
+++ /dev/null
@@ -1,2 +0,0 @@
diff --git a/old.go b/new.go
similarity index 90%
rename from old.go
rename to new.go
--- a/old.go
+++ b/new.go
@@ -4 +4 @@
";
        let added = added_lines(diff);
        assert_eq!(added.keys().collect::<Vec<_>>(), ["new.go", "src/a.rs"]);
        assert_eq!(added["src/a.rs"], set(&[3, 4, 5, 11]));
        assert_eq!(added["new.go"], set(&[4]));
    }
}
