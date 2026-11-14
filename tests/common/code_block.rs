//! The read of the fenced code blocks of a markdown text, as the markdown of GitHub reads them.
//! The tests of the root package read a block of the book with it, and the text of a markdown
//! output outside its blocks.
//!
//! The unit tests of `src/text.rs` and the report tests of `strata-preflight` include this file
//! with `#[path]`. So each test reads a fence in one way, and a change of the rule changes this
//! file only.

/// A line of a markdown text, and its place in or out of the fenced blocks.
enum Line<'a> {
    /// A line outside each block.
    Text(&'a str),
    /// The fence that opens a block: the spaces before it, and the info string after it.
    Open { indent: usize, info: &'a str },
    /// A line in a block.
    Code(&'a str),
    /// The fence that closes a block.
    Close,
}

/// Each line of `text`, with its place in or out of the fenced blocks.
///
/// A line of three or more backticks opens a block, when its info string holds no backtick. The
/// next line that holds only backticks, at least as many, closes it. Up to three spaces can come
/// before each fence. A block that no line closes continues to the end.
fn lines(text: &str) -> Vec<Line<'_>> {
    let mut open: Option<usize> = None;
    text.lines()
        .map(|line| {
            let rest = line.trim_start_matches(' ');
            let indent = line.len() - rest.len();
            let run = rest.len() - rest.trim_start_matches('`').len();
            let after = &rest[run..];
            let fence = indent <= 3 && run >= 3;
            match open {
                None if fence && !after.contains('`') => {
                    open = Some(run);
                    Line::Open {
                        indent,
                        info: after.trim(),
                    }
                }
                None => Line::Text(line),
                Some(n) if fence && run >= n && after.trim().is_empty() => {
                    open = None;
                    Line::Close
                }
                Some(_) => Line::Code(line),
            }
        })
        .collect()
}

/// The fenced blocks of the language `lang` in `text`, in order. A block loses the indent of its
/// opening fence, so the block of a list item reads as it does in a terminal.
pub fn code_blocks(text: &str, lang: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut block: Option<(usize, Vec<&str>)> = None;
    for line in lines(text) {
        match line {
            Line::Open { indent, info } if info == lang => block = Some((indent, Vec::new())),
            Line::Code(code) => {
                if let Some((indent, lines)) = &mut block {
                    lines.push(code.get(*indent..).unwrap_or(""));
                }
            }
            Line::Close => blocks.extend(block.take().map(|(_, lines)| lines.join("\n"))),
            Line::Open { .. } | Line::Text(_) => {}
        }
    }
    blocks
}

/// The lines of `markdown` outside its fenced code blocks.
pub fn outside_code_blocks(markdown: &str) -> String {
    lines(markdown)
        .into_iter()
        .filter_map(|line| match line {
            Line::Text(text) => Some(format!("{text}\n")),
            _ => None,
        })
        .collect()
}
