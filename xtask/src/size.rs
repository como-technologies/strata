//! The size limits of `docs/src/development/code.md` for the Rust files of strata, xtask,
//! strata-preflight, strata-fuzz, strata-report-endpoint, and strata-benches: a module file has at
//! most 800 lines, and a function has at most 80 lines.

use std::path::{Path, PathBuf};

use syn::visit::{self, Visit};
use syn::{Attribute, Block, ImplItemFn, ItemFn, ItemImpl, ItemMod, ItemTrait, Signature};
use syn::{TraitItemFn, parse_file};

use crate::files::files_under;

/// The most lines that a module file has.
const MODULE_LINES: usize = 800;

/// The most lines that a function has, from its `fn` line to its closing brace.
const FUNCTION_LINES: usize = 80;

/// True when `attrs` mark the item as test code: `#[test]` or `#[cfg(test)]`.
fn is_test_code(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("test")
            || (attr.path().is_ident("cfg")
                && attr
                    .parse_args::<syn::Path>()
                    .is_ok_and(|path| path.is_ident("test")))
    })
}

/// The functions that a file holds with a body, each with its length in lines. The items that are
/// test code are out of the list, and the items after them are in it.
#[derive(Default)]
struct Lengths(Vec<(String, usize)>);

impl Lengths {
    /// Note the function `sig` with the body `block`: its length goes from the line of `fn` to the
    /// line of the closing brace.
    fn note(&mut self, sig: &Signature, block: &Block) {
        let first = sig.fn_token.span.start().line;
        let last = block.brace_token.span.close().end().line;
        self.0.push((sig.ident.to_string(), last - first + 1));
    }
}

impl<'ast> Visit<'ast> for Lengths {
    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        if !is_test_code(&item.attrs) {
            self.note(&item.sig, &item.block);
            visit::visit_item_fn(self, item);
        }
    }

    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        if !is_test_code(&item.attrs) {
            self.note(&item.sig, &item.block);
            visit::visit_impl_item_fn(self, item);
        }
    }

    fn visit_trait_item_fn(&mut self, item: &'ast TraitItemFn) {
        if let Some(block) = &item.default
            && !is_test_code(&item.attrs)
        {
            self.note(&item.sig, block);
            visit::visit_trait_item_fn(self, item);
        }
    }

    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        if !is_test_code(&item.attrs) {
            visit::visit_item_mod(self, item);
        }
    }

    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        if !is_test_code(&item.attrs) {
            visit::visit_item_impl(self, item);
        }
    }

    fn visit_item_trait(&mut self, item: &'ast ItemTrait) {
        if !is_test_code(&item.attrs) {
            visit::visit_item_trait(self, item);
        }
    }
}

/// The functions of the Rust file `text` with their length in lines: from the `fn` line to the
/// closing brace. A function of the unit tests is out of the list. A file that is not Rust is an
/// error.
fn function_lengths(text: &str) -> Result<Vec<(String, usize)>, syn::Error> {
    let mut lengths = Lengths::default();
    lengths.visit_file(&parse_file(text)?);
    Ok(lengths.0)
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
            function_lengths(text)
                .unwrap_or_else(|e| panic!("{file}: {e}"))
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

/// The directory of the source files of strata-report-endpoint.
fn endpoint_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../report-endpoint/src")
}

/// The directory of the source files of strata-benches.
fn benches_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../benches/src")
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
fn no_module_file_of_strata_report_endpoint_has_more_than_800_lines() {
    let long = long_modules(&endpoint_src());
    assert!(long.is_empty(), "divide these module files: {long:?}");
}

#[test]
fn no_function_of_strata_report_endpoint_has_more_than_80_lines() {
    let long = long_functions(&endpoint_src());
    assert!(long.is_empty(), "divide these functions: {long:?}");
}

#[test]
fn no_module_file_of_strata_benches_has_more_than_800_lines() {
    let long = long_modules(&benches_src());
    assert!(long.is_empty(), "divide these module files: {long:?}");
}

#[test]
fn no_function_of_strata_benches_has_more_than_80_lines() {
    let long = long_functions(&benches_src());
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
        function_lengths(text).unwrap(),
        [
            ("long".to_string(), 4),
            ("method".to_string(), 3),
            ("tiny".to_string(), 1)
        ]
    );
}

#[test]
fn a_function_counts_with_each_head_that_rust_allows() {
    let text = "\
async fn a() {
}
unsafe fn b() {
}
pub(in crate::ops) fn c() {
}
pub const unsafe fn d() {
}
pub(crate) fn e() {
}
trait Shape {
    fn declared(&self);
    fn provided(&self) {
    }
}
";
    let names: Vec<String> = function_lengths(text)
        .unwrap()
        .into_iter()
        .map(|(name, lines)| format!("{name} {lines}"))
        .collect();
    assert_eq!(names, ["a 2", "b 2", "c 2", "d 2", "e 2", "provided 2"]);
}

#[test]
fn a_closing_brace_in_a_raw_string_does_not_end_a_function() {
    let text = "\
fn holds_text() {
    let text = r#\"
}
\"#;
    let more = 1;
}
";
    assert_eq!(
        function_lengths(text).unwrap(),
        [("holds_text".to_string(), 6)]
    );
}

#[test]
fn a_function_after_the_unit_tests_of_a_module_file_is_in_the_function_count() {
    let text = "\
fn before() {
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_test() {
        let a = 1;
    }
}

fn after() {
    let b = 2;
}

#[cfg(test)]
fn helper() {
}

impl Thing {
    #[cfg(test)]
    fn only_in_tests(&self) {
    }

    fn method(&self) {
    }
}
";
    let names: Vec<String> = function_lengths(text)
        .unwrap()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, ["before", "after", "method"]);
}

#[test]
fn the_unit_tests_of_a_module_file_are_out_of_the_function_count() {
    let dir = tempfile::tempdir().unwrap();
    let body = "    let a = 1;\n".repeat(FUNCTION_LINES);
    let helper = format!("fn helper() {{\n{body}}}\n");
    std::fs::write(dir.path().join("tests.rs"), &helper).unwrap();
    std::fs::write(
        dir.path().join("inline.rs"),
        format!("fn code() {{\n}}\n\n#[cfg(test)]\nmod tests {{\n{helper}}}\n"),
    )
    .unwrap();
    assert!(long_functions(dir.path()).is_empty());
    std::fs::write(dir.path().join("long.rs"), &helper).unwrap();
    assert_eq!(long_functions(dir.path()), ["long.rs: helper has 82 lines"]);
}

#[test]
fn a_long_function_after_the_unit_tests_of_a_module_file_is_found() {
    let dir = tempfile::tempdir().unwrap();
    let body = "    let a = 1;\n".repeat(FUNCTION_LINES);
    std::fs::write(
        dir.path().join("late.rs"),
        format!("#[cfg(test)]\nmod tests {{\n}}\n\nfn late() {{\n{body}}}\n"),
    )
    .unwrap();
    assert_eq!(long_functions(dir.path()), ["late.rs: late has 82 lines"]);
}
