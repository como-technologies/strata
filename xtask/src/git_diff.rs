//! The git commands of the checks of a change: `xtask comment-words` and `xtask copies`.
//!
//! The two checks read the lines that a diff adds in one way. So they judge the same lines of a
//! change.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::Path;
use std::process::Command;

use crate::copies::go_lex;
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
    added_lines(&git(repo, &args)?)
}

/// The numbers of the lines that `diff` adds, for each file of the new side. `diff` is the output
/// of `git diff --unified=0`, so a hunk holds only the lines that it adds or removes.
///
/// The header of a file starts at `diff --git`, and its first hunk ends it. So an added line that
/// starts with `++ ` is no header. The error names a file whose name does not decode to UTF-8.
fn added_lines(diff: &str) -> Result<BTreeMap<String, BTreeSet<usize>>, String> {
    let mut added: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    let mut file = None;
    let mut header = false;
    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            header = true;
            file = None;
        } else if header && let Some(path) = line.strip_prefix("+++ ") {
            file = new_path(path)?;
        } else if let Some(hunk) = line.strip_prefix("@@ ") {
            header = false;
            if let (Some(path), Some(lines)) = (&file, new_lines(hunk)) {
                added.entry(path.clone()).or_default().extend(lines);
            }
        }
    }
    Ok(added)
}

/// The path of the new side of a file, from its header line without the first `+++ `. A deleted
/// file has `/dev/null` as its new side, and no path.
///
/// git writes a name that holds a space with a tab at its end. It writes a name that holds a
/// control character, a `"`, a `\`, or a character that is not ASCII in quotes, with the escapes of
/// C. The error names a name in quotes that does not decode to UTF-8.
fn new_path(name: &str) -> Result<Option<String>, String> {
    let path = if name.starts_with('"') {
        let bytes = go_lex::unquote(name).map_err(|e| format!("git diff: {e}"))?;
        String::from_utf8(bytes)
            .map_err(|_| format!("git diff: the file name {name} is not UTF-8"))?
    } else {
        name.strip_suffix('\t').unwrap_or(name).to_string()
    };
    Ok(path.strip_prefix("b/").map(str::to_string))
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
        let added = added_lines(diff).unwrap();
        assert_eq!(added.keys().collect::<Vec<_>>(), ["new.go", "src/a.rs"]);
        assert_eq!(added["src/a.rs"], BTreeSet::from([3, 4, 5, 11]));
        assert_eq!(added["new.go"], BTreeSet::from([4]));
    }

    /// The criterion of #944. git writes a tab after a name with a space. It writes a name with a
    /// mark or a character that is not ASCII in quotes. A name that is not UTF-8 is an error.
    #[test]
    fn the_added_lines_of_a_diff_name_a_file_with_a_space_a_mark_or_a_character_that_is_not_ascii()
    {
        let diff = "\
diff --git a/a b.rs b/a b.rs
+++ b/a b.rs\t
@@ -0,0 +1 @@
diff --git \"a/caf\\303\\251.rs\" \"b/caf\\303\\251.rs\"
+++ \"b/caf\\303\\251.rs\"
@@ -0,0 +2 @@
diff --git \"a/q\\\"t\\tb.rs\" \"b/q\\\"t\\tb.rs\"
+++ \"b/q\\\"t\\tb.rs\"
@@ -0,0 +3 @@
";
        let added = added_lines(diff).unwrap();
        assert_eq!(
            added,
            BTreeMap::from([
                ("a b.rs".to_string(), BTreeSet::from([1])),
                ("café.rs".to_string(), BTreeSet::from([2])),
                ("q\"t\tb.rs".to_string(), BTreeSet::from([3])),
            ])
        );

        let latin = "diff --git \"a/caf\\351.rs\" \"b/caf\\351.rs\"\n+++ \"b/caf\\351.rs\"\n";
        assert_eq!(
            added_lines(latin).unwrap_err(),
            "git diff: the file name \"b/caf\\351.rs\" is not UTF-8"
        );
    }
}
