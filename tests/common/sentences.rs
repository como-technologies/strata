//! The sentences and the words of a text, as "The style rules" of `documentation.md` count them.
//!
//! `tests/common/mod.rs` re-exports this file for the tests of the book and of the comments. The
//! check `xtask comment-words` includes it with `#[path]`. So the tests and the check count the
//! words of a sentence in one way.

use super::table::cells;

/// The most words in a sentence of a description. "The style rules" of `documentation.md` give it.
pub const MOST_WORDS: usize = 25;

/// The sentences of `block`. A sentence ends at one of three marks: a full stop, a question mark,
/// or an exclamation mark. The mark is outside a code span, and a space or the end follows it.
pub fn sentences(block: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_code = false;
    let mut chars = block.chars().peekable();
    while let Some(c) = chars.next() {
        current.push(c);
        in_code ^= c == '`';
        let ends = matches!(c, '.' | '?' | '!') && chars.peek().is_none_or(|n| n.is_whitespace());
        if ends && !in_code {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

/// The sentences of a block. A table row has the sentences of each cell.
pub fn block_sentences(block: &str) -> Vec<String> {
    if block.starts_with('|') {
        return cells(block).into_iter().flat_map(sentences).collect();
    }
    sentences(block)
}

/// `text` without the target of each link: `[words](target)` keeps `[words]`. A target is a path
/// or an address, and no word of the prose.
pub fn without_link_targets(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("](") {
        out.push_str(&rest[..=at]);
        rest = rest[at + 2..]
            .find(')')
            .map_or("", |end| &rest[at + 2 + end + 1..]);
    }
    out.push_str(rest);
    out
}

/// The words of a sentence: each run of characters between spaces that holds a letter or a
/// digit. The target of a link is not a word.
pub fn words(sentence: &str) -> usize {
    without_link_targets(sentence)
        .split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .count()
}

/// The faults of the sentences `found`, which `place` holds: one line for each sentence of more
/// than `limit` words.
pub fn word_faults(place: &str, found: &[String], limit: usize) -> Vec<String> {
    found
        .iter()
        .filter(|s| words(s) > limit)
        .map(|s| {
            let count = words(s);
            format!(
                "{place}: {count} words in a sentence of {limit} at most: {}",
                s.trim()
            )
        })
        .collect()
}

/// The comments that a kind of file holds.
pub struct Marks {
    /// The mark of a line comment, when the kind has one.
    pub line: Option<&'static str>,
    /// True when the kind has block comments, `/* ... */`.
    pub block: bool,
}

/// The comments of the file at `path`, by its extension or its name. `None` for a kind of file
/// with no comments that the checks read, such as a page of the book or a JSON document.
pub fn marks_of(path: &str) -> Option<Marks> {
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

/// A line of a comment block: the number of the line in its file, from 1, and its words without
/// the marks.
pub struct CommentLine<'a> {
    pub number: usize,
    pub words: &'a str,
}

/// The comment blocks of a file, each with its lines. A block is a run of lines that start with
/// `mark`. A line that is not one, a blank comment line, a list item, a literal line, or a fence
/// ends the run. `///` and `//!` are `//` with one more mark.
///
/// A literal line has two spaces or more after the mark, as a table or a usage text has. It is a
/// block of its own. A fenced block of code in a comment is not prose, so no block holds it. A
/// shebang is not a comment.
pub fn comment_lines<'a>(text: &'a str, mark: &str) -> Vec<Vec<CommentLine<'a>>> {
    fn end_block<'a>(blocks: &mut Vec<Vec<CommentLine<'a>>>, run: &mut Vec<CommentLine<'a>>) {
        if !run.is_empty() {
            blocks.push(std::mem::take(run));
        }
    }
    fn is_list_item(line: &str) -> bool {
        let digits = line.trim_start_matches(|c: char| c.is_ascii_digit());
        line.starts_with("- ")
            || line.starts_with("* ")
            || (digits.len() < line.len() && (digits.starts_with(". ") || digits.starts_with(") ")))
    }
    let mut blocks = Vec::new();
    let mut run = Vec::new();
    let mut fenced = false;
    for (at, line) in text.lines().enumerate() {
        let line = line.trim_start();
        let rest = line
            .strip_prefix(mark)
            .filter(|_| !(at == 0 && line.starts_with("#!")));
        let Some(rest) = rest else {
            end_block(&mut blocks, &mut run);
            fenced = false;
            continue;
        };
        let rest = rest.trim_start_matches(['/', '!']);
        let words = rest.trim();
        if words.starts_with("```") {
            end_block(&mut blocks, &mut run);
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        if words.is_empty() {
            end_block(&mut blocks, &mut run);
            continue;
        }
        let literal = rest.starts_with("  ");
        if literal || is_list_item(words) {
            end_block(&mut blocks, &mut run);
        }
        run.push(CommentLine {
            number: at + 1,
            words,
        });
        if literal {
            end_block(&mut blocks, &mut run);
        }
    }
    end_block(&mut blocks, &mut run);
    blocks
}

/// The comment blocks of a file, as [`comment_lines`] gives them. A block is one line, without the
/// marks.
pub fn comment_blocks(text: &str, mark: &str) -> Vec<String> {
    comment_lines(text, mark)
        .iter()
        .map(|block| {
            let words: Vec<&str> = block.iter().map(|line| line.words).collect();
            words.join(" ")
        })
        .collect()
}
