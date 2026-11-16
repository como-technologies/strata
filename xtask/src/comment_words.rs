//! The check of the comment sentences of a change: `xtask comment-words BASE`.
//!
//! "The style rules" of `documentation.md` give a sentence of a description 25 words or fewer,
//! also in a code comment. The check reads the diff of the work tree against the merge base of
//! `BASE` and `HEAD`. It counts the words of each comment sentence that holds an added line. It
//! reads the line comments, `//` and `#`, and the block comments, `/* ... */`.
//!
//! A sentence that the change does not touch is not in the count, also in a block that the change
//! touches. The lines of a file that git finds as moved are not added lines.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::Path;
use std::process::Command;

use crate::process::CommandExt;
use crate::sentences::{
    CommentLine, MOST_WORDS, block_sentences, comment_lines, sentences, word_faults,
};

/// The start of the path of each file that a tool writes. `just docs-theme` installs them: see
/// "The theme" in `documentation.md`.
const GENERATED: [&str; 2] = ["docs/gruvbox/", "docs/mermaid"];

/// What the check found in the comments of a change.
#[derive(Debug, Default)]
pub struct Found {
    /// The count of the comment sentences that the change adds or changes.
    pub sentences: usize,
    /// One line for each such sentence of more than [`MOST_WORDS`] words.
    pub faults: Vec<String>,
}

/// Count the words of each comment sentence that the diff of the work tree against `base` adds.
/// The diff starts at the merge base of `base` and `HEAD`, so a newer `base` adds no line.
///
/// `repo` is the root of the work tree. The error names the git command or the file that failed.
pub fn check(repo: &Path, base: &str) -> Result<Found, String> {
    let since = git(repo, &["merge-base", base, "HEAD"])?;
    let diff = git(
        repo,
        &[
            "diff",
            "--unified=0",
            "--no-color",
            "--no-ext-diff",
            "--find-renames",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            since.trim(),
            "--",
        ],
    )?;
    let mut found = Found::default();
    for (path, added) in added_lines(&diff) {
        let Some(marks) = marks_of(&path) else {
            continue;
        };
        if GENERATED.iter().any(|start| path.starts_with(start)) {
            continue;
        }
        let text = std::fs::read_to_string(repo.join(&path)).map_err(|e| format!("{path}: {e}"))?;
        for sentence in changed_sentences(&marks, &text, &added) {
            found.sentences += 1;
            let place = format!("{path}:{}", sentence.line);
            found
                .faults
                .extend(word_faults(&place, &[sentence.text], MOST_WORDS));
        }
    }
    Ok(found)
}

/// The output of `git -C repo` with `args`. The error names the command and gives its message.
fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
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
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The comments that a kind of file holds.
#[derive(Debug, PartialEq, Eq)]
struct Marks {
    /// The mark of a line comment, when the kind has one.
    line: Option<&'static str>,
    /// True when the kind has block comments.
    block: bool,
}

/// The comments of the file at `path`, by its extension or its name. `None` for a kind of file
/// with no comments that the check reads, such as a page of the book or a JSON document.
fn marks_of(path: &str) -> Option<Marks> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let (line, block) = match name.rsplit_once('.').map(|(_, extension)| extension) {
        Some("rs" | "go" | "js") => (Some("//"), true),
        Some("css") => (None, true),
        Some("sh" | "yml" | "yaml" | "toml" | "tf" | "hcl" | "jq" | "service" | "timer") => {
            (Some("#"), false)
        }
        _ if name == "Dockerfile" || name == "justfile" => (Some("#"), false),
        _ => return None,
    };
    Some(Marks { line, block })
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

/// A comment sentence, and the number of its first line in the file.
#[derive(Debug, PartialEq, Eq)]
struct Sentence {
    line: usize,
    text: String,
}

/// The comment sentences of `text` that hold a line of `added`.
fn changed_sentences(marks: &Marks, text: &str, added: &BTreeSet<usize>) -> Vec<Sentence> {
    let block_comments = if marks.block {
        block_comment_lines(text)
    } else {
        String::new()
    };
    let mut blocks = Vec::new();
    if let Some(mark) = marks.line {
        blocks.extend(comment_lines(text, mark));
    }
    blocks.extend(comment_lines(&block_comments, "//"));
    blocks
        .iter()
        .flat_map(|block| touched(block, added))
        .collect()
}

