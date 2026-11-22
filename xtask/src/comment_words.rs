//! The check of the comment sentences of a change: `xtask comment-words BASE`.
//!
//! "The style rules" of `documentation.md` give a sentence of a description 25 words or fewer,
//! also in a code comment. The check reads the diff of the work tree against the merge base of
//! `BASE` and `HEAD`. It counts the words of each comment sentence that holds an added line. It
//! reads the line comments, `//` and `#`, and the block comments, `/* ... */`.
//!
//! A sentence that the change does not touch is not in the count, also in a block that the change
//! touches. The lines of a file that git finds as moved are not added lines. A comment after code
//! on its line is not in the count. A `//`, a `#`, or a `/*` after code can also be in a string,
//! and the check reads no string of a language.

use std::collections::BTreeSet;
use std::path::Path;

use crate::git_diff::{self, git};
use crate::sentences::{
    CommentLine, MOST_WORDS, Marks, block_sentences, comment_lines, marks_of, sentences,
    word_faults,
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
    let mut found = Found::default();
    for (path, added) in git_diff::added(repo, since.trim(), &[])? {
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

    /// A sentence of `count` words.
    fn sentence_of(count: usize) -> String {
        format!("{}.", vec!["word"; count].join(" "))
    }

    /// The criteria of #903 and #944. A sentence of more than 25 words fails in a line comment `//`
    /// and `#`, in a block comment, and in a query of CodeQL.
    #[test]
    fn a_comment_sentence_of_more_than_25_words_is_a_fault_in_each_form_of_comment() {
        let over = sentence_of(MOST_WORDS + 1);
        let at_limit = sentence_of(MOST_WORDS);
        for (path, text) in [
            ("a.rs", format!("/// {over}\nfn a() {{}}\n")),
            ("a.sh", format!("#!/bin/sh\n# {over}\n")),
            ("a.go", format!("/*\n{over}\n*/\npackage a\n")),
            ("a.css", format!("/* {over} */\n")),
            ("a.ql", format!("// {over}\nselect 1\n")),
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
        let found = changed_sentences(&marks, &text, &BTreeSet::from([3]));
        assert_eq!(
            found,
            [Sentence {
                line: 2,
                text: " The first half of a sentence and its second half.".to_string(),
            }]
        );
        assert!(changed_sentences(&marks, &text, &BTreeSet::from([4])).is_empty());
        assert_eq!(
            changed_sentences(&marks, &text, &BTreeSet::from([1])).len(),
            1
        );
    }

    /// The criterion of #944. A comment after code on its line is not in the count, as
    /// "The check of the comment sentences" in `documentation.md` states.
    #[test]
    fn a_comment_after_code_on_its_line_is_not_in_the_count() {
        let over = sentence_of(MOST_WORDS + 1);
        for (path, text) in [
            ("a.rs", format!("let a = 1; // {over}\n")),
            ("a.sh", format!("a=1 # {over}\n")),
            ("a.ql", format!("select 1 /* {over} */\n")),
        ] {
            let marks = marks_of(path).unwrap();
            let found = changed_sentences(&marks, &text, &BTreeSet::from([1]));
            assert!(found.is_empty(), "{path}: {found:?}");
        }
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
            crate::git::git(repo, args);
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

    /// The criterion of #944. git writes a tab after a name with a space, and a name that is not
    /// ASCII in quotes. The check reads each such file, and names it as the tree names it.
    #[test]
    fn the_check_reads_a_file_with_a_space_or_a_character_that_is_not_ascii_in_its_name() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        let over = sentence_of(MOST_WORDS + 1);
        crate::git::git(repo, &["init", "--quiet", "--initial-branch=main"]);
        crate::git::git(repo, &["commit", "--quiet", "--allow-empty", "-m", "base"]);
        for name in ["a b.rs", "café.rs"] {
            std::fs::write(repo.join(name), format!("// {over}\n")).unwrap();
        }
        crate::git::git(repo, &["add", "."]);

        let found = check(repo, "main").unwrap();
        assert_eq!(found.sentences, 2, "{found:?}");
        let places: Vec<&str> = found
            .faults
            .iter()
            .filter_map(|fault| fault.split(": ").next())
            .collect();
        assert_eq!(places, ["a b.rs:1", "café.rs:1"], "{found:?}");
    }
}
