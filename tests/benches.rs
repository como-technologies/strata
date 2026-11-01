//! The benchmarks of `benches/` (#214): each file of the directory is a target that criterion
//! runs, and the book says what each one measures.

use std::fs;
use std::path::Path;

use crate::common::repo_file;

/// The names of the benchmark targets: the files `benches/*.rs`, without the extension, sorted.
fn targets() -> Vec<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("benches");
    let mut names: Vec<String> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.expect("an entry of benches/").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .filter_map(|path| Some(path.file_stem()?.to_str()?.to_string()))
        .collect();
    names.sort();
    names
}

/// `autobenches` is off, because `benches/` is a package of its own. So a file that the manifest
/// does not name never runs, and it never fails: `just bench` and the job of CI pass
/// without it. The manifest must name each file, and the file must run criterion.
#[test]
fn each_file_of_benches_is_a_bench_target_that_runs_criterion() {
    let manifest = repo_file("benches/Cargo.toml");
    let names = targets();
    assert!(names.len() >= 6, "{names:?}");
    for name in &names {
        let target =
            format!("[[bench]]\nname = \"{name}\"\npath = \"{name}.rs\"\nharness = false\n");
        assert!(
            manifest.contains(&target),
            "benches/Cargo.toml has no target:\n{target}"
        );
        let source = repo_file(&format!("benches/{name}.rs"));
        assert!(
            source.contains("criterion_main!("),
            "benches/{name}.rs runs no benchmark"
        );
    }
    let declared = manifest.matches("[[bench]]").count();
    assert_eq!(
        declared,
        names.len(),
        "a target names a file that is not there"
    );
}

/// The root package builds each file of `benches/` as a benchmark of its own, and fails to find
/// criterion, when it keeps `autobenches`.
#[test]
fn the_root_package_leaves_the_benches_directory_to_its_package() {
    assert!(repo_file("Cargo.toml").contains("\nautobenches = false\n"));
}

/// The book tells a reader what each benchmark measures. A target that the page does not name is
/// a benchmark that nobody knows how to read.
#[test]
fn the_book_names_each_bench_target() {
    let page = repo_file("docs/src/development/build-test.md");
    assert!(page.contains("\n## The benchmarks\n"));
    for name in targets() {
        assert!(
            page.contains(&format!("`benches/{name}.rs`")),
            "no row for {name}"
        );
    }
}