/// The sentences of `block` that hold a line of `added`. A table row is one whole: when one of its
/// lines is added, each sentence of each cell counts.
fn touched(block: &[CommentLine], added: &BTreeSet<usize>) -> Vec<Sentence> {
    let Some(first) = block.first().map(|line| line.number) else {
        return Vec::new();
    };
    if !block.iter().any(|line| added.contains(&line.number)) {
        return Vec::new();
    }
    let words: Vec<&str> = block.iter().map(|line| line.words).collect();
    let text = words.join(" ");
    if text.starts_with('|') {
        return block_sentences(&text)
            .into_iter()
            .map(|text| Sentence { line: first, text })
            .collect();
    }
    // Each line has its range in `text`, and one space joins two lines. The sentences follow one
    // another in `text`, so the end of one is the start of the next.
    let mut spans = Vec::new();
    let mut at = 0;
    for line in block {
        spans.push((line.number, at..at + line.words.len()));
        at += line.words.len() + 1;
    }
    let mut out = Vec::new();
    let mut start = 0;
    for sentence in sentences(&text) {
        let end = start + sentence.len();
        let lines: Vec<usize> = spans
            .iter()
            .filter(|(_, span)| span.start < end && start < span.end)
            .map(|(number, _)| *number)
            .collect();
        if let Some(&line) = lines.first()
            && lines.iter().any(|number| added.contains(number))
        {
            out.push(Sentence {
                line,
                text: sentence,
            });
        }
        start = end;
    }
    out
}

