//! The size limits of `docs/src/development/code.md` for the Rust files of xtask: a module file
//! has at most 800 lines, and a function has at most 80 lines.

use std::path::Path;

use regex::Regex;

use crate::files::files_under;

/// The most lines that a module file has.
const MODULE_LINES: usize = 800;

/// The most lines that a function has, from its `fn` line to its closing brace.
const FUNCTION_LINES: usize = 80;

/// The functions of `text` with their length in lines: from the `fn` line to the closing brace.
fn function_lengths(text: &str) -> Vec<(String, usize)> {
    let start = Regex::new(r"^(\s*)(?:pub(?:\([a-z]+\))? )?(?:const )?fn (\w+)")
        .expect("the pattern is valid");
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    for (first, line) in lines.iter().enumerate() {
        let Some(head) = start.captures(line) else {
            continue;
        };
        let closing = format!("{}}}", &head[1]);
        let text = line.trim_end();
        let length = if text.ends_with(';') || (text.ends_with('}') && text.contains('{')) {
            1
        } else {
            lines[first..]
                .iter()
                .position(|l| *l == closing)
                .map_or(1, |last| last + 1)
        };
        found.push((head[2].to_string(), length));
    }
    found
}

/// Each Rust file under `src` with its text, in name order.
fn sources(src: &Path) -> Vec<(String, String)> {
    files_under(src, "rs")
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            (
                path.strip_prefix(src)
                    .unwrap_or(&path)
                    .display()
                    .to_string(),
                text,
            )
        })
        .collect()
}

/// The directory of the source files of xtask.
fn xtask_src() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

#[test]
fn no_module_file_of_xtask_has_more_than_800_lines() {
    let long: Vec<String> = sources(&xtask_src())
        .iter()
        .map(|(file, text)| (file, text.lines().count()))
        .filter(|(_, lines)| *lines > MODULE_LINES)
        .map(|(file, lines)| format!("{file}: {lines} lines"))
        .collect();
    assert!(long.is_empty(), "divide these module files: {long:?}");
}

#[test]
fn no_function_of_xtask_has_more_than_80_lines() {
    let long: Vec<String> = sources(&xtask_src())
        .iter()
        .flat_map(|(file, text)| {
            function_lengths(text)
                .into_iter()
                .filter(|(_, lines)| *lines > FUNCTION_LINES)
                .map(move |(name, lines)| format!("{file}: {name} has {lines} lines"))
        })
        .collect();
    assert!(long.is_empty(), "divide these functions: {long:?}");
}

#[test]
fn a_function_counts_from_its_fn_line_to_its_closing_brace() {
    let text = "\
/// A doc comment is no part of the function.
pub(super) fn long() {
    let a = 1;
    let b = 2;
}

impl Thing {
    fn method(&self) {
        let c = 3;
    }

    const fn tiny() {}
}
";
    assert_eq!(
        function_lengths(text),
        [
            ("long".to_string(), 4),
            ("method".to_string(), 3),
            ("tiny".to_string(), 1)
        ]
    );
}
