//! The scans of a directory and of a Rust file that the tools of the book share.

use std::path::{Path, PathBuf};

use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Attribute, ImplItemFn, Item, TraitItemFn, parse_file};

/// The book, from the root of the repository.
const BOOK: &str = "docs/src";

/// The directory under the book that `just docs-examples` writes.
const OUTPUT: &str = "output";

/// The directory of the book, in the repository at `repo`.
pub fn book(repo: &Path) -> PathBuf {
    repo.join(BOOK)
}

/// The directory of the outputs that the book includes, in the repository at `repo`.
pub fn output_dir(repo: &Path) -> PathBuf {
    book(repo).join(OUTPUT)
}

/// The entries of `dir` in name order. A directory that cannot be read has none.
fn entries(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    entries
}

fn has_extension(path: &Path, ext: &str) -> bool {
    path.extension().is_some_and(|e| e == ext)
}

/// The files directly in `dir` with the extension `ext`, in name order.
///
/// The sort gives the same listing on each file system.
pub fn files_with_ext(dir: &Path, ext: &str) -> Vec<PathBuf> {
    entries(dir)
        .into_iter()
        .filter(|p| p.is_file() && has_extension(p, ext))
        .collect()
}

/// Every file under `dir` with the extension `ext`, at any depth, in name order.
pub fn files_under(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for path in entries(dir) {
        if path.is_dir() {
            out.extend(files_under(&path, ext));
        } else if has_extension(&path, ext) {
            out.push(path);
        }
    }
    out
}

/// Every markdown page under `dir`, at any depth, in name order.
pub fn pages(dir: &Path) -> Vec<PathBuf> {
    files_under(dir, "md")
}

/// True when `attrs` mark the item as test code: `#[test]` or `#[cfg(test)]`.
pub fn is_test_code(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("test")
            || (attr.path().is_ident("cfg")
                && attr
                    .parse_args::<syn::Path>()
                    .is_ok_and(|path| path.is_ident("test")))
    })
}

/// The attributes of `item`. An item of a kind that carries none, such as a bare macro call, gives
/// an empty list.
fn attrs_of(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(i) => &i.attrs,
        Item::Enum(i) => &i.attrs,
        Item::Fn(i) => &i.attrs,
        Item::Impl(i) => &i.attrs,
        Item::Macro(i) => &i.attrs,
        Item::Mod(i) => &i.attrs,
        Item::Static(i) => &i.attrs,
        Item::Struct(i) => &i.attrs,
        Item::Trait(i) => &i.attrs,
        Item::Type(i) => &i.attrs,
        Item::Union(i) => &i.attrs,
        Item::Use(i) => &i.attrs,
        _ => &[],
    }
}

/// The lines, first and last, that each item of test code takes. A line number starts at 1.
#[derive(Default)]
struct TestLines(Vec<(usize, usize)>);

impl TestLines {
    fn note(&mut self, span: Span) {
        self.0.push((span.start().line, span.end().line));
    }

    fn holds(&self, line: usize) -> bool {
        self.0
            .iter()
            .any(|(first, last)| (*first..=*last).contains(&line))
    }
}

impl<'ast> Visit<'ast> for TestLines {
    fn visit_item(&mut self, item: &'ast Item) {
        if is_test_code(attrs_of(item)) {
            self.note(item.span());
        } else {
            visit::visit_item(self, item);
        }
    }

    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        if is_test_code(&item.attrs) {
            self.note(item.span());
        } else {
            visit::visit_impl_item_fn(self, item);
        }
    }

    fn visit_trait_item_fn(&mut self, item: &'ast TraitItemFn) {
        if is_test_code(&item.attrs) {
            self.note(item.span());
        } else {
            visit::visit_trait_item_fn(self, item);
        }
    }
}

/// The lines of the Rust file `text` that are no comment and no test code. An item for the tests
/// (`#[cfg(test)]`, or `#[test]`) is out at any place of the file: an inline module, a line
/// `mod tests;`, or a function. The code after such an item stays. A file that is not Rust is an
/// error.
pub fn code_without_tests(text: &str) -> Result<String, syn::Error> {
    let mut tests = TestLines::default();
    tests.visit_file(&parse_file(text)?);
    Ok(text
        .lines()
        .enumerate()
        .filter(|(index, line)| !tests.holds(index + 1) && !line.trim_start().starts_with("//"))
        .map(|(_, line)| line)
        .collect::<Vec<_>>()
        .join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory with `b.md`, `a.md`, `c.txt`, and `sub/d.md`. It goes when the test ends.
    fn tree() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("sub")).unwrap();
        for file in ["b.md", "a.md", "c.txt", "sub/d.md"] {
            std::fs::write(root.path().join(file), "").unwrap();
        }
        root
    }

    fn names(root: &Path, files: Vec<PathBuf>) -> Vec<String> {
        files
            .iter()
            .map(|f| f.strip_prefix(root).unwrap().display().to_string())
            .collect()
    }

    #[test]
    fn the_files_of_a_directory_with_an_extension_come_in_name_order_without_the_subdirectories() {
        let temp = tree();
        let root = temp.path();
        let found = names(root, files_with_ext(root, "md"));
        assert_eq!(found, ["a.md", "b.md"]);
    }

    #[test]
    fn the_pages_of_a_directory_come_in_name_order_at_any_depth() {
        let temp = tree();
        let root = temp.path();
        let found = names(root, pages(root));
        assert_eq!(found, ["a.md", "b.md", "sub/d.md"]);
    }

    #[test]
    fn a_directory_that_does_not_exist_has_no_file() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing");
        assert!(files_with_ext(&missing, "md").is_empty());
        assert!(pages(&missing).is_empty());
    }

    #[test]
    fn the_code_without_tests_stops_at_no_module_and_keeps_the_code_after_it() {
        let text = "fn code() {}\n\n#[cfg(test)]\nmod other_tests {\n    fn t() {}\n}\n\n#[cfg(test)]\nmod tests;\nfn later() {}\n";
        assert_eq!(
            code_without_tests(text).unwrap(),
            "fn code() {}\n\n\nfn later() {}"
        );
    }

    #[test]
    fn the_code_without_tests_drops_each_item_for_the_tests_and_each_comment() {
        let text = "// a comment\nfn code() {}\n#[test]\nfn a_test() {}\n#[cfg(test)]\nimpl Call {\n    fn helper() {}\n}\nstruct Thing;\nimpl Thing {\n    #[cfg(test)]\n    fn only_in_tests(&self) {}\n    fn method(&self) {}\n}\n/// docs\nfn last() {}\n";
        assert_eq!(
            code_without_tests(text).unwrap(),
            "fn code() {}\nstruct Thing;\nimpl Thing {\n    fn method(&self) {}\n}\nfn last() {}"
        );
    }

    #[test]
    fn a_module_that_is_not_for_the_tests_is_code() {
        let text = "mod tests {\n    fn kept() {}\n}\n#[cfg(not(test))]\nfn also_kept() {}\n";
        assert_eq!(code_without_tests(text).unwrap(), text.trim_end());
    }

    #[test]
    fn a_text_that_is_no_rust_is_an_error() {
        assert!(code_without_tests("fn {").is_err());
    }
}
