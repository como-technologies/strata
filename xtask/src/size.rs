//! The size limits of `docs/src/development/code.md` for the Rust files of strata, xtask,
//! strata-preflight, and strata-fuzz: a module file has at most 800 lines, and a function has at
//! most 80 lines.

use std::path::{Path, PathBuf};

use regex::Regex;

use crate::files::files_under;

/// The most lines that a module file has.
const MODULE_LINES: usize = 800;

/// The most lines that a function has, from its `fn` line to its closing brace.
const FUNCTION_LINES: usize = 80;

/// The line that starts the unit tests inside a module file. A test function is out of the count.
const INLINE_TESTS: &str = "\n#[cfg(test)]\nmod tests {";

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

/// Each Rust file under `src` with its text, in name order. A file `tests.rs` holds only tests, so
/// it is not in the list: a file of tests can be longer than the limit.
fn sources(src: &Path) -> Vec<(String, String)> {
    files_under(src, "rs")
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|name| name != "tests.rs"))
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

/// The module files under `src` with more than [`MODULE_LINES`] lines, each with its count.
fn long_modules(src: &Path) -> Vec<String> {
    sources(src)
        .iter()
        .map(|(file, text)| (file, text.lines().count()))
        .filter(|(_, lines)| *lines > MODULE_LINES)
        .map(|(file, lines)| format!("{file}: {lines} lines"))
        .collect()
}

/// The functions under `src` with more than [`FUNCTION_LINES`] lines, outside the unit tests that
/// a module file holds.
fn long_functions(src: &Path) -> Vec<String> {
    sources(src)
        .iter()
        .flat_map(|(file, text)| {
            let code = text.split(INLINE_TESTS).next().unwrap_or(text);
            function_lengths(code)
                .into_iter()
                .filter(|(_, lines)| *lines > FUNCTION_LINES)
                .map(move |(name, lines)| format!("{file}: {name} has {lines} lines"))
        })
        .collect()
}

/// The directory of the source files of strata.
fn strata_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../src")
}

/// The directory of the source files of xtask.
fn xtask_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// The directory of the source files of strata-preflight.
fn preflight_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../preflight/src")
}

/// The directory of the source files of strata-fuzz.
fn fuzz_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../fuzz/src")
}

#[test]
fn no_module_file_of_strata_has_more_than_800_lines() {
    let long = long_modules(&strata_src());
    assert!(long.is_empty(), "divide these module files: {long:?}");
}

#[test]
fn no_function_of_strata_has_more_than_80_lines() {
    let long = long_functions(&strata_src());
    assert!(long.is_empty(), "divide these functions: {long:?}");
}

#[test]
fn no_module_file_of_xtask_has_more_than_800_lines() {
    let long = long_modules(&xtask_src());
    assert!(long.is_empty(), "divide these module files: {long:?}");
}

#[test]
fn no_function_of_xtask_has_more_than_80_lines() {
    let long = long_functions(&xtask_src());
    assert!(long.is_empty(), "divide these functions: {long:?}");
}

#[test]
fn no_module_file_of_strata_preflight_has_more_than_800_lines() {
    let long = long_modules(&preflight_src());
    assert!(long.is_empty(), "divide these module files: {long:?}");
}

#[test]
fn no_function_of_strata_preflight_has_more_than_80_lines() {
    let long = long_functions(&preflight_src());
    assert!(long.is_empty(), "divide these functions: {long:?}");
}

#[test]
fn no_module_file_of_strata_fuzz_has_more_than_800_lines() {
    let long = long_modules(&fuzz_src());
    assert!(long.is_empty(), "divide these module files: {long:?}");
}

#[test]
fn no_function_of_strata_fuzz_has_more_than_80_lines() {
    let long = long_functions(&fuzz_src());
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

#[test]
fn the_unit_tests_of_a_module_file_are_out_of_the_function_count() {
    let dir = std::env::temp_dir().join(format!("strata-size-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let body = "    let a = 1;\n".repeat(FUNCTION_LINES);
    let helper = format!("fn helper() {{\n{body}}}\n");
    std::fs::write(dir.join("tests.rs"), &helper).unwrap();
    std::fs::write(
        dir.join("inline.rs"),
        format!("fn code() {{\n}}\n\n#[cfg(test)]\nmod tests {{\n{helper}}}\n"),
    )
    .unwrap();
    assert!(long_functions(&dir).is_empty());
    std::fs::write(dir.join("long.rs"), &helper).unwrap();
    let long = long_functions(&dir);
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(long, ["long.rs: helper has 82 lines"]);
}