/// The block comments of `text`, as `//` lines with the line numbers of `text`. So
/// [`comment_lines`] reads them as it reads the line comments.
///
/// A block comment starts at a line that starts with `/*`, and it ends at the next `*/`. A line of
/// it gives `//` and its words, without a `*` at its start. Each other line is empty. A `/*` after
/// code on its line, or in a string, starts no block.
fn block_comment_lines(text: &str) -> String {
    let mut out = Vec::new();
    let mut open = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let body = if open {
            match trimmed.strip_prefix('*') {
                Some(after) if !after.starts_with('/') => after,
                _ => trimmed,
            }
        } else if let Some(after) = trimmed.strip_prefix("/*") {
            open = true;
            // `/**` and `/*!` start a doc comment
            match after.strip_prefix(['*', '!']) {
                Some(rest) if !rest.starts_with('/') => rest,
                _ => after,
            }
        } else {
            out.push(String::new());
            continue;
        };
        let words = match body.find("*/") {
            Some(end) => {
                open = false;
                &body[..end]
            }
            None => body,
        };
        if words.trim().is_empty() {
            out.push(String::new());
        } else if words.starts_with(' ') {
            // two spaces after the mark keep a literal line literal
            out.push(format!("//{words}"));
        } else {
            out.push(format!("// {words}"));
        }
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::git::isolate_git;

    /// A sentence of `count` words.
    fn sentence_of(count: usize) -> String {
        format!("{}.", vec!["word"; count].join(" "))
    }

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

    #[test]
    fn each_kind_of_file_has_the_marks_of_its_comments() {
        let line = |mark| Marks {
            line: Some(mark),
            block: false,
        };
        for (path, marks) in [
            (
                "src/main.rs",
                Some(Marks {
                    line: Some("//"),
                    block: true,
                }),
            ),
            (
                "observer/internal/web/assets/app.css",
                Some(Marks {
                    line: None,
                    block: true,
                }),
            ),
            ("examples/live/lib.sh", Some(line("#"))),
            (".github/workflows/ci.yml", Some(line("#"))),
            ("deploy/observer/Dockerfile", Some(line("#"))),
            ("justfile", Some(line("#"))),
            ("docs/src/loop.md", None),
            ("examples/registries.json", None),
        ] {
            assert_eq!(marks_of(path), marks, "{path}");
        }
    }

    /// The criterion of #903: a sentence of more than 25 words fails, in a line comment `//` and
    /// `#`, and in a block comment.
    #[test]
    fn a_comment_sentence_of_more_than_25_words_is_a_fault_in_each_form_of_comment() {
        let over = sentence_of(MOST_WORDS + 1);
        let at_limit = sentence_of(MOST_WORDS);
        for (path, text) in [
            ("a.rs", format!("/// {over}\nfn a() {{}}\n")),
            ("a.sh", format!("#!/bin/sh\n# {over}\n")),
            ("a.go", format!("/*\n{over}\n*/\npackage a\n")),
            ("a.css", format!("/* {over} */\n")),
        ] {
            let marks = marks_of(path).unwrap();
            let all: BTreeSet<usize> = (1..=text.lines().count()).collect();
            let found = changed_sentences(&marks, &text, &all);
            assert_eq!(found.len(), 1, "{path}: {found:?}");
            let faults = word_faults(path, &[found[0].text.clone()], MOST_WORDS);
            assert_eq!(faults.len(), 1, "{path}: {faults:?}");

            let short = text.replace(&over, &at_limit);
            let found = changed_sentences(&marks, &short, &all);
            assert!(
                word_faults(path, &[found[0].text.clone()], MOST_WORDS).is_empty(),
                "{path}"
            );
        }
    }

    /// The check counts the sentences that a change touches. An old sentence of the same block
    /// stays out, and a sentence with one added line counts whole.
    #[test]
    fn only_a_sentence_that_holds_an_added_line_counts_and_it_names_its_first_line() {
        let text = format!(
            "// {}\n// The first half of a sentence\n// and its second half.\n",
            sentence_of(MOST_WORDS + 1)
        );
        let marks = marks_of("a.rs").unwrap();
        let found = changed_sentences(&marks, &text, &set(&[3]));
        assert_eq!(
            found,
            [Sentence {
                line: 2,
                text: " The first half of a sentence and its second half.".to_string(),
            }]
        );
        assert!(changed_sentences(&marks, &text, &set(&[4])).is_empty());
        assert_eq!(changed_sentences(&marks, &text, &set(&[1])).len(), 1);
    }

    #[test]
    fn a_block_comment_keeps_its_line_numbers_and_loses_its_marks() {
        let text = "code();\n/**\n * One line.\n *  a literal line\n */\nx /* no block */\n";
        assert_eq!(
            block_comment_lines(text),
            "\n\n// One line.\n//  a literal line\n\n"
        );
    }

    /// The check reads the diff from the merge base, so a commit of the base after the branch adds
    /// no line. A file that git does not track is not in the diff.
    #[test]
    fn the_check_reads_the_diff_from_the_merge_base_and_names_the_file_and_the_line() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        let run = |args: &[&str]| {
            let out = isolate_git(Command::new("git").arg("-C").arg(repo).args(args))
                .output_locked()
                .unwrap();
            assert!(out.status.success(), "git {args:?}: {out:?}");
        };
        let write = |file: &str, text: &str| std::fs::write(repo.join(file), text).unwrap();
        let over = sentence_of(MOST_WORDS + 1);

        run(&["init", "--quiet", "--initial-branch=main"]);
        write("lib.rs", &format!("// {over}\nfn a() {{}}\n"));
        run(&["add", "."]);
        run(&["commit", "--quiet", "-m", "base"]);
        run(&["switch", "--quiet", "-c", "change"]);
        write(
            "lib.rs",
            &format!("// {over}\nfn a() {{}}\n// A short one.\n"),
        );
        write("run.sh", &format!("#!/bin/sh\n# {over}\n"));
        run(&["add", "run.sh"]);
        write("notes.sh", &format!("# {over}\n"));
        run(&["commit", "--quiet", "-m", "change"]);
        run(&["switch", "--quiet", "main"]);
        write("main.rs", &format!("// {over}\n"));
        run(&["add", "main.rs"]);
        run(&["commit", "--quiet", "-m", "later on main"]);
        run(&["switch", "--quiet", "change"]);

        let found = check(repo, "main").unwrap();
        assert_eq!(found.sentences, 2, "{found:?}");
        assert_eq!(found.faults.len(), 1, "{found:?}");
        assert!(
            found.faults[0].starts_with("run.sh:2: 26 words in a sentence of 25 at most: "),
            "{found:?}"
        );
        assert!(check(repo, "no-such-ref").is_err());
    }
}
